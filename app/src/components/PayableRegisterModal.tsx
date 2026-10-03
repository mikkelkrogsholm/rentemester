import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { Button, Dialog, Input, Select, Textarea } from "./ui";
// PayableRegisterModal — the cockpit's "Registrér leverandørfaktura"-modal
// (#340). Opens from the Leverandørfaktura-arbejdsbordet and turns an
// ingested purchase document (bilag) into a kreditorpost.
//
// Slim by design: the modal is just the picker (purchase document + expense
// account + due date + optional vat treatment + optional vendor link) and the
// busy/error chrome. The write goes through `api.registerPayable`, which
// itself goes through the SAME `core/payables.ts#registerPayable` the CLI's
// `payable register` command uses. The cockpit never reimplements bookkeeping.
//
// Confirm-gated: the action posts a journal entry (debit udgift + købsmoms,
// credit 7000 Leverandørgæld) — write-irreversible — so the modal itself IS
// the confirmation step. `api.registerPayable` adds `confirm: true` to the
// request body.

import { useEffect, useRef, useState } from "react";
import { api, ApiError } from "../lib/api";
import { formatKroner, todayIso } from "../lib/format";
import type {
  CompanyPayables,
  PayableExpenseAccountOption,
  PayableRegisterSummary,
  PayableVendorOption,
  UnregisteredPurchaseDocumentRow,
} from "../lib/types";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";

type MaybeApiError = { code?: string; message?: string };

export type PayableRegisterModalProps = {
  slug: string;
  /** The picker rows + bilag list — same payload the list view already loaded. */
  payables: CompanyPayables;
  /** Re-runs the list view after a successful registration. */
  onRegistered: () => void;
  /** Closes the modal without acting. */
  onClose: () => void;
};

/**
 * Adds `days` calendar days to `dateIso` (YYYY-MM-DD), returning the same
 * format. A blank/garbled input falls back to today + `days`. Keeps the modal
 * deterministic on the client side; the server validates the final value.
 */
