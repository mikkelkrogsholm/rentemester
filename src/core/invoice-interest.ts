import { runSql } from "./sqlite";
import type { Database } from "bun:sqlite";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { getInvoiceStatus } from "./invoice-payments";
import { insertAuditLog } from "./actor";
import { decodeTransactionRejection, TransactionRejectionError } from "./transaction-rejection";
import { isValidIsoDate as looksLikeIsoDate, diffDays } from "./dates";
import { addDkk, compareDkk, cumulativeInterestDkk, fromOre, roundDkk, subtractDkk, toOre } from "./money";
import { accountRoleCompatibility, resolveAccountRole } from "./account-roles";
import {
  calculateClaimReceivableBalances,
  calculateInterestIncomeBalances,
  calculateInterestReceivableBalances,
  resolveClaimIncomeAccount,
} from "./invoice-claim-receivable";

const RULE_ID = "DK-INVOICE-LATE-INTEREST-001";
const REGISTER_RULE_ID = "DK-INVOICE-LATE-INTEREST-REGISTER-001";
const BOOKKEEPING_RULE_ID = "DK-INVOICE-LATE-INTEREST-BOOKKEEPING-001";

// Statutory surcharge added to the reference rate to get the annual morarente
// (renteloven § 5, stk. 1). 8 pct since 2013-03-01 (7 pct before).
const STATUTORY_SURCHARGE_PERCENT = 8;

// morarente = reference rate + statutory surcharge (renteloven § 5, stk. 1),
// rounded to two decimals the same way the headline annualInterestRatePercent is.
function rateFromReference(referencePercent: number): number {
  return roundDkk(addDkk(referencePercent, STATUTORY_SURCHARGE_PERCENT));
}

// Versioned half-yearly reference-rate table (renteloven § 5, stk. 1 & 2). The
// reference rate is Danmarks Nationalbanks official lending rate (udlånsrente)
// in effect on 1 January / 1 July; it is FIXED for the whole following half-year
// regardless of intra-period rate moves, so the table only ever changes on those
// two dates. Each entry is `effectiveFrom` (inclusive) → referencePercent; the
// applicable rate for a given date is the latest entry whose effectiveFrom is on
// or before it. morarente = referencePercent + STATUTORY_SURCHARGE_PERCENT.
//
// Sources (verified 2026-06):
//   - renteloven § 5: https://danskelove.dk/renteloven/5
//   - Nationalbanken official interest rates:
//     https://www.nationalbanken.dk/en/what-we-do/stable-prices-monetary-policy-and-the-danish-economy/official-interest-rates
//   - morarente history (referencesats = morarente − 8 pct):
//     https://forbrug.dk/regler/opslagsvaerk-forbrugerleksikon/morarenten
//
//   period        morarente   referencePercent (morarente − 8)
//   2023-01-01    9.90 %      1.90
//   2023-07-01   11.25 %      3.25
//   2024-01-01   11.75 %      3.75
//   2024-07-01   11.50 %      3.50
//   2025-01-01   10.75 %      2.75
//   2025-07-01    9.75 %      1.75
//   2026-01-01    9.75 %      1.75
const REFERENCE_RATE_TABLE: ReadonlyArray<{ effectiveFrom: string; referencePercent: number }> = [
  { effectiveFrom: "2023-01-01", referencePercent: 1.9 },
  { effectiveFrom: "2023-07-01", referencePercent: 3.25 },
  { effectiveFrom: "2024-01-01", referencePercent: 3.75 },
  { effectiveFrom: "2024-07-01", referencePercent: 3.5 },
  { effectiveFrom: "2025-01-01", referencePercent: 2.75 },
  { effectiveFrom: "2025-07-01", referencePercent: 1.75 },
  { effectiveFrom: "2026-01-01", referencePercent: 1.75 },
];

// Tolerance (in percentage points) within which a manually supplied reference
// rate is accepted silently. A larger deviation from the table for the relevant
// half-year is flagged with a warning but NOT rejected — the human bookkeeper may
// knowingly override (e.g. a contractually agreed rate, or a not-yet-tabled
// period). Human-in-the-loop is preserved; we only surface the discrepancy.
const REFERENCE_RATE_SANITY_TOLERANCE_PCT = 0.25;

/**
 * The statutory reference rate (Nationalbankens udlånsrente per renteloven § 5)
 * in effect for the half-year containing `asOfDate`, or null when the date falls
 * before the earliest tabled period (no default available — the caller must
 * supply a rate explicitly). Picks the latest entry whose effectiveFrom ≤ asOfDate.
 */
export function lookupStatutoryReferenceRate(asOfDate: string): number | null {
  let match: number | null = null;
  for (const entry of REFERENCE_RATE_TABLE) {
    if (entry.effectiveFrom <= asOfDate) match = entry.referencePercent;
    else break;
  }
  return match;
}

/**
 * The half-yearly statutory rate-change dates (1 January / 1 July) strictly
 * inside the open window (from, to) — i.e. from < boundary < to. The reference
 * rate is fixed per half-year (renteloven § 5, stk. 2), so a window crossing
 * one of these dates must be split there and each part forrentes at its own
 * half-year's reference rate. Returns the ascending list of crossing dates;
 * empty when the whole window lies inside a single half-year.
 */
function statutoryRateBoundariesBetween(from: string, to: string): string[] {
  const boundaries: string[] = [];
  for (let year = Number(from.slice(0, 4)); year <= Number(to.slice(0, 4)); year++) {
    for (const date of [`${year}-01-01`, `${year}-07-01`]) {
      if (date > from && date < to) boundaries.push(date);
    }
  }
  return boundaries;
}

/**
 * The ordered rate-windows for ONE claim window [from, to], used identically by
 * the late-interest calculation (when it bills a claim) and by the correction
 * proposal (when it reconstructs that claim's lawful interest) — so the two can
 * never diverge on the multi-half-year case.
 *
 * - A statutory-table claim is split at every half-yearly reference-rate change
 *   (1/1, 1/7) strictly inside the window, each part forrentet at THAT half-year's
 *   reference rate + 8 (renteloven § 5). `fallbackRate` is used only on the
 *   hypothetical pre-table day the table cannot cover.
 * - A manual-override claim is one deliberate rate (`singleAnnualRate`) for the
 *   whole window — the human-in-the-loop choice is never re-segmented.
 */
function claimRateWindows(
  from: string,
  to: string,
  source: "statutory-table" | "manual-override",
  singleAnnualRate: number,
  fallbackRate: number,
): Array<{ end: string; annualRatePercent: number }> {
  if (source !== "statutory-table") return [{ end: to, annualRatePercent: singleAnnualRate }];
  const windows: Array<{ end: string; annualRatePercent: number }> = [];
  let segmentStart = from;
  for (const boundary of statutoryRateBoundariesBetween(from, to)) {
    const segmentRate = lookupStatutoryReferenceRate(segmentStart);
    windows.push({ end: boundary, annualRatePercent: rateFromReference(segmentRate ?? fallbackRate) });
    segmentStart = boundary;
  }
  const lastRate = lookupStatutoryReferenceRate(segmentStart);
  windows.push({ end: to, annualRatePercent: rateFromReference(lastRate ?? fallbackRate) });
  return windows;
}

export type CalculateInvoiceLateInterestInput = {
  invoiceDocumentId: number;
  asOfDate: string;
  // The reference rate (Nationalbankens udlånsrente, renteloven § 5). Optional:
  // when omitted, the statutory table value for asOfDate's half-year is used as
  // the default. A manually supplied value is honoured (human-in-the-loop) but a
  // large deviation from the table for the relevant period yields a warning.
  referenceRatePercent?: number;
};

