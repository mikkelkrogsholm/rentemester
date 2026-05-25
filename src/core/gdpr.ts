/**
 * GDPR tooling (#184) — data-subject export and retention-respecting erasure.
 *
 * Rentemester stores personal data about customers and vendors (names,
 * addresses, emails, CVR, and free-text on bank transactions). The Danish
 * business running it is a data controller and must answer data-subject
 * access and erasure requests — but the bookkeeping-law retention requirement
 * (records kept ~5 years) overrides erasure for data still under retention.
 *
 * This module provides two narrow, deterministic operations:
 *
 *  - `buildGdprSubjectExport` — gathers every piece of personal data
 *    Rentemester holds about one customer/vendor/person into one report,
 *    each record annotated with its bookkeeping-retention verdict.
 *
 *  - `eraseGdprSubject` — redacts personal data that is no longer under
 *    retention and clearly REFUSES to erase data still legally required to
 *    be kept.
 *
 * The append-only ledger (`journal_entries` / `journal_lines` / `audit_log`)
 * and its hash chain are NEVER modified. Master-data rows are themselves
 * append-only, so an erasure does not UPDATE/DELETE them either — it records
 * an append-only tombstone in `gdpr_erasures`, and the export layer overlays
 * those tombstones so redacted data never resurfaces. `verifyAuditChain`
 * therefore still passes after any erasure.
 */

import type { Database } from "bun:sqlite";
import { effectiveRetainUntil } from "./retention";
import { insertAuditLog } from "./actor";
import { currentUtcIsoDate } from "./sequences";

// The bookkeeping-retention rule that overrides the GDPR right to erasure is
// the canonical, YAML-declared rule. The two GDPR-process labels below are
// operation identifiers (not bookkeeping rule IDs), so they intentionally do
// not use the `DK-…-NNN` rule-bundle namespace.
const EXPORT_RULE_ID = "GDPR-SUBJECT-EXPORT";
const ERASURE_RULE_ID = "GDPR-RETENTION-BOUNDED-ERASURE";
const RETENTION_RULE_ID = "DK-BOOKKEEPING-RETENTION-001";

export type GdprSubjectKey = {
  /** CVR / VAT identifier of the data subject. */
  cvr?: string | null;
  /** Exact display name of the data subject. */
  name?: string | null;
  /** Evaluation date (defaults to the DB clock). */
  asOf?: string | null;
};

export type GdprPersonalData = {
  name: string | null;
  address: string | null;
  email: string | null;
  vatOrCvr: string | null;
};

export type GdprExportRecord = {
  source: "customers" | "vendors" | "documents" | "bank_transactions";
  sourceRowId: number;
  /** Human label, e.g. document_no or bank reference. */
  label: string | null;
  personalData: GdprPersonalData;
  /** Bookkeeping retention deadline, or null when none applies. */
  retainUntil: string | null;
  /** True when the record must still be kept for bookkeeping law. */
  underRetention: boolean;
  /** True when a prior erasure tombstone covers this record. */
  erased: boolean;
};

export type GdprSubjectExport = {
  ok: boolean;
  asOf: string;
  appliedRules: string[];
  subject: { cvr: string | null; name: string | null };
  records: GdprExportRecord[];
  errors: string[];
};

export type GdprErasureRefusal = {
  source: GdprExportRecord["source"];
  sourceRowId: number;
  label: string | null;
  retainUntil: string;
  reason: string;
};

export type GdprErasureRecord = {
  source: GdprExportRecord["source"];
  sourceRowId: number;
  label: string | null;
  redactedFields: string[];
};

export type GdprErasureResult = {
  ok: boolean;
  asOf: string;
  appliedRules: string[];
  subject: { cvr: string | null; name: string | null };
  erasedCount: number;
  refusedCount: number;
  alreadyErasedCount: number;
  erased: GdprErasureRecord[];
  refused: GdprErasureRefusal[];
  errors: string[];
};

