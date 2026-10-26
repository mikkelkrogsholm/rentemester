import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { type FormEvent, useState } from "react";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { Button, Input, Select, Textarea } from "../components/ui";
import { workspaceRegistryApi } from "../lib/api/workspace-registry";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";

const predicates = ["business_description", "operating_status", "products_services", "revenue_models", "markets", "customer_types", "industry_tags", "operational_systems", "document_sources", "reporting_dimensions", "responsible_party", "canonical_reference"];
const kinds = ["user", "registry", "party", "corporate_record", "external_snapshot", "canonical_setting"];
export function CompanyKnowledgePanel({ slug }: { slug: string }) {
 const [asOf, setAsOf] = useState(() => new Date().toISOString().slice(0, 10));
 const [includeProposed, setIncludeProposed] = useState(false);
 const knowledge = useAsync(signal => workspaceRegistryApi.companyKnowledge(slug, { signal }, { asOf, includeProposed }), [slug, asOf, includeProposed]);
 const [predicate, setPredicate] = useState(predicates[0]);
 const [value, setValue] = useState('""');
 const [kind, setKind] = useState("user");
 const [ref, setRef] = useState("");
 const [from, setFrom] = useState(() => new Date().toISOString().slice(0, 10));
 const [to, setTo] = useState("");
 const [certainty, setCertainty] = useState("confirmed");
 const [replacement, setReplacement] = useState<string>();
 const [reason, setReason] = useState("");
 const [confirmed, setConfirmed] = useState(false);
 const [busy, setBusy] = useState(false);
 const [result, setResult] = useState<string>();
 const outcome = useMutationOutcome(knowledge.reload);
 async function act(action: "propose" | "review" | "supersede", body: Record<string, unknown>) {
  if (!confirmed || busy || outcome.isBlocked()) return;
  setBusy(true); setResult(undefined);
  try { await workspaceRegistryApi.knowledgeMutate(slug, action, body).catch(outcome.reject); setResult("Ændringen er registreret. Læs den aktuelle tilstand og historik."); setConfirmed(false); setReplacement(undefined); knowledge.reload(); }
  catch (error) { setResult(error instanceof Error ? error.message : "Ændringen blev ikke gennemført."); }
  finally { setBusy(false); }
 }
 function propose(event: FormEvent) {
  event.preventDefault();
  try { const proposal = { predicate, value: JSON.parse(value), source: { kind, ref }, validFrom: from, ...(to ? { validToExclusive: to } : {}), certainty }; void act(replacement ? "supersede" : "propose", replacement ? { assertionId: replacement, replacement: proposal } : proposal); }
  catch { setResult("Værdi skal være gyldig JSON."); }
 }
 const disabled = busy || outcome.blocked || !confirmed;
 return <section {...stylex.props(cockpitStyles.card)} aria-label="Virksomhedsviden">
  <h3>Virksomhedskontekst</h3><p {...stylex.props(cockpitStyles.muted)}>Kildeunderbygget, dateret viden er rådgivende. Den ændrer aldrig bogføring automatisk. Nye erstatningsforslag skal reviewes. Identiske eksisterende erstatninger bevarer deres faktiske reviewstatus; se historikken.</p>
  <label>Viden pr. dato <Input aria-label="Knowledge as-of" type="date" value={asOf} onChange={e => setAsOf(e.target.value)} /></label>
  <label><Input type="checkbox" checked={includeProposed} onChange={e => setIncludeProposed(e.target.checked)} /> Medtag forslag i kontekst</label>
  <Button variant="secondary" onClick={knowledge.reload}>Opdater viden</Button>
  {knowledge.loading && !knowledge.data && <Loading label="Henter viden…" />}
  {knowledge.error && <ErrorState message={knowledge.error} onRetry={knowledge.reload} />}
  {knowledge.data?.context && <>
   {!!knowledge.data.context.conflicts.length && <Banner kind="warning">Konflikt kræver review: {knowledge.data.context.conflicts.join(", ")}. Ingen automatisk anvendelse.</Banner>}
   <h4>Effektiv kontekst</h4><pre>{JSON.stringify(knowledge.data.context, null, 2)}</pre>
   <h4>Forslag og historik</h4>{knowledge.data.history?.map(a => <details key={a.assertionId}><summary>{a.assertionId} · {a.predicate} · {a.reviewState}</summary><pre>{JSON.stringify(a, null, 2)}</pre>
    {a.reviewState === "proposed" && <><Button requiredPermission="company.knowledge.manage" disabled={disabled} onClick={() => void act("review", { assertionId: a.assertionId, decision: "approved", reason })}>Godkend {a.assertionId}</Button><Button requiredPermission="company.knowledge.manage" disabled={disabled} onClick={() => void act("review", { assertionId: a.assertionId, decision: "rejected", reason })}>Afvis {a.assertionId}</Button></>}
    {a.reviewState === "approved" && <Button requiredPermission="company.knowledge.manage" disabled={busy || outcome.blocked} onClick={() => { setReplacement(a.assertionId); setPredicate(a.predicate); setValue(JSON.stringify(a.value)); setKind(a.source.kind); setRef(a.source.ref); setFrom(new Date().toISOString().slice(0, 10)); setTo(""); setConfirmed(false); }}>Forbered erstatning af {a.assertionId}</Button>}
   </details>)}
  </>}
  {outcome.feedback}
  <label>Review-begrundelse <Input aria-label="Knowledge review reason" value={reason} onChange={e => { setReason(e.target.value); setConfirmed(false); }} maxLength={4096} disabled={busy || outcome.blocked} /></label>
  <label><Input aria-label="Knowledge confirm" type="checkbox" checked={confirmed} onChange={e => setConfirmed(e.target.checked)} disabled={busy || outcome.blocked} /> Jeg har gennemgået kilde, værdi, datoer og den valgte ændring</label>
  <form onSubmit={propose} onChange={() => setConfirmed(false)}><h4>{replacement ? `Erstat ${replacement} med et nyt forslag` : "Foreslå viden"}</h4>
   <label>Prædikat <Select aria-label="Knowledge predicate" value={predicate} onChange={e => setPredicate(e.target.value)}>{predicates.map(p => <option key={p}>{p}</option>)}</Select></label>
   <label>Værdi (JSON) <Textarea aria-label="Knowledge value" value={value} onChange={e => setValue(e.target.value)} required /></label>
   <label>Kildetype <Select aria-label="Knowledge source kind" value={kind} onChange={e => setKind(e.target.value)}>{kinds.map(k => <option key={k}>{k}</option>)}</Select></label>
   <label>Kildereference <Input aria-label="Knowledge source ref" value={ref} onChange={e => setRef(e.target.value)} required maxLength={512} /></label>
   <label>Gyldig fra <Input aria-label="Knowledge valid from" type="date" value={from} onChange={e => setFrom(e.target.value)} required /></label>
   <label>Gyldig indtil (eksklusiv, valgfri) <Input aria-label="Knowledge valid to" type="date" value={to} onChange={e => setTo(e.target.value)} /></label>
   <label>Sikkerhed <Select aria-label="Knowledge certainty" value={certainty} onChange={e => setCertainty(e.target.value)}>{["confirmed", "probable", "uncertain"].map(c => <option key={c}>{c}</option>)}</Select></label>
   <Button requiredPermission="company.knowledge.manage" type="submit" disabled={disabled}>{replacement ? "Registrér erstatningsforslag" : "Registrér vidensforslag"}</Button>
   {replacement && <Button variant="secondary" onClick={() => { setReplacement(undefined); setConfirmed(false); }}>Annullér erstatning</Button>}
  </form>{result && <Banner kind={result.startsWith("Ændringen er registreret.") ? "success" : "error"}>{result}</Banner>}
 </section>;
}
