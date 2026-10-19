import { useRef, useState } from "react";
import { scheduledRecurringApi, type HostedScheduledPlan, type ScheduledRow } from "../lib/api/scheduled-recurring";
import { ApiError } from "../lib/api/_shared";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { ScheduledTemplateReview } from "./ScheduledTemplateReview";
import { useScheduledRecurringAutomation } from "./ScheduledRecurringProvider";
import { Button, Input } from "./ui";
export function HostedScheduledRecurring({ slug, row, onChanged }: { slug: string; row: ScheduledRow; onChanged: () => void }) {
  const [plan, setPlan] = useState<HostedScheduledPlan | null>(null);
  const [reason, setReason] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const sending = useRef(false);
  const automation = useScheduledRecurringAutomation();
  const outcome = useMutationOutcome(onChanged, "scheduled-recurring", slug);
  const reviewed = plan?.templateId === row.templateId && plan.templateHash === row.templateHash && plan.stateHash === row.stateHash && plan.policy === "normal_due_only";
  const clean = reviewed && !row.restoreConsent?.required && !row.error && !plan!.schedule.truncated && plan!.schedule.missedCount === 0 && plan!.schedule.queued.length === 0 && ["enabled", "paused"].includes(row.state);
  const hasReason = !!reason.trim() && reason.trim().length <= 1000;
  async function load() {
    if (sending.current) return;
    sending.current = true; setBusy(true); setPlan(null); setConfirmed(false); setMessage("");
    try { setPlan(await scheduledRecurringApi.hostedPlan(slug, row.templateId)); }
    catch (error) { setMessage(error instanceof Error ? error.message : "Serverplanen kunne ikke hentes."); }
    finally { sending.current = false; setBusy(false); }
  }
  async function apply(revoke: boolean) {
    if (sending.current || !confirmed || !hasReason || outcome.isBlocked() || (!revoke && !clean)) return;
    sending.current = true; setBusy(true); setMessage("");
    try {
      automation?.stop(slug, row.templateId);
      const body = revoke ? { templateId: row.templateId, expectedStateHash: row.stateHash, reason, confirm: true } : {
        templateId: row.templateId, serviceAccountId: plan!.executor.serviceAccountId, credentialId: plan!.executor.credentialId,
        planHash: plan!.planHash, reason, confirm: true,
      };
      const result = await scheduledRecurringApi.mutateHosted(slug, revoke ? "hosted-revoke" : "hosted-consent", body).then(result => {
        const next = result.scheduledRecurring;
        if (next?.templateId !== row.templateId || next.templateHash !== row.templateHash ||
            (revoke ? next.state !== "paused" || next.hostedConsent?.active === true : next.executionMode !== "hosted" || next.hostedConsent?.active !== true || next.hostedConsent.serviceAccountId !== plan!.executor.serviceAccountId || next.hostedConsent.credentialId !== plan!.executor.credentialId)) {
          throw new ApiError("internal", "Serverens samtykkekvittering kunne ikke bekræftes. Kontrollér historikken før du fortsætter.", 200);
        }
        return result;
      }).catch(outcome.reject);
      setPlan(null); setConfirmed(false); onChanged();
      setMessage(result.scheduledRecurring.hostedConsent?.active ? "Samtykket er gemt. Den konfigurerede server kan udstede dagens forekomst, også når browseren er lukket. Bogføring og afsendelse sker særskilt." : "Serverens samtykke er tilbagekaldt, og planlægningen er sat på pause.");
    } catch (error) { setMessage(error instanceof Error ? error.message : "Handlingen kunne ikke bekræftes."); }
    finally { sending.current = false; setBusy(false); }
  }
  return <section aria-label="Planlægning på serveren"><h3>Planlægning på serveren</h3>
    <p>Serveren kan udstede dagens planlagte faktura, mens browseren er lukket. Det kræver en konfigureret servicekonto og dit særskilte samtykke. Manglende og uafklarede kørsler kræver gennemgang; der køres ikke automatisk tilbage i tiden.</p>
    <p>{row.executionMode === "hosted" ? "Planlægningen er knyttet til serveren." : "Planlægningen bruger appens automatik."} {row.hostedConsent?.active ? "Et serversamtykke er registreret. Det beviser ikke, at serverarbejderen kører." : "Intet aktivt serversamtykke."}</p>
    {row.hostedConsent && <p>Servicekonto: <code>{row.hostedConsent.serviceAccountId}</code> · Nøgle: <code>{row.hostedConsent.credentialId}</code> · Godkendt af: <code>{row.hostedConsent.grantorUserId}</code></p>}
    <Button requiredPermission="company.admin" disabled={busy || outcome.blocked} onClick={() => void load()}>Gennemgå serverplan og servicekonto</Button>
    {plan && <><ScheduledTemplateReview template={plan.template} /><p>Konfigureret servicekonto: {plan.executor.displayName} · <code>{plan.executor.serviceAccountId}</code></p><p>Nøgle-ID: <code>{plan.executor.credentialId}</code></p><p>Plan pr. {plan.schedule.asOfDate}: {plan.schedule.missedCount} manglende forekomster · {plan.schedule.queued.length} i kø. Kun dagens normale forekomst kan udstedes automatisk.</p>{!clean && <p>Afklar manglende forekomster, kø, fejl og eventuelt gendannelsessamtykke før serversamtykke.</p>}</>}
    <label>Begrundelse for serversamtykke<Input value={reason} maxLength={1000} disabled={busy || outcome.blocked} onChange={e => { setReason(e.target.value); setConfirmed(false); }} /></label>
    <label><input type="checkbox" checked={confirmed} disabled={busy || outcome.blocked} onChange={e => setConfirmed(e.target.checked)} /> Godkend den viste serverhandling og servicekonto.</label>
    <Button requiredPermission="company.admin" disabled={!clean || !confirmed || !hasReason || busy || outcome.blocked} onClick={() => void apply(false)}>Godkend automatisk udstedelse på serveren</Button>
    {row.hostedConsent?.active && <Button requiredPermission="company.admin" variant="danger" disabled={!confirmed || !hasReason || busy || outcome.blocked} onClick={() => void apply(true)}>Tilbagekald serversamtykke og sæt på pause</Button>}
    {outcome.feedback}{message && <p role="status">{message}</p>}
  </section>;
}
