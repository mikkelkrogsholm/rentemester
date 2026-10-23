//! Original VAT purchase contracts; allocation and persistence remain native.
use crate::{expense, idempotency::PrincipalKind};
use rentemester_engine::{
    Error, currency::Currency, document::money, fiscal_year::Date, fx::FxRate,
    identifiers::DocumentId, manual_journal::ManualJournal, money::Money,
};
use rusqlite::{OptionalExtension, Transaction};
use serde::Deserialize;
use serde_json::{Number, Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    EuService,
    Representation,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub transaction_date: String,
    pub text: String,
    pub net_amount: Number,
    pub document_id: Option<i64>,
    pub invoice_no: Option<String>,
    pub expense_account_no: Option<String>,
    pub payment_account_no: Option<String>,
    pub source_bank_transaction_id: Option<i64>,
    pub currency: Option<String>,
    pub amount_foreign: Option<Number>,
    pub amount_dkk: Option<Number>,
    pub fx_rate_to_dkk: Option<Number>,
}
fn fail(message: impl Into<String>) -> Error {
    Error::new("vat_purchase_refused", message)
}
fn sql(_: rusqlite::Error) -> Error {
    fail("VAT purchase source evidence unavailable; abort transaction")
}
fn positive_id(id: i64) -> Result<i64, Error> {
    if !(1..=9_007_199_254_740_991).contains(&id) {
        return Err(fail(
            "document and bank references must be positive safe integers",
        ));
    }
    Ok(id)
}
/// Resolve inside the admitted company transaction. Invoice selectors are exact,
/// newest-first, and intentionally override a supplied document reference.
pub fn resolve_document(
    tx: &Transaction<'_>,
    input: &Input,
    operation: Operation,
) -> Result<i64, Error> {
    if let Some(id) = input.document_id {
        positive_id(id)?;
    }
    if let Some(id) = input.source_bank_transaction_id {
        positive_id(id)?;
    }
    if operation == Operation::Representation && input.invoice_no.is_some() {
        return Err(fail(
            "invoiceNo is not supported for representation purchases",
        ));
    }
    if operation == Operation::EuService {
        if let Some(no) = input
            .invoice_no
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return tx
                .query_row(
                    "SELECT id FROM documents WHERE invoice_no=?1 ORDER BY id DESC LIMIT 1",
                    [no],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql)?
                .ok_or_else(|| fail(format!("Could not resolve document for invoiceNo {no}")))
                .and_then(positive_id);
        }
    }
    positive_id(
        input
            .document_id
            .ok_or_else(|| fail("documentId must be a positive safe integer"))?,
    )
}
fn expense_account(
    tx: &Transaction<'_>,
    input: &Input,
    operation: Operation,
) -> Result<String, Error> {
    if let Some(no) = &input.expense_account_no {
        if no.trim().is_empty() {
            return Err(fail("expenseAccountNo is required"));
        }
        return Ok(no.trim().to_owned());
    }
    if operation == Operation::EuService {
        return Err(fail("expenseAccountNo is required"));
    }
    // No chart-number/name fallback: omission means an explicit unique mapping.
    let mut q = tx.prepare("SELECT account_no FROM accounts WHERE active=1 AND type='expense' AND default_vat_code='REPRESENTATION_SPECIAL' ORDER BY account_no").map_err(sql)?;
    let accounts = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    if accounts.len() != 1 {
        return Err(fail(
            "provide explicit expenseAccountNo: a unique active REPRESENTATION_SPECIAL expense account must be configured",
        ));
    }
    Ok(accounts[0].clone())
}
fn exact(n: &Number) -> Result<Money, Error> {
    let m = money(n)?;
    if crate::journal_post::money(&m)?
        != n.as_f64()
            .ok_or_else(|| fail("exact numeric amount required"))?
    {
        return Err(fail("amount must be exact cents"));
    }
    Ok(m)
}
fn prepare(
    tx: &Transaction<'_>,
    input: &Input,
    operation: Operation,
    now: &str,
) -> Result<ManualJournal, Error> {
    let id = resolve_document(tx, input, operation)?;
    let date = Date::parse(&input.transaction_date)?;
    if input.text.trim().is_empty() {
        return Err(fail("text is required"));
    }
    let net = exact(&input.net_amount)?;
    if net.is_negative() || net == Money::default() {
        return Err(fail("netAmount must be a positive number"));
    }
    let registered: bool = tx
        .query_row(
            "SELECT vat_period_type IS NOT NULL FROM companies WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if !registered {
        return Err(fail("VAT registration required for VAT purchase"));
    }
    let used: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_entries WHERE document_id=?1) OR EXISTS(SELECT 1 FROM payables WHERE document_id=?1)", [id], |r| r.get(0)).map_err(sql)?;
    if used {
        return Err(fail(
            "document is already posted or registered as a payable",
        ));
    }
    crate::invoice_extraction::require_completed_or_absent(tx, id)?;
    let meta = expense::metadata(tx, id)?;
    if meta.kind() == "internal_voucher" || meta.exemption_code.is_some() {
        return Err(fail("valid original purchase invoice required"));
    }
    if meta.purchase_vat_lines.is_some() {
        return Err(fail("VAT purchase does not support purchaseVatLines"));
    }
    if operation == Operation::EuService {
        let supplier = crate::purchase_preflight::supplier(tx, id)?
            .ok_or_else(|| fail("resolved persisted EU supplier required"))?;
        if supplier.kind != "eu_vat" || supplier.country == "DK" {
            return Err(fail(
                "EU service purchase requires a non-Danish EU supplier; domestic and non-EU suppliers are not supported",
            ));
        }
        crate::vies::require(
            tx,
            supplier
                .identifier
                .as_deref()
                .ok_or_else(|| fail("EU VAT identifier required"))?,
            now,
        )?;
    }
    if meta
        .issue_date
        .as_deref()
        .is_none_or(|d| d > input.transaction_date.as_str())
    {
        return Err(fail(
            "purchase date must not precede immutable invoice date",
        ));
    }
    let expense = expense_account(tx, input, operation)?;
    let treatment = if operation == Operation::EuService {
        "reverse_charge"
    } else {
        "representation"
    };
    let currency = input
        .currency
        .as_deref()
        .unwrap_or("DKK")
        .trim()
        .to_uppercase();
    Currency::parse(&currency)?;
    if currency != meta.currency() {
        return Err(fail("currency must match immutable original"));
    }
    let gross = meta
        .amount_inc_vat
        .as_ref()
        .map(exact)
        .transpose()?
        .ok_or_else(|| fail("document gross amount required"))?;
    if input
        .payment_account_no
        .as_deref()
        .is_some_and(|s| s.trim().is_empty())
    {
        return Err(fail("paymentAccountNo must not be blank"));
    }
    if let Some(bank) = input.source_bank_transaction_id {
        let (bank_date, bound): (String, Option<String>) = tx.query_row(
            "SELECT bt.transaction_date,ba.ledger_account_no FROM bank_transactions bt LEFT JOIN bank_accounts ba ON ba.id=bt.bank_account_id WHERE bt.id=?1",
            [bank], |r| Ok((r.get(0)?, r.get(1)?))).map_err(sql)?;
        let selected = input
            .payment_account_no
            .as_deref()
            .map(str::trim)
            .map(str::to_owned)
            .map(Ok)
            .unwrap_or_else(|| expense::role(tx, "bank"))?;
        if bound.as_deref().is_some_and(|no| no != selected) {
            return Err(fail(
                "payment account conflicts with imported bank identity",
            ));
        }
        if currency != "DKK" && bank_date != input.transaction_date {
            return Err(fail("FX purchase must use dated bank valuation"));
        }
    }
    let mut request = if let Some(bank) = input.source_bank_transaction_id {
        // Keep all existing imported-bank amount, FX, role and reconciliation guards.
        expense::prepare_at(
            tx,
            &expense::Input {
                document_id: id,
                bank_transaction_id: bank,
                expense_account_no: expense,
                vat_treatment: Some(treatment.into()),
                payment_account_no: Some(
                    input
                        .payment_account_no
                        .clone()
                        .map(Ok)
                        .unwrap_or_else(|| expense::role(tx, "bank"))?,
                ),
                transaction_date: Some(input.transaction_date.clone()),
                text: Some(input.text.clone()),
            },
            now,
        )?
        .request
    } else {
        let rate = if currency == "DKK" {
            FxRate::parse("1")?
        } else {
            FxRate::parse(
                &input
                    .fx_rate_to_dkk
                    .as_ref()
                    .ok_or_else(|| fail("non-DKK purchase requires explicit FX evidence"))?
                    .to_string(),
            )?
        };
        let gross_dkk = rate.apply(&gross);
        let allocated = expense::purchase_at(
            tx,
            id,
            &meta,
            &expense,
            Some(treatment),
            &gross_dkk,
            &rate,
            Some(now),
        )?;
        let payment = match input.payment_account_no.as_deref() {
            Some(no) if !no.trim().is_empty() => no.trim().to_owned(),
            Some(_) => return Err(fail("paymentAccountNo must not be blank")),
            None => expense::role(tx, "bank")?,
        };
        let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND type='asset' AND active=1)", [&payment], |r| r.get(0)).map_err(sql)?;
        if !valid {
            return Err(fail("payment account must be an active asset account"));
        }
        let mut lines = allocated.lines;
        let payment_line = expense::line(
            &payment,
            Money::default(),
            gross_dkk.clone(),
            None,
            "Payment / liability".into(),
        )?;
        if operation == Operation::EuService {
            lines.insert(2, payment_line);
        } else {
            lines.push(payment_line);
        }
        ManualJournal {
            transaction_date: date,
            text: input.text.trim().into(),
            currency: Currency::parse(&currency)?,
            document_id: Some(DocumentId::from_sqlite(id)),
            source_bank_transaction_id: None,
            amount_foreign: (currency != "DKK").then(|| gross.clone()),
            amount_dkk: (currency != "DKK").then(|| gross_dkk.clone()),
            fx_rate_to_dkk: (currency != "DKK").then(|| rate.clone()),
            created_by_program: None,
            lines,
        }
    };
    let rate = request
        .fx_rate_to_dkk
        .clone()
        .unwrap_or(FxRate::parse("1")?);
    let gross_dkk = request.amount_dkk.clone().unwrap_or(gross.clone());
    let allocation = expense::purchase_at(
        tx,
        id,
        &meta,
        request.lines[0].account_no.as_str(),
        Some(treatment),
        &gross_dkk,
        &rate,
        Some(now),
    )?;
    if allocation.net != net {
        return Err(fail(
            "netAmount must match immutable original DKK net amount",
        ));
    }
    for (provided, expected) in [
        (&input.amount_foreign, &gross),
        (&input.amount_dkk, &gross_dkk),
    ] {
        if let Some(n) = provided {
            if exact(n)? != *expected {
                return Err(fail(
                    "FX amounts must match original and documented payment",
                ));
            }
        }
    }
    if currency != "DKK" && (input.amount_foreign.is_none() || input.amount_dkk.is_none()) {
        return Err(fail(
            "non-DKK purchase requires amountForeign and amountDkk",
        ));
    }
    if let Some(n) = &input.fx_rate_to_dkk {
        if FxRate::parse(&n.to_string())? != rate {
            return Err(fail("FX rate must match documented payment"));
        }
    }
    // Shared expense order is expense/input/output/payment; preserve source VAT ABI order.
    if operation == Operation::EuService && input.source_bank_transaction_id.is_some() {
        request.lines.swap(2, 3);
    }
    Ok(request)
}
/// Every Err requires rollback. Core false remains a real native no-effect result.
pub fn post(
    tx: &Transaction<'_>,
    input: &Input,
    operation: Operation,
    subject: &str,
    latest: Date,
    principal: PrincipalKind,
    now: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if subject.trim().is_empty() || subject.len() > 256 || subject.chars().any(char::is_control) {
        return Err(fail("authenticated stable principal required"));
    }
    let request = prepare(tx, input, operation, now)?;
    let (prefix, program) = match principal {
        PrincipalKind::User => ("user", "rentemester-rust"),
        PrincipalKind::ServiceAccount => ("service_account", "service-principal"),
    };
    let actor = format!("{prefix}:{subject}");
    let mut result = crate::journal_post::post(
        tx,
        &request,
        &crate::journal_preview::manual_journal_rule_version()?,
        latest,
        crate::journal_post::Actor {
            subject: &actor,
            program,
        },
    )?;
    let rule = if operation == Operation::EuService {
        "DK-VAT-REVERSE-CHARGE-001"
    } else {
        "DK-VAT-REPRESENTATION-001"
    };
    let mut rules = result["appliedRules"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if !rules.iter().any(|v| v == rule) {
        if result["ok"] == true {
            rules.push(json!(rule));
        } else {
            rules.insert(0, json!(rule));
        }
    }
    result["appliedRules"] = json!(rules);
    Ok(result)
}
#[cfg(test)]
mod tests;
