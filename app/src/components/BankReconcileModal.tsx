// BankReconcileModal — the cockpit's one-click match of an unmatched bank row
// against an open sales invoice (#365).
//
// Without this modal the Bank view marked transactions as `Uafstemt` but
// offered no way to act — the owner had to drop to the CLI/agent to actually
// post the settlement, which is a blocker for a non-technical ApS owner. The
// modal stays on top of the existing `/invoices/settle` write endpoint, so
// every settlement still goes through the same `settleInvoiceWithBankPayment`
// core function the CLI uses; the modal owns nothing more than the picker,
// the busy state and the inline error/lock rendering.

import { useEffect, useRef, useState } from "react";
import {
  ApiError,
  api,
  type InvoiceSettleSummary,
} from "../lib/api";
import type { CompanyInvoiceRow } from "../lib/types";
import { formatKroner } from "../lib/format";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";

/** The minimum bank-row context the modal needs to settle it. */
export type BankReconcileTransaction = {
  id: number;
  date: string;
  text: string;
  amount: number;
  currency: string;
};

export type BankReconcileModalProps = {
  /** Company slug the settlement targets. */
  slug: string;
  /** The unmatched bank row the owner is acting on. */
  transaction: BankReconcileTransaction;
  /** Re-runs the Bank view load after a successful settlement. */
  onReconciled: () => void;
  /** Closes the modal without acting. */
  onClose: () => void;
};

type MaybeApiError = { code?: string; message?: string };

// An invoice is "open" — and therefore a valid match for an incoming payment —
// when it still carries an outstanding balance. `paid`/`credited`/`refunded`
// invoices are filtered out so the owner cannot pick something settled.
function isMatchable(inv: CompanyInvoiceRow): boolean {
  return inv.openBalance > 0.005 && inv.status !== "paid";
}

export function BankReconcileModal({
  slug,
  transaction,
  onReconciled,
  onClose,
}: BankReconcileModalProps) {
  const [openInvoices, setOpenInvoices] = useState<CompanyInvoiceRow[] | null>(
    null,
  );
  const [loadError, setLoadError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<number | "">("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [locked, setLocked] = useState<string | null>(null);
  const [done, setDone] = useState<InvoiceSettleSummary | null>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  // Load the company's invoices once; filter to those with an open balance.
  useEffect(() => {
    let cancelled = false;
    api
      .invoices(slug)
      .then((res) => {
        if (cancelled) return;
        const matchable = res.invoices.filter(isMatchable);
        setOpenInvoices(matchable);
        // Pre-select the only candidate if there is exactly one.
        if (matchable.length === 1) setSelectedId(matchable[0]!.documentId);
      })
      .catch((err) => {
        if (cancelled) return;
        const e = err as MaybeApiError;
        setLoadError(e?.message ?? "Fakturaerne kunne ikke hentes.");
      });
    return () => {
      cancelled = true;
    };
  }, [slug]);

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
    if (typeof selectedId !== "number") {
      setError("Vælg en faktura at matche mod.");
      return;
    }
    setBusy(true);
    setError(null);
    setLocked(null);
    try {
      const summary = await api.settleInvoice(slug, {
        invoiceDocumentId: selectedId,
        bankTransactionId: transaction.id,
        paymentDate: transaction.date,
      });
      setDone(summary);
      onReconciled();
    } catch (err) {
      const e = err as MaybeApiError;
      const message = e?.message ?? "Bogføringen kunne ikke gennemføres.";
      if (e?.code === "conflict") setLocked(message);
      else setError(message);
    } finally {
      setBusy(false);
    }
  }

  const currency = transaction.currency || "DKK";
  const noneMatchable = openInvoices !== null && openInvoices.length === 0;

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
        aria-label="Bogfør banktransaktion"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="modal-title">Bogfør banktransaktion</h3>

        {done ? (
          <>
            <div className="modal-body">
              <p>
                Banktransaktionen blev bogført som betaling af faktura{" "}
                <strong>{done.invoiceNumber ?? "—"}</strong>.
              </p>
              {done.openBalance !== null && (
                <p className="muted">
                  Resterende åben saldo:{" "}
                  {formatKroner(done.openBalance, currency)}.
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
              <p>
                <strong>{transaction.date}</strong> · {transaction.text} ·{" "}
                {formatKroner(transaction.amount, currency)}
              </p>
              <p className="muted">
                Vælg den åbne faktura denne banktransaktion betaler. Selve
                postering og bilagsnummer dannes af regnskabskernen — samme vej
                som via kommandolinjen.
              </p>
            </div>

            {locked && <LockBanner message={locked} />}
            {error && <Banner kind="error">{error}</Banner>}
            {loadError && <Banner kind="error">{loadError}</Banner>}

            <label className="modal-field">
              Match mod faktura
              {openInvoices === null ? (
                <select disabled>
                  <option>Henter fakturaer…</option>
                </select>
              ) : noneMatchable ? (
                <select disabled>
                  <option>Ingen åbne fakturaer at matche mod</option>
                </select>
              ) : (
                <select
                  value={selectedId === "" ? "" : String(selectedId)}
                  onChange={(e) => {
                    const v = e.target.value;
                    setSelectedId(v === "" ? "" : Number(v));
                  }}
                  disabled={busy}
                >
                  <option value="">— vælg faktura —</option>
                  {openInvoices.map((inv) => (
                    <option key={inv.documentId} value={String(inv.documentId)}>
                      {inv.invoiceNo}
                      {inv.customerName ? ` · ${inv.customerName}` : ""} ·{" "}
                      {formatKroner(inv.openBalance, inv.currency)} åben
                    </option>
                  ))}
                </select>
              )}
            </label>

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
                onClick={handleBook}
                disabled={
                  busy ||
                  noneMatchable ||
                  openInvoices === null ||
                  typeof selectedId !== "number"
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
// instance-checking, mirroring `BankImportModal`'s shape.
void ApiError;
