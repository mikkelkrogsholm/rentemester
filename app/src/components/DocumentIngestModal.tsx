// DocumentIngestModal — the human document-intake action for the Cockpit
// (#213, slice 3).
//
// A person opens this from the Bilag view, picks a voucher/document file (a
// PDF, image or text receipt) and fills in its metadata. The file may be
// binary, so the browser base64-encodes it and POSTs it inline (no multipart);
// the server decodes it to a temp file and runs the same `ingestDocument`
// core function the CLI and MCP use.
//
// The modal owns the file picker, the busy state and inline error/lock
// rendering — it mirrors `ConfirmDialog`'s shape and reuses the shared
// `LockBanner` for a 409 backup-lock rejection.

import { useEffect, useRef, useState } from "react";
import { api, type DocumentIngestMetadata } from "../lib/api";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";

/** Shape of the API error the cockpit's `api.ts` throws. */
type MaybeApiError = { code?: string; message?: string };

type EditablePurchaseVatLine = {
  classification: "dk_purchase_25" | "exempt";
  netAmount: string;
  vatAmount: string;
};

export type DocumentIngestModalProps = {
  /** Company slug the ingest targets. */
  slug: string;
  /** Re-runs the Bilag view load after a successful ingest. */
  onIngested: () => void;
  /** Closes the modal without acting. */
  onClose: () => void;
};

/** Reads a (possibly binary) file as a base64 string, chunked to stay safe. */
async function fileToBase64(file: File): Promise<string> {
  const buffer = await file.arrayBuffer();
  const bytes = new Uint8Array(buffer);
  let binary = "";
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(binary);
}

