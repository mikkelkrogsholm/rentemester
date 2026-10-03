import { ButtonLink } from "./components/ui";
// App shell + routing for the cockpit SPA (#171).
//
// Routes:
//   /                                   portfolio overview (→ onboarding)
//   /companies/new                      add a company
//   /companies/:slug                    Overblik (per-company dashboard)
//   /companies/:slug/resultatopgorelse  Resultatopgørelse (income statement)
//   /companies/:slug/balance            Balance (balance sheet)
//   /companies/:slug/saldobalance       Saldobalance (trial balance)
//   /companies/:slug/forpligtelser      Forpligtelser (obligations / payables)
//   /companies/:slug/likviditet         Likviditet (cash flow / pengestrøm)
//   /companies/:slug/posteringer        Posteringer (journal + drill-down)
//   /companies/:slug/bank               Bank (transactions + reconciliation)
//   /companies/:slug/moms               Moms (VAT return)
//   /companies/:slug/bilag              Bilag (ingested documents)
//   /companies/:slug/arkiv              Om arkivet (read-only #197 explainer)
//   /companies/:slug/fleraar            Flerårsoversigt (multi-year comparison)
//   /companies/:slug/fakturaer          Fakturaer (issued invoices)
//   /companies/:slug/kontakter          Kontakter (customers + vendors)
//   /companies/:slug/koersel            Kørsel (mileage register, #335)
//   /companies/:slug/anlaeg             Anlæg (fixed assets + depreciation)
//   /companies/:slug/manage             rename / archive
//   /help                                hjælp og support (#421)
//
// The per-company views share a sub-navigation and a fiscal-year selector
// (`CompanyNav`); the chosen year is carried in the URL as `?year=`.

import { NavLink, Route, Routes, Link, useLocation, useParams, useSearchParams } from "react-router-dom";
import { AuthProvider, useAuth } from "./lib/auth-context";
import { api } from "./lib/api";
import { useAsync } from "./lib/useAsync";
import { ForgotPasswordView, LoginView, ResetPasswordView, VerificationRecoveryView } from "./views/LoginView";
import { MfaEnrollmentView, VerificationRequiredView } from "./views/MfaEnrollmentView";
import { AccountMenu } from "./components/AccountMenu";
import { CompanySwitcher } from "./components/CompanySwitcher";
import { CockpitLayout, SkipLink } from "./components/AppChrome";
import { useCapabilities } from "./lib/useCapabilities";
import { ErrorState } from "./components/Feedback";
import packageJson from "../package.json";
import { PortfolioView } from "./views/PortfolioView";
import { AddCompanyView } from "./views/AddCompanyView";
import { DashboardView } from "./views/DashboardView";
import { IncomeStatementView } from "./views/IncomeStatementView";
import { BalanceView } from "./views/BalanceView";
import { TrialBalanceView } from "./views/TrialBalanceView";
import { ObligationsView } from "./views/ObligationsView";
import { LiquidityView } from "./views/LiquidityView";
import { BudgetView } from "./views/BudgetView";
import { JournalView } from "./views/JournalView";
import { BankView } from "./views/BankView";
import { VatView } from "./views/VatView";
import { DocumentsView, DocumentDetailView } from "./views/DocumentsView";
import { ArchiveView } from "./views/ArchiveView";
import { MultiYearView } from "./views/MultiYearView";
import { InvoicesView, InvoiceDetailView } from "./views/InvoicesView";
import { PayablesView } from "./views/PayablesView";
import { RecurringInvoicesView } from "./views/RecurringInvoicesView";
import { ContactsView } from "./views/ContactsView";
import { MileageView } from "./views/MileageView";
import { AssetsView } from "./views/AssetsView";
import { SuggestionsView } from "./views/SuggestionsView";
import { ManageCompanyView } from "./views/ManageCompanyView";
import { HelpView } from "./views/HelpView";
import { RulesView } from "./views/RulesView";
import { RetentionView } from "./views/RetentionView";
import { IntegrityView } from "./views/IntegrityView";
import { AccountsView } from "./views/AccountsView";
import { ExceptionsView } from "./views/ExceptionsView";
import { PeriodsView } from "./views/PeriodsView";
import { BankAccountsView } from "./views/BankAccountsView";
import { GdprView } from "./views/GdprView";
import { AccrualsView } from "./views/AccrualsView";
import { AnnualReportView } from "./views/AnnualReportView";
import { BilagsmailView } from "./views/BilagsmailView";
import { GroupOverviewView } from "./views/GroupOverviewView";
import { AccountingDraftsView } from "./views/AccountingDraftsView";
import { InvitationView } from "./views/InvitationView";
import { WorkspaceAccessView } from "./views/WorkspaceAccessView";
import { PostingRulesView } from "./views/PostingRulesView";
import { BookkeepingBatchView } from "./views/BookkeepingBatchView";
import { WorkspaceRegistryView } from "./views/WorkspaceRegistryView";
import { WorkspaceInboxView } from "./views/WorkspaceInboxView";
import { CfoCockpitView } from "./views/CfoCockpitView";
import { DimensionsView } from "./views/DimensionsView";
import {
  CompanyNavigationShell,
} from "./components/CompanyNav";
import { InvoiceCreateView } from "./views/InvoiceCreateView";
import { DocumentBookingView } from "./views/DocumentBookingView";
import { cloneElement, type ReactElement } from "react";
import {
  COMPANY_ROUTE_DEFINITIONS,
  assertCompanyRouteCoverage,
  companyRoutePattern,
  type CompanyRouteId,
  companyRouteForPath,
} from "./company-navigation";

