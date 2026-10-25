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
  type DocumentVatPreflight,
} from "../lib/api";
import { formatKroner } from "../lib/format";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";
import { Button, Select, Dialog } from "./ui";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useOptionalAuth } from "../lib/auth-context";
import { useCapabilities } from "../lib/useCapabilities";
import { useMutationBlock } from "../lib/mutation-memory";
import { ConfirmDialog } from "./ConfirmDialog";
import { UnknownMutationNotice } from "./UnknownMutationNotice";
import type { NativeDocumentVatPreflight } from "../lib/api/documents";
import type { DocumentRow } from "../lib/types";

export type DocumentBookExpenseModalProps = {
  slug: string;
  /** The bilag id the action targets. */
  documentId: number;
  initialBankTransactionId?: number;
  /** Re-runs the Bilag view load after a successful booking. */
  onBooked: () => void;
  /** Closes the modal without acting. */
  onClose: () => void;
};

type MaybeApiError = { code?: string; message?: string };

const VAT_TREATMENT_LABELS: Record<ExpenseVatTreatment, string> = {
  standard: "Standard (25% købsmoms)",
  reverse_charge: "Omvendt betalingspligt (udenlandsk ydelse)",
  representation: "Repræsentation (delvis fradragsret)",
  exempt: "Momsfri",
  non_deductible: "Ikke fradragsberettiget (momsen absorberes i udgiften)",
};

