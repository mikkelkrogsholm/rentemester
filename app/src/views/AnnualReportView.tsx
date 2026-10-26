import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { ButtonLink, Button, Input, PageHeader } from "../components/ui";
// Årsrapport-builder UI (#338).
//
// Per-virksomhed view der bygger en regnskabsklasse-B-årsrapport for et
// regnskabsår. Brugeren angiver start + slut; resten kommer fra kernens
// `buildAnnualReport` (samme funktion som CLI'ens `report annual`).
// Forudsætnings-fejl (CVR mangler, periode er ikke låst, bøgerne
// balancerer ikke) vises tydeligt så ejeren kan rette dem og prøve igen.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import { formatKroner } from "../lib/format";
import type {
  AnnualReport,
  CompanyAnnualReportResponse,
} from "../lib/types";

export function AnnualReportView() {
  const { slug = "" } = useParams();
  const today = new Date();
  const defaultYear = today.getUTCFullYear() - 1;
  const [fiscalYearStart, setFiscalYearStart] = useState(`${defaultYear}-01-01`);
  const [fiscalYearEnd, setFiscalYearEnd] = useState(`${defaultYear}-12-31`);
  const [data, setData] = useState<CompanyAnnualReportResponse["annualReport"] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const build = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setLoading(true);
    try {
      const result = await api.annualReport(slug, fiscalYearStart, fiscalYearEnd);
      setData(result);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Kunne ikke bygge årsrapport.");
      setData(null);
    } finally {
      setLoading(false);
    }
  };

  return (
    <section {...stylex.props(...[cockpitStyles.statement])}>
      <PageHeader title="Årsrapport" actions={<><div {...stylex.props(cockpitStyles.rowActions)}>
          <ButtonLink  to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>

          <p {...stylex.props(cockpitStyles.muted)}>
            Forbereder en årsrapport for et lukket regnskabsår. Rentemester
            samler resultatopgørelse, balance og noter. Ejer eller revisor
            gennemgår materialet og er ansvarlig for indberetning.
          </p>
        </div>

      </PageHeader>

      <section {...stylex.props(cockpitStyles.card)}>
        <h3 {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}>Vælg regnskabsår</h3>
        <form onSubmit={build} {...stylex.props(cockpitStyles.filterBar)}>
          <label>
            Start (YYYY-MM-DD)
            <Input
              type="date"
              value={fiscalYearStart}
              onChange={(e) => setFiscalYearStart(e.target.value)}
              required
            />
          </label>
          <label>
            Slut (YYYY-MM-DD)
            <Input
              type="date"
              value={fiscalYearEnd}
              onChange={(e) => setFiscalYearEnd(e.target.value)}
              required
            />
          </label>
          <Button type="submit"  disabled={loading}>
            {loading ? "Bygger …" : "Byg årsrapport"}
          </Button>
        </form>
      </section>

      {error && (
        <div {...stylex.props(...[cockpitStyles.banner, cockpitStyles.bannerError])} role="alert">
          {error}
        </div>
      )}

      {data && <ReportPanel report={data.report} />}
    </section>
  );
}

