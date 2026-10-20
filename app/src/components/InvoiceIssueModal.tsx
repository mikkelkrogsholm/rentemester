// InvoiceIssueModal — the human invoice-issue action for the Cockpit (#213,
// slice 4).
//
// A person opens this from the Fakturaer view, enters the customer and one or
// more line items (description, quantity, unit price ex-VAT) and a single VAT
// rate. The browser POSTs only those essentials; the server runs the SAME
// compute+validate+issue core path the CLI's guided `invoice create` command
// uses — Rentemester computes every line total, the net amount, the VAT amount
// and the gross amount. The human NEVER does invoice arithmetic.
//
// A plain `ConfirmDialog` cannot carry the repeating line-item rows, so this
// is its own multi-field modal — it follows the `BankImportModal` /
// `DocumentIngestModal` shape and reuses the shared `LockBanner` for a 409
// backup-lock rejection.

import { useEffect, useRef, useState } from "react";
import { api, type InvoiceIssueSummary, type InvoiceLineInput } from "../lib/api";
import type {InvoiceTaxClassification,DomesticReverseChargeBasis} from "../lib/api/invoices";
import { formatKroner, parseDanishAmount } from "../lib/format";
import type { ContactCustomerRow, CompanyInvoices } from "../lib/types";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";
import { Button, Input, Select, Dialog } from "./ui";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useOptionalAuth } from "../lib/auth-context";

/** Shape of the API error the cockpit's `api.ts` throws. */
type MaybeApiError = { code?: string; message?: string };

export type InvoiceIssueModalProps = {
  /** Company slug the invoice targets. */
  slug: string;
  /** Selected fiscal year for authoritative read-back. */
  year?: string;
  /** Re-runs the Fakturaer view load after a successful issue. */
  onIssued: () => void;
  /** Closes the modal without acting. */
  onClose: () => void;
};

/** One editable line-item row in the modal — all fields are free-text inputs. */
type LineDraft = {
  description: string;
  quantity: string;
  unitPriceExVat: string;
  taxClassification: InvoiceTaxClassification;
  reverseChargeBasis: DomesticReverseChargeBasis | "";
};

const EMPTY_LINE: LineDraft = {
  description: "",
  quantity: "",
  unitPriceExVat: "",
  taxClassification: "taxable",
  reverseChargeBasis: "",
};

