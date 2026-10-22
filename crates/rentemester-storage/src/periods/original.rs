//! Additive source CLI contracts over the native period evidence and locks.
use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Review {
    pub period_start: String,
    pub period_end: String,
    pub packet_hash: String,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseInput {
    pub period_start: String,
    pub period_end: String,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub reference: Option<String>,
    pub packet_hash: Option<String>,
    pub review_id: Option<i64>,
    pub force: Option<bool>,
    pub reason: Option<String>,
    pub confirm: bool,
}
pub fn hash_valid(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn status(tx: &Transaction<'_>, id: i64) -> Result<Value, Error> {
    let available: i64 = tx.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('period_close_reviews','period_close_readiness_packets')", [], |r| r.get(0)).map_err(invalid)?;
    if available != 2 {
        return Ok(
            json!({"ok":true,"status":"unavailable","code":"PERIOD_CLOSE_REVIEW_SCHEMA_UNAVAILABLE","review":null}),
        );
    }
    let review = persisted_review(tx, id)?;
    Ok(json!({"ok":true,"review":review}))
}
pub fn persisted_review(tx: &Transaction<'_>, id: i64) -> Result<Option<Value>, Error> {
    let row: Option<(i64,String,Option<String>,String,String)> = tx.query_row("SELECT r.id,r.reviewer_actor,r.reviewer_principal,r.created_at,p.packet_json FROM period_close_reviews r JOIN period_close_readiness_packets p ON p.id=r.packet_id WHERE r.id=?1", [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(invalid)?;
    row.map(|(id,actor,principal,created,packet)| {
        let packet: Value = serde_json::from_str(&packet).map_err(invalid)?;
        let principal: Value = principal.map(|p| serde_json::from_str(&p).map_err(invalid)).transpose()?.unwrap_or(Value::Null);
        Ok(json!({"id":id,"packet":packet,"reviewerActor":actor,"reviewerPrincipal":principal,"createdAt":created}))
    }).transpose()
}
pub fn review_expected(
    tx: &Transaction<'_>,
    input: &Review,
    packet: &Value,
    actor: &str,
) -> Result<Value, Error> {
    if !input.confirm || !hash_valid(&input.packet_hash) {
        return Err(refused("confirmation and exact packet hash required"));
    }
    if packet["hash"] != input.packet_hash {
        return Ok(
            json!({"ok":false,"errors":["PERIOD_CLOSE_PACKET_STALE_OR_MISSING"],"packetHash":packet["hash"]}),
        );
    }
    review(tx, packet, actor)
}
pub(super) fn envelope(mut value: Value) -> Value {
    let object = value.as_object_mut().expect("native period result object");
    if let Some(id) = object.remove("id") {
        object.insert("periodId".into(), id);
    }
    if object.get("reference") == Some(&Value::Null) {
        object.remove("reference");
    }
    object.insert("ok".into(), json!(true));
    object.insert(
        "appliedRules".into(),
        json!(["DK-BOOKKEEPING-PERIOD-LOCK-001"]),
    );
    object.insert("errors".into(), json!([]));
    value
}
pub fn refusal(message: &str, packet: Option<&Value>) -> Value {
    let mut value =
        json!({"ok":false,"appliedRules":["DK-BOOKKEEPING-PERIOD-LOCK-001"],"errors":[message]});
    if let Some(packet) = packet {
        value["readinessPacket"] = packet.clone();
    }
    value
}
/// Errors are converted only after rollback. This also protects callers which
/// handle a business refusal as a successful transport result in the outer fence.
pub fn atomic(
    tx: &Transaction<'_>,
    packet: Option<&Value>,
    apply: impl FnOnce() -> Result<Value, Error>,
) -> Result<Value, Error> {
    tx.execute_batch("SAVEPOINT original_period_operation")
        .map_err(invalid)?;
    match apply() {
        Ok(value) => {
            tx.execute_batch("RELEASE original_period_operation")
                .map_err(invalid)?;
            Ok(value)
        }
        Err(error) => {
            tx.execute_batch(
                "ROLLBACK TO original_period_operation; RELEASE original_period_operation",
            )
            .map_err(invalid)?;
            // Storage/integrity failures are not business refusals.
            if error.code != "invoice_refused" {
                return Err(error);
            }
            Ok(refusal(&error.message, packet))
        }
    }
}
/// A locked period's report must be resolved before any current readiness read.
pub fn report_existing(
    tx: &Transaction<'_>,
    input: &CloseInput,
    actor: &str,
) -> Result<Option<Value>, Error> {
    if input.status.as_deref() != Some("reported") {
        return Ok(None);
    }
    let kind = kind(input.kind.as_deref())?;
    let row: Option<(i64,String,Option<String>)> = tx.query_row("SELECT id,status,reference FROM accounting_periods WHERE period_start=?1 AND period_end=?2 AND (kind=?3 OR (?3='vat_period' AND kind='vat_quarter')) ORDER BY CASE kind WHEN 'vat_period' THEN 0 ELSE 1 END,id DESC LIMIT 1",params![input.period_start,input.period_end,kind],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(invalid)?;
    let Some((id, state, reference)) = row else {
        return Ok(None);
    };
    let effective = effective(tx, id, state)?;
    if effective == "open" {
        return Ok(None);
    }
    if input.force == Some(true) {
        return Err(refused("PERIOD_REPORT_DOES_NOT_ALLOW_FORCE"));
    }
    let proof: Option<(String,String)> = tx.query_row("SELECT d.packet_hash,p.packet_json FROM period_close_decisions d JOIN period_close_readiness_packets p ON p.packet_hash=d.packet_hash WHERE d.period_id=?1 AND d.decision IN ('closed','forced_closed') ORDER BY d.id DESC LIMIT 1",[id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(invalid)?;
    let (hash, packet) = proof.ok_or_else(|| refused("PERIOD_REPORT_CLOSE_PACKET_MISMATCH"))?;
    if input.packet_hash.as_deref() != Some(&hash) {
        return Err(refused("PERIOD_REPORT_CLOSE_PACKET_MISMATCH"));
    }
    if effective == "reported" {
        if input.reference != reference {
            return Err(refused("PERIOD_REPORT_REFERENCE_CONFLICT"));
        }
        return Ok(Some(envelope(
            json!({"id":id,"periodStart":input.period_start,"periodEnd":input.period_end,"kind":kind,"status":"reported","reference":reference}),
        )));
    }
    let reference = input
        .reference
        .as_deref()
        .filter(|r| !r.is_empty())
        .ok_or_else(|| refused("PERIOD_REPORT_REFERENCE_REQUIRED"))?;
    let native = Report {
        period_start: input.period_start.clone(),
        period_end: input.period_end.clone(),
        kind: input.kind.clone(),
        reference: reference.into(),
        packet_hash: hash,
        confirm: input.confirm,
    };
    let mut value = envelope(report(tx, &native, actor)?);
    value["readinessPacket"] = serde_json::from_str::<Value>(&packet).map_err(invalid)?;
    Ok(Some(value))
}
pub fn close_current(
    tx: &Transaction<'_>,
    input: &CloseInput,
    packet: &Value,
    actor: &str,
    today: &str,
) -> Result<Value, Error> {
    if !matches!(input.status.as_deref(), None | Some("closed" | "reported")) {
        return Err(refused("status must be closed or reported"));
    }
    if input.force == Some(true) {
        return Err(refused(
            "FORCED_CLOSE_REQUIRES_COMPANY_PERIOD_FORCE_CLOSE_PERMISSION_CONFIRM_ACTOR_AND_REASON",
        ));
    }
    let reviewed = input
        .review_id
        .map(|id| persisted_review(tx, id))
        .transpose()?
        .flatten();
    if reviewed.as_ref().is_none_or(|r| r["packet"] != *packet)
        || input.packet_hash.as_deref() != packet["hash"].as_str()
        || packet["periodStart"] != input.period_start
        || packet["periodEnd"] != input.period_end
    {
        return Err(refused("PERIOD_CLOSE_PACKET_STALE_OR_MISSING"));
    }
    if let Some(items) = packet["items"].as_array() {
        let blocked = items.iter().filter(|i| i["status"] == "blocked").count();
        let unavailable = items
            .iter()
            .filter(|i| i["status"] == "unavailable")
            .count();
        if blocked > 0 {
            return Err(refused(format!("PERIOD_CLOSE_BLOCKED:{blocked}")));
        }
        if unavailable > 0 {
            return Err(refused(format!(
                "PERIOD_CLOSE_ASSURANCE_UNAVAILABLE:{unavailable}"
            )));
        }
    }
    let native = Close {
        period_start: input.period_start.clone(),
        period_end: input.period_end.clone(),
        kind: input.kind.clone(),
        reference: input.reference.clone(),
        packet_hash: input.packet_hash.clone().unwrap_or_default(),
        review_id: input.review_id.unwrap_or(0),
        force: input.force,
        reason: input.reason.clone(),
        confirm: input.confirm,
    };
    let mut value = close_with_status(tx, &native, packet, actor, today, input.status.as_deref() == Some("reported"))?;
    value["readinessPacket"] = packet.clone();
    Ok(envelope(value))
}
pub fn reopen_current(
    tx: &Transaction<'_>,
    input: &Reopen,
    packet: &Value,
    actor: &str,
) -> Result<Value, Error> {
    reopen(tx, input, packet, actor).map(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> rusqlite::Connection {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Original Period")
            .unwrap();
        db.execute_batch("UPDATE companies SET cvr='87654321',vat_period_type='quarter'; INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('B','Bank','asset','debit'),('R','Receivable','asset','debit'),('C','Creditor','liability','credit'); INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('bank','B',1,'synthetic'),('debtors','R',1,'synthetic'),('creditors','C',1,'synthetic');").unwrap();
        db
    }
    fn counts(tx: &Transaction<'_>) -> (i64, i64, i64, i64) {
        tx.query_row("SELECT (SELECT count(*) FROM period_close_readiness_packets),(SELECT count(*) FROM period_close_reviews),(SELECT count(*) FROM audit_log),(SELECT count(*) FROM accounting_periods)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap()
    }
    #[test]
    fn expected_hash_stale_has_no_effect_and_history_is_persisted_nullable() {
        let mut db = setup();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).unwrap();
        let first = compute(&tx, "2026-07-01", "2026-09-30", Ok(vec![])).unwrap();
        let input = Review {
            period_start: "2026-07-01".into(),
            period_end: "2026-09-30".into(),
            packet_hash: first["hash"].as_str().unwrap().into(),
            confirm: true,
        };
        let review = review_expected(&tx, &input, &first, "synthetic").unwrap();
        let id = review["id"].as_i64().unwrap();
        assert_eq!(status(&tx, id).unwrap()["review"], review);
        let current = compute(
            &tx,
            "2026-07-01",
            "2026-09-30",
            Ok(vec![json!({"documentId":999,"error":"changed original"})]),
        )
        .unwrap();
        assert_ne!(first["hash"], current["hash"]);
        let before = counts(&tx);
        let stale = review_expected(&tx, &input, &current, "synthetic").unwrap();
        assert_eq!(
            stale,
            json!({"ok":false,"errors":["PERIOD_CLOSE_PACKET_STALE_OR_MISSING"],"packetHash":current["hash"]})
        );
        assert_eq!(counts(&tx), before);
        assert_eq!(status(&tx, id).unwrap()["review"], review);
        // Legacy nullable principals are read as null, not attributed to today's caller.
        let legacy: i64 = tx.query_row("INSERT INTO period_close_reviews(packet_id,packet_hash,reviewer_actor,reviewer_principal) SELECT packet_id,packet_hash,'legacy',NULL FROM period_close_reviews WHERE id=?1 RETURNING id",[id],|r|r.get(0)).unwrap();
        assert!(status(&tx, legacy).unwrap()["review"]["reviewerPrincipal"].is_null());
        tx.commit().unwrap();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).unwrap();
        assert_eq!(status(&tx, id).unwrap()["review"], review);
    }
    fn close_input(
        packet: &Value,
        review: &Value,
        status: &str,
        reference: Option<&str>,
    ) -> CloseInput {
        serde_json::from_value(json!({"periodStart":"2026-07-01","periodEnd":"2026-09-30","status":status,"reference":reference,"packetHash":packet["hash"],"reviewId":review["id"],"confirm":true})).unwrap()
    }
    #[test]
    fn fresh_report_optional_receipt_and_existing_report_exact_replay() {
        for direct in [false, true] {
            let mut db = setup();
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).unwrap();
            let packet = compute(&tx, "2026-07-01", "2026-09-30", Ok(vec![])).unwrap();
            assert_eq!(packet["blockers"], 0, "{packet}");
            let review = review(&tx, &packet, "synthetic").unwrap();
            let input = close_input(
                &packet,
                &review,
                if direct { "reported" } else { "closed" },
                None,
            );
            let closed = atomic(&tx, Some(&packet), || {
                close_current(&tx, &input, &packet, "synthetic", "2026-10-22")
            })
            .unwrap();
            assert_eq!(closed["ok"], true, "{closed}");
            assert!(!closed.as_object().unwrap().contains_key("id"));
            let audit_events: Vec<String> = tx.prepare("SELECT event_type FROM audit_log WHERE entity_type='accounting_period' ORDER BY id").unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
            assert_eq!(audit_events, vec![if direct { "period_report" } else { "period_close" }]);
            let report_input = close_input(
                &packet,
                &review,
                "reported",
                if direct { None } else { Some("SYN-RECEIPT") },
            );
            let reported = atomic(&tx, None, || {
                Ok(report_existing(&tx, &report_input, "synthetic")?.unwrap())
            })
            .unwrap();
            assert_eq!(reported["periodId"], closed["periodId"]);
            if direct {
                assert!(reported.get("reference").is_none());
            } else {
                assert_eq!(reported["readinessPacket"], packet);
            }
            let before = counts(&tx);
            let replay = report_existing(&tx, &report_input, "synthetic")
                .unwrap()
                .unwrap();
            assert_eq!(replay["periodId"], closed["periodId"]);
            assert_eq!(counts(&tx), before);
            let conflict = close_input(&packet, &review, "reported", Some("OTHER"));
            assert_eq!(
                atomic(&tx, None, || Ok(report_existing(
                    &tx,
                    &conflict,
                    "synthetic"
                )?
                .unwrap()))
                .unwrap()["ok"],
                false
            );
            assert_eq!(counts(&tx), before);
            let reopen = Reopen {
                period_start: input.period_start,
                period_end: input.period_end,
                kind: None,
                reason: "Correction".into(),
                confirm: true,
            };
            assert_eq!(
                atomic(&tx, None, || reopen_current(
                    &tx,
                    &reopen,
                    &packet,
                    "synthetic"
                ))
                .unwrap()["ok"],
                false
            );
            assert_eq!(counts(&tx), before);
        }
    }
    #[test]
    fn nested_late_refusal_rolls_back_all_effects() {
        let mut db = setup();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).unwrap();
        let packet = compute(&tx, "2026-07-01", "2026-09-30", Ok(vec![])).unwrap();
        let review = review(&tx, &packet, "synthetic").unwrap();
        let input = close_input(&packet, &review, "reported", None);
        // Inject a failure after the native close has inserted its row/audit/decision.
        tx.execute_batch("CREATE TEMP TRIGGER fail_terminal BEFORE INSERT ON audit_log WHEN NEW.event_type='period_report' BEGIN SELECT RAISE(ABORT,'synthetic terminal failure'); END;").unwrap();
        let before = counts(&tx);
        assert!(
            atomic(&tx, Some(&packet), || close_current(
                &tx,
                &input,
                &packet,
                "synthetic",
                "2026-10-22"
            ))
            .is_err()
        );
        assert_eq!(counts(&tx), before);
        let result = atomic(&tx, None, || {
            atomic(&tx, None, || {
                review_expected(
                    &tx,
                    &Review {
                        period_start: input.period_start.clone(),
                        period_end: input.period_end.clone(),
                        packet_hash: packet["hash"].as_str().unwrap().into(),
                        confirm: true,
                    },
                    &packet,
                    "synthetic",
                )?;
                Err(refused("synthetic nested refusal"))
            })
        })
        .unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(counts(&tx), before);
    }
}