export function DocumentIngestModal({
  slug,
  onIngested,
  onClose,
}: DocumentIngestModalProps) {
  const [fileName, setFileName] = useState<string | null>(null);
  const [fileBase64, setFileBase64] = useState<string | null>(null);
  const [documentType, setDocumentType] =
    useState<DocumentIngestMetadata["documentType"]>("purchase_sale");
  const [source, setSource] = useState("cockpit-upload");
  const [issueDate, setIssueDate] = useState("");
  const [invoiceNo, setInvoiceNo] = useState("");
  const [deliveryDescription, setDeliveryDescription] = useState("");
  const [amountIncVat, setAmountIncVat] = useState("");
  const [vatAmount, setVatAmount] = useState("");
  const [currency, setCurrency] = useState("DKK");
  const [senderName, setSenderName] = useState("");
  const [senderAddress, setSenderAddress] = useState("");
  const [senderVat, setSenderVat] = useState("");
  const [senderCountryCode, setSenderCountryCode] = useState("");
  const [senderIdentifierKind, setSenderIdentifierKind] = useState<"" | "dk_cvr" | "eu_vat" | "non_eu">("");
  const [recipientName, setRecipientName] = useState("");
  const [recipientAddress, setRecipientAddress] = useState("");
  const [recipientVat, setRecipientVat] = useState("");
  const [reverseChargeWordingExcerpt, setReverseChargeWordingExcerpt] = useState("");
  const [reverseChargeWordingLocation, setReverseChargeWordingLocation] = useState("");
  const [purchaseVatLines, setPurchaseVatLines] = useState<EditablePurchaseVatLine[]>([]);
  const [sourceBankTransactionId, setSourceBankTransactionId] = useState("");
  const [internalVoucherKind, setInternalVoucherKind] = useState<"bank_evidenced" | "non_cash_balance_correction">("bank_evidenced");
  const [accountingRationale, setAccountingRationale] = useState("");
  const [payrollPeriod, setPayrollPeriod] = useState("");
  const [payrollReference, setPayrollReference] = useState("");

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [locked, setLocked] = useState<string | null>(null);
  const [done, setDone] = useState<{ documentNo: string | null } | null>(null);
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

  async function handleFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    setError(null);
    if (!file) {
      setFileName(null);
      setFileBase64(null);
      return;
    }
    try {
      setFileBase64(await fileToBase64(file));
      setFileName(file.name);
    } catch {
      setError("Filen kunne ikke læses.");
      setFileName(null);
      setFileBase64(null);
    }
  }

  // A cash-register receipt is exempt from the full statutory field set, so
  // those inputs are only required (and only shown as required) for køb/salg.
  const isPurchaseSale = documentType === "purchase_sale";
  const isInternalVoucher = documentType === "internal_voucher";
  const isExternalPayroll = documentType === "external_accounting_evidence";
  const isNonCashBalanceCorrection = isInternalVoucher && internalVoucherKind === "non_cash_balance_correction";

  async function handleIngest() {
    if (!fileBase64 || !fileName) {
      setError("Vælg en bilagsfil først.");
      return;
    }
    if (!source.trim()) {
      setError("Angiv en kilde.");
      return;
    }
    const amountNum = amountIncVat.trim() ? Number(amountIncVat) : undefined;
    const vatNum = vatAmount.trim() ? Number(vatAmount) : undefined;
    if (amountNum !== undefined && !Number.isFinite(amountNum)) {
      setError("Beløb inkl. moms skal være et tal.");
      return;
    }
    if (vatNum !== undefined && !Number.isFinite(vatNum)) {
      setError("Momsbeløb skal være et tal.");
      return;
    }
    const bankTransactionId = sourceBankTransactionId.trim()
      ? Number(sourceBankTransactionId)
      : undefined;
    if (
      isInternalVoucher && !isNonCashBalanceCorrection &&
      (!Number.isInteger(bankTransactionId) || Number(bankTransactionId) <= 0)
    ) {
      setError("Angiv den importerede banktransaktions id.");
      return;
    }
    if (
      isInternalVoucher &&
      (!issueDate.trim() || !deliveryDescription.trim() || !(Number(amountNum) > 0))
    ) {
      setError("Internt bilag kræver dato, beskrivelse og et positivt beløb.");
      return;
    }
    if (isInternalVoucher && !accountingRationale.trim()) {
      setError("Angiv den regnskabsmæssige begrundelse.");
      return;
    }
    if (isExternalPayroll && (!issueDate.trim() || !recipientName.trim() || !senderName.trim() || !/^\d{4}-(0[1-9]|1[0-2])$/.test(payrollPeriod) || !payrollReference.trim() || !(Number(amountNum) > 0))) {
      setError("Eksternt lønbilag kræver rapportdato, udsteder, virksomhed, lønperiode, ekstern reference og samlet beløb.");
      return;
    }
    let parsedPurchaseVatLines: NonNullable<DocumentIngestMetadata["purchaseVatLines"]> = [];
    try {
      parsedPurchaseVatLines = isPurchaseSale ? purchaseVatLines.map((line, index) => {
        const netAmount = Number(line.netAmount);
        const lineVatAmount = line.vatAmount.trim() === "" ? 0 : Number(line.vatAmount);
        if (!line.netAmount.trim() || !Number.isFinite(netAmount) || netAmount < 0) {
          throw new Error(`Nettobeløb på momslinje ${index + 1} skal være et ikke-negativt tal.`);
        }
        if (!Number.isFinite(lineVatAmount) || lineVatAmount < 0) {
          throw new Error(`Momsbeløb på momslinje ${index + 1} skal være et ikke-negativt tal.`);
        }
        return {
          classification: line.classification,
          netAmount,
          vatAmount: lineVatAmount,
        };
      }) : [];
    } catch (lineError) {
      setError(lineError instanceof Error ? lineError.message : "Momsfordelingen er ugyldig.");
      return;
    }

    const metadata: DocumentIngestMetadata = {
      source: source.trim(),
      documentType,
      currency: currency.trim() || "DKK",
    };
    if (issueDate.trim()) metadata.issueDate = issueDate.trim();
    if (invoiceNo.trim()) metadata.invoiceNo = invoiceNo.trim();
    if (deliveryDescription.trim())
      metadata.deliveryDescription = deliveryDescription.trim();
    if (amountNum !== undefined) metadata.amountIncVat = amountNum;
    if (isInternalVoucher || isExternalPayroll) metadata.vatAmount = 0;
    else if (vatNum !== undefined) metadata.vatAmount = vatNum;
    if (isInternalVoucher) {
      metadata.internalVoucherKind = internalVoucherKind;
      if (!isNonCashBalanceCorrection) metadata.sourceBankTransactionId = bankTransactionId!;
      metadata.accountingRationale = accountingRationale.trim();
    }
    if (isExternalPayroll) metadata.externalAccountingEvidence = { category: "payroll", accountingPeriod: payrollPeriod, externalReference: payrollReference.trim(), totals: { debitAmount: amountNum!, creditAmount: amountNum! } };
    if (isPurchaseSale && parsedPurchaseVatLines.length > 0) {
      metadata.purchaseVatLines = parsedPurchaseVatLines;
    }
    if (isPurchaseSale && (reverseChargeWordingExcerpt.trim() || reverseChargeWordingLocation.trim())) {
      if (!reverseChargeWordingExcerpt.trim() || !reverseChargeWordingLocation.trim()) {
        setError("Angiv både ordlyd og placering på bilaget for omvendt betalingspligt.");
        return;
      }
      metadata.reverseChargeWordingEvidence = {
        excerpt: reverseChargeWordingExcerpt.trim(),
        location: reverseChargeWordingLocation.trim(),
      };
    }
    if (senderName.trim() || senderAddress.trim() || senderVat.trim() || senderCountryCode.trim() || senderIdentifierKind) {
      metadata.sender = {
        name: senderName.trim() || undefined,
        address: senderAddress.trim() || undefined,
        vatOrCvr: senderVat.trim() || undefined,
        countryCode: senderCountryCode.trim() || undefined,
        identifierKind: senderIdentifierKind || undefined,
      };
    }
    if (
      recipientName.trim() ||
      recipientAddress.trim() ||
      recipientVat.trim()
    ) {
      metadata.recipient = {
        name: recipientName.trim() || undefined,
        address: recipientAddress.trim() || undefined,
        vatOrCvr: recipientVat.trim() || undefined,
      };
    }

    setBusy(true);
    setError(null);
    setLocked(null);
    try {
      const result = await api.ingestDocument(slug, {
        fileName,
        fileBase64,
        metadata,
      });
      setDone({ documentNo: result.documentNo });
      onIngested();
    } catch (err) {
      const e = err as MaybeApiError;
      const message = e?.message ?? "Bilaget kunne ikke indlæses.";
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
        aria-label="Indlæs bilag"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="modal-title">Indlæs bilag</h3>

        {done ? (
          <>
            <div className="modal-body">
              <p>
                Bilaget er indlæst
                {done.documentNo ? ` som ${done.documentNo}` : ""}.
              </p>
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
                Vælg en bilagsfil (PDF, billede eller tekst) og udfyld
                oplysningerne. Interne bilag skal bindes til den importerede
                bankpost, der udgør det primære bevis.
              </p>
            </div>

            {locked && <LockBanner message={locked} />}
            {error && <Banner kind="error">{error}</Banner>}

            <label className="modal-field">
              Bilagsfil
              <input
                type="file"
                accept=".pdf,.png,.jpg,.jpeg,.txt,application/pdf,image/png,image/jpeg,text/plain"
                onChange={handleFile}
                disabled={busy}
              />
            </label>
            {fileName && (
              <p className="muted" style={{ margin: 0 }}>
                Valgt: {fileName}
              </p>
            )}

            <div className="modal-field-grid">
              <label className="modal-field">
                Bilagstype
                <select
                  value={documentType}
                  onChange={(e) =>
                    setDocumentType(
                      e.target.value as DocumentIngestMetadata["documentType"],
                    )
                  }
                  disabled={busy}
                >
                  <option value="purchase_sale">Køb/salg</option>
                  <option value="cash_register_receipt">Kassebon</option>
                  <option value="internal_voucher">Internt bilag</option>
                  <option value="external_accounting_evidence">Eksternt lønbilag</option>
                </select>
              </label>
              <label className="modal-field">
                Kilde
                <input
                  type="text"
                  value={source}
                  onChange={(e) => setSource(e.target.value)}
                  disabled={busy}
                />
              </label>
            </div>

            {isExternalPayroll && <div className="modal-field-grid"><label className="modal-field">Lønperiode<input type="month" value={payrollPeriod} onChange={(e) => setPayrollPeriod(e.target.value)} disabled={busy} /></label><label className="modal-field">Ekstern lønreference<input type="text" value={payrollReference} onChange={(e) => setPayrollReference(e.target.value)} disabled={busy} /></label></div>}

            <div className="modal-field-grid">
              <label className="modal-field">
                Bilagsdato{isPurchaseSale || isInternalVoucher || isExternalPayroll ? "" : " (valgfri)"}
                <input
                  type="date"
                  value={issueDate}
                  onChange={(e) => setIssueDate(e.target.value)}
                  disabled={busy}
                />
              </label>
              <label className="modal-field">
                Fakturanr. (valgfri)
                <input
                  type="text"
                  value={invoiceNo}
                  onChange={(e) => setInvoiceNo(e.target.value)}
                  disabled={busy}
                />
              </label>
            </div>

            <div className="modal-field-grid">
              <label className="modal-field">
                {isInternalVoucher
                  ? "Beløb"
                  : isExternalPayroll
                    ? "Samlet lønrapport (debet/kredit)"
                  : `Beløb inkl. moms${isPurchaseSale ? "" : " (valgfri)"}`}
                <input
                  type="number"
                  inputMode="decimal"
                  value={amountIncVat}
                  onChange={(e) => setAmountIncVat(e.target.value)}
                  disabled={busy}
                />
              </label>
              <label className="modal-field">
                Momsbeløb{isInternalVoucher || isExternalPayroll ? " (altid 0)" : isPurchaseSale ? "" : " (valgfri)"}
                <input
                  type="number"
                  inputMode="decimal"
                  value={vatAmount}
                  onChange={(e) => setVatAmount(e.target.value)}
                  disabled={busy || isInternalVoucher || isExternalPayroll}
                  placeholder={isInternalVoucher || isExternalPayroll ? "0" : undefined}
                />
              </label>
            </div>

            <label className="modal-field">
              Valuta
              <input
                type="text"
                value={currency}
                onChange={(e) => setCurrency(e.target.value)}
                disabled={busy}
              />
            </label>

            {(isPurchaseSale || isInternalVoucher) && (
              <label className="modal-field">
                {isInternalVoucher ? "Beskrivelse" : "Beskrivelse af leverance"}
                <input
                  type="text"
                  value={deliveryDescription}
                  onChange={(e) => setDeliveryDescription(e.target.value)}
                  disabled={busy}
                />
              </label>
            )}

            {isInternalVoucher && (
              <>
                <label className="modal-field">
                  Intern bilagstype
                  <select value={internalVoucherKind} onChange={(e) => setInternalVoucherKind(e.target.value as "bank_evidenced" | "non_cash_balance_correction")} disabled={busy}>
                    <option value="bank_evidenced">Bankdokumenteret</option>
                    <option value="non_cash_balance_correction">Balancekorrektion uden bankbevægelse</option>
                  </select>
                </label>
                {isNonCashBalanceCorrection && <p className="muted">Kun to balancekonti, dato, valuta og beløb skal senere stemme præcist med journalen. Moms er altid 0.</p>}
                {!isNonCashBalanceCorrection &&
                <label className="modal-field">
                  Banktransaktions-id
                  <input
                    type="number"
                    min="1"
                    step="1"
                    value={sourceBankTransactionId}
                    onChange={(e) => setSourceBankTransactionId(e.target.value)}
                    disabled={busy}
                  />
                </label>
                }
                <label className="modal-field">
                  Regnskabsmæssig begrundelse
                  <textarea
                    value={accountingRationale}
                    onChange={(e) => setAccountingRationale(e.target.value)}
                    disabled={busy}
                    maxLength={2000}
                  />
                </label>
              </>
            )}

            {(isPurchaseSale || isExternalPayroll) && (
              <fieldset className="modal-field">
                <legend>Momsfordeling (valgfri)</legend>
                <p className="muted">
                  Brug linjer når kun en del af fakturaen er momspligtig. Summen
                  af nettobeløb og moms skal svare til fakturaens total.
                </p>
                {purchaseVatLines.map((line, index) => (
                  <div className="modal-field-grid" key={index}>
                    <label className="modal-field">
                      Momsart {index + 1}
                      <select
                        value={line.classification}
                        onChange={(e) => setPurchaseVatLines((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, classification: e.target.value as EditablePurchaseVatLine["classification"] } : item))}
                        disabled={busy}
                      >
                        <option value="dk_purchase_25">Dansk køb, 25 %</option>
                        <option value="exempt">Momsfrit/udlæg</option>
                      </select>
                    </label>
                    <label className="modal-field">
                      Nettobeløb {index + 1}
                      <input
                        type="number"
                        inputMode="decimal"
                        value={line.netAmount}
                        onChange={(e) => setPurchaseVatLines((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, netAmount: e.target.value } : item))}
                        disabled={busy}
                      />
                    </label>
                    <label className="modal-field">
                      Momsbeløb {index + 1}
                      <input
                        type="number"
                        inputMode="decimal"
                        value={line.vatAmount}
                        onChange={(e) => setPurchaseVatLines((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, vatAmount: e.target.value } : item))}
                        disabled={busy}
                      />
                    </label>
                    <button
                      type="button"
                      className="btn secondary"
                      aria-label={`Fjern momslinje ${index + 1}`}
                      onClick={() => setPurchaseVatLines((current) => current.filter((_, itemIndex) => itemIndex !== index))}
                      disabled={busy}
                    >
                      Fjern
                    </button>
                  </div>
                ))}
                <button
                  type="button"
                  className="btn secondary"
                  onClick={() => setPurchaseVatLines((current) => [...current, { classification: "dk_purchase_25", netAmount: "", vatAmount: "" }])}
                  disabled={busy}
                >
                  Tilføj momslinje
                </button>
              </fieldset>
            )}

            {(isPurchaseSale || isExternalPayroll) && (
              <>
                <div className="modal-field-grid">
                  <label className="modal-field">
                    Afsender
                    <input
                      type="text"
                      value={senderName}
                      placeholder="Navn"
                      onChange={(e) => setSenderName(e.target.value)}
                      disabled={busy}
                    />
                  </label>
                  <label className="modal-field">
                    Afsender CVR/moms
                    <input
                      type="text"
                      value={senderVat}
                      onChange={(e) => setSenderVat(e.target.value)}
                      disabled={busy}
                    />
                  </label>
                  <label className="modal-field">
                    Leverandørland (ISO)
                    <input type="text" value={senderCountryCode} onChange={(e) => setSenderCountryCode(e.target.value.toUpperCase())} placeholder="US" maxLength={2} disabled={busy} />
                  </label>
                  <label className="modal-field">
                    Identitetstype
                    <select value={senderIdentifierKind} onChange={(e) => setSenderIdentifierKind(e.target.value as typeof senderIdentifierKind)} disabled={busy}>
                      <option value="">Vælg</option><option value="dk_cvr">Dansk CVR</option><option value="eu_vat">EU-momsnr.</option><option value="non_eu">Ikke-EU</option>
                    </select>
                  </label>
                </div>
                <label className="modal-field">
                  Afsenderadresse
                  <input
                    type="text"
                    value={senderAddress}
                    onChange={(e) => setSenderAddress(e.target.value)}
                    disabled={busy}
                  />
                </label>
                <div className="modal-field-grid">
                  <label className="modal-field">
                    Modtager
                    <input
                      type="text"
                      value={recipientName}
                      placeholder="Navn"
                      onChange={(e) => setRecipientName(e.target.value)}
                      disabled={busy}
                    />
                  </label>
                  <label className="modal-field">
                    Modtager CVR/moms
                    <input
                      type="text"
                      value={recipientVat}
                      onChange={(e) => setRecipientVat(e.target.value)}
                      disabled={busy}
                    />
                  </label>
                </div>
                <label className="modal-field">
                  Modtageradresse
                  <input
                    type="text"
                    value={recipientAddress}
                    onChange={(e) => setRecipientAddress(e.target.value)}
                    disabled={busy}
                  />
                </label>
                {isPurchaseSale && senderIdentifierKind === "non_eu" && (
                  <div className="modal-field-grid">
                    <label className="modal-field">
                      Ordlyd om omvendt betalingspligt (valgfri)
                      <input value={reverseChargeWordingExcerpt} onChange={(e) => setReverseChargeWordingExcerpt(e.target.value)} placeholder="Citat fra bilaget" disabled={busy} />
                    </label>
                    <label className="modal-field">
                      Placering på bilaget
                      <input value={reverseChargeWordingLocation} onChange={(e) => setReverseChargeWordingLocation(e.target.value)} placeholder="Fx side 1" disabled={busy} />
                    </label>
                  </div>
                )}
              </>
            )}

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
                onClick={handleIngest}
                disabled={busy || !fileBase64}
              >
                {busy ? "Indlæser…" : "Indlæs bilag"}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
