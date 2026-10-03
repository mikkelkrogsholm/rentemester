import { PageHeader } from "../components/ui";
import { useParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { ErrorState, Loading } from "../components/Feedback";

/** A deliberately separate operational view; legal source material remains at /lovgrundlag. */
export function PostingRulesView() {
  const { slug = "" } = useParams();
  const state = useAsync((signal) => api.postingRules(slug, { signal }), [slug]);
  if (state.loading && !state.data) return <Loading label="Henter posteringsregler…" />;
  if (state.error) return <ErrorState message={state.error} onRetry={state.reload} />;
  return <section className="statement"><PageHeader title="Posteringsregler"><div><p className="muted">Selskabslokale forslag og godkendte versioner — adskilt fra Lovgrundlag.</p></div></PageHeader><p className="muted">Detaljer og handlinger sker med hash, begrundelse og eksplicit bekræftelse via API/CLI/MCP.</p><div className="table-scroll"><table className="data"><thead><tr><th>Regel</th><th>Version</th><th>Proveniens</th><th>Evidens</th></tr></thead><tbody>{state.data!.length ? state.data!.map((rule) => <tr key={`${rule.ruleId}-${rule.version}`}><td>{rule.ruleId}</td><td>v{rule.version}</td><td>{rule.provenance}</td><td><code>{rule.payloadHash.slice(0, 12)}…</code></td></tr>) : <tr><td colSpan={4} className="muted">Ingen posteringsregler endnu.</td></tr>}</tbody></table></div></section>;
}
