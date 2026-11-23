import type { Database } from "bun:sqlite";
import { insertAuditLog, resolveActor, type ResolveActorInput } from "./actor";
import { ensureNullableVatPeriodColumn } from "./companies-schema";
import { isValidIsoDate as looksLikeIsoDate, addDays, todayIsoDate, MONTH_NAMES_DA } from "./dates";
import { loadVatAccountSemantics, VAT_LINE_CODES } from "./vat-account-semantics";
import { computePeriodCloseReadiness, loadPeriodCloseReview, reviewPeriodCloseReadiness, latestPeriodCloseDecision, recordForcedPeriodCloseOpenItems, recordPeriodCloseDecision, type CloseReadinessPacket, type CloseReviewPrincipal } from "./period-close-readiness";

/** `vat_period` is cadence-neutral; `vat_quarter` is read-only legacy compatibility. */
export type AccountingPeriodKind = "vat_period" | "vat_quarter" | "fiscal_year" | "custom";
export type AccountingPeriodStatus = "open" | "closed" | "reported";

/**
 * #289: the VAT settlement cadence a company is registered for with SKAT.
 * Danish businesses file monthly, quarterly or half-yearly VAT depending on
 * turnover. Rentemester historically hardcoded `quarter`; this type makes the
 * cadence an explicit, per-company setting so VAT periods and their deadlines
 * follow the company's real registration. `quarter` stays the default so
 * companies created before the setting existed are unaffected.
 */
export type VatPeriodType = "month" | "quarter" | "half-year";

/** The default VAT cadence — Rentemester's historical assumption. */
export const DEFAULT_VAT_PERIOD_TYPE: VatPeriodType = "quarter";

const VAT_PERIOD_TYPES = new Set<VatPeriodType>(["month", "quarter", "half-year"]);

/** Read only the cadence without importing `company.ts` (which depends here). */
function registeredVatPeriodType(db: Database): VatPeriodType | null {
  // Readiness must be a strictly read-only inspection.  Compatibility repair
  // belongs to `migrate()`, never to a close preflight: a missing legacy
  // column is interpreted through the documented historic default instead.
  const columns = db.query("PRAGMA table_info(companies)").all() as Array<{ name: string }>;
  if (!columns.some((column) => column.name === "vat_period_type")) return DEFAULT_VAT_PERIOD_TYPE;
  const row = db.query(
    "SELECT vat_period_type AS value FROM companies WHERE id = 1",
  ).get() as { value: string | null } | null;
  if (!row) return DEFAULT_VAT_PERIOD_TYPE;
  if (row.value === null) return null;
  return normalizeVatPeriodType(row.value) ?? DEFAULT_VAT_PERIOD_TYPE;
}

/**
 * Validate a VAT-period-type input. Accepts exactly `month`, `quarter` or
 * `half-year`; returns null on anything else so callers can surface a clear
 * error. Note this is the canonical machine value — distinct from the Danish
 * display label ("måned" / "kvartal" / "halvår").
 */
export function normalizeVatPeriodType(value?: string | null): VatPeriodType | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return VAT_PERIOD_TYPES.has(trimmed as VatPeriodType) ? (trimmed as VatPeriodType) : null;
}

/** Danish display label for a VAT period type — used in onboarding/help output. */
export function vatPeriodTypeLabelDa(type: VatPeriodType): string {
  switch (type) {
    case "month":
      return "måned";
    case "half-year":
      return "halvår";
    case "quarter":
    default:
      return "kvartal";
  }
}

/** Number of calendar months a single VAT period spans for each cadence. */
function vatPeriodMonthSpan(type: VatPeriodType): number {
  switch (type) {
    case "month":
      return 1;
    case "half-year":
      return 6;
    case "quarter":
    default:
      return 3;
  }
}

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

function isoUtc(year: number, month: number, day: number): string {
  return new Date(Date.UTC(year, month - 1, day)).toISOString().slice(0, 10);
}

/** Gregorian Easter Sunday (Meeus/Jones/Butcher), used for Danish holidays. */
function easterSunday(year: number): string {
  const a = year % 19;
  const b = Math.floor(year / 100);
  const c = year % 100;
  const d = Math.floor(b / 4);
  const e = b % 4;
  const f = Math.floor((b + 8) / 25);
  const g = Math.floor((b - f + 1) / 3);
  const h = (19 * a + b - d - g + 15) % 30;
  const i = Math.floor(c / 4);
  const k = c % 4;
  const l = (32 + 2 * e + 2 * i - h - k) % 7;
  const m = Math.floor((a + 11 * h + 22 * l) / 451);
  const month = Math.floor((h + l - 7 * m + 114) / 31);
  const day = ((h + l - 7 * m + 114) % 31) + 1;
  return isoUtc(year, month, day);
}

function danishBankHolidays(year: number): Set<string> {
  const easter = easterSunday(year);
  return new Set([
    `${year}-01-01`,
    addDays(easter, -3), // Skærtorsdag
    addDays(easter, -2), // Langfredag
    addDays(easter, 1), // 2. påskedag
    addDays(easter, 39), // Kristi himmelfartsdag
    addDays(easter, 50), // 2. pinsedag
    `${year}-12-25`,
    `${year}-12-26`,
  ]);
}

/** Move a statutory due date to the next Danish banking day. */
function nextDanishBankingDay(isoDate: string): string {
  let candidate = isoDate;
  for (;;) {
    const date = new Date(`${candidate}T00:00:00.000Z`);
    const day = date.getUTCDay();
    const year = date.getUTCFullYear();
    if (
      day !== 0 &&
      day !== 6 &&
      !danishBankHolidays(year).has(candidate)
    ) {
      return candidate;
    }
    candidate = addDays(candidate, 1);
  }
}

/**
 * Canonical SKAT filing/payment deadline for a registered VAT cadence.
 *
 * - month: the 25th of the following month;
 * - June month: the special statutory 17 August deadline;
 * - quarter/half-year: the 1st of the third month after period end; and
 * - a weekend/Danish bank holiday moves to the next banking day.
 *
 * The default remains `quarter` for source compatibility with older callers;
 * every live surface passes the company's registered cadence explicitly.
 */
export function vatFilingDeadline(
  periodEnd: string,
  type: VatPeriodType = DEFAULT_VAT_PERIOD_TYPE,
): string | null {
  if (!looksLikeIsoDate(periodEnd)) return null;
  const year = Number(periodEnd.slice(0, 4));
  const month = Number(periodEnd.slice(5, 7));

  let unshifted: string;
  if (type === "month") {
    if (month === 6) {
      unshifted = `${year}-08-17`;
    } else {
      const next = new Date(Date.UTC(year, month, 25));
      unshifted = isoUtc(
        next.getUTCFullYear(),
        next.getUTCMonth() + 1,
        next.getUTCDate(),
      );
    }
  } else {
    const thirdMonth = new Date(Date.UTC(year, month + 2, 1));
    unshifted = isoUtc(
      thirdMonth.getUTCFullYear(),
      thirdMonth.getUTCMonth() + 1,
      1,
    );
  }
  return nextDanishBankingDay(unshifted);
}