type RawSourceRow = {
  source: GdprExportRecord["source"];
  sourceRowId: number;
  label: string | null;
  personalData: GdprPersonalData;
  retainUntil: string | null;
};

function trim(value: string | null | undefined): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function resolveSubject(key: GdprSubjectKey) {
  return { cvr: trim(key.cvr), name: trim(key.name) };
}

/**
 * Loads prior erasure tombstones keyed by `source:rowId`. Each value is the
 * set of field names that were redacted.
 */
function loadErasures(db: Database): Map<string, Set<string>> {
  const rows = db
    .query("SELECT source, source_row_id, redacted_fields FROM gdpr_erasures")
    .all() as Array<{ source: string; source_row_id: number; redacted_fields: string }>;
  const map = new Map<string, Set<string>>();
  for (const row of rows) {
    let fields: string[];
    try {
      fields = JSON.parse(row.redacted_fields) as string[];
    } catch {
      fields = [];
    }
    map.set(`${row.source}:${row.source_row_id}`, new Set(fields));
  }
  return map;
}

export type GdprDiscoveryRow = {
  source: GdprExportRecord["source"];
  sourceRowId: number;
  label: string | null;
  personalData: GdprPersonalData;
  retainUntil: string | null;
};

export type GdprDiscoveryResult = {
  ok: boolean;
  subject: { cvr: string | null; name: string | null };
  rows: GdprDiscoveryRow[];
  byTable: Record<GdprExportRecord["source"], number>;
  errors: string[];
};

/**
 * Subject-discovery på tværs af tabeller (#353). Wrapper omkring den interne
 * `collectSourceRows` så CLI'ens \`gdpr discover\` og cockpit-views kan kalde
 * den uden at gå gennem export-pipelinen. Read-only.
 */
export function findGdprSubject(
  db: Database,
  key: GdprSubjectKey,
): GdprDiscoveryResult {
  const subject = resolveSubject(key);
  if (!subject.cvr && !subject.name) {
    return {
      ok: false,
      subject,
      rows: [],
      byTable: {
        customers: 0,
        vendors: 0,
        documents: 0,
        bank_transactions: 0,
      },
      errors: ["a GDPR subject must be identified by cvr or name"],
    };
  }
  const rows = collectSourceRows(db, subject);
  const byTable: Record<GdprExportRecord["source"], number> = {
    customers: 0,
    vendors: 0,
    documents: 0,
    bank_transactions: 0,
  };
  for (const r of rows) byTable[r.source] += 1;
  // #355 — audit-log også discovery, så Datatilsynet kan se den fulde
  // GDPR-aktivitetshistorik (export + discover + erasure).
  const subjectKey = subject.cvr ?? subject.name!;
  insertAuditLog(db, {
    eventType: "gdpr_discover",
    entityType: "gdpr_subject",
    entityId: subjectKey,
    message: `GDPR discover for ${subjectKey}: ${rows.length} row(s) across ${Object.values(byTable).filter((n) => n > 0).length} table(s)`,
  });
  return { ok: true, subject, rows, byTable, errors: [] };
}

/**
 * Collects the raw source rows that mention the data subject across the
 * master-data and document-metadata layers (never the ledger itself).
 */
