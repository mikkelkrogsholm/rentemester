import * as stylex from "@stylexjs/stylex";
import { ReportSourceNotice } from "../components/ReportSourceNotice";
import { ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Saldobalance — the per-company trial balance (cockpit-redesign iteration 2).
//
// Renders `/api/companies/:slug/trial-balance?year=`: a table of every account
// that moved in the year, with its summed debit total, credit total and the
// signed net balance. A totals row closes the table; the report is balanced
// when total debit equals total credit. All money fields are kroner.
import { Link, useParams } from "react-router-dom";
import { ArchivedBanner } from "../components/ArchivedBanner";
import { accountPostingsTo, CompanyNav, useCompanyYear, } from "../components/CompanyNav";
import { ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import type { CompanyTrialBalance } from "../lib/types";
import { useAsync } from "../lib/useAsync";
export function TrialBalanceView() {
    const { slug = "" } = useParams();
    const { year, source, setYear } = useCompanyYear();
    const state = useAsync<CompanyTrialBalance>((signal) => api.trialBalance(slug, year, { signal, ...(source ? { source } : {}) }), [slug, year, source]);
    if (state.loading && !state.data)
        return <Loading label="Henter saldobalance…"/>;
    if (state.error)
        return <ErrorState message={state.error} onRetry={state.reload}/>;
    const t = state.data!;
    const currency = t.declaredCurrency ?? t.company.currency ?? "DKK";
    const selectedSource = t.selectedSource ?? source;
    return (<section>
      <PageHeader title="Saldobalance" actions={<><div {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rowActions,
							)}>
          {/* #372 — "Hent CSV". #463 — "Hent PDF" som printbar version. */}
          {t.completeness && t.completeness.status !== "ready" ? <p>CSV og PDF kræver et fuldstændigt rapportgrundlag.</p> : <>
          <a href={api.statementCsvUrl(slug, "trial-balance", t.selectedYear, selectedSource)} download>
            Hent CSV
          </a>
          <a href={api.statementPdfUrl(slug, "trial-balance", t.selectedYear, selectedSource)} download>
            Hent PDF
          </a>
          </>}
          <ButtonLink to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>

          <p {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}>
            {t.company.cvr ? `CVR ${t.company.cvr} · ` : ""}
            {t.company.country} · {currency} · Saldobalance
          </p>
        </div>

      </PageHeader>

      <CompanyNav slug={slug} years={t.fiscalYears} selectedYear={t.selectedYear} selectedSource={selectedSource} onYearChange={setYear}/>

      <ReportSourceNotice report={t} slug={slug}/>
      {t.archived && (<ArchivedBanner year={t.selectedYear} source={t.archivedSource}/>)}
      <p {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
					cockpitStyles.statementAsof,
				)}>
        {t.periodStart && t.periodEnd ? `${t.periodStart} – ${t.periodEnd}` : "Arkiveret afslutningssnapshot — dato ikke oplyst"}{t.archived && " · Afslutningssaldi, ikke årets debet-/kreditomsætning"}
      </p>
      <div>
        <table {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.tableData,
						cockpitStyles.tableStatementTable,
						cockpitStyles.statementTableScrollTable,
					)}>
          <thead {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
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
              <th {...stylex.props(
									cockpitStyles.statementTableScrollTableThComposition2,
								)}>Debet</th>
              <th {...stylex.props(
									cockpitStyles.statementTableScrollTableThComposition2,
								)}>Kredit</th>
              <th {...stylex.props(
									cockpitStyles.statementTableScrollTableThComposition2,
								)}>Saldo</th>
            </tr>
          </thead>
          <tbody {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            {t.rows.length === 0 ? (<tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
                <td colSpan={5} {...stylex.props(
										cockpitStyles.statementTableScrollTableTdComposition2,
									)}>
                  {t.completeness?.status === "unavailable" ? "Afslutningssaldi ikke tilgængelige." : "Ingen posteringer i året."}
                </td>
              </tr>) : (t.rows.map((row) => (<tr key={row.accountNo} {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.trAccountRowHover,
									)}>
                  <td {...stylex.props(
											cockpitStyles.tableStatementTableTdAccountNoComposition,
										)}>
                    <Link to={accountPostingsTo(slug, t.selectedYear, row.accountNo, selectedSource)}>
                      {row.accountNo}
                    </Link>
                  </td>
                  <td {...stylex.props(
											cockpitStyles.statementTableScrollTableTdComposition,
										)}>{row.name}{row.sourceRows && <details><summary>Oprindelige saldi for konto {row.accountNo}</summary>{row.sourceRows.map(source => <p key={source.lineNo}>Række {source.lineNo}: {source.sourceAmount} · rapportbeløb {formatKroner(source.amount, currency)}</p>)}</details>}</td>
                  <td {...stylex.props(cockpitStyles.tableDataTdNumComposition)}>
                    {formatKroner(row.debit, currency)}
                  </td>
                  <td {...stylex.props(cockpitStyles.tableDataTdNumComposition)}>
                    {formatKroner(row.credit, currency)}
                  </td>
                  <td {...stylex.props(cockpitStyles.tableDataTdNumComposition)}>
                    {formatKroner(row.balance, currency)}
                  </td>
                </tr>)))}
          </tbody>
          <tfoot {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <tr {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
              <td colSpan={2} {...stylex.props(
									cockpitStyles.statementTableScrollTableTdComposition3,
								)}>I alt</td>
              <td {...stylex.props(cockpitStyles.tableDataTdNumComposition2)}>
                {formatKroner(t.totalDebit, currency)}
              </td>
              <td {...stylex.props(cockpitStyles.tableDataTdNumComposition2)}>
                {formatKroner(t.totalCredit, currency)}
              </td>
              <td {...stylex.props(cockpitStyles.tableDataTdNumComposition2)} />
            </tr>
          </tfoot>
        </table>
      </div>
      <p {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.statementCheck,
					t.balanced && cockpitStyles.statementCheckOk,
					!t.balanced && cockpitStyles.statementCheckAlert,
				)}>
        {t.balanced === null ? "Saldobalancens kontrol kan ikke opgøres med det tilgængelige grundlag." : t.balanced
            ? "Saldobalancen stemmer — debet = kredit."
            : "Saldobalancen stemmer ikke. Kontrollér ledgeren."}
			</p>
		</section>);
}
