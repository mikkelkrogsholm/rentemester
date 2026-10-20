import { useRef, useState } from "react";
import { downloadNativeBackup } from "../lib/api/native-backup";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { NativeBackupDestinations } from "./NativeBackupDestinations";
import { Button } from "./ui";

export function NativeBackupCard() {
  const outcome = useMutationOutcome(undefined, "native-backup", "workspace");
  const sending = useRef(false);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  async function download() {
    if (!confirmed || sending.current || outcome.isBlocked()) return;
    sending.current = true;
    setBusy(true); setError(""); setMessage("");
    try {
      const blob = await downloadNativeBackup().catch(outcome.reject);
      const url = URL.createObjectURL(blob);
      try {
        const anchor = document.createElement("a");
        anchor.href = url;
        anchor.download = "workspace.native-backup.json";
        document.body.appendChild(anchor);
        anchor.click();
        anchor.remove();
      } finally { URL.revokeObjectURL(url); }
      setConfirmed(false);
      setMessage("Download startet. Kontrollér, at filen er gemt. Gendannelse er endnu ikke testet.");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Backupfilen kunne ikke hentes.");
    } finally { sending.current = false; setBusy(false); }
  }
  return <section className="card native-backup">
    <h3>Hent workspace-backup</h3>
    <p>Hent en signeret pakke med workspace-regnskaber og originalbilag. Du skal have adgang til at administrere hele workspacet og alle virksomhederne.</p>
    <p className="muted">Gem filen sikkert og serverens offentlige verifikationsnøgle separat. Ved gendannelse skal login og adgang oprettes på ny. Serverens administrator skal have konfigureret backup, før filen kan hentes.</p>
    {outcome.feedback}
    <label><input type="checkbox" checked={confirmed} disabled={busy || outcome.blocked} onChange={event => setConfirmed(event.target.checked)} /> Jeg vil hente en backup af hele workspacet.</label>
    <Button type="button" disabled={!confirmed || busy || outcome.blocked} onClick={() => void download()}>{busy ? "Danner backup…" : "Dan og download backup"}</Button>
    {error && <p role="alert">{error}</p>}
    {message && <p role="status">{message}</p>}
    <NativeBackupDestinations />
  </section>;
}