export type VatPeriodWindow = {
  /** First day of the VAT period (YYYY-MM-DD). */
  start: string;
  /** Last day of the VAT period (YYYY-MM-DD). */
  end: string;
  /** SKAT filing/payment deadline for this cadence, shifted to a banking day. */
  filingDeadline: string;
  /** The cadence this window was computed for. */
  vatPeriodType: VatPeriodType;
};

/**
 * #289: the VAT period window that contains `isoDate`, for a company on the
 * given VAT cadence. A monthly company gets a one-month window, a quarterly
 * company a three-month window, a half-yearly company a six-month window — so
 * the period (and therefore every VAT deadline derived from it) follows the
 * company's real SKAT registration instead of a hardcoded quarter.
 *
 * The filing deadline comes from the cadence-aware statutory engine above.
 */
export function vatPeriodWindowFor(isoDate: string, type: VatPeriodType): VatPeriodWindow {
  const year = Number(isoDate.slice(0, 4));
  const month = Number(isoDate.slice(5, 7)); // 1-based
  const span = vatPeriodMonthSpan(type);
  // The 1-based index of the period within the year, then its first month.
  const periodIndex = Math.floor((month - 1) / span);
  const startMonth = periodIndex * span + 1;
  const endMonth = startMonth + span - 1;
  // Day 0 of the next month is the last day of `endMonth`.
  const lastDay = new Date(Date.UTC(year, endMonth, 0)).getUTCDate();
  const end = `${year}-${pad2(endMonth)}-${pad2(lastDay)}`;
  return {
    start: `${year}-${pad2(startMonth)}-01`,
    end,
    filingDeadline: vatFilingDeadline(end, type)!,
    vatPeriodType: type,
  };
}

/**
 * #299: a short Danish display label for the VAT period a window describes.
 * Quarterly periods read "Q1 2026"; monthly periods read "Maj 2026"; half-year
 * periods read "1. halvår 2026" / "2. halvår 2026". A quarterly company keeps
 * the exact "Q<n> <year>" string the cockpit and dashboard have always shown,
 * so back-compat for the default cadence is byte-identical.
 */
export function vatPeriodLabel(window: VatPeriodWindow): string {
  const year = Number(window.start.slice(0, 4));
  const startMonth = Number(window.start.slice(5, 7)); // 1-based
  switch (window.vatPeriodType) {
    case "month":
      return `${MONTH_NAMES_DA[startMonth - 1]} ${year}`;
    case "half-year": {
      const half = startMonth <= 6 ? 1 : 2;
      return `${half}. halvår ${year}`;
    }
    case "quarter":
    default: {
      const quarter = Math.floor((startMonth - 1) / 3) + 1;
      return `Q${quarter} ${year}`;
    }
  }
}

/**
 * #299: every VAT period window that starts inside calendar `year`, for a
 * company on the given cadence — 12 for a monthly company, 4 for a quarterly
 * company, 2 for a half-yearly company, and `[]` when `type` is `null` (the
 * company is not VAT-registered). Returned in chronological order.
 *
 * This is the single source of truth for "which VAT periods does a company
 * have in a year" — the cockpit's per-period selection, the obligations list
 * and the dashboard all iterate it instead of hardcoding Q1..Q4, and they
 * automatically render zero VAT lines for a non-registered company.
 */
export function vatPeriodsForYear(year: number, type: VatPeriodType | null): VatPeriodWindow[] {
  if (type === null) return [];
  const span = vatPeriodMonthSpan(type);
  const windows: VatPeriodWindow[] = [];
  for (let startMonth = 1; startMonth <= 12; startMonth += span) {
    windows.push(vatPeriodWindowFor(`${year}-${pad2(startMonth)}-01`, type));
  }
  return windows;
}

/**
 * #300: writes the company's VAT settlement cadence onto the single
 * `companies` row. `type` is `month` / `quarter` / `half-year`, or `null` to
 * mark the company as NOT VAT-registered (DK-VAT-REGISTRATION-001) — every
 * VAT-aware surface gates on the null state. Used by `company set-profile`,
 * the cockpit's PATCH-profile endpoint and `init --no-vat`.
 *
 * Defensively ensures the `vat_period_type` column exists and is nullable
 * before writing — calls `ensureNullableVatPeriodColumn` directly so a
 * caller that forgot `migrate(db)` still gets the correct shape rather than
 * a SQLite "no such column" error. A CHECK constraint guards the value.
 * Returns whether the value actually changed.
 */
