//! Wire and company-local policy for the existing explicit bookkeeping loop.
//! The coordinator and authoritative writers live outside the pure engine.
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};

pub const PROGRAM: &str = "rentemester-runtime-agent";
pub const RULE: &str = "DK-RUNTIME-AGENT-001";
pub const PHASES: [&str; 7] = [
    "ingest",
    "book",
    "route",
    "payables",
    "reconcile",
    "deadlines",
    "report",
];
pub const AUTO_BOOK_CONFIDENCE: f64 = 0.65;
pub const FIXED_ASSET_REVIEW_DKK: i64 = 5000;
pub const DEADLINE_HORIZON_DAYS: i64 = 45;

/// Explicit trusted operator configuration per company. No built-in account mappings.
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Policy {
    #[serde(default)]
    pub supplier_rules: Vec<SupplierRule>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SupplierRule {
    pub token: String,
    pub expense_account: String,
    pub vat_treatment: String,
    pub label: String,
    pub asset_like: bool,
}

/// Local CLI input is packaged as bytes/parsed metadata, never server file paths.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Request {
    pub as_of: String,
    pub confirm: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inbox: Option<Inbox>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bank: Option<BankInput>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Inbox {
    Files { items: Vec<InboxDocument> },
    Failed { error: String },
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InboxDocument {
    pub file_name: String,
    pub file_base64: String,
    /// Parsed source JSON; actual metadata validation belongs to the existing intake pipeline.
    pub metadata: Value,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum BankInput {
    Csv {
        #[serde(rename = "fileName")]
        file_name: String,
        csv: String,
    },
    Failed {
        error: String,
    },
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Report {
    pub ok: bool,
    pub actor: String,
    pub as_of: String,
    /// Canonical selected company slug; never a private server filesystem root.
    pub company: String,
    pub phases: Vec<String>,
    pub documents_ingested: u64,
    pub documents_rejected: u64,
    pub bank_transactions_imported: u64,
    pub expenses_booked: Vec<BookedExpense>,
    pub payables_matched: Vec<PayableMatch>,
    pub accrual_recognitions_due: u64,
    pub open_exceptions: Vec<RoutedException>,
    pub upcoming_deadlines: Vec<DeadlineNotice>,
    pub summary: Vec<String>,
    pub errors: Vec<String>,
}
impl Report {
    pub fn new(actor: String, company: String, as_of: String) -> Self {
        Self {
            ok: true,
            actor,
            company,
            as_of,
            phases: vec![],
            documents_ingested: 0,
            documents_rejected: 0,
            bank_transactions_imported: 0,
            expenses_booked: vec![],
            payables_matched: vec![],
            accrual_recognitions_due: 0,
            open_exceptions: vec![],
            upcoming_deadlines: vec![],
            summary: vec![],
            errors: vec![],
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BookedExpense {
    pub document_no: String,
    pub document_id: i64,
    pub bank_transaction_id: i64,
    pub supplier: String,
    pub amount: Number,
    pub currency: String,
    pub expense_account: String,
    pub vat_treatment: String,
    pub label: String,
    pub journal_entry_no: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PayableMatch {
    pub payable_id: i64,
    pub document_id: i64,
    pub bank_transaction_id: i64,
    pub supplier: String,
    pub amount: Number,
    pub journal_entry_no: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RoutedException {
    pub exception_id: i64,
    #[serde(rename = "type")]
    pub kind: String,
    pub severity: String,
    pub message: String,
    pub required_action: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DeadlineNotice {
    pub kind: String,
    pub period_start: String,
    pub period_end: String,
    pub due_date: String,
    pub days_remaining: i64,
    pub ready: bool,
    pub note: String,
}
#[derive(Clone)]
pub struct Proposal {
    pub kind: String,
    pub severity: String,
    pub message: String,
    pub required_action: Option<String>,
    pub related_bank_transaction_id: Option<i64>,
    pub related_document_id: Option<i64>,
    pub source_evidence: Value,
    pub posting_preview: Option<Value>,
}
pub struct ReviewSnapshot {
    pub accrual_recognitions_due: u64,
    pub upcoming_deadlines: Vec<DeadlineNotice>,
}