export function InvoiceIssueForm({
  slug,
  year,
  onIssued,
  onClose,
  presentation = "dialog",
}: InvoiceIssueModalProps & { presentation?: "dialog" | "page" }) {
  const native=useOptionalAuth()?.backend==="rust";
  const [issueDate, setIssueDate] = useState("");
  const [dueDate, setDueDate] = useState("");
  const [vatRatePercent, setVatRatePercent] = useState("25");
  const [vatTreatment,setVatTreatment]=useState<"standard"|"domestic_reverse_charge">("standard");
  const [lineTaxes,setLineTaxes]=useState(false);
  const [reverseChargeBasis,setReverseChargeBasis]=useState<DomesticReverseChargeBasis|"">("");
  const [reverseChargeNote,setReverseChargeNote]=useState("");
  const explicitTaxes=native&&(lineTaxes||vatTreatment==="domestic_reverse_charge");
  const [currency, setCurrency] = useState("DKK");
  const [sellerName, setSellerName] = useState("");
  const [sellerAddress, setSellerAddress] = useState("");
  const [sellerVat, setSellerVat] = useState("");
  const [buyerName, setBuyerName] = useState("");
  const [buyerAddress, setBuyerAddress] = useState("");
  const [buyerVat, setBuyerVat] = useState("");
  const [buyerEan,setBuyerEan]=useState("");
  const [publicRecipient,setPublicRecipient]=useState(false);
  const [buyerReference,setBuyerReference]=useState("");
  const [orderReference,setOrderReference]=useState("");
  const [lines, setLines] = useState<LineDraft[]>([{ ...EMPTY_LINE }]);

  // #380: surfacing the company's contact list inside the invoice modal so the
  // owner can pick an existing customer instead of retyping name/address/CVR
  // every time. The list is fetched lazily on mount; a fetch failure simply
  // leaves the picker empty — the owner can still type the buyer manually.
  const [customers, setCustomers] = useState<ContactCustomerRow[]>([]);
  const [selectedCustomerId, setSelectedCustomerId] = useState<string>("");

  // Which action is in flight, if any. A single `busy` flag would swap BOTH
  // buttons' labels at once, so the idle button shows the wrong progress text
  // (e.g. "Udsteder…" while only the preview is running). `busy` is derived
  // from this so all the existing disable/guard logic stays untouched.
  const [pending, setPending] = useState<"preview" | "issue" | null>(null);
  const busy = pending !== null;
  const [error, setError] = useState<string | null>(null);
  const [locked, setLocked] = useState<string | null>(null);
  const [done, setDone] = useState<InvoiceIssueSummary | null>(null);
  // #284: true when the company has no bank account configured — an invoice
  // would then go out with no payment instructions. Null until the company
  // settings have loaded; false once a payment account is confirmed present.
  const [missingPayment, setMissingPayment] = useState<boolean | null>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const [dirty, setDirty] = useState(false);
  const [readBack, setReadBack] = useState<CompanyInvoices | null>(null);
  const outcome = useMutationOutcome(async () => { setReadBack(await api.invoices(slug, year)); }, `invoice-issue:${slug}`);
  const uncertain = outcome.blocked;
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const submitRef = useRef(false);
  const dateRef = useRef<HTMLInputElement>(null);
  const vatRef = useRef<HTMLInputElement>(null);
  const formRef = useRef<HTMLDivElement>(null);
  const guard = useDiscardGuard(dirty && !done, onClose);
  useEffect(() => () => { if (previewUrl) URL.revokeObjectURL(previewUrl); }, [previewUrl]);


  // #380: load the contact list so the modal can offer a "Vælg kunde" picker.
  // The fetch is best-effort: any failure leaves the dropdown empty and the
  // owner falls back to typing the buyer manually — invoicing must never be
  // blocked by a side-channel like the contacts route.
  useEffect(() => {
    let cancelled = false;
    api
      .contacts(slug)
      .then((data) => {
        if (cancelled) return;
        setCustomers(data.customers);
      })
      .catch(() => {
        // A failed contacts lookup must not block invoicing.
      });
    return () => {
      cancelled = true;
    };
  }, [slug]);

  // Check up-front whether the company has payment details — an invoice with
  // no bank account carries no "BETALING" block, so the human is warned (#284).
  // The same settings response carries the company's own CVR, which we use to
  // prefill the Sælger CVR field so the owner does not retype it every time.
  // The field stays editable for the rare one-off where a different seller VAT
  // applies.
  useEffect(() => {
    let cancelled = false;
    api
      .companySettings(slug)
      .then((settings) => {
        if (cancelled) return;
        const payment = settings.payment;
        const hasPayment = Boolean(
          payment &&
            (payment.bankName ||
              payment.registrationNo ||
              payment.accountNo ||
              payment.iban),
        );
        setMissingPayment(!hasPayment);
        // Prefill the seller CVR from company settings, but never clobber a
        // value the owner has already typed before the fetch resolves.
        if (settings.cvr) {
          setSellerVat((prev) => (prev.trim() ? prev : settings.cvr ?? ""));
        }
      })
      .catch(() => {
        // A failed settings lookup must not block invoicing — skip the warning.
        if (!cancelled) setMissingPayment(false);
      });
    return () => {
      cancelled = true;
    };
  }, [slug]);

  /**
   * #380: prefill the buyer fields from a Kontakter entry. The fields stay
   * editable — the invoice's buyer block is a snapshot, not a live reference,
   * so the owner can still tweak a one-off address for a single invoice. An
   * empty selection clears the dropdown but leaves any typed-in buyer alone.
   */
  function selectCustomer(rawId: string) {
    setSelectedCustomerId(rawId);
    if (rawId === "") return;
    const id = Number(rawId);
    const row = customers.find((c) => c.id === id);
    if (!row) return;
    setBuyerName(row.name);
    setBuyerAddress(row.address ?? "");
    setBuyerVat(row.vatOrCvr ?? "");
    setBuyerEan(row.eanNumber ?? "");
    setPublicRecipient(Boolean(row.eanNumber));
  }

  function updateLine(index: number, patch: Partial<LineDraft>) {
    setLines((prev) =>
      prev.map((line, i) => (i === index ? { ...line, ...patch } : line)),
    );
  }

  function addLine() {
    setDirty(true);
    setLines((prev) => [...prev, { ...EMPTY_LINE }]);
  }

  function removeLine(index: number) {
    setDirty(true);
    setLines((prev) =>
      prev.length === 1 ? prev : prev.filter((_, i) => i !== index),
    );
  }

  /**
   * Parses + validates the modal's inputs into the shape both `handleIssue`
   * and `handlePreview` (#440) POST to the server. Surfaces the same human-
   * friendly errors in both flows — the preview path must not break with an
   * exception when the owner forgets a field, it must show the same red
   * banner as Udsted would.
   *
   * Returns `null` and sets `error` if validation fails, mirroring the
   * inline behaviour the original `handleIssue` had.
   */
  function buildPayload():
    | {
        issueDate: string;
        vatNum: number;
        parsedLines: InvoiceLineInput[];
      }
    | null {
    if (!issueDate.trim()) {
      setError("Angiv en fakturadato.");
      dateRef.current?.focus();
      return null;
    }
    const vatNum = Number(vatRatePercent);
    if (!explicitTaxes&&(!Number.isFinite(vatNum) || vatNum < 0)) {
      setError("Momssats skal være et tal (procent, fx 25).");
      vatRef.current?.focus();
      return null;
    }
    if(native&&vatTreatment==="domestic_reverse_charge"&&(!reverseChargeBasis||!reverseChargeNote.trim())){
      setError("Vælg dansk lovgrundlag og angiv en note om omvendt betalingspligt.");
      return null;
    }
    const parsedLines:InvoiceLineInput[] = [];
    for (const [i, line] of lines.entries()) {
      if (!line.description.trim()) {
        setError(`Linje ${i + 1}: angiv en beskrivelse.`);
        formRef.current?.querySelector<HTMLInputElement>(`[aria-label="Linje ${i + 1} beskrivelse"]`)?.focus();
        return null;
      }
      const quantity = parseDanishAmount(line.quantity);
      const unitPrice = parseDanishAmount(line.unitPriceExVat);
      if (quantity === null) {
        setError(`Linje ${i + 1}: antal skal være et tal.`);
        formRef.current?.querySelector<HTMLInputElement>(`[aria-label="Linje ${i + 1} antal"]`)?.focus();
        return null;
      }
      if (unitPrice === null) {
        setError(`Linje ${i + 1}: enhedspris skal være et tal.`);
        formRef.current?.querySelector<HTMLInputElement>(`[aria-label="Linje ${i + 1} enhedspris"]`)?.focus();
        return null;
      }
      const classification=vatTreatment==="domestic_reverse_charge"?"reverse_charge":line.taxClassification;
      const basis=vatTreatment==="domestic_reverse_charge"?reverseChargeBasis:line.reverseChargeBasis;
      if(explicitTaxes&&classification==="reverse_charge"&&!basis){setError(`Linje ${i+1}: vælg dansk lovgrundlag for omvendt betalingspligt.`);return null;}
      parsedLines.push({
        description: line.description.trim(),
        quantity,
        unitPriceExVat: unitPrice,
        ...(explicitTaxes?{taxClassification:classification,vatRate:classification==="taxable"?0.25:0,...(classification==="reverse_charge"&&basis?{reverseChargeBasis:basis}:{})}:{}),
      });
    }
    return { issueDate: issueDate.trim(), vatNum, parsedLines };
  }

  /**
   * Builds the optional seller/buyer/dueDate/currency parts that both
   * `handleIssue` and `handlePreview` (#440) send. Kept separate so the two
   * call sites stay one source of truth — a divergence here would let the
   * preview render a DIFFERENT PDF than the eventual issued PDF, which is
   * exactly the failure mode #440 must rule out.
   */
  function buildPartyAndExtras() {
    return {
      ...(explicitTaxes?{vatTreatment,...(vatTreatment==="domestic_reverse_charge"&&reverseChargeBasis?{reverseChargeBasis,reverseChargeNote:reverseChargeNote.trim()}:{})}:{}),
      customerId: selectedCustomerId ? Number(selectedCustomerId) : undefined,
      currency: currency.trim() || "DKK",
      dueDate: dueDate.trim() || undefined,
      seller:
        sellerName.trim() || sellerAddress.trim() || sellerVat.trim()
          ? {
              name: sellerName.trim() || undefined,
              address: sellerAddress.trim() || undefined,
              vatOrCvr: sellerVat.trim() || undefined,
            }
          : undefined,
      buyer:
        buyerName.trim() || buyerAddress.trim() || buyerVat.trim() || buyerEan.trim() || publicRecipient || buyerReference.trim() || orderReference.trim()
          ? {
              name: buyerName.trim() || undefined,
              address: buyerAddress.trim() || undefined,
              vatOrCvr: buyerVat.trim() || undefined,
              ...(native&&buyerEan.trim()?{eanNumber:buyerEan.trim()}:{}),
              ...(native&&(publicRecipient||buyerEan.trim())?{publicRecipient}:{}),
              ...(native&&buyerReference.trim()?{buyerReference:buyerReference.trim()}:{}),
              ...(native&&orderReference.trim()?{orderReference:orderReference.trim()}:{}),
            }
          : undefined,
    };
  }

  /**
   * #440 — Forhåndsvis. Builds the same payload Udsted would send and POSTs
   * it to the read-only `/invoices/preview` endpoint. The response is a PDF
   * blob; we open it in a new tab via `URL.createObjectURL` so the owner can
   * eyeball the layout/amounts BEFORE clicking Udsted. The preview is
   * read-only: no sequence draw, no audit_log, no journal entry — only the
   * server-side renderer runs.
   */
  async function handlePreview() {
    if (submitRef.current || done) return;
    setError(null);
    setLocked(null);

    const parsed = buildPayload();
    if (!parsed) return;
    const extras = buildPartyAndExtras();

    submitRef.current = true;
    setPending("preview");
    try {
      const blob = await api.previewInvoice(slug, {
        issueDate: parsed.issueDate,
        lines: parsed.parsedLines,
        ...(!explicitTaxes?{vatRatePercent:parsed.vatNum}:{}),
        ...extras,
      });
      const url = URL.createObjectURL(blob);
      // Keep a fallback link for browsers that block the asynchronous popup.
      // The blob is released on replacement or unmount.
      window.open(url, "_blank", "noopener");
      setPreviewUrl(url);
    } catch (err) {
      const e = err as MaybeApiError;
      const message = e?.message ?? "Forhåndsvisningen kunne ikke hentes.";
      if (e?.code === "conflict") setLocked(message);
      else setError(message);
    } finally {
      submitRef.current = false;
      setPending(null);
    }
  }

  async function handleIssue() {
    if (submitRef.current || done || outcome.isBlocked()) return;
    setError(null);
    setLocked(null);

    const parsed = buildPayload();
    if (!parsed) return;

    submitRef.current = true;
    setPending("issue");
    try {
      const extras = buildPartyAndExtras();
      const summary = await api.issueInvoice(slug, {
        issueDate: parsed.issueDate,
        lines: parsed.parsedLines,
        ...(!explicitTaxes?{vatRatePercent:parsed.vatNum}:{}),
        ...extras,
      }).catch(outcome.reject);
      setDone(summary);
      setDirty(false);
      onIssued();
    } catch (err) {
      const e = err as MaybeApiError;
      const outcomeUncertain = e?.code === "network" || e?.code === "internal";
      const message = outcomeUncertain ? "Serverens resultat kunne ikke bekræftes. Kontrollér fakturaoversigten, før du udsteder igen." : e?.message ?? "Fakturaen kunne ikke udstedes.";
      // A 409 conflict from the backup lock is shown kindly, not as an error.
      if (e?.code === "conflict") setLocked(message);
      else setError(message);
    } finally {
      submitRef.current = false;
      setPending(null);
    }
  }

  return (
    <Dialog title="Udsted faktura" onClose={guard.onClose} busy={busy} mode={presentation} initialFocusRef={closeRef}>
      <div className="workflow-form" ref={formRef} onChange={() => setDirty(true)}>
        {done ? (
          // After a successful issue the modal becomes a short receipt that
          // shows the human exactly what Rentemester computed.
          <>
            <div className="modal-body">
              <p>
                Faktura{" "}
                {done.invoiceNumber ? <strong>{done.invoiceNumber}</strong> : ""}{" "}
                udstedt.
              </p>
            </div>
            <table className="data">
              <tbody>
                <tr>
                  <td>Netto</td>
                  <td className="num">
                    {formatKroner(done.netAmount, currency)}
                  </td>
                </tr>
                <tr>
                  <td>{done.vatTreatment==="domestic_reverse_charge"?"Omvendt betalingspligt":typeof done.vatRate==="number"?`Moms (${Math.round(done.vatRate*100)}%)`:"Moms (pr. linje)"}</td>
                  <td className="num">
                    {done.vatTreatment==="domestic_reverse_charge"?"Køber afregner moms":typeof done.vatAmount==="number"?formatKroner(done.vatAmount,currency):"Ikke oplyst af serveren"}
                  </td>
                </tr>
                <tr>
                  <td>{done.vatTreatment==="domestic_reverse_charge"?"I alt — uden opkrævet moms":"I alt inkl. moms"}</td>
                  <td className="num">
                    <strong>{formatKroner(done.grossAmount, currency)}</strong>
                  </td>
                </tr>
              </tbody>
            </table>
            {native&&done.lines.some(line=>line.taxClassification)&&<table className="data" aria-label="Udstedte linjers moms">
              <thead><tr><th>Linje</th><th>Momstype</th><th>Moms</th></tr></thead>
              <tbody>{done.lines.map((line,index)=><tr key={index}><td>{line.description}</td><td>{line.taxClassification==="taxable"?"Momspligtig — 25 %":line.taxClassification==="exempt"?"Momsfritaget":line.taxClassification==="reverse_charge"?"Dansk omvendt betalingspligt":"Ikke oplyst"}{line.reverseChargeBasis&&<div>{line.reverseChargeBasis}</div>}</td><td>{typeof line.vatAmount==="number"?formatKroner(line.vatAmount,currency):"Ikke oplyst af serveren"}</td></tr>)}</tbody>
            </table>}
            {done.reverseChargeNote&&<p>{done.reverseChargeNote}</p>}
            {done.reverseChargeBasis&&<p>Lovgrundlag: {done.reverseChargeBasis}</p>}
            <div className="modal-actions">
              {/* #378: the owner just registered the invoice — the next thing
                  she needs is the file to send to the customer. Surfaced
                  immediately here so she does not have to find the row in the
                  table first. Hidden if the issue summary lacks a document id
                  (defensive: the summary always carries one for a real issue). */}
              {done.documentId !== null && (
                <a
                  className="btn secondary"
                  href={api.invoicePdfUrl(slug, done.documentId)}
                  target="_blank"
                  rel="noopener"
                >
                  Hent PDF
                </a>
              )}
              <Button
                type="button"
                className="btn"
                ref={closeRef}
                onClick={guard.dismiss}
              >
                Luk
              </Button>
            </div>
          </>
        ) : (
          <>
            <div className="modal-body">
              <p>
                Indtast kunden og fakturalinjerne. Rentemester beregner
                linjetotaler, netto, moms og bruttobeløb — du skal aldrig selv
                regne på en faktura.
              </p>
            </div>

            {locked && <LockBanner message={locked} />}
            {error && <Banner kind="error">{error}</Banner>}
            {outcome.feedback}
            {readBack && uncertain && <section className="card" role="status" aria-label="Fakturastatus fra serveren">
              <h2>Seneste fakturaer fra serveren</h2>
              <p>Regnskabsår {readBack.selectedYear}: {readBack.invoices.length} fakturaer. Sammenhold dato, kunde og beløb med din handling. Listen afgør ikke automatisk, om den afbrudte udstedelse blev gennemført.</p>
              <ul>{[...readBack.invoices].sort((a, b) => b.documentId - a.documentId).slice(0, 5).map(invoice => <li key={invoice.documentId}>{invoice.invoiceNo} · {invoice.invoiceDate ?? "Dato ukendt"} · {invoice.customerName ?? "Kunde ukendt"} · {formatKroner(invoice.grossAmount, invoice.currency)}</li>)}</ul>
              <p>Ved tvivl: gennemgå den fulde fakturaoversigt og revisionssporet. Udstedelse er fortsat blokeret.</p>
            </section>}
            {previewUrl && <p className="muted">Forhåndsvisningen er klar. <a href={previewUrl} target="_blank" rel="noopener">Åbn forhåndsvisning</a>, hvis browseren ikke åbnede PDF’en.</p>}
            {missingPayment && (
              <Banner kind="warning">
                Virksomheden har ingen bankkonto registreret — fakturaen
                udstedes uden betalingsoplysninger. Tilføj en konto under
                Administrér, så kunden ved hvortil der skal betales.
              </Banner>
            )}

            <div className="modal-field-grid">
              <label className="modal-field">
                Fakturadato
                <Input
                  type="date"
                  ref={dateRef}
                  aria-invalid={error === "Angiv en fakturadato."}
                  value={issueDate}
                  onChange={(e) => setIssueDate(e.target.value)}
                  disabled={busy || uncertain}
                />
              </label>
              <label className="modal-field">
                Forfaldsdato (valgfri)
                <Input
                  type="date"
                  value={dueDate}
                  onChange={(e) => setDueDate(e.target.value)}
                  disabled={busy || uncertain}
                />
              </label>
            </div>

            {native&&<fieldset disabled={busy||uncertain} className="modal-body">
              <legend>Moms på fakturaen</legend>
              <label className="modal-field">Momsbehandling
                <Select value={vatTreatment} onChange={e=>setVatTreatment(e.target.value as typeof vatTreatment)}>
                  <option value="standard">Almindelig dansk faktura</option>
                  <option value="domestic_reverse_charge">Dansk omvendt betalingspligt</option>
                </Select>
              </label>
              {vatTreatment==="standard"?<label><input type="checkbox" checked={lineTaxes} onChange={e=>setLineTaxes(e.target.checked)}/> Specificér moms pr. linje</label>:<>
                <label className="modal-field">Dansk lovgrundlag<Select value={reverseChargeBasis} onChange={e=>setReverseChargeBasis(e.target.value as typeof reverseChargeBasis)}>
                  <option value="">Vælg lovgrundlag</option>
                  <option value="DK_MOMSLOVEN_§46_STK_1_NR_6">Momsloven § 46, stk. 1, nr. 6</option>
                  <option value="DK_MOMSLOVEN_§46_STK_1_NR_7">Momsloven § 46, stk. 1, nr. 7</option>
                </Select></label>
                <label className="modal-field">Note om omvendt betalingspligt<Input value={reverseChargeNote} onChange={e=>setReverseChargeNote(e.target.value)}/></label>
              </>}
              <p className="muted">Vælg kun fritagelse eller omvendt betalingspligt, når grundlaget er afklaret. Serveren kontrollerer valg og beregner beløbene; udenlandsk moms håndteres ikke her.</p>
            </fieldset>}
            <div className="modal-field-grid">
              {!explicitTaxes&&<label className="modal-field">
                Momssats (%)
                <Input
                  type="text"
                  inputMode="decimal"
                  ref={vatRef}
                  value={vatRatePercent}
                  onChange={(e) => setVatRatePercent(e.target.value)}
                  disabled={busy || uncertain}
                />
              </label>}
              <label className="modal-field">
                Valuta
                <Input
                  type="text"
                  value={currency}
                  onChange={(e) => setCurrency(e.target.value)}
                  disabled={busy || uncertain}
                />
              </label>
            </div>

            <div className="modal-field-grid">
              <label className="modal-field">
                Sælger
                <Input
                  type="text"
                  value={sellerName}
                  placeholder="Navn"
                  onChange={(e) => setSellerName(e.target.value)}
                  disabled={busy || uncertain}
                />
              </label>
              <label className="modal-field">
                Sælger CVR/moms
                <Input
                  type="text"
                  value={sellerVat}
                  onChange={(e) => setSellerVat(e.target.value)}
                  disabled={busy || uncertain}
                />
              </label>
            </div>
            <label className="modal-field">
              Sælgeradresse
              <Input
                type="text"
                value={sellerAddress}
                onChange={(e) => setSellerAddress(e.target.value)}
                disabled={busy || uncertain}
              />
            </label>

            {customers.length > 0 && (
              <label className="modal-field">
                Vælg kunde
                <Select
                  value={selectedCustomerId}
                  onChange={(e) => selectCustomer(e.target.value)}
                  disabled={busy || uncertain}
                  aria-label="Vælg kunde"
                >
                  <option value="">— Ny kunde (indtast nedenfor) —</option>
                  {customers.map((c) => (
                    <option key={c.id} value={String(c.id)}>
                      {c.name}
                      {c.vatOrCvr ? ` · ${c.vatOrCvr}` : ""}
                    </option>
                  ))}
                </Select>
              </label>
            )}

            <div className="modal-field-grid">
              <label className="modal-field">
                Kunde
                <Input
                  type="text"
                  value={buyerName}
                  placeholder="Navn"
                  onChange={(e) => setBuyerName(e.target.value)}
                  disabled={busy || uncertain}
                />
              </label>
              <label className="modal-field">
                Kunde CVR/moms
                <Input
                  type="text"
                  value={buyerVat}
                  onChange={(e) => setBuyerVat(e.target.value)}
                  disabled={busy || uncertain}
                />
              </label>
            </div>
            <label className="modal-field">
              Kundeadresse
              <Input
                type="text"
                value={buyerAddress}
                onChange={(e) => setBuyerAddress(e.target.value)}
                disabled={busy || uncertain}
              />
            </label>

            {native&&<fieldset className="modal-field">
              <legend>E-faktura til offentlig modtager</legend>
              <label className="modal-field">EAN-nummer<Input aria-label="EAN-nummer" value={buyerEan} maxLength={64} onChange={e=>{setBuyerEan(e.target.value);setPublicRecipient(Boolean(e.target.value.trim()));}} disabled={busy||uncertain}/></label>
              <label><input type="checkbox" checked={publicRecipient} onChange={e=>setPublicRecipient(e.target.checked)} disabled={busy||uncertain}/> Offentlig modtager</label>
              <div className="modal-field-grid">
                <label className="modal-field">Købers reference<Input aria-label="Købers reference" value={buyerReference} maxLength={1024} onChange={e=>setBuyerReference(e.target.value)} disabled={busy||uncertain}/></label>
                <label className="modal-field">Ordrenummer<Input aria-label="Ordrenummer" value={orderReference} maxLength={1024} onChange={e=>setOrderReference(e.target.value)} disabled={busy||uncertain}/></label>
              </div>
              <p className="muted">Et gyldigt EAN behandles som offentlig modtager. EAN og referencer gemmes på fakturaen. Udstedelse sender ikke fakturaen.</p>
              {selectedCustomerId&&customers.find(c=>String(c.id)===selectedCustomerId)?.eanNumber&&<p className="muted">EAN fra den valgte kunde bruges, hvis feltet er tomt.</p>}
            </fieldset>}

            <fieldset className="modal-field invoice-lines">
              <legend>Fakturalinjer</legend>
              {lines.map((line, index) => (
                <div key={index} className="invoice-line-row">
                  <label className="modal-field">
                    Beskrivelse
                    <Input
                      type="text"
                      value={line.description}
                      aria-label={`Linje ${index + 1} beskrivelse`}
                      onChange={(e) =>
                        updateLine(index, { description: e.target.value })
                      }
                      disabled={busy || uncertain}
                    />
                  </label>
                  <label className="modal-field">
                    Antal
                    <Input
                      type="text"
                      inputMode="decimal"
                      value={line.quantity}
                      aria-label={`Linje ${index + 1} antal`}
                      onChange={(e) =>
                        updateLine(index, { quantity: e.target.value })
                      }
                      disabled={busy || uncertain}
                    />
                  </label>
                  <label className="modal-field">
                    Enhedspris ekskl. moms
                    <Input
                      type="text"
                      inputMode="decimal"
                      value={line.unitPriceExVat}
                      aria-label={`Linje ${index + 1} enhedspris`}
                      onChange={(e) =>
                        updateLine(index, { unitPriceExVat: e.target.value })
                      }
                      disabled={busy || uncertain}
                    />
                  </label>
                  {explicitTaxes&&vatTreatment==="standard"&&<>
                    <label className="modal-field">Momstype<Select aria-label={`Linje ${index+1} momstype`} value={line.taxClassification} disabled={busy||uncertain} onChange={e=>updateLine(index,{taxClassification:e.target.value as InvoiceTaxClassification})}>
                      <option value="taxable">Momspligtig — 25 %</option><option value="exempt">Momsfritaget</option><option value="reverse_charge">Dansk omvendt betalingspligt</option>
                    </Select></label>
                    {line.taxClassification==="reverse_charge"&&<label className="modal-field">Lovgrundlag<Select aria-label={`Linje ${index+1} lovgrundlag`} value={line.reverseChargeBasis} disabled={busy||uncertain} onChange={e=>updateLine(index,{reverseChargeBasis:e.target.value as DomesticReverseChargeBasis})}>
                      <option value="">Vælg lovgrundlag</option><option value="DK_MOMSLOVEN_§46_STK_1_NR_6">Momsloven § 46, stk. 1, nr. 6</option><option value="DK_MOMSLOVEN_§46_STK_1_NR_7">Momsloven § 46, stk. 1, nr. 7</option>
                    </Select></label>}
                  </>}
                  {lines.length > 1 && (
                    <Button
                      type="button"
                      className="btn secondary"
                      onClick={() => removeLine(index)}
                      disabled={busy || uncertain}
                      aria-label={`Fjern linje ${index + 1}`}
                    >
                      Fjern
                    </Button>
                  )}
                </div>
              ))}
              <Button
                type="button"
                className="btn secondary"
                onClick={addLine}
                disabled={busy || uncertain}
              >
                Tilføj linje
              </Button>
            </fieldset>

            <div className="modal-actions">
              <Button
                type="button"
                className="btn secondary"
                onClick={guard.onClose}
                disabled={busy || uncertain}
              >
                Annullér
              </Button>
              {/* #440 — Forhåndsvis renders the customer-facing PDF without
                  drawing a sequence number, writing a documents row, or
                  appending to audit_log. The owner can verify layout +
                  amounts BEFORE clicking Udsted. */}
              <Button
                type="button"
                className="btn secondary"
                onClick={handlePreview}
                disabled={busy || uncertain}
              >
                {/* Only swap the label on the button actually in flight — both
                    stay disabled while either action runs (#440). */}
                {pending === "preview" ? "Henter…" : "Forhåndsvis"}
              </Button>
              <Button
                type="button"
                className="btn"
                requiredPermission="company.draft.write" onClick={handleIssue}
                disabled={busy || uncertain}
              >
                {pending === "issue" ? "Udsteder…" : "Udsted faktura"}
              </Button>
            </div>
          </>
        )}
      </div>
      {guard.confirmation}
    </Dialog>
  );
}

/** Compatibility wrapper for callers that still need a short dialog. */
export function InvoiceIssueModal(props: InvoiceIssueModalProps) {
  return <InvoiceIssueForm {...props} presentation="dialog" />;
}
