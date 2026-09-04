import type { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { extname, join } from "node:path";
import { insertAuditLog, resolveActor } from "./actor";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import {
  type DocumentSnapshot,
  ensureCanonicalDocumentStore,
  publishDocumentSnapshot,
  removePublishedSnapshot,
  snapshotDocumentSource,
  snapshotRegisteredDocument,
} from "./document-storage";
import { strengthenGdprErasureAliasesForIdentity } from "./gdpr";
import { asDocumentId, type DocumentId } from "./ids";
import { compareDkk, percentOfDkk, roundDkk, sumDkk } from "./money";
import { companyPaths } from "./paths";
import { retainUntilForDate } from "./retention";
import { companySequenceScope, currentUtcIsoDate, fiscalYearLabelFromDate, nextSequenceValue } from "./sequences";
import { resolveLegacySupplierIdentity, resolveSupplierIdentity, type SupplierIdentifierKind } from "./supplier-identity";

export type DocumentType =
  | "purchase_sale"
  | "cash_register_receipt"
  | "issued_invoice_pdf"
  | "internal_voucher"
  | "external_accounting_evidence";
export type DocumentExemptionCode = "FOREIGN_PHYSICAL_ONLY" | null;
export type PurchaseVatClassification = "dk_purchase_25" | "exempt";
export type PurchaseVatLine = { classification: PurchaseVatClassification; netAmount: number; vatAmount?: number };
export type InternalVoucherKind = "bank_evidenced" | "non_cash_balance_correction" | "legacy_opening_creditor_reclassification";

export type DocumentMetadata = {
  source: string;
  documentType?: DocumentType;
  issueDate?: string;
  invoiceNo?: string;
  deliveryDescription?: string;
  amountIncVat?: number;
  currency?: string;
  sender?: { name?: string; address?: string; vatOrCvr?: string; countryCode?: string; identifierKind?: SupplierIdentifierKind };
  recipient?: { name?: string; address?: string; vatOrCvr?: string };
  vatAmount?: number;
  /** Purchase tax bases, retained verbatim with the voucher.  Omit for legacy uniform VAT documents. */
  purchaseVatLines?: PurchaseVatLine[];
  /** Human-confirmed invoice evidence required before a non-EU service can be
   * posted with automatic reverse-charge input-VAT deduction. */
  reverseChargeWordingConfirmed?: boolean;
  /** Verbatim, source-bound evidence of the reverse-charge statement.  The
   * original source and this payload are both integrity-hashed at ingest; this
   * is intentionally not an inferred classification. */
  reverseChargeWordingEvidence?: { excerpt: string; location: string };
  /** Explicit source fact: the issuer supplied a Danish simplified purchase invoice. */
  danishSimplifiedPurchaseInvoice?: boolean;
  /** Source-preserving intake marker. It permits storage of a standard invoice
   * with absent buyer fields, but never asserts VAT eligibility. */
  incompleteStandardPurchaseInvoice?: boolean;
  paymentDetails?: string;
  exemptionCode?: DocumentExemptionCode;
  /** Imported bank row that is the immutable primary evidence for an internal voucher. */
  sourceBankTransactionId?: number;
  /** Explicit evidence contract. Omitted legacy vouchers remain bank_evidenced. */
  internalVoucherKind?: InternalVoucherKind;
  legacyOpeningJournalEntryId?: number;
  legacyOpeningJournalLineId?: number;
  /** Human accounting explanation for why the internal voucher is booked. */
  accountingRationale?: string;
  externalAccountingEvidence?: { category: "payroll"; accountingPeriod: string; externalReference: string; totals: { debitAmount: number; creditAmount: number } };
};

export type DocumentValidationResult = {
  ok: boolean;
  appliedRules: string[];
  errors: string[];
};

export type IngestDocumentResult = {
  ok: boolean;
  documentId?: DocumentId;
  documentNo?: string;
  sha256?: string;
  storedPath?: string;
  errors?: string[];
};

export type IngestDocumentOptions = {
  forceDuplicateLogicalIdentity?: boolean;
  createdBy?: string;
  createdByProgram?: string;
  /** Internal bulk-import use only: the enclosing import writes one audit event. */
  suppressAudit?: boolean;
  /** Scanner policy is off unless a caller explicitly requires it. */
  scannerPolicy?: "off" | "required";
  /** Upper bound for a single scanner decision. Required scanners fail closed on expiry. */
  scannerTimeoutMs?: number;
  /** A vendor-neutral async seam. It receives immutable snapshot bytes only. */
  scanner?: DocumentScanner;
};

export type EnrichDocumentMetadataOptions = {
  createdBy?: string;
  createdByProgram?: string;
};

export type EnrichDocumentMetadataResult = {
  ok: boolean;
  documentId?: DocumentId;
  enriched?: boolean;
  errors?: string[];
};

export type DocumentScanner = {
  scan(input: { bytes: Buffer; sha256: string; mimeType: string; filename: string; signal: AbortSignal }): Promise<
    | { ok: true; scannerId: string; scannerVersion?: string; evidenceRef?: string }
    | { ok: false; error?: string }
  >;
};

const DEFAULT_SCANNER_TIMEOUT_MS = 15_000;
const UNSAFE_SCANNER_EVIDENCE_TEXT = /[\p{Cc}\p{Cf}]/u;

function boundedScannerEvidence(value: string | undefined, maxLength: number): string | undefined {
  const normalized = value?.trim().normalize("NFC") ?? "";
  if (!normalized || normalized.length > maxLength || UNSAFE_SCANNER_EVIDENCE_TEXT.test(normalized)) return undefined;
  return normalized;
}

function scannerTimeoutMs(value: number | undefined): number {
  if (value === undefined) return DEFAULT_SCANNER_TIMEOUT_MS;
  if (!Number.isInteger(value) || value < 100 || value > 120_000) {
    throw new Error("document scanner timeout must be an integer between 100 and 120000 ms");
  }
  return value;
}

/** A scanner receives a private buffer and is bounded even when it ignores AbortSignal. */
async function scanSnapshot(scanner: DocumentScanner, snapshot: DocumentSnapshot, mimeType: string, timeoutMs: number): Promise<
  | { ok: true; scannerId: string; scannerVersion?: string; evidenceRef?: string }
  | { ok: false; reason: "rejected" | "failed" }
> {
  const controller = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    const timeout = new Promise<never>((_, reject) => {
      timer = setTimeout(() => {
        controller.abort();
        reject(new Error("document scanner timed out"));
      }, timeoutMs);
    });
    // Buffer is mutable: never give a third-party scanner a reference to the
    // canonical snapshot which is later published as accounting evidence.
    const input = {
      bytes: Buffer.from(snapshot.bytes),
      sha256: snapshot.sha256,
      mimeType,
      filename: snapshot.filename,
      signal: controller.signal,
    };
    const result = await Promise.race([scanner.scan(input), timeout]);
    if (!result.ok || !hasText(result.scannerId)) return { ok: false, reason: "rejected" };
    const scannerId = boundedScannerEvidence(result.scannerId, 160);
    const scannerVersion = boundedScannerEvidence(result.scannerVersion, 160);
    const evidenceRef = boundedScannerEvidence(result.evidenceRef, 512);
    if (!scannerId || (result.scannerVersion !== undefined && !scannerVersion) || (result.evidenceRef !== undefined && !evidenceRef)) {
      return { ok: false, reason: "failed" };
    }
    return {
      ok: true,
      scannerId,
      scannerVersion,
      evidenceRef,
    };
  } catch {
    return { ok: false, reason: "failed" };
  } finally {
    if (timer !== undefined) clearTimeout(timer);
    controller.abort();
  }
}

