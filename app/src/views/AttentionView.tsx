import { useNavigate, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { ErrorState, Loading } from "../components/Feedback";
import type { CompanyAttention } from "../lib/types";

const sourceLabel = { exception: "Undtagelse", "agent-proposal": "Agentforslag", readiness: "Lukkeparathed", workbench: "Bogføringskø" } as const;

/** The human-facing view of the read-only #649 attention projection. */
export function AttentionView() {
  const { slug = "" } = useParams();
  const navigate = useNavigate();
  const state = useAsync<CompanyAttention>(() => api.attention(slug), [slug]);
  if (state.loading && !state.data) return <section data-evidence-issue="649"><h2 data-evidence-heading>Opgaver der kræver opmærksomhed</h2><p data-evidence-status="loading">Henter opgaver</p><Loading label="Henter opgaver" /></section>;
  if (state.error) {
    const blocked = /\b403\b|forbudt|adgang/i.test(state.error);
    return <section data-evidence-issue="649"><h2 data-evidence-heading>Opgaver der kræver opmærksomhed</h2><p data-evidence-status={blocked ? "warning-or-blocked" : "error"}>{blocked ? "Opgaver er blokeret" : "Opgaver kunne ikke hentes"}</p><ErrorState message={blocked ? "Du har ikke adgang til disse opgaver." : "Opgaver kunne ikke hentes"} onRetry={state.reload} /></section>;
  }
  const attention = state.data!;
  return <section className="attention-view" data-evidence-issue="649">
    <header className="page-head">
      <div><h2 data-evidence-heading>Opgaver der kræver opmærksomhed</h2><p className="muted">{attention.company.name} · én samlet liste over det, der skal afklares.</p></div>
    </header>
    {attention.items.length === 0 ? <div className="card"><p data-evidence-status="empty">Ingen opgaver kræver opmærksomhed</p></div> : <>
      <p className="muted" data-evidence-status="normal">Opgaver klar</p>
      <h3>Prioriteret opgaveliste</h3>
      <ol className="attention-list" aria-label="Prioriteret opgaveliste" data-evidence-data>
        {attention.items.map((item, index) => <li key={item.id} className={`card attention-item severity-${item.severity}`}>
          <div><h3>{item.title}</h3><p>{item.reason}</p><p className="muted">{sourceLabel[item.source]}</p></div>
          <div className="attention-actions">
            <button type="button" className="btn primary" data-evidence-core-action={index === 0 ? true : undefined} onClick={() => navigate(`/companies/${slug}/${item.destination}`)}>
              {index === 0 ? "Åbn næste opgave" : "Åbn opgave"}
            </button>
            <details data-evidence-progressive={index === 0 ? true : undefined}><summary>Se grundlag for opgaven</summary><dl><dt>Kilde</dt><dd>{item.sourceIdentity}</dd><dt>Aktør</dt><dd>{item.actor ?? "Ikke angivet"}</dd><dt>Teknisk grundlag</dt><dd><code>{JSON.stringify(item.evidence)}</code></dd></dl></details>
          </div>
        </li>)}
      </ol>
    </>}
  </section>;
}
