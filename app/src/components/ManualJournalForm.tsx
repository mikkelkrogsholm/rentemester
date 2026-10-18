import { useRef, useState, type FormEvent } from "react";
import { Button, Field, Input, Select } from "./ui";
import { ConfirmDialog } from "./ConfirmDialog";
import { request } from "../lib/api/_shared";
import type { NativePostingOptions } from "../lib/types/journal";

/** Small manual transfer. Domain validation, document requirements and access stay in Rust. */
export function ManualJournalForm({ slug, currency, options, onSaved, onClose }: { slug: string; currency: string; options: NativePostingOptions; onSaved: () => void; onClose: () => void }) {
  const [date, setDate] = useState(() => new Date().toLocaleDateString("sv-SE"));
  const [text, setText] = useState("");
  const [debit, setDebit] = useState("");
  const [credit, setCredit] = useState("");
  const [amount, setAmount] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const key = useRef(crypto.randomUUID());
  const accounts = options.accounts.filter((a) => a.active && a.allowDirectPosting);
  const canonicalAmount = () => {
    const normalized = amount.replace(",", ".");
    if (!/^\d+(?:\.\d{1,2})?$/.test(normalized)) throw new Error("Indtast et positivt beløb med højst to decimaler.");
    const [whole, cents = ""] = normalized.split(".");
    const result = `${whole.replace(/^0+(?=\d)/, "")}.${cents.padEnd(2, "0")}`;
    if (/^0\.00$/.test(result)) throw new Error("Beløbet skal være større end nul.");
    return result;
  };
  const review = (event: FormEvent) => {
    event.preventDefault(); setError(null);
    try { canonicalAmount(); if (debit === credit) throw new Error("Vælg to forskellige konti."); setConfirming(true); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "Kontrollér posteringen."); }
  };
  const post = async () => {
    await request(`/api/companies/${encodeURIComponent(slug)}/manual-journal`, { method: "POST", body: JSON.stringify({ confirm: true, input: {
      format: "rentemester-manual-journal-post/1", expectedRuleVersion: options.ruleVersion, idempotencyKey: key.current,
      request: { transactionDate: date, text, currency, lines: [{ accountNo: debit, debitAmount: canonicalAmount() }, { accountNo: credit, creditAmount: canonicalAmount() }] },
    } }) });
    onSaved(); onClose();
  };
  return <section className="card" aria-label="Ny manuel postering"><h2>Ny postering</h2><p>Beløbet bogføres i debet og kredit. Indtægter og udgifter kræver et tilknyttet bilag.</p>
    {error && <p role="alert">{error}</p>}
    <form onSubmit={review}>
      <Field label="Dato"><Input type="date" required value={date} onChange={(e) => setDate(e.target.value)} /></Field>
      <Field label="Posteringstekst"><Input required maxLength={500} value={text} onChange={(e) => setText(e.target.value)} /></Field>
      <Field label="Debetkonto"><Select required value={debit} onChange={(e) => setDebit(e.target.value)}><option value="">Vælg konto</option>{accounts.map((a) => <option key={a.accountNo} value={a.accountNo}>{a.accountNo} · {a.name}</option>)}</Select></Field>
      <Field label="Kreditkonto"><Select required value={credit} onChange={(e) => setCredit(e.target.value)}><option value="">Vælg konto</option>{accounts.map((a) => <option key={a.accountNo} value={a.accountNo}>{a.accountNo} · {a.name}</option>)}</Select></Field>
      <Field label={`Beløb i ${currency}`}><Input required inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} /></Field>
      <Button type="submit">Gennemse postering</Button><Button variant="secondary" onClick={onClose}>Annullér</Button>
    </form>
    {confirming && <ConfirmDialog title="Bogfør postering" body={<p>{date} · {text}<br />Debet {debit}, kredit {credit}: {canonicalAmount()} {currency}.</p>} confirmLabel="Bogfør postering" operationKey={`manual-post:${key.current}`} onConfirm={post} onClose={() => setConfirming(false)} onRefresh={onSaved} />}
  </section>;
}
