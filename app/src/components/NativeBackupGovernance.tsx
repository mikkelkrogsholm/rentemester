import { useRef, useState } from "react";
import { request } from "../lib/api/_shared";
import { Button } from "./ui";

type Receipt = { operationId: string; archiveSha256: string; observedAt: string; archiveSizeBytes: number };
type Destination = { id: string; label: string; offsiteAttested: boolean; placements: unknown[]; remoteVerifications: Receipt[] };
type View = {
  ok: true; slug: string;
  backup: { backupDue: boolean; latestBackupAt: string | null; latestBackupId: string | null; checkedAt: string; errors: string[] };
  lock: { locked: boolean };
  governance: { format: string; latestBackupId: string | null; latestBackupExternalCopyObserved: boolean; latestBackupExternalCopyAtDeclaredDestination: boolean; attestationIndependentlyVerified: false; recoveryVerified: false; destinations: Destination[]; errors: string[] };
};
function verified(value: View, slug: string): View {
  const governance = value?.governance;
  if (value?.ok !== true || value.slug !== slug || !value.backup || typeof value.backup.backupDue !== "boolean"
    || typeof value.lock?.locked !== "boolean" || !Array.isArray(value.backup.errors)
    || !governance || governance.format !== "rentemester-native-backup-governance/1"
    || governance.latestBackupId !== value.backup.latestBackupId
    || !(governance.latestBackupId === null || /^[a-f0-9]{64}$/.test(governance.latestBackupId))
    || typeof governance.latestBackupExternalCopyObserved !== "boolean"
    || typeof governance.latestBackupExternalCopyAtDeclaredDestination !== "boolean"
    || governance.attestationIndependentlyVerified !== false || governance.recoveryVerified !== false
    || !Array.isArray(governance.errors) || !Array.isArray(governance.destinations)
    || governance.destinations.some(destination => typeof destination.label !== "string" || typeof destination.id !== "string"
      || typeof destination.offsiteAttested !== "boolean" || !Array.isArray(destination.placements) || !Array.isArray(destination.remoteVerifications)
      || destination.remoteVerifications.some(receipt => receipt.archiveSha256 !== governance.latestBackupId
        || !Number.isFinite(Date.parse(receipt.observedAt)) || !Number.isSafeInteger(receipt.archiveSizeBytes) || receipt.archiveSizeBytes <= 0))) {
    throw Error("Serverens samlede backupstatus kunne ikke bekræftes.");
  }
  return value;
}
export function NativeBackupGovernance({ slug }: { slug: string }) {
  const [value, setValue] = useState<View | null>(null), [busy, setBusy] = useState(false), [error, setError] = useState("");
  const sending = useRef(false);
  async function load() {
    if (sending.current) return;
    sending.current = true; setBusy(true); setError(""); setValue(null);
    try { setValue(verified(await request<View>(`/api/companies/${encodeURIComponent(slug)}/backup-governance`), slug)); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "Samlet backupstatus kunne ikke hentes."); }
    finally { sending.current = false; setBusy(false); }
  }
  return <section className="card" aria-label="Samlet backupstatus">
    <h3>Samlet backupstatus</h3>
    <p className="muted">Sammenhold den gemte backup med kvitteringer for præcis samme fil. En tidligere kontrol af en ekstern kopi beviser ikke, at kopien stadig er tilgængelig.</p>
    <Button type="button" disabled={busy} onClick={() => void load()}>Hent samlet backupstatus</Button>
    {value && <>
      <dl>
        <dt>Seneste verificerede lokale backup</dt><dd>{value.backup.latestBackupAt ?? "Ingen brugbar gemt backup"}</dd>
        <dt>Backupbehov</dt><dd>{value.backup.backupDue ? "Dan en ny backup" : "Backup er ikke forfalden"}</dd>
        <dt>Bogføring</dt><dd>{value.lock.locked ? "Bogføring er låst" : "Bogføring er ikke låst"}</dd>
        <dt>Ekstern kopi</dt><dd>{value.governance.latestBackupExternalCopyObserved ? "Ekstern kopi observeret" : "Ingen verificeret ekstern kopi til denne backup"}</dd>
        {value.backup.latestBackupId && <><dt>Backupfilens SHA256</dt><dd>{value.backup.latestBackupId}</dd></>}
      </dl>
      {value.governance.destinations.map(destination => <div key={destination.id}>
        <h4>{destination.label}</h4>
        <p>{destination.offsiteAttested ? "Administratorens erklæringer om ekstern opbevaring er registreret." : "Destinationen har ikke alle erklæringer om ekstern opbevaring."} Erklæringerne er ikke uafhængigt verificeret.</p>
        <p>Registrerede placeringer af denne backup: {destination.placements.length}.</p>
        {destination.remoteVerifications.map(receipt => <p key={receipt.operationId}>Ekstern kopi observeret {receipt.observedAt} ({receipt.archiveSizeBytes} bytes). Kontrol-id: {receipt.operationId}.</p>)}
      </div>)}
      {value.governance.destinations.length === 0 && <p>Bed serverens administrator konfigurere en backupdestination.</p>}
      {!value.governance.latestBackupExternalCopyObserved && <p>Placér den aktuelle backup og kontrollér dens eksterne kopi ovenfor.</p>}
      <p className="muted">En afprøvet gendannelse og opbevaringsstedets øvrige forhold er ikke verificeret i denne visning.</p>
      {[...value.backup.errors, ...value.governance.errors].map((message, index) => <p role="alert" key={`${index}:${message}`}>{message}</p>)}
    </>}
    {error && <p role="alert">{error}</p>}
  </section>;
}