function ReportPanel({ report }: { report: AnnualReport }) {
  if (!report.ok) {
    return (
      <section {...stylex.props(cockpitStyles.card)}>
        <h3 {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}>Forudsætninger mangler</h3>
        <p {...stylex.props(cockpitStyles.muted)}>
          For at en årsrapport kan dannes skal: virksomhedens CVR være
          registreret under <Link to="../manage">Administrér</Link>, regnskabsåret være lukket under{" "}
          <Link to="../periodelas">Periodelås</Link>, og bøgerne skal balancere.
        </p>
        <ul {...stylex.props(...[cockpitStyles.bannerError])}>
          {report.errors.map((err, i) => (
            <li key={i} {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>{err}</li>
          ))}
        </ul>
      </section>
    );
  }
  const currency = report.company.currency || "DKK";
  return (
    <>
      <section {...stylex.props(cockpitStyles.card)}>
        <h3 {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}>
          Stamdata pr. {report.fiscalYearStart} — {report.fiscalYearEnd}
        </h3>
        <div {...stylex.props(cockpitStyles.tableScroll)}><table {...stylex.props(...[cockpitStyles.tableData])}>
          <tbody {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
            <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <th {...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>Virksomhed</th>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{report.company.name}</td>
            </tr>
            <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <th {...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>CVR</th>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{report.company.cvr ?? "—"}</td>
            </tr>
            <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <th {...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>Land</th>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{report.company.country}</td>
            </tr>
            <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <th {...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>Valuta</th>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{report.company.currency}</td>
            </tr>
          </tbody>
        </table></div>
      </section>

      {report.profitAndLoss && (
        <section {...stylex.props(cockpitStyles.card)}>
          <h3 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
						)}>Resultatopgørelse</h3>
          <div {...stylex.props(cockpitStyles.tableScroll)}><table {...stylex.props(...[cockpitStyles.tableData])}>
            <tbody>
              <tr>
                <th>Indtægter i alt</th>
                <td {...stylex.props(cockpitStyles.num)}>
                  {formatKroner(report.profitAndLoss.income.total, currency)}
                </td>
              </tr>
              <tr>
                <th>Omkostninger i alt</th>
                <td {...stylex.props(cockpitStyles.num)}>
                  {formatKroner(report.profitAndLoss.expense.total, currency)}
                </td>
              </tr>
              <tr {...stylex.props(...[cockpitStyles.statementResultTd])}>
                <th>Årets resultat</th>
                <td {...stylex.props(cockpitStyles.num)}>
                  {formatKroner(report.profitAndLoss.result, currency)}
                </td>
              </tr>
            </tbody>
          </table></div>
        </section>
      )}

      {report.balanceSheet && (
        <section {...stylex.props(cockpitStyles.card)}>
          <h3>Balance ultimo {report.fiscalYearEnd}</h3>
          <div {...stylex.props(cockpitStyles.tableScroll)}><table {...stylex.props(...[cockpitStyles.tableData])}>
            <tbody {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <tr {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>
                <th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Aktiver i alt</th>
                <td {...stylex.props(cockpitStyles.num)}>
                  {formatKroner(report.balanceSheet.assets.total, currency)}
                </td>
              </tr>
              <tr {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>
                <th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Gæld i alt</th>
                <td {...stylex.props(cockpitStyles.num)}>
                  {formatKroner(
                    report.balanceSheet.liabilities.total,
                    currency,
                  )}
                </td>
              </tr>
              <tr {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>
                <th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Egenkapital i alt</th>
                <td {...stylex.props(cockpitStyles.num)}>
                  {formatKroner(report.balanceSheet.equity.total, currency)}
                </td>
              </tr>
            </tbody>
          </table></div>
        </section>
      )}

      {report.comparison && (
        <section {...stylex.props(cockpitStyles.card)}>
          <h3 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
						)}>Sammenligningstal {report.comparison.fiscalYearStart} — {report.comparison.fiscalYearEnd}</h3>
          {!report.comparison.available ? <p {...stylex.props(cockpitStyles.muted)}>{report.comparison.reason === "prior_range_precedes_explicit_cutover"
            ? "Sammenligningstal er ikke tilgængelige: den foregående periode starter før den eksplicitte overtagelsesdato. Arkivbeløb indgår ikke automatisk."
            : "Ingen sammenligningstal — ingen saldi i den foregående periode."}</p> : (
            <div {...stylex.props(cockpitStyles.tableScroll)}><table {...stylex.props(...[cockpitStyles.tableData])}><tbody {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <tr {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}><th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Indtægter i alt</th><td {...stylex.props(cockpitStyles.num)}>{formatKroner(report.comparison.totalIncome, currency)}</td></tr>
              <tr {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}><th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Omkostninger i alt</th><td {...stylex.props(cockpitStyles.num)}>{formatKroner(report.comparison.totalExpense, currency)}</td></tr>
              <tr {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}><th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Årets resultat</th><td {...stylex.props(cockpitStyles.num)}>{formatKroner(report.comparison.aretsResultat, currency)}</td></tr>
              <tr><th>Aktiver i alt</th><td {...stylex.props(cockpitStyles.num)}>{formatKroner(report.comparison.totalAssets, currency)}</td></tr>
              <tr><th>Egenkapital i alt</th><td {...stylex.props(cockpitStyles.num)}>{formatKroner(report.comparison.equity, currency)}</td></tr>
            </tbody></table></div>
          )}
        </section>
      )}
      {report.disclaimer && <p {...stylex.props(...[cockpitStyles.banner])}>{report.disclaimer}</p>}

      {report.notes && report.notes.length > 0 && (
        <section {...stylex.props(cockpitStyles.card)}>
          <h3 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
						)}>Noter</h3>
          <ul {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            {report.notes.map((n) => (
              <li key={n.id} {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
                <strong {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{n.title}</strong>
                <p {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>{n.body}</p>
                {n.placeholder && <span {...stylex.props(cockpitStyles.muted)}>Udfyldes og gennemgås af ejer eller revisor.</span>}
              </li>
            ))}
          </ul>
        </section>
      )}

      {report.ledelsespategning && (
        <section {...stylex.props(cockpitStyles.card)}>
          <h3 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
						)}>Ledelsespåtegning</h3>
          {report.ledelsespategning.placeholder && <p {...stylex.props(cockpitStyles.muted)}>Skabelon — udfyld dato, sted og underskrifter inden indberetning.</p>}
          <p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>{report.ledelsespategning.text}</p>
        </section>
      )}
    </>
  );
}