export type CalculateInvoiceLateInterestResult = {
  ok: boolean;
  invoiceDocumentId?: number;
  invoiceNumber?: string;
  asOfDate?: string;
  effectiveDueDate?: string;
  overdueDays?: number;
  // The day this claim's interest starts accruing from: the latest existing
  // claim date, or the effective due date if there is none. Anchors the
  // incremental segment so a later claim never re-bills days already covered.
  interestFromDate?: string;
  // Days actually billed by THIS claim (interestFromDate → asOfDate). Equals
  // overdueDays for a first claim; the incremental window for a later one.
  claimableDays?: number;
  principalOpenBalance?: number;
  // The reference rate in effect AT the as-of date. For a statutory-table window
  // that crosses one or more half-yearly rate changes, the underlying accrual
  // uses each half-year's own reference rate (see totalInterestToDate); this
  // single field reports the as-of-date rate as the representative headline (the
  // least confusing summary), NOT the only rate applied. A manual override is one
  // rate for the whole window, so the field is exactly that rate.
  referenceRatePercent?: number;
  // Annual morarente at the as-of date = referenceRatePercent + 8 (renteloven § 5).
  // Same multi-half-year caveat as referenceRatePercent: a long statutory window
  // accrues per half-year, so this is the as-of-date headline, not the sole rate.
  annualInterestRatePercent?: number;
  // The interest claimable NOW — the incremental amount for the period since the
  // last claim. For a first claim this equals the full overdue window. Computed
  // as totalInterestToDate minus what has already been claimed, so summing the
  // staged claims reproduces a single continuous calculation with no øre drift.
  accruedInterestAmount?: number;
  // Sum of interest already registered on this invoice up to the as-of date
  // (0 when none).
  priorClaimedInterest?: number;
  // Cumulative statutory interest accrued through the as-of date, rounded to øre
  // exactly once across all contiguous segments (no per-segment drift).
  totalInterestToDate?: number;
  // The amount by which immutable earlier claims already billed MORE than the
  // now-lawful cumulative — non-zero only when a back-dated balance reduction
  // retroactively lowered the principal under an already-issued claim. A signal
  // that a correcting credit may be due; never auto-applied. 0 in the normal case.
  overClaimedInterest?: number;
  // Whether the reference rate used came from the statutory table (default) or
  // was supplied by the caller. Lets the human see when an override is in effect.
  referenceRateSource?: "statutory-table" | "manual-override";
  // Non-fatal advisories — e.g. a manual reference rate that deviates materially
  // from the statutory table for the relevant half-year. The calculation still
  // proceeds (human-in-the-loop); the warning only surfaces the discrepancy.
  warnings?: string[];
  appliedRules: string[];
  errors: string[];
};

export type RegisterInvoiceLateInterestInput = CalculateInvoiceLateInterestInput & {
  note?: string;
  // Actor attribution for the registration audit_log row (the post step already
  // threads these; the register step must too, or the row leaks the OS user).
  createdBy?: string;
  createdByProgram?: string;
};

export type RegisterInvoiceLateInterestResult = CalculateInvoiceLateInterestResult & {
  claimId?: number;
  claimDate?: string;
  claimOpenBalance?: number;
};

