import type { Database } from "bun:sqlite";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { roundDkk } from "./money";

export type ReconciliationStatus = "all" | "matched" | "unmatched" | "ambiguous";

export type BankReconciliationFilters = {
  status?: ReconciliationStatus;
  textMatch?: string;
  amount?: number;
  // ===== BANK CLUSTER (#187) =====
  /** Restrict to a single bank account (numeric id). */
  bankAccountId?: number;
  // ===== END BANK CLUSTER (#187) =====
};

export type BankTransactionListResult = {
  ok: boolean;
  count: number;
  rows: Array<{
    id: number;
    transactionDate: string;
    bookingDate: string | null;
    text: string;
    amount: number;
    currency: string;
    reference: string | null;
    importBatchId: string | null;
    bankAccountId: number | null;
    ledgerStatus: string;
    reconciliationStatus: Exclude<ReconciliationStatus, "all">;
    journalEntryId: number | null;
    journalEntryNo: string | null;
  }>;
  errors: string[];
};

export type BankReconciliationReport = {
  ok: boolean;
  appliedRules: string[];
  periodStart: string;
  periodEnd: string;
  matchedCount: number;
  unmatchedCount: number;
  ambiguousCount: number;
  matchedAmountTotal: number;
  unmatchedAmountTotal: number;
  ambiguousAmountTotal: number;
  matched: Array<{
    bankTransactionId: number;
    transactionDate: string;
    text: string;
    amount: number;
    journalEntryId: number;
    journalEntryNo: string;
  }>;
  unmatched: Array<{
    bankTransactionId: number;
    transactionDate: string;
    text: string;
    amount: number;
    importBatchId: string | null;
  }>;
  // A journal linked to more than one bank row reconciles the ledger movement
  // twice; such rows are evidence of a broken state, never a green match.
  ambiguous: Array<{
    bankTransactionId: number;
    transactionDate: string;
    text: string;
    amount: number;
    journalEntryId: number;
    journalEntryNo: string;
  }>;
  errors: string[];
};

const RULE_ID = "DK-BOOKKEEPING-RECONCILIATION-001";



function normalizedStatus(status?: string): ReconciliationStatus {
  return status === "matched" || status === "unmatched" || status === "ambiguous" || status === "all" ? status : "all";
}

function validateStatus(status?: string) {
  if (!status || status === "all" || status === "matched" || status === "unmatched" || status === "ambiguous") return null;
  return "status must be one of all, matched, unmatched, ambiguous when present";
}

function normalizeTextMatch(textMatch?: string) {
  const value = textMatch?.trim().toLowerCase();
  return value ? value : null;
}

function matchesFilters(row: {
  amount: number;
  text: string;
  reconciliation_status: "matched" | "unmatched" | "ambiguous";
}, filters: BankReconciliationFilters = {}) {
  const status = normalizedStatus(filters.status);
  const textMatch = normalizeTextMatch(filters.textMatch);
  if (status !== "all" && row.reconciliation_status !== status) return false;
  if (typeof filters.amount === "number" && Number.isFinite(filters.amount) && roundDkk(row.amount) !== roundDkk(filters.amount)) return false;
  if (textMatch && !row.text.toLowerCase().includes(textMatch)) return false;
  return true;
}

export function listBankTransactions(db: Database, filters: BankReconciliationFilters & { from?: string; to?: string } = {}): BankTransactionListResult {
  const errors: string[] = [];
  if (filters.from && !looksLikeIsoDate(filters.from)) errors.push("from must be YYYY-MM-DD when present");
  if (filters.to && !looksLikeIsoDate(filters.to)) errors.push("to must be YYYY-MM-DD when present");
  const statusError = validateStatus(filters.status);
  if (statusError) errors.push(statusError);
  if (errors.length > 0) return { ok: false, count: 0, rows: [], errors };

  const bankAccountId = filters.bankAccountId ?? null;
  const rows = db.query(
    `SELECT bt.id, bt.transaction_date, bt.booking_date, bt.text, bt.amount, bt.currency, bt.reference, bt.import_batch_id, bt.status, bt.bank_account_id,
            br.journal_entry_id, br.journal_entry_no AS entry_no,
            CASE
              WHEN br.journal_entry_id IS NULL THEN 'unmatched'
              WHEN EXISTS(SELECT 1 FROM bank_journal_reconciliations other
                           WHERE other.journal_entry_id = br.journal_entry_id
                             AND other.bank_transaction_id <> bt.id) THEN 'ambiguous'
              ELSE 'matched'
            END as reconciliation_status
     FROM bank_transactions bt
     LEFT JOIN bank_journal_reconciliations br ON br.bank_transaction_id = bt.id
     WHERE (? IS NULL OR bt.transaction_date >= ?)
       AND (? IS NULL OR bt.transaction_date <= ?)
       AND (? IS NULL OR bt.bank_account_id = ?)
     ORDER BY bt.transaction_date DESC, bt.id DESC`
  ).all(
    filters.from ?? null, filters.from ?? null,
    filters.to ?? null, filters.to ?? null,
    bankAccountId, bankAccountId,
  ) as Array<{
    id: number;
    transaction_date: string;
    booking_date: string | null;
    text: string;
    amount: number;
    currency: string;
    reference: string | null;
    import_batch_id: string | null;
    status: string;
    bank_account_id: number | null;
    journal_entry_id: number | null;
    entry_no: string | null;
    reconciliation_status: "matched" | "unmatched" | "ambiguous";
  }>;

  const filtered = rows.filter((row) => matchesFilters(row, filters)).map((row) => ({
    id: row.id,
    transactionDate: row.transaction_date,
    bookingDate: row.booking_date,
    text: row.text,
    amount: roundDkk(Number(row.amount)),
    currency: row.currency,
    reference: row.reference,
    importBatchId: row.import_batch_id,
    bankAccountId: row.bank_account_id,
    ledgerStatus: row.status,
    reconciliationStatus: row.reconciliation_status,
    journalEntryId: row.journal_entry_id,
    journalEntryNo: row.entry_no,
  }));

  return { ok: true, count: filtered.length, rows: filtered, errors: [] };
}