function collectSourceRows(db: Database, subject: { cvr: string | null; name: string | null }): RawSourceRow[] {
  const rows: RawSourceRow[] = [];
  if (!subject.cvr && !subject.name) return rows;

  // Master-data rows (customers/vendors) have no retention deadline of their
  // own, but they are needed to interpret the bookkeeping records that DO
  // mention the subject. So a master-data row inherits the LATEST retention
  // deadline among the subject's documents and bank transactions: while any
  // such record must be kept, the master-data record must be kept too. We
  // therefore collect documents and bank rows FIRST.
  const linkedRetentions: string[] = [];
  const bookkeepingRows: RawSourceRow[] = [];

  // Documents carry sender and recipient personal data plus a bookkeeping
  // retention deadline that overrides erasure while it is in the future.
  const documents = db
    .query(
      `SELECT id, document_no, sender_name, sender_address, sender_vat_cvr,
              recipient_name, recipient_address, recipient_vat_cvr,
              retain_until, COALESCE(invoice_date, substr(upload_datetime, 1, 10)) AS basis_date
       FROM documents
       WHERE (? IS NOT NULL AND (sender_vat_cvr = ? OR recipient_vat_cvr = ?))
          OR (? IS NOT NULL AND (sender_name = ? OR recipient_name = ?))
       ORDER BY id ASC`,
    )
    .all(
      subject.cvr,
      subject.cvr,
      subject.cvr,
      subject.name,
      subject.name,
      subject.name,
    ) as Array<{
    id: number;
    document_no: string | null;
    sender_name: string | null;
    sender_address: string | null;
    sender_vat_cvr: string | null;
    recipient_name: string | null;
    recipient_address: string | null;
    recipient_vat_cvr: string | null;
    retain_until: string | null;
    basis_date: string | null;
  }>;
  for (const d of documents) {
    // Pick whichever party (sender/recipient) matched the subject.
    const isSender =
      (subject.cvr && d.sender_vat_cvr === subject.cvr) ||
      (subject.name && d.sender_name === subject.name);
    const personalData: GdprPersonalData = isSender
      ? { name: d.sender_name, address: d.sender_address, email: null, vatOrCvr: d.sender_vat_cvr }
      : { name: d.recipient_name, address: d.recipient_address, email: null, vatOrCvr: d.recipient_vat_cvr };
    const retainUntil = effectiveRetainUntil(db, d.retain_until, d.basis_date);
    if (retainUntil) linkedRetentions.push(retainUntil);
    bookkeepingRows.push({
      source: "documents",
      sourceRowId: d.id,
      label: d.document_no,
      personalData,
      retainUntil,
    });
  }

  // Bank transactions can mention a person in their free-text. We match on the
  // subject name only — CVR is rarely present in bank text.
  if (subject.name) {
    const bank = db
      .query(
        `SELECT id, text, reference, retain_until,
                COALESCE(booking_date, transaction_date) AS basis_date
         FROM bank_transactions
         WHERE text LIKE ? ESCAPE '\\'
         ORDER BY id ASC`,
      )
      .all(`%${subject.name.replace(/[\\%_]/g, "\\$&")}%`) as Array<{
      id: number;
      text: string;
      reference: string | null;
      retain_until: string | null;
      basis_date: string | null;
    }>;
    for (const b of bank) {
      const retainUntil = effectiveRetainUntil(db, b.retain_until, b.basis_date);
      if (retainUntil) linkedRetentions.push(retainUntil);
      bookkeepingRows.push({
        source: "bank_transactions",
        sourceRowId: b.id,
        label: b.reference,
        personalData: { name: b.text, address: null, email: null, vatOrCvr: null },
        retainUntil,
      });
    }
  }

  // The latest deadline among the subject's bookkeeping records — master-data
  // rows must be kept at least as long as the records that reference them.
  const masterDataRetainUntil =
    linkedRetentions.length > 0 ? linkedRetentions.slice().sort().at(-1)! : null;

  const customers = db
    .query(
      `SELECT id, name, address, email, vat_or_cvr
       FROM customers
       WHERE (? IS NOT NULL AND vat_or_cvr = ?) OR (? IS NOT NULL AND name = ?)
       ORDER BY id ASC`,
    )
    .all(subject.cvr, subject.cvr, subject.name, subject.name) as Array<{
    id: number;
    name: string;
    address: string | null;
    email: string | null;
    vat_or_cvr: string | null;
  }>;
  for (const c of customers) {
    rows.push({
      source: "customers",
      sourceRowId: c.id,
      label: c.name,
      personalData: { name: c.name, address: c.address, email: c.email, vatOrCvr: c.vat_or_cvr },
      retainUntil: masterDataRetainUntil,
    });
  }

  const vendors = db
    .query(
      `SELECT id, name, address, vat_or_cvr
       FROM vendors
       WHERE (? IS NOT NULL AND vat_or_cvr = ?) OR (? IS NOT NULL AND name = ?)
       ORDER BY id ASC`,
    )
    .all(subject.cvr, subject.cvr, subject.name, subject.name) as Array<{
    id: number;
    name: string;
    address: string | null;
    vat_or_cvr: string | null;
  }>;
  for (const v of vendors) {
    rows.push({
      source: "vendors",
      sourceRowId: v.id,
      label: v.name,
      personalData: { name: v.name, address: v.address, email: null, vatOrCvr: v.vat_or_cvr },
      retainUntil: masterDataRetainUntil,
    });
  }

  rows.push(...bookkeepingRows);
  return rows;
}