export type PostInvoiceLateInterestToLedgerInput = {
  invoiceDocumentId: number;
  claimId?: number;
  transactionDate?: string;
  receivableAccountNo?: string;
  interestIncomeAccountNo?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type PostInvoiceLateInterestToLedgerResult = JournalPostResult & {
  claimId?: number;
  invoiceDocumentId?: number;
  invoiceNumber?: string;
  claimDate?: string;
  accruedInterestAmount?: number;
  claimOpenBalance?: number;
};

type InterestSegment = { principalAmount: number; annualRatePercent: number; days: number };

/**
 * Date-aware accrual primitives for one invoice, built from a getInvoiceStatus
 * result. The open balance is reconstructed AS OF EACH DAY from the balance-
 * changing events (payments, credit notes, refunds, write-offs) by effective
 * date — getInvoiceStatus itself sums them date-blind, so we redo it date-aware.
 * Shared by the interest calculation and the correction proposal so the two can
 * never drift apart on money-critical logic.
 */
function buildBalanceTimeline(
  status: {
    grossAmount?: number;
    payments?: Array<{ paymentDate: string; amount: number }>;
    creditNotes?: Array<{ issueDate: string | null; amount: number }>;
    refunds?: Array<{ refundDate: string; amount: number }>;
    badDebtWriteOffs?: Array<{ writeOffDate: string; grossAmount: number }>;
  },
  fallbackDate: string,
) {
  const grossAmount = roundDkk(Number(status.grossAmount ?? 0));
  const balanceEvents: Array<{ date: string; delta: number }> = [
    ...(status.payments ?? []).map((p) => ({ date: p.paymentDate, delta: -Number(p.amount) })),
    ...(status.creditNotes ?? []).map((c) => ({ date: c.issueDate ?? fallbackDate, delta: -Number(c.amount) })),
    ...(status.refunds ?? []).map((r) => ({ date: r.refundDate, delta: Number(r.amount) })),
    ...(status.badDebtWriteOffs ?? []).map((w) => ({ date: w.writeOffDate, delta: -Number(w.grossAmount) })),
  ].filter((e): e is { date: string; delta: number } => typeof e.date === "string");

  // Open principal balance as of a date: gross plus every balance event effective
  // on/before that date. Mirrors getInvoiceStatus's openBalance formula.
  const openBalanceAsOf = (asOf: string): number => {
    let balance = grossAmount;
    for (const event of balanceEvents) {
      if (event.date <= asOf) balance = addDkk(balance, event.delta);
    }
    return roundDkk(balance);
  };

  const sortedEventDates = [...new Set(balanceEvents.map((e) => e.date))].sort();
  // One claim window [from, to] at a single rate, split into sub-segments at each
  // balance-change date so every sub-segment uses the principal outstanding then.
  const windowSegments = (from: string | undefined, to: string, annualRatePercent: number): InterestSegment[] => {
    if (!from || diffDays(from, to) <= 0) return [];
    const breakpoints = [from, ...sortedEventDates.filter((d) => d > from && d < to), to];
    const out: InterestSegment[] = [];
    for (let i = 0; i < breakpoints.length - 1; i++) {
      const start = breakpoints[i]!;
      const days = diffDays(start, breakpoints[i + 1]!);
      if (days <= 0) continue;
      const principalAmount = openBalanceAsOf(start);
      if (principalAmount > 0) out.push({ principalAmount, annualRatePercent, days });
    }
    return out;
  };

  // Statutory interest summed across ordered rate-windows starting at `from`
  // (each [prevEnd, end] at its rate), date-aware, rounded to øre exactly once.
  const accrueWindows = (
    from: string | undefined,
    windows: Array<{ end: string; annualRatePercent: number }>,
  ): number => {
    const segments: InterestSegment[] = [];
    let start = from;
    for (const w of windows) {
      segments.push(...windowSegments(start, w.end, w.annualRatePercent));
      start = w.end;
    }
    return cumulativeInterestDkk(segments);
  };

  return { openBalanceAsOf, windowSegments, accrueWindows };
}

export function calculateInvoiceLateInterest(db: Database, input: CalculateInvoiceLateInterestInput): CalculateInvoiceLateInterestResult {
  const errors: string[] = [];
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) errors.push("invoiceDocumentId must be a positive integer");
  if (!looksLikeIsoDate(input.asOfDate)) errors.push("asOfDate must be YYYY-MM-DD");
  // A manually supplied rate must be finite; when omitted we fall back to the
  // statutory table for the as-of half-year (validated below once asOfDate is known).
  const manualRate = input.referenceRatePercent;
  if (manualRate !== undefined && !Number.isFinite(manualRate)) errors.push("referenceRatePercent must be a finite number when provided");
  if (manualRate !== undefined && Number.isFinite(manualRate) && manualRate < 0) errors.push("referenceRatePercent must not be negative");
  if (errors.length > 0) return { ok: false, appliedRules: [RULE_ID], errors };

  // Resolve the reference rate: caller override, else the statutory table.
  const warnings: string[] = [];
  const tableRate = lookupStatutoryReferenceRate(input.asOfDate);
  let referenceRatePercent: number;
  let referenceRateSource: "statutory-table" | "manual-override";
  if (manualRate === undefined) {
    if (tableRate === null) {
      return {
        ok: false,
        appliedRules: [RULE_ID],
        errors: [`no statutory reference rate is tabled for ${input.asOfDate}; supply referenceRatePercent explicitly`],
      };
    }
    referenceRatePercent = tableRate;
    referenceRateSource = "statutory-table";
  } else {
    referenceRatePercent = manualRate;
    referenceRateSource = "manual-override";
    // Sanity-bound: flag (do not reject) a manual rate that deviates materially
    // from the statutory table for the relevant half-year.
    if (tableRate !== null && Math.abs(roundDkk(subtractDkk(manualRate, tableRate))) > REFERENCE_RATE_SANITY_TOLERANCE_PCT) {
      warnings.push(
        `manual reference rate ${roundDkk(manualRate)} pct deviates from the statutory rate ${roundDkk(tableRate)} pct for the half-year containing ${input.asOfDate} (renteloven § 5); verify the override is intended`,
      );
    }
  }

  const status = getInvoiceStatus(db, input.invoiceDocumentId, input.asOfDate);
  if (!status.ok) return { ok: false, appliedRules: [RULE_ID], errors: status.errors };

  const overdueDays = Number(status.overdueDays ?? 0);
  const annualInterestRatePercent = rateFromReference(referenceRatePercent);
  const effectiveDueDate = status.effectiveDueDate;

  // Morarente accrues continuously, day by day, on the UNPAID principal from the
  // due date (renteloven § 3) at the reference rate + 8 pct (§ 5, stk. 1). It is
  // simple interest — there is no statutory rente-af-rente.
  //
  // The accrual is DATE-AWARE (buildBalanceTimeline): the principal is the open
  // balance as of each day, reconstructed from payments/credit notes/refunds/
  // write-offs by effective date. The overdue window is split both by the existing
  // claims (each carries its own reference rate) and by every balance-change date,
  // and the statutory interest is summed across all those sub-segments and rounded
  // to øre exactly ONCE — so a second claim re-bills no day an earlier one covered,
  // partial payments lower the later days' principal exactly, and staged claims
  // never drift by accumulated øre.
  //
  // A new claim bills only totalInterestToDate − what has already been claimed. If
  // a back-dated balance reduction makes the now-lawful cumulative LOWER than what
  // immutable earlier claims already billed, the increment clamps to 0 (nothing new
  // is owed) and the excess is reported as overClaimedInterest — issuing a
  // correcting credit for the already-posted claim is done via postInterestCorrection.
  const timeline = buildBalanceTimeline(status, effectiveDueDate ?? input.asOfDate);

  // The principal as of the as-of date gates "is there anything to bill".
  const principalOpenBalance = timeline.openBalanceAsOf(input.asOfDate);

  const priorClaims = db
    .query(
      `SELECT claim_date, annual_interest_rate_percent, amount_dkk
       FROM invoice_interest_claims
       WHERE invoice_document_id = ?
       ORDER BY claim_date ASC, id ASC`,
    )
    .all(input.invoiceDocumentId) as Array<{
    claim_date: string;
    annual_interest_rate_percent: number;
    amount_dkk: number;
  }>;

  const lastClaimDate = priorClaims.length
    ? priorClaims[priorClaims.length - 1]!.claim_date
    : null;

  // This claim's incremental window anchors at the latest existing claim (over ALL
  // claims, so a later claim is never overlapped) or the due date. The reported
  // from-date never exceeds the as-of date: a backwards query bills nothing new.
  const anchor = effectiveDueDate
    ? lastClaimDate && lastClaimDate > effectiveDueDate
      ? lastClaimDate
      : effectiveDueDate
    : undefined;
  const interestFromDate = anchor && anchor > input.asOfDate ? input.asOfDate : anchor;
  const claimableDays =
    interestFromDate && principalOpenBalance > 0
      ? Math.max(0, diffDays(interestFromDate, input.asOfDate))
      : 0;

  // Ordered rate-windows up to the as-of date: one per existing claim (clamped so
  // it never extends past the as-of date, at the claim's own rate), plus the new
  // increment window. priorClaimedInterest is what was actually billed in claims
  // dated on/before the as-of date — the baseline the new increment is measured against.
  const windows: Array<{ end: string; annualRatePercent: number }> = [];
  let priorClaimedInterest = 0;
  for (const claim of priorClaims) {
    windows.push({
      end: claim.claim_date < input.asOfDate ? claim.claim_date : input.asOfDate,
      annualRatePercent: Number(claim.annual_interest_rate_percent),
    });
    if (claim.claim_date <= input.asOfDate) {
      priorClaimedInterest = addDkk(priorClaimedInterest, Number(claim.amount_dkk));
    }
  }
  if (claimableDays > 0) {
    // This claim's NEW increment runs [interestFromDate, asOfDate). When the rate
    // comes from the statutory table, that window is split at every half-yearly
    // reference-rate change (1/1, 1/7) and each part forrentes at THAT half-year's
    // reference rate + 8 (renteloven § 5) — otherwise a window crossing a rate
    // change would unlawfully bill the whole span at the as-of half-year's rate.
    // A MANUAL override is never segmented: the human knowingly chose one rate for
    // the whole window (human-in-the-loop), so it stays a single window.
    if (referenceRateSource === "statutory-table" && interestFromDate) {
      windows.push(
        ...claimRateWindows(interestFromDate, input.asOfDate, "statutory-table", annualInterestRatePercent, referenceRatePercent),
      );
    } else {
      windows.push({ end: input.asOfDate, annualRatePercent: annualInterestRatePercent });
    }
  }
  // Net off any booked corrections (over-claimed interest already reversed): the
  // baseline is what was EFFECTIVELY billed, so a later claim re-bills the days a
  // correction gave back instead of permanently losing them.
  const priorCorrections = roundDkk(
    Number(
      (
        db
          .query(`SELECT COALESCE(SUM(amount_dkk), 0) AS total FROM invoice_interest_corrections WHERE invoice_document_id = ? AND correction_date <= ?`)
          .get(input.invoiceDocumentId, input.asOfDate) as { total: number } | null
      )?.total ?? 0,
    ),
  );
  priorClaimedInterest = roundDkk(subtractDkk(priorClaimedInterest, priorCorrections));

  const totalInterestToDate = timeline.accrueWindows(effectiveDueDate, windows);
  // Claimable now = cumulative interest through the as-of date minus what has
  // already been billed up to that date, clamped to ≥ 0.
  const accruedInterestAmount =
    claimableDays > 0 && totalInterestToDate > priorClaimedInterest
      ? roundDkk(subtractDkk(totalInterestToDate, priorClaimedInterest))
      : 0;
  // When immutable earlier claims already billed MORE than the now-lawful
  // cumulative (e.g. a back-dated payment retroactively lowered the balance),
  // surface the excess so a correcting credit can be considered. Not auto-applied.
  const overClaimedInterest =
    priorClaimedInterest > totalInterestToDate
      ? roundDkk(subtractDkk(priorClaimedInterest, totalInterestToDate))
      : 0;

  return {
    ok: true,
    invoiceDocumentId: input.invoiceDocumentId,
    invoiceNumber: status.invoiceNumber,
    asOfDate: input.asOfDate,
    effectiveDueDate,
    overdueDays,
    interestFromDate,
    claimableDays,
    principalOpenBalance,
    referenceRatePercent: roundDkk(referenceRatePercent),
    annualInterestRatePercent,
    accruedInterestAmount,
    priorClaimedInterest,
    totalInterestToDate,
    overClaimedInterest,
    referenceRateSource,
    warnings,
    appliedRules: [RULE_ID],
    errors: [],
  };
}

export function registerInvoiceLateInterest(db: Database, input: RegisterInvoiceLateInterestInput): RegisterInvoiceLateInterestResult {
  // Concurrency: the anchor (latest existing claim) is read inside
  // calculateInvoiceLateInterest and the new claim is inserted afterwards. Two
  // *separate processes* registering interest on the SAME invoice could both read
  // the same anchor before either inserts and bill overlapping windows. Wrapping
  // the read-then-write in a BEGIN IMMEDIATE transaction takes the write lock up
  // front, so the second process blocks until the first commits and then reads the
  // freshly inserted claim as its anchor — no overlap. (KODE-5)
  return db.transaction(() => registerInvoiceLateInterestTxn(db, input)).immediate();
}

