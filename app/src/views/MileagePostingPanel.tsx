import { useState } from "react";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { useOptionalAuth } from "../lib/auth-context";
import { useCapabilities } from "../lib/useCapabilities";
import { useMutationBlock } from "../lib/mutation-memory";
import { Button, Field, Input, Select } from "../components/ui";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ErrorState, Loading } from "../components/Feedback";
import { formatKroner } from "../lib/format";
import type { MileageEntryRow, MileagePostPreview, MileagePostResult, MileageRateInput } from "../lib/types/mileage";

type Props = { slug: string; entries: MileageEntryRow[]; disabled?: boolean };
export function MileagePostingPanel(props: Props) {
  const auth = useOptionalAuth();
  const { can, archived } = useCapabilities(props.slug);
  if (auth?.backend !== "rust" || props.disabled || archived || !can("company.read")) return null;
  return <Panel key={`${props.slug}:${auth.currentSessionId}:${auth.session?.id}`} {...props} />;
}
function Panel({ slug, entries }: Props) {
  const [epoch, setEpoch] = useState(0);
  const refresh = () => setEpoch(e => e + 1);
  const state = useAsync(async signal => {
    const [rates, receipts] = await Promise.all([api.mileageRateVersions(slug, { signal }), api.mileagePostings(slug, { signal })]);
    return { ...rates, ...receipts };
  }, [slug, epoch]);
  const [trip, setTrip] = useState("");
  const [version, setVersion] = useState("");
  const [doc, setDoc] = useState("");
  const rate = state.data?.rateVersions.find(r => r.version === version);
  return <section aria-label="Kørselsbogføring" className="card"><h2>Kørselsbogføring</h2>
    <p>Registrering er kun dokumentation indtil særskilt bogføring efter serverens forhåndsvisning. Konfigureret takst er ikke skatteberettigelse eller automatisk udbetaling. Ingen moms eller betaling oprettes.</p>
    <Button onClick={refresh}>Opdatér bogføringsstatus</Button>
    {state.loading ? <Loading /> : state.error ? <ErrorState message={state.error} onRetry={refresh} /> : state.data && <>
      {state.data.postings.map(p => <Receipt key={p.mileageEntryId} p={p} />)}
      <Field label="Kørsel"><Select value={trip} onChange={e => setTrip(e.target.value)}><option value="">Vælg kørsel</option>{entries.map(e => <option key={e.id} value={e.id}>{e.entryNo} · {e.tripDate} · {e.purpose}</option>)}</Select></Field>
      <Field label="Takstversion"><Select value={version} onChange={e => setVersion(e.target.value)}><option value="">Vælg eksplicit version</option>{state.data.rateVersions.map(r => <option key={r.version} value={r.version}>{r.version} · {r.validFrom} – {r.validTo} · {r.ratePerKm} kr/km · {r.vehicle}</option>)}</Select></Field>
      {rate && <p>Grundlag: {rate.basis}. Kilde: {rate.source}. {rate.validFrom} – {rate.validTo}. Takst {rate.ratePerKm} kr/km.</p>}
      <Field label="Bilags-id"><Input type="number" min="1" step="1" value={doc} onChange={e => setDoc(e.target.value)} /></Field>
      {!state.data.postings.some(p => p.mileageEntryId === Number(trip)) && <Review key={`${trip}:${version}:${doc}:${epoch}`} slug={slug} trip={trip} version={version} doc={doc} available={!!rate && entries.some(e => e.id === Number(trip))} refresh={refresh} />}
      <RateConfiguration slug={slug} refresh={refresh} />
    </>}
  </section>;
}
function Receipt({ p }: { p: MileagePostResult }) {
  return <p role="status">Bogført kørsel {p.entryNo}: journal {p.journalEntryNo} (id {p.journalEntryId}), {formatKroner(p.amount, "DKK")} · {p.tripDate} · bilag {p.documentId}. Eventuel tilbageførsel kontrolleres i journalen.</p>;
}
function Preview({ p }: { p: MileagePostPreview }) {
  return <div><h3>Serverens forhåndsvisning</h3><p>{p.entryNo} · {p.tripDate} · {p.kilometers} km · {p.ratePerKm} kr/km · version {p.rateVersion}</p><p>{formatKroner(p.amount, "DKK")} · Debet {p.expenseAccountNo} · Kredit {p.liabilityAccountNo} · Bilag {p.documentId}</p><p>Dokumenthash: {p.documentHash}</p></div>;
}
function Review({ slug, trip, version, doc, available, refresh }: { slug: string; trip: string; version: string; doc: string; available: boolean; refresh: () => void }) {
  const [requested, setRequested] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [receipt, setReceipt] = useState<MileagePostResult | null>(null);
  const [operationKey] = useState(() => crypto.randomUUID());
  const { can } = useCapabilities(slug);
  const scope = `/api/companies/${encodeURIComponent(slug)}/mileage`;
  // Identical to ConfirmDialog identity, independent of version/document changes.
  const block = useMutationBlock(`${scope}:Bogfør kørsel:${trip}`);
  const input = { mileageEntryId: Number(trip), rateVersion: version, documentId: Number(doc) };
  const valid = available && Number.isSafeInteger(input.mileageEntryId) && input.mileageEntryId > 0 && Number.isSafeInteger(input.documentId) && input.documentId > 0;
  const state = useAsync(async signal => requested && valid ? api.previewMileagePost(slug, input, { signal }) : null, [requested]);
  const p = state.data;
  const ready = p && !state.loading && !state.error && p.mileageEntryId === input.mileageEntryId && p.rateVersion === version && p.documentId === input.documentId && /^[a-f0-9]{64}$/.test(p.previewHash);
  if (receipt) return <Receipt p={receipt} />;
  return <>
    <Button requiredPermission="company.read" disabled={!valid || requested && state.loading || block.blocked} onClick={() => { setRequested(true); state.reload(); }}>Forhåndsvis bogføring</Button>
    {requested && state.loading && <Loading />}{state.error && <ErrorState message={state.error} />}
    {ready && <><Preview p={p} /><Button requiredPermission="company.ledger.post" disabled={block.blocked} onClick={() => setConfirm(true)}>Bogfør kørsel</Button></>}
    {block.blocked && !confirm && <p role="alert">Udfaldet er ukendt. Kontrollér bogføringsstatus; handlingen gentages ikke automatisk.</p>}
    {confirm && <ConfirmDialog title="Bogfør kørsel" confirmLabel="Bekræft bogføring" body={p ? <Preview p={p} /> : "Forhåndsvisning mangler."} operationScope={scope} operationKey={trip} confirmDisabled={!ready || !can("company.ledger.post")} onRefresh={refresh} onClose={() => setConfirm(false)} onConfirm={async () => {
      if (!ready || !can("company.ledger.post")) throw new Error("Hent en gyldig forhåndsvisning først.");
      try { const result = await api.postMileage(slug, { ...input, expectedPreviewHash: p.previewHash, operationKey, confirm: true }); setReceipt(result); refresh(); }
      catch (error) { setRequested(false); throw error; }
    }} />}
  </>;
}
const fields: Array<[keyof MileageRateInput, string, string?]> = [["version", "Version"], ["validFrom", "Gyldig fra", "date"], ["validTo", "Gyldig til", "date"], ["vehicle", "Køretøj"], ["ratePerKm", "Takst pr. km (decimalstreng)"], ["basis", "Grundlag"], ["source", "Kilde"], ["expenseAccountNo", "Omkostningskonto"], ["liabilityAccountNo", "Gældskonto"]];
function RateConfiguration({ slug, refresh }: { slug: string; refresh: () => void }) {
  const [input, setInput] = useState<MileageRateInput>({ version: "", validFrom: "", validTo: "", vehicle: "", ratePerKm: "", basis: "", source: "", expenseAccountNo: "", liabilityAccountNo: "" });
  const [review, setReview] = useState<MileageRateInput | null>(null);
  const { can } = useCapabilities(slug);
  if (!can("company.admin")) return null;
  return <details><summary>Konfigurér takstversion</summary><p>Append-only virksomhedskonfiguration med egne konti og dokumenteret kilde. Ingen standardsats hentes.</p><form onSubmit={e => { e.preventDefault(); if (Object.values(input).every(v => v.trim()) && input.validFrom <= input.validTo && /^\d+(\.\d+)?$/.test(input.ratePerKm) && /[1-9]/.test(input.ratePerKm)) setReview({ ...input }); }}>
    {fields.map(([key, label, type]) => <Field key={key} label={label}><Input required type={type ?? "text"} maxLength={key === "version" || key.endsWith("AccountNo") ? 128 : 4096} value={input[key]} onChange={e => setInput({ ...input, [key]: e.target.value })} /></Field>)}
    <Button requiredPermission="company.admin" type="submit">Gennemse takstversion</Button>
  </form>{review && <ConfirmDialog title="Gem takstversion" confirmLabel="Bekræft takstversion" body={<dl>{fields.map(([key, label]) => <div key={key}><dt>{label}</dt><dd>{review[key]}</dd></div>)}</dl>} operationScope={`/api/companies/${encodeURIComponent(slug)}/mileage`} operationKey={`rate:${review.version}`} confirmDisabled={!can("company.admin")} onClose={() => setReview(null)} onRefresh={refresh} onConfirm={async () => { if (!can("company.admin")) throw new Error("Ingen adgang."); await api.configureMileageRates(slug, review); refresh(); }} />}</details>;
}
