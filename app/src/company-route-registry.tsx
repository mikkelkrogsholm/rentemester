/**
 * Company-route registry: the one source of truth for a company page's URL,
 * navigation metadata and rendered view.  Keep the groups aligned with the
 * task areas shown in CompanyTaskNavigation.
 */
import type { ReactElement } from "react";
import { TasksView } from "./views/TasksView";
import { AccountingDraftsView } from "./views/AccountingDraftsView";
import { AttentionView } from "./views/AttentionView";
import { InvoiceCreateView } from "./views/InvoiceCreateView";
import { DocumentBookingView } from "./views/DocumentBookingView";
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
import { DocumentsView, DocumentDetailView } from "./views/DocumentsView";
import { ExceptionsView } from "./views/ExceptionsView";
import { GdprView } from "./views/GdprView";
import { IncomeStatementView } from "./views/IncomeStatementView";
import { InvoicesView, InvoiceDetailView } from "./views/InvoicesView";
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
import { PartyHubView, PartyProfileView } from "./views/PartyHubView";
import {
  companyRouteForPath as findCompanyRouteForPath,
  type CompanyRoutePathDescriptor,
} from "./company-route-path";

export const COMPANY_TASK_AREAS = [
  { id: "status", label: "Status", destination: "" },
  { id: "attention", label: "Kræver opmærksomhed", destination: "opmaerksomhed" },
  { id: "money-documents", label: "Penge og bilag", destination: "bank" },
  { id: "invoices", label: "Fakturaer", destination: "fakturaer" },
  { id: "vat-deadlines", label: "Moms og frister", destination: "moms" },
  { id: "reports", label: "Rapporter", destination: "resultatopgorelse" },
  { id: "knowledge", label: "Viden", destination: "parter" },
  { id: "administration", label: "Administration", destination: "manage" },
] as const;

export type CompanyTaskAreaId = (typeof COMPANY_TASK_AREAS)[number]["id"];

export type CompanyRouteDescriptor = CompanyRoutePathDescriptor & {
  id: string;
  segment: string;
  label: string;
  area: CompanyTaskAreaId;
  element: ReactElement;
  /** Human-facing grouping used by the Administration landing page. */
  administrationGroup?: "profile" | "daily" | "advanced";
  administrationPurpose?: string;
  administrationNextStep?: string;
};

