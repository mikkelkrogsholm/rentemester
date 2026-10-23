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
      setMessage("Download startet. Kontrollér, at filen er gemt. Downloadet er ikke en gendannelsesprøve.");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Backupfilen kunne ikke hentes.");
    } finally { sending.current = false; setBusy(false); }
  }
  return <section className="card native-backup">
    <h3>Hent workspace-backup</h3>
    <p>Hent en signeret pakke med workspace-regnskaber og originalbilag. Du skal have adgang til at administrere hele workspacet og alle virksomhederne.</p>
    <p className="muted">Gem filen sikkert og serverens offentlige verifikationsnøgle separat. Ved gendannelse skal login og adgang oprettes på ny. Serverens administrator skal have konfigureret backup, før filen kan hentes.</p>
    <details>
      <summary>Hvis computeren eller adgangen mistes</summary>
      <p>Gem backupfilen og den betroede offentlige verifikationsnøgle uden for denne computer. En kopi på den mistede disk kan ikke bruges til gendannelse.</p>
      <ol>
        <li><strong>Dig som ejer:</strong> Find den separat gemte backup og verifikationsnøgle, og aftal gendannelse med serverens operatør. Du skal ikke dele adgangskoder, recovery-koder eller den private signeringsnøgle.</li>
        <li><strong>Serverens operatør:</strong> Verificér pakken med den separat betroede nøgle, og gendan til en ny mappe. Opret derefter ejerens login og totrinsbekræftelse på ny, og kontrollér regnskaber og originalbilag. Ved lokal drift er du selv operatør.</li>
        <li><strong>Mangler fil, nøgle eller adgang?</strong> Bed den ansvarlige administrator om den separat gemte kopi og den eksisterende betroede verifikationsnøgle. En ny nøgle eller en nøgle fra selve arkivet erstatter ikke den manglende tillid. Uden den rigtige fil og nøgle kan backup'en ikke godkendes til gendannelse.</li>
      </ol>
    </details>
    {outcome.feedback}
    <label><input type="checkbox" checked={confirmed} disabled={busy || outcome.blocked} onChange={event => setConfirmed(event.target.checked)} /> Jeg vil hente en backup af hele workspacet.</label>
    <Button type="button" disabled={!confirmed || busy || outcome.blocked} onClick={() => void download()}>{busy ? "Danner backup…" : "Dan og download backup"}</Button>
    {error && <p role="alert">{error}</p>}
    {message && <p role="status">{message}</p>}
    <NativeBackupDestinations />
  </section>;
}
