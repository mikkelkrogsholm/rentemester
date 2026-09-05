/** Reviewed, source-byte-bound completion of legacy vendor identity typing. */
import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { insertAuditLog } from "./actor";
import { canonicalJson } from "./canonical-json";
import { DocumentEvidenceError, snapshotRegisteredDocument } from "./document-storage";
import { resolveSupplierIdentity, type SupplierIdentifierKind } from "./supplier-identity";

export const VENDOR_IDENTITY_ENRICHMENT_ERRORS = {
  VENDOR_NOT_FOUND: "VENDOR_NOT_FOUND",
  EVIDENCE_NOT_FOUND: "EVIDENCE_NOT_FOUND",
  EVIDENCE_INVALID: "EVIDENCE_INVALID",
  EVIDENCE_MISMATCH: "EVIDENCE_MISMATCH",
  IDENTITY_INVALID: "IDENTITY_INVALID",
  IDENTITY_CONFLICT: "IDENTITY_CONFLICT",
  IDENTIFIER_INVENTION: "IDENTIFIER_INVENTION",
  NAME_OR_ADDRESS_MISMATCH: "NAME_OR_ADDRESS_MISMATCH",
  PLAN_HASH_MISMATCH: "PLAN_HASH_MISMATCH",
  CONFIRMATION_REQUIRED: "CONFIRMATION_REQUIRED",
  ACTOR_REQUIRED: "ACTOR_REQUIRED",
  PRINCIPAL_REQUIRED: "PRINCIPAL_REQUIRED",
  IDEMPOTENCY_KEY_REQUIRED: "IDEMPOTENCY_KEY_REQUIRED",
  IDEMPOTENCY_CONFLICT: "IDEMPOTENCY_CONFLICT",
} as const;

type ErrorCode = (typeof VENDOR_IDENTITY_ENRICHMENT_ERRORS)[keyof typeof VENDOR_IDENTITY_ENRICHMENT_ERRORS];
export type VendorIdentityEnrichmentInput = {
  companySlug: string;
  vendorId: number;
  documentId: number;
  countryCode: string;
  identifierKind: SupplierIdentifierKind;
  identifier?: string | null;
  reviewedReference: string;
};
type VendorSnapshot = {
  id: number; name: string; address: string | null; vat_or_cvr: string | null;
  country_code: string | null; identifier_kind: string | null; identity_status: string;
  notes: string | null;
};
type DocumentSnapshot = {
  id: number; sha256_hash: string; document_type: string; sender_name: string | null;
  sender_address: string | null; sender_vat_cvr: string | null;
  supplier_country_code: string | null; supplier_identifier_kind: string | null;
  supplier_identity_status: string | null;
};

const fail = (error: ErrorCode) => ({ ok: false as const, errors: [error] as ErrorCode[] });
const bounded = (value: unknown, max = 500) =>
  typeof value === "string" && value.trim().length > 0 && value.trim().length <= max ? value.trim() : null;
const normalizeText = (value: unknown) =>
  typeof value === "string" ? value.trim().replace(/\s+/g, " ").toLocaleLowerCase("en-US") : "";
const normalizeIdentifier = (value: unknown) =>
  typeof value === "string" ? value.replace(/[^a-z0-9]/gi, "").toUpperCase() : "";
const hashCanonical = (value: unknown) => createHash("sha256").update(canonicalJson(value)).digest("hex");
const hashKey = (value: string) => createHash("sha256").update(value).digest("hex");

function vendorSnapshot(db: Database, id: number): VendorSnapshot | null {
  return db.query(`SELECT id,name,address,vat_or_cvr,country_code,identifier_kind,
    identity_status,notes FROM vendors WHERE id=?`).get(id) as VendorSnapshot | null;
}
function documentSnapshot(db: Database, id: number): DocumentSnapshot | null {
  return db.query(`SELECT id,sha256_hash,document_type,sender_name,sender_address,
    sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status
    FROM documents WHERE id=?`).get(id) as DocumentSnapshot | null;
}

