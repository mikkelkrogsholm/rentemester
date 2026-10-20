// Journal / Posteringer wire types (GET .../journal?year=).
//
// All money fields below are kroner (DKK with decimals) — use `formatKroner`.

import type { FiscalYearEntry, StatementCompany, ReportMoney, ReportSourceMetadata } from "./common";

export type JournalLine = {
  /** Immutable ledger id used only to resolve its append-only dimension history. */
  journalLineId: number | null;
  accountNo: string;
  accountName: string;
  debit: number | string;
  credit: number | string;
  text: string | null;
  sourceId?: string; sourceAmount?: string; sourceRunningBalance?: string | null; balance?: ReportMoney; runningBalance?: ReportMoney; date?: string; voucher?: string; voucherType?: string; vatType?: string;
};

export type JournalEntry = {
  sourceId?: string;
  reversalOfEntryId?: number | null;
  reversalOfEntryNo?: string | null;
  reversedByEntryId?: number | null;
  reversedByEntryNo?: string | null;
  id: number;
  entryNo: string;
  date: string;
  text: string;
  /** Sum of the debit side — the entry total, kroner. */
  total: ReportMoney;
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

export type NativePostingOptions = { ruleVersion: string; accounts: Array<{ accountNo: string; name: string; type?: string; active: boolean; allowDirectPosting: boolean }> };

export type ArchiveJournalEntry = Omit<JournalEntry,"id"> & {id:null;sourceId:string};

export type CompanyJournal = ReportSourceMetadata & {
  nativePosting?: NativePostingOptions;
  slug: string;
  selectedYear: string;
  /** True when the entries are derived from the #197 archived Posteringer. */
  archived: boolean;
  /** The archive's source system when archived, else null. */
  archivedSource: string | null;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string | null;
  periodEnd: string | null;
  entries: Array<JournalEntry | ArchiveJournalEntry>;
  /** The account the entries are filtered to, when `?account=` is set. */
  accountFilter: { accountNo: string; name: string } | null;
};

export type JournalResponse = {
  ok: true;
  journal: CompanyJournal;
};
