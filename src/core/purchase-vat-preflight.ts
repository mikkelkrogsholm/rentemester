import type { Database } from "bun:sqlite";
import { recordException } from "./exceptions";
import {
  normalizeEuVatNumber,
  storeViesValidation,
  type NormalizedEuVat,
} from "./vies";
import { resolvePersistedSupplierIdentity } from "./supplier-identity";
import { inspectDocumentInvoiceExtraction } from "./invoice-extraction";
import { nonEuReverseChargeEvidenceErrors } from "./vat";

/** Evidence is reusable for 90 days. This is deliberately explicit so callers
 * can show when a provider would be contacted without performing I/O. */
export const PURCHASE_VAT_EVIDENCE_MAX_AGE_DAYS = 90;
export type PurchaseVatClassification = "DK" | "EU" | "NON_EU" | "CONFLICT";
export type VatValidationProviderResult = { status: "valid" | "invalid" | "inconclusive" | "unavailable"; name?: string | null; address?: string | null; rawResponse?: string | null };
/** A normalized identity is the only legal input to an EU VAT provider. */
export type VatValidationProvider = {
  validate(input: NormalizedEuVat): Promise<VatValidationProviderResult>;
};
export type VatValidationClock = { now(): Date };
export const systemVatValidationClock: VatValidationClock = { now: () => new Date() };

type SupplierDocument = { sender_vat_cvr: string | null; supplier_country_code: string | null; supplier_identifier_kind: string | null; supplier_identity_status: string | null };
function supplier(db: Database, documentId: number): SupplierDocument | null {
  return db.query("SELECT sender_vat_cvr, supplier_country_code, supplier_identifier_kind, supplier_identity_status FROM documents WHERE id = ?").get(documentId) as SupplierDocument | null;
}
function classification(row: SupplierDocument | null): { classification: PurchaseVatClassification; identifier: string | null; country: string | null } {
  if (!row) return { classification: "CONFLICT", identifier: null, country: null };
  const identity = resolvePersistedSupplierIdentity({ supplierVatOrCvr: row.sender_vat_cvr, supplierCountryCode: row.supplier_country_code, supplierIdentifierKind: row.supplier_identifier_kind, supplierIdentityStatus: row.supplier_identity_status });
  if (!identity.ok) return { classification: "CONFLICT", identifier: null, country: row.supplier_country_code };
  if (identity.identifierKind === "dk_cvr") return { classification: "DK", identifier: identity.identifier, country: identity.country };
  if (identity.identifierKind === "non_eu") return { classification: "NON_EU", identifier: identity.identifier, country: identity.country };
  return { classification: "EU", identifier: identity.identifier, country: identity.country };
}
function freshEvidence(db: Database, documentId: number, now: string, country: string | null, identifier: string | null) {
  // Evidence is bound to both the immutable document and the normalized legal
  // identity.  A later correction to supplier facts must never reuse an old
  // document-local success event.
  return db.query("SELECT id, evidence_expires_at FROM vat_validation_events WHERE document_id = ? AND supplier_country_code IS ? AND supplier_identifier IS ? AND event_type = 'preflight_passed' AND evidence_expires_at >= ? ORDER BY id DESC LIMIT 1").get(documentId, country, identifier, now) as { id: number; evidence_expires_at: string } | null;
}
function event(db: Database, input: { documentId: number; eventType: string; classification: PurchaseVatClassification; providerStatus: string; country?: string | null; identifier?: string | null; name?: string | null; address?: string | null; actor?: string; expiresAt?: string | null; detail?: unknown; at: string }) {
  db.query("INSERT INTO vat_validation_events(document_id,event_type,supplier_country_code,supplier_identifier,classification,provider_status,provider_name,provider_address,actor,evidence_expires_at,detail_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").run(input.documentId, input.eventType, input.country ?? null, input.identifier ?? null, input.classification, input.providerStatus, input.name ?? null, input.address ?? null, input.actor ?? null, input.expiresAt ?? null, input.detail === undefined ? null : JSON.stringify(input.detail), input.at);
}
export type PurchaseVatPreflightInspection = { ok: boolean; documentId: number; classification: PurchaseVatClassification; cached: boolean; evidenceExpiresAt: string | null; wouldCallProvider: boolean; errors: string[] };
/** Pure inspection: it never calls a provider and never writes evidence. */
export function inspectPurchaseVatPreflight(db: Database, documentId: number, options: { clock?: VatValidationClock } = {}): PurchaseVatPreflightInspection {
  const extracted = inspectDocumentInvoiceExtraction(db, documentId);
  if (extracted && extracted.status !== "completed") return { ok: false, documentId, classification: "CONFLICT", cached: false, evidenceExpiresAt: null, wouldCallProvider: false, errors: ["invoice extraction evidence requires human resolution before VAT preflight"] };
  const now = (options.clock ?? systemVatValidationClock).now().toISOString();
  const facts = classification(supplier(db, documentId));
  const cached = freshEvidence(db, documentId, now, facts.country, facts.identifier);
  if (facts.classification === "DK") return { ok: true, documentId, classification: facts.classification, cached: !!cached, evidenceExpiresAt: cached?.evidence_expires_at ?? null, wouldCallProvider: false, errors: [] };
  if (facts.classification === "NON_EU") {
    const errors = nonEuReverseChargeEvidenceErrors(db, documentId);
    return { ok: errors.length === 0, documentId, classification: facts.classification, cached: !!cached, evidenceExpiresAt: cached?.evidence_expires_at ?? null, wouldCallProvider: false, errors };
  }
  if (facts.classification === "CONFLICT") return { ok: false, documentId, classification: "CONFLICT", cached: false, evidenceExpiresAt: null, wouldCallProvider: false, errors: ["documented supplier evidence is incomplete or conflicting; human resolution is required"] };
  return { ok: !!cached, documentId, classification: "EU", cached: !!cached, evidenceExpiresAt: cached?.evidence_expires_at ?? null, wouldCallProvider: !cached, errors: cached ? [] : ["fresh EU VAT validation evidence is required"] };
}