const RULES = {
  STORAGE: "DK-DOCUMENT-STORAGE-001",
  CASH_RECEIPT: "DK-DOCUMENT-CASH-RECEIPT-001",
  SIMPLIFIED_INVOICE: "DK-INVOICE-SIMPLIFIED-001",
  FOREIGN_PHYSICAL: "DK-DOCUMENT-FOREIGN-PHYSICAL-001",
  INTEGRITY: "DK-DOCUMENT-INTEGRITY-001",
} as const;

function hasText(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function hasNonNegativeNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}

export function validatePurchaseVatLines(metadata: Pick<DocumentMetadata, "amountIncVat" | "vatAmount" | "purchaseVatLines">): string[] {
  const lines = metadata.purchaseVatLines;
  if (lines === undefined) return [];
  if (!Array.isArray(lines) || lines.length === 0) return ["purchaseVatLines must be a non-empty array when present"];
  const errors: string[] = [];
  if (!hasNonNegativeNumber(metadata.amountIncVat)) errors.push("purchaseVatLines requires amountIncVat");
  if (!hasNonNegativeNumber(metadata.vatAmount)) errors.push("purchaseVatLines requires vatAmount");
  const allowed = new Set<PurchaseVatClassification>(["dk_purchase_25", "exempt"]);
  let net = 0;
  let vat = 0;
  for (const [index, line] of lines.entries()) {
    if (!line || typeof line !== "object" || !allowed.has(line.classification)) {
      errors.push(`purchaseVatLines[${index}].classification must be dk_purchase_25 or exempt`);
      continue;
    }
    if (!hasNonNegativeNumber(line.netAmount)) errors.push(`purchaseVatLines[${index}].netAmount must be a non-negative number`);
    const lineVat = line.vatAmount ?? 0;
    if (!hasNonNegativeNumber(lineVat)) errors.push(`purchaseVatLines[${index}].vatAmount must be a non-negative number when present`);
    if (line.classification === "dk_purchase_25" && hasNonNegativeNumber(line.netAmount) && compareDkk(roundDkk(lineVat), percentOfDkk(line.netAmount, 25)) !== 0) {
      errors.push(`purchaseVatLines[${index}] dk_purchase_25 vatAmount must equal 25% of netAmount (${percentOfDkk(line.netAmount, 25)})`);
    }
    if (line.classification !== "dk_purchase_25" && compareDkk(roundDkk(lineVat), 0) !== 0) errors.push(`purchaseVatLines[${index}] ${line.classification} vatAmount must be 0`);
    net = sumDkk([net, Number(line.netAmount ?? 0)]);
    vat = sumDkk([vat, Number(lineVat)]);
  }
  if (hasNonNegativeNumber(metadata.vatAmount) && compareDkk(vat, metadata.vatAmount) !== 0) errors.push(`purchaseVatLines VAT ${vat} must equal vatAmount ${roundDkk(metadata.vatAmount)}`);
  const gross = sumDkk([net, vat]);
  if (hasNonNegativeNumber(metadata.amountIncVat) && compareDkk(gross, metadata.amountIncVat) !== 0) errors.push(`purchaseVatLines net + VAT ${gross} must equal amountIncVat ${roundDkk(metadata.amountIncVat)}`);
  return errors;
}

export type PurchaseVatLinesPayloadResult =
  | { status: "absent"; lines: null; errors: [] }
  | { status: "valid"; lines: PurchaseVatLine[]; errors: [] }
  | { status: "invalid"; lines: null; errors: string[] };

/**
 * Parse a persisted purchase split without collapsing corrupt structured tax
 * data into the legacy "no split" state. Mutating consumers must reject the
 * invalid branch; read views may use the compatibility wrapper below.
 */
export type CanonicalPurchaseVatTotals = {
  amountIncVat: number | null | undefined;
  vatAmount: number | null | undefined;
};

export function parsePurchaseVatLinesPayload(
  payloadJson: string | null | undefined,
  canonicalTotals?: CanonicalPurchaseVatTotals,
): PurchaseVatLinesPayloadResult {
  if (!payloadJson) return { status: "absent", lines: null, errors: [] };
  try {
    const parsed = JSON.parse(payloadJson) as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return { status: "invalid", lines: null, errors: ["document payload_json must contain a metadata object"] };
    }
    if (!Object.prototype.hasOwnProperty.call(parsed, "purchaseVatLines")) {
      return { status: "absent", lines: null, errors: [] };
    }
    const metadata = parsed as DocumentMetadata;
    const errors: string[] = [];
    if (canonicalTotals) {
      if (!hasNonNegativeNumber(canonicalTotals.amountIncVat)) {
        errors.push("persisted purchaseVatLines requires canonical documents.amount_inc_vat");
      }
      if (!hasNonNegativeNumber(canonicalTotals.vatAmount)) {
        errors.push("persisted purchaseVatLines requires canonical documents.vat_amount");
      }
      if (!hasNonNegativeNumber(metadata.amountIncVat)) {
        errors.push("persisted purchaseVatLines payload requires amountIncVat");
      } else if (hasNonNegativeNumber(canonicalTotals.amountIncVat) && compareDkk(metadata.amountIncVat, canonicalTotals.amountIncVat) !== 0) {
        errors.push(`payload amountIncVat ${roundDkk(metadata.amountIncVat)} must equal canonical documents.amount_inc_vat ${roundDkk(canonicalTotals.amountIncVat)}`);
      }
      if (!hasNonNegativeNumber(metadata.vatAmount)) {
        errors.push("persisted purchaseVatLines payload requires vatAmount");
      } else if (hasNonNegativeNumber(canonicalTotals.vatAmount) && compareDkk(metadata.vatAmount, canonicalTotals.vatAmount) !== 0) {
        errors.push(`payload vatAmount ${roundDkk(metadata.vatAmount)} must equal canonical documents.vat_amount ${roundDkk(canonicalTotals.vatAmount)}`);
      }
    }
    errors.push(...validatePurchaseVatLines(canonicalTotals
      ? { ...metadata, amountIncVat: canonicalTotals.amountIncVat ?? undefined, vatAmount: canonicalTotals.vatAmount ?? undefined }
      : metadata));
    if (errors.length > 0) return { status: "invalid", lines: null, errors };
    return { status: "valid", lines: metadata.purchaseVatLines!, errors: [] };
  } catch {
    return { status: "invalid", lines: null, errors: ["document payload_json is not valid JSON"] };
  }
}

/** Compatibility reader for list/UI surfaces. Invalid data remains hidden but
 * can never become posting input because mutations use the strict parser. */
export function purchaseVatLinesFromPayload(payloadJson: string | null | undefined): PurchaseVatLine[] | null {
  const parsed = parsePurchaseVatLinesPayload(payloadJson);
  return parsed.status === "valid" ? parsed.lines : null;
}


/**
 * Allow-list of ingestable document types. Plain-text receipts are
 * legitimate (the smoke ingests several `.txt` files), so `text/plain`
 * and `application/json` are included alongside PDF/PNG/JPEG.
 */