function registerInvoiceLateInterestTxn(db: Database, input: RegisterInvoiceLateInterestInput): RegisterInvoiceLateInterestResult {
  const calculation = calculateInvoiceLateInterest(db, input);
  if (!calculation.ok) return { ...calculation, appliedRules: [...new Set([...(calculation.appliedRules ?? []), REGISTER_RULE_ID])] };
  // The effective reference rate (caller override OR statutory-table default) is
  // resolved inside the calculation; the duplicate check and insert below must
  // use THAT, not the raw (possibly omitted) input — otherwise a table-defaulted
  // claim would never match an existing row and could be double-registered.
  const effectiveReferenceRate = roundDkk(Number(calculation.referenceRatePercent));
  // Reject an exact duplicate (same as-of date AND reference rate) BEFORE the
  // positive-amount check below: re-registering the identical claim should
  // report "already registered" rather than the zero-increment "must be
  // positive" message it would otherwise hit (its incremental window is 0 days).
  const existing = db.query(
    `SELECT id FROM invoice_interest_claims WHERE invoice_document_id = ? AND claim_date = ? AND reference_rate_percent = ? LIMIT 1`
  ).get(input.invoiceDocumentId, input.asOfDate, effectiveReferenceRate) as { id: number } | null;
  if (existing) {
    return {
      ...calculation,
      ok: false,
      appliedRules: [RULE_ID, REGISTER_RULE_ID],
      errors: [`late interest for invoice ${input.invoiceDocumentId} is already registered for ${input.asOfDate} at reference rate ${effectiveReferenceRate}`],
    };
  }

  // accruedInterestAmount is the INCREMENTAL interest since the last claim (see
  // calculateInvoiceLateInterest). A non-positive increment means there is
  // nothing new to bill — the as-of date is on/before the last claim, or the
  // principal is settled. Because each claim only covers the days an earlier
  // claim has not, no open-claim guard is needed to prevent a double-charge:
  // staged claims (e.g. one per reminder) are both lawful and safe.
  if (!(Number(calculation.accruedInterestAmount ?? 0) > 0)) {
    return {
      ...calculation,
      ok: false,
      appliedRules: [...new Set([...(calculation.appliedRules ?? []), REGISTER_RULE_ID])],
      errors: ["late interest must be positive before it can be registered"],
    };
  }

  const inserted = db.query(
    `INSERT INTO invoice_interest_claims (
      invoice_document_id, claim_date, reference_rate_percent, annual_interest_rate_percent,
      reference_rate_source, overdue_days, principal_open_balance, amount_dkk, note
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    RETURNING id`
  ).get(
    input.invoiceDocumentId,
    input.asOfDate,
    effectiveReferenceRate,
    roundDkk(Number(calculation.annualInterestRatePercent)),
    // Record whether the rate came from the statutory table or a manual override,
    // so proposeInterestCorrection later reconstructs the lawful interest with the
    // SAME half-year segmentation this claim was billed with (JUR-7). A table claim
    // whose window crossed a 1/1 or 1/7 change billed multiple rates; a manual one
    // is a single deliberate rate — the stored rate alone cannot tell them apart.
    calculation.referenceRateSource ?? "manual-override",
    // Store the days THIS claim covers (the incremental segment). amount_dkk is
    // the rounded-cumulative delta (see calculateInvoiceLateInterest), so it is
    // reproducible from the full claim sequence, not the single row in isolation.
    Number(calculation.claimableDays),
    roundDkk(Number(calculation.principalOpenBalance)),
    roundDkk(Number(calculation.accruedInterestAmount)),
    input.note ?? null,
  ) as { id: number };

  insertAuditLog(db, {
    eventType: "invoice_interest_register",
    entityType: "invoice_interest_claim",
    entityId: inserted.id,
    message: `Registered late interest ${roundDkk(Number(calculation.accruedInterestAmount))} on invoice ${calculation.invoiceNumber}`,
    createdBy: input.createdBy,
    createdByProgram: input.createdByProgram,
  });

  const statusAfter = getInvoiceStatus(db, input.invoiceDocumentId, input.asOfDate);
  return {
    ...calculation,
    ok: true,
    claimId: inserted.id,
    claimDate: input.asOfDate,
    claimOpenBalance: statusAfter.ok ? statusAfter.claimOpenBalance : undefined,
    appliedRules: [RULE_ID, REGISTER_RULE_ID],
    errors: [],
  };
}

