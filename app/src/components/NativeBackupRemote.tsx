import { useRef, useState } from "react";
import { request } from "../lib/api/_shared";
import { Button } from "./ui";

type Intent = { operationId: string; placementOperationId: string; remoteObjectId: string };
type Receipt = Intent & { destinationId: string; archiveSha256: string; archiveSizeBytes: number; observedAt: string; remoteSyncVerified: boolean };
const id = (value: string) => /^[a-z0-9-]{8,64}$/.test(value);
const objectId = (value: string) => /^[A-Za-z0-9_-]{1,160}$/.test(value);
const storageKey = (workspace: string, destination: string) => `rentemester-remote-backup:${workspace}:${destination}`;
function remembered(key: string): Intent | null {
  try {
    const value = JSON.parse(localStorage.getItem(key) ?? "null");
    return value && typeof value.operationId === "string" && id(value.operationId)
      && typeof value.placementOperationId === "string" && id(value.placementOperationId)
      && typeof value.remoteObjectId === "string" && objectId(value.remoteObjectId) ? value : null;
  } catch { return null; }
}
export function hasRememberedRemoteVerification(workspace: string, destination: string): boolean {
  const key = storageKey(workspace, destination);
  return remembered(key) !== null || history(key).length > 0;
}
function history(key: string): Intent[] {
  try {
    const values = JSON.parse(localStorage.getItem(`${key}:history`) ?? "[]");
    return Array.isArray(values) ? values.filter(value => value && id(value.operationId)
      && id(value.placementOperationId) && objectId(value.remoteObjectId)) : [];
  } catch { return []; }
}
export function NativeBackupRemote({ workspace, destination, placementOperationId, enabled = true }: { workspace: string; destination: string; placementOperationId: string; enabled?: boolean }) {
  const key = storageKey(workspace, destination);
  const [pending, setPending] = useState<Intent | null>(() => remembered(key));
  const [remoteObjectId, setRemoteObjectId] = useState(() => remembered(key)?.remoteObjectId ?? "");
  const [confirmed, setConfirmed] = useState(false), [busy, setBusy] = useState(false);
  const [missing, setMissing] = useState(false), [receipt, setReceipt] = useState<Receipt | null>(null);
  const [error, setError] = useState("");
  const [previous, setPrevious] = useState<Intent[]>(() => history(key));
  const sending = useRef(false);
  async function run(read: boolean) {
    if (sending.current || busy) return;
    if (!read && (!enabled || !confirmed || receipt || (pending && !missing))) return;
    const operation = pending ?? (!read && id(placementOperationId) && objectId(remoteObjectId)
      ? { operationId: crypto.randomUUID(), placementOperationId, remoteObjectId } : null);
    if (!operation) return;
    sending.current = true;
    if (!read && !pending) {
      try { localStorage.setItem(key, JSON.stringify(operation)); }
      catch { sending.current = false; setError("Kontrol-id kunne ikke gemmes. Ingen kontrol er startet."); return; }
      setPending(operation);
    }
    setBusy(true); setConfirmed(false); setMissing(false); setError("");
    const path = `/api/workspace/backup-destinations/${encodeURIComponent(destination)}/remote-verifications`;
    try {
      const result = await request<{ verification: Receipt | null }>(read ? `${path}/${operation.operationId}` : path,
        read ? undefined : { method: "POST", body: JSON.stringify({ ...operation, confirm: true }) });
      if (result.verification === null && read) { setMissing(true); return; }
      const value = result.verification;
      if (!value || value.destinationId !== destination || value.operationId !== operation.operationId
        || value.placementOperationId !== operation.placementOperationId || value.remoteObjectId !== operation.remoteObjectId
        || value.remoteSyncVerified !== true || !/^[a-f0-9]{64}$/.test(value.archiveSha256)
        || !Number.isSafeInteger(value.archiveSizeBytes) || value.archiveSizeBytes <= 0 || !Number.isFinite(Date.parse(value.observedAt))) {
        throw Error("Serverens kvittering matcher ikke denne kontrol. Hent kvitteringen igen.");
      }
      setReceipt(value);
    } catch (cause) { setError(`${cause instanceof Error ? cause.message : "Kontrollen kunne ikke bekræftes."} Hent kvitteringen med samme kontrol-id før et nyt forsøg.`); }
    finally { sending.current = false; setBusy(false); }
  }
  function next() {
    if (!receipt) return;
    localStorage.removeItem(key); setPending(null); setReceipt(null); setConfirmed(false); setMissing(false); setError("");
  }
  function correctObject() {
    if (!pending || !missing || busy) return;
    const retained = [...previous.filter(value => value.operationId !== pending.operationId), pending];
    try {
      localStorage.setItem(`${key}:history`, JSON.stringify(retained));
      localStorage.removeItem(key);
    } catch { setError("Kontrol-id kunne ikke bevares. Hent kvitteringen igen."); return; }
    setPrevious(retained); setPending(null); setRemoteObjectId(""); setMissing(false); setConfirmed(false); setError("");
  }
  function reopen(operation: Intent) {
    if (pending || busy) return;
    try { localStorage.setItem(key, JSON.stringify(operation)); }
    catch { setError("Kontrol-id kunne ikke gemmes."); return; }
    setPending(operation); setRemoteObjectId(operation.remoteObjectId); setReceipt(null); setMissing(false); setConfirmed(false); setError("");
  }
  return <section aria-label="Kontrol af Google Drive-kopi">
    <h4>Kontrollér kopien i Google Drive</h4>
    <p className="muted">Når backupfilen er kopieret til den konfigurerede Drive-mappe, kan serveren genlæse den og sammenligne de præcise bytes. Kontrollen uploader ingen fil.</p>
    {!enabled && <p>Ny Drive-kontrol er deaktiveret på serveren. Du kan stadig hente gemte kvitteringer.</p>}
    {!pending && <label>Drive-filens id<input aria-label="Drive-filens id" value={remoteObjectId} disabled={busy || !enabled} onChange={event => setRemoteObjectId(event.target.value)} /></label>}
    {!pending && !id(placementOperationId) && <p>Placér eller kontrollér først backupfilen ovenfor.</p>}
    {pending && <><p>Kontrol-id: {pending.operationId}</p><Button type="button" disabled={busy} onClick={() => void run(true)}>Hent fjernkopiens kvittering</Button></>}
    {missing && <p role="status">Der er endnu ingen kvittering. Kontrollen kan stadig køre. Du kan hente kvitteringen igen eller gennemgå et forsøg med samme id.</p>}
    {missing && enabled && <Button type="button" disabled={busy} onClick={correctObject}>Gem kontrol-id og ret fil-id</Button>}
    {!pending && previous.map(operation => <p key={operation.operationId}>Tidligere kontrol: {operation.operationId} ({operation.remoteObjectId}) <Button type="button" disabled={busy} onClick={() => reopen(operation)}>Åbn tidligere kontrol {operation.operationId}</Button></p>)}
    {!receipt && enabled && (!pending || missing) && <>
      <label><input type="checkbox" checked={confirmed} disabled={busy || (!pending && (!id(placementOperationId) || !objectId(remoteObjectId)))} onChange={event => setConfirmed(event.target.checked)} />Jeg vil kontrollere den præcise backupkopi i Google Drive.</label>
      <Button type="button" disabled={busy || !confirmed} onClick={() => void run(false)}>{pending ? "Gentag kontrollen med samme id" : "Kontrollér fjernkopien"}</Button>
    </>}
    {receipt && <><p role="status">Serveren verificerede fjernkopiens bytes {receipt.observedAt} ({receipt.archiveSizeBytes} bytes). SHA256: {receipt.archiveSha256}. Det er en kvittering for denne kontrol; opbevaringsstedets øvrige erklæringer er ikke verificeret.</p><Button type="button" disabled={busy} onClick={next}>Klargør ny fjernkontrol</Button></>}
    {error && <p role="alert">{error}</p>}
  </section>;
}
