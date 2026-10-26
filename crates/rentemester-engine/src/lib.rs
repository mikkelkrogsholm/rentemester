//! Pure accounting values and read projections. No filesystem, SQL or transport.
pub mod workspace_manifest;
pub mod invoice_validation;
pub mod invoice_compensation;
pub mod invoice_interest;
pub mod vies;
pub mod sales_fx;
pub mod public_einvoice;
pub mod public_einvoice_preview;
pub mod peppol_handoff;
pub mod mail_intake;
pub mod document_scan;
pub mod document_pdf;
pub mod invoice_extraction;
pub mod company_knowledge;
pub mod parties;
pub mod calendar;
pub mod currency;
mod decimal;
pub mod financial_statements;
pub mod fiscal_year;
pub mod fx;
pub mod identifiers;
pub mod journal_rules;
pub mod manual_journal;
pub mod money;
pub mod trial_balance;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub code: &'static str,
    pub message: String,
}
impl Error {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {}

pub mod document;

pub mod bank;

pub mod expense_fx;

pub mod vat_period;
pub mod vat_report;

pub mod invoice_pdf;
pub mod sales_invoice;

pub mod credit_note;

pub mod asset;

pub mod accrual;
pub mod mileage;

pub mod annual_report;

pub mod saft;
pub mod reminder_claims;

pub mod ownership;

pub mod intercompany;
pub mod intercompany_dispositions;

pub mod eliminations;
pub mod consolidation;

pub mod scheduled_recurring;

pub mod bookkeeping_batch;

pub mod contact_import;

pub mod cfo_analytics;

pub mod archive_import;
pub mod opening_balance;
pub mod archive_reports;

pub mod cashflow;
pub mod portfolio;

pub mod agent_discovery;

pub mod backup_policy;

pub mod dinero_import;
pub mod imported_receivables;

pub mod synthetic_import;

pub mod dashboard_html;

pub mod tax_return;

pub mod ixbrl;

pub mod regulatory;
pub mod compliance_report;

pub mod mileage_posting;

pub mod digisense_received;

/// Exact SHA-256 of original bytes. A digest does not attest provenance or grant authority.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

pub mod agent_run;