/**
 * Persist the local, provider-free preflight used by an atomic purchase
 * posting. EU evidence is never manufactured here: it must already be fresh.
 */
/** Applies locally stored VAT evidence inside a caller-owned transaction. */
export function applyStoredPurchaseVatPreflightInCurrentTransaction(db: Database, documentId: number, options: { clock?: VatValidationClock; actor?: string } = {}) {
  const clock = options.clock ?? systemVatValidationClock;
  const inspection = inspectPurchaseVatPreflight(db, documentId, { clock });
  const facts = classification(supplier(db, documentId));
  const now = clock.now().toISOString();
  const existing = freshEvidence(db, documentId, now, facts.country, facts.identifier);
  if (existing) return { ...inspection, vatPreflightId: existing.id, exceptionId: undefined };
  if (!inspection.ok) {
    const exception = recordException(db, { type: "PURCHASE_VAT_PREFLIGHT", severity: "high", relatedDocumentId: documentId, message: inspection.errors[0]!, requiredAction: "Repair the VAT evidence, then resume this batch", resolutionKey: `purchase-vat-preflight:${documentId}` });
    event(db, { documentId, eventType: "preflight_failed", classification: facts.classification, providerStatus: "blocked", country: facts.country, identifier: facts.identifier, actor: options.actor, detail: inspection.errors, at: now });
    return { ...inspection, vatPreflightId: undefined, exceptionId: exception.exceptionId };
  }
  event(db, { documentId, eventType: "preflight_passed", classification: facts.classification, providerStatus: "local", country: facts.country, identifier: facts.identifier, actor: options.actor, expiresAt: null, at: now });
  const row = db.query("SELECT id FROM vat_validation_events WHERE document_id=? AND event_type='preflight_passed' ORDER BY id DESC LIMIT 1").get(documentId) as { id: number };
  return { ...inspection, vatPreflightId: row.id, exceptionId: undefined };
}
/** Standalone compatibility wrapper. */
export function applyStoredPurchaseVatPreflight(db: Database, documentId: number, options: { clock?: VatValidationClock; actor?: string } = {}) {
  return db.transaction(() => applyStoredPurchaseVatPreflightInCurrentTransaction(db, documentId, options)).immediate();
}
export async function ensurePurchaseVatPreflight(db: Database, documentId: number, provider: VatValidationProvider, options: { clock?: VatValidationClock; actor?: string } = {}) {
  const clock = options.clock ?? systemVatValidationClock;
  const inspection = inspectPurchaseVatPreflight(db, documentId, { clock });
  const facts = classification(supplier(db, documentId));
  if (inspection.ok) return { ...inspection, reusedEvidence: inspection.cached, exceptionId: undefined };
  // The inspection also detects extraction conflicts.  Do this before any
  // provider I/O, even when the persisted supplier identity itself looks EU.
  if (inspection.classification === "CONFLICT" || facts.classification === "CONFLICT") {
    const exception = recordException(db, { type: "PURCHASE_VAT_PREFLIGHT", severity: "high", relatedDocumentId: documentId, message: inspection.errors[0]!, requiredAction: "Resolve supplier country and typed identifier, then retry VAT preflight", resolutionKey: `purchase-vat-preflight:${documentId}` });
    event(db, { documentId, eventType: "preflight_failed", classification: facts.classification, providerStatus: "conflict", actor: options.actor, detail: inspection.errors, at: clock.now().toISOString() });
    return { ...inspection, reusedEvidence: false, exceptionId: exception.exceptionId };
  }
  const parsed = normalizeEuVatNumber(facts.identifier);
  if (!parsed) throw new Error("resolved EU supplier identity must have a normalizable VAT identifier");
  event(db, { documentId, eventType: "provider_requested", classification: "EU", providerStatus: "requested", country: facts.country, identifier: facts.identifier, actor: options.actor, at: clock.now().toISOString() });
  let result: VatValidationProviderResult;
  try { result = await provider.validate(parsed); } catch (cause) { result = { status: "unavailable", rawResponse: cause instanceof Error ? cause.message : "provider failure" }; }
  const now = clock.now().toISOString(); const expiresAt = new Date(clock.now().getTime() + PURCHASE_VAT_EVIDENCE_MAX_AGE_DAYS * 86400000).toISOString();
  event(db, { documentId, eventType: "provider_result", classification: "EU", providerStatus: result.status, country: facts.country, identifier: facts.identifier, name: result.name, address: result.address, actor: options.actor, expiresAt: result.status === "valid" ? expiresAt : null, detail: result.rawResponse, at: now });
  if (result.status === "valid") {
    storeViesValidation(db, { vatOrCvr: facts.identifier, valid: true, name: result.name, address: result.address, validatedAt: now, expiresAt, rawResponse: result.rawResponse });
    event(db, { documentId, eventType: "preflight_passed", classification: "EU", providerStatus: "valid", country: facts.country, identifier: facts.identifier, name: result.name, address: result.address, actor: options.actor, expiresAt, at: now });
    return { ...inspectPurchaseVatPreflight(db, documentId, { clock }), reusedEvidence: false, exceptionId: undefined };
  }
  const message = `EU VAT provider returned ${result.status}; purchase posting is blocked until VAT preflight succeeds`;
  const exception = recordException(db, { type: "PURCHASE_VAT_PREFLIGHT", severity: "high", relatedDocumentId: documentId, message, requiredAction: "Retry VAT preflight when authoritative supplier evidence is available", resolutionKey: `purchase-vat-preflight:${documentId}`, sourceEvidence: { providerStatus: result.status } });
  event(db, { documentId, eventType: "preflight_failed", classification: "EU", providerStatus: result.status, country: facts.country, identifier: facts.identifier, actor: options.actor, detail: result.rawResponse, at: now });
  return { ok: false, documentId, classification: "EU" as const, cached: false, evidenceExpiresAt: null, wouldCallProvider: true, errors: [message], reusedEvidence: false, exceptionId: exception.exceptionId };
}
