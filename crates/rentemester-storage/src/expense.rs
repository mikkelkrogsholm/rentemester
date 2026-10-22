//! Purchase voucher booking uses the same immutable native journal writer.
use rentemester_engine::{
    Error,
    currency::Currency,
    document::{Metadata, money},
    expense_fx::{self, Payment},
    fiscal_year::Date,
    fx::FxRate,
    identifiers::{AccountNo, BankTransactionId, DocumentId},
    manual_journal::{JournalLine, ManualJournal},
    money::Money,
};
use rusqlite::{OptionalExtension, Transaction};
use serde::Deserialize;
use serde_json::{Value, json};
fn fail(message: impl Into<String>) -> Error {
    Error::new("expense_refused", message)
}
fn sql(_: rusqlite::Error) -> Error {
    fail("expense source evidence unavailable; abort transaction")
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub document_id: i64,
    pub bank_transaction_id: i64,
    pub expense_account_no: String,
    pub vat_treatment: Option<String>,
    pub payment_account_no: Option<String>,
    pub transaction_date: Option<String>,
    pub text: Option<String>,
}
pub(crate) fn role(tx: &Transaction<'_>, role: &str) -> Result<String, Error> {
    let kind = match role {
        "bank" => "asset",
        "input_vat" | "reverse_charge_vat" => "vat",
        "creditors" => "liability",
        _ => return Err(fail("unsupported expense account role")),
    };
    tx.query_row(
        "SELECT m.account_no FROM account_role_mappings m JOIN accounts a ON a.account_no=m.account_no WHERE m.role=?1 AND m.status='confirmed' AND a.active=1 AND a.type=?2 AND a.normal_balance=CASE WHEN ?1 IN ('creditors','reverse_charge_vat') THEN 'credit' ELSE 'debit' END",
        [role,kind],
        |r| r.get(0),
    )
    .optional()
    .map_err(sql)?
    .ok_or_else(|| fail(format!("confirmed account role '{role}' required")))
}
pub(crate) fn metadata(tx: &Transaction<'_>, id: i64) -> Result<Metadata, Error> {
    crate::documents_original::validate_history(tx)?;
    let payload: Option<String> = tx
        .query_row(
            "SELECT payload_json FROM documents WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let metadata: Metadata = serde_json::from_str(
        payload
            .as_deref()
            .ok_or_else(|| fail("document has no original metadata"))?,
    )
    .map_err(|_| fail("invalid stored document metadata"))?;
    metadata.validate()?;
    Ok(metadata)
}
pub fn options(tx: &Transaction<'_>, slug: &str, id: i64) -> Result<Value, Error> {
    let list = crate::documents::list(tx, slug)?;
    let mut document = list["documents"]
        .as_array()
        .and_then(|rows| rows.iter().find(|d| d["id"] == id))
        .cloned()
        .ok_or_else(|| fail("document does not exist"))?;
    let meta = metadata(tx, id)?;
    document["vatAmount"] = meta.vat_amount.map(Value::Number).unwrap_or(Value::Null);
    let mut q=tx.prepare("SELECT account_no,name,default_vat_code FROM accounts WHERE type='expense' AND active=1 ORDER BY account_no").map_err(sql)?;
    let accounts=q.query_map([],|r|Ok(json!({"accountNo":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"defaultVatCode":r.get::<_,Option<String>>(2)?}))).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?;
    let mut q=tx.prepare("SELECT bt.id,bt.transaction_date,bt.text,bt.amount,bt.currency,bt.amount_dkk,bt.fx_rate_to_dkk,bt.reference FROM bank_transactions bt WHERE bt.amount<0 AND NOT EXISTS(SELECT 1 FROM bank_journal_reconciliations br WHERE br.bank_transaction_id=bt.id) ORDER BY bt.transaction_date DESC,bt.id DESC").map_err(sql)?;
    let bank=q.query_map([],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"date":r.get::<_,String>(1)?,"text":r.get::<_,String>(2)?,"amount":r.get::<_,f64>(3)?,"currency":r.get::<_,String>(4)?,"amountDkk":r.get::<_,Option<f64>>(5)?,"fxRateToDkk":r.get::<_,Option<f64>>(6)?,"reference":r.get::<_,Option<String>>(7)?}))).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?;
    Ok(json!({"document":document,"expenseAccounts":accounts,"unmatchedOutgoingBank":bank}))
}
pub(crate) fn line(
    account: &str,
    debit: Money,
    credit: Money,
    code: Option<&str>,
    text: String,
) -> Result<JournalLine, Error> {
    Ok(JournalLine {
        account_no: AccountNo::parse(account)?,
        debit_amount: debit,
        credit_amount: credit,
        vat_code: code.map(str::to_owned),
        text: Some(text),
    })
}
pub(crate) struct Allocation {
    pub lines: Vec<JournalLine>,
    pub net: Money,
    pub deductible: Money,
    pub treatment: String,
}
pub(crate) fn purchase(
    tx: &Transaction<'_>,
    document_id: i64,
    meta: &Metadata,
    expense: &str,
    requested: Option<&str>,
    gross_dkk: &Money,
    rate: &FxRate,
) -> Result<Allocation, Error> {
    let (kind, active, default): (String, i64, Option<String>) = tx
        .query_row(
            "SELECT type,active,default_vat_code FROM accounts WHERE account_no=?1",
            [expense.trim()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(sql)?;
    if kind != "expense" || active != 1 {
        return Err(fail("expense account must be active and of type expense"));
    }
    let gross = meta
        .amount_inc_vat
        .as_ref()
        .map(money)
        .transpose()?
        .ok_or_else(|| fail("document gross amount required"))?;
    let vat = meta
        .vat_amount
        .as_ref()
        .map(money)
        .transpose()?
        .unwrap_or_default();
    if gross == Money::default()
        || gross.is_negative()
        || vat.is_negative()
        || gross.subtract(&vat).is_negative()
    {
        return Err(fail(
            "document requires positive gross and VAT between zero and gross",
        ));
    }
    let registered: Option<String> = tx
        .query_row(
            "SELECT vat_period_type FROM companies WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let treatment = requested
        .or(match default.as_deref() {
            Some("DK_PURCHASE_25") => Some(if registered.is_some() {
                "standard"
            } else {
                "non_deductible"
            }),
            Some("EU_SERVICE_REVERSE_CHARGE") => Some("reverse_charge"),
            Some("EU_GOODS_ACQUISITION") => Some("eu_goods_acquisition"),
            Some("REPRESENTATION_SPECIAL") => Some(if registered.is_some() {
                "representation"
            } else {
                "non_deductible"
            }),
            _ => None,
        })
        .ok_or_else(|| {
            fail("expense account has no mapped VAT treatment; choose an explicit treatment")
        })?;
    if ![
        "standard",
        "reverse_charge",
        "eu_goods_acquisition",
        "representation",
        "exempt",
        "non_deductible",
    ]
    .contains(&treatment)
    {
        return Err(fail("unknown expense VAT treatment"));
    }
    if treatment == "exempt" && vat != Money::default() {
        return Err(fail("exempt expense requires zero document VAT"));
    }
    if treatment == "standard" || treatment == "representation" {
        if registered.is_none() {
            return Err(fail("company is not VAT registered; use non_deductible"));
        }
        let persisted = crate::purchase_preflight::supplier(tx, document_id)?
            .ok_or_else(|| fail("resolved persisted supplier identity required"))?;
        let supplier = meta
            .sender
            .as_ref()
            .ok_or_else(|| fail("Danish supplier identity required"))?;
        if rentemester_engine::document::supplier(supplier)?.kind != "dk_cvr"
            || persisted.kind != "dk_cvr"
            || persisted.identifier != rentemester_engine::document::supplier(supplier)?.identifier
        {
            return Err(fail("deductible Danish VAT requires a Danish supplier CVR"));
        }
        if treatment == "standard"
            && meta.kind() == "purchase_sale"
            && meta
                .recipient
                .as_ref()
                .is_none_or(|p| p.vat_or_cvr.as_ref().is_none_or(|s| s.trim().is_empty()))
            && !crate::documents_original::valid_context_for_document(tx, document_id, meta)?
        {
            return Err(fail(
                "invoice-stated recipient identity or hash-bound simplified invoice context required",
            ));
        }
        let difference = vat.subtract(&gross.subtract(&vat).percent("25")?);
        if vat == Money::default()
            || meta.purchase_vat_lines.is_none()
                && difference != Money::default()
                && difference != Money::parse("0.01")?
                && difference != Money::parse("-0.01")?
        {
            return Err(fail("standard VAT must reconcile with the 25% base"));
        }
    }
    let mut lines = Vec::new();
    let expense = expense.trim();
    let full_vat = rate.apply(&vat);
    let net = if treatment == "standard" || treatment == "representation" {
        gross_dkk.subtract(&full_vat)
    } else {
        gross_dkk.clone()
    };
    // Allocation.deductible is the source result's document VAT summary, not the
    // self-assessed VAT on foreign purchases. Representation reports full VAT.
    let deductible = if treatment == "standard" || treatment == "representation" {
        full_vat.clone()
    } else {
        Money::default()
    };
    if treatment == "reverse_charge" || treatment == "eu_goods_acquisition" {
        if registered.is_none() {
            return Err(fail(
                "VAT registration required for foreign reverse charge; separate section 50 b registration requires human resolution",
            ));
        }
        if vat != Money::default() {
            return Err(fail("foreign reverse charge requires zero document VAT"));
        }
        if meta.purchase_vat_lines.is_some() {
            return Err(fail(
                "foreign reverse charge does not support structured purchaseVatLines",
            ));
        }
        let supplier = crate::purchase_preflight::supplier(tx, document_id)?
            .ok_or_else(|| fail("resolved foreign supplier identity required"))?;
        let original = rentemester_engine::document::supplier(
            meta.sender
                .as_ref()
                .ok_or_else(|| fail("invoice supplier required"))?,
        )?;
        if supplier.kind != original.kind
            || supplier.country != original.country
            || supplier.identifier != original.identifier
        {
            return Err(fail(
                "persisted supplier identity conflicts with immutable invoice",
            ));
        }
        let code = if supplier.kind == "eu_vat" {
            let now: String = tx
                .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
                    r.get(0)
                })
                .map_err(sql)?;
            crate::vies::require(
                tx,
                supplier
                    .identifier
                    .as_deref()
                    .ok_or_else(|| fail("EU VAT identifier required"))?,
                &now,
            )?;
            if treatment == "eu_goods_acquisition" {
                "EU_GOODS_ACQUISITION"
            } else {
                "EU_SERVICE_REVERSE_CHARGE"
            }
        } else if supplier.kind == "non_eu" && treatment == "reverse_charge" {
            let company: Option<String> = tx
                .query_row("SELECT cvr FROM companies WHERE id=1", [], |r| r.get(0))
                .map_err(sql)?;
            let normalize = |s: &str| {
                s.chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
                    .to_uppercase()
            };
            let buyer = meta
                .recipient
                .as_ref()
                .and_then(|p| p.vat_or_cvr.as_deref())
                .map(normalize)
                .unwrap_or_default();
            let company = company.map(|s| normalize(&s)).map(|s| {
                if s.starts_with("DK") {
                    s
                } else {
                    format!("DK{s}")
                }
            });
            if supplier.identifier.is_none()
                || buyer.len() != 10
                || !buyer.starts_with("DK")
                || !buyer[2..].bytes().all(|b| b.is_ascii_digit())
                || company.as_deref() != Some(&buyer)
                || meta.reverse_charge_wording_confirmed != Some(true)
            {
                return Err(fail(
                    "non-EU reverse charge requires invoice supplier registration, matching configured Danish buyer VAT and confirmed reverse-charge wording",
                ));
            }
            "NON_EU_SERVICE_REVERSE_CHARGE"
        } else {
            return Err(fail(
                "foreign treatment requires appropriate non-Danish supplier identity",
            ));
        };
        let assessed = gross_dkk.percent("25")?;
        lines.push(line(
            expense,
            gross_dkk.clone(),
            Money::default(),
            Some(code),
            "Foreign purchase base".into(),
        )?);
        lines.push(line(
            &role(tx, "input_vat")?,
            assessed.clone(),
            Money::default(),
            None,
            "Deductible self-assessed VAT".into(),
        )?);
        lines.push(line(
            &role(tx, "reverse_charge_vat")?,
            Money::default(),
            assessed,
            None,
            "Self-assessed output VAT".into(),
        )?);
    } else if treatment == "representation" {
        if meta.purchase_vat_lines.is_some() {
            return Err(fail(
                "representation does not support structured purchaseVatLines",
            ));
        }
        let partial = net.percent("25")?.percent("25")?;
        let cost = gross_dkk.subtract(&net).subtract(&partial);
        lines.push(line(
            expense,
            net.clone(),
            Money::default(),
            Some("REPRESENTATION_SPECIAL"),
            "Representation purchase base".into(),
        )?);
        lines.push(line(
            expense,
            cost,
            Money::default(),
            Some("REPRESENTATION_NON_DEDUCTIBLE_VAT"),
            "Non-deductible representation VAT (75%)".into(),
        )?);
        lines.push(line(
            &role(tx, "input_vat")?,
            partial,
            Money::default(),
            None,
            "Deductible representation VAT (25%)".into(),
        )?);
    } else if treatment == "standard" {
        if let Some(split) = &meta.purchase_vat_lines {
            let scaled = expense_fx::scale_net(split, rate, &net)?;
            for (part, amount) in split.iter().zip(scaled) {
                lines.push(line(
                    expense,
                    amount,
                    Money::default(),
                    Some(if part.classification == "dk_purchase_25" {
                        "DK_PURCHASE_25"
                    } else {
                        "DK_PURCHASE_EXEMPT"
                    }),
                    "Udgift, grundbeløb".into(),
                )?);
            }
        } else {
            lines.push(line(
                expense,
                net.clone(),
                Money::default(),
                Some("DK_PURCHASE_25"),
                "Udgift, grundbeløb".into(),
            )?);
        }
        lines.push(line(
            &role(tx, "input_vat")?,
            deductible.clone(),
            Money::default(),
            None,
            "Købsmoms".into(),
        )?);
    } else {
        lines.push(line(
            expense,
            gross_dkk.clone(),
            Money::default(),
            None,
            if treatment == "non_deductible" {
                "Udgift inkl. ikke-fradragsberettiget moms"
            } else {
                "Udgift"
            }
            .into(),
        )?);
    }
    Ok(Allocation {
        lines,
        net,
        deductible,
        treatment: treatment.into(),
    })
}
/// Shared read-only preparation; callers must still use post for admission/writes.
pub(crate) struct Prepared {
    pub request: ManualJournal,
    pub gross: Money,
    pub net: Money,
    pub deductible: Money,
    pub treatment: String,
    pub basis: expense_fx::Basis,
}
pub(crate) fn prepare(tx: &Transaction<'_>, input: &Input) -> Result<Prepared, Error> {
    prepare_mode(tx, input, false)
}
/// Receipt admission checks current source without refusing the already accepted
/// reconciliation. Only the shared keyed writer may return an accepted replay.
pub(crate) fn prepare_for_receipt(
    tx: &Transaction<'_>,
    input: &Input,
    latest: Date,
) -> Result<(), Error> {
    let mut prepared = prepare_mode(tx, input, true)?;
    // The payment may already be reconciled by this receipt; all other current
    // account, period, horizon and rule controls still precede receipt lookup.
    prepared.request.source_bank_transaction_id = None;
    let facts = crate::journal_preview::collect_facts(tx, &prepared.request, latest)?;
    let rules = crate::journal_preview::compiled_rules()?;
    let validation = rentemester_engine::manual_journal::validate(
        &prepared.request,
        &facts,
        &rules,
        rules.version(),
    )?;
    if !validation.ok {
        return Err(fail(
            "current expense period/account/rule controls refuse this request",
        ));
    }
    Ok(())
}
fn prepare_mode(tx: &Transaction<'_>, input: &Input, receipt: bool) -> Result<Prepared, Error> {
    crate::journal_preview::require_current(tx)?;
    if input.document_id <= 0
        || input.bank_transaction_id <= 0
        || input.document_id > 9_007_199_254_740_991
        || input.bank_transaction_id > 9_007_199_254_740_991
    {
        return Err(fail("positive document and bank references required"));
    }
    let registered: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM payables WHERE document_id=?1)",
            [input.document_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if registered {
        return Err(fail(
            "purchase is already registered as a payable; settle its creditor balance",
        ));
    }
    crate::invoice_extraction::require_completed_or_absent(tx, input.document_id)?;
    let meta = metadata(tx, input.document_id)?;
    let gross = meta
        .amount_inc_vat
        .as_ref()
        .map(money)
        .transpose()?
        .ok_or_else(|| fail("document gross amount required"))?;
    let vat = meta
        .vat_amount
        .as_ref()
        .map(money)
        .transpose()?
        .unwrap_or_default();
    if gross == Money::default()
        || gross.is_negative()
        || vat.is_negative()
        || gross.subtract(&vat).is_negative()
    {
        return Err(fail(
            "document requires positive gross and VAT between zero and gross",
        ));
    }
    struct BankSource {
        date: String,
        text: String,
        currency: String,
        amount: f64,
        amount_dkk: Option<f64>,
        rate: Option<f64>,
        payment: Option<String>,
    }
    let bank=tx.query_row("SELECT bt.transaction_date,bt.text,bt.currency,bt.amount,bt.amount_dkk,bt.fx_rate_to_dkk,ba.ledger_account_no FROM bank_transactions bt LEFT JOIN bank_accounts ba ON ba.id=bt.bank_account_id WHERE bt.id=?1",[input.bank_transaction_id],|r|Ok(BankSource{date:r.get(0)?,text:r.get(1)?,currency:r.get(2)?,amount:r.get(3)?,amount_dkk:r.get(4)?,rate:r.get(5)?,payment:r.get(6)?})).map_err(sql)?;
    let signed = |value| crate::bank::signed_money(rusqlite::types::ValueRef::Real(value));
    let bank_amount = signed(bank.amount)?;
    let bank_dkk = bank.amount_dkk.map(signed).transpose()?;
    let bank_rate = bank
        .rate
        .map(|value| {
            if !value.is_finite() {
                return Err(fail("bank FX rate must be finite"));
            }
            FxRate::parse(ryu_js::Buffer::new().format_finite(value))
        })
        .transpose()?;
    let currency = meta.currency();
    let bank_currency = bank.currency.trim().to_uppercase();
    let basis = expense_fx::resolve(
        &currency,
        &gross,
        Payment {
            currency: &bank_currency,
            amount: &bank_amount,
            amount_dkk: bank_dkk.as_ref(),
            rate: bank_rate.as_ref(),
        },
    )?;
    let BankSource {
        date: bank_date,
        text: bank_text,
        payment,
        ..
    } = bank;
    let linked:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM bank_journal_reconciliations WHERE bank_transaction_id=?1)",[input.bank_transaction_id],|r|r.get(0)).map_err(sql)?;
    if linked && !receipt {
        return Err(fail(
            "bank transaction is already linked to a journal entry",
        ));
    }
    let allocated = purchase(
        tx,
        input.document_id,
        &meta,
        &input.expense_account_no,
        input.vat_treatment.as_deref(),
        &basis.gross_dkk,
        &basis.rate,
    )?;
    let treatment = allocated.treatment.clone();
    let date = input.transaction_date.as_deref().unwrap_or(&bank_date);
    if meta.kind() == "internal_voucher" {
        let bound: i64 = tx
            .query_row(
                "SELECT bank_transaction_id FROM internal_voucher_evidence WHERE document_id=?1",
                [input.document_id],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if bound != input.bank_transaction_id || date != bank_date || treatment != "exempt" {
            return Err(fail(
                "internal voucher must use its bound bank row, bank date and exempt treatment",
            ));
        }
    }
    let payment = match input
        .payment_account_no
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(no) => no.to_owned(),
        None => match payment {
            Some(no) => no,
            None => role(tx, "bank")?,
        },
    };
    let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND type='asset' AND active=1)",[&payment],|r|r.get(0)).map_err(sql)?;
    if !valid {
        return Err(fail("payment account must be an active asset account"));
    }
    let mut lines = allocated.lines;
    let net = allocated.net;
    let deductible = allocated.deductible;
    lines.push(line(
        &payment,
        Money::default(),
        basis.gross_dkk.clone(),
        None,
        bank_text,
    )?);
    let text = input
        .text
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Udgift (banktransaktion {})", input.bank_transaction_id));
    let request = ManualJournal {
        transaction_date: Date::parse(date)?,
        text,
        currency: Currency::parse(&currency)?,
        document_id: Some(DocumentId::from_sqlite(input.document_id)),
        source_bank_transaction_id: Some(BankTransactionId::from_sqlite(input.bank_transaction_id)),
        amount_foreign: (currency != "DKK").then(|| gross.clone()),
        amount_dkk: (currency != "DKK").then(|| basis.gross_dkk.clone()),
        fx_rate_to_dkk: (currency != "DKK").then(|| basis.rate.clone()),
        created_by_program: None,
        lines,
    };
    Ok(Prepared {
        request,
        gross,
        net,
        deductible,
        treatment,
        basis,
    })
}
pub fn post(
    tx: &Transaction<'_>,
    input: &Input,
    subject: &str,
    latest: Date,
) -> Result<Value, Error> {
    post_with_principal(
        tx,
        input,
        subject,
        latest,
        crate::idempotency::PrincipalKind::User,
    )
}
pub fn post_with_principal(
    tx: &Transaction<'_>,
    input: &Input,
    subject: &str,
    latest: Date,
    principal_kind: crate::idempotency::PrincipalKind,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let Prepared {
        request,
        gross,
        net,
        deductible,
        treatment,
        basis,
    } = prepare(tx, input)?;
    let (prefix, program) = match principal_kind {
        crate::idempotency::PrincipalKind::User => ("user", "rentemester-rust"),
        crate::idempotency::PrincipalKind::ServiceAccount => {
            ("service_account", "service-principal")
        }
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
    if result["ok"] != true {
        return Err(fail(result["errors"].to_string()));
    }
    for (key, value) in [
        ("documentId", json!(input.document_id)),
        ("bankTransactionId", json!(input.bank_transaction_id)),
        ("vatTreatment", json!(treatment)),
        ("grossAmount", crate::cockpit::number(&gross)?),
        ("netAmount", crate::cockpit::number(&net)?),
        ("vatAmount", crate::cockpit::number(&deductible)?),
        ("grossAmountDkk", crate::cockpit::number(&basis.gross_dkk)?),
        ("netAmountDkk", crate::cockpit::number(&net)?),
        ("vatAmountDkk", crate::cockpit::number(&deductible)?),
        ("grossAmountForeign", crate::cockpit::number(&gross)?),
        (
            "fxRateToDkk",
            json!(crate::journal_post::rate(&basis.rate)?),
        ),
    ] {
        result[key] = value;
    }
    let treatment_rule = match treatment.as_str() {
        "representation" => Some("DK-VAT-REPRESENTATION-001"),
        "eu_goods_acquisition" => Some("DK-VAT-REVERSE-CHARGE-001"),
        "reverse_charge" => Some(
            if request
                .lines
                .iter()
                .any(|l| l.vat_code.as_deref() == Some("NON_EU_SERVICE_REVERSE_CHARGE"))
            {
                "DK-VAT-NON-EU-SERVICE-REVERSE-CHARGE-001"
            } else {
                "DK-VAT-REVERSE-CHARGE-001"
            },
        ),
        _ => None,
    };
    if let Some(rule) = treatment_rule {
        let mut applied = result["appliedRules"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !applied.iter().any(|v| v == rule) {
            applied.push(json!(rule));
        }
        result["appliedRules"] = json!(applied);
    }
    if let Some(source) = basis.rate_source {
        result["fxRateSource"] = json!(source);
        result["fxReconstructionDifferenceDkk"] =
            crate::cockpit::number(&basis.rate.apply(&gross).subtract(&basis.gross_dkk))?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_purchase_books_exact_vat_and_rejects_second_use_of_payment() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut company = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic Expense")
            .unwrap();
        let tx = company
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("UPDATE companies SET vat_period_type='quarter' WHERE id=1; INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic cash','asset','debit'),('X','Synthetic expense','expense','debit'),('V','Synthetic input VAT','vat','debit'); INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by) VALUES('input_vat','V','confirmed',1,'synthetic'); INSERT INTO bank_accounts(slug,name,ledger_account_no) VALUES('synthetic','Synthetic bank','A');").unwrap();
        let metadata:Metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-10-18","deliveryDescription":"Synthetic supplies","invoiceNo":"SYN-1","amountIncVat":125,"vatAmount":25,"sender":{"name":"Synthetic supplier","address":"Synthetic street 1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic buyer","address":"Synthetic street 2","vatOrCvr":"DK87654321"}})).unwrap();
        crate::documents::register(
            &tx,
            crate::documents::Original {
                filename: "synthetic.txt",
                hash: &"a".repeat(64),
                path: "synthetic-original",
                mime: "text/plain",
            },
            &metadata,
            "synthetic",
            "2026-10-18",
        )
        .unwrap();
        let csv = "date;text;amount\n2026-10-18;Synthetic payment;-125\n";
        let rows = rentemester_engine::bank::parse(csv).unwrap();
        crate::bank::import(
            &tx,
            csv,
            &rows,
            Some("synthetic"),
            "synthetic",
            "2026-10-18T12:00:00Z",
            "synthetic-original",
        )
        .unwrap();
        let input:Input=serde_json::from_value(json!({"documentId":1,"bankTransactionId":1,"expenseAccountNo":"X","vatTreatment":"standard"})).unwrap();
        let result = post(&tx, &input, "synthetic", Date::parse("2026-12-01").unwrap()).unwrap();
        let vat = crate::vat_report::read(&tx, "synthetic", "2026-10-18", None).unwrap();
        assert_eq!(vat["inputVat"].as_f64(), Some(25.0));
        assert_eq!(vat["payable"].as_f64(), Some(-25.0));
        assert_eq!(vat["periodStart"], "2026-10-01");
        assert_eq!(vat["periodStatus"], "open");
        assert_eq!(vat["vatReportErrors"], json!([]));
        assert_eq!(vat["fiscalYears"].as_array().unwrap().len(), 1);
        let bank = crate::bank::read(&tx, "synthetic", "2026-10-18", None).unwrap();
        assert_eq!(bank["matchedCount"], 1, "{bank}");
        assert_eq!(bank["bookedBalance"].as_f64(), Some(-125.0), "{bank}");
        assert_eq!(result["netAmount"].as_f64(), Some(100.0));
        assert_eq!(result["vatAmount"].as_f64(), Some(25.0));
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_lines", [], |r| r.get(0))
                .unwrap(),
            3
        );
        assert!(
            post(&tx, &input, "synthetic", Date::parse("2026-12-01").unwrap())
                .unwrap_err()
                .message
                .contains("already linked")
        );
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
                .unwrap(),
            1
        );
    }
}

