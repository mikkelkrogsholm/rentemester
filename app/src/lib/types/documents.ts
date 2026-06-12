// Documents / Bilag wire types (GET .../documents).

import type { StatementCompany } from "./common";

export type DocumentRow = {
  id: number;
  documentNo: string | null;
  source: string;
  filename: string | null;
  documentType: string;
  supplierName: string | null;
  invoiceNo: string | null;
  invoiceDate: string | null;
  amountIncVat: number | null;
  currency: string;
  status: string;
  voucherRef: string | null;
  journalEntryNo: string | null;
  journalEntryId: number | null;
  /** The linked journal entry's posting text — what the voucher is for. */
  journalEntryText: string | null;
  /** The linked journal entry's total (summed debit side), kroner. */
  journalEntryTotal: number | null;
  /** True when the document has a stored file the cockpit can open. */
  hasFile: boolean;
};

export type CompanyDocuments = {
  slug: string;
  company: StatementCompany;
  documents: DocumentRow[];
  linkedCount: number;
  unlinkedCount: number;
};

export type DocumentsResponse = {
  ok: true;
  documents: CompanyDocuments;
};
