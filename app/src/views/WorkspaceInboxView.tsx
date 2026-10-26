import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useRef, useState } from "react";
import { useParams, Link } from "react-router-dom";
import { Button, PageHeader, Select } from "../components/ui";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { DocumentIngestModal } from "../components/DocumentIngestModal";
import { useAsync } from "../lib/useAsync";
import { api, ApiError } from "../lib/api";
import { workspaceInboxApi, type InboxSource } from "../lib/api/workspace-inbox";
import { useMutationOutcome } from "../lib/useMutationOutcome";

type Review = { source: InboxSource; companySlug: string; action: "assign" | "complete" };
const container = (source: InboxSource) => source.mail?.role === "eml" || source.mail?.role === "refusal";
const hasMetadata = (source: InboxSource, target: string) => !source.mail || (source.mail.role === "attachment" && source.metadataReview?.companySlug === target && source.metadataReview.sourceHash === source.sha256);
export function WorkspaceInboxView() {
  const { slug = "" } = useParams();
  return <CompanyInbox key={slug} slug={slug} />;
}
/** The route company grants visibility; only an explicit reviewed target receives a document. */
function CompanyInbox({ slug }: { slug: string }) {
  const [cursor, setCursor] = useState(0);
  const data = useAsync(signal => workspaceInboxApi.list(slug, { signal }, cursor), [slug, cursor]);
  const companies = useAsync(signal => api.companies({ signal }), []);
  const [target, setTarget] = useState("");
  const [upload, setUpload] = useState(false);
  const [metadataSource, setMetadataSource] = useState<InboxSource>();
  const [review, setReview] = useState<Review>();
  const [message, setMessage] = useState<string>();
  const ingestIdentity = useRef<{ key: string; receivedAt: string }>();
  const outcome = useMutationOutcome(() => data.reload());
  const reload = () => data.reload();
  useEffect(() => { setReview(undefined); setMetadataSource(undefined); }, [target]);
  function inspect(source: InboxSource, action: Review["action"]) {
    if (!target) { setMessage("Vælg målvirksomheden efter gennemgang."); return; }
    if (container(source) || !hasMetadata(source, target)) { setMessage("Gennemgå først vedhæftningens kildeoplysninger for den valgte målvirksomhed."); return; }
    setReview({ source, companySlug: target, action });
  }
  async function apply() {
    if (!review || outcome.isBlocked()) return;
    if (review.companySlug !== target || container(review.source) || !hasMetadata(review.source, target)) throw new Error("Målet eller kildegennemgangen er ændret. Gennemgå igen.");
    await outcome.run(() => workspaceInboxApi[review.action](slug, review.source.sourceId, review.companySlug));
    setMessage(review.action === "assign" ? "Ruting er godkendt. Handoff er stadig et separat trin." : "Kilden er overdraget én gang til virksomhedens dokumentflow.");
    reload();
  }
  if (data.loading && !data.data) return <Loading label="Henter workspace-indbakke…" />;
  if (data.error) return <ErrorState message={data.error} onRetry={reload} />;
  const targetName = companies.data?.find(c => c.slug === target)?.name ?? target;
  return <section data-cockpit-page="workspace-inbox" data-evidence-issue="655" {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible, cockpitStyles.element)}>
    {outcome.feedback}
    <PageHeader title="Fælles dokumentindbakke" actions={<Button variant="secondary" type="button" onClick={reload} xstyle={[cockpitStyles.buttonComposition]}>Opdater</Button>}>
      <div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}><p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible, cockpitStyles.muted)}>Workspace</p><p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible, cockpitStyles.muted)}>Upload og godkendt ruting ændrer ingen hovedbog. Mailvedhæftninger kræver gennemgåede kildeoplysninger for det valgte mål. Ved bekræftet handoff oprettes ét ubogført bilag.</p></div>
    </PageHeader>
    <section {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible, cockpitStyles.card)}><h3 {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible, cockpitStyles.h3)}>Indlæs kilde</h3><p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Vælg faktisk bilagstype og kildeoplysninger. Fakturaer og kasseboner understøttes; interne bilag oprettes i virksomhedens eget dokumentflow.</p>
      <Button requiredPermission="company.documents.upload" disabled={outcome.blocked} onClick={() => { ingestIdentity.current = undefined; setUpload(true); }} xstyle={[cockpitStyles.buttonComposition]}>Indlæs til indbakke</Button>
    </section>
    <section {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible, cockpitStyles.card)}><h3 {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible, cockpitStyles.h3)}>Review og overdragelse</h3>
      <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Målvirksomhed efter gennemgang<Select aria-label="Inbox målvirksomhed" disabled={outcome.blocked} value={target} onChange={e => setTarget(e.target.value)} xstyle={[cockpitStyles.selectComposition]}><option value="" {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Vælg virksomhed</option>{companies.data?.filter(c => !c.archived).map(c => <option key={c.slug} value={c.slug} {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>{c.name}</option>)}</Select></label>
      {companies.error && <Banner kind="error">Målvirksomheder kunne ikke hentes. Opdater siden før ruting.</Banner>}
      {data.data?.rows.length ? <ul {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>{data.data.rows.map(source => {
        const approved = source.assignments.find(a => a.state === "approved");
        const completed = source.assignments.find(a => a.state === "completed");
        const claimed = source.handoffs?.some(h => h.state === "claimed");
        const metadata = source.mail ? source.metadataReview?.metadata : source.metadata;
        const ready = hasMetadata(source, target);
        return <li key={source.sourceId} {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}><article aria-label={`Kilde ${source.sourceId}`} {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
          <strong {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>{source.filename}</strong><p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible, cockpitStyles.muted)}>{source.transport}{source.mail ? ` · ${source.mail.role === "attachment" ? "Mailvedhæftning" : source.mail.role === "eml" ? "Original mail" : "Modtagelsesafvisning"}` : ""} · {metadata?.documentType ?? "Bilagstype mangler"} · {completed ? "Overdraget" : approved ? "Ruting godkendt" : source.exception?.code ?? "Kræver review"}</p>
          <a {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible, )} href={workspaceInboxApi.originalUrl(slug, source.sourceId)} download>Hent original {source.filename}</a>
          <details {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}><summary {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible, cockpitStyles.summaryComposition)}>Kilde og historik: {source.filename}</summary><p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>SHA-256: {source.sha256}</p><p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Modtaget: {source.receivedAt}</p>{source.mail && <p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Message-ID: {source.mail.messageId ?? "Ikke tilgængeligt"} · EML-kilde: {source.mail.emlSourceId ?? "Ikke tilgængelig"}</p>}<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Bilagstype: {metadata?.documentType ?? "Mangler"} · Faktura: {metadata?.invoiceNo ?? "Ikke angivet"} · Dato: {metadata?.issueDate ?? "Ikke angivet"}</p><p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Afsender: {metadata?.sender?.name ?? "Ikke angivet"} · Modtager: {metadata?.recipient?.name ?? "Ikke angivet"} · Beløb: {metadata?.amountIncVat ?? "Ikke angivet"} {metadata?.currency ?? ""}</p>{source.history && <ol {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>{source.history.map((e, n) => <li key={n} {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>{e.event_type} · {e.created_at} · {e.actor}</li>)}</ol>}</details>
          {source.metadataReview && <section aria-label={`Gennemgåede kildeoplysninger ${source.sourceId}`} {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}><p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Gennemgået til {source.metadataReview.companySlug} · ledger {source.metadataReview.ledgerUuid}</p><p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Gennemgået af {source.metadataReview.reviewedBy} · {source.metadataReview.reviewedAt}</p><p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Kildehash: {source.metadataReview.sourceHash} · Metadatahash: {source.metadataReview.metadataHash}</p>{!ready && <p role="status" {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Gennemgangen gælder ikke det valgte mål og den aktuelle kilde. Gennemgå igen før ruting.</p>}</section>}
          {source.assignments.map(a => <p key={a.companySlug} {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Mål: {companies.data?.find(c => c.slug === a.companySlug)?.name ?? a.companySlug} · {a.state}{a.documentId != null && <> · <Link to={`/companies/${a.companySlug}/bilag?documentId=${a.documentId}`}>{a.documentNo ?? "Åbn bilag"}</Link></>}</p>)}
          {source.handoffs?.filter(h => h.state === "claimed").map(h => <p key={h.companySlug} role="status" {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Et handoff er startet. Kontrollér målvirksomhedens bilag før genoptagelse. Kravet kan genoptages efter {h.leaseExpiresAt}; serveren afgør, hvornår det er udløbet.</p>)}
          {container(source) ? <p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Mailcontaineren eller afvisningskvitteringen bevares som kilde og kan kun hentes. Kun en understøttet vedhæftning kan gennemgås og overdrages som bilag.</p> : !completed && <div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible, cockpitStyles.modalActions)}>
            {source.mail?.role === "attachment" && <Button requiredPermission="company.documents.upload" disabled={outcome.blocked || !target || Boolean(approved) || Boolean(claimed)} variant="secondary" onClick={() => setMetadataSource(source)} xstyle={[cockpitStyles.buttonComposition]}>Gennemgå kildeoplysninger</Button>}
            <Button requiredPermission="company.documents.upload" disabled={outcome.blocked || Boolean(approved) || !ready} variant="secondary" onClick={() => inspect(source, "assign")} xstyle={[cockpitStyles.buttonComposition]}>Godkend ruting</Button>
            <Button requiredPermission="company.documents.upload" disabled={outcome.blocked || !ready || !approved || approved.companySlug !== target} variant="secondary" onClick={() => inspect(source, "complete")} xstyle={[cockpitStyles.buttonComposition]}>Fuldfør handoff</Button>
          </div>}
        </article></li>;
      })}</ul> : <p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible, cockpitStyles.muted)}>Ingen synlige kilder.</p>}
      <nav aria-label="Indbakkesider" {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}><Button variant="secondary" disabled={cursor === 0 || data.loading} onClick={() => setCursor(Math.max(0, cursor - 25))} xstyle={[cockpitStyles.buttonComposition]}>Forrige side</Button><Button variant="secondary" disabled={data.data?.nextCursor == null || data.loading} onClick={() => setCursor(data.data?.nextCursor ?? cursor)} xstyle={[cockpitStyles.buttonComposition]}>Næste side</Button></nav>
    </section>
    {message && <Banner kind="success">{message}</Banner>}
    {upload && <DocumentIngestModal slug={slug} title="Indlæs kilde til fælles indbakke" allowInternalVoucher={false} successText="Kilden er gemt i den fælles indbakke. Gennemgå målvirksomheden og godkend ruting; intet er bogført." onClose={() => setUpload(false)} onIngested={reload} submit={async (input, mimeType) => {
      if (!ingestIdentity.current) ingestIdentity.current = { key: crypto.randomUUID(), receivedAt: new Date().toISOString() };
      await workspaceInboxApi.ingest(slug, { idempotencyKey: ingestIdentity.current.key, receivedAt: ingestIdentity.current.receivedAt, bytesBase64: input.fileBase64, filename: input.fileName, mimeType, metadata: input.metadata, transport: "upload", candidates: [] });
      return { documentNo: null };
    }} />}
    {metadataSource && target && <DocumentIngestModal key={`${slug}:${metadataSource.sourceId}:${target}:${metadataSource.sha256}`} slug={target} title="Gennemgå mailbilag" allowInternalVoucher={false} successText="Kildeoplysninger og ruting er godkendt for det gennemgåede mål. Fuldfør handoff separat; intet er bogført." onClose={() => setMetadataSource(undefined)} onIngested={reload} metadataReview={{ filename: `${metadataSource.filename} → ${targetName}`, source: `workspace-inbox:${metadataSource.sourceId}`, sourceHash: metadataSource.sha256, originalUrl: workspaceInboxApi.originalUrl(slug, metadataSource.sourceId), metadata: hasMetadata(metadataSource, target) ? metadataSource.metadataReview?.metadata : undefined, save: async metadata => {
      const current = (await workspaceInboxApi.inspect(slug, metadataSource.sourceId).catch(() => { throw new Error("Kilden kunne ikke genlæses. Ingen gennemgang er sendt; opdater indbakken før du prøver igen."); })).source;
      if (current.sha256 !== metadataSource.sha256 || current.mail?.role !== "attachment" || current.assignments.some(a => a.state === "completed") || current.handoffs?.some(h => h.state === "claimed")) throw new Error("Kilden eller dens handoff er ændret. Opdater indbakken og gennemgå igen.");
      const updated = (await workspaceInboxApi.review(slug, current.sourceId, target, current.sha256, metadata)).source;
      const latest = (await workspaceInboxApi.inspect(slug, current.sourceId).catch(() => { throw new ApiError("internal", "Gennemgangen er sendt, men det gemte udfald kunne ikke genlæses. Kontrollér kilden før en ny handling.", 0); })).source;
      if (!updated || !hasMetadata(updated, target) || !hasMetadata(latest, target) || latest.sourceId !== current.sourceId || latest.sha256 !== current.sha256 || latest.metadataReview?.metadataHash !== updated.metadataReview?.metadataHash || !latest.assignments.some(a => a.companySlug === target && a.state === "approved")) throw new ApiError("internal", "Gennemgangens gemte udfald kunne ikke bekræftes. Kontrollér kilden og målvirksomheden før en ny handling.", 0);
    } }} />}
    {review && <ConfirmDialog title={review.action === "assign" ? "Bekræft ruting" : "Bekræft handoff"} operationKey={`${review.source.sourceId}:${review.companySlug}`} body={<><p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>{review.source.filename} → {companies.data?.find(c => c.slug === review.companySlug)?.name ?? review.companySlug}</p><p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>SHA-256: {review.source.sha256}</p><p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>{review.action === "assign" ? "Dette låser det godkendte mål. Filen er endnu ikke overdraget." : "Dette opretter ét ubogført bilag med den oprindelige fil og dine kildeoplysninger i målvirksomheden. Det ændrer ingen journal."}</p></>} confirmLabel={review.action === "assign" ? "Bekræft ruting" : "Bekræft handoff"} onClose={() => setReview(undefined)} onConfirm={apply} onRefresh={reload} />}
  </section>;
}