export const COMPANY_ROUTE_REGISTRY = [
  // Overblik
  { id: "dashboard", segment: "", label: "Status", area: "status", element: <DashboardView /> },
  { id: "tasks", segment: "opgaver", label: "Opgaver", area: "attention", element: <TasksView /> },
  { id: "attention", segment: "opmaerksomhed", label: "Kræver opmærksomhed", area: "attention", element: <AttentionView /> },

  // Bogføring
  { id: "journal", segment: "posteringer", label: "Posteringer", area: "money-documents", element: <JournalView /> },
  { id: "drafts", segment: "kladder", label: "Kladder", area: "money-documents", element: <AccountingDraftsView /> },
  { id: "approval-policy", segment: "godkendelsespolitik", label: "Godkendelsespolitik", area: "administration", element: <AccountingApprovalPolicyView />, administrationGroup: "advanced", administrationPurpose: "Fastlæg sikker godkendelse af bogføring.", administrationNextStep: "Gennemgå godkendelsespolitik" },
  { id: "posting-rules", segment: "posteringsregler", label: "Posteringsregler", area: "administration", element: <PostingRulesView />, administrationGroup: "advanced", administrationPurpose: "Gennemgå automatiske bogføringsregler.", administrationNextStep: "Gennemgå posteringsregler" },
  { id: "batch-bookkeeping", segment: "batchbogfoering", label: "Bogføring", area: "money-documents", element: <BookkeepingBatchView /> },
  { id: "bank", segment: "bank", label: "Bank", area: "money-documents", element: <BankView /> },
  { id: "documents", segment: "bilag", label: "Bilag", area: "money-documents", element: <DocumentsView /> },
  { id: "payables", segment: "leverandoerfaktura", label: "Leverandørfaktura", area: "invoices", element: <PayablesView /> },
  { id: "purchase-overview", segment: "koebsoverblik", label: "Købsoverblik", area: "money-documents", element: <PurchaseOverviewView /> },
  { id: "mileage", segment: "koersel", label: "Kørsel", area: "money-documents", element: <MileageView /> },
  { id: "assets", segment: "anlaeg", label: "Anlæg", area: "money-documents", element: <AssetsView /> },
  { id: "suggestions", segment: "agent-forslag", label: "Agent-forslag", area: "attention", element: <SuggestionsView /> },
  { id: "exceptions", segment: "undtagelser", label: "Undtagelser", area: "attention", element: <ExceptionsView /> },

  // Salg og debitorer
  { id: "invoices", segment: "fakturaer", label: "Fakturaer", area: "invoices", element: <InvoicesView /> },
  { id: "invoice-templates", segment: "faktura-skabeloner", label: "Skabeloner", area: "invoices", element: <RecurringInvoicesView /> },
  { id: "contacts", segment: "kontakter", label: "Kontakter", area: "knowledge", element: <ContactsView /> },
  { id: "party-hub", segment: "parter", label: "Parter", area: "knowledge", element: <PartyHubView /> },

  // Moms og perioder
  { id: "vat", segment: "moms", label: "Moms", area: "vat-deadlines", element: <VatView /> },
  { id: "period-lock", segment: "periodelas", label: "Periodelås", area: "vat-deadlines", element: <PeriodsView /> },
  { id: "accruals", segment: "periodisering", label: "Periodisering", area: "vat-deadlines", element: <AccrualsView /> },

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
  { id: "workspace-register", segment: "workspace-register", label: "Styring og dokumentation", area: "administration", element: <WorkspaceRegistryView />, administrationGroup: "advanced", administrationPurpose: "Fælles styring, governance og dokumentation.", administrationNextStep: "Åbn styringen" },
  { id: "workspace-inbox", segment: "workspace-inbox", label: "Fælles indbakke", area: "administration", element: <WorkspaceInboxView />, administrationGroup: "daily", administrationPurpose: "Fælles indbakke for arbejdsområdet.", administrationNextStep: "Åbn indbakken" },
  { id: "archive", segment: "arkiv", label: "Arkiv", area: "administration", element: <ArchiveView />, administrationGroup: "advanced", administrationPurpose: "Læs tidligere, skrivebeskyttede regnskabsår.", administrationNextStep: "Se arkivet" },
  { id: "manage", segment: "manage", label: "Virksomhedsprofil", area: "administration", element: <ManageCompanyView />, administrationGroup: "profile", administrationPurpose: "Redigér virksomhedens stamdata og betalingsoplysninger.", administrationNextStep: "Redigér profil" },
  { id: "retention", segment: "retention", label: "Opbevaring", area: "administration", element: <RetentionView />, administrationGroup: "advanced", administrationPurpose: "Se opbevaringspligt og udløb.", administrationNextStep: "Gennemgå opbevaring" },
  { id: "integrity", segment: "integritet", label: "Integritet og backup", area: "administration", element: <IntegrityView />, administrationGroup: "advanced", administrationPurpose: "Kontrollér bogføringens integritet og backup.", administrationNextStep: "Verificér integritet" },
  { id: "accounts", segment: "kontoplan", label: "Kontoplan", area: "administration", element: <AccountsView />, administrationGroup: "daily", administrationPurpose: "Find og gennemgå konti til bogføring.", administrationNextStep: "Gennemgå kontoplan" },
  { id: "dimensions", segment: "dimensioner", label: "Dimensioner", area: "administration", element: <DimensionsView />, administrationGroup: "daily", administrationPurpose: "Organisér bogføring efter fx projekt eller afdeling.", administrationNextStep: "Opret dimension" },
  { id: "bank-accounts", segment: "bankkonti", label: "Bankkonti", area: "administration", element: <BankAccountsView />, administrationGroup: "daily", administrationPurpose: "Registrér konti, der bruges i bankarbejdet.", administrationNextStep: "Opret bankkonto" },
  { id: "gdpr", segment: "gdpr", label: "GDPR", area: "administration", element: <GdprView />, administrationGroup: "advanced", administrationPurpose: "Find og håndtér personoplysninger sikkert.", administrationNextStep: "Find oplysninger" },
  { id: "receipt-email", segment: "bilagsmail", label: "Bilagsmail", area: "administration", element: <BilagsmailView />, administrationGroup: "daily", administrationPurpose: "Modtag bilag i én fælles indbakke.", administrationNextStep: "Vælg bilagsmail" },
] as const satisfies readonly CompanyRouteDescriptor[];

