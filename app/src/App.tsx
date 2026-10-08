import { cockpitStyles } from './design/cockpit.stylex';
import { colors } from './design/tokens.stylex';
import * as stylex from '@stylexjs/stylex';
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
import { MutationMemoryProvider } from "./lib/mutation-memory";
import { useAsync } from "./lib/useAsync";
import { ForgotPasswordView, LoginView, ResetPasswordView, VerificationRecoveryView } from "./views/LoginView";
import { MfaEnrollmentView, VerificationRequiredView } from "./views/MfaEnrollmentView";
import { AccountMenu } from "./components/AccountMenu";
import { CompanySwitcher } from "./components/CompanySwitcher";
import { CockpitLayout, SkipLink } from "./components/AppChrome";
import { useCapabilities } from "./lib/useCapabilities";
import { ErrorState } from "./components/Feedback";
import packageJson from "../package.json";
import { KnowledgePageView } from "./views/KnowledgePageView";
import { TasksView, TaskDetailView } from "./views/TasksView";
import { PortfolioView } from "./views/PortfolioView";
import { AddCompanyView } from "./views/AddCompanyView";
import { HelpView } from "./views/HelpView";
import { RulesView } from "./views/RulesView";
import { GroupOverviewView } from "./views/GroupOverviewView";
import { InvitationView } from "./views/InvitationView";
import { WorkspaceAccessView } from "./views/WorkspaceAccessView";
import { CfoCockpitView } from "./views/CfoCockpitView";
import {
  CompanyNavigationShell,
} from "./components/CompanyNav";
import { cloneElement, type ReactElement } from "react";
import {
  COMPANY_ROUTE_REGISTRY,
  COMPANY_FLOW_ROUTE_REGISTRY,
  COMPANY_TASK_AREAS,
  companyRoutePattern,
  companyRouteForPath,
  type CompanyRouteId,
} from "./company-route-registry";

const COMPANY_NAVIGATION = { routes: COMPANY_ROUTE_REGISTRY, areas: COMPANY_TASK_AREAS };

