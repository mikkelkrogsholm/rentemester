import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { ButtonLink, Button, PageHeader } from "../components/ui";
// Consumer review of existing exceptions; native production is outside this flow.
import { useEffect, useRef, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { useOptionalAuth } from "../lib/auth-context";
import { useCapabilities } from "../lib/useCapabilities";
import { ErrorState, Loading } from "../components/Feedback";
import { ApiError, api } from "../lib/api";
import type { AgentSuggestionRow, CompanyAgentSuggestions } from "../lib/types";
import { useAsync } from "../lib/useAsync";
const SEVERITY_LABEL: Record<AgentSuggestionRow["severity"], string> = {
    high: "Høj prioritet",
    medium: "Mellem prioritet",
    low: "Lav prioritet",
};
export function SuggestionsView() {
    const { slug = "" } = useParams();
    const auth = useOptionalAuth();
    const capabilities = useCapabilities(slug);
    const readable = !auth?.loading && capabilities.can("company.read");
    const state = useAsync<CompanyAgentSuggestions>((signal) => readable
        ? api.agentSuggestions(slug, { signal })
        : Promise.reject(new ApiError("access_denied", "Din adgang til forslagene kunne ikke bekræftes. Kontrollér din session og adgang.", 403)), [slug, auth?.session?.id, auth?.currentSessionId, auth?.context, readable]);
    if (state.loading && !state.data)
        return <Loading label="Henter agent-forslag…"/>;
    if (state.error && !state.data)
        return <ErrorState message={state.error} onRetry={state.reload}/>;
    const data = state.data!;
    const currency = data.company.currency || "DKK";
    return (<section data-cockpit-page="suggestions" data-evidence-issue="655" {...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statement,
			)}>
			{state.error && (<div role="alert" {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.banner,
						cockpitStyles.bannerWarning,
					)}>
					Status kunne ikke opdateres. De tidligere hentede oplysninger vises
					fortsat.
				</div>)}
			<PageHeader title="Forslag" actions={<>
						<div {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.rowActions,
							)}>
							<ButtonLink to={`/companies/${slug}/manage`} variant={"secondary"} xstyle={[cockpitStyles.statementBtnComposition2]}>
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
						{data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
						{data.company.country} · {currency} · Agent-forslag
					</p>
				</div>
			</PageHeader>

      <p {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
					cockpitStyles.statementAsof,
				)}>
        Her gennemgår du eksisterende åbne Agent-forslag. En regel vises kun, når
        den findes i kildegrundlaget. Godkendelse og afvisning løser kun
        undtagelsen og gemmer beslutningen på revisionssporet. Ingen bogføring udføres her.
      </p>

			<div {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.invoicesSummary,
					cockpitStyles.statusGrid,
				)}>
				<div {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
						cockpitStyles.statusCard,
					)}>
					<h3 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.statusCardH3,
						)}>
						Forslag i kø
					</h3>
					<div {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
						)}>
						{data.count}
					</div>
					<p {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							cockpitStyles.statusNote,
						)}>
						{data.count === 0
            ? "Agenten har intet, der venter på en beslutning."
            : data.count === 1
                ? "Ét forslag venter på din beslutning."
                : `${data.count} forslag venter på din beslutning.`}
					</p>
				</div>
				<div {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
						cockpitStyles.statusCard,
					)}>
					<h3 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.statusCardH3,
						)}>
						Høj prioritet
					</h3>
					<div {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
						)}>
						{data.bySeverity.high}
					</div>
					<p {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							cockpitStyles.statusNote,
						)}>
						Bør afgøres først — kan udløse rentepåkrav eller manglende
						bogføring.
					</p>
				</div>
				<div {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
						cockpitStyles.statusCard,
					)}>
					<h3 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.statusCardH3,
						)}>
						Mellem / Lav
					</h3>
					<div {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.statusFigure,
						)}>
						{data.bySeverity.medium + data.bySeverity.low}
					</div>
					<p {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							cockpitStyles.statusNote,
						)}>
						{data.bySeverity.medium} mellem · {data.bySeverity.low} lav
					</p>
				</div>
			</div>

      <h3 {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h3,
					viewStyles.site0,
				)}>Forslag</h3>
      {data.rows.length === 0 ? (<div {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
						cockpitStyles.archivedNotice,
					)}>
          <p {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}>
            Der er ingen åbne forslag.
          </p>
        </div>) : (<div>
          <table {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.tableData,
							cockpitStyles.tableStatementTable,
							cockpitStyles.statementTableScrollTable,
						)}>
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
									)}>Prioritet</th>
                <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition,
									)}>Type</th>
                <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition,
									)}>Agentens vurdering</th>
                <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition,
									)}>Hjemmel</th>
                <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition,
									)}>Handling</th>
              </tr>
            </thead>
            <tbody {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
              {data.rows.map((row) => (<SuggestionRowView key={`${slug}:${row.exceptionId}`} row={row} slug={slug} onChanged={() => state.reload()} revision={data} refreshing={state.loading} companyName={data.company.name}/>))}
            </tbody>
          </table>
        </div>)}

			<p {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.statementCheck,
					cockpitStyles.statementCheckOk,
				)}>
				Hvert forslag er en åben undtagelse i ledger'en — godkendelse og
				afvisning løser undtagelsen via samme kerne (
				<code {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.code,
					)}>
					resolveException
				</code>
				) som CLI'en og MCP-kald bruger, så beslutningen står på audit-sporet
				med din actor og en beslutningstekst.
			</p>
		</section>);
}
function sourceText(value: unknown): string {
    return value == null ? "Ikke registreret" : typeof value === "string" ? value : JSON.stringify(value, null, 2);
}
function SuggestionRowView({ row, slug, onChanged, revision, refreshing, companyName }: {
    row: AgentSuggestionRow;
    slug: string;
    onChanged: () => void;
    revision: CompanyAgentSuggestions;
    refreshing: boolean;
    companyName: string;
}) {
    const auth = useOptionalAuth();
    const capabilities = useCapabilities(slug);
    const allowed = capabilities.can("company.review") && !auth?.loading;
    const [pending, setPending] = useState<{
        decision: "approve" | "reject";
        revision: CompanyAgentSuggestions;
        auth: typeof auth;
        context: unknown;
    } | null>(null);
    const current = useRef({ allowed, auth, context: auth?.context, revision, refreshing, slug });
    current.current = { allowed, auth, context: auth?.context, revision, refreshing, slug };
    const valid = pending && allowed && !refreshing && pending.revision === revision && pending.auth === auth && pending.context === auth?.context;
    useEffect(() => { if (pending && !valid)
        setPending(null); }, [pending, valid]);
    function review(decision: "approve" | "reject") {
        setPending({ decision, revision, auth, context: auth?.context });
    }
    async function decide(note: string) {
        const now = current.current;
        if (!pending || !now.allowed || now.refreshing || now.slug !== slug || now.revision !== pending.revision || now.auth !== pending.auth || now.context !== pending.context) {
            throw new ApiError("conflict", "Konteksten er ændret. Hent og gennemgå forslaget igen.", 409);
        }
        const captured = now;
        await (pending.decision === "approve" ? api.approveAgentSuggestion : api.rejectAgentSuggestion)(slug, row.exceptionId, note);
        if (current.current.slug === captured.slug && current.current.auth === captured.auth && current.current.revision === captured.revision)
            onChanged();
    }
    const severityLabel = SEVERITY_LABEL[row.severity];
    const flagClass = row.severity === "high"
        ? "warn"
        : row.severity === "medium"
            ? "neutral"
            : "ok";
    return (<tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>
        <span {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.flag,
						flagClass === "ok" && cockpitStyles.flagOk,
						flagClass === "neutral" && cockpitStyles.flagNeutral,
					)}>{severityLabel}</span>
      </td>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>
        <strong {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>{row.kindLabel}</strong>
        <div {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
						viewStyles.site1,
					)}>
          {row.type}
          {row.agentActor ? ` · ${row.agentActor}` : ""}
        </div>
      </td>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>
        <p {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						viewStyles.site2,
					)}>{row.rationale}</p>
        {row.requiredAction ? (<p {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
							viewStyles.site3,
						)}>
            <strong {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>Foreslået handling:</strong> {row.requiredAction}
          </p>) : null}
        {row.link ? (<p {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							viewStyles.site4,
						)}>
            <Link to={`/companies/${slug}/${row.link}`} {...stylex.props(cockpitStyles.aComposition)}>
              Åbn relateret side →
            </Link>
          </p>) : null}
      </td>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>
        {row.ruleId ? (<code {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.code,
						)}>{row.ruleId}</code>) : (<span {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}>—</span>)}
      </td>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>
        <div {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.rowActions,
					)}>
          <Button requiredPermission="company.review" type="button" onClick={() => review("approve")} disabled={refreshing || !allowed} aria-label={`Godkend ${row.kindLabel}`}>
            Godkend
          </Button>
          <Button requiredPermission="company.review" variant="secondary" type="button" onClick={() => review("reject")} disabled={refreshing || !allowed} aria-label={`Afvis ${row.kindLabel}`}>
            Afvis
          </Button>
        </div>
        {!allowed && <p>Kun læseadgang. Bed workspace-ejeren om rettigheden company.review for at beslutte forslag.</p>}
        {valid && (<ConfirmDialog title={`Beslut forslag #${row.exceptionId}`} operationScope={`/companies/${slug}/agent-suggestions`}
        // One resource lock across decisions; the frozen pending review binds the decision.
        operationKey={String(row.exceptionId)} body={<>
              <p><strong>{pending.decision === "approve" ? "Godkend" : "Afvis"}: {row.kindLabel}</strong></p>
              <p {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>Virksomhed: {companyName} ({slug}) · ID #{row.exceptionId}</p>
              <p {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>Dette løser kun undtagelsen med audit. Ingen bogføring udføres.</p>
              <dl>
                <dt>Vurdering</dt><dd>{row.rationale}</dd>
                <dt>Foreslået handling</dt><dd>{row.requiredAction ?? "Ikke registreret"}</dd>
                <dt>Regel</dt><dd>{row.ruleId ?? "Ikke registreret"}</dd>
                <dt>Kildegrundlag</dt><dd><pre>{sourceText(row.sourceEvidence)}</pre></dd>
                <dt>Posteringsforslag (udføres ikke)</dt><dd><pre>{sourceText(row.postingPreview)}</pre></dd>
                <dt>Registreret af / program</dt><dd>{row.agentActor ?? "Ikke registreret"} / {row.agentProgram ?? "Ikke registreret"}</dd>
              </dl>
            </>} confirmLabel={pending.decision === "approve" ? "Godkend" : "Afvis forslag"} confirmKind={pending.decision === "approve" ? "primary" : "danger"} noteLabel="Begrundelse (valgfri)" onConfirm={decide} onClose={() => setPending(null)} onRefresh={onChanged}/>)}
      </td>
    </tr>);
}
const viewStyles = stylex.create({
    site0: { marginTop: "1.5rem" },
    site1: { fontSize: "0.85em" },
    site2: { margin: 0 },
    site3: { margin: "0.25rem 0 0 0", fontSize: "0.9em" },
    site4: { margin: "0.25rem 0 0 0", fontSize: "0.9em" },
});
