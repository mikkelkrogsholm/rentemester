import { useOptionalAuth } from "../lib/auth-context";
import { ReviewedBankQueue } from "./ReviewedBankQueue";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useEffect, useRef, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { Button, Input, PageHeader, Select, Amount } from "../components/ui";
import { api } from "../lib/api";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ApiError } from "../lib/api/_shared";
type BookkeepingBatchPlan = Awaited<ReturnType<typeof api.bookkeepingBatchPlan>>["plan"];
type WorkbenchRow = Awaited<ReturnType<typeof api.bookkeepingWorkbench>>["workbench"]["rows"][number];
type WorkbenchStatus = WorkbenchRow["status"];

const STATUS_LABELS: Record<WorkbenchStatus, string> = {
  ready: "Klar til gennemgang", suggestedMatch: "Foreslået match", missingDocument: "Bilag mangler",
  partyUnresolved: "Modpart skal afklares", accountingDecisionRequired: "Konto skal afklares",
  vatEvidenceRequired: "Momsdokumentation mangler", dimensionEvidenceRequired: "Dimension skal afklares",
  stalePlan: "Plan skal opdateres", applyFailed: "Anvendelse kræver gennemgang",
};
const CHECK_LABELS: Record<string, string> = { audit_chain: "Revisionsspor", trial_balance: "Saldobalance", reconciliation: "Bankafstemning", vat: "Moms" };
const OUTCOME_LABELS: Record<string, string> = { applied: "Bogført", duplicate: "Allerede bogført", failed: "Kræver gennemgang", stale: "Grundlag ændret" };
type Run = { companySlug: string; runId: number; plan: BookkeepingBatchPlan; state?: Awaited<ReturnType<typeof api.bookkeepingBatchStatus>>["state"] };
type Revision = { planHash?: string; approvedAt?: string | null };
const EMPTY_FILTERS = { status: "", bankAccountId: "", partyId: "", documentQuality: "", account: "", vatTreatment: "", dimension: "", search: "" };

export function BookkeepingBatchView() {
  const auth = useOptionalAuth();
  const [params] = useSearchParams();
  const { slug = "" } = useParams();
  const native = auth?.backend === "rust";
  return native && params.get("mode") === "single" ? <><p><Link to={`/companies/${slug}/batchbogfoering`}>Åbn samlet batchkø</Link></p><ReviewedBankQueue /></> : <BatchWorkflowView key={`${auth?.session?.id ?? ""}:${auth?.currentSessionId ?? ""}:${slug}`} native={native} />;
}

