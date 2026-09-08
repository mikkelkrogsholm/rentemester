import { Database } from "bun:sqlite";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { mkdirSync } from "node:fs";
import { ensureNullableVatPeriodColumn } from "./companies-schema";
import { backfillRetentionDeadlines } from "./retention";
import { seedNativeAccountRoles } from "./account-roles";
import { applySchemaMigrations, assertSchemaCompatibility, recordSchemaBaseline, schemaHistoryIsCurrent } from "./schema-version";

function hasColumn(db: Database, table: string, column: string) {
  const cols = db.query(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>;
  return cols.some((col) => col.name === column);
}

function hasTable(db: Database, table: string) {
  return db.query(
    "SELECT 1 AS present FROM sqlite_master WHERE type = 'table' AND name = ?",
  ).get(table) != null;
}

/** Legacy `vat_quarter` rows are only safe to canonicalise when their bounds
 * match the company's stored cadence. Pre-fix monthly and half-yearly writers
 * also used the misleading kind, while still persisting their real bounds. */
function isCanonicalLegacyVatPeriod(
  start: string,
  end: string,
  cadence: "month" | "quarter" | "half-year",
): boolean {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(start);
  if (!match || match[3] !== "01") return false;
  const year = Number(match[1]);
  const startMonth = Number(match[2]);
  const span = cadence === "month" ? 1 : cadence === "half-year" ? 6 : 3;
  if (startMonth < 1 || startMonth > 12 || (startMonth - 1) % span !== 0) {
    return false;
  }
  const endDate = new Date(Date.UTC(year, startMonth + span - 1, 0));
  const expectedEnd = endDate.toISOString().slice(0, 10);
  return end === expectedEnd;
}

/**
 * Legacy ledgers encoded VAT cadence in a misleading `vat_quarter` kind. The
 * table rebuild is deliberately lossless (including ids and lifecycle audit
 * references). It runs either for the old CHECK constraint or when a ledger
 * still contains legacy rows despite already having the newer constraint.
 * Conflicting filed periods are a legal/human resolution problem, never
 * something a migration may guess how to split.
 */
function migrateLegacyVatPeriodKind(db: Database) {
  const sql = (db.query("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'accounting_periods'").get() as { sql?: string } | null)?.sql ?? "";
  const schemaSupportsCanonicalKind = /['"]vat_period['"]/.test(sql);
  const legacyCount = (db.query("SELECT COUNT(*) AS n FROM accounting_periods WHERE kind = 'vat_quarter'").get() as { n: number }).n;
  if (schemaSupportsCanonicalKind && legacyCount === 0) return;

  const cadenceRow = hasColumn(db, "companies", "vat_period_type")
    ? (db.query("SELECT vat_period_type AS value FROM companies WHERE id = 1").get() as {
        value: string | null;
      } | null)
    : null;
  const storedCadence = cadenceRow ? cadenceRow.value : "quarter";
  const registeredCadence =
    storedCadence === "month" ||
    storedCadence === "quarter" ||
    storedCadence === "half-year"
      ? storedCadence
      : null;

  const vatPeriodRows = db.query(
    `SELECT id, period_start, period_end, kind, status
       FROM accounting_periods
      WHERE kind IN ('vat_period', 'vat_quarter')
      ORDER BY id ASC`,
  ).all() as Array<{
    id: number;
    period_start: string;
    period_end: string;
    kind: "vat_period" | "vat_quarter";
    status: string;
  }>;
  const noncanonicalPeriod = registeredCadence
    ? vatPeriodRows.find(
        (row) =>
          !isCanonicalLegacyVatPeriod(
            row.period_start,
            row.period_end,
            registeredCadence,
          ),
      )
    : null;
  if (noncanonicalPeriod) {
    throw new Error(
      `VAT period migration blocked: ${noncanonicalPeriod.kind} period #${noncanonicalPeriod.id} ` +
        `${noncanonicalPeriod.period_start}..${noncanonicalPeriod.period_end} ` +
        `(${noncanonicalPeriod.status}) does not match the registered ${registeredCadence} cadence. ` +
        "The row was left unchanged; a human must verify the historical SKAT cadence and migrate or quarantine it explicitly before retrying.",
    );
  }

  if (!registeredCadence) {
    // Deregistration erased the former cadence. Every VAT period involved in
    // this rebuild — including already-canonical rows in a partially migrated
    // ledger — must nevertheless agree on one possible historical cadence.
    // Row-by-row guessing can otherwise relabel a monthly and quarterly mix
    // into a state that no future registration can reopen or file.
    const commonHistoricalCadences = (
      ["month", "quarter", "half-year"] as const
    ).filter((cadence) =>
      vatPeriodRows.every((row) =>
        isCanonicalLegacyVatPeriod(
          row.period_start,
          row.period_end,
          cadence,
        ),
      ),
    );
    if (commonHistoricalCadences.length === 0) {
      const periods = vatPeriodRows
        .map(
          (row) =>
            `#${row.id} ${row.period_start}..${row.period_end} (${row.kind}, ${row.status})`,
        )
        .join(", ");
      throw new Error(
        "VAT period migration blocked: the deregistered company's existing " +
          `VAT periods do not share one canonical historical cadence (${periods}). ` +
          "All rows were left unchanged; a human must record and reconcile the historical SKAT cadence before retrying.",
      );
    }
  }

  const exactDuplicate = db.query(
    `SELECT legacy.id AS legacy_id, canonical.id AS canonical_id
       FROM accounting_periods legacy
       JOIN accounting_periods canonical
         ON canonical.kind = 'vat_period'
        AND canonical.period_start = legacy.period_start
        AND canonical.period_end = legacy.period_end
      WHERE legacy.kind = 'vat_quarter'
      LIMIT 1`,
  ).get() as { legacy_id: number; canonical_id: number } | null;
  if (exactDuplicate) {
    throw new Error(
      `VAT period migration blocked: legacy period #${exactDuplicate.legacy_id} duplicates canonical period #${exactDuplicate.canonical_id}; resolve with a human before migration`,
    );
  }

  const conflicts = db.query(
    `SELECT a.id AS left_id, b.id AS right_id
       FROM accounting_periods a
       JOIN accounting_periods b ON a.id < b.id
      WHERE a.kind IN ('vat_period', 'vat_quarter')
        AND b.kind IN ('vat_period', 'vat_quarter')
        AND a.status = 'reported' AND b.status = 'reported'
        AND NOT (a.period_end < b.period_start OR a.period_start > b.period_end)
      LIMIT 1`,
  ).get() as { left_id: number; right_id: number } | null;
  if (conflicts) {
    throw new Error(`VAT period migration blocked: reported legacy periods #${conflicts.left_id} and #${conflicts.right_id} overlap; resolve with a human before migration`);
  }
  db.transaction(() => {
    db.exec("DROP TRIGGER IF EXISTS accounting_periods_guard_update; DROP TRIGGER IF EXISTS accounting_periods_no_delete;");
    db.exec(`
      DROP TABLE IF EXISTS accounting_periods_vat_period_rebuild;
      CREATE TABLE accounting_periods_vat_period_rebuild (
        id INTEGER PRIMARY KEY,
        period_start TEXT NOT NULL,
        period_end TEXT NOT NULL,
        kind TEXT NOT NULL CHECK(kind IN ('vat_period','vat_quarter','fiscal_year','custom')),
        status TEXT NOT NULL CHECK(status IN ('open','closed','reported')) DEFAULT 'open',
        closed_at TEXT, closed_by TEXT, reported_at TEXT, reference TEXT,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        UNIQUE(period_start, period_end, kind)
      );
      INSERT INTO accounting_periods_vat_period_rebuild
      SELECT id, period_start, period_end,
             CASE WHEN kind = 'vat_quarter' THEN 'vat_period' ELSE kind END,
             status, closed_at, closed_by, reported_at, reference, created_at
        FROM accounting_periods;
      DROP TABLE accounting_periods;
      ALTER TABLE accounting_periods_vat_period_rebuild RENAME TO accounting_periods;
    `);
  })();
}

/**
 * GDPR audit-log rows became overlay-redactable and tombstones became
 * subject-scoped after the original table shipped. SQLite cannot widen the
 * CHECK/UNIQUE constraints in place, so rebuild the append-only table without
 * changing ids, timestamps, legacy raw subject keys or existing evidence.
 */
function migrateGdprErasureTable(db: Database) {
  const tableSql =
    (db
      .query(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'gdpr_erasures'",
      )
      .get() as { sql?: string } | null)?.sql ?? "";
  const supportsAuditOverlay = /['"]audit_log['"]/.test(tableSql);
  const subjectScoped =
    /UNIQUE\s*\(\s*subject_key\s*,\s*source\s*,\s*source_row_id\s*\)/i.test(
      tableSql,
    );
  if (supportsAuditOverlay && subjectScoped) return;

  db.transaction(() => {
    db.exec(`
      DROP TRIGGER IF EXISTS gdpr_erasures_no_update;
      DROP TRIGGER IF EXISTS gdpr_erasures_no_delete;
      DROP TABLE IF EXISTS gdpr_erasures_source_rebuild;
      CREATE TABLE gdpr_erasures_source_rebuild (
        id INTEGER PRIMARY KEY,
        subject_key TEXT NOT NULL,
        source TEXT NOT NULL CHECK(source IN ('customers','vendors','documents','bank_transactions','audit_log')),
        source_row_id INTEGER NOT NULL,
        redacted_fields TEXT NOT NULL,
        rule_id TEXT NOT NULL,
        reason TEXT NOT NULL,
        retained_until_at_erasure TEXT,
        erased_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        UNIQUE(subject_key, source, source_row_id)
      );
      INSERT INTO gdpr_erasures_source_rebuild
        (id, subject_key, source, source_row_id, redacted_fields, rule_id,
         reason, retained_until_at_erasure, erased_at)
      SELECT id, subject_key, source, source_row_id, redacted_fields, rule_id,
             reason, retained_until_at_erasure, erased_at
        FROM gdpr_erasures;
      DROP TABLE gdpr_erasures;
      ALTER TABLE gdpr_erasures_source_rebuild RENAME TO gdpr_erasures;
      CREATE INDEX idx_gdpr_erasures_subject
        ON gdpr_erasures(subject_key);
    `);
  })();
}

export type OpenDbOptions = {
  /**
   * Normal ledgers use WAL. Disposable restore staging databases use DELETE so
   * Bun on Windows releases every filesystem handle before the atomic swap.
   */
  journalMode?: "WAL" | "DELETE";
};

function makeCloseDeterministic(db: Database): void {
  const close = Database.prototype.close;
  let closed = false;
  Object.defineProperty(db, "close", {
    value: function (this: Database, throwOnError = false) {
      if (closed) return;
      (this as Database & { clearQueryCache(): void }).clearQueryCache();
      // Finish this connection's WAL work while the handle is still alive.
      // PASSIVE never waits for another writer, so concurrent CLI processes
      // retain their normal serialization without a strict-close deadlock.
      try {
        this.run("PRAGMA wal_checkpoint(PASSIVE)");
      } catch {
        // An exceptional caller may still be unwinding a transaction. Native
        // close performs the rollback; checkpointing must not mask its error.
      }
      close.call(this, throwOnError);
      closed = true;
    },
  });
}

export function openDb(path: string, options: OpenDbOptions = {}) {
  // Compatibility is a read-only preflight. In particular, do not change the
  // persistent journal mode or create WAL sidecars before rejecting a database
  // written by newer software.
  if (existsSync(path)) {
    const preflight = new Database(path, { readonly: true });
    try {
      assertSchemaCompatibility(preflight);
    } finally {
      (preflight as Database & { clearQueryCache(): void }).clearQueryCache();
      preflight.close();
    }
  }
  mkdirSync(dirname(path), { recursive: true });
  const db = new Database(path);
  // busy_timeout must absorb transient contention when several short-lived
  // processes open the same company ledger at once (the agent-demo pipeline,
  // parallel test runs on a saturated CI host). 5s was too tight under load;
  // 30s lets the single writer finish rather than erroring "database is locked".
  db.run("PRAGMA foreign_keys = ON");
  db.run("PRAGMA busy_timeout = 30000");
  db.exec(options.journalMode === "DELETE"
    ? "PRAGMA journal_mode = DELETE;"
    : "PRAGMA journal_mode = WAL;");
  makeCloseDeterministic(db);
  return db;
}

// Protective triggers are declared with CREATE TRIGGER IF NOT EXISTS, so a
// plain re-run of the schema cannot repair a trigger that was dropped and
// re-created with a tampered (or missing) body. Re-applying every trigger
// definition unconditionally guarantees migrate() restores the canonical
// append-only protection.
function restoreSchemaTriggers(db: Database, schema: string) {
  const triggerStatements = schema.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
  db.transaction(() => {
    for (const statement of triggerStatements) {
      const nameMatch = /CREATE TRIGGER(?:\s+IF\s+NOT\s+EXISTS)?\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement);
      if (!nameMatch) continue;
      const name = nameMatch[1];
      const canonical = statement.replace(/CREATE TRIGGER\s+IF\s+NOT\s+EXISTS/i, "CREATE TRIGGER");
      // SQLite DDL is transactional. Keeping every DROP+CREATE pair in one
      // write transaction means a malformed/new trigger body cannot leave a
      // live ledger with its previous guard missing, and concurrent migrate()
      // calls never observe the half-restored interval.
      db.run(`DROP TRIGGER IF EXISTS ${name}`);
      db.exec(canonical);
    }
  }).immediate();
}

// Kept as a baseline-migration compatibility seam. schema.sql creates views
// that are missing from older ledgers, but a present view is never replaced by
// routine migration: drift is inspected and repaired only through the
// explicit, audited system repair-schema-views command.
function restoreSchemaViews(db: Database, schema: string) {
  void db;
  void schema;
}

export function migrate(db: Database) {
  assertSchemaCompatibility(db);
  const queryOnly = Number(Object.values(
    db.query("PRAGMA query_only").get() as Record<string, unknown>,
  )[0]) === 1;
  if (queryOnly) {
    if (!schemaHistoryIsCurrent(db)) {
      throw new Error("ledger schema migration is required before read-only access");
    }
    return;
  }
  // A current, checksummed ledger is an inspection target: routine opens must
  // never recreate a missing canonical view and thereby hide drift. Pending
  // and fresh ledgers still evaluate the complete baseline schema.
  const currentBeforeSchema = schemaHistoryIsCurrent(db);
  const missingCurrentViews = currentBeforeSchema
    ? (readFileSync(join(import.meta.dir, "../../src/core/schema.sql"), "utf8").match(/CREATE VIEW(?:\s+IF\s+NOT\s+EXISTS)?\s+[A-Za-z_][A-Za-z0-9_]*/gi) ?? []).map((statement) => /CREATE VIEW(?:\s+IF\s+NOT\s+EXISTS)?\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement)![1]!).filter((name) => !db.query("SELECT 1 FROM sqlite_master WHERE type='view' AND name=?").get(name))
    : [];
  // BASELINE_MIGRATION_V1_NORMALIZATION_START
  // The canonical schema now defines INSERT triggers that reference the
  // journal evidence columns. CREATE TABLE IF NOT EXISTS does not add those
  // columns to a legacy table, so make the nullable, lossless upgrade before
  // evaluating schema.sql. Fresh databases still get the stronger NOT NULL +
  // FK table definitions below. Existing rows are never assigned guessed
  // evidence; NULL remains an explicit unresolved legacy state.
  if (hasTable(db, "invoice_payments") && !hasColumn(db, "invoice_payments", "journal_entry_id")) {
    db.exec("ALTER TABLE invoice_payments ADD COLUMN journal_entry_id INTEGER REFERENCES journal_entries(id);");
  }
  if (hasTable(db, "invoice_refunds") && !hasColumn(db, "invoice_refunds", "journal_entry_id")) {
    db.exec("ALTER TABLE invoice_refunds ADD COLUMN journal_entry_id INTEGER REFERENCES journal_entries(id);");
  }
  if (hasTable(db, "invoice_claim_payments") && !hasColumn(db, "invoice_claim_payments", "journal_entry_id")) {
    db.exec("ALTER TABLE invoice_claim_payments ADD COLUMN journal_entry_id INTEGER REFERENCES journal_entries(id);");
  }
  const schema = readFileSync(join(import.meta.dir, "../../src/core/schema.sql"), "utf8");
  db.exec(schema);
  migrateLegacyVatPeriodKind(db);
  migrateGdprErasureTable(db);
  if (!hasColumn(db, "companies", "cvr")) db.exec("ALTER TABLE companies ADD COLUMN cvr TEXT;");
  if (!hasColumn(db, "companies", "fiscal_year_start_month")) db.exec("ALTER TABLE companies ADD COLUMN fiscal_year_start_month INTEGER NOT NULL DEFAULT 1 CHECK(fiscal_year_start_month BETWEEN 1 AND 12);");
  if (!hasColumn(db, "companies", "fiscal_year_label_strategy")) db.exec("ALTER TABLE companies ADD COLUMN fiscal_year_label_strategy TEXT NOT NULL DEFAULT 'end-year' CHECK(fiscal_year_label_strategy IN ('end-year', 'start-year', 'span'));");
  // CVR-register stamdata columns — older ledgers predate `company sync-cvr`.
  if (!hasColumn(db, "companies", "address")) db.exec("ALTER TABLE companies ADD COLUMN address TEXT;");
  if (!hasColumn(db, "companies", "postal_code")) db.exec("ALTER TABLE companies ADD COLUMN postal_code TEXT;");
  if (!hasColumn(db, "companies", "city")) db.exec("ALTER TABLE companies ADD COLUMN city TEXT;");
  if (!hasColumn(db, "companies", "company_form")) db.exec("ALTER TABLE companies ADD COLUMN company_form TEXT;");
  if (!hasColumn(db, "companies", "industry_code")) db.exec("ALTER TABLE companies ADD COLUMN industry_code TEXT;");
  if (!hasColumn(db, "companies", "industry_text")) db.exec("ALTER TABLE companies ADD COLUMN industry_text TEXT;");
  if (!hasColumn(db, "companies", "cvr_status")) db.exec("ALTER TABLE companies ADD COLUMN cvr_status TEXT;");
  if (!hasColumn(db, "companies", "audit_waived")) db.exec("ALTER TABLE companies ADD COLUMN audit_waived INTEGER;");
  if (!hasColumn(db, "companies", "cvr_synced_at")) db.exec("ALTER TABLE companies ADD COLUMN cvr_synced_at TEXT;");
  // #221: the owner's own payment terms — default days from invoice issue date
  // to due date. Captured once on the company profile so every invoice inherits
  // it instead of the owner re-typing it. Older ledgers predate the column.
  if (!hasColumn(db, "companies", "payment_terms_days")) db.exec("ALTER TABLE companies ADD COLUMN payment_terms_days INTEGER NOT NULL DEFAULT 14 CHECK(payment_terms_days BETWEEN 0 AND 365);");
  // The VAT-cadence column is nullable — `null` means the company is not
  // VAT-registered. On older ledgers the column may have been added as
  // `NOT NULL DEFAULT 'quarter'`; the helper rebuilds the table to relax it.
  ensureNullableVatPeriodColumn(db);
  // #350 — Per-virksomhed mail-alias: hash-friendly identifier brugt som
  // localpart i bilagsmail-adressen (fx "<alias>@bilag.rentemester.dk").
  // Cockpit/CLI håndhæver unicitet før den skrives; her er det nullbart fordi
  // ikke alle virksomheder har et alias konfigureret.
  if (!hasColumn(db, "companies", "mail_alias")) db.exec("ALTER TABLE companies ADD COLUMN mail_alias TEXT;");
  // Contact-detail columns on customers/vendors — older ledgers predate the
  // Dinero contacts import + CVR enrichment.
  if (!hasColumn(db, "customers", "phone")) db.exec("ALTER TABLE customers ADD COLUMN phone TEXT;");
  if (!hasColumn(db, "customers", "website")) db.exec("ALTER TABLE customers ADD COLUMN website TEXT;");
  if (!hasColumn(db, "vendors", "email")) db.exec("ALTER TABLE vendors ADD COLUMN email TEXT;");
  if (!hasColumn(db, "vendors", "phone")) db.exec("ALTER TABLE vendors ADD COLUMN phone TEXT;");
  if (!hasColumn(db, "vendors", "website")) db.exec("ALTER TABLE vendors ADD COLUMN website TEXT;");
  if (!hasColumn(db, "vendors", "country_code")) db.exec("ALTER TABLE vendors ADD COLUMN country_code TEXT;");
  if (!hasColumn(db, "vendors", "identifier_kind")) db.exec("ALTER TABLE vendors ADD COLUMN identifier_kind TEXT;");
  if (!hasColumn(db, "vendors", "identity_status")) db.exec("ALTER TABLE vendors ADD COLUMN identity_status TEXT NOT NULL DEFAULT 'human_resolution_required';");
  if (!hasColumn(db, "documents", "supplier_country_code")) db.exec("ALTER TABLE documents ADD COLUMN supplier_country_code TEXT;");
  if (!hasColumn(db, "documents", "supplier_identifier_kind")) db.exec("ALTER TABLE documents ADD COLUMN supplier_identifier_kind TEXT;");
  if (!hasColumn(db, "documents", "supplier_identity_status")) db.exec("ALTER TABLE documents ADD COLUMN supplier_identity_status TEXT;");
  if (!hasColumn(db, "account_role_mappings", "confirmation_source")) {
    db.exec("ALTER TABLE account_role_mappings ADD COLUMN confirmation_source TEXT NOT NULL DEFAULT 'explicit' CHECK(confirmation_source IN ('native_seed','explicit'));");
    db.run("UPDATE account_role_mappings SET confirmation_source = 'native_seed' WHERE confirmed_by = 'system'");
  }
  // EJER-3: customers.payment_terms_days became nullable — NULL means "ingen
  // eksplicit kundefrist; arv virksomhedens profilfrist på fakturatidspunktet".
  // Older ledgers carry the legacy NOT NULL DEFAULT 30 definition, which SQLite
  // cannot relax in place, so the table is rebuilt once. Existing rows keep
  // their stored value: a pre-migration 30 cannot be told apart from a
  // deliberate 30, so legacy customers stay on their stored frist (documented
  // backward-compat choice — only customers created WITHOUT a frist after this
  // migration inherit the profile). No FKs reference customers, so the rebuild
  // is a plain copy.
  const customersPaymentTerms = (db.query(`PRAGMA table_info(customers)`).all() as Array<{ name: string; notnull: number }>)
    .find((col) => col.name === "payment_terms_days");
  if (customersPaymentTerms && customersPaymentTerms.notnull === 1) {
    // Wrap the rebuild in an explicit transaction so an abnormal termination
    // mid-rebuild rolls back cleanly instead of leaving the ledger with a
    // half-built rebuild table and no `customers`. DROP TABLE IF EXISTS first
    // makes recovery idempotent: a rebuild table left behind by an earlier
    // aborted attempt is discarded rather than crashing this migrate() with
    // "table customers_payment_terms_rebuild already exists".
    db.transaction(() => {
      db.exec(`
      DROP TABLE IF EXISTS customers_payment_terms_rebuild;
      CREATE TABLE customers_payment_terms_rebuild (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        address TEXT,
        vat_or_cvr TEXT,
        email TEXT,
        phone TEXT,
        website TEXT,
        ean_number TEXT,
        payment_terms_days INTEGER CHECK(payment_terms_days IS NULL OR payment_terms_days > 0),
        default_currency TEXT NOT NULL DEFAULT 'DKK',
        notes TEXT,
        archived INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        UNIQUE(vat_or_cvr, name)
      );
      INSERT INTO customers_payment_terms_rebuild (id, name, address, vat_or_cvr, email, phone, website, ean_number, payment_terms_days, default_currency, notes, archived, created_at)
        SELECT id, name, address, vat_or_cvr, email, phone, website, ean_number, payment_terms_days, default_currency, notes, archived, created_at FROM customers;
      DROP TABLE customers;
      ALTER TABLE customers_payment_terms_rebuild RENAME TO customers;
    `);
    })();
  }
  // customers/vendors are no longer append-only — drop the legacy guard
  // triggers from ledgers created before that change.
  for (const trigger of [
    "customers_no_update",
    "customers_no_delete",
    "vendors_no_update",
    "vendors_no_delete",
  ]) {
    db.run(`DROP TRIGGER IF EXISTS ${trigger}`);
  }
  if (!hasColumn(db, "documents", "retain_until")) db.exec("ALTER TABLE documents ADD COLUMN retain_until TEXT;");
  if (!hasColumn(db, "bank_transactions", "retain_until")) db.exec("ALTER TABLE bank_transactions ADD COLUMN retain_until TEXT;");
  if (!hasColumn(db, "journal_entries", "retain_until")) db.exec("ALTER TABLE journal_entries ADD COLUMN retain_until TEXT;");
  if (!hasColumn(db, "bank_transactions", "amount_dkk")) db.exec("ALTER TABLE bank_transactions ADD COLUMN amount_dkk NUMERIC;");
  if (!hasColumn(db, "bank_transactions", "fx_rate_to_dkk")) db.exec("ALTER TABLE bank_transactions ADD COLUMN fx_rate_to_dkk NUMERIC;");
  if (!hasColumn(db, "journal_entries", "currency")) db.exec("ALTER TABLE journal_entries ADD COLUMN currency TEXT NOT NULL DEFAULT 'DKK';");
  if (!hasColumn(db, "journal_entries", "amount_foreign")) db.exec("ALTER TABLE journal_entries ADD COLUMN amount_foreign NUMERIC;");
  if (!hasColumn(db, "journal_entries", "amount_dkk")) db.exec("ALTER TABLE journal_entries ADD COLUMN amount_dkk NUMERIC;");
  if (!hasColumn(db, "journal_entries", "fx_rate_to_dkk")) db.exec("ALTER TABLE journal_entries ADD COLUMN fx_rate_to_dkk NUMERIC;");
  if (!hasColumn(db, "invoice_payments", "journal_entry_id")) db.exec("ALTER TABLE invoice_payments ADD COLUMN journal_entry_id INTEGER REFERENCES journal_entries(id);");
  if (!hasColumn(db, "invoice_refunds", "journal_entry_id")) db.exec("ALTER TABLE invoice_refunds ADD COLUMN journal_entry_id INTEGER REFERENCES journal_entries(id);");
  if (!hasColumn(db, "invoice_claim_payments", "journal_entry_id")) db.exec("ALTER TABLE invoice_claim_payments ADD COLUMN journal_entry_id INTEGER REFERENCES journal_entries(id);");
  if (!hasColumn(db, "exceptions", "source_evidence")) db.exec("ALTER TABLE exceptions ADD COLUMN source_evidence TEXT;");
  if (!hasColumn(db, "exceptions", "posting_preview")) db.exec("ALTER TABLE exceptions ADD COLUMN posting_preview TEXT;");
  // ===== BANK CLUSTER (#186-189,#182) =====
  // New columns on the existing bank_transactions table are not picked up by
  // CREATE TABLE IF NOT EXISTS, so older ledgers are upgraded here. Guards on
  // PRAGMA table_info keep every ALTER idempotent.
  if (!hasColumn(db, "bank_transactions", "bank_account_id")) db.exec("ALTER TABLE bank_transactions ADD COLUMN bank_account_id INTEGER REFERENCES bank_accounts(id);");
  if (!hasColumn(db, "bank_transactions", "counterparty_name")) db.exec("ALTER TABLE bank_transactions ADD COLUMN counterparty_name TEXT;");
  if (!hasColumn(db, "bank_transactions", "counterparty_account")) db.exec("ALTER TABLE bank_transactions ADD COLUMN counterparty_account TEXT;");
  if (!hasColumn(db, "bank_transactions", "message")) db.exec("ALTER TABLE bank_transactions ADD COLUMN message TEXT;");
  if (!hasColumn(db, "bank_transactions", "archive_reference")) db.exec("ALTER TABLE bank_transactions ADD COLUMN archive_reference TEXT;");
  if (!hasColumn(db, "bank_transactions", "customer_reference")) db.exec("ALTER TABLE bank_transactions ADD COLUMN customer_reference TEXT;");
  if (!hasColumn(db, "bank_transactions", "balance_after")) db.exec("ALTER TABLE bank_transactions ADD COLUMN balance_after NUMERIC;");
  if (!hasColumn(db, "bank_transactions", "raw_json")) db.exec("ALTER TABLE bank_transactions ADD COLUMN raw_json TEXT;");
  db.exec("CREATE INDEX IF NOT EXISTS idx_bank_transactions_account ON bank_transactions(bank_account_id);");
  // ===== END BANK CLUSTER (#186-189,#182) =====
  // JUR-7: persist whether an interest claim's reference rate came from the
  // statutory table or a manual override, so proposeInterestCorrection can
  // reconstruct the lawful interest with the SAME half-year segmentation it was
  // billed with. Legacy rows default to 'manual-override' (single stored rate,
  // reconstructed exactly as before — never retroactively re-segmented).
  if (!hasColumn(db, "invoice_interest_claims", "reference_rate_source")) {
    db.exec(
      "ALTER TABLE invoice_interest_claims ADD COLUMN reference_rate_source TEXT NOT NULL DEFAULT 'manual-override' CHECK(reference_rate_source IN ('statutory-table', 'manual-override'));",
    );
  }
  db.exec("CREATE UNIQUE INDEX IF NOT EXISTS idx_invoice_payments_journal_entry ON invoice_payments(journal_entry_id) WHERE journal_entry_id IS NOT NULL;");
  db.exec("CREATE UNIQUE INDEX IF NOT EXISTS idx_invoice_refunds_journal_entry ON invoice_refunds(journal_entry_id) WHERE journal_entry_id IS NOT NULL;");
  db.exec("CREATE UNIQUE INDEX IF NOT EXISTS idx_invoice_claim_payments_journal_entry ON invoice_claim_payments(journal_entry_id) WHERE journal_entry_id IS NOT NULL;");
  db.exec("CREATE INDEX IF NOT EXISTS idx_invoice_payments_document ON invoice_payments(invoice_document_id);");
  db.exec("CREATE INDEX IF NOT EXISTS idx_invoice_refunds_document ON invoice_refunds(invoice_document_id);");
  db.exec("CREATE INDEX IF NOT EXISTS idx_invoice_claim_payments_document ON invoice_claim_payments(invoice_document_id);");
  db.exec("CREATE INDEX IF NOT EXISTS idx_accounting_periods_covering_date ON accounting_periods(period_start, period_end, status);");
  // Recreate canonical evidence views and guards only after every legacy table
  // has the columns they reference. This also replaces stale definitions from
  // earlier releases instead of leaving a latent runtime failure.
  restoreSchemaViews(db, schema);
  restoreSchemaTriggers(db, schema);
  // Existing native ledgers predate #544. Seed only mappings whose account
  // metadata is compatible; imported/non-native charts remain incomplete.
  seedNativeAccountRoles(db);
  backfillRetentionDeadlines(db);
  // BASELINE_MIGRATION_V1_NORMALIZATION_END
  // schema.sql uses CREATE VIEW IF NOT EXISTS. Preserve the pre-open absence
  // on an already-current ledger so drift remains visible until explicit
  // repair; fresh and pending ledgers retain the schema-created views.
  for (const view of missingCurrentViews) db.exec(`DROP VIEW IF EXISTS ${view}`);
  recordSchemaBaseline(db);
  applySchemaMigrations(db);
  // #539 follows the immutable v1 normalisation. These guards apply equally
  // to freshly-created and legacy ledgers and are lossless on repeated opens.
  if (!hasColumn(db, "bank_accounts", "bic")) db.exec("ALTER TABLE bank_accounts ADD COLUMN bic TEXT;");
  if (!hasColumn(db, "bank_accounts", "account_owner")) db.exec("ALTER TABLE bank_accounts ADD COLUMN account_owner TEXT;");
  if (!hasColumn(db, "bank_accounts", "customer_no")) db.exec("ALTER TABLE bank_accounts ADD COLUMN customer_no TEXT;");
  db.exec("DROP TRIGGER IF EXISTS bank_accounts_guard_update;");
  db.exec(`CREATE TRIGGER bank_accounts_guard_update
    BEFORE UPDATE ON bank_accounts
    WHEN OLD.slug != NEW.slug OR OLD.created_at != NEW.created_at
    BEGIN SELECT RAISE(ABORT, 'bank account identity is immutable; use the audited update operation for its payment profile'); END;`);
}

export function dbExists(path: string) {
  return existsSync(path);
}
