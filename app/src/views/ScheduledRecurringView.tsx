import { useScheduledRecurringAutomation } from "../components/ScheduledRecurringProvider";
import { useRef, useState } from "react";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { Button, Input, Select } from "../components/ui";
import { scheduledRecurringApi, type ScheduledAction, type ScheduledPlan } from "../lib/api/scheduled-recurring";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";

const labels: Record<ScheduledAction, string> = {
  enable: "Aktivér planlagte fakturaer", pause: "Sæt på pause", resume: "Genoptag",
  cancel: "Annullér planlægning", "catch-up": "Sæt valgte forekomster i kø",
  skip: "Spring valgte forekomster over", dispatch: "Udsted valgt forekomst",
};
const intervals: Record<string, string> = { weekly: "uge", monthly: "måned", quarterly: "kvartal", yearly: "år" };
const states = { disabled: "Ikke aktiveret", enabled: "Aktiveret", paused: "På pause", cancelled: "Annulleret" };
/** All dates, periods, consent and issuance decisions come from the Rust server. */
export function ScheduledRecurringView({ slug, templates = [] }: { slug: string; templates?: Array<{ id: number; name: string }> }) {
  const automation = useScheduledRecurringAutomation();
  const current = useAsync(signal => scheduledRecurringApi.current(slug, signal), [slug]);
  const [templateId, setTemplateId] = useState("");
  const [action, setAction] = useState<ScheduledAction>("enable");
  const [plan, setPlan] = useState<ScheduledPlan | null>(null);
  const [periods, setPeriods] = useState<number[]>([]);
  const [dispatchPeriod, setDispatchPeriod] = useState("");
  const [reason, setReason] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const sending = useRef(false);
  function refresh() { setPlan(null); setPeriods([]); setDispatchPeriod(""); setConfirmed(false); current.reload(); }
  const outcome = useMutationOutcome(refresh, "scheduled-recurring", slug);
  const row = current.data?.rows.find(r => String(r.templateId) === templateId);
  const reviewed = plan?.template && String(plan.templateId) === templateId && plan.templateHash === row?.templateHash && plan.stateHash === row?.stateHash;
  const usesPlanHash = ["catch-up", "skip", "dispatch"].includes(action);
  const needsPlan = usesPlanHash || action === "enable" || action === "resume";
  const needsReason = ["pause", "resume", "cancel", "skip"].includes(action);
  const choices = plan ? [...(plan.normalDue ? [plan.normalDue] : []), ...plan.queued] : [];
  const canSend = row && confirmed && !busy && !outcome.blocked &&
    (!needsReason || reason.trim()) && (!needsPlan || reviewed) &&
    (!["catch-up", "skip"].includes(action) || periods.length > 0) &&
    (action !== "dispatch" || choices.some(p => String(p.periodIndex) === dispatchPeriod));
  async function loadPlan() {
    if (!row || sending.current) return;
    sending.current = true; setBusy(true); setMessage(""); setConfirmed(false);
    try { setPlan(await scheduledRecurringApi.plan(slug, row.templateId)); setPeriods([]); setDispatchPeriod(""); }
    catch (e) { setPlan(null); setMessage(e instanceof Error ? e.message : "Planen kunne ikke hentes."); }
    finally { sending.current = false; setBusy(false); }
  }
  async function submit() {
    if (!canSend || !row || sending.current || outcome.isBlocked()) return;
    sending.current = true; setBusy(true); setMessage("");
    const body: Record<string, unknown> = { templateId: row.templateId, confirm: true };
    if (action === "enable") body.templateHash = row.templateHash;
    else if (usesPlanHash) {
      body.planHash = plan!.planHash;
      if (action === "dispatch") body.periodIndex = Number(dispatchPeriod);
      else body.periodIndexes = periods;
      if (action === "skip") body.reason = reason;
    } else { body.expectedStateHash = row.stateHash; body.reason = reason; }
    try {
      automation?.stop(slug, row.templateId);
      const result = await scheduledRecurringApi.mutate(slug, action, body).catch(outcome.reject);
      setMessage(result.dispatch?.status === "failed" ? `Kørslen mislykkedes: ${result.dispatch.error ?? "Se kørselsstatus"}.` :
        result.generation ? "Fakturaen er udstedt med original og PDF. Den er ikke bogført eller sendt." : "Planlægningen er opdateret.");
      if (action === "enable" || action === "resume") automation?.start(slug, row.templateId, result.scheduledRecurring.templateHash);
      refresh();
    } catch (e) { setMessage(e instanceof Error ? e.message : "Handlingen kunne ikke bekræftes."); }
    finally { sending.current = false; setBusy(false); }
  }
  if (current.loading && !current.data) return <Loading label="Henter planlagte fakturaer…" />;
  if (current.error) return <ErrorState message={current.error} onRetry={refresh} />;
  return <section className="card" aria-label="Planlagte fakturaer">
    <h2>Planlagte fakturaer</h2>
    <p>Gennemgå forfaldne forekomster og vælg, hvilke der skal køres eller springes over. Udstedelse opretter en faktura med PDF; bogføring og afsendelse er særskilte handlinger.</p>
    <p className="muted">Tidspunkt: {current.data?.asOfDate} · {current.data?.timeZone}. Automatik starter kun ved dit valg i denne app og stopper, når appen lukkes eller adgangen bortfalder. Efter dvale skal manglende forekomster gennemgås. Status og genåbning starter ikke automatik.</p>
    <Button type="button" onClick={refresh} disabled={busy}>Opdatér planlægningsstatus</Button>
    <label>Skabelon<Select aria-label="Planlagt fakturaskabelon" value={templateId} disabled={busy} onChange={e => {
      setTemplateId(e.target.value); setPlan(null); setPeriods([]); setDispatchPeriod(""); setConfirmed(false); setMessage("");
    }}><option value="">Vælg skabelon</option>{current.data?.rows.map(r => <option key={r.templateId} value={r.templateId}>{templates.find(t => t.id === r.templateId)?.name ?? `Skabelon ${r.templateId}`} — {states[r.state]}</option>)}</Select></label>
    {!current.data?.rows.length && <p>Opret først en fakturaskabelon ovenfor.</p>}
    {row && <>
      <dl><dt>Status</dt><dd>{states[row.state]}</dd><dt>Næste forekomst</dt><dd>{row.nextOccurrence?.issueDate ?? "Ingen"}</dd>
        <dt>Automatik i denne app</dt><dd>{automation?.active(slug, row.templateId) ? "Kører" : "Ikke startet"}</dd><dt>Manglende forekomster</dt><dd>{row.missedCount}</dd><dt>I kø</dt><dd>{row.queued.length}</dd>
        <dt>Seneste kørsel</dt><dd>{row.lastRun ? `${String(row.lastRun.issueDate ?? "Registreret")} · ${{ succeeded: "Udstedt", failed: "Mislykket", unresolved: "Uafklaret" }[String(row.lastRun.status)] ?? String(row.lastRun.status ?? "")}` : "Ingen"}</dd></dl>
      {row.error && <Banner kind="warning">Kørslen kræver handling: {row.error}</Banner>}
      <Button type="button" disabled={busy} onClick={() => void loadPlan()}>Gennemgå forekomster</Button>
      {plan && <div><h3>Plan pr. {plan.asOfDate}</h3>
        {plan.template && <section aria-label="Gennemgå fakturaskabelon"><h4>{plan.template.name}</h4><p>Sælger: {plan.template.payload.seller?.name ?? "Ikke angivet"} · Kunde: {plan.template.payload.buyer?.name ?? "Ikke angivet"}</p><p>Første fakturadato: {plan.template.firstIssueDate} · Betalingsfrist: {plan.template.paymentTermsDays} dage · Interval: {plan.template.intervalCount} × {intervals[plan.template.interval] ?? plan.template.interval}</p><p>Momssats i skabelonen: {plan.template.payload.vatRatePercent == null ? "Ikke angivet" : `${plan.template.payload.vatRatePercent} %`}</p><table><thead><tr><th>Beskrivelse</th><th>Antal</th><th>Enhedspris ekskl. moms</th></tr></thead><tbody>{plan.template.payload.lines.map((line, i) => <tr key={i}><td>{line.description}</td><td>{String(line.quantity)}</td><td>{String(line.unitPriceExVat)} {plan.template.payload.currency ?? ""}</td></tr>)}</tbody></table><p>Beløbene er skabelonens kildepriser. Endelige beløb valideres og beregnes ved udstedelse.</p></section>}
        {plan.truncated && <Banner kind="warning">Planen viser højst 100 forekomster. Gennemgå resten efter denne behandling.</Banner>}
        {plan.missed.length ? <fieldset disabled={busy}><legend>Manglende forekomster</legend>{plan.missed.map(p => <label key={p.periodIndex}>
          <input type="checkbox" checked={periods.includes(p.periodIndex)} onChange={e => { setPeriods(e.target.checked ? [...periods, p.periodIndex] : periods.filter(i => i !== p.periodIndex)); setConfirmed(false); }} /> {p.issueDate} (periode {p.periodIndex})</label>)}</fieldset> : <p>Ingen manglende forekomster.</p>}
      </div>}
      <label>Handling<Select aria-label="Planlægningshandling" value={action} disabled={busy} onChange={e => { setAction(e.target.value as ScheduledAction); setConfirmed(false); }}>{Object.entries(labels).map(([id, label]) => <option key={id} value={id}>{label}</option>)}</Select></label>
      {action === "dispatch" && <label>Forekomst til udstedelse<Select aria-label="Forekomst til udstedelse" value={dispatchPeriod} disabled={busy} onChange={e => { setDispatchPeriod(e.target.value); setConfirmed(false); }}><option value="">Vælg gennemgået forekomst</option>{choices.map(p => <option key={p.periodIndex} value={p.periodIndex}>{p.issueDate} (periode {p.periodIndex})</option>)}</Select></label>}
      {needsReason && <label>Begrundelse<Input aria-label="Planlægningsbegrundelse" value={reason} disabled={busy} onChange={e => { setReason(e.target.value); setConfirmed(false); }} /></label>}
      {needsPlan && !reviewed && <p>Hent en aktuel plan før denne handling.</p>}
      <label><input type="checkbox" checked={confirmed} disabled={busy || outcome.blocked} onChange={e => setConfirmed(e.target.checked)} /> Jeg bekræfter den valgte handling og de viste forekomster.</label>
      {automation && row.state === "enabled" && <Button type="button" requiredPermission="company.draft.write" disabled={!confirmed || !reviewed || busy || outcome.blocked || automation.active(slug, row.templateId)} onClick={() => { if (!outcome.isBlocked()) { automation.start(slug, row.templateId, row.templateHash); setConfirmed(false); } }}>Start automatik mens appen er åben</Button>}
      {outcome.feedback}
      <Button type="button" requiredPermission="company.draft.write" disabled={!canSend} onClick={() => void submit()}>{labels[action]}</Button>
    </>}
    {message && <p role="status">{message}</p>}
  </section>;
}
