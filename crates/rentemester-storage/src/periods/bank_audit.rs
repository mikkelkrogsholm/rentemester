//! Read-only native bank-link assurance. Caller must additionally re-read bank
//! statement originals with NamespaceGuard + verify_source and journal document
//! originals. No filesystem authority, candidate admission or accounting effects.
use crate::{bank_reconciliation as bank, invalid};
use rentemester_engine::Error;
use rusqlite::Transaction;
use serde_json::{Value, json};
use std::collections::BTreeSet;
fn refusal(message: &str) -> Error {
    Error::new("bank_reconciliation_audit_refused", message)
}
fn check(ok: bool, message: &str) -> Result<(), Error> {
    if ok { Ok(()) } else { Err(refusal(message)) }
}
struct Audit {
    id: i64,
    bank: i64,
    actor: String,
    value: Value,
}
fn audits(tx: &Transaction<'_>, kind: &str) -> Result<Vec<Audit>, Error> {
    let mut q=tx.prepare("SELECT id,entity_type,entity_id,actor,message FROM audit_log WHERE event_type=?1 ORDER BY id").map_err(invalid)?;
    let mut rows = q.query([kind]).map_err(invalid)?;
    let mut out = vec![];
    while let Some(r) = rows.next().map_err(invalid)? {
        let entity: String = r.get(1).map_err(invalid)?;
        let raw: String = r.get(2).map_err(invalid)?;
        let bank = raw.parse::<i64>().map_err(invalid)?;
        check(
            entity == "bank_transaction"
                && (1..=9_007_199_254_740_991).contains(&bank)
                && raw == bank.to_string(),
            "bank audit identity differs",
        )?;
        let value: Value =
            serde_json::from_str(&r.get::<_, String>(4).map_err(invalid)?).map_err(invalid)?;
        check(
            value.as_object().is_some_and(|v| v.len() == 3),
            "native bank audit envelope differs",
        )?;
        out.push(Audit {
            id: r.get(0).map_err(invalid)?,
            bank,
            actor: r.get(3).map_err(invalid)?,
            value,
        });
    }
    Ok(out)
}
fn matching<'a>(rows: &'a [Audit], bank: i64, hash: &str) -> Result<&'a Audit, Error> {
    let matches = rows
        .iter()
        .filter(|a| a.bank == bank && a.value["plan"]["planHash"] == hash)
        .collect::<Vec<_>>();
    check(
        matches.len() == 1,
        "exactly one original native audit proof required",
    )?;
    Ok(matches[0])
}
fn problem(code: &str, id: i64, error: Error) -> Value {
    json!({"code":code,"id":id,"error":error.message})
}
/// Empty is passed. Corruption is nonempty evidence (or Err/unavailable), never
/// silently omitted. Every persisted native row and native audit is accounted for.
pub(super) fn verify(tx: &Transaction<'_>) -> Result<Vec<Value>, Error> {
    crate::validate(tx)?;
    let link_audits = audits(tx, "bank_journal_reconciliation_link")?;
    let correction_audits = audits(tx, "bank_reconciliation_corrected")?;
    let mut errors = vec![];
    let mut used = BTreeSet::new();
    let mut banks = BTreeSet::new();
    let mut q=tx.prepare("SELECT id,bank_transaction_id,journal_entry_id,match_method,source_reference,note,created_by,created_by_program FROM bank_journal_reconciliation_links ORDER BY id").map_err(invalid)?;
    let mut rows = q.query([]).map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        let id: i64 = r.get(0).map_err(invalid)?;
        let bank_id: i64 = r.get(1).map_err(invalid)?;
        let journal: i64 = r.get(2).map_err(invalid)?;
        banks.insert(bank_id);
        let result = (|| {
            let method: String = r.get(3).map_err(invalid)?;
            let reference: Option<String> = r.get(4).map_err(invalid)?;
            let note: Option<String> = r.get(5).map_err(invalid)?;
            let actor: String = r.get(6).map_err(invalid)?;
            let program: String = r.get(7).map_err(invalid)?;
            check(
                method == "manual-review"
                    && program == "rentemester-native-bank-link"
                    && actor.strip_prefix("user:").is_some_and(|s| !s.is_empty()),
                "unsupported link provenance; only exact native manual evidence is ported",
            )?;
            let note = note.ok_or_else(|| refusal("link reason unavailable"))?;
            check(
                !note.is_empty() && note == note.trim() && note.len() <= 1000,
                "native link reason differs",
            )?;
            let facts = bank::persisted_journal_facts(tx, bank_id, journal)?;
            let hash = facts["planHash"]
                .as_str()
                .ok_or_else(|| refusal("link facts hash unavailable"))?;
            check(
                reference.as_deref() == Some(format!("native-plan:{hash}").as_str()),
                "native link stored proof differs",
            )?;
            let audit = matching(&link_audits, bank_id, hash)?;
            check(
                audit.actor == actor
                    && audit.value
                        == json!({"plan":facts,"note":note,"effect":"link_only_no_posting"}),
                "link current facts, actor, note or accounting effect differ from original audit proof",
            )?;
            check(used.insert(audit.id), "bank audit reused across links")?;
            Ok::<(), Error>(())
        })();
        if let Err(e) = result {
            errors.push(problem("BANK_LINK_PROOF_INVALID", id, e));
        }
    }
    let mut q=tx.prepare("SELECT id,bank_transaction_id,supersedes_kind,supersedes_id,supersedes_reconciliation_id,superseded_journal_entry_id,replacement_journal_entry_id,bank_account_no,bank_amount_dkk,replacement_journal_hash,plan_hash,reason,actor,principal_kind,principal_subject_id,created_at FROM bank_reconciliation_correction_events ORDER BY id").map_err(invalid)?;
    let mut rows = q.query([]).map_err(invalid)?;
    // The full source history is immutable. Reconstruct each event's prefix,
    // preserving the public proof builder's canonical array order.
    while let Some(r) = rows.next().map_err(invalid)? {
        let id: i64 = r.get(0).map_err(invalid)?;
        let bank_id: i64 = r.get(1).map_err(invalid)?;
        banks.insert(bank_id);
        let result = (|| {
            let kind: String = r.get(2).map_err(invalid)?;
            let predecessor: i64 = r.get(3).map_err(invalid)?;
            let reconciliation: String = r.get(4).map_err(invalid)?;
            let old: i64 = r.get(5).map_err(invalid)?;
            let replacement: i64 = r.get(6).map_err(invalid)?;
            check(
                reconciliation == format!("{kind}:{predecessor}"),
                "correction predecessor identity differs",
            )?;
            let account: String = r.get(7).map_err(invalid)?;
            let amount = crate::bank::signed_money(r.get_ref(8).map_err(invalid)?)?;
            let replacement_hash: String = r.get(9).map_err(invalid)?;
            let hash: String = r.get(10).map_err(invalid)?;
            let reason: String = r.get(11).map_err(invalid)?;
            let actor: String = r.get(12).map_err(invalid)?;
            let principal_kind: String = r.get(13).map_err(invalid)?;
            let subject: String = r.get(14).map_err(invalid)?;
            check(
                principal_kind == "user"
                    && !subject.trim().is_empty()
                    && actor == format!("user:{subject}")
                    && !reason.is_empty()
                    && reason == reason.trim()
                    && reason.chars().count() <= 1000,
                "native correction reason or principal differs",
            )?;
            crate::cookie_session::timestamp(&r.get::<_, String>(15).map_err(invalid)?)?;
            let full = bank::correction::history(tx, bank_id)?;
            let full = full
                .as_array()
                .ok_or_else(|| refusal("bank history unavailable"))?;
            let prefix = full
                .iter()
                .filter(|e| e["kind"] != "correction" || e["id"].as_i64().is_some_and(|n| n < id))
                .cloned()
                .collect::<Vec<_>>();
            let facts = bank::correction::persisted_plan_facts(
                tx,
                bank_id,
                &reconciliation,
                old,
                replacement,
                json!(prefix),
            )?;
            let plan = &facts["plan"];
            check(
                plan["planHash"] == hash
                    && plan["bankAccountNo"] == account
                    && plan["replacementJournalHash"] == replacement_hash
                    && rentemester_engine::money::Money::parse(&plan["bankAmountDkk"].to_string())?
                        == amount,
                "correction persisted movement, source, replacement or full historical plan hash differs",
            )?;
            let audit = matching(&correction_audits, bank_id, &hash)?;
            check(
                audit.actor == actor
                    && audit.value
                        == json!({"plan":plan,"reason":reason,"effect":"correction_evidence_only_no_posting"}),
                "correction original proof/current lines/reversal/source/history/effect differ",
            )?;
            check(
                used.insert(audit.id),
                "bank audit reused across corrections",
            )?;
            Ok::<(), Error>(())
        })();
        if let Err(e) = result {
            errors.push(problem("BANK_CORRECTION_PROOF_INVALID", id, e));
        }
    }
    // A journal is accounting evidence for one bank occurrence, including all
    // superseded history. Historical reconstruction must not bypass the normal
    // candidate gate's global prohibition on reusing an original/replacement.
    let mut q = tx.prepare("SELECT journal_entry_id FROM (SELECT id AS journal_entry_id FROM journal_entries WHERE source_bank_transaction_id IS NOT NULL AND reversal_of_entry_id IS NULL UNION ALL SELECT journal_entry_id FROM bank_journal_reconciliation_links UNION ALL SELECT replacement_journal_entry_id FROM bank_reconciliation_correction_events) GROUP BY journal_entry_id HAVING COUNT(*)>1 ORDER BY journal_entry_id").map_err(invalid)?;
    let reused = q.query_map([], |r| r.get::<_, i64>(0)).map_err(invalid)?;
    for journal in reused {
        errors.push(json!({"code":"BANK_JOURNAL_EVIDENCE_REUSED","journalEntryId":journal.map_err(invalid)?}));
    }
    for bank_id in banks {
        let result = (|| {
            let history = bank::correction::history(tx, bank_id)?;
            let items = history
                .as_array()
                .ok_or_else(|| refusal("bank history unavailable"))?;
            let bases = items
                .iter()
                .filter(|e| e["kind"] != "correction")
                .collect::<Vec<_>>();
            check(
                bases.len() == 1,
                "one same-bank original reconciliation required",
            )?;
            let base = bases[0];
            let kind = base["kind"]
                .as_str()
                .ok_or_else(|| refusal("base kind unavailable"))?;
            let id = base["id"]
                .as_i64()
                .ok_or_else(|| refusal("base identity unavailable"))?;
            let mut expected = format!("{kind}:{id}");
            let mut journal = if kind == "direct" {
                id
            } else {
                base["journalEntryId"]
                    .as_i64()
                    .ok_or_else(|| refusal("base journal unavailable"))?
            };
            let mut previous = 0;
            for event in items.iter().filter(|e| e["kind"] == "correction") {
                let id = event["id"]
                    .as_i64()
                    .ok_or_else(|| refusal("correction identity unavailable"))?;
                check(
                    id > previous
                        && event["supersedes"] == expected
                        && event["oldJournalEntryId"] == journal,
                    "bank correction chain forks, cycles or crosses another bank/journal",
                )?;
                expected = format!("correction:{id}");
                journal = event["replacementJournalEntryId"]
                    .as_i64()
                    .ok_or_else(|| refusal("replacement unavailable"))?;
                previous = id;
            }
            let current = bank::correction::current(tx, bank_id)?;
            check(
                current.0 == expected && current.1 == journal,
                "canonical current reconciliation differs from historical chain",
            )?;
            let unresolved: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM journal_entries WHERE reversal_of_entry_id=?1)",
                    [journal],
                    |r| r.get(0),
                )
                .map_err(invalid)?;
            check(
                !unresolved,
                "current bank target was reversed without a completed explicit correction",
            )?;
            Ok::<(), Error>(())
        })();
        if let Err(e) = result {
            errors.push(problem("BANK_RECONCILIATION_CHAIN_INVALID", bank_id, e));
        }
    }
    for audit in link_audits.iter().chain(&correction_audits) {
        if !used.contains(&audit.id) {
            errors.push(json!({"code":"BANK_RECONCILIATION_ORPHAN_AUDIT","auditId":audit.id,"bankTransactionId":audit.bank}));
        }
    }
    // Journal hashes bind initial-link counter-lines that are represented by a
    // journalHash rather than duplicated in the native initial-link plan.
    if !link_audits.is_empty() || !correction_audits.is_empty() {
        for e in crate::journal_hash::verify(tx)?.errors {
            errors.push(json!({"code":"BANK_RECONCILIATION_JOURNAL_HASH_INVALID","error":e}));
        }
    }
    Ok(errors)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal_post::Actor;
    use rentemester_engine::{fiscal_year::Date, manual_journal::ManualJournal};
    use rusqlite::{Connection, TransactionBehavior};
    const CSV: &str = "transaction_date,text,amount,currency\n2026-04-01,Synthetic period linked source,12.34,DKK\n2026-04-02,Synthetic distinct bank identity,12.34,DKK\n";
    fn posted(tx: &Transaction<'_>, counter: &str) -> i64 {
        let request:ManualJournal=serde_json::from_value(json!({"transactionDate":"2026-04-01","text":"Synthetic bank-audit accounting","currency":"DKK","lines":[{"accountNo":"1751","debitAmount":"12.34"},{"accountNo":counter,"creditAmount":"12.34"}]})).unwrap();
        let value = crate::journal_post::post(
            tx,
            &request,
            &crate::journal_preview::manual_journal_rule_version().unwrap(),
            Date::parse("2026-05-01").unwrap(),
            Actor {
                subject: "user:synthetic",
                program: "rentemester-rust",
            },
        )
        .unwrap();
        assert_eq!(value["ok"], true);
        value["entryId"].as_i64().unwrap()
    }
    fn corrected(tx: &Transaction<'_>, old: i64, counter: &str, key: &str) -> i64 {
        let request = crate::journal_reversal::Input {
            entry_id: rentemester_engine::identifiers::JournalEntryId::from_sqlite(old),
            transaction_date: Date::parse("2026-04-01").unwrap(),
            reason: "Synthetic actual explicit reversal".into(),
        };
        let reversal = crate::journal_reversal::reverse(
            tx,
            &request,
            &crate::journal_preview::manual_journal_rule_version().unwrap(),
            Date::parse("2026-05-01").unwrap(),
            Actor {
                subject: "user:synthetic",
                program: "rentemester-rust",
            },
        )
        .unwrap();
        assert_eq!(reversal["ok"], true);
        let replacement = posted(tx, counter);
        let plan = bank::correction::plan(tx, 1, replacement).unwrap();
        let input = bank::correction::Apply {
            bank_transaction_id: 1,
            replacement_journal_entry_id: replacement,
            expected_reconciliation_id: plan["plan"]["reconciliationId"].as_str().unwrap().into(),
            plan_hash: plan["plan"]["planHash"].as_str().unwrap().into(),
            reason: "Synthetic reviewed existing reversal and replacement".into(),
            idempotency_key: key.into(),
            confirm: true,
        };
        bank::verify_source(tx, 1, CSV.as_bytes()).unwrap();
        let result = bank::correction::apply(tx, &input, "synthetic", 1_780_000_000_000).unwrap();
        assert_eq!(result["ok"], true);
        replacement
    }
    fn fixture(correction: bool) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.sqlite");
        let mut db = Connection::open(&path).unwrap();
        let mut control = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic bank audit").unwrap();
        db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('1751','Synthetic bank','asset','debit'),('8751','Synthetic equity one','equity','credit'),('8752','Synthetic equity two','equity','credit');INSERT INTO bank_accounts(slug,name,ledger_account_no,currency)VALUES('synthetic-audit','Synthetic bank audit','1751','DKK');").unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        crate::bank::import(
            &tx,
            CSV,
            &rentemester_engine::bank::parse(CSV).unwrap(),
            Some("synthetic-audit"),
            "synthetic",
            "2026-04-01T00:00:00Z",
            &format!(
                "bank/processed/{:x}.txt",
                sha2::Sha256::digest(CSV.as_bytes())
            ),
        )
        .unwrap();
        let original = posted(&tx, "8751");
        let facts = bank::candidates(&tx, 1).unwrap();
        let plan = &facts["plans"][0];
        bank::verify_source(&tx, 1, CSV.as_bytes()).unwrap();
        bank::apply(
            &tx,
            &bank::Apply {
                bank_transaction_id: 1,
                journal_entry_id: original,
                plan_hash: plan["planHash"].as_str().unwrap().into(),
                note: "Synthetic original reviewed link".into(),
                confirm: true,
            },
            "synthetic",
        )
        .unwrap();
        assert!(verify(&tx).unwrap().is_empty());
        assert!(
            bank::candidates(&tx, 1).unwrap()["plans"]
                .as_array()
                .unwrap()
                .is_empty(),
            "historical facts must not weaken candidate eligibility"
        );
        if correction {
            corrected(&tx, original, "8752", "synthetic-bank-audit-correction");
            assert!(verify(&tx).unwrap().is_empty());
        }
        tx.commit().unwrap();
        drop(db);
        (dir, path)
    }
    use sha2::Digest;
    fn rewrite(db: &Connection, sql: &str) {
        // Synthetic corruption only: restore the exact canonical DDL afterwards,
        // so schema admission cannot mask whether persisted facts are checked.
        let triggers = db
            .prepare("SELECT name,sql FROM sqlite_schema WHERE type='trigger' ORDER BY name")
            .unwrap()
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        db.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
        for (name, _) in &triggers {
            db.execute_batch(&format!("DROP TRIGGER \"{name}\";"))
                .unwrap();
        }
        db.execute_batch(sql).unwrap();
        for (_, ddl) in triggers {
            db.execute_batch(&ddl).unwrap();
        }
        db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    }
    fn assert_blocked(db: &mut Connection) {
        let before = db.total_changes();
        let tx = db.transaction().unwrap();
        let result = verify(&tx);
        assert!(
            result.is_err() || !result.unwrap().is_empty(),
            "corrupt evidence passed"
        );
        tx.rollback().unwrap();
        assert_eq!(db.total_changes(), before, "audit must be read-only");
    }
    #[test]
    fn bank_audit_real_public_links_and_repeated_corrections_remain_readonly_after_reopen() {
        let (_dir, path) = fixture(true);
        let mut db = Connection::open(&path).unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let last:i64=tx.query_row("SELECT replacement_journal_entry_id FROM bank_reconciliation_correction_events ORDER BY id DESC LIMIT 1",[],|r|r.get(0)).unwrap();
        corrected(&tx, last, "8751", "synthetic-bank-audit-second-correction");
        assert!(verify(&tx).unwrap().is_empty());
        assert!(
            bank::correction::plan(&tx, 1, last).is_err(),
            "persisted reconstruction is not candidate authority"
        );
        tx.commit().unwrap();
        drop(db);
        let mut db = Connection::open(path).unwrap();
        let before = db.total_changes();
        let tx = db.transaction().unwrap();
        assert!(verify(&tx).unwrap().is_empty());
        tx.rollback().unwrap();
        assert_eq!(db.total_changes(), before);
    }
    #[test]
    fn bank_audit_individually_valid_proofs_cannot_reuse_one_journal_for_two_banks() {
        let (_dir, path) = fixture(false);
        let mut db = Connection::open(path).unwrap();
        let tx = db.transaction().unwrap();
        let journal: i64 = tx
            .query_row(
                "SELECT journal_entry_id FROM bank_journal_reconciliation_links",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            bank::candidates(&tx, 2).unwrap()["plans"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        // Deliberately forge a fully consistent persisted proof using the
        // historical reader: only the global one-evidence invariant distinguishes
        // this corruption from two independently valid proofs.
        let facts = bank::persisted_journal_facts(&tx, 2, journal).unwrap();
        tx.execute("INSERT INTO bank_journal_reconciliation_links(bank_transaction_id,journal_entry_id,match_method,source_reference,note,created_by,created_by_program)VALUES(2,?1,'manual-review',?2,'Synthetic duplicate evidence','user:synthetic','rentemester-native-bank-link')", rusqlite::params![journal,format!("native-plan:{}",facts["planHash"].as_str().unwrap())]).unwrap();
        tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('bank_journal_reconciliation_link','bank_transaction','2',?1,'user:synthetic')",[json!({"plan":facts,"note":"Synthetic duplicate evidence","effect":"link_only_no_posting"}).to_string()]).unwrap();
        let errors = verify(&tx).unwrap();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0]["code"], "BANK_JOURNAL_EVIDENCE_REUSED");
        tx.rollback().unwrap();
    }
    #[test]
    fn bank_audit_missing_corrupt_link_proof_source_journal_and_crossbank_rows_refuse() {
        let (_dir, path) = fixture(false);
        for sql in [
            "DELETE FROM audit_log WHERE event_type='bank_journal_reconciliation_link'",
            "UPDATE bank_journal_reconciliation_links SET source_reference='native-plan:invented'",
            "UPDATE bank_journal_reconciliation_links SET created_by_program='unported-workflow'",
            "UPDATE bank_journal_reconciliation_links SET note='Different reason'",
            "UPDATE bank_journal_reconciliation_links SET bank_transaction_id=2",
            "UPDATE bank_transactions SET amount=12.35 WHERE id=1",
            "UPDATE bank_transactions SET source_file_hash='ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff' WHERE id=1",
            "UPDATE journal_lines SET account_id=(SELECT id FROM accounts WHERE account_no='8752') WHERE credit_amount>0",
            "DELETE FROM bank_transactions WHERE id=1",
            "DELETE FROM journal_entries WHERE id=1",
            "DELETE FROM bank_journal_reconciliation_links",
            "UPDATE audit_log SET message='{}' WHERE event_type='bank_journal_reconciliation_link'",
        ] {
            let copy = tempfile::tempdir().unwrap();
            let copy_path = copy.path().join("corrupt.sqlite");
            std::fs::copy(&path, &copy_path).unwrap();
            let mut db = Connection::open(copy_path).unwrap();
            rewrite(&db, sql);
            assert_blocked(&mut db);
        }
    }
    #[test]
    fn bank_audit_correction_fullproof_inverse_and_canonical_samebank_chain_refuse_corruption() {
        let (_dir, path) = fixture(true);
        for sql in [
            "UPDATE bank_reconciliation_correction_events SET plan_hash='ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff'",
            "UPDATE bank_reconciliation_correction_events SET reason='Different reviewed reason'",
            "UPDATE bank_reconciliation_correction_events SET actor='user:another'",
            "UPDATE bank_reconciliation_correction_events SET supersedes_id=999,supersedes_reconciliation_id='append-only:999'",
            "UPDATE bank_reconciliation_correction_events SET bank_transaction_id=2",
            "UPDATE bank_reconciliation_correction_events SET superseded_journal_entry_id=3",
            "UPDATE bank_reconciliation_correction_events SET bank_amount_dkk=12.35",
            "UPDATE bank_reconciliation_correction_events SET replacement_journal_hash='ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff'",
            "DELETE FROM journal_entries WHERE reversal_of_entry_id=1",
            "UPDATE journal_lines SET credit_amount=12.35 WHERE journal_entry_id=2 AND credit_amount>0",
            "DELETE FROM bank_journal_reconciliation_links",
            "DELETE FROM audit_log WHERE event_type='bank_reconciliation_corrected'",
            "UPDATE audit_log SET message=json_set(message,'$.effect','manufactured_posting') WHERE event_type='bank_reconciliation_corrected'",
        ] {
            let copy = tempfile::tempdir().unwrap();
            let copy_path = copy.path().join("corrupt.sqlite");
            std::fs::copy(&path, &copy_path).unwrap();
            let mut db = Connection::open(copy_path).unwrap();
            rewrite(&db, sql);
            assert_blocked(&mut db);
        }
    }
}
