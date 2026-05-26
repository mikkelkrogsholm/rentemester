import { Database } from "bun:sqlite";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { mkdirSync } from "node:fs";
import { backfillRetentionDeadlines } from "./retention";

function hasColumn(db: Database, table: string, column: string) {
  const cols = db.query(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>;
  return cols.some((col) => col.name === column);
}

export function openDb(path: string) {
  mkdirSync(dirname(path), { recursive: true });
  const db = new Database(path);
  // busy_timeout must absorb transient contention when several short-lived
  // processes open the same company ledger at once (the agent-demo pipeline,
  // parallel test runs on a saturated CI host). 5s was too tight under load;
  // 30s lets the single writer finish rather than erroring "database is locked".
  db.exec("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 30000;");
  return db;
}

// Protective triggers are declared with CREATE TRIGGER IF NOT EXISTS, so a
// plain re-run of the schema cannot repair a trigger that was dropped and
// re-created with a tampered (or missing) body. Re-applying every trigger
// definition unconditionally guarantees migrate() restores the canonical
// append-only protection.
function restoreSchemaTriggers(db: Database, schema: string) {
  const triggerStatements = schema.match(/CREATE TRIGGER[\s\S]*?END;/gi) ?? [];
  for (const statement of triggerStatements) {
    const nameMatch = /CREATE TRIGGER(?:\s+IF\s+NOT\s+EXISTS)?\s+([A-Za-z_][A-Za-z0-9_]*)/i.exec(statement);
    if (!nameMatch) continue;
    const name = nameMatch[1];
    const canonical = statement.replace(/CREATE TRIGGER\s+IF\s+NOT\s+EXISTS/i, "CREATE TRIGGER");
    db.run(`DROP TRIGGER IF EXISTS ${name}`);
    db.exec(canonical);
  }
}

export function migrate(db: Database) {
  const schema = readFileSync(join(import.meta.dir, "../../src/core/schema.sql"), "utf8");
  db.exec(schema);
  restoreSchemaTriggers(db, schema);
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
  if (!hasColumn(db, "invoice_payments", "journal_entry_id")) db.exec("ALTER TABLE invoice_payments ADD COLUMN journal_entry_id INTEGER;");
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
  db.exec("CREATE UNIQUE INDEX IF NOT EXISTS idx_invoice_payments_journal_entry ON invoice_payments(journal_entry_id) WHERE journal_entry_id IS NOT NULL;");
  db.exec("CREATE INDEX IF NOT EXISTS idx_accounting_periods_covering_date ON accounting_periods(period_start, period_end, status);");
  backfillRetentionDeadlines(db);
}

export function dbExists(path: string) {
  return existsSync(path);
}