const COMPANY_ROUTE_ELEMENTS: Record<CompanyRouteId, ReactElement> = {
  dashboard: <DashboardView />,
  "income-statement": <IncomeStatementView />,
  balance: <BalanceView />,
  "trial-balance": <TrialBalanceView />,
  obligations: <ObligationsView />,
  liquidity: <LiquidityView />,
  budget: <BudgetView />,
  journal: <JournalView />,
  drafts: <AccountingDraftsView />,
  "posting-rules": <PostingRulesView />,
  "batch-bookkeeping": <BookkeepingBatchView />,
  bank: <BankView />,
  vat: <VatView />,
  documents: <DocumentsView />,
  payables: <PayablesView />,
  invoices: <InvoicesView />,
  "invoice-templates": <RecurringInvoicesView />,
  contacts: <ContactsView />,
  "workspace-register": <WorkspaceRegistryView />,
  "workspace-inbox": <WorkspaceInboxView />,
  mileage: <MileageView />,
  assets: <AssetsView />,
  suggestions: <SuggestionsView />,
  archive: <ArchiveView />,
  "multi-year": <MultiYearView />,
  manage: <ManageCompanyView />,
  retention: <RetentionView />,
  integrity: <IntegrityView />,
  accounts: <AccountsView />,
  dimensions: <DimensionsView />,
  exceptions: <ExceptionsView />,
  "period-lock": <PeriodsView />,
  "bank-accounts": <BankAccountsView />,
  gdpr: <GdprView />,
  accruals: <AccrualsView />,
  "annual-report": <AnnualReportView />,
  "receipt-email": <BilagsmailView />,
  "invoice-create": <InvoiceCreateView />,
  "invoice-detail": <InvoiceDetailView />,
  "document-detail": <DocumentDetailView />,
  "document-booking": <DocumentBookingView />,
};

// Keep the executable route registration and the navigation catalogue in lockstep.
assertCompanyRouteCoverage(Object.keys(COMPANY_ROUTE_ELEMENTS));

export function App() {
  const health = useAsync((signal) => api.health({ signal }), []);
  const profile = health.data?.deploymentProfile;
  if (health.loading) return <div className="state-msg">Starter Rentemester…</div>;
  // This gate deliberately has no fallback. A reverse proxy error or an old
  // server must not accidentally expose a local/trusted cockpit in production.
  if (
    health.error ||
    (profile !== "local" && profile !== "local-container" && profile !== "hosted")
  ) {
    return <div className="state-msg" role="alert">Kunne ikke bekræfte Rentemesters sikkerhedsprofil. Prøv igen senere.</div>;
  }
  return <AuthProvider hosted={profile === "hosted"}><AuthGate /></AuthProvider>;
}

