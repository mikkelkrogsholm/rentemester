/**
 * Reviewed bridge from one legacy company contact to one canonical party.
 *
 * The bridge is control-plane evidence only. It never updates the contact,
 * document, journal or VAT state. A mapping is authorised by an exact source
 * document snapshot and an explicit review reference; names alone are never
 * sufficient.
 */
import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { canonicalJson } from "./canonical-json";
import { inspectParty } from "./party-registry";
import { resolveSupplierIdentity } from "./supplier-identity";

export const LEGACY_PARTY_MAPPING_ERRORS = {
  CONTACT_NOT_FOUND: "CONTACT_NOT_FOUND",
  CONTACT_KIND_INVALID: "CONTACT_KIND_INVALID",
  PARTY_NOT_FOUND: "PARTY_NOT_FOUND",
  ROLE_MISMATCH: "ROLE_MISMATCH",
  EVIDENCE_REQUIRED: "EVIDENCE_REQUIRED",
  EVIDENCE_NOT_FOUND: "EVIDENCE_NOT_FOUND",
  CONTACT_IDENTITY_MISMATCH: "CONTACT_IDENTITY_MISMATCH",
  PLAN_HASH_MISMATCH: "PLAN_HASH_MISMATCH",
  CONFIRMATION_REQUIRED: "CONFIRMATION_REQUIRED",
  ACTOR_REQUIRED: "ACTOR_REQUIRED",
  PRINCIPAL_REQUIRED: "PRINCIPAL_REQUIRED",
  IDEMPOTENCY_KEY_REQUIRED: "IDEMPOTENCY_KEY_REQUIRED",
  IDEMPOTENCY_CONFLICT: "IDEMPOTENCY_CONFLICT",
  CURRENT_STATE_CONFLICT: "CURRENT_STATE_CONFLICT",
  MAPPING_NOT_FOUND: "MAPPING_NOT_FOUND",
  SUPERSEDE_REASON_REQUIRED: "SUPERSEDE_REASON_REQUIRED",
} as const;

type ErrorCode = typeof LEGACY_PARTY_MAPPING_ERRORS[keyof typeof LEGACY_PARTY_MAPPING_ERRORS];
export type LegacyKind = "customer" | "vendor";
export type LegacyRole = "customer" | "vendor";
export type LegacyPartyMappingInput = {
  companySlug: string;
  legacyKind: LegacyKind;
  legacyId: string;
  partyId: string;
  role: LegacyRole;
  documentId: number;
  reviewedLegacyReference: string;
};

type EventRow = {
  id: number;
  event_type: "mapped" | "superseded";
  version: number;
  event_hash: string;
  plan_hash: string;
  party_id: string;
  party_role: LegacyRole;
  contact_snapshot: string;
  contact_fingerprint: string;
  evidence_json: string;
  idempotency_payload_hash: string;
};

const fail = (error: ErrorCode) => ({ ok: false as const, errors: [error] as ErrorCode[] });
const bounded = (value: unknown, max: number) => typeof value === "string" && value.trim().length > 0 && value.trim().length <= max ? value.trim() : null;
const sha256 = (value: unknown) => createHash("sha256").update(canonicalJson(value)).digest("hex");
const keyHash = (value: string) => createHash("sha256").update(value).digest("hex");
const normalText = (value: unknown) => typeof value === "string" ? value.trim().replace(/\s+/g, " ").toLocaleLowerCase("en-US") : "";
const normalIdentifier = (value: unknown) => typeof value === "string" ? value.replace(/[^a-z0-9]/gi, "").toUpperCase() : "";
const canonicalLegacyId = (value: unknown) => {
  const raw = bounded(value, 160);
  if (!raw) return null;
  const id = Number(raw);
  return Number.isSafeInteger(id) && id > 0 && raw === String(id) ? raw : null;
};

function legacyContact(ledger: Database, kind: LegacyKind, legacyId: string) {
  const id = Number(legacyId);
  if (!Number.isSafeInteger(id) || id <= 0) return null;
  if (kind === "vendor") {
    const row = ledger.query(`SELECT id,name,address,vat_or_cvr,country_code,identifier_kind,identity_status,email,phone,website,
      default_expense_account,default_vat_treatment,notes,archived,created_at FROM vendors WHERE id=?`).get(id) as Record<string, unknown> | null;
    return row ? { kind, ...row } : null;
  }
  const row = ledger.query(`SELECT id,name,address,vat_or_cvr,email,phone,website,ean_number,payment_terms_days,
    default_currency,notes,archived,created_at FROM customers WHERE id=?`).get(id) as Record<string, unknown> | null;
  return row ? { kind, ...row } : null;
}

