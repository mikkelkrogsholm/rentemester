import { useState } from "react";
import { api } from "../lib/api";
import type { AssetDisposalInput, AssetDisposalPreview, AssetDisposalReceipt, AssetRow } from "../lib/types/assets";
import { useAsync } from "../lib/useAsync";
import { useMutationBlock } from "../lib/mutation-memory";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { formatKroner, todayIso } from "../lib/format";
import { ConfirmDialog } from "./ConfirmDialog";
import { Button, Dialog, Input, Select } from "./ui";

/** Key and pre-send confirmation lock survive dismissal/reload. No automatic replay. */
export function AssetDisposalDialog({ slug, asset, currency, onClose, onPosted }: {
  slug: string; asset: AssetRow; currency: string; onClose: () => void; onPosted: () => void;
}) {
  const scope = `/companies/${slug}/assets/${asset.assetId}`;
  const storageKey = `asset-disposal-key:${scope}`;
  const [key] = useState(() => {
    try {
      const saved = sessionStorage.getItem(storageKey);
      if (saved) return saved;
      const next = crypto.randomUUID();
      sessionStorage.setItem(storageKey, next);
      return sessionStorage.getItem(storageKey) === next ? next : "";
    } catch { return ""; }
  });
  const docs = useAsync(signal => api.documents(slug, { signal }), [slug]);
  const chart = useAsync(signal => api.accounts(slug, { signal }), [slug]);
  const [transactionDate, setDate] = useState(todayIso());
  const [documentId, setDocument] = useState("");
  const [lossAccountNo, setAccount] = useState("");
  const [reason, setReason] = useState("");
  const [review, setReview] = useState<{ input: AssetDisposalInput; preview: AssetDisposalPreview } | null>(null);
  const [receipt, setReceipt] = useState<AssetDisposalReceipt | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const readback = async () => {
    if (!key) throw new Error("Kvitteringsnøglen kunne ikke gemmes.");
    const found = await api.assetDisposalReceipt(slug, asset.assetId, key);
    setReceipt(found); onPosted();
  };
  const outcome = useMutationOutcome(readback, "asset-disposal", scope);
  const sendLock = useMutationBlock(`${scope}:outcome:asset-disposal`);
  const locked = outcome.blocked || busy || !!receipt;
  const previewBody = review && <>
    <p>Kostpris: {formatKroner(review.preview.cost, currency)} · Akkumuleret afskrivning: {formatKroner(review.preview.accumulatedDepreciation, currency)} · Restværdi til udgift: {formatKroner(review.preview.carryingValue, currency)}</p>
    <table><thead><tr><th>Konto</th><th>Debet</th><th>Kredit</th></tr></thead><tbody>{review.preview.lines.map((line,i) => <tr key={i}><td>{line.accountNo}</td><td>{formatKroner(line.debit,currency)}</td><td>{formatKroner(line.credit,currency)}</td></tr>)}</tbody></table>
    <p>Planhash: <code>{review.preview.planHash}</code></p>
    <p>{review.input.transactionDate} · Bilag #{review.input.documentId} · {review.input.reason}</p>
  </>;
  return <Dialog title={`Afhænd anlæg: ${asset.name}`} onClose={onClose} busy={busy}>
    <p>Afhændelse fjerner det kapitaliserede anlæg fra balancen. Salgsindtægt og moms er ikke med; bogfør eventuelt salg og moms særskilt. Dette er ikke straksafskrivning af en småanskaffelse.</p>
    {outcome.feedback}
    {!key && <p role="alert">Handlingen er blokeret: kvitteringsnøglen kunne ikke gemmes i browseren.</p>}
    {error && <p role="alert">{error}</p>}
    {(docs.error || chart.error) && <p role="alert">{docs.error || chart.error}</p>}
    <form onSubmit={async e => {
      e.preventDefault(); if (outcome.isBlocked() || !key) return;
      setBusy(true); setError(null); setReview(null);
      const input: AssetDisposalInput = { transactionDate, documentId: Number(documentId), reason, ...(lossAccountNo ? {lossAccountNo} : {}) };
      try { setReview({ input, preview: await api.assetDisposalPreview(slug,asset.assetId,input) }); }
      catch (e) { setError(e instanceof Error ? e.message : "Forhåndsvisningen kunne ikke hentes."); }
      finally { setBusy(false); }
    }}>
      <fieldset disabled={locked || confirming}>
        <label>Bogføringsdato<Input type="date" required value={transactionDate} onChange={e => {setDate(e.target.value);setReview(null);}} /></label>
        <label>Bilag for afhændelse<Select required value={documentId} onChange={e => {setDocument(e.target.value);setReview(null);}}><option value="">Vælg bilag</option>{(docs.data?.documents ?? []).map(d => <option key={d.id} value={d.id}>#{d.id} · {d.documentNo ?? "uden nr."}</option>)}</Select></label>
        <label>Tabskonto (påkrævet ved positiv restværdi)<Select value={lossAccountNo} onChange={e => {setAccount(e.target.value);setReview(null);}}><option value="">Ingen / vælg konto</option>{(chart.data?.accounts ?? []).filter(a => a.active !== false && a.type === "expense").map(a => <option key={a.accountNo} value={a.accountNo}>{a.accountNo} · {a.name}</option>)}</Select></label>
        <label>Begrundelse<Input required maxLength={4096} value={reason} onChange={e => {setReason(e.target.value);setReview(null);}} /></label>
        <Button type="submit" requiredPermission="company.ledger.post" disabled={!key}>Forhåndsvis afhændelse</Button>
      </fieldset>
    </form>
    {previewBody}
    {review && !receipt && <Button requiredPermission="company.ledger.post" disabled={locked || !key} onClick={() => setConfirming(true)}>Gennemgå og bekræft</Button>}
    <Button variant="secondary" disabled={!key || busy} onClick={async () => {setError(null);try {await readback();} catch(e) {setError(e instanceof Error ? e.message : "Kvitteringen kunne ikke hentes.");}}}>Hent afhændelseskvittering</Button>
    {receipt && <div role="status"><p>Afhændet {receipt.transactionDate}. Restværdi: {formatKroner(receipt.remainingCarryingValue,currency)}. Ingen senere afskrivning.</p><p>Postering {receipt.journalEntryNo} (#{receipt.journalEntryId}) · Afhændelse #{receipt.disposalId}</p><code>{receipt.journalEntryHash}</code></div>}
    {confirming && review && <ConfirmDialog title={`Bekræft afhændelse: ${asset.assetId}`} body={previewBody} confirmLabel="Bogfør afhændelse" confirmKind="danger" operationScope={scope} operationKey="asset-disposal" onRefresh={readback} onClose={() => setConfirming(false)} confirmDisabled={outcome.blocked || !!receipt || !key} onConfirm={async () => {
      if (outcome.isBlocked() || !key || receipt) return;
      if (!sendLock.block()) throw new Error("Beskyttelsen mod gentagelse kunne ikke gemmes.");
      try {
        const result = await api.disposeAsset(slug,asset.assetId,{...review.input,expectedPlanHash:review.preview.planHash,idempotencyKey:key,confirm:true}).catch(outcome.reject);
        setReceipt(result); sendLock.release(); onPosted();
      } catch(e) {
        const failure = e as { status?: number; code?: string };
        if (failure.status && failure.status >= 400 && failure.status < 500 && failure.status !== 408 && failure.code !== "internal") sendLock.release();
        throw e;
      }
    }} />}
  </Dialog>;
}
