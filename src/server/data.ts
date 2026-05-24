// Read-side data assembly for the cockpit backend (#170) — re-export barrel.
//
// Every figure here is computed by an existing core function — this module
// only opens the right ledger, calls core, and shapes the JSON. No business
// logic is duplicated and nothing here mutates a ledger.
//
// #320: what was one ~3.7k-LOC god module is now a set of cohesive modules
// under `src/server/data/`:
//
//   data/shared.ts        — request-param resolvers, kroner rounding, the
//                           company/fiscal-year context the views share
//   data/bank.ts          — booked + actual bank-balance helpers
//   data/vat.ts           — VAT period selection and position computation
//   data/archive.ts       — #197 archived-year read helpers + the Arkiv view
//   data/exceptions.ts    — exception grouping for the "Opgaver" card
//   data/portfolio.ts     — portfolio overview + per-company dashboard data
//   data/statements.ts    — Overblik + the four core financial-statement views
//   data/company-views.ts — journal, bank, VAT, documents, invoices, contacts,
//                           obligations, cashflow + the company-settings view
//
// This file is a THIN BARREL: it re-exports exactly the public surface the
// pre-split `server/data.ts` exposed, so every existing
// `import ... from "./data"` / `"../server/data"` keeps resolving unchanged.
// The split is purely internal — no consumer file changes, no behaviour change.

// --- data/shared.ts --------------------------------------------------------
export {
  todayIsoDate,
  resolveAsOfDate,
  resolveYearParam,
  resolvePathYear,
  buildCompanyFiscalYears,
} from "./data/shared";
export type {
  FiscalYearEntry,
  CompanyFiscalYears,
  StatementCompanyBlock,
} from "./data/shared";

// --- data/exceptions.ts ----------------------------------------------------
export type { ExceptionGroup } from "./data/exceptions";

// --- data/vat.ts -----------------------------------------------------------
export type { VatPosition, VatRubrikker } from "./data/vat";

// --- data/archive.ts -------------------------------------------------------
export { buildCompanyArchiveYear } from "./data/archive";
export type { ArchiveBalanceRow, CompanyArchiveYear } from "./data/archive";

// --- data/portfolio.ts -----------------------------------------------------
export {
  buildPortfolioOverview,
  buildCompanyDashboardData,
} from "./data/portfolio";
export type {
  CompanyVatSummary,
  CompanySummary,
  PortfolioOverview,
  CompanyDashboardData,
} from "./data/portfolio";

// --- data/statements.ts ----------------------------------------------------
export {
  buildCompanyOverview,
  buildCompanyIncomeStatement,
  buildCompanyBalance,
  buildCompanyTrialBalance,
  buildCompanyMultiYear,
} from "./data/statements";
export type {
  OverviewMonth,
  OverviewVat,
  CompanyOverview,
  IncomeStatementLine,
  CompanyIncomeStatement,
  BalanceLine,
  BalanceSection,
  CompanyBalance,
  TrialBalanceRow,
  CompanyTrialBalance,
  MultiYearRow,
  CompanyMultiYear,
} from "./data/statements";

// --- data/payables-view.ts -------------------------------------------------
export { buildCompanyPayables } from "./data/payables-view";
export type {
  CompanyPayables,
  CompanyPayableRow,
  UnregisteredPurchaseDocumentRow,
  PayableExpenseAccountOption,
  PayableVendorOption,
} from "./data/payables-view";

// --- data/budget.ts --------------------------------------------------------
export {
  buildCompanyBudget,
  buildCompanyBudgetVsActual,
} from "./data/budget";
export type {
  CompanyBudgetLine,
  CompanyBudget,
  CompanyBudgetVsActualLine,
  CompanyBudgetVsActual,
} from "./data/budget";

// --- data/company-views.ts -------------------------------------------------
export {
  buildCompanySettings,
  syncCompanyCvr,
  buildCompanyJournal,
  buildCompanyBank,
  buildCompanyVat,
  buildCompanyDocuments,
  buildDocumentBookingOptions,
  resolveCompanyDocumentFile,
  resolveCompanyIssuedInvoicePdf,
  buildCompanyInvoices,
  buildCompanyContacts,
  buildCompanyObligations,
  buildCompanyCashflow,
  buildCompanyRecurringInvoices,
  buildCompanyMileage,
  buildCompanyAssets,
  buildAssetNextDepreciationPeriod,
} from "./data/company-views";
export type {
  CompanySettingsView,
  JournalLine,
  JournalEntry,
  CompanyJournal,
  BankTransactionRow,
  CompanyBank,
  CompanyVat,
  DocumentRow,
  CompanyDocuments,
  DocumentBookingOptions,
  DocumentBookingOptionsDocument,
  ExpenseAccountOption,
  UnmatchedBankOption,
  CompanyInvoiceRow,
  CompanyInvoices,
  ContactCustomerRow,
  ContactVendorRow,
  CompanyContacts,
  ObligationRow,
  CompanyObligations,
  CashflowMonth,
  CashflowBalancePoint,
  CompanyCashflow,
  RecurringInvoiceGenerationRow,
  RecurringInvoiceTemplateRow,
  CompanyRecurringInvoices,
  MileageEntryRow,
  MileageMonthRow,
  CompanyMileage,
  AssetRow,
  AssetWriteOffRow,
  CompanyAssets,
  NextDepreciationPeriodView,
} from "./data/company-views";