export function DocumentBookExpenseForm({
  slug,
  documentId,
  initialBankTransactionId,
  onBooked,
  onClose,
  presentation = "dialog",
}: DocumentBookExpenseModalProps & { presentation?: "dialog" | "page" }) {
  const optionsState = useAsync<DocumentBookingOptions>((signal) => api.documentBookingOptions(slug, documentId, { signal }), [slug, documentId]);
  const auth = useOptionalAuth();
  const native = auth?.backend === "rust";
  const capabilities = useCapabilities(slug);
  const allowed = capabilities.can("company.ledger.post") && capabilities.can("company.external-lookup");
  const providerTitle = "Hent momsvalidering";
  const providerScope = `purchase-vat:${slug}:${documentId}`;
  // Identical to ConfirmDialog's canonical key, independent of modal/page route.
  const providerBlock = useMutationBlock(`${providerScope}:${providerTitle}:provider`);
  const [review, setReview] = useState<{ context: object; body: React.ReactNode; preflight: DocumentVatPreflight } | null>(null);
  const contextRef = useRef<object>({});
  const previousContext = useRef<unknown[]>([]);
  const contextParts = [slug, documentId, native, auth?.session, auth?.currentSessionId, auth?.context, auth?.loading, allowed, optionsState.data];
  if (contextParts.some((part, i) => part !== previousContext.current[i])) {
    contextRef.current = {};
    previousContext.current = contextParts;
  }
  const currentContext = contextRef.current;
  const latest = useRef({ allowed, currentContext });
  latest.current = { allowed, currentContext };
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const options = optionsState.data;
  const loadError = optionsState.error;
  // Internal vouchers already use the server's bound-bank/date/exempt contract;
  // they do not have an external supplier identity to validate.
  const nativeInternalVoucher = native && options?.document.documentType === "internal_voucher";
  const nativeBoundInternalVoucher = nativeInternalVoucher && Number.isSafeInteger(options?.document.sourceBankTransactionId)
    && (options?.document.sourceBankTransactionId ?? 0) > 0
    && options?.unmatchedOutgoingBank.some(row => row.id === options.document.sourceBankTransactionId) === true;
  const [expenseAccountNo, setExpenseAccountNo] = useState<string>("");
  const [bankTransactionId, setBankTransactionId] = useState<number | "">(initialBankTransactionId ?? "");
  const [vatTreatment, setVatTreatment] = useState<ExpenseVatTreatment | "">(
    "",
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [locked, setLocked] = useState<string | null>(null);
  const [done, setDone] = useState<DocumentBookExpenseSummary | null>(null);
  const [preflight, setPreflight] = useState<DocumentVatPreflight | null>(null);
  const [preflightBusy, setPreflightBusy] = useState(false);
  const closeRef = useRef<HTMLButtonElement>(null);
  const [dirty, setDirty] = useState(false);
  const [readBack, setReadBack] = useState<DocumentRow[] | null>(null);
  const outcome = useMutationOutcome(async () => {
    const documents = await api.documents(slug);
    setReadBack(documents.documents.filter(document => document.id === documentId));
  }, `book-document:${slug}:${documentId}`);
  const uncertain = outcome.blocked || providerBlock.blocked;
  const submitRef = useRef(false);
  const guard = useDiscardGuard(dirty && !done, onClose);

  // Server options remain the source for all account/bank choices and hints.
  useEffect(() => {
    if (!options) return;
    const res = options;
    if (res.document.documentType === "internal_voucher" && res.document.sourceBankTransactionId !== null) {
      setBankTransactionId(res.document.sourceBankTransactionId);
      setVatTreatment("exempt");
      return;
    }
    const gross = res.document.amountIncVat;
    if (gross !== null) {
      const exact = res.unmatchedOutgoingBank.filter((transaction) => {
        const documentCurrency = res.document.currency.toUpperCase();
        const bankCurrency = transaction.currency.toUpperCase();
        if (bankCurrency === documentCurrency) return Math.abs(Math.abs(transaction.amount) - Math.abs(gross)) < 0.005;
        if (bankCurrency === "DKK" && documentCurrency !== "DKK" && transaction.fxRateToDkk && transaction.fxRateToDkk > 0) return Math.abs(Math.abs(transaction.amount) - Math.abs(gross * transaction.fxRateToDkk)) < 0.005;
        return false;
      });
      if (exact.length === 1) setBankTransactionId((previous) => previous === "" ? exact[0]!.id : previous);
    }
  }, [options]);

  const [preflightReload, setPreflightReload] = useState(0);
  const [preflightLoading, setPreflightLoading] = useState(true);
  const [preflightError, setPreflightError] = useState<string | null>(null);
  const [preflightContext, setPreflightContext] = useState<object | null>(null);
  const visiblePreflight = !nativeInternalVoucher && preflightContext === currentContext ? preflight : null;
  const nativeValidationBlocked = native && (nativeInternalVoucher
    ? !nativeBoundInternalVoucher || bankTransactionId !== options?.document.sourceBankTransactionId || vatTreatment !== "exempt"
    : !visiblePreflight?.ok || preflightLoading);
  useEffect(() => {
    let cancelled = false;
    const controller = new AbortController();
    setReview(null);
    setPreflight(null);
    setPreflightLoading(true);
    setPreflightError(null);
    if (native && (!options || nativeInternalVoucher)) {
      setPreflightLoading(!options);
      return () => { cancelled = true; controller.abort(); };
    }
    const read = native ? api.nativeDocumentVatPreflight : api.documentVatPreflight;
    read(slug, documentId, { signal: controller.signal }).then((result) => {
      if (!cancelled && latest.current.currentContext === currentContext) {
        setPreflight(result); setPreflightContext(currentContext);
      }
    }).catch((err) => {
      if (!cancelled && latest.current.currentContext === currentContext) setPreflightError((err as MaybeApiError)?.message ?? "Momsvalideringen kunne ikke hentes.");
    }).finally(() => { if (!cancelled) setPreflightLoading(false); });
    return () => { cancelled = true; controller.abort(); };
  }, [slug, documentId, native, currentContext, preflightReload]);

  function reviewPreflight() {
    if (!allowed || providerBlock.isBlocked() || preflightBusy || busy || !visiblePreflight || !options) return;
    setReview({ context: currentContext, preflight: visiblePreflight, body: <>
      <p>Virksomhed: {slug} · Bilag {documentId} · {options.document.documentNo ?? "—"} · Faktura {options.document.invoiceNo ?? "—"}</p>
      <p>Leverandør: {options.document.supplierName ?? "—"} · {options.document.supplierVatOrCvr ?? "—"} · {options.document.supplierCountryCode ?? "—"}</p>
      <p>{options.document.invoiceDate ?? "—"} · {formatKroner(options.document.amountIncVat ?? 0, options.document.currency)}</p>
      <p>Momsstatus: {visiblePreflight.derivedRegion} · {visiblePreflight.requiredValidation ?? "Ingen ekstern validering kræves"} · {visiblePreflight.errors.join(" · ")} · Evidens gyldig til {visiblePreflight.cache.freshUntil ?? "—"}</p>
      <p>Hent momsvalidering fra den eksterne udbyder for dette bilag. Handlingen kan gemme evidens eller en undtagelse; den bogfører ikke bilaget.</p>
    </> });
  }

  async function applyPreflight() {
    // ConfirmDialog already persisted the ONE shared operation barrier before entry.
    if (!review || review.context !== latest.current.currentContext || !latest.current.allowed || preflightBusy) {
      throw new ApiError("access_denied", "Adgang eller bilagskontekst er ændret. Gennemgå bilaget igen.", 403);
    }
    const selection = review;
    setPreflightBusy(true);
    try {
      const result = native
        ? await api.applyNativeDocumentVatPreflight(slug, documentId, { confirm: true })
        : await api.applyDocumentVatPreflight(slug, documentId);
      if (mounted.current && latest.current.currentContext === selection.context && latest.current.allowed) {
        setPreflight(result); setPreflightContext(selection.context); setPreflightError(null);
      }
    } finally {
      if (mounted.current) setPreflightBusy(false);
    }
  }


  async function handleBook() {
    if (submitRef.current || done || outcome.isBlocked() || providerBlock.isBlocked() || preflightBusy || nativeValidationBlocked) return;
    if (!expenseAccountNo) {
      setError("Vælg en udgiftskonto.");
      return;
    }
    if (typeof bankTransactionId !== "number") {
      setError("Vælg en banktransaktion at parre bilaget med.");
      return;
    }
    submitRef.current = true;
    setBusy(true);
    setError(null);
    setLocked(null);
    try {
      const summary = await api.bookDocumentExpense(slug, {
        documentId,
        bankTransactionId,
        expenseAccountNo,
        ...(vatTreatment ? { vatTreatment } : {}),
      }).catch(outcome.reject);
      setDone(summary);
      setDirty(false);
      onBooked();
    } catch (err) {
      const e = err as MaybeApiError;
      const outcomeUncertain = e?.code === "network" || e?.code === "internal";
      const message = outcomeUncertain ? "Serverens resultat kunne ikke bekræftes. Kontrollér bilaget og posteringerne, før du bogfører igen." : e?.message ?? "Bogføringen kunne ikke gennemføres.";
      if (e?.code === "conflict") setLocked(message);
      else setError(message);
    } finally {
      submitRef.current = false;
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
    <Dialog title="Bogfør bilag" onClose={guard.onClose} busy={busy} mode={presentation} initialFocusRef={closeRef}>
      <div className="workflow-form" onChange={() => setDirty(true)}>
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
                  {currency !== "DKK" && done.grossAmountDkk !== null && (
                    <> {formatKroner(done.grossAmountDkk, "DKK")} ·</>
                  )}
                  Nettobeløb:{" "}
                  {done.netAmountDkk !== null || done.netAmount !== null
                    ? formatKroner(done.netAmountDkk ?? done.netAmount!, "DKK")
                    : "—"}{" "}
                  · Købsmoms:{" "}
                  {done.vatAmountDkk !== null || done.vatAmount !== null
                    ? formatKroner(done.vatAmountDkk ?? done.vatAmount!, "DKK")
                    : "—"}
                  .
                </p>
              )}
            </div>
            <div className="modal-actions">
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
              {options === null && !loadError && (
                <p className="muted">Henter bogføringsdata…</p>
              )}
              {nativeInternalVoucher && <p role="status">Internt bilag: ekstern leverandørmoms valideres ikke. Serveren kontrollerer fortsat det bundne bankbilag, bankdatoen og momsfri behandling.</p>}
              {nativeInternalVoucher && !nativeBoundInternalVoucher && <p role="alert">Det bundne bankbilag er ikke tilgængeligt. Opdatér bilag og bank, før du bogfører.</p>}
              {preflightLoading && <p role="status">Henter momsvalidering…</p>}
              {preflightError && <Banner kind="error">{preflightError} <Button variant="secondary" onClick={() => setPreflightReload(n => n + 1)}>Prøv momsvalidering igen</Button></Banner>}
              {providerBlock.blocked && !preflightBusy && !review && <UnknownMutationNotice onRefresh={() => setPreflightReload(n => n + 1)} onRelease={providerBlock.release} persistent={providerBlock.persistent} verifyPersistence={providerBlock.verifyPersistence} />}
              {doc && (
                <>
                  <p>
                    <strong>
                      {doc.documentType === "internal_voucher"
                        ? "Internt bilag"
                        : doc.supplierName ?? "Ukendt leverandør"}
                    </strong>
                    {doc.invoiceNo ? ` · faktura ${doc.invoiceNo}` : ""} ·{" "}
                    {doc.invoiceDate ?? "—"} ·{" "}
                    {doc.amountIncVat !== null
                      ? formatKroner(doc.amountIncVat, currency)
                      : "—"}{" "}
                    inkl. moms
                  </p>
                  <p className="muted">
                    Vælg den udgiftskonto bilaget hører til. {doc.documentType === "internal_voucher"
                      ? "Banktransaktionen og den momsfrie behandling er låst til bilagets evidens. "
                      : "Vælg også den banktransaktion det betaler. "}Selve posteringen og bilagets
                    moms-beregning dannes af regnskabskernen — samme vej som
                    via kommandolinjen.
                  </p>
                  {(doc.supplierCountryCode || doc.supplierIdentifierKind || doc.supplierIdentityStatus) && (
                    <p className="muted">
                      Leverandøridentitet: {doc.supplierCountryCode ?? "—"} · {doc.supplierIdentifierKind ?? "—"} · {doc.supplierIdentityStatus ?? "—"}
                      {doc.supplierVatOrCvr ? ` · ${doc.supplierVatOrCvr}` : ""}
                    </p>
                  )}
                  {doc.purchaseVatLines && doc.purchaseVatLines.length > 0 && (
                    <div className="muted" role="group" aria-label="Momsfordeling">
                      Momsfordeling: {doc.purchaseVatLines.map((line) => `${line.classification}: ${formatKroner(line.netAmount, currency)} + ${formatKroner(line.vatAmount ?? 0, currency)}`).join(" · ")}
                    </div>
                  )}
                  {visiblePreflight && (
                    <div className="muted" role="status">
                      Momsvalidering: {{ DK: "Danmark", EU: "EU", NON_EU: "Uden for EU", CONFLICT: "Modstridende landeoplysninger" }[visiblePreflight.derivedRegion]}
                      {visiblePreflight.requiredValidation ? ` · kræver ${visiblePreflight.requiredValidation}` : " · ingen ekstern validering kræves"}
                      {visiblePreflight.cache.freshUntil ? ` · evidens gyldig til ${visiblePreflight.cache.freshUntil}` : ""}.
                      {visiblePreflight.errors.length ? ` ${visiblePreflight.errors.join(" · ")}` : visiblePreflight.ok ? " Momsoplysningerne er valideret." : " Momsoplysningerne skal afklares før bogføring."}
                      <p>Cache: {visiblePreflight.cache.reused ? "genbrugt" : "ikke genbrugt"} · Status: {visiblePreflight.ok ? "godkendt" : "kræver afklaring"}</p>
                      {visiblePreflight.exception && <p>Undtagelse ({visiblePreflight.exception.status}): {visiblePreflight.exception.message} {visiblePreflight.exception.requiredAction}</p>}
                      {"evidence" in visiblePreflight && (visiblePreflight as NativeDocumentVatPreflight).evidence.map(row => <p key={row.id}>{row.event_type} · {row.provider_status} · {row.created_at}</p>)}
                      {visiblePreflight.applyWouldCallProvider && (
                        <Button type="button" className="btn btn-secondary" disabled={busy || uncertain || preflightBusy || !allowed} requiredPermission="company.external-lookup" onClick={reviewPreflight}>
                          {preflightBusy ? "Validerer…" : "Hent momsvalidering"}
                        </Button>
                      )}
                    </div>
                  )}
                </>
              )}
            </div>

            {locked && <LockBanner message={locked} />}
            {error && <Banner kind="error">{error}</Banner>}
            {outcome.feedback}
            {readBack && uncertain && <section className="card" role="status" aria-label="Bilagsstatus fra serveren">
              <h2>Bilagsstatus fra serveren</h2>
              {readBack.some(document => document.journalEntryId !== null) ? <><p>Serveren viser følgende tilknyttede posteringer. Kontrollér, at de svarer til din handling.</p><ul>{readBack.filter(document => document.journalEntryId !== null).map((document, index) => <li key={`${document.journalEntryId}:${index}`}>{document.journalEntryNo ?? `Journalpost ${document.journalEntryId}`} · {document.journalEntryText ?? "Ingen posteringstekst"}</li>)}</ul></> : <p>Serveren viser endnu ingen tilknyttede posteringer. Det afklarer ikke i sig selv den afbrudte handling.</p>}
              <p>Bogføring er fortsat blokeret. Gennemgå bilagsdetaljerne og revisionssporet ved tvivl.</p>
            </section>}
            {loadError && <Banner kind="error">{loadError} <Button variant="secondary" onClick={optionsState.reload}>Prøv igen</Button></Banner>}

            <label className="modal-field">
              Udgiftskonto
              {options === null ? (
                <Select disabled>
                  <option>Henter konti…</option>
                </Select>
              ) : noAccounts ? (
                <Select disabled>
                  <option>Ingen udgiftskonti — kør først kontoplanen.</option>
                </Select>
              ) : (
                <Select
                  value={expenseAccountNo}
                  onChange={(e) => setExpenseAccountNo(e.target.value)}
                  disabled={busy || uncertain}
                >
                  <option value="">— vælg konto —</option>
                  {options.expenseAccounts.map((a) => (
                    <option key={a.accountNo} value={a.accountNo}>
                      {a.accountNo} · {a.name}
                    </option>
                  ))}
                </Select>
              )}
            </label>

            <label className="modal-field">
              Banktransaktion (uafstemt, udgående)
              {options === null ? (
                <Select disabled>
                  <option>Henter banktransaktioner…</option>
                </Select>
              ) : noneToMatch ? (
                <Select disabled>
                  <option>
                    Ingen uafstemte udgående banktransaktioner — importér først
                    bank-CSV.
                  </option>
                </Select>
              ) : (
                <Select
                  value={
                    bankTransactionId === "" ? "" : String(bankTransactionId)
                  }
                  onChange={(e) => {
                    const v = e.target.value;
                    setBankTransactionId(v === "" ? "" : Number(v));
                  }}
                  disabled={busy || uncertain || doc?.documentType === "internal_voucher"}
                >
                  <option value="">— vælg banktransaktion —</option>
                  {options.unmatchedOutgoingBank.map((t) => (
                    <option key={t.id} value={String(t.id)}>
                      {t.date} · {t.text} ·{" "}
                      {formatKroner(t.amount, t.currency)}
                    </option>
                  ))}
                </Select>
              )}
            </label>

            <label className="modal-field">
              Moms-behandling (valgfri — udledes ellers af kontoen)
              <Select
                value={vatTreatment}
                onChange={(e) =>
                  setVatTreatment(
                    e.target.value === ""
                      ? ""
                      : (e.target.value as ExpenseVatTreatment),
                  )
                }
                disabled={
                  busy || uncertain || visiblePreflight?.ok === false || preflightBusy || nativeValidationBlocked ||
                  options === null ||
                  doc?.documentType === "internal_voucher"
                }
              >
                <option value="">— udled fra konto —</option>
                {(Object.keys(VAT_TREATMENT_LABELS) as ExpenseVatTreatment[]).map(
                  (k) => (
                    <option key={k} value={k}>
                      {VAT_TREATMENT_LABELS[k]}
                    </option>
                  ),
                )}
              </Select>
            </label>

            <div className="modal-actions">
              <Button
                type="button"
                className="btn secondary"
                onClick={guard.onClose}
                disabled={busy || uncertain}
                ref={closeRef}
              >
                Annullér
              </Button>
              <Button
                type="button"
                className="btn"
                requiredPermission="company.ledger.post" onClick={handleBook}
                disabled={
                  busy || uncertain || visiblePreflight?.ok === false || preflightBusy || nativeValidationBlocked ||
                  options === null ||
                  noneToMatch ||
                  noAccounts ||
                  !expenseAccountNo ||
                  typeof bankTransactionId !== "number"
                }
              >
                {busy ? "Bogfører…" : "Bogfør"}
              </Button>
            </div>
          </>
        )}
      </div>
      {review && review.context === currentContext && <ConfirmDialog title={providerTitle} body={review.body} confirmLabel="Hent momsvalidering" operationScope={providerScope} operationKey="provider" confirmDisabled={!allowed || review.preflight !== visiblePreflight} onConfirm={applyPreflight} onClose={() => setReview(null)} onRefresh={() => setPreflightReload(n => n + 1)} />}
      {guard.confirmation}
    </Dialog>
  );
}

// `ApiError` is re-imported above purely so the build tracks the dependency;
// the runtime branch reads `code`/`message` off the thrown value rather than
// instance-checking, mirroring `BankReconcileModal`'s shape.
void ApiError;

/** Compatibility wrapper for callers that still need a short dialog. */
export function DocumentBookExpenseModal(props: DocumentBookExpenseModalProps) {
  return <DocumentBookExpenseForm {...props} presentation="dialog" />;
}
