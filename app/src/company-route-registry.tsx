/**
 * Company-route registry: the one source of truth for a company page's URL,
 * navigation metadata and rendered view.  Keep the groups aligned with the
 * task areas shown in CompanyTaskNavigation.
 */
import type { ReactElement } from "react";
import { AccountingDraftsView } from "./views/AccountingDraftsView";
import { AccountingApprovalPolicyView } from "./views/AccountingApprovalPolicyView";
import { AccountsView } from "./views/AccountsView";
import { AccrualsView } from "./views/AccrualsView";
import { AnnualReportView } from "./views/AnnualReportView";
import { ArchiveView } from "./views/ArchiveView";
import { AssetsView } from "./views/AssetsView";
import { BalanceView } from "./views/BalanceView";
import { BankAccountsView } from "./views/BankAccountsView";
import { BankView } from "./views/BankView";
import { BilagsmailView } from "./views/BilagsmailView";
import { BookkeepingBatchView } from "./views/BookkeepingBatchView";
import { BudgetView } from "./views/BudgetView";
import { ContactsView } from "./views/ContactsView";
import { DashboardView } from "./views/DashboardView";
import { DimensionsView } from "./views/DimensionsView";
import { DocumentsView } from "./views/DocumentsView";
import { ExceptionsView } from "./views/ExceptionsView";
import { GdprView } from "./views/GdprView";
import { IncomeStatementView } from "./views/IncomeStatementView";
import { InvoicesView } from "./views/InvoicesView";
import { IntegrityView } from "./views/IntegrityView";
import { JournalView } from "./views/JournalView";
import { LiquidityView } from "./views/LiquidityView";
import { ManageCompanyView } from "./views/ManageCompanyView";
import { MileageView } from "./views/MileageView";
import { MultiYearView } from "./views/MultiYearView";
import { ObligationsView } from "./views/ObligationsView";
import { PayablesView } from "./views/PayablesView";
import { PurchaseOverviewView } from "./views/PurchaseOverviewView";
import { PeriodsView } from "./views/PeriodsView";
import { PostingRulesView } from "./views/PostingRulesView";
import { RecurringInvoicesView } from "./views/RecurringInvoicesView";
import { RetentionView } from "./views/RetentionView";
import { SuggestionsView } from "./views/SuggestionsView";
import { TrialBalanceView } from "./views/TrialBalanceView";
import { VatView } from "./views/VatView";
import { WorkspaceInboxView } from "./views/WorkspaceInboxView";
import { WorkspaceRegistryView } from "./views/WorkspaceRegistryView";
import {
  companyRouteForPath as findCompanyRouteForPath,
  type CompanyRoutePathDescriptor,
} from "./company-route-path";

export const COMPANY_TASK_AREAS = [
  { id: "overview", label: "Overblik" },
  { id: "bookkeeping", label: "Bogføring" },
  { id: "sales", label: "Salg og debitorer" },
  { id: "vat-periods", label: "Moms og perioder" },
  { id: "reports", label: "Rapporter og planlægning" },
  { id: "administration", label: "Virksomhedsadministration" },
] as const;

export type CompanyTaskAreaId = (typeof COMPANY_TASK_AREAS)[number]["id"];

export type CompanyRouteDescriptor = CompanyRoutePathDescriptor & {
  id: string;
  segment: string;
  label: string;
  area: CompanyTaskAreaId;
  element: ReactElement;
};

