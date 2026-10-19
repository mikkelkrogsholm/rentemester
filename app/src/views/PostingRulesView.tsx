import { useState, type FormEvent } from "react";
import { PageHeader, Input, Button, ButtonLink } from "../components/ui";
import { useParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ErrorState, Loading } from "../components/Feedback";
import { todayIso } from "../lib/format";
type Rule = Awaited<ReturnType<typeof api.postingRules>>[number] & { state?: string; conditions?: { attributes?: { bankText?: string } }; outcome?: { account?: string } };
export function PostingRulesView() {
  const { slug = "" } = useParams();
  const state = useAsync((signal) => api.postingRules(slug, { signal }), [slug]);
  const [ruleId, setRuleId] = useState(""); const [bankText, setBankText] = useState("");
  const [account, setAccount] = useState(""); const [date, setDate] = useState(todayIso());
  const [reason, setReason] = useState(""); const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<{ action: "propose" | "approve" | "disable"; rule?: Rule } | null>(null);
  const outcome = useMutationOutcome(state.reload);
  function propose(e: FormEvent) { e.preventDefault(); if (!outcome.isBlocked()) setPending({ action: "propose" }); }
  async function confirm(note: string) {
    if (!pending || outcome.isBlocked()) return;
    try {
      const body = pending.action === "propose" ? { proposal: { ruleId, effectiveFrom: date, conditions: { currency: "DKK", attributes: { bankText } }, outcome: { account }, provenance: "cockpit-manual", rationale: reason } } : { ruleId: pending.rule!.ruleId, version: pending.rule!.version, expectedPayloadHash: pending.rule!.payloadHash, rationale: note, provenance: "cockpit-manual" };
      if (pending.action !== "propose" && !note.trim()) throw new Error("Angiv en begrundelse.");
      await api.mutatePostingRule(slug, pending.action, body).catch(outcome.reject);
      setError(null); state.reload();
      if (pending.action === "propose") { setRuleId(""); setBankText(""); setAccount(""); setReason(""); }
    } catch (e) { setError(e instanceof Error ? e.message : "Reglen kunne ikke gemmes."); throw e; }
  }
  if (state.loading && !state.data) return <Loading label="Henter posteringsregler…" />;
  if (state.error && !state.data) return <ErrorState message={state.error} onRetry={state.reload} />;
  return <section className="statement"><PageHeader title="Posteringsregler" actions={<ButtonLink to={`/companies/${slug}/batchbogfoering`}>Åbn bankens arbejdskø</ButtonLink>}><p className="muted">Selskabslokale forslag og godkendte versioner — adskilt fra Lovgrundlag.</p></PageHeader>
    <p>En anden bruger med godkendelsesret skal godkende forslaget. Regler foreslår en konto ved præcist match på banktekst og DKK; hver bogføring kræver stadig en gennemgået kladde. Moms og andre vurderinger må ikke udledes af bankteksten.</p>
    {outcome.feedback}{error && <p role="alert">{error}</p>}{state.error && <p role="alert">{state.error}</p>}
    <form className="card form-grid" onSubmit={propose}><h3>Foreslå regel</h3>
      <label>Regel-id<Input required value={ruleId} maxLength={128} onChange={e => setRuleId(e.target.value)} /></label>
      <label>Præcis banktekst<Input required value={bankText} maxLength={512} onChange={e => setBankText(e.target.value)} /></label>
      <label>Foreslået modkonto<Input required value={account} onChange={e => setAccount(e.target.value)} /></label>
      <label>Gælder fra<Input required type="date" value={date} onChange={e => setDate(e.target.value)} /></label>
      <label>Begrundelse<Input required value={reason} maxLength={2000} onChange={e => setReason(e.target.value)} /></label>
      <Button requiredPermission="company.review" type="submit" disabled={outcome.blocked}>Gennemgå forslag</Button>
    </form>
    <div className="table-scroll"><table className="data"><thead><tr><th>Regel</th><th>Version</th><th>Proveniens</th><th>Evidens</th><th>Status og handling</th></tr></thead><tbody>{state.data!.length ? (state.data! as Rule[]).map(rule => <tr key={`${rule.ruleId}-${rule.version}`}><td>{rule.ruleId}<div>{rule.conditions?.attributes?.bankText} → {rule.outcome?.account}</div><p>{rule.rationale}</p></td><td>v{rule.version}</td><td>{rule.provenance}</td><td><code title={rule.payloadHash}>{rule.payloadHash.slice(0, 12)}…</code><div>Forfatter: {rule.createdBy}</div></td><td>{rule.state ?? "Status ikke oplyst"}{rule.state === "proposed" && <Button requiredPermission="company.review" disabled={outcome.blocked} onClick={() => setPending({ action: "approve", rule })}>Godkend regel</Button>}{rule.state === "approved" && <Button requiredPermission="company.review" disabled={outcome.blocked} onClick={() => setPending({ action: "disable", rule })}>Deaktivér regel</Button>}</td></tr>) : <tr><td colSpan={5} className="muted">Ingen posteringsregler endnu.</td></tr>}</tbody></table></div>
    {pending && <ConfirmDialog title={pending.action === "propose" ? "Gem regelforslag" : pending.action === "approve" ? "Godkend præcis regelversion" : "Deaktivér regel"} body={<p>{pending.action === "propose" ? `${bankText} → konto ${account}. ${reason}` : `${pending.rule!.ruleId} v${pending.rule!.version} · ${pending.rule!.payloadHash}`} Reglen bogfører ikke automatisk.</p>} confirmLabel="Bekræft" noteLabel={pending.action === "propose" ? undefined : "Begrundelse"} onConfirm={confirm} onClose={() => setPending(null)} onRefresh={state.reload} operationKey={`rule-${pending.rule?.payloadHash ?? ruleId}`} />}
  </section>;
}
