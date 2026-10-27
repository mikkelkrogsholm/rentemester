import { createHash } from "node:crypto";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import type { Database } from "bun:sqlite";
import { companyPaths } from "./paths";
import { addDays, isValidIsoDate } from "./dates";
import { validateInvoice, type InvoicePayload } from "./invoice";
import { promoteTempFileExclusive, removeIfExists, writeTempFileFor } from "./atomic-file";
import { insertAuditLog, type ResolveActorInput } from "./actor";
import { companySequenceScope, fiscalYearLabelFromDate, reserveSequenceValue, nextSequenceValue, currentSequenceValue } from "./sequences";
import { retainUntilForDate } from "./retention";
import { validateJournalTransactionDate } from "./periods";
import { requireCachedViesValidation } from "./vies";
import { buildIssuedInvoicePdf } from "./invoice-pdf";
import { canonicalJson } from "./canonical-json";
import { approvedOioUblXml } from "./public-einvoice";
import { strengthenGdprErasureAliasesForIdentity } from "./gdpr";
import {
  companyAddressLine,
  getCompanySettings,
  resolveCompanyPaymentDetails,
} from "./company";
import {
  asDocumentId,
  asInvoiceNumber,
  type DocumentId,
  type InvoiceNumber,
} from "./ids";

export type IssueInvoiceResult = {
  ok: boolean;
  documentId?: DocumentId;
  invoiceNumber?: InvoiceNumber;
  storedPath?: string;
  sha256?: string;
  pdfDocumentId?: DocumentId;
  pdfStoredPath?: string;
  pdfSha256?: string;
  appliedRules: string[];
  errors: string[];
  // EJER-6: non-blocking advisories. Currently surfaces the case where the
  // invoice's issue date falls in an already-closed/reported accounting period
  // — the invoice document is fine, but the journal entry that books it will be
  // rejected by the period lock, so the owner is warned up front.
  warnings?: string[];
};

const RULE_ID = "DK-INVOICE-ISSUE-001";
const LOCK_RULE_ID = "DK-INVOICE-LOCK-001";

