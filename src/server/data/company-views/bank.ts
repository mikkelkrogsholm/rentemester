import { listBankAccounts } from "../../../core/bank";
import {
  resolveStatementContext,
  roundKroner,
  statementCompanyBlock,
} from "../shared";
import { bankBalanceAsOf, actualBankBalanceAsOf } from "../bank";

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
                je.entry_no      AS journalEntryNo
           FROM bank_transactions bt
           LEFT JOIN journal_entries je
             ON je.source_bank_transaction_id = bt.id
            AND je.status = 'posted'
          WHERE bt.transaction_date >= ? AND bt.transaction_date <= ?
          ORDER BY bt.transaction_date ASC, bt.id ASC`,
      )
      .all(yearStart, yearEnd) as Array<{
      id: number;
      date: string;
      text: string;
      amount: number;
      runningBalance: number | null;
      journalEntryNo: string | null;
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
    }));
    const matchedCount = transactions.filter(
      (t) => t.reconciliationStatus === "matched",
    ).length;

    // The booked ledger balance vs the actual statement balance (the latest
    // imported `balance_after`). Their gap is the headline of a bank page —
    // money the owner has on paper but not in the account, or vice versa.
    const bookedBalance = bankBalanceAsOf(ctx.db, yearEnd);
    const actualBalance = actualBankBalanceAsOf(ctx.db, yearEnd);
    const difference =
      actualBalance === null ? null : roundKroner(bookedBalance - actualBalance);

    // #305: `actualBalance === null` has two very different causes — no
    // statement imported at all, or a statement WAS imported but its CSV had
    // no balance column. The cockpit must not say "intet kontoudtog
    // importeret" for the second case: an owner who just imported a CSV would
    // think the import silently failed. `bankStatementStatus` distinguishes
    // them so the UI can say "banksaldo ukendt — kontoudtoget havde ingen
    // saldo-kolonne" instead.
    const hasAnyTransactions =
      transactions.length > 0 ||
      (ctx.db
        .query("SELECT 1 FROM bank_transactions LIMIT 1")
        .get() as unknown) !== null;
    const bankStatementStatus: "known" | "no-balance-column" | "none" =
      actualBalance !== null
        ? "known"
        : hasAnyTransactions
          ? "no-balance-column"
          : "none";

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
      transactions,
      matchedCount,
      unmatchedCount: transactions.length - matchedCount,
    };
  } finally {
    ctx.db.close();
  }
}