const REDACTED_PLACEHOLDER = "[redigeret — GDPR]";

/**
 * Returns a copy of `personalData` with every field listed in `redacted`
 * replaced: text fields become a placeholder, structured fields become null.
 */
function applyRedaction(personalData: GdprPersonalData, redacted: Set<string>): GdprPersonalData {
  if (redacted.size === 0) return personalData;
  return {
    name: redacted.has("name") ? REDACTED_PLACEHOLDER : personalData.name,
    address: redacted.has("address") ? null : personalData.address,
    email: redacted.has("email") ? null : personalData.email,
    vatOrCvr: redacted.has("vatOrCvr") ? null : personalData.vatOrCvr,
  };
}

/**
 * Builds a complete data-subject access report: every customer, vendor,
 * document and bank transaction holding personal data about the subject,
 * each annotated with its bookkeeping retention verdict and whether a prior
 * erasure already redacted it.
 */
export function buildGdprSubjectExport(db: Database, key: GdprSubjectKey): GdprSubjectExport {
  const asOf = trim(key.asOf) ?? currentUtcIsoDate(db);
  const subject = resolveSubject(key);
  if (!subject.cvr && !subject.name) {
    return {
      ok: false,
      asOf,
      appliedRules: [EXPORT_RULE_ID],
      subject,
      records: [],
      errors: ["a GDPR subject must be identified by cvr or name"],
    };
  }

  const erasures = loadErasures(db);
  const records: GdprExportRecord[] = collectSourceRows(db, subject).map((row) => {
    const redacted = erasures.get(`${row.source}:${row.sourceRowId}`) ?? new Set<string>();
    return {
      source: row.source,
      sourceRowId: row.sourceRowId,
      label: row.label,
      personalData: applyRedaction(row.personalData, redacted),
      retainUntil: row.retainUntil,
      underRetention: row.retainUntil !== null && row.retainUntil >= asOf,
      erased: redacted.size > 0,
    };
  });

  // #355 — audit-log hver indsigtssøgning så ejeren kan bevise overfor
  // Datatilsynet hvilke subject-data der er udleveret hvornår.
  const subjectKey = subject.cvr ?? subject.name!;
  insertAuditLog(db, {
    eventType: "gdpr_export",
    entityType: "gdpr_subject",
    entityId: subjectKey,
    message: `GDPR export for ${subjectKey}: ${records.length} record(s) returned (as-of ${asOf})`,
  });

  return {
    ok: true,
    asOf,
    appliedRules: [EXPORT_RULE_ID, RETENTION_RULE_ID],
    subject,
    records,
    errors: [],
  };
}

/** The personal-data field names redactable per source. */
const REDACTABLE_FIELDS: Record<GdprExportRecord["source"], string[]> = {
  customers: ["name", "address", "email", "vatOrCvr"],
  vendors: ["name", "address", "vatOrCvr"],
  documents: ["name", "address", "vatOrCvr"],
  bank_transactions: ["name"],
};

