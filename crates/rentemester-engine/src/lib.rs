//! Pure accounting values and read projections. No filesystem, SQL or transport.
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

pub mod mileage;
