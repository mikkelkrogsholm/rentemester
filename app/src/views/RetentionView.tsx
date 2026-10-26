import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { Button, ButtonLink, PageHeader } from "../components/ui";
// Retention status view (#343) — per-virksomhed read-only side der viser
// hvor langt 5-års bogføringspligten er kommet for hver data-domæne. Bygger
// på det eksisterende `buildRetentionStatusReport` i kernen (ingen
// genimplementering) og deep-linker til Lovgrundlag-viewet (#347) for at
// citere bogføringslovens § 12, stk. 1.
import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import type { CompanyRetention } from "../lib/types";
import { useAsync } from "../lib/useAsync";
const TABLE_LABELS: Record<string, string> = {
    documents: "Bilag",
    journal_entries: "Posteringer",
    bank_transactions: "Banktransaktioner",
};
export function RetentionView() {
    const { slug = "" } = useParams();
    const [reportError, setReportError] = useState<string | null>(null);
    const [reportBusy, setReportBusy] = useState(false);
    const state = useAsync<CompanyRetention>((signal) => api.retention(slug, { signal }), [slug]);
    if (state.loading)
        return <Loading />;
    if (state.error)
        return <ErrorState message={state.error}/>;
    const r = state.data!;
    const totalExpired = r.report.rows.reduce((acc, row) => acc + row.expired, 0);
    return (<section data-cockpit-page="retention" data-evidence-issue="655" {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
			<PageHeader title="Opbevaring" actions={<>
						<div {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rowActions,
							)}>
							<ButtonLink to={`/companies/${slug}/manage`} variant={"secondary"} xstyle={[cockpitStyles.retentionViewBtnComposition]}>
								Administrér
							</ButtonLink>
						</div>
					</>}>
				<div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					<p {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}>
						{r.company.cvr ? `CVR ${r.company.cvr} · ` : ""}
						{r.company.country} · Retention (5-års bogføringspligt)
					</p>
				</div>
			</PageHeader>

			<p {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}>
				Overblik over, hvornår personoplysninger fortsat skal opbevares sammen
				med bogføringsmaterialet.
			</p>

      <p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
        Pr. {r.report.asOf}. Hver data-domæne skal opbevares i 5 år efter udløb
        af det regnskabsår posten vedrører. Tabellen viser hvor mange poster der
        har passeret opbevaringsfristen (kan gennemgås til{" "}
        <Link to={`/companies/${slug}/gdpr`} {...stylex.props(cockpitStyles.aComposition)}>GDPR-anonymisering</Link>) og
        hvornår næste post udløber.
      </p>

      {totalExpired > 0 && (<div>
          {totalExpired} post{totalExpired === 1 ? "" : "er"} har overskredet
          den 5-årige opbevaringspligt og kan gennemgås til anonymisering. Journaltekst og beskyttede originaler bevares.
        </div>)}

      {r.report.rows.some(row => (row.unknown ?? 0) > 0) && <p role="alert">Nogle opbevaringsfrister kan ikke fastslås. De poster kan ikke anonymiseres, før grundlaget er afklaret.</p>}
      <div><table {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.retentionViewTableTable,
					)}>
        <thead {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
          <tr {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
            <th {...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.retentionViewTableTableThLastChild,
								)}>Domæne</th>
            <th {...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.retentionViewTableTableThLastChild,
								)}>Antal poster</th>
            <th {...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.retentionViewTableTableThLastChild,
								)}>Udløbet</th>
            <th {...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.retentionViewTableTableThLastChild,
								)}>Næste udløb</th>
            <th {...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.retentionViewTableTableThLastChild,
								)}>Ældste udløbet</th>
          </tr>
        </thead>
        <tbody {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
          {r.report.rows.map((row) => (<tr key={row.table} {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.retentionViewTableTableTdLastChild,
									)}>{TABLE_LABELS[row.table] ?? row.table}</td>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.retentionViewTableTableTdLastChild,
									)}>{row.total}</td>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.retentionViewTableTableTdLastChild,
									)}>{row.expired}</td>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.retentionViewTableTableTdLastChild,
									)}>{row.nextExpiry ?? "—"}</td>
              <td {...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.retentionViewTableTableTdLastChild,
									)}>{row.oldestExpired ?? "—"}</td>
            </tr>))}
        </tbody>
      </table></div>

      <section {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}>
        <h3 {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}>Virksomhedens compliance-rapport</h3>
        <p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Autoriseret HTML fra serveren med faktiske og utilgængelige evidensfelter. Kildehenvisninger er ikke juridisk godkendelse.</p>
        <Button requiredPermission="company.read" disabled={reportBusy} onClick={async () => {
            setReportBusy(true);
            setReportError(null);
            try {
                const report = await api.complianceReport(slug);
                const url = URL.createObjectURL(new Blob([report.html], { type: report.contentType }));
                const link = document.createElement("a");
                link.href = url;
                link.download = "compliance-report.html";
                link.click();
                URL.revokeObjectURL(url);
            }
            catch (e) {
                setReportError(e instanceof Error ? e.message : "Rapporten kunne ikke hentes.");
            }
            finally {
                setReportBusy(false);
            }
        }}>Hent compliance-rapport (HTML)</Button>
        {reportError && <p role="alert">{reportError}</p>}
      </section>
      <section>
        <h3>Lovgrundlag</h3>
        <p>
          {r.legalCitation.note}{" "}
          <Link to={`/lovgrundlag#${r.legalCitation.sourceId}`} {...stylex.props(cockpitStyles.aComposition)}>
            Se {r.legalCitation.sourceId} i Lovgrundlag-visningen
          </Link>
          .
        </p>
        <p {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}>
          Anvendte regler: {r.report.appliedRules.join(", ")}
        </p>
      </section>
    </section>);
}
