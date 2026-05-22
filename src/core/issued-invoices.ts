import { createHash } from "node:crypto";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import type { Database } from "bun:sqlite";
import { companyPaths } from "./paths";
import { addDays } from "./dates";
import { validateInvoice, type InvoicePayload } from "./invoice";
import { promoteTempFile, removeIfExists, writeTempFileFor } from "./atomic-file";
import { insertAuditLog } from "./actor";
import { companySequenceScope, fiscalYearLabelFromDate, reserveSequenceValue, nextSequenceValue } from "./sequences";
import { retainUntilForDate } from "./retention";
import { requireCachedViesValidation } from "./vies";
import { buildIssuedInvoicePdf } from "./invoice-pdf";
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

// #251: the single canonical issued-invoice-number format. The fortløbende
// nummer is the fiscal-year scope, a hyphen, and the sequence value padded to
// four digits (`2026-0001`). Every issuing path — `invoice issue`, the guided
// `invoice create`, the MCP `invoice_issue` tool and recurring invoices — funnels
// through `issueInvoice`, so this one function fixes the format for all of them.
// Four digits matches the credit-note series (`CN-<scope>-NNNN`); journal
// entries keep their own separate `entry_no` series.
const INVOICE_NUMBER_DIGITS = 4;

function canonicalInvoiceNumber(scope: string, value: number): InvoiceNumber {
  return asInvoiceNumber(`${scope}-${String(value).padStart(INVOICE_NUMBER_DIGITS, "0")}`);
}

function invoiceSequenceState(db: Database, issueDate: string) {
  const scope = fiscalYearLabelFromDate(db, issueDate);
  // The GLOB matches the canonical four-digit suffix; `substr(invoice_no, -4)`
  // then yields exactly that suffix as the numeric floor.
  const row = db.query(`SELECT COALESCE(MAX(CAST(substr(invoice_no, -4) AS INTEGER)), 0) AS n FROM documents WHERE document_type = 'issued_invoice' AND invoice_no GLOB ?`).get(`${scope}-[0-9][0-9][0-9][0-9]`) as { n: number };
  return { scope, currentFloor: Number(row.n ?? 0), sequenceScope: companySequenceScope(db, scope) };
}

// Manual invoice numbers must be <scope>-<digits>: the scope is everything
// before the final hyphen, the suffix is one or more decimal digits. The
// numeric value of the suffix is what is reserved against the sequence.
//
// #251: the suffix is always re-padded to the canonical 5-digit form before it
// is stored, so a manually supplied `2026-0001` and an auto-generated number
// for the same sequence value both become the identical string `2026-00001`.
// Without this, `invoice issue` (from example JSON carrying a 4-digit number)
// and `invoice create` (auto-numbered, 5-digit) produced two different,
// colliding formats in the same ledger — a fortløbende-nummer compliance fault.
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
  // #251: store the canonical 5-digit form, never the verbatim manual string,
  // so the issued series stays one consistent format regardless of how the
  // number was supplied.
  return { ok: true as const, invoiceNumber: canonicalInvoiceNumber(scope, requestedValue) };
}

function nextIssuedInvoiceNumber(db: Database, issueDate: string) {
  const { scope, currentFloor, sequenceScope } = invoiceSequenceState(db, issueDate);
  const nextValue = nextSequenceValue(db, "issued_invoice", sequenceScope, currentFloor);
  return canonicalInvoiceNumber(scope, nextValue);
}

export function issueInvoice(db: Database, companyRoot: string, rawPayload: InvoicePayload): IssueInvoiceResult {
  // #221: fill the seller identity + due date from the stored company profile
  // before validation, so the owner never re-types their own master data.
  const payload = enrichInvoiceFromCompany(db, rawPayload);
  const validation = validateInvoice(payload);
  const appliedRules = [...new Set([...(validation.appliedRules ?? []), RULE_ID, LOCK_RULE_ID])];
  if (!validation.ok) return { ok: false, appliedRules, errors: validation.errors };

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

  try {
    const result = db.transaction(() => {
      let invoiceNumber: InvoiceNumber;
      if (explicitInvoiceNumber !== undefined) {
        const reserved = reserveManualInvoiceNumber(db, payload.issueDate!, explicitInvoiceNumber);
        if (!reserved.ok) return { ok: false as const, error: reserved.error };
        // #251: use the canonicalised 5-digit number, not the verbatim input,
        // so the persisted snapshot, PDF and documents row all carry the same
        // consistent format as an auto-numbered invoice.
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
      };
      const serialized = JSON.stringify(issuedPayload, null, 2);
      const hash = sha256(serialized);
      const pdfBytes = buildIssuedInvoicePdf(issuedPayload);
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
      payload.currency ?? 'DKK',
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
      retainUntilForDate(db, payload.issueDate),
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
      payload.currency ?? 'DKK',
      payload.seller?.name ?? null,
      payload.seller?.address ?? null,
      payload.seller?.vatOrCvr ?? null,
      payload.buyer?.name ?? null,
      payload.buyer?.address ?? null,
      payload.buyer?.vatOrCvr ?? null,
      vatAmount,
      serialized,
      retainUntilForDate(db, payload.issueDate),
    ) as { id: number };

      insertAuditLog(db, {
        eventType: "invoice_issue",
        entityType: "document",
        entityId: inserted.id,
        message: `Issued invoice ${invoiceNumber}`,
      });
      insertAuditLog(db, {
        eventType: "invoice_render_pdf",
        entityType: "document",
        entityId: pdfInserted.id,
        message: `Rendered invoice PDF ${invoiceNumber}`,
      });

      return { ok: true as const, documentId: asDocumentId(inserted.id), invoiceNumber, sha256: hash, pdfDocumentId: asDocumentId(pdfInserted.id), pdfSha256: pdfHash };
    }, { immediate: true })();

    if (!result.ok) return { ok: false, appliedRules, errors: [result.error] };
    promoteTempFile(tempPath!, storedPath!);
    promoteTempFile(pdfTempPath!, pdfStoredPath!);
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
    };
  } catch (error) {
    if (tempPath) removeIfExists(tempPath);
    if (pdfTempPath) removeIfExists(pdfTempPath);
    throw error;
  }
}
