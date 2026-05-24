// ContactFormModal — create or edit a customer/vendor from the Cockpit (#390).
//
// Until now the Kontakter page only exposed Import + Administrér; the only
// path to a new contact was the CLI or a one-shot CSV migration. This modal
// becomes the cockpit's daily-maintenance surface: pick the contact type,
// fill the stamdata the ledger keys off (navn, CVR, e-mail, valuta,
// betalingsfrist, standardkonto, momsbehandling), and save. A CVR lookup
// button prefills name + address from the CVR register when an 8-digit
// Danish CVR is entered, so the data the momsangivelse later rests on is
// correct from the start.

import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import type {
  ContactCustomerRow,
  ContactVendorRow,
  CustomerInput,
  VendorInput,
} from "../lib/types";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";

/** Shape of the API error the cockpit's `api.ts` throws. */
type MaybeApiError = { code?: string; message?: string };

export type ContactKind = "customer" | "vendor";

export type ContactFormModalProps = {
  slug: string;
  kind: ContactKind;
  /** When set, the modal opens in edit-mode with the row prefilled. */
  customer?: ContactCustomerRow;
  vendor?: ContactVendorRow;
  /** Called after a successful create/update so the calling view can refresh. */
  onSaved: () => void;
  /** Closes the modal without acting. */
  onClose: () => void;
};

type FormState = {
  name: string;
  vatOrCvr: string;
  email: string;
  phone: string;
  website: string;
  address: string;
  notes: string;
  // customer-only
  eanNumber: string;
  paymentTermsDays: string;
  defaultCurrency: string;
  // vendor-only
  defaultExpenseAccount: string;
  defaultVatTreatment: string;
};

const VAT_TREATMENT_OPTIONS: Array<{ value: string; label: string }> = [
  { value: "", label: "—" },
  { value: "standard", label: "Standardmoms" },
  { value: "domestic_reverse_charge", label: "Omvendt betalingspligt (DK)" },
  { value: "foreign_reverse_charge", label: "Omvendt betalingspligt (udland)" },
  { value: "exempt", label: "Momsfritaget" },
];

function emptyForm(): FormState {
  return {
    name: "",
    vatOrCvr: "",
    email: "",
    phone: "",
    website: "",
    address: "",
    notes: "",
    eanNumber: "",
    paymentTermsDays: "30",
    defaultCurrency: "DKK",
    defaultExpenseAccount: "",
    defaultVatTreatment: "",
  };
}

function customerToForm(c: ContactCustomerRow): FormState {
  return {
    name: c.name,
    vatOrCvr: c.vatOrCvr ?? "",
    email: c.email ?? "",
    phone: c.phone ?? "",
    website: c.website ?? "",
    address: c.address ?? "",
    notes: c.notes ?? "",
    eanNumber: c.eanNumber ?? "",
    paymentTermsDays: String(c.paymentTermsDays),
    defaultCurrency: c.defaultCurrency,
    defaultExpenseAccount: "",
    defaultVatTreatment: "",
  };
}

function vendorToForm(v: ContactVendorRow): FormState {
  return {
    name: v.name,
    vatOrCvr: v.vatOrCvr ?? "",
    email: v.email ?? "",
    phone: v.phone ?? "",
    website: v.website ?? "",
    address: v.address ?? "",
    notes: v.notes ?? "",
    eanNumber: "",
    paymentTermsDays: "30",
    defaultCurrency: "DKK",
    defaultExpenseAccount: v.defaultExpenseAccount ?? "",
    defaultVatTreatment: v.defaultVatTreatment ?? "",
  };
}

