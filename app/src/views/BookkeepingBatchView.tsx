import { useCallback, useEffect, useMemo, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import type { BatchApplyResponse, BatchPlanResponse, BatchRunState, BatchScope, BookkeepingPlan, Workbench, WorkbenchStatus } from "../lib/api/bookkeeping-batch";
import type { FiscalYearEntry } from "../lib/types";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { FilterBar, PageState, ResponsiveTable } from "../components/CockpitPrimitives";

const stateLabels: Record<WorkbenchStatus, string> = { missingDocument: "Mangler bilag", partyUnresolved: "Mangler part", accountingDecisionRequired: "Mangler kontering/moms", vatEvidenceRequired: "Mangler kontering/moms", dimensionEvidenceRequired: "Mangler kontering/moms", ready: "Klar til dry run", suggestedMatch: "Mangler bilag", stalePlan: "Kræver godkendelse", applyFailed: "Kræver godkendelse" };
const stateOrder: Array<[string, WorkbenchStatus[]]> = [["Mangler bilag", ["missingDocument", "suggestedMatch"]], ["Mangler part", ["partyUnresolved"]], ["Mangler kontering/moms", ["accountingDecisionRequired", "vatEvidenceRequired", "dimensionEvidenceRequired"]], ["Klar til dry run", ["ready"]], ["Kræver godkendelse", ["stalePlan", "applyFailed"]], ["Bogført", []]];
type Run = { runId: number; plan: BookkeepingPlan; state: BatchRunState };

function issueMessage(queue: Workbench | undefined, plan: BatchPlanResponse | undefined, run: Run | undefined): string | undefined {
  if (run) return undefined;
  if (!queue) return "Arbejdskøen er ikke indlæst endnu.";
  if (queue.state !== "available") return queue.completeness.nextAction;
  if (queue.population.blockers > 0) return `Hele perioden har ${queue.population.blockers} afklaringer. Filtre kan ikke omgå dem.`;
  if (!plan || queue.plan?.planHash !== plan.plan.planHash) return "Forhåndsvisningen er ikke den aktuelle eksakte plan. Opdatér den før planen gemmes.";
  return undefined;
}

export function BookkeepingBatchView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const [params] = useSearchParams();
  const requestedRunId = params.get("runId");
  const [years, setYears] = useState<FiscalYearEntry[]>();
  const [filters, setFilters] = useState({ status: "" as "" | WorkbenchStatus, search: "" });
  const [cursor, setCursor] = useState(0);
  const [workbench, setWorkbench] = useState<Workbench>();
  const [plan, setPlan] = useState<BatchPlanResponse>();
  const [run, setRun] = useState<Run>();
  const [result, setResult] = useState<BatchApplyResponse>();
  const [error, setError] = useState<string>();
  const [loading, setLoading] = useState(true);
  const [outcome, setOutcome] = useState("");
  const selectedYear = useMemo(() => year ?? years?.find((item) => item.source === "live")?.label ?? years?.[0]?.label ?? "", [year, years]);
  const fiscalPeriod = useMemo(() => years?.find((item) => item.label === selectedYear), [years, selectedYear]);
  const scope = useMemo<BatchScope | undefined>(() => fiscalPeriod?.start && fiscalPeriod.end ? { companyId: 1, accountingFrom: fiscalPeriod.start, accountingTo: fiscalPeriod.end, bankFrom: fiscalPeriod.start, bankTo: fiscalPeriod.end } : undefined, [fiscalPeriod]);

  useEffect(() => { let active = true; setLoading(true); api.fiscalYears(slug).then((value) => { if (active) setYears(value); }).catch((cause: unknown) => { if (active) setError(cause instanceof Error ? cause.message : String(cause)); }).finally(() => { if (active) setLoading(false); }); return () => { active = false; }; }, [slug]);
  const refresh = useCallback(async (next = 0, preview = false) => { if (!scope) return; setLoading(true); try { const [queue, nextPlan] = await Promise.all([api.bookkeepingWorkbench(slug, { from: scope.bankFrom, to: scope.bankTo, status: filters.status || undefined, search: filters.search || undefined, cursor: next, limit: 25 }), api.bookkeepingBatchPlan(slug, scope)]); setWorkbench(queue.workbench); setPlan(nextPlan); setCursor(next); setError(undefined); if (preview) setOutcome("Samlet dry run forhåndsvist"); } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); } finally { setLoading(false); } }, [filters.search, filters.status, scope, slug]);
  useEffect(() => { if (scope) void refresh(0); }, [refresh, scope]);
  useEffect(() => { const runId = Number(requestedRunId); if (!Number.isSafeInteger(runId) || runId < 1) return; api.bookkeepingBatchStatus(slug, runId).then(({ state }) => { if (!state.run?.plan) throw new Error("Batchkørslen findes ikke."); setRun({ runId, plan: JSON.parse(state.run.plan) as BookkeepingPlan, state }); }).catch((cause: unknown) => setError(cause instanceof Error ? cause.message : String(cause))); }, [requestedRunId, slug]);

  const persistDisabled = issueMessage(workbench, plan, run);
  const approval = run?.state.revisions.length ?? 0;
  const approveDisabled = !run ? "Gem først den eksakte plan til review." : approval ? "Planen er allerede godkendt eller sendt til review; se den varige historik." : undefined;
  const applyDisabled = !run ? "Godkend først en gemt eksakt plan." : !approval ? "Godkend den eksakte plan før bogføring." : undefined;
  const persist = async () => { if (!scope || persistDisabled) return; try { const saved = await api.bookkeepingBatchPersist(slug, { ...scope, runKey: `cockpit:${Date.now()}` }); setRun(saved); setOutcome("Eksakt plan gemt til review"); } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); } };
  const approve = async () => { if (!run || approveDisabled) return; try { const approved = await api.bookkeepingBatchApprove(slug, { runId: run.runId, planHash: run.plan.planHash }); setRun({ ...run, state: approved.state }); setOutcome("Eksakt plan godkendt"); } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); } };
  const apply = async () => { if (!run || applyDisabled) return; try { const applied = await api.bookkeepingBatchApply(slug, { runId: run.runId, planHash: run.plan.planHash }); setResult(applied); setOutcome("Bogføring gennemført"); await refresh(0); } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); } };
  const counts = (states: WorkbenchStatus[]) => states.reduce((sum, state) => sum + (workbench?.counts[state] ?? 0), 0);

  if (loading && !years) return <PageState kind="loading" title="Henter bogføringskø" />;
  if (error && !years) return <PageState kind={/403|forbudt|adgang/i.test(error) ? "blocked" : "error"} title={/403|forbudt|adgang/i.test(error) ? "Bogføring kræver afklaring" : "Bogføringskø kunne ikke hentes"} onRetry={() => void refresh(0)}>{error}</PageState>;
  const status = !scope ? "empty" : error ? (/403|forbudt|adgang/i.test(error) ? "warning-or-blocked" : "error") : loading && !workbench ? "loading" : workbench?.population.total === 0 ? "empty" : workbench?.population.blockers ? "warning-or-blocked" : "normal";
  return <section className="statement" data-cockpit-page="batch-bookkeeping" data-evidence-issue="650">
    <div className="page-head"><div><h2 data-evidence-heading>Bogføringsarbejdsbord</h2><p className="muted" data-evidence-status={status}>{status === "normal" ? "Bogføringskø klar" : status === "loading" ? "Henter bogføringskø" : status === "empty" ? "Ingen poster klar til bogføring" : status === "warning-or-blocked" ? "Bogføring kræver afklaring" : "Bogføringskø kunne ikke hentes"}</p><p className="muted">Bankposten er arbejdskøen. Klargør → samlet dry run/forhåndsvis → gem eksakt plan → godkend → bogfør.</p></div></div>
    {years && <CompanyNav slug={slug} years={years} selectedYear={selectedYear} onYearChange={setYear} />}
    {!scope ? <p>Det valgte regnskabsår har ingen aktiv bogføringsperiode.</p> : <>
      <div className="row-actions"><button type="button" className="btn" data-evidence-core-action onClick={() => void refresh(0, true)} disabled={loading}>Forhåndsvis samlet dry run</button><button type="button" className="btn secondary" disabled={Boolean(persistDisabled)} title={persistDisabled} onClick={() => void persist()}>Gem eksakt plan</button><button type="button" className="btn secondary" disabled={Boolean(approveDisabled)} title={approveDisabled} onClick={() => void approve()}>Godkend</button><button type="button" className="btn" disabled={Boolean(applyDisabled)} title={applyDisabled} onClick={() => void apply()}>Bogfør</button></div>
      <p className="muted">{persistDisabled ?? "Forhåndsvisningen matcher den kanoniske periode og kan gemmes til review."}</p>
      {approveDisabled && run && <p className="muted">{approveDisabled}</p>}{applyDisabled && <p className="muted">{applyDisabled}</p>}
      {error && <p role="alert">{error}</p>}{outcome && <p data-evidence-task-outcome>{outcome}</p>}
      <div className="status-grid" data-evidence-data>{stateOrder.map(([label, states]) => <div className="card" key={label}><h3>{label}</h3><div className="status-figure">{label === "Bogført" ? (result?.results.length ?? 0) : counts(states)}</div></div>)}</div>
      <FilterBar activeFilters={[filters.status && `Status: ${stateLabels[filters.status]}`, filters.search && `Søgning: ${filters.search}`].filter(Boolean) as string[]} onReset={() => { setFilters({ status: "", search: "" }); void refresh(0); }} advanced={<p className="muted">Tekniske identifikatorer vises kun i dokumenteret evidens på den enkelte række.</p>}><label>Vis <select aria-label="Statusfilter" value={filters.status} onChange={(event) => setFilters((current) => ({ ...current, status: event.target.value as "" | WorkbenchStatus }))}><option value="">Alle tilstande</option>{Object.entries(stateLabels).map(([value, label]) => <option value={value} key={value}>{label}</option>)}</select></label><label>Søg <input aria-label="Søg i bank, bilag, part eller konto" value={filters.search} onChange={(event) => setFilters((current) => ({ ...current, search: event.target.value }))} placeholder="Søg på navn eller tekst" /></label></FilterBar>
      {workbench?.population.blockers ? <p className="muted">Hele perioden har {workbench.population.blockers} afklaringer; et filter ændrer ikke dens kanoniske blokeringer.</p> : null}
      {workbench && workbench.state !== "zero" && <ResponsiveTable label="Bogføringskø"><thead><tr><th scope="col">Bankpost</th><th scope="col">Bilag og part</th><th scope="col">Konto og moms</th><th scope="col">Grundlag og næste skridt</th></tr></thead><tbody>{workbench.rows.map((row) => <tr key={row.bankTransactionId}><td data-label="Bankpost"><strong>{row.date}</strong><br />{row.text}<br />{row.amount} {row.currency}<br /><Link to={`/companies/${slug}/bank?transactionId=${row.bankTransactionId}`}>Åbn Bank</Link></td><td data-label="Bilag og part">{row.document ? <><Link to={`/companies/${slug}/bilag?documentId=${row.document.id}`}>Åbn bilag</Link><br />{row.document.party ? <Link to={`/companies/${slug}/workspace-register#party-${encodeURIComponent(row.document.party.id)}`}>{row.document.party.name}</Link> : "Part ikke afklaret"}</> : "Bilag mangler"}</td><td data-label="Konto og moms">Konto: {row.proposed.account ?? "ikke afklaret"}<br />Moms: {row.proposed.vatTreatment ?? "ikke afklaret"}</td><td data-label="Grundlag og næste skridt"><strong>{stateLabels[row.status]}</strong><br />{row.nextAction}<details data-evidence-progressive><summary>Se bilag og kontering</summary><p>Bankkonto: {row.bankAccount.name ?? "ukendt"} · Bilag: {row.document?.id ?? "mangler"} · Canonical part-id: {row.document?.party?.id ?? "ikke afklaret"} · Kildehash: {row.sourceHash}</p></details></td></tr>)}</tbody></ResponsiveTable>}
      {workbench?.state === "zero" && <p>Ingen uafstemte bankposter i perioden.</p>}
      {workbench && <div className="row-actions"><button type="button" className="btn secondary" disabled={cursor === 0} onClick={() => void refresh(Math.max(0, cursor - 25))}>Forrige</button><button type="button" className="btn secondary" disabled={workbench.page.nextCursor === null} onClick={() => void refresh(workbench.page.nextCursor ?? 0)}>Næste</button><Link to={`/companies/${slug}/periodelas?from=${scope.bankFrom}&to=${scope.bankTo}`}>Åbn periodeluk</Link></div>}
      {plan && <p>Plan-hash: <code>{(run?.plan ?? plan.plan).planHash}</code></p>}{run && <div className="card"><h3>Varig historik</h3><p>Revisioner: {run.state.revisions.length} · Forsøg: {run.state.attempts.length} · Kvitteringer: {run.state.receipts.length}</p></div>}{result && <div className="card"><h3>Kørselsresultat</h3><p>{result.results.length} poster behandlet.</p></div>}
    </>}
  </section>;
}