export function setCompanyVatPeriodType(
  db: Database,
  type: VatPeriodType | null,
  actor: ResolveActorInput = {},
): { ok: boolean; changed: boolean; errors: string[] } {
  if (type !== null && !VAT_PERIOD_TYPES.has(type)) {
    return {
      ok: false,
      changed: false,
      errors: ["vatPeriodType must be one of month, quarter, half-year, or null"],
    };
  }
  ensureNullableVatPeriodColumn(db);
  const before = db
    .query("SELECT vat_period_type AS t FROM companies WHERE id = 1")
    .get() as { t: string | null } | null;
  if (!before) {
    return {
      ok: false,
      changed: false,
      errors: ["company has not been initialised — run 'rentemester init' first"],
    };
  }
  const changed = before.t !== type;
  // A cadence is currently stored as one company-wide value, not as an
  // effective-dated registration history. Once VAT periods or VAT-classified
  // postings exist, changing to another non-null cadence would reinterpret
  // old activity and can make immutable closed periods overlap the new
  // windows. Refuse until an explicit effective-dated migration exists.
  if (changed && type !== null) {
    const existingVatPeriods = db
      .query(
        `SELECT id, period_start, period_end
           FROM accounting_periods
          WHERE kind IN ('vat_period', 'vat_quarter')`,
      )
      .all() as Array<{ id: number; period_start: string; period_end: string }>;
    const vatAccounts = loadVatAccountSemantics(db);
    const postedLines = db
      .query(
        `SELECT a.account_no, jl.vat_code, coalesce(je.reversal_of_entry_id, je.id) AS entry_id
           FROM journal_lines jl
           JOIN accounts a ON a.id = jl.account_id
           JOIN journal_entries je ON je.id = jl.journal_entry_id
          WHERE je.status = 'posted'`,
      )
      .all() as Array<{ account_no: string; vat_code: string | null; entry_id: number }>;
    const hasVatActivity = postedLines.some((line) => {
      const code = typeof line.vat_code === "string" ? line.vat_code.trim() : "";
      return vatAccounts.amountSideForEntry(line.account_no, line.entry_id) !== undefined || VAT_LINE_CODES.has(code);
    });
    const incompatibleRecoveryPeriod =
      before.t === null
        ? existingVatPeriods.find((period) => {
            const window = vatPeriodWindowFor(period.period_start, type);
            return (
              window.start !== period.period_start ||
              window.end !== period.period_end
            );
          })
        : undefined;
    const cannotRecoverPreviousCadence =
      before.t === null
        ? incompatibleRecoveryPeriod !== undefined ||
          (existingVatPeriods.length === 0 && hasVatActivity)
        : existingVatPeriods.length > 0 || hasVatActivity;
    if (cannotRecoverPreviousCadence) {
      return {
        ok: false,
        changed: false,
        errors: [
          incompatibleRecoveryPeriod
            ? `momsregistreringen kan ikke genaktiveres med ${type}: eksisterende momsperiode #${incompatibleRecoveryPeriod.id} ${incompatibleRecoveryPeriod.period_start}..${incompatibleRecoveryPeriod.period_end} følger en anden kadence`
            : "momsperiodens kadence kan ikke ændres, efter der er bogført momsaktivitet eller oprettet momsperioder; ændringen kræver en effective-dated kadencemigration, så historiske angivelser ikke omfortolkes eller strandes",
        ],
      };
    }
  }
  // Going from registered → not-registered must not silently strand posted
  // VAT activity (output VAT on a sale, deductible input VAT on a bilag).
  // Refuse when the ledger carries any posted entry on a `vat`-type account
  // whose date is NOT inside an EFFECTIVELY closed/reported VAT period — i.e.
  // any open VAT obligation that would otherwise lose its filing path. The
  // owner must close + indberette before deregistering.
  //
  // Coverage is judged by `effectivePeriodState`, NOT the period row's stored
  // status: a period that was closed and then reopened (#247) keeps its row at
  // `closed` while being effectively `open` again, so a raw `ap.status` check
  // would let a reopen→deregister sequence strand posted VAT activity — the
  // exact failure this guard exists to prevent. This mirrors the lens
  // `closeAccountingPeriod` / `validateJournalTransactionDate` already use.
  if (type === null && before.t !== null) {
    const vatAccounts = loadVatAccountSemantics(db);
    const vatEntryDates = (db
      .query(
        `SELECT DISTINCT je.transaction_date AS d, a.account_no AS account_no, coalesce(je.reversal_of_entry_id, je.id) AS entry_id
           FROM journal_lines jl
           JOIN accounts a ON a.id = jl.account_id
           JOIN journal_entries je ON je.id = jl.journal_entry_id
          WHERE je.status = 'posted'`,
      )
      .all() as Array<{ d: string; account_no: string; entry_id: number }>)
      .filter((row) => vatAccounts.amountSideForEntry(row.account_no, row.entry_id) !== undefined);
    if (vatEntryDates.length > 0) {
      const vatPeriods = db
        .query(
          `SELECT id, period_start, period_end, status
             FROM accounting_periods
            WHERE kind IN ('vat_period', 'vat_quarter')`,
        )
        .all() as Array<{
        id: number;
        period_start: string;
        period_end: string;
        status: AccountingPeriodStatus;
      }>;
      const settledPeriods = vatPeriods.filter((p) => {
        const state = effectivePeriodState(db, p.id, p.status);
        return state === "closed" || state === "reported";
      });
      const hasOpenVatActivity = vatEntryDates.some(
        (entry) =>
          !settledPeriods.some(
            (p) => entry.d >= p.period_start && entry.d <= p.period_end,
          ),
      );
      if (hasOpenVatActivity) {
        return {
          ok: false,
          changed: false,
          errors: [
            "selskabet har bogført momsaktivitet i en åben momsperiode — luk og indberet perioden før selskabet markeres som ikke-momsregistreret: kør 'period close' for at lukke momsperioden, derefter 'vat momsangivelse' for den indberetningsklare angivelse, og indberet via TastSelv",
          ],
        };
      }
    }
  }
  db.query("UPDATE companies SET vat_period_type = ? WHERE id = 1").run(type);
  if (changed) {
    // Itemize the registration-state transition in the append-only audit_log
    // so deregistering (registered → not-registered), re-registering, or a
    // cadence change is attributable and visible in the company history —
    // not a silent UPDATE. A deregistration in particular ends the company's
    // VAT obligation and must leave a trail.
    const label = (t: string | null) => (t === null ? "ikke momsregistreret" : t);
    const eventType =
      type === null
        ? "company_vat_deregistered"
        : before.t === null
          ? "company_vat_registered"
          : "company_vat_period_changed";
    insertAuditLog(db, {
      ...actor,
      eventType,
      entityType: "company",
      entityId: 1,
      message: `Momsregistrering ændret: ${label(before.t)} → ${label(type)}`,
    });
  }
  return { ok: true, changed, errors: [] };
}

export type CloseAccountingPeriodInput = {
  periodStart: string;
  periodEnd: string;
  kind?: AccountingPeriodKind;
  status?: Exclude<AccountingPeriodStatus, "open">;
  reference?: string;
  createdBy?: string;
  createdByProgram?: string;
  /** Stable evidence root used to verify the same reviewed packet at close. */
  companyRoot?: string;
  /**
   * Per round-2 review: closing a period with open high/medium exceptions
   * silently hides them — the default `exceptions list` filter no longer
   * includes them, so a user can lukke en periode og glemme to åbne
   * bankposter. The close call now refuses if any open high/medium
   * exception has a date inside the period. Set `force: true` to bypass
   * (the bypass + the open-exception count is logged to the audit chain).
   *
   * EJER-4: `force` also bypasses the unreconciled-bank-transactions guard —
   * the close otherwise refuses when the period contains bank transactions
   * without a posted journal entry (see `unreconciledBankTransactionsIn`).
   * A forced bypass is recorded on the period's close audit event.
   */
  force?: boolean;
  readinessPacketHash?: string;
  /** Exact durable review snapshot. A calculated packet is never closable. */
  readinessReviewId?: number;
  forceReason?: string;
  /** Trusted surface-derived, stable principal authority; actor never authorizes. */
  forceAuthorization?: { principal: CloseReviewPrincipal; permissions: readonly string[] };
  forceConfirmed?: boolean;
};

export type CloseAccountingPeriodResult = {
  ok: boolean;
  periodId?: number;
  periodStart?: string;
  periodEnd?: string;
  kind?: AccountingPeriodKind;
  status?: Exclude<AccountingPeriodStatus, "open">;
  reference?: string;
  appliedRules: string[];
  errors: string[];
  readinessPacket?: CloseReadinessPacket;
};

const PERIOD_RULE_ID = "DK-BOOKKEEPING-PERIOD-LOCK-001";
const PERIOD_KINDS = new Set<AccountingPeriodKind>(["vat_period", "vat_quarter", "fiscal_year", "custom"]);
const PERIOD_STATUSES = new Set<Exclude<AccountingPeriodStatus, "open">>(["closed", "reported"]);

