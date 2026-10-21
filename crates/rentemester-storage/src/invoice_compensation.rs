//! Human-attributed compensation: assessment, separate registration, separate DKK posting.
use crate::{invalid, legacy_money};
use rentemester_engine::{
    Error,
    currency::Currency,
    fiscal_year::Date,
    identifiers::{AccountNo, DocumentId},
    invoice_compensation::{self as policy, refused},
    manual_journal::{JournalLine, ManualJournal},
    money::Money,
    sales_invoice::wire,
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Number, Value, json};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Calculate {
    pub invoice_document_id: i64,
    pub as_of_date: String,
    pub compensation_amount_dkk: Option<Number>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Register {
    pub invoice_document_id: i64,
    pub as_of_date: String,
    pub compensation_amount_dkk: Option<Number>,
    pub note: Option<String>,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Post {
    pub invoice_document_id: i64,
    pub transaction_date: Option<String>,
    pub receivable_account_no: String,
    pub compensation_income_account_no: String,
    pub confirm: bool,
}
fn positive(id: i64) -> Result<(), Error> {
    if (1..=9007199254740991).contains(&id) {
        Ok(())
    } else {
        Err(refused("positive exact invoice identifier required"))
    }
}
fn actor(subject: &str) -> Result<(), Error> {
    if subject.is_empty() || subject.len() > 256 || subject.chars().any(char::is_control) {
        Err(refused("trusted bounded current human subject required"))
    } else {
        Ok(())
    }
}
struct Invoice {
    no: String,
    date: String,
    due: String,
    currency: String,
    payload: Value,
}
fn invoice(tx: &Transaction<'_>, id: i64) -> Result<Invoice, Error> {
    positive(id)?;
    let row:Option<(String,String,String,String,String)>=tx.query_row("SELECT d.invoice_no,d.invoice_date,d.currency,d.payload_json,d.sha256_hash FROM documents d JOIN issued_invoice_postings p ON p.invoice_document_id=d.id JOIN journal_entries j ON j.id=p.journal_entry_id WHERE d.id=?1 AND d.document_type='issued_invoice' AND j.status='posted' AND j.document_id=d.id AND j.transaction_date=d.invoice_date AND j.source_bank_transaction_id IS NULL AND j.reversal_of_entry_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id)",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(invalid)?;
    let (no, date, currency, raw, hash) =
        row.ok_or_else(|| refused("active explicitly posted issued invoice required"))?;
    if crate::invoice_email::sha(raw.as_bytes()) != hash {
        return Err(refused("immutable invoice snapshot hash differs"));
    }
    let payload: Value = serde_json::from_str(&raw).map_err(invalid)?;
    if payload["invoiceNumber"] != no
        || payload["issueDate"] != date
        || payload["currency"] != currency
    {
        return Err(refused("immutable invoice identity differs"));
    }
    Date::parse(&date)?;
    Currency::parse(&currency)?;
    let due = rentemester_engine::reminder_claims::effective_due_date(
        &date,
        payload["dueDate"].as_str(),
    )?;
    Ok(Invoice {
        no,
        date,
        due,
        currency,
        payload,
    })
}
fn existing(tx: &Transaction<'_>, id: i64) -> Result<Value, Error> {
    type ExistingClaim = (
        i64,
        String,
        Money,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        bool,
    );
    let row:Option<ExistingClaim>=tx.query_row("SELECT c.id,c.claim_date,c.amount_dkk,p.journal_entry_id,j.entry_no,j.status,j.document_id,j.source_bank_transaction_id,j.reversal_of_entry_id,EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) FROM invoice_compensation_claims c LEFT JOIN invoice_compensation_postings p ON p.compensation_claim_id=c.id LEFT JOIN journal_entries j ON j.id=p.journal_entry_id WHERE c.invoice_document_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,legacy_money(r.get_ref(2)?).map_err(|_|rusqlite::Error::InvalidQuery)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?))).optional().map_err(invalid)?;
    let Some((claim, date, amount, journal, no, status, doc, bank, reversal, reversed)) = row
    else {
        return Ok(
            json!({"claimId":null,"claimDate":null,"registeredAmountDkk":null,"postedJournalEntryId":null,"postedJournalEntryNo":null}),
        );
    };
    Date::parse(&date)?;
    if amount.is_negative()
        || amount == Money::default()
        || Money::parse(policy::MAXIMUM_DKK)?
            .subtract(&amount)
            .is_negative()
    {
        return Err(refused(
            "registered compensation amount differs from supported policy",
        ));
    }
    if journal.is_some()
        && (no.is_none()
            || status.as_deref() != Some("posted")
            || doc != Some(id)
            || bank.is_some()
            || reversal.is_some()
            || reversed)
    {
        return Err(refused(
            "registered compensation posting is missing, reversed or inconsistent",
        ));
    }
    Ok(
        json!({"claimId":claim,"claimDate":date,"registeredAmountDkk":wire(&amount)?,"postedJournalEntryId":journal,"postedJournalEntryNo":no}),
    )
}
/// Pure read under the caller's owned/current authorization transaction. Existing
/// claim identity remains visible when its principal is no longer eligible.
pub fn calculate(tx: &Transaction<'_>, input: &Calculate) -> Result<Value, Error> {
    crate::journal_preview::require_current(tx)?;
    let i = invoice(tx, input.invoice_document_id)?;
    let (principal, currency) =
        crate::reminder_claims::principal_open(tx, input.invoice_document_id)?;
    if currency != i.currency {
        return Err(refused("principal currency differs from immutable invoice"));
    }
    let amount = input
        .compensation_amount_dkk
        .as_ref()
        .map(|n| Money::parse(&n.to_string()))
        .transpose()?;
    let assessment = policy::assess(
        &i.date,
        &i.due,
        &input.as_of_date,
        &i.payload["buyer"],
        &principal,
        amount,
    )?;
    let mut out = assessment.value()?;
    let object = out.as_object_mut().unwrap();
    object.extend(
        existing(tx, input.invoice_document_id)?
            .as_object()
            .unwrap()
            .clone(),
    );
    object.insert("ok".into(), json!(true));
    object.insert("invoiceDocumentId".into(), json!(input.invoice_document_id));
    object.insert("invoiceNumber".into(), json!(i.no));
    object.insert("asOfDate".into(), json!(input.as_of_date));
    object.insert("effectiveDueDate".into(), json!(i.due));
    object.insert("principalOpenBalance".into(), wire(&principal)?);
    object.insert("principalCurrency".into(), json!(currency));
    Ok(out)
}
fn audit(
    tx: &Transaction<'_>,
    event: &str,
    id: i64,
    message: &str,
    subject: &str,
) -> Result<(), Error> {
    actor(subject)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES(?1,'invoice_compensation_claim',?2,?3,?4)",params![event,id.to_string(),message,format!("user:{subject}")]).map_err(invalid)?;
    Ok(())
}
pub fn register(tx: &Transaction<'_>, input: &Register, subject: &str) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    actor(subject)?;
    if !input.confirm {
        return Err(refused(
            "explicit compensation registration confirmation required",
        ));
    }
    if input.note.as_ref().is_some_and(|s| s.len() > 4096) {
        return Err(refused("compensation note exceeds4096 bytes"));
    }
    let request = Calculate {
        invoice_document_id: input.invoice_document_id,
        as_of_date: input.as_of_date.clone(),
        compensation_amount_dkk: input.compensation_amount_dkk.clone(),
    };
    let before = calculate(tx, &request)?;
    if !before["claimId"].is_null() {
        return Err(refused(
            "invoice already has a registered compensation claim; read its canonical status",
        ));
    }
    if before["eligible"] != true {
        return Err(refused(
            before["reason"]
                .as_str()
                .unwrap_or("invoice is not eligible"),
        ));
    }
    let amount = Money::parse(&before["compensationAmountDkk"].to_string())?;
    if amount == Money::default() || amount.is_negative() {
        return Err(refused("registered compensation must be positive"));
    }
    let id:i64=tx.query_row("INSERT INTO invoice_compensation_claims(invoice_document_id,claim_date,amount_dkk,note)VALUES(?1,?2,?3,?4)RETURNING id",params![input.invoice_document_id,input.as_of_date,amount.decimal(),input.note],|r|r.get(0)).map_err(invalid)?;
    audit(
        tx,
        "invoice_compensation_register",
        id,
        &format!(
            "Registered compensation claim {} on invoice {}",
            amount.decimal(),
            before["invoiceNumber"].as_str().unwrap()
        ),
        subject,
    )?;
    let mut after = calculate(tx, &request)?;
    after["appliedRules"] = json!([policy::CALCULATE_RULE, policy::REGISTER_RULE]);
    Ok(after)
}
pub fn post(
    tx: &Transaction<'_>,
    input: &Post,
    subject: &str,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    actor(subject)?;
    positive(input.invoice_document_id)?;
    if !input.confirm {
        return Err(refused(
            "explicit separate compensation posting confirmation required",
        ));
    }
    let i = invoice(tx, input.invoice_document_id)?;
    // principal_open also validates every existing native claim/cash relation. Zero
    // principal does not erase an already registered commercial compensation.
    crate::reminder_claims::principal_open(tx, input.invoice_document_id)?;
    let claim = existing(tx, input.invoice_document_id)?;
    let id = claim["claimId"]
        .as_i64()
        .ok_or_else(|| refused("register a compensation claim before posting"))?;
    if !claim["postedJournalEntryId"].is_null() {
        return Err(refused(
            "compensation claim already posted; read its canonical status",
        ));
    }
    let date = input
        .transaction_date
        .as_deref()
        .unwrap_or(claim["claimDate"].as_str().unwrap());
    Date::parse(date)?;
    if date < claim["claimDate"].as_str().unwrap() {
        return Err(refused("posting cannot predate registered compensation"));
    }
    for (number, kind, normal) in [
        (&input.receivable_account_no, "asset", "debit"),
        (&input.compensation_income_account_no, "income", "credit"),
    ] {
        let compatible:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND type=?2 AND normal_balance=?3 AND active=1 AND allow_direct_posting=1)",params![number,kind,normal],|r|r.get(0)).map_err(invalid)?;
        if !compatible {
            return Err(refused(
                "explicit active directly-postable compatible compensation receivable/income accounts required",
            ));
        }
    }
    crate::invoice_compensation_schema::install(tx)?;
    let amount = Money::parse(&claim["registeredAmountDkk"].to_string())?;
    let line = |account: &str, debit: Money, credit: Money| -> Result<JournalLine, Error> {
        Ok(JournalLine {
            account_no: AccountNo::parse(account)?,
            debit_amount: debit,
            credit_amount: credit,
            vat_code: None,
            text: Some(format!("Compensation {}", i.no)),
        })
    };
    let request = ManualJournal {
        transaction_date: Date::parse(date)?,
        text: format!("Compensation claim {}", i.no),
        currency: Currency::parse("DKK")?,
        document_id: Some(DocumentId::from_sqlite(input.invoice_document_id)),
        source_bank_transaction_id: None,
        amount_foreign: None,
        amount_dkk: None,
        fx_rate_to_dkk: None,
        created_by_program: None,
        lines: vec![
            line(
                &input.receivable_account_no,
                amount.clone(),
                Money::default(),
            )?,
            line(
                &input.compensation_income_account_no,
                Money::default(),
                amount.clone(),
            )?,
        ],
    };
    let posted = crate::journal_post::post(
        tx,
        &request,
        &crate::journal_preview::manual_journal_rule_version()?,
        latest,
        crate::journal_post::Actor {
            subject: &format!("user:{subject}"),
            program: "rentemester-rust",
        },
    )?;
    if posted["ok"] != true {
        return Err(refused(posted["errors"].to_string()));
    }
    tx.execute("INSERT INTO invoice_compensation_postings(compensation_claim_id,journal_entry_id)VALUES(?1,?2)",params![id,posted["entryId"].as_i64().ok_or_else(||refused("successful claim journal identity missing"))?]).map_err(invalid)?;
    audit(
        tx,
        "invoice_compensation_post",
        id,
        &format!(
            "Posted compensation claim {} for invoice {} in journal entry {}",
            amount.decimal(),
            i.no,
            posted["entryNo"].as_str().unwrap_or("")
        ),
        subject,
    )?;
    let mut out = calculate(
        tx,
        &Calculate {
            invoice_document_id: input.invoice_document_id,
            as_of_date: date.into(),
            compensation_amount_dkk: None,
        },
    )?;
    out.as_object_mut().unwrap().extend(
        posted
            .as_object()
            .ok_or_else(|| refused("posting result unavailable"))?
            .clone(),
    );
    out["compensationAmountDkk"] = wire(&amount)?;
    out["appliedRules"] = json!([policy::CALCULATE_RULE, policy::POST_RULE]);
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, TransactionBehavior};
    fn fixture(currency: &str, commercial: bool) -> Connection {
        fixture_hash(currency, commercial, false)
    }
    fn fixture_hash(currency: &str, commercial: bool, wrong_hash: bool) -> Connection {
        let mut control = Connection::open_in_memory().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic compensation seller")
            .unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("UPDATE companies SET address='Synthetic Street 1',cvr='87654321',vat_period_type='quarter' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R','Synthetic original receivable','asset','debit'),('S','Synthetic sales','income','credit'),('V','Synthetic VAT','vat','credit'),('CR','Synthetic claim receivable','asset','debit'),('CI','Synthetic compensation income','income','credit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');").unwrap();
        let mut body = json!({"issueDate":"2026-01-01","dueDate":"2026-01-31","currency":currency,"buyer":{"name":"Synthetic buyer","address":"Synthetic buyer street"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100}]});
        if currency != "DKK" {
            body["fxRateToDkk"] = json!(7.46);
        }
        if commercial {
            body["buyer"]["vatOrCvr"] = json!("DK12345678");
        }
        let input: rentemester_engine::sales_invoice::Input = serde_json::from_value(body).unwrap();
        let payload = crate::sales::prepare(&tx, &input, "2026-01-01T12:00:00Z", true).unwrap();
        let raw = serde_json::to_string_pretty(&payload).unwrap();
        let hash = if wrong_hash {
            "0".repeat(64)
        } else {
            crate::invoice_email::sha(raw.as_bytes())
        };
        let pdf = b"%PDF-Synthetic compensated invoice fixture\n";
        let pdf_hash = crate::invoice_email::sha(pdf);
        let invoice = crate::sales::register(
            &tx,
            &payload,
            crate::sales::Artifact {
                filename: "synthetic.json",
                path: "synthetic.json",
                hash: &hash,
                mime: "application/json",
            },
            crate::sales::Artifact {
                filename: "synthetic.pdf",
                path: "synthetic.pdf",
                hash: &pdf_hash,
                mime: "application/pdf",
            },
            "synthetic",
        )
        .unwrap();
        assert_eq!(invoice["documentId"], 1);
        crate::sales::post(
            &tx,
            &crate::sales::Post {
                invoice_document_id: 1,
                revenue_account_no: "S".into(),
                confirm: true,
            },
            "synthetic",
            Date::parse("2026-01-01").unwrap(),
        )
        .unwrap();
        tx.commit().unwrap();
        db
    }
    fn amount_is(value: &Value, expected: &str) {
        assert_eq!(
            crate::invoice_fx::amount(value).unwrap(),
            Money::parse(expected).unwrap()
        );
    }
    fn calculate_request() -> Calculate {
        Calculate {
            invoice_document_id: 1,
            as_of_date: "2026-03-02".into(),
            compensation_amount_dkk: None,
        }
    }
    fn register_request() -> Register {
        Register {
            invoice_document_id: 1,
            as_of_date: "2026-03-02".into(),
            compensation_amount_dkk: None,
            note: Some("Synthetic reviewed commercial claim".into()),
            confirm: true,
        }
    }
    fn post_request() -> Post {
        Post {
            invoice_document_id: 1,
            transaction_date: Some("2026-03-02".into()),
            receivable_account_no: "CR".into(),
            compensation_income_account_no: "CI".into(),
            confirm: true,
        }
    }
    fn counts(db: &Connection) -> (i64, i64, i64) {
        db.query_row("SELECT (SELECT count(*) FROM invoice_compensation_claims),(SELECT count(*) FROM invoice_compensation_postings),(SELECT count(*) FROM journal_entries)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap()
    }
    #[test]
    fn dkk_and_foreign_separate_registration_posting_and_canonical_readback() {
        for currency in ["DKK", "EUR"] {
            let mut db = fixture(currency, true);
            {
                let tx = db
                    .transaction_with_behavior(TransactionBehavior::Deferred)
                    .unwrap();
                let assessment = calculate(&tx, &calculate_request()).unwrap();
                assert_eq!(assessment["eligible"], true);
                assert_eq!(assessment["principalCurrency"], currency);
                amount_is(&assessment["principalOpenBalance"], "125");
                amount_is(&assessment["compensationAmountDkk"], "310");
                assert!(assessment["claimId"].is_null());
            }
            {
                let tx = db
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .unwrap();
                let registered = register(&tx, &register_request(), "synthetic-owner").unwrap();
                assert!(registered["claimId"].as_i64().unwrap() > 0);
                assert!(registered["postedJournalEntryId"].is_null());
                amount_is(&registered["registeredAmountDkk"], "310");
                tx.commit().unwrap();
            }
            assert_eq!(counts(&db), (1, 0, 1));
            {
                let tx = db
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .unwrap();
                let posted = post(
                    &tx,
                    &post_request(),
                    "synthetic-owner",
                    Date::parse("2026-03-02").unwrap(),
                )
                .unwrap();
                assert_eq!(posted["entryId"], posted["postedJournalEntryId"]);
                assert!(posted["postedJournalEntryNo"].as_str().is_some());
                assert_eq!(posted["principalCurrency"], currency);
                amount_is(&posted["principalOpenBalance"], "125");
                amount_is(&posted["registeredAmountDkk"], "310");
                tx.commit().unwrap();
            }
            assert_eq!(counts(&db), (1, 1, 2));
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .unwrap();
            let same = calculate(&tx, &calculate_request()).unwrap();
            assert_eq!(same["postedJournalEntryId"], 2);
            let recovery = calculate(
                &tx,
                &Calculate {
                    invoice_document_id: 1,
                    as_of_date: "2026-01-31".into(),
                    compensation_amount_dkk: None,
                },
            )
            .unwrap();
            assert_eq!(recovery["eligible"], false);
            assert_eq!(recovery["claimId"], same["claimId"]);
            assert_eq!(
                recovery["postedJournalEntryId"],
                same["postedJournalEntryId"]
            );
            amount_is(&recovery["registeredAmountDkk"], "310");
            let detail:(String,Option<f64>,Option<f64>,Option<f64>,String)=tx.query_row("SELECT currency,amount_foreign,amount_dkk,fx_rate_to_dkk,created_by FROM journal_entries WHERE id=2",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
            assert_eq!(
                detail,
                (
                    "DKK".into(),
                    None,
                    None,
                    None,
                    "user:synthetic-owner".into()
                )
            );
            let actual:Vec<(String,String,String)>=tx.prepare("SELECT a.account_no,CAST(l.debit_amount AS TEXT),CAST(l.credit_amount AS TEXT) FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=2 ORDER BY a.account_no").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<Result<_,_>>().unwrap();
            assert_eq!(
                actual,
                vec![
                    ("CI".into(), "0".into(), "310".into()),
                    ("CR".into(), "310".into(), "0".into())
                ]
            );
            let audits:i64=tx.query_row("SELECT count(*) FROM audit_log WHERE event_type IN ('invoice_compensation_register','invoice_compensation_post') AND entity_type='invoice_compensation_claim' AND actor='user:synthetic-owner'",[],|r|r.get(0)).unwrap();
            assert_eq!(audits, 2);
        }
    }
    #[test]
    fn calculation_is_readonly_and_private_or_not_due_is_ineligible() {
        for commercial in [false, true] {
            let mut db = fixture("DKK", commercial);
            let before = counts(&db);
            db.execute_batch("PRAGMA query_only=ON").unwrap();
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .unwrap();
            let mut req = calculate_request();
            if commercial {
                req.as_of_date = "2026-01-31".into();
            }
            let value = calculate(&tx, &req).unwrap();
            assert_eq!(value["eligible"], false);
            amount_is(&value["compensationAmountDkk"], "0");
            drop(tx);
            assert_eq!(counts(&db), before);
        }
    }
    #[test]
    fn duplicates_unconfirmed_overcap_and_bad_post_accounts_dates_do_not_add_effects() {
        let mut db = fixture("DKK", true);
        for fee in [Some(json!(310.01)), Some(json!(0))] {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let mut req = register_request();
            req.compensation_amount_dkk = fee.map(|n| n.as_number().unwrap().clone());
            assert!(register(&tx, &req, "synthetic-owner").is_err());
        }
        assert_eq!(counts(&db), (0, 0, 1));
        {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let mut req = register_request();
            req.confirm = false;
            assert!(register(&tx, &req, "synthetic-owner").is_err());
        }
        assert_eq!(counts(&db), (0, 0, 1));
        {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            register(&tx, &register_request(), "synthetic-owner").unwrap();
            tx.commit().unwrap();
        }
        {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            assert!(register(&tx, &register_request(), "synthetic-owner").is_err());
        }
        for case in 0..5 {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let mut req = post_request();
            match case {
                0 => req.confirm = false,
                1 => req.transaction_date = Some("2026-03-01".into()),
                2 => req.receivable_account_no = "S".into(),
                3 => req.compensation_income_account_no = "R".into(),
                _ => {
                    tx.execute("UPDATE accounts SET active=0 WHERE account_no='CI'", [])
                        .unwrap();
                }
            }
            assert!(
                post(
                    &tx,
                    &req,
                    "synthetic-owner",
                    Date::parse("2026-03-02").unwrap()
                )
                .is_err()
            );
        }
        assert_eq!(counts(&db), (1, 0, 1));
        {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            post(
                &tx,
                &post_request(),
                "synthetic-owner",
                Date::parse("2026-03-02").unwrap(),
            )
            .unwrap();
            tx.commit().unwrap();
        }
        {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            assert!(
                post(
                    &tx,
                    &post_request(),
                    "synthetic-owner",
                    Date::parse("2026-03-02").unwrap()
                )
                .is_err()
            );
        }
        assert_eq!(counts(&db), (1, 1, 2));
    }
    #[test]
    fn closed_period_posting_refuses_without_claim_journal() {
        let mut db = fixture("DKK", true);
        {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            register(&tx, &register_request(), "synthetic-owner").unwrap();
            tx.commit().unwrap();
        }
        {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            tx.execute("INSERT INTO accounting_periods(period_start,period_end,kind,status)VALUES('2026-03-01','2026-03-31','custom','closed')",[]).unwrap();
            assert!(
                post(
                    &tx,
                    &post_request(),
                    "synthetic-owner",
                    Date::parse("2026-03-02").unwrap()
                )
                .is_err()
            );
        }
        assert_eq!(counts(&db), (1, 0, 1));
    }
    #[test]
    fn original_snapshot_hash_and_nondefault_registered_amount_are_authoritative() {
        let mut bad = fixture_hash("DKK", true, true);
        let before = counts(&bad);
        let tx = bad
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .unwrap();
        assert!(calculate(&tx, &calculate_request()).is_err());
        drop(tx);
        assert_eq!(counts(&bad), before);
        let mut db = fixture("DKK", true);
        {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let mut req = register_request();
            req.compensation_amount_dkk = Some(serde_json::Number::from_f64(30.05).unwrap());
            let value = register(&tx, &req, "synthetic-owner").unwrap();
            assert_eq!(value["registeredAmountDkk"], json!(30.05));
            tx.commit().unwrap();
        }
        {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let value = post(
                &tx,
                &post_request(),
                "synthetic-owner",
                Date::parse("2026-03-02").unwrap(),
            )
            .unwrap();
            assert_eq!(value["compensationAmountDkk"], json!(30.05));
            assert_eq!(value["registeredAmountDkk"], json!(30.05));
            tx.commit().unwrap();
        }
    }
    #[test]
    fn dto_rejects_actor_unknown_and_confirmation_in_calculation() {
        assert!(
            serde_json::from_value::<Calculate>(
                json!({"invoiceDocumentId":1,"asOfDate":"2026-03-02","confirm":true})
            )
            .is_err()
        );
        assert!(serde_json::from_value::<Register>(json!({"invoiceDocumentId":1,"asOfDate":"2026-03-02","confirm":true,"actor":"forged"})).is_err());
    }
}
