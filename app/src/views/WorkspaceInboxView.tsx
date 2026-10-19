import { useEffect, useRef, useState } from "react";
import { useParams, Link } from "react-router-dom";
import { Button, PageHeader, Select } from "../components/ui";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { DocumentIngestModal } from "../components/DocumentIngestModal";
import { useAsync } from "../lib/useAsync";
import { api } from "../lib/api";
import { workspaceInboxApi, type InboxSource } from "../lib/api/workspace-inbox";
import { useMutationOutcome } from "../lib/useMutationOutcome";

type Review = { source: InboxSource; companySlug: string; action: "assign" | "complete" };
/** The route company grants visibility; only an explicit reviewed target receives a document. */
export function WorkspaceInboxView() {
  const { slug = "" } = useParams();
  const [cursor, setCursor] = useState(0);
  const data = useAsync(signal => workspaceInboxApi.list(slug, { signal }, cursor), [slug, cursor]);
  useEffect(() => setCursor(0), [slug]);
  const companies = useAsync(signal => api.companies({ signal }), []);
  const [target, setTarget] = useState("");
  const [upload, setUpload] = useState(false);
  const [review, setReview] = useState<Review>();
  const [message, setMessage] = useState<string>();
  const ingestIdentity = useRef<{ key: string; receivedAt: string }>();
  const outcome = useMutationOutcome(() => data.reload());
  const reload = () => data.reload();
  function inspect(source: InboxSource, action: Review["action"]) {
    if (!target) { setMessage("Vælg målvirksomheden efter gennemgang."); return; }
    setReview({ source, companySlug: target, action });
  }
  async function apply() {
    if (!review || outcome.isBlocked()) return;
    await workspaceInboxApi[review.action](slug, review.source.sourceId, review.companySlug).catch(outcome.reject);
    setMessage(review.action === "assign" ? "Ruting er godkendt. Handoff er stadig et separat trin." : "Kilden er overdraget én gang til virksomhedens dokumentflow.");
    reload();
  }
  if (data.loading && !data.data) return <Loading label="Henter workspace-indbakke…" />;
  if (data.error) return <ErrorState message={data.error} onRetry={reload} />;
  return <section className="page">
    {outcome.feedback}
    <PageHeader title="Fælles dokumentindbakke" actions={<Button variant="secondary" type="button" onClick={reload}>Opdater</Button>}>
      <div><p className="eyebrow">Workspace</p><p className="muted">Upload og godkendt ruting ændrer ingen hovedbog. Først ved bekræftet handoff oprettes ét bilag i den valgte virksomhed; det bogføres ikke.</p></div>
    </PageHeader>
    <section className="card"><h3>Indlæs kilde</h3><p>Vælg faktisk bilagstype og kildeoplysninger. Fakturaer og kasseboner understøttes; interne bilag oprettes i virksomhedens eget dokumentflow.</p>
      <Button requiredPermission="company.documents.upload" disabled={outcome.blocked} onClick={() => { ingestIdentity.current = undefined; setUpload(true); }}>Indlæs til indbakke</Button>
    </section>
    <section className="card"><h3>Review og overdragelse</h3>
      <label>Målvirksomhed efter gennemgang<Select aria-label="Inbox målvirksomhed" disabled={outcome.blocked} value={target} onChange={e => setTarget(e.target.value)}><option value="">Vælg virksomhed</option>{companies.data?.filter(c => !c.archived).map(c => <option key={c.slug} value={c.slug}>{c.name}</option>)}</Select></label>
      {companies.error && <Banner kind="error">Målvirksomheder kunne ikke hentes. Opdater siden før ruting.</Banner>}
      {data.data?.rows.length ? <ul>{data.data.rows.map(source => {
        const approved = source.assignments.find(a => a.state === "approved");
        const completed = source.assignments.find(a => a.state === "completed");
        return <li key={source.sourceId}>
          <strong>{source.filename}</strong><p className="muted">{source.transport} · {source.metadata?.documentType ?? "Bilagstype mangler"} · {completed ? "Overdraget" : approved ? "Ruting godkendt" : source.exception?.code ?? "Kræver review"}</p>
          <details><summary>Kilde og historik: {source.filename}</summary><p>SHA-256: {source.sha256}</p><p>Modtaget: {source.receivedAt}</p><p>Bilagstype: {source.metadata?.documentType ?? "Mangler"} · Faktura: {source.metadata?.invoiceNo ?? "Ikke angivet"} · Dato: {source.metadata?.issueDate ?? "Ikke angivet"}</p><p>Afsender: {source.metadata?.sender?.name ?? "Ikke angivet"} · Modtager: {source.metadata?.recipient?.name ?? "Ikke angivet"} · Beløb: {source.metadata?.amountIncVat ?? "Ikke angivet"} {source.metadata?.currency ?? ""}</p>{source.history && <ol>{source.history.map((e, n) => <li key={n}>{e.event_type} · {e.created_at} · {e.actor}</li>)}</ol>}</details>
          {source.assignments.map(a => <p key={a.companySlug}>Mål: {companies.data?.find(c => c.slug === a.companySlug)?.name ?? a.companySlug} · {a.state}{a.documentId != null && <> · <Link to={`/companies/${a.companySlug}/bilag?documentId=${a.documentId}`}>{a.documentNo ?? "Åbn bilag"}</Link></>}</p>)}
          {source.handoffs?.filter(h => h.state === "claimed").map(h => <p key={h.companySlug} role="status">Et handoff er startet. Kontrollér målvirksomhedens bilag før genoptagelse. Kravet kan genoptages efter {h.leaseExpiresAt}; serveren afgør, hvornår det er udløbet.</p>)}
          {!completed && <div className="modal-actions">
            <Button requiredPermission="company.documents.upload" disabled={outcome.blocked || Boolean(approved)} variant="secondary" onClick={() => inspect(source, "assign")}>Godkend ruting</Button>
            <Button requiredPermission="company.documents.upload" disabled={outcome.blocked || !approved || approved.companySlug !== target} variant="secondary" onClick={() => inspect(source, "complete")}>Fuldfør handoff</Button>
          </div>}
        </li>;
      })}</ul> : <p className="muted">Ingen synlige kilder.</p>}
      <nav aria-label="Indbakkesider"><Button variant="secondary" disabled={cursor === 0 || data.loading} onClick={() => setCursor(Math.max(0, cursor - 25))}>Forrige side</Button><Button variant="secondary" disabled={data.data?.nextCursor == null || data.loading} onClick={() => setCursor(data.data?.nextCursor ?? cursor)}>Næste side</Button></nav>
    </section>
    {message && <Banner kind="success">{message}</Banner>}
    {upload && <DocumentIngestModal slug={slug} title="Indlæs kilde til fælles indbakke" allowInternalVoucher={false} successText="Kilden er gemt i den fælles indbakke. Gennemgå målvirksomheden og godkend ruting; intet er bogført." onClose={() => setUpload(false)} onIngested={reload} submit={async (input, mimeType) => {
      if (!ingestIdentity.current) ingestIdentity.current = { key: crypto.randomUUID(), receivedAt: new Date().toISOString() };
      await workspaceInboxApi.ingest(slug, { idempotencyKey: ingestIdentity.current.key, receivedAt: ingestIdentity.current.receivedAt, bytesBase64: input.fileBase64, filename: input.fileName, mimeType, metadata: input.metadata, transport: "upload", candidates: [] });
      return { documentNo: null };
    }} />}
    {review && <ConfirmDialog title={review.action === "assign" ? "Bekræft ruting" : "Bekræft handoff"} operationKey={`${review.source.sourceId}:${review.companySlug}`} body={<><p>{review.source.filename} → {companies.data?.find(c => c.slug === review.companySlug)?.name ?? review.companySlug}</p><p>SHA-256: {review.source.sha256}</p><p>{review.action === "assign" ? "Dette låser det godkendte mål. Filen er endnu ikke overdraget." : "Dette opretter ét ubogført bilag med den oprindelige fil og dine kildeoplysninger i målvirksomheden. Det ændrer ingen journal."}</p></>} confirmLabel={review.action === "assign" ? "Bekræft ruting" : "Bekræft handoff"} onClose={() => setReview(undefined)} onConfirm={apply} onRefresh={reload} />}
  </section>;
}