/**
 * Erases (redacts) personal data about the subject that is no longer under
 * bookkeeping retention, and refuses any record still legally required to be
 * kept. Each redaction is recorded as an append-only tombstone in
 * `gdpr_erasures`; no append-only master-data row and no ledger row is ever
 * modified, so the audit chain stays verifiable.
 */
export function eraseGdprSubject(db: Database, key: GdprSubjectKey): GdprErasureResult {
  const asOf = trim(key.asOf) ?? currentUtcIsoDate(db);
  const subject = resolveSubject(key);
  if (!subject.cvr && !subject.name) {
    return {
      ok: false,
      asOf,
      appliedRules: [ERASURE_RULE_ID],
      subject,
      erasedCount: 0,
      refusedCount: 0,
      alreadyErasedCount: 0,
      erased: [],
      refused: [],
      errors: ["a GDPR subject must be identified by cvr or name"],
    };
  }

  const subjectKey = subject.cvr ?? subject.name!;
  const erased: GdprErasureRecord[] = [];
  const refused: GdprErasureRefusal[] = [];
  let alreadyErasedCount = 0;

  db.transaction(() => {
    const existing = loadErasures(db);
    const insert = db.prepare(
      `INSERT INTO gdpr_erasures
         (subject_key, source, source_row_id, redacted_fields, rule_id, reason, retained_until_at_erasure)
       VALUES (?, ?, ?, ?, ?, ?, ?)`,
    );

    for (const row of collectSourceRows(db, subject)) {
      const tombstoneKey = `${row.source}:${row.sourceRowId}`;
      if (existing.has(tombstoneKey)) {
        alreadyErasedCount += 1;
        continue;
      }

      // A future retention deadline overrides the erasure request: the law
      // requires the record to be kept, so we clearly refuse it.
      if (row.retainUntil !== null && row.retainUntil >= asOf) {
        refused.push({
          source: row.source,
          sourceRowId: row.sourceRowId,
          label: row.label,
          retainUntil: row.retainUntil,
          reason:
            `bookkeeping retention requires this record until ${row.retainUntil}; ` +
            `erasure refused (rule ${RETENTION_RULE_ID})`,
        });
        continue;
      }

      const fields = REDACTABLE_FIELDS[row.source];
      insert.run(
        subjectKey,
        row.source,
        row.sourceRowId,
        JSON.stringify(fields),
        ERASURE_RULE_ID,
        `personal data redacted: no longer under bookkeeping retention as of ${asOf}`,
        row.retainUntil,
      );
      erased.push({
        source: row.source,
        sourceRowId: row.sourceRowId,
        label: row.label,
        redactedFields: fields,
      });
    }

    if (erased.length > 0) {
      insertAuditLog(db, {
        eventType: "gdpr_erasure",
        entityType: "gdpr_subject",
        entityId: subjectKey,
        message:
          `GDPR erasure for ${subjectKey}: redacted ${erased.length} record(s), ` +
          `refused ${refused.length} still under retention`,
      });
    }
  }, { immediate: true })();

  return {
    ok: true,
    asOf,
    appliedRules: [ERASURE_RULE_ID, RETENTION_RULE_ID],
    subject,
    erasedCount: erased.length,
    refusedCount: refused.length,
    alreadyErasedCount,
    erased,
    refused,
    errors: [],
  };
}

// ---------------------------------------------------------------------------
// #355 — Signed GDPR audit-log export.
//
// Genbruger den eksisterende audit_log-tabel (kerne-bogføringens append-only
// log) og filtrerer til alle `gdpr_*`-events. Pakken kan signeres med samme
// Ed25519-nøgle som backup-systemet bruger, så Datatilsynet eller subject'et
// selv kan verificere pakken uden at Rentemester er installeret.