export const COMPANY_ROUTE_REGISTRY = [
  // Overblik
  { id: "dashboard", segment: "", label: "Overblik", area: "overview", element: <DashboardView /> },

  // Bogføring
  { id: "journal", segment: "posteringer", label: "Posteringer", area: "bookkeeping", element: <JournalView /> },
  { id: "drafts", segment: "kladder", label: "Kladder", area: "bookkeeping", element: <AccountingDraftsView /> },
  { id: "approval-policy", segment: "godkendelsespolitik", label: "Godkendelsespolitik", area: "bookkeeping", element: <AccountingApprovalPolicyView /> },
  { id: "posting-rules", segment: "posteringsregler", label: "Posteringsregler", area: "bookkeeping", element: <PostingRulesView /> },
  { id: "batch-bookkeeping", segment: "batchbogfoering", label: "Bogføring", area: "bookkeeping", element: <BookkeepingBatchView /> },
  { id: "bank", segment: "bank", label: "Bank", area: "bookkeeping", element: <BankView /> },
  { id: "documents", segment: "bilag", label: "Bilag", area: "bookkeeping", element: <DocumentsView /> },
  { id: "payables", segment: "leverandoerfaktura", label: "Leverandørfaktura", area: "bookkeeping", element: <PayablesView /> },
  { id: "purchase-overview", segment: "koebsoverblik", label: "Købsoverblik", area: "bookkeeping", element: <PurchaseOverviewView /> },
  { id: "mileage", segment: "koersel", label: "Kørsel", area: "bookkeeping", element: <MileageView /> },
  { id: "assets", segment: "anlaeg", label: "Anlæg", area: "bookkeeping", element: <AssetsView /> },
  { id: "suggestions", segment: "agent-forslag", label: "Agent-forslag", area: "bookkeeping", element: <SuggestionsView /> },
  { id: "exceptions", segment: "undtagelser", label: "Undtagelser", area: "bookkeeping", element: <ExceptionsView /> },

  // Salg og debitorer
  { id: "invoices", segment: "fakturaer", label: "Fakturaer", area: "sales", element: <InvoicesView /> },
  { id: "invoice-templates", segment: "faktura-skabeloner", label: "Skabeloner", area: "sales", element: <RecurringInvoicesView /> },
  { id: "contacts", segment: "kontakter", label: "Kontakter", area: "sales", element: <ContactsView /> },

  // Moms og perioder
  { id: "vat", segment: "moms", label: "Moms", area: "vat-periods", element: <VatView /> },
  { id: "period-lock", segment: "periodelas", label: "Periodelås", area: "vat-periods", element: <PeriodsView /> },
  { id: "accruals", segment: "periodisering", label: "Periodisering", area: "vat-periods", element: <AccrualsView /> },

  // Rapporter og planlægning
  { id: "income-statement", segment: "resultatopgorelse", label: "Resultatopgørelse", area: "reports", element: <IncomeStatementView /> },
  { id: "balance", segment: "balance", label: "Balance", area: "reports", element: <BalanceView /> },
  { id: "trial-balance", segment: "saldobalance", label: "Saldobalance", area: "reports", element: <TrialBalanceView /> },
  { id: "obligations", segment: "forpligtelser", label: "Forpligtelser", area: "reports", element: <ObligationsView /> },
  { id: "liquidity", segment: "likviditet", label: "Likviditet", area: "reports", element: <LiquidityView /> },
  { id: "budget", segment: "budget", label: "Budget", area: "reports", element: <BudgetView /> },
  { id: "multi-year", segment: "fleraar", label: "Flerår", area: "reports", element: <MultiYearView /> },
  { id: "annual-report", segment: "aarsrapport", label: "Årsrapport", area: "reports", element: <AnnualReportView /> },

  // Virksomhedsadministration
  { id: "workspace-register", segment: "workspace-register", label: "Workspace-register", area: "administration", element: <WorkspaceRegistryView /> },
  { id: "workspace-inbox", segment: "workspace-inbox", label: "Fælles indbakke", area: "administration", element: <WorkspaceInboxView /> },
  { id: "archive", segment: "arkiv", label: "Arkiv", area: "administration", element: <ArchiveView /> },
  { id: "manage", segment: "manage", label: "Virksomhedsoplysninger", area: "administration", element: <ManageCompanyView /> },
  { id: "retention", segment: "retention", label: "Retention", area: "administration", element: <RetentionView /> },
  { id: "integrity", segment: "integritet", label: "Integritet", area: "administration", element: <IntegrityView /> },
  { id: "accounts", segment: "kontoplan", label: "Kontoplan", area: "administration", element: <AccountsView /> },
  { id: "dimensions", segment: "dimensioner", label: "Dimensioner", area: "administration", element: <DimensionsView /> },
  { id: "bank-accounts", segment: "bankkonti", label: "Bankkonti", area: "administration", element: <BankAccountsView /> },
  { id: "gdpr", segment: "gdpr", label: "GDPR", area: "administration", element: <GdprView /> },
  { id: "receipt-email", segment: "bilagsmail", label: "Bilagsmail", area: "administration", element: <BilagsmailView /> },
] as const satisfies readonly CompanyRouteDescriptor[];

export type CompanyRouteId = (typeof COMPANY_ROUTE_REGISTRY)[number]["id"];
export type CompanyRouteDefinition = Omit<(typeof COMPANY_ROUTE_REGISTRY)[number], "element">;

export function companyRoutePattern(segment: string): string {
  return segment ? `/companies/:slug/${segment}` : "/companies/:slug";
}

/** Fails closed if the single route registry becomes internally inconsistent. */
export function assertCompanyRouteRegistry() {
  const duplicateIds = COMPANY_ROUTE_REGISTRY.filter(
    (route, index, routes) => routes.findIndex((candidate) => candidate.id === route.id) !== index,
  );
  const duplicateSegments = COMPANY_ROUTE_REGISTRY.filter(
    (route, index, routes) => routes.findIndex((candidate) => candidate.segment === route.segment) !== index,
  );
  const invalidAreas = COMPANY_ROUTE_REGISTRY.filter(
    (route) => !COMPANY_TASK_AREAS.some((area) => area.id === route.area),
  );
  const missingElements = COMPANY_ROUTE_REGISTRY.filter((route) => !route.element);

  if (duplicateIds.length || duplicateSegments.length || invalidAreas.length || missingElements.length) {
    throw new Error(
      `Company route registry failed: duplicateIds=${duplicateIds.length}; duplicateSegments=${duplicateSegments.length}; invalidAreas=${invalidAreas.length}; missingElements=${missingElements.length}`,
    );
  }
}

export function companyRouteForPath(pathname: string): CompanyRouteDefinition | undefined {
  return findCompanyRouteForPath(pathname, COMPANY_ROUTE_REGISTRY);
}