const ALLOWED_MIME_TYPES = new Set([
  "application/pdf",
  "image/png",
  "image/jpeg",
  "text/plain",
  "application/json",
  // Received e-invoices (Digisense MODTAG, #efaktura) arrive as UBL XML and are
  // legitimate bilag, so application/xml is ingestable like the other text formats.
  "application/xml",
]);

const EXTENSION_MIME: Record<string, string> = {
  ".pdf": "application/pdf",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".txt": "text/plain",
  ".json": "application/json",
  ".xml": "application/xml",
};

function startsWithBytes(buf: Buffer, signature: number[]): boolean {
  if (buf.length < signature.length) return false;
  for (let i = 0; i < signature.length; i += 1) {
    if (buf[i] !== signature[i]) return false;
  }
  return true;
}

/**
 * Sniffs the leading magic bytes of a file and returns the MIME type
 * they indicate, or `null` for content with no recognised binary
 * signature (treated as plain text).
 */
function sniffMimeType(bytes: Buffer): string | null {
  const buf = bytes.subarray(0, 16);
  if (startsWithBytes(buf, [0x25, 0x50, 0x44, 0x46, 0x2d])) return "application/pdf"; // %PDF-
  if (startsWithBytes(buf, [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) return "image/png";
  if (startsWithBytes(buf, [0xff, 0xd8, 0xff])) return "image/jpeg";
  return null;
}

const BINARY_MIME_TYPES = new Set(["application/pdf", "image/png", "image/jpeg"]);

/**
 * Resolves the MIME type for an ingested file by combining the file
 * extension with magic-byte content sniffing. Throws if the bytes
 * contradict the extension, or if the type is outside the allow-list.
 */
function detectMimeType(filename: string, bytes: Buffer): string {
  const ext = extname(filename).toLowerCase();
  const expected = EXTENSION_MIME[ext];
  const sniffed = sniffMimeType(bytes);

  if (!expected) {
    throw new Error(`unsupported document type for extension '${ext || "(none)"}'`);
  }

  if (BINARY_MIME_TYPES.has(expected)) {
    // Binary formats must carry their signature.
    if (sniffed !== expected) {
      throw new Error(
        `file content does not match its '${ext}' extension (expected ${expected})`,
      );
    }
  } else if (sniffed && sniffed !== expected) {
    // A .txt/.json file must not actually contain binary document bytes.
    throw new Error(
      `file content does not match its '${ext}' extension (looks like ${sniffed})`,
    );
  }

  if (!ALLOWED_MIME_TYPES.has(expected)) {
    throw new Error(`document type ${expected} is not on the ingestion allow-list`);
  }
  return expected;
}

function nextDocumentNo(db: Database, issueDate?: string) {
  const scope = fiscalYearLabelFromDate(db, issueDate ?? currentUtcIsoDate(db));
  const row = db.query(`SELECT COALESCE(MAX(CAST(substr(document_no, -6) AS INTEGER)), 0) AS n FROM documents WHERE document_no GLOB ?`).get(`DOC-${scope}-[0-9][0-9][0-9][0-9][0-9][0-9]`) as { n: number };
  const nextValue = nextSequenceValue(db, "document", companySequenceScope(db, scope), Number(row.n ?? 0));
  return `DOC-${scope}-${String(nextValue).padStart(6, "0")}`;
}

export function validateDocumentMetadata(metadata: DocumentMetadata): DocumentValidationResult {
  const errors: string[] = [];
  const documentType = metadata.documentType ?? "purchase_sale";
  const exemptionCode = metadata.exemptionCode ?? null;
  const currency = (metadata.currency ?? "DKK").trim().toUpperCase();
  const appliedRules: string[] = [RULES.STORAGE, RULES.INTEGRITY];

  if (!hasText(metadata.source)) errors.push("source is required");
  if (metadata.reverseChargeWordingConfirmed !== undefined && typeof metadata.reverseChargeWordingConfirmed !== "boolean") {
    errors.push("reverseChargeWordingConfirmed must be a boolean when present");
  }
  if (metadata.reverseChargeWordingEvidence !== undefined) {
    const evidence = metadata.reverseChargeWordingEvidence;
    if (!evidence || typeof evidence !== "object" || !hasText(evidence.excerpt) || !hasText(evidence.location)) {
      errors.push("reverseChargeWordingEvidence requires non-empty excerpt and location");
    }
  }
  if (metadata.danishSimplifiedPurchaseInvoice !== undefined && typeof metadata.danishSimplifiedPurchaseInvoice !== "boolean") {
    errors.push("danishSimplifiedPurchaseInvoice must be a boolean when present");
  }
  if (metadata.incompleteStandardPurchaseInvoice !== undefined && typeof metadata.incompleteStandardPurchaseInvoice !== "boolean") {
    errors.push("incompleteStandardPurchaseInvoice must be a boolean when present");
  }
  if (metadata.incompleteStandardPurchaseInvoice === true && metadata.danishSimplifiedPurchaseInvoice === true) {
    errors.push("incompleteStandardPurchaseInvoice cannot be combined with danishSimplifiedPurchaseInvoice");
  }
  if (metadata.danishSimplifiedPurchaseInvoice === true) {
    appliedRules.splice(appliedRules.length - 1, 0, RULES.SIMPLIFIED_INVOICE);
    errors.push(...validateDanishSimplifiedPurchaseInvoiceMetadata(metadata));
  }
  if (!/^[A-Z]{3}$/.test(currency)) errors.push("currency must be a 3-letter ISO code");
  if (documentType === "cash_register_receipt") appliedRules.splice(1, 0, RULES.CASH_RECEIPT);
  if (exemptionCode === "FOREIGN_PHYSICAL_ONLY") appliedRules.splice(appliedRules.length - 1, 0, RULES.FOREIGN_PHYSICAL);
  // A statutory-field exemption never exempts supplied structured tax data
  // from internal consistency checks. If a receipt carries a split, validate
  // it before any document-type shortcut can accept malformed amounts.
  errors.push(...validatePurchaseVatLines(metadata));

  if (documentType === "internal_voucher") {
    const internalVoucherKind = metadata.internalVoucherKind ?? "bank_evidenced";
    if (!looksLikeIsoDate(metadata.issueDate)) {
      errors.push("internal voucher issueDate must be present in YYYY-MM-DD format");
    }
    if (!hasText(metadata.deliveryDescription)) {
      errors.push("internal voucher deliveryDescription is required");
    }
    if (!hasNonNegativeNumber(metadata.amountIncVat) || metadata.amountIncVat <= 0) {
      errors.push("internal voucher amountIncVat must be greater than 0");
    }
    if (metadata.vatAmount !== 0) {
      errors.push("internal voucher vatAmount must be exactly 0");
    }
    if (!['bank_evidenced', 'non_cash_balance_correction', 'legacy_opening_creditor_reclassification'].includes(internalVoucherKind)) {
      errors.push("internal voucher internalVoucherKind must be bank_evidenced, non_cash_balance_correction or legacy_opening_creditor_reclassification");
    } else if (internalVoucherKind === "bank_evidenced" && (
      !Number.isInteger(metadata.sourceBankTransactionId) || Number(metadata.sourceBankTransactionId) <= 0
    )) {
      errors.push("internal voucher sourceBankTransactionId must be a positive integer");
    } else if ((internalVoucherKind === "non_cash_balance_correction" || internalVoucherKind === "legacy_opening_creditor_reclassification") && metadata.sourceBankTransactionId !== undefined) {
      errors.push("non-cash balance correction must not reference a bank transaction");
    }
    if ((internalVoucherKind === "non_cash_balance_correction" || internalVoucherKind === "legacy_opening_creditor_reclassification") && (metadata.currency ?? "DKK").trim().toUpperCase() !== "DKK") {
      errors.push("non-cash balance correction currency must be DKK");
    }
    if (internalVoucherKind === "legacy_opening_creditor_reclassification" && (!Number.isInteger(metadata.legacyOpeningJournalEntryId) || Number(metadata.legacyOpeningJournalEntryId) <= 0 || !Number.isInteger(metadata.legacyOpeningJournalLineId) || Number(metadata.legacyOpeningJournalLineId) <= 0)) errors.push("legacy opening creditor reclassification requires positive legacyOpeningJournalEntryId and legacyOpeningJournalLineId");
    if (internalVoucherKind !== "legacy_opening_creditor_reclassification" && (metadata.legacyOpeningJournalEntryId !== undefined || metadata.legacyOpeningJournalLineId !== undefined)) errors.push("legacy opening journal references are only allowed for legacy_opening_creditor_reclassification");
    if (!hasText(metadata.accountingRationale)) {
      errors.push("internal voucher accountingRationale is required");
    }
    if (metadata.purchaseVatLines !== undefined) {
      errors.push("internal voucher cannot contain purchaseVatLines");
    }
    if (metadata.reverseChargeWordingConfirmed !== undefined) {
      errors.push("internal voucher cannot contain reverseChargeWordingConfirmed");
    }
    if (metadata.reverseChargeWordingEvidence !== undefined) {
      errors.push("internal voucher cannot contain reverseChargeWordingEvidence");
    }
    if (metadata.exemptionCode !== undefined && metadata.exemptionCode !== null) {
      errors.push("internal voucher cannot contain exemptionCode");
    }
  }
  if (documentType === "external_accounting_evidence") {
    const evidence = metadata.externalAccountingEvidence;
    if (!evidence || evidence.category !== "payroll") errors.push("external accounting evidence requires category payroll");
    if (!evidence || !/^\d{4}-(0[1-9]|1[0-2])$/.test(evidence.accountingPeriod)) errors.push("external accounting evidence requires accountingPeriod YYYY-MM");
    if (!evidence || !hasText(evidence.externalReference)) errors.push("external accounting evidence requires externalReference");
    if (!evidence || !hasNonNegativeNumber(evidence.totals?.debitAmount) || !hasNonNegativeNumber(evidence.totals?.creditAmount) || evidence.totals.debitAmount <= 0 || Math.abs(evidence.totals.debitAmount - evidence.totals.creditAmount) > 0.00001) errors.push("external accounting evidence requires balanced positive debitAmount and creditAmount totals");
    if (!looksLikeIsoDate(metadata.issueDate)) errors.push("external accounting evidence requires issueDate YYYY-MM-DD");
    if (!hasText(metadata.sender?.name)) errors.push("external accounting evidence requires the external issuer name");
    if (!hasText(metadata.recipient?.name)) errors.push("external accounting evidence requires the reported company name");
    if (metadata.vatAmount !== 0) errors.push("external accounting evidence vatAmount must be exactly 0");
  }

  const exemptFromMinimumFields =
    documentType === "cash_register_receipt" ||
    documentType === "issued_invoice_pdf" ||
    documentType === "internal_voucher" ||
    documentType === "external_accounting_evidence" ||
    exemptionCode === "FOREIGN_PHYSICAL_ONLY";
  if (!exemptFromMinimumFields) {
    if (!looksLikeIsoDate(metadata.issueDate)) errors.push("issueDate must be present in YYYY-MM-DD format");
    if (!hasText(metadata.deliveryDescription)) errors.push("deliveryDescription is required");
    if (!hasNonNegativeNumber(metadata.amountIncVat)) errors.push("amountIncVat is required");
    if (!hasText(metadata.sender?.name)) errors.push("sender.name is required");
    if (!hasText(metadata.sender?.address)) errors.push("sender.address is required");
    const suppliedIdentity = metadata.sender?.countryCode !== undefined || metadata.sender?.identifierKind !== undefined;
    const identity = suppliedIdentity
      ? resolveSupplierIdentity({ country: metadata.sender?.countryCode ?? "", identifier: metadata.sender?.vatOrCvr, identifierKind: metadata.sender?.identifierKind })
      : resolveLegacySupplierIdentity(metadata.sender?.vatOrCvr);
    if (!identity.ok) errors.push(...identity.errors.map((error) => `sender: human_resolution_required: ${error}`));
    // A simplified invoice does not need buyer identity. If the issuer printed
    // a recipient (including an individual), those values are retained exactly
    // as supplied; company context is recorded separately after ingestion.
    if (metadata.danishSimplifiedPurchaseInvoice !== true) {
      if (!hasText(metadata.recipient?.name)) errors.push("recipient.name is required");
      if (!hasText(metadata.recipient?.address) && metadata.incompleteStandardPurchaseInvoice !== true) errors.push("recipient.address is required");
      // A buyer registration number is not a universal statutory field on a
      // full invoice. Preserve its absence rather than fabricating it.
    }
    if (!hasNonNegativeNumber(metadata.vatAmount)) errors.push("vatAmount is required");
  }

  return { ok: errors.length === 0, appliedRules, errors };
}

/** Mandatory reduced facts for a Danish simplified purchase invoice (§ 66). */
export function validateDanishSimplifiedPurchaseInvoiceMetadata(metadata: DocumentMetadata): string[] {
  const errors: string[] = [];
  if (metadata.danishSimplifiedPurchaseInvoice !== true) {
    return ["danishSimplifiedPurchaseInvoice must be true"];
  }
  if ((metadata.documentType ?? "purchase_sale") !== "purchase_sale") {
    errors.push("Danish simplified invoice must be a purchase_sale document");
  }
  if ((metadata.currency ?? "DKK").trim().toUpperCase() !== "DKK") {
    errors.push("Danish simplified invoice must be denominated in DKK");
  }
  if (!looksLikeIsoDate(metadata.issueDate)) errors.push("Danish simplified invoice requires issueDate in YYYY-MM-DD format");
  if (!hasText(metadata.invoiceNo)) errors.push("Danish simplified invoice requires invoiceNo");
  if (!hasText(metadata.sender?.name) || !hasText(metadata.sender?.address)) {
    errors.push("Danish simplified invoice requires sender name and address");
  }
  const suppliedIdentity = metadata.sender?.countryCode !== undefined || metadata.sender?.identifierKind !== undefined;
  const identity = suppliedIdentity
    ? resolveSupplierIdentity({ country: metadata.sender?.countryCode ?? "", identifier: metadata.sender?.vatOrCvr, identifierKind: metadata.sender?.identifierKind })
    : resolveLegacySupplierIdentity(metadata.sender?.vatOrCvr);
  if (!identity.ok || identity.country !== "DK" || identity.identifierKind !== "dk_cvr") {
    errors.push("Danish simplified invoice requires a valid Danish supplier CVR");
  }
  if (!hasText(metadata.deliveryDescription)) errors.push("Danish simplified invoice requires deliveryDescription");
  const gross = metadata.amountIncVat;
  const vat = metadata.vatAmount;
  if (typeof gross !== "number" || !Number.isFinite(gross) || compareDkk(gross, 0) <= 0 || compareDkk(gross, 3000) > 0) {
    errors.push("Danish simplified invoice gross amount must be greater than 0 and at most DKK 3000");
  }
  if (metadata.purchaseVatLines !== undefined) {
    const lineErrors = validatePurchaseVatLines(metadata);
    errors.push(...lineErrors);
    if (lineErrors.length === 0) {
      const taxableLines = metadata.purchaseVatLines?.filter((line) => line.classification === "dk_purchase_25") ?? [];
      if (taxableLines.length === 0 || taxableLines.every((line) => compareDkk(line.vatAmount ?? 0, 0) <= 0)) {
        errors.push("Danish simplified invoice requires an explicitly documented taxable purchaseVatLines amount");
      }
    }
  } else if (typeof gross === "number" && Number.isFinite(gross) && typeof vat === "number" && Number.isFinite(vat)) {
    const net = roundDkk(gross - vat);
    const expectedVat = percentOfDkk(net, 25);
    if (compareDkk(vat, 0) <= 0 || compareDkk(vat, gross) >= 0 || compareDkk(Math.abs(roundDkk(vat - expectedVat)), 0.01) > 0) {
      errors.push("Danish simplified invoice requires VAT consistent with the 25% Danish standard rate");
    }
  } else {
    errors.push("Danish simplified invoice requires vatAmount");
  }
  return [...new Set(errors)];
}

/** Stable JSON is used only to recognise an identical enrichment retry. */
function canonicalJson(value: unknown): string {
  if (value === undefined) return "null";
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value && typeof value === "object") {
    const object = value as Record<string, unknown>;
    return `{${Object.keys(object).filter((key) => object[key] !== undefined).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(object[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

function metadataValueIsPresent(value: unknown): boolean {
  return value !== null && value !== undefined && value !== "";
}

function documentMetadataColumns(metadata: DocumentMetadata) {
  const documentType = metadata.documentType ?? "purchase_sale";
  const senderIdentity = documentType === "purchase_sale"
    ? (metadata.sender?.countryCode !== undefined || metadata.sender?.identifierKind !== undefined
      ? resolveSupplierIdentity({ country: metadata.sender?.countryCode ?? "", identifier: metadata.sender?.vatOrCvr, identifierKind: metadata.sender?.identifierKind })
      : resolveLegacySupplierIdentity(metadata.sender?.vatOrCvr))
    : null;
  return {
    source: metadata.source, document_type: documentType, supplier_name: metadata.sender?.name ?? null,
    invoice_no: metadata.invoiceNo ?? null, invoice_date: metadata.issueDate ?? null,
    amount_inc_vat: metadata.amountIncVat ?? null, currency: (metadata.currency ?? "DKK").trim().toUpperCase(),
    delivery_description: metadata.deliveryDescription ?? null, sender_name: metadata.sender?.name ?? null,
    sender_address: metadata.sender?.address ?? null,
    sender_vat_cvr: senderIdentity?.ok ? senderIdentity.identifier : metadata.sender?.vatOrCvr?.trim() ?? null,
    supplier_country_code: senderIdentity?.ok ? senderIdentity.country : null,
    supplier_identifier_kind: senderIdentity?.ok ? senderIdentity.identifierKind : null,
    supplier_identity_status: senderIdentity?.ok ? senderIdentity.status : null,
    recipient_name: metadata.recipient?.name ?? null, recipient_address: metadata.recipient?.address ?? null,
    recipient_vat_cvr: metadata.recipient?.vatOrCvr ?? null, vat_amount: metadata.vatAmount ?? null,
    payment_details: metadata.paymentDetails ?? null, exemption_code: metadata.exemptionCode ?? null,
  };
}

/** Keep the document payload to the validated metadata contract, never caller extras. */
function normalizedEnrichedMetadata(metadata: DocumentMetadata): DocumentMetadata {
  return {
    source: metadata.source,
    documentType: metadata.documentType ?? "purchase_sale",
    issueDate: metadata.issueDate,
    invoiceNo: metadata.invoiceNo,
    deliveryDescription: metadata.deliveryDescription,
    amountIncVat: metadata.amountIncVat,
    currency: (metadata.currency ?? "DKK").trim().toUpperCase(),
    sender: metadata.sender && {
      name: metadata.sender.name, address: metadata.sender.address, vatOrCvr: metadata.sender.vatOrCvr,
      countryCode: metadata.sender.countryCode, identifierKind: metadata.sender.identifierKind,
    },
    recipient: metadata.recipient && {
      name: metadata.recipient.name, address: metadata.recipient.address, vatOrCvr: metadata.recipient.vatOrCvr,
    },
    vatAmount: metadata.vatAmount,
    purchaseVatLines: metadata.purchaseVatLines,
    reverseChargeWordingConfirmed: metadata.reverseChargeWordingConfirmed,
    reverseChargeWordingEvidence: metadata.reverseChargeWordingEvidence,
    danishSimplifiedPurchaseInvoice: metadata.danishSimplifiedPurchaseInvoice,
    incompleteStandardPurchaseInvoice: metadata.incompleteStandardPurchaseInvoice,
    paymentDetails: metadata.paymentDetails,
    exemptionCode: metadata.exemptionCode,
    externalAccountingEvidence: metadata.externalAccountingEvidence,
  };
}

/** Existing leaves are binding; omitted leaves may safely be completed. */
function metadataSubsetCompatible(existing: unknown, incoming: unknown): boolean {
  if (!metadataValueIsPresent(existing)) return true;
  if (Array.isArray(existing) || Array.isArray(incoming)) return canonicalJson(existing) === canonicalJson(incoming);
  if (existing && typeof existing === "object") {
    if (!incoming || typeof incoming !== "object") return false;
    return Object.entries(existing as Record<string, unknown>).every(([key, value]) =>
      metadataSubsetCompatible(value, (incoming as Record<string, unknown>)[key]),
    );
  }
  return canonicalJson(existing) === canonicalJson(incoming);
}

function findPurchaseSaleLogicalDuplicate(
  db: Database,
  metadata: DocumentMetadata,
  excludeDocumentId?: number,
): { id: number; document_no: string } | null {
  const documentType = metadata.documentType ?? "purchase_sale";
  const invoiceNo = metadata.invoiceNo?.trim();
  if (documentType !== "purchase_sale" || !invoiceNo) return null;
  const senderIdentity = metadata.sender?.countryCode !== undefined || metadata.sender?.identifierKind !== undefined
    ? resolveSupplierIdentity({ country: metadata.sender?.countryCode ?? "", identifier: metadata.sender?.vatOrCvr, identifierKind: metadata.sender?.identifierKind })
    : resolveLegacySupplierIdentity(metadata.sender?.vatOrCvr);
  const senderVatOrCvr = senderIdentity.ok ? senderIdentity.identifier : metadata.sender?.vatOrCvr?.trim();
  const excluded = excludeDocumentId ?? -1;
  const existingByIdentifier = senderVatOrCvr
    ? db.query(`SELECT id, document_no FROM documents WHERE document_type = 'purchase_sale' AND sender_vat_cvr = ? AND invoice_no = ? AND id != ? LIMIT 1`).get(senderVatOrCvr, invoiceNo, excluded) as { id: number; document_no: string } | null
    : null;
  const senderName = metadata.sender?.name?.trim();
  const existingByNonEuCountryAndName = senderIdentity.ok && senderIdentity.identifierKind === "non_eu" && senderName
    ? db.query(`SELECT id, document_no FROM documents WHERE document_type = 'purchase_sale' AND supplier_country_code = ? AND lower(trim(sender_name)) = lower(trim(?)) AND invoice_no = ? AND id != ? LIMIT 1`).get(senderIdentity.country, senderName, invoiceNo, excluded) as { id: number; document_no: string } | null
    : null;
  return existingByIdentifier ?? existingByNonEuCountryAndName;
}

/** Completes a pre-accounting legacy document without touching its evidence identity. */
export function enrichDocumentMetadata(db: Database, documentId: number, metadata: DocumentMetadata, options: EnrichDocumentMetadataOptions = {}): EnrichDocumentMetadataResult {
  if (metadata.documentType === "internal_voucher") return { ok: false, errors: ["internal voucher metadata cannot be enriched"] };
  const validation = validateDocumentMetadata(metadata);
  if (!validation.ok) return { ok: false, errors: validation.errors };
  const normalizedMetadata = normalizedEnrichedMetadata(metadata);
  const incoming = documentMetadataColumns(normalizedMetadata);
  const enrichedMetadataJson = canonicalJson(normalizedMetadata);
  const metadataHash = createHash("sha256").update(enrichedMetadataJson).digest("hex");
  try {
    return db.transaction(() => {
      const row = db.query(`SELECT id, status, source, document_type, supplier_name, invoice_no, invoice_date,
        amount_inc_vat, currency, delivery_description, sender_name, sender_address, sender_vat_cvr,
        supplier_country_code, supplier_identifier_kind, supplier_identity_status, recipient_name,
        recipient_address, recipient_vat_cvr, vat_amount, payment_details, exemption_code, payload_json
        FROM documents WHERE id = ?`).get(documentId) as Record<string, unknown> | null;
      if (!row) return { ok: false, errors: [`document ${documentId} does not exist`] };
      if (row.document_type === "internal_voucher") return { ok: false, errors: ["internal voucher metadata cannot be enriched"] };
      if (row.status !== "ingested") return { ok: false, errors: ["document is already posted or otherwise non-enrichable"] };
      if (db.query("SELECT 1 FROM journal_entries WHERE document_id = ? LIMIT 1").get(documentId)
        || db.query("SELECT 1 FROM import_document_links WHERE document_id = ? AND journal_entry_id IS NOT NULL LIMIT 1").get(documentId)
        || db.query("SELECT 1 FROM dinero_import_document_links WHERE document_id = ? AND (journal_entry_id IS NOT NULL OR disposition = 'linked') LIMIT 1").get(documentId)) {
        return { ok: false, errors: ["document is linked to accounting evidence and cannot be enriched"] };
      }
      const enrichment = db.query("SELECT enriched_metadata_sha256 FROM document_metadata_enrichments WHERE document_id = ?").get(documentId) as { enriched_metadata_sha256: string } | null;
      if (enrichment) return enrichment.enriched_metadata_sha256 === metadataHash
        ? { ok: true, documentId: asDocumentId(documentId), enriched: false }
        : { ok: false, errors: ["document metadata was already enriched with different metadata"] };
      const originalPayloadJson = typeof row.payload_json === "string" ? row.payload_json : null;
      if (originalPayloadJson) {
        try {
          const parsed = JSON.parse(originalPayloadJson) as Record<string, unknown>;
          const existingMetadata = normalizedEnrichedMetadata(parsed as DocumentMetadata);
          if (!metadataSubsetCompatible(existingMetadata, normalizedMetadata)) return { ok: false, errors: ["existing document metadata conflicts with enrichment"] };
        } catch { return { ok: false, errors: ["document payload_json is not valid JSON"] }; }
      }
      for (const [column, value] of Object.entries(incoming)) {
        if (metadataValueIsPresent(row[column]) && canonicalJson(row[column]) !== canonicalJson(value)) return { ok: false, errors: [`existing document ${column} conflicts with enrichment`] };
      }
      const duplicate = findPurchaseSaleLogicalDuplicate(db, normalizedMetadata, documentId);
      if (duplicate) return { ok: false, errors: [`a document with this supplier and invoice identity is already ingested as ${duplicate.document_no}`] };
      const columns = Object.keys(incoming);
      const assignments = columns.map((column) => `${column} = CASE WHEN ${column} IS NULL OR ${column} = '' THEN ? ELSE ${column} END`).join(", ");
      const originalPayloadHash = originalPayloadJson ? createHash("sha256").update(originalPayloadJson).digest("hex") : null;
      const actor = resolveActor({ createdBy: options.createdBy, createdByProgram: options.createdByProgram });
      db.query(`UPDATE documents SET ${assignments}, payload_json = ? WHERE id = ?`).run(...columns.map((column) => incoming[column as keyof typeof incoming]), enrichedMetadataJson, documentId);
      db.query(`INSERT INTO document_metadata_enrichments (document_id, original_payload_json, original_payload_sha256, enriched_metadata_json, enriched_metadata_sha256, actor, program) VALUES (?, ?, ?, ?, ?, ?, ?)`)
        .run(documentId, originalPayloadJson, originalPayloadHash, enrichedMetadataJson, metadataHash, actor.createdBy, actor.createdByProgram);
      strengthenGdprErasureAliasesForIdentity(db, { name: normalizedMetadata.sender?.name, cvr: incoming.sender_vat_cvr as string | null });
      strengthenGdprErasureAliasesForIdentity(db, { name: normalizedMetadata.recipient?.name, cvr: normalizedMetadata.recipient?.vatOrCvr });
      insertAuditLog(db, { eventType: "document_metadata_enriched", entityType: "document", entityId: documentId, message: `Enriched document metadata for document ${documentId} (original_payload_sha256=${originalPayloadHash ?? "null"}, enriched_metadata_sha256=${metadataHash})`, createdBy: actor.createdBy, createdByProgram: actor.createdByProgram });
      return { ok: true, documentId: asDocumentId(documentId), enriched: true };
    }).immediate();
  } catch (error) { return { ok: false, errors: [error instanceof Error ? error.message : String(error)] }; }
}

function validateInternalVoucherBankEvidence(
  db: Database,
  metadata: DocumentMetadata,
): string[] {
  if (metadata.documentType !== "internal_voucher" || (metadata.internalVoucherKind ?? "bank_evidenced") !== "bank_evidenced") return [];
  const bankTransactionId = Number(metadata.sourceBankTransactionId);
  const bank = db.query(
    `SELECT id, transaction_date, amount, currency, transaction_hash,
            source_file_hash, import_batch_id
       FROM bank_transactions
      WHERE id = ?`,
  ).get(bankTransactionId) as
    | {
        id: number;
        transaction_date: string;
        amount: number;
        currency: string;
        transaction_hash: string | null;
        source_file_hash: string | null;
        import_batch_id: string | null;
      }
    | null;
  if (!bank) return [`sourceBankTransactionId ${bankTransactionId} does not exist`];

  const errors: string[] = [];
  if (!bank.transaction_hash && !(bank.source_file_hash && bank.import_batch_id)) {
    errors.push(
      `bank transaction ${bank.id} has no stable import identity and cannot back an internal voucher`,
    );
  }
  if (!(Number(bank.amount) < 0)) {
    errors.push(`bank transaction ${bank.id} is not an outgoing payment`);
  }
  if (metadata.issueDate !== bank.transaction_date) {
    errors.push(
      `internal voucher issueDate ${metadata.issueDate ?? "(missing)"} does not match bank transaction date ${bank.transaction_date}`,
    );
  }
  const currency = (metadata.currency ?? "DKK").trim().toUpperCase();
  if (currency !== bank.currency.trim().toUpperCase()) {
    errors.push(
      `internal voucher currency ${currency} does not match bank transaction currency ${bank.currency}`,
    );
  }
  if (
    hasNonNegativeNumber(metadata.amountIncVat) &&
    compareDkk(metadata.amountIncVat, Math.abs(Number(bank.amount))) !== 0
  ) {
    errors.push(
      `internal voucher amount ${roundDkk(metadata.amountIncVat)} does not match bank transaction amount ${roundDkk(Math.abs(Number(bank.amount)))}`,
    );
  }
  const existing = db.query(
    "SELECT document_id FROM internal_voucher_evidence WHERE bank_transaction_id = ?",
  ).get(bank.id) as { document_id: number } | null;
  if (existing) {
    errors.push(
      `bank transaction ${bank.id} already backs internal voucher document ${existing.document_id}`,
    );
  }
  return errors;
}

/**
 * Legacy synchronous entrypoint. It remains safe because snapshotting and
 * publishing are synchronous, but it refuses a configured scanner instead of
 * silently bypassing an async security decision. New external ingress points
 * should use ingestDocumentAsync.
 */
export function ingestDocument(db: Database, companyRoot: string, filePath: string, metadata: DocumentMetadata, options: IngestDocumentOptions = {}): IngestDocumentResult {
  if (options.scannerPolicy === "required" || options.scanner) {
    return { ok: false, errors: ["document scanner requires async ingestDocumentAsync"] };
  }
  return ingestDocumentSnapshot(db, companyRoot, filePath, metadata, options);
}

/** Async entrypoint for ingress stacks that may mandate malware scanning. */
export async function ingestDocumentAsync(db: Database, companyRoot: string, filePath: string, metadata: DocumentMetadata, options: IngestDocumentOptions = {}): Promise<IngestDocumentResult> {
  const validation = validateDocumentMetadata(metadata);
  if (!validation.ok) return { ok: false, errors: validation.errors };
  let snapshot: DocumentSnapshot;
  let mimeType: string;
  try {
    snapshot = snapshotDocumentSource(filePath);
    mimeType = detectMimeType(snapshot.filename, snapshot.bytes);
  } catch (error) {
    return { ok: false, errors: [error instanceof Error ? error.message : String(error)] };
  }
  if (options.scannerPolicy === "required" && !options.scanner) {
    return { ok: false, errors: ["document scanner is required but unavailable"] };
  }
  let scanEvidence: { scannerId: string; scannerVersion?: string; evidenceRef?: string } | undefined;
  if (options.scanner) {
    const result = await scanSnapshot(options.scanner, snapshot, mimeType, scannerTimeoutMs(options.scannerTimeoutMs));
    if (!result.ok) {
      return { ok: false, errors: [result.reason === "rejected" ? "document scanner rejected the document" : "document scanner failed"] };
    }
    scanEvidence = result;
  }
  return ingestDocumentSnapshot(db, companyRoot, filePath, metadata, options, snapshot, mimeType, scanEvidence);
}

function ingestDocumentSnapshot(
  db: Database,
  companyRoot: string,
  filePath: string,
  metadata: DocumentMetadata,
  options: IngestDocumentOptions,
  suppliedSnapshot?: DocumentSnapshot,
  suppliedMimeType?: string,
  scanEvidence?: { scannerId: string; scannerVersion?: string; evidenceRef?: string },
): IngestDocumentResult {
  const validation = validateDocumentMetadata(metadata);
  if (!validation.ok) return { ok: false, errors: validation.errors };
  const internalEvidenceErrors = validateInternalVoucherBankEvidence(db, metadata);
  if (internalEvidenceErrors.length > 0) {
    return { ok: false, errors: internalEvidenceErrors };
  }
  let snapshot: DocumentSnapshot;
  let mimeType: string;
  try {
    snapshot = suppliedSnapshot ?? snapshotDocumentSource(filePath);
    mimeType = suppliedMimeType ?? detectMimeType(snapshot.filename, snapshot.bytes);
  } catch (error) {
    return { ok: false, errors: [error instanceof Error ? error.message : String(error)] };
  }
  const sha256 = snapshot.sha256;
  const existing = db.query("SELECT id, document_no, stored_path FROM documents WHERE sha256_hash = ?").get(sha256) as { id: number; document_no: string; stored_path: string } | null;
  if (existing) {
    return { ok: false, errors: [`duplicate document content already ingested as ${existing.document_no}`] };
  }

  const docType = metadata.documentType ?? "purchase_sale";
  const senderIdentity = docType === "purchase_sale"
    ? (metadata.sender?.countryCode !== undefined || metadata.sender?.identifierKind !== undefined
      ? resolveSupplierIdentity({ country: metadata.sender?.countryCode ?? "", identifier: metadata.sender?.vatOrCvr, identifierKind: metadata.sender?.identifierKind })
      : resolveLegacySupplierIdentity(metadata.sender?.vatOrCvr))
    : null;
  const senderVatOrCvr = senderIdentity?.ok ? senderIdentity.identifier : metadata.sender?.vatOrCvr?.trim();
  const invoiceNo = metadata.invoiceNo?.trim();
  if (!options.forceDuplicateLogicalIdentity && docType === "purchase_sale" && invoiceNo) {
    const existingLogical = findPurchaseSaleLogicalDuplicate(db, metadata);
    if (existingLogical) {
      const supplierKey = senderVatOrCvr ?? `${senderIdentity && senderIdentity.ok ? senderIdentity.country : "unknown"}:${metadata.sender?.name?.trim() ?? "unknown"}`;
      return { ok: false, errors: [`a document from ${supplierKey} with invoice ${invoiceNo} is already ingested as ${existingLogical.document_no}. Use --force to add another scan.`] };
    }
  }

  const ext = extname(snapshot.filename).toLowerCase() || ".bin";
  let evidenceStore: string;
  try {
    evidenceStore = ensureCanonicalDocumentStore(companyRoot, docType === "issued_invoice_pdf" ? "invoices/issued" : "documents/originals");
  } catch (error) {
    return { ok: false, errors: [error instanceof Error ? error.message : String(error)] };
  }
  const storedPath = join(evidenceStore, `${sha256}${ext}`);

  const currency = (metadata.currency ?? "DKK").trim().toUpperCase();
  const retentionBasisDate = metadata.issueDate ?? currentUtcIsoDate(db);
  let published = false;

  try {
    // Publish before the DB transaction so a document row can never point at
    // a not-yet-durable file. A stale/unregistered final is rejected below.
    const alreadyRegistered = db.query("SELECT 1 FROM documents WHERE sha256_hash = ?").get(sha256);
    if (alreadyRegistered) return { ok: false, errors: ["duplicate document content already ingested"] };
    // This is immediately before the exclusive create. The scanner only got a
    // copy, but rechecking here makes the publication boundary independently
    // fail closed if any future caller accidentally mutates a supplied snapshot.
    const canonicalHash = createHash("sha256").update(snapshot.bytes).digest("hex");
    const canonicalMimeType = detectMimeType(snapshot.filename, snapshot.bytes);
    if (canonicalHash !== sha256 || canonicalMimeType !== mimeType) {
      return { ok: false, errors: ["document snapshot changed before publication"] };
    }
    const publication = publishDocumentSnapshot(evidenceStore, `${sha256}${ext}`, snapshot);
    published = publication.published;
    if (!published) {
      // A pre-existing same-byte file without its immutable DB register is not
      // safe to adopt: it could be left by a failed/crashed writer.
      const registered = db.query("SELECT 1 FROM documents WHERE sha256_hash = ?").get(sha256);
      if (!registered) return { ok: false, errors: ["document evidence destination exists without a registered document"] };
      return { ok: false, errors: ["duplicate document content already ingested"] };
    }
    const result = db.transaction(() => {
      const contentDuplicate = db.query("SELECT document_no FROM documents WHERE sha256_hash = ?").get(sha256) as { document_no: string } | null;
      if (contentDuplicate) throw new Error(`duplicate document content already ingested as ${contentDuplicate.document_no}`);
      const documentNo = nextDocumentNo(db, metadata.issueDate);

      const inserted = db.query(
        `INSERT INTO documents (
          document_no, source, original_filename, stored_path, mime_type, sha256_hash,
          supplier_name, invoice_no, invoice_date, amount_inc_vat, currency, status,
          document_type, delivery_description, sender_name, sender_address, sender_vat_cvr, supplier_country_code, supplier_identifier_kind, supplier_identity_status,
          recipient_name, recipient_address, recipient_vat_cvr, vat_amount, payment_details, exemption_code, payload_json, retain_until
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'ingested', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING id`
      ).get(
        documentNo,
        metadata.source,
        snapshot.filename,
        storedPath,
        mimeType,
        sha256,
        metadata.sender?.name ?? null,
        metadata.invoiceNo ?? null,
        metadata.issueDate ?? null,
        metadata.amountIncVat ?? null,
        currency,
        docType,
        metadata.deliveryDescription ?? null,
        metadata.sender?.name ?? null,
        metadata.sender?.address ?? null,
        senderVatOrCvr ?? null,
        senderIdentity?.ok ? senderIdentity.country : null,
        senderIdentity?.ok ? senderIdentity.identifierKind : null,
        senderIdentity?.ok ? senderIdentity.status : null,
        metadata.recipient?.name ?? null,
        metadata.recipient?.address ?? null,
        metadata.recipient?.vatOrCvr ?? null,
        metadata.vatAmount ?? null,
        metadata.paymentDetails ?? null,
        metadata.exemptionCode ?? null,
        JSON.stringify(metadata),
        retainUntilForDate(db, retentionBasisDate),
      ) as { id: number };

      if (docType === "internal_voucher") {
        const actor = resolveActor({
          createdBy: options.createdBy,
          createdByProgram: options.createdByProgram,
        });
        if ((metadata.internalVoucherKind ?? "bank_evidenced") === "bank_evidenced") {
          db.query(
            `INSERT INTO internal_voucher_evidence
               (document_id, bank_transaction_id, accounting_rationale,
                prepared_by, prepared_by_program)
             VALUES (?, ?, ?, ?, ?)`,
          ).run(inserted.id, metadata.sourceBankTransactionId!, metadata.accountingRationale!.trim(), actor.createdBy, actor.createdByProgram);
        } else {
          db.query(
            `INSERT INTO non_cash_balance_correction_evidence
               (document_id, document_sha256, issue_date, amount, currency,
                accounting_rationale, prepared_by, prepared_by_program)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
          ).run(inserted.id, sha256, metadata.issueDate!, metadata.amountIncVat!, currency, metadata.accountingRationale!.trim(), actor.createdBy, actor.createdByProgram);
          if (metadata.internalVoucherKind === "legacy_opening_creditor_reclassification") db.query("INSERT INTO legacy_opening_creditor_reclassification_evidence (document_id,opening_journal_entry_id,opening_journal_line_id) VALUES(?,?,?)").run(inserted.id, metadata.legacyOpeningJournalEntryId!, metadata.legacyOpeningJournalLineId!);
        }
      }

      strengthenGdprErasureAliasesForIdentity(db, {
        name: metadata.sender?.name,
        cvr: senderVatOrCvr,
      });

      if (scanEvidence) {
        db.query(
          `INSERT INTO document_scan_evidence (document_id, sha256_hash, scanner_id, scanner_version, result, evidence_ref)
           VALUES (?, ?, ?, ?, 'clean', ?)`,
        ).run(inserted.id, sha256, scanEvidence.scannerId, scanEvidence.scannerVersion ?? null, scanEvidence.evidenceRef ?? null);
      }
      strengthenGdprErasureAliasesForIdentity(db, {
        name: metadata.recipient?.name,
        cvr: metadata.recipient?.vatOrCvr,
      });

      if (!options.suppressAudit) {
        insertAuditLog(db, {
          eventType: "document_ingest",
          entityType: "document",
          entityId: inserted.id,
          message: `Ingested supporting document ${documentNo} (${sha256})`,
          createdBy: options.createdBy,
          createdByProgram: options.createdByProgram,
        });
      }

      return { id: asDocumentId(inserted.id), documentNo };
    }).immediate();

    return { ok: true, documentId: result.id, documentNo: result.documentNo, sha256, storedPath };
  } catch (error) {
    if (published) {
      // Never delete a possible concurrent winner. The unique hash register is
      // authoritative; only remove our final when no row references it.
      const registered = db.query("SELECT 1 FROM documents WHERE sha256_hash = ?").get(sha256);
      if (!registered) removePublishedSnapshot(storedPath, snapshot);
    }
    return { ok: false, errors: [error instanceof Error ? error.message : String(error)] };
  }
}