export function App() {
  const health = useAsync((signal) => api.health({ signal }), []);
  const profile = health.data?.deploymentProfile;
  if (health.loading) return <div  {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Starter Rentemester…</div>;
  // This gate deliberately has no fallback. A reverse proxy error or an old
  // server must not accidentally expose a local/trusted cockpit in production.
  if (
    health.error ||
    (profile !== "local" && profile !== "local-container" && profile !== "hosted")
  ) {
    return <div  role="alert" {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Kunne ikke bekræfte Rentemesters sikkerhedsprofil. Prøv igen senere.</div>;
  }
  return <AuthProvider hosted={profile === "hosted"}><AuthGate /></AuthProvider>;
}

function AuthGate() {
  const { hosted, loading, session, context } = useAuth();
  const location = useLocation();
  if (!hosted) return <MutationMemoryProvider><CockpitApp /></MutationMemoryProvider>;
  if (loading) return <div  {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Kontrollerer din session…</div>;
  if (location.pathname === "/invite") return <InvitationView />;
  if (!session) return <AuthRecoveryRoutes />;
  if (!session.emailVerified) return <VerificationRequiredView />;
  if (!session.twoFactorEnabled) return <MfaEnrollmentView />;
  if (!context) return <div  {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Indlæser din adgang…</div>;
  return <MutationMemoryProvider key={session.id} scope={`user:${session.id}`}><CockpitApp /></MutationMemoryProvider>;
}

function AuthRecoveryRoutes() {
  return <Routes><Route path="/invite" element={<InvitationView />} /><Route path="/forgot-password" element={<ForgotPasswordView />} /><Route path="/reset-password" element={<ResetPasswordView />} /><Route path="/verify-email" element={<VerificationRecoveryView />} /><Route path="*" element={<LoginView />} /></Routes>;
}


const navigationStyles = stylex.create({
  navigation: { display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: '8px', marginLeft: 'auto' },
  link: {
    "display": 'inline-flex',
    "alignItems": 'center',
    "minHeight": 44,
    "paddingInline": '12px',
    "color": { "default": colors.inkMuted, ":hover": colors.ink },
    "textDecoration": { "default": 'none', ":hover": 'none' },
    "borderBottomWidth": 2,
    "borderBottomStyle": 'solid',
    "borderBottomColor": 'transparent',
  },
  active: { color: colors.ink, fontWeight: 600, borderBottomColor: colors.accent },
  menu: { position: 'relative' },
});
function CockpitApp() {
  const { hosted, context } = useAuth();
  const location = useLocation();
  const canManageWorkspace = !hosted || context?.workspaceRole === "workspace_owner";
  return (
    <div  {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible, cockpitStyles.appShell)}>
      <header  {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible, cockpitStyles.topbar)}>
        <SkipLink />
        <Link  to="/" {...stylex.props(cockpitStyles.topbarBrandComposition)}>Rentemester <span  title="Installeret Rentemester-version" {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible, cockpitStyles.topbarBuildVersion, cockpitStyles.brandBuildVersion)}>v{packageJson.version}</span></Link>
        <nav aria-label="Workspace" {...stylex.props(cockpitStyles.element, cockpitStyles.topbarNav, navigationStyles.navigation)}>
          <NavLink to="/" end {...stylex.props(cockpitStyles.focusVisible, navigationStyles.link, location.pathname === '/' && navigationStyles.active)}>Portefølje</NavLink>
          <NavLink to="/opgaver" {...stylex.props(cockpitStyles.focusVisible, navigationStyles.link, location.pathname.startsWith('/opgaver') && navigationStyles.active)}>Opgaver</NavLink>
          {hosted && <NavLink to="/cfo" {...stylex.props(cockpitStyles.focusVisible, navigationStyles.link, location.pathname.startsWith('/cfo') && navigationStyles.active)}>CFO-overblik</NavLink>}
          <details {...stylex.props(cockpitStyles.element, navigationStyles.menu)}><summary {...stylex.props(cockpitStyles.focusVisible, cockpitStyles.globalNavigationSummary)}>Workspace</summary><div {...stylex.props(cockpitStyles.element, cockpitStyles.globalLinks)}>
          {canManageWorkspace && <NavLink to="/companies/new" {...stylex.props(cockpitStyles.topbarNavAComposition)}>Tilføj virksomhed</NavLink>}
          {hosted && canManageWorkspace && <NavLink to="/koncernstruktur" {...stylex.props(cockpitStyles.topbarNavAComposition)}>Koncernstruktur</NavLink>}
          {hosted && canManageWorkspace && <NavLink to="/adgang" {...stylex.props(cockpitStyles.topbarNavAComposition)}>Brugere</NavLink>}
          <NavLink to="/lovgrundlag" {...stylex.props(cockpitStyles.topbarNavAComposition)}>Lovgrundlag</NavLink>
          <NavLink to="/help" {...stylex.props(cockpitStyles.topbarNavAComposition)}>Hjælp</NavLink>
        </div></details></nav>
        {hosted && !companyRouteForPath(location.pathname) && <CompanySwitcher />}
        {hosted && <AccountMenu />}
      </header>

      <CompanyNavigationShell navigation={COMPANY_NAVIGATION} rendersNavigation>
        <CockpitLayout>
          <TaskSourceReturnLink />
          <Routes>
            <Route path="/" element={<PortfolioView />} />
            <Route path="/opgaver" element={<TasksView />} />
            <Route path="/opgaver/:taskId" element={<TaskDetailView />} />
            <Route path="/viden/:pageId" element={<KnowledgePageView />} />
            {hosted && <Route path="/cfo" element={<CfoCockpitView />} />}
            <Route path="/companies/new" element={canManageWorkspace ? <AddCompanyView /> : <ErrorState message="Du har ikke adgang til at oprette virksomheder." />} />
            {hosted && canManageWorkspace && <Route path="/koncernstruktur" element={<GroupOverviewView />} />}
            {hosted && canManageWorkspace && <Route path="/adgang" element={<WorkspaceAccessView />} />}
            {COMPANY_ROUTE_REGISTRY.map((route) => (
              <Route
                key={route.id}
                path={companyRoutePattern(route.segment)}
                element={<CompanyRouteGate id={route.id}>{route.element}</CompanyRouteGate>}
              />
            ))}
            {COMPANY_FLOW_ROUTE_REGISTRY.map((route) => (
              <Route key={route.id} path={companyRoutePattern(route.segment)} element={<CompanyRouteGate id={route.id}>{route.element}</CompanyRouteGate>} />
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
  const { slug, documentId, partyId } = useParams();
  const [params] = useSearchParams();
  const { can } = useCapabilities(slug);
  const permission = id === "manage" ? "company.admin" : id === "invoice-create" ? "company.draft.write" : id === "document-booking" ? "company.ledger.post" : "company.read";
  if (!can(permission)) return <ErrorState message="Din adgang giver ikke rettighed til denne side. Vælg en anden side i menuen." />;
  // Reset local forms and manually loaded results at the resource boundary.
  // Read hooks also mask stale snapshots during render and abort old requests.
  return cloneElement(children, { key: `${id}:${slug}:${documentId ?? partyId ?? ""}:${params.get("year") ?? ""}` });
}

function NotFound() {
  return (
    <section  {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
      <p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Siden findes ikke.</p>
      <ButtonLink  to="/" variant={"secondary"} xstyle={[cockpitStyles.aComposition]}>
        Til porteføljen
      </ButtonLink>
    </section>
  );
}

function TaskSourceReturnLink() {
  const [params] = useSearchParams();
  const raw = params.get("returnTo");
  if (!raw || /[\\\r\n]/.test(raw)) return null;
  try {
    const url = new URL(raw, "https://rentemester.invalid");
    if (url.origin !== "https://rentemester.invalid" || !/^\/opgaver\/[^/]+$/.test(url.pathname)) return null;
    return <ButtonLink variant="secondary" to={`${url.pathname}${url.search}`} xstyle={[cockpitStyles.aComposition]}>Tilbage til opgaven</ButtonLink>;
  } catch { return null; }
}