function sourceDocument(ledger: Database, documentId: number) {
  if (!Number.isSafeInteger(documentId) || documentId <= 0) return null;
  return ledger.query(`SELECT id,sha256_hash,payload_json,document_type,sender_name,sender_address,sender_vat_cvr,
    supplier_country_code,supplier_identifier_kind,supplier_identity_status,recipient_name,recipient_address,recipient_vat_cvr
    FROM documents WHERE id=?`).get(documentId) as Record<string, unknown> | null;
}

function partySnapshot(control: Database, partyId: string, companySlug: string, role: LegacyRole) {
  const party = inspectParty(control, partyId);
  if (!party) return null;
  const matchingRoles = party.roles.filter((candidate: any) => candidate.companySlug === companySlug && candidate.role === role);
  if (matchingRoles.length !== 1) return { mismatch: true as const };
  return {
    partyId: party.partyId,
    kind: party.kind,
    name: party.name,
    identifiers: party.identifiers ?? [],
    roles: matchingRoles,
    history: party.history,
  };
}

function exactIdentifierMatches(contact: Record<string, unknown>, document: Record<string, unknown>, party: any): boolean {
  const raw = bounded(contact.vat_or_cvr, 160);
  if (!raw) return false;
  if (contact.kind === "vendor") {
    const resolved = resolveSupplierIdentity({
      country: String(contact.country_code ?? ""),
      identifierKind: contact.identifier_kind as any,
      identifier: raw,
    });
    if (!resolved.ok || !resolved.identifier) return false;
    if (document.supplier_country_code !== resolved.country ||
      document.supplier_identifier_kind !== resolved.identifierKind ||
      normalIdentifier(document.sender_vat_cvr) !== normalIdentifier(resolved.identifier)) return false;
    return party.identifiers.some((identifier: any) => identifier.country === resolved.country &&
      identifier.identifierKind === resolved.identifierKind && normalIdentifier(identifier.identifier) === normalIdentifier(resolved.identifier));
  }
  if (normalIdentifier(document.recipient_vat_cvr) !== normalIdentifier(raw)) return false;
  return party.identifiers.some((identifier: any) => normalIdentifier(identifier.identifier) === normalIdentifier(raw));
}

function reviewedIdentifierlessVendorMatches(contact: Record<string, unknown>, document: Record<string, unknown>, party: any): boolean {
  if (contact.kind !== "vendor" || bounded(contact.vat_or_cvr, 160) !== null) return false;
  if (contact.identifier_kind !== "non_eu" || contact.identity_status !== "resolved" || !bounded(contact.country_code, 2)) return false;
  if (document.sender_vat_cvr !== null || document.supplier_identifier_kind !== "non_eu" || document.supplier_identity_status !== "resolved") return false;
  if (document.supplier_country_code !== contact.country_code || party.identifiers.length !== 0) return false;
  const contactName = normalText(contact.name), documentName = normalText(document.sender_name);
  const contactAddress = normalText(contact.address), documentAddress = normalText(document.sender_address);
  return contactName.length > 0 && contactAddress.length > 0 &&
    contactName === documentName && contactAddress === documentAddress && normalText(party.name) === contactName;
}

function latestEvent(control: Database, companySlug: string, legacyKind: LegacyKind, legacyId: string) {
  return control.query(`SELECT id,event_type,version,event_hash,plan_hash,party_id,party_role,contact_snapshot,
    contact_fingerprint,evidence_json,idempotency_payload_hash FROM rm_legacy_party_mapping_events
    WHERE company_slug=? AND legacy_kind=? AND legacy_id=? ORDER BY version DESC LIMIT 1`).get(companySlug, legacyKind, legacyId) as EventRow | null;
}

