//! Recurring history is evidence, never fresh dispatch authority after recovery.
use rentemester_engine::{Error, calendar::valid_iso_date};
use rusqlite::{Connection, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const MESSAGE_PREFIX: &str = "rentemester-recurring/1: ";
const AUDIT: &str = "original_recurring_delivery_event";
fn refused() -> Error {
    Error::new(
        "recurring_history_invalid",
        "Recurring source or delivery evidence is invalid; reconcile before dispatch",
    )
}
fn sql(_: rusqlite::Error) -> Error {
    refused()
}
fn ensure(value: bool) -> Result<(), Error> {
    if value { Ok(()) } else { Err(refused()) }
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn sha(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Proof {
    version: u32,
    generation_id: i64,
    event_id: i64,
    document_id: i64,
    document_sha256: String,
    channel: String,
    event_type: String,
    provider_id: Option<String>,
    event_message: String,
    actor: String,
    mode: String,
}
struct Event {
    id: i64,
    generation: i64,
    document: i64,
    channel: String,
    kind: String,
    provider: Option<String>,
    message: String,
    document_sha: String,
    raw_payload: String,
}
fn public_claim(db: &Connection, e: &Event) -> Result<(), Error> {
    if !matches!(
        e.kind.as_str(),
        "acknowledged" | "accepted_pending" | "terminal_failed"
    ) {
        return Ok(());
    }
    let provider = e
        .provider
        .as_deref()
        .filter(|s| text(s, 512))
        .ok_or_else(refused)?;
    let (_, source, accepted) =
        crate::public_einvoice::accepted(db, e.document)?.ok_or_else(refused)?;
    ensure(accepted == provider && source.json_sha256 == e.document_sha)?;
    let row = crate::public_einvoice::get(db, e.document)?.ok_or_else(refused)?;
    let history = row["history"].as_array().ok_or_else(refused)?;
    ensure(history.iter().any(|h| {
        h["documentId"] == provider
            && match e.kind.as_str() {
                "acknowledged" => h["status"] == "delivered",
                "accepted_pending" => {
                    h["status"] == "queued-for-delivery"
                        || h["status"] == "temporary-upstream-error"
                }
                "terminal_failed" => matches!(
                    h["status"].as_str(),
                    Some("document-not-valid" | "unable-to-deliver" | "unknown-server-error")
                ),
                _ => false,
            }
    }))
}
/// Validate source links and every typed event. Unsigned legacy events stay historical;
/// they acquire no new provenance merely because their complete SQLite image is signed.
pub fn validate_history(db: &Connection) -> Result<(), Error> {
    let mut q = db.prepare("SELECT id,interval,interval_count,delivery_channel,first_issue_date,next_issue_date,payment_terms_days,delivery_period_mode,payload_json FROM recurring_invoice_templates ORDER BY id").map_err(sql)?;
    let templates = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u32>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, u32>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    for (id, interval, count, channel, first, next, terms, mode, raw) in templates {
        ensure(
            id > 0
                && ["weekly", "monthly", "quarterly", "yearly"].contains(&interval.as_str())
                && (1..=120).contains(&count)
                && ["manual", "email", "digisense"].contains(&channel.as_str())
                && valid_iso_date(&first)
                && valid_iso_date(&next)
                && next >= first
                && terms <= 365
                && ["none", "issue_month", "interval_window"].contains(&mode.as_str()),
        )?;
        let mut payload: Value = serde_json::from_str(&raw).map_err(|_| refused())?;
        ensure(payload.is_object())?;
        payload["issueDate"] = json!(first);
        if payload.get("invoiceType").is_some() {
            ensure(rentemester_engine::invoice_validation::validate(&payload)["ok"] == true)?;
        } else {
            let input = serde_json::from_value(payload).map_err(|_| refused())?;
            rentemester_engine::sales_invoice::compute(&input)?;
        }
    }
    let orphan:i64 = db.query_row("SELECT count(*) FROM recurring_invoice_generations g LEFT JOIN recurring_invoice_templates t ON t.id=g.template_id LEFT JOIN documents d ON d.id=g.document_id WHERE t.id IS NULL OR d.id IS NULL OR d.document_type!='issued_invoice'",[],|r|r.get(0)).map_err(sql)?;
    ensure(orphan == 0)?;
    let mut q = db.prepare("SELECT g.template_id,g.period_index,g.document_id,g.invoice_number,g.issue_date,d.invoice_no,d.invoice_date,d.payload_json FROM recurring_invoice_generations g JOIN documents d ON d.id=g.document_id ORDER BY g.id").map_err(sql)?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, String>(7)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    for (template, index, document, number, issue, stored_number, stored_issue, raw) in rows {
        let payload: Value = serde_json::from_str(&raw).map_err(|_| refused())?;
        let claimed:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM recurring_invoice_generation_claims WHERE template_id=?1 AND period_index=?2)",params![template,index],|r|r.get(0)).map_err(sql)?;
        ensure(
            claimed
                && document > 0
                && stored_number.as_deref() == Some(number.as_str())
                && stored_issue.as_deref() == Some(issue.as_str())
                && payload["invoiceNumber"] == number
                && payload["issueDate"] == issue
                && crate::recurring_invoices::scheduled_date(db, template, index)? == issue,
        )?;
        if let Some(value) = payload.get("recurringTemplateId") {
            ensure(value == template && payload["recurringPeriodIndex"] == index)?;
        }
    }
    let orphan:i64=db.query_row("SELECT count(*) FROM recurring_invoice_delivery_events e LEFT JOIN recurring_invoice_generations g ON g.id=e.generation_id WHERE g.id IS NULL",[],|r|r.get(0)).map_err(sql)?;
    ensure(orphan == 0)?;
    let mut q=db.prepare("SELECT e.id,e.generation_id,g.document_id,e.channel,e.event_type,e.provider_id,e.message,d.sha256_hash,d.payload_json FROM recurring_invoice_delivery_events e JOIN recurring_invoice_generations g ON g.id=e.generation_id JOIN documents d ON d.id=g.document_id ORDER BY e.id").map_err(sql)?;
    let events = q
        .query_map([], |r| {
            Ok(Event {
                id: r.get(0)?,
                generation: r.get(1)?,
                document: r.get(2)?,
                channel: r.get(3)?,
                kind: r.get(4)?,
                provider: r.get(5)?,
                message: r.get(6)?,
                document_sha: r.get(7)?,
                raw_payload: r.get(8)?,
            })
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let mut typed_count = 0i64;
    let mut state = BTreeMap::<i64, (bool, String, Option<String>, bool)>::new();
    for e in events {
        let channel:String=db.query_row("SELECT t.delivery_channel FROM recurring_invoice_generations g JOIN recurring_invoice_templates t ON t.id=g.template_id WHERE g.id=?1",[e.generation],|r|r.get(0)).map_err(sql)?;
        ensure(
            channel == e.channel
                && ["email", "digisense"].contains(&e.channel.as_str())
                && [
                    "attempted",
                    "acknowledged",
                    "accepted_pending",
                    "terminal_failed",
                    "uncertain",
                    "preflight_failed",
                ]
                .contains(&e.kind.as_str())
                && e.provider.as_deref().is_none_or(|s| text(s, 512)),
        )?;
        let proofs = db.prepare("SELECT entity_type,message,actor FROM audit_log WHERE event_type=?1 AND entity_id=?2 ORDER BY id").map_err(sql)?.query_map(params![AUDIT,e.id.to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?;
        if !e.message.starts_with(MESSAGE_PREFIX) {
            ensure(proofs.is_empty())?;
            let attempted = state.get(&e.generation).is_some_and(|p| p.0) || e.kind == "attempted";
            state.insert(e.generation, (attempted, e.kind, e.provider, true));
            continue;
        }
        ensure(proofs.len() == 1)?;
        let (entity, raw, actor) = &proofs[0];
        let value: Value = serde_json::from_str(raw).map_err(|_| refused())?;
        let p: Proof = serde_json::from_value(value.clone()).map_err(|_| refused())?;
        let expected = json!({"version":p.version,"generationId":p.generation_id,"eventId":p.event_id,"documentId":p.document_id,"documentSha256":p.document_sha256,"channel":p.channel,"eventType":p.event_type,"providerId":p.provider_id,"eventMessage":p.event_message,"actor":p.actor,"mode":p.mode});
        ensure(
            value == expected
                && actor
                    .strip_prefix("user:")
                    .or_else(|| actor.strip_prefix("service_account:"))
                    .is_some_and(|s| text(s, 480)),
        )?;
        ensure(
            serde_json::to_vec(&value).map_err(|_| refused())? == raw.as_bytes()
                && entity == "recurring_invoice_delivery_event"
                && p.version == 1
                && p.event_id == e.id
                && p.generation_id == e.generation
                && p.document_id == e.document
                && p.document_sha256 == e.document_sha
                && p.channel == e.channel
                && p.event_type == e.kind
                && p.provider_id == e.provider
                && p.event_message == e.message
                && p.actor == *actor
                && text(actor, 512)
                && (actor.starts_with("user:") || actor.starts_with("service_account:"))
                && sha(e.raw_payload.as_bytes()) == e.document_sha,
        )?;
        ensure(
            p.mode
                == if e.channel == "email" {
                    "email_dry_run"
                } else {
                    "native_public"
                },
        )?;
        let previous = state.get(&e.generation);
        let attempted = previous.is_some_and(|p| p.0);
        match e.kind.as_str() {
            "preflight_failed" => ensure(!attempted && e.provider.is_none())?,
            "attempted" => ensure(
                !attempted
                    && e.provider.is_none()
                    && previous.is_none_or(|p| p.1 == "preflight_failed" && !p.3),
            )?,
            _ => ensure(
                attempted
                    && previous
                        .is_some_and(|p| matches!(p.1.as_str(), "attempted" | "accepted_pending")),
            )?,
        }
        if let Some(prior) = previous.and_then(|p| p.2.as_ref()) {
            ensure(e.provider.as_ref() == Some(prior))?;
        }
        if e.channel == "email" {
            ensure(
                e.provider.is_none()
                    && matches!(
                        e.kind.as_str(),
                        "attempted" | "acknowledged" | "uncertain" | "preflight_failed"
                    ),
            )?;
            if e.kind == "acknowledged" {
                ensure(e.message.to_lowercase().contains("dry-run"))?;
            }
        } else {
            public_claim(db, &e)?;
        }
        state.insert(
            e.generation,
            (
                attempted || e.kind == "attempted",
                e.kind,
                e.provider,
                false,
            ),
        );
        typed_count += 1;
    }
    let audits: i64 = db
        .query_row(
            "SELECT count(*) FROM audit_log WHERE event_type=?1",
            [AUDIT],
            |r| r.get(0),
        )
        .map_err(sql)?;
    ensure(audits == typed_count)
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
