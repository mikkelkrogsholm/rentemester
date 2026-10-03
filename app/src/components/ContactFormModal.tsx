import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import * as stylex from "@stylexjs/stylex";
import { Button, Dialog, Input, Select, Textarea } from "./ui";
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

import { useRef, useState } from "react";
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
  countryCode: string;
  identifierKind: "" | "dk_cvr" | "eu_vat" | "non_eu";
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
    countryCode: "",
    identifierKind: "",
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
    countryCode: "",
    identifierKind: "",
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
    countryCode: v.countryCode ?? "",
    identifierKind: v.identifierKind ?? "",
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
  onClose: onDismiss,
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



  const outcome = useMutationOutcome(onSaved);
  const guard = useDiscardGuard(JSON.stringify(form) !== JSON.stringify(customer ? customerToForm(customer) : vendor ? vendorToForm(vendor) : emptyForm()), onDismiss);
  const { onClose } = guard;

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
    if (outcome.isBlocked()) return;
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
          await api.updateCustomer(slug, customer.id, input).catch(outcome.reject);
        } else {
          await api.createCustomer(slug, input).catch(outcome.reject);
        }
      } else {
        const input: VendorInput = { name: form.name.trim() };
        const vatOrCvr = maybe(form.vatOrCvr);
        if (vatOrCvr !== undefined) input.vatOrCvr = vatOrCvr;
        const countryCode = maybe(form.countryCode);
        if (countryCode !== undefined) input.countryCode = countryCode;
        if (form.identifierKind) input.identifierKind = form.identifierKind;
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
          await api.updateVendor(slug, vendor.id, input).catch(outcome.reject);
        } else {
          await api.createVendor(slug, input).catch(outcome.reject);
        }
      }
      onSaved();
      guard.dismiss();
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
    <Dialog title={title} onClose={onClose} busy={busy} initialFocusRef={nameRef}>
    {outcome.feedback}
      {guard.confirmation}



        {locked && <LockBanner message={locked} />}
        {error && <Banner kind="error">{error}</Banner>}
        {cvrInfo && <Banner kind="warning">{cvrInfo}</Banner>}

        <label className="modal-field">
          Navn
          <Input
            ref={nameRef}
            type="text"
            value={form.name}
            onChange={(e) => update("name", e.target.value)}
            disabled={outcome.blocked || (busy)}
            required
          />
        </label>

        <label className="modal-field">
          CVR / moms-nr.
          <div {...stylex.props(viewStyles.site0)}>
            <Input
              type="text"
              value={form.vatOrCvr}
              onChange={(e) => update("vatOrCvr", e.target.value)}
              disabled={outcome.blocked || (busy)}
              placeholder="DK12345678 eller 12345678"
              {...stylex.props(viewStyles.site1)}
            />
            <Button requiredPermission="company.external-lookup" variant="secondary"
              type="button"
              className="btn secondary"
              onClick={handleCvrLookup}
              disabled={busy || cvrBusy || !looksLikeDanishCvr(form.vatOrCvr)}
              title="Slå CVR-nummeret op og udfyld navn + adresse"
            >
              {cvrBusy ? "Slår op…" : "Slå CVR op"}
            </Button>
          </div>
        </label>

        <label className="modal-field">
          E-mail
          <Input
            type="email"
            value={form.email}
            onChange={(e) => update("email", e.target.value)}
            disabled={outcome.blocked || (busy)}
          />
        </label>

        <label className="modal-field">
          Telefon
          <Input
            type="tel"
            value={form.phone}
            onChange={(e) => update("phone", e.target.value)}
            disabled={outcome.blocked || (busy)}
          />
        </label>

        <label className="modal-field">
          Adresse
          <Input
            type="text"
            value={form.address}
            onChange={(e) => update("address", e.target.value)}
            disabled={outcome.blocked || (busy)}
          />
        </label>

        <label className="modal-field">
          Hjemmeside
          <Input
            type="url"
            value={form.website}
            onChange={(e) => update("website", e.target.value)}
            disabled={outcome.blocked || (busy)}
          />
        </label>

        {kind === "customer" ? (
        <>
            <label className="modal-field">
              EAN-nummer (offentlige kunder)
              <Input
                type="text"
                value={form.eanNumber}
                onChange={(e) => update("eanNumber", e.target.value)}
                disabled={outcome.blocked || (busy)}
                placeholder="13 cifre"
              />
            </label>

            <label className="modal-field">
              Betalingsfrist (dage)
              <Input
                type="number"
                min="1"
                value={form.paymentTermsDays}
                onChange={(e) => update("paymentTermsDays", e.target.value)}
                disabled={outcome.blocked || (busy)}
              />
            </label>

            <label className="modal-field">
              Valuta
              <Input
                type="text"
                value={form.defaultCurrency}
                onChange={(e) =>
                  update("defaultCurrency", e.target.value.toUpperCase())
                }
                disabled={outcome.blocked || (busy)}
                maxLength={3}
                placeholder="DKK"
              />
            </label>
          </>
        ) : (
          <>
            <label className="modal-field">
              Leverandørland (ISO)
              <Input type="text" value={form.countryCode} onChange={(e) => update("countryCode", e.target.value.toUpperCase())} maxLength={2} placeholder="DK, DE, US" disabled={outcome.blocked || (busy)} />
            </label>
            <label className="modal-field">
              Leverandøridentitet
              <Select value={form.identifierKind} onChange={(e) => update("identifierKind", e.target.value as FormState["identifierKind"])} disabled={outcome.blocked || (busy)}>
                <option value="">Ikke klassificeret</option><option value="dk_cvr">Dansk CVR</option><option value="eu_vat">EU-momsnr.</option><option value="non_eu">Ikke-EU</option>
              </Select>
            </label>
            <label className="modal-field">
              Standard udgiftskonto
              <Input
                type="text"
                value={form.defaultExpenseAccount}
                onChange={(e) =>
                  update("defaultExpenseAccount", e.target.value)
                }
                disabled={outcome.blocked || (busy)}
                placeholder="fx 3000"
              />
            </label>

            <label className="modal-field">
              Momsbehandling
              <Select
                value={form.defaultVatTreatment}
                onChange={(e) => update("defaultVatTreatment", e.target.value)}
                disabled={outcome.blocked || (busy)}
              >
                {VAT_TREATMENT_OPTIONS.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </Select>
            </label>
          </>
        )}

        <label className="modal-field">
          Noter
          <Textarea
            value={form.notes}
            onChange={(e) => update("notes", e.target.value)}
            disabled={outcome.blocked || (busy)}
            rows={2}
          />
        </label>

        <div className="modal-actions">
          <Button variant="secondary"
            ref={closeRef}
            type="button"
            className="btn secondary"
            onClick={onClose}
            disabled={busy}
          >
            Annullér
          </Button>
          <Button requiredPermission="company.master-data"
            type="button"
            className="btn"
            onClick={handleSave}
            disabled={outcome.blocked || (busy || !form.name.trim())}
          >
            {busy ? "Gemmer…" : editing ? "Gem ændringer" : "Opret"}
          </Button>
        </div>

    </Dialog>
  );
}

const viewStyles = stylex.create({
site0: { display: "flex", gap: "0.5rem" },
site1: { flex: 1 }
});
