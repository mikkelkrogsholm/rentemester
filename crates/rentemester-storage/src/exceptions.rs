//! Review queue state only. Resolving a task never posts or repairs its underlying invariant.
use crate::invalid;
use rentemester_engine::Error;
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolve {
    pub note: Option<String>,
}
fn refused(message: impl Into<String>) -> Error {
    Error::new("exception_review_refused", message)
}
fn parsed(value: Option<String>) -> Value {
    value
        .map(|s| serde_json::from_str(&s).unwrap_or(Value::String(s)))
        .unwrap_or(Value::Null)
}
pub(crate) fn rows(tx: &Transaction<'_>, status: &str) -> Result<Vec<Value>, Error> {
    if !["open", "resolved", "all"].contains(&status) {
        return Err(refused("status must be open, resolved or all"));
    }
    let cutover: Option<String> = tx
        .query_row(
            "SELECT cut_over_date FROM opening_balances LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(invalid)?
        .flatten();
    let mut q = tx
        .prepare("SELECT id,period_start,period_end,status FROM accounting_periods ORDER BY id")
        .map_err(invalid)?;
    let periods = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let mut closed = vec![];
    for (id, start, end, state) in periods {
        if ["closed", "reported"].contains(&crate::periods::effective(tx, id, state)?.as_str()) {
            closed.push((start, end));
        }
    }
    let mut q=tx.prepare("SELECT e.id,e.type,e.severity,e.status,e.related_bank_transaction_id,e.related_document_id,e.message,e.required_action,e.source_evidence,e.posting_preview,e.created_at,e.resolved_at,e.resolved_by,e.resolution_note,COALESCE(bt.transaction_date,d.invoice_date,substr(d.upload_datetime,1,10)) FROM exceptions e LEFT JOIN bank_transactions bt ON bt.id=e.related_bank_transaction_id LEFT JOIN documents d ON d.id=e.related_document_id WHERE ?1='all' OR e.status=?1 ORDER BY e.id DESC").map_err(invalid)?;
    let mut query = q.query([status]).map_err(invalid)?;
    let mut rows = vec![];
    while let Some(r) = query.next().map_err(invalid)? {
        let date: Option<String> = r.get(14).map_err(invalid)?;
        let archived = date.as_ref().is_some_and(|d| {
            cutover.as_ref().is_some_and(|cut| d < cut)
                || closed.iter().any(|(start, end)| d >= start && d <= end)
        });
        if archived {
            continue;
        }
        rows.push(json!({"id":r.get::<_,i64>(0).map_err(invalid)?,"type":r.get::<_,String>(1).map_err(invalid)?,"severity":r.get::<_,String>(2).map_err(invalid)?,"status":r.get::<_,String>(3).map_err(invalid)?,"relatedBankTransactionId":r.get::<_,Option<i64>>(4).map_err(invalid)?,"relatedDocumentId":r.get::<_,Option<i64>>(5).map_err(invalid)?,"message":r.get::<_,String>(6).map_err(invalid)?,"requiredAction":r.get::<_,Option<String>>(7).map_err(invalid)?,"sourceEvidence":parsed(r.get(8).map_err(invalid)?),"postingPreview":parsed(r.get(9).map_err(invalid)?),"createdAt":r.get::<_,String>(10).map_err(invalid)?,"resolvedAt":r.get::<_,Option<String>>(11).map_err(invalid)?,"resolvedBy":r.get::<_,Option<String>>(12).map_err(invalid)?,"resolutionNote":r.get::<_,Option<String>>(13).map_err(invalid)?,"archived":false}));
    }
    Ok(rows)
}
pub fn list(tx: &Transaction<'_>, slug: &str, status: &str) -> Result<Value, Error> {
    crate::validate(tx)?;
    let rows = rows(tx, status)?;
    let mut counts = [0; 3];
    for row in &rows {
        let i = match row["severity"].as_str() {
            Some("high") => 0,
            Some("medium") => 1,
            _ => 2,
        };
        counts[i] += 1;
    }
    Ok(
        json!({"slug":slug,"company":crate::company_setup::profile(tx)?,"status":status,"rows":rows,"count":rows.len(),"bySeverity":{"high":counts[0],"medium":counts[1],"low":counts[2]}}),
    )
}
pub fn resolve(
    tx: &Transaction<'_>,
    id: i64,
    input: &Resolve,
    actor: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if id <= 0 || id > 9_007_199_254_740_991 || input.note.as_ref().is_some_and(|s| s.len() > 4096)
    {
        return Err(refused(
            "positive safe exception id and note up to 4096 bytes required",
        ));
    }
    let row: Option<(String, String, String, Option<String>)> = tx
        .query_row(
            "SELECT status,type,message,required_action FROM exceptions WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(invalid)?;
    let (status, kind, message, required_action) =
        row.ok_or_else(|| refused(format!("exception {id} does not exist")))?;
    if status == "resolved" {
        return Ok(json!({"id":id,"resolved":false}));
    }
    if status != "open" {
        return Err(refused("exception is no longer open; refresh the queue"));
    }
    let note = input
        .note
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let changed=tx.execute("UPDATE exceptions SET status='resolved',resolved_at=CURRENT_TIMESTAMP,resolved_by=?1,resolution_note=?2 WHERE id=?3 AND status='open'",params![format!("user:{actor}"),note,id]).map_err(invalid)?;
    if changed != 1 {
        return Err(refused("exception state changed; refresh the queue"));
    }
    let evidence = json!({"exceptionId":id,"type":kind,"message":message,"requiredAction":required_action,"note":note,"resolvedBy":format!("user:{actor}"),"effect":"review_only_no_posting"});
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('exception_resolve','exception',?1,?2,?3)",params![id.to_string(),evidence.to_string(),format!("user:{actor}")]).map_err(invalid)?;
    Ok(json!({"id":id,"resolved":true}))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (rusqlite::Connection, rusqlite::Connection) {
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        let mut l = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Exceptions").unwrap();
        l.execute_batch("INSERT INTO exceptions(id,type,severity,message,required_action,source_evidence,posting_preview) VALUES(1,'SYNTHETIC_REVIEW_REQUIRED','high','Synthetic exception review fixture','Review only; underlying posting is separate','{\"source\":\"synthetic\"}','{\"noPosting\":true}'),(2,'SYNTHETIC_OPTIONAL_NOTE','medium','Synthetic optional note',NULL,NULL,NULL);").unwrap();
        (c, l)
    }
    #[test]
    fn review_preserves_facts_first_actor_optional_note_and_immutable_audit_without_posting() {
        let (_c, mut l) = fixture();
        let tx = l
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let before = list(&tx, "synthetic", "open").unwrap();
        assert_eq!(before["count"], 2);
        let input = Resolve {
            note: Some("  Synthetic review completed; posting remains separate  ".into()),
        };
        assert_eq!(
            resolve(&tx, 1, &input, "reviewer-one").unwrap(),
            json!({"id":1,"resolved":true})
        );
        let first = list(&tx, "synthetic", "resolved").unwrap();
        let row = &first["rows"][0];
        assert_eq!(row["message"], "Synthetic exception review fixture");
        assert_eq!(
            row["requiredAction"],
            "Review only; underlying posting is separate"
        );
        assert_eq!(row["sourceEvidence"]["source"], "synthetic");
        assert_eq!(row["postingPreview"]["noPosting"], true);
        assert_eq!(row["resolvedBy"], "user:reviewer-one");
        assert_eq!(
            row["resolutionNote"],
            "Synthetic review completed; posting remains separate"
        );
        assert_eq!(
            resolve(
                &tx,
                1,
                &Resolve {
                    note: Some("replacement note".into())
                },
                "reviewer-two"
            )
            .unwrap(),
            json!({"id":1,"resolved":false})
        );
        assert_eq!(list(&tx, "synthetic", "resolved").unwrap(), first);
        resolve(&tx, 2, &Resolve { note: None }, "reviewer-one").unwrap();
        assert!(list(&tx, "synthetic", "resolved").unwrap()["rows"][0]["resolutionNote"].is_null());
        let audit:String=tx.query_row("SELECT message FROM audit_log WHERE event_type='exception_resolve' AND entity_id='1'",[],|r|r.get(0)).unwrap();
        let audit: Value = serde_json::from_str(&audit).unwrap();
        assert_eq!(audit["effect"], "review_only_no_posting");
        assert_eq!(
            audit["requiredAction"],
            "Review only; underlying posting is separate"
        );
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM audit_log WHERE event_type='exception_resolve'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        assert_eq!(
            tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        for sql in [
            "UPDATE audit_log SET message='altered' WHERE event_type='exception_resolve'",
            "DELETE FROM audit_log WHERE event_type='exception_resolve'",
            "DELETE FROM exceptions",
            "UPDATE exceptions SET status='open' WHERE id=1",
        ] {
            assert!(tx.execute_batch(sql).is_err());
        }
    }
    #[test]
    fn missing_invalid_or_stale_states_do_not_write_and_history_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        let file = dir.path().join("company.sqlite");
        let mut l = rusqlite::Connection::open(&file).unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Exceptions").unwrap();
        l.execute_batch("INSERT INTO exceptions(id,type,severity,status,message) VALUES(1,'SYNTHETIC','high','open','Synthetic fixture'),(2,'SYNTHETIC_STALE','low','ignored','Synthetic state');").unwrap();
        let tx = l
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let before = tx.total_changes();
        for id in [0, -1, 3, 2, 9_007_199_254_740_992] {
            assert!(resolve(&tx, id, &Resolve { note: None }, "synthetic").is_err());
        }
        assert!(
            resolve(
                &tx,
                1,
                &Resolve {
                    note: Some("x".repeat(4097))
                },
                "synthetic"
            )
            .is_err()
        );
        assert_eq!(tx.total_changes(), before);
        assert!(rows(&tx, "invalid").is_err());
        resolve(
            &tx,
            1,
            &Resolve {
                note: Some(" ".into()),
            },
            "synthetic",
        )
        .unwrap();
        let expected = list(&tx, "synthetic", "resolved").unwrap();
        tx.commit().unwrap();
        drop(l);
        let mut l = rusqlite::Connection::open(file).unwrap();
        let tx = l
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(list(&tx, "synthetic", "resolved").unwrap(), expected);
        assert_eq!(
            resolve(&tx, 1, &Resolve { note: None }, "other").unwrap()["resolved"],
            false
        );
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM audit_log WHERE event_type='exception_resolve'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
    #[test]
    fn list_follows_effective_closed_periods_and_keeps_undated_tasks_visible() {
        let (_c, mut l) = fixture();
        l.execute_batch("INSERT INTO bank_transactions(id,transaction_date,text,amount,currency) VALUES(1,'2026-01-01','Synthetic subject',1,'DKK');INSERT INTO exceptions(id,type,severity,message,related_bank_transaction_id) VALUES(3,'SYNTHETIC_DATED','low','Synthetic archived task',1);INSERT INTO accounting_periods(id,period_start,period_end,kind,status) VALUES(1,'2026-01-01','2026-01-31','custom','open');INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('period_close','accounting_period','1','Synthetic closed period','user:synthetic');").unwrap();
        let tx = l.transaction().unwrap();
        let listed = list(&tx, "synthetic", "open").unwrap();
        assert_eq!(listed["count"], 2);
        assert!(
            !listed["rows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["id"] == 3)
        );
    }
}