/** A stored bilag file resolved for read-only serving. */
export type ResolvedDocumentFile = {
  /** Absolute path to the file on disk. */
  path: string;
  /** Stored MIME type, or a safe default when none was recorded. */
  mimeType: string;
  /** A human-friendly download name. */
  filename: string;
};

/**
 * Resolves the stored file of an ingested document so a caller (the cockpit's
 * read route) can serve it back to a human.
 *
 * The shared evidence resolver rebases only an exact known storage suffix
 * below THIS company, then verifies the immutable bytes against the register.
 * Returns an error (never throws) when evidence cannot be proven safe.
 */
export function resolveDocumentFile(
  db: Database,
  companyRoot: string,
  documentId: number,
):
  | { ok: true; file: ResolvedDocumentFile }
  | { ok: false; error: string } {
  const row = db
    .query(
      `SELECT stored_path AS storedPath, mime_type AS mimeType,
              original_filename AS filename, document_no AS documentNo,
              document_type AS documentType
         FROM documents WHERE id = ?`,
    )
    .get(documentId) as
    | {
        storedPath: string | null;
        mimeType: string | null;
        filename: string | null;
        documentNo: string | null;
        documentType: string;
      }
    | null;
  if (!row) {
    return { ok: false, error: `document ${documentId} does not exist` };
  }
  if (!row.storedPath) {
    return { ok: false, error: `document ${documentId} has no stored file` };
  }
  let resolved: ReturnType<typeof snapshotRegisteredDocument>;
  try {
    resolved = snapshotRegisteredDocument(db, companyRoot, documentId);
  } catch {
    return { ok: false, error: `document ${documentId} file is missing on disk` };
  }
  return {
    ok: true,
    file: {
      path: resolved.path,
      mimeType: row.mimeType ?? "application/octet-stream",
      filename: row.filename ?? row.documentNo ?? `bilag-${documentId}`,
    },
  };
}
