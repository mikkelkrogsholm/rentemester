// The single HTTP seam between the cockpit SPA and `rentemester serve` (#170).
//
// Every backend response is `{ok:true,...}` or
// `{ok:false, errors:[...], code:"..."}` — the unified envelope shared with
// MCP + CLI (#368). `request` normalises both into a typed value or a thrown
// `ApiError`, so views only ever deal with data or a single error type.
//
// This file is now a thin barrel that assembles per-domain modules from
// `./api/*` into the single `api` object the cockpit imports. Domain files
// hold the actual `fetch`-call bodies + their wire-input/output types.

export { ApiError } from "./api/_shared";

// Re-export the wire-input/output types each domain owns.
export type {
  BankImportInput,
  BankImportSummary,
} from "./api/bank";
export type {
  DataImportCounts,
  DataImportDetected,
  DataImportInput,
  DataImportSummary,
  DocumentBookExpenseInput,
  DocumentBookExpenseSummary,
  DocumentBookingOptions,
  DocumentBookingOptionsDocument,
  DocumentVatPreflight,
  DocumentIngestInput,
  DocumentIngestMetadata,
  ExpenseAccountOption,
  ExpenseVatTreatment,
  UnmatchedBankOption,
} from "./api/documents";
export type { AccountantExportResult } from "./api/accountant";
export type {
  InvoiceCreditNoteInput,
  InvoiceCreditNoteSummary,
  InvoiceIssueInput,
  InvoiceIssueSummary,
  InvoiceLineInput,
  InvoicePartyInput,
  InvoiceSendEInvoiceInput,
  InvoiceSendEInvoiceSummary,
  InvoiceSendEmailInput,
  InvoiceSendEmailSummary,
  InvoiceSendReminderInput,
  InvoiceSendReminderSummary,
  InvoiceSettleInput,
  InvoiceSettleSummary,
} from "./api/invoices";

import { accountantApi } from "./api/accountant";
import { accountingDraftsApi } from "./api/accounting-drafts";
import { accountsApi } from "./api/accounts";
import { accrualsApi } from "./api/accruals";
import { agentSuggestionsApi, exceptionsApi } from "./api/exceptions";
import { annualReportApi } from "./api/annual-report";
import { assetsApi } from "./api/assets";
import { bankApi } from "./api/bank";
import { bilagsmailApi } from "./api/bilagsmail";
import { budgetApi } from "./api/budget";
import { companiesApi } from "./api/companies";
import { contactsApi } from "./api/contacts";
import { dashboardApi } from "./api/dashboard";
import { documentsApi } from "./api/documents";
import { gdprApi } from "./api/gdpr";
import { groupApi } from "./api/group";
import { integrityApi } from "./api/integrity";
import { invoicesApi } from "./api/invoices";
import { mileageApi } from "./api/mileage";
import { payablesApi } from "./api/payables";
import { periodsApi } from "./api/periods";
import { retentionApi } from "./api/retention";
import { statementsApi } from "./api/statements";
import { systemApi } from "./api/system";
import { vatApi } from "./api/vat";
import { workspaceApi } from "./api/workspace";
import { postingRulesApi } from "./api/posting-rules";
import { bookkeepingBatchApi } from "./api/bookkeeping-batch";
import { workspaceRegistryApi } from "./api/workspace-registry";
import { cfoAnalyticsApi } from "./api/cfo-analytics";
import { supplierCommitmentsApi } from "./api/supplier-commitments";
import { dimensionsApi } from "./api/dimensions";
import { purchaseCasesApi } from "./api/purchase-cases";

// One method per key — the earlier legacy `closePeriod` / `reopenPeriod` /
// `resolveException` duplicates (#UI-9) have been deleted, so no spread-order
// shadowing is load-bearing any more.
export const api = {
  ...accountingDraftsApi,
  ...systemApi,
  ...retentionApi,
  ...integrityApi,
  ...accountsApi,
  ...annualReportApi,
  ...accrualsApi,
  ...gdprApi,
  ...bilagsmailApi,
  ...bankApi,
  ...companiesApi,
  ...dashboardApi,
  ...statementsApi,
  ...vatApi,
  ...documentsApi,
  ...invoicesApi,
  ...contactsApi,
  ...mileageApi,
  ...budgetApi,
  ...periodsApi,
  ...exceptionsApi,
  ...accountantApi,
  ...assetsApi,
  ...payablesApi,
  ...agentSuggestionsApi,
  ...groupApi,
  ...workspaceApi,
  ...postingRulesApi,
  ...bookkeepingBatchApi,
  ...workspaceRegistryApi,
  ...cfoAnalyticsApi,
  ...supplierCommitmentsApi,
  ...dimensionsApi,
  ...purchaseCasesApi,
};
