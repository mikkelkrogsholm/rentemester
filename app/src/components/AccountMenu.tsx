import { Button, Dialog, Field, Input } from "./ui";
import { ConfirmDialog } from "./ConfirmDialog";
import { usePendingChanges } from "../lib/useUnsavedChanges";
import { useId, useRef, useState } from "react";
import { authClient } from "../lib/auth-client";
import { useAuth } from "../lib/auth-context";

type ListedSession = {
  id: string;
  token: string;
  createdAt: string | Date;
  userAgent?: string | null;
};

function workspaceRoleLabel(role: "workspace_owner" | "member" | undefined): string {
  return role === "workspace_owner" ? "Workspace-ejer" : "Medlem";
}

function deviceHint(userAgent: string | null | undefined): string {
  if (!userAgent) return "Ukendt browser";
  if (/firefox/i.test(userAgent)) return "Firefox";
  if (/edg/i.test(userAgent)) return "Edge";
  if (/chrome|chromium/i.test(userAgent)) return "Chrome";
  if (/safari/i.test(userAgent)) return "Safari";
  return "Anden browser";
}

function createdLabel(value: string | Date): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "Tidspunkt ukendt" : `Oprettet ${date.toLocaleString("da-DK")}`;
}

export function AccountMenu() {
  const { session, currentSessionId, context, clear, refresh } = useAuth();
  const pendingChanges = usePendingChanges();
  const [confirmation, setConfirmation] = useState<"sign-out" | "all" | ListedSession | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [sessions, setSessions] = useState<ListedSession[] | null>(null);
  const [showPassword, setShowPassword] = useState(false);
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const sessionsId = useId();
  const passwordId = useId();
  const sessionsTrigger = useRef<HTMLButtonElement>(null);
  const passwordTrigger = useRef<HTMLButtonElement>(null);
  const closePassword = () => {
    setCurrentPassword(""); setNewPassword(""); setConfirmPassword("");
    setShowPassword(false); setError(null);
  };
  if (!session) return null;
  const signOut = async (reportFailure = false) => {
    setBusy(true); setError(null);
    try {
      const result = await authClient.signOut();
      if (result.error) throw new Error("sign out failed");
      clear();
    } catch { const message = "Kunne ikke logge ud. Prøv igen."; setError(message); if (reportFailure) throw new Error(message); }
    finally { setBusy(false); }
  };
  const revokeAll = async () => {
    setBusy(true); setError(null);
    try {
      const result = await authClient.revokeSessions();
      if (result.error) throw new Error("revoke failed");
      clear();
    } catch { const message = "Kunne ikke logge ud på alle enheder. Din nuværende session er stadig aktiv."; throw new Error(message); }
    finally { setBusy(false); }
  };
  const showSessions = async () => {
    if (sessions) { setSessions(null); return; }
    closePassword();
    setBusy(true); setError(null);
    try {
      const result = await authClient.listSessions();
      if (result.error || !Array.isArray(result.data)) throw new Error("list failed");
      setSessions(result.data.flatMap((entry) =>
        typeof entry.id === "string" && typeof entry.token === "string"
          ? [{ id: entry.id, token: entry.token, createdAt: entry.createdAt, userAgent: entry.userAgent }]
          : []));
    } catch { setError("Kunne ikke hente aktive sessioner."); }
    finally { setBusy(false); }
  };
  const revokeOne = async (listed: ListedSession) => {
    setBusy(true); setError(null);
    try {
      const result = await authClient.revokeSession({ token: listed.token });
      if (result.error) throw new Error("revoke failed");
      setSessions((current) => current?.filter((entry) => entry.id !== listed.id) ?? null);
    } catch { const message = "Kunne ikke afslutte sessionen."; throw new Error(message); }
    finally { setBusy(false); }
  };
  const changePassword = async () => {
    setError(null); setMessage(null);
    if (newPassword.length < 12 || newPassword !== confirmPassword) {
      setError("Den nye adgangskode skal være mindst 12 tegn, og gentagelsen skal være ens.");
      return;
    }
    setBusy(true);
    try {
      const result = await authClient.changePassword({
        currentPassword, newPassword, revokeOtherSessions: true,
      });
      if (result.error) throw new Error("password change failed");
      setCurrentPassword(""); setNewPassword(""); setConfirmPassword("");
      setShowPassword(false);
      await refresh();
      setMessage("Adgangskoden er ændret, og alle tidligere sessioner er afsluttet.");
    } catch {
      setCurrentPassword(""); setNewPassword(""); setConfirmPassword("");
      setError("Adgangskoden kunne ikke ændres. Log ind igen og prøv på ny.");
    } finally { setBusy(false); }
  };
  return <div className="account-menu" role="group" aria-label="Konto">
    <span className="account-identity"><span className="account-email">{session.email}</span><span className="account-role">{workspaceRoleLabel(context?.workspaceRole)}</span></span>
    <Button className="btn secondary" type="button" onClick={() => pendingChanges ? setConfirmation("sign-out") : void signOut()} disabled={busy}>Log ud</Button>
    <Button ref={sessionsTrigger} className="account-revoke" type="button" aria-haspopup="dialog" aria-expanded={sessions !== null} aria-controls={sessionsId} onClick={() => void showSessions()} disabled={busy}>Sessioner</Button>
    <Button ref={passwordTrigger} className="account-revoke" type="button" aria-haspopup="dialog" aria-expanded={showPassword} aria-controls={passwordId} onClick={() => { setSessions(null); setError(null); setMessage(null); setShowPassword(true); }} disabled={busy}>Skift adgangskode</Button>
    {sessions && <Dialog id={sessionsId} title="Aktive sessioner" onClose={() => setSessions(null)} busy={busy} returnFocusRef={sessionsTrigger}>
      <div className="account-panel">
      {sessions.length === 0 && <p>Ingen aktive sessioner.</p>}
      <ul>{sessions.map((listed) => <li key={listed.id}>
        <span><strong>{listed.id === currentSessionId ? "Denne enhed" : deviceHint(listed.userAgent)}</strong><small>{createdLabel(listed.createdAt)}</small></span>
        {listed.id !== currentSessionId && <Button type="button" className="account-revoke" disabled={busy} onClick={() => setConfirmation(listed)}>Afslut</Button>}
      </li>)}</ul>
      <Button className="account-revoke" type="button" onClick={() => setConfirmation("all")} disabled={busy}>Log ud på alle enheder</Button>
      <div className="modal-actions"><Button variant="secondary" onClick={() => setSessions(null)} disabled={busy}>Luk</Button></div>
      </div>
    </Dialog>}
    {showPassword && <Dialog id={passwordId} title="Skift adgangskode" onClose={closePassword} busy={busy} returnFocusRef={passwordTrigger}>
      <form className="workflow-form" onSubmit={(event) => { event.preventDefault(); void changePassword(); }}>
        {error && <p role="alert" className="banner error">{error}</p>}
        <Field label="Nuværende adgangskode"><Input type="password" autoComplete="current-password" required value={currentPassword} onChange={(event) => setCurrentPassword(event.target.value)} disabled={busy} /></Field>
        <Field label="Ny adgangskode" help="Mindst 12 tegn. Andre enheder skal logge ind igen."><Input type="password" autoComplete="new-password" minLength={12} required value={newPassword} onChange={(event) => setNewPassword(event.target.value)} disabled={busy} /></Field>
        <Field label="Gentag ny adgangskode"><Input type="password" autoComplete="new-password" minLength={12} required value={confirmPassword} onChange={(event) => setConfirmPassword(event.target.value)} disabled={busy} /></Field>
        <div className="modal-actions">
          <Button variant="secondary" onClick={closePassword} disabled={busy}>Annullér</Button>
          <Button type="submit" busy={busy} disabled={!currentPassword || !newPassword || !confirmPassword}>Skift adgangskode</Button>
        </div>
      </form>
    </Dialog>}
    {confirmation && <ConfirmDialog title={confirmation === "sign-out" ? "Log ud?" : confirmation === "all" ? "Log ud på alle enheder?" : "Afslut session?"} body={<p>{confirmation === "all" ? "Alle enheder skal logge ind igen." : confirmation === "sign-out" ? "Du bliver logget ud." : "Denne enhed skal logge ind igen."}{pendingChanges ? " Du har ændringer, som ikke er gemt, og de bliver kasseret." : ""}</p>} confirmLabel={typeof confirmation === "object" ? "Afslut session" : "Log ud"} confirmKind="danger" onClose={() => setConfirmation(null)} onConfirm={async () => { if (confirmation === "all") await revokeAll(); else if (confirmation === "sign-out") await signOut(true); else await revokeOne(confirmation); }} />}
    {message && <span role="status" className="account-message">{message}</span>}
    {error && !showPassword && <span role="alert" className="account-error">{error}</span>}
  </div>;
}
