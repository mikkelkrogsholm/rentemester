import { ReportSourceNotice } from "../components/ReportSourceNotice";
import { ButtonLink, PageHeader } from "../components/ui";
// Resultatopgørelse — the per-company income statement (cockpit-redesign it. 2).
//
// Renders `/api/companies/:slug/income-statement?year=`: income accounts and
// expense accounts grouped under section headings, each with a prior-year
// comparison column, and a clear result figure at the bottom. All money fields
// are kroner — `formatKroner` is used throughout.

import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner, moneyNegative } from "../lib/format";
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
  const { year, source, setYear } = useCompanyYear();
  const state = useAsync<CompanyIncomeStatement>(
    (signal) => api.incomeStatement(slug, year, { signal, ...(source ? {source} : {}) }),
    [slug, year, source],
  );

  if (state.loading && !state.data)
    return <Loading label="Henter resultatopgørelse…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const s = state.data!;
  const currency = s.declaredCurrency ?? s.company.currency ?? "DKK";
  const selectedSource = s.selectedSource ?? source;
  const priorYear = s.priorYearLabel === null ? "Sammenligning ikke tilgængelig" : s.priorYearLabel ?? String(parseInt(s.selectedYear, 10) - 1);
  const positive = !moneyNegative(s.result);

  return (
    <section className="statement">
      <PageHeader title="Resultatopgørelse" actions={<><div className="row-actions">
          {/* #372 — CSV-eksport til Excel/Numbers/Sheets.
              #463 — PDF-eksport, ren printbar uden cockpit-chrome. */}
          {s.completeness && s.completeness.status !== "ready" ? <p>CSV og PDF kræver et fuldstændigt rapportgrundlag.</p> : <>
          <a
            className="btn secondary"
            href={api.statementCsvUrl(slug, "income-statement", s.selectedYear, selectedSource)}
            download
          >
            Hent CSV
          </a>
          <a
            className="btn secondary"
            href={api.statementPdfUrl(slug, "income-statement", s.selectedYear, selectedSource)}
            download
          >
            Hent PDF
          </a>
          </>}
          <ButtonLink className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div>

          <p className="muted">
            {s.company.cvr ? `CVR ${s.company.cvr} · ` : ""}
            {s.company.country} · {currency} · Resultatopgørelse
          </p>
        </div>

      </PageHeader>

      <CompanyNav
        slug={slug}
        years={s.fiscalYears}
        selectedYear={s.selectedYear}
        selectedSource={selectedSource}
        onYearChange={setYear}
      />

      <ReportSourceNotice report={s} slug={slug}/>
      {s.archived && (
        <ArchivedBanner year={s.selectedYear} source={s.archivedSource} />
      )}
      <div className="card statement-card">
        <div className="table-scroll"><table className="data statement-table">
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
            source={selectedSource}
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
            source={selectedSource}
          />
          <tbody>
            <tr className={`statement-result ${s.result === null ? "" : positive ? "positive" : "negative"}`}>
              <td colSpan={2}>Årets resultat</td>
              <td className="num">{formatKroner(s.result, currency)}</td>
              <td className="num">
                {formatKroner(s.priorResult, currency)}
              </td>
            </tr>
          </tbody>
        </table></div>
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
  source,
}: {
  heading: string;
  lines: IncomeStatementLine[];
  total: import("../lib/types/common").ReportMoney;
  priorTotal: import("../lib/types/common").ReportMoney;
  totalLabel: string;
  currency: string;
  slug: string;
  year: string;
  source?: "live" | "archive";
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
                to={accountPostingsTo(slug, year, line.accountNo, source)}
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