function AuthGate() {
  const { hosted, loading, session, context } = useAuth();
  const location = useLocation();
  if (!hosted) return <CockpitApp />;
  if (loading) return <div className="state-msg">Kontrollerer din session…</div>;
  if (location.pathname === "/invite") return <InvitationView />;
  if (!session) return <AuthRecoveryRoutes />;
  if (!session.emailVerified) return <VerificationRequiredView />;
  if (!session.twoFactorEnabled) return <MfaEnrollmentView />;
  if (!context) return <div className="state-msg">Indlæser din adgang…</div>;
  return <CockpitApp />;
}

function AuthRecoveryRoutes() {
  return <Routes><Route path="/invite" element={<InvitationView />} /><Route path="/forgot-password" element={<ForgotPasswordView />} /><Route path="/reset-password" element={<ResetPasswordView />} /><Route path="/verify-email" element={<VerificationRecoveryView />} /><Route path="*" element={<LoginView />} /></Routes>;
}

function CockpitApp() {
  const { hosted, context } = useAuth();
  const location = useLocation();
  const canManageWorkspace = !hosted || context?.workspaceRole === "workspace_owner";
  return (
    <div className="app-shell">
      <header className="topbar">
        <SkipLink />
        <Link className="brand" to="/">Rentemester <span className="build-version" title="Installeret Rentemester-version">v{packageJson.version}</span></Link>
        <nav className="global-navigation" aria-label="Workspace"><details><summary>Workspace</summary><div className="global-links">
          <NavLink to="/" end>
            Portefølje
          </NavLink>
          {hosted && <NavLink to="/cfo">CFO-overblik</NavLink>}
          {canManageWorkspace && <NavLink to="/companies/new">Tilføj virksomhed</NavLink>}
          {hosted && canManageWorkspace && <NavLink to="/koncernstruktur">Koncernstruktur</NavLink>}
          {hosted && canManageWorkspace && <NavLink to="/adgang">Brugere</NavLink>}
          <NavLink to="/lovgrundlag">Lovgrundlag</NavLink>
          <NavLink to="/help">Hjælp</NavLink>
        </div></details></nav>
        {hosted && !companyRouteForPath(location.pathname) && <CompanySwitcher />}
        {hosted && <AccountMenu />}
      </header>

      <CompanyNavigationShell>
        <CockpitLayout>
          <Routes>
            <Route path="/" element={<PortfolioView />} />
            {hosted && <Route path="/cfo" element={<CfoCockpitView />} />}
            <Route path="/companies/new" element={canManageWorkspace ? <AddCompanyView /> : <ErrorState message="Du har ikke adgang til at oprette virksomheder." />} />
            {hosted && canManageWorkspace && <Route path="/koncernstruktur" element={<GroupOverviewView />} />}
            {hosted && canManageWorkspace && <Route path="/adgang" element={<WorkspaceAccessView />} />}
            {COMPANY_ROUTE_DEFINITIONS.map((route) => (
              <Route
                key={route.id}
                path={companyRoutePattern(route.segment)}
                element={<CompanyRouteGate id={route.id}>{COMPANY_ROUTE_ELEMENTS[route.id]}</CompanyRouteGate>}
              />
            ))}
            <Route path="/help" element={<HelpView />} />
            <Route path="/lovgrundlag" element={<RulesView />} />
            <Route path="*" element={<NotFound />} />
          </Routes>
        </CockpitLayout>
      </CompanyNavigationShell>
    </div>
  );
}

function CompanyRouteGate({ id, children }: { id: CompanyRouteId; children: ReactElement }) {
  const { slug, documentId } = useParams();
  const [params] = useSearchParams();
  const { can } = useCapabilities();
  const permission = id === "contacts" ? "company.master-data" : id === "manage" ? "company.admin" : id === "invoice-create" ? "company.draft.write" : id === "document-booking" ? "company.ledger.post" : "company.read";
  if (!can(permission)) return <ErrorState message="Din adgang giver ikke rettighed til denne side. Vælg en anden side i menuen." />;
  // Reset local forms and manually loaded results at the resource boundary.
  // Read hooks also mask stale snapshots during render and abort old requests.
  return cloneElement(children, { key: `${id}:${slug}:${documentId ?? ""}:${params.get("year") ?? ""}` });
}

function NotFound() {
  return (
    <section className="state-msg">
      <p>Siden findes ikke.</p>
      <ButtonLink className="btn secondary" to="/">
        Til porteføljen
      </ButtonLink>
    </section>
  );
}
