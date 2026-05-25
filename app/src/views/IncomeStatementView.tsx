// Resultatopgørelse — the per-company income statement (cockpit-redesign it. 2).
//
// Renders `/api/companies/:slug/income-statement?year=`: income accounts and
// expense accounts grouped under section headings, each with a prior-year
// comparison column, and a clear result figure at the bottom. All money fields
// are kroner — `formatKroner` is used throughout.

import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type { CompanyIncomeStatement, IncomeStatementLine } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { ArchivedBanner } from "../components/ArchivedBanner";
import {
  CompanyNav,
  accountPostingsTo,
  useCompanyYear,
} from "../components/CompanyNav";

export function IncomeStatementView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const state = useAsync<CompanyIncomeStatement>(
    () => api.incomeStatement(slug, year),
    [slug, year],
  );

  if (state.loading && !state.data)
    return <Loading label="Henter resultatopgørelse…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const s = state.data!;
  const currency = s.company.currency || "DKK";
  const priorYear = String(parseInt(s.selectedYear, 10) - 1);
  const positive = s.result >= 0;

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{s.company.name}</h2>
          <p className="muted">
            {s.company.cvr ? `CVR ${s.company.cvr} · ` : ""}
            {s.company.country} · {currency} · Resultatopgørelse
          </p>
        </div>
        <div className="row-actions">
          {/* #372 — CSV-eksport til Excel/Numbers/Sheets.
              #463 — PDF-eksport, ren printbar uden cockpit-chrome. */}
          <a
            className="btn secondary"
            href={api.statementCsvUrl(slug, "income-statement", s.selectedYear)}
            download
          >
            Hent CSV
          </a>
          <a
            className="btn secondary"
            href={api.statementPdfUrl(slug, "income-statement", s.selectedYear)}
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
        years={s.fiscalYears}
        selectedYear={s.selectedYear}
        onYearChange={setYear}
      />

      {s.archived && (
        <ArchivedBanner year={s.selectedYear} source={s.archivedSource} />
      )}
      <div className="card statement-card">
        <table className="data statement-table">
          <thead>
            <tr>
              <th>Konto</th>
              <th>Navn</th>
              <th className="num">{s.selectedYear}</th>
              <th className="num">{priorYear}</th>
            </tr>
          </thead>
          <StatementSection
            heading="Indtægter"
            lines={s.income}
            total={s.totalIncome}
            priorTotal={s.priorTotalIncome}
            totalLabel="Indtægter i alt"
            currency={currency}
            slug={slug}
            year={s.selectedYear}
          />
          <StatementSection
            heading="Udgifter"
            lines={s.expense}
            total={s.totalExpense}
            priorTotal={s.priorTotalExpense}
            totalLabel="Udgifter i alt"
            currency={currency}
            slug={slug}
            year={s.selectedYear}
          />
          <tbody>
            <tr className={`statement-result ${positive ? "positive" : "negative"}`}>
              <td colSpan={2}>Årets resultat</td>
              <td className="num">{formatKroner(s.result, currency)}</td>
              <td className="num">
                {formatKroner(s.priorResult, currency)}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
  );
}

function StatementSection({
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
  lines: IncomeStatementLine[];
  total: number;
  priorTotal: number;
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
            Ingen posteringer i året.
          </td>
        </tr>
      ) : (
        lines.map((line) => (
          <tr key={line.accountNo} className="account-row">
            <td className="account-no">
              <Link
                className="account-link"
                to={accountPostingsTo(slug, year, line.accountNo)}
              >
                {line.accountNo}
              </Link>
            </td>
            <td>{line.name}</td>
            <td className="num">{formatKroner(line.amount, currency)}</td>
            <td className="num muted">
              {formatKroner(line.priorAmount, currency)}
            </td>
          </tr>
        ))
      )}
      <tr className="statement-subtotal">
        <td colSpan={2}>{totalLabel}</td>
        <td className="num">{formatKroner(total, currency)}</td>
        <td className="num muted">{formatKroner(priorTotal, currency)}</td>
      </tr>
    </tbody>
  );
}
