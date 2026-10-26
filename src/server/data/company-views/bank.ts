import { listBankAccounts } from "../../../core/bank";
import {
  resolveStatementContext,
  roundKroner,
  statementCompanyBlock,
} from "../shared";
import {
  bankBalanceAsOf,
  resolveActualBankBalanceAsOf,
} from "../bank";

// --------------------------------------------------------------------------
// Per-company bank transactions (Bank, year-aware) — cockpit-redesign it. 3
// --------------------------------------------------------------------------

export type BankTransactionRow = {
  id: number;
  date: string;
  text: string;
  amount: number;
  /** Running balance from the import, kroner; null when the export omits it. */
  runningBalance: number | null;
  /** "matched" when a posted journal entry references this row, else "unmatched". */
  reconciliationStatus: "matched" | "unmatched";
  /** The matched journal entry's number, when reconciled. */
  journalEntryNo: string | null;
  /** Exact reviewed bank-row party decision; never inferred from text. */
  partyId: string | null;
};

export type CompanyBank = ReturnType<typeof buildCompanyBank>;

/**
 * Bank — the imported `bank_transactions` rows for the selected calendar
 * fiscal year, each with its reconciliation status (matched vs unmatched to a
 * posted journal entry), plus the registered bank account and its booked
 * ledger balance at the year end. Money is kroner.
 */
export function buildCompanyBank(
  workspaceRoot: string,
  slug: string,
  year: number | null,
) {
  const ctx = resolveStatementContext(workspaceRoot, slug, year);
  try {
    const companyBlock = statementCompanyBlock(ctx.company);
    const accounts = listBankAccounts(ctx.db).accounts.map((a) => ({
      id: a.id,
      name: a.name,
      bankName: a.bankName,
      accountNo: a.accountNo,
      ledgerAccountNo: a.ledgerAccountNo,
    }));
    // Archived years (#197) keep their LEDGER in the read-only archive — but a
    // bank statement is live, append-only data that legitimately spans both
    // archived and live years (a Dinero migration archives 2023-2025 while the
    // owner's bank CSV covers 2024-2026). The imported `bank_transactions` rows
    // are therefore shown for every selected year; only the `archived` flag
    // tells the cockpit to present them without the live-ledger reconciliation
    // and booked-balance comparison it cannot meaningfully do for those years.
    const yearNum = parseInt(ctx.selectedLabel, 10);
    const yearStart = `${yearNum}-01-01`;
    const yearEnd = `${yearNum}-12-31`;

    // Bank rows for the year, oldest-first so the running balance reads
    // naturally down the table. The LEFT JOIN to a posted journal entry on
    // `source_bank_transaction_id` is the reconciliation status — the same
    // join `core/reconciliation.listBankTransactions` uses.
    const rows = ctx.db
      .query(
        `SELECT bt.id            AS id,
                bt.transaction_date AS date,
                bt.text          AS text,
                bt.amount        AS amount,
                bt.balance_after AS runningBalance,
                br.journal_entry_no AS journalEntryNo,
                party_decision.party_id AS partyId
           FROM bank_transactions bt
           LEFT JOIN bank_journal_reconciliations br ON br.bank_transaction_id = bt.id
           LEFT JOIN current_party_coverage_bank_resolution_events party_decision ON party_decision.bank_transaction_id = bt.id
          WHERE bt.transaction_date >= ? AND bt.transaction_date <= ?
          ORDER BY bt.transaction_date ASC,
                   CASE WHEN bt.statement_order = 'descending' THEN -bt.statement_row_index ELSE bt.statement_row_index END ASC,
                   bt.id ASC`,
      )
      .all(yearStart, yearEnd) as Array<{
      id: number;
      date: string;
      text: string;
      amount: number;
      runningBalance: number | null;
      journalEntryNo: string | null;
      partyId: string | null;
    }>;
    const transactions: BankTransactionRow[] = rows.map((r) => ({
      id: r.id,
      date: r.date,
      text: r.text,
      amount: roundKroner(r.amount),
      runningBalance:
        r.runningBalance === null || r.runningBalance === undefined
          ? null
          : roundKroner(r.runningBalance),
      reconciliationStatus: r.journalEntryNo ? "matched" : "unmatched",
      journalEntryNo: r.journalEntryNo,
      partyId: r.partyId,
    }));
    const matchedCount = transactions.filter(
      (t) => t.reconciliationStatus === "matched",
    ).length;

    // The booked ledger balance vs the actual statement balance (the latest
    // imported `balance_after`). Their gap is the headline of a bank page —
    // money the owner has on paper but not in the account, or vice versa.
    const bookedBalance = bankBalanceAsOf(ctx.db, yearEnd);
    const statementBalance = resolveActualBankBalanceAsOf(ctx.db, yearEnd);
    const actualBalance = statementBalance.balance;
    const difference =
      actualBalance === null ? null : roundKroner(bookedBalance - actualBalance);

    // #305: `actualBalance === null` has two very different causes — no
    // statement imported at all, or a statement WAS imported but its CSV had
    // no balance column. The cockpit must not say "intet kontoudtog
    // importeret" for the second case: an owner who just imported a CSV would
    // think the import silently failed. `bankStatementStatus` distinguishes
    // them so the UI can say "banksaldo ukendt — kontoudtoget havde ingen
    // saldo-kolonne" instead. EJER-12: the shared `bankStatementStatusAsOf`
    // helper is used so the portfolio card and dashboard tell the same story.
    const bankStatementStatus = statementBalance.status;

    return {
      slug: ctx.entry.slug,
      selectedYear: ctx.selectedLabel,
      archived: ctx.isArchivedOnly,
      company: companyBlock,
      fiscalYears: ctx.years,
      periodStart: yearStart,
      periodEnd: yearEnd,
      accounts,
      bookedBalance,
      actualBalance,
      difference,
      bankStatementStatus,
      actualBalanceProvenance: statementBalance.provenance,
      bankStatementDiagnostics: statementBalance.diagnostics,
      transactions,
      matchedCount,
      unmatchedCount: transactions.length - matchedCount,
    };
  } finally {
    ctx.db.close();
  }
}
