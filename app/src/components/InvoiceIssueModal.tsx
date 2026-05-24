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
import { api, type InvoiceIssueSummary } from "../lib/api";
import { formatKroner } from "../lib/format";
import type { ContactCustomerRow } from "../lib/types";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";

/** Shape of the API error the cockpit's `api.ts` throws. */
type MaybeApiError = { code?: string; message?: string };

export type InvoiceIssueModalProps = {
  /** Company slug the invoice targets. */
  slug: string;
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
};

const EMPTY_LINE: LineDraft = {
  description: "",
  quantity: "",
  unitPriceExVat: "",
};

export function InvoiceIssueModal({
  slug,
  onIssued,
  onClose,
}: InvoiceIssueModalProps) {
  const [issueDate, setIssueDate] = useState("");
  const [dueDate, setDueDate] = useState("");
  const [vatRatePercent, setVatRatePercent] = useState("25");
  const [currency, setCurrency] = useState("DKK");
  const [sellerName, setSellerName] = useState("");
  const [sellerAddress, setSellerAddress] = useState("");
  const [sellerVat, setSellerVat] = useState("");
  const [buyerName, setBuyerName] = useState("");
  const [buyerAddress, setBuyerAddress] = useState("");
  const [buyerVat, setBuyerVat] = useState("");
  const [lines, setLines] = useState<LineDraft[]>([{ ...EMPTY_LINE }]);

  // #380: surfacing the company's contact list inside the invoice modal so the
  // owner can pick an existing customer instead of retyping name/address/CVR
  // every time. The list is fetched lazily on mount; a fetch failure simply
  // leaves the picker empty — the owner can still type the buyer manually.
  const [customers, setCustomers] = useState<ContactCustomerRow[]>([]);
  const [selectedCustomerId, setSelectedCustomerId] = useState<string>("");

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [locked, setLocked] = useState<string | null>(null);
  const [done, setDone] = useState<InvoiceIssueSummary | null>(null);
  // #284: true when the company has no bank account configured — an invoice
  // would then go out with no payment instructions. Null until the company
  // settings have loaded; false once a payment account is confirmed present.
  const [missingPayment, setMissingPayment] = useState<boolean | null>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  // Move focus into the dialog and let Escape dismiss it — basic modal hygiene.
  useEffect(() => {
    closeRef.current?.focus();
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && !busy) onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

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
  }

  function updateLine(index: number, patch: Partial<LineDraft>) {
    setLines((prev) =>
      prev.map((line, i) => (i === index ? { ...line, ...patch } : line)),
    );
  }

  function addLine() {
    setLines((prev) => [...prev, { ...EMPTY_LINE }]);
  }

  function removeLine(index: number) {
    setLines((prev) =>
      prev.length === 1 ? prev : prev.filter((_, i) => i !== index),
    );
  }

  async function handleIssue() {
    setError(null);
    setLocked(null);

    if (!issueDate.trim()) {
      setError("Angiv en fakturadato.");
      return;
    }
    const vatNum = Number(vatRatePercent);
    if (!Number.isFinite(vatNum) || vatNum < 0) {
      setError("Momssats skal være et tal (procent, fx 25).");
      return;
    }

    // Every line must carry the three essentials. Rentemester computes the
    // totals server-side — the modal only validates that the inputs are
    // numeric so the human gets an immediate, clear message.
    const parsedLines = [];
    for (const [i, line] of lines.entries()) {
      if (!line.description.trim()) {
        setError(`Linje ${i + 1}: angiv en beskrivelse.`);
        return;
      }
      const quantity = Number(line.quantity);
      const unitPrice = Number(line.unitPriceExVat);
      if (!line.quantity.trim() || !Number.isFinite(quantity)) {
        setError(`Linje ${i + 1}: antal skal være et tal.`);
        return;
      }
      if (!line.unitPriceExVat.trim() || !Number.isFinite(unitPrice)) {
        setError(`Linje ${i + 1}: enhedspris skal være et tal.`);
        return;
      }
      parsedLines.push({
        description: line.description.trim(),
        quantity,
        unitPriceExVat: unitPrice,
      });
    }

    setBusy(true);
    try {
      const summary = await api.issueInvoice(slug, {
        issueDate: issueDate.trim(),
        lines: parsedLines,
        vatRatePercent: vatNum,
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
          buyerName.trim() || buyerAddress.trim() || buyerVat.trim()
            ? {
                name: buyerName.trim() || undefined,
                address: buyerAddress.trim() || undefined,
                vatOrCvr: buyerVat.trim() || undefined,
              }
            : undefined,
      });
      setDone(summary);
      onIssued();
    } catch (err) {
      const e = err as MaybeApiError;
      const message = e?.message ?? "Fakturaen kunne ikke udstedes.";
      // A 409 conflict from the backup lock is shown kindly, not as an error.
      if (e?.code === "conflict") setLocked(message);
      else setError(message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="modal-overlay"
      role="presentation"
      onClick={() => {
        if (!busy) onClose();
      }}
    >
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Udsted faktura"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="modal-title">Udsted faktura</h3>

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
                  <td>Moms ({Math.round(done.vatRate * 100)}%)</td>
                  <td className="num">
                    {formatKroner(done.vatAmount, currency)}
                  </td>
                </tr>
                <tr>
                  <td>I alt inkl. moms</td>
                  <td className="num">
                    <strong>{formatKroner(done.grossAmount, currency)}</strong>
                  </td>
                </tr>
              </tbody>
            </table>
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
              <button
                type="button"
                className="btn"
                ref={closeRef}
                onClick={onClose}
              >
                Luk
              </button>
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
                <input
                  type="date"
                  value={issueDate}
                  onChange={(e) => setIssueDate(e.target.value)}
                  disabled={busy}
                />
              </label>
              <label className="modal-field">
                Forfaldsdato (valgfri)
                <input
                  type="date"
                  value={dueDate}
                  onChange={(e) => setDueDate(e.target.value)}
                  disabled={busy}
                />
              </label>
            </div>

            <div className="modal-field-grid">
              <label className="modal-field">
                Momssats (%)
                <input
                  type="number"
                  inputMode="decimal"
                  value={vatRatePercent}
                  onChange={(e) => setVatRatePercent(e.target.value)}
                  disabled={busy}
                />
              </label>
              <label className="modal-field">
                Valuta
                <input
                  type="text"
                  value={currency}
                  onChange={(e) => setCurrency(e.target.value)}
                  disabled={busy}
                />
              </label>
            </div>

            <div className="modal-field-grid">
              <label className="modal-field">
                Sælger
                <input
                  type="text"
                  value={sellerName}
                  placeholder="Navn"
                  onChange={(e) => setSellerName(e.target.value)}
                  disabled={busy}
                />
              </label>
              <label className="modal-field">
                Sælger CVR/moms
                <input
                  type="text"
                  value={sellerVat}
                  onChange={(e) => setSellerVat(e.target.value)}
                  disabled={busy}
                />
              </label>
            </div>
            <label className="modal-field">
              Sælgeradresse
              <input
                type="text"
                value={sellerAddress}
                onChange={(e) => setSellerAddress(e.target.value)}
                disabled={busy}
              />
            </label>

            {customers.length > 0 && (
              <label className="modal-field">
                Vælg kunde
                <select
                  value={selectedCustomerId}
                  onChange={(e) => selectCustomer(e.target.value)}
                  disabled={busy}
                  aria-label="Vælg kunde"
                >
                  <option value="">— Ny kunde (indtast nedenfor) —</option>
                  {customers.map((c) => (
                    <option key={c.id} value={String(c.id)}>
                      {c.name}
                      {c.vatOrCvr ? ` · ${c.vatOrCvr}` : ""}
                    </option>
                  ))}
                </select>
              </label>
            )}

            <div className="modal-field-grid">
              <label className="modal-field">
                Kunde
                <input
                  type="text"
                  value={buyerName}
                  placeholder="Navn"
                  onChange={(e) => setBuyerName(e.target.value)}
                  disabled={busy}
                />
              </label>
              <label className="modal-field">
                Kunde CVR/moms
                <input
                  type="text"
                  value={buyerVat}
                  onChange={(e) => setBuyerVat(e.target.value)}
                  disabled={busy}
                />
              </label>
            </div>
            <label className="modal-field">
              Kundeadresse
              <input
                type="text"
                value={buyerAddress}
                onChange={(e) => setBuyerAddress(e.target.value)}
                disabled={busy}
              />
            </label>

            <fieldset className="modal-field" style={{ border: "none", padding: 0 }}>
              <legend>Fakturalinjer</legend>
              {lines.map((line, index) => (
                <div key={index} className="invoice-line-row">
                  <label className="modal-field">
                    Beskrivelse
                    <input
                      type="text"
                      value={line.description}
                      aria-label={`Linje ${index + 1} beskrivelse`}
                      onChange={(e) =>
                        updateLine(index, { description: e.target.value })
                      }
                      disabled={busy}
                    />
                  </label>
                  <label className="modal-field">
                    Antal
                    <input
                      type="number"
                      inputMode="decimal"
                      value={line.quantity}
                      aria-label={`Linje ${index + 1} antal`}
                      onChange={(e) =>
                        updateLine(index, { quantity: e.target.value })
                      }
                      disabled={busy}
                    />
                  </label>
                  <label className="modal-field">
                    Enhedspris ekskl. moms
                    <input
                      type="number"
                      inputMode="decimal"
                      value={line.unitPriceExVat}
                      aria-label={`Linje ${index + 1} enhedspris`}
                      onChange={(e) =>
                        updateLine(index, { unitPriceExVat: e.target.value })
                      }
                      disabled={busy}
                    />
                  </label>
                  {lines.length > 1 && (
                    <button
                      type="button"
                      className="btn secondary"
                      onClick={() => removeLine(index)}
                      disabled={busy}
                      aria-label={`Fjern linje ${index + 1}`}
                    >
                      Fjern
                    </button>
                  )}
                </div>
              ))}
              <button
                type="button"
                className="btn secondary"
                onClick={addLine}
                disabled={busy}
              >
                Tilføj linje
              </button>
            </fieldset>

            <div className="modal-actions">
              <button
                type="button"
                className="btn secondary"
                onClick={onClose}
                disabled={busy}
              >
                Annullér
              </button>
              <button
                type="button"
                className="btn"
                onClick={handleIssue}
                disabled={busy}
              >
                {busy ? "Udsteder…" : "Udsted faktura"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