function addDays(dateIso: string | null, days: number): string {
  const base = dateIso && /^\d{4}-\d{2}-\d{2}$/.test(dateIso)
    ? new Date(`${dateIso}T00:00:00`)
    : new Date();
  base.setUTCDate(base.getUTCDate() + days);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${base.getUTCFullYear()}-${pad(base.getUTCMonth() + 1)}-${pad(
    base.getUTCDate(),
  )}`;
}

export function PayableRegisterModal({
  slug,
  payables,
  onRegistered,
  onClose: onDismiss,
}: PayableRegisterModalProps) {
  const docs: UnregisteredPurchaseDocumentRow[] = payables.unregisteredDocuments;
  const accounts: PayableExpenseAccountOption[] = payables.expenseAccounts;
  const vendors: PayableVendorOption[] = payables.vendors;

  const [documentId, setDocumentId] = useState<number | "">(
    docs.length > 0 ? docs[0]!.id : "",
  );
  const selected = docs.find((d) => d.id === documentId);
  const selectedInvoiceDate = selected?.invoiceDate;
  const [billDate, setBillDate] = useState<string>(
    selected?.invoiceDate ?? todayIso(),
  );
  const [dueDate, setDueDate] = useState<string>(
    addDays(selected?.invoiceDate ?? todayIso(), 30),
  );
  const [expenseAccountNo, setExpenseAccountNo] = useState<string>("");
  const [vatTreatment, setVatTreatment] = useState<"" | "standard" | "exempt">(
    "",
  );
  const [vendorId, setVendorId] = useState<number | "">("");
  const [note, setNote] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [locked, setLocked] = useState<string | null>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  const outcome = useMutationOutcome(onRegistered);
  const guard = useDiscardGuard(Boolean(expenseAccountNo || vatTreatment || vendorId || note) || documentId !== (docs[0]?.id ?? "") || dueDate !== addDays(docs[0]?.invoiceDate ?? todayIso(), 30), onDismiss);
  const { onClose } = guard;

  // When the document picker changes, refresh the prefilled bill/due dates so
  // a freshly-picked bilag shows the right window without a manual edit.
  useEffect(() => {
    if (selectedInvoiceDate) {
      setBillDate(selectedInvoiceDate);
      setDueDate(addDays(selectedInvoiceDate, 30));
    }
  }, [selectedInvoiceDate]);

  // Basic modal hygiene: focus + Escape-to-close.


  // No bilag to register? Surface a calm guidance state instead of a broken
  // form — the owner has to ingest a bilag before there is anything to do here.
  if (docs.length === 0) {
    return (
      <Dialog title="Registrér leverandørfaktura" onClose={onClose} busy={busy} initialFocusRef={closeRef}>
    {outcome.feedback}
      {guard.confirmation}


          <div className="modal-body">
            <p>
              Der er ingen indlæste leverandørfakturaer at registrere. Læg en
              købs-PDF i Bilag-visningen, så dukker den op her.
            </p>
          </div>
          <div className="modal-actions">
            <Button variant="secondary"
              ref={closeRef}
              type="button"
              className="btn secondary"
              onClick={onClose}
            >
              Luk
            </Button>
          </div>

      </Dialog>
    );
  }

  async function handleRegister() {
    if (outcome.isBlocked()) return;
    if (typeof documentId !== "number") {
      setError("Vælg en leverandørfaktura at registrere.");
      return;
    }
    if (!billDate.trim()) {
      setError("Angiv en bilagsdato.");
      return;
    }
    if (!dueDate.trim()) {
      setError("Angiv en forfaldsdato.");
      return;
    }
    if (!expenseAccountNo) {
      setError("Vælg en udgiftskonto.");
      return;
    }
    setBusy(true);
    setError(null);
    setLocked(null);
    try {
      const summary: PayableRegisterSummary = await outcome.run(() => api.registerPayable(slug, {
        documentId,
        billDate,
        dueDate,
        expenseAccountNo,
        ...(vatTreatment ? { vatTreatment } : {}),
        ...(typeof vendorId === "number" ? { vendorId } : {}),
        ...(note.trim() ? { note: note.trim() } : {}),
      }));
      // Closes the modal and asks the list view to reload — the new payable
      // is now in the kreditorliste with status "open".
      onRegistered();
      guard.dismiss();
      // Avoid unused-variable warning while keeping the typed return for tests
      void summary;
    } catch (err) {
      const e = err as MaybeApiError;
      const message = e?.message ?? "Registreringen kunne ikke gennemføres.";
      if (e?.code === "conflict") setLocked(message);
      else setError(message);
      setBusy(false);
    }
  }

  return (
    <Dialog title="Registrér leverandørfaktura" onClose={onClose} busy={busy} initialFocusRef={closeRef}>
    {outcome.feedback}
      {guard.confirmation}


        <div className="modal-body">
          <p>
            Vælg den indlæste leverandørfaktura og hvilken udgiftskonto den
            skal bogføres mod. Posten lægges som en kreditorpost (debet
            udgift + købsmoms, kredit Leverandørgæld) og kan ikke fortrydes.
          </p>
        </div>

        {locked && <LockBanner message={locked} />}
        {error && <Banner kind="error">{error}</Banner>}

        <label className="modal-field">
          Leverandørfaktura (bilag)
          <Select
            value={documentId}
            onChange={(e) => setDocumentId(Number(e.target.value))}
            disabled={outcome.blocked || (busy)}
            aria-label="Vælg bilag"
          >
            {docs.map((d) => (
              <option key={d.id} value={d.id}>
                {(d.supplierName ?? "Ukendt leverandør") +
                  (d.invoiceNo ? ` · ${d.invoiceNo}` : "") +
                  (d.amountIncVat !== null
                    ? ` · ${formatKroner(d.amountIncVat)}`
                    : "")}
              </option>
            ))}
          </Select>
        </label>

        <label className="modal-field">
          Bilagsdato
          <Input
            type="date"
            value={billDate}
            onChange={(e) => setBillDate(e.target.value)}
            disabled={outcome.blocked || (busy)}
          />
        </label>

        <label className="modal-field">
          Forfaldsdato
          <Input
            type="date"
            value={dueDate}
            onChange={(e) => setDueDate(e.target.value)}
            disabled={outcome.blocked || (busy)}
          />
        </label>

        <label className="modal-field">
          Udgiftskonto
          <Select
            value={expenseAccountNo}
            onChange={(e) => setExpenseAccountNo(e.target.value)}
            disabled={outcome.blocked || (busy)}
            aria-label="Vælg udgiftskonto"
          >
            <option value="">— vælg konto —</option>
            {accounts.map((a) => (
              <option key={a.accountNo} value={a.accountNo}>
                {a.accountNo} · {a.name}
              </option>
            ))}
          </Select>
        </label>

        <label className="modal-field">
          Momsbehandling
          <Select
            value={vatTreatment}
            onChange={(e) =>
              setVatTreatment(e.target.value as "" | "standard" | "exempt")
            }
            disabled={outcome.blocked || (busy)}
            aria-label="Vælg momsbehandling"
          >
            <option value="">Auto (bestemt af bilagets momsbeløb)</option>
            <option value="standard">Standard (25% købsmoms)</option>
            <option value="exempt">Momsfri</option>
          </Select>
        </label>

        {vendors.length > 0 && (
          <label className="modal-field">
            Leverandør (valgfri)
            <Select
              value={vendorId}
              onChange={(e) =>
                setVendorId(e.target.value ? Number(e.target.value) : "")
              }
              disabled={outcome.blocked || (busy)}
              aria-label="Vælg leverandør"
            >
              <option value="">— ingen leverandør valgt —</option>
              {vendors.map((v) => (
                <option key={v.id} value={v.id}>
                  {v.name}
                </option>
              ))}
            </Select>
          </label>
        )}

        <label className="modal-field">
          Note (valgfri)
          <Textarea
            value={note}
            onChange={(e) => setNote(e.target.value)}
            disabled={outcome.blocked || (busy)}
            rows={2}
            placeholder="Fri tekst til revisionssporet"
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
          <Button requiredPermission="company.ledger.post"
            type="button"
            className="btn"
            onClick={handleRegister}
            disabled={outcome.blocked || (busy)}
          >
            {busy ? "Arbejder…" : "Registrér leverandørfaktura"}
          </Button>
        </div>

    </Dialog>
  );
}

// Re-export `ApiError` so test files can reference it without re-importing
// from the api module — same convention as the other modals in this folder.
export { ApiError };
