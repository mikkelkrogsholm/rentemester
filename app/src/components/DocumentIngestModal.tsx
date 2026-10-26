import { cockpitStyles } from "../design/cockpit.stylex";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { parseDanishAmount } from "../lib/format";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import * as stylex from "@stylexjs/stylex";
import { Button, Dialog, Input, Select, Textarea, MoneyInput } from "./ui";
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
import { api, type DocumentIngestMetadata, type DocumentIngestInput } from "../lib/api";
import { Banner } from "./Feedback";
import { useAsync } from "../lib/useAsync";
import { LockBanner } from "./LockBanner";

/** Shape of the API error the cockpit's `api.ts` throws. */
type MaybeApiError = { code?: string; message?: string };

type EditablePurchaseVatLine = {
  classification: "" | "dk_purchase_25" | "exempt";
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
  /** Explicit alternate intake; ordinary company upload keeps its existing default. */
  submit?: (input: DocumentIngestInput, mimeType: string) => Promise<{ documentNo: string | null }>;
  title?: string;
  successText?: string;
  allowInternalVoucher?: boolean;
  /** Review a retained attachment; no file selection, bytes or ingestion request. */
  metadataReview?: {
    filename: string; source: string; sourceHash: string; originalUrl: string;
    metadata?: Partial<DocumentIngestMetadata>;
    save: (metadata: DocumentIngestMetadata) => Promise<void>;
  };
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
  onClose: onDismiss,
  submit, title = "Indlæs bilag", successText, allowInternalVoucher = true, metadataReview,
}: DocumentIngestModalProps) {
  const initial = metadataReview?.metadata;
  const initialCurrency = metadataReview ? initial?.currency ?? "" : "DKK";
  const initialType = metadataReview ? initial?.documentType ?? "" : "purchase_sale";
  const initialSource = metadataReview?.source ?? "cockpit-upload";
  const moneyText = (value: number | undefined) => value == null ? "" : String(value).replace(".", ",");
  const [metadataConfirmed, setMetadataConfirmed] = useState(false);
  const contacts = useAsync((signal) => api.contacts(slug, { signal }), [slug]);
  const [selectedVendor, setSelectedVendor] = useState("");
  const [mimeType, setMimeType] = useState("application/octet-stream");
  const [fileName, setFileName] = useState<string | null>(null);
  const [fileBase64, setFileBase64] = useState<string | null>(null);
  const [readingFile, setReadingFile] = useState(false);
  const fileSelection = useRef(0);
  useEffect(() => () => { fileSelection.current++; }, []);
  const [documentType, setDocumentType] =
    useState<DocumentIngestMetadata["documentType"] | "">(initialType);
  const [source, setSource] = useState(initialSource);
  const [issueDate, setIssueDate] = useState(initial?.issueDate ?? "");
  const [invoiceNo, setInvoiceNo] = useState(initial?.invoiceNo ?? "");
  const [deliveryDescription, setDeliveryDescription] = useState(initial?.deliveryDescription ?? "");
  const [amountIncVat, setAmountIncVat] = useState(moneyText(initial?.amountIncVat));
  const [vatAmount, setVatAmount] = useState(moneyText(initial?.vatAmount));
  const [currency, setCurrency] = useState(initialCurrency);
  const [senderName, setSenderName] = useState(initial?.sender?.name ?? "");
  const [senderAddress, setSenderAddress] = useState(initial?.sender?.address ?? "");
  const [senderVat, setSenderVat] = useState(initial?.sender?.vatOrCvr ?? "");
  const [senderCountryCode, setSenderCountryCode] = useState(initial?.sender?.countryCode ?? "");
  const [senderIdentifierKind, setSenderIdentifierKind] = useState<"" | "dk_cvr" | "eu_vat" | "non_eu">(initial?.sender?.identifierKind ?? "");
  const [recipientName, setRecipientName] = useState(initial?.recipient?.name ?? "");
  const [recipientAddress, setRecipientAddress] = useState(initial?.recipient?.address ?? "");
  const [recipientVat, setRecipientVat] = useState(initial?.recipient?.vatOrCvr ?? "");
  const [reverseChargeWordingConfirmed, setReverseChargeWordingConfirmed] = useState(initial?.reverseChargeWordingConfirmed ?? false);
  const [purchaseVatLines, setPurchaseVatLines] = useState<EditablePurchaseVatLine[]>(initial?.purchaseVatLines?.map(line => ({classification:line.classification,netAmount:moneyText(line.netAmount),vatAmount:moneyText(line.vatAmount)})) ?? []);
  const [sourceBankTransactionId, setSourceBankTransactionId] = useState("");
  const [accountingRationale, setAccountingRationale] = useState("");

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [locked, setLocked] = useState<string | null>(null);
  const [done, setDone] = useState<{ documentNo: string | null } | null>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  // Move focus into the dialog and let Escape dismiss it — basic modal hygiene.


  const outcome = useMutationOutcome(onIngested);
  const guard = useDiscardGuard(!done && (Boolean(fileName || fileBase64 || invoiceNo || issueDate || amountIncVat || vatAmount || senderName || senderAddress || senderVat || senderCountryCode || senderIdentifierKind || recipientName || recipientAddress || recipientVat || deliveryDescription || accountingRationale || sourceBankTransactionId || reverseChargeWordingConfirmed || purchaseVatLines.length) || currency !== initialCurrency || documentType !== initialType || source !== initialSource), () => { fileSelection.current++; onDismiss(); });
  const { onClose } = guard;

  async function handleFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    const selection = ++fileSelection.current;
    setError(null);
    setFileName(file?.name ?? null);
    setFileBase64(null);
    setMimeType(file?.type || "application/octet-stream");
    setReadingFile(Boolean(file));
    if (!file) return;
    try {
      const bytes = await fileToBase64(file);
      if (selection !== fileSelection.current) return;
      setFileBase64(bytes);
    } catch {
      if (selection !== fileSelection.current) return;
      setError("Filen kunne ikke læses.");
      setFileName(null);
      setFileBase64(null);
    } finally {
      if (selection === fileSelection.current) setReadingFile(false);
    }
  }

  // Cash receipts omit invoice-only fields, but expense posting still needs
  // documented supplier evidence. Retain the same explicit identity controls.
  const isPurchaseSale = documentType === "purchase_sale";
  const isInternalVoucher = documentType === "internal_voucher";

  async function handleIngest() {
    if (busy || readingFile || outcome.isBlocked()) return;
    if (!metadataReview && (!fileBase64 || !fileName)) {
      setError("Vælg en bilagsfil først.");
      return;
    }
    if (metadataReview && (!documentType || !currency.trim() || !metadataConfirmed)) {
      setError("Vælg faktisk bilagstype og valuta, og bekræft gennemgangen af originalen.");
      return;
    }
    if (!source.trim()) {
      setError("Angiv en kilde.");
      return;
    }
    const amountNum = amountIncVat.trim() ? parseDanishAmount(amountIncVat) : undefined;
    const vatNum = vatAmount.trim() ? parseDanishAmount(vatAmount) : undefined;
    if (amountNum === null) {
      setError("Beløb inkl. moms skal være et tal.");
      return;
    }
    if (vatNum === null) {
      setError("Momsbeløb skal være et tal.");
      return;
    }
    const bankTransactionId = sourceBankTransactionId.trim()
      ? Number(sourceBankTransactionId)
      : undefined;
    if (
      isInternalVoucher &&
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
    let parsedPurchaseVatLines: NonNullable<DocumentIngestMetadata["purchaseVatLines"]> = [];
    try {
      parsedPurchaseVatLines = isPurchaseSale ? purchaseVatLines.map((line, index) => {
        if (!line.classification) throw new Error(`Vælg momsart på linje ${index + 1}.`);
        if (metadataReview && !line.vatAmount.trim()) throw new Error(`Angiv momsbeløb på linje ${index + 1}, også når det er 0.`);
        const netAmount = parseDanishAmount(line.netAmount);
        const lineVatAmount = line.vatAmount.trim() === "" ? 0 : parseDanishAmount(line.vatAmount);
        if (!line.netAmount.trim() || netAmount === null || netAmount < 0) {
          throw new Error(`Nettobeløb på momslinje ${index + 1} skal være et ikke-negativt tal.`);
        }
        if (lineVatAmount === null || lineVatAmount < 0) {
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
      documentType: documentType || "purchase_sale",
      currency: currency.trim() || "DKK",
    };
    if (issueDate.trim()) metadata.issueDate = issueDate.trim();
    if (invoiceNo.trim()) metadata.invoiceNo = invoiceNo.trim();
    if (deliveryDescription.trim())
      metadata.deliveryDescription = deliveryDescription.trim();
    if (amountNum !== undefined) metadata.amountIncVat = amountNum;
    if (isInternalVoucher) metadata.vatAmount = 0;
    else if (vatNum !== undefined) metadata.vatAmount = vatNum;
    if (isInternalVoucher) {
      metadata.sourceBankTransactionId = bankTransactionId!;
      metadata.accountingRationale = accountingRationale.trim();
    }
    if (isPurchaseSale && parsedPurchaseVatLines.length > 0) {
      metadata.purchaseVatLines = parsedPurchaseVatLines;
    }
    if (isPurchaseSale && reverseChargeWordingConfirmed) {
      metadata.reverseChargeWordingConfirmed = true;
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
      const input: DocumentIngestInput = {
        fileName: fileName!,
        fileBase64: fileBase64!,
        metadata,
      };
      const result = metadataReview
        ? await metadataReview.save(metadata).then(() => ({ documentNo: null })).catch(outcome.reject)
        : await (submit ? submit(input, mimeType) : api.ingestDocument(slug, input)).catch(outcome.reject);
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
    <Dialog title={title} onClose={onClose} busy={busy} initialFocusRef={closeRef}>
    {outcome.feedback}
      {guard.confirmation}



        {done ? (
          <>
            <div {...stylex.props(cockpitStyles.modalBody)}>
              <p {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.modalBodyP,
							)}>
                {successText ?? `Bilaget er indlæst${done.documentNo ? ` som ${done.documentNo}` : ""}.`}
              </p>
            </div>
            <div {...stylex.props(cockpitStyles.modalActions)}>
              <Button
                type="button"

                ref={closeRef}
                onClick={onClose} xstyle={[cockpitStyles.buttonComposition]}
              >
                Luk
              </Button>
            </div>
          </>
        ) : (
          <>
            <div {...stylex.props(cockpitStyles.modalBody)}>
              <p {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.modalBodyP,
							)}>
                {metadataReview ? "Gennemgå den bevarede original og indtast kun de kildeoplysninger, du har kontrolleret. Filen indlæses ikke igen." : "Vælg en bilagsfil (PDF, billede eller tekst) og udfyld oplysningerne."} {allowInternalVoucher ? "Interne bilag skal bindes til den importerede bankpost, der udgør det primære bevis." : "Vælg den faktiske bilagstype. Interne bilag oprettes direkte i virksomheden med dens bankbevis; kontrakter og andre dokumenttyper understøttes endnu ikke i denne indbakke."}
              </p>
            </div>

            {locked && <LockBanner message={locked} />}
            {error && <Banner kind="error">{error}</Banner>}

            {metadataReview ? <section aria-label="Bevaret original"><p>{metadataReview.filename}</p><p>SHA-256: {metadataReview.sourceHash}</p><a href={metadataReview.originalUrl} download>Hent original til gennemgang</a></section> : <label {...stylex.props(cockpitStyles.modalField)}>
              Bilagsfil
              <Input
                type="file"
                accept=".pdf,.png,.jpg,.jpeg,.txt,application/pdf,image/png,image/jpeg,text/plain"
                onChange={handleFile}
                disabled={outcome.blocked || (busy)}
              />
            </label>}
            {fileName && (
              <p {...stylex.props(...[cockpitStyles.muted, viewStyles.site0])} >
                Valgt: {fileName}
              </p>
            )}
            {readingFile && <p role="status">Læser bilagsfil…</p>}

            <div {...stylex.props(cockpitStyles.modalFieldGrid)}>
              <label {...stylex.props(cockpitStyles.modalField)}>
                Bilagstype
                <Select
                  aria-label="Bilagstype"
                  value={documentType}
                  onChange={(e) =>
                    setDocumentType(
                      e.target.value as DocumentIngestMetadata["documentType"],
                    )
                  }
                  disabled={outcome.blocked || (busy)}
                >
                  {metadataReview && <option value="">Vælg faktisk bilagstype</option>}
                  <option value="purchase_sale" {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>Køb/salg</option>
                  <option value="cash_register_receipt" {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>Kassebon</option>
                  {allowInternalVoucher && <option value="internal_voucher" {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>Internt bilag</option>}
                </Select>
              </label>
              <label {...stylex.props(cockpitStyles.modalField)}>
                Kilde
                <Input
                  type="text"
                  value={source}
                  readOnly={Boolean(metadataReview)}
                  onChange={(e) => setSource(e.target.value)}
                  disabled={outcome.blocked || (busy)}
                />
              </label>
            </div>

            <div {...stylex.props(cockpitStyles.modalFieldGrid)}>
              <label {...stylex.props(cockpitStyles.modalField)}>
                Bilagsdato{isPurchaseSale || isInternalVoucher ? "" : " (valgfri)"}
                <Input
                  type="date"
                  value={issueDate}
                  onChange={(e) => setIssueDate(e.target.value)}
                  disabled={outcome.blocked || (busy)}
                />
              </label>
              <label {...stylex.props(cockpitStyles.modalField)}>
                Fakturanr. (valgfri)
                <Input
                  type="text"
                  value={invoiceNo}
                  onChange={(e) => setInvoiceNo(e.target.value)}
                  disabled={outcome.blocked || (busy)}
                />
              </label>
            </div>

            <div {...stylex.props(cockpitStyles.modalFieldGrid)}>
              <label {...stylex.props(cockpitStyles.modalField)}>
                {isInternalVoucher
                  ? "Beløb"
                  : `Beløb inkl. moms${isPurchaseSale ? "" : " (valgfri)"}`}
                <MoneyInput
                  inputMode="decimal"
                  value={amountIncVat}
                  onValueChange={(e) => setAmountIncVat(e)}
                  disabled={outcome.blocked || (busy)}
                />
              </label>
              <label {...stylex.props(cockpitStyles.modalField)}>
                Momsbeløb{isInternalVoucher ? " (altid 0)" : isPurchaseSale ? "" : " (valgfri)"}
                <MoneyInput
                  inputMode="decimal"
                  value={vatAmount}
                  onValueChange={(e) => setVatAmount(e)}
                  disabled={outcome.blocked || (busy || isInternalVoucher)}
                  placeholder={isInternalVoucher ? "0" : undefined}
                />
              </label>
            </div>

            <label {...stylex.props(cockpitStyles.modalField)}>
              Valuta
              <Input
                type="text"
                value={currency}
                onChange={(e) => setCurrency(e.target.value)}
                disabled={outcome.blocked || (busy)}
              />
            </label>

            {(isPurchaseSale || isInternalVoucher) && (
              <label {...stylex.props(cockpitStyles.modalField)}>
                {isInternalVoucher ? "Beskrivelse" : "Beskrivelse af leverance"}
                <Input
                  type="text"
                  value={deliveryDescription}
                  onChange={(e) => setDeliveryDescription(e.target.value)}
                  disabled={outcome.blocked || (busy)}
                />
              </label>
            )}

            {isInternalVoucher && (
              <>
                <label {...stylex.props(cockpitStyles.modalField)}>
                  Banktransaktions-id
                  <Input
                    type="number"
                    min="1"
                    step="1"
                    value={sourceBankTransactionId}
                    onChange={(e) => setSourceBankTransactionId(e.target.value)}
                    disabled={outcome.blocked || (busy)}
                  />
                </label>
                <label {...stylex.props(cockpitStyles.modalField)}>
                  Regnskabsmæssig begrundelse
                  <Textarea
                    value={accountingRationale}
                    onChange={(e) => setAccountingRationale(e.target.value)}
                    disabled={outcome.blocked || (busy)}
                    maxLength={2000}
                  />
                </label>
              </>
            )}

            {isPurchaseSale && (
              <fieldset {...stylex.props(cockpitStyles.modalField)}>
                <legend {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>Momsfordeling (valgfri)</legend>
                <p {...stylex.props(cockpitStyles.muted)}>
                  Brug linjer når kun en del af fakturaen er momspligtig. Summen
                  af nettobeløb og moms skal svare til fakturaens total.
                </p>
                {purchaseVatLines.map((line, index) => (
                  <div {...stylex.props(cockpitStyles.modalFieldGrid)} key={index}>
                    <label {...stylex.props(cockpitStyles.modalField)}>
                      Momsart {index + 1}
                      <Select
                        value={line.classification}
                        onChange={(e) => setPurchaseVatLines((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, classification: e.target.value as EditablePurchaseVatLine["classification"] } : item))}
                        disabled={outcome.blocked || (busy)}
                      >
                        {metadataReview && <option value="">Vælg momsart</option>}
                        <option value="dk_purchase_25" {...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}>Dansk køb, 25 %</option>
                        <option value="exempt" {...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}>Momsfrit/udlæg</option>
                      </Select>
                    </label>
                    <label {...stylex.props(cockpitStyles.modalField)}>
                      Nettobeløb {index + 1}
                      <MoneyInput
                        inputMode="decimal"
                        value={line.netAmount}
                        onValueChange={(e) => setPurchaseVatLines((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, netAmount: e } : item))}
                        disabled={outcome.blocked || (busy)}
                      />
                    </label>
                    <label {...stylex.props(cockpitStyles.modalField)}>
                      Momsbeløb {index + 1}
                      <MoneyInput
                        inputMode="decimal"
                        value={line.vatAmount}
                        onValueChange={(e) => setPurchaseVatLines((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, vatAmount: e } : item))}
                        disabled={outcome.blocked || (busy)}
                      />
                    </label>
                    <Button variant="secondary"
                      type="button"

                      aria-label={`Fjern momslinje ${index + 1}`}
                      onClick={() => setPurchaseVatLines((current) => current.filter((_, itemIndex) => itemIndex !== index))}
                      disabled={busy}
                    >
                      Fjern
                    </Button>
                  </div>
                ))}
                <Button variant="secondary"
                  type="button"

                  onClick={() => setPurchaseVatLines((current) => [...current, { classification: metadataReview ? "" : "dk_purchase_25", netAmount: "", vatAmount: "" }])}
                  disabled={busy}
                >
                  Tilføj momslinje
                </Button>
              </fieldset>
            )}

            {(isPurchaseSale || documentType === "cash_register_receipt") && (
              <>
                {documentType === "cash_register_receipt" && <p {...stylex.props(cockpitStyles.muted)}>Leverandøren skal være dokumenteret, før kassebonen kan bogføres. Udfyld kun oplysninger, du har kontrolleret på bilaget.</p>}
                {contacts.loading && <p {...stylex.props(cockpitStyles.muted)} role="status">Henter leverandører…</p>}
                {contacts.error && <p {...stylex.props(cockpitStyles.muted)}>Leverandører kunne ikke hentes. Udfyld afsenderoplysningerne manuelt.</p>}
                {!!contacts.data?.vendors.length && <label {...stylex.props(cockpitStyles.modalField)}>
                  Udfyld fra leverandør (valgfri)
                  <Select value={selectedVendor} disabled={outcome.blocked || busy} onChange={event => {
                    setSelectedVendor(event.target.value);
                    const vendor = contacts.data?.vendors.find(item => String(item.id) === event.target.value);
                    if (!vendor) return;
                    setSenderName(vendor.name); setSenderAddress(vendor.address ?? ""); setSenderVat(vendor.vatOrCvr ?? "");
                    setSenderCountryCode(vendor.countryCode ?? ""); setSenderIdentifierKind(vendor.identifierKind ?? "");
                  }}><option value="">Udfyld manuelt</option>{contacts.data.vendors.map(vendor => <option key={vendor.id} value={vendor.id}>{vendor.name}</option>)}</Select>
                </label>}
                <div {...stylex.props(cockpitStyles.modalFieldGrid)}>
                  <label {...stylex.props(cockpitStyles.modalField)}>
                    Afsender
                    <Input
                      type="text"
                      value={senderName}
                      placeholder="Navn"
                      onChange={(e) => setSenderName(e.target.value)}
                      disabled={outcome.blocked || (busy)}
                    />
                  </label>
                  <label {...stylex.props(cockpitStyles.modalField)}>
                    Afsender CVR/moms
                    <Input
                      type="text"
                      value={senderVat}
                      onChange={(e) => setSenderVat(e.target.value)}
                      disabled={outcome.blocked || (busy)}
                    />
                  </label>
                  <label {...stylex.props(cockpitStyles.modalField)}>
                    Leverandørland (ISO)
                    <Input type="text" value={senderCountryCode} onChange={(e) => setSenderCountryCode(e.target.value.toUpperCase())} placeholder="US" maxLength={2} disabled={outcome.blocked || (busy)} />
                  </label>
                  <label {...stylex.props(cockpitStyles.modalField)}>
                    Identitetstype
                    <Select value={senderIdentifierKind} onChange={(e) => setSenderIdentifierKind(e.target.value as typeof senderIdentifierKind)} disabled={outcome.blocked || (busy)}>
                      <option value="" {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>Vælg</option><option value="dk_cvr" {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>Dansk CVR</option><option value="eu_vat" {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>EU-momsnr.</option><option value="non_eu" {...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>Ikke-EU</option>
                    </Select>
                  </label>
                </div>
                <label {...stylex.props(cockpitStyles.modalField)}>
                  Afsenderadresse
                  <Input
                    type="text"
                    value={senderAddress}
                    onChange={(e) => setSenderAddress(e.target.value)}
                    disabled={outcome.blocked || (busy)}
                  />
                </label>
              </>
            )}
            {isPurchaseSale && (
              <>
                <div {...stylex.props(cockpitStyles.modalFieldGrid)}>
                  <label {...stylex.props(cockpitStyles.modalField)}>
                    Modtager
                    <Input
                      type="text"
                      value={recipientName}
                      placeholder="Navn"
                      onChange={(e) => setRecipientName(e.target.value)}
                      disabled={outcome.blocked || (busy)}
                    />
                  </label>
                  <label {...stylex.props(cockpitStyles.modalField)}>
                    Modtager CVR/moms
                    <Input
                      type="text"
                      value={recipientVat}
                      onChange={(e) => setRecipientVat(e.target.value)}
                      disabled={outcome.blocked || (busy)}
                    />
                  </label>
                </div>
                <label {...stylex.props(cockpitStyles.modalField)}>
                  Modtageradresse
                  <Input
                    type="text"
                    value={recipientAddress}
                    onChange={(e) => setRecipientAddress(e.target.value)}
                    disabled={outcome.blocked || (busy)}
                  />
                </label>
                {senderIdentifierKind === "non_eu" && (
                  <label {...stylex.props(cockpitStyles.modalField)}>
                    <Input
                      type="checkbox"
                      checked={reverseChargeWordingConfirmed}
                      onChange={(e) => setReverseChargeWordingConfirmed(e.target.checked)}
                      disabled={outcome.blocked || (busy)}
                    />
                    Jeg har kontrolleret, at bilaget indeholder ordlyd om omvendt betalingspligt
                  </label>
                )}
              </>
            )}

            {metadataReview && <label {...stylex.props(cockpitStyles.modalField)}><Input type="checkbox" checked={metadataConfirmed} onChange={e => setMetadataConfirmed(e.target.checked)} disabled={outcome.blocked || busy} />Jeg har gennemgået originalen, målvirksomheden og de indtastede kildeoplysninger.</label>}
            <div {...stylex.props(cockpitStyles.modalActions)}>
              <Button variant="secondary"
                type="button"

                onClick={onClose}
                disabled={busy}
              >
                Annullér
              </Button>
              <Button requiredPermission="company.documents.upload"
                type="button"

                onClick={handleIngest}
                disabled={outcome.blocked || busy || readingFile || (metadataReview ? !metadataConfirmed || !documentType || !currency.trim() : !fileBase64)}
              >
                {busy ? "Gemmer…" : metadataReview ? "Gem gennemgåede kildeoplysninger" : "Indlæs bilag"}
              </Button>
            </div>
          </>
        )}

    </Dialog>
  );
}

const viewStyles = stylex.create({
site0: { margin: 0 }
});