export function postInvoiceLateInterestToLedger(db: Database, input: PostInvoiceLateInterestToLedgerInput): PostInvoiceLateInterestToLedgerResult {
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) {
    return { ok: false, appliedRules: [BOOKKEEPING_RULE_ID], errors: ["invoiceDocumentId must be a positive integer"] };
  }
  if (input.transactionDate !== undefined && !looksLikeIsoDate(input.transactionDate)) {
    return { ok: false, appliedRules: [BOOKKEEPING_RULE_ID], errors: ["transactionDate must be YYYY-MM-DD when present"] };
  }

  // With no explicit claimId, post the OLDEST not-yet-posted claim (chronological
  // booking order). Now that staged multi-claim registration is allowed, the
  // default must skip already-posted claims, or it would re-select claim 1 and
  // refuse with "already posted" once it is booked. An explicit claimId selects
  // that claim regardless of posting state, so re-posting it still reports
  // "already posted".
  const claim = db.query(
    `SELECT c.id, c.invoice_document_id, c.claim_date, c.amount_dkk, d.invoice_no
     FROM invoice_interest_claims c
     JOIN documents d ON d.id = c.invoice_document_id
     LEFT JOIN invoice_interest_postings p ON p.interest_claim_id = c.id
     WHERE c.invoice_document_id = ?
       AND (? IS NULL OR c.id = ?)
       AND (? IS NOT NULL OR p.id IS NULL)
     ORDER BY c.claim_date ASC, c.id ASC
     LIMIT 1`
  ).get(
    input.invoiceDocumentId,
    input.claimId ?? null,
    input.claimId ?? null,
    input.claimId ?? null,
  ) as {
    id: number;
    invoice_document_id: number;
    claim_date: string;
    amount_dkk: number;
    invoice_no: string;
  } | null;

  if (!claim) {
    if (!input.claimId) {
      const anyClaim = db
        .query(`SELECT id FROM invoice_interest_claims WHERE invoice_document_id = ? LIMIT 1`)
        .get(input.invoiceDocumentId) as { id: number } | null;
      if (anyClaim) {
        return { ok: false, appliedRules: [BOOKKEEPING_RULE_ID], errors: [`all registered late-interest claims for invoice ${input.invoiceDocumentId} are already posted`] };
      }
    }
    return { ok: false, appliedRules: [BOOKKEEPING_RULE_ID], errors: [input.claimId ? `interest claim ${input.claimId} does not exist for invoice ${input.invoiceDocumentId}` : `invoice ${input.invoiceDocumentId} has no registered late-interest claim`] };
  }

  const existing = db.query(
    `SELECT p.id, p.journal_entry_id, j.entry_no
     FROM invoice_interest_postings p
     JOIN journal_entries j ON j.id = p.journal_entry_id
     WHERE p.interest_claim_id = ?`
  ).get(claim.id) as { id: number; journal_entry_id: number; entry_no: string } | null;

  if (existing) {
    return {
      ok: false,
      claimId: claim.id,
      invoiceDocumentId: claim.invoice_document_id,
      invoiceNumber: claim.invoice_no,
      claimDate: claim.claim_date,
      accruedInterestAmount: roundDkk(Number(claim.amount_dkk)),
      appliedRules: [BOOKKEEPING_RULE_ID],
      errors: [`interest claim ${claim.id} is already posted in journal entry ${existing.entry_no}`],
    };
  }

  const amount = roundDkk(Number(claim.amount_dkk));
  const receivable = input.receivableAccountNo
    ? (accountRoleCompatibility(db, "debtors", input.receivableAccountNo).ok
      ? { ok: true as const, accountNo: input.receivableAccountNo }
      : { ok: false as const, error: `account ${input.receivableAccountNo} is not compatible with role 'debtors'` })
    : resolveAccountRole(db, "debtors");
  if (!receivable.ok) {
    return { ok: false, claimId: claim.id, invoiceDocumentId: claim.invoice_document_id, invoiceNumber: claim.invoice_no, claimDate: claim.claim_date, accruedInterestAmount: amount, appliedRules: [BOOKKEEPING_RULE_ID], errors: [receivable.error] };
  }
  const income = resolveClaimIncomeAccount(db, input.interestIncomeAccountNo ?? "1010");
  if (!income.ok) {
    return { ok: false, claimId: claim.id, invoiceDocumentId: claim.invoice_document_id, invoiceNumber: claim.invoice_no, claimDate: claim.claim_date, accruedInterestAmount: amount, appliedRules: [BOOKKEEPING_RULE_ID], errors: [income.error] };
  }
  const transactionDate = input.transactionDate ?? claim.claim_date;
  if (transactionDate < claim.claim_date) {
    return { ok: false, claimId: claim.id, invoiceDocumentId: claim.invoice_document_id, invoiceNumber: claim.invoice_no, claimDate: claim.claim_date, accruedInterestAmount: amount, appliedRules: [BOOKKEEPING_RULE_ID], errors: [`interest posting date ${transactionDate} cannot predate claim date ${claim.claim_date}`] };
  }
  try {
    return db.transaction(() => {
      const journal = postJournalEntry(db, {
        transactionDate,
        text: `Late interest ${claim.invoice_no}`,
        documentId: claim.invoice_document_id,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines: [
          { accountNo: receivable.accountNo, debitAmount: amount, text: `Late-interest receivable ${claim.invoice_no}` },
          { accountNo: income.accountNo, creditAmount: amount, text: `Late-interest income ${claim.invoice_no}` },
        ],
      });
      if (!journal.ok) {
        return { ...journal, claimId: claim.id, invoiceDocumentId: claim.invoice_document_id, invoiceNumber: claim.invoice_no, claimDate: claim.claim_date, accruedInterestAmount: amount, appliedRules: [...new Set([...(journal.appliedRules ?? []), BOOKKEEPING_RULE_ID])] };
      }

      runSql(db,
        `INSERT INTO invoice_interest_postings (interest_claim_id, journal_entry_id) VALUES (?, ?)`,
        claim.id,
        journal.entryId ?? null,
      );

      insertAuditLog(db, {
        eventType: "invoice_interest_post",
        entityType: "invoice_interest_claim",
        entityId: claim.id,
        message: `Posted late interest ${amount} for invoice ${claim.invoice_no} in journal entry ${journal.entryNo}`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      const statusAfter = getInvoiceStatus(db, claim.invoice_document_id, transactionDate);
      return {
        ...journal,
        claimId: claim.id,
        invoiceDocumentId: claim.invoice_document_id,
        invoiceNumber: claim.invoice_no,
        claimDate: claim.claim_date,
        accruedInterestAmount: amount,
        claimOpenBalance: statusAfter.ok ? statusAfter.claimOpenBalance : undefined,
        appliedRules: [...new Set([...(journal.appliedRules ?? []), BOOKKEEPING_RULE_ID])],
      };
    })();
  } catch (error) {
    return {
      ok: false,
      claimId: claim.id,
      invoiceDocumentId: claim.invoice_document_id,
      invoiceNumber: claim.invoice_no,
      claimDate: claim.claim_date,
      accruedInterestAmount: amount,
      appliedRules: [BOOKKEEPING_RULE_ID],
      errors: [String(error)],
    };
  }
}

// --- Correcting over-claimed late interest (back-dated balance reductions) ----

export type ProposeInterestCorrectionInput = {
  invoiceDocumentId: number;
  /** Internal recursion guard for the central evidence audit. */
  skipEvidenceValidation?: boolean;
};

export type ProposeInterestCorrectionResult = {
  ok: boolean;
  invoiceDocumentId?: number;
  invoiceNumber?: string;
  hasProposal?: boolean;
  postedInterest?: number;
  lawfulInterest?: number;
  alreadyCorrected?: number;
  overClaimedAmount?: number;
  // The invoice's outstanding claim balance. A correction credits the receivable,
  // so it can only be booked against an outstanding receivable.
  outstandingClaimBalance?: number;
  possibleOutstandingInterestReceivable?: number;
  ambiguousInterestReceivable?: number;
  claimOverclaims?: Array<{
    claimId: number;
    claimDate: string;
    journalEntryId: number;
    overClaimedAmount: number;
  }>;
  // True when overClaimedAmount exceeds the outstanding balance — the excess was
  // already collected in cash and needs a REFUND, not a receivable credit.
  // postInterestCorrection refuses these.
  requiresRefund?: boolean;
  throughDate?: string;
  appliedRules: string[];
  errors: string[];
};

/**
 * Detect whether POSTED late-interest claims now exceed the lawful date-aware
 * interest for their windows — e.g. because a payment or credit note was later
 * recorded with an effective date inside an already-booked claim's window
 * (renteloven § 5: interest only on the amount actually outstanding). Read-only;
 * recommends a correcting credit of overClaimedAmount. Already-issued corrections
 * are netted off, so it never proposes correcting the same excess twice.
 */
export function proposeInterestCorrection(db: Database, input: ProposeInterestCorrectionInput): ProposeInterestCorrectionResult {
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["invoiceDocumentId must be a positive integer"] };
  }
  const status = getInvoiceStatus(
    db,
    input.invoiceDocumentId,
    undefined,
    { skipEvidenceValidation: input.skipEvidenceValidation },
  );
  if (!status.ok) return { ok: false, appliedRules: [RULE_ID], errors: status.errors };

  // ALL claims, with a posted flag. We need every claim — posted or not — to
  // anchor each posted claim's incremental window: a claim's amount_dkk covers
  // [previous claim, its own date], whichever claim precedes it, posted or not.
  const claims = db.query(
    `SELECT c.id, c.claim_date, c.reference_rate_percent, c.annual_interest_rate_percent,
            c.reference_rate_source, c.amount_dkk,
            p.journal_entry_id, (p.id IS NOT NULL) AS posted
     FROM invoice_interest_claims c
     LEFT JOIN invoice_interest_postings p ON p.interest_claim_id = c.id
     WHERE c.invoice_document_id = ?
     ORDER BY c.claim_date ASC, c.id ASC`,
  ).all(input.invoiceDocumentId) as Array<{
    id: number;
    claim_date: string;
    reference_rate_percent: number;
    annual_interest_rate_percent: number;
    reference_rate_source: "statutory-table" | "manual-override";
    amount_dkk: number;
    journal_entry_id: number | null;
    posted: number;
  }>;

  const effectiveDueDate = status.effectiveDueDate;
  // A correction may only credit the portion that is provably still attributable
  // to posted interest. Generic claim receipts can make a shared account
  // ambiguous; the conservative helper excludes that amount instead of silently
  // consuming an unrelated reminder or compensation receivable.
  const interestReceivables = calculateInterestReceivableBalances(db, {
    invoiceDocumentId: input.invoiceDocumentId,
    allowUnpostedClaims: true,
  });
  if (!interestReceivables.ok) {
    return { ok: false, appliedRules: [RULE_ID], errors: interestReceivables.errors };
  }
  const outstandingClaimBalance = interestReceivables.totalDkk;
  const base = {
    ok: true as const,
    invoiceDocumentId: input.invoiceDocumentId,
    invoiceNumber: status.invoiceNumber,
    outstandingClaimBalance,
    possibleOutstandingInterestReceivable: interestReceivables.possibleTotalDkk,
    ambiguousInterestReceivable: interestReceivables.ambiguousDkk,
    appliedRules: [RULE_ID],
    errors: [] as string[],
  };

  const lastClaimDate = claims.length ? claims[claims.length - 1]!.claim_date : undefined;
  const timeline = buildBalanceTimeline(status, effectiveDueDate ?? lastClaimDate ?? "1970-01-01");

  // Accrue each POSTED claim over its OWN incremental window [previous claim, its
  // date], anchored at the immediately preceding claim (posted or not) — exactly
  // as the claim's amount was billed. Collect every segment and round ONCE, so the
  // lawful figure matches the posted amounts window-for-window even when the posted
  // claims are NOT a contiguous run from the due date (a later claim may be posted
  // while an earlier one is left unposted).
  const postedSegments: InterestSegment[] = [];
  const rawClaimOverclaims: Array<{
    claimId: number;
    claimDate: string;
    journalEntryId: number;
    overClaimedAmount: number;
  }> = [];
  let postedInterest = 0;
  let throughDate: string | undefined;
  let prevDate = effectiveDueDate;
  for (const claim of claims) {
    if (claim.posted) {
      const claimSegments: InterestSegment[] = [];
      // Reconstruct this claim's lawful interest the SAME way it was billed
      // (JUR-7): a statutory-table claim whose window [prevDate, claim_date]
      // crossed a half-yearly rate change was billed with each half-year's own
      // rate, so it must be re-segmented here too — accruing it at the single
      // stored as-of rate would invent a phantom over- (or under-) claim. A
      // manual-override claim stays one deliberate rate for the whole window.
      if (claim.reference_rate_source === "statutory-table" && prevDate) {
        let segStart = prevDate;
        for (const w of claimRateWindows(
          prevDate,
          claim.claim_date,
          "statutory-table",
          Number(claim.annual_interest_rate_percent),
          Number(claim.reference_rate_percent),
        )) {
          claimSegments.push(...timeline.windowSegments(segStart, w.end, w.annualRatePercent));
          segStart = w.end;
        }
      } else {
        claimSegments.push(...timeline.windowSegments(prevDate, claim.claim_date, Number(claim.annual_interest_rate_percent)));
      }
      postedSegments.push(...claimSegments);
      const claimLawfulInterest = cumulativeInterestDkk(claimSegments);
      const claimOverClaimed = Math.max(0, roundDkk(Number(claim.amount_dkk) - claimLawfulInterest));
      if (claimOverClaimed > 0 && claim.journal_entry_id != null) {
        rawClaimOverclaims.push({
          claimId: claim.id,
          claimDate: claim.claim_date,
          journalEntryId: claim.journal_entry_id,
          overClaimedAmount: claimOverClaimed,
        });
      }
      postedInterest = addDkk(postedInterest, Number(claim.amount_dkk));
      throughDate = claim.claim_date;
    }
    prevDate = claim.claim_date;
  }

  if (throughDate === undefined) {
    return { ...base, hasProposal: false, postedInterest: 0, lawfulInterest: 0, alreadyCorrected: 0, overClaimedAmount: 0, requiresRefund: false, claimOverclaims: [] };
  }

  const lawfulInterest = cumulativeInterestDkk(postedSegments);
  postedInterest = roundDkk(postedInterest);
  const alreadyCorrected = roundDkk(
    Number(
      (
        db
          .query(`SELECT COALESCE(SUM(amount_dkk), 0) AS total FROM invoice_interest_corrections WHERE invoice_document_id = ?`)
          .get(input.invoiceDocumentId) as { total: number } | null
      )?.total ?? 0,
    ),
  );
  const overClaimedAmount = Math.max(
    0,
    roundDkk(subtractDkk(subtractDkk(postedInterest, lawfulInterest), alreadyCorrected)),
  );
  const grossCorrectionCeiling = Math.max(0, roundDkk(subtractDkk(postedInterest, lawfulInterest)));
  let remainingCeilingOre = toOre(grossCorrectionCeiling);
  const claimOverclaims: NonNullable<ProposeInterestCorrectionResult["claimOverclaims"]> = [];
  for (const candidate of rawClaimOverclaims) {
    if (remainingCeilingOre <= 0n) break;
    const availableOre = toOre(candidate.overClaimedAmount);
    const allocatedOre = availableOre < remainingCeilingOre ? availableOre : remainingCeilingOre;
    if (allocatedOre > 0n) {
      claimOverclaims.push({
        claimId: candidate.claimId,
        claimDate: candidate.claimDate,
        journalEntryId: candidate.journalEntryId,
        overClaimedAmount: fromOre(allocatedOre),
      });
      remainingCeilingOre -= allocatedOre;
    }
  }
  if (remainingCeilingOre > 0n && rawClaimOverclaims.length > 0) {
    const fallback = rawClaimOverclaims[rawClaimOverclaims.length - 1]!;
    const existing = claimOverclaims.find((row) => row.claimId === fallback.claimId);
    if (existing) existing.overClaimedAmount = fromOre(toOre(existing.overClaimedAmount) + remainingCeilingOre);
    else claimOverclaims.push({ ...fallback, overClaimedAmount: fromOre(remainingCeilingOre) });
    remainingCeilingOre = 0n;
  }
  // A correcting credit reverses the receivable, so it can only be booked against
  // an OUTSTANDING receivable. If the over-claim exceeds the open claim balance the
  // excess was already paid in cash and needs a refund, not a receivable credit.
  const requiresRefund = overClaimedAmount > outstandingClaimBalance;

  return {
    ...base,
    hasProposal: overClaimedAmount > 0,
    postedInterest,
    lawfulInterest,
    alreadyCorrected,
    overClaimedAmount,
    claimOverclaims,
    requiresRefund,
    throughDate,
  };
}