function canonicalPeriodKind(kind: AccountingPeriodKind): AccountingPeriodKind {
  return kind === "vat_quarter" ? "vat_period" : kind;
}

/**
 * Audit-log event type that drives a controlled reopen. The *latest*
 * lifecycle event for an `accounting_period` entity is the authoritative
 * state — that is how `period reopen` works without ever mutating the
 * (immutable) period row: a reopen is an appended `period_reopen` fact, a
 * re-close a fresh `period_close` fact. See `effectivePeriodState`.
 */
const PERIOD_REOPEN_EVENT = "period_reopen";

/**
 * The append-only lifecycle of an accounting period.
 *
 * The `accounting_periods_guard_update` schema trigger makes a closed/reported
 * period row immutable — a row can never be moved back to `open`. A controlled
 * reopen (#247) therefore cannot mutate the row; instead it appends a
 * `period_reopen` event to the append-only, actor-attributed `audit_log`.
 *
 * `effectivePeriodState` replays those audit events for one period and returns
 * its current effective state: a period whose most recent lifecycle event is a
 * `period_reopen` is effectively `open` again (new postings allowed) even
 * though its row still reads `closed`. A later `period close` appends a fresh
 * `period_close` event and locks it again. Nothing is ever deleted, so the
 * full close/reopen history stays auditable forever.
 */
export type EffectivePeriodState = "open" | "closed" | "reported";

/**
 * Replays the append-only audit lifecycle for a single accounting period and
 * returns its effective state. `rowStatus` is the period row's stored status,
 * used as the baseline when no lifecycle audit events exist (older periods,
 * or periods closed before audit logging covered them).
 */
export function effectivePeriodState(
  db: Database,
  periodId: number,
  rowStatus: AccountingPeriodStatus,
): EffectivePeriodState {
  const events = db
    .query(
      `SELECT event_type
         FROM audit_log
        WHERE entity_type = 'accounting_period'
          AND entity_id = ?
        ORDER BY id ASC`,
    )
    .all(String(periodId)) as Array<{ event_type: string }>;

  let state: EffectivePeriodState = rowStatus;
  for (const ev of events) {
    // `reported` is TERMINAL (KODE-9): once a period has been submitted to the
    // authority (SKAT / Erhvervsstyrelsen) it can never be reopened — undoing
    // an authority filing is not a bookkeeping operation, and reopenAccounting-
    // Period already refuses it. A `period_reopen` event appended AFTER a
    // `period_report` (e.g. a raw audit_log INSERT bypassing that guard) must
    // therefore be ignored, so a reported period cannot be effectively reopened
    // and have new postings land in it.
    if (state === "reported") continue;
    if (ev.event_type === "period_report") state = "reported";
    else if (ev.event_type === "period_close") state = "closed";
    else if (ev.event_type === PERIOD_REOPEN_EVENT) state = "open";
  }
  return state;
}


/**
 * EJER-4: every bank transaction dated inside [periodStart, periodEnd] that
 * has NO posted journal entry linked via
 * `journal_entries.source_bank_transaction_id`. This is the same
 * "uafstemt" definition the bank reconciliation report
 * (`core/reconciliation.ts`) and the unmatched-bank exception sync
 * (`core/exceptions.ts#syncUnmatchedBankTransactionExceptions`) use, so the
 * close guard agrees byte-for-byte with what the Bank tab shows as
 * "Uafstemt". Booked internal transfers and settled invoices all carry a
 * posted journal entry with this link, so they never false-positive here;
 * a reversed entry correctly makes the transaction unreconciled again.
 *
 * Why the close must look here and not only at the exception queue: an
 * UNMATCHED_BANK_TRANSACTION exception can be "resolved" with a free-text
 * note WITHOUT booking anything. The open-exceptions guard then passes,
 * the period locks, the VAT filing is short, and the late booking is
 * rejected by the period lock — the exact EJER-4 incident.
 */
function unreconciledBankTransactionsIn(
  db: Database,
  periodStart: string,
  periodEnd: string,
): Array<{ id: number; transaction_date: string; text: string; amount: number; currency: string }> {
  return db.query(
    `SELECT bt.id, bt.transaction_date, bt.text, bt.amount, bt.currency
       FROM bank_transactions bt
       LEFT JOIN bank_journal_reconciliations br ON br.bank_transaction_id = bt.id
      WHERE br.journal_entry_id IS NULL
        AND bt.transaction_date BETWEEN ? AND ?
      ORDER BY bt.transaction_date ASC, bt.id ASC`,
  ).all(periodStart, periodEnd) as Array<{
    id: number;
    transaction_date: string;
    text: string;
    amount: number;
    currency: string;
  }>;
}

/** "1 uafstemt bankpostering" / "3 uafstemte bankposteringer" — Danish count phrase. */
function uafstemtePhrase(count: number): string {
  return count === 1 ? "1 uafstemt bankpostering" : `${count} uafstemte bankposteringer`;
}

function maxFutureDays() {
  const raw = process.env.RENTEMESTER_MAX_FUTURE_DAYS;
  if (raw === undefined) return 45;
  const parsed = Number(raw);
  return Number.isInteger(parsed) && parsed >= 0 ? parsed : 45;
}

export function validateJournalTransactionDate(db: Database, transactionDate: string, fieldName = "transactionDate") {
  const errors: string[] = [];
  if (!looksLikeIsoDate(transactionDate)) return errors;

  const latestAllowedDate = addDays(todayIsoDate(), maxFutureDays());
  if (transactionDate > latestAllowedDate) {
    errors.push(`${fieldName} ${transactionDate} cannot be later than ${latestAllowedDate}`);
  }

  // Every closed/reported period that covers the date. The row status alone
  // is not authoritative — a period may have been reopened via the
  // append-only `period reopen` path (#247), which leaves the row reading
  // `closed` while its effective state is `open`. Such a period must NOT
  // block new postings, so each candidate is replayed through its audit
  // lifecycle and only those still effectively locked are reported.
  const candidates = db.query(
    `SELECT id, period_start, period_end, kind, status, reference
       FROM accounting_periods
      WHERE period_start <= ?
        AND period_end >= ?
        AND status IN ('closed', 'reported')
      ORDER BY period_end ASC, id ASC`
  ).all(transactionDate, transactionDate) as Array<{
    id: number;
    period_start: string;
    period_end: string;
    kind: AccountingPeriodKind;
    status: Exclude<AccountingPeriodStatus, "open">;
    reference: string | null;
  }>;

  const blocking = candidates.find(
    (period) => effectivePeriodState(db, period.id, period.status) !== "open",
  );

  if (blocking) {
    const effective = effectivePeriodState(db, blocking.id, blocking.status);
    const referenceText = blocking.reference ? ` ref ${blocking.reference}` : "";
    errors.push(
      `${fieldName} ${transactionDate} falls in ${effective} period ${blocking.kind} ${blocking.period_start}..${blocking.period_end}${referenceText}`
    );
  }

  return errors;
}

