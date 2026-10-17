//! Advisory pure manual preflight. Trusted facts grant no transaction or writer authority.
use crate::{
    Error,
    currency::Currency,
    fiscal_year::Date,
    fx::FxRate,
    identifiers::{AccountNo, BankTransactionId, DocumentId, JournalEntryId, PeriodId},
    journal_rules::{self, ManualJournalRules},
    money::Money,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JournalLine {
    pub account_no: AccountNo,
    #[serde(default)]
    pub debit_amount: Money,
    #[serde(default)]
    pub credit_amount: Money,
    pub vat_code: Option<String>,
    pub text: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManualJournal {
    pub transaction_date: Date,
    pub text: String,
    pub currency: Currency,
    pub document_id: Option<DocumentId>,
    pub source_bank_transaction_id: Option<BankTransactionId>,
    pub amount_foreign: Option<Money>,
    pub amount_dkk: Option<Money>,
    pub fx_rate_to_dkk: Option<FxRate>,
    pub created_by_program: Option<String>,
    pub lines: Vec<JournalLine>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Asset,
    Liability,
    Equity,
    Income,
    Expense,
    Vat,
}
impl AccountKind {
    fn requires_document(self) -> bool {
        matches!(self, Self::Income | Self::Expense)
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountFact {
    pub account_no: AccountNo,
    pub kind: AccountKind,
    pub active: bool,
    pub vat_control: bool,
    pub vat_settlement: bool,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentFact {
    pub id: DocumentId,
    pub existing_credit_note_entry: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BankFact {
    pub id: BankTransactionId,
    pub linked_journal: Option<JournalEntryId>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodKind {
    VatPeriod,
    VatQuarter,
    FiscalYear,
    Custom,
}
impl PeriodKind {
    fn name(self) -> &'static str {
        match self {
            Self::VatPeriod => "vat_period",
            Self::VatQuarter => "vat_quarter",
            Self::FiscalYear => "fiscal_year",
            Self::Custom => "custom",
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodState {
    Open,
    Closed,
    Reported,
}
impl PeriodState {
    fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Reported => "reported",
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PeriodFact {
    pub id: PeriodId,
    pub start: Date,
    pub end: Date,
    pub kind: PeriodKind,
    pub effective_state: PeriodState,
    pub reference: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JournalFacts {
    pub transaction_date: Date,
    pub latest_allowed_date: Date,
    pub accounts: Vec<AccountFact>,
    pub documents: Vec<DocumentFact>,
    pub banks: Vec<BankFact>,
    pub periods: Vec<PeriodFact>,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Violation {
    pub code: &'static str,
    pub field: String,
    pub message: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Validation {
    pub ok: bool,
    pub rule_version: String,
    pub required_rules: Vec<&'static str>,
    pub required_storage_rules: Vec<&'static str>,
    pub total_debit: Money,
    pub total_credit: Money,
    pub errors: Vec<Violation>,
}
// ECMAScript WhiteSpace + LineTerminator, not Rust Unicode White_Space (e.g.0085).
fn trim_js(value: &str) -> &str {
    value.trim_matches(|c| matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}'))
}
fn push(
    errors: &mut Vec<Violation>,
    code: &'static str,
    field: impl Into<String>,
    message: impl Into<String>,
) {
    errors.push(Violation {
        code,
        field: field.into(),
        message: message.into(),
    });
}
fn positive(m: &Money) -> bool {
    !m.is_negative() && m != &Money::default()
}
pub fn validate(
    request: &ManualJournal,
    facts: &JournalFacts,
    rules: &ManualJournalRules,
    expected_version: &str,
) -> Result<Validation, Error> {
    rules.require_version(expected_version)?;
    let invalid = || {
        Error::new(
            "invalid_context",
            "snapshot date, identities or period ranges are inconsistent",
        )
    };
    if request.transaction_date != facts.transaction_date {
        return Err(invalid());
    }
    let mut accounts = BTreeMap::new();
    let mut documents = BTreeMap::new();
    let mut banks = BTreeMap::new();
    let mut period_ids = BTreeSet::new();
    for a in &facts.accounts {
        if accounts.insert(&a.account_no, a).is_some() {
            return Err(invalid());
        }
    }
    for d in &facts.documents {
        if documents.insert(d.id, d).is_some() {
            return Err(invalid());
        }
    }
    for b in &facts.banks {
        if banks.insert(b.id, b).is_some() {
            return Err(invalid());
        }
    }
    for p in &facts.periods {
        if p.start > p.end || !period_ids.insert(p.id) {
            return Err(invalid());
        }
    }
    let mut errors = Vec::new();
    let mut required_rules = vec![
        journal_rules::BALANCED,
        journal_rules::APPEND_ONLY,
        journal_rules::PERIOD_LOCK,
    ];
    if trim_js(&request.text).is_empty() {
        push(&mut errors, "text_required", "text", "text is required");
    }
    if request.transaction_date > facts.latest_allowed_date {
        push(
            &mut errors,
            "future_date",
            "transactionDate",
            format!(
                "transactionDate {} cannot be later than {}",
                request.transaction_date, facts.latest_allowed_date
            ),
        );
    }
    if let Some(p) = facts
        .periods
        .iter()
        .filter(|p| {
            p.start <= request.transaction_date
                && p.end >= request.transaction_date
                && !matches!(p.effective_state, PeriodState::Open)
        })
        .min_by_key(|p| (p.end, p.id))
    {
        let reference = p
            .reference
            .as_ref()
            .filter(|s| !s.is_empty())
            .map(|s| format!(" ref {s}"))
            .unwrap_or_default();
        push(
            &mut errors,
            "period_locked",
            "transactionDate",
            format!(
                "transactionDate {} falls in {} period {} {}..{}{reference}",
                request.transaction_date,
                p.effective_state.name(),
                p.kind.name(),
                p.start,
                p.end
            ),
        );
    }
    if request.lines.len() < 2 {
        push(
            &mut errors,
            "lines_required",
            "lines",
            "at least two journal lines are required",
        );
    }
    if request.created_by_program.as_deref() == Some("rentemester-import-postings") {
        push(
            &mut errors,
            "reserved_program",
            "createdByProgram",
            "createdByProgram 'rentemester-import-postings' is reserved for the verified historical-import adapter",
        );
    }
    let mut debit_sum = Money::default();
    let mut credit_sum = Money::default();
    let mut requires_document = false;
    let mut touches_control = false;
    let mut has_base = false;
    let mut touches_settlement = false;
    let mut only_vat = true;
    for (index, line) in request.lines.iter().enumerate() {
        let field = format!("lines[{index}]");
        let Some(account) = accounts.get(&line.account_no) else {
            push(
                &mut errors,
                "account_missing",
                format!("{field}.accountNo"),
                format!("{field}.accountNo must reference an existing account"),
            );
            continue;
        };
        touches_control |= account.vat_control;
        touches_settlement |= account.vat_settlement;
        only_vat &= account.vat_control || account.vat_settlement;
        if let Some(code) = line
            .vat_code
            .as_deref()
            .map(trim_js)
            .filter(|s| !s.is_empty())
        {
            if !journal_rules::VAT_CODES.contains(&code) {
                push(
                    &mut errors,
                    "vat_code_unsupported",
                    format!("{field}.vatCode"),
                    format!("{field}.vatCode '{code}' is not a supported VAT code"),
                );
            } else if account.kind.requires_document() {
                has_base = true;
            }
        }
        if !account.active {
            push(
                &mut errors,
                "account_inactive",
                format!("{field}.accountNo"),
                format!("{field}.accountNo refers to an inactive account"),
            );
        }
        if line.debit_amount.is_negative() || line.credit_amount.is_negative() {
            push(
                &mut errors,
                "negative_amount",
                &field,
                format!("{field} debit and credit amounts must not be negative"),
            );
        } else if positive(&line.debit_amount) == positive(&line.credit_amount) {
            push(
                &mut errors,
                "line_side",
                &field,
                format!("{field} must have either debitAmount or creditAmount"),
            );
        }
        debit_sum.add(&line.debit_amount);
        credit_sum.add(&line.credit_amount);
        requires_document |= account.kind.requires_document();
    }
    if debit_sum != credit_sum {
        push(
            &mut errors,
            "unbalanced",
            "lines",
            format!(
                "journal entry must balance: debit {} != credit {}",
                debit_sum.decimal(),
                credit_sum.decimal()
            ),
        );
    }
    if touches_control && !has_base && !(touches_settlement && only_vat) {
        push(
            &mut errors,
            "vat_base_required",
            "lines",
            "manual journal entries that affect a VAT amount account require an explicit vatCode on the VAT base line (for example DK_SALE_25, DK_PURCHASE_25, EU_SERVICE_REVERSE_CHARGE, or REPRESENTATION_SPECIAL)",
        );
    }
    if request.currency.as_str() != "DKK" {
        required_rules.push(journal_rules::FX);
        let zero = Money::default();
        let foreign = request.amount_foreign.as_ref().unwrap_or(&zero);
        let dkk = request.amount_dkk.as_ref().unwrap_or(&zero);
        if !positive(foreign) {
            push(
                &mut errors,
                "foreign_amount_required",
                "amountForeign",
                "amountForeign must be positive for non-DKK journal entries",
            );
        }
        if !positive(dkk) {
            push(
                &mut errors,
                "dkk_amount_required",
                "amountDkk",
                "amountDkk must be positive for non-DKK journal entries",
            );
        }
        if request.fx_rate_to_dkk.is_none() {
            push(
                &mut errors,
                "fx_rate_required",
                "fxRateToDkk",
                "fxRateToDkk must be positive for non-DKK journal entries",
            );
        }
        if let Some(rate) = request
            .fx_rate_to_dkk
            .as_ref()
            .filter(|_| positive(foreign))
        {
            let expected = rate.apply(foreign);
            if dkk != &expected {
                push(
                    &mut errors,
                    "fx_amount_mismatch",
                    "amountDkk",
                    format!(
                        "amountDkk must equal amountForeign * fxRateToDkk ({})",
                        expected.decimal()
                    ),
                );
            }
        }
    }
    if requires_document {
        required_rules.push(journal_rules::DOCUMENT);
        match request.document_id {
            None => push(
                &mut errors,
                "document_required",
                "documentId",
                "documentId is required when posting expense or income lines",
            ),
            Some(id) if !documents.contains_key(&id) => push(
                &mut errors,
                "document_missing",
                "documentId",
                format!("documentId {id} does not exist"),
            ),
            _ => {}
        }
    }
    if let Some((id, entry)) = request.document_id.and_then(|id| {
        documents
            .get(&id)
            .and_then(|d| d.existing_credit_note_entry.as_ref())
            .map(|entry| (id, entry))
    }) {
        push(
            &mut errors,
            "credit_note_already_posted",
            "documentId",
            format!(
                "credit note document {id} already has accounting journal {entry}; issue a new credit note document for another correction"
            ),
        );
    }
    if let Some(id) = request.source_bank_transaction_id {
        match banks.get(&id) {
            None => push(
                &mut errors,
                "bank_missing",
                "sourceBankTransactionId",
                format!("sourceBankTransactionId {id} does not exist"),
            ),
            Some(b) => {
                if let Some(link) = b.linked_journal {
                    push(
                        &mut errors,
                        "bank_reconciled",
                        "sourceBankTransactionId",
                        format!(
                            "sourceBankTransactionId {id} is already reconciled to journal entry {link}"
                        ),
                    );
                }
            }
        }
    }
    Ok(Validation {
        ok: errors.is_empty(),
        rule_version: rules.version().to_owned(),
        required_rules,
        required_storage_rules: vec![journal_rules::APPEND_ONLY],
        total_debit: debit_sum,
        total_credit: credit_sum,
        errors,
    })
}
