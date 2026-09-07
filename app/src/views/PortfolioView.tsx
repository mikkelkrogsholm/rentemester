// Portfolio overview — the workspace-level landing page.
//
// A cross-company roll-up strip answers "how is the whole portfolio doing",
// and one card per company shows the headline health an owner judges a
// company on. Companies that need attention sort to the top and are flagged.

import { useEffect } from "react";
import { Link, useNavigate } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner, sortByAttention } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import { CompanyCard } from "../components/CompanyCard";
import { ErrorState, Loading } from "../components/Feedback";
import { Onboarding } from "./Onboarding";

export function PortfolioView() {
  const navigate = useNavigate();
  const state = useAsync(() => api.portfolio(), []);
  useEffect(() => {
    if (state.data?.companies.length === 1) {
      navigate(`/companies/${state.data.companies[0]!.slug}`, { replace: true });
    }
  }, [navigate, state.data]);

  if (state.loading) return <Loading label="Henter portefølje…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const portfolio = state.data!;

  // The server has already filtered this list to the authenticated member's
  // visible companies. Do not reconstruct membership in the browser.
  // First run: an empty workspace drops straight into onboarding.
  if (portfolio.companies.length === 0) {
    return (
      <Onboarding onCreated={(slug) => navigate(`/companies/${slug}`)} />
    );
  }

  if (portfolio.companies.length === 1) return <Loading label="Åbner virksomhedsoverblik…" />;

  const ordered = sortByAttention(portfolio.companies);
  const needAttention = ordered.filter(
    (c) =>
      !c.archived &&
      (c.ledgerMissing ||
        !c.auditChainOk ||
        c.resultat < 0 ||
        c.openTaskCount > 0 ||
        (c.vat !== null &&
          c.vat.payable > 0 &&
          c.vat.daysRemaining <= 30)),
  ).length;

  const { rollup } = portfolio;

  return (
    <section>
      <div className="page-head">
        <div>
          <h2>Portefølje</h2>
          <p className="muted">
            {portfolio.companyCount} virksomhed
            {portfolio.companyCount === 1 ? "" : "er"} · {needAttention} kræver
            opmærksomhed · pr. {portfolio.asOf}
          </p>
        </div>
        <Link className="btn" to="/companies/new">
          Tilføj virksomhed
        </Link>
      </div>

      {rollup && (
        <div className="rollup-strip" role="group" aria-label="Tværgående overblik">
          <div
            className={`rollup-cell ${rollup.resultat < 0 ? "neg" : "pos"}`}
          >
            <span className="rollup-label">Samlet resultat</span>
            <span className="rollup-value">
              {formatKroner(rollup.resultat)}
            </span>
          </div>
          <div className="rollup-cell">
            <span className="rollup-label">Samlet likviditet</span>
            <span className="rollup-value">
              {rollup.liquidity === null ? "—" : formatKroner(rollup.liquidity)}
            </span>
            {!rollup.liquidityComplete && <span className="rollup-note">Ufuldstændig — kontrollér Bank</span>}
          </div>
          <div className="rollup-cell">
            <span className="rollup-label">Moms at betale</span>
            <span className="rollup-value">
              {formatKroner(rollup.vatPayable)}
            </span>
          </div>
          <div
            className={`rollup-cell ${
              rollup.openTaskCount > 0 ? "warn" : ""
            }`}
          >
            <span className="rollup-label">Åbne opgaver</span>
            <span className="rollup-value">{rollup.openTaskCount}</span>
          </div>
        </div>
      )}

      <div className="company-grid">
        {ordered.map((c) => (
          <CompanyCard key={c.slug} company={c} />
        ))}
      </div>
    </section>
  );
}
