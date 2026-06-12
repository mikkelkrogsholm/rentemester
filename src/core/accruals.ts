/**
 * Accruals / periodeafgrænsningsposter.
 *
 * The append-only ledger has period locking but no support for matching a cost
 * or revenue to the period it belongs to. This module adds that, entirely
 * within the ledger discipline — balanced double-entry, append-only, reversals
 * not edits, an audit trail and a rule version on every posting.
 *
 * Three accrual types are supported:
 *
 *  - `prepaid_expense` (forudbetalt omkostning): a cost already paid that
 *    belongs to one or more later periods. Registration debits a balance-sheet
 *    asset (1300 Forudbetalte omkostninger), crediting a settlement account.
 *    Each recognition period moves one slice off the asset and onto an expense
 *    account.
 *
 *  - `accrued_expense` (skyldig omkostning): a cost that belongs to the current
 *    period but is not yet invoiced/paid. Registration credits a balance-sheet
 *    liability (7300 Skyldige omkostninger), debiting an expense account, so
 *    the cost lands in the right period straight away; the unwind period(s)
 *    settle the liability against a settlement/payment account.
 *
 *  - `deferred_revenue` (forudbetalt indtægt): cash received for a service not
 *    yet delivered. Registration credits a balance-sheet liability (7310
 *    Forudbetalt indtægt), debiting a settlement account. Each recognition
 *    period moves one slice off the liability and onto an income account.
 *
 * The amount is split as evenly as integer-øre arithmetic allows across
 * `recognitionPeriods`; the final period carries the rounding remainder so the
 * schedule sums exactly to the registered amount. The schedule is a pure
 * function of the header, so identical inputs always yield identical output.
 *
 * Rentemester assists the bookkeeping workflow; the user/advisor remains
 * responsible for choosing the period split, the accounts and the accounting
 * treatment.
 */

import type { Database } from "bun:sqlite";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { insertAuditLog } from "./actor";
import { isValidIsoDate as looksLikeIsoDate, diffDays } from "./dates";
import { fromOre, roundDkk, sumDkk, toOre } from "./money";

const ACCRUAL_RULE_ID = "DK-BOOKKEEPING-ACCRUAL-001";

export type AccrualType = "prepaid_expense" | "accrued_expense" | "deferred_revenue";

const ACCRUAL_TYPES = new Set<AccrualType>([
  "prepaid_expense",
  "accrued_expense",
  "deferred_revenue",
]);

/**
 * Default chart-of-accounts numbers for each accrual type's balance-sheet
 * parking account (seeded by `seedAccounts()` in `ledger.ts`).
 */
const DEFAULT_BALANCE_ACCOUNT: Record<AccrualType, string> = {
  prepaid_expense: "1300", // Forudbetalte omkostninger (asset)
  accrued_expense: "7300", // Skyldige omkostninger (liability)
  deferred_revenue: "7310", // Forudbetalt indtægt (liability)
};

/** Default settlement (payment) account — the company bank account. */
const DEFAULT_SETTLEMENT_ACCOUNT = "2000";

/**
 * One recognition period of an accrual's schedule. `periodIndex` is 1-based;
 * `recognitionDate` is the YYYY-MM-DD posting date for that period.
 */
export type AccrualSchedulePeriod = {
  periodIndex: number;
  recognitionDate: string;
  amount: number;
};

export type ComputeAccrualScheduleInput = {
  totalAmount: number;
  recognitionPeriods: number;
  firstRecognitionDate: string;
  /** Calendar months between consecutive recognition periods (default 1). */
  periodStepMonths?: number;
};

/**
 * Advance a YYYY-MM-DD date by `months` whole calendar months, clamping the
 * day-of-month to the last day of the target month (so 2026-01-31 + 1 month is
 * 2026-02-28). UTC-based.
 */
