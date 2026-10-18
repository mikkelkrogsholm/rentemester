//! Append-only asset register and depreciation using the admitted native writer.
use crate::{invalid, legacy_money};
use rentemester_engine::{
    Error, asset,
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
    pub name: String,
    pub category: String,
    pub acquisition_date: String,
    pub cost: Number,
    pub useful_life_months: u32,
    pub purchase_document_id: i64,
    pub asset_account_no: Option<String>,
    pub depreciation_expense_account_no: Option<String>,
    pub accumulated_depreciation_account_no: Option<String>,
    pub note: Option<String>,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Depreciate {
    pub transaction_date: Option<String>,
    pub period_index: Option<u32>,
    pub confirm: bool,
}
fn audit(tx: &Transaction<'_>, event: &str, id: i64, actor: &str) -> Result<(), Error> {
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'asset',?2,?3,?4)",params![event,id.to_string(),format!("{event} {id}"),format!("user:{actor}")]).map_err(invalid)?;
    Ok(())
}
fn account(
    tx: &Transaction<'_>,
    value: Option<&str>,
    kind: &str,
    balance: &str,
) -> Result<String, Error> {
    let value = value
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| refused("select all three asset and depreciation accounts explicitly"))?;
    let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND active=1 AND type=?2 AND normal_balance=?3 AND allow_direct_posting=1)",params![value,kind,balance],|r|r.get(0)).map_err(invalid)?;
    if !valid {
        return Err(refused(
            "asset/depreciation account must be active, directly postable and have the correct type and normal balance",
        ));
    }
    Ok(value.to_owned())
}
struct Asset {
    id: i64,
    name: String,
    cost: Money,
    months: u32,
    acquisition: String,
    expense: String,
    accumulated: String,
    document: i64,
}
fn load(tx: &Transaction<'_>, id: i64) -> Result<Asset, Error> {
    tx.query_row("SELECT id,name,cost,useful_life_months,acquisition_date,depreciation_expense_account_no,accumulated_depreciation_account_no,purchase_document_id FROM assets WHERE id=?1",[id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,legacy_money(r.get_ref(2)?),r.get::<_,u32>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?,r.get::<_,i64>(7)?))).optional().map_err(invalid)?.ok_or_else(||refused("asset not found")).and_then(|(id,name,cost,months,acquisition,expense,accumulated,document)|Ok(Asset{id,name,cost:cost?,months,acquisition,expense,accumulated,document}))
}
pub fn document_id(tx: &Transaction<'_>, id: i64) -> Result<i64, Error> {
    Ok(load(tx, id)?.document)
}
fn posted(tx: &Transaction<'_>, a: &Asset) -> Result<(Vec<u32>, Money), Error> {
    let mut q=tx.prepare("SELECT d.period_index,d.amount,j.status,EXISTS(SELECT 1 FROM journal_entries rev WHERE rev.reversal_of_entry_id=j.id) FROM asset_depreciation_entries d JOIN journal_entries j ON j.id=d.journal_entry_id WHERE d.asset_id=?1 ORDER BY d.period_index").map_err(invalid)?;
    let mut rows = q.query([a.id]).map_err(invalid)?;
    let mut indices = vec![];
    let mut total = Money::default();
    while let Some(r) = rows.next().map_err(invalid)? {
        let index: u32 = r.get(0).map_err(invalid)?;
        let value = legacy_money(r.get_ref(1).map_err(invalid)?)?;
        let status: String = r.get(2).map_err(invalid)?;
        if status != "posted"
            || r.get::<_, bool>(3).map_err(invalid)?
            || value != asset::amount(&a.cost, a.months, index)?
        {
            return Err(refused(
                "asset depreciation evidence requires correction before continuing",
            ));
        }
        total.add(&value);
        indices.push(index);
    }
    Ok((indices, total))
}
pub fn next(tx: &Transaction<'_>, id: i64) -> Result<Value, Error> {
    let a = load(tx, id)?;
    let (indices, _) = posted(tx, &a)?;
    let next = (1..=a.months).find(|i| !indices.contains(i));
    Ok(
        json!({"assetId":id,"totalPeriods":a.months,"postedPeriods":indices.len(),"remainingPeriods":a.months as usize-indices.len(),"nextPeriodIndex":next,"nextPeriodAmount":next.map(|i|asset::amount(&a.cost,a.months,i).and_then(|v|wire(&v))).transpose()?}),
    )
}
pub fn register(
    tx: &Transaction<'_>,
    i: &Register,
    actor: &str,
    today: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !i.confirm
        || i.purchase_document_id <= 0
        || [&i.name, &i.category]
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > 500)
        || i.note.as_ref().is_some_and(|s| s.len() > 4096)
    {
        return Err(refused(
            "asset name, category, purchase document and confirmation required",
        ));
    }
    if Date::parse(&i.acquisition_date)? > today {
        return Err(refused("asset acquisition must not be in the future"));
    }
    let cost = money(&i.cost)?;
    let first = asset::amount(&cost, i.useful_life_months, 1)?;
    let asset = account(tx, i.asset_account_no.as_deref(), "asset", "debit")?;
    let expense = account(
        tx,
        i.depreciation_expense_account_no.as_deref(),
        "expense",
        "debit",
    )?;
    let accumulated = account(
        tx,
        i.accumulated_depreciation_account_no.as_deref(),
        "asset",
        "credit",
    )?;
    if asset == accumulated {
        return Err(refused(
            "asset and accumulated depreciation accounts must differ",
        ));
    }
    let meta = crate::expense::metadata(tx, i.purchase_document_id)?;
    if meta.currency.as_deref().unwrap_or("DKK") != "DKK" {
        return Err(refused(
            "asset acquisition currently requires a DKK purchase document",
        ));
    }
    let kind: String = tx
        .query_row(
            "SELECT document_type FROM documents WHERE id=?1",
            [i.purchase_document_id],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if !matches!(kind.as_str(), "purchase_sale" | "cash_register_receipt") {
        return Err(refused("asset requires purchase evidence"));
    }
    // Registration describes an already capitalised purchase; never invent a ledger balance.
    let mut available = Money::default();
    let mut q=tx.prepare("SELECT l.debit_amount,l.credit_amount,j.transaction_date FROM journal_lines l JOIN accounts a ON a.id=l.account_id JOIN journal_entries j ON j.id=l.journal_entry_id WHERE j.document_id=?1 AND a.account_no=?2 ORDER BY j.id,l.id").map_err(invalid)?;
    let mut rows = q
        .query(params![i.purchase_document_id, asset])
        .map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        let date: String = r.get(2).map_err(invalid)?;
        if date.as_str() <= i.acquisition_date.as_str() {
            available.add(&legacy_money(r.get_ref(0).map_err(invalid)?)?);
            available = available.subtract(&legacy_money(r.get_ref(1).map_err(invalid)?)?);
        }
    }
    let mut q = tx
        .prepare("SELECT cost FROM assets WHERE purchase_document_id=?1 AND asset_account_no=?2")
        .map_err(invalid)?;
    let mut rows = q
        .query(params![i.purchase_document_id, asset])
        .map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        available = available.subtract(&legacy_money(r.get_ref(0).map_err(invalid)?)?);
    }
    if available.subtract(&cost).is_negative() {
        return Err(refused(
            "purchase must first be capitalised on the selected asset account; unallocated booked cost is insufficient",
        ));
    }
    let id:i64=tx.query_row("INSERT INTO assets(name,category,acquisition_date,cost,useful_life_months,asset_account_no,depreciation_expense_account_no,accumulated_depreciation_account_no,purchase_document_id,note) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) RETURNING id",params![i.name.trim(),i.category.trim(),i.acquisition_date,crate::journal_post::money(&cost)?,i.useful_life_months,asset,expense,accumulated,i.purchase_document_id,i.note.as_deref()],|r|r.get(0)).map_err(invalid)?;
    audit(tx, "asset_register", id, actor)?;
    Ok(json!({"assetId":id,"totalPeriods":i.useful_life_months,"periodAmount":wire(&first)?}))
}
pub fn depreciate(
    tx: &Transaction<'_>,
    id: i64,
    i: &Depreciate,
    actor: &str,
    today: &str,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !i.confirm {
        return Err(refused("depreciation confirmation required"));
    }
    let a = load(tx, id)?;
    let preview = next(tx, id)?;
    let index = i
        .period_index
        .or_else(|| preview["nextPeriodIndex"].as_u64().map(|n| n as u32))
        .ok_or_else(|| refused("asset is fully depreciated"))?;
    let (indices, _) = posted(tx, &a)?;
    if indices.contains(&index) {
        return Err(refused("depreciation period already posted"));
    }
    let value = asset::amount(&a.cost, a.months, index)?;
    let date = i.transaction_date.as_deref().unwrap_or(today);
    if Date::parse(date)? < Date::parse(&a.acquisition)? {
        return Err(refused("depreciation cannot precede acquisition"));
    }
    account(tx, Some(&a.expense), "expense", "debit")?;
    account(tx, Some(&a.accumulated), "asset", "credit")?;
    let request = ManualJournal {
        transaction_date: Date::parse(date)?,
        text: format!(
            "Depreciation period {index}/{} for asset {}",
            a.months, a.name
        ),
        currency: Currency::parse("DKK")?,
        document_id: Some(DocumentId::from_sqlite(a.document)),
        source_bank_transaction_id: None,
        amount_foreign: None,
        amount_dkk: None,
        fx_rate_to_dkk: None,
        created_by_program: None,
        lines: vec![
            crate::expense::line(
                &a.expense,
                value.clone(),
                Money::default(),
                None,
                format!("Depreciation {}", a.name),
            )?,
            crate::expense::line(
                &a.accumulated,
                Money::default(),
                value.clone(),
                None,
                format!("Accumulated depreciation {}", a.name),
            )?,
        ],
    };
    let journal = crate::journal_post::post(
        tx,
        &request,
        &crate::journal_preview::manual_journal_rule_version()?,
        latest,
        crate::journal_post::Actor {
            subject: &format!("user:{actor}"),
            program: "rentemester-rust",
        },
    )?;
    if journal["ok"] != true {
        return Err(refused(journal["errors"].to_string()));
    }
    let entry = journal["entryId"]
        .as_i64()
        .ok_or_else(|| refused("missing depreciation journal"))?;
    tx.execute("INSERT INTO asset_depreciation_entries(asset_id,period_index,transaction_date,amount,journal_entry_id) VALUES(?1,?2,?3,?4,?5)",params![id,index,date,crate::journal_post::money(&value)?,entry]).map_err(invalid)?;
    audit(tx, "asset_depreciation_post", id, actor)?;
    Ok(json!({"entryId":entry,"assetId":id,"periodIndex":index,"periodAmount":wire(&value)?}))
}
pub fn list(tx: &Transaction<'_>, slug: &str) -> Result<Value, Error> {
    let company = crate::company_setup::profile(tx)?;
    let mut q = tx
        .prepare("SELECT id,category FROM assets ORDER BY acquisition_date,id")
        .map_err(invalid)?;
    let references = q
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let mut rows = vec![];
    let mut cost = Money::default();
    let mut accumulated = Money::default();
    let mut active = 0;
    for (id, category) in references {
        let a = load(tx, id)?;
        let (indices, total) = posted(tx, &a)?;
        let remaining = a.months as usize - indices.len();
        if remaining > 0 {
            active += 1;
        }
        cost.add(&a.cost);
        accumulated.add(&total);
        rows.push(json!({"assetId":id,"name":a.name,"category":category,"acquisitionDate":a.acquisition,"cost":wire(&a.cost)?,"usefulLifeMonths":a.months,"postedPeriods":indices.len(),"accumulatedDepreciation":wire(&total)?,"netBookValue":wire(&a.cost.subtract(&total))?,"status":if remaining==0{"fully-depreciated"}else{"active"},"remainingPeriods":remaining}));
    }
    let mut q=tx.prepare("SELECT id,name,category,acquisition_date,writeoff_date,cost,expense_account_no,threshold_dkk,threshold_rule_source,note,purchase_document_id,journal_entry_id FROM asset_writeoffs ORDER BY writeoff_date,id").map_err(invalid)?;
    let mut writeoffs = vec![];
    let mut total = Money::default();
    let mut rs = q.query([]).map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        let value = legacy_money(r.get_ref(5).map_err(invalid)?)?;
        total.add(&value);
        writeoffs.push(json!({"id":r.get::<_,i64>(0).map_err(invalid)?,"name":r.get::<_,String>(1).map_err(invalid)?,"category":r.get::<_,String>(2).map_err(invalid)?,"acquisitionDate":r.get::<_,String>(3).map_err(invalid)?,"writeOffDate":r.get::<_,String>(4).map_err(invalid)?,"cost":wire(&value)?,"expenseAccountNo":r.get::<_,String>(6).map_err(invalid)?,"thresholdDkk":wire(&legacy_money(r.get_ref(7).map_err(invalid)?)?)?,"thresholdRuleSource":r.get::<_,String>(8).map_err(invalid)?,"note":r.get::<_,Option<String>>(9).map_err(invalid)?,"purchaseDocumentId":r.get::<_,i64>(10).map_err(invalid)?,"journalEntryId":r.get::<_,i64>(11).map_err(invalid)?}));
    }
    Ok(
        json!({"slug":slug,"company":company,"nativeAssetAccounts":true,"writeOffAvailable":false,"assets":rows,"writeOffs":writeoffs,"totals":{"cost":wire(&cost)?,"accumulatedDepreciation":wire(&accumulated)?,"netBookValue":wire(&cost.subtract(&accumulated))?,"activeCount":active,"fullyDepreciatedCount":rows.len()-active,"writeOffCount":writeoffs.len(),"writeOffTotal":wire(&total)?}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capitalized_purchase_depreciates_exactly_once_with_locked_period_protection() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Assets").unwrap();
        let tx = db.transaction().unwrap();
        tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic equipment','asset','debit'),('D','Synthetic depreciation','expense','debit'),('AD','Synthetic accumulated depreciation','asset','credit'),('C','Synthetic payable','liability','credit');").unwrap();
        let meta=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-07-01","invoiceNo":"SYN-ASSET-1","deliveryDescription":"Synthetic equipment","amountIncVat":1000,"vatAmount":0,"sender":{"name":"Synthetic Supplier","address":"Synthetic 2","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic Company","address":"Synthetic 1","vatOrCvr":"DK87654321"}})).unwrap();
        let document = crate::documents::register(
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
        let register_input: Register=serde_json::from_value(json!({"name":"Synthetic equipment","category":"equipment","acquisitionDate":"2026-07-01","cost":1000,"usefulLifeMonths":3,"purchaseDocumentId":document,"assetAccountNo":"A","depreciationExpenseAccountNo":"D","accumulatedDepreciationAccountNo":"AD","confirm":true})).unwrap();
        let today = Date::parse("2026-10-19").unwrap();
        assert!(register(&tx, &register_input, "synthetic", today).is_err());
        let acquisition:ManualJournal=serde_json::from_value(json!({"transactionDate":"2026-07-01","text":"Synthetic equipment purchase","currency":"DKK","documentId":document.to_string(),"lines":[{"accountNo":"A","debitAmount":"1000.00"},{"accountNo":"C","creditAmount":"1000.00"}]})).unwrap();
        let posted = crate::journal_post::post(
            &tx,
            &acquisition,
            &crate::journal_preview::manual_journal_rule_version().unwrap(),
            today,
            crate::journal_post::Actor {
                subject: "user:synthetic",
                program: "rentemester-rust",
            },
        )
        .unwrap();
        assert_eq!(posted["ok"], true, "{posted}");
        let registered = register(&tx, &register_input, "synthetic", today).unwrap();
        let id = registered["assetId"].as_i64().unwrap();
        assert!(register(&tx, &register_input, "synthetic", today).is_err());
        assert_eq!(registered["periodAmount"], 333.33);
        let mut request: Depreciate = serde_json::from_value(
            json!({"confirm":true,"periodIndex":1,"transactionDate":"2026-09-30"}),
        )
        .unwrap();
        tx.execute_batch("INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('2026-07-01','2026-09-30','vat_period','closed');").unwrap();
        assert!(depreciate(&tx, id, &request, "synthetic", "2026-10-19", today).is_err());
        assert_eq!(next(&tx, id).unwrap()["postedPeriods"], 0);
        request.transaction_date = Some("2026-10-19".into());
        let mut entries = vec![];
        for index in 1..=3 {
            request.period_index = Some(index);
            let result = depreciate(&tx, id, &request, "synthetic", "2026-10-19", today).unwrap();
            entries.push(result["entryId"].as_i64().unwrap());
            assert_eq!(
                result["periodAmount"],
                if index == 3 { 333.34 } else { 333.33 }
            );
            assert!(depreciate(&tx, id, &request, "synthetic", "2026-10-19", today).is_err());
        }
        let listed = list(&tx, "synthetic").unwrap();
        assert_eq!(listed["totals"]["netBookValue"], 0.0);
        assert_eq!(listed["totals"]["accumulatedDepreciation"], 1000.0);
        assert_eq!(listed["assets"][0]["status"], "fully-depreciated");
        assert_eq!(next(&tx, id).unwrap()["nextPeriodIndex"], Value::Null);
        let snapshot = crate::cockpit::read(&tx, "synthetic", "2026-10-19", None, None).unwrap();
        assert_eq!(snapshot.trial_balance["balanced"], true);
        assert_eq!(snapshot.income_statement["totalExpense"], 1000.0);
        entries.push(posted["entryId"].as_i64().unwrap());
        for entry in entries {
            let reverse=serde_json::from_value(json!({"entryId":entry.to_string(),"transactionDate":"2026-10-19","reason":"Synthetic refused correction"})).unwrap();
            assert_eq!(
                crate::journal_reversal::reverse(
                    &tx,
                    &reverse,
                    &crate::journal_preview::manual_journal_rule_version().unwrap(),
                    today,
                    crate::journal_post::Actor {
                        subject: "user:synthetic",
                        program: "synthetic"
                    }
                )
                .unwrap()["ok"],
                false
            );
        }
    }
}
