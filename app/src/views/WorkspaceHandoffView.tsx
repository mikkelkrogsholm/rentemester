import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useWorkspaceTransfer } from "../components/WorkspaceTransferProvider";
import { useAuth } from "../lib/auth-context";
import { ApiError } from "../lib/api/_shared";
import { fullTransfer, workspaceTransferApi as api, type TransferChallenge, type Transfer, type RestoredReceipt } from "../lib/api/workspace-transfer";
import { useMutationBlock } from "../lib/mutation-memory";
import { UnknownMutationNotice } from "../components/UnknownMutationNotice";
import { Button, Dialog, Input, PageHeader, Select, Textarea } from "../components/ui";
import { useAsync } from "../lib/useAsync";

type Review = { action: "freeze" | "cancel" | "export" | "release" | "activate"; title: string; button: string; body: Record<string, unknown>; snapshot: Transfer } |
  { action: "map"; title: string; button: string; body: Record<string, unknown>; snapshot: Transfer; company: string; receipt: RestoredReceipt };
export function transferNote(value: string, maximum = 2000) {
  const v = value.trim(), n = new TextEncoder().encode(v).length;
  return n >= 10 && n <= maximum && !/[\u0000-\u001f\u007f-\u009f]/u.test(v);
}
function download(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  try { const a = document.createElement("a"); a.href = url; a.download = name; a.click(); } finally { URL.revokeObjectURL(url); }
}
export function WorkspaceHandoffView() {
  const auth = useAuth(), lifecycle = useWorkspaceTransfer();
  if (!lifecycle?.enabled) return <p>Flytning kræver den native Rust-server.</p>;
  return <Handoff key={lifecycle.scope} owner={auth.context?.workspaceRole === "workspace_owner"} />;
}
function Handoff({ owner }: { owner: boolean }) {
  const lifecycle = useWorkspaceTransfer()!;
  const [destination, setDestination] = useState("");
  const [reason, setReason] = useState("");
  const [challenge, setChallenge] = useState("");
  const [release, setRelease] = useState("");
  const [independent, setIndependent] = useState(false);
  const [trusted, setTrusted] = useState(false);
  const [review, setReview] = useState<Review | null>(null);
  const [message, setMessage] = useState("");
  const [archiveHash, setArchiveHash] = useState("");
  const t = lifecycle.transfer;
  const available = owner && t && fullTransfer(t) && !lifecycle.error && !lifecycle.loading;
  useEffect(() => { void lifecycle.refresh(); }, []);
  function prepare(action: Exclude<Review["action"], "map">) {
    if (!available || !t?.stateHash) return;
    const common = { expectedStateHash: t.stateHash, confirm: true, reason: reason.trim() };
    if (action !== "export" && !transferNote(reason)) { setMessage("Angiv en konkret begrundelse på 10–2000 bytes i én linje."); return; }
    if (action === "freeze") {
      if (t.mode !== "active" || !transferNote(destination, 300)) { setMessage("Beskriv destinationen på 10–300 bytes i én linje."); return; }
      setReview({ action, title: "Bekræft frysning", button: "Frys workspace", body: { ...common, destination: destination.trim() }, snapshot: t });
    } else if (action === "cancel") {
      if (t.mode !== "frozen" || t.offeredArchiveHashes?.length) return;
      setReview({ action, title: "Bekræft annullering før eksport", button: "Annullér flytning før eksport", body: common, snapshot: t });
    } else if (action === "export") {
      if (t.mode !== "frozen") return;
      setReview({ action, title: "Bekræft flyttearkiv", button: "Dan og hent flyttearkiv", body: { expectedStateHash: t.stateHash, confirm: true }, snapshot: t });
    } else if (action === "release") {
      if (t.mode !== "frozen" || !t.offeredArchiveHashes?.length || !independent) return;
      try {
        const parsed = JSON.parse(challenge) as TransferChallenge;
        if (parsed.version !== 1 || !Array.isArray(parsed.companies)) throw new Error();
        setReview({ action, title: "Bekræft frigivelse af kilden", button: "Frigiv til dette mål", body: { ...common, challenge: parsed }, snapshot: t });
      } catch { setMessage("Indsæt målets fulde udfordring som gyldig JSON. Ingen nøgle i udfordringen etablerer tillid."); }
    } else if (action === "activate") {
      if (t.mode !== "staged" || !trusted || !release.trim()) return;
      setReview({ action, title: "Bekræft aktivering af målet", button: "Aktivér dette mål", body: { ...common, release: release.trim() }, snapshot: t });
    }
  }
  return <section {...stylex.props(cockpitStyles.statement)}>
    <PageHeader title="Flyt workspace" description={`Denne forbindelse: ${window.location.origin}`} />
    <p>Flytning er særskilt fra almindelig backup. Kilden fryses, et signeret arkiv klargøres på et nyt mål, og ejeren frigiver derefter netop dette mål. Ingen automatisk ophævelse af frysning eller aktivering.</p>
    <Button variant="secondary" onClick={() => void lifecycle.refresh()}>Genlæs flyttestatus</Button>
    {lifecycle.error && <p role="alert">{lifecycle.error}</p>}
    {message && <p role="status">{message}</p>}
    {t && <p>Aktuel tilstand: <strong>{t.mode}</strong></p>}
    {!owner && <p>Kun workspace-ejeren kan administrere flytningen. Du kan fortsat åbne autoriserede rapporter.</p>}
    {owner && t && !fullTransfer(t) && <p>Den fulde ejerstatus kunne ikke bekræftes. Ingen flyttehandling er tilgængelig.</p>}
    {owner && t && fullTransfer(t) && <>
      <p>Destination: {t.destination ?? "Ikke valgt"}</p>
      <h2>Uafklarede eksterne effekter</h2>
      {!t.pendingEffects?.length && <p>Ingen registrerede uafklarede SMTP-forsøg.</p>}
      {t.pendingEffects?.map(e => <p key={`${e.companySlug}:${e.messageId}`}>Uafklaret mail · {e.companySlug} · <Link to={`/companies/${encodeURIComponent(e.companySlug)}/fakturaer`}>Åbn mailhistorik for {e.companySlug}</Link></p>)}
      <p>Flytning afklarer eller gensender ingen mail. Historikken følger arkivet. Kontrollér SMTP-log og dokumentation særskilt.</p>
      {t.mode !== "moved" && <label>Begrundelse<Textarea aria-label="Begrundelse" value={reason} onChange={e => setReason(e.target.value)} maxLength={2000} /></label>}
      {t.mode === "active" && <>
        <label>Destination<Input aria-label="Destination" value={destination} maxLength={300} onChange={e => setDestination(e.target.value)} placeholder="Beskriv det nye lokale workspace eller serveren" /></label>
        <p>Frysning stopper nye ændringer og automatisk kørsel. Kontroller destinationen før du fortsætter.</p>
        <Button disabled={!available} onClick={() => prepare("freeze")}>Gennemgå frysning</Button>
      </>}
      {t.mode === "frozen" && <>
        <h2>Frossen kilde</h2><p>Rapporter og arkivet kan læses. Efter tilbud om ét arkiv kan flytningen ikke annulleres.</p>
        <Button disabled={!available} onClick={() => prepare("export")}>Gennemgå flyttearkiv</Button>
        {!t.offeredArchiveHashes?.length && <Button variant="secondary" disabled={!available} onClick={() => prepare("cancel")}>Gennemgå annullering før eksport</Button>}
        {archiveHash && <p>Arkiv SHA256: <code>{archiveHash}</code></p>}
        <ul>{t.offeredArchiveHashes?.map(h => <li key={h}>Tilbudt arkiv SHA256: <code>{h}</code></li>)}</ul>
        <h2>Klargør et nyt mål</h2>
        <p>Administrator skal kontrollere arkivsignaturen med kildens offentlige nøgle fra en separat betroet kanal og klargøre et helt nyt workspace i transfer-tilstand. Almindelig gendannelse kan ikke erstatte dette trin. Opret derefter ejerens normale e-mailbekræftelse og MFA på målet.</p>
        <label>Målets udfordring (JSON)<Textarea aria-label="Målets udfordring (JSON)" value={challenge} maxLength={16384} onChange={e => { setChallenge(e.target.value); setIndependent(false); }} /></label>
        <label><input type="checkbox" checked={independent} onChange={e => setIndependent(e.target.checked)} />Jeg har kontrolleret mål og udfordring via en uafhængig kanal</label>
        <Button disabled={!available || !independent || !t.offeredArchiveHashes?.length} onClick={() => prepare("release")}>Gennemgå frigivelse</Button>
      </>}
      {t.mode === "moved" && <>
        <h2>Kilden er permanent skrivebeskyttet</h2><p>Frigivelsen autoriserer kun det navngivne mål. Den beviser ikke, at målet er blevet aktiveret. Kontrollér målets egen status efter separat aktivering.</p>
        {t.release ? <><label>Signeret frigivelse<Textarea aria-label="Signeret frigivelse" readOnly value={t.release} /></label><Button variant="secondary" onClick={() => download(new Blob([t.release!], { type: "application/json" }), "workspace-transfer-release.json")}>Hent signeret frigivelse</Button></> : <p>Signeret frigivelse kunne ikke hentes. Genlæs status; forsøg ikke en ny frigivelse.</p>}
      </>}
      {t.mode === "staged" && <>
        <h2>Klargjort, endnu ikke aktivt mål</h2>
        <p>Kildens administrator skal selvstændigt kontrollere denne destination og udfordring. Nøgler eller adresser i et importeret dokument er ikke i sig selv betroede.</p>
        <label>Målets udfordring<Textarea aria-label="Målets udfordring" readOnly value={JSON.stringify(t.challenge, null, 2)} /></label>
        <label>Signeret frigivelse fra kilden<Textarea aria-label="Signeret frigivelse fra kilden" value={release} maxLength={16384} onChange={e => setRelease(e.target.value)} /></label>
        <label><input type="checkbox" checked={trusted} onChange={e => setTrusted(e.target.checked)} />Kildens offentlige nøgle er konfigureret og kontrolleret separat</label>
        <Button disabled={!available || !trusted || !release.trim()} onClick={() => prepare("activate")}>Gennemgå aktivering</Button>
        <p>Aktivering starter ingen mails, indlæsning eller planlagte fakturaer. Ny adgang og nyt samtykke kræves særskilt.</p>
      </>}
      {(t.mode === "staged" || t.mode === "active") && <ReceiptMapping transfer={t} onReview={setReview} enabled={Boolean(available)} />}
      <details><summary>Flyttehistorik og kildebeviser</summary><pre>{JSON.stringify(t.history, null, 2)}</pre></details>
    </>}
    {review && <TransferConfirmation key={`${lifecycle.scope}:${review.action}`} review={review} onClose={() => setReview(null)} onDone={(value, hash) => { if (value) lifecycle.adopt(value); if (hash) setArchiveHash(hash); setMessage(review.action === "map" ? "Kvitteringen er tilknyttet. Ingen postering er udført." : "Handlingen er bekræftet; den viste workspace-tilstand kommer fra denne server."); setReview(null); void lifecycle.refresh(); }} />}
  </section>;
}
function ReceiptMapping({ transfer, onReview, enabled }: { transfer: Transfer; onReview: (r: Review) => void; enabled: boolean }) {
  const auth = useAuth(), [company, setCompany] = useState(""), [reason, setReason] = useState("");
  const state = useAsync(signal => company ? api.receipts(company, { signal }) : Promise.resolve({ rows: [] }), [company]);
  return <section aria-label="Importerede kvitteringer"><h2>Importerede kvitteringer</h2>
    <p>Tilknyt kun en faktisk bevaret kvittering efter gennemgang af den oprindelige kommando og resultatet. Tilknytningen bogfører intet og sender ingen mail. Almindeligt genkald kræver et aktivt mål og nøjagtig oprindelig nøgle og payload.</p>
    <label>Virksomhed til kvitteringer<Select aria-label="Virksomhed til kvitteringer" value={company} onChange={e => { setCompany(e.target.value); setReason(""); }}><option value="">Vælg virksomhed</option>{auth.context?.companies.filter(c => c.role === "owner" && !c.archived).map(c => <option key={c.slug} value={c.slug}>{c.name}</option>)}</Select></label>
    <Button variant="secondary" onClick={state.reload}>Genlæs importerede kvitteringer</Button>
    {state.error && <p role="alert">{state.error}</p>}
    <label>Begrundelse for tilknytning<Textarea aria-label="Begrundelse for tilknytning" value={reason} onChange={e => setReason(e.target.value)} maxLength={2000} /></label>
    {state.data?.rows.map(row => <article key={row.receiptId} aria-label={`Kvittering ${row.receiptId}`}>
      <p>Kvittering {row.receiptId} · {row.operation} · Oprindelig bruger: {row.sourcePrincipal.kind}:{row.sourcePrincipal.subject} · Ny bruger: {auth.session?.id}</p>
      <pre>{JSON.stringify({ ledgerUuid: row.ledgerUuid, keyHash: row.keyHash, payloadHash: row.payloadHash, archiveSha256: row.archiveSha256, restoreBoundaryId: row.restoreBoundaryId, result: row.result, mapping: row.mapping }, null, 2)}</pre>
      {!row.resultAvailable && <p>Det oprindelige resultat er ikke bevaret. Kvitteringen kan ikke tilknyttes.</p>}
      <Button disabled={!enabled || state.loading || Boolean(state.error) || !row.resultAvailable || row.result === null || Boolean(row.mapping) || !transferNote(reason)} onClick={() => onReview({ action: "map", title: "Bekræft tilknytning af kvittering", button: "Tilknyt kvittering til min bruger", body: { receiptId: row.receiptId, expectedStateHash: row.stateHash, reason: reason.trim(), confirm: true }, snapshot: transfer, company, receipt: row })}>Gennemgå tilknytning af kvittering {row.receiptId}</Button>
    </article>)}
  </section>;
}
function TransferConfirmation({ review, onClose, onDone }: { review: Review; onClose: () => void; onDone: (value?: Transfer, hash?: string) => void }) {
  const lifecycle = useWorkspaceTransfer()!, auth = useAuth();
  const marker = useMutationBlock(`${lifecycle.scope}:workspace-transfer:${review.action}`);
  const [confirmed, setConfirmed] = useState(false), [busy, setBusy] = useState(false), [error, setError] = useState("");
  const sending = useRef(false), alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  async function send() {
    if (!confirmed || sending.current || marker.isBlocked()) return;
    sending.current = true; setBusy(true); setError("");
    let attempted = false;
    try {
      const fresh = await api.status();
      if (fresh.stateHash !== review.snapshot.stateHash || fresh.mode !== review.snapshot.mode) throw new ApiError("stale_source", "Workspace-tilstanden er ændret. Genlæs og gennemgå igen.", 422);
      if (review.action === "map") {
        const receipts = await api.receipts(review.company);
        if (!receipts.rows.some(r => r.receiptId === review.receipt.receiptId && r.stateHash === review.receipt.stateHash && r.resultAvailable && !r.mapping)) throw new ApiError("stale_source", "Kvitteringen er ændret eller kan ikke tilknyttes.", 422);
      }
      if (!alive.current) return;
      if (!marker.block()) throw new Error("Browseren kunne ikke gemme udfaldsmarkeringen. Ingen handling blev sendt.");
      attempted = true;
      if (review.action === "export") {
        const blob = await api.export(review.body as Parameters<typeof api.export>[0]);
        const bytes = await blob.arrayBuffer();
        const digest = await crypto.subtle.digest("SHA-256", bytes);
        const hash = Array.from(new Uint8Array(digest), b => b.toString(16).padStart(2, "0")).join("");
        const current = await api.status();
        if (!current.offeredArchiveHashes?.includes(hash)) throw new Error("Arkivets wire-hash kunne ikke bekræftes i kildens tilbud. Kontrollér status; gentag ikke eksporten automatisk.");
        download(blob, "workspace-transfer.native-backup.json"); marker.release(); onDone(current, hash);
      } else if (review.action === "map") {
        const result = await api.mapReceipt(review.company, review.body as Parameters<typeof api.mapReceipt>[1]);
        if (!result.rows.some(r => r.receiptId === review.receipt.receiptId && r.mapping?.targetUserId === auth.session?.id && r.archiveSha256 === review.receipt.archiveSha256 && r.restoreBoundaryId === review.receipt.restoreBoundaryId)) throw new Error("Tilknytningen kunne ikke bekræftes for den gennemgåede kvittering og bruger.");
        marker.release(); onDone();
      } else {
        const value = await api[review.action](review.body as never);
        const expected = { freeze: "frozen", cancel: "active", release: "moved", activate: "active" }[review.action];
        if (value.mode !== expected || !fullTransfer(value)) throw new Error("Serverens kvittering bekræftede ikke den forventede workspace-tilstand.");
        marker.release(); onDone(value);
      }
    } catch (cause) {
      if (cause instanceof ApiError && cause.status >= 400 && cause.status < 500 && cause.status !== 408 && cause.code !== "internal") marker.release();
      else if (!attempted) marker.release();
      setError(cause instanceof Error ? cause.message : "Udfaldet er uafklaret. Kontrollér status før næste handling.");
      void lifecycle.refresh();
    } finally { sending.current = false; setBusy(false); }
  }
  return <Dialog title={review.title} onClose={onClose} busy={busy}>
    <p>Bruger: {auth.session?.id} · Forbindelse: {window.location.origin}</p>
    <p>Gennemgå præcis kilde, mål og denne ene handling. En frigivet kilde kan ikke automatisk åbnes igen; kun målets egen status kan bekræfte aktivering.</p>
    <pre>{JSON.stringify(review.body, null, 2)}</pre>
    {review.action === "map" && <pre>{JSON.stringify(review.receipt, null, 2)}</pre>}
    {error && <p role="alert">{error}</p>}
    {marker.blocked && <UnknownMutationNotice onRefresh={lifecycle.refresh} onRelease={() => { marker.release(); setError(""); }} persistent={marker.persistent} verifyPersistence={marker.verifyPersistence} />}
    <label><input type="checkbox" checked={confirmed} disabled={busy || marker.blocked} onChange={e => setConfirmed(e.target.checked)} />Jeg bekræfter den gennemgåede handling</label>
    <div {...stylex.props(cockpitStyles.modalActions)}><Button variant="secondary" disabled={busy} onClick={onClose}>Tilbage uden handling</Button><Button disabled={busy || !confirmed || marker.blocked} onClick={() => void send()}>{review.button}</Button></div>
  </Dialog>;
}
