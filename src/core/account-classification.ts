// Account → financial-statement section classification (#321).
//
// "Which section of the financial statements does an account belong to" is a
// single derivation that several read surfaces need: the live balance-sheet
// builder in `core/financial-statements.ts`, and the archive-aware statement
// views in `server/data.ts` (the archived Balance and the Flerårsoversigt),
// which classify a Dinero-exported `SaldoBalance` line by joining it to the
// live chart of accounts.
//
// Before #321 each of those re-derived the rule independently and could
// disagree — notably the Flerårsoversigt left `vat` accounts unclassified
// while the Balance view placed them by `normalBalance`. This module is the
// one place the rule lives, so every surface agrees.
//
// The rule, by `accounts.type` (constrained by the schema.sql CHECK):
//  - `asset`                       → assets
//  - `liability`                   → liabilities
//  - `equity`                      → equity
//  - `income`                      → income
//  - `expense`                     → expense
//  - `vat`  + normal_balance debit  → assets   (input VAT, a receivable)
//  - `vat`  + normal_balance credit → liabilities (output VAT, a payable)
//
// A `vat` account is placed by its normal balance: a debit-normal VAT account
// (input VAT / købsmoms) is a receivable and sits under assets; a credit-normal
// VAT account (output VAT / salgsmoms) is a payable and sits under liabilities.
// This is exactly the placement `buildBalanceSheet` documented and applied.

import type { AccountType } from "./financial-statements";

/** The financial-statement section an account's balance contributes to. */
export type AccountSection =
  | "asset"
  | "liability"
  | "equity"
  | "income"
  | "expense";

/**
 * The financial-statement section an account belongs to, derived from its
 * `type` and `normalBalance`. `vat` accounts are placed by their normal
 * balance (debit → asset, credit → liability); every other type maps to its
 * own section. Returns `null` when the type is unknown (e.g. an archived
 * account that has no row in the live chart of accounts).
 *
 * Pure: identical input always yields identical output.
 */
export function classifyAccountSection(
  type: AccountType | string | null | undefined,
  normalBalance: "debit" | "credit" | null | undefined,
): AccountSection | null {
  switch (type) {
    case "asset":
      return "asset";
    case "liability":
      return "liability";
    case "equity":
      return "equity";
    case "income":
      return "income";
    case "expense":
      return "expense";
    case "vat":
      return normalBalance === "debit" ? "asset" : "liability";
    default:
      return null;
  }
}
