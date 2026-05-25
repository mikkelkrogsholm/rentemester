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

import { NavLink, Route, Routes, Link } from "react-router-dom";
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
import { DocumentsView } from "./views/DocumentsView";
import { ArchiveView } from "./views/ArchiveView";
import { MultiYearView } from "./views/MultiYearView";
import { InvoicesView } from "./views/InvoicesView";
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

export function App() {
  return (
    <div className="app-shell">
      <header className="topbar">
        <h1>
          Rentemester <span className="brand-dot">Cockpit</span>
        </h1>
        <nav>
          <NavLink to="/" end>
            Portefølje
          </NavLink>
          <NavLink to="/companies/new">Tilføj virksomhed</NavLink>
          <NavLink to="/lovgrundlag">Lovgrundlag</NavLink>
          <NavLink to="/help">Hjælp</NavLink>
        </nav>
      </header>

      <main>
        <Routes>
          <Route path="/" element={<PortfolioView />} />
          <Route path="/companies/new" element={<AddCompanyView />} />
          <Route path="/companies/:slug" element={<DashboardView />} />
          <Route
            path="/companies/:slug/resultatopgorelse"
            element={<IncomeStatementView />}
          />
          <Route path="/companies/:slug/balance" element={<BalanceView />} />
          <Route
            path="/companies/:slug/saldobalance"
            element={<TrialBalanceView />}
          />
          <Route
            path="/companies/:slug/forpligtelser"
            element={<ObligationsView />}
          />
          <Route
            path="/companies/:slug/likviditet"
            element={<LiquidityView />}
          />
          <Route
            path="/companies/:slug/budget"
            element={<BudgetView />}
          />
          <Route
            path="/companies/:slug/posteringer"
            element={<JournalView />}
          />
          <Route path="/companies/:slug/bank" element={<BankView />} />
          <Route path="/companies/:slug/moms" element={<VatView />} />
          <Route path="/companies/:slug/bilag" element={<DocumentsView />} />
          <Route
            path="/companies/:slug/leverandoerfaktura"
            element={<PayablesView />}
          />
          <Route path="/companies/:slug/arkiv" element={<ArchiveView />} />
          <Route
            path="/companies/:slug/fleraar"
            element={<MultiYearView />}
          />
          <Route
            path="/companies/:slug/fakturaer"
            element={<InvoicesView />}
          />
          <Route
            path="/companies/:slug/faktura-skabeloner"
            element={<RecurringInvoicesView />}
          />
          <Route
            path="/companies/:slug/kontakter"
            element={<ContactsView />}
          />
          <Route
            path="/companies/:slug/koersel"
            element={<MileageView />}
          />
          <Route
            path="/companies/:slug/anlaeg"
            element={<AssetsView />}
          />
          <Route
            path="/companies/:slug/agent-forslag"
            element={<SuggestionsView />}
          />
          <Route
            path="/companies/:slug/manage"
            element={<ManageCompanyView />}
          />
          <Route path="/help" element={<HelpView />} />
          <Route path="/lovgrundlag" element={<RulesView />} />
          <Route
            path="/companies/:slug/retention"
            element={<RetentionView />}
          />
          <Route
            path="/companies/:slug/integritet"
            element={<IntegrityView />}
          />
          <Route
            path="/companies/:slug/kontoplan"
            element={<AccountsView />}
          />
          <Route path="*" element={<NotFound />} />
        </Routes>
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