function BatchWorkflowView({ native }: { native: boolean }) {
  const auth = useOptionalAuth();
  const active = useRef(true);
  useEffect(() => { active.current = true; return () => { active.current = false; }; }, []);
  const { slug = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const requestedRunId = params.get("runId");
  const [from, setFrom] = useState(params.get("from") ?? "");
  const [to, setTo] = useState(params.get("to") ?? "");
  const [filters, setFilters] = useState({ ...EMPTY_FILTERS, ...Object.fromEntries(Object.keys(EMPTY_FILTERS).map(key => [key, params.get(key) ?? ""])) });
  const [cursor, setCursor] = useState(0);
  const [workbench, setWorkbench] = useState<Awaited<ReturnType<typeof api.bookkeepingWorkbench>>>();
  const [plan, setPlan] = useState<Awaited<ReturnType<typeof api.bookkeepingBatchPlan>>>();
  const [storedRun, setRun] = useState<Run>();
  const run = storedRun?.companySlug === slug ? storedRun : undefined;
  const [loadedScope, setLoadedScope] = useState("");
  const [result, setResult] = useState<Awaited<ReturnType<typeof api.bookkeepingBatchApply>>>();
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [reviewed, setReviewed] = useState(false);
  const [confirmation, setConfirmation] = useState<{ action: "persist" | "approve" | "apply"; planHash: string; plan: BookkeepingBatchPlan; slug: string } | null>(null);
  const [bankOptions, setBankOptions] = useState<Array<{ id: number; name: string }>>([]);
  const [partyOptions, setPartyOptions] = useState<Array<{ partyId: string; name: string }>>([]);
  const [optionError, setOptionError] = useState<string>();
  const requestSequence = useRef(0);
  const workbenchRead = useRef<AbortController | null>(null);
  const actionInFlight = useRef(false);
  const persistedRunKey = useRef<string>();
  const [lookupKey, setLookupKey] = useState("");
  const attemptedPlan = useRef<BookkeepingBatchPlan>();
  const scope = { ...(native ? {} : { companyId: 1 }), accountingFrom: from, accountingTo: to, bankFrom: from, bankTo: to };

  const writeParam = (name: string, value: string) => setParams(current => {
    const next = new URLSearchParams(current);
    if (value) next.set(name, value); else next.delete(name);
    if (name === "from" || name === "to") next.delete("runId");
    return next;
  }, { replace: true });
  const setFilter = (name: keyof typeof filters, value: string) => {
    workbenchRead.current?.abort();
    requestSequence.current++;
    setFilters(current => ({ ...current, [name]: value }));
    writeParam(name, value); setWorkbench(undefined); setPlan(undefined); setReviewed(false); setConfirmation(null); setCursor(0);
  };
  const outcome = useMutationOutcome(async () => {
    if (run) {
      const response = await api.bookkeepingBatchStatus(slug, run.runId);
      setRun({ ...run, state: response.state });
    } else if (native && persistedRunKey.current) await loadRunByKey(persistedRunKey.current);
    else await refresh(0);
  });
  async function loadRunByKey(key: string) {
    const response = await api.bookkeepingBatchStatusByKey(slug, key);
    const saved = response.state;
    if (!saved || saved.run.runKey !== key || !Number.isSafeInteger(saved.run.runId) || saved.run.runId <= 0) throw new Error("Den præcise kørselsnøgle kunne ikke bekræftes.");
    const restored = JSON.parse(saved.run.plan) as BookkeepingBatchPlan;
    if (!/^[a-f0-9]{64}$/.test(restored.planHash) || restored.planHash !== saved.run.planHash || !Array.isArray(restored.items) || (outcome.isBlocked() && attemptedPlan.current && restored.planHash !== attemptedPlan.current.planHash)) throw new Error("Den gemte plan svarer ikke til den forsøgte gennemgåede plan.");
    setRun({companySlug:slug,runId:saved.run.runId,plan:restored,state:saved});
    setFrom(restored.bankFrom); setTo(restored.bankTo); setReviewed(false);
  }
  function changeDate(name: "from" | "to", value: string) {
    workbenchRead.current?.abort();
    persistedRunKey.current = undefined;
    attemptedPlan.current = undefined;
    requestSequence.current++; if (name === "from") setFrom(value); else setTo(value);
    writeParam(name, value); setWorkbench(undefined); setPlan(undefined); setRun(undefined); setResult(undefined); setError(undefined); setReviewed(false); setConfirmation(null); setCursor(0);
  }

  useEffect(() => {
    let cancelled = false;
    const controller = new AbortController();
    setBankOptions([]); setPartyOptions([]); setOptionError(undefined); setBusy(false); setError(undefined); setResult(undefined);
    void Promise.allSettled([api.bankAccounts(slug, { signal: controller.signal }), api.workspaceParties(slug, { signal: controller.signal })]).then(([bank, parties]) => {
      if (cancelled) return;
      if (bank.status === "fulfilled") setBankOptions(bank.value?.accounts ?? []);
      if (parties.status === "fulfilled") setPartyOptions(parties.value?.rows ?? []);
      if (bank.status === "rejected" || parties.status === "rejected") setOptionError("Nogle filtermuligheder kunne ikke hentes. Opdatér siden for at prøve igen.");
    });
    return () => { cancelled = true; controller.abort(); workbenchRead.current?.abort(); requestSequence.current++; };
  }, [slug]);

  useEffect(() => {
    let cancelled = false;
    const controller = new AbortController();
    const runId = Number(requestedRunId);
    if (!Number.isSafeInteger(runId) || runId < 1) return;
    void api.bookkeepingBatchStatus(slug, runId, { signal: controller.signal }).then(response => {
      if (cancelled) return;
      const state = response.state;
      if (!state?.run?.plan) throw new Error("Batchkørslen findes ikke.");
      const restoredPlan = JSON.parse(state.run.plan) as BookkeepingBatchPlan & { scope?: Partial<BookkeepingBatchPlan> };
      if (!/^[a-f0-9]{64}$/.test(restoredPlan.planHash) || !Array.isArray(restoredPlan.items)) throw new Error("Kørslens plan kunne ikke læses.");
      setRun({ companySlug: slug, runId, plan: restoredPlan, state });
      setFrom(restoredPlan.bankFrom ?? restoredPlan.scope?.bankFrom ?? "");
      setTo(restoredPlan.bankTo ?? restoredPlan.scope?.bankTo ?? "");
    }).catch(cause => { if (!cancelled) setError(cause instanceof Error ? cause.message : String(cause)); });
    return () => { cancelled = true; controller.abort(); };
  }, [requestedRunId, slug]);

  async function refresh(next = cursor) {
    workbenchRead.current?.abort();
    const controller = new AbortController();
    workbenchRead.current = controller;
    const sequence = ++requestSequence.current;
    setBusy(true); setError(undefined);
    try {
      const documentQuality = filters.documentQuality === "matched" || filters.documentQuality === "missing" ? filters.documentQuality : undefined;
      const [queueResponse, planResponse] = await Promise.all([
        api.bookkeepingWorkbench(slug, { from, to, status: Object.hasOwn(STATUS_LABELS, filters.status) ? filters.status as WorkbenchStatus : undefined, search: filters.search || undefined,
          bankAccountId: filters.bankAccountId ? Number(filters.bankAccountId) : undefined,
          partyId: filters.partyId || undefined, documentQuality, account: filters.account || undefined,
          vatTreatment: filters.vatTreatment || undefined, dimension: filters.dimension || undefined, cursor: next, limit: 25 }, { signal: controller.signal }),
        api.bookkeepingBatchPlan(slug, scope, { signal: controller.signal }),
      ]);
      if (sequence !== requestSequence.current) return;
      setReviewed(false); setWorkbench(queueResponse); setPlan(planResponse); setLoadedScope(`${slug}:${from}:${to}`); setCursor(next);
    } catch (cause) { if (sequence === requestSequence.current) setError(cause instanceof Error ? cause.message : String(cause)); }
    finally { if (sequence === requestSequence.current) setBusy(false); }
  }

  const currentScope = loadedScope === `${slug}:${from}:${to}`;
  const queue = currentScope ? workbench?.workbench : undefined;
  const visible = outcome.blocked ? attemptedPlan.current : run?.plan ?? (currentScope ? plan?.plan : undefined);
  const selection = queue?.selection ?? queue?.population;
  const canPersist = Boolean(plan && queue && queue.state === "available" && queue.population.blockers === 0 && queue.plan?.planHash === plan.plan?.planHash && !run && (!native || plan.plan.executionPolicy === "atomic_v1"));
  const approved = Boolean(run?.state?.revisions?.some((revision: Revision) => revision.planHash === run.plan.planHash && revision.approvedAt));
  const selfReview = native && Boolean(run?.state?.revisions.some(revision => revision.planHash === run.plan.planHash && revision.plannerKind === "user" && revision.plannerSubjectId === auth?.session?.id));
  function requestAction(action: "persist" | "approve" | "apply") {
    if (!native) { void perform(action); return; }
    if (!visible || !reviewed || visible.executionPolicy !== "atomic_v1") return;
    setConfirmation({ action, planHash: visible.planHash, plan: visible, slug });
  }
  async function perform(action: "persist" | "approve" | "apply") {
    if (outcome.isBlocked()) return;
    if (native && (!confirmation || confirmation.action !== action || confirmation.slug !== slug || confirmation.planHash !== visible?.planHash || visible.executionPolicy !== "atomic_v1")) throw new Error("Planen skal gennemgås igen i den aktuelle virksomhed.");
    if (actionInFlight.current || (action === "persist" && !canPersist) || (action !== "persist" && !run) || (action === "apply" && !approved)) return;
    attemptedPlan.current = visible;
    if (action === "persist" && !persistedRunKey.current) { persistedRunKey.current = `cockpit:${crypto.randomUUID()}`; setLookupKey(persistedRunKey.current); }
    actionInFlight.current = true; setBusy(true); setError(undefined);
    try {
      if (native && run && action !== "persist") {
        const fresh = await api.bookkeepingBatchStatus(slug, run.runId).catch(() => { throw new ApiError("source_unavailable", "Kørslens aktuelle kilde kunne ikke kontrolleres. Ingen handling blev sendt; genlæs og gennemgå igen.", 422); });
        if (!fresh.state || fresh.state.run.planHash !== run.plan.planHash || (action === "apply" && !fresh.state.revisions.some(r => r.planHash === run.plan.planHash && r.approvedAt))) throw new ApiError("stale_source", "Den præcise gennemgåede revision er ikke længere tilgængelig eller godkendt.", 422);
      }
      if (native && !active.current) throw new ApiError("cancelled", "Forbindelsen eller virksomheden er ændret. Ingen handling blev sendt.", 422);
      if (action === "persist") {
        const saved = await api.bookkeepingBatchPersist(slug, { ...scope, runKey: persistedRunKey.current!, ...(native ? { expectedPlanHash: confirmation!.planHash } : {}) }).catch(outcome.reject);
        if (native && (!Number.isSafeInteger(saved.runId) || saved.runId <= 0 || saved.plan?.planHash !== confirmation!.planHash || saved.plan.executionPolicy !== "atomic_v1" || saved.plan.companyId !== confirmation!.plan.companyId)) throw new ApiError("internal", "Serverens kvittering bekræftede ikke den gennemgåede plan. Kontrollér varig historik før næste handling.", 200);
        setRun({ ...saved, companySlug: slug });
      }
      else if (action === "approve" && run) {
        const receipt = await api.bookkeepingBatchApprove(slug, { runId: run.runId, planHash: run.plan.planHash }).catch(outcome.reject);
        if (native && !receipt.state?.revisions.some(r => r.planHash === run.plan.planHash && r.approvedAt)) throw new ApiError("internal", "Godkendelsen kunne ikke bekræftes for den gennemgåede plan. Kontrollér historikken.", 200);
        setRun({ ...run, ...receipt });
      }
      else if (run) {
        const response = await api.bookkeepingBatchApply(slug, { runId: run.runId, planHash: run.plan.planHash }).catch(outcome.reject);
        if (native && (response.ok !== true || response.runId !== run.runId || response.planHash !== run.plan.planHash || !["audit_chain", "trial_balance", "reconciliation", "vat"].every(name => response.checks?.some(c => c.name === name && c.ok === true)) || !run.plan.items.filter(i => i.partition === "ready").every(i => response.results?.some(r => r.actionKey === i.actionKey && ["applied", "duplicate"].includes(r.outcome))))) throw new ApiError("internal", "Den samlede anvendelse og alle fire kontroller kunne ikke bekræftes. Kontrollér kørslens historik; gentag ikke automatisk.", 200);
        setResult(response); if (response.state) setRun({ ...run, state: response.state });
        await refresh(0);
      }
      if (native) setConfirmation(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      // Read durable evidence after an interrupted write; never retry a mutation.
      if (run) {
        try { const response = await api.bookkeepingBatchStatus(slug, run.runId); setRun({ ...run, state: response.state }); }
        catch { setError("Handlingen kunne ikke bekræftes. Kontrollér kørslens varige historik, før du forsøger igen."); }
      }
      if (native) outcome.reject(cause);
    } finally { actionInFlight.current = false; setBusy(false); }
  }

  return <section className="statement bookkeeping-workflow">
    {outcome.feedback}
    {native && outcome.blocked && !run && persistedRunKey.current && <p role="status">Gemningens udfald er uafklaret. Bevar denne kørselsnøgle til manuel kontrol: <code>{persistedRunKey.current}</code>. En ny planlæsning beviser ikke, om kørslen blev gemt. Start ikke en ny gemning uden afklaring.</p>}
    <PageHeader title="Bogføringskø" description="Gennemgå bankposter og deres dokumentation, før den præcise plan godkendes og bogføres." />
    {native && <form className="card form-grid" onSubmit={async e => {e.preventDefault();setError(undefined);try {await loadRunByKey(lookupKey);} catch(cause) {setError(cause instanceof Error ? cause.message : String(cause));}}}><label>Kørselsnøgle til status<Input aria-label="Kørselsnøgle til status" value={lookupKey} maxLength={1024} disabled={busy} onChange={e=>setLookupKey(e.target.value)} /></label><Button type="submit" variant="secondary" disabled={busy || !lookupKey}>Hent kørsel via nøgle</Button></form>}
    {native && <><p>Ny native batch: atomic_v1. Alle nye posteringer, kildebeviser og afsluttende kontroller skal lykkes samlet; ellers rulles hele anvendelsen tilbage. Historiske delvise kørsler kan læses, men kan ikke anvendes her.</p><Link to={`/companies/${slug}/batchbogfoering?mode=single`}>Opret én gennemgået bankkladde</Link></>}
    <ol className="workflow-progress" aria-label="Bogføringens trin">
      <li aria-current={!visible ? "step" : undefined}>1. Vis arbejdskø</li>
      <li aria-current={visible && !run ? "step" : undefined}>2. Gennemgå plan</li>
      <li aria-current={run && !approved ? "step" : undefined}>3. Godkend</li>
      <li aria-current={approved && !result ? "step" : undefined}>4. Anvend</li>
      <li aria-current={result ? "step" : undefined}>5. Kvittering</li>
    </ol>
    <form className="card form-grid" onSubmit={event => { event.preventDefault(); void refresh(0); }}>
      <label>Fra dato<Input aria-label="Fra dato" type="date" value={from} disabled={busy || outcome.blocked} onChange={event => changeDate("from", event.target.value)} /></label>
      <label>Til dato<Input aria-label="Til dato" type="date" value={to} disabled={busy || outcome.blocked} onChange={event => changeDate("to", event.target.value)} /></label>
      <label>Søg<Input aria-label="Søg" value={filters.search} disabled={busy || outcome.blocked} onChange={event => setFilter("search", event.target.value)} placeholder="Tekst, beløb, modpart eller konto" /></label>
      <details className="full-width"><summary>Flere filtre</summary><div className="form-grid">
        <label>Status<Select aria-label="Status" disabled={busy || outcome.blocked} value={filters.status} onChange={event => setFilter("status", event.target.value)}><option value="">Alle statusser</option>{Object.entries(STATUS_LABELS).map(([status, label]) => <option key={status} value={status}>{label}</option>)}</Select></label>
        <label>Bankkonto<Select aria-label="Bankkonto" disabled={busy || outcome.blocked} value={filters.bankAccountId} onChange={event => setFilter("bankAccountId", event.target.value)}><option value="">Alle bankkonti</option>{bankOptions.map(bank => <option key={bank.id} value={bank.id}>{bank.name}</option>)}</Select></label>
        <label>Modpart<Select aria-label="Modpart" disabled={busy || outcome.blocked} value={filters.partyId} onChange={event => setFilter("partyId", event.target.value)}><option value="">Alle modparter</option>{partyOptions.map(party => <option key={party.partyId} value={party.partyId}>{party.name}</option>)}</Select></label>
        <label>Bilagskvalitet<Select aria-label="Bilagskvalitet" disabled={busy || outcome.blocked} value={filters.documentQuality} onChange={event => setFilter("documentQuality", event.target.value)}><option value="">Alle bilag</option><option value="matched">Matchet</option><option value="missing">Mangler</option></Select></label>
        <label>Konto<Input aria-label="Konto" disabled={busy || outcome.blocked} value={filters.account} onChange={event => setFilter("account", event.target.value)} /></label>
        <label>Momsbehandling<Input aria-label="Moms" disabled={busy || outcome.blocked} value={filters.vatTreatment} onChange={event => setFilter("vatTreatment", event.target.value)} /></label>
        <label>Dimension<Input aria-label="Dimension" disabled={busy || outcome.blocked} value={filters.dimension} onChange={event => setFilter("dimension", event.target.value)} placeholder="dimension:medlem" /></label>
      </div></details>
      <div className="row-actions full-width"><Button type="submit" disabled={busy || !from || !to || from > to}>Vis arbejdskø</Button></div>
      {optionError && <p className="muted full-width">{optionError}</p>}
    </form>
    {error && <p role="alert">{error}</p>}
    {queue && selection && <>
      <div className="status-grid"><div className="card"><h2>Klar i udvalg</h2><div className="status-figure">{selection.ready}</div></div><div className="card"><h2>Blokeringer i udvalg</h2><div className="status-figure">{selection.blockers}</div></div></div>
      {queue.population.blockers > 0 && <p className="muted">Hele perioden har {queue.population.blockers} blokeringer; en filtrering kan ikke omgå dem.</p>}
      {queue.state === "zero" ? <p>Ingen uafstemte bankposter i perioden.</p> : <>
        <p className="muted">{queue.completeness.nextAction}</p>
        <div className="table-scroll"><table className="data"><thead><tr><th>Dato</th><th>Banktekst og kilder</th><th className="num">Beløb</th><th>Status</th><th>Næste skridt</th></tr></thead><tbody>{queue.rows.map((row: WorkbenchRow) => <tr key={row.bankTransactionId}>
          <td>{row.date}</td><td><strong>{row.text}</strong><details><summary>Detaljer og kilder</summary><p>Bilag: {row.document?.id ?? "mangler"} · Modpart: {row.document?.party?.name ?? (row.document?.resolutionState === "internal_no_external_party" ? "Intern post" : "Uafklaret")} · Konto: {row.proposed.account ?? "uafklaret"} · Moms: {row.proposed.vatTreatment ?? "uafklaret"} · Dimensioner: {row.proposed.dimensions.map(dimension => `${dimension.dimensionId}: ${dimension.memberId}`).join(", ") || "ingen"}</p><p><Link to={`/companies/${slug}/bank?transactionId=${row.bankTransactionId}`}>Bank og afstemning</Link>{row.drilldown.documentId && <> · <Link to={`/companies/${slug}/bilag?documentId=${row.drilldown.documentId}`}>Bilag</Link></>}{row.drilldown.partyId && <> · <Link to={`/companies/${slug}/workspace-register#party-${encodeURIComponent(row.drilldown.partyId)}`}>Modpart</Link></>}{row.drilldown.runId && <> · <Link to={`/companies/${slug}/batchbogfoering?runId=${row.drilldown.runId}`}>Gennemgået kørsel</Link></>}{row.drilldown.journalEntryId && <> · <Link to={`/companies/${slug}/posteringer?journalEntryId=${row.drilldown.journalEntryId}`}>Postering</Link></>}</p></details></td>
          <td className="num">{typeof row.amount === "string" ? <span>{row.amount} {row.currency}</span> : <Amount value={row.amount} currency={row.currency} />}</td><td>{STATUS_LABELS[row.status] ?? "Ukendt status"}</td><td>{row.nextAction}</td>
        </tr>)}</tbody></table></div>
        <div className="row-actions"><Button variant="secondary" type="button" disabled={busy || cursor === 0} onClick={() => refresh(Math.max(0, cursor - 25))}>Forrige</Button><Button variant="secondary" type="button" disabled={busy || queue.page.nextCursor === null} onClick={() => refresh(queue.page.nextCursor ?? 0)}>Næste</Button></div>
      </>}
      <p className="muted">Periodelukning: {queue.periodClose?.status === "available" ? `${queue.periodClose.blockers} blokeringer` : "Kontrol er ikke tilgængelig"} · <Link to={`/companies/${slug}/periodelas?from=${from}&to=${to}`}>Åbn periodeluk</Link></p>
    </>}
    <section className="card"><h2>Gennemgå og godkend planen</h2>
      <p className="muted">Planen omfatter hele den valgte periode. Filtre ændrer kun visningen af arbejdskøen. Godkendelse skal foretages af en anden bruger end planlæggeren.</p>
      {visible && <><p>Plan-hash: <code>{visible.planHash}</code></p>{native && <>
        <p>Virksomheds-id fra ledger: {visible.companyId} · Politik: {visible.executionPolicy ?? "Historisk, uden native eksekveringspolitik"}</p>
        <details open><summary>Præcis plan, poster og kildebeviser</summary><pre>{JSON.stringify(visible, null, 2)}</pre></details>
        {visible.executionPolicy !== "atomic_v1" ? <p role="alert">Denne historiske plan er kun til gennemgang. Opret en ny native plan med aktuelle kildebeviser.</p> : <label><input type="checkbox" checked={reviewed} disabled={busy || outcome.blocked} onChange={e => setReviewed(e.target.checked)} />Jeg har gennemgået alle poster, kildebeviser og den præcise plan-hash</label>}
        {selfReview && <p>En anden bruger skal godkende denne plan. Planlæggeren kan ikke godkende sit eget arbejde.</p>}
      </>}</>}
      <div className="row-actions">
        <Button variant="secondary" requiredPermission="company.draft.write" type="button" disabled={outcome.blocked || (busy || !canPersist) || (native && !reviewed)} onClick={() => requestAction("persist")}>Gem eksakt plan</Button>
        <Button variant="secondary" requiredPermission="company.review" type="button" disabled={outcome.blocked || (busy || !run || approved || Boolean(result)) || (native && (!reviewed || selfReview || visible?.executionPolicy !== "atomic_v1"))} onClick={() => requestAction("approve")}>Godkend</Button>
        <Button requiredPermission="company.ledger.post" type="button" disabled={outcome.blocked || (busy || !approved || Boolean(result)) || (native && (!reviewed || visible?.executionPolicy !== "atomic_v1"))} onClick={() => requestAction("apply")}>Anvend</Button>
      </div>
    </section>
    {run?.state && <section className="card"><h2>Varig historik</h2>{native && <><Button variant="secondary" disabled={busy} onClick={async () => { try { const response = await api.bookkeepingBatchStatus(slug, run.runId); setRun({ ...run, state: response.state }); } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); } }}>Genlæs kørsel</Button><details><summary>Revisioner, godkendere, forsøg, kvitteringer og kontroller</summary><pre>{JSON.stringify(run.state, null, 2)}</pre></details></>}<p>Revisioner: {run.state.revisions?.length ?? 0} · Forsøg: {run.state.attempts?.length ?? 0} · Kvitteringer: {run.state.receipts?.length ?? 0}</p><p><Link to={`/companies/${slug}/batchbogfoering?runId=${run.runId}`}>Åbn kørsel #{run.runId}</Link></p></section>}
    {result && <section className="card" role="status"><h2>Kørselsresultat</h2><p>Kørsel #{result.runId} · {result.results?.length ?? 0} behandlede poster. Kontrollér kvitteringer og afsluttende kontroller nedenfor.</p>
      {result.results?.length > 0 && <ul>{result.results.map((item: { actionKey: string; outcome: string; error?: string }) => <li key={item.actionKey}><strong>{OUTCOME_LABELS[item.outcome] ?? "Ukendt udfald"}</strong> · {item.actionKey}{item.error && <p>{item.error}</p>}</li>)}</ul>}
      {result.checks?.length > 0 ? <ul aria-label="Afsluttende kontroller">{result.checks.map((check: { name: string; ok: boolean }) => <li key={check.name}>{CHECK_LABELS[check.name] ?? check.name}: {check.ok ? "Bestået" : "Kræver gennemgang"}</li>)}</ul> : <p className="muted">Ingen afsluttende kontroller i dette svar.</p>}
      <Link to={`/companies/${slug}/posteringer`}>Se bogførte posteringer</Link>
    </section>}
    {native && confirmation && <ConfirmDialog title={confirmation.action === "persist" ? "Gem præcis batchplan" : confirmation.action === "approve" ? "Godkend præcis batchplan" : "Anvend atomisk batchplan"} body={<><p>Kun denne virksomhed og denne plan-hash. Godkendelse og bogføring er særskilte handlinger. Der sendes ingen mail.</p><pre>{JSON.stringify(confirmation.plan, null, 2)}</pre></>} confirmLabel={confirmation.action === "persist" ? "Gem gennemgået plan" : confirmation.action === "approve" ? "Godkend gennemgået plan" : "Bogfør gennemgået batch"} onConfirm={async () => { await perform(confirmation.action); }} onClose={() => setConfirmation(null)} onRefresh={async () => { if (run) { const response = await api.bookkeepingBatchStatus(slug, run.runId); setRun({ ...run, state: response.state }); } else if (persistedRunKey.current) await loadRunByKey(persistedRunKey.current); else await refresh(0); }} operationKey={`batch-${confirmation.action}-${confirmation.planHash}`} />}
  </section>;
}
