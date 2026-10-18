import { useRef, useState, type FormEvent } from "react";
import { Button, Field, Input, Textarea } from "./ui";
import { ConfirmDialog } from "./ConfirmDialog";
import { request } from "../lib/api/_shared";
import type { JournalEntry } from "../lib/types/journal";

export function JournalReversalForm({ slug, entry, ruleVersion, onSaved, onClose }: { slug: string; entry: JournalEntry; ruleVersion: string; onSaved: () => void; onClose: () => void }) {
  const [date, setDate] = useState(() => new Date().toLocaleDateString("sv-SE"));
  const [reason, setReason] = useState("");
  const [confirming, setConfirming] = useState(false);
  const key = useRef(crypto.randomUUID());
  const review = (event: FormEvent) => { event.preventDefault(); if (reason.trim()) setConfirming(true); };
  const reverse = async () => {
    await request(`/api/companies/${encodeURIComponent(slug)}/manual-journal-reversal`, { method: "POST", body: JSON.stringify({ confirm: true, input: {
      format: "rentemester-manual-journal-reversal/1", expectedRuleVersion: ruleVersion, idempotencyKey: key.current,
      request: { entryId: String(entry.id), transactionDate: date, reason: reason.trim() },
    } }) });
    onSaved(); onClose();
  };
  return <section className="card" aria-label="Tilbagefør postering"><h3>Tilbagefør {entry.entryNo}</h3><p>Originalen bevares. Der oprettes en ny postering med omvendt debet og kredit.</p>
    <form onSubmit={review}>
      <Field label="Tilbageførselsdato"><Input required type="date" value={date} onChange={(e) => setDate(e.target.value)} /></Field>
      <Field label="Begrundelse"><Textarea required maxLength={1000} value={reason} onChange={(e) => setReason(e.target.value)} /></Field>
      <Button type="submit" disabled={!reason.trim()}>Gennemse tilbageførsel</Button><Button variant="secondary" onClick={onClose}>Annullér</Button>
    </form>
    {confirming && <ConfirmDialog title="Bekræft tilbageførsel" body={<p>Tilbagefør {entry.entryNo} den {date}.<br />{reason}</p>} confirmLabel="Bogfør tilbageførsel" operationKey={`journal-reverse:${key.current}`} onConfirm={reverse} onClose={() => setConfirming(false)} onRefresh={onSaved} />}
  </section>;
}