function closeAccountingPeriodInImmediateTransaction(db: Database, input: CloseAccountingPeriodInput): CloseAccountingPeriodResult {
  const appliedRules = [PERIOD_RULE_ID];
  const errors: string[] = [];
  const periodStart = input.periodStart?.trim();
  const periodEnd = input.periodEnd?.trim();
  const requestedKind = (input.kind ?? "vat_period").trim() as AccountingPeriodKind;
  const status = (input.status ?? "closed").trim() as Exclude<AccountingPeriodStatus, "open">;
  const reference = input.reference?.trim();

  if (!looksLikeIsoDate(periodStart)) errors.push("periodStart must be YYYY-MM-DD");
  if (!looksLikeIsoDate(periodEnd)) errors.push("periodEnd must be YYYY-MM-DD");
  if (looksLikeIsoDate(periodStart) && looksLikeIsoDate(periodEnd) && periodStart > periodEnd) {
    errors.push("periodStart must be before or equal to periodEnd");
  }
  if (!PERIOD_KINDS.has(requestedKind)) errors.push("kind must be one of vat_period, vat_quarter (legacy), fiscal_year, custom");
  if (!PERIOD_STATUSES.has(status)) errors.push("status must be closed or reported");

  if (errors.length > 0) return { ok: false, appliedRules, errors };
  const kind = canonicalPeriodKind(requestedKind);
  // Reporting is a distinct terminal lifecycle transition. It must bind to
  // the original durable close packet, never recompute readiness after the
  // period is locked (which would make its own close evidence appear stale).
  if (status === "reported") {
    const existing = db.query("SELECT id,status,reference FROM accounting_periods WHERE period_start=? AND period_end=? AND kind=? ORDER BY id DESC LIMIT 1").get(periodStart, periodEnd, kind) as {id:number;status:AccountingPeriodStatus;reference:string|null}|null;
    if (existing?.status === "reported") {
      const latest = db.query("SELECT packet_hash FROM period_close_decisions WHERE period_id=? ORDER BY id DESC LIMIT 1").get(existing.id) as {packet_hash:string}|null;
      if (input.readinessPacketHash !== latest?.packet_hash) return {ok:false,appliedRules,errors:["PERIOD_REPORT_CLOSE_PACKET_MISMATCH"]};
      if (reference !== existing.reference) return {ok:false,appliedRules,errors:["PERIOD_REPORT_REFERENCE_CONFLICT"]};
      return {ok:true,periodId:existing.id,periodStart,periodEnd,kind,status,reference,appliedRules,errors};
    }
    if (existing?.status === "closed") {
      if (!reference) return { ok:false, appliedRules, errors:["PERIOD_REPORT_REFERENCE_REQUIRED"] };
      if (input.force) return { ok:false, appliedRules, errors:["PERIOD_REPORT_DOES_NOT_ALLOW_FORCE"] };
      const original = db.query("SELECT p.packet_json,d.packet_hash FROM period_close_decisions d JOIN period_close_readiness_packets p ON p.packet_hash=d.packet_hash WHERE d.period_id=? AND d.decision IN ('closed','forced_closed') ORDER BY d.id DESC LIMIT 1").get(existing.id) as {packet_json:string;packet_hash:string}|null;
      if (!original || input.readinessPacketHash !== original.packet_hash) return { ok:false, appliedRules, errors:["PERIOD_REPORT_CLOSE_PACKET_MISMATCH"] };
      const packet = JSON.parse(original.packet_json) as CloseReadinessPacket;
      // Some legacy ledgers carry a terminal `period_report` only in the
      // append-only lifecycle, while the immutable row still says `closed`.
      // A single attributable receipt backfill remains legal, but it must be
      // bound to the original close packet — never to a freshly computed one.
      if (effectivePeriodState(db, existing.id, "closed") === "reported" && existing.reference === null) {
        db.query("UPDATE accounting_periods SET status='reported',reported_at=COALESCE(reported_at,CURRENT_TIMESTAMP),reference=? WHERE id=? AND status='closed'").run(reference, existing.id);
        insertAuditLog(db,{eventType:"period_report_reference_backfill",entityType:"accounting_period",entityId:existing.id,message:`Backfilled filing reference for reported ${kind} period ${periodStart}..${periodEnd} (${reference})`,createdBy:input.createdBy,createdByProgram:input.createdByProgram});
        return {ok:true,periodId:existing.id,periodStart,periodEnd,kind,status,reference,appliedRules,errors,readinessPacket:packet};
      }
      db.query("UPDATE accounting_periods SET status='reported',reported_at=CURRENT_TIMESTAMP,reference=? WHERE id=? AND status='closed'").run(reference,existing.id);
      insertAuditLog(db,{eventType:"period_report",entityType:"accounting_period",entityId:existing.id,message:`Marked ${kind} period ${periodStart}..${periodEnd} reported (${reference})`,createdBy:input.createdBy,createdByProgram:input.createdByProgram});
      // The v25 decision table predates a distinct `reported` enum value;
      // retain the original close decision identity and record the terminal
      // reporting transition in the append-only audit lifecycle.
      recordPeriodCloseDecision(db,{periodId:existing.id,packet,decision:"closed",actor:input.createdBy??"system",reason:`reported:${reference}`,supersedesDecisionId:latestPeriodCloseDecision(db,existing.id)});
      return {ok:true,periodId:existing.id,periodStart,periodEnd,kind,status,reference,appliedRules,errors,readinessPacket:packet};
    }
  }
  const review = typeof input.readinessReviewId === "number" ? loadPeriodCloseReview(db, input.readinessReviewId) : null;
  const currentPacket = computePeriodCloseReadiness(db, { periodStart, periodEnd, companyRoot: input.companyRoot });
  if (!review || review.packet.periodStart !== periodStart || review.packet.periodEnd !== periodEnd || input.readinessPacketHash !== review.packet.hash || currentPacket.hash !== review.packet.hash) return { ok: false, appliedRules, errors: ["PERIOD_CLOSE_PACKET_STALE_OR_MISSING"], readinessPacket: currentPacket };
  const packet = review.packet;
  // A control which could not be run is not a successful control. In
  // particular, a close must never turn an absent receivables/control-account
  // assurance into an implicit waiver. Report an actual blocker first on a
  // normal close, then the unavailable assurance; force may waive only an
  // explicit waivable blocker and can never alter an unavailable result.
  const unavailableControls = packet.items.filter(item => item.status === "unavailable");
  const blockedControls = packet.items.filter(item => item.status === "blocked");
  if (!input.force && blockedControls.length > 0) return { ok: false, appliedRules, errors: [`PERIOD_CLOSE_BLOCKED:${blockedControls.length}`], readinessPacket: packet };
  if (!input.force && unavailableControls.length > 0) {
    return {
      ok: false,
      appliedRules,
      errors: [`PERIOD_CLOSE_ASSURANCE_UNAVAILABLE:${unavailableControls.length}`],
      readinessPacket: packet,
    };
  }
  const forceAuthorized = input.forceAuthorization?.permissions.includes("company.period.force-close") === true;
  if (input.force && (!input.forceReason?.trim() || !input.createdBy?.trim() || !forceAuthorized || input.forceConfirmed !== true)) return { ok: false, appliedRules, errors: ["FORCED_CLOSE_REQUIRES_COMPANY_PERIOD_FORCE_CLOSE_PERMISSION_CONFIRM_ACTOR_AND_REASON"], readinessPacket: packet };
  if (input.force && (unavailableControls.length > 0 || packet.items.some(item => !item.waivable && item.status === "blocked"))) return { ok: false, appliedRules, errors: ["PERIOD_CLOSE_HAS_NONWAIVABLE_BLOCKERS"], readinessPacket: packet };

  if (kind === "vat_period") {
    const vatPeriodType = registeredVatPeriodType(db);
    if (vatPeriodType === null) {
      return {
        ok: false,
        appliedRules,
        errors: ["selskabet er ikke momsregistreret og har derfor ingen momsperiode at lukke"],
      };
    }
    const canonicalWindow = vatPeriodWindowFor(periodStart, vatPeriodType);
    if (
      canonicalWindow.start !== periodStart ||
      canonicalWindow.end !== periodEnd
    ) {
      return {
        ok: false,
        appliedRules,
        errors: [
          `VAT period ${periodStart}..${periodEnd} does not match the company's registered ${vatPeriodType} cadence; the canonical period containing ${periodStart} is ${canonicalWindow.start}..${canonicalWindow.end}`,
        ],
      };
    }
  }

  // EJER-6: refuse to close a period that ends in the FUTURE. A period whose
  // end date has not arrived yet (typically a whole fiscal year closed early)
  // is not over — legitimate postings can still land on its remaining days, and
  // locking now would reject them at the period lock. `force: true` bypasses
  // (the same escape hatch as the other close guards). The future-close note is
  // recorded on the close audit event below.
  const today = todayIsoDate();
  const endsInFuture = periodEnd > today;
  if (!input.force && endsInFuture) {
    return {
      ok: false,
      appliedRules,
      errors: [
        `${kind} period ${periodStart}..${periodEnd} ends in the future (i dag er ${today}) — ` +
          `regnskabsåret/perioden er ikke omme endnu, så lukning ville afvise senere posteringer i perioden. ` +
          `Vent til perioden er slut, eller send force:true for at lukke alligevel (bypass audit-logges).`,
      ],
    };
  }

  // Round-2 review: refuse to silently hide open high/medium exceptions
  // by closing the period they fall in. The bypass is `force: true`.
  if (!input.force) {
    const open = db.query(
      `SELECT e.id, e.type, e.severity
         FROM exceptions e
         LEFT JOIN bank_transactions bt ON bt.id = e.related_bank_transaction_id
         LEFT JOIN documents d ON d.id = e.related_document_id
        WHERE e.status = 'open'
          AND e.severity IN ('high', 'medium')
          AND (
            (bt.transaction_date BETWEEN ? AND ?)
            OR (d.invoice_date BETWEEN ? AND ?)
          )
        ORDER BY e.id ASC`,
    ).all(periodStart, periodEnd, periodStart, periodEnd) as Array<{
      id: number;
      type: string;
      severity: "high" | "medium";
    }>;
    if (open.length > 0) {
      const summary = open
        .slice(0, 5)
        .map((e) => `#${e.id} (${e.severity}, ${e.type})`)
        .join(", ");
      const more = open.length > 5 ? `, +${open.length - 5} more` : "";
      return {
        ok: false,
        appliedRules,
        errors: [
          `${open.length} åbne high/medium-undtagelser i ${kind} ${periodStart}..${periodEnd}: ${summary}${more}. Løs dem først eller send force:true for at lukke alligevel (bypass logges).`,
        ],
      };
    }
  }

  // EJER-4: refuse to lock a period that still contains unreconciled bank
  // transactions — money moved in/out of the bank without a posted journal
  // entry means income/expense (and its VAT) is possibly unbooked, and the
  // period lock would reject the late booking. This guard reads the
  // reconciliation model directly, so a note-"resolved" exception cannot
  // hide the gap. `force: true` bypasses; the bypass is audit-logged below.
  const unreconciled = unreconciledBankTransactionsIn(db, periodStart, periodEnd);
  if (!input.force && unreconciled.length > 0) {
    const example = unreconciled[0];
    return {
      ok: false,
      appliedRules,
      errors: [
        `${uafstemtePhrase(unreconciled.length)} i ${kind} ${periodStart}..${periodEnd} uden bogført journalpostering ` +
          `(fx #${example.id} ${example.transaction_date} ${example.amount} ${example.currency} '${example.text}'). ` +
          `Indtægter/udgifter i perioden er muligvis ikke bogført — momsangivelsen bliver forkert. ` +
          `Bogfør eller afstem dem først, eller send force:true for at lukke alligevel (bypass audit-logges).`,
      ],
    };
  }
  // Recorded on the close audit event so a deliberate bypass is permanent,
  // attributable evidence — not just a CLI flag that leaves no trace.
  const bypassNotes: string[] = [];
  if (input.force && unreconciled.length > 0) {
    bypassNotes.push(`lukket trods ${uafstemtePhrase(unreconciled.length)} i perioden`);
  }
  if (input.force && endsInFuture) {
    // EJER-6: a deliberately forced future-period close is permanent evidence.
    bypassNotes.push(`lukket selvom perioden slutter i fremtiden (${periodEnd} > ${today})`);
  }
  const forceBypassNote = bypassNotes.length > 0 ? ` — force: ${bypassNotes.join("; ")}` : "";

  const overlap = (kind === "vat_period"
    ? db.query(
        `SELECT id, period_start, period_end, kind, status, reference
           FROM accounting_periods
          WHERE kind IN ('vat_period', 'vat_quarter')
            AND NOT (period_end < ? OR period_start > ?)
          ORDER BY CASE kind WHEN 'vat_period' THEN 0 ELSE 1 END, id ASC
          LIMIT 1`,
      ).get(periodStart, periodEnd)
    : db.query(
        `SELECT id, period_start, period_end, kind, status, reference
           FROM accounting_periods
          WHERE kind = ?
            AND NOT (period_end < ? OR period_start > ?)
          LIMIT 1`,
      ).get(kind, periodStart, periodEnd)) as
    | { id: number; period_start: string; period_end: string; kind: AccountingPeriodKind; status: AccountingPeriodStatus; reference: string | null }
    | null;

  if (overlap) {
    // An overlapping period that is *exactly* this period and has been
    // reopened (#247) is not a real conflict — it is the same period being
    // closed again. Re-close it by appending a fresh `period_close` event;
    // the immutable row keeps its original `closed`/`reported` status.
    const isSamePeriod = overlap.period_start === periodStart && overlap.period_end === periodEnd;
    const overlapEffective = effectivePeriodState(db, overlap.id, overlap.status);
    const lifecycleTransition =
      isSamePeriod &&
      (overlapEffective === "open" ||
        (overlapEffective === "closed" && status === "reported"));
    if (lifecycleTransition) {
      db.transaction(() => {
        if (status === "reported" || reference !== undefined) {
          db.query(
            `UPDATE accounting_periods
                SET status = ?,
                    reported_at = CASE WHEN ? = 'reported' THEN ? ELSE reported_at END,
                    reference = COALESCE(?, reference)
              WHERE id = ?`,
          ).run(
            status === "reported" ? "reported" : overlap.status,
            status,
            status === "reported" ? new Date().toISOString() : null,
            reference ?? null,
            overlap.id,
          );
        }
        insertAuditLog(db, {
          eventType: status === "reported" ? "period_report" : "period_close",
          entityType: "accounting_period",
          entityId: overlap.id,
          message:
            `${status === "reported" ? "Marked reported" : "Re-closed"} ${kind} period ${periodStart}..${periodEnd}` +
            `${reference ? ` (${reference})` : ""}${overlapEffective === "open" ? " after a controlled reopen" : ""}${forceBypassNote}`,
          createdBy: input.createdBy,
          createdByProgram: input.createdByProgram,
        });
        // A re-close after reopen is a fresh lifecycle decision, not merely
        // an audit-log line. Keep the exact reviewed packet and every waived
        // blocker linked to this new decision in the same transaction.
        const decisionId = recordPeriodCloseDecision(db, {
          periodId: overlap.id,
          packet,
          decision: input.force ? "forced_closed" : "closed",
          actor: input.createdBy ?? "system",
          reason: input.force ? input.forceReason : undefined,
          supersedesDecisionId: latestPeriodCloseDecision(db, overlap.id),
        });
        const waivedBlockers = packet.items.filter((item) => item.waivable && item.status === "blocked");
        if (input.force && waivedBlockers.length > 0) {
          recordForcedPeriodCloseOpenItems(db, overlap.id, decisionId, packet, input.forceReason!.trim(), input.createdBy!);
          insertAuditLog(db, { eventType: "PERIOD_CLOSED_WITH_OPEN_ITEMS", entityType: "accounting_period", entityId: overlap.id, message: `Forced re-close with ${waivedBlockers.length} waived readiness blocker(s): ${input.forceReason!.trim()}`, createdBy: input.createdBy, createdByProgram: input.createdByProgram });
        }
      })();
      const effectiveReference = reference ?? overlap.reference ?? undefined;
      return {
        ok: true,
        periodId: overlap.id,
        periodStart,
        periodEnd,
        kind,
        status,
        reference: effectiveReference,
        appliedRules,
        errors,
        readinessPacket: packet,
      };
    }
    if (isSamePeriod && overlapEffective === "reported" && status === "reported") {
      // Some legacy ledgers recorded `period_report` only in the append-only
      // audit lifecycle, leaving the row itself closed with no receipt. The
      // effective state is already terminal, but one attributable receipt
      // backfill must remain possible. Closed -> reported is permitted by the
      // row guard; once persisted, ordinary reported immutability applies.
      if (
        overlap.status === "closed" &&
        overlap.reference === null &&
        reference !== undefined
      ) {
        db.transaction(() => {
          db.query(
            `UPDATE accounting_periods
                SET status = 'reported',
                    reported_at = COALESCE(reported_at, ?),
                    reference = ?
              WHERE id = ?`,
          ).run(new Date().toISOString(), reference, overlap.id);
          insertAuditLog(db, {
            eventType: "period_report_reference_backfill",
            entityType: "accounting_period",
            entityId: overlap.id,
            message: `Backfilled filing reference for reported ${kind} period ${periodStart}..${periodEnd} (${reference})`,
            createdBy: input.createdBy,
            createdByProgram: input.createdByProgram,
          });
        })();
        return {
          ok: true,
          periodId: overlap.id,
          periodStart,
          periodEnd,
          kind,
          status,
          reference,
          appliedRules,
          errors,
        };
      }
      if (reference !== undefined && reference !== overlap.reference) {
        return {
          ok: false,
          appliedRules,
          errors: [
            `${kind} period ${periodStart}..${periodEnd} is already reported with reference ${overlap.reference ?? "(none)"}; its immutable filing reference cannot be replaced with ${reference}`,
          ],
        };
      }
      return {
        ok: true,
        periodId: overlap.id,
        periodStart,
        periodEnd,
        kind,
        status,
        reference: overlap.reference ?? undefined,
        appliedRules,
        errors,
      };
    }
    return {
      ok: false,
      appliedRules,
      errors: [`${kind} period ${periodStart}..${periodEnd} overlaps existing period ${overlap.period_start}..${overlap.period_end}`],
    };
  }

  const inserted = db.query(
    `INSERT INTO accounting_periods (
      period_start, period_end, kind, status, closed_at, closed_by, reported_at, reference
    ) VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP, ?, ?, ?)
    RETURNING id`
  ).get(
    periodStart,
    periodEnd,
    kind,
    status,
    input.createdBy ?? null,
    status === "reported" ? new Date().toISOString() : null,
    reference ?? null,
  ) as { id: number };

  insertAuditLog(db, {
    eventType: status === "reported" ? "period_report" : "period_close",
    entityType: "accounting_period",
    entityId: inserted.id,
    message: `${status === "reported" ? "Marked" : "Closed"} ${kind} period ${periodStart}..${periodEnd}${reference ? ` (${reference})` : ""}${forceBypassNote}`,
    createdBy: input.createdBy,
    createdByProgram: input.createdByProgram,
  });
  const decisionId = recordPeriodCloseDecision(db, { periodId: inserted.id, packet, decision: input.force ? "forced_closed" : "closed", actor: input.createdBy ?? "system", reason: input.force ? input.forceReason : undefined });
  const waivedBlockers = packet.items.filter((item) => item.waivable && item.status === "blocked");
  if (input.force && waivedBlockers.length > 0) {
    recordForcedPeriodCloseOpenItems(db, inserted.id, decisionId, packet, input.forceReason!.trim(), input.createdBy!);
    insertAuditLog(db, { eventType: "PERIOD_CLOSED_WITH_OPEN_ITEMS", entityType: "accounting_period", entityId: inserted.id, message: `Forced close with ${waivedBlockers.length} waived readiness blocker(s): ${input.forceReason!.trim()}`, createdBy: input.createdBy, createdByProgram: input.createdByProgram });
  }

  return {
    ok: true,
    periodId: inserted.id,
    periodStart,
    periodEnd,
    kind,
    status,
    reference,
    appliedRules,
    errors,
    readinessPacket: packet,
  };
}

