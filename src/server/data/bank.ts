// Bank-balance read helpers for the cockpit backend (#320).
//
// Two figures the cockpit needs across several views: the booked ledger
// balance of the cash/bank accounts, and the actual statement balance from the
// most recent imported `bank_transactions` row. Split out of `server/data.ts`
// by #320; behaviour is unchanged — `server/data.ts` re-exports nothing from
// here directly, but the portfolio and statement modules consume it.

import type { Database } from "bun:sqlite";
import { listBankAccounts } from "../../core/bank";
import { fromOre, toOre } from "../../core/money";
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

export type BankStatementStatus = "known" | "no-balance-column" | "none" | "ambiguous";

export type StatementBalanceResolution = {
  status: BankStatementStatus;
  balance: number | null;
  /** Immutable source rows that establish the returned closing balance. */
  provenance: Array<{ bankAccountId: number | null; transactionId: number; transactionDate: string; sourceOrder: "ascending" | "descending" | "single-row" }>;
  diagnostics: string[];
};

type StatementRow = {
  id: number;
  bankAccountId: number | null;
  transactionDate: string;
  amount: number;
  balanceAfter: number | null;
  statementRowIndex: number | null;
  statementOrder: "ascending" | "descending" | null;
  statementOrderProvenance: string | null;
};

/**
 * Resolves statement closing balances exclusively from persisted source order.
 * SQLite row ids and import order are never chronology. A same-date cluster
 * without one declared direction/provenance is intentionally ambiguous.
 */
export function resolveActualBankBalanceAsOf(db: Database, asOfDate: string): StatementBalanceResolution {
  const rows = db.query(
    `SELECT id, bank_account_id AS bankAccountId, transaction_date AS transactionDate,
            amount, balance_after AS balanceAfter, statement_row_index AS statementRowIndex,
            statement_order AS statementOrder, statement_order_provenance AS statementOrderProvenance
       FROM bank_transactions WHERE transaction_date <= ?
       ORDER BY bank_account_id, transaction_date, id`,
  ).all(asOfDate) as StatementRow[];
  if (rows.length === 0) return { status: "none", balance: null, provenance: [], diagnostics: [] };

  const accounts = new Map<string, StatementRow[]>();
  for (const row of rows) {
    const key = row.bankAccountId === null ? "legacy:null" : `account:${row.bankAccountId}`;
    accounts.set(key, [...(accounts.get(key) ?? []), row]);
  }
  const provenance: StatementBalanceResolution["provenance"] = [];
  const diagnostics: string[] = [];
  let totalOre = 0n;
  let knownAccounts = 0;
  let hadBalance = false;
  let accountsWithoutBalances = 0;

  for (const accountRows of accounts.values()) {
    const balanceRows = accountRows.filter((row) => row.balanceAfter !== null);
    if (balanceRows.length === 0) {
      accountsWithoutBalances += 1;
      diagnostics.push(`bank account ${accountRows[0]!.bankAccountId ?? "legacy"} has no balance_after`);
      continue;
    }
    hadBalance = true;
    // A partial running-balance stream cannot prove its movement check.
    if (balanceRows.length !== accountRows.length) {
      diagnostics.push(`bank account ${accountRows[0]!.bankAccountId ?? "legacy"} has transactions without balance_after`);
      continue;
    }
    const byDate = new Map<string, StatementRow[]>();
    for (const row of balanceRows) byDate.set(row.transactionDate, [...(byDate.get(row.transactionDate) ?? []), row]);
    let previousOre: bigint | null = null;
    let accountAmbiguous = false;
    let closing: StatementRow | null = null;
    for (const date of [...byDate.keys()].sort()) {
      const cluster = byDate.get(date)!;
      let ordered: StatementRow[];
      let sourceOrder: "ascending" | "descending" | "single-row";
      if (cluster.length === 1) {
        ordered = cluster;
        sourceOrder = "single-row";
      } else {
        const direction = cluster[0]!.statementOrder;
        const provenanceId = cluster[0]!.statementOrderProvenance;
        const valid = direction !== null && provenanceId !== null && cluster.every((row) =>
          row.statementOrder === direction && row.statementOrderProvenance === provenanceId && Number.isInteger(row.statementRowIndex),
        );
        const uniqueIndices = new Set(cluster.map((row) => row.statementRowIndex)).size === cluster.length;
        if (!valid || !uniqueIndices) {
          diagnostics.push(`bank account ${cluster[0]!.bankAccountId ?? "legacy"} has ambiguous statement order on ${date}`);
          accountAmbiguous = true;
          break;
        }
        sourceOrder = direction;
        ordered = [...cluster].sort((a, b) => direction === "ascending"
          ? Number(a.statementRowIndex) - Number(b.statementRowIndex)
          : Number(b.statementRowIndex) - Number(a.statementRowIndex));
      }
      for (const row of ordered) {
        const balanceOre = toOre(Number(row.balanceAfter));
        if (previousOre !== null && previousOre + toOre(Number(row.amount)) !== balanceOre) {
          diagnostics.push(`bank account ${row.bankAccountId ?? "legacy"} has inconsistent running balance at transaction ${row.id}`);
          accountAmbiguous = true;
          break;
        }
        previousOre = balanceOre;
        closing = row;
        if (cluster.length > 1 || date === [...byDate.keys()].sort().at(-1)) {
          // The final element is replaced below; retaining only the endpoint
          // makes this read model compact and traceable.
        }
      }
      if (accountAmbiguous) break;
      if (closing && date === [...byDate.keys()].sort().at(-1)) {
        provenance.push({ bankAccountId: closing.bankAccountId, transactionId: closing.id, transactionDate: closing.transactionDate, sourceOrder });
      }
    }
    if (accountAmbiguous || closing === null || previousOre === null) continue;
    totalOre += previousOre;
    knownAccounts += 1;
  }
  // A portfolio total is meaningful only when every imported account has a
  // provable statement closing balance. Do not sum the known subset.
  if (hadBalance && diagnostics.length > 0) return { status: "ambiguous", balance: null, provenance: [], diagnostics };
  if (!hadBalance && accountsWithoutBalances > 0) return { status: "no-balance-column", balance: null, provenance: [], diagnostics: [] };
  if (!hadBalance || knownAccounts === 0) return { status: "no-balance-column", balance: null, provenance: [], diagnostics: [] };
  return { status: "known", balance: fromOre(totalOre), provenance, diagnostics: [] };
}

/** Actual statement balance in kroner; null when its source is not provable. */
export function actualBankBalanceAsOf(db: Database, asOfDate: string): number | null {
  return resolveActualBankBalanceAsOf(db, asOfDate).balance;
}

/**
 * Why the actual statement balance is or is not known (#305, EJER-12).
 *
 *  - `"known"`            — a statement balance could be read.
 *  - `"no-balance-column"` — bank transactions WERE imported, but none carried
 *                            a running balance (the CSV had no balance column).
 *  - `"none"`            — no bank transaction has been imported at all.
 *
 * A null `actualBankBalanceAsOf` has these two very different causes; without
 * distinguishing them the cockpit said "intet kontoudtog importeret" even when
 * a CSV WAS imported (just without a balance column), so an owner who had just
 * imported would think the import silently failed. The bank-tab view already
 * makes this distinction inline; this shared helper lets the portfolio card and
 * the dashboard tell the SAME story with the Bank tab's exact wording.
 */
export function bankStatementStatusAsOf(
  db: Database,
  asOfDate: string,
): BankStatementStatus {
  return resolveActualBankBalanceAsOf(db, asOfDate).status;
}
