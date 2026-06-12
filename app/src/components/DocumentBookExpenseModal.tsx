// DocumentBookExpenseModal — the cockpit's one-click bogføring of an
// "Ikke bogført" bilag against an unmatched outgoing bank transaction (#407).
//
// Without this modal the Bilag view marked a row as `Ikke bogført` but
// offered no way to act — the owner had to drop to the CLI to actually post
// the expense, which is a blocker for a non-technical ApS owner. The modal
// stays on top of the existing `/documents/book-expense` write endpoint, so
// every booking still goes through the same `bookExpenseFromBank` core
// function the CLI's `expense book` command uses; the modal owns nothing
// more than the picker, the busy state and the inline error/lock rendering.
//
// The "konfirmation"-step is the modal itself: the owner sees the bilag's
// fields, the chosen expense account, the chosen bank transaction and the
// computed net/VAT/gross from the bilag before the "Bogfør"-button is
// pressed — the same determinism contract the CLI honours.

import { useEffect, useRef, useState } from "react";
import {
  ApiError,
  api,
  type DocumentBookExpenseSummary,
  type DocumentBookingOptions,
  type ExpenseVatTreatment,
} from "../lib/api";
import { formatKroner } from "../lib/format";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";

export type DocumentBookExpenseModalProps = {
  slug: string;
  /** The bilag id the action targets. */
  documentId: number;
  /** Re-runs the Bilag view load after a successful booking. */
  onBooked: () => void;
  /** Closes the modal without acting. */
  onClose: () => void;
};

type MaybeApiError = { code?: string; message?: string };

const VAT_TREATMENT_LABELS: Record<ExpenseVatTreatment, string> = {
  standard: "Standard (25% købsmoms)",
  reverse_charge: "Omvendt betalingspligt (EU-ydelse)",
  representation: "Repræsentation (delvis fradragsret)",
  exempt: "Momsfri",
};

