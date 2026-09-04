import type { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { insertAuditLog, resolveActor } from "./actor";
import { getCompanySettings } from "./company";
import { validateDanishSimplifiedPurchaseInvoiceMetadata, type DocumentMetadata } from "./documents";
import { deductibleDanishPurchaseSupplierErrors, resolveLegacySupplierIdentity, resolveSupplierIdentity } from "./supplier-identity";

export type SetDocumentCompanyContextInput = {
  documentId: number;
  sourceReference: string;
  businessUseReason: string;
  /** Deliberate human confirmation; API adapters must require it too. */
  confirm: boolean;
  createdBy?: string;
  createdByProgram?: string;
};

export type SetDocumentCompanyContextResult = {
  ok: boolean;
  documentId?: number;
  applied?: boolean;
  errors: string[];
};

type ContextRow = { document_sha256: string; payload_sha256: string; context_sha256: string };

function canonicalJson(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  const record = value as Record<string, unknown>;
  return `{${Object.keys(record).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(record[key])}`).join(",")}}`;
}
function sha256(value: string): string { return createHash("sha256").update(value).digest("hex"); }
function text(value: unknown, name: string, errors: string[]): string | null {
  if (typeof value !== "string" || value.trim().length === 0 || value.trim().length > 2000) { errors.push(`${name} is required and must be at most 2000 characters`); return null; }
  return value.trim();
}

function nullableText(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

/** Persisted booking columns must still be the exact projection of the bound payload. */
function persistedFactsMatchPayload(document: Record<string, unknown>, payload: DocumentMetadata): boolean {
  const identity = payload.sender?.countryCode !== undefined || payload.sender?.identifierKind !== undefined
    ? resolveSupplierIdentity({ country: payload.sender?.countryCode ?? "", identifier: payload.sender?.vatOrCvr, identifierKind: payload.sender?.identifierKind })
    : resolveLegacySupplierIdentity(payload.sender?.vatOrCvr);
  if (!identity.ok) return false;
  const sameNumber = (persisted: unknown, source: unknown) => typeof source === "number" && Number.isFinite(source) && Number(persisted) === source;
  return document.document_type === (payload.documentType ?? "purchase_sale")
    && sameNumber(document.amount_inc_vat, payload.amountIncVat)
    && sameNumber(document.vat_amount, payload.vatAmount)
    && String(document.currency).trim().toUpperCase() === (payload.currency ?? "DKK").trim().toUpperCase()
    && nullableText(document.sender_name) === (payload.sender?.name ?? null)
    && nullableText(document.sender_address) === (payload.sender?.address ?? null)
    && nullableText(document.sender_vat_cvr) === identity.identifier
    && nullableText(document.recipient_name) === (payload.recipient?.name ?? null)
    && nullableText(document.recipient_address) === (payload.recipient?.address ?? null)
    && nullableText(document.recipient_vat_cvr) === (payload.recipient?.vatOrCvr ?? null)
    && nullableText(document.invoice_date) === (payload.issueDate ?? null)
    && nullableText(document.invoice_no) === (payload.invoiceNo ?? null)
    && nullableText(document.delivery_description) === (payload.deliveryDescription ?? null);
}

/**
 * A context can supplement a simplified invoice or record independently
 * reviewed company attribution for a truthfully incomplete standard invoice.
 * It is intentionally not a document recipient: no company field is copied
 * back to documents.recipient_* or its payload.
 */
export function setDocumentCompanyContext(db: Database, input: SetDocumentCompanyContextInput): SetDocumentCompanyContextResult {
  const errors: string[] = [];
  if (!Number.isInteger(input.documentId) || input.documentId <= 0) errors.push("documentId must be a positive integer");
  const sourceReference = text(input.sourceReference, "sourceReference", errors);
  const businessUseReason = text(input.businessUseReason, "businessUseReason", errors);
  if (input.confirm !== true) errors.push("document company context requires explicit confirm: true");
  if (errors.length) return { ok: false, errors };
  try {
    return db.transaction(() => {
      const document = db.query(`SELECT id, status, document_type, amount_inc_vat, vat_amount, currency, sha256_hash, payload_json,
        sender_name, sender_address, sender_vat_cvr, supplier_country_code, supplier_identifier_kind, supplier_identity_status,
        recipient_name, recipient_address, recipient_vat_cvr, invoice_date, invoice_no, delivery_description
        FROM documents WHERE id = ?`).get(input.documentId) as Record<string, unknown> | null;
      if (!document) return { ok: false, errors: [`document ${input.documentId} does not exist`] };
      if (document.status !== "ingested") return { ok: false, errors: ["document must be ingested and unposted"] };
      if (db.query("SELECT 1 FROM journal_entries WHERE document_id = ? LIMIT 1").get(input.documentId)
        || db.query("SELECT 1 FROM import_document_links WHERE document_id = ? LIMIT 1").get(input.documentId)
        || db.query("SELECT 1 FROM dinero_import_document_links WHERE document_id = ? LIMIT 1").get(input.documentId)
        || db.query("SELECT 1 FROM payables WHERE document_id = ? LIMIT 1").get(input.documentId)) return { ok: false, errors: ["document is linked to accounting evidence"] };
      let payload: DocumentMetadata;
      try { payload = JSON.parse(String(document.payload_json)) as DocumentMetadata; } catch { return { ok: false, errors: ["document payload_json is not valid JSON"] }; }
      const simplifiedErrors = validateDanishSimplifiedPurchaseInvoiceMetadata(payload);
      const incompleteStandard = payload.incompleteStandardPurchaseInvoice === true
        && payload.danishSimplifiedPurchaseInvoice !== true
        && (payload.documentType ?? "purchase_sale") === "purchase_sale";
      const supplierErrors = deductibleDanishPurchaseSupplierErrors({
        supplierVatOrCvr: document.sender_vat_cvr as string | null,
        supplierCountryCode: document.supplier_country_code as string | null,
        supplierIdentifierKind: document.supplier_identifier_kind as string | null,
        supplierIdentityStatus: document.supplier_identity_status as string | null,
      });
      if ((!incompleteStandard && simplifiedErrors.length) || supplierErrors.length || !persistedFactsMatchPayload(document, payload)) return { ok: false, errors: [incompleteStandard ? "document is not a valid incomplete standard purchase invoice" : "document does not independently satisfy Danish simplified purchase invoice facts", ...(!incompleteStandard ? simplifiedErrors : []), ...supplierErrors] };
      const company = getCompanySettings(db);
      if (!company.name.trim() || company.country !== "DK" || !company.cvr || !company.address?.trim() || !company.postalCode?.trim() || !company.city?.trim() || company.vatPeriodType === null) {
        return { ok: false, errors: ["a complete VAT-registered Danish company profile is required"] };
      }
      const documentHash = String(document.sha256_hash);
      const payloadHash = sha256(String(document.payload_json));
      const snapshot = { id: company.id, name: company.name, country: company.country, cvr: company.cvr, address: company.address, postalCode: company.postalCode, city: company.city, vatPeriodType: company.vatPeriodType };
      const contextJson = canonicalJson({ documentHash, payloadHash, snapshot, sourceReference, businessUseReason });
      const contextHash = sha256(contextJson);
      const existing = db.query("SELECT document_sha256, payload_sha256, context_sha256 FROM document_company_contexts WHERE document_id = ?").get(input.documentId) as ContextRow | null;
      if (existing) return existing.document_sha256 === documentHash && existing.payload_sha256 === payloadHash && existing.context_sha256 === contextHash
        ? { ok: true, documentId: input.documentId, applied: false, errors: [] }
        : { ok: false, errors: ["document company context already exists with conflicting evidence"] };
      const actor = resolveActor({ createdBy: input.createdBy, createdByProgram: input.createdByProgram });
      db.query(`INSERT INTO document_company_contexts (document_id, company_id, document_sha256, payload_sha256, company_snapshot_json, company_snapshot_sha256, source_reference, business_use_reason, context_sha256, actor, program)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`).run(input.documentId, company.id, documentHash, payloadHash, canonicalJson(snapshot), sha256(canonicalJson(snapshot)), sourceReference!, businessUseReason!, contextHash, actor.createdBy, actor.createdByProgram);
      insertAuditLog(db, { eventType: "document_company_context_set", entityType: "document", entityId: input.documentId, message: `Recorded ${incompleteStandard ? "incomplete-standard-invoice" : "simplified-invoice"} company context for document ${input.documentId} (document_sha256=${documentHash}, payload_sha256=${payloadHash})`, createdBy: actor.createdBy, createdByProgram: actor.createdByProgram });
      return { ok: true, documentId: input.documentId, applied: true, errors: [] };
    }).immediate();
  } catch (error) { return { ok: false, errors: [error instanceof Error ? error.message : String(error)] }; }
}

/** Strict read gate for standard purchase VAT. Any tamper or missing fact fails closed. */
export function validSimplifiedPurchaseCompanyContext(db: Database, documentId: number): boolean {
  const row = db.query(`SELECT d.sha256_hash, d.payload_json, d.document_type, d.amount_inc_vat, d.vat_amount, d.currency,
    d.sender_name, d.sender_address, d.sender_vat_cvr, d.recipient_name, d.recipient_address, d.recipient_vat_cvr,
    d.invoice_date, d.invoice_no, d.delivery_description,
    c.company_id, c.document_sha256, c.payload_sha256, c.company_snapshot_json, c.company_snapshot_sha256, c.context_sha256, c.source_reference, c.business_use_reason
    FROM documents d JOIN document_company_contexts c ON c.document_id = d.id WHERE d.id = ?`).get(documentId) as Record<string, unknown> | null;
  if (!row) return false;
  try {
    const payload = JSON.parse(String(row.payload_json)) as DocumentMetadata;
    const snapshot = JSON.parse(String(row.company_snapshot_json)) as Record<string, unknown>;
    return validateDanishSimplifiedPurchaseInvoiceMetadata(payload).length === 0
      && persistedFactsMatchPayload(row, payload)
      && row.company_id === snapshot.id
      && typeof snapshot.cvr === "string" && /^DK\d{8}$/.test(snapshot.cvr)
      && snapshot.country === "DK" && typeof snapshot.vatPeriodType === "string"
      && row.document_sha256 === row.sha256_hash && row.payload_sha256 === sha256(String(row.payload_json))
      && row.company_snapshot_sha256 === sha256(canonicalJson(snapshot))
      && row.context_sha256 === sha256(canonicalJson({ documentHash: row.document_sha256, payloadHash: row.payload_sha256, snapshot, sourceReference: row.source_reference, businessUseReason: row.business_use_reason }));
  } catch { return false; }
}
