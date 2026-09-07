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

import { NavLink, Route, Routes, Link, useLocation } from "react-router-dom";
import { AuthProvider, useAuth } from "./lib/auth-context";
import { api } from "./lib/api";
import { useAsync } from "./lib/useAsync";
import { ForgotPasswordView, LoginView, ResetPasswordView, VerificationRecoveryView } from "./views/LoginView";
import { MfaEnrollmentView, VerificationRequiredView } from "./views/MfaEnrollmentView";
import { AccountMenu } from "./components/AccountMenu";
import { CompanySwitcher } from "./components/CompanySwitcher";
import packageJson from "../package.json";
import { PortfolioView } from "./views/PortfolioView";
import { AddCompanyView } from "./views/AddCompanyView";
import { HelpView } from "./views/HelpView";
import { RulesView } from "./views/RulesView";
import { GroupOverviewView } from "./views/GroupOverviewView";
import { InvitationView } from "./views/InvitationView";
import { WorkspaceAccessView } from "./views/WorkspaceAccessView";
import { CfoCockpitView } from "./views/CfoCockpitView";
import { PartyProfileView } from "./views/PartyHubView";
import {
  CompanyNavigationShell,
  CompanyTaskNavigation,
} from "./components/CompanyNav";
import {
  COMPANY_ROUTE_REGISTRY,
  COMPANY_TASK_AREAS,
  companyRoutePattern,
} from "./company-route-registry";

const COMPANY_NAVIGATION = {
  routes: COMPANY_ROUTE_REGISTRY,
  areas: COMPANY_TASK_AREAS,
};
// Detail route belongs to the Party Hub; fixed company pages remain owned by
// the route registry below.
const partyProfilePath = "/companies/:slug/parter/:partyId";

export function App() {
  const health = useAsync(() => api.health(), []);
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
  const canManageWorkspace = !hosted || context?.workspaceRole === "workspace_owner";
  return (
    <div className="app-shell">
      <header className="topbar">
        <h1>
          Rentemester <span className="brand-dot">Cockpit</span>{" "}
          <span className="build-version" title="Installeret Rentemester-version">
            v{packageJson.version}
          </span>
        </h1>
        <nav>
          <NavLink to="/" end>
            Portefølje
          </NavLink>
          {hosted && <NavLink to="/cfo">CFO-overblik</NavLink>}
          {canManageWorkspace && <NavLink to="/companies/new">Tilføj virksomhed</NavLink>}
          {hosted && canManageWorkspace && <NavLink to="/koncernstruktur">Koncernstruktur</NavLink>}
          {hosted && canManageWorkspace && <NavLink to="/adgang">Brugere</NavLink>}
          <NavLink to="/lovgrundlag">Lovgrundlag</NavLink>
          <NavLink to="/help">Hjælp</NavLink>
        </nav>
        {hosted && <CompanySwitcher />}
        {hosted && <AccountMenu />}
      </header>

      <main>
        <CompanyNavigationShell navigation={COMPANY_NAVIGATION} rendersNavigation>
          <CompanyTaskNavigation />
          <Routes>
            <Route path="/" element={<PortfolioView />} />
            {hosted && <Route path="/cfo" element={<CfoCockpitView />} />}
            <Route path="/companies/new" element={<AddCompanyView />} />
            <Route path={partyProfilePath} element={<PartyProfileView />} />
            {hosted && canManageWorkspace && <Route path="/koncernstruktur" element={<GroupOverviewView />} />}
            {hosted && canManageWorkspace && <Route path="/adgang" element={<WorkspaceAccessView />} />}
            {COMPANY_ROUTE_REGISTRY.map((route) => (
              <Route
                key={route.id}
                path={companyRoutePattern(route.segment)}
                element={route.element}
              />
            ))}
            <Route path="/help" element={<HelpView />} />
            <Route path="/lovgrundlag" element={<RulesView />} />
            <Route path="*" element={<NotFound />} />
          </Routes>
        </CompanyNavigationShell>
      </main>
    </div>
  );
}

function NotFound() {
  return (
    <section className="state-msg">
      <p>Siden findes ikke.</p>
      <Link className="btn secondary" to="/">
        Til porteføljen
      </Link>
    </section>
  );
}
