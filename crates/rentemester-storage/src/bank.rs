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
pub(crate) fn fingerprint(
    row: &Row,
    account: Option<i64>,
    occurrence: usize,
) -> Result<String, Error> {
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
            if let Some(balance) = &row.balance_after {
                let stored: Option<Money> = tx
                    .query_row(
                        "SELECT balance_after FROM bank_transactions WHERE transaction_hash=?1",
                        [&hash],
                        |r| match r.get_ref(0)? {
                            rusqlite::types::ValueRef::Null => Ok(None),
                            value => signed_money(value)
                                .map(Some)
                                .map_err(|_| rusqlite::Error::InvalidQuery),
                        },
                    )
                    .map_err(sql)?;
                if stored.as_ref() != Some(balance) {
                    return Err(fail(
                        "duplicate bank transaction has different or previously missing statement balance; source evidence requires explicit correction",
                    ));
                }
            }
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
        tx.execute("INSERT INTO bank_transactions(transaction_date,booking_date,text,amount,currency,reference,amount_dkk,fx_rate_to_dkk,source_file_hash,import_batch_id,transaction_hash,status,retain_until,bank_account_id,balance_after) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'imported',?12,?13,?14)",params![row.date,row.booking_date,row.text,crate::journal_post::money(&row.amount)?,row.currency,row.reference,row.amount_dkk.as_ref().map(crate::journal_post::money).transpose()?,row.rate.as_ref().map(crate::journal_post::rate).transpose()?,source_hash,batch,hash,retain,account_id,row.balance_after.as_ref().map(crate::journal_post::money).transpose()?]).map_err(sql)?;
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
    let mut q=tx.prepare("SELECT bt.id,bt.transaction_date,bt.text,COALESCE(bt.amount_dkk,bt.amount),br.journal_entry_no,bt.balance_after,bt.currency,ba.currency,EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=br.journal_entry_id),br.reconciliation_id,link.note,link.created_by,link.source_reference FROM bank_transactions bt LEFT JOIN bank_accounts ba ON ba.id=bt.bank_account_id LEFT JOIN bank_journal_reconciliations br ON br.bank_transaction_id=bt.id LEFT JOIN bank_journal_reconciliation_links link ON link.bank_transaction_id=bt.id AND br.link_kind='append-only' WHERE bt.transaction_date BETWEEN ?1 AND ?2 ORDER BY bt.transaction_date,bt.id").map_err(sql)?;
    let transactions=q.query_map(params![start,end],|r|{let journal:Option<String>=r.get(4)?;let reversed:bool=r.get(8)?;Ok(json!({"id":r.get::<_,i64>(0)?,"date":r.get::<_,String>(1)?,"text":r.get::<_,String>(2)?,"amount":r.get::<_,f64>(3)?,"runningBalance":if r.get::<_,Option<String>>(7)?.as_deref().unwrap_or(&r.get::<_,String>(6)?)=="DKK" {r.get::<_,Option<f64>>(5)?}else{None},"reconciliationStatus":if journal.is_some() && !reversed{"matched"}else{"unmatched"},"reconciliationProblem":if reversed{Some("Tilknyttet postering er tilbageført; særskilt linkkorrektion kræves.")}else{None},"journalEntryNo":journal,"reconciliationId":r.get::<_,Option<String>>(9)?,"reconciliationNote":r.get::<_,Option<String>>(10)?,"reconciliationActor":r.get::<_,Option<String>>(11)?,"reconciliationProof":r.get::<_,Option<String>>(12)?}))}).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?;
    let matched = transactions
        .iter()
        .filter(|v| v["reconciliationStatus"] == "matched")
        .count();
    let mut booked = Money::default();
    let mut balances=tx.prepare("SELECT jl.debit_amount,jl.credit_amount FROM journal_lines jl JOIN journal_entries je ON je.id=jl.journal_entry_id JOIN accounts a ON a.id=jl.account_id WHERE je.status IN ('posted','reversed') AND je.transaction_date<=?1 AND a.account_no IN (SELECT ledger_account_no FROM bank_accounts WHERE ledger_account_no IS NOT NULL UNION SELECT account_no FROM account_role_mappings WHERE role='bank' AND status='confirmed')").map_err(sql)?;
    let mut rows = balances.query([&end]).map_err(sql)?;
    while let Some(r) = rows.next().map_err(sql)? {
        booked.add(&crate::legacy_money(r.get_ref(0).map_err(sql)?)?);
        booked.add(&Money::default().subtract(&crate::legacy_money(r.get_ref(1).map_err(sql)?)?));
    }
    let statement = statement_balance(tx, &end)?;
    let difference = statement
        .balance
        .as_ref()
        .map(|balance| crate::cockpit::number(&booked.subtract(balance)))
        .transpose()?;
    let fiscal_years:Vec<_>=years.values().map(|y|json!({"label":y.display_label,"start":y.start.to_string(),"end":y.end.to_string(),"source":"live"})).collect();
    Ok(
        json!({"slug":slug,"selectedYear":label,"archived":false,"manualReconciliationAvailable":true,"reconciliationCorrectionAvailable":false,"company":all_accounts["company"],"fiscalYears":fiscal_years,"periodStart":start,"periodEnd":end,"accounts":all_accounts["accounts"],"bookedBalance":crate::cockpit::number(&booked)?,"actualBalance":statement.balance.as_ref().map(crate::cockpit::number).transpose()?,"difference":difference,"bankStatementStatus":statement.status,"bankStatementProblem":statement.problem,"matchedCount":matched,"unmatchedCount":transactions.len()-matched,"transactions":transactions}),
    )
}

