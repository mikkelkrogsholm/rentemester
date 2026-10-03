import { useMutationOutcome } from "../lib/useMutationOutcome";
import { parseDanishAmount } from "../lib/format";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import * as stylex from "@stylexjs/stylex";
import { Button, Dialog, Input, Select, MoneyInput } from "./ui";
// RecurringInvoiceTemplateModal — the human-driven create-flow for a
// recurring-invoice template from the Cockpit (#386).
//
// Before #386 the only way to create a template was the CLI:
//   rentemester recurring-invoice create --company <path> --input template.json
// The empty-state in `RecurringInvoicesView` hard-coded that snippet and gave
// the SMB-owner no UI affordance to actually start using the feature. This
// modal closes that gap by surfacing the same shape the CLI accepts behind a
// friendly Danish form — kunde, interval, første udstedelsesdato, betalings-
// frist, fakturalinjer, momssats, og en valgfri note. The server still runs
// the SAME `createRecurringInvoiceTemplate` core path: the cockpit hands it a
// minimal payload, and the server computes line totals, net, moms and brutto
// via `computeInvoiceAmounts` — the human never does invoice arithmetic.
//
// The modal follows the InvoiceIssueModal shape (#213) — same `modal-overlay`
// + `modal-actions` markup, same Escape-to-close, same LockBanner mapping for
// a 409 backup-lock rejection.

import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import type {
  ContactCustomerRow,
  RecurringInterval,
  RecurringDeliveryChannel,
  DeliveryPeriodMode,
} from "../lib/types";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";

/** Shape of the API error the cockpit's `api.ts` throws. */
type MaybeApiError = { code?: string; message?: string };