/** Deterministic and read-only: it snapshots both mutable source rows. */
export function planLegacyPartyMapping(ledger: Database, control: Database, input: LegacyPartyMappingInput) {
  const companySlug = bounded(input.companySlug, 120), legacyId = canonicalLegacyId(input.legacyId), partyId = bounded(input.partyId, 64);
  const reviewedLegacyReference = bounded(input.reviewedLegacyReference, 500);
  if (!reviewedLegacyReference) return fail(LEGACY_PARTY_MAPPING_ERRORS.EVIDENCE_REQUIRED);
  if (!companySlug || !legacyId || !partyId ||
    !(["customer", "vendor"] as const).includes(input.legacyKind)) return fail(LEGACY_PARTY_MAPPING_ERRORS.CONTACT_KIND_INVALID);
  if (input.role !== input.legacyKind) return fail(LEGACY_PARTY_MAPPING_ERRORS.ROLE_MISMATCH);
  const contact = legacyContact(ledger, input.legacyKind, legacyId);
  if (!contact) return fail(LEGACY_PARTY_MAPPING_ERRORS.CONTACT_NOT_FOUND);
  const document = sourceDocument(ledger, input.documentId);
  if (!document) return fail(LEGACY_PARTY_MAPPING_ERRORS.EVIDENCE_NOT_FOUND);
  const party = partySnapshot(control, partyId, companySlug, input.role);
  if (!party) return fail(LEGACY_PARTY_MAPPING_ERRORS.PARTY_NOT_FOUND);
  if ("mismatch" in party) return fail(LEGACY_PARTY_MAPPING_ERRORS.ROLE_MISMATCH);
  if (!exactIdentifierMatches(contact, document, party) && !reviewedIdentifierlessVendorMatches(contact, document, party)) {
    return fail(LEGACY_PARTY_MAPPING_ERRORS.CONTACT_IDENTITY_MISMATCH);
  }
  const prior = latestEvent(control, companySlug, input.legacyKind, legacyId);
  if (prior?.event_type === "mapped" && prior.party_id !== partyId) return fail(LEGACY_PARTY_MAPPING_ERRORS.CURRENT_STATE_CONFLICT);
  const old = control.query("SELECT party_id FROM rm_party_legacy_links WHERE company_slug=? AND legacy_kind=? AND legacy_id=?")
    .get(companySlug, input.legacyKind, legacyId) as { party_id: string } | null;
  if (!prior && old && old.party_id !== partyId) return fail(LEGACY_PARTY_MAPPING_ERRORS.CURRENT_STATE_CONFLICT);
  let documentPayload: unknown;
  try { documentPayload = JSON.parse(String(document.payload_json ?? "{}")); }
  catch { return fail(LEGACY_PARTY_MAPPING_ERRORS.EVIDENCE_NOT_FOUND); }
  const payload = {
    companySlug,
    legacyKind: input.legacyKind,
    legacyId,
    partyId,
    role: input.role,
    contactSnapshot: contact,
    contactFingerprint: sha256(contact),
    partySnapshot: party,
    evidence: {
      kind: "reviewed_source_document",
      documentId: Number(document.id),
      documentSha256: document.sha256_hash,
      documentPayloadSha256: sha256(documentPayload),
      identitySnapshot: {
        documentType: document.document_type,
        senderName: document.sender_name,
        senderAddress: document.sender_address,
        senderVatOrCvr: document.sender_vat_cvr,
        supplierCountryCode: document.supplier_country_code,
        supplierIdentifierKind: document.supplier_identifier_kind,
        supplierIdentityStatus: document.supplier_identity_status,
        recipientName: document.recipient_name,
        recipientAddress: document.recipient_address,
        recipientVatOrCvr: document.recipient_vat_cvr,
      },
      reviewedLegacyReference,
    },
  };
  return { ok: true as const, plan: { ...payload, planHash: sha256(payload) } };
}

function idempotentEvent(control: Database, input: { companySlug: string; principal: string; eventType: string; idempotencyKey: string }) {
  return control.query(`SELECT id,event_type,version,event_hash,plan_hash,party_id,party_role,contact_snapshot,
    contact_fingerprint,evidence_json,idempotency_payload_hash FROM rm_legacy_party_mapping_events
    WHERE company_slug=? AND principal=? AND event_type=? AND idempotency_key_hash=?`).get(
      input.companySlug, input.principal, input.eventType, keyHash(input.idempotencyKey),
    ) as EventRow | null;
}