export type InterestCorrectionEvidencePlanResult =
  | {
      ok: true;
      receivableCredits: Array<{ accountNo: string; amountDkk: number }>;
      incomeDebits: Array<{ accountNo: string; amountDkk: number }>;
      causalClaims: Array<{
        claimId: number;
        claimDate: string;
        amountDkk: number;
        claimCeilingDkk: number;
      }>;
      earliestCorrectionDate: string | null;
    }
  | { ok: false; errors: string[] };

type CorrectionAccountBucket = { accountNo: string; remainingOre: bigint };
type CorrectionClaimBucket = {
  claimId: number;
  claimDate: string;
  ceilingOre: bigint;
  remainingOre: bigint;
  receivables: CorrectionAccountBucket[];
  incomes: CorrectionAccountBucket[];
};

function takeCorrectionAccounts(
  accounts: CorrectionAccountBucket[],
  amountOre: bigint,
): Map<string, bigint> | null {
  let remaining = amountOre;
  const taken = new Map<string, bigint>();
  for (const account of accounts) {
    if (remaining <= 0n) break;
    const amount = account.remainingOre < remaining ? account.remainingOre : remaining;
    if (amount <= 0n) continue;
    taken.set(account.accountNo, (taken.get(account.accountNo) ?? 0n) + amount);
    account.remainingOre -= amount;
    remaining -= amount;
  }
  return remaining === 0n ? taken : null;
}

function addCorrectionEffects(target: Map<string, bigint>, source: Map<string, bigint>) {
  for (const [accountNo, ore] of source) {
    target.set(accountNo, (target.get(accountNo) ?? 0n) + ore);
  }
}

/**
 * Validate every persisted interest correction against the lawful per-claim
 * overcharge and the exact accounts used by that claim. With nextAmount, also
 * return the only journal allocation that may be posted next.
 */
