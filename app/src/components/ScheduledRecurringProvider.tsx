import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { scheduledRecurringApi } from "../lib/api/scheduled-recurring";
import { useOptionalAuth } from "../lib/auth-context";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useMutationBlock } from "../lib/mutation-memory";
import { ApiError } from "../lib/api/_shared";

type Job = { slug: string; templateId: number; templateHash: string; scope: string };
type Controls = { start: (slug: string, templateId: number, templateHash: string) => void;
  stop: (slug: string, templateId: number) => void; active: (slug: string, templateId: number) => boolean };
const Context = createContext<Controls | null>(null);
export const useScheduledRecurringAutomation = () => useContext(Context);
const key = (job: Pick<Job, "slug" | "templateId">) => JSON.stringify([job.slug, job.templateId]);

/** Only explicit activation in this authenticated app session starts a driver. No persisted worker credentials. */
export function ScheduledRecurringProvider({ children }: { children: ReactNode }) {
  const auth = useOptionalAuth();
  const scope = auth?.backend === "rust" && auth.context?.user.id && auth.currentSessionId
    ? `${auth.context.user.id}:${auth.currentSessionId}` : null;
  const [jobs, setJobs] = useState<Job[]>([]);
  const [message, setMessage] = useState("");
  const [busyCompanies] = useState(() => new Set<string>());
  function stop(slug: string, templateId: number) {
    setJobs(old => old.filter(j => key(j) !== key({ slug, templateId })));
  }
  const controls: Controls = {
    start(slug, templateId, templateHash) {
      if (!scope) return;
      setMessage("");
      setJobs(old => [...old.filter(j => key(j) !== key({ slug, templateId })), { slug, templateId, templateHash, scope }]);
    },
    stop,
    active: (slug, templateId) => jobs.some(j => j.scope === scope && key(j) === key({ slug, templateId })),
  };
  useEffect(() => { setJobs(old => old.filter(j => j.scope === scope)); setMessage(""); }, [scope]);
  const active = jobs.filter(j => j.scope === scope);
  return <Context.Provider value={controls}>
    {active.length > 0 && <aside className="banner" role="status">Planlagte fakturaer kører, mens denne app er åben og din adgang er gyldig. Bogføring og afsendelse sker særskilt.
      <button type="button" onClick={() => setJobs([])}>Stop automatik i denne app</button>
    </aside>}
    {message && <p role="status">{message}</p>}
    {active.map(job => <Runner key={key(job)} busyCompanies={busyCompanies} job={job} ownerUserId={auth!.context!.user.id} stopped={notice => {
      stop(job.slug, job.templateId); setMessage(notice);
    }} />)}
    {children}
  </Context.Provider>;
}
function Runner({ job, ownerUserId, stopped, busyCompanies }: { busyCompanies: Set<string>; job: Job; ownerUserId: string; stopped: (notice: string) => void }) {
  const outcome = useMutationOutcome(undefined, "scheduled-recurring", job.slug);
  const pending = useMutationBlock(`${job.slug}:outcome:scheduled-recurring`);
  // Callback changes during status rendering must not restart the timer or replay a write.
  const [callbacks] = useState(() => ({ stopped, outcome, pending }));
  callbacks.stopped = stopped; callbacks.outcome = outcome; callbacks.pending = pending;
  useEffect(() => {
    let cancelled = false, timer: ReturnType<typeof setTimeout> | undefined;
    async function tick() {
      if (cancelled) return;
      if (busyCompanies.has(job.slug)) { timer = setTimeout(() => void tick(), 1000); return; }
      if (callbacks.outcome.isBlocked()) {
        callbacks.stopped("Automatik er stoppet. En tidligere handling har uafklaret udfald; gennemgå kørslen før du fortsætter."); return;
      }
      try {
        const current = await scheduledRecurringApi.current(job.slug);
        if (cancelled) return;
        const row = current.rows.find(r => r.templateId === job.templateId);
        if (!row || row.state !== "enabled" || row.ownerUserId !== ownerUserId || row.templateHash !== job.templateHash || row.error) {
          callbacks.stopped("Automatik er stoppet. Gennemgå skabelon, samtykke og aktuel adgang."); return;
        }
        const plan = await scheduledRecurringApi.plan(job.slug, job.templateId);
        if (cancelled) return;
        if (plan.templateId !== job.templateId || plan.templateHash !== job.templateHash || plan.stateHash !== row.stateHash || plan.truncated || plan.missedCount > 0 || plan.queued.length > 0) {
          callbacks.stopped("Automatik er stoppet. Gennemgå manglende eller køsatte forekomster og vælg kørsel eller spring over."); return;
        }
        if (plan.normalDue && plan.normalDue.issueDate === plan.asOfDate) {
          if (cancelled || callbacks.outcome.isBlocked()) return;
          if (busyCompanies.has(job.slug)) { timer = setTimeout(() => void tick(), 1000); return; }
          busyCompanies.add(job.slug);
          // Persist before dispatch so page closure cannot erase an unresolved attempt.
          const persisted = callbacks.pending.block();
          try {
            if (!persisted) throw new ApiError("internal", "Udfaldslåsen kunne ikke gemmes.", 0);
            const result = await scheduledRecurringApi.mutate(job.slug, "dispatch", {
              templateId: job.templateId, planHash: plan.planHash, periodIndex: plan.normalDue.periodIndex, confirm: true,
            });
            const generation = result.generation as Record<string, unknown> | undefined;
            if (result.dispatch?.status !== "failed" && (result.dispatch?.status !== "succeeded" || !generation ||
                generation.templateId !== job.templateId || generation.periodIndex !== plan.normalDue.periodIndex ||
                typeof generation.documentId !== "number" || !Number.isSafeInteger(generation.documentId) || generation.documentId <= 0)) {
              throw new ApiError("internal", "Serverens kørselskvittering kunne ikke bekræftes.", 200);
            }
            callbacks.pending.release();
            if (cancelled) return;
            if (result.dispatch.status === "failed") {
              callbacks.stopped("Automatik er stoppet efter en mislykket kørsel. Gennemgå kørselsstatus; intet gensendes automatisk."); return;
            }
          } catch (error) {
            if (error instanceof ApiError && error.status >= 400 && error.status < 500 && error.status !== 408 && error.code !== "network" && error.code !== "internal") callbacks.pending.release();
            throw error;
          } finally { busyCompanies.delete(job.slug); }
        }
        if (!cancelled) timer = setTimeout(() => void tick(), 60_000);
      } catch {
        if (!cancelled) callbacks.stopped("Automatik er stoppet. Kontrollér kørselsstatus og adgang, før du starter igen.");
      }
    }
    void tick();
    return () => { cancelled = true; if (timer) clearTimeout(timer); };
  }, [job.slug, job.templateId, job.templateHash, job.scope, ownerUserId, callbacks, busyCompanies]);
  return null;
}