export function applyLegacyPartyMapping(ledger: Database, control: Database, input: LegacyPartyMappingInput & {
  planHash: string; confirm: boolean; actor?: string; principal?: string; idempotencyKey?: string;
}) {
  if (!input.confirm) return fail(LEGACY_PARTY_MAPPING_ERRORS.CONFIRMATION_REQUIRED);
  const actor = bounded(input.actor, 160), principal = bounded(input.principal, 160), idempotencyKey = bounded(input.idempotencyKey, 128);
  if (!actor) return fail(LEGACY_PARTY_MAPPING_ERRORS.ACTOR_REQUIRED);
  if (!principal) return fail(LEGACY_PARTY_MAPPING_ERRORS.PRINCIPAL_REQUIRED);
  if (!idempotencyKey) return fail(LEGACY_PARTY_MAPPING_ERRORS.IDEMPOTENCY_KEY_REQUIRED);
  const companySlug = bounded(input.companySlug, 120), legacyId = canonicalLegacyId(input.legacyId);
  if (!companySlug || !legacyId) return fail(LEGACY_PARTY_MAPPING_ERRORS.CONTACT_KIND_INVALID);
  const requestHash = sha256({ operation: "mapped", companySlug, legacyKind: input.legacyKind, legacyId,
    partyId: input.partyId, role: input.role, documentId: input.documentId, reviewedLegacyReference: input.reviewedLegacyReference, planHash: input.planHash });
  return control.transaction(() => {
    const replay = idempotentEvent(control, { companySlug, principal, eventType: "mapped", idempotencyKey });
    if (replay) {
      if (replay.idempotency_payload_hash !== requestHash) return fail(LEGACY_PARTY_MAPPING_ERRORS.IDEMPOTENCY_CONFLICT);
      return { ok: true as const, id: replay.id, eventHash: replay.event_hash, planHash: replay.plan_hash, idempotent: true };
    }
    const planned = planLegacyPartyMapping(ledger, control, { ...input, companySlug, legacyId });
    if (!planned.ok) return planned;
    if (input.planHash !== planned.plan.planHash) return fail(LEGACY_PARTY_MAPPING_ERRORS.PLAN_HASH_MISMATCH);
    const prior = latestEvent(control, planned.plan.companySlug, planned.plan.legacyKind, planned.plan.legacyId);
    if (prior?.event_type === "mapped") {
      return fail(LEGACY_PARTY_MAPPING_ERRORS.CURRENT_STATE_CONFLICT);
    }
    const old = control.query("SELECT party_id FROM rm_party_legacy_links WHERE company_slug=? AND legacy_kind=? AND legacy_id=?")
      .get(planned.plan.companySlug, planned.plan.legacyKind, planned.plan.legacyId) as { party_id: string } | null;
    if (old && old.party_id !== planned.plan.partyId) return fail(LEGACY_PARTY_MAPPING_ERRORS.CURRENT_STATE_CONFLICT);
    const version = (prior?.version ?? 0) + 1;
    const createdAt = new Date().toISOString();
    const eventHash = sha256({ eventType: "mapped", version, priorEventHash: prior?.event_hash ?? null, planHash: input.planHash,
      actor, principal, createdAt });
    const row = control.query(`INSERT INTO rm_legacy_party_mapping_events(company_slug,legacy_kind,legacy_id,party_id,party_role,
      event_type,version,prior_event_hash,event_hash,contact_snapshot,contact_fingerprint,evidence_json,plan_hash,
      idempotency_key_hash,idempotency_payload_hash,reason,actor,principal,created_at)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id`).get(
        planned.plan.companySlug, planned.plan.legacyKind, planned.plan.legacyId, planned.plan.partyId, planned.plan.role,
        "mapped", version, prior?.event_hash ?? null, eventHash, canonicalJson(planned.plan.contactSnapshot), planned.plan.contactFingerprint,
        canonicalJson(planned.plan.evidence), input.planHash, keyHash(idempotencyKey), requestHash, null, actor, principal, createdAt,
      ) as { id: number };
    return { ok: true as const, id: row.id, eventHash, planHash: input.planHash, idempotent: false };
  }).immediate();
}

