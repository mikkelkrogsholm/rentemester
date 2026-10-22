//! Narrow authenticated MCP storage adapters; native/CLI force remains refused.
use super::*;
pub fn review_principal(tx: &Transaction<'_>, packet: &Value, actor: &str) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    tx.execute("INSERT OR IGNORE INTO period_close_readiness_packets(packet_hash,period_start,period_end,cutoff,packet_json) VALUES(?1,?2,?3,?4,?5)",params![packet["hash"].as_str(),packet["periodStart"].as_str(),packet["periodEnd"].as_str(),packet["cutoff"].as_str(),packet.to_string()]).map_err(invalid)?;
    let principal = if let Some(subject) = actor.strip_prefix("service-account:") {
        json!({"kind":"service-account","subjectId":subject})
    } else {
        json!({"kind":"user","subjectId":actor})
    };
    let actor = audit_actor(actor);
    let (id,created):(i64,String)=tx.query_row("INSERT INTO period_close_reviews(packet_id,packet_hash,reviewer_actor,reviewer_principal) VALUES((SELECT id FROM period_close_readiness_packets WHERE packet_hash=?1),?1,?2,?3) RETURNING id,created_at",params![packet["hash"].as_str(),actor,principal.to_string()],|r|Ok((r.get(0)?,r.get(1)?))).map_err(invalid)?;
    Ok(
        json!({"id":id,"packet":packet,"reviewerActor":actor,"reviewerPrincipal":principal,"createdAt":created}),
    )
}
pub fn list_original(tx: &Transaction<'_>) -> Result<Value, Error> {
    let mut q = tx.prepare("SELECT id,period_start,period_end,kind,status,reference,created_at FROM accounting_periods ORDER BY period_start DESC,id DESC").map_err(invalid)?;
    let periods = q.query_map([], |r| Ok(json!({"id":r.get::<_,i64>(0)?,"periodStart":r.get::<_,String>(1)?,"periodEnd":r.get::<_,String>(2)?,"kind":r.get::<_,String>(3)?,"status":r.get::<_,String>(4)?,"reference":r.get::<_,Option<String>>(5)?,"createdAt":r.get::<_,String>(6)?}))).map_err(invalid)?.collect::<Result<Vec<_>,_>>().map_err(invalid)?;
    Ok(json!({"ok":true,"count":periods.len(),"periods":periods}))
}
pub fn close_mcp(
    tx: &Transaction<'_>,
    input: &original::CloseInput,
    packet: &Value,
    actor: &str,
    today: &str,
) -> Result<Value, Error> {
    if input.force != Some(true) {
        return original::close_current(tx, input, packet, actor, today);
    }
    if !input.confirm
        || actor.trim().is_empty()
        || input.reason.as_deref().is_none_or(|r| r.trim().is_empty())
    {
        return Err(refused(
            "FORCED_CLOSE_REQUIRES_COMPANY_PERIOD_FORCE_CLOSE_PERMISSION_CONFIRM_ACTOR_AND_REASON",
        ));
    }
    if input.status.as_deref() == Some("reported") {
        return Err(refused("PERIOD_REPORT_DOES_NOT_ALLOW_FORCE"));
    }
    let items = packet["items"]
        .as_array()
        .ok_or_else(|| refused("PERIOD_CLOSE_ASSURANCE_UNAVAILABLE"))?;
    if items.iter().any(|i| {
        i["status"] == "unavailable" || (i["status"] == "blocked" && i["waivable"] != true)
    }) {
        return Err(refused("PERIOD_CLOSE_HAS_NONWAIVABLE_BLOCKERS"));
    }
    let native = Close {
        period_start: input.period_start.clone(),
        period_end: input.period_end.clone(),
        kind: input.kind.clone(),
        reference: input.reference.clone(),
        packet_hash: input.packet_hash.clone().unwrap_or_default(),
        review_id: input.review_id.unwrap_or(0),
        force: input.force,
        reason: input.reason.as_deref().map(|r| r.trim().to_owned()),
        confirm: input.confirm,
    };
    let mut value = close_authorized(tx, &native, packet, actor, today, false, true)?;
    value["readinessPacket"] = packet.clone();
    Ok(original::envelope(value))
}
pub(super) fn obligations(
    tx: &Transaction<'_>,
    period: i64,
    packet: &Value,
    actor: &str,
    reason: &str,
) -> Result<(), Error> {
    let decision: i64 = tx
        .query_row(
            "SELECT id FROM period_close_decisions WHERE period_id=?1 ORDER BY id DESC LIMIT 1",
            [period],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let mut count = 0;
    for item in packet["items"]
        .as_array()
        .ok_or_else(|| refused("PERIOD_CLOSE_ASSURANCE_UNAVAILABLE"))?
        .iter()
        .filter(|i| i["status"] == "blocked" && i["waivable"] == true)
    {
        tx.execute("INSERT INTO period_close_open_items(decision_id,period_id,packet_hash,code,severity,count,amount,evidence_json,reason,actor) VALUES(?1,?2,?3,?4,'blocker',?5,?6,?7,?8,?9)",params![decision,period,packet["hash"].as_str(),item["code"].as_str(),item["count"].as_i64(),item["amount"].as_f64(),item["evidence"].to_string(),reason,audit_actor(actor)]).map_err(invalid)?;
        count += 1;
    }
    if count > 0 {
        event(
            tx,
            period,
            "PERIOD_CLOSED_WITH_OPEN_ITEMS",
            actor,
            &format!("Forced close with {count} open controls: {reason}"),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod period_mcp_tests {
    use super::*;
    fn setup() -> rusqlite::Connection {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic MCP Period").unwrap();
        db.execute_batch("UPDATE companies SET cvr='87654321',vat_period_type='quarter'; INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('B','Bank','asset','debit'),('R','Receivable','asset','debit'),('C','Creditor','liability','credit'); INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('bank','B',1,'synthetic'),('debtors','R',1,'synthetic'),('creditors','C',1,'synthetic');").unwrap();
        db
    }
    fn input(packet: &Value, review: &Value) -> original::CloseInput {
        serde_json::from_value(json!({"periodStart":"2026-07-01","periodEnd":"2026-09-30","packetHash":packet["hash"],"reviewId":review["id"],"force":true,"reason":"  Explicit waiver  ","confirm":true})).unwrap()
    }
    #[test]
    fn period_mcp_zero_block_force_preserves_service_principal_and_native_refusal() {
        let mut db = setup();
        let tx = db.transaction().unwrap();
        let packet = compute(&tx, "2026-07-01", "2026-09-30", Ok(vec![])).unwrap();
        assert_eq!(packet["blockers"], 0, "{packet}");
        let review = review_principal(&tx, &packet, "service-account:synthetic").unwrap();
        assert_eq!(
            review["reviewerPrincipal"],
            json!({"kind":"service-account","subjectId":"synthetic"})
        );
        let i = input(&packet, &review);
        assert!(
            original::close_current(&tx, &i, &packet, "service-account:synthetic", "2026-08-01")
                .is_err()
        );
        let closed = original::atomic(&tx, Some(&packet), || {
            close_mcp(&tx, &i, &packet, "service-account:synthetic", "2026-08-01")
        })
        .unwrap();
        assert_eq!(closed["ok"], true, "{closed}");
        let stored: (String, String, String) = tx
            .query_row(
                "SELECT decision,actor,reason FROM period_close_decisions",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            stored,
            (
                "forced_closed".into(),
                "service-account:synthetic".into(),
                "Explicit waiver".into()
            )
        );
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM period_close_open_items", [], |r| r
                .get(0))
                .unwrap(),
            0
        );
        assert_eq!(list_original(&tx).unwrap()["count"], 1);
    }
    #[test]
    fn period_mcp_waiver_exact_obligation_source_exception_stays_open() {
        let mut db = setup();
        let tx = db.transaction().unwrap();
        tx.execute_batch("INSERT INTO documents(id,source,sha256_hash,invoice_date,status,document_type) VALUES(91,'synthetic','synthetic-source-hash','2026-08-01','ingested','other'); INSERT INTO exceptions(id,type,severity,status,related_document_id,message) VALUES(92,'synthetic-review','high','open',91,'Human review remains necessary');").unwrap();
        let packet = compute(&tx, "2026-07-01", "2026-09-30", Ok(vec![])).unwrap();
        let blocked: Vec<_> = packet["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["status"] == "blocked")
            .collect();
        assert_eq!(blocked.len(), 1, "{packet}");
        assert_eq!(blocked[0]["code"], "EXCEPTIONS_OPEN");
        let review = review_principal(&tx, &packet, "service-account:synthetic").unwrap();
        let i = input(&packet, &review);
        let closed = original::atomic(&tx, Some(&packet), || {
            close_mcp(&tx, &i, &packet, "service-account:synthetic", "2026-10-22")
        })
        .unwrap();
        assert_eq!(closed["ok"], true, "{closed}");
        let obligation: Value = tx.query_row("SELECT decision_id,period_id,packet_hash,code,severity,count,amount,evidence_json,reason,actor,status FROM period_close_open_items",[],|r|Ok(json!({"decision":r.get::<_,i64>(0)?,"period":r.get::<_,i64>(1)?,"hash":r.get::<_,String>(2)?,"code":r.get::<_,String>(3)?,"severity":r.get::<_,String>(4)?,"count":r.get::<_,i64>(5)?,"amount":r.get::<_,f64>(6)?,"evidence":r.get::<_,String>(7)?,"reason":r.get::<_,String>(8)?,"actor":r.get::<_,String>(9)?,"status":r.get::<_,String>(10)?}))).unwrap();
        assert_eq!(obligation["period"], closed["periodId"]);
        assert_eq!(obligation["hash"], packet["hash"]);
        assert_eq!(obligation["code"], blocked[0]["code"]);
        assert_eq!(obligation["count"], blocked[0]["count"]);
        assert_eq!(obligation["amount"], json!(0.0));
        assert_eq!(obligation["evidence"], blocked[0]["evidence"].to_string());
        assert_eq!(obligation["severity"], "blocker");
        assert_eq!(obligation["reason"], "Explicit waiver");
        assert_eq!(obligation["actor"], "service-account:synthetic");
        assert_eq!(obligation["status"], "open");
        assert_eq!(
            tx.query_row::<String, _, _>("SELECT status FROM exceptions WHERE id=92", [], |r| r
                .get(0))
                .unwrap(),
            "open"
        );
        assert_eq!(
            tx.query_row::<i64, _, _>(
                "SELECT count(*) FROM audit_log WHERE event_type='PERIOD_CLOSED_WITH_OPEN_ITEMS'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            1
        );
    }
    #[test]
    fn period_mcp_force_nonwaivable_stale_and_late_failure_are_atomic() {
        let mut db = setup();
        let tx = db.transaction().unwrap();
        let packet = compute(&tx, "2026-07-01", "2026-09-30", Ok(vec![])).unwrap();
        let review = review_principal(&tx, &packet, "synthetic").unwrap();
        let i = input(&packet, &review);
        let bad = compute(
            &tx,
            "2026-07-01",
            "2026-09-30",
            Ok(vec![json!({"documentId":9,"error":"missing original"})]),
        )
        .unwrap();
        let refusal = original::atomic(&tx, Some(&bad), || {
            close_mcp(&tx, &i, &bad, "synthetic", "2026-10-22")
        })
        .unwrap();
        assert_eq!(refusal["ok"], false);
        let mut stale = input(&packet, &review);
        stale.packet_hash = Some("0".repeat(64));
        assert_eq!(
            original::atomic(&tx, Some(&packet), || close_mcp(
                &tx,
                &stale,
                &packet,
                "synthetic",
                "2026-10-22"
            ))
            .unwrap()["ok"],
            false
        );
        tx.execute_batch("CREATE TEMP TRIGGER fail_period_decision BEFORE INSERT ON period_close_decisions BEGIN SELECT RAISE(ABORT,'synthetic late failure'); END;").unwrap();
        assert!(
            original::atomic(&tx, Some(&packet), || close_mcp(
                &tx,
                &i,
                &packet,
                "synthetic",
                "2026-10-22"
            ))
            .is_err()
        );
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM accounting_periods", [], |r| r.get(0))
                .unwrap(),
            0
        );
        assert_eq!(
            tx.query_row::<i64, _, _>(
                "SELECT count(*) FROM audit_log WHERE entity_type='accounting_period'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            0
        );
    }
}