export function buildBankReconciliationReport(db: Database, periodStart: string, periodEnd: string, filters: BankReconciliationFilters = {}): BankReconciliationReport {
  const errors: string[] = [];
  if (!looksLikeIsoDate(periodStart)) errors.push("periodStart must be YYYY-MM-DD");
  if (!looksLikeIsoDate(periodEnd)) errors.push("periodEnd must be YYYY-MM-DD");
  const statusError = validateStatus(filters.status);
  if (statusError) errors.push(statusError);
  if (errors.length === 0 && periodStart > periodEnd) errors.push("periodStart must be before or equal to periodEnd");
  if (errors.length > 0) {
    return {
      ok: false,
      appliedRules: [RULE_ID],
      periodStart,
      periodEnd,
      matchedCount: 0,
      unmatchedCount: 0,
      ambiguousCount: 0,
      matchedAmountTotal: 0,
      unmatchedAmountTotal: 0,
      ambiguousAmountTotal: 0,
      matched: [],
      unmatched: [],
      ambiguous: [],
      errors,
    };
  }

  const bankAccountId = filters.bankAccountId ?? null;
  const rows = db.query(
    `SELECT bt.id as bank_transaction_id, bt.transaction_date, bt.text, bt.amount, bt.import_batch_id,
            br.journal_entry_id, br.journal_entry_no AS entry_no,
            CASE
              WHEN br.journal_entry_id IS NULL THEN 'unmatched'
              WHEN EXISTS(SELECT 1 FROM bank_journal_reconciliations other
                           WHERE other.journal_entry_id = br.journal_entry_id
                             AND other.bank_transaction_id <> bt.id) THEN 'ambiguous'
              ELSE 'matched'
            END as reconciliation_status
     FROM bank_transactions bt
     LEFT JOIN bank_journal_reconciliations br ON br.bank_transaction_id = bt.id
     WHERE bt.transaction_date >= ? AND bt.transaction_date <= ?
       AND (? IS NULL OR bt.bank_account_id = ?)
     ORDER BY bt.transaction_date ASC, bt.id ASC`
  ).all(periodStart, periodEnd, bankAccountId, bankAccountId) as Array<{
    bank_transaction_id: number;
    transaction_date: string;
    text: string;
    amount: number;
    import_batch_id: string | null;
    journal_entry_id: number | null;
    entry_no: string | null;
    reconciliation_status: "matched" | "unmatched" | "ambiguous";
  }>;

  const matched: BankReconciliationReport["matched"] = [];
  const unmatched: BankReconciliationReport["unmatched"] = [];
  const ambiguous: BankReconciliationReport["ambiguous"] = [];
  let matchedAmountTotal = 0;
  let unmatchedAmountTotal = 0;
  let ambiguousAmountTotal = 0;

  for (const row of rows) {
    if (!matchesFilters({ amount: Number(row.amount), text: row.text, reconciliation_status: row.reconciliation_status }, filters)) continue;
    const amount = roundDkk(Number(row.amount));
    if (row.reconciliation_status === "ambiguous") {
      ambiguous.push({
        bankTransactionId: row.bank_transaction_id,
        transactionDate: row.transaction_date,
        text: row.text,
        amount,
        journalEntryId: row.journal_entry_id!,
        journalEntryNo: row.entry_no!,
      });
      ambiguousAmountTotal += amount;
    } else if (row.journal_entry_id) {
      matched.push({
        bankTransactionId: row.bank_transaction_id,
        transactionDate: row.transaction_date,
        text: row.text,
        amount,
        journalEntryId: row.journal_entry_id,
        journalEntryNo: row.entry_no!,
      });
      matchedAmountTotal += amount;
    } else {
      unmatched.push({
        bankTransactionId: row.bank_transaction_id,
        transactionDate: row.transaction_date,
        text: row.text,
        amount,
        importBatchId: row.import_batch_id,
      });
      unmatchedAmountTotal += amount;
    }
  }

  return {
    ok: true,
    appliedRules: [RULE_ID],
    periodStart,
    periodEnd,
    matchedCount: matched.length,
    unmatchedCount: unmatched.length,
    ambiguousCount: ambiguous.length,
    matchedAmountTotal: roundDkk(matchedAmountTotal),
    unmatchedAmountTotal: roundDkk(unmatchedAmountTotal),
    ambiguousAmountTotal: roundDkk(ambiguousAmountTotal),
    matched,
    unmatched,
    ambiguous,
    errors: [],
  };
}