/** Fixed pages retain main's exhaustive page inventory; flows inherit a page. */
export const COMPANY_FLOW_ROUTE_REGISTRY = [
  { id: "invoice-create", segment: "fakturaer/ny", label: "Udsted faktura", area: "invoices", kind: "flow", parentId: "invoices", element: <InvoiceCreateView /> },
  { id: "invoice-detail", segment: "fakturaer/:documentId", label: "Fakturadetaljer", area: "invoices", kind: "flow", parentId: "invoices", element: <InvoiceDetailView /> },
  { id: "document-detail", segment: "bilag/:documentId", label: "Bilagsdetaljer", area: "money-documents", kind: "flow", parentId: "documents", element: <DocumentDetailView /> },
  { id: "document-booking", segment: "bilag/:documentId/bogfoer", label: "Bogfør bilag", area: "money-documents", kind: "flow", parentId: "documents", element: <DocumentBookingView /> },
  { id: "party-profile", segment: "parter/:partyId", label: "Partsprofil", area: "knowledge", kind: "flow", parentId: "party-hub", element: <PartyProfileView /> },
] as const satisfies readonly (CompanyRouteDescriptor & { kind: "flow"; parentId: (typeof COMPANY_ROUTE_REGISTRY)[number]["id"] })[];

export const COMPANY_ROUTE_DEFINITIONS = [
  ...COMPANY_ROUTE_REGISTRY.map((route) => ({ ...route, kind: "page" as const, parentId: undefined })),
  ...COMPANY_FLOW_ROUTE_REGISTRY,
] as const;

export type CompanyRouteId = (typeof COMPANY_ROUTE_DEFINITIONS)[number]["id"];
export type CompanyRouteDefinition = Omit<(typeof COMPANY_ROUTE_DEFINITIONS)[number], "element">;
export type CompanyYearScope = "year" | "company" | "multi-year" | "vat-period";
export function companyYearScope(id: CompanyRouteId): CompanyYearScope {
  if (["tasks", "attention", "purchase-overview", "approval-policy", "party-hub", "party-profile", "documents", "document-detail", "workspace-register", "workspace-inbox", "contacts", "invoice-templates", "manage", "accounts", "dimensions", "bank-accounts", "gdpr", "retention", "integrity", "receipt-email", "posting-rules", "drafts", "suggestions", "exceptions"].includes(id)) return "company";
  if (id === "multi-year") return "multi-year";
  if (id === "vat") return "vat-period";
  // The batch workbench on main uses the selected canonical fiscal period.
  return "year";
}

export function companyRoutePattern(segment: string): string {
  return segment ? `/companies/:slug/${segment}` : "/companies/:slug";
}

/** Fails closed if the single route registry becomes internally inconsistent. */
export function assertCompanyRouteRegistry() {
  const duplicateIds = COMPANY_ROUTE_DEFINITIONS.filter(
    (route, index, routes) => routes.findIndex((candidate) => candidate.id === route.id) !== index,
  );
  const duplicateSegments = COMPANY_ROUTE_DEFINITIONS.filter(
    (route, index, routes) => routes.findIndex((candidate) => candidate.segment === route.segment) !== index,
  );
  const invalidAreas = COMPANY_ROUTE_DEFINITIONS.filter(
    (route) => !COMPANY_TASK_AREAS.some((area) => area.id === route.area),
  );
  const missingElements = COMPANY_ROUTE_DEFINITIONS.filter((route) => !route.element);

  if (duplicateIds.length || duplicateSegments.length || invalidAreas.length || missingElements.length) {
    throw new Error(
      `Company route registry failed: duplicateIds=${duplicateIds.length}; duplicateSegments=${duplicateSegments.length}; invalidAreas=${invalidAreas.length}; missingElements=${missingElements.length}`,
    );
  }
}

export function companyRouteForPath(pathname: string): CompanyRouteDefinition | undefined {
  return findCompanyRouteForPath(pathname, COMPANY_ROUTE_DEFINITIONS);
}
