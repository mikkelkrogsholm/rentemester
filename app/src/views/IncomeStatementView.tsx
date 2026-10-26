import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
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
    <section {...stylex.props(cockpitStyles.statement)}>
      <PageHeader title="Resultatopgørelse" actions={<><div {...stylex.props(cockpitStyles.rowActions)}>
          {/* #372 — CSV-eksport til Excel/Numbers/Sheets.
              #463 — PDF-eksport, ren printbar uden cockpit-chrome. */}
          {s.completeness && s.completeness.status !== "ready" ? <p>CSV og PDF kræver et fuldstændigt rapportgrundlag.</p> : <>
          <a
            {...stylex.props(...[cockpitStyles.buttonComposition, cockpitStyles.element])}
            href={api.statementCsvUrl(slug, "income-statement", s.selectedYear, selectedSource)}
            download
          >
            Hent CSV
          </a>
          <a
            {...stylex.props(...[cockpitStyles.buttonComposition, cockpitStyles.element])}
            href={api.statementPdfUrl(slug, "income-statement", s.selectedYear, selectedSource)}
            download
          >
            Hent PDF
          </a>
          </>}
          <ButtonLink  to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>

          <p {...stylex.props(cockpitStyles.muted)}>
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
      <div {...stylex.props(cockpitStyles.card, cockpitStyles.statementCard)}>
        <div {...stylex.props(cockpitStyles.tableScroll)}><table {...stylex.props(...[cockpitStyles.tableData, cockpitStyles.tableStatementTable])}>
          <thead {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
            <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition,
									)}>Konto</th>
              <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition,
									)}>Navn</th>
              <th {...stylex.props(cockpitStyles.num)}>{s.selectedYear}</th>
              <th {...stylex.props(cockpitStyles.num)}>{priorYear}</th>
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
          <tbody {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
            <tr {...stylex.props(...[cockpitStyles.statementResultTd, ...(s.result === null ? [] : (positive ? [cockpitStyles.amountPositive] : [cockpitStyles.amountNegative]))])}>
              <td colSpan={2} {...stylex.props(
										cockpitStyles.statementTableScrollTableTdComposition10,
									)}>Årets resultat</td>
              <td {...stylex.props(cockpitStyles.num)}>{formatKroner(s.result, currency)}</td>
              <td {...stylex.props(cockpitStyles.num)}>
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
    <tbody {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
      <tr {...stylex.props(...[cockpitStyles.statementSubhead])}>
        <th colSpan={4} {...stylex.props(cockpitStyles.statementSectionHeadThComposition)}>{heading}</th>
      </tr>
      {lines.length === 0 ? (
        <tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
          <td colSpan={4} {...stylex.props(cockpitStyles.emptyInline)}>
            Ingen posteringer i året.
          </td>
        </tr>
      ) : (
        lines.map((line) => (
          <tr key={line.accountNo} {...stylex.props(...[cockpitStyles.trAccountRowHover])}>
            <td {...stylex.props(...[cockpitStyles.num])}>
              <Link
                {...stylex.props(...[cockpitStyles.aComposition])}
                to={accountPostingsTo(slug, year, line.accountNo, source)}
              >
                {line.accountNo}
              </Link>
            </td>
            <td {...stylex.props(cockpitStyles.tableStatementTableTdComposition4)}>{line.name}</td>
            <td {...stylex.props(cockpitStyles.num)}>{formatKroner(line.amount, currency)}</td>
            <td {...stylex.props(cockpitStyles.num, cockpitStyles.muted)}>
              {formatKroner(line.priorAmount, currency)}
            </td>
          </tr>
        ))
      )}
      <tr {...stylex.props(...[cockpitStyles.statementSubtotalTd])}>
        <td colSpan={2} {...stylex.props(cockpitStyles.statementSubtotalTdComposition)}>{totalLabel}</td>
        <td {...stylex.props(cockpitStyles.num)}>{formatKroner(total, currency)}</td>
        <td {...stylex.props(cockpitStyles.num, cockpitStyles.muted)}>{formatKroner(priorTotal, currency)}</td>
      </tr>
    </tbody>
  );
}