export type RecurringInvoiceTemplateModalProps = {
  /** Company slug the template belongs to. */
  slug: string;
  /** Re-runs the RecurringInvoicesView load after a successful create. */
  onCreated: () => void;
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

const INTERVAL_OPTIONS: Array<{ value: RecurringInterval; label: string }> = [
  { value: "weekly", label: "Ugentligt" },
  { value: "monthly", label: "Månedligt" },
  { value: "quarterly", label: "Kvartalsvist" },
  { value: "yearly", label: "Årligt" },
];
const DELIVERY_CHANNEL_OPTIONS: Array<{ value: RecurringDeliveryChannel; label: string }> = [
  { value: "manual", label: "Manuel kladde" },
  { value: "email", label: "E-mail" },
  { value: "digisense", label: "E-faktura (DigiSense)" },
];

const DELIVERY_MODE_OPTIONS: Array<{
  value: DeliveryPeriodMode;
  label: string;
  hint: string;
}> = [
  {
    value: "issue_month",
    label: "Udstedelsesmåneden",
    hint: "Leveringsperioden er hele den måned fakturaen udstedes i.",
  },
  {
    value: "interval_window",
    label: "Intervallet fra udstedelse",
    hint: "Leveringsperioden følger intervallet (fx en måned fremad).",
  },
  {
    value: "none",
    label: "Ingen leveringsperiode",
    hint: "Fakturaen får ingen leveringsperiode (typisk engangsservice).",
  },
];

export function RecurringInvoiceTemplateModal({
  slug,
  onCreated,
  onClose: onDismiss,
}: RecurringInvoiceTemplateModalProps) {
  const [name, setName] = useState("");
  const [interval, setInterval] = useState<RecurringInterval>("monthly");
  const [intervalCount, setIntervalCount] = useState("1");
  const [deliveryChannel, setDeliveryChannel] = useState<RecurringDeliveryChannel>("manual");
  const [firstIssueDate, setFirstIssueDate] = useState("");
  const [paymentTermsDays, setPaymentTermsDays] = useState("30");
  const [deliveryPeriodMode, setDeliveryPeriodMode] =
    useState<DeliveryPeriodMode>("issue_month");
  const [vatRatePercent, setVatRatePercent] = useState("25");
  const [currency, setCurrency] = useState("DKK");
  const [notes, setNotes] = useState("");

  // Buyer fields — owner can type them directly OR pick a known customer.
  const [buyerName, setBuyerName] = useState("");
  const [buyerAddress, setBuyerAddress] = useState("");
  const [buyerVat, setBuyerVat] = useState("");
  const [customers, setCustomers] = useState<ContactCustomerRow[]>([]);
  const [selectedCustomerId, setSelectedCustomerId] = useState<string>("");

  const [lines, setLines] = useState<LineDraft[]>([{ ...EMPTY_LINE }]);

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [locked, setLocked] = useState<string | null>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  // Modal hygiene: focus the close button and let Escape dismiss.


  // Best-effort load of the customer list — same pattern as InvoiceIssueModal.
  // If the lookup fails the picker stays empty and the owner types the buyer
  // manually; template creation must not be blocked by a side-channel.
  const outcome = useMutationOutcome(onCreated);
  const guard = useDiscardGuard(Boolean(name || firstIssueDate || buyerName || buyerAddress || buyerVat || notes || selectedCustomerId) || interval !== "monthly" || intervalCount !== "1" || deliveryChannel !== "manual" || paymentTermsDays !== "30" || deliveryPeriodMode !== "issue_month" || vatRatePercent !== "25" || currency !== "DKK" || JSON.stringify(lines) !== JSON.stringify([EMPTY_LINE]), onDismiss);
  const { onClose } = guard;

  useEffect(() => {
    let cancelled = false;
    api
      .contacts(slug)
      .then((data) => {
        if (cancelled) return;
        setCustomers(data.customers);
      })
      .catch(() => {
        // ignore — fall back to manual entry
      });
    return () => {
      cancelled = true;
    };
  }, [slug]);

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

  async function handleCreate() {
    if (outcome.isBlocked()) return;
    setError(null);
    setLocked(null);

    if (!name.trim()) {
      setError("Angiv et navn på skabelonen (fx 'ABC ApS · månedligt abonnement').");
      return;
    }
    if (!firstIssueDate.trim()) {
      setError("Angiv første udstedelsesdato.");
      return;
    }
    const cadenceCount = Number(intervalCount);
    if (!Number.isInteger(cadenceCount) || cadenceCount < 1 || cadenceCount > 120) {
      setError("Gentagelse skal være et helt tal mellem 1 og 120.");
      return;
    }
    const payTerms = Number(paymentTermsDays);
    if (!Number.isInteger(payTerms) || payTerms < 0 || payTerms > 365) {
      setError("Betalingsfrist skal være et helt tal mellem 0 og 365 dage.");
      return;
    }
    const vatNum = Number(vatRatePercent);
    if (!Number.isFinite(vatNum) || vatNum < 0) {
      setError("Momssats skal være et tal (procent, fx 25).");
      return;
    }
    if (!buyerName.trim() && selectedCustomerId === "") {
      setError("Vælg en kunde fra listen eller indtast et kundenavn manuelt.");
      return;
    }

    const parsedLines: Array<{ description: string; quantity: number; unitPriceExVat: number }> = [];
    for (const [i, line] of lines.entries()) {
      if (!line.description.trim()) {
        setError(`Linje ${i + 1}: angiv en beskrivelse.`);
        return;
      }
      const quantity = Number(line.quantity);
      const unitPrice = parseDanishAmount(line.unitPriceExVat);
      if (!line.quantity.trim() || !Number.isFinite(quantity) || quantity <= 0) {
        setError(`Linje ${i + 1}: antal skal være et positivt tal.`);
        return;
      }
      if (
        !line.unitPriceExVat.trim() ||
        unitPrice === null ||
        unitPrice < 0
      ) {
        setError(`Linje ${i + 1}: enhedspris skal være et tal større end eller lig 0.`);
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
      await outcome.run(() => api.createRecurringInvoiceTemplate(slug, {
        name: name.trim(),
        interval,
        intervalCount: cadenceCount,
        deliveryChannel,
        firstIssueDate: firstIssueDate.trim(),
        paymentTermsDays: payTerms,
        deliveryPeriodMode,
        notes: notes.trim() || undefined,
        vatRatePercent: vatNum,
        currency: currency.trim() || "DKK",
        ...(selectedCustomerId !== ""
          ? { customerId: Number(selectedCustomerId) }
          : {}),
        buyer:
          buyerName.trim() || buyerAddress.trim() || buyerVat.trim()
            ? {
                name: buyerName.trim() || undefined,
                address: buyerAddress.trim() || undefined,
                vatOrCvr: buyerVat.trim() || undefined,
              }
            : undefined,
        lines: parsedLines,
      }));
      onCreated();
      guard.dismiss();
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
    <Dialog title="Opret faktura-skabelon" onClose={onClose} busy={busy} initialFocusRef={closeRef}>
    {outcome.feedback}
      {guard.confirmation}



        <div className="modal-body">
          <p>
            Skabelonen genererer en faktura på fast interval. Du udsteder hver
            faktura med ét klik, når perioden er forfaldet — Rentemester
            beregner linjetotaler, netto, moms og brutto fra de tal du
            indtaster her.
          </p>
        </div>

        {locked && <LockBanner message={locked} />}
        {error && <Banner kind="error">{error}</Banner>}

        <label className="modal-field">
          Navn
          <Input
            type="text"
            value={name}
            placeholder="fx 'ABC ApS · månedligt abonnement'"
            onChange={(e) => setName(e.target.value)}
            disabled={outcome.blocked || (busy)}
            aria-label="Skabelonens navn"
          />
        </label>

        <div className="modal-field-grid">
          <label className="modal-field">
            Interval
            <Select
              value={interval}
              onChange={(e) => setInterval(e.target.value as RecurringInterval)}
              disabled={outcome.blocked || (busy)}
              aria-label="Interval"
            >
              {INTERVAL_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </Select>
          </label>
          <label className="modal-field">
            Gentag hver
            <Input
              type="number"
              min="1"
              max="120"
              inputMode="numeric"
              value={intervalCount}
              onChange={(e) => setIntervalCount(e.target.value)}
              disabled={outcome.blocked || (busy)}
              aria-label="Gentag hver"
            />
          </label>
          <label className="modal-field">
            Første udstedelsesdato
            <Input
              type="date"
              value={firstIssueDate}
              onChange={(e) => setFirstIssueDate(e.target.value)}
              disabled={outcome.blocked || (busy)}
              aria-label="Første udstedelsesdato"
            />
          </label>
        </div>

        <label className="modal-field">
          Afsendelseskanal
          <Select
            value={deliveryChannel}
            onChange={(e) => setDeliveryChannel(e.target.value as RecurringDeliveryChannel)}
            disabled={outcome.blocked || (busy)}
            aria-label="Afsendelseskanal"
          >
            {DELIVERY_CHANNEL_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>{opt.label}</option>
            ))}
          </Select>
          <span className={["muted", stylex.props(viewStyles.site0).className].filter(Boolean).join(" ")} >
            Manuel opretter kun fakturaen. E-mail og e-faktura afsendes af den eksterne scheduler, når kanalen er konfigureret.
          </span>
        </label>

        <div className="modal-field-grid">
          <label className="modal-field">
            Betalingsfrist (dage)
            <Input
              type="number"
              inputMode="numeric"
              value={paymentTermsDays}
              onChange={(e) => setPaymentTermsDays(e.target.value)}
              disabled={outcome.blocked || (busy)}
              aria-label="Betalingsfrist i dage"
            />
          </label>
          <label className="modal-field">
            Momssats (%)
            <Input
              type="number"
              inputMode="decimal"
              value={vatRatePercent}
              onChange={(e) => setVatRatePercent(e.target.value)}
              disabled={outcome.blocked || (busy)}
              aria-label="Momssats i procent"
            />
          </label>
        </div>

        <label className="modal-field">
          Leveringsperiode
          <Select
            value={deliveryPeriodMode}
            onChange={(e) =>
              setDeliveryPeriodMode(e.target.value as DeliveryPeriodMode)
            }
            disabled={outcome.blocked || (busy)}
            aria-label="Leveringsperiode"
          >
            {DELIVERY_MODE_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </Select>
          <span className={["muted", stylex.props(viewStyles.site1).className].filter(Boolean).join(" ")} >
            {
              DELIVERY_MODE_OPTIONS.find((opt) => opt.value === deliveryPeriodMode)
                ?.hint
            }
          </span>
        </label>

        <label className="modal-field">
          Valuta
          <Input
            type="text"
            value={currency}
            onChange={(e) => setCurrency(e.target.value)}
            disabled={outcome.blocked || (busy)}
            aria-label="Valuta"
          />
        </label>

        {customers.length > 0 && (
          <label className="modal-field">
            Vælg kunde
            <Select
              value={selectedCustomerId}
              onChange={(e) => selectCustomer(e.target.value)}
              disabled={outcome.blocked || (busy)}
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
              disabled={outcome.blocked || (busy)}
              aria-label="Kundens navn"
            />
          </label>
          <label className="modal-field">
            Kunde CVR/moms
            <Input
              type="text"
              value={buyerVat}
              onChange={(e) => setBuyerVat(e.target.value)}
              disabled={outcome.blocked || (busy)}
              aria-label="Kundens CVR eller momsnummer"
            />
          </label>
        </div>
        <label className="modal-field">
          Kundeadresse
          <Input
            type="text"
            value={buyerAddress}
            onChange={(e) => setBuyerAddress(e.target.value)}
            disabled={outcome.blocked || (busy)}
            aria-label="Kundens adresse"
          />
        </label>

        <fieldset className={["modal-field", stylex.props(viewStyles.site2).className].filter(Boolean).join(" ")} >
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
                  disabled={outcome.blocked || (busy)}
                />
              </label>
              <label className="modal-field">
                Antal
                <Input
                  type="number"
                  inputMode="decimal"
                  value={line.quantity}
                  aria-label={`Linje ${index + 1} antal`}
                  onChange={(e) =>
                    updateLine(index, { quantity: e.target.value })
                  }
                  disabled={outcome.blocked || (busy)}
                />
              </label>
              <label className="modal-field">
                Enhedspris ekskl. moms
                <MoneyInput
                  inputMode="decimal"
                  value={line.unitPriceExVat}
                  aria-label={`Linje ${index + 1} enhedspris`}
                  onValueChange={(e) =>
                    updateLine(index, { unitPriceExVat: e })
                  }
                  disabled={outcome.blocked || (busy)}
                />
              </label>
              {lines.length > 1 && (
                <Button variant="secondary"
                  type="button"
                  className="btn secondary"
                  onClick={() => removeLine(index)}
                  disabled={busy}
                  aria-label={`Fjern linje ${index + 1}`}
                >
                  Fjern
                </Button>
              )}
            </div>
          ))}
          <Button variant="secondary"
            type="button"
            className="btn secondary"
            onClick={addLine}
            disabled={busy}
          >
            Tilføj linje
          </Button>
        </fieldset>

        <label className="modal-field">
          Note (valgfri)
          <Input
            type="text"
            value={notes}
            placeholder="fx 'Faktura sendes på e-mail'"
            onChange={(e) => setNotes(e.target.value)}
            disabled={outcome.blocked || (busy)}
            aria-label="Note"
          />
        </label>

        <div className="modal-actions">
          <Button variant="secondary"
            type="button"
            className="btn secondary"
            ref={closeRef}
            onClick={onClose}
            disabled={busy}
          >
            Annullér
          </Button>
          <Button requiredPermission="company.draft.write"
            type="button"
            className="btn"
            onClick={handleCreate}
            disabled={outcome.blocked || (busy)}
          >
            {busy ? "Opretter…" : "Opret skabelon"}
          </Button>
        </div>

    </Dialog>
  );
}

const viewStyles = stylex.create({
site0: { fontSize: "0.85em" },
site1: { fontSize: "0.85em" },
site2: { border: "none", padding: 0 }
});
