import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { useRef, useState } from "react";
import { api } from "../lib/api";
import { ApiError } from "../lib/api/_shared";
import { servicePrincipalsApi, type ServiceAction, type ServicePrincipal, type ServiceReceipt } from "../lib/api/service-principals";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { Banner, ErrorState, Loading } from "./Feedback";
import { ConfirmDialog } from "./ConfirmDialog";
import { Button, Input } from "./ui";
const markerKey = "rentemester:service-operation:v1";
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
function savedOperation(): string {
  try { return sessionStorage.getItem(markerKey) ?? ""; } catch { return "Lagring utilgængelig"; }
}
/** Only an opaque operation ID survives reload. The one-time credential remains in React memory. */
export function ServicePrincipalsPanel({ onChanged }: { onChanged: (principals: ServicePrincipal[]) => void }) {
  const current = useAsync(async signal => {
    const rows = await servicePrincipalsApi.list({ signal }); onChanged(rows); return rows;
  }, []);
  const [name, setName] = useState("");
  const [pending, setPending] = useState<{ action: ServiceAction | "activate"; principal?: ServicePrincipal; credentialId?: string } | null>(null);
  const [operationId, setOperationId] = useState(savedOperation);
  const [recoveryId, setRecoveryId] = useState("");
  const [receipt, setReceipt] = useState<ServiceReceipt | null>(null);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const sending = useRef(false);
  const outcome = useMutationOutcome(() => current.reload(), "service-access");
  function clearOperation() { sessionStorage.removeItem(markerKey); setOperationId(""); }
  async function apply() {
    if (!pending || sending.current || outcome.isBlocked()) return;
    sending.current = true; setBusy(true); setReceipt(null); setMessage("");
    const { action, principal, credentialId } = pending;
    try {
      if (action === "activate") {
        await api.updateWorkspaceMemberAccess({ userId: principal!.serviceAccountId, action: "set-role", workspaceRole: "member" }).catch(outcome.reject);
        setMessage("Servicekontoens workspace-adgang er aktiveret. Tildel derefter den nødvendige virksomhedsrolle under Ændr adgang.");
      } else {
        if (action !== "recover" && operationId) throw new Error("Afklar den tidligere servicehandling først.");
        const hadPendingOperation = !!operationId;
        const id = action === "recover" ? (operationId || recoveryId) : crypto.randomUUID();
        if (!uuid.test(id)) throw new Error("Angiv et gyldigt operations-ID.");
        // Persist before issuing a key; never persist the request or its returned secret.
        sessionStorage.setItem(markerKey, id);
        if (sessionStorage.getItem(markerKey) !== id) throw new Error("Operations-ID kunne ikke gemmes; intet er sendt.");
        setOperationId(id);
        const body: Record<string, unknown> = { operationId: id, confirm: true };
        if (action === "create") body.displayName = name.trim();
        if (action === "rotate" || action === "revoke") { body.serviceAccountId = principal!.serviceAccountId; body.credentialId = credentialId; }
        let result: ServiceReceipt;
        try {
          result = await servicePrincipalsApi.mutate(action, body);
          if (result.ok !== true || result.operationId !== id ||
              (["create", "rotate"].includes(action) && (typeof result.serviceAccountId !== "string" || !result.serviceAccountId || typeof result.credentialId !== "string" || !result.credentialId)) ||
              ((action === "rotate" || action === "revoke") && result.serviceAccountId !== undefined && result.serviceAccountId !== principal!.serviceAccountId) ||
              (["create", "rotate"].includes(action) && (typeof result.secret !== "string" || !/^rms_[A-Za-z0-9_-]{64}$/.test(result.secret))) ||
              (action === "revoke" && result.revoked !== true) ||
              (action === "recover" && !["completed", "recovered", "recovering"].includes(result.status ?? ""))) {
            throw new ApiError("internal", "Kvitteringen kunne ikke bekræftes. Afklar operations-ID før en ny handling.", 200);
          }
        } catch (error) {
          if (error instanceof ApiError && error.status >= 400 && error.status < 500 && error.status !== 408) {
            if (action !== "recover" || !hadPendingOperation) clearOperation();
            throw new ApiError("refused", error.status === 429
              ? "Der er foretaget for mange servicehandlinger. Vent et minut, før du prøver igen."
              : error.message, error.status);
          }
          throw error;
        }
        if (result.status !== "recovering") clearOperation();
        setReceipt(result);
        setMessage(action === "recover" ? `Operationsstatus: ${result.status}. En tidligere hemmelig nøgle vises aldrig igen. Hvis den ikke blev gemt, skal den tilbagekaldes og erstattes.` : action === "revoke" ? "Servicenøglen er tilbagekaldt." : "Gem den nye nøgle sikkert nu. Den vises kun denne ene gang og forsvinder, når siden forlades.");
        if (action === "create") setName("");
      }
      current.reload();
    } finally { sending.current = false; setBusy(false); }
  }
  return <section {...stylex.props(cockpitStyles.card)} aria-label="Servicekonti">
    <h3>Servicekonti</h3>
    <p>Opret en særskilt konto til serverens planlagte fakturaer. Aktivér workspace-adgang og tildel kun den nødvendige virksomhedsrolle. Nøglen gives til serveroperatøren; den gemmes ikke i browseren.</p>
    {current.loading && !current.data && <Loading label="Henter servicekonti…" />}
    {current.error && <ErrorState message={current.error} onRetry={current.reload} />}
    <label>Navn på servicekonto<Input value={name} maxLength={120} disabled={busy || !!operationId} onChange={e => setName(e.target.value)} /></label>
    <Button requiredPermission="workspace.members.manage" disabled={busy || !!operationId || outcome.blocked || !name.trim()} onClick={() => setPending({ action: "create" })}>Opret servicekonto</Button>
    {current.data?.map(principal => <section key={principal.serviceAccountId} aria-label={principal.displayName}>
      <h4>{principal.displayName}</h4><p>Service-ID: <code>{principal.serviceAccountId}</code></p>
      <Button requiredPermission="workspace.members.manage" disabled={busy || !!operationId || outcome.blocked} onClick={() => setPending({ action: "activate", principal })}>Aktivér workspace-adgang som medlem</Button>
      <ul>{principal.credentials.map(key => <li key={key.credentialId}><code>{key.credentialId}</code> · {key.enabled === false ? "Tilbagekaldt" : "Registreret"} · Udløber: {key.expiresAt ?? "Ikke angivet"}
        <Button requiredPermission="workspace.members.manage" disabled={busy || !!operationId || outcome.blocked} onClick={() => setPending({ action: "rotate", principal, credentialId: key.credentialId })}>Erstat nøgle</Button>
        <Button requiredPermission="workspace.members.manage" variant="danger" disabled={busy || !!operationId || outcome.blocked || key.enabled === false} onClick={() => setPending({ action: "revoke", principal, credentialId: key.credentialId })}>Tilbagekald nøgle</Button>
      </li>)}</ul>
    </section>)}
    {operationId && <Banner kind="warning">En servicehandling kræver afklaring. Operations-ID: <code>{operationId}</code>. Oprettelse, erstatning og tilbagekaldelse gentages ikke automatisk.</Banner>}
    <label>Operations-ID til afklaring<Input value={operationId || recoveryId} readOnly={!!operationId} onChange={e => setRecoveryId(e.target.value)} /></label>
    <Button requiredPermission="workspace.members.manage" disabled={busy || !uuid.test(operationId || recoveryId)} onClick={() => setPending({ action: "recover" })}>Afklar servicehandling</Button>
    {message && <p role="status">{message}</p>}
    {receipt?.secret && <section aria-label="Kvittering med engangsnøgle"><p>Service-ID: <code>{receipt.serviceAccountId}</code></p><p>Nøgle-ID: <code>{receipt.credentialId}</code></p><label>Engangsnøgle<Input type="password" autoComplete="off" readOnly value={receipt.secret} onFocus={e => e.target.select()} /></label><Button onClick={() => setReceipt(null)}>Skjul og glem nøglen</Button></section>}
    {outcome.feedback}
    {pending && <ConfirmDialog title="Bekræft servicehandling" operationKey={`${pending.action}:${operationId || recoveryId}`} body={<p>{pending.action === "create" ? `Opret servicekontoen ${name.trim()} uden virksomhedsadgang.` : pending.action === "recover" ? "Afklar den valgte operation. Uafsluttede udstedelser sikres ved tilbagekaldelse; en hemmelig nøgle genskabes aldrig." : pending.action === "activate" ? `Aktivér ${pending.principal?.displayName} som workspace-medlem. Virksomhedsadgang tildeles særskilt.` : `${pending.action === "revoke" ? "Tilbagekald" : "Erstat"} nøgle ${pending.credentialId} for ${pending.principal?.displayName}. Tidligere hostet samtykke skal gennemgås igen.`}</p>} confirmLabel="Gennemfør servicehandling" onConfirm={apply} onClose={() => setPending(null)} onRefresh={current.reload} />}
  </section>;
}
