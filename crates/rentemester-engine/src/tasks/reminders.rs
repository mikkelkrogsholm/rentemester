//! Explicit reminder policy and canonical durable slots. No clock, recipient or authority grants.
use super::*;
use serde_json::json;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Notification {
    pub notification_id: String,
    pub recipient_id: String,
    pub task_id: String,
    pub reminder_id: String,
    pub slot_key: String,
    pub created_at: String,
    pub title: String,
}
pub fn normalize(v: &Value) -> Result<Value, Error> {
    input::object(
        v,
        &[
            "reminderId",
            "recipientId",
            "enabled",
            "kind",
            "frequency",
            "timeZone",
            "daysBefore",
            "followUpDate",
        ],
    )?;
    let kind = input::choice(&v["kind"], &["deadline", "follow_up"])?;
    let zone = v
        .get("timeZone")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Europe/Copenhagen");
    if zone.len() > 100 || zone.parse::<chrono_tz::Tz>().is_err() {
        return Err(invalid("Invalid reminder time zone"));
    }
    let mut out = json!({"reminderId":id(&v["reminderId"] )?,"recipientId":id(&v["recipientId"] )?,"enabled":v["enabled"].as_bool().ok_or_else(||invalid("Invalid reminder enabled"))?,"kind":kind,"frequency":input::choice(&v["frequency"],&["once","daily"] )?,"timeZone":zone});
    if let Some(n) = v.get("daysBefore") {
        out["daysBefore"] = json!(
            n.as_u64()
                .filter(|n| *n <= 365)
                .ok_or_else(|| invalid("Invalid reminder daysBefore"))?
        );
    } else if kind == "deadline" {
        out["daysBefore"] = json!(0);
    }
    if let Some(d) = v.get("followUpDate") {
        out["followUpDate"] = json!(date(d)?);
    }
    if kind == "follow_up" && out.get("followUpDate").is_none() {
        return Err(invalid("Follow-up date required"));
    }
    Ok(out)
}
pub fn configure(
    prior: &Task,
    raw: &Value,
    expected: Option<u64>,
    now: &str,
    boards: &[Board],
) -> Result<Task, Error> {
    let reminder = normalize(raw)?;
    if reminder["enabled"] == true && reminder["kind"] == "deadline" && prior.deadline.is_null() {
        return Err(invalid("Task deadline required"));
    }
    let mut all = prior
        .reminders
        .iter()
        .filter(|r| r["reminderId"] != reminder["reminderId"])
        .cloned()
        .collect::<Vec<_>>();
    all.push(reminder);
    update(prior, &json!({"reminders":all}), expected, now, boards)
}
pub fn schedule(task: &Task, reminder: &Value) -> Result<Option<(String, String)>, Error> {
    let r = normalize(reminder)?;
    if r["enabled"] != true || task.status == "done" || task.relevance == "not_relevant" {
        return Ok(None);
    }
    let due = if r["kind"] == "deadline" {
        let Some(d) = task.deadline["date"].as_str() else {
            return Ok(None);
        };
        let day = crate::vat_period::date(d)?
            - time::Duration::days(r["daysBefore"].as_i64().unwrap_or(0));
        day.to_string()
    } else {
        r["followUpDate"].as_str().unwrap().to_owned()
    };
    date(&json!(due))?;
    let signature=payload_hash(&json!({"reminder":r,"due":due,"deadline":if r["kind"]=="deadline"{task.deadline.clone()}else{Value::Null}}))?[..24].to_owned();
    Ok(Some((due, signature)))
}
pub fn notification(
    task: &Task,
    raw: &Value,
    today: &str,
    now: &str,
) -> Result<Option<Notification>, Error> {
    date(&json!(today))?;
    let r = normalize(raw)?;
    let Some((due, signature)) = schedule(task, &r)? else {
        return Ok(None);
    };
    if today < due.as_str() {
        return Ok(None);
    }
    let slot = format!(
        "{signature}:{}",
        if r["frequency"] == "once" {
            "once"
        } else {
            today
        }
    );
    let recipient = r["recipientId"].as_str().unwrap();
    let reminder = r["reminderId"].as_str().unwrap();
    let id = format!(
        "notification-{}",
        &payload_hash(&json!([recipient, task.task_id, reminder, slot]))?[..32]
    );
    Ok(Some(Notification {
        notification_id: id,
        recipient_id: recipient.into(),
        task_id: task.task_id.clone(),
        reminder_id: reminder.into(),
        slot_key: slot,
        created_at: instant(now)?.0,
        title: task.title.clone(),
    }))
}
pub fn current(task: &Task, n: &Notification) -> Result<bool, Error> {
    let Some(r) = task
        .reminders
        .iter()
        .find(|r| r["reminderId"] == n.reminder_id && r["recipientId"] == n.recipient_id)
    else {
        return Ok(false);
    };
    Ok(schedule(task, r)?.is_some_and(|(_, signature)| n.slot_key.starts_with(&(signature + ":"))))
}

#[cfg(test)]
mod tests;

/// Pure IANA calendar conversion shared by live delivery and source-free slot verification.
pub fn recipient_date(now_ms: i64, zone: &str) -> Result<String, Error> {
    use chrono::Datelike;
    let tz = zone
        .parse::<chrono_tz::Tz>()
        .map_err(|_| invalid("Invalid reminder time zone"))?;
    let d = chrono::DateTime::from_timestamp_millis(now_ms)
        .ok_or_else(|| invalid("Invalid reminder timestamp"))?
        .with_timezone(&tz)
        .date_naive();
    if !(0..=9999).contains(&d.year()) {
        return Err(invalid("Reminder date outside supported range"));
    }
    Ok(d.format("%Y-%m-%d").to_string())
}
