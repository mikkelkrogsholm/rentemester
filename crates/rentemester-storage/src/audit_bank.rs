//! Full read-only bank relationship assurance over legacy and native evidence.
//! Physical originals are independently read by the owning namespace adapter.
use crate::{bank_reconciliation as bank, invalid};
use rentemester_engine::{Error, money::Money, sales_invoice::wire};
use rusqlite::{Transaction, types::ValueRef};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
fn refused(message: impl Into<String>) -> Error {
    Error::new("bank_audit_invalid", message)
}
fn check(ok: bool, message: &str) -> Result<(), Error> {
    if ok { Ok(()) } else { Err(refused(message)) }
}
fn rows(tx: &Transaction<'_>, sql: &str) -> Result<Vec<Value>, Error> {
    let mut q = tx.prepare(sql).map_err(invalid)?;
    let names = q
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut iter = q.query([]).map_err(invalid)?;
    let mut out = vec![];
    while let Some(r) = iter.next().map_err(invalid)? {
        let mut v = serde_json::Map::new();
        for (i, name) in names.iter().enumerate() {
            v.insert(
                name.clone(),
                match r.get_ref(i).map_err(invalid)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(n) => json!(n),
                    ValueRef::Real(n) => serde_json::Number::from_f64(n)
                        .map(Value::Number)
                        .ok_or_else(|| refused("non-finite persisted bank number"))?,
                    ValueRef::Text(t) => json!(std::str::from_utf8(t).map_err(invalid)?),
                    _ => return Err(refused("unexpected bank evidence blob")),
                },
            );
        }
        out.push(Value::Object(v));
    }
    Ok(out)
}
fn id(v: &Value, k: &str) -> Result<i64, Error> {
    v[k].as_i64()
        .ok_or_else(|| refused(format!("missing integer {k}")))
}
fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str, Error> {
    v[k].as_str()
        .ok_or_else(|| refused(format!("missing text {k}")))
}
fn money(v: &Value) -> Result<Money, Error> {
    if let Some(s) = v.as_str() {
        Money::parse(s)
    } else if v.is_number() {
        Money::parse(&v.to_string())
    } else {
        Err(refused("missing exact bank amount"))
    }
}
fn finding(errors: &mut Vec<Value>, code: &str, id: i64, result: Result<(), Error>) {
    if let Err(e) = result {
        errors.push(json!({"code":code,"id":id,"error":e.message}));
    }
}
#[derive(Clone)]
struct Journal {
    no: String,
    hash: String,
    status: String,
    reversal: Option<i64>,
    direct: Option<i64>,
}
fn journal(tx: &Transaction<'_>, id: i64) -> Result<Journal, Error> {
    tx.query_row("SELECT entry_no,entry_hash,status,reversal_of_entry_id,source_bank_transaction_id FROM journal_entries WHERE id=?1",[id],|r|Ok(Journal{no:r.get(0)?,hash:r.get(1)?,status:r.get(2)?,reversal:r.get(3)?,direct:r.get(4)?})).map_err(invalid)
}
fn reversed(tx: &Transaction<'_>, id: i64) -> Result<bool, Error> {
    tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM journal_entries WHERE reversal_of_entry_id=?1)",
        [id],
        |r| r.get(0),
    )
    .map_err(invalid)
}
fn context(tx: &Transaction<'_>, id: i64) -> Result<(String, Money), Error> {
    let mut q=tx.prepare("SELECT b.currency,b.amount,b.amount_dkk,COALESCE(a.ledger_account_no,(SELECT account_no FROM account_role_mappings WHERE role='bank' AND status='confirmed' ORDER BY version DESC LIMIT 1)) FROM bank_transactions b LEFT JOIN bank_accounts a ON a.id=b.bank_account_id WHERE b.id=?1").map_err(invalid)?;
    let mut rows = q.query([id]).map_err(invalid)?;
    let r = rows
        .next()
        .map_err(invalid)?
        .ok_or_else(|| refused("referenced bank row missing"))?;
    let currency: String = r.get(0).map_err(invalid)?;
    let amount = crate::bank::signed_money(
        r.get_ref(if currency.trim().eq_ignore_ascii_case("DKK") {
            1
        } else {
            2
        })
        .map_err(invalid)?,
    )?;
    let account: Option<String> = r.get(3).map_err(invalid)?;
    Ok((
        account
            .filter(|s| !s.is_empty())
            .ok_or_else(|| refused("historical bank account mapping missing"))?,
        amount,
    ))
}
fn movement(tx: &Transaction<'_>, journal: i64, account: &str) -> Result<Money, Error> {
    let mut q=tx.prepare("SELECT l.debit_amount,l.credit_amount FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1 AND a.account_no=?2 ORDER BY l.id").map_err(invalid)?;
    let mut rows = q
        .query(rusqlite::params![journal, account])
        .map_err(invalid)?;
    let mut sum = Money::default();
    while let Some(r) = rows.next().map_err(invalid)? {
        sum.add(
            &crate::legacy_money(r.get_ref(0).map_err(invalid)?)?
                .subtract(&crate::legacy_money(r.get_ref(1).map_err(invalid)?)?),
        );
    }
    Ok(sum)
}
fn legacy_actor(actor: &str, program: &str) -> String {
    if actor == "system" && program == "system" {
        "system".into()
    } else {
        format!("{actor} via {program}")
    }
}
fn exact_audit(
    audits: &[Value],
    used: &mut BTreeSet<i64>,
    event: &str,
    bank: i64,
    actor: &str,
    message: &str,
) -> Result<(), Error> {
    let bank_text = bank.to_string();
    let found = audits
        .iter()
        .filter(|a| {
            a["event_type"] == event
                && a["entity_type"] == "bank_transaction"
                && a["entity_id"] == bank_text
                && a["actor"] == actor
                && a["message"] == message
        })
        .collect::<Vec<_>>();
    check(
        found.len() == 1,
        "exactly one matching original bank audit event required",
    )?;
    check(used.insert(id(found[0], "id")?), "bank audit proof reused")
}
fn native_correction(audits: &[Value], bank: i64, hash: &str) -> bool {
    let bank_text = bank.to_string();
    audits.iter().any(|a| {
        a["event_type"] == "bank_reconciliation_corrected"
            && a["entity_id"] == bank_text
            && a["message"]
                .as_str()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .is_some_and(|v| v["plan"]["planHash"] == hash)
    })
}
/// Every relationship is inspected; record corruption remains a finding. SQL or
/// schema failure returns unavailable to the aggregate instead of an empty pass.
pub fn verify(tx: &Transaction<'_>) -> Result<Value, Error> {
    let direct = rows(
        tx,
        "SELECT id,source_bank_transaction_id,reversal_of_entry_id FROM journal_entries WHERE source_bank_transaction_id IS NOT NULL ORDER BY id",
    )?;
    let links = rows(
        tx,
        "SELECT * FROM bank_journal_reconciliation_links ORDER BY id",
    )?;
    let corrections = rows(
        tx,
        "SELECT * FROM bank_reconciliation_correction_events ORDER BY id",
    )?;
    let purchases = rows(
        tx,
        "SELECT * FROM direct_bank_purchase_payable_corrections ORDER BY id",
    )?;
    let audits = rows(
        tx,
        "SELECT id,event_type,entity_type,entity_id,message,actor FROM audit_log WHERE event_type IN('bank_journal_reconciliation_link','bank_reconciliation_corrected','direct_bank_purchase_payable_corrected') ORDER BY id",
    )?;
    let mut errors = vec![];
    let mut used = BTreeSet::new();
    let mut histories: BTreeMap<i64, BTreeMap<String, i64>> = BTreeMap::new();
    let mut superseded = BTreeSet::new();
    let mut evidence: BTreeMap<i64, i64> = BTreeMap::new();
    for row in &direct {
        let entry = id(row, "id")?;
        let bank = id(row, "source_bank_transaction_id")?;
        if row["reversal_of_entry_id"].is_null() {
            histories
                .entry(bank)
                .or_default()
                .insert(format!("direct:{entry}"), entry);
            if evidence.insert(entry, bank).is_some() {
                errors.push(json!({"code":"BANK_JOURNAL_REUSED","id":entry}));
            }
        }
        finding(
            &mut errors,
            "BANK_DIRECT_LINK_INVALID",
            entry,
            (|| {
                let (account, amount) = context(tx, bank)?;
                let j = journal(tx, entry)?;
                let expected = if j.reversal.is_some() {
                    Money::default().subtract(&amount)
                } else {
                    amount
                };
                check(
                    movement(tx, entry, &account)? == expected,
                    "direct source bank movement or direction differs",
                )?;
                if let Some(original) = j.reversal {
                    check(
                        bank::correction::reversal(tx, original)?.0 == entry,
                        "direct reversal identity differs",
                    )?;
                }
                Ok(())
            })(),
        );
    }
    for row in &links {
        let link = id(row, "id")?;
        let bank = id(row, "bank_transaction_id")?;
        let entry = id(row, "journal_entry_id")?;
        histories
            .entry(bank)
            .or_default()
            .insert(format!("append-only:{link}"), entry);
        if evidence.insert(entry, bank).is_some() {
            errors.push(json!({"code":"BANK_JOURNAL_REUSED","id":entry}));
        }
        finding(
            &mut errors,
            "BANK_APPEND_LINK_INVALID",
            link,
            (|| {
                let (account, amount) = context(tx, bank)?;
                let j = journal(tx, entry)?;
                check(
                    j.status == "posted" && j.reversal.is_none() && j.direct.is_none(),
                    "append-only target is not an original unlinked posting",
                )?;
                check(
                    !direct
                        .iter()
                        .any(|d| d["source_bank_transaction_id"] == bank),
                    "append-only link conflicts with direct source link",
                )?;
                check(
                    movement(tx, entry, &account)? == amount,
                    "append-only bank movement differs",
                )?;
                if text(row, "created_by_program")? != "rentemester-native-bank-link" {
                    let method = text(row, "match_method")?;
                    check(
                        [
                            "exact-date-amount",
                            "settlement-lag-amount",
                            "source-reference",
                            "manual-review",
                        ]
                        .contains(&method),
                        "unknown legacy bank match method",
                    )?;
                    exact_audit(
                        &audits,
                        &mut used,
                        "bank_journal_reconciliation_link",
                        bank,
                        &legacy_actor(text(row, "created_by")?, text(row, "created_by_program")?),
                        &format!(
                            "Reconciled bank transaction {bank} to existing journal entry {} without a new posting ({method})",
                            j.no
                        ),
                    )?;
                }
                Ok(())
            })(),
        );
    }
    for row in &corrections {
        let correction = id(row, "id")?;
        let bank = id(row, "bank_transaction_id")?;
        let old = id(row, "superseded_journal_entry_id")?;
        let replacement = id(row, "replacement_journal_entry_id")?;
        let predecessor = format!(
            "{}:{}",
            text(row, "supersedes_kind")?,
            id(row, "supersedes_id")?
        );
        finding(
            &mut errors,
            "BANK_CORRECTION_INVALID",
            correction,
            (|| {
                let history = histories.entry(bank).or_default();
                check(
                    row["supersedes_reconciliation_id"] == predecessor
                        && history.get(&predecessor) == Some(&old),
                    "correction predecessor is not the same-bank historical target",
                )?;
                check(
                    superseded.insert(predecessor.clone()),
                    "correction history forks or reuses predecessor",
                )?;
                let (account, amount) = context(tx, bank)?;
                let old_journal = journal(tx, old)?;
                let new = journal(tx, replacement)?;
                check(
                    old != replacement && bank::correction::reversal(tx, old).is_ok(),
                    "old reconciliation lacks exact original reversal",
                )?;
                check(
                    new.status == "posted" && new.reversal.is_none() && new.direct.is_none(),
                    "replacement is not an unlinked original posting",
                )?;
                check(
                    text(row, "bank_account_no")? == account
                        && money(&row["bank_amount_dkk"])? == amount
                        && movement(tx, old, &account)? == amount
                        && movement(tx, replacement, &account)? == amount,
                    "correction frozen DKK amount, account or journal movement differs",
                )?;
                check(
                    text(row, "replacement_journal_hash")? == new.hash,
                    "replacement journal hash differs",
                )?;
                let kind = text(row, "principal_kind")?;
                let subject = text(row, "principal_subject_id")?;
                let actor = text(row, "actor")?;
                let reason = text(row, "reason")?;
                check(
                    ["user", "service-account"].contains(&kind)
                        && !subject.trim().is_empty()
                        && !actor.trim().is_empty()
                        && reason == reason.trim()
                        && !reason.is_empty()
                        && reason.chars().count() <= 1000,
                    "correction principal or reason differs",
                )?;
                crate::cookie_session::timestamp(text(row, "created_at")?)?;
                let hash = text(row, "plan_hash")?;
                if !native_correction(&audits, bank, hash) {
                    let plan = json!({"schemaVersion":"rentemester-bank-reconciliation-correction-v1","reconciliationId":predecessor,"bankTransactionId":bank,"currentJournalEntryId":old,"currentJournalEntryNo":old_journal.no,"replacementJournalEntryId":replacement,"replacementJournalEntryNo":new.no,"replacementJournalHash":new.hash,"bankAccountNo":account,"bankAmountDkk":wire(&amount)?,"replacementBankMovement":wire(&movement(tx,replacement,&account)?)?});
                    let expected = format!(
                        "{:x}",
                        Sha256::digest(
                            crate::intercompany_dispositions::canonical(&plan).as_bytes()
                        )
                    );
                    check(
                        hash == expected,
                        "legacy correction canonical reviewed plan hash differs",
                    )?;
                    exact_audit(
                        &audits,
                        &mut used,
                        "bank_reconciliation_corrected",
                        bank,
                        &legacy_actor(actor, "bank-reconciliation-correction"),
                        &format!(
                            "Superseded reconciliation {predecessor} with journal {}",
                            new.no
                        ),
                    )?;
                }
                Ok(())
            })(),
        );
        if evidence.insert(replacement, bank).is_some() {
            errors.push(json!({"code":"BANK_JOURNAL_REUSED","id":replacement}));
        }
        histories
            .entry(bank)
            .or_default()
            .insert(format!("correction:{correction}"), replacement);
    }
    for (bank, history) in &histories {
        finding(
            &mut errors,
            "BANK_EFFECTIVE_RECONCILIATION_INVALID",
            *bank,
            (|| {
                let roots = history
                    .iter()
                    .filter(|(name, _)| !name.starts_with("correction:"))
                    .count();
                check(
                    roots == 1,
                    "bank has multiple original reconciliation roots",
                )?;
                let tips = history
                    .iter()
                    .filter(|(name, _)| !superseded.contains(*name))
                    .collect::<Vec<_>>();
                check(
                    tips.len() == 1,
                    "bank correction history has no unique effective target",
                )?;
                let (name, entry) = tips[0];
                let j = journal(tx, *entry)?;
                if !name.starts_with("direct:") {
                    check(
                        !reversed(tx, *entry)?,
                        "effective append-only or correction target was reversed without replacement",
                    )?;
                }
                let actual:Vec<(String,i64)>=tx.prepare("SELECT reconciliation_id,journal_entry_id FROM bank_journal_reconciliations WHERE bank_transaction_id=?1 ORDER BY reconciliation_id").map_err(invalid)?.query_map([bank],|r|Ok((r.get(0)?,r.get(1)?))).map_err(invalid)?.collect::<Result<_,_>>().map_err(invalid)?;
                check(
                    j.status == "posted" && actual == vec![(name.clone(), *entry)],
                    "effective view differs from full historical chain",
                )
            })(),
        );
    }
    for row in &purchases {
        let correction = id(row, "id")?;
        finding(
            &mut errors,
            "BANK_PURCHASE_PAYABLE_CORRECTION_INVALID",
            correction,
            (|| {
                crate::direct_bank_payable_correction::validate_audit_row(tx, row)?;
                let kind = text(row, "principal_kind")?;
                let subject = text(row, "principal_subject_id")?;
                let reason = text(row, "reason")?;
                check(
                    ["user", "service-account"].contains(&kind)
                        && !subject.trim().is_empty()
                        && !reason.trim().is_empty()
                        && reason.chars().count() <= 1000,
                    "direct purchase correction principal or reason differs",
                )?;
                crate::cookie_session::timestamp(text(row, "created_at")?)?;
                let bank = id(row, "bank_transaction_id")?;
                let actor = text(row, "actor")?;
                let payable = id(row, "payable_id")?;
                let date: String = tx
                    .query_row(
                        "SELECT transaction_date FROM bank_transactions WHERE id=?1",
                        [bank],
                        |r| r.get(0),
                    )
                    .map_err(invalid)?;
                let native = format!(
                    "Corrected direct-bank purchase into payable {payable} and settled on {date}"
                );
                let legacy = format!(
                    "Corrected direct-bank purchase into payable {payable} and settled it on {date}"
                );
                let bank_text = bank.to_string();
                let matching = audits
                    .iter()
                    .filter(|a| {
                        a["event_type"] == "direct_bank_purchase_payable_corrected"
                            && a["entity_type"] == "bank_transaction"
                            && a["entity_id"] == bank_text
                            && ((a["message"] == native && a["actor"] == actor)
                                || (a["message"] == legacy
                                    && a["actor"]
                                        == legacy_actor(
                                            actor,
                                            "direct-bank-purchase-payable-correction",
                                        )))
                    })
                    .collect::<Vec<_>>();
                check(
                    matching.len() == 1,
                    "direct purchase correction original audit differs",
                )?;
                check(
                    used.insert(id(matching[0], "id")?),
                    "direct purchase audit reused",
                )
            })(),
        );
    }
    if let Err(e) = crate::direct_bank_payable_correction::validate_backup(tx) {
        errors.push(json!({"code":"BANK_PURCHASE_PAYABLE_HISTORY_INVALID","error":e.message}));
    }
    errors.extend(crate::periods::bank_audit::verify_supported(tx)?);
    for audit in &audits {
        let message = text(audit, "message")?;
        if !message.trim_start().starts_with('{') && !used.contains(&id(audit, "id")?) {
            errors.push(json!({"code":"BANK_ORPHAN_LEGACY_AUDIT","id":id(audit,"id")?}));
        }
    }
    errors.sort_by_key(crate::intercompany_dispositions::canonical);
    let bank_rows: i64 = tx
        .query_row("SELECT count(*) FROM bank_transactions", [], |r| r.get(0))
        .map_err(invalid)?;
    Ok(
        json!({"available":true,"ok":errors.is_empty(),"checked":{"bankRows":bank_rows,"directLinks":direct.len(),"appendOnlyLinks":links.len(),"corrections":corrections.len(),"directPurchasePayableCorrections":purchases.len()},"errors":errors}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rentemester_engine::{fiscal_year::Date, manual_journal::ManualJournal};
    use rusqlite::{Connection, TransactionBehavior};
    const CSV: &str = "date;text;amount;currency\n2026-04-01;Synthetic source;12.34;DKK\n";
    fn fresh() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        let mut d = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut d, "Synthetic audit bank").unwrap();
        d.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('B','Synthetic bank','asset','debit'),('E','Synthetic equity','equity','credit'),('F','Synthetic replacement equity','equity','credit');INSERT INTO bank_accounts(slug,name,ledger_account_no,currency)VALUES('synthetic-bank','Synthetic bank','B','DKK');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('bank','B',1,'synthetic');").unwrap();
        d
    }
    fn imported(tx: &Transaction<'_>, mapped: bool) {
        crate::bank::import(
            tx,
            CSV,
            &rentemester_engine::bank::parse(CSV).unwrap(),
            mapped.then_some("synthetic-bank"),
            "synthetic",
            "2026-04-01T00:00:00Z",
            &format!("bank/processed/{:x}.txt", Sha256::digest(CSV.as_bytes())),
        )
        .unwrap();
    }
    fn posted(tx: &Transaction<'_>, account: &str) -> i64 {
        let request:ManualJournal=serde_json::from_value(json!({"transactionDate":"2026-04-01","text":"Synthetic bank relationship","currency":"DKK","lines":[{"accountNo":"B","debitAmount":"12.34"},{"accountNo":account,"creditAmount":"12.34"}]})).unwrap();
        let result = crate::journal_post::post(
            tx,
            &request,
            &crate::journal_preview::manual_journal_rule_version().unwrap(),
            Date::parse("2026-05-01").unwrap(),
            crate::journal_post::Actor {
                subject: "user:synthetic",
                program: "rentemester-rust",
            },
        )
        .unwrap();
        assert_eq!(result["ok"], true);
        result["entryId"].as_i64().unwrap()
    }
    fn legacy_fixture(corrected: bool) -> Connection {
        let mut d = fresh();
        let tx = d
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        imported(&tx, true);
        let old = posted(&tx, "E");
        tx.execute("INSERT INTO bank_journal_reconciliation_links(bank_transaction_id,journal_entry_id,match_method,created_by,created_by_program)VALUES(1,?1,'exact-date-amount','user:synthetic','rentemester-cli')",[old]).unwrap();
        let no = journal(&tx, old).unwrap().no;
        tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('bank_journal_reconciliation_link','bank_transaction','1',?1,'user:synthetic via rentemester-cli')",[format!("Reconciled bank transaction 1 to existing journal entry {no} without a new posting (exact-date-amount)")]).unwrap();
        if corrected {
            crate::journal_reversal::reverse(
                &tx,
                &crate::journal_reversal::Input {
                    entry_id: rentemester_engine::identifiers::JournalEntryId::from_sqlite(old),
                    transaction_date: Date::parse("2026-04-01").unwrap(),
                    reason: "Synthetic correction".into(),
                },
                &crate::journal_preview::manual_journal_rule_version().unwrap(),
                Date::parse("2026-05-01").unwrap(),
                crate::journal_post::Actor {
                    subject: "user:synthetic",
                    program: "rentemester-rust",
                },
            )
            .unwrap();
            let replacement = posted(&tx, "F");
            let new = journal(&tx, replacement).unwrap();
            let plan = json!({"schemaVersion":"rentemester-bank-reconciliation-correction-v1","reconciliationId":"append-only:1","bankTransactionId":1,"currentJournalEntryId":old,"currentJournalEntryNo":no,"replacementJournalEntryId":replacement,"replacementJournalEntryNo":new.no,"replacementJournalHash":new.hash,"bankAccountNo":"B","bankAmountDkk":12.34,"replacementBankMovement":12.34});
            let hash = format!(
                "{:x}",
                Sha256::digest(crate::intercompany_dispositions::canonical(&plan).as_bytes())
            );
            tx.execute("INSERT INTO bank_reconciliation_correction_events(bank_transaction_id,supersedes_kind,supersedes_id,supersedes_reconciliation_id,superseded_journal_entry_id,replacement_journal_entry_id,bank_account_no,bank_amount_dkk,replacement_journal_hash,plan_hash,reason,actor,principal_kind,principal_subject_id,created_at)VALUES(1,'append-only',1,'append-only:1',?1,?2,'B',12.34,?3,?4,'Synthetic reviewed correction','user:synthetic','user','synthetic','2026-04-01T12:00:00.000Z')",rusqlite::params![old,replacement,new.hash,hash]).unwrap();
            tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('bank_reconciliation_corrected','bank_transaction','1',?1,'user:synthetic via bank-reconciliation-correction')",[format!("Superseded reconciliation append-only:1 with journal {}",new.no)]).unwrap();
        }
        let result = verify(&tx).unwrap();
        assert_eq!(result["ok"], true, "{result}");
        tx.commit().unwrap();
        d
    }
    fn corrupt(db: &Connection, table: &str, sql: &str) {
        let guards = crate::migration::schema(db)
            .unwrap()
            .into_iter()
            .filter(|o| o.kind == "trigger" && o.table == table)
            .collect::<Vec<_>>();
        for guard in &guards {
            db.execute_batch(&format!(
                "DROP TRIGGER {}",
                crate::migration::quote(&guard.name)
            ))
            .unwrap();
        }
        db.execute_batch(sql).unwrap();
        for guard in guards {
            db.execute_batch(&guard.sql).unwrap();
        }
    }
    #[test]
    fn populated_legacy_link_and_correction_have_complete_positive_assurance() {
        for corrected in [false, true] {
            let mut db = legacy_fixture(corrected);
            let tx = db.transaction().unwrap();
            let before = crate::test_support::typed_tables(&tx);
            let report = verify(&tx).unwrap();
            assert_eq!(report["ok"], true, "{report}");
            assert_eq!(report["checked"]["appendOnlyLinks"], 1);
            assert_eq!(report["checked"]["corrections"], i64::from(corrected));
            assert_eq!(crate::test_support::typed_tables(&tx), before);
        }
    }
    #[test]
    fn legacy_movement_plan_actor_and_orphan_tampering_is_not_accepted() {
        for (table, sql) in [
            (
                "bank_transactions",
                "UPDATE bank_transactions SET amount=12.35 WHERE id=1",
            ),
            (
                "bank_reconciliation_correction_events",
                "UPDATE bank_reconciliation_correction_events SET plan_hash='bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' WHERE id=1",
            ),
            (
                "audit_log",
                "UPDATE audit_log SET actor='user:forged' WHERE event_type='bank_reconciliation_corrected'",
            ),
            (
                "audit_log",
                "INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('bank_journal_reconciliation_link','bank_transaction','1','orphan proof','user:synthetic')",
            ),
        ] {
            let mut db = legacy_fixture(true);
            corrupt(&db, table, sql);
            let tx = db.transaction().unwrap();
            assert_ne!(verify(&tx).unwrap()["ok"], true, "{sql}");
        }
    }
    #[test]
    fn populated_direct_purchase_payable_correction_and_tampered_reference() {
        let mut db = fresh();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('X','Synthetic expense','expense','debit'),('V','Synthetic input VAT','vat','debit'),('C','Synthetic creditors','liability','credit');INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by)VALUES('input_vat','V','confirmed',1,'synthetic'),('creditors','C','confirmed',1,'synthetic');").unwrap();
        let metadata:rentemester_engine::document::Metadata=serde_json::from_value(json!({"source":"synthetic-audit","documentType":"purchase_sale","issueDate":"2026-06-16","invoiceNo":"SYNTHETIC-119","deliveryDescription":"Synthetic goods","amountIncVat":125,"vatAmount":25,"currency":"DKK","sender":{"name":"Synthetic supplier","address":"Synthetic street1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic buyer","address":"Synthetic street2","vatOrCvr":"DK87654321"}})).unwrap();
        crate::documents::register(
            &tx,
            crate::documents::Original {
                filename: "synthetic.txt",
                hash: &"a".repeat(64),
                path: "documents/originals/synthetic.txt",
                mime: "text/plain",
            },
            &metadata,
            "synthetic",
            "2026-10-19",
        )
        .unwrap();
        let csv = "date;text;amount\n2026-06-20;Synthetic purchase;-125\n";
        crate::bank::import(
            &tx,
            csv,
            &rentemester_engine::bank::parse(csv).unwrap(),
            Some("synthetic-bank"),
            "synthetic",
            "2026-10-19T12:00:00Z",
            "bank/processed/synthetic.txt",
        )
        .unwrap();
        let latest = Date::parse("2026-12-31").unwrap();
        crate::expense::post(
            &tx,
            &crate::expense::Input {
                document_id: 1,
                bank_transaction_id: 1,
                expense_account_no: "X".into(),
                vat_treatment: Some("standard".into()),
                payment_account_no: None,
                transaction_date: None,
                text: None,
            },
            "synthetic",
            latest,
        )
        .unwrap();
        let request = crate::direct_bank_payable_correction::PlanInput {
            document_id: 1,
            bank_transaction_id: 1,
            bill_date: "2026-06-16".into(),
            due_date: "2026-07-16".into(),
            expense_account_no: "X".into(),
            vat_treatment: Some("standard".into()),
            vendor_id: None,
            note: Some("Synthetic correction".into()),
        };
        let plan = crate::direct_bank_payable_correction::plan(&tx, &request, latest).unwrap();
        let mut input = serde_json::to_value(&request).unwrap();
        input["planHash"] = plan["planHash"].clone();
        input["reason"] = json!("Synthetic reviewed correction");
        input["confirm"] = json!(true);
        input["idempotencyKey"] = json!("synthetic-audit-correction-119");
        let input: crate::direct_bank_payable_correction::ApplyInput =
            serde_json::from_value(input).unwrap();
        crate::direct_bank_payable_correction::apply(
            &tx,
            &input,
            "synthetic",
            latest,
            1_792_368_000_000,
        )
        .unwrap();
        let report = verify(&tx).unwrap();
        assert_eq!(report["ok"], true, "{report}");
        assert_eq!(report["checked"]["directPurchasePayableCorrections"], 1);
        tx.commit().unwrap();
        corrupt(
            &db,
            "direct_bank_purchase_payable_corrections",
            "UPDATE direct_bank_purchase_payable_corrections SET document_hash='bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' WHERE id=1",
        );
        let tx = db.transaction().unwrap();
        assert_eq!(verify(&tx).unwrap()["ok"], false);
    }
    #[test]
    fn native_proof_and_historical_source_survive_deactivation_without_admitting_mutation() {
        let mut db = fresh();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        imported(&tx, true);
        let entry = posted(&tx, "E");
        let plan = bank::candidates(&tx, 1).unwrap()["plans"][0].clone();
        bank::verify_source(&tx, 1, CSV.as_bytes()).unwrap();
        bank::apply(
            &tx,
            &bank::Apply {
                bank_transaction_id: 1,
                journal_entry_id: entry,
                plan_hash: plan["planHash"].as_str().unwrap().into(),
                note: "Synthetic reviewed native link".into(),
                confirm: true,
            },
            "synthetic",
        )
        .unwrap();
        tx.execute("UPDATE bank_accounts SET active=0 WHERE id=1", [])
            .unwrap();
        assert!(bank::verify_source(&tx, 1, CSV.as_bytes()).is_err());
        bank::verify_audit_source(&tx, 1, CSV.as_bytes()).unwrap();
        let report = verify(&tx).unwrap();
        assert_eq!(report["ok"], true, "{report}");
        assert!(bank::verify_audit_source(&tx, 1, b"changed").is_err());
    }
    #[test]
    fn nullable_historical_bank_account_source_identity_and_fallback_are_not_lost() {
        let mut db = fresh();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        imported(&tx, false);
        bank::verify_audit_source(&tx, 1, CSV.as_bytes()).unwrap();
        assert!(bank::verify_source(&tx, 1, CSV.as_bytes()).is_err());
        assert_eq!(verify(&tx).unwrap()["ok"], true);
    }
    #[test]
    fn legacy_named_profile_replay_is_bound_to_original_bytes_and_nullable_identity() {
        let mut db = fresh();
        let bytes = b"Dato;Tekst;Bel\xf8b\n03.04.2026;Synthetic;100\n";
        let decoded = rentemester_engine::bank::decode_bytes(bytes, Some("danske-bank")).unwrap();
        let parsed =
            rentemester_engine::bank::parse_with_profile(&decoded, Some("danske-bank")).unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        crate::bank::import_bytes(
            &tx,
            bytes,
            &parsed,
            None,
            "synthetic",
            "2026-04-01T00:00:00Z",
            "bank/processed/synthetic.txt",
        )
        .unwrap();
        tx.commit().unwrap();
        // Reproduce the source-defined TS import provenance: it retained the
        // batch event but not a named-parser marker. This is historical data,
        // never a production path that removes native provenance.
        corrupt(
            &db,
            "audit_log",
            "UPDATE audit_log SET entity_type='bank_transaction_batch',message='Imported 1 bank transactions from synthetic.csv; skipped 0 duplicates' WHERE event_type='bank_import'; DELETE FROM audit_log WHERE event_type='bank_import_profile'",
        );
        let tx = db.transaction().unwrap();
        bank::verify_audit_source(&tx, 1, bytes).unwrap();
        assert!(bank::verify_audit_source(&tx, 1, b"changed").is_err());
    }
}
