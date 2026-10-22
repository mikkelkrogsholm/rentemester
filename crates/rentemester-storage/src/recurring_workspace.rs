//! Bounded gap selection and append-only delivery linkage for the explicit workspace runner.
use crate::invalid;
use rentemester_engine::Error;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::{Value, json};
pub mod validation;
pub use validation::validate_history;
fn refused(message: &str) -> Error {
    Error::new("recurring_workspace_refused", message)
}
pub fn channel(db: &Connection, id: i64) -> Result<String, Error> {
    db.query_row("SELECT t.delivery_channel FROM recurring_invoice_generations g JOIN recurring_invoice_templates t ON t.id=g.template_id WHERE g.id=?1",[id],|r|r.get(0)).optional().map_err(invalid)?.ok_or_else(||refused("recurring generation unavailable"))
}
pub fn document_id(db: &Connection, id: i64) -> Result<i64, Error> {
    db.query_row(
        "SELECT document_id FROM recurring_invoice_generations WHERE id=?1",
        [id],
        |r| r.get(0),
    )
    .map_err(invalid)
}
pub fn plan(tx: &Transaction<'_>, as_of: &str, max: usize) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !(1..=5000).contains(&max) {
        return Err(refused(
            "maxGenerations must be an integer between1 and5000",
        ));
    }
    rentemester_engine::fiscal_year::Date::parse(as_of)?;
    validate_history(tx)?;
    let mut q=tx.prepare("SELECT id,delivery_channel FROM recurring_invoice_templates WHERE active=1 ORDER BY id").map_err(invalid)?;
    let templates = q
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let mut missing = vec![];
    let mut deliveries = vec![];
    let mut initial = 0u64;
    for (id, channel) in templates {
        let Some(latest) = crate::recurring_invoices::scheduled_latest(tx, id, as_of)? else {
            continue;
        };
        let count:u32=tx.query_row("SELECT count(*) FROM recurring_invoice_generations WHERE template_id=?1 AND period_index<=?2",params![id,latest],|r|r.get(0)).map_err(invalid)?;
        initial = initial
            .checked_add(u64::from(latest) + 1 - u64::from(count))
            .ok_or_else(|| refused("generation count exceeds range"))?;
        let mut cursor = 0u32;
        while missing.len() < max && cursor <= latest {
            let gap:Option<u32>=tx.query_row("SELECT MIN(period_index) FROM (SELECT ?1 period_index WHERE NOT EXISTS(SELECT 1 FROM recurring_invoice_generations WHERE template_id=?2 AND period_index=?1) UNION ALL SELECT g.period_index+1 FROM recurring_invoice_generations g WHERE g.template_id=?2 AND g.period_index>=?1 AND g.period_index<?3 AND NOT EXISTS(SELECT 1 FROM recurring_invoice_generations n WHERE n.template_id=g.template_id AND n.period_index=g.period_index+1)) WHERE period_index<=?3",params![cursor,id,latest],|r|r.get(0)).map_err(invalid)?;
            let Some(index) = gap else { break };
            missing.push(json!({"templateId":id,"periodIndex":index}));
            cursor = index + 1;
        }
        if channel == "manual" || deliveries.len() >= max {
            continue;
        }
        let mut q=tx.prepare("SELECT g.id,g.document_id,(SELECT e.event_type FROM recurring_invoice_delivery_events e WHERE e.generation_id=g.id ORDER BY e.id DESC LIMIT 1),(SELECT e.provider_id FROM recurring_invoice_delivery_events e WHERE e.generation_id=g.id ORDER BY e.id DESC LIMIT 1),(SELECT e.message FROM recurring_invoice_delivery_events e WHERE e.generation_id=g.id ORDER BY e.id DESC LIMIT 1) FROM recurring_invoice_generations g WHERE g.template_id=?1 AND g.period_index<=?2 AND COALESCE((SELECT e.event_type FROM recurring_invoice_delivery_events e WHERE e.generation_id=g.id ORDER BY e.id DESC LIMIT 1),'')!='acknowledged' ORDER BY g.period_index LIMIT ?3").map_err(invalid)?;
        let rows=q.query_map(params![id,latest,i64::try_from(max-deliveries.len()).map_err(|_|refused("delivery cap exceeds range"))?],|r|Ok(json!({"generationId":r.get::<_,i64>(0)?,"templateId":id,"documentId":r.get::<_,i64>(1)?,"channel":channel,"latestEvent":r.get::<_,Option<String>>(2)?,"providerId":r.get::<_,Option<String>>(3)?,"message":r.get::<_,Option<String>>(4)?.map(|s|s.strip_prefix("rentemester-recurring/1: ").unwrap_or(&s).to_owned())}))).map_err(invalid)?.collect::<Result<Vec<_>,_>>().map_err(invalid)?;
        for mut row in rows {
            let (number, recipient) = email_identity(
                tx,
                row["documentId"]
                    .as_i64()
                    .ok_or_else(|| refused("generation document identity missing"))?,
            )?;
            row["invoiceNumber"] = json!(number);
            row["emailRecipient"] = json!(recipient);
            let receipt = crate::public_einvoice::get(
                tx,
                row["documentId"]
                    .as_i64()
                    .ok_or_else(|| refused("generation document identity missing"))?,
            )?;
            row["canObserve"] = json!(
                row["latestEvent"] == "accepted_pending"
                    && row["providerId"].is_string()
                    && receipt.as_ref().is_some_and(
                        |v| v["legacy"] == false && v["transmissionId"] == row["providerId"]
                    )
            );
            deliveries.push(row);
        }
    }
    Ok(json!({"initialMissing":initial,"missing":missing,"deliveries":deliveries}))
}
fn email_identity(db: &Connection, id: i64) -> Result<(String, Option<String>), Error> {
    let (number,name,payload):(String,Option<String>,String)=db.query_row("SELECT invoice_no,recipient_name,payload_json FROM documents WHERE id=?1 AND document_type='issued_invoice'",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(invalid)?;
    let payload: Value = serde_json::from_str(&payload).map_err(invalid)?;
    let name = name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| payload["buyer"]["name"].as_str().map(str::trim))
        .unwrap_or("");
    let email:Option<String>=db.query_row("SELECT email FROM customers WHERE name=?1 AND archived=0 AND email IS NOT NULL AND TRIM(email)!='' ORDER BY id DESC LIMIT 1",[name],|r|r.get(0)).optional().map_err(invalid)?;
    Ok((
        number,
        email.and_then(|s| crate::invoice_email::recipient(&s).ok()),
    ))
}
fn latest(db: &Connection, id: i64) -> Result<Option<(i64, String, Option<String>)>, Error> {
    db.query_row("SELECT id,event_type,provider_id FROM recurring_invoice_delivery_events WHERE generation_id=?1 ORDER BY id DESC LIMIT 1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(invalid)
}
fn typed(db: &Connection, event: i64) -> Result<bool, Error> {
    db.query_row("SELECT count(*)=1 FROM audit_log WHERE event_type='original_recurring_delivery_event' AND entity_type='recurring_invoice_delivery_event' AND entity_id=?1",[event.to_string()],|r|r.get(0)).map_err(invalid)
}
fn append(
    tx: &Transaction<'_>,
    id: i64,
    event: &str,
    provider: Option<&str>,
    message: &str,
    actor: &str,
) -> Result<(), Error> {
    let channel = channel(tx, id)?;
    const PREFIX: &str = "rentemester-recurring/1: ";
    let message = format!(
        "{PREFIX}{}",
        message
            .chars()
            .scan(0usize, |size, c| {
                *size += c.len_utf8();
                Some((*size, c))
            })
            .take_while(|(size, _)| *size <= 240 - PREFIX.len())
            .map(|(_, c)| c)
            .collect::<String>()
    );
    let (document,sha):(i64,String)=tx.query_row("SELECT d.id,d.sha256_hash FROM recurring_invoice_generations g JOIN documents d ON d.id=g.document_id AND d.document_type='issued_invoice' WHERE g.id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(invalid)?;
    let event_id:i64=tx.query_row("INSERT INTO recurring_invoice_delivery_events(generation_id,channel,event_type,provider_id,message) VALUES(?1,?2,?3,?4,?5) RETURNING id",params![id,channel,event,provider,message],|r|r.get(0)).map_err(invalid)?;
    let actor = format!("user:{actor}");
    let proof = json!({"version":1,"generationId":id,"eventId":event_id,"documentId":document,"documentSha256":sha,"channel":channel,"eventType":event,"providerId":provider,"eventMessage":message,"actor":actor,"mode":if channel=="email"{"email_dry_run"}else{"native_public"}});
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('original_recurring_delivery_event','recurring_invoice_delivery_event',?1,?2,?3)",params![event_id.to_string(),proof.to_string(),actor]).map_err(invalid)?;
    validate_history(tx)
}
pub fn reserve(tx: &Transaction<'_>, id: i64, actor: &str) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    validate_history(tx)?;
    if let Some((event, kind, _)) = latest(tx, id)?
        && (kind != "preflight_failed" || !typed(tx, event)?)
    {
        return Err(refused(
            "delivery requires reconciliation; no new attempt permitted",
        ));
    }
    if !matches!(channel(tx, id)?.as_str(), "email" | "digisense") {
        return Err(refused("manual generation has no delivery"));
    }
    append(tx, id, "attempted", None, "", actor)?;
    Ok(json!({"reserved":true}))
}
pub fn event(
    tx: &Transaction<'_>,
    id: i64,
    event: &str,
    provider: Option<&str>,
    message: &str,
    actor: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    validate_history(tx)?;
    if message.len() > 240
        || message.chars().any(char::is_control)
        || message.contains("https://")
        || message.contains("http://")
        || provider
            .is_some_and(|s| s.is_empty() || s.len() > 1024 || s.chars().any(char::is_control))
    {
        return Err(refused("bounded sanitized delivery evidence required"));
    }
    let previous = latest(tx, id)?;
    if event == "preflight_failed" {
        if previous
            .as_ref()
            .is_some_and(|(_, k, _)| k != "preflight_failed")
            || previous
                .as_ref()
                .is_some_and(|(id, _, _)| !typed(tx, *id).unwrap_or(false))
        {
            return Err(refused("preflight cannot reset attempted delivery"));
        }
    } else if !matches!(
        event,
        "acknowledged" | "accepted_pending" | "terminal_failed" | "uncertain"
    ) || previous
        .as_ref()
        .is_none_or(|(_, k, _)| !matches!(k.as_str(), "attempted" | "accepted_pending"))
    {
        return Err(refused("delivery event is not a legal continuation"));
    }
    let channel = channel(tx, id)?;
    if channel == "email" {
        if provider.is_some()
            || !matches!(event, "acknowledged" | "uncertain" | "preflight_failed")
            || event == "acknowledged"
                && (!message.contains("dry-run") || !message.contains("no actual delivery"))
        {
            return Err(refused(
                "email runner only records truthful dry-run completion",
            ));
        }
    } else if channel == "digisense"
        && matches!(
            event,
            "acknowledged" | "accepted_pending" | "terminal_failed"
        )
    {
        let receipt = crate::public_einvoice::get(tx, document_id(tx, id)?)?
            .ok_or_else(|| refused("native public receipt required"))?;
        let expected = match event {
            "acknowledged" => "acknowledged",
            "accepted_pending" => "prepared",
            _ => "failed",
        };
        if receipt["legacy"] != false
            || receipt["status"] != expected
            || provider.is_none()
            || receipt["transmissionId"].as_str() != provider
        {
            return Err(refused(
                "public outcome must match admitted native provider receipt",
            ));
        }
    }
    if let Some((_, kind, Some(old))) = &previous
        && kind == "accepted_pending"
        && provider != Some(old.as_str())
    {
        return Err(refused("accepted provider identity cannot change"));
    }
    append(tx, id, event, provider, message, actor)?;
    Ok(json!({"recorded":true}))
}