function hasText(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

/**
 * #221: enrich the invoice with the company's own master data so the owner
 * never re-types it. Seller identity (name / address / CVR) is filled from the
 * stored company profile whenever the payload leaves a field blank; an explicit
 * payload value always wins. When the company profile exists and the payload
 * has no due date, the due date defaults to the company's payment terms. The
 * result still goes through `validateInvoice`, so a company with no CVR/address
 * configured still fails with the same clear error.
 */
function enrichInvoiceFromCompany(db: Database, payload: InvoicePayload): InvoicePayload {
  let settings: ReturnType<typeof getCompanySettings>;
  let companyRowExists = false;
  try {
    settings = getCompanySettings(db);
    companyRowExists =
      (db.query("SELECT id FROM companies WHERE id = 1").get() as { id: number } | null) !== null;
  } catch {
    // Older ledgers without the profile columns: leave the payload untouched.
    return payload;
  }
  const companyAddress = companyAddressLine(settings);
  const seller = {
    name: hasText(payload.seller?.name) ? payload.seller!.name : settings.name || undefined,
    address: hasText(payload.seller?.address)
      ? payload.seller!.address
      : companyAddress ?? undefined,
    vatOrCvr: hasText(payload.seller?.vatOrCvr)
      ? payload.seller!.vatOrCvr
      : settings.cvr ?? undefined,
  };
  // The due date only defaults from the company's payment terms when the
  // company profile actually exists — a never-initialised ledger keeps the
  // payload's (possibly absent) due date untouched.
  const dueDate =
    hasText(payload.dueDate)
      ? payload.dueDate
      : companyRowExists && hasText(payload.issueDate) && settings.paymentTermsDays >= 0
        ? addDays(payload.issueDate!, settings.paymentTermsDays)
        : payload.dueDate;
  return { ...payload, seller, ...(dueDate !== undefined ? { dueDate } : {}) };
}

// EJER-6: warn (do not block) when the invoice's issue date falls inside an
// already-closed/reported accounting period. Reuses the exact period-lock
// detection postings use (validateJournalTransactionDate), but downgrades the
// closed-period finding to an advisory: the invoice document is not itself a
// ledger posting, yet the journal entry that books it later WILL be rejected by
// the period lock — so the owner should know now. Future-date findings from the
// same validator are NOT surfaced here (an invoice may legitimately be future-
// dated); only the closed/reported-period case becomes a warning.
function closedPeriodIssueWarnings(db: Database, issueDate: string | undefined): string[] {
  if (!issueDate) return [];
  return validateJournalTransactionDate(db, issueDate)
    .filter((message) => message.includes("period"))
    .map((message) => `Fakturadato ${issueDate} ligger i en lukket periode: ${message}. Fakturaen er udstedt, men bogføringen vil blive afvist af periodelåsen.`);
}

function deliveryDescription(payload: InvoicePayload) {
  if (payload.deliveryDate) return `Delivery date ${payload.deliveryDate}`;
  if (payload.deliveryPeriodStart && payload.deliveryPeriodEnd) {
    return `Delivery period ${payload.deliveryPeriodStart}..${payload.deliveryPeriodEnd}`;
  }
  return payload.lines?.map((l) => l.description).filter(Boolean).join('; ') ?? null;
}

function sha256(text: string) {
  return createHash("sha256").update(text).digest("hex");
}

function hasCommittedDocumentAtPath(db: Database, storedPath: string): boolean | null {
  try {
    return db.query("SELECT 1 AS present FROM documents WHERE stored_path = ? LIMIT 1").get(storedPath) != null;
  } catch {
    // An ambiguous COMMIT/I/O failure must retain a possibly authoritative
    // legal artifact. A recoverable orphan is safer than a committed row whose
    // file was deleted merely because database state could not be re-read.
    return null;
  }
}

// Every issuance path uses this shared numbering policy. Fiscal-year series
// retain four digits; daily RM series use at least two digits.
const INVOICE_NUMBER_DIGITS = 4;

function canonicalInvoiceNumber(scope: string, value: number): InvoiceNumber {
  return asInvoiceNumber(`${scope}-${String(value).padStart(scope.startsWith("RM-") ? 2 : INVOICE_NUMBER_DIGITS, "0")}`);
}

function invoiceSequenceState(db: Database, issueDate: string) {
  const daily = getCompanySettings(db).invoiceNumberFormat === "daily";
  // Two-digit years repeat after a century; fail closed rather than reuse a
  // daily series identity for a different date.
  if (daily && (issueDate < "2000-01-01" || issueDate > "2099-12-31")) {
    throw new Error("daily invoice numbering supports issue dates from 2000 through 2099");
  }
  const scope = daily ? `RM-${issueDate.slice(2)}` : fiscalYearLabelFromDate(db, issueDate);
  // Read the complete numeric suffix rather than a fixed width: a daily series
  // can grow past 99 and a yearly series can grow past 9,999.
  const prefix = `${scope}-`;
  const row = db
    .query(
      `SELECT COALESCE(MAX(CAST(substr(invoice_no, ? + 1) AS INTEGER)), 0) AS n
         FROM documents
        WHERE document_type = 'issued_invoice'
          AND substr(invoice_no, 1, ?) = ?
          AND length(invoice_no) > ?
          AND substr(invoice_no, ? + 1) NOT GLOB '*[^0-9]*'`,
    )
    .get(prefix.length, prefix.length, prefix, prefix.length, prefix.length) as { n: number };
  return { scope, currentFloor: Number(row.n ?? 0), sequenceScope: companySequenceScope(db, scope) };
}

// Manual invoice numbers must be <scope>-<digits>: the scope is everything
// before the final hyphen, the suffix is one or more decimal digits. The
// numeric value of the suffix is what is reserved against the sequence.
//
// Canonicalise manual suffixes to the configured width so different padding
// cannot create two representations of the same position in a series.
const MANUAL_INVOICE_NUMBER_RE = /^(.+)-([0-9]+)$/;

function validateManualInvoiceNumberScope(db: Database, issueDate: string, invoiceNumber: string) {
  const { scope } = invoiceSequenceState(db, issueDate);
  const match = MANUAL_INVOICE_NUMBER_RE.exec(invoiceNumber);
  if (!match) {
    return `manual invoiceNumber ${invoiceNumber} must be of the form <scope>-<number>`;
  }
  if (match[1] !== scope) {
    return `manual invoiceNumber ${invoiceNumber} does not match current fiscal scope ${scope}`;
  }
  return null;
}

function reserveManualInvoiceNumber(db: Database, issueDate: string, invoiceNumber: string) {
  const { scope, currentFloor, sequenceScope } = invoiceSequenceState(db, issueDate);
  const match = MANUAL_INVOICE_NUMBER_RE.exec(invoiceNumber);
  if (!match || match[1] !== scope) {
    return { ok: false as const, error: `manual invoiceNumber ${invoiceNumber} must be of the form <scope>-<number>` };
  }
  const requestedValue = Number(match[2]);
  const reserved = reserveSequenceValue(db, "issued_invoice", sequenceScope, requestedValue, currentFloor);
  if (!reserved.ok) {
    return {
      ok: false as const,
      error: `manual invoiceNumber ${invoiceNumber} exceeds næste fortløbende nummer ${canonicalInvoiceNumber(scope, reserved.expectedValue)}`,
    };
  }
  // #251: store the canonical four-digit form, never the verbatim manual
  // string, so the issued series stays one consistent format regardless of how
  // the number was supplied.
  return { ok: true as const, invoiceNumber: canonicalInvoiceNumber(scope, requestedValue) };
}

function nextIssuedInvoiceNumber(db: Database, issueDate: string) {
  const { scope, currentFloor, sequenceScope } = invoiceSequenceState(db, issueDate);
  const nextValue = nextSequenceValue(db, "issued_invoice", sequenceScope, currentFloor);
  return canonicalInvoiceNumber(scope, nextValue);
}

/** A non-binding candidate; issuance must still reserve it transactionally. */
export function previewNextInvoiceNumber(db: Database, issueDate: string): InvoiceNumber {
  if (!isValidIsoDate(issueDate)) throw new Error("issueDate must be present in YYYY-MM-DD format");
  const { scope, currentFloor, sequenceScope } = invoiceSequenceState(db, issueDate);
  return canonicalInvoiceNumber(scope, currentSequenceValue(db, "issued_invoice", sequenceScope, currentFloor) + 1);
}

/** Snapshot the financial and payment fields that an invoice approval covers. */
export function prepareInvoiceApprovalPayload(db: Database, rawPayload: InvoicePayload) {
  if (Object.hasOwn(rawPayload, "approvedPdf")) return { ok: false as const, errors: ["approvedPdf is reserved for issued evidence"] };
  const payload = enrichInvoiceFromCompany(db, rawPayload);
  const validation = validateInvoice(payload);
  if (!validation.ok) return { ok: false as const, errors: validation.errors };
  const invoiceNumber = previewNextInvoiceNumber(db, payload.issueDate!);
  if (payload.invoiceNumber && payload.invoiceNumber !== invoiceNumber) {
    return { ok: false as const, errors: [`invoice number is no longer the next available number: ${invoiceNumber}`] };
  }
  const currency = (payload.currency ?? "DKK").trim().toUpperCase();
  const payment = resolveCompanyPaymentDetails(db, currency);
  const snapshot = { ...payload, currency, invoiceNumber, ...(payment ? { payment } : {}) };
  const payloadSha256 = createHash("sha256").update(canonicalJson(snapshot)).digest("hex");
  return { ok: true as const, payload: snapshot, payloadSha256, invoiceNumber };
}

/**
 * Result of a dry-run invoice render (#440). The bytes are the customer-facing
 * PDF — identical to what `issueInvoice` produces for the same input — but
 * NOTHING is written to the ledger: no document row, no audit_log entry, no
 * sequence draw, no file written to disk. The `invoiceNumber` is a non-binding
 * preview tag (`<scope>-UDKAST`) so the owner sees a fakturanummer placeholder
 * without burning a number from the fortløbende serie.
 */
export type PreviewIssuedInvoicePdfResult =
  | { ok: true; pdfBytes: Uint8Array; invoiceNumber: string; appliedRules: string[]; errors: [] }
  | { ok: false; pdfBytes?: undefined; invoiceNumber?: undefined; appliedRules: string[]; errors: string[] };

/**
 * Render the customer-facing invoice PDF without writing anything to the
 * ledger (#440). The owner sees exactly the same layout, amounts and payment
 * details the real `issueInvoice` call would produce, but the fortløbende-
 * nummer-sekvens is untouched and no audit_log / documents row is created.
 *
 * Determinism: identical to `issueInvoice` for the same input — same enrich
 * step (company master data), same validator, same `buildIssuedInvoicePdf`
 * renderer, same payment-details resolution. Only the invoice number differs
 * (a placeholder `<scope>-UDKAST`), because drawing the real next number from
 * the sequence would create a hole in the serie if the owner closed the
 * preview without issuing.
 */
export function previewIssuedInvoicePdf(
  db: Database,
  rawPayload: InvoicePayload,
): PreviewIssuedInvoicePdfResult {
  const PREVIEW_RULE_ID = "DK-INVOICE-PREVIEW-001";
  const payload = enrichInvoiceFromCompany(db, rawPayload);
  const validation = validateInvoice(payload);
  const appliedRules = [...new Set([...(validation.appliedRules ?? []), PREVIEW_RULE_ID])];
  if (!validation.ok) {
    return { ok: false, appliedRules, errors: validation.errors };
  }

  // Compute the placeholder from the configured series; it is not a final
  // number and does not reserve a sequence value. Falls back
  // to a plain "UDKAST" if the fiscal-year scope cannot be resolved (older
  // ledgers / missing companies row).
  let previewNumber = payload.invoiceNumber?.trim() || "UDKAST";
  if (!payload.invoiceNumber?.trim()) {
    try {
      if (payload.issueDate) {
        const scope = invoiceSequenceState(db, payload.issueDate).scope;
        previewNumber = `${scope}-UDKAST`;
      }
    } catch {
      // keep "UDKAST" fallback
    }
  }

  const invoiceCurrency = (payload.currency ?? "DKK").trim().toUpperCase();
  let paymentDetails: ReturnType<typeof resolveCompanyPaymentDetails> | undefined;
  try {
    paymentDetails = resolveCompanyPaymentDetails(db, invoiceCurrency);
  } catch {
    paymentDetails = undefined;
  }

  const pdfBytes = buildIssuedInvoicePdf({
    ...payload,
    invoiceNumber: previewNumber,
    status: "preview",
    ...(paymentDetails ? { payment: paymentDetails } : {}),
  });

  return {
    ok: true,
    pdfBytes,
    invoiceNumber: previewNumber,
    appliedRules,
    errors: [],
  };
}

export function issueInvoice(
  db: Database,
  companyRoot: string,
  rawPayload: InvoicePayload,
  actor: ResolveActorInput = {},
  approvedPdf?: { preflightId: string; pdfBytes: Uint8Array; visualReviewConfirmed: boolean; providerBinding: string },
): IssueInvoiceResult {
  // #221: fill the seller identity + due date from the stored company profile
  // before validation, so the owner never re-types their own master data.
  const payload = enrichInvoiceFromCompany(db, rawPayload);
  const validation = validateInvoice(payload);
  const appliedRules = [...new Set([...(validation.appliedRules ?? []), RULE_ID, LOCK_RULE_ID])];
  if (Object.hasOwn(rawPayload, "approvedPdf")) return { ok: false, appliedRules, errors: ["approvedPdf is reserved for issued evidence"] };
  if (!validation.ok) return { ok: false, appliedRules, errors: validation.errors };
  if (approvedPdf && (!approvedPdf.visualReviewConfirmed || approvedPdf.pdfBytes.byteLength === 0 || approvedPdf.pdfBytes.byteLength > 7_000_000)) {
    return { ok: false, appliedRules, errors: ["Approved PDF requires visual review and a supported PDF size"] };
  }
  // Hold exactly one snapshot through verification and immutable publication.
  const approvedPdfBytes = approvedPdf ? Buffer.from(approvedPdf.pdfBytes) : undefined;

  if (getCompanySettings(db).invoiceNumberFormat === "daily" &&
      (payload.issueDate! < "2000-01-01" || payload.issueDate! > "2099-12-31")) {
    return { ok: false, appliedRules, errors: ["daily invoice numbering supports issue dates from 2000 through 2099"] };
  }

  // #221: resolve the company's payment details once, so the customer-facing
  // PDF built at issue time always carries the BETALING block (where to pay).
  const invoiceCurrency = (payload.currency ?? "DKK").trim().toUpperCase();
  const paymentDetails = resolveCompanyPaymentDetails(db, invoiceCurrency);

  let viesValidation: ReturnType<typeof requireCachedViesValidation>["validation"] | undefined;
  if (payload.vatTreatment === "foreign_reverse_charge") {
    const viesCheck = requireCachedViesValidation(db, payload.buyer?.vatOrCvr, "buyer.vatOrCvr");
    if (!viesCheck.ok) return { ok: false, appliedRules: [...new Set([...appliedRules, ...viesCheck.appliedRules])], errors: viesCheck.errors };
    viesValidation = viesCheck.validation;
  }

  const explicitInvoiceNumber = payload.invoiceNumber?.trim();
  if (explicitInvoiceNumber) {
    const scopeError = validateManualInvoiceNumberScope(db, payload.issueDate!, explicitInvoiceNumber);
    if (scopeError) return { ok: false, appliedRules, errors: [scopeError] };
  }
  const paths = companyPaths(companyRoot);
  mkdirSync(paths.invoicesIssued, { recursive: true });

  let tempPath: string | undefined;
  let storedPath: string | undefined;
  let pdfTempPath: string | undefined;
  let pdfStoredPath: string | undefined;
  let storedPathPromoted = false;
  let pdfStoredPathPromoted = false;

  // Calculate advisory-only output before any irreversible publication. A
  // warning lookup failure must never make a committed legal document look as
  // though issuance failed to its caller.
  const warnings = closedPeriodIssueWarnings(db, payload.issueDate);

  try {
    const result = db.transaction(() => {
      let approvalEvidence: { id: string; invoice_no: string; payload_sha256: string; pdf_sha256: string; xml_sha256: string; created_at: string; provider_binding: string } | undefined;
      if (approvedPdf && approvedPdfBytes) {
        approvalEvidence = db.query("SELECT id,invoice_no,payload_sha256,pdf_sha256,xml_sha256,created_at,provider_binding FROM approved_invoice_preflights WHERE id=?").get(approvedPdf.preflightId) as typeof approvalEvidence;
        const current = prepareInvoiceApprovalPayload(db, rawPayload);
        const issuanceSnapshot = current.ok ? { ...payload, currency: invoiceCurrency, invoiceNumber: current.invoiceNumber, ...(paymentDetails ? { payment: paymentDetails } : {}) } : null;
        const companyCvr = getCompanySettings(db).cvr?.replace(/^DK/i, "").replace(/\s/g, "");
        const sellerCvr = payload.seller?.vatOrCvr?.replace(/^DK/i, "").replace(/\s/g, "");
        const age = approvalEvidence ? Date.now() - Date.parse(`${approvalEvidence.created_at.replace(" ", "T")}Z`) : NaN;
        if (!approvalEvidence || approvalEvidence.provider_binding !== approvedPdf.providerBinding || !Number.isFinite(age) || age < 0 || age > 60 * 60 * 1000 || !current.ok || current.invoiceNumber !== approvalEvidence.invoice_no ||
            !companyCvr || companyCvr !== sellerCvr ||
            current.payloadSha256 !== approvalEvidence.payload_sha256 ||
            createHash("sha256").update(canonicalJson(issuanceSnapshot)).digest("hex") !== current.payloadSha256 ||
            createHash("sha256").update(approvedPdfBytes).digest("hex") !== approvalEvidence.pdf_sha256 ||
            !approvedOioUblXml(current.invoiceNumber, current.payload, approvedPdfBytes, approvalEvidence.xml_sha256)) {
          return { ok: false as const, error: "Approved PDF or invoice changed since preflight; repeat preflight and review" };
        }
        if (explicitInvoiceNumber && explicitInvoiceNumber !== current.invoiceNumber) return { ok: false as const, error: "Approved invoice number is no longer available" };
      }
      let invoiceNumber: InvoiceNumber;
      if (explicitInvoiceNumber !== undefined) {
        const reserved = reserveManualInvoiceNumber(db, payload.issueDate!, explicitInvoiceNumber);
        if (!reserved.ok) return { ok: false as const, error: reserved.error };
        // Use the canonicalised number, not the verbatim
        // input, so the persisted snapshot, PDF and documents row all carry the
        // same consistent format as an auto-numbered invoice.
        invoiceNumber = asInvoiceNumber(reserved.invoiceNumber);
      } else {
        invoiceNumber = nextIssuedInvoiceNumber(db, payload.issueDate!);
      }

      const issuedAt = new Date().toISOString();
      const issuedPayload = {
        ...payload,
        invoiceNumber,
        issuedAt,
        status: "issued",
        ...(viesValidation ? { viesValidation } : {}),
        // #221: persist the resolved payment details into the snapshot so the
        // at-issue PDF — and any later `invoice render` — show where to pay.
        ...(paymentDetails ? { payment: paymentDetails } : {}),
        ...(approvalEvidence ? { approvedPdf: { preflightId: approvalEvidence.id, sha256: approvalEvidence.pdf_sha256, xmlSha256: approvalEvidence.xml_sha256, providerBinding: approvalEvidence.provider_binding } } : {}),
      };
      const serialized = JSON.stringify(issuedPayload, null, 2);
      const hash = sha256(serialized);
      const pdfBytes = approvedPdfBytes ?? buildIssuedInvoicePdf(issuedPayload);
      const pdfHash = createHash("sha256").update(pdfBytes).digest("hex");
      storedPath = join(paths.invoicesIssued, `${invoiceNumber}.json`);
      pdfStoredPath = join(paths.invoicesIssued, `${invoiceNumber}.pdf`);
      tempPath = writeTempFileFor(storedPath, serialized);
      pdfTempPath = writeTempFileFor(pdfStoredPath, pdfBytes);

      const grossAmount = payload.totals?.grossAmount ?? null;
      const vatAmount = payload.totals?.vatAmount ?? null;
      const inserted = db.query(
      `INSERT INTO documents (
        document_no, source, original_filename, stored_path, mime_type, sha256_hash,
        supplier_name, invoice_no, invoice_date, amount_inc_vat, currency, status,
        document_type, delivery_description, sender_name, sender_address, sender_vat_cvr,
        recipient_name, recipient_address, recipient_vat_cvr, vat_amount, payment_details, exemption_code, payload_json, retain_until
      ) VALUES (?, 'rentemester', ?, ?, 'application/json', ?, ?, ?, ?, ?, ?, 'issued', 'issued_invoice', ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?)
      RETURNING id`
    ).get(
      invoiceNumber,
      `${invoiceNumber}.json`,
      storedPath,
      hash,
      payload.seller?.name ?? null,
      invoiceNumber,
      payload.issueDate ?? null,
      grossAmount,
      invoiceCurrency,
      deliveryDescription(payload),
      payload.seller?.name ?? null,
      payload.seller?.address ?? null,
      payload.seller?.vatOrCvr ?? null,
      payload.buyer?.name ?? null,
      payload.buyer?.address ?? null,
      payload.buyer?.vatOrCvr ?? null,
      vatAmount,
      payload.reverseChargeBasis ?? null,
      serialized,
      retainUntilForDate(db, payload.issueDate!),
    ) as { id: number };

      const pdfInserted = db.query(
      `INSERT INTO documents (
        document_no, source, original_filename, stored_path, mime_type, sha256_hash,
        supplier_name, invoice_no, invoice_date, amount_inc_vat, currency, status,
        document_type, sender_name, sender_address, sender_vat_cvr,
        recipient_name, recipient_address, recipient_vat_cvr, vat_amount, payload_json, retain_until
      ) VALUES (?, 'rentemester', ?, ?, 'application/pdf', ?, ?, ?, ?, ?, ?, 'issued', 'issued_invoice_pdf', ?, ?, ?, ?, ?, ?, ?, ?, ?)
      RETURNING id`
    ).get(
      `${invoiceNumber}-pdf`,
      `${invoiceNumber}.pdf`,
      pdfStoredPath,
      pdfHash,
      payload.seller?.name ?? null,
      invoiceNumber,
      payload.issueDate ?? null,
      grossAmount,
      invoiceCurrency,
      payload.seller?.name ?? null,
      payload.seller?.address ?? null,
      payload.seller?.vatOrCvr ?? null,
      payload.buyer?.name ?? null,
      payload.buyer?.address ?? null,
      payload.buyer?.vatOrCvr ?? null,
      vatAmount,
      serialized,
      retainUntilForDate(db, payload.issueDate!),
      ) as { id: number };

      strengthenGdprErasureAliasesForIdentity(db, {
        name: payload.seller?.name,
        cvr: payload.seller?.vatOrCvr,
      });
      strengthenGdprErasureAliasesForIdentity(db, {
        name: payload.buyer?.name,
        cvr: payload.buyer?.vatOrCvr,
      });

      insertAuditLog(db, {
        eventType: "invoice_issue",
        entityType: "document",
        entityId: inserted.id,
        message: `Issued invoice ${invoiceNumber}`,
        createdBy: actor.createdBy,
        createdByProgram: actor.createdByProgram,
      });
      insertAuditLog(db, {
        eventType: "invoice_render_pdf",
        entityType: "document",
        entityId: pdfInserted.id,
        message: `Rendered invoice PDF ${invoiceNumber}`,
        createdBy: actor.createdBy,
        createdByProgram: actor.createdByProgram,
      });

      // Publish both immutable artifacts while the database transaction is
      // still open. A publication failure throws and rolls every document,
      // audit, and sequence row back. If COMMIT itself then fails, the outer
      // catch removes the already-published paths.
      promoteTempFileExclusive(tempPath!, storedPath!);
      storedPathPromoted = true;
      promoteTempFileExclusive(pdfTempPath!, pdfStoredPath!);
      pdfStoredPathPromoted = true;

      return { ok: true as const, documentId: asDocumentId(inserted.id), invoiceNumber, sha256: hash, pdfDocumentId: asDocumentId(pdfInserted.id), pdfSha256: pdfHash };
    }).immediate();

    if (!result.ok) return { ok: false, appliedRules, errors: [result.error] };
    return {
      ok: true,
      documentId: result.documentId,
      invoiceNumber: result.invoiceNumber,
      storedPath,
      sha256: result.sha256,
      pdfDocumentId: result.pdfDocumentId,
      pdfStoredPath,
      pdfSha256: result.pdfSha256,
      appliedRules,
      errors: [],
      ...(warnings.length > 0 ? { warnings } : {}),
    };
  } catch (error) {
    if (tempPath) removeIfExists(tempPath);
    if (pdfTempPath) removeIfExists(pdfTempPath);
    if (
      pdfStoredPathPromoted &&
      pdfStoredPath &&
      hasCommittedDocumentAtPath(db, pdfStoredPath) === false
    ) removeIfExists(pdfStoredPath);
    if (
      storedPathPromoted &&
      storedPath &&
      hasCommittedDocumentAtPath(db, storedPath) === false
    ) removeIfExists(storedPath);
    throw error;
  }
}