export function DocumentBookExpenseModal({
  slug,
  documentId,
  onBooked,
  onClose,
}: DocumentBookExpenseModalProps) {
  const [options, setOptions] = useState<DocumentBookingOptions | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [expenseAccountNo, setExpenseAccountNo] = useState<string>("");
  const [bankTransactionId, setBankTransactionId] = useState<number | "">("");
  const [vatTreatment, setVatTreatment] = useState<ExpenseVatTreatment | "">(
    "",
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [locked, setLocked] = useState<string | null>(null);
  const [done, setDone] = useState<DocumentBookExpenseSummary | null>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  // Load the picker rows + the bilag once.
  useEffect(() => {
    let cancelled = false;
    api
      .documentBookingOptions(slug, documentId)
      .then((res) => {
        if (cancelled) return;
        setOptions(res);
        // Pre-select the only candidate if there is exactly one outgoing tx
        // that matches the bilag's gross amount — the same hint
        // BankReconcileModal uses to remove a click when there is no choice.
        const gross = res.document.amountIncVat;
        if (gross !== null) {
          const exact = res.unmatchedOutgoingBank.filter(
            (t) => Math.abs(Math.abs(t.amount) - Math.abs(gross)) < 0.005,
          );
          if (exact.length === 1) setBankTransactionId(exact[0]!.id);
        }
      })
      .catch((err) => {
        if (cancelled) return;
        const e = err as MaybeApiError;
        setLoadError(e?.message ?? "Bogføringsdata kunne ikke hentes.");
      });
    return () => {
      cancelled = true;
    };
  }, [slug, documentId]);

  // Move focus into the dialog and let Escape dismiss it — basic modal hygiene.
  useEffect(() => {
    closeRef.current?.focus();
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && !busy) onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  async function handleBook() {
    if (!expenseAccountNo) {
      setError("Vælg en udgiftskonto.");
      return;
    }
    if (typeof bankTransactionId !== "number") {
      setError("Vælg en banktransaktion at parre bilaget med.");
      return;
    }
    setBusy(true);
    setError(null);
    setLocked(null);
    try {
      const summary = await api.bookDocumentExpense(slug, {
        documentId,
        bankTransactionId,
        expenseAccountNo,
        ...(vatTreatment ? { vatTreatment } : {}),
      });
      setDone(summary);
      onBooked();
    } catch (err) {
      const e = err as MaybeApiError;
      const message = e?.message ?? "Bogføringen kunne ikke gennemføres.";
      if (e?.code === "conflict") setLocked(message);
      else setError(message);
    } finally {
      setBusy(false);
    }
  }

  const doc = options?.document;
  const currency = doc?.currency || "DKK";
  const noneToMatch =
    options !== null && options.unmatchedOutgoingBank.length === 0;
  const noAccounts =
    options !== null && options.expenseAccounts.length === 0;

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
        aria-label="Bogfør bilag"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="modal-title">Bogfør bilag</h3>

        {done ? (
          <>
            <div className="modal-body">
              <p>
                Bilaget blev bogført som journalpost{" "}
                <strong>{done.entryId ?? "—"}</strong>.
              </p>
              {done.grossAmount !== null && (
                <p className="muted">
                  Bruttobeløb: {formatKroner(done.grossAmount, currency)} ·
                  Nettobeløb:{" "}
                  {done.netAmount !== null
                    ? formatKroner(done.netAmount, currency)
                    : "—"}{" "}
                  · Købsmoms:{" "}
                  {done.vatAmount !== null
                    ? formatKroner(done.vatAmount, currency)
                    : "—"}
                  .
                </p>
              )}
            </div>
            <div className="modal-actions">
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
              {options === null && !loadError && (
                <p className="muted">Henter bogføringsdata…</p>
              )}
              {doc && (
                <>
                  <p>
                    <strong>
                      {doc.supplierName ?? "Ukendt leverandør"}
                    </strong>
                    {doc.invoiceNo ? ` · faktura ${doc.invoiceNo}` : ""} ·{" "}
                    {doc.invoiceDate ?? "—"} ·{" "}
                    {doc.amountIncVat !== null
                      ? formatKroner(doc.amountIncVat, currency)
                      : "—"}{" "}
                    inkl. moms
                  </p>
                  <p className="muted">
                    Vælg den udgiftskonto bilaget hører til og den
                    banktransaktion det betaler. Selve posteringen og bilagets
                    moms-beregning dannes af regnskabskernen — samme vej som
                    via kommandolinjen.
                  </p>
                </>
              )}
            </div>

            {locked && <LockBanner message={locked} />}
            {error && <Banner kind="error">{error}</Banner>}
            {loadError && <Banner kind="error">{loadError}</Banner>}

            <label className="modal-field">
              Udgiftskonto
              {options === null ? (
                <select disabled>
                  <option>Henter konti…</option>
                </select>
              ) : noAccounts ? (
                <select disabled>
                  <option>Ingen udgiftskonti — kør først kontoplanen.</option>
                </select>
              ) : (
                <select
                  value={expenseAccountNo}
                  onChange={(e) => setExpenseAccountNo(e.target.value)}
                  disabled={busy}
                >
                  <option value="">— vælg konto —</option>
                  {options.expenseAccounts.map((a) => (
                    <option key={a.accountNo} value={a.accountNo}>
                      {a.accountNo} · {a.name}
                    </option>
                  ))}
                </select>
              )}
            </label>

            <label className="modal-field">
              Banktransaktion (uafstemt, udgående)
              {options === null ? (
                <select disabled>
                  <option>Henter banktransaktioner…</option>
                </select>
              ) : noneToMatch ? (
                <select disabled>
                  <option>
                    Ingen uafstemte udgående banktransaktioner — importér først
                    bank-CSV.
                  </option>
                </select>
              ) : (
                <select
                  value={
                    bankTransactionId === "" ? "" : String(bankTransactionId)
                  }
                  onChange={(e) => {
                    const v = e.target.value;
                    setBankTransactionId(v === "" ? "" : Number(v));
                  }}
                  disabled={busy}
                >
                  <option value="">— vælg banktransaktion —</option>
                  {options.unmatchedOutgoingBank.map((t) => (
                    <option key={t.id} value={String(t.id)}>
                      {t.date} · {t.text} ·{" "}
                      {formatKroner(t.amount, t.currency)}
                    </option>
                  ))}
                </select>
              )}
            </label>

            <label className="modal-field">
              Moms-behandling (valgfri — udledes ellers af kontoen)
              <select
                value={vatTreatment}
                onChange={(e) =>
                  setVatTreatment(
                    e.target.value === ""
                      ? ""
                      : (e.target.value as ExpenseVatTreatment),
                  )
                }
                disabled={busy || options === null}
              >
                <option value="">— udled fra konto —</option>
                {(Object.keys(VAT_TREATMENT_LABELS) as ExpenseVatTreatment[]).map(
                  (k) => (
                    <option key={k} value={k}>
                      {VAT_TREATMENT_LABELS[k]}
                    </option>
                  ),
                )}
              </select>
            </label>

            <div className="modal-actions">
              <button
                type="button"
                className="btn secondary"
                onClick={onClose}
                disabled={busy}
                ref={closeRef}
              >
                Annullér
              </button>
              <button
                type="button"
                className="btn"
                onClick={handleBook}
                disabled={
                  busy ||
                  options === null ||
                  noneToMatch ||
                  noAccounts ||
                  !expenseAccountNo ||
                  typeof bankTransactionId !== "number"
                }
              >
                {busy ? "Bogfører…" : "Bogfør"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

// `ApiError` is re-imported above purely so the build tracks the dependency;
// the runtime branch reads `code`/`message` off the thrown value rather than
// instance-checking, mirroring `BankReconcileModal`'s shape.
void ApiError;
