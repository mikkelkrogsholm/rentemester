//! Codec5 authenticates usable reminder/run receipts and canonical business notification slots.
use super::*;
use crate::tasks::reminders::MaintenanceResult;
use rentemester_engine::tasks::reminders as engine;
pub(super) fn needed(db: &Connection) -> Result<bool, Error> {
    db.query_row("SELECT EXISTS(SELECT 1 FROM rm_task_receipts WHERE operation IN ('reminder-set','runtime-run')) OR EXISTS(SELECT 1 FROM rm_task_notifications)",[],|r|r.get(0)).map_err(sql)
}
fn selection(scope: &domain::Scope) -> Result<&Vec<String>, Error> {
    let domain::Scope::Workspace { company_slugs } = scope else {
        return Err(refused("Runtime result scope differs"));
    };
    if company_slugs.len() > 100000
        || company_slugs.iter().collect::<BTreeSet<_>>().len() != company_slugs.len()
    {
        return Err(refused("Runtime selection differs"));
    }
    for slug in company_slugs {
        domain::scope(&serde_json::json!({"kind":"company","companySlug":slug}))?;
    }
    Ok(company_slugs)
}
pub(super) fn receipt(
    db: &Connection,
    principal: &str,
    operation: &str,
    raw: &str,
    at: &str,
) -> Result<bool, Error> {
    match operation {
        "reminder-set" => {
            let task = task_snapshot(raw)?;
            let prior: Option<String> = db
                .query_row(
                    "SELECT payload_json FROM rm_task_events WHERE task_id=?1 AND version=?2",
                    params![task.task_id, i64::try_from(task.version).map_err(sql)? - 1],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql)?;
            let before =
                task_snapshot(&prior.ok_or_else(|| refused("Reminder receipt lacks prior task"))?)?;
            let exact:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM rm_task_events WHERE task_id=?1 AND version=?2 AND operation='updated' AND principal=?3 AND payload_json=?4 AND created_at=?5)",params![task.task_id,i64::try_from(task.version).map_err(sql)?,principal,raw,at],|r|r.get(0)).map_err(sql)?;
            if !exact {
                return Err(refused(
                    "Reminder receipt differs from exact principal/event snapshot",
                ));
            }
            let mut expected = before.clone();
            expected.reminders = task.reminders.clone();
            expected.version = task.version;
            expected.updated_at = task.updated_at.clone();
            if expected != task {
                return Err(refused("Reminder receipt changes unrelated fields"));
            }
            let old = before
                .reminders
                .iter()
                .map(|r| (r["reminderId"].as_str().unwrap(), r))
                .collect::<std::collections::BTreeMap<_, _>>();
            let new = task
                .reminders
                .iter()
                .map(|r| (r["reminderId"].as_str().unwrap(), r))
                .collect::<std::collections::BTreeMap<_, _>>();
            if old.keys().any(|id| !new.contains_key(id))
                || new.len() > old.len() + 1
                || new
                    .iter()
                    .filter(|(id, r)| old.get(*id).is_none_or(|v| *v != **r))
                    .count()
                    > 1
            {
                return Err(refused("Reminder receipt changes more than one schedule"));
            }
            Ok(true)
        }
        "runtime-run" => {
            let result: MaintenanceResult = serde_json::from_str(raw).map_err(sql)?;
            let companies = selection(&result.scope)?;
            let notification_companies = selection(&result.notification_scope)?;
            if !companies.iter().all(|s| notification_companies.contains(s))
                || result.includes_workspace && !result.notifications_include_workspace
                || result.run.delivered > 100000
            {
                return Err(refused("Runtime summary scope/count differs"));
            }
            let notification_count: i64 = db
                .query_row("SELECT count(*) FROM rm_task_notifications", [], |r| {
                    r.get(0)
                })
                .map_err(sql)?;
            if result.run.delivered > usize::try_from(notification_count).map_err(sql)? {
                return Err(refused("Delivered count lacks notification slots"));
            }
            sources::receipt(
                db,
                "sources-sync",
                &domain::canonical_json(
                    &serde_json::json!({"scope":result.scope,"includesWorkspace":false,"sync":result.run.sync}),
                ),
                at,
            )?;
            series::receipt(
                db,
                principal,
                "series-materialize",
                &domain::canonical_json(
                    &serde_json::json!({"scope":result.scope,"includesWorkspace":result.includes_workspace,"materialized":result.run.materialized}),
                ),
                at,
            )?;
            Ok(true)
        }
        _ => Ok(false),
    }
}
pub(super) fn notifications(db: &Connection) -> Result<(), Error> {
    let mut q=db.prepare("SELECT notification_id,recipient_id,task_id,reminder_id,slot_key,payload_json,created_at FROM rm_task_notifications").map_err(sql)?;
    for row in q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })
        .map_err(sql)?
    {
        let (id, recipient, task, reminder, slot, raw, at) = row.map_err(sql)?;
        let n: engine::Notification = serde_json::from_str(&raw).map_err(sql)?;
        domain::instant(&at)?;
        if n.notification_id != id
            || n.recipient_id != recipient
            || n.task_id != task
            || n.reminder_id != reminder
            || n.slot_key != slot
            || n.created_at != at
        {
            return Err(refused("Notification row/snapshot differs"));
        }
        let expected = format!(
            "notification-{}",
            &domain::payload_hash(&serde_json::json!([recipient, task, reminder, slot]))?[..32]
        );
        if expected != id {
            return Err(refused("Notification slot identity differs"));
        }
        let mut found = false;
        let mut events = db
            .prepare("SELECT payload_json FROM rm_task_events WHERE task_id=?1 ORDER BY version")
            .map_err(sql)?;
        for raw in events
            .query_map([&task], |r| r.get::<_, String>(0))
            .map_err(sql)?
        {
            let t = task_snapshot(&raw.map_err(sql)?)?;
            if t.title != n.title {
                continue;
            }
            let Some(r) = t
                .reminders
                .iter()
                .find(|r| r["reminderId"] == reminder && r["recipientId"] == recipient)
            else {
                continue;
            };
            let milliseconds = i64::try_from(domain::instant(&n.created_at)?.1).map_err(sql)?;
            let today = engine::recipient_date(
                milliseconds,
                r["timeZone"]
                    .as_str()
                    .ok_or_else(|| refused("Notification time zone missing"))?,
            )?;
            if engine::notification(&t, r, &today, &n.created_at)?
                .is_some_and(|expected| expected == n)
            {
                found = true;
                break;
            }
        }
        if !found {
            return Err(refused(
                "Notification lacks exact historical reminder signature/title",
            ));
        }
    }
    Ok(())
}
use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests;
