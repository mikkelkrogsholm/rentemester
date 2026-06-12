// Journal / Posteringer wire types (GET .../journal?year=).
//
// All money fields below are kroner (DKK with decimals) — use `formatKroner`.

import type { FiscalYearEntry, StatementCompany } from "./common";

export type JournalLine = {
  accountNo: string;
  accountName: string;
  debit: number;
  credit: number;
  text: string | null;
};

export type JournalEntry = {
  id: number;
  entryNo: string;
  date: string;
  text: string;
  /** Sum of the debit side — the entry total, kroner. */
  total: number;
  lines: JournalLine[];
  /**
   * #379 — the id of the document (bilag) backing this entry, when one is
   * linked. `null` when the entry has no underlying document (e.g. a manual
   * kassekladde-post). Used by the UI to surface an "Åbn bilag" link.
   */
  documentId: number | null;
  /** The linked document's `document_no` for display next to the link. */
  documentNo: string | null;
};

export type CompanyJournal = {
  slug: string;
  selectedYear: string;
  /** True when the entries are derived from the #197 archived Posteringer. */
  archived: boolean;
  /** The archive's source system when archived, else null. */
  archivedSource: string | null;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  entries: JournalEntry[];
  /** The account the entries are filtered to, when `?account=` is set. */
  accountFilter: { accountNo: string; name: string } | null;
};

export type JournalResponse = {
  ok: true;
  journal: CompanyJournal;
};