/// Independent statement evidence; never synthesize a closing balance from the ledger.
pub(crate) struct StatementBalance {
    pub balance: Option<Money>,
    pub status: &'static str,
    pub problem: Option<String>,
}
pub(crate) fn statement_balance(
    tx: &Transaction<'_>,
    cutoff: &str,
) -> Result<StatementBalance, Error> {
    let missing = |problem: String| StatementBalance {
        balance: None,
        status: "unavailable",
        problem: Some(problem),
    };
    let any: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bank_transactions WHERE transaction_date<=?1)",
            [cutoff],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if !any {
        return Ok(StatementBalance {
            balance: None,
            status: "none",
            problem: None,
        });
    }
    let balances:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM bank_transactions WHERE transaction_date<=?1 AND balance_after IS NOT NULL)",[cutoff],|r|r.get(0)).map_err(sql)?;
    if !balances {
        return Ok(StatementBalance {
            balance: None,
            status: "no-balance-column",
            problem: None,
        });
    }
    let mut q=tx.prepare("SELECT id,name,currency FROM bank_accounts WHERE ledger_account_no IS NOT NULL OR EXISTS(SELECT 1 FROM bank_transactions t WHERE t.bank_account_id=bank_accounts.id AND t.transaction_date<=?1) ORDER BY id").map_err(sql)?;
    let mut streams = q
        .query_map([cutoff], |r| {
            Ok((
                Some(r.get::<_, i64>(0)?),
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let unassigned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM bank_transactions WHERE bank_account_id IS NULL AND transaction_date<=?1)",[cutoff],|r|r.get(0)).map_err(sql)?;
    if unassigned {
        if !streams.is_empty() {
            return Ok(missing("Kontoudtoget indeholder både tilknyttede og ikke-tilknyttede bankkonti. Saldoernes sammenhæng skal afklares.".into()));
        }
        streams.push((None, "Ikke tilknyttet bankkonto".into(), "DKK".into()));
    }
    if streams.is_empty() {
        return Ok(missing(
            "Kontoudtogets bankkonto kan ikke bestemmes.".into(),
        ));
    }
    let mut total = Money::default();
    for (account, name, currency) in streams {
        if currency != "DKK" {
            return Ok(missing(format!(
                "Kontoudtogets saldo for {name} er i {currency}; en uafhængig DKK-slutsaldo mangler."
            )));
        }
        let latest:Option<String>=tx.query_row("SELECT MAX(transaction_date) FROM bank_transactions WHERE bank_account_id IS ?1 AND transaction_date<=?2",params![account,cutoff],|r|r.get(0)).map_err(sql)?;
        let Some(latest) = latest else {
            return Ok(missing(format!("Der mangler et kontoudtog for {name}.")));
        };
        let mut q=tx.prepare("SELECT COALESCE(amount_dkk,amount),balance_after,currency FROM bank_transactions WHERE bank_account_id IS ?1 AND transaction_date=?2 ORDER BY id").map_err(sql)?;
        let mut rows = q.query(params![account, latest]).map_err(sql)?;
        let mut candidates = vec![];
        while let Some(row) = rows.next().map_err(sql)? {
            if account.is_none() && row.get::<_, String>(2).map_err(sql)? != "DKK" {
                return Ok(missing(
                    "Bankkontoens saldovaluta skal afklares før en DKK-sammenligning.".into(),
                ));
            }
            let amount = signed_money(row.get_ref(0).map_err(sql)?)?;
            if row.get_ref(1).map_err(sql)? == rusqlite::types::ValueRef::Null {
                return Ok(missing(format!(
                    "Der mangler saldi på bankposter for {name} den {latest}."
                )));
            }
            let balance = signed_money(row.get_ref(1).map_err(sql)?)?;
            candidates.push((amount, balance));
        }
        let before: std::collections::HashSet<_> = candidates
            .iter()
            .map(|(amount, balance)| balance.subtract(amount).decimal())
            .collect();
        let endpoints: Vec<_> = candidates
            .iter()
            .filter(|(_, balance)| !before.contains(&balance.decimal()))
            .collect();
        if endpoints.len() != 1 {
            return Ok(missing(format!(
                "Rækkefølgen i kontoudtoget for {name} den {latest} giver ikke en entydig slutsaldo."
            )));
        }
        // A terminal balance alone is insufficient if unrelated or branching
        // row fragments were mixed into the same account/day.
        let mut degree: HashMap<String, i64> = HashMap::new();
        let mut adjacent: HashMap<String, Vec<String>> = HashMap::new();
        for (amount, balance) in &candidates {
            let from = balance.subtract(amount).decimal();
            let to = balance.decimal();
            *degree.entry(from.clone()).or_default() += 1;
            *degree.entry(to.clone()).or_default() -= 1;
            adjacent.entry(from.clone()).or_default().push(to.clone());
            adjacent.entry(to).or_default().push(from);
        }
        let end = endpoints[0].1.decimal();
        let mut seen = std::collections::HashSet::new();
        let mut pending = vec![end.clone()];
        while let Some(node) = pending.pop() {
            if seen.insert(node.clone()) {
                pending.extend(
                    adjacent
                        .get(&node)
                        .into_iter()
                        .flatten()
                        .filter(|n| !seen.contains(*n))
                        .cloned(),
                );
            }
        }
        if seen.len() != degree.len()
            || degree.values().filter(|v| **v == 1).count() != 1
            || degree.get(&end) != Some(&-1)
            || degree.values().any(|v| !(-1..=1).contains(v))
            || degree.values().filter(|v| **v == -1).count() != 1
        {
            return Ok(missing(format!(
                "Saldoerne i kontoudtoget for {name} den {latest} hænger ikke sammen i ét forløb."
            )));
        }
        total.add(&endpoints[0].1);
    }
    Ok(StatementBalance {
        balance: Some(total),
        status: "known",
        problem: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_statement_balance_ignores_import_order_and_refuses_missing_or_ambiguous_evidence()
     {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Bank").unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic bank','asset','debit');").unwrap();
        let account = |name: &str| {
            serde_json::from_value::<AccountInput>(json!({"name":name,"ledgerAccountNo":"A"}))
                .unwrap()
        };
        create_account(&tx, &account("First"), "synthetic").unwrap();
        // The ending row comes first: insertion order cannot pick the balance.
        let csv = "date;text;amount;saldo\n2026-10-18;Synthetic outgoing;-250;750\n2026-10-18;Synthetic incoming;1000;1000\n";
        let rows = rentemester_engine::bank::parse(csv).unwrap();
        import(
            &tx,
            csv,
            &rows,
            Some("first"),
            "synthetic",
            "2026-10-18T12:00:00Z",
            "synthetic-original",
        )
        .unwrap();
        let value = read(&tx, "synthetic", "2026-10-18", None).unwrap();
        assert_eq!(value["actualBalance"].as_f64(), Some(750.0));
        assert_eq!(value["difference"].as_f64(), Some(-750.0));
        assert_eq!(
            value["transactions"][0]["runningBalance"].as_f64(),
            Some(750.0)
        );
        assert_eq!(
            import(
                &tx,
                csv,
                &rows,
                Some("first"),
                "synthetic",
                "2026-10-18T12:00:00Z",
                "synthetic-original"
            )
            .unwrap()["imported"],
            0
        );
        let changed = csv.replace(";750\n", ";751\n");
        assert!(
            import(
                &tx,
                &changed,
                &rentemester_engine::bank::parse(&changed).unwrap(),
                Some("first"),
                "synthetic",
                "2026-10-18T12:00:00Z",
                "synthetic-original"
            )
            .is_err()
        );
        create_account(&tx, &account("Second"), "synthetic").unwrap();
        assert_eq!(
            read(&tx, "synthetic", "2026-10-18", None).unwrap()["bankStatementStatus"],
            "unavailable"
        );
        let second =
            "date;text;amount;balance_after\n2026-10-18;Synthetic second account;100;100\n";
        import(
            &tx,
            second,
            &rentemester_engine::bank::parse(second).unwrap(),
            Some("second"),
            "synthetic",
            "2026-10-18T12:00:00Z",
            "synthetic-original",
        )
        .unwrap();
        assert_eq!(
            read(&tx, "synthetic", "2026-10-18", None).unwrap()["actualBalance"].as_f64(),
            Some(850.0)
        );
        let ambiguous = "date;text;amount;balance_after\n2026-10-19;Synthetic ambiguous first;100;100\n2026-10-19;Synthetic ambiguous second;50;250\n";
        import(
            &tx,
            ambiguous,
            &rentemester_engine::bank::parse(ambiguous).unwrap(),
            Some("first"),
            "synthetic",
            "2026-10-19T12:00:00Z",
            "synthetic-original",
        )
        .unwrap();
        let value = read(&tx, "synthetic", "2026-10-19", None).unwrap();
        assert_eq!(value["actualBalance"], Value::Null);
        assert_eq!(value["difference"], Value::Null);
        assert_eq!(value["bankStatementStatus"], "unavailable");
        assert!(
            value["bankStatementProblem"]
                .as_str()
                .unwrap()
                .contains("entydig")
        );
        let disconnected = "date;text;amount;balance_after\n2026-10-20;Synthetic cycle one;100;100\n2026-10-20;Synthetic cycle two;-100;0\n2026-10-20;Synthetic disconnected;50;250\n";
        import(
            &tx,
            disconnected,
            &rentemester_engine::bank::parse(disconnected).unwrap(),
            Some("first"),
            "synthetic",
            "2026-10-20T12:00:00Z",
            "synthetic-original",
        )
        .unwrap();
        let value = read(&tx, "synthetic", "2026-10-20", None).unwrap();
        assert_eq!(value["actualBalance"], Value::Null);
        assert!(
            value["bankStatementProblem"]
                .as_str()
                .unwrap()
                .contains("hænger ikke sammen")
        );
    }
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
