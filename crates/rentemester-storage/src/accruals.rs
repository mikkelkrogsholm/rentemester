//! Append-only accrual registration and recognition through the native journal writer.
use crate::{invalid, legacy_money};
use rentemester_engine::{
    Error, accrual,
    currency::Currency,
    document::money,
    fiscal_year::Date,
    identifiers::DocumentId,
    manual_journal::ManualJournal,
    money::Money,
    sales_invoice::{refused, wire},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Number, Value, json};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Register {
    pub accrual_type: String,
    pub description: String,
    pub total_amount: Number,
    pub recognition_periods: u32,
    pub first_recognition_date: String,
    pub registration_date: String,
    pub period_step_months: u32,
    pub result_account_no: String,
    pub balance_account_no: String,
    pub settlement_account_no: String,
    pub document_id: i64,
    pub note: Option<String>,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Recognize {
    pub period_index: u32,
    pub transaction_date: String,
    pub settlement_account_no: Option<String>,
    pub confirm: bool,
}
struct Accrual {
    id: i64,
    kind: String,
    description: String,
    total: Money,
    periods: u32,
    first: String,
    step: u32,
    balance: String,
    result: String,
    document: i64,
    registration: i64,
}
fn load(tx: &Transaction<'_>, id: i64) -> Result<Accrual, Error> {
    let row=tx.query_row("SELECT id,accrual_type,description,total_amount,recognition_periods,first_recognition_date,period_step_months,balance_account_no,result_account_no,document_id,registration_journal_entry_id FROM accruals WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,legacy_money(r.get_ref(3)?),r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?))).optional().map_err(invalid)?.ok_or_else(||refused("accrual not found"))?;
    Ok(Accrual {
        id: row.0,
        kind: row.1,
        description: row.2,
        total: row.3?,
        periods: row.4,
        first: row.5,
        step: row.6,
        balance: row.7,
        result: row.8,
        document: row.9,
        registration: row.10,
    })
}
pub fn document_id(tx: &Transaction<'_>, id: i64) -> Result<i64, Error> {
    Ok(load(tx, id)?.document)
}
fn account(tx: &Transaction<'_>, no: &str, kind: &str, balance: &str) -> Result<(), Error> {
    let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND active=1 AND allow_direct_posting=1 AND type=?2 AND normal_balance=?3)",params![no,kind,balance],|r|r.get(0)).map_err(invalid)?;
    if !valid {
        return Err(refused(
            "select an active directly postable account of the required type and normal balance",
        ));
    }
    Ok(())
}
fn accounts(
    tx: &Transaction<'_>,
    kind: &str,
    balance: &str,
    result: &str,
    settlement: Option<&str>,
) -> Result<(), Error> {
    match kind {
        "prepaid_expense" => {
            account(tx, balance, "asset", "debit")?;
            account(tx, result, "expense", "debit")?;
        }
        "accrued_expense" => {
            account(tx, balance, "liability", "credit")?;
            account(tx, result, "expense", "debit")?;
        }
        "deferred_revenue" => {
            account(tx, balance, "liability", "credit")?;
            account(tx, result, "income", "credit")?;
        }
        _ => return Err(refused("unsupported accrual type")),
    }
    if let Some(s) = settlement {
        account(tx, s, "asset", "debit")?;
        if s == balance || s == result {
            return Err(refused("settlement and accrual accounts must differ"));
        }
    }
    Ok(())
}
fn audit(tx: &Transaction<'_>, id: i64, event: &str, actor: &str) -> Result<(), Error> {
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'accrual',?2,?3,?4)",params![event,id.to_string(),format!("{event} {id}"),format!("user:{actor}")]).map_err(invalid)?;
    Ok(())
}
fn post(
    tx: &Transaction<'_>,
    date: &str,
    description: &str,
    document: i64,
    debit: &str,
    credit: &str,
    amount: &Money,
    actor: &str,
    latest: Date,
) -> Result<i64, Error> {
    let request = ManualJournal {
        transaction_date: Date::parse(date)?,
        text: description.to_owned(),
        currency: Currency::parse("DKK")?,
        document_id: Some(DocumentId::from_sqlite(document)),
        source_bank_transaction_id: None,
        amount_foreign: None,
        amount_dkk: None,
        fx_rate_to_dkk: None,
        created_by_program: None,
        lines: vec![
            crate::expense::line(
                debit,
                amount.clone(),
                Money::default(),
                None,
                description.to_owned(),
            )?,
            crate::expense::line(
                credit,
                Money::default(),
                amount.clone(),
                None,
                description.to_owned(),
            )?,
        ],
    };
    let result = crate::journal_post::post(
        tx,
        &request,
        &crate::journal_preview::manual_journal_rule_version()?,
        latest,
        crate::journal_post::Actor {
            subject: &format!("user:{actor}"),
            program: "rentemester-rust",
        },
    )?;
    if result["ok"] != true {
        return Err(refused(result["errors"].to_string()));
    }
    result["entryId"]
        .as_i64()
        .ok_or_else(|| refused("missing accrual journal"))
}
fn intact(tx: &Transaction<'_>, id: i64) -> Result<(), Error> {
    let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_entries j WHERE j.id=?1 AND j.status='posted' AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id))",[id],|r|r.get(0)).map_err(invalid)?;
    if !valid {
        return Err(refused(
            "accrual journal evidence requires correction before continuing",
        ));
    }
    Ok(())
}
fn posted(tx: &Transaction<'_>, a: &Accrual) -> Result<(Vec<u32>, Money), Error> {
    intact(tx, a.registration)?;
    let mut q=tx.prepare("SELECT period_index,amount,journal_entry_id FROM accrual_schedule_postings WHERE accrual_id=?1 ORDER BY period_index").map_err(invalid)?;
    let mut rows = q.query([a.id]).map_err(invalid)?;
    let mut indices = vec![];
    let mut total = Money::default();
    while let Some(r) = rows.next().map_err(invalid)? {
        let index = r.get(0).map_err(invalid)?;
        let value = legacy_money(r.get_ref(1).map_err(invalid)?)?;
        intact(tx, r.get(2).map_err(invalid)?)?;
        if value != accrual::period(&a.total, a.periods, &a.first, a.step, index)?.1 {
            return Err(refused("accrual schedule amount differs from evidence"));
        }
        indices.push(index);
        total.add(&value);
    }
    Ok((indices, total))
}
fn schedule(tx: &Transaction<'_>, a: &Accrual) -> Result<Value, Error> {
    let (indices, _) = posted(tx, a)?;
    let mut rows = vec![];
    for index in 1..=a.periods {
        let (date, amount) = accrual::period(&a.total, a.periods, &a.first, a.step, index)?;
        rows.push(json!({"periodIndex":index,"recognitionDate":date,"amount":wire(&amount)?,"recognized":indices.contains(&index)}));
    }
    Ok(json!(rows))
}
pub fn preview(tx: &Transaction<'_>, id: i64) -> Result<Value, Error> {
    let a = load(tx, id)?;
    Ok(
        json!({"accrualId":id,"schedule":schedule(tx,&a)?,"documentId":a.document,"accrualType":a.kind}),
    )
}
pub fn register(
    tx: &Transaction<'_>,
    i: &Register,
    actor: &str,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !i.confirm
        || i.document_id <= 0
        || i.description.trim().is_empty()
        || i.description.len() > 500
        || i.note.as_ref().is_some_and(|s| s.len() > 4096)
    {
        return Err(refused(
            "description, original document and confirmation required",
        ));
    }
    let date = Date::parse(&i.registration_date)?;
    if date > Date::parse(&i.first_recognition_date)? {
        return Err(refused(
            "registration must not follow first recognition date",
        ));
    }
    let total = money(&i.total_amount)?;
    accrual::period(
        &total,
        i.recognition_periods,
        &i.first_recognition_date,
        i.period_step_months,
        i.recognition_periods,
    )?;
    accounts(
        tx,
        &i.accrual_type,
        &i.balance_account_no,
        &i.result_account_no,
        Some(&i.settlement_account_no),
    )?;
    let meta = crate::expense::metadata(tx, i.document_id)?;
    if meta.currency.as_deref().unwrap_or("DKK") != "DKK" {
        return Err(refused("accrual evidence must be DKK"));
    }
    let (debit, credit) = match i.accrual_type.as_str() {
        "prepaid_expense" => (&i.balance_account_no, &i.settlement_account_no),
        "accrued_expense" => (&i.result_account_no, &i.balance_account_no),
        _ => (&i.settlement_account_no, &i.balance_account_no),
    };
    let entry = post(
        tx,
        &i.registration_date,
        &format!("Accrual registration: {}", i.description.trim()),
        i.document_id,
        debit,
        credit,
        &total,
        actor,
        latest,
    )?;
    let id:i64=tx.query_row("INSERT INTO accruals(accrual_type,description,total_amount,recognition_periods,first_recognition_date,period_step_months,balance_account_no,result_account_no,document_id,registration_journal_entry_id,note) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) RETURNING id",params![i.accrual_type,i.description.trim(),crate::journal_post::money(&total)?,i.recognition_periods,i.first_recognition_date,i.period_step_months,i.balance_account_no,i.result_account_no,i.document_id,entry,i.note],|r|r.get(0)).map_err(invalid)?;
    audit(tx, id, "accrual_register", actor)?;
    Ok(json!({"accrualId":id,"entryId":entry,"schedule":schedule(tx,&load(tx,id)?)?}))
}
pub fn recognize(
    tx: &Transaction<'_>,
    id: i64,
    i: &Recognize,
    actor: &str,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !i.confirm {
        return Err(refused("recognition confirmation required"));
    }
    let a = load(tx, id)?;
    let (indices, _) = posted(tx, &a)?;
    if indices.contains(&i.period_index) {
        return Err(refused("accrual period already recognized"));
    }
    let (scheduled, amount) =
        accrual::period(&a.total, a.periods, &a.first, a.step, i.period_index)?;
    // Recognition cannot bring a future service period forward; a later open posting date remains explicit.
    if Date::parse(&i.transaction_date)? < Date::parse(&scheduled)? {
        return Err(refused("posting date cannot precede scheduled recognition"));
    }
    let settlement = if a.kind == "accrued_expense" {
        Some(
            i.settlement_account_no
                .as_deref()
                .ok_or_else(|| refused("choose settlement account explicitly"))?,
        )
    } else {
        None
    };
    accounts(tx, &a.kind, &a.balance, &a.result, settlement)?;
    let (debit, credit) = match a.kind.as_str() {
        "prepaid_expense" => (a.result.as_str(), a.balance.as_str()),
        "accrued_expense" => (a.balance.as_str(), settlement.unwrap()),
        _ => (a.balance.as_str(), a.result.as_str()),
    };
    let entry = post(
        tx,
        &i.transaction_date,
        &format!(
            "Accrual period {}/{}: {}",
            i.period_index, a.periods, a.description
        ),
        a.document,
        debit,
        credit,
        &amount,
        actor,
        latest,
    )?;
    tx.execute("INSERT INTO accrual_schedule_postings(accrual_id,period_index,recognition_date,amount,journal_entry_id) VALUES(?1,?2,?3,?4,?5)",params![id,i.period_index,i.transaction_date,crate::journal_post::money(&amount)?,entry]).map_err(invalid)?;
    audit(tx, id, "accrual_recognize", actor)?;
    Ok(
        json!({"accrualId":id,"entryId":entry,"periodIndex":i.period_index,"periodAmount":wire(&amount)?}),
    )
}
pub fn list(tx: &Transaction<'_>, slug: &str) -> Result<Value, Error> {
    let mut q = tx
        .prepare("SELECT id FROM accruals ORDER BY id")
        .map_err(invalid)?;
    let ids = q
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let mut rows = vec![];
    let mut total = Money::default();
    let mut recognized = Money::default();
    for id in ids {
        let a = load(tx, id)?;
        let (indices, value) = posted(tx, &a)?;
        total.add(&a.total);
        recognized.add(&value);
        rows.push(json!({"accrualId":id,"accrualType":a.kind,"description":a.description,"totalAmount":wire(&a.total)?,"recognitionPeriods":a.periods,"recognizedPeriods":indices.len(),"recognizedAmount":wire(&value)?,"remainingAmount":wire(&a.total.subtract(&value))?,"fullyRecognized":indices.len()==a.periods as usize,"balanceAccountNo":a.balance,"resultAccountNo":a.result,"firstRecognitionDate":a.first,"periodStepMonths":a.step}));
    }
    Ok(
        json!({"slug":slug,"nativeAccruals":true,"company":crate::company_setup::profile(tx)?,"report":{"ok":true,"accruals":rows,"totals":{"totalAmount":wire(&total)?,"recognizedAmount":wire(&recognized)?,"remainingAmount":wire(&total.subtract(&recognized))?},"errors":[]}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_kinds_preserve_exact_schedule_and_fail_closed() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Accruals").unwrap();
        let tx = db.transaction().unwrap();
        tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('P','Prepaid','asset','debit'),('L','Accrued','liability','credit'),('D','Deferred','liability','credit'),('E','Expense','expense','debit'),('I','Income','income','credit'),('B','Settlement','asset','debit');").unwrap();
        let meta=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-01-01","invoiceNo":"SYN-ACCRUAL","deliveryDescription":"Synthetic service","amountIncVat":1000,"vatAmount":0,"sender":{"name":"Synthetic Supplier","address":"Synthetic2","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic Company","address":"Synthetic1","vatOrCvr":"DK87654321"}})).unwrap();
        let doc = crate::documents::register(
            &tx,
            crate::documents::Original {
                filename: "synthetic.txt",
                path: "synthetic.txt",
                hash: &"a".repeat(64),
                mime: "text/plain",
            },
            &meta,
            "synthetic",
            "2026-10-19",
        )
        .unwrap()["document"]["id"]
            .as_i64()
            .unwrap();
        let latest = Date::parse("2026-10-19").unwrap();
        for (kind, balance, result) in [
            ("prepaid_expense", "P", "E"),
            ("accrued_expense", "L", "E"),
            ("deferred_revenue", "D", "I"),
        ] {
            let mut input:Register=serde_json::from_value(json!({"accrualType":kind,"description":"Synthetic service","totalAmount":1000,"recognitionPeriods":3,"firstRecognitionDate":"2026-01-31","registrationDate":"2026-01-01","periodStepMonths":1,"balanceAccountNo":balance,"resultAccountNo":result,"settlementAccountNo":"B","documentId":doc,"confirm":true})).unwrap();
            input.balance_account_no = "E".into();
            assert!(register(&tx, &input, "synthetic", latest).is_err());
            input.balance_account_no = balance.into();
            input.confirm = false;
            assert!(register(&tx, &input, "synthetic", latest).is_err());
            input.confirm = true;
            let registered = register(&tx, &input, "synthetic", latest).unwrap();
            let id = registered["accrualId"].as_i64().unwrap();
            assert_eq!(registered["schedule"][1]["recognitionDate"], "2026-02-28");
            assert_eq!(registered["schedule"][2]["recognitionDate"], "2026-03-31");
            assert_eq!(registered["schedule"][2]["amount"], 333.34);
            let mut request:Recognize=serde_json::from_value(json!({"periodIndex":1,"transactionDate":"2026-01-31","settlementAccountNo":"B","confirm":true})).unwrap();
            tx.execute_batch("INSERT OR IGNORE INTO accounting_periods(period_start,period_end,kind,status) VALUES('2026-01-31','2026-01-31','vat_period','closed');").unwrap();
            assert!(recognize(&tx, id, &request, "synthetic", latest).is_err());
            assert_eq!(
                preview(&tx, id).unwrap()["schedule"][0]["recognized"],
                false
            );
            request.transaction_date = "2026-10-19".into();
            for period in 1..=3 {
                request.period_index = period;
                let posted = recognize(&tx, id, &request, "synthetic", latest).unwrap();
                assert_eq!(
                    posted["periodAmount"],
                    if period == 3 { 333.34 } else { 333.33 }
                );
                assert!(recognize(&tx, id, &request, "synthetic", latest).is_err());
                let linked: i64 = tx
                    .query_row(
                        "SELECT document_id FROM journal_entries WHERE id=?1",
                        [posted["entryId"].as_i64().unwrap()],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(linked, doc);
            }
            assert_eq!(preview(&tx, id).unwrap()["schedule"][2]["recognized"], true);
        }
        let report = list(&tx, "synthetic").unwrap();
        assert_eq!(report["report"]["totals"]["totalAmount"], 3000.0);
        assert_eq!(report["report"]["totals"]["remainingAmount"], 0.0);
        let snapshot = crate::cockpit::read(&tx, "synthetic", "2026-10-19", None, None).unwrap();
        assert_eq!(snapshot.trial_balance["balanced"], true);
        assert_eq!(snapshot.income_statement["totalExpense"], 2000.0);
        assert_eq!(snapshot.income_statement["totalIncome"], 1000.0);
        let a = load(&tx, 1).unwrap();
        assert!(intact(&tx, a.registration + 100000).is_err());
    }
}
