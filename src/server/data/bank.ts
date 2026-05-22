// Bank-balance read helpers for the cockpit backend (#320).
//
// Two figures the cockpit needs across several views: the booked ledger
// balance of the cash/bank accounts, and the actual statement balance from the
// most recent imported `bank_transactions` row. Split out of `server/data.ts`
// by #320; behaviour is unchanged — `server/data.ts` re-exports nothing from
// here directly, but the portfolio and statement modules consume it.

import type { Database } from "bun:sqlite";
import { listBankAccounts } from "../../core/bank";
import { roundKroner } from "./shared";

/**
 * Booked balance of the bank / cash asset accounts at `asOfDate`, kroner.
 *
 * Bank accounts are identified by the `bank_accounts.ledger_account_no` link
 * when any bank account is registered; otherwise it falls back to every
 * `asset`-type account whose name reads as a bank or cash account. This keeps
 * the figure independent of any one chart's account numbering.
 */
export function bankBalanceAsOf(db: Database, asOfDate: string): number {
  const linked = listBankAccounts(db)
    .accounts.map((a) => a.ledgerAccountNo)
    .filter((no): no is string => typeof no === "string" && no.length > 0);

  let accountNos: string[];
  if (linked.length > 0) {
    accountNos = [...new Set(linked)];
  } else {
    accountNos = (
      db
        .query(
          `SELECT account_no FROM accounts
            WHERE type = 'asset'
              AND (lower(name) LIKE '%bank%' OR lower(name) LIKE '%kasse%'
                   OR lower(name) LIKE '%giro%')`,
        )
        .all() as Array<{ account_no: string }>
    ).map((r) => r.account_no);
  }
  if (accountNos.length === 0) return 0;

  const placeholders = accountNos.map(() => "?").join(", ");
  const row = db
    .query(
      `SELECT COALESCE(SUM(jl.debit_amount - jl.credit_amount), 0) AS bal
         FROM journal_entries je
         JOIN journal_lines jl ON jl.journal_entry_id = je.id
         JOIN accounts a       ON a.id = jl.account_id
        WHERE je.status = 'posted'
          AND je.transaction_date <= ?
          AND a.account_no IN (${placeholders})`,
    )
    .get(asOfDate, ...accountNos) as { bal: number };
  return roundKroner(row.bal);
}

/**
 * Actual bank balance from the imported statement, kroner — the `balance_after`
 * of the most recent `bank_transactions` row (latest `transaction_date`, then
 * latest id) per bank account, summed across accounts.
 *
 * This is what the owner's bank app shows; it can differ from the booked
 * ledger balance when transactions are imported but not yet reconciled. Rows
 * with no `balance_after` (a generic CSV import that omitted the running
 * balance) are skipped. Returns `null` when no statement balance is known.
 */
export function actualBankBalanceAsOf(
  db: Database,
  asOfDate: string,
): number | null {
  // The latest dated row per bank account (id breaks same-date ties). Rows
  // imported before #187 have a null bank_account_id — group them together as
  // one logical account so a single statement still surfaces.
  const rows = db
    .query(
      `SELECT bt.balance_after AS balanceAfter
         FROM bank_transactions bt
         JOIN (
           SELECT bank_account_id AS acc,
                  MAX(transaction_date) AS maxDate
             FROM bank_transactions
            WHERE transaction_date <= ?
              AND balance_after IS NOT NULL
            GROUP BY bank_account_id
         ) latest
           ON (bt.bank_account_id IS latest.acc
               OR (bt.bank_account_id IS NULL AND latest.acc IS NULL))
          AND bt.transaction_date = latest.maxDate
        WHERE bt.transaction_date <= ?
          AND bt.balance_after IS NOT NULL
          AND bt.id = (
            SELECT MAX(b2.id) FROM bank_transactions b2
             WHERE (b2.bank_account_id IS bt.bank_account_id)
               AND b2.transaction_date = bt.transaction_date
               AND b2.balance_after IS NOT NULL
               AND b2.transaction_date <= ?
          )`,
    )
    .all(asOfDate, asOfDate, asOfDate) as Array<{ balanceAfter: number }>;
  if (rows.length === 0) return null;
  const total = rows.reduce((sum, r) => sum + Number(r.balanceAfter ?? 0), 0);
  return roundKroner(total);
}
