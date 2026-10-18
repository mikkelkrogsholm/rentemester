//! Native bank master data and imported statement rows on the admitted transaction.
use rentemester_engine::{Error, bank::Row, currency::Currency, money::Money};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
fn fail(message: impl Into<String>) -> Error {
    Error::new("bank_refused", message)
}
fn sql(_: rusqlite::Error) -> Error {
    fail("bank storage unavailable; abort transaction")
}
fn audit(
    tx: &Transaction<'_>,
    event: &str,
    id: &str,
    message: &str,
    actor: &str,
) -> Result<(), Error> {
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'bank',?2,?3,?4)",params![event,id,message,format!("user:{actor}")]).map_err(sql)?;
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AccountInput {
    pub name: String,
    pub slug: Option<String>,
    pub bank_name: Option<String>,
    pub registration_no: Option<String>,
    pub account_no: Option<String>,
    pub iban: Option<String>,
    pub bic: Option<String>,
    pub account_owner: Option<String>,
    pub customer_no: Option<String>,
    pub currency: Option<String>,
    pub ledger_account_no: Option<String>,
}
pub fn create_account(
    tx: &Transaction<'_>,
    input: &AccountInput,
    actor: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let name = input.name.trim();
    if name.is_empty() || name.len() > 256 {
        return Err(fail("bank account name required (max 256 bytes)"));
    }
    let slug = input
        .slug
        .as_deref()
        .unwrap_or(name)
        .trim()
        .to_lowercase()
        .replace('æ', "ae")
        .replace('ø', "o")
        .replace('å', "a")
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() || slug.len() > 128 {
        return Err(fail("a valid bank account slug is required"));
    }
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bank_accounts WHERE slug=?1)",
            [&slug],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if exists {
        return Err(fail("bank account slug already exists"));
    }
    let currency = input
        .currency
        .as_deref()
        .unwrap_or("DKK")
        .trim()
        .to_uppercase();
    Currency::parse(&currency)?;
    let ledger = input
        .ledger_account_no
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(no) = ledger {
        let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND type='asset' AND active=1)",[no],|r|r.get(0)).map_err(sql)?;
        if !valid {
            return Err(fail("bank ledger account must be an active asset account"));
        }
    }
    let clean = |s: &Option<String>| {
        s.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let id:i64=tx.query_row("INSERT INTO bank_accounts(slug,name,bank_name,registration_no,account_no,iban,bic,account_owner,customer_no,currency,ledger_account_no) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) RETURNING id",params![slug,name,clean(&input.bank_name),clean(&input.registration_no),clean(&input.account_no),clean(&input.iban),clean(&input.bic),clean(&input.account_owner),clean(&input.customer_no),currency,ledger],|r|r.get(0)).map_err(sql)?;
    audit(
        tx,
        "bank_account_add",
        &id.to_string(),
        &format!("Added bank account '{slug}' ({name})"),
        actor,
    )?;
    Ok(json!({"id":id,"slug":slug}))
}
fn company(tx: &Transaction<'_>) -> Result<Value, Error> {
    tx.query_row("SELECT name,cvr,country,currency FROM companies WHERE id=1",[],|r|Ok(json!({"name":r.get::<_,String>(0)?,"cvr":r.get::<_,Option<String>>(1)?,"country":r.get::<_,String>(2)?,"currency":r.get::<_,String>(3)?}))).map_err(sql)
}
pub fn accounts(tx: &Transaction<'_>, slug: &str) -> Result<Value, Error> {
    let mut q=tx.prepare("SELECT id,slug,name,bank_name,registration_no,account_no,iban,bic,account_owner,customer_no,currency,ledger_account_no,active,created_at FROM bank_accounts ORDER BY id").map_err(sql)?;
    let rows=q.query_map([],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"slug":r.get::<_,String>(1)?,"name":r.get::<_,String>(2)?,"bankName":r.get::<_,Option<String>>(3)?,"registrationNo":r.get::<_,Option<String>>(4)?,"accountNo":r.get::<_,Option<String>>(5)?,"iban":r.get::<_,Option<String>>(6)?,"bic":r.get::<_,Option<String>>(7)?,"accountOwner":r.get::<_,Option<String>>(8)?,"customerNo":r.get::<_,Option<String>>(9)?,"currency":r.get::<_,String>(10)?,"ledgerAccountNo":r.get::<_,Option<String>>(11)?,"active":r.get::<_,i64>(12)?==1,"createdAt":r.get::<_,String>(13)?}))).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?;
    Ok(json!({"slug":slug,"company":company(tx)?,"accounts":rows,"profiles":[]}))
}
pub(crate) fn signed_money(value: rusqlite::types::ValueRef<'_>) -> Result<Money, Error> {
    use rusqlite::types::ValueRef;
    match value {
        ValueRef::Real(value) if value < 0.0 => {
            Ok(Money::default().subtract(&crate::legacy_money(ValueRef::Real(-value))?))
        }
        ValueRef::Integer(value) if value < 0 => {
            let positive = value
                .checked_neg()
                .ok_or_else(|| fail("bank amount exceeds supported numeric domain"))?;
            Ok(Money::default().subtract(&crate::legacy_money(ValueRef::Integer(positive))?))
        }
        value => crate::legacy_money(value),
    }
}
fn quoted(s: &str) -> String {
    serde_json::to_string(s).expect("string encoding")
}
fn numeric(m: &Money) -> Result<String, Error> {
    Ok(ryu_js::Buffer::new()
        .format_finite(crate::journal_post::money(m)?)
        .to_owned())
}
fn optional_string(s: Option<&str>) -> String {
    s.map(quoted).unwrap_or_else(|| "null".into())
}
fn fingerprint(row: &Row, account: Option<i64>, occurrence: usize) -> Result<String, Error> {
    let rate = row
        .rate
        .as_ref()
        .map(crate::journal_post::rate)
        .transpose()?
        .map(|n| ryu_js::Buffer::new().format_finite(n).to_owned())
        .unwrap_or_else(|| "null".into());
    // Exact existing JS field order and number spelling keep re-import identity stable.
    let wire = format!(
        "{{\"bank_account_id\":{},\"transaction_date\":{},\"booking_date\":{},\"text\":{},\"amount\":{},\"currency\":{},\"reference\":{},\"amount_dkk\":{},\"fx_rate_to_dkk\":{},\"occurrence\":{}}}",
        account
            .map(|n| n.to_string())
            .unwrap_or_else(|| "null".into()),
        quoted(&row.date),
        optional_string(row.booking_date.as_deref()),
        quoted(&row.text),
        numeric(&row.amount)?,
        quoted(&row.currency),
        optional_string(row.reference.as_deref()),
        row.amount_dkk
            .as_ref()
            .map(numeric)
            .transpose()?
            .unwrap_or_else(|| "null".into()),
        rate,
        occurrence
    );
    Ok(format!("{:x}", Sha256::digest(wire.as_bytes())))
}
pub fn import(
    tx: &Transaction<'_>,
    content: &str,
    rows: &[Row],
    account: Option<&str>,
    actor: &str,
    now: &str,
    archive: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let resolved=account.filter(|s|!s.trim().is_empty()).map(|s|{
        let row:Option<(i64,String,i64)>=tx.query_row("SELECT id,slug,active FROM bank_accounts WHERE id=?1 OR slug=?2 ORDER BY (id=?1) DESC LIMIT 1",params![s.parse::<i64>().ok(),s],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(sql)?;
        let row=row.ok_or_else(||fail("unknown bank account"))?;if row.2!=1{return Err(fail("bank account is inactive"));}Ok(row)
    }).transpose()?;
    let account_id = resolved.as_ref().map(|r| r.0);
    let source_hash = format!("{:x}", Sha256::digest(content.as_bytes()));
    let batch = format!(
        "BANK-{}-{}",
        now.chars()
            .filter(char::is_ascii_digit)
            .take(14)
            .collect::<String>(),
        &source_hash[..8]
    );
    let month: i32 = tx
        .query_row(
            "SELECT fiscal_year_start_month FROM companies WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let mut occurrence = HashMap::new();
    let mut imported = 0;
    let mut skipped = Vec::new();
    for row in rows {
        let content_hash = fingerprint(row, account_id, 0)?;
        let count = occurrence.entry(content_hash).or_insert(0);
        let hash = fingerprint(row, account_id, *count)?;
        *count += 1;
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM bank_transactions WHERE transaction_hash=?1)",
                [&hash],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if exists {
            skipped.push(format!(
                "{} {} {} {}",
                row.date,
                row.text,
                row.amount.decimal(),
                row.currency
            ));
            continue;
        }
        let retain = rentemester_engine::calendar::retention_deadline(
            row.booking_date.as_deref().unwrap_or(&row.date),
            month as u32,
        )?;
        tx.execute("INSERT INTO bank_transactions(transaction_date,booking_date,text,amount,currency,reference,amount_dkk,fx_rate_to_dkk,source_file_hash,import_batch_id,transaction_hash,status,retain_until,bank_account_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'imported',?12,?13)",params![row.date,row.booking_date,row.text,crate::journal_post::money(&row.amount)?,row.currency,row.reference,row.amount_dkk.as_ref().map(crate::journal_post::money).transpose()?,row.rate.as_ref().map(crate::journal_post::rate).transpose()?,source_hash,batch,hash,retain,account_id]).map_err(sql)?;
        imported += 1;
    }
    audit(
        tx,
        "bank_import",
        &batch,
        &format!(
            "Imported {imported} bank rows; skipped {} duplicates; source {source_hash}; original {archive}",
            skipped.len()
        ),
        actor,
    )?;
    Ok(
        json!({"importBatchId":batch,"imported":imported,"skippedDuplicates":skipped.len(),"skippedDuplicateRows":skipped,"bankAccountSlug":resolved.map(|r|r.1),"profile":Value::Null,"balanceWarnings":[],"exceptionsCreated":0}),
    )
}
pub fn read(
    tx: &Transaction<'_>,
    slug: &str,
    today: &str,
    selected: Option<&str>,
) -> Result<Value, Error> {
    use rentemester_engine::fiscal_year::{self, Date, LabelStrategy, StartMonth};
    let (month, strategy): (i32, String) = tx
        .query_row(
            "SELECT fiscal_year_start_month,fiscal_year_label_strategy FROM companies WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(sql)?;
    let month = StartMonth::new(month)?;
    let strategy = LabelStrategy::try_from(strategy.as_str())?;
    let current = fiscal_year::for_date(Date::parse(today)?, month, strategy)?;
    let current_label = current.display_label.clone();
    let mut years = std::collections::BTreeMap::new();
    years.insert(current_label.clone(), current);
    let mut dates=tx.prepare("SELECT DISTINCT transaction_date FROM bank_transactions UNION SELECT DISTINCT transaction_date FROM journal_entries").map_err(sql)?;
    for date in dates
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(sql)?
    {
        let year = fiscal_year::for_date(Date::parse(&date.map_err(sql)?)?, month, strategy)?;
        years.insert(year.display_label.clone(), year);
    }
    let label = selected.unwrap_or(&current_label);
    let year = years
        .get(label)
        .ok_or_else(|| fail("unknown fiscal year"))?;
    let start = year.start.to_string();
    let end = year.end.to_string();
    let all_accounts = accounts(tx, slug)?;
    let mut q=tx.prepare("SELECT bt.id,bt.transaction_date,bt.text,COALESCE(bt.amount_dkk,bt.amount),br.journal_entry_no FROM bank_transactions bt LEFT JOIN bank_journal_reconciliations br ON br.bank_transaction_id=bt.id WHERE bt.transaction_date BETWEEN ?1 AND ?2 ORDER BY bt.transaction_date,bt.id").map_err(sql)?;
    let transactions=q.query_map(params![start,end],|r|{let journal:Option<String>=r.get(4)?;Ok(json!({"id":r.get::<_,i64>(0)?,"date":r.get::<_,String>(1)?,"text":r.get::<_,String>(2)?,"amount":r.get::<_,f64>(3)?,"runningBalance":Value::Null,"reconciliationStatus":if journal.is_some(){"matched"}else{"unmatched"},"journalEntryNo":journal}))}).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?;
    let matched = transactions
        .iter()
        .filter(|v| v["reconciliationStatus"] == "matched")
        .count();
    let mut booked = Money::default();
    let mut balances=tx.prepare("SELECT jl.debit_amount,jl.credit_amount FROM journal_lines jl JOIN journal_entries je ON je.id=jl.journal_entry_id JOIN accounts a ON a.id=jl.account_id WHERE je.status IN ('posted','reversed') AND je.transaction_date<=?1 AND a.account_no IN (SELECT ledger_account_no FROM bank_accounts WHERE ledger_account_no IS NOT NULL)").map_err(sql)?;
    let mut rows = balances.query([&end]).map_err(sql)?;
    while let Some(r) = rows.next().map_err(sql)? {
        booked.add(&crate::legacy_money(r.get_ref(0).map_err(sql)?)?);
        booked.add(&Money::default().subtract(&crate::legacy_money(r.get_ref(1).map_err(sql)?)?));
    }
    let fiscal_years:Vec<_>=years.values().map(|y|json!({"label":y.display_label,"start":y.start.to_string(),"end":y.end.to_string(),"source":"live"})).collect();
    Ok(
        json!({"slug":slug,"selectedYear":label,"archived":false,"company":all_accounts["company"],"fiscalYears":fiscal_years,"periodStart":start,"periodEnd":end,"accounts":all_accounts["accounts"],"bookedBalance":crate::cockpit::number(&booked)?,"actualBalance":Value::Null,"difference":Value::Null,"bankStatementStatus":if transactions.is_empty(){"none"}else{"no-balance-column"},"matchedCount":matched,"unmatchedCount":transactions.len()-matched,"transactions":transactions}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_identical_payments_keep_occurrences_and_account_identity() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut company = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic Bank").unwrap();
        let tx = company
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic cash','asset','debit');").unwrap();
        let account = |name: &str| {
            serde_json::from_value::<AccountInput>(json!({"name":name,"ledgerAccountNo":"A"}))
                .unwrap()
        };
        create_account(&tx, &account("First"), "synthetic").unwrap();
        create_account(&tx, &account("Second"), "synthetic").unwrap();
        let csv = "date;text;amount\n2026-10-18;Synthetic payment;-250\n2026-10-18;Synthetic payment;-250\n";
        let rows = rentemester_engine::bank::parse(csv).unwrap();
        assert_eq!(
            fingerprint(&rows[0], Some(1), 0).unwrap(),
            "5b9e36c21c0a56c3bba25efcf79fb01d2493cd311c43196b55298538a21ee1b0"
        );
        let first = import(
            &tx,
            csv,
            &rows,
            Some("first"),
            "synthetic",
            "2026-10-18T12:00:00Z",
            "synthetic-original",
        )
        .unwrap();
        assert_eq!(first["imported"], 2);
        let repeated = import(
            &tx,
            csv,
            &rows,
            Some("first"),
            "synthetic",
            "2026-10-18T12:01:00Z",
            "synthetic-original",
        )
        .unwrap();
        assert_eq!(repeated["imported"], 0);
        assert_eq!(repeated["skippedDuplicates"], 2);
        assert_eq!(
            import(
                &tx,
                csv,
                &rows,
                Some("second"),
                "synthetic",
                "2026-10-18T12:02:00Z",
                "synthetic-original"
            )
            .unwrap()["imported"],
            2
        );
        assert!(
            import(
                &tx,
                csv,
                &rows,
                Some("unknown"),
                "synthetic",
                "2026-10-18T12:03:00Z",
                "synthetic-original"
            )
            .is_err()
        );
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM bank_transactions", [], |r| r.get(0))
                .unwrap(),
            4
        );
    }
}
