// RecurringInvoiceTemplateModal — the cockpit-side "Opret faktura-skabelon"
// action (#386).
//
// Before this slice the only way an owner could create a recurring-invoice
// template was the CLI command `rentemester recurring-invoice create`. The
// cockpit's empty state literally dumped that command at the human. This
// modal puts a real form in front of the owner: name, interval, first issue
// date, payment terms, and the same per-line description/quantity/unit-price
// inputs the regular Udsted-faktura modal uses. Rentemester (server-side)
// computes every total via `computeInvoiceAmounts`; the human never does
// invoice arithmetic.
//
// The modal POSTs to the SAME write path the CLI's `recurring-invoice create`
// runs through (`createRecurringInvoiceTemplate` core) — no new core path.

import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";

type MaybeApiError = { code?: string; message?: string };

export type RecurringInvoiceTemplateModalProps = {
  slug: string;
  /** Reloads the parent view after a successful create. */
  onCreated: () => void;
  /** Closes the modal without acting. */
  onClose: () => void;
};

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

const INTERVAL_LABELS: Array<{
  value: "monthly" | "quarterly" | "yearly";
  label: string;
}> = [
  { value: "monthly", label: "Månedligt" },
  { value: "quarterly", label: "Kvartalsvist" },
  { value: "yearly", label: "Årligt" },
];

export function RecurringInvoiceTemplateModal({
  slug,
  onCreated,
  onClose,
}: RecurringInvoiceTemplateModalProps) {
  const [name, setName] = useState("");
  const [interval, setInterval] = useState<"monthly" | "quarterly" | "yearly">(
    "monthly",
  );
  const [firstIssueDate, setFirstIssueDate] = useState("");
  const [paymentTermsDays, setPaymentTermsDays] = useState("30");
  const [vatRatePercent, setVatRatePercent] = useState("25");
  const [currency, setCurrency] = useState("DKK");
  const [buyerName, setBuyerName] = useState("");
  const [buyerAddress, setBuyerAddress] = useState("");
  const [buyerVat, setBuyerVat] = useState("");
  const [notes, setNotes] = useState("");
  const [lines, setLines] = useState<LineDraft[]>([{ ...EMPTY_LINE }]);

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [locked, setLocked] = useState<string | null>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    closeRef.current?.focus();
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && !busy) onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

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

  async function handleCreate() {
    setError(null);
    setLocked(null);

    if (!name.trim()) {
      setError("Angiv et navn på skabelonen.");
      return;
    }
    if (!firstIssueDate.trim()) {
      setError("Angiv første udstedelsesdato.");
      return;
    }
    const vatNum = Number(vatRatePercent);
    if (!Number.isFinite(vatNum) || vatNum < 0) {
      setError("Momssats skal være et tal (procent, fx 25).");
      return;
    }
    const termsNum = Number(paymentTermsDays);
    if (
      !Number.isInteger(termsNum) ||
      termsNum < 0 ||
      termsNum > 365
    ) {
      setError("Betalingsfrist skal være et heltal mellem 0 og 365 dage.");
      return;
    }

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
      await api.createRecurringInvoiceTemplate(slug, {
        name: name.trim(),
        interval,
        firstIssueDate: firstIssueDate.trim(),
        lines: parsedLines,
        vatRatePercent: vatNum,
        paymentTermsDays: termsNum,
        currency: currency.trim() || "DKK",
        notes: notes.trim() || undefined,
        buyer:
          buyerName.trim() || buyerAddress.trim() || buyerVat.trim()
            ? {
                name: buyerName.trim() || undefined,
                address: buyerAddress.trim() || undefined,
                vatOrCvr: buyerVat.trim() || undefined,
              }
            : undefined,
      });
      onCreated();
      onClose();
    } catch (err) {
      const e = err as MaybeApiError;
      const message = e?.message ?? "Skabelonen kunne ikke oprettes.";
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
        aria-label="Opret faktura-skabelon"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="modal-title">Opret faktura-skabelon</h3>

        <div className="modal-body">
          <p>
            En faktura-skabelon udsteder den samme faktura på fast interval
            (måned, kvartal eller år). Du kan altid generere den næste faktura
            i listen med ét klik — Rentemester regner linjetotaler, netto,
            moms og bruttobeløb, du skal aldrig selv regne.
          </p>
        </div>

        {locked && <LockBanner message={locked} />}
        {error && <Banner kind="error">{error}</Banner>}

        <label className="modal-field">
          Navn på skabelonen
          <input
            type="text"
            value={name}
            placeholder="fx Månedlig konsulentydelse – Kunde A/S"
            onChange={(e) => setName(e.target.value)}
            disabled={busy}
          />
        </label>

        <div className="modal-field-grid">
          <label className="modal-field">
            Interval
            <select
              value={interval}
              onChange={(e) =>
                setInterval(
                  e.target.value as "monthly" | "quarterly" | "yearly",
                )
              }
              disabled={busy}
            >
              {INTERVAL_LABELS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
          </label>
          <label className="modal-field">
            Første udstedelsesdato
            <input
              type="date"
              value={firstIssueDate}
              onChange={(e) => setFirstIssueDate(e.target.value)}
              disabled={busy}
            />
          </label>
        </div>

        <div className="modal-field-grid">
          <label className="modal-field">
            Betalingsfrist (dage)
            <input
              type="number"
              inputMode="numeric"
              value={paymentTermsDays}
              onChange={(e) => setPaymentTermsDays(e.target.value)}
              disabled={busy}
            />
          </label>
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
        </div>

        <div className="modal-field-grid">
          <label className="modal-field">
            Valuta
            <input
              type="text"
              value={currency}
              onChange={(e) => setCurrency(e.target.value)}
              disabled={busy}
            />
          </label>
          <label className="modal-field">
            Noter (valgfri)
            <input
              type="text"
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              disabled={busy}
            />
          </label>
        </div>

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

        <fieldset
          className="modal-field"
          style={{ border: "none", padding: 0 }}
        >
          <legend>Fakturalinjer (gælder hver generation)</legend>
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
            ref={closeRef}
            onClick={onClose}
            disabled={busy}
          >
            Annullér
          </button>
          <button
            type="button"
            className="btn"
            onClick={handleCreate}
            disabled={busy}
          >
            {busy ? "Opretter…" : "Opret skabelon"}
          </button>
        </div>
      </div>
    </div>
  );
}
