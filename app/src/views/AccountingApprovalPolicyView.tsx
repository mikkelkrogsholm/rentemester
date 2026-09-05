import { useState } from "react";
import { useParams } from "react-router-dom";
import { api } from "../lib/api";
import type { AccountingApprovalPolicy } from "../lib/api/accounting-approval-policy";
import { useAsync } from "../lib/useAsync";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ErrorState, Loading } from "../components/Feedback";

const LABEL: Record<AccountingApprovalPolicy["reviewMode"], string> = {
  independent_reviewer: "Uafhængig reviewer",
  sole_authorized_bookkeeper: "Autoriseret bogholder",
};

export function AccountingApprovalPolicyView() {
  const { slug = "" } = useParams();
  const state = useAsync(() => api.accountingApprovalPolicy(slug), [slug]);
  const [pending, setPending] = useState<AccountingApprovalPolicy["reviewMode"] | null>(null);
  const [error, setError] = useState<string | null>(null);
  if (state.loading && state.data === undefined) return <Loading label="Henter godkendelsespolitik…" />;
  if (state.error) return <ErrorState message={state.error} onRetry={state.reload} />;
  const policy = state.data;
  async function save() {
    if (!pending) return;
    setError(null);
    try { await api.setAccountingApprovalPolicy(slug, pending, policy?.eventHash ?? null); state.reload(); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "Politikken kunne ikke ændres."); throw cause; }
  }
  return <section className="statement">
    <div className="page-head"><div><h2>Godkendelsespolitik</h2><p className="muted">Bestemmer hvem der kan færdiggøre kontrollerede køb, batches og kladder. Den ændrer aldrig adgang eller bogføring i sig selv.</p></div></div>
    {error && <div className="card archived-notice" role="alert"><p>{error}</p></div>}
    <section className="card"><h3>Aktiv politik</h3>
      <p><strong>{policy ? LABEL[policy.reviewMode] : LABEL.independent_reviewer}</strong></p>
      <p className="muted">{policy ? `Version ${policy.version}. ` : "Ingen særregel er sat; fail-safe standarden gælder. "}{policy ? <code title={policy.eventHash}>{policy.eventHash.slice(0, 12)}…</code> : ""}</p>
      <div className="row-actions"><button className="btn secondary" type="button" disabled={(policy?.reviewMode ?? "independent_reviewer") === "independent_reviewer"} onClick={() => setPending("independent_reviewer")}>Kræv uafhængig reviewer</button><button className="btn" type="button" disabled={policy?.reviewMode === "sole_authorized_bookkeeper"} onClick={() => setPending("sole_authorized_bookkeeper")}>Tillad autoriseret bogholder</button></div>
    </section>
    {pending && <ConfirmDialog title="Ændr godkendelsespolitik" body={<p>Ændringen opretter en ny, append-only policy-version. Eksisterende adgang, dokumentation, moms- og periodelåse ændres ikke.</p>} confirmLabel="Gem politik" onConfirm={save} onClose={() => setPending(null)} />}
  </section>;
}
