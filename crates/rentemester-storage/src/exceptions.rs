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
    rows_with_archived(tx, status, false)
}
fn is_archived(date: Option<&str>, cutover: Option<&str>, closed: &[(String, String)]) -> bool {
    date.is_some_and(|d| {
        cutover.is_some_and(|cut| d < cut)
            || closed
                .iter()
                .any(|(start, end)| d >= start.as_str() && d <= end.as_str())
    })
}
pub(crate) fn rows_with_archived(
    tx: &Transaction<'_>,
    status: &str,
    include_archived: bool,
) -> Result<Vec<Value>, Error> {
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
        let archived = is_archived(date.as_deref(), cutover.as_deref(), &closed);
        if archived && !include_archived {
            continue;
        }
        rows.push(json!({"id":r.get::<_,i64>(0).map_err(invalid)?,"type":r.get::<_,String>(1).map_err(invalid)?,"severity":r.get::<_,String>(2).map_err(invalid)?,"status":r.get::<_,String>(3).map_err(invalid)?,"relatedBankTransactionId":r.get::<_,Option<i64>>(4).map_err(invalid)?,"relatedDocumentId":r.get::<_,Option<i64>>(5).map_err(invalid)?,"message":r.get::<_,String>(6).map_err(invalid)?,"requiredAction":r.get::<_,Option<String>>(7).map_err(invalid)?,"sourceEvidence":parsed(r.get(8).map_err(invalid)?),"postingPreview":parsed(r.get(9).map_err(invalid)?),"createdAt":r.get::<_,String>(10).map_err(invalid)?,"resolvedAt":r.get::<_,Option<String>>(11).map_err(invalid)?,"resolvedBy":r.get::<_,Option<String>>(12).map_err(invalid)?,"resolutionNote":r.get::<_,Option<String>>(13).map_err(invalid)?,"archived":archived}));
    }
    Ok(rows)
}
pub fn list(tx: &Transaction<'_>, slug: &str, status: &str) -> Result<Value, Error> {
    list_with_archived(tx, slug, status, false)
}
pub fn list_with_archived(
    tx: &Transaction<'_>,
    slug: &str,
    status: &str,
    include_archived: bool,
) -> Result<Value, Error> {
    crate::validate(tx)?;
    let rows = rows_with_archived(tx, status, include_archived)?;
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
/// Consumer projection only: native producers are not supplied by this flow.
pub fn agent_suggestions(tx: &Transaction<'_>, slug: &str) -> Result<Value, Error> {
    crate::validate(tx)?;
    let mut rows = vec![];
    for row in rows_with_archived(tx, "open", true)? {
        let kind = row["type"].as_str().unwrap_or("");
        if !kind.starts_with("AGENT_") {
            continue;
        }
        let (label, link) = match kind {
            "AGENT_PAYABLE_OVERDUE" => ("Overforfalden kreditorpost", Some("leverandoerfaktura")),
            "AGENT_ACCRUAL_RECOGNITION_DUE" => {
                ("Periodeafgrænsning klar til bogføring", Some("posteringer"))
            }
            "AGENT_TAX_RETURN_NEEDS_REVIEW" => ("Oplysningsskema kræver gennemgang", None),
            "AGENT_POSSIBLE_FIXED_ASSET" => ("Muligt anlæg — bør kapitaliseres", Some("anlaeg")),
            _ => ("Agent-forslag", None),
        };
        let severity = match row["severity"].as_str() {
            Some("high") => "high",
            Some("medium") => "medium",
            _ => "low",
        };
        // Retain genuine audit attribution, including nullable provenance. As in the
        // original LEFT JOIN, each matching audit record is a correlated row.
        let mut q = tx.prepare("SELECT actor,event_type FROM audit_log WHERE entity_type='exception' AND entity_id=?1 ORDER BY id").map_err(invalid)?;
        let mut provenance = q
            .query_map([row["id"].to_string()], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                ))
            })
            .map_err(invalid)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(invalid)?;
        if provenance.is_empty() {
            provenance.push((None, None));
        }
        let (evidence, preview): (Option<String>, Option<String>) = tx
            .query_row(
                "SELECT source_evidence,posting_preview FROM exceptions WHERE id=?1",
                [row["id"].as_i64().unwrap()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(invalid)?;
        let source_parse = |raw: Option<String>| parsed(raw.filter(|s| !s.is_empty()));
        let evidence = source_parse(evidence);
        let preview = source_parse(preview);
        for (actor, program) in provenance {
            rows.push(json!({"exceptionId":row["id"],"type":kind,"kindLabel":label,"severity":severity,"rationale":row["message"],"requiredAction":row["requiredAction"],"ruleId":evidence.get("rule").and_then(Value::as_str),"sourceEvidence":evidence,"postingPreview":preview,"agentActor":actor,"agentProgram":program,"createdAt":row["createdAt"],"relatedDocumentId":row["relatedDocumentId"],"relatedBankTransactionId":row["relatedBankTransactionId"],"link":link}));
        }
    }
    let rank = |r: &Value| match r["severity"].as_str() {
        Some("high") => 2,
        Some("medium") => 1,
        _ => 0,
    };
    rows.sort_by(|a, b| {
        rank(b)
            .cmp(&rank(a))
            .then_with(|| b["exceptionId"].as_i64().cmp(&a["exceptionId"].as_i64()))
    });
    let mut counts = [0; 3];
    for row in &rows {
        counts[rank(row)] += 1;
    }
    let profile = crate::company_setup::profile(tx)?;
    let company = json!({"name":profile["name"],"cvr":profile["cvr"],"country":profile["country"],"currency":profile["currency"],"fiscalYearStartMonth":profile["fiscalYearStartMonth"],"fiscalYearLabelStrategy":profile["fiscalYearLabelStrategy"]});
    Ok(
        json!({"slug":slug,"company":company,"count":rows.len(),"rows":rows,"bySeverity":{"high":counts[2],"medium":counts[1],"low":counts[0]}}),
    )
}
#[derive(Clone, Copy)]
pub enum AgentDecision {
    Approve,
    Reject,
}
impl AgentDecision {
    pub fn note(self, note: Option<&str>) -> Result<String, Error> {
        let prefix = match self {
            Self::Approve => "Godkendt af ejer i cockpit",
            Self::Reject => "Afvist af ejer i cockpit",
        };
        let note = note.unwrap_or("").trim();
        let note = if note.is_empty() {
            prefix.to_owned()
        } else {
            format!("{prefix}: {note}")
        };
        if note.len() > 4096 {
            return Err(Error::new(
                "agent_suggestion_invalid",
                "note including decision prefix must not exceed 4096 bytes",
            ));
        }
        Ok(note)
    }
}
pub fn decide_agent_suggestion(
    tx: &Transaction<'_>,
    id: i64,
    decision: AgentDecision,
    note: Option<&str>,
    principal: Principal<'_>,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !(1..=9_007_199_254_740_991).contains(&id) {
        return Err(Error::new(
            "agent_suggestion_invalid",
            "positive safe exception id required",
        ));
    }
    let note = decision.note(note)?;
    let row: Option<(String, String)> = tx
        .query_row(
            "SELECT status,type FROM exceptions WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(invalid)?;
    let Some((status, kind)) = row else {
        return Err(Error::new(
            "agent_suggestion_stale",
            "suggestion absent or no longer open",
        ));
    };
    if status != "open" {
        return Err(Error::new(
            "agent_suggestion_stale",
            "suggestion absent or no longer open",
        ));
    }
    if !kind.starts_with("AGENT_") {
        return Err(Error::new(
            "agent_suggestion_wrong_type",
            "exception is not an Agent suggestion",
        ));
    }
    resolve_principal(tx, id, &Resolve { note: Some(note) }, principal)?;
    Ok(
        json!({"id":id,"decision":match decision { AgentDecision::Approve => "approved", AgentDecision::Reject => "rejected" },"resolved":true}),
    )
}
pub fn resolve(
    tx: &Transaction<'_>,
    id: i64,
    input: &Resolve,
    actor: &str,
) -> Result<Value, Error> {
    resolve_principal(tx, id, input, Principal::Human(actor))
}
/// Trusted identity kind and subject supplied only by the current authority fence.
pub enum Principal<'a> {
    Human(&'a str),
    ServiceAccount(&'a str),
}
pub fn resolve_principal(
    tx: &Transaction<'_>,
    id: i64,
    input: &Resolve,
    principal: Principal<'_>,
) -> Result<Value, Error> {
    let actor = match principal {
        Principal::Human(subject) => format!("user:{subject}"),
        Principal::ServiceAccount(subject) => format!("service-account:{subject}"),
    };
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
    let changed=tx.execute("UPDATE exceptions SET status='resolved',resolved_at=CURRENT_TIMESTAMP,resolved_by=?1,resolution_note=?2 WHERE id=?3 AND status='open'",params![actor,note,id]).map_err(invalid)?;
    if changed != 1 {
        return Err(refused("exception state changed; refresh the queue"));
    }
    let evidence = json!({"exceptionId":id,"type":kind,"message":message,"requiredAction":required_action,"note":note,"resolvedBy":actor,"effect":"review_only_no_posting"});
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('exception_resolve','exception',?1,?2,?3)",params![id.to_string(),evidence.to_string(),actor]).map_err(invalid)?;
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
    fn agent_suggestions_projection_archived_unknown_provenance_and_review_only() {
        let (_c, mut l) = fixture();
        l.execute_batch("INSERT INTO bank_transactions(id,transaction_date,text,amount,currency) VALUES(1,'2026-01-01','Synthetic archived evidence',1,'DKK'); INSERT INTO accounting_periods(id,period_start,period_end,kind,status) VALUES(1,'2026-01-01','2026-01-31','custom','closed'); INSERT INTO exceptions(id,type,severity,message,source_evidence,posting_preview,related_bank_transaction_id) VALUES(3,'AGENT_PAYABLE_OVERDUE','high','Synthetic agent fixture','{\"rule\":\"SYNTHETIC-RULE\"}','{\"noPosting\":true}',1),(4,'AGENT_UNKNOWN','medium','Synthetic unknown','invalid json','',NULL),(5,'AGENT_POSSIBLE_FIXED_ASSET','low','Synthetic low','{\"rule\":3}',NULL,NULL),(6,'AGENTX_FALSE','high','Not literal prefix',NULL,NULL,NULL); INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('synthetic_agent_record','exception','3','Synthetic provenance','system:synthetic-agent');").unwrap();
        let tx = l
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let changes = tx.total_changes();
        let listed = agent_suggestions(&tx, "synthetic").unwrap();
        assert_eq!(tx.total_changes(), changes);
        assert_eq!(listed["count"], 3);
        assert_eq!(listed["bySeverity"], json!({"high":1,"medium":1,"low":1}));
        let rows = listed["rows"].as_array().unwrap();
        assert_eq!(rows[0]["exceptionId"], 3);
        assert_eq!(rows[0]["ruleId"], "SYNTHETIC-RULE");
        assert_eq!(rows[0]["agentActor"], "system:synthetic-agent");
        assert_eq!(rows[0]["agentProgram"], "synthetic_agent_record");
        assert_eq!(rows[0]["link"], "leverandoerfaktura");
        assert_eq!(rows[0].as_object().unwrap().len(), 15);
        assert_eq!(rows[1]["kindLabel"], "Agent-forslag");
        assert_eq!(rows[1]["sourceEvidence"], "invalid json");
        assert!(rows[1]["postingPreview"].is_null());
        assert!(rows[1]["agentActor"].is_null());
        assert!(rows[1]["link"].is_null());
        assert!(rows[2]["ruleId"].is_null());
        // Snapshot every existing non-review table: decisions must not touch any
        // journal, bank, VAT, payable, provider or other business state.
        let snapshot = || {
            let mut q = tx.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT IN ('exceptions','audit_log','sqlite_sequence') ORDER BY name").unwrap();
            let names = q
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            names
                .into_iter()
                .map(|name| {
                    let mut q = tx
                        .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
                        .unwrap();
                    let columns = q.column_count();
                    let rows = q
                        .query_map([], |r| {
                            (0..columns)
                                .map(|i| r.get::<_, rusqlite::types::Value>(i))
                                .collect::<Result<Vec<_>, _>>()
                        })
                        .unwrap()
                        .collect::<Result<Vec<_>, _>>()
                        .unwrap();
                    (name, rows)
                })
                .collect::<Vec<_>>()
        };
        let facts = snapshot();
        assert_eq!(
            decide_agent_suggestion(
                &tx,
                3,
                AgentDecision::Approve,
                Some("  Synthetic accepted  "),
                Principal::Human("actual-human")
            )
            .unwrap(),
            json!({"id":3,"decision":"approved","resolved":true})
        );
        assert_eq!(
            decide_agent_suggestion(
                &tx,
                4,
                AgentDecision::Reject,
                None,
                Principal::ServiceAccount("actual-service")
            )
            .unwrap()["decision"],
            "rejected"
        );
        let changes = tx.total_changes();
        for (id, code) in [
            (3, "agent_suggestion_stale"),
            (999, "agent_suggestion_stale"),
            (1, "agent_suggestion_wrong_type"),
        ] {
            assert_eq!(
                decide_agent_suggestion(
                    &tx,
                    id,
                    AgentDecision::Reject,
                    None,
                    Principal::Human("other")
                )
                .unwrap_err()
                .code,
                code
            );
        }
        assert!(
            AgentDecision::Approve
                .note(Some(&"x".repeat(4096)))
                .is_err()
        );
        assert_eq!(tx.total_changes(), changes);
        let audit: String = tx.query_row("SELECT message FROM audit_log WHERE event_type='exception_resolve' AND entity_id='3' AND actor='user:actual-human'", [], |r| r.get(0)).unwrap();
        let audit: Value = serde_json::from_str(&audit).unwrap();
        assert_eq!(
            audit["note"],
            "Godkendt af ejer i cockpit: Synthetic accepted"
        );
        assert_eq!(audit["effect"], "review_only_no_posting");
        assert_eq!(tx.query_row("SELECT resolution_note FROM exceptions WHERE id=4 AND resolved_by='service-account:actual-service'", [], |r| r.get::<_,String>(0)).unwrap(), "Afvist af ejer i cockpit");
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
        assert_eq!(snapshot(), facts);
        assert_eq!(
            tx.query_row("SELECT amount FROM bank_transactions WHERE id=1", [], |r| r
                .get::<_, f64>(0))
                .unwrap(),
            1.0
        );
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
    fn archival_dates_use_strict_cutover_inclusive_closed_periods_and_no_undated_guess() {
        let closed = vec![("2026-02-01".into(), "2026-02-28".into())];
        assert!(is_archived(Some("2025-12-31"), Some("2026-01-01"), &closed));
        assert!(!is_archived(
            Some("2026-01-01"),
            Some("2026-01-01"),
            &closed
        ));
        for date in ["2026-02-01", "2026-02-14", "2026-02-28"] {
            assert!(is_archived(Some(date), None, &closed));
        }
        assert!(!is_archived(Some("2026-03-01"), None, &closed));
        assert!(!is_archived(None, Some("2026-01-01"), &closed));
    }
    #[test]
    fn list_follows_effective_closed_periods_and_keeps_undated_tasks_visible() {
        let (_c, mut l) = fixture();
        l.execute_batch("INSERT INTO bank_transactions(id,transaction_date,text,amount,currency) VALUES(1,'2026-01-01','Synthetic subject',1,'DKK');INSERT INTO exceptions(id,type,severity,message,related_bank_transaction_id) VALUES(3,'SYNTHETIC_DATED','low','Synthetic archived task',1);INSERT INTO accounting_periods(id,period_start,period_end,kind,status) VALUES(1,'2026-01-01','2026-01-31','custom','open');INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('period_close','accounting_period','1','Synthetic closed period','user:synthetic');").unwrap();
        let tx = l.transaction().unwrap();
        let listed = list(&tx, "synthetic", "open").unwrap();
        assert_eq!(listed["count"], 2);
        let changes = tx.total_changes();
        assert_eq!(
            listed,
            list_with_archived(&tx, "synthetic", "open", false).unwrap()
        );
        let included = list_with_archived(&tx, "synthetic", "open", true).unwrap();
        assert_eq!(included["count"], 3);
        assert_eq!(included["rows"][0]["id"], 3);
        assert_eq!(included["rows"][0]["archived"], true);
        assert_eq!(included["rows"][1]["archived"], false);
        assert_eq!(included["bySeverity"], json!({"high":1,"medium":1,"low":1}));
        assert_eq!(
            list_with_archived(&tx, "synthetic", "resolved", true).unwrap()["count"],
            0
        );
        assert!(list_with_archived(&tx, "synthetic", "invalid", true).is_err());
        assert_eq!(tx.total_changes(), changes);
        assert!(
            !listed["rows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["id"] == 3)
        );
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    #[test]
    fn exception_original_resolution_preserves_genuine_forced_open_obligations() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(
            &mut control,
            &mut db,
            "Synthetic Exception Obligations",
        )
        .unwrap();
        db.execute_batch("UPDATE companies SET cvr='87654321',vat_period_type='quarter'; INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('B','Bank','asset','debit'),('R','Receivable','asset','debit'),('C','Creditor','liability','credit'); INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('bank','B',1,'synthetic'),('debtors','R',1,'synthetic'),('creditors','C',1,'synthetic'); INSERT INTO documents(id,source,sha256_hash,invoice_date,status,document_type) VALUES(91,'synthetic','synthetic-source-hash','2026-08-01','ingested','other'); INSERT INTO exceptions(id,type,severity,status,related_document_id,message) VALUES(92,'synthetic-review','high','open',91,'Human review remains necessary');").unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let packet = crate::periods::compute(&tx, "2026-07-01", "2026-09-30", Ok(vec![])).unwrap();
        assert!(packet["blockers"].as_i64().unwrap() > 0);
        let principal = crate::periods::Principal::Human("reviewer");
        let review = crate::periods::review_principal(&tx, &packet, principal).unwrap();
        let input = serde_json::from_value(json!({"periodStart":"2026-07-01","periodEnd":"2026-09-30","packetHash":packet["hash"],"reviewId":review["id"],"force":true,"reason":"Explicit synthetic waiver","confirm":true})).unwrap();
        let closed = crate::periods::original::atomic(&tx, Some(&packet), || {
            crate::periods::close_mcp(&tx, &input, &packet, principal, "2026-10-22")
        })
        .unwrap();
        assert_eq!(closed["ok"], true, "{closed}");
        let snapshot = || {
            let mut q = tx
                .prepare("SELECT * FROM period_close_open_items ORDER BY id")
                .unwrap();
            let n = q.column_count();
            q.query_map([], |r| {
                (0..n)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
        };
        let obligations = snapshot();
        assert!(!obligations.is_empty());
        assert_eq!(
            resolve_principal(
                &tx,
                92,
                &Resolve {
                    note: Some(" first ".into())
                },
                Principal::ServiceAccount("reviewer")
            )
            .unwrap()["resolved"],
            true
        );
        assert_eq!(snapshot(), obligations);
        let first = list_with_archived(&tx, "synthetic", "resolved", true).unwrap();
        assert_eq!(first["rows"][0]["resolvedBy"], "service-account:reviewer");
        assert_eq!(first["rows"][0]["resolutionNote"], "first");
        assert_eq!(
            resolve(
                &tx,
                92,
                &Resolve {
                    note: Some("overwrite".into())
                },
                "service-account:human"
            )
            .unwrap()["resolved"],
            false
        );
        assert_eq!(
            list_with_archived(&tx, "synthetic", "resolved", true).unwrap(),
            first
        );
        let changes = tx.total_changes();
        let error = resolve(&tx, 999, &Resolve { note: None }, "reviewer").unwrap_err();
        assert_eq!(error.message, "exception 999 does not exist");
        assert_eq!(tx.total_changes(), changes);
        assert_eq!(snapshot(), obligations);
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM audit_log WHERE event_type='exception_resolve'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn exception_original_effective_reopened_visibility_and_typed_human_service_like_id() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Reopen").unwrap();
        db.execute_batch("INSERT INTO accounting_periods(id,period_start,period_end,kind,status) VALUES(1,'2026-01-01','2026-01-31','custom','closed'); INSERT INTO bank_transactions(id,transaction_date,text,amount,currency) VALUES(1,'2026-01-01','Synthetic',1,'DKK'); INSERT INTO exceptions(id,type,severity,message,related_bank_transaction_id) VALUES(1,'SYNTHETIC','low','Synthetic',1); INSERT INTO exceptions(id,type,severity,message) VALUES(2,'UNDATED','low','Synthetic undated');").unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(list(&tx, "synthetic", "open").unwrap()["count"], 1);
        tx.execute_batch("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('period_reopen','accounting_period','1','Synthetic reopen','user:reviewer');").unwrap();
        let visible = list(&tx, "synthetic", "open").unwrap();
        assert_eq!(visible["count"], 2);
        assert_eq!(visible["rows"][1]["archived"], false);
        assert_eq!(visible["rows"][1].as_object().unwrap().len(), 15);
        resolve(
            &tx,
            1,
            &Resolve {
                note: Some("   ".into()),
            },
            "service-account:human",
        )
        .unwrap();
        let resolved = list(&tx, "synthetic", "resolved").unwrap();
        assert_eq!(
            resolved["rows"][0]["resolvedBy"],
            "user:service-account:human"
        );
        assert!(resolved["rows"][0]["resolutionNote"].is_null());
    }
}