function addMonthsClamped(isoDate: string, months: number): string {
  const year = Number(isoDate.slice(0, 4));
  const month = Number(isoDate.slice(5, 7)); // 1-based
  const day = Number(isoDate.slice(8, 10));
  const zeroBased = month - 1 + months;
  const targetYear = year + Math.floor(zeroBased / 12);
  const targetMonth = ((zeroBased % 12) + 12) % 12; // 0-based
  const lastDay = new Date(Date.UTC(targetYear, targetMonth + 1, 0)).getUTCDate();
  const clampedDay = Math.min(day, lastDay);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${targetYear}-${pad(targetMonth + 1)}-${pad(clampedDay)}`;
}

/**
 * Deterministic accrual recognition schedule.
 *
 * The total is split as evenly as integer-øre arithmetic allows; the final
 * period carries the rounding remainder so the schedule sums exactly to the
 * registered amount. Pure function — identical inputs always yield identical
 * output.
 */
export function computeAccrualSchedule(
  input: ComputeAccrualScheduleInput,
): AccrualSchedulePeriod[] {
  const periods = input.recognitionPeriods;
  if (!Number.isInteger(periods) || periods <= 0) {
    throw new Error("recognitionPeriods must be a positive integer");
  }
  const stepMonths = input.periodStepMonths ?? 1;
  if (!Number.isInteger(stepMonths) || stepMonths <= 0) {
    throw new Error("periodStepMonths must be a positive integer");
  }
  if (!looksLikeIsoDate(input.firstRecognitionDate)) {
    throw new Error("firstRecognitionDate must be YYYY-MM-DD");
  }
  const totalOre = toOre(roundDkk(input.totalAmount));
  if (totalOre <= 0n) throw new Error("totalAmount must be positive");

  const periodsBig = BigInt(periods);
  const baseOre = totalOre / periodsBig;
  const remainderOre = totalOre - baseOre * periodsBig;

  const schedule: AccrualSchedulePeriod[] = [];
  for (let i = 0; i < periods; i += 1) {
    // The remainder is concentrated in the final period so every period before
    // it gets an identical amount and the total reconciles exactly.
    const periodOre = i === periods - 1 ? baseOre + remainderOre : baseOre;
    schedule.push({
      periodIndex: i + 1,
      recognitionDate: addMonthsClamped(input.firstRecognitionDate, i * stepMonths),
      amount: fromOre(periodOre),
    });
  }
  return schedule;
}

export type RegisterAccrualInput = {
  accrualType: AccrualType;
  description: string;
  totalAmount: number;
  recognitionPeriods: number;
  firstRecognitionDate: string;
  /** Posting date of the registration entry (default: firstRecognitionDate). */
  registrationDate?: string;
  periodStepMonths?: number;
  /**
   * The income-statement account each period is recognised on — an expense
   * account for prepaid/accrued expenses, an income account for deferred
   * revenue.
   */
  resultAccountNo: string;
  /** Balance-sheet parking account; defaults per accrual type. */
  balanceAccountNo?: string;
  /**
   * The settlement/payment account used on the registration entry's other
   * leg (default 2000 Bank).
   */
  settlementAccountNo?: string;
  documentId?: number;
  note?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type RegisterAccrualResult = JournalPostResult & {
  accrualId?: number;
  accrualType?: AccrualType;
  totalPeriods?: number;
  periodAmount?: number;
};

type AccountRow = { account_no: string; type: string; active: number };

function loadAccount(db: Database, accountNo: string): AccountRow | null {
  return db
    .query("SELECT account_no, type, active FROM accounts WHERE account_no = ?")
    .get(accountNo) as AccountRow | null;
}

/**
 * Build the registration journal entry's lines for an accrual type.
 *
 * Every variant is a balanced two-line double entry. The expense/income leg is
 * what makes the entry require document evidence (`postJournalEntry` enforces
 * `documentId` whenever an expense or income account is touched).
 */
function registrationLines(
  accrualType: AccrualType,
  amount: number,
  balanceAccountNo: string,
  resultAccountNo: string,
  settlementAccountNo: string,
  description: string,
): Array<{ accountNo: string; debitAmount?: number; creditAmount?: number; text: string }> {
  switch (accrualType) {
    case "prepaid_expense":
      // Park the prepaid cost on a balance-sheet asset; the cash already left.
      return [
        { accountNo: balanceAccountNo, debitAmount: amount, text: `Forudbetalt omkostning: ${description}` },
        { accountNo: settlementAccountNo, creditAmount: amount, text: `Betaling: ${description}` },
      ];
    case "accrued_expense":
      // The cost belongs to this period: recognise the expense now, against a
      // liability that the unwind period(s) will settle.
      return [
        { accountNo: resultAccountNo, debitAmount: amount, text: `Skyldig omkostning: ${description}` },
        { accountNo: balanceAccountNo, creditAmount: amount, text: `Skyldig omkostning (passiv): ${description}` },
      ];
    case "deferred_revenue":
      // Cash received but not yet earned: park it as a liability.
      return [
        { accountNo: settlementAccountNo, debitAmount: amount, text: `Indbetaling: ${description}` },
        { accountNo: balanceAccountNo, creditAmount: amount, text: `Forudbetalt indtægt: ${description}` },
      ];
  }
}

/**
 * Build one recognition period's journal entry lines. This is the leg that
 * moves a slice between the balance-sheet parking account and the
 * income-statement account.
 */
function recognitionLines(
  accrualType: AccrualType,
  amount: number,
  balanceAccountNo: string,
  resultAccountNo: string,
  settlementAccountNo: string,
  description: string,
  periodIndex: number,
  totalPeriods: number,
): Array<{ accountNo: string; debitAmount?: number; creditAmount?: number; text: string }> {
  const label = `${description} (periode ${periodIndex}/${totalPeriods})`;
  switch (accrualType) {
    case "prepaid_expense":
      // Recognise the expense, releasing one slice off the prepaid asset.
      return [
        { accountNo: resultAccountNo, debitAmount: amount, text: `Periodiseret omkostning: ${label}` },
        { accountNo: balanceAccountNo, creditAmount: amount, text: `Frigivet forudbetaling: ${label}` },
      ];
    case "accrued_expense":
      // Settle one slice of the accrued-expense liability. The expense was
      // already recognised at registration, so the unwind clears the liability
      // against the settlement account (the later payment).
      return [
        { accountNo: balanceAccountNo, debitAmount: amount, text: `Afvikling skyldig omkostning: ${label}` },
        { accountNo: settlementAccountNo, creditAmount: amount, text: `Betaling skyldig omkostning: ${label}` },
      ];
    case "deferred_revenue":
      // Recognise the revenue, releasing one slice off the deferred-revenue
      // liability.
      return [
        { accountNo: balanceAccountNo, debitAmount: amount, text: `Indtægtsført forudbetaling: ${label}` },
        { accountNo: resultAccountNo, creditAmount: amount, text: `Periodiseret indtægt: ${label}` },
      ];
  }
}

/**
 * Register a periodeafgrænsningspost: post the balanced registration entry that
 * parks the amount on a balance-sheet accrual account, and store the header so
 * each later period can be recognised against a deterministic schedule.
 *
 * The accrual is append-only and references the registration journal entry so
 * a later correction is always a reversal, never an edit.
 */
export function registerAccrual(
  db: Database,
  input: RegisterAccrualInput,
): RegisterAccrualResult {
  const errors: string[] = [];
  if (!ACCRUAL_TYPES.has(input.accrualType)) {
    errors.push("accrualType must be one of prepaid_expense, accrued_expense, deferred_revenue");
  }
  if (typeof input.description !== "string" || input.description.trim().length === 0) {
    errors.push("description is required");
  }
  if (!Number.isFinite(input.totalAmount) || input.totalAmount <= 0) {
    errors.push("totalAmount must be a positive number");
  }
  if (!Number.isInteger(input.recognitionPeriods) || input.recognitionPeriods <= 0) {
    errors.push("recognitionPeriods must be a positive integer");
  }
  if (!looksLikeIsoDate(input.firstRecognitionDate)) {
    errors.push("firstRecognitionDate must be YYYY-MM-DD");
  }
  const registrationDate = input.registrationDate ?? input.firstRecognitionDate;
  if (input.registrationDate !== undefined && !looksLikeIsoDate(input.registrationDate)) {
    errors.push("registrationDate must be YYYY-MM-DD when present");
  }
  const periodStepMonths = input.periodStepMonths ?? 1;
  if (!Number.isInteger(periodStepMonths) || periodStepMonths <= 0) {
    errors.push("periodStepMonths must be a positive integer when present");
  }
  if (typeof input.resultAccountNo !== "string" || input.resultAccountNo.trim().length === 0) {
    errors.push("resultAccountNo is required");
  }
  if (errors.length > 0) return { ok: false, appliedRules: [ACCRUAL_RULE_ID], errors };

  const balanceAccountNo = input.balanceAccountNo ?? DEFAULT_BALANCE_ACCOUNT[input.accrualType];
  const settlementAccountNo = input.settlementAccountNo ?? DEFAULT_SETTLEMENT_ACCOUNT;
  const resultAccountNo = input.resultAccountNo.trim();

  const balanceAccount = loadAccount(db, balanceAccountNo);
  const resultAccount = loadAccount(db, resultAccountNo);
  const settlementAccount = loadAccount(db, settlementAccountNo);
  if (!balanceAccount || !balanceAccount.active) {
    errors.push(`balanceAccountNo ${balanceAccountNo} does not exist or is inactive`);
  }
  if (!resultAccount || !resultAccount.active) {
    errors.push(`resultAccountNo ${resultAccountNo} does not exist or is inactive`);
  }
  if (!settlementAccount || !settlementAccount.active) {
    errors.push(`settlementAccountNo ${settlementAccountNo} does not exist or is inactive`);
  }
  if (errors.length > 0) return { ok: false, appliedRules: [ACCRUAL_RULE_ID], errors };

  // The result account must match the accrual type: expense for a prepaid or
  // accrued expense, income for deferred revenue. A mismatch would book the
  // amount on the wrong side of the income statement.
  if (
    (input.accrualType === "prepaid_expense" || input.accrualType === "accrued_expense") &&
    resultAccount!.type !== "expense"
  ) {
    errors.push(`resultAccountNo ${resultAccountNo} must be an expense account for accrualType ${input.accrualType}`);
  }
  if (input.accrualType === "deferred_revenue" && resultAccount!.type !== "income") {
    errors.push(`resultAccountNo ${resultAccountNo} must be an income account for accrualType deferred_revenue`);
  }
  if (errors.length > 0) return { ok: false, appliedRules: [ACCRUAL_RULE_ID], errors };

  if (input.documentId !== undefined) {
    if (!Number.isInteger(input.documentId) || input.documentId <= 0) {
      return { ok: false, appliedRules: [ACCRUAL_RULE_ID], errors: ["documentId must be a positive integer when present"] };
    }
    const doc = db.query("SELECT id FROM documents WHERE id = ?").get(input.documentId) as { id: number } | null;
    if (!doc) {
      return { ok: false, appliedRules: [ACCRUAL_RULE_ID], errors: [`document ${input.documentId} does not exist`] };
    }
  }

  const totalAmount = roundDkk(input.totalAmount);
  const description = input.description.trim();

  // Compute the schedule up front so a bad split is rejected before any write.
  const schedule = computeAccrualSchedule({
    totalAmount,
    recognitionPeriods: input.recognitionPeriods,
    firstRecognitionDate: input.firstRecognitionDate,
    periodStepMonths,
  });

  try {
    const result = db.transaction(() => {
      // Insert the header first (registration_journal_entry_id NULL), so the
      // journal entry can reference it conceptually and the row exists for the
      // single permitted update once the entry is posted.
      const header = db.query(
        `INSERT INTO accruals (
           accrual_type, description, total_amount, recognition_periods,
           balance_account_no, result_account_no, first_recognition_date,
           period_step_months, document_id, note
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING id`,
      ).get(
        input.accrualType,
        description,
        totalAmount,
        input.recognitionPeriods,
        balanceAccountNo,
        resultAccountNo,
        input.firstRecognitionDate,
        periodStepMonths,
        input.documentId ?? null,
        input.note?.trim() || null,
      ) as { id: number };

      const journal = postJournalEntry(db, {
        transactionDate: registrationDate,
        text: `Registrér periodeafgrænsningspost (${input.accrualType}): ${description}`,
        documentId: input.documentId,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines: registrationLines(
          input.accrualType,
          totalAmount,
          balanceAccountNo,
          resultAccountNo,
          settlementAccountNo,
          description,
        ),
      });
      if (!journal.ok) {
        throw new Error(JSON.stringify({ appliedRules: journal.appliedRules, errors: journal.errors }));
      }

      // The one permitted accruals-row mutation: link the registration entry.
      db.query("UPDATE accruals SET registration_journal_entry_id = ? WHERE id = ?")
        .run(journal.entryId!, header.id);

      insertAuditLog(db, {
        eventType: "accrual_register",
        entityType: "accrual",
        entityId: header.id,
        message:
          `Registered ${input.accrualType} accrual "${description}" ` +
          `(${totalAmount} over ${input.recognitionPeriods} periods)`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      return {
        ...journal,
        accrualId: header.id,
        accrualType: input.accrualType,
        totalPeriods: schedule.length,
        periodAmount: schedule[0]?.amount,
        appliedRules: [...new Set([ACCRUAL_RULE_ID, ...(journal.appliedRules ?? [])])],
      };
    })();
    return result;
  } catch (error) {
    const parsed = parseTransactionError(error);
    return {
      ok: false,
      appliedRules: [...new Set([ACCRUAL_RULE_ID, ...((parsed?.appliedRules as string[] | undefined) ?? [])])],
      errors: (parsed?.errors as string[] | undefined) ?? [String(error)],
    };
  }
}

export type RecognizeAccrualPeriodInput = {
  accrualId: number;
  periodIndex: number;
  /** Posting date for this recognition entry (default: schedule date). */
  transactionDate?: string;
  /** Settlement account, only used by accrued_expense recognition. */
  settlementAccountNo?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type RecognizeAccrualPeriodResult = JournalPostResult & {
  accrualId?: number;
  periodIndex?: number;
  periodAmount?: number;
  recognizedPeriods?: number;
  totalPeriods?: number;
  fullyRecognized?: boolean;
};

type AccrualHeaderRow = {
  id: number;
  accrual_type: AccrualType;
  description: string;
  total_amount: number;
  recognition_periods: number;
  balance_account_no: string;
  result_account_no: string;
  first_recognition_date: string;
  period_step_months: number;
  document_id: number | null;
};

function loadAccrual(db: Database, accrualId: number): AccrualHeaderRow | null {
  return db.query(
    `SELECT id, accrual_type, description, total_amount, recognition_periods,
            balance_account_no, result_account_no, first_recognition_date,
            period_step_months, document_id
     FROM accruals WHERE id = ?`,
  ).get(accrualId) as AccrualHeaderRow | null;
}

/**
 * Recognise one period of an accrual's schedule: post the balanced journal
 * entry that moves that period's slice between the balance-sheet parking
 * account and the income-statement account.
 *
 * Re-posting an already-recognised period is blocked deterministically by the
 * `UNIQUE(accrual_id, period_index)` constraint and an explicit pre-check.
 */
export function recognizeAccrualPeriod(
  db: Database,
  input: RecognizeAccrualPeriodInput,
): RecognizeAccrualPeriodResult {
  const errors: string[] = [];
  if (!Number.isInteger(input.accrualId) || input.accrualId <= 0) {
    errors.push("accrualId must be a positive integer");
  }
  if (!Number.isInteger(input.periodIndex) || input.periodIndex <= 0) {
    errors.push("periodIndex must be a positive integer");
  }
  if (input.transactionDate !== undefined && !looksLikeIsoDate(input.transactionDate)) {
    errors.push("transactionDate must be YYYY-MM-DD when present");
  }
  if (errors.length > 0) return { ok: false, appliedRules: [ACCRUAL_RULE_ID], errors };

  const accrual = loadAccrual(db, input.accrualId);
  if (!accrual) {
    return { ok: false, appliedRules: [ACCRUAL_RULE_ID], errors: [`accrual ${input.accrualId} does not exist`] };
  }

  const schedule = computeAccrualSchedule({
    totalAmount: Number(accrual.total_amount),
    recognitionPeriods: accrual.recognition_periods,
    firstRecognitionDate: accrual.first_recognition_date,
    periodStepMonths: accrual.period_step_months,
  });
  if (input.periodIndex > schedule.length) {
    return {
      ok: false,
      appliedRules: [ACCRUAL_RULE_ID],
      errors: [`periodIndex ${input.periodIndex} is outside the accrual schedule (1..${schedule.length})`],
    };
  }

  const existing = db.query(
    "SELECT id FROM accrual_schedule_postings WHERE accrual_id = ? AND period_index = ? LIMIT 1",
  ).get(input.accrualId, input.periodIndex) as { id: number } | null;
  if (existing) {
    return {
      ok: false,
      appliedRules: [ACCRUAL_RULE_ID],
      errors: [`accrual ${input.accrualId} period ${input.periodIndex} is already recognized`],
    };
  }

  const period = schedule[input.periodIndex - 1]!;
  const transactionDate = input.transactionDate ?? period.recognitionDate;
  const settlementAccountNo = input.settlementAccountNo ?? DEFAULT_SETTLEMENT_ACCOUNT;

  // accrued_expense recognition touches the settlement account — validate it.
  if (accrual.accrual_type === "accrued_expense") {
    const settlementAccount = loadAccount(db, settlementAccountNo);
    if (!settlementAccount || !settlementAccount.active) {
      return {
        ok: false,
        appliedRules: [ACCRUAL_RULE_ID],
        errors: [`settlementAccountNo ${settlementAccountNo} does not exist or is inactive`],
      };
    }
  }

  try {
    const result = db.transaction(() => {
      const journal = postJournalEntry(db, {
        transactionDate,
        text:
          `Periodisering (${accrual.accrual_type}) ${input.periodIndex}/${schedule.length}: ` +
          accrual.description,
        documentId: accrual.document_id ?? undefined,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines: recognitionLines(
          accrual.accrual_type,
          period.amount,
          accrual.balance_account_no,
          accrual.result_account_no,
          settlementAccountNo,
          accrual.description,
          input.periodIndex,
          schedule.length,
        ),
      });
      if (!journal.ok) {
        throw new Error(JSON.stringify({ appliedRules: journal.appliedRules, errors: journal.errors }));
      }

      const posting = db.query(
        `INSERT INTO accrual_schedule_postings (accrual_id, period_index, recognition_date, amount, journal_entry_id)
         VALUES (?, ?, ?, ?, ?)
         RETURNING id`,
      ).get(input.accrualId, input.periodIndex, transactionDate, period.amount, journal.entryId!) as { id: number };

      const recognizedPeriods = db.query(
        "SELECT COUNT(*) AS n FROM accrual_schedule_postings WHERE accrual_id = ?",
      ).get(input.accrualId) as { n: number };

      insertAuditLog(db, {
        eventType: "accrual_recognize",
        entityType: "accrual_schedule_posting",
        entityId: posting.id,
        message:
          `Recognized accrual ${input.accrualId} period ${input.periodIndex}/${schedule.length} ` +
          `(${period.amount})`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      return {
        ...journal,
        accrualId: input.accrualId,
        periodIndex: input.periodIndex,
        periodAmount: period.amount,
        recognizedPeriods: Number(recognizedPeriods.n),
        totalPeriods: schedule.length,
        fullyRecognized: Number(recognizedPeriods.n) === schedule.length,
        appliedRules: [...new Set([ACCRUAL_RULE_ID, ...(journal.appliedRules ?? [])])],
      };
    })();
    return result;
  } catch (error) {
    const parsed = parseTransactionError(error);
    return {
      ok: false,
      appliedRules: [...new Set([ACCRUAL_RULE_ID, ...((parsed?.appliedRules as string[] | undefined) ?? [])])],
      errors: (parsed?.errors as string[] | undefined) ?? [String(error)],
    };
  }
}

export type AccrualRegisterRow = {
  accrualId: number;
  accrualType: AccrualType;
  description: string;
  totalAmount: number;
  recognitionPeriods: number;
  recognizedPeriods: number;
  recognizedAmount: number;
  remainingAmount: number;
  fullyRecognized: boolean;
  balanceAccountNo: string;
  resultAccountNo: string;
  firstRecognitionDate: string;
  periodStepMonths: number;
};

export type AccrualRegisterReport = {
  ok: boolean;
  accruals: AccrualRegisterRow[];
  totals: { totalAmount: number; recognizedAmount: number; remainingAmount: number };
  errors: string[];
};

/**
 * Accrual register report: every registered periodeafgrænsningspost with its
 * recognised periods, recognised amount and remaining balance-sheet exposure,
 * plus portfolio totals. Read-only.
 */
export function buildAccrualRegisterReport(db: Database): AccrualRegisterReport {
  const rows = db.query(
    `SELECT a.id, a.accrual_type, a.description, a.total_amount, a.recognition_periods,
            a.balance_account_no, a.result_account_no, a.first_recognition_date, a.period_step_months,
            COUNT(p.id) AS recognized_periods,
            COALESCE(SUM(p.amount), 0) AS recognized_amount
     FROM accruals a
     LEFT JOIN accrual_schedule_postings p ON p.accrual_id = a.id
     GROUP BY a.id
     ORDER BY a.id ASC`,
  ).all() as Array<{
    id: number;
    accrual_type: AccrualType;
    description: string;
    total_amount: number;
    recognition_periods: number;
    balance_account_no: string;
    result_account_no: string;
    first_recognition_date: string;
    period_step_months: number;
    recognized_periods: number;
    recognized_amount: number;
  }>;

  let totalOre = 0n;
  let recognizedOre = 0n;
  const accruals: AccrualRegisterRow[] = rows.map((row) => {
    const totalAmount = roundDkk(Number(row.total_amount));
    const recognizedAmount = roundDkk(Number(row.recognized_amount));
    const remainingAmount = fromOre(toOre(totalAmount) - toOre(recognizedAmount));
    totalOre += toOre(totalAmount);
    recognizedOre += toOre(recognizedAmount);
    return {
      accrualId: row.id,
      accrualType: row.accrual_type,
      description: row.description,
      totalAmount,
      recognitionPeriods: row.recognition_periods,
      recognizedPeriods: Number(row.recognized_periods),
      recognizedAmount,
      remainingAmount,
      fullyRecognized: Number(row.recognized_periods) === row.recognition_periods,
      balanceAccountNo: row.balance_account_no,
      resultAccountNo: row.result_account_no,
      firstRecognitionDate: row.first_recognition_date,
      periodStepMonths: row.period_step_months,
    };
  });

  return {
    ok: true,
    accruals,
    totals: {
      totalAmount: fromOre(totalOre),
      recognizedAmount: fromOre(recognizedOre),
      remainingAmount: fromOre(totalOre - recognizedOre),
    },
    errors: [],
  };
}

/**
 * One recognition period of a registered accrual that is *due* — its scheduled
 * recognition date has arrived (or passed) relative to an as-of date — and has
 * NOT yet been posted via {@link recognizeAccrualPeriod}.
 */
export type DueAccrualRecognitionPeriod = {
  accrualId: number;
  accrualType: AccrualType;
  description: string;
  periodIndex: number;
  totalPeriods: number;
  recognitionDate: string;
  amount: number;
  /** Days from `recognitionDate` to the as-of date; 0 when due today. */
  overdueDays: number;
};

export type DueAccrualRecognitionResult = {
  ok: boolean;
  asOfDate: string;
  periods: DueAccrualRecognitionPeriod[];
  /** Sum of `periods[].amount` — the income-statement effect still un-posted. */
  totalDueAmount: number;
  errors: string[];
};

/**
 * Lists the accrual recognition periods that are *due or overdue* as of a date
 * and have not yet been posted to the ledger.
 *
 * A recognition period is due when its deterministic schedule date (see
 * {@link computeAccrualSchedule}) is on or before `asOfDate`. A period that
 * already has an `accrual_schedule_postings` row is recognised and excluded.
 *
 * This is a READ-ONLY surface — it never posts. It deliberately does NOT
 * recognise the period itself: choosing the posting date is the owner's
 * decision, exactly as the agent loop never auto-posts a fixed-asset purchase.
 * Callers (dashboard, agent loop, exception queue) use it to *surface* the
 * pending work.
 */
export function listDueAccrualRecognitionPeriods(
  db: Database,
  asOfDate: string,
): DueAccrualRecognitionResult {
  if (!looksLikeIsoDate(asOfDate)) {
    return { ok: false, asOfDate, periods: [], totalDueAmount: 0, errors: ["asOfDate must be YYYY-MM-DD"] };
  }

  const headers = db.query(
    `SELECT id, accrual_type, description, total_amount, recognition_periods,
            first_recognition_date, period_step_months
     FROM accruals ORDER BY id ASC`,
  ).all() as Array<{
    id: number;
    accrual_type: AccrualType;
    description: string;
    total_amount: number;
    recognition_periods: number;
    first_recognition_date: string;
    period_step_months: number;
  }>;

  const periods: DueAccrualRecognitionPeriod[] = [];
  for (const header of headers) {
    const postedIndexes = new Set(
      (
        db.query(
          "SELECT period_index FROM accrual_schedule_postings WHERE accrual_id = ?",
        ).all(header.id) as Array<{ period_index: number }>
      ).map((r) => r.period_index),
    );
    const schedule = computeAccrualSchedule({
      totalAmount: Number(header.total_amount),
      recognitionPeriods: header.recognition_periods,
      firstRecognitionDate: header.first_recognition_date,
      periodStepMonths: header.period_step_months,
    });
    for (const period of schedule) {
      if (postedIndexes.has(period.periodIndex)) continue;
      if (period.recognitionDate > asOfDate) continue;
      periods.push({
        accrualId: header.id,
        accrualType: header.accrual_type,
        description: header.description,
        periodIndex: period.periodIndex,
        totalPeriods: schedule.length,
        recognitionDate: period.recognitionDate,
        amount: period.amount,
        overdueDays: Math.max(0, diffDays(period.recognitionDate, asOfDate)),
      });
    }
  }

  // Stable order: oldest recognition date first, then accrual id / period.
  periods.sort((a, b) => {
    if (a.recognitionDate !== b.recognitionDate) {
      return a.recognitionDate < b.recognitionDate ? -1 : 1;
    }
    if (a.accrualId !== b.accrualId) return a.accrualId - b.accrualId;
    return a.periodIndex - b.periodIndex;
  });

  return {
    ok: true,
    asOfDate,
    periods,
    totalDueAmount: sumDkk(periods.map((p) => p.amount)),
    errors: [],
  };
}

/**
 * Unpacks the JSON error thrown to abort a posting transaction so the original
 * `appliedRules`/`errors` from the failed `postJournalEntry` can be surfaced.
 */
function parseTransactionError(error: unknown): { appliedRules?: unknown; errors?: unknown } | null {
  if (typeof error === "object" && error && "message" in error) {
    try {
      return JSON.parse(String((error as { message: unknown }).message));
    } catch {
      return null;
    }
  }
  return null;
}