/** A trimmed empty string becomes undefined — we only POST set fields. */
function maybe(value: string): string | undefined {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

function looksLikeDanishCvr(value: string): boolean {
  const digits = value.replace(/\D/g, "");
  return digits.length === 8;
}

export function ContactFormModal({
  slug,
  kind,
  customer,
  vendor,
  onSaved,
  onClose,
}: ContactFormModalProps) {
  const editing = Boolean(customer ?? vendor);
  const [form, setForm] = useState<FormState>(() => {
    if (customer) return customerToForm(customer);
    if (vendor) return vendorToForm(vendor);
    return emptyForm();
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [locked, setLocked] = useState<string | null>(null);
  const [cvrInfo, setCvrInfo] = useState<string | null>(null);
  const [cvrBusy, setCvrBusy] = useState(false);
  const closeRef = useRef<HTMLButtonElement>(null);
  const nameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    (nameRef.current ?? closeRef.current)?.focus();
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && !busy) onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  function update<K extends keyof FormState>(key: K, value: FormState[K]) {
    setForm((prev) => ({ ...prev, [key]: value }));
  }

  async function handleCvrLookup() {
    setError(null);
    setCvrInfo(null);
    if (!looksLikeDanishCvr(form.vatOrCvr)) {
      setError("Indtast et 8-cifret CVR-nummer for at slå op.");
      return;
    }
    setCvrBusy(true);
    try {
      const digits = form.vatOrCvr.replace(/\D/g, "");
      const result = await api.cvrLookup(slug, digits);
      if (!result.ok || !result.company) {
        const message =
          result.errors[0] ??
          "CVR-opslag mislykkedes. Kontrollér CVR-nummeret eller udfyld manuelt.";
        setCvrInfo(message);
        return;
      }
      const c = result.company;
      setForm((prev) => {
        const next = { ...prev };
        if (!prev.name.trim()) next.name = c.name;
        if (!prev.vatOrCvr.trim() || /^\d{8}$/.test(prev.vatOrCvr.trim())) {
          next.vatOrCvr = `DK${c.cvr}`;
        }
        if (!prev.address.trim()) {
          const city = [c.postalCode, c.city].filter(Boolean).join(" ");
          const full = [c.address, city].filter(Boolean).join(", ");
          if (full) next.address = full;
        }
        if (!prev.email.trim() && c.email) next.email = c.email;
        if (!prev.phone.trim() && c.phone) next.phone = c.phone;
        if (!prev.website.trim() && c.website) next.website = c.website;
        return next;
      });
      setCvrInfo(
        result.cached
          ? `Hentet fra CVR-cachen: ${c.name}.`
          : `Hentet fra CVR-registeret: ${c.name}.`,
      );
    } catch (err) {
      const e = err as MaybeApiError;
      setCvrInfo(e?.message ?? "CVR-opslag mislykkedes.");
    } finally {
      setCvrBusy(false);
    }
  }

  async function handleSave() {
    setError(null);
    setLocked(null);
    if (!form.name.trim()) {
      setError("Navn er påkrævet.");
      return;
    }
    setBusy(true);
    try {
      if (kind === "customer") {
        const paymentTerms = Number(form.paymentTermsDays);
        if (!Number.isInteger(paymentTerms) || paymentTerms <= 0) {
          setError("Betalingsfrist skal være et positivt heltal (dage).");
          setBusy(false);
          return;
        }
        const input: CustomerInput = {
          name: form.name.trim(),
          paymentTermsDays: paymentTerms,
          defaultCurrency: form.defaultCurrency.trim() || "DKK",
        };
        const vatOrCvr = maybe(form.vatOrCvr);
        if (vatOrCvr !== undefined) input.vatOrCvr = vatOrCvr;
        const email = maybe(form.email);
        if (email !== undefined) input.email = email;
        const phone = maybe(form.phone);
        if (phone !== undefined) input.phone = phone;
        const website = maybe(form.website);
        if (website !== undefined) input.website = website;
        const address = maybe(form.address);
        if (address !== undefined) input.address = address;
        const ean = maybe(form.eanNumber);
        if (ean !== undefined) input.eanNumber = ean;
        const notes = maybe(form.notes);
        if (notes !== undefined) input.notes = notes;

        if (customer) {
          await api.updateCustomer(slug, customer.id, input);
        } else {
          await api.createCustomer(slug, input);
        }
      } else {
        const input: VendorInput = { name: form.name.trim() };
        const vatOrCvr = maybe(form.vatOrCvr);
        if (vatOrCvr !== undefined) input.vatOrCvr = vatOrCvr;
        const email = maybe(form.email);
        if (email !== undefined) input.email = email;
        const phone = maybe(form.phone);
        if (phone !== undefined) input.phone = phone;
        const website = maybe(form.website);
        if (website !== undefined) input.website = website;
        const address = maybe(form.address);
        if (address !== undefined) input.address = address;
        const expenseAcct = maybe(form.defaultExpenseAccount);
        if (expenseAcct !== undefined) input.defaultExpenseAccount = expenseAcct;
        const vatTreat = maybe(form.defaultVatTreatment);
        if (vatTreat !== undefined) input.defaultVatTreatment = vatTreat;
        const notes = maybe(form.notes);
        if (notes !== undefined) input.notes = notes;

        if (vendor) {
          await api.updateVendor(slug, vendor.id, input);
        } else {
          await api.createVendor(slug, input);
        }
      }
      onSaved();
      onClose();
    } catch (err) {
      const e = err as MaybeApiError;
      const message = e?.message ?? "Kontakten kunne ikke gemmes.";
      if (e?.code === "conflict") setLocked(message);
      else setError(message);
      setBusy(false);
    }
  }

  const title = editing
    ? kind === "customer"
      ? "Redigér kunde"
      : "Redigér leverandør"
    : kind === "customer"
      ? "Tilføj kunde"
      : "Tilføj leverandør";

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
        aria-label={title}
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="modal-title">{title}</h3>

        {locked && <LockBanner message={locked} />}
        {error && <Banner kind="error">{error}</Banner>}
        {cvrInfo && <Banner kind="warning">{cvrInfo}</Banner>}

        <label className="modal-field">
          Navn
          <input
            ref={nameRef}
            type="text"
            value={form.name}
            onChange={(e) => update("name", e.target.value)}
            disabled={busy}
            required
          />
        </label>

        <label className="modal-field">
          CVR / moms-nr.
          <div style={{ display: "flex", gap: "0.5rem" }}>
            <input
              type="text"
              value={form.vatOrCvr}
              onChange={(e) => update("vatOrCvr", e.target.value)}
              disabled={busy}
              placeholder="DK12345678 eller 12345678"
              style={{ flex: 1 }}
            />
            <button
              type="button"
              className="btn secondary"
              onClick={handleCvrLookup}
              disabled={busy || cvrBusy || !looksLikeDanishCvr(form.vatOrCvr)}
              title="Slå CVR-nummeret op og udfyld navn + adresse"
            >
              {cvrBusy ? "Slår op…" : "Slå CVR op"}
            </button>
          </div>
        </label>

        <label className="modal-field">
          E-mail
          <input
            type="email"
            value={form.email}
            onChange={(e) => update("email", e.target.value)}
            disabled={busy}
          />
        </label>

        <label className="modal-field">
          Telefon
          <input
            type="tel"
            value={form.phone}
            onChange={(e) => update("phone", e.target.value)}
            disabled={busy}
          />
        </label>

        <label className="modal-field">
          Adresse
          <input
            type="text"
            value={form.address}
            onChange={(e) => update("address", e.target.value)}
            disabled={busy}
          />
        </label>

        <label className="modal-field">
          Hjemmeside
          <input
            type="url"
            value={form.website}
            onChange={(e) => update("website", e.target.value)}
            disabled={busy}
          />
        </label>

        {kind === "customer" ? (
          <>
            <label className="modal-field">
              EAN-nummer (offentlige kunder)
              <input
                type="text"
                value={form.eanNumber}
                onChange={(e) => update("eanNumber", e.target.value)}
                disabled={busy}
                placeholder="13 cifre"
              />
            </label>

            <label className="modal-field">
              Betalingsfrist (dage)
              <input
                type="number"
                min="1"
                value={form.paymentTermsDays}
                onChange={(e) => update("paymentTermsDays", e.target.value)}
                disabled={busy}
              />
            </label>

            <label className="modal-field">
              Valuta
              <input
                type="text"
                value={form.defaultCurrency}
                onChange={(e) =>
                  update("defaultCurrency", e.target.value.toUpperCase())
                }
                disabled={busy}
                maxLength={3}
                placeholder="DKK"
              />
            </label>
          </>
        ) : (
          <>
            <label className="modal-field">
              Standard udgiftskonto
              <input
                type="text"
                value={form.defaultExpenseAccount}
                onChange={(e) =>
                  update("defaultExpenseAccount", e.target.value)
                }
                disabled={busy}
                placeholder="fx 3000"
              />
            </label>

            <label className="modal-field">
              Momsbehandling
              <select
                value={form.defaultVatTreatment}
                onChange={(e) => update("defaultVatTreatment", e.target.value)}
                disabled={busy}
              >
                {VAT_TREATMENT_OPTIONS.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
            </label>
          </>
        )}

        <label className="modal-field">
          Noter
          <textarea
            value={form.notes}
            onChange={(e) => update("notes", e.target.value)}
            disabled={busy}
            rows={2}
          />
        </label>

        <div className="modal-actions">
          <button
            ref={closeRef}
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
            onClick={handleSave}
            disabled={busy || !form.name.trim()}
          >
            {busy ? "Gemmer…" : editing ? "Gem ændringer" : "Opret"}
          </button>
        </div>
      </div>
    </div>
  );
}
