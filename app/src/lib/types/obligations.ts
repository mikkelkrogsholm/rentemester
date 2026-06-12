// Obligations / Forpligtelser (GET .../obligations?year=) — it. 7.
//
// All money fields below are kroner (DKK with decimals) — use `formatKroner`.

import type { FiscalYearEntry, StatementCompany } from "./common";

export type ObligationKind =
  | "vat"
  | "corporation-tax"
  | "annual-report"
  | "creditors"
  | "auditor"
  | "other";

export type ObligationRow = {
  kind: ObligationKind;
  /** A human Danish label, e.g. "Moms — Q2 2026". */
  label: string;
  /** The amount owed, kroner; positive is payable. */
  amount: number;
  /** The filing/payment deadline as YYYY-MM-DD, or null when none is known. */
  dueDate: string | null;
  /** Signed countdown from today to `dueDate`; null when `dueDate` is null. */
  daysRemaining: number | null;
  /** The ledger account the figure was read from, when one applies. */
  accountNo: string | null;
};

export type CompanyObligations = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  /** Payables sorted by due date, soonest first; dateless rows last. */
  obligations: ObligationRow[];
  /** Sum of every obligation's amount, kroner. */
  totalOwed: number;
};

export type ObligationsResponse = {
  ok: true;
  obligations: CompanyObligations;
};
