// Payables / Leverandørfaktura wire types (GET .../payables) — #340.
//
// The cockpit's Leverandørfaktura-arbejdsbord (#340). All money fields in
// kroner; use `formatKroner`.

import type { FiscalYearEntry, StatementCompany } from "./common";

export type PayableStatusWire = "open" | "paid";
export type PayableAgingBucketWire =
  | "not-due"
  | "0-30"
  | "31-60"
  | "61-90"
  | "90+";
export type PayableListStatusFilter = "open" | "paid" | "overdue" | "all";

export type CompanyPayableRow = {
  payableId: number;
  documentId: number;
  billNo: string | null;
  billDate: string;
  dueDate: string;
  supplierName: string | null;
  vendorId: number | null;
  grossAmount: number;
  currency: string;
  paidAmount: number;
  openBalance: number;
  status: PayableStatusWire;
  isOverdue: boolean;
  overdueDays: number;
  agingBucket: PayableAgingBucketWire;
};

export type UnregisteredPurchaseDocumentRow = {
  id: number;
  documentNo: string | null;
  invoiceNo: string | null;
  invoiceDate: string | null;
  supplierName: string | null;
  amountIncVat: number | null;
  vatAmount: number | null;
  currency: string;
};

export type PayableExpenseAccountOption = {
  accountNo: string;
  name: string;
  defaultVatCode: string | null;
};

export type PayableVendorOption = {
  id: number;
  name: string;
  defaultExpenseAccount: string | null;
  defaultVatTreatment: string | null;
};

export type DirectBankCorrectionCandidate = {
  documentId: number; documentNo: string | null; invoiceNo: string | null;
  invoiceDate: string; supplierName: string | null; amountIncVat: number;
  vatAmount: number | null; currency: string; bankTransactionId: number;
  bankDate: string; originalJournalEntryId: number; originalJournalEntryNo: string;
};

export type CompanyPayables = {
  slug: string;
  asOfDate: string;
  status: PayableListStatusFilter;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  rows: CompanyPayableRow[];
  count: number;
  totalOpenBalance: number;
  overdueOpenBalance: number;
  notYetDueOpenBalance: number;
  unregisteredDocuments: UnregisteredPurchaseDocumentRow[];
  directBankCorrectionCandidates?: DirectBankCorrectionCandidate[];
  expenseAccounts: PayableExpenseAccountOption[];
  vendors: PayableVendorOption[];
};

export type PayablesResponse = {
  ok: true;
  payables: CompanyPayables;
};

/** Input for `api.registerPayable` (#340). */
export type PayableRegisterInput = {
  documentId: number;
  billDate: string;
  dueDate: string;
  expenseAccountNo: string;
  vatTreatment?: "standard" | "exempt";
  vendorId?: number;
  note?: string;
};

/** The register result the server echoes back (#340). */
export type PayableRegisterSummary = {
  payableId: number | null;
  documentId: number | null;
  supplierName: string | null;
  billNo: string | null;
  grossAmount: number;
  netAmount: number;
  vatAmount: number;
  dueDate: string | null;
  entryId: number | null;
  entryNo: string | null;
};

/** Input for `api.payPayable` (#340). */
export type PayablePayInput = {
  payableId: number;
  bankTransactionId: number;
  paymentDate?: string;
  amount?: number;
  paymentAccountNo?: string;
  note?: string;
};

/** The pay result the server echoes back (#340). */
export type PayablePaySummary = {
  paymentId: number | null;
  journalEntryId: number | null;
  payableId: number;
  openBalance: number | null;
};

export type DirectBankPayableCorrectionInput = {
  documentId: number; bankTransactionId: number; billDate: string; dueDate: string;
  expenseAccountNo: string; vatTreatment?: "standard" | "exempt" | "non_deductible";
  vendorId?: number; note?: string;
};

export type DirectBankPayableCorrectionPlan = DirectBankPayableCorrectionInput & {
  schemaVersion: string; planHash: string; documentHash: string; originalJournalHash: string;
  summary?: {
    document: { id: number; documentNo: string | null; invoiceNo: string | null; invoiceDate: string; supplierName: string | null; currency: string; grossAmount: number; vatAmount: number | null };
    bank: { id: number; date: string; text: string; currency: string; signedAmount: number; amount: number };
    originalJournal: { id: number; entryNo: string; date: string; hash: string };
    expenseAccount: { accountNo: string; name: string };
    effectiveVatTreatment: "standard" | "exempt" | "non_deductible";
  };
  originalJournalEntryId: number; originalJournalEntryNo?: string; originalJournalDate?: string;
  documentVatAmount?: number | null; bankDate: string; bankAmount: number;
};