export function supersedeLegacyPartyMapping(control: Database, input: {
  companySlug: string; legacyKind: LegacyKind; legacyId: string; planHash: string; reason: string;
  confirm: boolean; actor?: string; principal?: string; idempotencyKey?: string;
}) {
  if (!input.confirm) return fail(LEGACY_PARTY_MAPPING_ERRORS.CONFIRMATION_REQUIRED);
  const actor = bounded(input.actor, 160), principal = bounded(input.principal, 160), idempotencyKey = bounded(input.idempotencyKey, 128);
  const companySlug = bounded(input.companySlug, 120), legacyId = canonicalLegacyId(input.legacyId), reason = bounded(input.reason, 1000);
  if (!actor) return fail(LEGACY_PARTY_MAPPING_ERRORS.ACTOR_REQUIRED);
  if (!principal) return fail(LEGACY_PARTY_MAPPING_ERRORS.PRINCIPAL_REQUIRED);
  if (!idempotencyKey) return fail(LEGACY_PARTY_MAPPING_ERRORS.IDEMPOTENCY_KEY_REQUIRED);
  if (!reason) return fail(LEGACY_PARTY_MAPPING_ERRORS.SUPERSEDE_REASON_REQUIRED);
  if (!companySlug || !legacyId) return fail(LEGACY_PARTY_MAPPING_ERRORS.MAPPING_NOT_FOUND);
  const requestHash = sha256({ operation: "superseded", companySlug, legacyKind: input.legacyKind, legacyId, planHash: input.planHash, reason });
  return control.transaction(() => {
    const replay = idempotentEvent(control, { companySlug, principal, eventType: "superseded", idempotencyKey });
    if (replay) {
      if (replay.idempotency_payload_hash !== requestHash) return fail(LEGACY_PARTY_MAPPING_ERRORS.IDEMPOTENCY_CONFLICT);
      return { ok: true as const, id: replay.id, eventHash: replay.event_hash, planHash: replay.plan_hash, idempotent: true };
    }
    const current = latestEvent(control, companySlug, input.legacyKind, legacyId);
    if (!current || current.event_type !== "mapped" || current.plan_hash !== input.planHash) return fail(LEGACY_PARTY_MAPPING_ERRORS.MAPPING_NOT_FOUND);
    const version = current.version + 1;
    const createdAt = new Date().toISOString();
    const eventHash = sha256({ eventType: "superseded", version, priorEventHash: current.event_hash, planHash: current.plan_hash,
      reason, actor, principal, createdAt });
    const row = control.query(`INSERT INTO rm_legacy_party_mapping_events(company_slug,legacy_kind,legacy_id,party_id,party_role,
      event_type,version,prior_event_hash,event_hash,contact_snapshot,contact_fingerprint,evidence_json,plan_hash,
      idempotency_key_hash,idempotency_payload_hash,reason,actor,principal,created_at)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?) RETURNING id`).get(
        companySlug, input.legacyKind, legacyId, current.party_id, current.party_role, "superseded", version, current.event_hash,
        eventHash, current.contact_snapshot, current.contact_fingerprint, current.evidence_json, current.plan_hash,
        keyHash(idempotencyKey), requestHash, reason, actor, principal, createdAt,
      ) as { id: number };
    return { ok: true as const, id: row.id, eventHash, planHash: current.plan_hash, idempotent: false };
  }).immediate();
}

export function inspectLegacyPartyMappings(control: Database, input: { companySlug: string; legacyKind?: LegacyKind; legacyId?: string }) {
  const rows = control.query(`SELECT id,company_slug,legacy_kind,legacy_id,party_id,party_role,event_type,version,prior_event_hash,
    event_hash,contact_snapshot,contact_fingerprint,evidence_json,plan_hash,reason,actor,principal,created_at
    FROM rm_legacy_party_mapping_events WHERE company_slug=? AND (? IS NULL OR legacy_kind=?) AND (? IS NULL OR legacy_id=?) ORDER BY id`)
    .all(input.companySlug, input.legacyKind ?? null, input.legacyKind ?? null, input.legacyId ?? null, input.legacyId ?? null) as any[];
  return rows.map((row) => ({
    id: row.id, companySlug: row.company_slug, legacyKind: row.legacy_kind, legacyId: row.legacy_id,
    partyId: row.party_id, role: row.party_role, eventType: row.event_type, version: row.version,
    priorEventHash: row.prior_event_hash, eventHash: row.event_hash, contactFingerprint: row.contact_fingerprint,
    contactSnapshot: JSON.parse(row.contact_snapshot), evidence: JSON.parse(row.evidence_json), planHash: row.plan_hash,
    reason: row.reason, actor: row.actor, principal: row.principal, createdAt: row.created_at,
  }));
}
