// Balance — the per-company balance sheet (cockpit-redesign iteration 2).
//
// Renders `/api/companies/:slug/balance?year=`: assets, liabilities and equity
// sections with section totals, as of the fiscal year's end date. The fiscal
// year's result is folded into the equity section as an "Årets resultat" line
// so `equity.total` is the equity an owner reads and the sheet balances
// (assets = liabilities + equity). All money fields are kroner.
//
// Sammenligningstal (#400 / ÅRL § 24): every line and every section total
// also carries the prior year's figure, so the balance fulfils the same
// regnskabskrav as resultatopgørelsen. When the ledger has no foregående
// regnskabsår, the prior column shows «—» rather than 0.

import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type { BalanceLine, CompanyBalance } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { ArchivedBanner } from "../components/ArchivedBanner";
import {
  CompanyNav,
  accountPostingsTo,
  useCompanyYear,
} from "../components/CompanyNav";

/** Render a prior-year amount cell — «—» when no prior year exists. */
function priorCell(amount: number | null, currency: string) {
  if (amount === null) return "—";
  return formatKroner(amount, currency);
}

export function BalanceView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const state = useAsync<CompanyBalance>(
    () => api.balance(slug, year),
    [slug, year],
  );

  if (state.loading && !state.data) return <Loading label="Henter balance…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const b = state.data!;
  const currency = b.company.currency || "DKK";
  const priorYear = String(parseInt(b.selectedYear, 10) - 1);

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{b.company.name}</h2>
          <p className="muted">
            {b.company.cvr ? `CVR ${b.company.cvr} · ` : ""}
            {b.company.country} · {currency} · Balance
          </p>
        </div>
        <div className="row-actions">
          {/* #372 — "Hent CSV". #463 — "Hent PDF" som ren printbar version. */}
          <a
            className="btn secondary"
            href={api.statementCsvUrl(slug, "balance", b.selectedYear)}
            download
          >
            Hent CSV
          </a>
          <a
            className="btn secondary"
            href={api.statementPdfUrl(slug, "balance", b.selectedYear)}
            download
          >
            Hent PDF
          </a>
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </div>

      <CompanyNav
        slug={slug}
        years={b.fiscalYears}
        selectedYear={b.selectedYear}
        onYearChange={setYear}
      />

      {b.archived && (
        <ArchivedBanner year={b.selectedYear} source={b.archivedSource} />
      )}
      <p className="statement-asof muted">Pr. {b.asOfDate}</p>
      <div className="card statement-card">
        <table className="data statement-table">
          <thead>
            <tr>
              <th>Konto</th>
              <th>Navn</th>
              <th className="num">Pr. {b.asOfDate}</th>
              <th className="num">Pr. {priorYear}-12-31</th>
            </tr>
          </thead>
          <BalanceSection
            heading="Aktiver"
            lines={b.assets.lines}
            total={b.assets.total}
            priorTotal={b.assets.priorTotal}
            totalLabel="Aktiver i alt"
            currency={currency}
            slug={slug}
            year={b.selectedYear}
          />
          <BalanceSection
            heading="Passiver"
            lines={b.liabilities.lines}
            total={b.liabilities.total}
            priorTotal={b.liabilities.priorTotal}
            totalLabel="Gæld i alt"
            currency={currency}
            slug={slug}
            year={b.selectedYear}
          />
          <BalanceSection
            heading="Egenkapital"
            lines={b.equity.lines}
            total={b.equity.total}
            priorTotal={b.equity.priorTotal}
            totalLabel="Egenkapital i alt"
            currency={currency}
            slug={slug}
            year={b.selectedYear}
          />
          <tbody>
            <tr className="statement-result">
              <td colSpan={2}>Passiver og egenkapital i alt</td>
              <td className="num">
                {formatKroner(b.totalLiabilitiesAndEquity, currency)}
              </td>
              <td className="num muted">
                {priorCell(b.priorTotalLiabilitiesAndEquity, currency)}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <BalanceCheck balanced={b.balanced} />
    </section>
  );
}

function BalanceSection({
  heading,
  lines,
  total,
  priorTotal,
  totalLabel,
  currency,
  slug,
  year,
}: {
  heading: string;
  lines: BalanceLine[];
  total: number;
  priorTotal: number | null;
  totalLabel: string;
  currency: string;
  slug: string;
  year: string;
}) {
  return (
    <tbody>
      <tr className="statement-section-head">
        <th colSpan={4}>{heading}</th>
      </tr>
      {lines.length === 0 ? (
        <tr>
          <td colSpan={4} className="empty-inline">
            Ingen konti.
          </td>
        </tr>
      ) : (
        lines.map((line) => (
          <tr key={line.accountNo} className="account-row">
            <td className="account-no">
              {/* A synthetic line (e.g. "Årets resultat") has no real
                  account number — render it plain rather than as a link. */}
              {line.accountNo === "—" ? (
                "—"
              ) : (
                <Link
                  className="account-link"
                  to={accountPostingsTo(slug, year, line.accountNo)}
                >
                  {line.accountNo}
                </Link>
              )}
            </td>
            <td>{line.name}</td>
            <td className="num">{formatKroner(line.amount, currency)}</td>
            <td className="num muted">
              {priorCell(line.priorAmount, currency)}
            </td>
          </tr>
        ))
      )}
      <tr className="statement-subtotal">
        <td colSpan={2}>{totalLabel}</td>
        <td className="num">{formatKroner(total, currency)}</td>
        <td className="num muted">{priorCell(priorTotal, currency)}</td>
      </tr>
    </tbody>
  );
}

function BalanceCheck({ balanced }: { balanced: boolean }) {
  return (
    <p className={`statement-check ${balanced ? "ok" : "alert"}`}>
      {balanced
        ? "Balancen stemmer — aktiver = passiver + egenkapital."
        : "Balancen stemmer ikke. Kontrollér ledgeren."}
    </p>
  );
}
