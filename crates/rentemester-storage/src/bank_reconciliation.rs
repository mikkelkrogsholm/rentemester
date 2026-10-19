//! Append-only manual evidence connecting an imported bank row to existing accounting.
pub mod correction;
use crate::{
    bank::{fingerprint, signed_money},
    invalid, legacy_money,
};
use rentemester_engine::{Error, bank::Row, fx::FxRate, money::Money, sales_invoice::wire};
use rusqlite::{OptionalExtension, Transaction, params, types::ValueRef};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
fn refused(message: impl Into<String>) -> Error {
    Error::new("bank_reconciliation_refused", message)
}
fn id(id: i64) -> Result<(), Error> {
    if !(1..=9_007_199_254_740_991).contains(&id) {
        Err(refused("positive bank and journal identifiers required"))
    } else {
        Ok(())
    }
}
struct Bank {
    row: Row,
    account: i64,
    account_no: String,
    source: String,
    hash: String,
}
fn bank(tx: &Transaction<'_>, id_value: i64) -> Result<Bank, Error> {
    id(id_value)?;
    let mut q=tx.prepare("SELECT b.transaction_date,b.booking_date,b.text,b.amount,b.currency,b.reference,b.amount_dkk,b.fx_rate_to_dkk,b.balance_after,b.source_file_hash,b.transaction_hash,a.id,a.ledger_account_no,a.active,c.type FROM bank_transactions b JOIN bank_accounts a ON a.id=b.bank_account_id JOIN accounts c ON c.account_no=a.ledger_account_no WHERE b.id=?1").map_err(invalid)?;
    let mut rows = q.query([id_value]).map_err(invalid)?;
    let r = rows
        .next()
        .map_err(invalid)?
        .ok_or_else(|| refused("bank row and explicit mapped account required"))?;
    if r.get::<_, i64>(13).map_err(invalid)? != 1
        || r.get::<_, String>(14).map_err(invalid)? != "asset"
    {
        return Err(refused("active mapped bank asset required"));
    }
    let optional = |i| -> Result<Option<Money>, Error> {
        match r.get_ref(i).map_err(invalid)? {
            ValueRef::Null => Ok(None),
            v => signed_money(v).map(Some),
        }
    };
    let rate = match r.get_ref(7).map_err(invalid)? {
        ValueRef::Null => None,
        ValueRef::Real(v) if v.is_finite() => {
            Some(FxRate::parse(ryu_js::Buffer::new().format_finite(v))?)
        }
        ValueRef::Integer(v) => Some(FxRate::parse(&v.to_string())?),
        _ => return Err(refused("unsupported FX evidence")),
    };
    let source: String = r.get(9).map_err(invalid)?;
    if source.len() != 64
        || !source
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(refused("bank statement source hash unavailable"));
    }
    Ok(Bank {
        row: Row {
            date: r.get(0).map_err(invalid)?,
            booking_date: r.get(1).map_err(invalid)?,
            text: r.get(2).map_err(invalid)?,
            amount: signed_money(r.get_ref(3).map_err(invalid)?)?,
            currency: r.get(4).map_err(invalid)?,
            reference: r.get(5).map_err(invalid)?,
            amount_dkk: optional(6)?,
            rate,
            balance_after: optional(8)?,
        },
        source,
        hash: r.get(10).map_err(invalid)?,
        account: r.get(11).map_err(invalid)?,
        account_no: r.get(12).map_err(invalid)?,
    })
}
pub fn source_hash(tx: &Transaction<'_>, bank_id: i64) -> Result<String, Error> {
    Ok(bank(tx, bank_id)?.source)
}
/// Prove current row identity and values against the exact archived import, including duplicate ordinal.
pub fn verify_source(tx: &Transaction<'_>, bank_id: i64, bytes: &[u8]) -> Result<(), Error> {
    let bank = bank(tx, bank_id)?;
    if format!("{:x}", Sha256::digest(bytes)) != bank.source {
        return Err(refused("bank statement original hash mismatch"));
    }
    let content = std::str::from_utf8(bytes).map_err(invalid)?;
    let mut occurrences = HashMap::new();
    for row in rentemester_engine::bank::parse(content)? {
        let first = fingerprint(&row, Some(bank.account), 0)?;
        let count = occurrences.entry(first).or_insert(0);
        let hash = fingerprint(&row, Some(bank.account), *count)?;
        if hash == bank.hash
            && fingerprint(&bank.row, Some(bank.account), *count)? == hash
            && row.balance_after == bank.row.balance_after
        {
            return Ok(());
        }
        *count += 1;
    }
    Err(refused(
        "current bank row no longer matches its archived statement; review source evidence",
    ))
}
fn amount(bank: &Bank) -> Result<Money, Error> {
    let value = if bank.row.currency == "DKK" {
        bank.row.amount.clone()
    } else {
        bank.row
            .amount_dkk
            .clone()
            .ok_or_else(|| refused("documented DKK amount required; no FX rate is invented"))?
    };
    if value == Money::default() {
        return Err(refused("zero movement cannot be reconciled manually"));
    }
    Ok(value)
}
pub fn document_id(tx: &Transaction<'_>, journal: i64) -> Result<Option<i64>, Error> {
    tx.query_row(
        "SELECT document_id FROM journal_entries WHERE id=?1",
        [journal],
        |r| r.get(0),
    )
    .map_err(invalid)
}
fn journal_plan(
    tx: &Transaction<'_>,
    bank_id: i64,
    journal: i64,
    require_unmatched_bank: bool,
) -> Result<Value, Error> {
    id(journal)?;
    let already:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM bank_journal_reconciliations WHERE (?3 AND bank_transaction_id=?1) OR journal_entry_id=?2) OR EXISTS(SELECT 1 FROM bank_journal_reconciliation_links WHERE journal_entry_id=?2) OR EXISTS(SELECT 1 FROM bank_reconciliation_correction_events WHERE replacement_journal_entry_id=?2)",params![bank_id,journal,require_unmatched_bank],|r|r.get(0)).map_err(invalid)?;
    if already {
        return Err(refused(
            "bank row or journal already reconciled; explicit correction required",
        ));
    }
    let reversed: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM journal_entries WHERE reversal_of_entry_id=?1)",
            [journal],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if reversed {
        return Err(refused("active original journal required"));
    }
    persisted_journal_facts(tx, bank_id, journal)
}
/// Read-only historical facts for native period audit. This is not candidate
/// admission: existing links/reversal children are expected in immutable history.
/// The caller validates proof/current-chain and verifies the actual archived source.
pub(crate) fn persisted_journal_facts(
    tx: &Transaction<'_>,
    bank_id: i64,
    journal: i64,
) -> Result<Value, Error> {
    id(journal)?;
    let bank = bank(tx, bank_id)?;
    let target = amount(&bank)?;
    type JournalProof = (
        String,
        String,
        String,
        String,
        Option<i64>,
        Option<i64>,
        String,
    );
    let entry:Option<JournalProof>=tx.query_row("SELECT entry_no,entry_hash,transaction_date,status,reversal_of_entry_id,source_bank_transaction_id,currency FROM journal_entries WHERE id=?1",[journal],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional().map_err(invalid)?;
    let (no, hash, date, status, reversal, direct, currency) =
        entry.ok_or_else(|| refused("active original journal required"))?;
    if status != "posted"
        || reversal.is_some()
        || direct.is_some()
        || currency != "DKK"
        || hash.len() != 64
    {
        return Err(refused("active unlinked DKK journal required"));
    }
    let mut q=tx.prepare("SELECT l.debit_amount,l.credit_amount FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1 AND a.account_no=?2 ORDER BY l.id").map_err(invalid)?;
    let mut rows = q
        .query(params![journal, bank.account_no])
        .map_err(invalid)?;
    let mut movement = Money::default();
    while let Some(r) = rows.next().map_err(invalid)? {
        movement.add(
            &legacy_money(r.get_ref(0).map_err(invalid)?)?
                .subtract(&legacy_money(r.get_ref(1).map_err(invalid)?)?),
        );
    }
    if movement != target {
        return Err(refused(
            "journal mapped bank movement does not equal statement amount",
        ));
    }
    let document = document_id(tx, journal)?
        .map(|id| crate::documents::evidence(tx, id).map(|d| json!({"id":id,"sha256":d.hash})))
        .transpose()?;
    let mut value = json!({"schemaVersion":"rentemester-native-bank-link-v1","bankTransactionId":bank_id,"bankDate":bank.row.date,"bankText":bank.row.text,"bankCurrency":bank.row.currency,"bankAmount":wire(&bank.row.amount)?,"bankAmountDkk":wire(&target)?,"bankAccountNo":bank.account_no,"sourceHash":bank.source,"transactionHash":bank.hash,"journalEntryId":journal,"journalEntryNo":no,"journalHash":hash,"journalDate":date,"journalBankMovement":wire(&movement)?,"document":document,"effect":"link_only_no_posting"});
    value["planHash"] = json!(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&value).map_err(invalid)?)
    ));
    Ok(value)
}
pub fn candidates(tx: &Transaction<'_>, bank_id: i64) -> Result<Value, Error> {
    crate::validate(tx)?;
    let bank = bank(tx, bank_id)?;
    amount(&bank)?;
    let mut q=tx.prepare("SELECT DISTINCT j.id FROM journal_entries j JOIN journal_lines l ON l.journal_entry_id=j.id JOIN accounts a ON a.id=l.account_id WHERE a.account_no=?1 AND j.status='posted' AND j.reversal_of_entry_id IS NULL AND j.source_bank_transaction_id IS NULL ORDER BY j.transaction_date,j.id").map_err(invalid)?;
    let mut plans = vec![];
    for candidate in q
        .query_map([bank.account_no], |r| r.get::<_, i64>(0))
        .map_err(invalid)?
    {
        match journal_plan(tx, bank_id, candidate.map_err(invalid)?, true) {
            Ok(p) => plans.push(p),
            Err(e) if e.code == "bank_reconciliation_refused" => {}
            Err(e) => return Err(e),
        }
    }
    Ok(
        json!({"plans":plans,"correctionAvailable":true,"note":"Vælg kun en allerede bogført postering for samme bankbevægelse. Manuel afstemning opretter ingen ny bogføring. En linkkorrektion kræver en allerede bogført tilbageførsel og en særskilt erstatningspostering, før korrektionens plan kan gennemgås."}),
    )
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Apply {
    pub bank_transaction_id: i64,
    pub journal_entry_id: i64,
    pub plan_hash: String,
    pub note: String,
    pub confirm: bool,
}
pub fn apply(tx: &Transaction<'_>, input: &Apply, actor: &str) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm || input.note.trim().is_empty() || input.note.trim().len() > 1000 {
        return Err(refused(
            "explicit confirmation and reason (1–1000 bytes) required",
        ));
    }
    let planned = journal_plan(tx, input.bank_transaction_id, input.journal_entry_id, true)?;
    if planned["planHash"] != input.plan_hash {
        return Err(refused(
            "reviewed bank/journal proof changed; inspect a new plan",
        ));
    }
    let by = format!("user:{actor}");
    tx.execute("INSERT INTO bank_journal_reconciliation_links(bank_transaction_id,journal_entry_id,match_method,source_reference,note,created_by,created_by_program)VALUES(?1,?2,'manual-review',?3,?4,?5,'rentemester-native-bank-link')",params![input.bank_transaction_id,input.journal_entry_id,format!("native-plan:{}",input.plan_hash),input.note.trim(),by]).map_err(invalid)?;
    let id = tx.last_insert_rowid();
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('bank_journal_reconciliation_link','bank_transaction',?1,?2,?3)",params![input.bank_transaction_id.to_string(),json!({"plan":planned,"note":input.note.trim(),"effect":"link_only_no_posting"}).to_string(),by]).map_err(invalid)?;
    Ok(
        json!({"id":id,"reconciliationId":format!("append-only:{id}"),"journalEntryId":input.journal_entry_id,"journalEntryNo":planned["journalEntryNo"],"effect":"link_only_no_posting"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const CSV: &str = "transaction_date,text,amount,currency\n2026-04-01,Synthetic reconciliation,15.27,DKK\n2026-04-02,Synthetic second statement row,15.27,DKK\n";
    fn fixture() -> (tempfile::TempDir, rusqlite::Connection) {
        let dir = tempfile::tempdir().unwrap();
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open(dir.path().join("company.sqlite")).unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Bank Reconciliation")
            .unwrap();
        db.execute_batch("INSERT INTO accounts(id,account_no,name,type,normal_balance)VALUES(1,'B','Synthetic bank asset','asset','debit'),(2,'E','Synthetic equity','equity','credit');INSERT INTO bank_accounts(id,slug,name,ledger_account_no,currency)VALUES(1,'synthetic-reconciliation','Synthetic bank','B','DKK');INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash)VALUES(1,'SYN-LINK','2026-04-01','Existing accounting','fixture','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount)VALUES(1,1,15.27,0),(1,2,0,15.27);").unwrap();
        {
            let tx = db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            crate::bank::import(
                &tx,
                CSV,
                &rentemester_engine::bank::parse(CSV).unwrap(),
                Some("synthetic-reconciliation"),
                "synthetic",
                "2026-04-01T00:00:00Z",
                "bank/processed/synthetic.txt",
            )
            .unwrap();
            tx.commit().unwrap();
        }
        (dir, db)
    }
    fn input(tx: &Transaction<'_>) -> Apply {
        let plan = &candidates(tx, 1).unwrap()["plans"][0];
        Apply {
            bank_transaction_id: 1,
            journal_entry_id: 1,
            plan_hash: plan["planHash"].as_str().unwrap().into(),
            note: "Synthetic reviewer confirmed statement and existing accounting are one event."
                .into(),
            confirm: true,
        }
    }
    #[test]
    fn exact_reasoned_link_preserves_journals_and_survives_reopen() {
        let (dir, mut db) = fixture();
        let result;
        {
            let tx = db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            verify_source(&tx, 1, CSV.as_bytes()).unwrap();
            let i = input(&tx);
            let before = crate::journal_hash::verify(&tx).unwrap();
            result = apply(&tx, &i, "synthetic").unwrap();
            assert_eq!(result["effect"], "link_only_no_posting");
            let view = crate::bank::read(&tx, "synthetic", "2026-04-05", Some("2026")).unwrap();
            assert_eq!(view["transactions"][0]["reconciliationId"], "append-only:1");
            assert_eq!(view["transactions"][0]["reconciliationStatus"], "matched");
            assert!(
                view["transactions"][0]["reconciliationNote"]
                    .as_str()
                    .unwrap()
                    .contains("Synthetic reviewer")
            );
            assert_eq!(
                crate::journal_hash::verify(&tx).unwrap().errors,
                before.errors
            );
            assert!(apply(&tx, &i, "synthetic").is_err());
            assert_eq!(
                tx.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert_eq!(
                tx.query_row(
                    "SELECT COUNT(*) FROM bank_journal_reconciliation_links",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            assert!(
                tx.execute(
                    "UPDATE bank_journal_reconciliation_links SET note='rewrite'",
                    []
                )
                .is_err()
            );
            assert!(
                tx.execute("DELETE FROM bank_journal_reconciliation_links", [])
                    .is_err()
            );
            tx.commit().unwrap();
        }
        drop(db);
        let mut again = rusqlite::Connection::open(dir.path().join("company.sqlite")).unwrap();
        let tx = again.transaction().unwrap();
        assert_eq!(tx.query_row("SELECT journal_entry_no FROM bank_journal_reconciliations WHERE bank_transaction_id=1",[],|r|r.get::<_,String>(0)).unwrap(),result["journalEntryNo"].as_str().unwrap());
        assert!(
            candidates(&tx, 2).unwrap()["plans"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn stale_mapping_wrong_proof_missing_confirmation_and_reason_have_no_effect() {
        let (_dir, mut db) = fixture();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let mut i = input(&tx);
        i.confirm = false;
        assert!(apply(&tx, &i, "synthetic").is_err());
        i.confirm = true;
        i.note = " ".into();
        assert!(apply(&tx, &i, "synthetic").is_err());
        i.note = "Reviewed".into();
        i.plan_hash = "0".repeat(64);
        assert!(apply(&tx, &i, "synthetic").is_err());
        let i = input(&tx);
        tx.execute("UPDATE bank_accounts SET active=0 WHERE id=1", [])
            .unwrap();
        assert!(apply(&tx, &i, "synthetic").is_err());
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM bank_journal_reconciliation_links",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn changed_statement_row_or_archive_cannot_supply_source_evidence() {
        let (_dir, mut db) = fixture();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert!(verify_source(&tx, 1, b"changed").is_err());
        verify_source(&tx, 1, CSV.as_bytes()).unwrap();
        tx.execute("UPDATE bank_transactions SET amount=16 WHERE id=1", [])
            .unwrap();
        assert!(verify_source(&tx, 1, CSV.as_bytes()).is_err());
        assert!(
            candidates(&tx, 1).unwrap()["plans"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}