export function buildInterestCorrectionEvidencePlan(
  db: Database,
  input: { invoiceDocumentId: number; nextAmount?: number },
): InterestCorrectionEvidencePlanResult {
  const proposal = proposeInterestCorrection(db, {
    invoiceDocumentId: input.invoiceDocumentId,
    skipEvidenceValidation: true,
  });
  if (!proposal.ok) return { ok: false, errors: proposal.errors };

  const errors: string[] = [];
  const buckets: CorrectionClaimBucket[] = [];
  for (const claim of proposal.claimOverclaims ?? []) {
    const effects = db.query(
      `SELECT a.account_no, a.type AS account_type, a.normal_balance,
              SUM(jl.debit_amount) - SUM(jl.credit_amount) AS asset_effect,
              SUM(jl.credit_amount) - SUM(jl.debit_amount) AS income_effect
         FROM journal_lines jl
         JOIN accounts a ON a.id = jl.account_id
        WHERE jl.journal_entry_id = ?
        GROUP BY a.account_no, a.type, a.normal_balance
        ORDER BY a.account_no ASC`,
    ).all(claim.journalEntryId) as Array<{
      account_no: string;
      account_type: string;
      normal_balance: string;
      asset_effect: number;
      income_effect: number;
    }>;
    const authorisedOre = toOre(claim.overClaimedAmount);
    const receivableOrigins = effects
      .filter((row) => row.account_type === "asset" && row.normal_balance === "debit" && compareDkk(Number(row.asset_effect), 0) > 0)
      .map((row) => ({ accountNo: row.account_no, remainingOre: toOre(Number(row.asset_effect)) }));
    const incomeOrigins = effects
      .filter((row) => row.account_type === "income" && row.normal_balance === "credit" && compareDkk(Number(row.income_effect), 0) > 0)
      .map((row) => ({ accountNo: row.account_no, remainingOre: toOre(Number(row.income_effect)) }));
    const receivableAuthorised = takeCorrectionAccounts(receivableOrigins, authorisedOre);
    const incomeAuthorised = takeCorrectionAccounts(incomeOrigins, authorisedOre);
    if (!receivableAuthorised || !incomeAuthorised) {
      errors.push(`interest claim ${claim.claimId} does not expose ${claim.overClaimedAmount} DKK on its original receivable and income accounts`);
      continue;
    }
    buckets.push({
      claimId: claim.claimId,
      claimDate: claim.claimDate,
      ceilingOre: authorisedOre,
      remainingOre: authorisedOre,
      receivables: [...receivableAuthorised.entries()].map(([accountNo, remainingOre]) => ({ accountNo, remainingOre })),
      incomes: [...incomeAuthorised.entries()].map(([accountNo, remainingOre]) => ({ accountNo, remainingOre })),
    });
  }

  const allocate = (amountDkk: number) => {
    let remaining = toOre(amountDkk);
    const receivables = new Map<string, bigint>();
    const incomes = new Map<string, bigint>();
    const causalClaims: Array<{
      claimId: number;
      claimDate: string;
      amountOre: bigint;
      claimCeilingOre: bigint;
    }> = [];
    for (const bucket of buckets) {
      if (remaining <= 0n) break;
      const amount = bucket.remainingOre < remaining ? bucket.remainingOre : remaining;
      if (amount <= 0n) continue;
      const receivable = takeCorrectionAccounts(bucket.receivables, amount);
      const income = takeCorrectionAccounts(bucket.incomes, amount);
      if (!receivable || !income) return null;
      addCorrectionEffects(receivables, receivable);
      addCorrectionEffects(incomes, income);
      causalClaims.push({
        claimId: bucket.claimId,
        claimDate: bucket.claimDate,
        amountOre: amount,
        claimCeilingOre: bucket.ceilingOre,
      });
      bucket.remainingOre -= amount;
      remaining -= amount;
    }
    return remaining === 0n ? { receivables, incomes, causalClaims } : null;
  };

  const corrections = db.query(
    `SELECT c.id, c.correction_date, c.amount_dkk, c.journal_entry_id,
            j.transaction_date, j.document_id, j.currency,
            j.amount_foreign, j.amount_dkk AS journal_amount_dkk,
            j.fx_rate_to_dkk, j.source_bank_transaction_id,
            j.status, j.reversal_of_entry_id,
            (SELECT reversal.id FROM journal_entries reversal
              WHERE reversal.reversal_of_entry_id = j.id
              ORDER BY reversal.id ASC LIMIT 1) AS reversed_by_entry_id
       FROM invoice_interest_corrections c
       LEFT JOIN journal_entries j ON j.id = c.journal_entry_id
      WHERE c.invoice_document_id = ?
      ORDER BY c.journal_entry_id ASC, c.id ASC`,
  ).all(input.invoiceDocumentId) as Array<{
    id: number;
    correction_date: string;
    amount_dkk: number;
    journal_entry_id: number;
    transaction_date: string | null;
    document_id: number | null;
    currency: string | null;
    amount_foreign: number | null;
    journal_amount_dkk: number | null;
    fx_rate_to_dkk: number | null;
    source_bank_transaction_id: number | null;
    status: string | null;
    reversal_of_entry_id: number | null;
    reversed_by_entry_id: number | null;
  }>;
  for (const correction of corrections) {
    const amount = roundDkk(Number(correction.amount_dkk));
    const expected = allocate(amount);
    const label = `interest correction ${correction.id}`;
    if (!expected) {
      errors.push(`${label} exceeds the lawful per-claim interest-correction ceiling`);
      continue;
    }
    if (
      correction.transaction_date !== correction.correction_date ||
      correction.document_id !== input.invoiceDocumentId ||
      correction.status !== "posted" ||
      correction.reversal_of_entry_id != null ||
      correction.reversed_by_entry_id != null ||
      (correction.currency ?? "DKK").trim().toUpperCase() !== "DKK" ||
      correction.amount_foreign != null ||
      correction.journal_amount_dkk != null ||
      correction.fx_rate_to_dkk != null ||
      correction.source_bank_transaction_id != null
    ) {
      errors.push(`${label} journal ${correction.journal_entry_id} has invalid date, document, status, reversal, currency, FX, or bank context`);
      continue;
    }
    const earliestCorrectionDate = expected.causalClaims
      .map((claim) => claim.claimDate)
      .sort()
      .at(-1);
    if (earliestCorrectionDate && correction.correction_date < earliestCorrectionDate) {
      errors.push(`${label} predates its latest causal interest claim ${earliestCorrectionDate}`);
      continue;
    }
    const lines = db.query(
      `SELECT a.account_no, a.type AS account_type, a.normal_balance,
              SUM(jl.debit_amount) AS debit_dkk,
              SUM(jl.credit_amount) AS credit_dkk
         FROM journal_lines jl
         JOIN accounts a ON a.id = jl.account_id
        WHERE jl.journal_entry_id = ?
        GROUP BY a.account_no, a.type, a.normal_balance
        ORDER BY a.account_no ASC`,
    ).all(correction.journal_entry_id) as Array<{
      account_no: string;
      account_type: string;
      normal_balance: string;
      debit_dkk: number;
      credit_dkk: number;
    }>;
    const actualReceivables = new Map<string, bigint>();
    const actualIncomes = new Map<string, bigint>();
    let totalDebitOre = 0n;
    let totalCreditOre = 0n;
    let unsupported = false;
    for (const line of lines) {
      const debitOre = toOre(Number(line.debit_dkk));
      const creditOre = toOre(Number(line.credit_dkk));
      totalDebitOre += debitOre;
      totalCreditOre += creditOre;
      if (line.account_type === "income" && line.normal_balance === "credit" && debitOre > 0n && creditOre === 0n) {
        actualIncomes.set(line.account_no, debitOre);
      } else if (line.account_type === "asset" && line.normal_balance === "debit" && creditOre > 0n && debitOre === 0n) {
        actualReceivables.set(line.account_no, creditOre);
      } else {
        unsupported = true;
      }
    }
    const amountOre = toOre(amount);
    const accountKeys = new Set([
      ...expected.receivables.keys(),
      ...expected.incomes.keys(),
      ...actualReceivables.keys(),
      ...actualIncomes.keys(),
    ]);
    const accountMismatch = [...accountKeys].some((accountNo) =>
      (expected.receivables.get(accountNo) ?? 0n) !== (actualReceivables.get(accountNo) ?? 0n) ||
      (expected.incomes.get(accountNo) ?? 0n) !== (actualIncomes.get(accountNo) ?? 0n)
    );
    if (unsupported || totalDebitOre !== amountOre || totalCreditOre !== amountOre || accountMismatch) {
      errors.push(`${label} journal ${correction.journal_entry_id} does not exactly reverse its causal interest receivable and income accounts`);
    }
  }

  if (errors.length > 0) return { ok: false, errors: [...new Set(errors)] };
  if (input.nextAmount === undefined) {
    return {
      ok: true,
      receivableCredits: [],
      incomeDebits: [],
      causalClaims: [],
      earliestCorrectionDate: null,
    };
  }
  const next = allocate(input.nextAmount);
  if (!next) return { ok: false, errors: ["interest correction exceeds the lawful per-claim correction ceiling"] };
  const certainReceivables = calculateInterestReceivableBalances(db, {
    invoiceDocumentId: input.invoiceDocumentId,
    allowUnpostedClaims: true,
  });
  if (!certainReceivables.ok) return certainReceivables;
  const certainByAccount = new Map(certainReceivables.balances.map((row) => [row.accountNo, toOre(row.amountDkk)] as const));
  for (const [accountNo, ore] of next.receivables) {
    if (ore > (certainByAccount.get(accountNo) ?? 0n)) {
      return {
        ok: false,
        errors: [`interest correction on receivable ${accountNo} is not provably outstanding; resolve ambiguous claim receipts or use a refund`],
      };
    }
  }
  return {
    ok: true,
    receivableCredits: [...next.receivables.entries()].map(([accountNo, ore]) => ({ accountNo, amountDkk: fromOre(ore) })),
    incomeDebits: [...next.incomes.entries()].map(([accountNo, ore]) => ({ accountNo, amountDkk: fromOre(ore) })),
    causalClaims: next.causalClaims.map((claim) => ({
      claimId: claim.claimId,
      claimDate: claim.claimDate,
      amountDkk: fromOre(claim.amountOre),
      claimCeilingDkk: fromOre(claim.claimCeilingOre),
    })),
    earliestCorrectionDate: next.causalClaims.map((claim) => claim.claimDate).sort().at(-1) ?? null,
  };
}

