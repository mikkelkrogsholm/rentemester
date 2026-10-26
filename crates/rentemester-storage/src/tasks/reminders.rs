//! Reminder writes/slots inside the admitted control writer. Recipients are checked live by caller.
use super::*;
use rentemester_engine::tasks::reminders::{self as domain, Notification};
pub fn configure_in_tx<A, R>(
    tx: &Transaction<'_>,
    id: &str,
    raw: &Value,
    ctx: &Context<'_>,
    mut authorize: A,
    mut recipient: R,
) -> Result<Task, Error>
where
    A: FnMut(&Task) -> Result<(), Error>,
    R: FnMut(&str, &Task) -> Result<(), Error>,
{
    super::series::fence(tx, ctx)?;
    tasks::id(&json!(id))?;
    let normalized = domain::normalize(raw)?;
    let current =
        super::read(tx, id)?.ok_or_else(|| Error::new("not_found", "Task unavailable"))?;
    authorize(&current)?;
    let payload = json!({"taskId":id,"reminder":raw});
    if let Some(prior) = super::series::prior(tx, ctx, "reminder-set", &payload)? {
        let historical: Task = serde_json::from_value(prior).map_err(invalid)?;
        authorize(&historical)?;
        return Ok(historical);
    }
    if normalized["enabled"] == true
        || !current.reminders.iter().any(|old| {
            old["reminderId"] == normalized["reminderId"]
                && old["recipientId"] == normalized["recipientId"]
        })
    {
        recipient(normalized["recipientId"].as_str().unwrap(), &current)?;
    }
    let task = domain::configure(
        &current,
        &normalized,
        ctx.expected_version,
        ctx.now,
        &super::boards(tx)?,
    )?;
    tx.execute("INSERT INTO rm_task_events(task_id,version,operation,payload_json,actor,principal,created_at,reason)VALUES(?1,?2,'updated',?3,?4,?5,?6,NULL)",params![task.task_id,i64::try_from(task.version).map_err(invalid)?,tasks::canonical_json(&json!(task)),ctx.actor,ctx.principal,task.updated_at]).map_err(invalid)?;
    super::series::append_receipt(tx, ctx, "reminder-set", &payload, &json!(task))?;
    Ok(task)
}
pub fn visible<A>(
    db: &Connection,
    recipient: &str,
    mut can_read: A,
) -> Result<Vec<Notification>, Error>
where
    A: FnMut(&Task) -> Result<bool, Error>,
{
    let mut q=db.prepare("SELECT payload_json FROM rm_task_notifications WHERE recipient_id=?1 ORDER BY created_at DESC,notification_id").map_err(invalid)?;
    let mut out = vec![];
    for raw in q
        .query_map([recipient], |r| r.get::<_, String>(0))
        .map_err(invalid)?
    {
        let mut n: Notification = serde_json::from_str(&raw.map_err(invalid)?).map_err(invalid)?;
        if n.recipient_id != recipient {
            return Err(invalid("Notification recipient differs"));
        }
        let Some(task) = super::read(db, &n.task_id)? else {
            continue;
        };
        if can_read(&task)? && domain::current(&task, &n)? {
            n.title = task.title.clone();
            out.push(n);
        }
    }
    Ok(out)
}
pub fn deliver<A, D>(
    tx: &Transaction<'_>,
    stamp: &str,
    mut can_read: A,
    mut today: D,
) -> Result<usize, Error>
where
    A: FnMut(&str, &Task) -> Result<bool, Error>,
    D: FnMut(&str) -> Result<String, Error>,
{
    if tx
        .transaction_state(Some(rusqlite::MAIN_DB))
        .map_err(invalid)?
        != rusqlite::TransactionState::Write
    {
        return Err(Error::new(
            "control_fence_required",
            "Notification writer required",
        ));
    }
    tasks::instant(stamp)?;
    let mut count = 0;
    for task in super::list(tx, |_| Ok(true))? {
        for r in &task.reminders {
            let r = domain::normalize(r)?;
            let recipient = r["recipientId"].as_str().unwrap();
            if !can_read(recipient, &task)? {
                continue;
            }
            let date = today(r["timeZone"].as_str().unwrap())?;
            let Some(n) = domain::notification(&task, &r, &date, stamp)? else {
                continue;
            };
            count+=tx.execute("INSERT OR IGNORE INTO rm_task_notifications(notification_id,recipient_id,task_id,reminder_id,slot_key,payload_json,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7)",params![n.notification_id,n.recipient_id,n.task_id,n.reminder_id,n.slot_key,tasks::canonical_json(&json!(n)),n.created_at]).map_err(invalid)?;
        }
    }
    Ok(count)
}
pub fn runtime_status(db: &Connection) -> Result<Value, Error> {
    let (at, error): (Option<String>, Option<String>) = db
        .query_row(
            "SELECT last_tick_at,last_error FROM rm_task_runtime WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(invalid)?;
    if let Some(at) = &at {
        tasks::instant(at)?;
    }
    Ok(json!({"running":false,"lastTickAt":at,"lastError":error}))
}
pub fn claim(tx: &Transaction<'_>, owner: &str, stamp: &str) -> Result<bool, Error> {
    tasks::id(&json!(owner))?;
    let (stamp, millis) = tasks::instant(stamp)?;
    let until = rentemester_engine::company_knowledge::timestamp(
        i64::try_from(millis)
            .map_err(invalid)?
            .checked_add(55_000)
            .ok_or_else(|| invalid("Lease time overflow"))?,
    )?;
    Ok(tx.execute("UPDATE rm_task_runtime SET lease_owner=?1,lease_until=?2 WHERE id=1 AND (lease_until IS NULL OR lease_until<=?3)",params![owner,until,stamp]).map_err(invalid)?==1)
}
pub fn require_lease(tx: &Transaction<'_>, owner: &str, stamp: &str) -> Result<(), Error> {
    let live:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM rm_task_runtime WHERE id=1 AND lease_owner=?1 AND lease_until>?2)",params![owner,stamp],|r|r.get(0)).map_err(invalid)?;
    if live {
        Ok(())
    } else {
        Err(Error::new(
            "task_runtime_lease_lost",
            "Current task runtime lease required",
        ))
    }
}
pub fn finish_lease(
    tx: &Transaction<'_>,
    owner: &str,
    stamp: &str,
    error: bool,
) -> Result<(), Error> {
    tasks::id(&json!(owner))?;
    tasks::instant(stamp)?;
    tx.execute("UPDATE rm_task_runtime SET last_tick_at=?2,last_error=?3,lease_owner=NULL,lease_until=NULL WHERE id=1 AND lease_owner=?1",params![owner,stamp,if error{Some("Påmindelsesruntime kunne ikke gennemføre seneste kørsel. Kontrollér workspace og kilder.")}else{None}]).map_err(invalid)?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaintenanceResult {
    pub scope: tasks::Scope,
    pub includes_workspace: bool,
    pub notification_scope: tasks::Scope,
    pub notifications_include_workspace: bool,
    pub run: MaintenanceSummary,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceSummary {
    pub sync: super::sources::Summary,
    pub materialized: super::series::Materialized,
    pub delivered: usize,
}