/**
 * Close is one SQLite BEGIN IMMEDIATE boundary.  It deliberately owns the
 * review/hash recheck, period transition, audit decision and obligations so a
 * concurrent posting cannot slip between a successful check and the lock.
 */
export function closeAccountingPeriod(db: Database, input: CloseAccountingPeriodInput): CloseAccountingPeriodResult {
  return db.transaction(() => closeAccountingPeriodInImmediateTransaction(db, input)).immediate();
}

export type ReopenAccountingPeriodInput = {
  periodStart: string;
  periodEnd: string;
  kind?: AccountingPeriodKind;
  /** Mandatory free-text justification — recorded verbatim in the audit log. */
  reason: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type ReopenAccountingPeriodResult = {
  ok: boolean;
  periodId?: number;
  periodStart?: string;
  periodEnd?: string;
  kind?: AccountingPeriodKind;
  /** The period's effective state AFTER the reopen — `open` on success. */
  effectiveStatus?: EffectivePeriodState;
  /** The audit actor the reopen was attributed to. */
  reopenedBy?: string;
  reason?: string;
  appliedRules: string[];
  errors: string[];
};

/**
 * Controlled, fully audit-logged reopen of a closed accounting period (#247).
 *
 * The previous `period close` help honestly stated there was no reopen path
 * and that an early close "kun [kan] rettes ved at redigere ledger-databasen
 * direkte" — a dead end for a non-technical owner. This is the supported
 * recovery path.
 *
 * Integrity is kept intact precisely *because* nothing is overwritten:
 *  - The `accounting_periods` row is never mutated — the schema trigger keeps
 *    it immutable, and its `closed_at`/`closed_by`/bounds stay as historical
 *    record.
 *  - The reopen is a NEW row appended to the append-only `audit_log`, carrying
 *    the actor, timestamp and the mandatory `reason`. It is therefore fully
 *    attributable and can never be silently undone.
 *  - A `reported` period (already submitted to SKAT/Erhvervsstyrelsen) is
 *    refused — undoing an authority filing is not a bookkeeping operation.
 *
 * After a reopen, postings with a transaction date inside the period are
 * accepted again (`validateJournalTransactionDate` replays the audit
 * lifecycle). A later `period close` re-locks the same period.
 */
export function reopenAccountingPeriod(
  db: Database,
  input: ReopenAccountingPeriodInput,
): ReopenAccountingPeriodResult {
  const appliedRules = [PERIOD_RULE_ID];
  const errors: string[] = [];
  const periodStart = input.periodStart?.trim();
  const periodEnd = input.periodEnd?.trim();
  const requestedKind = (input.kind ?? "vat_period").trim() as AccountingPeriodKind;
  const reason = input.reason?.trim();

  if (!looksLikeIsoDate(periodStart)) errors.push("periodStart must be YYYY-MM-DD");
  if (!looksLikeIsoDate(periodEnd)) errors.push("periodEnd must be YYYY-MM-DD");
  if (looksLikeIsoDate(periodStart) && looksLikeIsoDate(periodEnd) && periodStart > periodEnd) {
    errors.push("periodStart must be before or equal to periodEnd");
  }
  if (!PERIOD_KINDS.has(requestedKind)) errors.push("kind must be one of vat_period, vat_quarter (legacy), fiscal_year, custom");
  if (!reason) errors.push("reason is required: a reopen must record why the period is being reopened");

  if (errors.length > 0) return { ok: false, appliedRules, errors };
  const kind = canonicalPeriodKind(requestedKind);
  if (kind === "vat_period" && registeredVatPeriodType(db) === null) {
    return {
      ok: false,
      appliedRules,
      errors: [
        "en momsperiode kan ikke genåbnes, mens selskabet ikke er momsregistreret; genaktivér først den kompatible moms-kadence",
      ],
    };
  }

  const period = (kind === "vat_period"
    ? db.query(
        `SELECT id, period_start, period_end, kind, status
           FROM accounting_periods
          WHERE period_start = ? AND period_end = ?
            AND kind IN ('vat_period', 'vat_quarter')
          ORDER BY CASE kind WHEN 'vat_period' THEN 0 ELSE 1 END, id DESC
          LIMIT 1`,
      ).get(periodStart, periodEnd)
    : db.query(
        `SELECT id, period_start, period_end, kind, status
           FROM accounting_periods
          WHERE period_start = ? AND period_end = ? AND kind = ?
          LIMIT 1`,
      ).get(periodStart, periodEnd, kind)) as
    | { id: number; period_start: string; period_end: string; kind: AccountingPeriodKind; status: AccountingPeriodStatus }
    | null;

  if (!period) {
    return {
      ok: false,
      appliedRules,
      errors: [`no ${kind} period ${periodStart}..${periodEnd} exists to reopen`],
    };
  }

  const effective = effectivePeriodState(db, period.id, period.status);

  if (effective === "open") {
    return {
      ok: false,
      appliedRules,
      errors: [`${kind} period ${periodStart}..${periodEnd} is already open — nothing to reopen`],
    };
  }

  if (effective === "reported") {
    return {
      ok: false,
      appliedRules,
      errors: [
        `${kind} period ${periodStart}..${periodEnd} is reported (already submitted to the authority) ` +
          `and cannot be reopened; correct it with a new posting in an open period instead`,
      ],
    };
  }

  // effective === 'closed' — append the reopen fact. The actor resolves the
  // same way every mutating command does (--actor / RENTEMESTER_ACTOR / env),
  // so the reopen is always clearly attributable.
  const actor = resolveActor({ createdBy: input.createdBy, createdByProgram: input.createdByProgram });
  insertAuditLog(db, {
    eventType: PERIOD_REOPEN_EVENT,
    entityType: "accounting_period",
    entityId: period.id,
    message: `Reopened ${kind} period ${periodStart}..${periodEnd} — reason: ${reason}`,
    createdBy: input.createdBy,
    createdByProgram: input.createdByProgram,
  });
  // A reopen never mutates the original close evidence. It appends a decision
  // linked to a freshly persisted snapshot and explicitly supersedes the last
  // lifecycle decision for this period.
  const reopenPacket = computePeriodCloseReadiness(db, { periodStart, periodEnd });
  reviewPeriodCloseReadiness(db, { packet: reopenPacket, reviewerActor: actor.createdBy, reviewerPrincipal: { kind: "local-trusted", subjectId: actor.createdBy } });
  recordPeriodCloseDecision(db, { periodId: period.id, packet: reopenPacket, decision: "reopened", actor: actor.createdBy, reason, supersedesDecisionId: latestPeriodCloseDecision(db, period.id) });

  return {
    ok: true,
    periodId: period.id,
    periodStart,
    periodEnd,
    kind,
    effectiveStatus: "open",
    reopenedBy: actor.auditActor,
    reason,
    appliedRules,
    errors,
  };
}
