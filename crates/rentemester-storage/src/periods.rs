//! Reviewed period locks. All lifecycle changes append evidence on the admitted writer.
mod audit;
mod readiness;
use crate::invalid;
pub use readiness::compute;
use rentemester_engine::{Error, fiscal_year::Date, sales_invoice::refused, vat_period};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Bounds {
    pub period_start: String,
    pub period_end: String,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Close {
    pub period_start: String,
    pub period_end: String,
    pub kind: Option<String>,
    pub reference: Option<String>,
    pub packet_hash: String,
    pub review_id: i64,
    pub force: Option<bool>,
    pub reason: Option<String>,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reopen {
    pub period_start: String,
    pub period_end: String,
    pub kind: Option<String>,
    pub reason: String,
    pub confirm: bool,
}
pub(crate) fn bounds(start: &str, end: &str) -> Result<(), Error> {
    if Date::parse(start)? > Date::parse(end)? {
        return Err(refused("period start must not exceed period end"));
    }
    Ok(())
}
fn kind(value: Option<&str>) -> Result<&str, Error> {
    match value.unwrap_or("vat_period") {
        "vat_period" | "vat_quarter" => Ok("vat_period"),
        "fiscal_year" => Ok("fiscal_year"),
        "custom" => Ok("custom"),
        _ => Err(refused("invalid accounting period kind")),
    }
}
pub(crate) fn effective(tx: &Transaction<'_>, id: i64, mut state: String) -> Result<String, Error> {
    let mut q=tx.prepare("SELECT event_type FROM audit_log WHERE entity_type='accounting_period' AND entity_id=?1 ORDER BY id").map_err(invalid)?;
    for event in q
        .query_map([id.to_string()], |r| r.get::<_, String>(0))
        .map_err(invalid)?
    {
        if state != "reported" {
            match event.map_err(invalid)?.as_str() {
                "period_reopen" => state = "open".into(),
                "period_close" => state = "closed".into(),
                "period_report" => state = "reported".into(),
                _ => {}
            }
        }
    }
    Ok(state)
}
pub fn list(tx: &Transaction<'_>, slug: &str) -> Result<Value, Error> {
    let company:Value=tx.query_row("SELECT name,cvr,country,currency FROM companies WHERE id=1",[],|r|Ok(json!({"name":r.get::<_,String>(0)?,"cvr":r.get::<_,Option<String>>(1)?,"country":r.get::<_,String>(2)?,"currency":r.get::<_,String>(3)?}))).map_err(invalid)?;
    let mut q=tx.prepare("SELECT id,period_start,period_end,kind,status,closed_at,closed_by,reference FROM accounting_periods ORDER BY period_start,id").map_err(invalid)?;
    let mut rs = q.query([]).map_err(invalid)?;
    let mut rows = vec![];
    let mut counts = json!({"open":0,"closed":0,"reported":0});
    while let Some(r) = rs.next().map_err(invalid)? {
        let id: i64 = r.get(0).map_err(invalid)?;
        let status: String = r.get(4).map_err(invalid)?;
        let state = effective(tx, id, status.clone())?;
        counts[&state] = json!(
            counts[&state]
                .as_u64()
                .ok_or_else(|| refused("invalid period state"))?
                + 1
        );
        rows.push(json!({"id":id,"periodStart":r.get::<_,String>(1).map_err(invalid)?,"periodEnd":r.get::<_,String>(2).map_err(invalid)?,"kind":r.get::<_,String>(3).map_err(invalid)?,"rowStatus":status,"effectiveStatus":state,"closedAt":r.get::<_,Option<String>>(5).map_err(invalid)?,"closedBy":r.get::<_,Option<String>>(6).map_err(invalid)?,"reference":r.get::<_,Option<String>>(7).map_err(invalid)?}));
    }
    Ok(json!({"slug":slug,"company":company,"periods":rows,"byStatus":counts}))
}
pub fn review(tx: &Transaction<'_>, packet: &Value, actor: &str) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    tx.execute("INSERT OR IGNORE INTO period_close_readiness_packets(packet_hash,period_start,period_end,cutoff,packet_json) VALUES(?1,?2,?3,?4,?5)",params![packet["hash"].as_str(),packet["periodStart"].as_str(),packet["periodEnd"].as_str(),packet["cutoff"].as_str(),packet.to_string()]).map_err(invalid)?;
    let principal = json!({"kind":"user","subjectId":actor}).to_string();
    let (id,created):(i64,String)=tx.query_row("INSERT INTO period_close_reviews(packet_id,packet_hash,reviewer_actor,reviewer_principal) VALUES((SELECT id FROM period_close_readiness_packets WHERE packet_hash=?1),?1,?2,?3) RETURNING id,created_at",params![packet["hash"].as_str(),format!("user:{actor}"),principal],|r|Ok((r.get(0)?,r.get(1)?))).map_err(invalid)?;
    Ok(
        json!({"id":id,"packet":packet,"reviewerActor":format!("user:{actor}"),"reviewerPrincipal":{"kind":"user","subjectId":actor},"createdAt":created}),
    )
}
fn event(
    tx: &Transaction<'_>,
    id: i64,
    event: &str,
    actor: &str,
    message: &str,
) -> Result<(), Error> {
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'accounting_period',?2,?3,?4)",params![event,id.to_string(),message,format!("user:{actor}")]).map_err(invalid)?;
    Ok(())
}
fn decision(
    tx: &Transaction<'_>,
    id: i64,
    packet: &Value,
    value: &str,
    actor: &str,
    reason: Option<&str>,
) -> Result<(), Error> {
    tx.execute("INSERT INTO period_close_decisions(period_id,packet_hash,decision,actor,reason,supersedes_decision_id) VALUES(?1,?2,?3,?4,?5,(SELECT id FROM period_close_decisions WHERE period_id=?1 ORDER BY id DESC LIMIT 1))",params![id,packet["hash"].as_str(),value,format!("user:{actor}"),reason]).map_err(invalid)?;
    Ok(())
}
pub fn close(
    tx: &Transaction<'_>,
    input: &Close,
    packet: &Value,
    actor: &str,
    today: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    bounds(&input.period_start, &input.period_end)?;
    if !input.confirm || input.force == Some(true) {
        return Err(refused(
            "confirmation required; forced period close is not ported to native execution",
        ));
    }
    if Date::parse(&input.period_end)? > Date::parse(today)? {
        return Err(refused(
            "period ends in the future; wait until it is complete",
        ));
    }
    let kind = kind(input.kind.as_deref())?;
    if kind == "vat_period" {
        let cadence: Option<String> = tx
            .query_row(
                "SELECT vat_period_type FROM companies WHERE id=1",
                [],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        let cadence = cadence.ok_or_else(|| refused("VAT registration required"))?;
        let window = vat_period::window(&input.period_start, &cadence)?;
        if window.start != input.period_start || window.end != input.period_end {
            return Err(refused(
                "VAT close requires an exact period for the registered cadence",
            ));
        }
    }
    let stored:Option<String>=tx.query_row("SELECT p.packet_json FROM period_close_reviews r JOIN period_close_readiness_packets p ON p.id=r.packet_id WHERE r.id=?1 AND r.packet_hash=?2",params![input.review_id,input.packet_hash],|r|r.get(0)).optional().map_err(invalid)?;
    let stored: Value = stored
        .map(|s| serde_json::from_str(&s).map_err(invalid))
        .transpose()?
        .ok_or_else(|| refused("PERIOD_CLOSE_PACKET_STALE_OR_MISSING"))?;
    if stored != *packet
        || packet["hash"] != input.packet_hash
        || packet["periodStart"] != input.period_start
        || packet["periodEnd"] != input.period_end
    {
        return Err(refused("PERIOD_CLOSE_PACKET_STALE_OR_MISSING"));
    }
    if packet["blockers"].as_u64() != Some(0) {
        return Err(refused(
            "PERIOD_CLOSE_BLOCKED: resolve every blocked or unavailable control",
        ));
    }
    let overlap:Option<(i64,String,String,String,String)>=tx.query_row("SELECT id,period_start,period_end,kind,status FROM accounting_periods WHERE NOT(period_end<?1 OR period_start>?2) AND (kind=?3 OR (?3='vat_period' AND kind='vat_quarter')) ORDER BY id LIMIT 1",params![input.period_start,input.period_end,kind],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(invalid)?;
    let id = if let Some((id, start, end, _, status)) = overlap {
        if start != input.period_start
            || end != input.period_end
            || effective(tx, id, status)? != "open"
        {
            return Err(refused("period overlaps an existing locked period"));
        }
        id
    } else {
        tx.query_row("INSERT INTO accounting_periods(period_start,period_end,kind,status,closed_at,closed_by,reference) VALUES(?1,?2,?3,'closed',CURRENT_TIMESTAMP,?4,?5) RETURNING id",params![input.period_start,input.period_end,kind,format!("user:{actor}"),input.reference],|r|r.get(0)).map_err(invalid)?
    };
    event(
        tx,
        id,
        "period_close",
        actor,
        &format!("Closed {kind} {}..{}", input.period_start, input.period_end),
    )?;
    decision(tx, id, packet, "closed", actor, None)?;
    Ok(
        json!({"id":id,"periodStart":input.period_start,"periodEnd":input.period_end,"kind":kind,"status":"closed","reference":input.reference}),
    )
}
pub fn reopen(
    tx: &Transaction<'_>,
    input: &Reopen,
    packet: &Value,
    actor: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    bounds(&input.period_start, &input.period_end)?;
    let kind = kind(input.kind.as_deref())?;
    if !input.confirm || input.reason.trim().is_empty() || input.reason.len() > 4096 {
        return Err(refused("confirmation and bounded reopen reason required"));
    }
    if kind == "vat_period" {
        let cadence: Option<String> = tx
            .query_row(
                "SELECT vat_period_type FROM companies WHERE id=1",
                [],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        if cadence.is_none() {
            return Err(refused(
                "VAT registration required before reopening a VAT period",
            ));
        }
    }
    let row:Option<(i64,String)>=tx.query_row("SELECT id,status FROM accounting_periods WHERE period_start=?1 AND period_end=?2 AND (kind=?3 OR (?3='vat_period' AND kind='vat_quarter')) ORDER BY CASE kind WHEN 'vat_period' THEN 0 ELSE 1 END,id DESC LIMIT 1",params![input.period_start,input.period_end,kind],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(invalid)?;
    let (id, status) = row.ok_or_else(|| refused("period not found"))?;
    if effective(tx, id, status)? != "closed" {
        return Err(refused("only a closed, unreported period may be reopened"));
    }
    review(tx, packet, actor)?;
    event(
        tx,
        id,
        "period_reopen",
        actor,
        &format!(
            "Reopened {kind} {}..{} — reason: {}",
            input.period_start,
            input.period_end,
            input.reason.trim()
        ),
    )?;
    decision(tx, id, packet, "reopened", actor, Some(input.reason.trim()))?;
    Ok(
        json!({"id":id,"periodStart":input.period_start,"periodEnd":input.period_end,"kind":kind,"effectiveStatus":"open","reopenedBy":format!("user:{actor}"),"reason":input.reason.trim()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reviewed_close_rejects_stale_data_locks_posting_and_reopens_without_rewriting_history() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Period Company")
            .unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("UPDATE companies SET cvr='87654321',vat_period_type='quarter' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('B','Synthetic bank','asset','debit'),('R','Synthetic receivables','asset','debit'),('C','Synthetic creditors','liability','credit'),('E1','Synthetic equity one','equity','credit'),('E2','Synthetic equity two','equity','credit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('bank','B',1,'synthetic'),('debtors','R',1,'synthetic'),('creditors','C',1,'synthetic');").unwrap();
        let packet = || compute(&tx, "2026-07-01", "2026-09-30", Ok(vec![])).unwrap();
        let first = packet();
        assert_eq!(first["blockers"], 0, "{first}");
        let reviewed = review(&tx, &first, "synthetic").unwrap();
        let input = |review: &Value| {
            serde_json::from_value::<Close>(json!({"periodStart":"2026-07-01","periodEnd":"2026-09-30","kind":"vat_period","packetHash":review["packet"]["hash"],"reviewId":review["id"],"confirm":true})).unwrap()
        };
        let post = |date: &str| {
            let request:rentemester_engine::manual_journal::ManualJournal=serde_json::from_value(json!({"transactionDate":date,"text":"Synthetic equity transfer","currency":"DKK","lines":[{"accountNo":"E1","debitAmount":"10.00","creditAmount":"0.00"},{"accountNo":"E2","debitAmount":"0.00","creditAmount":"10.00"}]})).unwrap();
            crate::journal_post::post(
                &tx,
                &request,
                &crate::journal_preview::manual_journal_rule_version().unwrap(),
                Date::parse("2026-10-18").unwrap(),
                crate::journal_post::Actor {
                    subject: "user:synthetic",
                    program: "rentemester-rust",
                },
            )
            .unwrap()
        };
        assert_eq!(post("2026-08-01")["ok"], true);
        let current = packet();
        assert_eq!(current["blockers"], 0, "{current}");
        assert_ne!(first["hash"], current["hash"]);
        assert!(close(&tx, &input(&reviewed), &current, "synthetic", "2026-10-18").is_err());
        assert_eq!(list(&tx, "synthetic").unwrap()["periods"], json!([]));
        let approved = review(&tx, &current, "synthetic").unwrap();
        let closed = close(&tx, &input(&approved), &current, "synthetic", "2026-10-18").unwrap();
        assert_eq!(closed["status"], "closed");
        assert_eq!(post("2026-08-02")["ok"], false);
        assert!(close(&tx, &input(&approved), &packet(), "synthetic", "2026-10-18").is_err());
        let reopen_input = |reason: &str| {
            serde_json::from_value::<Reopen>(json!({"periodStart":"2026-07-01","periodEnd":"2026-09-30","reason":reason,"confirm":true})).unwrap()
        };
        assert!(reopen(&tx, &reopen_input(" "), &packet(), "synthetic").is_err());
        reopen(
            &tx,
            &reopen_input("Synthetic additional evidence"),
            &packet(),
            "synthetic",
        )
        .unwrap();
        let rows = list(&tx, "synthetic").unwrap();
        assert_eq!(rows["periods"][0]["rowStatus"], "closed");
        assert_eq!(rows["periods"][0]["effectiveStatus"], "open");
        assert_eq!(post("2026-08-02")["ok"], true);
        let current = packet();
        assert_eq!(current["blockers"], 0, "{current}");
        let approved = review(&tx, &current, "synthetic").unwrap();
        close(&tx, &input(&approved), &current, "synthetic", "2026-10-18").unwrap();
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM period_close_decisions", [], |r| r
                .get(0))
                .unwrap(),
            3
        );
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM accounting_periods", [], |r| r.get(0))
                .unwrap(),
            1
        );
        let mut future = input(&approved);
        future.period_start = "2026-10-01".into();
        future.period_end = "2026-12-31".into();
        assert!(close(&tx, &future, &current, "synthetic", "2026-10-18").is_err());
    }
}
