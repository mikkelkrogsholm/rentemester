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
fn role(tx: &Transaction<'_>, role: &str) -> Result<String, Error> {
    let kind = match role {
        "bank" => "asset",
        "input_vat" => "vat",
        _ => return Err(fail("unsupported expense account role")),
    };
    tx.query_row(
        "SELECT m.account_no FROM account_role_mappings m JOIN accounts a ON a.account_no=m.account_no WHERE m.role=?1 AND m.status='confirmed' AND a.active=1 AND a.type=?2 AND a.normal_balance='debit'",
        [role,kind],
        |r| r.get(0),
    )
    .optional()
    .map_err(sql)?
    .ok_or_else(|| fail(format!("confirmed account role '{role}' required")))
}
fn metadata(tx: &Transaction<'_>, id: i64) -> Result<Metadata, Error> {
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
pub fn post(
    tx: &Transaction<'_>,
    input: &Input,
    subject: &str,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if input.document_id <= 0
        || input.bank_transaction_id <= 0
        || input.document_id > 9_007_199_254_740_991
        || input.bank_transaction_id > 9_007_199_254_740_991
    {
        return Err(fail("positive document and bank references required"));
    }
    let (kind, active, default): (String, i64, Option<String>) = tx
        .query_row(
            "SELECT type,active,default_vat_code FROM accounts WHERE account_no=?1",
            [input.expense_account_no.trim()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(sql)?;
    if kind != "expense" || active != 1 {
        return Err(fail("expense account must be active and of type expense"));
    }
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
    if linked {
        return Err(fail(
            "bank transaction is already linked to a journal entry",
        ));
    }
    let registered: Option<String> = tx
        .query_row(
            "SELECT vat_period_type FROM companies WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let treatment = input
        .vat_treatment
        .as_deref()
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
    if !["standard", "exempt", "non_deductible"].contains(&treatment) {
        return Err(fail(
            "this VAT treatment is not ported to native expense booking yet",
        ));
    }
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
    if treatment == "exempt" && vat != Money::default() {
        return Err(fail("exempt expense requires zero document VAT"));
    }
    if treatment == "standard" {
        if registered.is_none() {
            return Err(fail("company is not VAT registered; use non_deductible"));
        }
        let supplier = meta
            .sender
            .as_ref()
            .ok_or_else(|| fail("Danish supplier identity required"))?;
        if rentemester_engine::document::supplier(supplier)?.kind != "dk_cvr" {
            return Err(fail("deductible Danish VAT requires a Danish supplier CVR"));
        }
        if meta.kind() == "purchase_sale"
            && meta
                .recipient
                .as_ref()
                .is_none_or(|p| p.vat_or_cvr.as_ref().is_none_or(|s| s.trim().is_empty()))
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
    let line = |account: &str,
                debit: Money,
                credit: Money,
                code: Option<&str>,
                text: String|
     -> Result<JournalLine, Error> {
        Ok(JournalLine {
            account_no: AccountNo::parse(account)?,
            debit_amount: debit,
            credit_amount: credit,
            vat_code: code.map(str::to_owned),
            text: Some(text),
        })
    };
    let mut lines = Vec::new();
    let expense = input.expense_account_no.trim();
    let deductible = if treatment == "standard" {
        basis.rate.apply(&vat)
    } else {
        Money::default()
    };
    let net = basis.gross_dkk.subtract(&deductible);
    if treatment == "standard" {
        if let Some(split) = &meta.purchase_vat_lines {
            let scaled = expense_fx::scale_net(split, &basis.rate, &net)?;
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
            basis.gross_dkk.clone(),
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
    let actor = format!("user:{subject}");
    let mut result = crate::journal_post::post(
        tx,
        &request,
        &crate::journal_preview::manual_journal_rule_version()?,
        latest,
        crate::journal_post::Actor {
            subject: &actor,
            program: "rentemester-rust",
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
    if let Some(source) = basis.rate_source {
        result["fxRateSource"] = json!(source);
        result["fxReconstructionDifferenceDkk"] = json!(0);
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