export type PostInterestCorrectionInput = {
  invoiceDocumentId: number;
  transactionDate?: string;
  receivableAccountNo?: string;
  interestIncomeAccountNo?: string;
  reason?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type PostInterestCorrectionResult = JournalPostResult & {
  invoiceDocumentId?: number;
  invoiceNumber?: string;
  correctionId?: number;
  correctedAmount?: number;
  claimOpenBalance?: number;
};

/**
 * Book a correcting reversal of over-claimed late interest: debit interest income,
 * credit the receivable for proposeInterestCorrection's overClaimedAmount, and
 * record it in invoice_interest_corrections so getInvoiceStatus nets it off the
 * interest-claim balance. Refuses when there is nothing to correct. write-irreversible.
 */
export function postInterestCorrection(db: Database, input: PostInterestCorrectionInput): PostInterestCorrectionResult {
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) {
    return { ok: false, appliedRules: [BOOKKEEPING_RULE_ID], errors: ["invoiceDocumentId must be a positive integer"] };
  }
  try {
    return db.transaction(() => {
      // The proposal and both ledger allocations are mutable reads. Hold the
      // write lock before computing them so two processes cannot post the same
      // correction from one stale snapshot.
      const proposal = proposeInterestCorrection(db, { invoiceDocumentId: input.invoiceDocumentId });
      if (!proposal.ok) return { ok: false, appliedRules: [BOOKKEEPING_RULE_ID], errors: proposal.errors };
      if (!proposal.hasProposal || !(Number(proposal.overClaimedAmount ?? 0) > 0)) {
        return {
          ok: false,
          invoiceDocumentId: input.invoiceDocumentId,
          invoiceNumber: proposal.invoiceNumber,
          appliedRules: [BOOKKEEPING_RULE_ID],
          errors: ["no over-claimed late interest to correct on this invoice"],
        };
      }
      // A correcting credit reverses the receivable. If the over-claim exceeds
      // the outstanding claim balance the excess was already collected in cash.
      if (proposal.requiresRefund) {
        return {
          ok: false,
          invoiceDocumentId: input.invoiceDocumentId,
          invoiceNumber: proposal.invoiceNumber,
          appliedRules: [BOOKKEEPING_RULE_ID],
          errors: [
            `the over-claimed late interest (${roundDkk(Number(proposal.overClaimedAmount))}) exceeds the provably outstanding interest receivable (${roundDkk(Number(proposal.outstandingClaimBalance))}); it was settled in cash or shares an ambiguously allocated claim account — resolve the allocation or use a refund instead of an automatic correction`,
          ],
        };
      }

      const amount = roundDkk(Number(proposal.overClaimedAmount));
      const transactionDate = input.transactionDate ?? proposal.throughDate!;
      const invoiceNo = proposal.invoiceNumber;
      const allocationPlan = buildInterestCorrectionEvidencePlan(db, {
        invoiceDocumentId: input.invoiceDocumentId,
        nextAmount: amount,
      });
      if (!allocationPlan.ok) {
        return {
          ok: false,
          invoiceDocumentId: input.invoiceDocumentId,
          invoiceNumber: invoiceNo,
          correctedAmount: amount,
          appliedRules: [BOOKKEEPING_RULE_ID],
          errors: allocationPlan.errors,
        };
      }
      if (
        allocationPlan.earliestCorrectionDate &&
        transactionDate < allocationPlan.earliestCorrectionDate
      ) {
        return {
          ok: false,
          invoiceDocumentId: input.invoiceDocumentId,
          invoiceNumber: invoiceNo,
          correctedAmount: amount,
          appliedRules: [BOOKKEEPING_RULE_ID],
          errors: [
            `interest correction date ${transactionDate} cannot precede its latest causal interest claim ${allocationPlan.earliestCorrectionDate}`,
          ],
        };
      }
      if (
        input.receivableAccountNo &&
        (allocationPlan.receivableCredits.length !== 1 || allocationPlan.receivableCredits[0]!.accountNo !== input.receivableAccountNo)
      ) {
        return {
          ok: false,
          invoiceDocumentId: input.invoiceDocumentId,
          invoiceNumber: invoiceNo,
          correctedAmount: amount,
          appliedRules: [BOOKKEEPING_RULE_ID],
          errors: [`interest correction must credit the ledger-backed receivable allocation, not ${input.receivableAccountNo}`],
        };
      }
      if (
        input.interestIncomeAccountNo &&
        (allocationPlan.incomeDebits.length !== 1 || allocationPlan.incomeDebits[0]!.accountNo !== input.interestIncomeAccountNo)
      ) {
        return {
          ok: false,
          invoiceDocumentId: input.invoiceDocumentId,
          invoiceNumber: invoiceNo,
          correctedAmount: amount,
          appliedRules: [BOOKKEEPING_RULE_ID],
          errors: [`interest correction must debit the ledger-backed income allocation, not ${input.interestIncomeAccountNo}`],
        };
      }

      const journal = postJournalEntry(db, {
        transactionDate,
        text: `Late-interest correction ${invoiceNo}`,
        documentId: input.invoiceDocumentId,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines: [
          ...allocationPlan.incomeDebits.map((allocation) => ({
            accountNo: allocation.accountNo,
            debitAmount: allocation.amountDkk,
            text: `Late-interest income reversal ${invoiceNo}`,
          })),
          ...allocationPlan.receivableCredits.map((allocation) => ({
            accountNo: allocation.accountNo,
            creditAmount: allocation.amountDkk,
            text: `Late-interest receivable reversal ${invoiceNo}`,
          })),
        ],
      });
      if (!journal.ok) {
        return { ...journal, invoiceDocumentId: input.invoiceDocumentId, invoiceNumber: invoiceNo, correctedAmount: amount, appliedRules: [...new Set([...(journal.appliedRules ?? []), BOOKKEEPING_RULE_ID])] };
      }

      // The append-only correction row is accepted only when this exact,
      // transaction-local causal plan exists. Direct INSERTs therefore cannot
      // permanently poison the register before the TypeScript evidence audit
      // gets a chance to run. Header, causal claims, account allocations and
      // the correction itself commit or roll back together.
      runSql(db,
        `INSERT INTO invoice_interest_correction_plans
           (journal_entry_id, invoice_document_id, correction_date, amount_dkk)
         VALUES (?, ?, ?, ?)`,
        journal.entryId ?? null,
        input.invoiceDocumentId,
        transactionDate,
        amount,
      );
      for (const claim of allocationPlan.causalClaims) {
        runSql(db,
          `INSERT INTO invoice_interest_correction_plan_claims
             (journal_entry_id, interest_claim_id, claim_date, amount_dkk, claim_ceiling_dkk)
           VALUES (?, ?, ?, ?, ?)`,
          journal.entryId ?? null,
          claim.claimId,
          claim.claimDate,
          claim.amountDkk,
          claim.claimCeilingDkk,
        );
      }
      for (const allocation of allocationPlan.incomeDebits) {
        runSql(db,
          `INSERT INTO invoice_interest_correction_plan_lines
             (journal_entry_id, account_no, debit_amount, credit_amount)
           VALUES (?, ?, ?, 0)`,
          journal.entryId ?? null,
          allocation.accountNo,
          allocation.amountDkk,
        );
      }
      for (const allocation of allocationPlan.receivableCredits) {
        runSql(db,
          `INSERT INTO invoice_interest_correction_plan_lines
             (journal_entry_id, account_no, debit_amount, credit_amount)
           VALUES (?, ?, 0, ?)`,
          journal.entryId ?? null,
          allocation.accountNo,
          allocation.amountDkk,
        );
      }

      const inserted = db.query(
        `INSERT INTO invoice_interest_corrections (invoice_document_id, correction_date, amount_dkk, reason, journal_entry_id)
         VALUES (?, ?, ?, ?, ?) RETURNING id`,
      ).get(input.invoiceDocumentId, transactionDate, amount, input.reason ?? null, journal.entryId ?? null) as { id: number };

      insertAuditLog(db, {
        eventType: "invoice_interest_correction",
        entityType: "invoice_interest_correction",
        entityId: inserted.id,
        message: `Corrected over-claimed late interest ${amount} on invoice ${invoiceNo} in journal entry ${journal.entryNo}`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      const verifiedReceivables = calculateClaimReceivableBalances(db, {
        invoiceDocumentId: input.invoiceDocumentId,
        allowUnpostedClaims: true,
      });
      const verifiedInterestReceivables = calculateInterestReceivableBalances(db, {
        invoiceDocumentId: input.invoiceDocumentId,
        allowUnpostedClaims: true,
      });
      const verifiedIncomes = calculateInterestIncomeBalances(db, {
        invoiceDocumentId: input.invoiceDocumentId,
        allowUnpostedClaims: true,
      });
      const verifiedPlan = buildInterestCorrectionEvidencePlan(db, {
        invoiceDocumentId: input.invoiceDocumentId,
      });
      if (!verifiedReceivables.ok || !verifiedInterestReceivables.ok || !verifiedIncomes.ok || !verifiedPlan.ok) {
        throw new TransactionRejectionError({
          appliedRules: [BOOKKEEPING_RULE_ID],
          errors: [
            ...(!verifiedReceivables.ok ? verifiedReceivables.errors : []),
            ...(!verifiedInterestReceivables.ok ? verifiedInterestReceivables.errors : []),
            ...(!verifiedIncomes.ok ? verifiedIncomes.errors : []),
            ...(!verifiedPlan.ok ? verifiedPlan.errors : []),
          ],
        });
      }
      const statusAfter = getInvoiceStatus(db, input.invoiceDocumentId, transactionDate);
      if (!statusAfter.ok) {
        throw new TransactionRejectionError({
          appliedRules: [BOOKKEEPING_RULE_ID],
          errors: statusAfter.errors,
        });
      }
      return {
        ...journal,
        invoiceDocumentId: input.invoiceDocumentId,
        invoiceNumber: invoiceNo,
        correctionId: inserted.id,
        correctedAmount: amount,
        claimOpenBalance: statusAfter.claimOpenBalance,
        appliedRules: [...new Set([...(journal.appliedRules ?? []), BOOKKEEPING_RULE_ID])],
      };
    }).immediate();
  } catch (error) {
    const parsed = decodeTransactionRejection(error);
    return {
      ok: false,
      invoiceDocumentId: input.invoiceDocumentId,
      appliedRules: [BOOKKEEPING_RULE_ID],
      errors: (parsed?.errors as string[] | undefined) ?? [String(error)],
    };
  }
}
