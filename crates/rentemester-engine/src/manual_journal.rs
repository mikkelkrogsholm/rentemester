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
    validate_policy(request, facts, rules, expected_version, false, false)
}
/// Internal-use pure reversal validation. Origin facts must be loaded from the original
/// under the storage writer. This is not a request DTO or an authorization grant.
pub struct ReversalOrigin<'a> {
    pub created_by_program: &'a str,
    pub document_id: Option<DocumentId>,
}
pub fn validate_reversal(
    request: &ManualJournal,
    facts: &JournalFacts,
    rules: &ManualJournalRules,
    expected_version: &str,
    origin: ReversalOrigin<'_>,
) -> Result<Validation, Error> {
    rules.require_reversal()?;
    let historical =
        origin.created_by_program == "rentemester-import-postings" && origin.document_id.is_none();
    if request.document_id != origin.document_id
        || request.created_by_program.as_deref()
            != historical.then_some("rentemester-import-postings")
    {
        return Err(Error::new(
            "invalid_context",
            "reversal origin does not match derived request",
        ));
    }
    let mut result = validate_policy(request, facts, rules, expected_version, historical, true)?;
    let mut required = vec![journal_rules::APPEND_ONLY, journal_rules::REVERSAL];
    for id in result.required_rules {
        if !required.contains(&id) {
            required.push(id);
        }
    }
    result.required_rules = required;
    Ok(result)
}
/// Pure advisory policy for a source-verified import. Only the internal storage
/// import writer can establish the required immutable physical provenance.
/// This function grants no authorization and normal manual admission stays strict.
pub fn validate_historical_import(
    request: &ManualJournal,
    facts: &JournalFacts,
    rules: &ManualJournalRules,
    expected_version: &str,
) -> Result<Validation, Error> {
    if request.created_by_program.as_deref() != Some("rentemester-import-postings")
        || request.document_id.is_some()
        || request.source_bank_transaction_id.is_some()
        || request.currency.as_str() != "DKK"
    {
        return Err(Error::new(
            "invalid_context",
            "historical import needs the derived DKK source adapter context",
        ));
    }
    validate_policy(request, facts, rules, expected_version, true, true)
}
fn validate_policy(
    request: &ManualJournal,
    facts: &JournalFacts,
    rules: &ManualJournalRules,
    expected_version: &str,
    historical_import: bool,
    allow_uncoded_vat_control: bool,
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
    if !historical_import
        && request.created_by_program.as_deref() == Some("rentemester-import-postings")
    {
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
    let valid_vat_base = has_base || (touches_settlement && only_vat);
    if !allow_uncoded_vat_control && touches_control && !valid_vat_base {
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
            None if !historical_import => push(
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

#[cfg(test)]
mod reversal_tests {
    use super::*;
    use journal_rules::RuleBundle;
    #[test]
    fn source_bound_reversal_policy_does_not_weaken_manual_admission() {
        let bundles = vec![
            RuleBundle {
                name: "bookkeeping".to_owned(),
                version: "dk-bookkeeping-v0.0.8".to_owned(),
                rule_ids: [
                    journal_rules::BALANCED,
                    journal_rules::APPEND_ONLY,
                    journal_rules::PERIOD_LOCK,
                    journal_rules::DOCUMENT,
                    journal_rules::FX,
                    journal_rules::REVERSAL,
                ]
                .map(str::to_owned)
                .to_vec(),
            },
            RuleBundle {
                name: "vat".to_owned(),
                version: "dk-vat-v0.0.5".to_owned(),
                rule_ids: vec![],
            },
        ];
        let rules = ManualJournalRules::from_bundles(bundles.clone()).unwrap();
        let request = ManualJournal {
            transaction_date: Date::parse("2026-01-01").unwrap(),
            text: "Derived reversal".to_owned(),
            currency: Currency::parse("DKK").unwrap(),
            document_id: None,
            source_bank_transaction_id: None,
            amount_foreign: None,
            amount_dkk: None,
            fx_rate_to_dkk: None,
            created_by_program: Some("rentemester-import-postings".to_owned()),
            lines: vec![
                JournalLine {
                    account_no: AccountNo::parse("3000").unwrap(),
                    debit_amount: Money::parse("10").unwrap(),
                    credit_amount: Money::default(),
                    vat_code: None,
                    text: None,
                },
                JournalLine {
                    account_no: AccountNo::parse("4000").unwrap(),
                    debit_amount: Money::default(),
                    credit_amount: Money::parse("10").unwrap(),
                    vat_code: None,
                    text: None,
                },
            ],
        };
        let facts = JournalFacts {
            transaction_date: request.transaction_date,
            latest_allowed_date: request.transaction_date,
            accounts: vec![
                AccountFact {
                    account_no: AccountNo::parse("3000").unwrap(),
                    kind: AccountKind::Expense,
                    active: true,
                    vat_control: false,
                    vat_settlement: false,
                },
                AccountFact {
                    account_no: AccountNo::parse("4000").unwrap(),
                    kind: AccountKind::Vat,
                    active: true,
                    vat_control: true,
                    vat_settlement: false,
                },
            ],
            documents: vec![],
            banks: vec![],
            periods: vec![],
        };
        let manual = validate(&request, &facts, &rules, rules.version()).unwrap();
        assert_eq!(
            manual.errors.iter().map(|e| e.code).collect::<Vec<_>>(),
            vec!["reserved_program", "vat_base_required", "document_required"]
        );
        let imported =
            validate_historical_import(&request, &facts, &rules, rules.version()).unwrap();
        assert!(imported.ok);
        assert!(imported.required_rules.contains(&journal_rules::DOCUMENT));
        let reversal = validate_reversal(
            &request,
            &facts,
            &rules,
            rules.version(),
            ReversalOrigin {
                created_by_program: "rentemester-import-postings",
                document_id: None,
            },
        )
        .unwrap();
        assert!(reversal.ok);
        assert!(reversal.required_rules.contains(&journal_rules::DOCUMENT));
        assert_eq!(
            reversal.required_rules[..2],
            [journal_rules::APPEND_ONLY, journal_rules::REVERSAL]
        );
        assert!(
            validate_reversal(
                &request,
                &facts,
                &rules,
                rules.version(),
                ReversalOrigin {
                    created_by_program: "fake-import",
                    document_id: None
                }
            )
            .is_err()
        );
        let mut incomplete = bundles;
        incomplete[0]
            .rule_ids
            .retain(|s| s != journal_rules::REVERSAL);
        let incomplete = ManualJournalRules::from_bundles(incomplete).unwrap();
        assert!(validate(&request, &facts, &incomplete, incomplete.version()).is_ok());
        assert!(
            validate_reversal(
                &request,
                &facts,
                &incomplete,
                incomplete.version(),
                ReversalOrigin {
                    created_by_program: "rentemester-import-postings",
                    document_id: None
                }
            )
            .is_err()
        );
    }
}
