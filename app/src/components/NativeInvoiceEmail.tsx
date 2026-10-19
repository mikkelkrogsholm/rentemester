import { useRef, useState } from "react";
import { api } from "../lib/api";
import { ApiError } from "../lib/api/_shared";
import type { InvoiceEmailRow, InvoiceEmailSend, InvoiceEmailRetry, InvoiceEmailReconcile } from "../lib/api/invoice-email";
import type { CompanyInvoiceRow } from "../lib/types";
import { useOptionalAuth } from "../lib/auth-context";
import { useAsync } from "../lib/useAsync";
import { useCapabilities } from "../lib/useCapabilities";
import { useMutationBlock } from "../lib/mutation-memory";
import { Banner } from "./Feedback";
import { UnknownMutationNotice } from "./UnknownMutationNotice";
import { Button, Dialog, Input, Select, Textarea } from "./ui";

type Props = { slug: string; invoices: CompanyInvoiceRow[]; archived: boolean; onChanged: () => void };
type Intent = { action: "send"; body: InvoiceEmailSend; row?: undefined } |
  { action: "retry"; body: InvoiceEmailRetry; row: InvoiceEmailRow } |
  { action: "reconcile"; body: InvoiceEmailReconcile; row: InvoiceEmailRow };
const labels = { accepted: "SMTP-serveren har accepteret mailen", refused: "Mailen blev afvist", unknown: "Udfaldet er uafklaret" };
export function knownInvoiceEmailRefusal(error: unknown) {
  return error instanceof ApiError && error.status >= 400 && error.status < 500 && error.status !== 408 && error.code !== "delivery_unknown" && error.code !== "internal";
}
export function validInvoiceEmailNote(value: string, maximum: number) {
  const trimmed = value.trim(), size = new TextEncoder().encode(trimmed).length;
  return size >= 10 && size <= maximum && !/[\u0000-\u001f\u007f-\u009f]/u.test(trimmed);
}
export function NativeInvoiceEmail(props: Props) {
  const auth = useOptionalAuth();
  const scope = `${window.location.origin}:${props.slug}:${auth?.session?.id ?? "local"}:${auth?.currentSessionId ?? "local"}`;
  return <DeliveryPanel key={scope} {...props} scope={scope} />;
}
function DeliveryPanel({ slug, invoices, archived, onChanged, scope }: Props & { scope: string }) {
  const { can } = useCapabilities(slug);
  const state = useAsync(signal => api.nativeInvoiceEmailStatus(slug, { signal }), [slug]);
  const [selected, setSelected] = useState("");
  const [to, setTo] = useState("");
  const [kind, setKind] = useState<"invoice" | "reminder">("invoice");
  const [opened, setOpened] = useState(false);
  const [reviewed, setReviewed] = useState(false);
  const [intent, setIntent] = useState<Intent | null>(null);
  const [message, setMessage] = useState("");
  const [actionRow, setActionRow] = useState<InvoiceEmailRow | null>(null);
  const [action, setAction] = useState<"retry" | "reconcile">("retry");
  const [reason, setReason] = useState("");
  const [evidence, setEvidence] = useState("");
  const [resolution, setResolution] = useState<"accepted" | "not_accepted">("not_accepted");
  const invoice = invoices.find(i => String(i.documentId) === selected);
  const maySend = !archived && can("company.external-send") && state.data?.configured === true;
  const mayReconcile = !archived && can("company.admin");
  const current = state.data && !state.loading && !state.error;
  function prepareSend() {
    if (!current || !maySend || !invoice || !reviewed || !opened) return;
    const recipient = to.trim();
    if (!recipient || recipient.length > 254 || (!/^[\x21-\x7e]+$/.test(recipient) || !/^[^@<>,;]+@[^@<>,;]+$/.test(recipient))) { setMessage("Angiv én gyldig modtageradresse."); return; }
    setIntent({ action: "send", body: Object.freeze({ invoiceDocumentId: invoice.documentId, to: recipient, kind, confirm: true }) });
  }
  function prepareAction() {
    if (!actionRow || !current) return;
    if (!state.data?.rows.some(r => r.messageId === actionRow.messageId && r.stateHash === actionRow.stateHash && r.state === actionRow.state)) { setMessage("Leveringsstatus er ændret. Vælg det aktuelle forsøg igen."); setActionRow(null); return; }
    if (!validInvoiceEmailNote(reason, 2000) || (action === "reconcile" && !validInvoiceEmailNote(evidence, 4000))) { setMessage("Angiv en konkret begrundelse på mindst 10 tegn i én linje og ved afklaring en reference til SMTP-loggens dokumentation."); return; }
    const common = { messageId: actionRow.messageId, expectedStateHash: actionRow.stateHash, reason: reason.trim(), confirm: true as const };
    if (action === "retry" && maySend && actionRow.state === "refused" && opened && reviewed) setIntent({ action, row: actionRow, body: Object.freeze(common) });
    if (action === "reconcile" && mayReconcile && actionRow.state === "unknown") setIntent({ action, row: actionRow, body: Object.freeze({ ...common, resolution, evidence: evidence.trim() }) });
  }
  function chooseRow(row: InvoiceEmailRow, mode: "retry" | "reconcile") {
    setActionRow(row); setAction(mode); setReason(""); setEvidence(""); setOpened(false); setReviewed(false); setMessage("");
  }
  return <section aria-label="Maillevering" className="statement">
    <h2>Maillevering</h2>
    <p>Udstedelse, bogføring og mail er separate handlinger. SMTP-accept beviser ikke, at modtageren har læst eller modtaget mailen i sin indbakke.</p>
    <Button variant="secondary" onClick={state.reload}>Genlæs leveringsstatus</Button>
    {state.loading && <p>Henter leveringsstatus…</p>}
    {state.error && <Banner kind="error">Leveringsstatus kunne ikke bekræftes: {state.error}</Banner>}
    {message && <Banner kind="warning">{message}</Banner>}
    {state.data?.configured === false && <p>Mailafsendelse er ikke konfigureret for virksomheden. En administrator skal konfigurere serverens mailforbindelse. Historik og ejerens afklaring er stadig tilgængelige.</p>}
    {current && <>
      {maySend && <div>
        <label>Faktura til mail<Select aria-label="Faktura til mail" value={selected} onChange={e => { setSelected(e.target.value); setTo(invoices.find(i => String(i.documentId) === e.target.value)?.customerEmail ?? ""); setOpened(false); setReviewed(false); }}><option value="">Vælg udstedt faktura</option>{invoices.map(i => <option key={i.documentId} value={i.documentId}>{i.invoiceNo} — {i.customerName ?? "Kunde"}</option>)}</Select></label>
        <label>Modtagerens e-mail<Input type="email" maxLength={254} value={to} onChange={e => { setTo(e.target.value); setReviewed(false); }} /></label>
        <label>Mailtype<Select aria-label="Mailtype" value={kind} onChange={e => { setKind(e.target.value as "invoice" | "reminder"); setReviewed(false); }}><option value="invoice">Faktura</option><option value="reminder">Påmindelse uden gebyr eller postering</option></Select></label>
        {invoice && <><a className="btn secondary" target="_blank" rel="noopener" href={api.invoicePdfUrl(slug, invoice.documentId)} onClick={() => setOpened(true)}>Åbn fakturaens gemte PDF</a><label><input type="checkbox" checked={reviewed} disabled={!opened} onChange={e => setReviewed(e.target.checked)} />Jeg har gennemgået modtageren og den gemte PDF</label></>}
        <Button disabled={!invoice || !reviewed || !opened} onClick={prepareSend}>Gennemgå mailafsendelse</Button>
      </div>}
      <h3>Leveringshistorik</h3>
      {!state.data?.rows.length && <p>Ingen registrerede mailforsøg.</p>}
      {state.data?.rows.map(row => <article key={row.messageId} aria-label={`Mail ${row.invoiceNumber} til ${row.recipient}`}>
        <p><strong>{row.invoiceNumber}</strong> · {row.recipient} · {row.kind === "invoice" ? "Faktura" : "Påmindelse"} · {labels[row.state]}</p>
        <p>{row.subject}</p>
        {row.pdfSha256 === null && <p>Historisk accepteret afsendelse uden bevaret PDF-hash. Historikken beviser ikke en bestemt PDF-version.</p>}
        <details><summary>Vis historik og dokumentation</summary><pre>{JSON.stringify(row.history, null, 2)}</pre>{row.pdfSha256 !== null && <p>PDF SHA256: {row.pdfSha256}</p>}</details>
        {row.state === "unknown" && <p>Send ikke igen. En ejer skal kontrollere SMTP-loggen og registrere en særskilt afklaring.</p>}
        {row.state === "unknown" && mayReconcile && <Button variant="secondary" onClick={() => chooseRow(row, "reconcile")}>Gennemgå afklaring</Button>}
        {row.state === "refused" && maySend && <Button variant="secondary" onClick={() => chooseRow(row, "retry")}>Gennemgå nyt forsøg</Button>}
      </article>)}
      {actionRow && <div role="group" aria-label={action === "retry" ? "Nyt mailforsøg" : "Afklar mailforsøg"}>
        <p>{actionRow.invoiceNumber} · {actionRow.recipient} · {labels[actionRow.state]}</p>
        <label>Begrundelse<Textarea value={reason} minLength={10} maxLength={2000} onChange={e => setReason(e.target.value)} /></label>
        {action === "reconcile" ? <>
          <label>Kontrolleret udfald<Select aria-label="Kontrolleret udfald" value={resolution} onChange={e => setResolution(e.target.value as "accepted" | "not_accepted")}><option value="not_accepted">SMTP-serveren accepterede ikke mailen</option><option value="accepted">SMTP-serveren accepterede mailen</option></Select></label>
          <label>Dokumentation fra SMTP-log<Textarea value={evidence} minLength={10} maxLength={4000} placeholder="Reference til den kontrollerede SMTP-log og dens resultat" onChange={e => setEvidence(e.target.value)} /></label>
          <p>Afklaringen er din dokumenterede erklæring. Den sender ingen mail. Et nyt forsøg kræver derefter en særskilt bekræftelse.</p>
        </> : <><a className="btn secondary" target="_blank" rel="noopener" href={api.invoicePdfUrl(slug, actionRow.invoiceDocumentId)} onClick={() => setOpened(true)}>Åbn fakturaens gemte PDF før nyt forsøg</a><label><input type="checkbox" disabled={!opened} checked={reviewed} onChange={e => setReviewed(e.target.checked)} />Jeg har gennemgået den faste modtager og PDF til nyt forsøg</label></>}
        <Button onClick={prepareAction} disabled={!reason.trim() || (action === "retry" && (!opened || !reviewed)) || (action === "reconcile" && !evidence.trim())}>Gennemgå {action === "retry" ? "det nye forsøg" : "afklaringen"}</Button>
        <Button variant="secondary" onClick={() => setActionRow(null)}>Annullér ændring</Button>
      </div>}
    </>}
    {intent && <DeliveryReview key={`${intent.action}:${"messageId" in intent.body ? intent.body.messageId : intent.body.invoiceDocumentId}`} scope={scope} slug={slug} allowed={intent.action === "reconcile" ? mayReconcile : maySend} intent={intent} onClose={() => setIntent(null)} onResult={text => { setMessage(text); setActionRow(null); state.reload(); onChanged(); }} onRefresh={state.reload} />}
  </section>;
}
function DeliveryReview({ scope, slug, allowed, intent, onClose, onResult, onRefresh }: { scope: string; slug: string; allowed: boolean; intent: Intent; onClose: () => void; onResult: (message: string) => void; onRefresh: () => void }) {
  const marker = useMutationBlock(`${scope}:invoice-mail:${intent.action}:${"messageId" in intent.body ? intent.body.messageId : intent.body.invoiceDocumentId}`);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const attempted = useRef(false);
  async function send() {
    if (attempted.current || marker.isBlocked() || !confirmed || !allowed) return;
    attempted.current = true; setBusy(true); setError("");
    try {
      const fresh = await api.nativeInvoiceEmailStatus(slug);
      if (intent.action !== "reconcile" && !fresh.configured) throw new ApiError("not_configured", "Mailafsendelse er ikke konfigureret.", 422);
      if (intent.row && !fresh.rows.some(r => r.messageId === intent.row?.messageId && r.stateHash === intent.row.stateHash && r.state === intent.row.state)) throw new ApiError("stale_source", "Leveringsstatus er ændret. Genlæs og gennemgå igen.", 422);
      if (intent.action === "send" && fresh.rows.some(r => r.invoiceDocumentId === intent.body.invoiceDocumentId && r.recipient === intent.body.to && r.kind === intent.body.kind)) throw new ApiError("existing_delivery", "Der findes allerede et registreret forsøg. Brug dets aktuelle historik og eventuelle særskilte afklaring eller nyt forsøg.", 422);
      if (!marker.block()) throw new Error("Browseren kunne ikke bevare markeringen før afsendelse. Ingen anmodning blev sendt.");
      const row = intent.action === "send" ? await api.nativeInvoiceEmailSend(slug, intent.body) : intent.action === "retry" ? await api.nativeInvoiceEmailRetry(slug, intent.body) : await api.nativeInvoiceEmailReconcile(slug, intent.body);
      if (row.invoiceDocumentId !== (intent.action === "send" ? intent.body.invoiceDocumentId : intent.row.invoiceDocumentId) || (intent.row && row.messageId !== intent.row.messageId) || (intent.action === "send" && (row.recipient !== intent.body.to || row.kind !== intent.body.kind))) throw new Error("Kvitteringen matcher ikke den gennemgåede handling. Udfaldet er uafklaret.");
      const expectedState = intent.action === "reconcile" && intent.body.resolution === "not_accepted" ? "refused" : "accepted";
      if (row.state !== expectedState) throw new Error("Kvitteringens udfald matcher ikke handlingen. Kontrollér leveringsstatus; gentag ikke.");
      marker.release(); onResult(`${labels[row.state]}. Kanonisk leveringsstatus genlæses. Ingen automatisk gentagelse.`); onClose();
    } catch (cause) {
      if (knownInvoiceEmailRefusal(cause)) {
        marker.release(); onResult(cause instanceof Error ? cause.message : "Mailhandlingen blev afvist."); onClose();
      } else {
        setError(cause instanceof Error ? cause.message : "Udfaldet er uafklaret. Kontrollér leveringsstatus før næste handling."); onRefresh();
      }
    } finally { attempted.current = false; setBusy(false); }
  }
  return <Dialog title={intent.action === "send" ? "Bekræft mailafsendelse" : intent.action === "retry" ? "Bekræft nyt mailforsøg" : "Bekræft afklaring uden afsendelse"} busy={busy} onClose={onClose}>
    <p>Gennemgå denne ene handling. Ingen automatisk gentagelse. Udstedelse og bogføring ændres ikke.</p>
    {intent.action === "send" ? <p>Fakturabilag {intent.body.invoiceDocumentId} · Modtager: {intent.body.to} · {intent.body.kind === "invoice" ? "Faktura" : "Påmindelse"} · Fakturaens gemte PDF vedhæftes.</p> : <><p>{intent.row.invoiceNumber} · {intent.row.recipient} · {labels[intent.row.state]}</p><p>Begrundelse: {intent.body.reason}</p>{intent.action === "reconcile" && <><p>Udfald: {intent.body.resolution === "accepted" ? "Accepteret" : "Ikke accepteret"}</p><p>SMTP-log: {intent.body.evidence}</p><p>Ingen mail sendes under afklaringen.</p></>}</>}
    {error && <Banner kind="error">{error}</Banner>}
    {marker.blocked && <UnknownMutationNotice onRefresh={onRefresh} onRelease={() => { marker.release(); setError(""); }} persistent={marker.persistent} verifyPersistence={marker.verifyPersistence} />}
    <label><input type="checkbox" checked={confirmed} disabled={busy || marker.blocked} onChange={e => setConfirmed(e.target.checked)} />Jeg bekræfter den gennemgåede handling</label>
    <div className="modal-actions"><Button variant="secondary" disabled={busy} onClick={onClose}>Tilbage uden afsendelse</Button><Button disabled={busy || !confirmed || marker.blocked || !allowed} onClick={() => void send()}>{intent.action === "reconcile" ? "Registrér afklaring" : "Send gennemgået mail én gang"}</Button></div>
  </Dialog>;
}