#[cfg(test)]
mod fx_tests {
    use super::*;
    #[test]
    fn foreign_settlement_persists_positive_quote_and_dkk_lines_without_deducting_foreign_tax() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut company = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic FX").unwrap();
        let tx = company
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic cash','asset','debit'),('X','Synthetic expense','expense','debit');INSERT INTO bank_accounts(slug,name,ledger_account_no) VALUES('synthetic','Synthetic bank','A');").unwrap();
        let meta:Metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"cash_register_receipt","issueDate":"2026-10-18","amountIncVat":100,"vatAmount":20,"currency":"EUR"})).unwrap();
        crate::documents::register(
            &tx,
            crate::documents::Original {
                filename: "synthetic.txt",
                hash: &"b".repeat(64),
                path: "synthetic-original",
                mime: "text/plain",
            },
            &meta,
            "synthetic",
            "2026-10-18",
        )
        .unwrap();
        let csv = "date;text;amount\n2026-10-18;Synthetic EUR settlement;-746.25\n";
        let rows = rentemester_engine::bank::parse(csv).unwrap();
        crate::bank::import(
            &tx,
            csv,
            &rows,
            Some("synthetic"),
            "synthetic",
            "2026-10-18T12:00:00Z",
            "synthetic-original",
        )
        .unwrap();
        let input:Input=serde_json::from_value(json!({"documentId":1,"bankTransactionId":1,"expenseAccountNo":"X","vatTreatment":"non_deductible"})).unwrap();
        let posted = post(&tx, &input, "synthetic", Date::parse("2026-12-01").unwrap()).unwrap();
        assert_eq!(posted["grossAmountDkk"].as_f64(), Some(746.25));
        assert_eq!(posted["fxRateSource"], "derived_dkk_settlement");
        assert_eq!(posted["vatAmountDkk"].as_f64(), Some(0.0));
        let (currency,foreign,dkk,rate):(String,f64,f64,f64)=tx.query_row("SELECT currency,amount_foreign,amount_dkk,fx_rate_to_dkk FROM journal_entries WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
        assert_eq!(
            (currency.as_str(), foreign, dkk, rate),
            ("EUR", 100.0, 746.25, 7.4625)
        );
        let amounts:Vec<(String,f64,f64)>=tx.prepare("SELECT a.account_no,jl.debit_amount,jl.credit_amount FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id ORDER BY jl.id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<Result<_,_>>().unwrap();
        assert_eq!(
            amounts,
            vec![("X".into(), 746.25, 0.0), ("A".into(), 0.0, 746.25)]
        );
    }
}
