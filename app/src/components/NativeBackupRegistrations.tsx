import { useRef, useState } from "react";
import { request } from "../lib/api/_shared";
import { Button } from "./ui";

type Intent = { action: "add" | "remove" | "declare"; operationId: string; destinationId: string; confirm: true; [field: string]: unknown };
type Operation = { format: string; operationId: string; action: Intent["action"]; destinationId: string; actor: string; createdAt: string; result: unknown };
type Registration = { destinationId: string; label: string; kind: string; location: string; attestedBy: string; active: boolean; operatorTargetId: string | null };
type Backup = { backupId: string; archiveSha256: string; archiveSizeBytes: number; backupCreatedAt: string };
type View = { ok: true; workspaceId: string; slug: string; destinations: Registration[]; history: Operation[]; backups: Backup[]; operatorTargets: { id: string; label: string; kind: string }[]; errors: string[] };
const validId = (value: unknown): value is string => typeof value === "string" && /^[a-z0-9-]{8,64}$/.test(value);
const key = (workspace: string, slug: string) => `rentemester:backup-registrations:${workspace}:${slug}`;
function remembered(storage: string): Intent | null {
  try { const value = JSON.parse(localStorage.getItem(storage) ?? "null"); return value && ["add", "remove", "declare"].includes(value.action) && validId(value.operationId) && validId(value.destinationId) && value.confirm === true ? value : null; }
  catch { return null; }
}
export function NativeBackupRegistrations({ slug }: { slug: string }) {
  const [saved, setSaved] = useState<Intent[]>([]);
  const [view, setView] = useState<View | null>(null), [pending, setPending] = useState<Intent | null>(null);
  const [busy, setBusy] = useState(false), [error, setError] = useState(""), [message, setMessage] = useState(""), [missing, setMissing] = useState(false);
  const [label, setLabel] = useState(""), [kind, setKind] = useState("ssh"), [location, setLocation] = useState(""), [attestedBy, setAttestedBy] = useState("");
  const [inEeaOrEu, setInEeaOrEu] = useState(false), [nonRelatedParty, setNonRelatedParty] = useState(false), [itSecurity, setItSecurity] = useState("");
  const [country, setCountry] = useState(""), [regionNote, setRegionNote] = useState(""), [securityNote, setSecurityNote] = useState(""), [operatorTargetId, setOperatorTargetId] = useState("");
  const [confirmed, setConfirmed] = useState(false), [declarationReviewed, setDeclarationReviewed] = useState(false), [removeReviewed, setRemoveReviewed] = useState(false), [retryReviewed, setRetryReviewed] = useState(false);
  const [selected, setSelected] = useState(""), [backupId, setBackupId] = useState(""), [note, setNote] = useState("");
  const sending = useRef(false), base = `/api/companies/${encodeURIComponent(slug)}/backup-registrations`;
  async function readList() {
    const value = await request<View>(base);
    if (value?.ok !== true || value.slug !== slug || typeof value.workspaceId !== "string" || !value.workspaceId
      || ![value.destinations, value.history, value.backups, value.operatorTargets, value.errors].every(Array.isArray)) throw Error("Serverens destinationsregister kunne ikke bekræftes.");
    setView(value); setPending(remembered(key(value.workspaceId, slug)));
    try { const history = JSON.parse(localStorage.getItem(`${key(value.workspaceId, slug)}:history`) ?? "[]"); setSaved(Array.isArray(history) ? history.filter(item => validId(item?.operationId) && validId(item?.destinationId) && ["add", "remove", "declare"].includes(item?.action)) : []); }
    catch { setSaved([]); }
    setSelected(previous => value.destinations.some(destination => destination.destinationId === previous) ? previous : value.destinations[0]?.destinationId ?? "");
    setBackupId(previous => value.backups.some(backup => backup.backupId === previous) ? previous : value.backups[0]?.backupId ?? "");
    return value;
  }
  async function load() {
    if (sending.current) return;
    sending.current = true; setBusy(true); setError("");
    try { await readList(); } catch (cause) { setError(cause instanceof Error ? cause.message : "Registeret kunne ikke hentes."); }
    finally { sending.current = false; setBusy(false); }
  }
  async function run(intent: Intent, read: boolean, fresh = false) {
    if (!view || sending.current) return;
    sending.current = true; setBusy(true); setError(""); setMessage(""); setMissing(false);
    setConfirmed(false); setDeclarationReviewed(false); setRemoveReviewed(false); setRetryReviewed(false);
    try {
      if (fresh) { localStorage.setItem(key(view.workspaceId, slug), JSON.stringify(intent)); setPending(intent); }
      const value = await request<{ ok: true; workspaceId: string; slug: string; operation: Operation | null }>(read ? `${base}/operations/${intent.operationId}` : base,
        read ? undefined : { method: "POST", body: JSON.stringify(intent) });
      if (value?.ok !== true || value.slug !== slug || value.workspaceId !== view.workspaceId) throw Error("Serverens svar matcher ikke workspacet.");
      if (read && value.operation === null) { setMissing(true); return; }
      const operation = value.operation;
      if (!operation || operation.format !== "rentemester-native-backup-registration-operation/1" || operation.operationId !== intent.operationId
        || operation.action !== intent.action || operation.destinationId !== intent.destinationId || !operation.actor.startsWith("user:") || !Number.isFinite(Date.parse(operation.createdAt))) throw Error("Kvitteringen matcher ikke handlingen. Hent resultatet igen.");
      localStorage.removeItem(key(view.workspaceId, slug)); setPending(null);
      setMessage(`Handlingen er registreret. Kontrol-id: ${operation.operationId}.`);
      await readList();
    } catch (cause) { setError(`${cause instanceof Error ? cause.message : "Handlingen kunne ikke bekræftes."} Hent resultatet med det gemte kontrol-id.`); }
    finally { sending.current = false; setBusy(false); }
  }
  function submit(action: Intent["action"]) {
    if (!view || pending || busy) return;
    const destinationId = action === "add" ? crypto.randomUUID() : selected;
    if (!validId(destinationId)) return;
    const common: Intent = { action, operationId: crypto.randomUUID(), destinationId, confirm: true };
    if (action === "add") {
      if (!confirmed || !label.trim() || !location.trim() || !attestedBy.trim()) return;
      Object.assign(common, { label: label.trim(), kind, location: location.trim(), attestedBy: attestedBy.trim(), inEeaOrEu, nonRelatedParty,
        regionCountry: country.trim() || null, regionNote: regionNote.trim() || null, itSecurityMeetsStandards: itSecurity === "" ? null : itSecurity === "true", itSecurityNote: securityNote.trim() || null, operatorTargetId: operatorTargetId || null });
    } else if (action === "declare") {
      if (!declarationReviewed || !view.backups.some(backup => backup.backupId === backupId)) return;
      Object.assign(common, { backupId, note: note.trim() || null });
    } else if (!removeReviewed) return;
    void run(common, false, true);
  }
  function retainAndRelease() {
    if (!view || !pending || !missing || busy) return;
    const storage = key(view.workspaceId, slug);
    try {
      const previous = JSON.parse(localStorage.getItem(`${storage}:history`) ?? "[]");
      const history = [...(Array.isArray(previous) ? previous : []), pending];
      localStorage.setItem(`${storage}:history`, JSON.stringify(history)); setSaved(history);
      localStorage.removeItem(storage); setPending(null); setMissing(false);
      setMessage("Det tidligere kontrol-id er bevaret lokalt. En tidligere anmodning kan stadig blive gennemført; afstem historikken, før du registrerer samme handling igen.");
    } catch { setError("Kontrol-id kunne ikke bevares. Hent resultatet igen."); }
  }
  const disabled = busy || !!pending;
  return <section className="card" aria-label="Registrerede backupdestinationer">
    <h3>Registrerede backupdestinationer</h3>
    <p className="muted">Registrér, hvor du opbevarer kopier. En adresse er en beskrivelse; registreringen uploader ingen fil og ændrer ikke serverens godkendte mål. Eksterne overførsler, du noterer her, er uverificerede.</p>
    <Button type="button" disabled={busy} onClick={() => void load()}>Hent registrerede destinationer</Button>
    {pending && <><p>Kontrol-id: {pending.operationId}</p><Button type="button" disabled={busy} onClick={() => void run(pending, true)}>Hent registreringens resultat</Button></>}
    {missing && pending && <>
      <p>Ingen kvittering er registreret endnu. Den tidligere anmodning kan stadig køre.</p>
      <label><input type="checkbox" checked={retryReviewed} disabled={busy} onChange={event => setRetryReviewed(event.target.checked)} />Jeg vil gentage præcis den gemte handling med samme kontrol-id.</label>
      <Button type="button" disabled={busy || !retryReviewed} onClick={() => void run(pending, false)}>Gentag registreringen med samme id</Button>
      <Button type="button" disabled={busy} onClick={retainAndRelease}>Bevar kontrol-id og frigiv formularen</Button>
    </>}
    {view && !pending && saved.map(intent => <p key={intent.operationId}>Gemt kontrol-id: {intent.operationId}. <Button type="button" disabled={busy} onClick={() => {
      try { localStorage.setItem(key(view.workspaceId, slug), JSON.stringify(intent)); setPending(intent); setMissing(false); setRetryReviewed(false); }
      catch { setError("Kontrol-id kunne ikke genåbnes."); }
    }}>Åbn gemt registrering {intent.operationId}</Button></p>)}
    {view && <>
      <h4>Ny destination</h4>
      <fieldset disabled={disabled} onChange={() => setConfirmed(false)}>
        <label>Destinationsnavn<input aria-label="Destinationsnavn" maxLength={128} value={label} onChange={event => setLabel(event.target.value)} /></label>
        <label>Destinationstype<select aria-label="Destinationstype" value={kind} onChange={event => { setKind(event.target.value); setOperatorTargetId(""); }}><option value="local-folder">Lokal mappe</option><option value="dropbox">Dropbox</option><option value="google-drive">Google Drive</option><option value="ssh">SSH</option><option value="other">Andet</option></select></label>
        <label>Placering eller beskrivelse<input aria-label="Placering eller beskrivelse" maxLength={1024} value={location} onChange={event => setLocation(event.target.value)} /></label>
        <label>Attesteret af<input aria-label="Attesteret af" maxLength={128} value={attestedBy} onChange={event => setAttestedBy(event.target.value)} /></label>
        <label><input type="checkbox" checked={inEeaOrEu} onChange={event => setInEeaOrEu(event.target.checked)} />Opbevaringen er i EU/EØS</label>
        <label><input type="checkbox" checked={nonRelatedParty} onChange={event => setNonRelatedParty(event.target.checked)} />Opbevaringen er hos en uafhængig part</label>
        <details><summary>Supplerende erklæringer</summary>
          <label>Landekode<input aria-label="Landekode" maxLength={2} value={country} onChange={event => setCountry(event.target.value.toUpperCase())} /></label>
          <label>Bemærkning om region<input maxLength={2048} value={regionNote} onChange={event => setRegionNote(event.target.value)} /></label>
          <label>Anerkendte IT-sikkerhedsstandarder<select value={itSecurity} onChange={event => setItSecurity(event.target.value)}><option value="">Ikke oplyst</option><option value="true">Ja</option><option value="false">Nej</option></select></label>
          <label>Bemærkning om IT-sikkerhed<input maxLength={2048} value={securityNote} onChange={event => setSecurityNote(event.target.value)} /></label>
          <label>Henvisning til godkendt servermål<select value={operatorTargetId} onChange={event => setOperatorTargetId(event.target.value)}><option value="">Ingen henvisning</option>{view.operatorTargets.filter(target => target.kind === kind).map(target => <option key={target.id} value={target.id}>{target.label}</option>)}</select></label>
          <p>Kopiering til et servermål sker separat i placeringspanelet.</p>
        </details>
      </fieldset>
      <label><input type="checkbox" checked={confirmed} disabled={disabled || !label.trim() || !location.trim() || !attestedBy.trim()} onChange={event => setConfirmed(event.target.checked)} />Jeg har gennemgået registreringen.</label>
      <Button type="button" disabled={disabled || !confirmed} onClick={() => submit("add")}>Registrér destination</Button>
      {view.destinations.length === 0 ? <p>Ingen aktive registreringer.</p> : <>
        <label>Registreret destination<select aria-label="Registreret destination" disabled={disabled} value={selected} onChange={event => { setSelected(event.target.value); setDeclarationReviewed(false); setRemoveReviewed(false); }}>{view.destinations.map(destination => <option key={destination.destinationId} value={destination.destinationId}>{destination.label}</option>)}</select></label>
        {view.destinations.map(destination => <p key={destination.destinationId}>{destination.label} ({destination.kind}): {destination.location}. Attesteret af {destination.attestedBy}.</p>)}
        <h4>Notér en ekstern overførsel</h4>
        <label>Overført backup<select aria-label="Overført backup" value={backupId} disabled={disabled} onChange={event => { setBackupId(event.target.value); setDeclarationReviewed(false); }}>{view.backups.map(backup => <option key={backup.backupId} value={backup.backupId}>{backup.backupCreatedAt} — {backup.archiveSha256}</option>)}</select></label>
        <label>Bemærkning til overførslen<input maxLength={4096} value={note} disabled={disabled} onChange={event => { setNote(event.target.value); setDeclarationReviewed(false); }} /></label>
        <label><input type="checkbox" checked={declarationReviewed} disabled={disabled || !backupId} onChange={event => setDeclarationReviewed(event.target.checked)} />Jeg har selv overført denne backup og vil registrere min erklæring.</label>
        <Button type="button" disabled={disabled || !declarationReviewed} onClick={() => submit("declare")}>Notér ekstern overførsel</Button>
        <label><input type="checkbox" checked={removeReviewed} disabled={disabled} onChange={event => setRemoveReviewed(event.target.checked)} />Jeg vil fjerne registreringen fra aktive destinationer. Backupfiler bevares.</label>
        <Button type="button" disabled={disabled || !removeReviewed} onClick={() => submit("remove")}>Fjern destinationsregistrering</Button>
      </>}
      <details><summary>Registreringshistorik ({view.history.length})</summary>{view.history.map(operation => <p key={operation.operationId}>{operation.action === "declare" ? "Ekstern overførsel erklæret — ikke verificeret" : operation.action === "remove" ? "Registrering fjernet" : "Destination registreret"}: {operation.destinationId}, {operation.createdAt}, {operation.actor}. Kontrol-id: {operation.operationId}.</p>)}</details>
      {view.errors.map((text, index) => <p role="alert" key={`${index}:${text}`}>{text}</p>)}
    </>}
    {error && <p role="alert">{error}</p>}{message && <p role="status">{message}</p>}
  </section>;
}
