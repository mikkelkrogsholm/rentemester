import { useParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { PageState } from "../components/CockpitPrimitives";

/** A deliberately separate operational view; legal source material remains at /lovgrundlag. */
export function PostingRulesView() {
  const { slug = "" } = useParams();
  const state = useAsync(() => api.postingRules(slug), [slug]);
  if (state.loading && !state.data) return <PageState kind="loading" title="Henter posteringsregler" />;
  if (state.error) return <PageState kind="error" title="Posteringsregler kunne ikke hentes" onRetry={state.reload}>{state.error}</PageState>;
  return <section className="statement" data-cockpit-page="posting-rules" data-evidence-issue="655"><div className="page-head"><div><h2>Posteringsregler</h2><p className="muted">Selskabslokale forslag og godkendte versioner — adskilt fra Lovgrundlag.</p></div></div><p className="muted">Detaljer og handlinger sker med hash, begrundelse og eksplicit bekræftelse via API/CLI/MCP.</p><div className="table-scroll"><table className="data responsive-table" aria-label="Posteringsregler"><thead><tr><th scope="col">Regel</th><th scope="col">Version</th><th scope="col">Proveniens</th><th scope="col">Evidens</th></tr></thead><tbody>{state.data!.length ? state.data!.map((rule) => <tr key={`${rule.ruleId}-${rule.version}`}><td data-label="Regel">{rule.ruleId}</td><td data-label="Version">v{rule.version}</td><td data-label="Proveniens">{rule.provenance}</td><td data-label="Evidens"><code>{rule.payloadHash.slice(0, 12)}…</code></td></tr>) : <tr><td colSpan={4} className="muted">Ingen posteringsregler endnu.</td></tr>}</tbody></table></div></section>;
}