function planInner(db: Database, companyRoot: string, input: VendorIdentityEnrichmentInput) {
  if (!Number.isSafeInteger(input.vendorId) || input.vendorId <= 0) return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.VENDOR_NOT_FOUND);
  const reference = bounded(input.reviewedReference);
  if (!reference) return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.EVIDENCE_NOT_FOUND);
  const vendor = vendorSnapshot(db, input.vendorId);
  if (!vendor) return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.VENDOR_NOT_FOUND);
  const document = documentSnapshot(db, input.documentId);
  if (!document) return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.EVIDENCE_NOT_FOUND);

  // Only complete imported unresolved typing. Never rewrite typed or resolved identity.
  if (vendor.country_code !== null || vendor.identifier_kind !== null || vendor.identity_status === "resolved") {
    return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.IDENTITY_CONFLICT);
  }
  const suppliedIdentifier = bounded(input.identifier, 160);
  const existingIdentifier = bounded(vendor.vat_or_cvr, 160);
  if (!existingIdentifier && suppliedIdentifier) return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.IDENTIFIER_INVENTION);
  if (existingIdentifier && suppliedIdentifier && normalizeIdentifier(existingIdentifier) !== normalizeIdentifier(suppliedIdentifier)) {
    return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.IDENTITY_CONFLICT);
  }
  const resolved = resolveSupplierIdentity({
    country: input.countryCode,
    identifierKind: input.identifierKind,
    identifier: existingIdentifier ?? undefined,
  });
  if (!resolved.ok) return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.IDENTITY_INVALID);

  if (!normalizeText(vendor.name) || !normalizeText(vendor.address) ||
    normalizeText(vendor.name) !== normalizeText(document.sender_name) ||
    normalizeText(vendor.address) !== normalizeText(document.sender_address)) {
    return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.NAME_OR_ADDRESS_MISMATCH);
  }
  if (document.supplier_identity_status !== "resolved" ||
    document.supplier_country_code !== resolved.country ||
    document.supplier_identifier_kind !== resolved.identifierKind ||
    normalizeIdentifier(document.sender_vat_cvr) !== normalizeIdentifier(resolved.identifier)) {
    return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.EVIDENCE_MISMATCH);
  }

  let source: ReturnType<typeof snapshotRegisteredDocument>;
  try {
    source = snapshotRegisteredDocument(db, companyRoot, input.documentId);
  } catch (error) {
    return fail(error instanceof DocumentEvidenceError
      ? VENDOR_IDENTITY_ENRICHMENT_ERRORS.EVIDENCE_INVALID
      : VENDOR_IDENTITY_ENRICHMENT_ERRORS.EVIDENCE_NOT_FOUND);
  }
  const payload = {
    companySlug: input.companySlug,
    vendorSnapshot: vendor,
    vendorFingerprint: hashCanonical(vendor),
    documentSnapshot: {
      id: document.id,
      sha256: document.sha256_hash,
      documentType: document.document_type,
      supplierCountryCode: document.supplier_country_code,
      supplierIdentifierKind: document.supplier_identifier_kind,
      supplierIdentityStatus: document.supplier_identity_status,
      senderName: document.sender_name,
      senderAddress: document.sender_address,
      senderVatOrCvr: document.sender_vat_cvr,
    },
    documentBytesSha256: source.sha256,
    proposedIdentity: {
      countryCode: resolved.country,
      identifierKind: resolved.identifierKind,
      identifier: resolved.identifier ?? null,
    },
    reviewedReference: reference,
  };
  return { ok: true as const, plan: { ...payload, planHash: hashCanonical(payload) } };
}

export function planVendorIdentityEnrichment(db: Database, companyRoot: string, input: VendorIdentityEnrichmentInput) {
  return planInner(db, companyRoot, input);
}

export function listVendorIdentityEnrichments(db: Database, input: { vendorId?: number }) {
  return db.query(`SELECT id,vendor_id,document_id,plan_hash,event_hash,actor,principal,created_at
    FROM vendor_identity_enrichment_events WHERE (? IS NULL OR vendor_id=?) ORDER BY id`)
    .all(input.vendorId ?? null, input.vendorId ?? null);
}

export function applyVendorIdentityEnrichment(db: Database, companyRoot: string,
  input: VendorIdentityEnrichmentInput & {
    planHash: string; idempotencyKey?: string; confirm: boolean; actor?: string; principal?: string;
  }) {
  if (!input.confirm) return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.CONFIRMATION_REQUIRED);
  const actor = bounded(input.actor, 160);
  const principal = bounded(input.principal, 160);
  const idempotencyKey = bounded(input.idempotencyKey, 128);
  if (!actor) return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.ACTOR_REQUIRED);
  if (!principal) return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.PRINCIPAL_REQUIRED);
  if (!idempotencyKey) return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.IDEMPOTENCY_KEY_REQUIRED);
  const requestHash = hashCanonical({
    companySlug: input.companySlug, vendorId: input.vendorId, documentId: input.documentId,
    countryCode: input.countryCode, identifierKind: input.identifierKind,
    identifier: input.identifier ?? null, reviewedReference: input.reviewedReference,
    planHash: input.planHash,
  });

  return db.transaction(() => {
    const existing = db.query(`SELECT id,plan_hash,request_hash,event_hash FROM vendor_identity_enrichment_events
      WHERE principal=? AND idempotency_key_hash=?`).get(principal, hashKey(idempotencyKey)) as
      { id: number; plan_hash: string; request_hash: string; event_hash: string } | null;
    if (existing) {
      if (existing.request_hash !== requestHash) return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.IDEMPOTENCY_CONFLICT);
      return { ok: true as const, id: existing.id, eventHash: existing.event_hash, planHash: existing.plan_hash, idempotent: true };
    }
    const planned = planInner(db, companyRoot, input);
    if (!planned.ok) return planned;
    if (input.planHash !== planned.plan.planHash) return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.PLAN_HASH_MISMATCH);
    const identity = planned.plan.proposedIdentity;
    const changed = db.query(`UPDATE vendors SET country_code=?,identifier_kind=?,identity_status='resolved'
      WHERE id=? AND country_code IS NULL AND identifier_kind IS NULL
      AND COALESCE(identity_status,'')!='resolved'`)
      .run(identity.countryCode, identity.identifierKind, input.vendorId).changes;
    if (changed !== 1) return fail(VENDOR_IDENTITY_ENRICHMENT_ERRORS.IDENTITY_CONFLICT);

    const createdAt = new Date().toISOString();
    const eventHash = hashCanonical({ planHash: input.planHash, requestHash, actor, principal, createdAt });
    const row = db.query(`INSERT INTO vendor_identity_enrichment_events
      (vendor_id,document_id,plan_hash,request_hash,idempotency_key_hash,actor,principal,payload_json,event_hash,created_at)
      VALUES(?,?,?,?,?,?,?,?,?,?) RETURNING id`).get(
      input.vendorId, input.documentId, input.planHash, requestHash, hashKey(idempotencyKey), actor,
      principal, canonicalJson(planned.plan), eventHash, createdAt,
    ) as { id: number };
    insertAuditLog(db, {
      eventType: "vendor_identity_enriched",
      entityType: "vendor",
      entityId: input.vendorId,
      message: `Completed reviewed vendor identity typing from document ${input.documentId}`,
      createdBy: actor,
      createdByProgram: "vendor-identity-enrichment",
    });
    return { ok: true as const, id: row.id, eventHash, planHash: input.planHash, idempotent: false };
  }).immediate();
}