import { createHash, createPrivateKey, sign as cryptoSign } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { backupEd25519PrivateKeyPath } from "./system-backups";

export type GdprAuditEvent = {
  id: number;
  occurredAt: string;
  eventType: string;
  subjectKey: string | null;
  actor: string;
  message: string;
};

export type GdprAuditExport = {
  ok: boolean;
  asOf: string;
  since: string | null;
  until: string | null;
  events: GdprAuditEvent[];
  fingerprint: string;
  signature?: { algorithm: "ed25519"; base64: string };
  errors: string[];
};

// Bevidst uden DK-prefix og -NNN-suffix — matcher det format de andre GDPR-
// rules bruger (EXPORT_RULE_ID, ERASURE_RULE_ID), så det IKKE optfanges af
// rules-metadata-consistency-testen som forventer DK-rules at være i YAML.
const GDPR_AUDIT_RULE_ID = "GDPR-AUDIT-LOG";

/**
 * Bygger en signeret GDPR-audit-log-eksport. Kun rækker hvor
 * `event_type LIKE 'gdpr_%'` returneres; fingerprint er sha256 af det
 * deterministiske JSON-output (uden signature-feltet selv).
 *
 * `signWithEd25519=true` aktiverer asymmetrisk signering med den
 * eksisterende backup-nøgle (samme nøgle, samme tillidskæde).
 */
export function buildGdprAuditExport(
  db: Database,
  options: {
    since?: string | null;
    until?: string | null;
    asOf?: string | null;
    signWithEd25519?: boolean;
    companyRoot?: string;
  } = {},
): GdprAuditExport {
  const asOf = trim(options.asOf ?? null) ?? currentUtcIsoDate(db);
  const since = trim(options.since ?? null);
  const until = trim(options.until ?? null);

  const filters: string[] = ["event_type LIKE 'gdpr_%'"];
  const params: unknown[] = [];
  if (since) {
    filters.push("created_at >= ?");
    params.push(since);
  }
  if (until) {
    filters.push("created_at <= ?");
    params.push(until);
  }

  const rows = db
    .query(
      `SELECT id, created_at, event_type, entity_id, actor, message
         FROM audit_log
        WHERE ${filters.join(" AND ")}
        ORDER BY id ASC`,
    )
    .all(...params) as Array<{
    id: number;
    created_at: string;
    event_type: string;
    entity_id: string | null;
    actor: string;
    message: string;
  }>;

  const events: GdprAuditEvent[] = rows.map((r) => ({
    id: r.id,
    occurredAt: r.created_at,
    eventType: r.event_type,
    subjectKey: r.entity_id,
    actor: r.actor,
    message: r.message,
  }));

  const payload = JSON.stringify(
    { asOf, since, until, events, ruleId: GDPR_AUDIT_RULE_ID },
    null,
    2,
  );
  const fingerprint = `sha256:${createHash("sha256").update(payload).digest("hex")}`;

  const result: GdprAuditExport = {
    ok: true,
    asOf,
    since: since ?? null,
    until: until ?? null,
    events,
    fingerprint,
    errors: [],
  };

  if (options.signWithEd25519) {
    if (!options.companyRoot) {
      return {
        ...result,
        ok: false,
        errors: ["companyRoot is required when signWithEd25519 is true"],
      };
    }
    const privPath = backupEd25519PrivateKeyPath(options.companyRoot);
    if (!existsSync(privPath)) {
      return {
        ...result,
        ok: false,
        errors: [
          `no ed25519 private key at ${privPath} — run "system backup --sign-with-ed25519" once to generate one`,
        ],
      };
    }
    const privateKeyPem = readFileSync(privPath, "utf8");
    const key = createPrivateKey(privateKeyPem);
    const sig = cryptoSign(null, Buffer.from(payload, "utf8"), key);
    result.signature = { algorithm: "ed25519", base64: sig.toString("base64") };
  }

  return result;
}
