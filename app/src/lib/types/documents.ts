// Documents / Bilag wire types (GET .../documents).

import type { StatementCompany } from "./common";

export type DocumentRow = {
  id: number;
  documentNo: string | null;
  source: string;
  filename: string | null;
  documentType: string;
  internalVoucherKind?: "bank_evidenced" | "non_cash_balance_correction" | null;
  sourceBankTransactionId: number | null;
  accountingRationale: string | null;
  preparedBy: string | null;
  preparedByProgram: string | null;
  preparedAt?: string | null;
  supplierName: string | null;
  supplierVatOrCvr: string | null;
  supplierCountryCode: string | null;
  supplierIdentifierKind: string | null;
  supplierIdentityStatus: string | null;
  partyId?: string | null;
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
  purchaseVatLines?: Array<{ classification: string; netAmount: number; vatAmount?: number }> | null;
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
