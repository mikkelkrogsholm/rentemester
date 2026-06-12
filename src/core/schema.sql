CREATE TABLE IF NOT EXISTS schema_migrations (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS companies (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL DEFAULT 'Unnamed company',
  country TEXT NOT NULL DEFAULT 'DK',
  currency TEXT NOT NULL DEFAULT 'DKK',
  cvr TEXT,
  fiscal_year_start_month INTEGER NOT NULL DEFAULT 1 CHECK(fiscal_year_start_month BETWEEN 1 AND 12),
  fiscal_year_label_strategy TEXT NOT NULL DEFAULT 'end-year' CHECK(fiscal_year_label_strategy IN ('end-year', 'start-year', 'span')),
  -- CVR-register stamdata, snapshotted by `company sync-cvr`. All nullable —
  -- the company works fully offline; these are only an enrichment.
  address TEXT,
  postal_code TEXT,
  city TEXT,
  company_form TEXT,
  industry_code TEXT,
  industry_text TEXT,
  cvr_status TEXT,
  audit_waived INTEGER,
  cvr_synced_at TEXT,
  -- #221: the owner's own default payment terms (days from issue to due date).
  -- Captured once on the company profile so every issued invoice inherits it.
  payment_terms_days INTEGER NOT NULL DEFAULT 14 CHECK(payment_terms_days BETWEEN 0 AND 365),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS accounts (
  id INTEGER PRIMARY KEY,
  account_no TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  type TEXT NOT NULL CHECK(type IN ('asset','liability','equity','income','expense','vat')),
  normal_balance TEXT NOT NULL CHECK(normal_balance IN ('debit','credit')),
  active INTEGER NOT NULL DEFAULT 1,
  default_vat_code TEXT,
  allow_direct_posting INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS sequences (
  kind TEXT NOT NULL,
  scope TEXT NOT NULL,
  value INTEGER NOT NULL CHECK(value >= 0),
  PRIMARY KEY (kind, scope)
);

CREATE TABLE IF NOT EXISTS vies_validations (
  country_code TEXT NOT NULL,
  vat_number TEXT NOT NULL,
  valid INTEGER NOT NULL,
  name TEXT,
  address TEXT,
  validated_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  raw_response TEXT,
  PRIMARY KEY (country_code, vat_number)
);

-- ===== CVR LOOKUP CACHE (CVR-register) =====
-- Snapshot cache for the Danish CVR register (distribution.virk.dk). CVR data
-- is non-deterministic external network data, so it is NEVER read live during
-- bookkeeping — a lookup writes one snapshot row here, stamped with fetched_at,
-- and every later read is served from this table. Like vies_validations this is
-- a deliberately MUTABLE cache (re-lookup overwrites via ON CONFLICT) — it has
-- no append-only trigger. The CVR number is stored as 8 digits, no DK prefix.
CREATE TABLE IF NOT EXISTS cvr_lookups (
  cvr TEXT PRIMARY KEY,
  name TEXT,
  address TEXT,
  postal_code TEXT,
  city TEXT,
  municipality_code INTEGER,
  company_form_code INTEGER,
  company_form_short TEXT,
  company_form_long TEXT,
  status TEXT,
  industry_code TEXT,
  industry_text TEXT,
  email TEXT,
  phone TEXT,
  website TEXT,
  start_date TEXT,
  fiscal_year_start TEXT,
  fiscal_year_end TEXT,
  audit_waived INTEGER,
  share_capital NUMERIC,
  share_capital_currency TEXT,
  employees INTEGER,
  advertising_protected INTEGER NOT NULL DEFAULT 0,
  management_json TEXT,
  raw_response TEXT,
  fetched_at TEXT NOT NULL,
  expires_at TEXT NOT NULL
);
-- ===== END CVR LOOKUP CACHE =====

CREATE TABLE IF NOT EXISTS customers (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  address TEXT,
  vat_or_cvr TEXT,
  email TEXT,
  phone TEXT,
  website TEXT,
  ean_number TEXT,
  payment_terms_days INTEGER NOT NULL DEFAULT 30 CHECK(payment_terms_days > 0),
  default_currency TEXT NOT NULL DEFAULT 'DKK',
  notes TEXT,
  archived INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(vat_or_cvr, name)
);

CREATE TABLE IF NOT EXISTS vendors (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  address TEXT,
  vat_or_cvr TEXT,
  email TEXT,
  phone TEXT,
  website TEXT,
  default_expense_account TEXT,
  default_vat_treatment TEXT,
  notes TEXT,
  archived INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(vat_or_cvr, name)
);

CREATE TABLE IF NOT EXISTS documents (
  id INTEGER PRIMARY KEY,
  document_no TEXT UNIQUE,
  source TEXT NOT NULL,
  original_filename TEXT,
  stored_path TEXT,
  mime_type TEXT,
  sha256_hash TEXT NOT NULL UNIQUE,
  upload_datetime TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  supplier_name TEXT,
  invoice_no TEXT,
  invoice_date TEXT,
  amount_inc_vat NUMERIC,
  currency TEXT NOT NULL DEFAULT 'DKK',
  status TEXT NOT NULL DEFAULT 'ingested',
  document_type TEXT NOT NULL DEFAULT 'purchase_sale',
  delivery_description TEXT,
  sender_name TEXT,
  sender_address TEXT,
  sender_vat_cvr TEXT,
  recipient_name TEXT,
  recipient_address TEXT,
  recipient_vat_cvr TEXT,
  vat_amount NUMERIC,
  payment_details TEXT,
  exemption_code TEXT,
  payload_json TEXT,
  retain_until TEXT
);

CREATE INDEX IF NOT EXISTS idx_documents_purchase_sale_logical_identity
ON documents(sender_vat_cvr, invoice_no, invoice_date)
WHERE document_type = 'purchase_sale';

CREATE UNIQUE INDEX IF NOT EXISTS idx_documents_issued_invoice_no_unique
ON documents(invoice_no)
WHERE document_type = 'issued_invoice';

-- ===== BANK CLUSTER (#186-189,#182) =====
-- A company can hold several bank accounts (driftskonto, valutakonto,
-- opsparingskonto, ...). Imported transactions are coupled to one of these so
-- reconciliation can be scoped to a single account (#187).
CREATE TABLE IF NOT EXISTS bank_accounts (
  id INTEGER PRIMARY KEY,
  slug TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  bank_name TEXT,
  registration_no TEXT,
  account_no TEXT,
  iban TEXT,
  currency TEXT NOT NULL DEFAULT 'DKK',
  ledger_account_no TEXT,
  active INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(ledger_account_no) REFERENCES accounts(account_no)
);
-- ===== END BANK CLUSTER (#186-189,#182) =====

CREATE TABLE IF NOT EXISTS bank_transactions (
  id INTEGER PRIMARY KEY,
  transaction_date TEXT NOT NULL,
  booking_date TEXT,
  text TEXT NOT NULL,
  amount NUMERIC NOT NULL,
  currency TEXT NOT NULL DEFAULT 'DKK',
  reference TEXT,
  amount_dkk NUMERIC,
  fx_rate_to_dkk NUMERIC,
  source_file_hash TEXT,
  import_batch_id TEXT,
  transaction_hash TEXT UNIQUE,
  status TEXT NOT NULL DEFAULT 'imported',
  retain_until TEXT,
  -- ===== BANK CLUSTER (#186-189,#182) =====
  -- bank_account_id is nullable for rows imported before #187; new imports
  -- always set it. Extra structured columns (#188) and the running balance
  -- (#189) are nullable and populated by import profiles that supply them.
  bank_account_id INTEGER REFERENCES bank_accounts(id),
  counterparty_name TEXT,
  counterparty_account TEXT,
  message TEXT,
  archive_reference TEXT,
  customer_reference TEXT,
  balance_after NUMERIC,
  raw_json TEXT
  -- ===== END BANK CLUSTER (#186-189,#182) =====
);

CREATE TABLE IF NOT EXISTS journal_entries (
  id INTEGER PRIMARY KEY,
  entry_no TEXT NOT NULL UNIQUE,
  transaction_date TEXT NOT NULL,
  registration_datetime TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  text TEXT NOT NULL,
  source_bank_transaction_id INTEGER,
  document_id INTEGER,
  currency TEXT NOT NULL DEFAULT 'DKK',
  amount_foreign NUMERIC,
  amount_dkk NUMERIC,
  fx_rate_to_dkk NUMERIC,
  rule_version TEXT NOT NULL,
  created_by TEXT NOT NULL DEFAULT 'system',
  created_by_program TEXT NOT NULL DEFAULT 'rentemester',
  status TEXT NOT NULL CHECK(status IN ('posted','reversed')) DEFAULT 'posted',
  reversal_of_entry_id INTEGER,
  previous_hash TEXT,
  entry_hash TEXT NOT NULL,
  locked INTEGER NOT NULL DEFAULT 1,
  retain_until TEXT,
  FOREIGN KEY(source_bank_transaction_id) REFERENCES bank_transactions(id),
  FOREIGN KEY(document_id) REFERENCES documents(id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_journal_entries_bank_source_posted
ON journal_entries(source_bank_transaction_id)
WHERE source_bank_transaction_id IS NOT NULL AND status = 'posted';

CREATE TABLE IF NOT EXISTS journal_lines (
  id INTEGER PRIMARY KEY,
  journal_entry_id INTEGER NOT NULL,
  account_id INTEGER NOT NULL,
  debit_amount NUMERIC NOT NULL DEFAULT 0 CHECK(debit_amount >= 0),
  credit_amount NUMERIC NOT NULL DEFAULT 0 CHECK(credit_amount >= 0),
  vat_code TEXT,
  currency TEXT NOT NULL DEFAULT 'DKK',
  text TEXT,
  FOREIGN KEY(journal_entry_id) REFERENCES journal_entries(id),
  FOREIGN KEY(account_id) REFERENCES accounts(id),
  CHECK(NOT (debit_amount > 0 AND credit_amount > 0))
);

CREATE TABLE IF NOT EXISTS audit_log (
  id INTEGER PRIMARY KEY,
  event_type TEXT NOT NULL,
  entity_type TEXT NOT NULL,
  entity_id TEXT,
  message TEXT NOT NULL,
  actor TEXT NOT NULL DEFAULT 'system',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS accounting_periods (
  id INTEGER PRIMARY KEY,
  period_start TEXT NOT NULL,
  period_end TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('vat_quarter','fiscal_year','custom')),
  status TEXT NOT NULL CHECK(status IN ('open','closed','reported')) DEFAULT 'open',
  closed_at TEXT,
  closed_by TEXT,
  reported_at TEXT,
  reference TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(period_start, period_end, kind)
);

CREATE TABLE IF NOT EXISTS invoice_payments (
  id INTEGER PRIMARY KEY,
  invoice_document_id INTEGER NOT NULL,
  bank_transaction_id INTEGER,
  journal_entry_id INTEGER NOT NULL UNIQUE,
  payment_date TEXT NOT NULL,
  amount NUMERIC NOT NULL CHECK(amount > 0),
  currency TEXT NOT NULL DEFAULT 'DKK',
  note TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(invoice_document_id) REFERENCES documents(id),
  FOREIGN KEY(bank_transaction_id) REFERENCES bank_transactions(id),
  FOREIGN KEY(journal_entry_id) REFERENCES journal_entries(id)
);

CREATE TABLE IF NOT EXISTS invoice_refunds (
  id INTEGER PRIMARY KEY,
  invoice_document_id INTEGER NOT NULL,
  bank_transaction_id INTEGER,
  refund_date TEXT NOT NULL,
  amount NUMERIC NOT NULL CHECK(amount > 0),
  currency TEXT NOT NULL DEFAULT 'DKK',
  note TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(invoice_document_id) REFERENCES documents(id),
  FOREIGN KEY(bank_transaction_id) REFERENCES bank_transactions(id)
);

CREATE TABLE IF NOT EXISTS invoice_reminders (
  id INTEGER PRIMARY KEY,
  invoice_document_id INTEGER NOT NULL,
  reminder_date TEXT NOT NULL,
  fee_amount NUMERIC NOT NULL CHECK(fee_amount > 0),
  currency TEXT NOT NULL DEFAULT 'DKK',
  note TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(invoice_document_id) REFERENCES documents(id)
);

CREATE TABLE IF NOT EXISTS invoice_compensation_claims (
  id INTEGER PRIMARY KEY,
  invoice_document_id INTEGER NOT NULL UNIQUE,
  claim_date TEXT NOT NULL,
  amount_dkk NUMERIC NOT NULL CHECK(amount_dkk > 0),
  note TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(invoice_document_id) REFERENCES documents(id)
);

CREATE TABLE IF NOT EXISTS invoice_interest_claims (
  id INTEGER PRIMARY KEY,
  invoice_document_id INTEGER NOT NULL,
  claim_date TEXT NOT NULL,
  reference_rate_percent NUMERIC NOT NULL,
  annual_interest_rate_percent NUMERIC NOT NULL,
  overdue_days INTEGER NOT NULL,
  principal_open_balance NUMERIC NOT NULL,
  amount_dkk NUMERIC NOT NULL CHECK(amount_dkk > 0),
  note TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(invoice_document_id, claim_date, reference_rate_percent),
  FOREIGN KEY(invoice_document_id) REFERENCES documents(id)
);

CREATE TABLE IF NOT EXISTS invoice_compensation_postings (
  id INTEGER PRIMARY KEY,
  compensation_claim_id INTEGER NOT NULL UNIQUE,
  journal_entry_id INTEGER NOT NULL UNIQUE,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(compensation_claim_id) REFERENCES invoice_compensation_claims(id),
  FOREIGN KEY(journal_entry_id) REFERENCES journal_entries(id)
);

CREATE TABLE IF NOT EXISTS invoice_reminder_postings (
  id INTEGER PRIMARY KEY,
  reminder_id INTEGER NOT NULL UNIQUE,
  journal_entry_id INTEGER NOT NULL UNIQUE,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(reminder_id) REFERENCES invoice_reminders(id),
  FOREIGN KEY(journal_entry_id) REFERENCES journal_entries(id)
);

CREATE TABLE IF NOT EXISTS invoice_interest_postings (
  id INTEGER PRIMARY KEY,
  interest_claim_id INTEGER NOT NULL UNIQUE,
  journal_entry_id INTEGER NOT NULL UNIQUE,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(interest_claim_id) REFERENCES invoice_interest_claims(id),
  FOREIGN KEY(journal_entry_id) REFERENCES journal_entries(id)
);

-- A correcting reversal of over-claimed morarente. When a balance reduction
-- (payment / credit note) is recorded with an effective date inside an already
-- POSTED interest claim's window, the lawful date-aware interest for that window
-- becomes lower than what was booked. This row records the correcting journal
-- entry (debit interest income, credit receivable) that reverses the excess.
-- amount_dkk is always the positive excess being reversed; getInvoiceStatus
-- subtracts it from the interest-claim balance. Append-only, like every claim.
CREATE TABLE IF NOT EXISTS invoice_interest_corrections (
  id INTEGER PRIMARY KEY,
  invoice_document_id INTEGER NOT NULL,
  correction_date TEXT NOT NULL,
  amount_dkk NUMERIC NOT NULL CHECK(amount_dkk > 0),
  reason TEXT,
  journal_entry_id INTEGER NOT NULL UNIQUE,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(invoice_document_id) REFERENCES documents(id),
  FOREIGN KEY(journal_entry_id) REFERENCES journal_entries(id)
);

CREATE TABLE IF NOT EXISTS invoice_claim_payments (
  id INTEGER PRIMARY KEY,
  invoice_document_id INTEGER NOT NULL,
  bank_transaction_id INTEGER,
  payment_date TEXT NOT NULL,
  amount NUMERIC NOT NULL CHECK(amount > 0),
  currency TEXT NOT NULL DEFAULT 'DKK',
  note TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(invoice_document_id) REFERENCES documents(id),
  FOREIGN KEY(bank_transaction_id) REFERENCES bank_transactions(id)
);

CREATE TABLE IF NOT EXISTS invoice_bad_debt_writeoffs (
  id INTEGER PRIMARY KEY,
  invoice_document_id INTEGER NOT NULL,
  writeoff_date TEXT NOT NULL,
  gross_amount NUMERIC NOT NULL CHECK(gross_amount > 0),
  net_amount NUMERIC NOT NULL CHECK(net_amount >= 0),
  vat_amount NUMERIC NOT NULL CHECK(vat_amount >= 0),
  note TEXT,
  journal_entry_id INTEGER NOT NULL UNIQUE,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(invoice_document_id) REFERENCES documents(id),
  FOREIGN KEY(journal_entry_id) REFERENCES journal_entries(id)
);

CREATE TABLE IF NOT EXISTS exceptions (
  id INTEGER PRIMARY KEY,
  type TEXT NOT NULL,
  severity TEXT NOT NULL DEFAULT 'medium',
  status TEXT NOT NULL DEFAULT 'open',
  related_bank_transaction_id INTEGER,
  related_document_id INTEGER,
  message TEXT NOT NULL,
  required_action TEXT,
  source_evidence TEXT,
  posting_preview TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  resolved_at TEXT,
  resolved_by TEXT,
  resolution_note TEXT
);

CREATE TRIGGER IF NOT EXISTS audit_log_no_update
BEFORE UPDATE ON audit_log
BEGIN
  SELECT RAISE(ABORT, 'audit_log is append-only');
END;

CREATE TRIGGER IF NOT EXISTS audit_log_no_delete
BEFORE DELETE ON audit_log
BEGIN
  SELECT RAISE(ABORT, 'audit_log is append-only');
END;

CREATE TRIGGER IF NOT EXISTS accounting_periods_guard_update
BEFORE UPDATE ON accounting_periods
WHEN OLD.period_start != NEW.period_start
   OR OLD.period_end != NEW.period_end
   OR OLD.kind != NEW.kind
   OR OLD.created_at != NEW.created_at
   OR OLD.status = 'reported'
   OR (OLD.status = 'closed' AND NEW.status = 'open')
   OR (OLD.status = 'open' AND NEW.status = 'reported')
   OR NEW.status NOT IN ('open', 'closed', 'reported')
BEGIN
  SELECT RAISE(ABORT, 'accounting periods may only progress open -> closed -> reported; period bounds are immutable');
END;

CREATE TRIGGER IF NOT EXISTS accounting_periods_no_delete
BEFORE DELETE ON accounting_periods
BEGIN
  SELECT RAISE(ABORT, 'accounting periods are append-only');
END;

CREATE TRIGGER IF NOT EXISTS sequences_monotone_update
BEFORE UPDATE ON sequences
WHEN OLD.kind != NEW.kind
   OR OLD.scope != NEW.scope
   OR NEW.value < OLD.value
BEGIN
  SELECT RAISE(ABORT, 'sequences are immutable identifiers and monotonically increasing');
END;

CREATE TRIGGER IF NOT EXISTS sequences_no_delete
BEFORE DELETE ON sequences
BEGIN
  SELECT RAISE(ABORT, 'sequences are append-only');
END;

CREATE TRIGGER IF NOT EXISTS exceptions_guard_update
BEFORE UPDATE ON exceptions
WHEN OLD.type != NEW.type
   OR OLD.severity != NEW.severity
   OR OLD.related_bank_transaction_id IS NOT NEW.related_bank_transaction_id
   OR OLD.related_document_id IS NOT NEW.related_document_id
   OR OLD.created_at != NEW.created_at
   OR (OLD.status = 'resolved' AND NEW.status = 'open')
BEGIN
  SELECT RAISE(ABORT, 'exceptions may only progress from open to resolved; identity is immutable');
END;

CREATE TRIGGER IF NOT EXISTS exceptions_no_delete
BEFORE DELETE ON exceptions
BEGIN
  SELECT RAISE(ABORT, 'exceptions are append-only; resolve them instead');
END;

CREATE TRIGGER IF NOT EXISTS companies_fiscal_lock
BEFORE UPDATE ON companies
WHEN (OLD.fiscal_year_start_month != NEW.fiscal_year_start_month
   OR OLD.fiscal_year_label_strategy != NEW.fiscal_year_label_strategy)
 AND EXISTS(SELECT 1 FROM journal_entries LIMIT 1)
BEGIN
  SELECT RAISE(ABORT, 'fiscal year configuration is locked after the first journal entry');
END;

-- customers and vendors are ordinary mutable master-data tables. They are
-- deliberately NOT append-only: the bookkeeping law's immutability requirement
-- covers the posted ledger and the invoice/document snapshots that materialise
-- buyer/sender fields at issue/ingest time — not the contact records, which
-- may be corrected and enriched freely. (Older ledgers had append-only triggers
-- here; db.ts migrate() drops them.)

CREATE TRIGGER IF NOT EXISTS journal_entries_no_update
BEFORE UPDATE ON journal_entries
BEGIN
  SELECT RAISE(ABORT, 'journal_entries are append-only; create reversal instead');
END;

CREATE TRIGGER IF NOT EXISTS journal_entries_no_delete
BEFORE DELETE ON journal_entries
BEGIN
  SELECT RAISE(ABORT, 'journal_entries are append-only; create reversal instead');
END;

CREATE TRIGGER IF NOT EXISTS journal_lines_no_update
BEFORE UPDATE ON journal_lines
BEGIN
  SELECT RAISE(ABORT, 'journal_lines are append-only; reverse the parent entry instead');
END;

CREATE TRIGGER IF NOT EXISTS journal_lines_no_delete
BEFORE DELETE ON journal_lines
BEGIN
  SELECT RAISE(ABORT, 'journal_lines are append-only; reverse the parent entry instead');
END;

CREATE TRIGGER IF NOT EXISTS documents_no_update_issued_invoice
BEFORE UPDATE ON documents
WHEN OLD.document_type IN ('issued_invoice','credit_note')
BEGIN
  SELECT RAISE(ABORT, 'issued invoice documents are append-only; create credit note instead');
END;

CREATE TRIGGER IF NOT EXISTS documents_no_delete_issued_invoice
BEFORE DELETE ON documents
WHEN OLD.document_type IN ('issued_invoice','credit_note')
BEGIN
  SELECT RAISE(ABORT, 'issued invoice documents are append-only; create credit note instead');
END;

CREATE TRIGGER IF NOT EXISTS documents_no_update_when_linked
BEFORE UPDATE ON documents
WHEN OLD.document_type IN ('purchase_sale','cash_register_receipt')
  AND EXISTS(SELECT 1 FROM journal_entries WHERE document_id = OLD.id)
BEGIN
  SELECT RAISE(ABORT, 'document is linked to a journal entry and cannot be modified; reverse the entry first');
END;

CREATE TRIGGER IF NOT EXISTS documents_no_delete_when_linked
BEFORE DELETE ON documents
WHEN OLD.document_type IN ('purchase_sale','cash_register_receipt')
  AND EXISTS(SELECT 1 FROM journal_entries WHERE document_id = OLD.id)
BEGIN
  SELECT RAISE(ABORT, 'document is linked to a journal entry and cannot be deleted; reverse the entry first');
END;

CREATE TRIGGER IF NOT EXISTS bank_transactions_no_update_when_referenced
BEFORE UPDATE ON bank_transactions
WHEN EXISTS(SELECT 1 FROM journal_entries WHERE source_bank_transaction_id = OLD.id)
   OR EXISTS(SELECT 1 FROM invoice_payments WHERE bank_transaction_id = OLD.id)
   OR EXISTS(SELECT 1 FROM invoice_refunds WHERE bank_transaction_id = OLD.id)
   OR EXISTS(SELECT 1 FROM invoice_claim_payments WHERE bank_transaction_id = OLD.id)
BEGIN
  SELECT RAISE(ABORT, 'bank transaction is referenced by ledger or payment records and cannot be modified');
END;

CREATE TRIGGER IF NOT EXISTS bank_transactions_no_delete
BEFORE DELETE ON bank_transactions
BEGIN
  SELECT RAISE(ABORT, 'bank transactions are append-only; correct via journal reversal or new import');
END;

-- ===== BANK CLUSTER (#186-189,#182) =====
-- Bank accounts are append-only identity records: only the `active` flag may
-- change (to retire a closed account), everything else is immutable so
-- already-imported transactions keep pointing at a stable account definition.
CREATE TRIGGER IF NOT EXISTS bank_accounts_guard_update
BEFORE UPDATE ON bank_accounts
WHEN OLD.slug != NEW.slug
   OR OLD.name != NEW.name
   OR OLD.bank_name IS NOT NEW.bank_name
   OR OLD.registration_no IS NOT NEW.registration_no
   OR OLD.account_no IS NOT NEW.account_no
   OR OLD.iban IS NOT NEW.iban
   OR OLD.currency != NEW.currency
   OR OLD.ledger_account_no IS NOT NEW.ledger_account_no
   OR OLD.created_at != NEW.created_at
BEGIN
  SELECT RAISE(ABORT, 'bank accounts are append-only; only the active flag may change');
END;

CREATE TRIGGER IF NOT EXISTS bank_accounts_no_delete
BEFORE DELETE ON bank_accounts
BEGIN
  SELECT RAISE(ABORT, 'bank accounts are append-only; deactivate them instead');
END;
-- ===== END BANK CLUSTER (#186-189,#182) =====

CREATE TRIGGER IF NOT EXISTS invoice_payments_require_journal
BEFORE INSERT ON invoice_payments
WHEN NEW.journal_entry_id IS NULL
BEGIN
  SELECT RAISE(ABORT, 'invoice payments must reference a journal entry');
END;

CREATE TRIGGER IF NOT EXISTS invoice_payments_no_update
BEFORE UPDATE ON invoice_payments
BEGIN
  SELECT RAISE(ABORT, 'invoice payments are append-only; add a correcting payment application instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_payments_no_delete
BEFORE DELETE ON invoice_payments
BEGIN
  SELECT RAISE(ABORT, 'invoice payments are append-only; add a correcting payment application instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_refunds_no_update
BEFORE UPDATE ON invoice_refunds
BEGIN
  SELECT RAISE(ABORT, 'invoice refunds are append-only; add a correcting refund application instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_refunds_no_delete
BEFORE DELETE ON invoice_refunds
BEGIN
  SELECT RAISE(ABORT, 'invoice refunds are append-only; add a correcting refund application instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_reminders_no_update
BEFORE UPDATE ON invoice_reminders
BEGIN
  SELECT RAISE(ABORT, 'invoice reminders are append-only; add a later reminder or corrective note instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_reminders_no_delete
BEFORE DELETE ON invoice_reminders
BEGIN
  SELECT RAISE(ABORT, 'invoice reminders are append-only; add a corrective note instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_compensation_claims_no_update
BEFORE UPDATE ON invoice_compensation_claims
BEGIN
  SELECT RAISE(ABORT, 'invoice compensation claims are append-only; add a correcting note instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_compensation_claims_no_delete
BEFORE DELETE ON invoice_compensation_claims
BEGIN
  SELECT RAISE(ABORT, 'invoice compensation claims are append-only; add a corrective note instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_interest_claims_no_update
BEFORE UPDATE ON invoice_interest_claims
BEGIN
  SELECT RAISE(ABORT, 'invoice interest claims are append-only; add a correcting note instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_interest_claims_no_delete
BEFORE DELETE ON invoice_interest_claims
BEGIN
  SELECT RAISE(ABORT, 'invoice interest claims are append-only; add a corrective note instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_compensation_postings_no_update
BEFORE UPDATE ON invoice_compensation_postings
BEGIN
  SELECT RAISE(ABORT, 'invoice compensation postings are append-only; reverse the journal entry instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_compensation_postings_no_delete
BEFORE DELETE ON invoice_compensation_postings
BEGIN
  SELECT RAISE(ABORT, 'invoice compensation postings are append-only; reverse the journal entry instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_reminder_postings_no_update
BEFORE UPDATE ON invoice_reminder_postings
BEGIN
  SELECT RAISE(ABORT, 'invoice reminder postings are append-only; reverse the journal entry instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_reminder_postings_no_delete
BEFORE DELETE ON invoice_reminder_postings
BEGIN
  SELECT RAISE(ABORT, 'invoice reminder postings are append-only; reverse the journal entry instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_interest_postings_no_update
BEFORE UPDATE ON invoice_interest_postings
BEGIN
  SELECT RAISE(ABORT, 'invoice interest postings are append-only; reverse the journal entry instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_interest_postings_no_delete
BEFORE DELETE ON invoice_interest_postings
BEGIN
  SELECT RAISE(ABORT, 'invoice interest postings are append-only; reverse the journal entry instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_interest_corrections_no_update
BEFORE UPDATE ON invoice_interest_corrections
BEGIN
  SELECT RAISE(ABORT, 'invoice interest corrections are append-only; add another correction instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_interest_corrections_no_delete
BEFORE DELETE ON invoice_interest_corrections
BEGIN
  SELECT RAISE(ABORT, 'invoice interest corrections are append-only; add another correction instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_claim_payments_no_update
BEFORE UPDATE ON invoice_claim_payments
BEGIN
  SELECT RAISE(ABORT, 'invoice claim payments are append-only; add a correcting claim payment application instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_claim_payments_no_delete
BEFORE DELETE ON invoice_claim_payments
BEGIN
  SELECT RAISE(ABORT, 'invoice claim payments are append-only; add a correcting claim payment application instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_bad_debt_writeoffs_no_update
BEFORE UPDATE ON invoice_bad_debt_writeoffs
BEGIN
  SELECT RAISE(ABORT, 'invoice bad-debt writeoffs are append-only; add a correcting journal entry instead');
END;

CREATE TRIGGER IF NOT EXISTS invoice_bad_debt_writeoffs_no_delete
BEFORE DELETE ON invoice_bad_debt_writeoffs
BEGIN
  SELECT RAISE(ABORT, 'invoice bad-debt writeoffs are append-only; add a correcting journal entry instead');
END;

-- ===== RECURRING INVOICES (#118) =====
-- Recurring-invoice templates and their explicit, deterministic generations.
-- A template captures the repeating invoice spec (interval, customer, lines,
-- VAT, delivery-period mode). Generation is an explicit step keyed by an
-- integer period_index counted from first_issue_date — no background
-- scheduling. UNIQUE(template_id, period_index) prevents duplicate generation
-- for the same template/period. Reminders/settlement live on the generated
-- documents row, never on the template.

CREATE TABLE IF NOT EXISTS recurring_invoice_templates (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  interval TEXT NOT NULL CHECK(interval IN ('monthly', 'quarterly', 'yearly')),
  first_issue_date TEXT NOT NULL,
  next_issue_date TEXT NOT NULL,
  payment_terms_days INTEGER NOT NULL DEFAULT 30 CHECK(payment_terms_days BETWEEN 0 AND 365),
  delivery_period_mode TEXT NOT NULL DEFAULT 'issue_month'
    CHECK(delivery_period_mode IN ('issue_month', 'interval_window', 'none')),
  payload_json TEXT NOT NULL,
  notes TEXT,
  active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0, 1)),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS recurring_invoice_generations (
  id INTEGER PRIMARY KEY,
  template_id INTEGER NOT NULL,
  period_index INTEGER NOT NULL CHECK(period_index >= 0),
  document_id INTEGER NOT NULL,
  invoice_number TEXT NOT NULL,
  issue_date TEXT NOT NULL,
  delivery_period_start TEXT,
  delivery_period_end TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(template_id) REFERENCES recurring_invoice_templates(id),
  FOREIGN KEY(document_id) REFERENCES documents(id),
  UNIQUE(template_id, period_index)
);

CREATE INDEX IF NOT EXISTS idx_recurring_invoice_generations_template
  ON recurring_invoice_generations(template_id, period_index);

-- Template identity and the embedded payload are immutable; only the
-- next_issue_date marker may advance and active may be retired (1 -> 0).
CREATE TRIGGER IF NOT EXISTS recurring_invoice_templates_guard_update
BEFORE UPDATE ON recurring_invoice_templates
WHEN OLD.name != NEW.name
   OR OLD.interval != NEW.interval
   OR OLD.first_issue_date != NEW.first_issue_date
   OR OLD.payment_terms_days != NEW.payment_terms_days
   OR OLD.delivery_period_mode != NEW.delivery_period_mode
   OR OLD.payload_json != NEW.payload_json
   OR OLD.created_at != NEW.created_at
   OR NEW.next_issue_date < OLD.next_issue_date
   OR (OLD.active = 0 AND NEW.active = 1)
BEGIN
  SELECT RAISE(ABORT, 'recurring invoice templates are append-only; only next_issue_date may advance and active may be retired');
END;

CREATE TRIGGER IF NOT EXISTS recurring_invoice_templates_no_delete
BEFORE DELETE ON recurring_invoice_templates
BEGIN
  SELECT RAISE(ABORT, 'recurring invoice templates are append-only; retire them with active = 0 instead');
END;

CREATE TRIGGER IF NOT EXISTS recurring_invoice_generations_no_update
BEFORE UPDATE ON recurring_invoice_generations
BEGIN
  SELECT RAISE(ABORT, 'recurring invoice generations are append-only audit links; issue a credit note on the generated invoice instead');
END;

CREATE TRIGGER IF NOT EXISTS recurring_invoice_generations_no_delete
BEFORE DELETE ON recurring_invoice_generations
BEGIN
  SELECT RAISE(ABORT, 'recurring invoice generations are append-only audit links; issue a credit note on the generated invoice instead');
END;
-- ===== END RECURRING INVOICES (#118) =====
-- ===== MAIL INTAKE (#122) =====
-- Append-only dedup ledger for the first deterministic bilagsmail intake
-- slice. One row per (message-id, attachment hash) pair that was ingested,
-- so rerunning the same maildrop never creates duplicate documents.
CREATE TABLE IF NOT EXISTS mail_intake_messages (
  id INTEGER PRIMARY KEY,
  message_id TEXT NOT NULL,
  attachment_sha256 TEXT NOT NULL,
  attachment_filename TEXT,
  document_id INTEGER,
  sender TEXT,
  subject TEXT,
  mail_date TEXT,
  ingested_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE (message_id, attachment_sha256)
);

CREATE INDEX IF NOT EXISTS idx_mail_intake_messages_message_id
ON mail_intake_messages(message_id);

CREATE TRIGGER IF NOT EXISTS mail_intake_messages_no_update
BEFORE UPDATE ON mail_intake_messages
BEGIN
  SELECT RAISE(ABORT, 'mail intake dedup rows are append-only; re-ingest creates a new row instead');
END;

CREATE TRIGGER IF NOT EXISTS mail_intake_messages_no_delete
BEFORE DELETE ON mail_intake_messages
BEGIN
  SELECT RAISE(ABORT, 'mail intake dedup rows are append-only and cannot be deleted');
END;
-- ===== MILEAGE LOG (#123) =====
-- Standalone kørselsregnskab register. Mileage entries are documentation/audit
-- data only; nothing here is posted to the journal/ledger. The per-kilometre
-- rate is user-supplied and source-backed (rate_basis), never a hardcoded tax
-- rate. Entries are append-only audit data.
CREATE TABLE IF NOT EXISTS mileage_entries (
  id INTEGER PRIMARY KEY,
  entry_no TEXT NOT NULL UNIQUE,
  trip_date TEXT NOT NULL,
  purpose TEXT NOT NULL,
  from_location TEXT NOT NULL,
  to_location TEXT NOT NULL,
  kilometers NUMERIC NOT NULL CHECK(kilometers > 0),
  vehicle TEXT NOT NULL,
  driver TEXT NOT NULL,
  rate_per_km NUMERIC NOT NULL CHECK(rate_per_km > 0),
  amount_basis NUMERIC NOT NULL CHECK(amount_basis >= 0),
  rate_basis TEXT NOT NULL,
  rate_source TEXT,
  notes TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_mileage_entries_trip_date
ON mileage_entries(trip_date);

CREATE TRIGGER IF NOT EXISTS mileage_entries_no_update
BEFORE UPDATE ON mileage_entries
BEGIN
  SELECT RAISE(ABORT, 'mileage_entries are append-only audit data; record a correcting entry instead');
END;

CREATE TRIGGER IF NOT EXISTS mileage_entries_no_delete
BEFORE DELETE ON mileage_entries
BEGIN
  SELECT RAISE(ABORT, 'mileage_entries are append-only audit data; record a correcting entry instead');
END;
-- ===== FIXED ASSETS (#124, #125) =====
-- Append-only fixed-asset register plus its depreciation entries (#124) and
-- immediate small-asset write-offs / straksafskrivning (#125). Money is stored
-- in DKK with 2 decimals; the workflow assists bookkeeping while the
-- user/advisor remains responsible for the tax treatment.

CREATE TABLE IF NOT EXISTS assets (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  category TEXT NOT NULL,
  acquisition_date TEXT NOT NULL,
  cost NUMERIC NOT NULL CHECK(cost > 0),
  depreciation_method TEXT NOT NULL DEFAULT 'linear' CHECK(depreciation_method IN ('linear')),
  useful_life_months INTEGER NOT NULL CHECK(useful_life_months > 0),
  asset_account_no TEXT NOT NULL,
  depreciation_expense_account_no TEXT NOT NULL,
  accumulated_depreciation_account_no TEXT NOT NULL,
  purchase_document_id INTEGER NOT NULL,
  note TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(purchase_document_id) REFERENCES documents(id)
);

CREATE TABLE IF NOT EXISTS asset_depreciation_entries (
  id INTEGER PRIMARY KEY,
  asset_id INTEGER NOT NULL,
  period_index INTEGER NOT NULL CHECK(period_index > 0),
  transaction_date TEXT NOT NULL,
  amount NUMERIC NOT NULL CHECK(amount > 0),
  journal_entry_id INTEGER NOT NULL UNIQUE,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(asset_id, period_index),
  FOREIGN KEY(asset_id) REFERENCES assets(id),
  FOREIGN KEY(journal_entry_id) REFERENCES journal_entries(id)
);

CREATE TABLE IF NOT EXISTS asset_writeoffs (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  category TEXT NOT NULL,
  acquisition_date TEXT NOT NULL,
  writeoff_date TEXT NOT NULL,
  cost NUMERIC NOT NULL CHECK(cost > 0),
  purchase_document_id INTEGER NOT NULL UNIQUE,
  expense_account_no TEXT NOT NULL,
  confirmed INTEGER NOT NULL DEFAULT 0 CHECK(confirmed IN (0,1)),
  threshold_dkk NUMERIC NOT NULL,
  threshold_rule_source TEXT NOT NULL,
  note TEXT,
  journal_entry_id INTEGER NOT NULL UNIQUE,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(purchase_document_id) REFERENCES documents(id),
  FOREIGN KEY(journal_entry_id) REFERENCES journal_entries(id)
);

CREATE INDEX IF NOT EXISTS idx_asset_depreciation_entries_asset ON asset_depreciation_entries(asset_id);
CREATE INDEX IF NOT EXISTS idx_assets_purchase_document ON assets(purchase_document_id);

CREATE TRIGGER IF NOT EXISTS assets_no_update
BEFORE UPDATE ON assets
BEGIN
  SELECT RAISE(ABORT, 'assets are append-only; register a correcting asset record instead');
END;

CREATE TRIGGER IF NOT EXISTS assets_no_delete
BEFORE DELETE ON assets
BEGIN
  SELECT RAISE(ABORT, 'assets are append-only; register a correcting asset record instead');
END;

CREATE TRIGGER IF NOT EXISTS asset_depreciation_entries_no_update
BEFORE UPDATE ON asset_depreciation_entries
BEGIN
  SELECT RAISE(ABORT, 'asset depreciation entries are append-only; reverse the journal entry instead');
END;

CREATE TRIGGER IF NOT EXISTS asset_depreciation_entries_no_delete
BEFORE DELETE ON asset_depreciation_entries
BEGIN
  SELECT RAISE(ABORT, 'asset depreciation entries are append-only; reverse the journal entry instead');
END;

CREATE TRIGGER IF NOT EXISTS asset_writeoffs_no_update
BEFORE UPDATE ON asset_writeoffs
BEGIN
  SELECT RAISE(ABORT, 'asset writeoffs are append-only; add a correcting journal entry instead');
END;

CREATE TRIGGER IF NOT EXISTS asset_writeoffs_no_delete
BEFORE DELETE ON asset_writeoffs
BEGIN
  SELECT RAISE(ABORT, 'asset writeoffs are append-only; add a correcting journal entry instead');
END;
-- ===== END FIXED ASSETS (#124, #125) =====
-- ===== PEPPOL SUBMISSION (#128) =====
-- Records a deterministic PEPPOL submission attempt built on top of an
-- existing OIOUBL handoff artifact. Submission records are audit data:
-- append-only, never updated or deleted. The idempotency key protects
-- against duplicate submissions. Access-point credentials are NEVER
-- stored here — only the non-secret access-point id used to derive the
-- submission envelope. The original invoice payload is not mutated.
CREATE TABLE IF NOT EXISTS peppol_submissions (
  id INTEGER PRIMARY KEY,
  invoice_document_id INTEGER NOT NULL,
  invoice_no TEXT,
  idempotency_key TEXT NOT NULL UNIQUE,
  submission_reference TEXT NOT NULL UNIQUE,
  access_point_id TEXT NOT NULL,
  receiver_endpoint_id TEXT NOT NULL,
  oioubl_sha256 TEXT NOT NULL,
  envelope_sha256 TEXT NOT NULL,
  envelope_xml TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('prepared','acknowledged')),
  transmission_id TEXT,
  acknowledged_at TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_peppol_submissions_invoice
  ON peppol_submissions(invoice_document_id);
CREATE INDEX IF NOT EXISTS idx_peppol_submissions_reference
  ON peppol_submissions(submission_reference);

CREATE TRIGGER IF NOT EXISTS peppol_submissions_no_update
BEFORE UPDATE ON peppol_submissions
BEGIN
  SELECT RAISE(ABORT, 'peppol submissions are append-only audit records; record a new submission attempt instead');
END;

CREATE TRIGGER IF NOT EXISTS peppol_submissions_no_delete
BEFORE DELETE ON peppol_submissions
BEGIN
  SELECT RAISE(ABORT, 'peppol submissions are append-only audit records; record a new submission attempt instead');
END;
-- ===== OPENING BALANCE (#179) =====
-- Marks that a company's opening balance (primobalance) has been posted.
-- The primobalance itself lives as a normal balanced journal entry in
-- journal_entries (posted via postJournalEntry, hash-chained and audited);
-- this table is only the idempotency marker — exactly one row per company.
-- The single-row constraint enforces "one primobalance per company": the
-- fixed CHECK(id = 1) primary key makes a second INSERT fail. The table is
-- append-only audit data: never updated, never deleted.
CREATE TABLE IF NOT EXISTS opening_balances (
  id INTEGER PRIMARY KEY CHECK(id = 1),
  cut_over_date TEXT NOT NULL,
  journal_entry_id INTEGER NOT NULL REFERENCES journal_entries(id),
  journal_entry_no TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TRIGGER IF NOT EXISTS opening_balances_no_update
BEFORE UPDATE ON opening_balances
BEGIN
  SELECT RAISE(ABORT, 'opening balance is append-only; reverse the journal entry instead');
END;

CREATE TRIGGER IF NOT EXISTS opening_balances_no_delete
BEFORE DELETE ON opening_balances
BEGIN
  SELECT RAISE(ABORT, 'opening balance is append-only; reverse the journal entry instead');
END;
-- ===== END OPENING BALANCE (#179) =====
-- ===== END PEPPOL SUBMISSION (#128) =====
-- ===== EMAIL DELIVERY (#180) =====
-- Append-only SMTP send log: records that an issued invoice / a reminder was
-- emailed to a customer, so a send is recorded and not silently repeated.
-- This is audit data — never updated or deleted. The unique message_id is
-- the idempotency key. SMTP CREDENTIALS are NEVER stored here; only the
-- non-secret smtp_host used for the send is recorded for the audit trail.
CREATE TABLE IF NOT EXISTS email_send_log (
  id INTEGER PRIMARY KEY,
  invoice_document_id INTEGER NOT NULL,
  invoice_no TEXT,
  kind TEXT NOT NULL CHECK(kind IN ('invoice','reminder')),
  recipient TEXT NOT NULL,
  sender TEXT NOT NULL,
  subject TEXT NOT NULL,
  message_id TEXT NOT NULL UNIQUE,
  body_sha256 TEXT NOT NULL,
  smtp_host TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_email_send_log_invoice
  ON email_send_log(invoice_document_id);

CREATE TRIGGER IF NOT EXISTS email_send_log_no_update
BEFORE UPDATE ON email_send_log
BEGIN
  SELECT RAISE(ABORT, 'email send log is append-only audit data; record a new send instead');
END;

CREATE TRIGGER IF NOT EXISTS email_send_log_no_delete
BEFORE DELETE ON email_send_log
BEGIN
  SELECT RAISE(ABORT, 'email send log is append-only audit data; record a new send instead');
END;
-- ===== END EMAIL DELIVERY (#180) =====

-- ===== GDPR (#184) =====
-- Append-only erasure tombstones. A GDPR erasure never UPDATEs/DELETEs the
-- append-only master-data rows or the ledger; instead it records one row per
-- redacted source record here. The GDPR export layer overlays these tombstones
-- so the redacted personal data never resurfaces. Keeping erasure as an
-- append-only journal means the audit chain and bookkeeping integrity are
-- untouched by a data-subject erasure.
CREATE TABLE IF NOT EXISTS gdpr_erasures (
  id INTEGER PRIMARY KEY,
  subject_key TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('customers','vendors','documents','bank_transactions')),
  source_row_id INTEGER NOT NULL,
  redacted_fields TEXT NOT NULL,
  rule_id TEXT NOT NULL,
  reason TEXT NOT NULL,
  retained_until_at_erasure TEXT,
  erased_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(source, source_row_id)
);

CREATE INDEX IF NOT EXISTS idx_gdpr_erasures_subject ON gdpr_erasures(subject_key);

CREATE TRIGGER IF NOT EXISTS gdpr_erasures_no_update
BEFORE UPDATE ON gdpr_erasures
BEGIN
  SELECT RAISE(ABORT, 'gdpr_erasures are append-only audit records; record a new erasure instead');
END;

CREATE TRIGGER IF NOT EXISTS gdpr_erasures_no_delete
BEFORE DELETE ON gdpr_erasures
BEGIN
  SELECT RAISE(ABORT, 'gdpr_erasures are append-only audit records; an erasure cannot be revoked');
END;
-- ===== END GDPR (#184) =====

-- ===== IMPORT ARCHIVE (#197) =====
-- A multi-year accounting-system export (Dinero #173) covers several fiscal
-- years; only the cut-over year is posted to the live ledger. The earlier,
-- pre-cut-over years are NOT posted — but discarding them loses audit history
-- and matching context. They are kept here as a READ-ONLY ARCHIVE: prior-year
-- Posteringer and SaldoBalance rows, tagged by source system and fiscal year.
--
-- This is reference data, deliberately OUTSIDE the live ledger: nothing here is
-- part of the hash-chained journal (journal_entries / journal_lines) and the
-- archive is never posted. Like the rest of Rentemester's audit data the rows
-- are append-only — re-importing creates a fresh batch row instead.
--
-- `import_archive_years` is the per-(source_system, fiscal_year) header; its
-- detail rows live in `import_archive_postings` (one per archived Posteringer
-- line) and `import_archive_balances` (one per archived SaldoBalance line).
-- Amounts are stored in KRONER (decimal), exactly as Dinero exports them.
CREATE TABLE IF NOT EXISTS import_archive_years (
  id INTEGER PRIMARY KEY,
  source_system TEXT NOT NULL,
  fiscal_year INTEGER NOT NULL,
  posting_count INTEGER NOT NULL DEFAULT 0 CHECK(posting_count >= 0),
  balance_count INTEGER NOT NULL DEFAULT 0 CHECK(balance_count >= 0),
  imported_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(source_system, fiscal_year)
);

CREATE TABLE IF NOT EXISTS import_archive_postings (
  id INTEGER PRIMARY KEY,
  archive_year_id INTEGER NOT NULL,
  line_no INTEGER NOT NULL CHECK(line_no >= 0),
  account_no TEXT NOT NULL,
  account_name TEXT,
  transaction_date TEXT,
  voucher TEXT,
  voucher_type TEXT,
  text TEXT,
  vat_type TEXT,
  amount NUMERIC NOT NULL,
  running_balance NUMERIC,
  FOREIGN KEY(archive_year_id) REFERENCES import_archive_years(id)
);

CREATE TABLE IF NOT EXISTS import_archive_balances (
  id INTEGER PRIMARY KEY,
  archive_year_id INTEGER NOT NULL,
  line_no INTEGER NOT NULL CHECK(line_no >= 0),
  account_no TEXT NOT NULL,
  account_name TEXT,
  amount NUMERIC NOT NULL,
  FOREIGN KEY(archive_year_id) REFERENCES import_archive_years(id)
);

CREATE INDEX IF NOT EXISTS idx_import_archive_postings_year
  ON import_archive_postings(archive_year_id);
CREATE INDEX IF NOT EXISTS idx_import_archive_balances_year
  ON import_archive_balances(archive_year_id);

CREATE TRIGGER IF NOT EXISTS import_archive_years_no_update
BEFORE UPDATE ON import_archive_years
BEGIN
  SELECT RAISE(ABORT, 'import archive years are append-only reference data; re-import creates a new batch instead');
END;

CREATE TRIGGER IF NOT EXISTS import_archive_years_no_delete
BEFORE DELETE ON import_archive_years
BEGIN
  SELECT RAISE(ABORT, 'import archive years are append-only reference data and cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS import_archive_postings_no_update
BEFORE UPDATE ON import_archive_postings
BEGIN
  SELECT RAISE(ABORT, 'import archive postings are append-only reference data');
END;

CREATE TRIGGER IF NOT EXISTS import_archive_postings_no_delete
BEFORE DELETE ON import_archive_postings
BEGIN
  SELECT RAISE(ABORT, 'import archive postings are append-only reference data and cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS import_archive_balances_no_update
BEFORE UPDATE ON import_archive_balances
BEGIN
  SELECT RAISE(ABORT, 'import archive balances are append-only reference data');
END;

CREATE TRIGGER IF NOT EXISTS import_archive_balances_no_delete
BEFORE DELETE ON import_archive_balances
BEGIN
  SELECT RAISE(ABORT, 'import archive balances are append-only reference data and cannot be deleted');
END;
-- ===== END IMPORT ARCHIVE (#197) =====

-- ===== IMPORT DOCUMENT LINKS (#196) =====
-- A Dinero export ships the actual receipts (`<year>/Bilag/<year>-Bilag-<n>`).
-- Each is ingested through the ordinary documents pipeline and must be linked
-- back to the journal entry its voucher (#195) was posted as.
--
-- `journal_entries` is append-only and locked (journal_entries_no_update), so
-- the entry's `document_id` cannot be set after the fact. Instead a posting and
-- its receipt are connected through THIS dedicated link table — additive, never
-- mutating a posted entry. One row per (document, journal entry) pair, carrying
-- the source voucher number (`Bilag`) the link was matched on for audit.
--
-- Like the rest of Rentemester's audit data the rows are append-only: a
-- re-import that re-ingests the same receipt by hash is a no-op (the documents
-- pipeline dedupes on sha256) and so produces no duplicate link.
CREATE TABLE IF NOT EXISTS import_document_links (
  id INTEGER PRIMARY KEY,
  source_system TEXT NOT NULL,
  voucher_ref TEXT NOT NULL,
  document_id INTEGER NOT NULL,
  journal_entry_id INTEGER NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(document_id, journal_entry_id),
  FOREIGN KEY(document_id) REFERENCES documents(id),
  FOREIGN KEY(journal_entry_id) REFERENCES journal_entries(id)
);

CREATE INDEX IF NOT EXISTS idx_import_document_links_entry
  ON import_document_links(journal_entry_id);

CREATE TRIGGER IF NOT EXISTS import_document_links_no_update
BEFORE UPDATE ON import_document_links
BEGIN
  SELECT RAISE(ABORT, 'import document links are append-only audit links');
END;

CREATE TRIGGER IF NOT EXISTS import_document_links_no_delete
BEFORE DELETE ON import_document_links
BEGIN
  SELECT RAISE(ABORT, 'import document links are append-only audit links and cannot be deleted');
END;
-- ===== END IMPORT DOCUMENT LINKS (#196) =====
-- ===== ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
-- Append-only register of periodeafgrænsningsposter: prepaid expenses
-- (forudbetalte omkostninger), accrued expenses (skyldige omkostninger) and
-- deferred revenue (forudbetalt indtægt). The `accruals` row is the header —
-- the initial balanced journal entry that parks the amount on a balance-sheet
-- accrual account. `accrual_schedule_postings` records each recognition period
-- as it is posted: a balanced journal entry that moves one period's slice off
-- the balance-sheet accrual account and onto the income-statement account.
-- Money is DKK with 2 decimals. Both tables are append-only — a wrong accrual
-- is corrected by reversing its journal entries, never by editing a row.
CREATE TABLE IF NOT EXISTS accruals (
  id INTEGER PRIMARY KEY,
  accrual_type TEXT NOT NULL CHECK(accrual_type IN ('prepaid_expense','accrued_expense','deferred_revenue')),
  description TEXT NOT NULL,
  total_amount NUMERIC NOT NULL CHECK(total_amount > 0),
  recognition_periods INTEGER NOT NULL CHECK(recognition_periods > 0),
  -- The balance-sheet account the amount is parked on between periods (an
  -- asset for prepaid expenses, a liability for accrued expenses / deferred
  -- revenue).
  balance_account_no TEXT NOT NULL,
  -- The income-statement account each period's slice is recognised on.
  result_account_no TEXT NOT NULL,
  -- The first recognition period (YYYY-MM-DD) and the month-step between
  -- periods, so the schedule is fully deterministic from the header alone.
  first_recognition_date TEXT NOT NULL,
  period_step_months INTEGER NOT NULL DEFAULT 1 CHECK(period_step_months > 0),
  document_id INTEGER,
  -- The journal entry that registered the accrual (parks it on the balance
  -- sheet). NULL only transiently inside the registering transaction.
  registration_journal_entry_id INTEGER UNIQUE,
  note TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(document_id) REFERENCES documents(id),
  FOREIGN KEY(registration_journal_entry_id) REFERENCES journal_entries(id)
);

CREATE TABLE IF NOT EXISTS accrual_schedule_postings (
  id INTEGER PRIMARY KEY,
  accrual_id INTEGER NOT NULL,
  period_index INTEGER NOT NULL CHECK(period_index > 0),
  recognition_date TEXT NOT NULL,
  amount NUMERIC NOT NULL CHECK(amount > 0),
  journal_entry_id INTEGER NOT NULL UNIQUE,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(accrual_id, period_index),
  FOREIGN KEY(accrual_id) REFERENCES accruals(id),
  FOREIGN KEY(journal_entry_id) REFERENCES journal_entries(id)
);

CREATE INDEX IF NOT EXISTS idx_accrual_schedule_postings_accrual ON accrual_schedule_postings(accrual_id);
CREATE INDEX IF NOT EXISTS idx_accruals_document ON accruals(document_id);

-- The registering transaction sets registration_journal_entry_id once, right
-- after the header row exists; that single transition is the only mutation an
-- accrual row may ever undergo. Everything else is append-only — a wrong
-- accrual is corrected by reversing its journal entries.
CREATE TRIGGER IF NOT EXISTS accruals_no_update
BEFORE UPDATE ON accruals
WHEN NOT (OLD.registration_journal_entry_id IS NULL AND NEW.registration_journal_entry_id IS NOT NULL
          AND OLD.id = NEW.id AND OLD.accrual_type = NEW.accrual_type
          AND OLD.description = NEW.description AND OLD.total_amount = NEW.total_amount
          AND OLD.recognition_periods = NEW.recognition_periods
          AND OLD.balance_account_no = NEW.balance_account_no
          AND OLD.result_account_no = NEW.result_account_no
          AND OLD.first_recognition_date = NEW.first_recognition_date
          AND OLD.period_step_months = NEW.period_step_months)
BEGIN
  SELECT RAISE(ABORT, 'accruals are append-only; reverse the journal entries to correct an accrual');
END;

CREATE TRIGGER IF NOT EXISTS accruals_no_delete
BEFORE DELETE ON accruals
BEGIN
  SELECT RAISE(ABORT, 'accruals are append-only; reverse the journal entries to correct an accrual');
END;

CREATE TRIGGER IF NOT EXISTS accrual_schedule_postings_no_update
BEFORE UPDATE ON accrual_schedule_postings
BEGIN
  SELECT RAISE(ABORT, 'accrual schedule postings are append-only; reverse the journal entry instead');
END;

CREATE TRIGGER IF NOT EXISTS accrual_schedule_postings_no_delete
BEFORE DELETE ON accrual_schedule_postings
BEGIN
  SELECT RAISE(ABORT, 'accrual schedule postings are append-only; reverse the journal entry instead');
END;
-- ===== END ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
-- ===== BUDGET (budget per konto pr. periode) =====
-- A budget line is an append-only revision: setting a budget for an
-- (account_no, period) pair inserts a NEW row; the row with the highest id for
-- that pair is the effective budget. Nothing is mutated or deleted, so the
-- full history of every budget change is preserved for audit — the same
-- append-only discipline the ledger and recurring-invoice templates follow.
--
-- `period` is a calendar month in YYYY-MM form. `amount` is a kroner figure in
-- the account's natural sign convention (a 5000 expense budget, a 20000 income
-- target) — never negative. The budget-vs-actual report and the liquidity
-- forecast both read the effective line.
CREATE TABLE IF NOT EXISTS budget_lines (
  id INTEGER PRIMARY KEY,
  account_no TEXT NOT NULL,
  period TEXT NOT NULL,
  amount NUMERIC NOT NULL CHECK(amount >= 0),
  notes TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(account_no) REFERENCES accounts(account_no)
);

CREATE INDEX IF NOT EXISTS idx_budget_lines_account_period
  ON budget_lines(account_no, period, id);

CREATE TRIGGER IF NOT EXISTS budget_lines_no_update
BEFORE UPDATE ON budget_lines
BEGIN
  SELECT RAISE(ABORT, 'budget lines are append-only; insert a new revision instead');
END;

CREATE TRIGGER IF NOT EXISTS budget_lines_no_delete
BEFORE DELETE ON budget_lines
BEGIN
  SELECT RAISE(ABORT, 'budget lines are append-only and cannot be deleted; insert a new revision instead');
END;
-- ===== END BUDGET =====

-- ===== PAYABLES / KREDITORSTYRING =====
-- The accounts-payable open-item register, symmetric to the debitor side.
-- A registered supplier bill (`payables`) is an open item that owes money to
-- a creditor: it carries a due date and is recognised in the ledger as a
-- balanced journal entry (debit expense + købsmoms, credit 7000 Leverandørgæld).
-- Outgoing bank payments are matched against the open item in `payable_payments`
-- (debit 7000 Leverandørgæld, credit bank). The open balance is gross amount
-- minus the sum of applied payments. Both tables are append-only audit data:
-- corrections go through journal reversals and a fresh payment application,
-- never an UPDATE/DELETE — exactly like invoice_payments on the debitor side.
CREATE TABLE IF NOT EXISTS payables (
  id INTEGER PRIMARY KEY,
  document_id INTEGER NOT NULL UNIQUE,
  vendor_id INTEGER,
  supplier_name TEXT,
  bill_no TEXT,
  bill_date TEXT NOT NULL,
  due_date TEXT NOT NULL,
  gross_amount NUMERIC NOT NULL CHECK(gross_amount > 0),
  net_amount NUMERIC NOT NULL CHECK(net_amount >= 0),
  vat_amount NUMERIC NOT NULL CHECK(vat_amount >= 0),
  currency TEXT NOT NULL DEFAULT 'DKK',
  journal_entry_id INTEGER NOT NULL UNIQUE,
  note TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(document_id) REFERENCES documents(id),
  FOREIGN KEY(vendor_id) REFERENCES vendors(id),
  FOREIGN KEY(journal_entry_id) REFERENCES journal_entries(id)
);

CREATE TABLE IF NOT EXISTS payable_payments (
  id INTEGER PRIMARY KEY,
  payable_id INTEGER NOT NULL,
  bank_transaction_id INTEGER,
  journal_entry_id INTEGER NOT NULL UNIQUE,
  payment_date TEXT NOT NULL,
  amount NUMERIC NOT NULL CHECK(amount > 0),
  currency TEXT NOT NULL DEFAULT 'DKK',
  note TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  FOREIGN KEY(payable_id) REFERENCES payables(id),
  FOREIGN KEY(bank_transaction_id) REFERENCES bank_transactions(id),
  FOREIGN KEY(journal_entry_id) REFERENCES journal_entries(id)
);

CREATE INDEX IF NOT EXISTS idx_payables_document ON payables(document_id);
CREATE INDEX IF NOT EXISTS idx_payable_payments_payable ON payable_payments(payable_id);

CREATE TRIGGER IF NOT EXISTS payables_no_update
BEFORE UPDATE ON payables
BEGIN
  SELECT RAISE(ABORT, 'payables are append-only; reverse the journal entry instead');
END;

CREATE TRIGGER IF NOT EXISTS payables_no_delete
BEFORE DELETE ON payables
BEGIN
  SELECT RAISE(ABORT, 'payables are append-only; reverse the journal entry instead');
END;

CREATE TRIGGER IF NOT EXISTS payable_payments_no_update
BEFORE UPDATE ON payable_payments
BEGIN
  SELECT RAISE(ABORT, 'payable payments are append-only; add a correcting payment application instead');
END;

CREATE TRIGGER IF NOT EXISTS payable_payments_no_delete
BEFORE DELETE ON payable_payments
BEGIN
  SELECT RAISE(ABORT, 'payable payments are append-only; add a correcting payment application instead');
END;
-- ===== END PAYABLES / KREDITORSTYRING =====
