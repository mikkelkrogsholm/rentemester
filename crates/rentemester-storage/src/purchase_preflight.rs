//! Document-local VAT evidence. Caller owns transactions, authority and provider I/O.
use rentemester_engine::{
    Error,
    document::{self, Party, Supplier},
    vies,
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};

pub struct ProviderResult {
    pub status: String,
    pub name: Option<String>,
    pub address: Option<String>,
    pub raw_response: Option<String>,
}
/// An admitted, immutable source selection, deliberately not deserializable.
pub struct Prepared {
    document_id: i64,
    facts: Value,
    provider_vat: Option<String>,
    actor: String,
    history_id: Option<i64>,
}
impl Prepared {
    pub fn provider_vat(&self) -> Option<&str> {
        self.provider_vat.as_deref()
    }
}
fn fail(message: &str) -> Error {
    Error::new("purchase_preflight_refused", message)
}
fn sql(_: rusqlite::Error) -> Error {
    Error::new(
        "storage_failure",
        "purchase preflight evidence unavailable; abort transaction",
    )
}
fn author(actor: &str) -> Result<(), Error> {
    if actor.trim().is_empty() || actor.len() > 256 || actor.chars().any(char::is_control) {
        return Err(fail("authenticated audit actor required"));
    }
    Ok(())
}
pub(crate) fn supplier(tx: &Transaction<'_>, id: i64) -> Result<Option<Supplier>, Error> {
    let row: (Option<String>,Option<String>,Option<String>,Option<String>) = tx.query_row("SELECT sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status FROM documents WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(sql)?;
    if row.3.as_deref() != Some("resolved") {
        return Ok(None);
    }
    Ok(document::supplier(&Party {
        vat_or_cvr: row.0,
        country_code: row.1,
        identifier_kind: row.2,
        ..Default::default()
    })
    .ok())
}
fn facts(tx: &Transaction<'_>, id: i64) -> Result<Value, Error> {
    crate::documents_original::validate_history(tx)?;
    crate::invoice_extraction::validate_history(tx)?;
    if !(1..=9_007_199_254_740_991).contains(&id) {
        return Err(fail("positive safe document ID required"));
    }
    let row=tx.query_row("SELECT sha256_hash,stored_path,payload_json,sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status FROM documents WHERE id=?1",[id],|r|Ok(json!([r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,Option<String>>(4)?,r.get::<_,Option<String>>(5)?,r.get::<_,Option<String>>(6)?]))).optional().map_err(sql)?.ok_or_else(||fail("document does not exist"))?;
    Ok(json!({"document":row,"extraction":crate::invoice_extraction::inspect(tx,id)?}))
}
fn classification(
    tx: &Transaction<'_>,
    id: i64,
) -> Result<(&'static str, Option<String>, Option<String>), Error> {
    Ok(match supplier(tx, id)? {
        Some(s) => (
            match s.kind.as_str() {
                "dk_cvr" => "DK",
                "eu_vat" => "EU",
                "non_eu" => "NON_EU",
                _ => "CONFLICT",
            },
            Some(s.country),
            s.identifier,
        ),
        None => ("CONFLICT", None, None),
    })
}
pub fn inspect(tx: &Transaction<'_>, document_id: i64, now: &str) -> Result<Value, Error> {
    vies::expiry(now)?;
    let selected = facts(tx, document_id)?;
    let (mut region, country, identifier) = classification(tx, document_id)?;
    let unresolved =
        !selected["extraction"].is_null() && selected["extraction"]["status"] != "completed";
    if unresolved {
        region = "CONFLICT";
    }
    let expires: Option<String> = if region == "CONFLICT" {
        None
    } else {
        tx.query_row("SELECT evidence_expires_at FROM vat_validation_events WHERE document_id=?1 AND supplier_country_code IS ?2 AND supplier_identifier IS ?3 AND event_type='preflight_passed' AND evidence_expires_at>=?4 AND created_at<=?4 ORDER BY id DESC LIMIT 1",params![document_id,country,identifier,now],|r|r.get(0)).optional().map_err(sql)?.flatten()
    };
    let cached = expires.is_some();
    let errors: Vec<String> = if unresolved {
        vec!["invoice extraction evidence requires human resolution before VAT preflight".into()]
    } else if region == "CONFLICT" {
        vec!["documented supplier evidence is incomplete or conflicting; human resolution is required".into()]
    } else if region == "EU" && !cached {
        vec!["fresh EU VAT validation evidence is required".into()]
    } else {
        vec![]
    };
    let mut q=tx.prepare("SELECT id,event_type,classification,provider_status,evidence_expires_at,created_at,actor FROM vat_validation_events WHERE document_id=?1 ORDER BY id DESC LIMIT 20").map_err(sql)?;
    let evidence=q.query_map([document_id],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"event_type":r.get::<_,String>(1)?,"classification":r.get::<_,String>(2)?,"provider_status":r.get::<_,String>(3)?,"evidence_expires_at":r.get::<_,Option<String>>(4)?,"created_at":r.get::<_,String>(5)?,"actor":r.get::<_,Option<String>>(6)?}))).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?;
    let exception=tx.query_row("SELECT id,status,severity,message,required_action,created_at FROM exceptions WHERE related_document_id=?1 AND type='PURCHASE_VAT_PREFLIGHT' ORDER BY id DESC LIMIT 1",[document_id],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"status":r.get::<_,String>(1)?,"severity":r.get::<_,String>(2)?,"message":r.get::<_,String>(3)?,"requiredAction":r.get::<_,Option<String>>(4)?,"createdAt":r.get::<_,String>(5)?}))).optional().map_err(sql)?;
    Ok(
        json!({"ok":errors.is_empty(),"documentId":document_id,"classification":region,"cached":cached,"evidenceExpiresAt":expires,"wouldCallProvider":region=="EU"&&!cached,"errors":errors,"derivedRegion":region,"requiredValidation":if region=="EU" {Some("EU VAT provider validation")} else {None},"cache":{"reused":cached,"freshUntil":expires},"applyWouldCallProvider":region=="EU"&&!cached,"evidence":evidence,"exception":exception}),
    )
}
fn event(
    tx: &Transaction<'_>,
    id: i64,
    kind: &str,
    region: &str,
    status: &str,
    actor: &str,
    now: &str,
    observation: Option<&ProviderResult>,
    expires: Option<&str>,
) -> Result<i64, Error> {
    let (_, country, identifier) = classification(tx, id)?;
    tx.execute("INSERT INTO vat_validation_events(document_id,event_type,supplier_country_code,supplier_identifier,classification,provider_status,provider_name,provider_address,actor,evidence_expires_at,detail_json,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![id,kind,country,identifier,region,status,observation.and_then(|r|r.name.as_deref()),observation.and_then(|r|r.address.as_deref()),actor,expires,observation.and_then(|r|r.raw_response.as_ref()).map(|s|json!(s).to_string()),now]).map_err(sql)?;
    Ok(tx.last_insert_rowid())
}
pub fn prepare_apply(
    tx: &Transaction<'_>,
    document_id: i64,
    actor: &str,
    now: &str,
) -> Result<Prepared, Error> {
    crate::idempotency::writer(tx)?;
    author(actor)?;
    let snapshot = inspect(tx, document_id, now)?;
    let provider_vat = if snapshot["wouldCallProvider"] == true {
        Some(
            vies::foreign(
                classification(tx, document_id)?
                    .2
                    .as_deref()
                    .ok_or_else(|| fail("resolved EU VAT identity required"))?,
            )?
            .normalized,
        )
    } else {
        None
    };
    if provider_vat.is_some() {
        event(
            tx,
            document_id,
            "provider_requested",
            "EU",
            "requested",
            actor,
            now,
            None,
            None,
        )?;
    }
    let history_id = tx
        .query_row(
            "SELECT MAX(id) FROM vat_validation_events WHERE document_id=?1",
            [document_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    Ok(Prepared {
        document_id,
        facts: facts(tx, document_id)?,
        provider_vat,
        actor: actor.into(),
        history_id,
    })
}
fn exception(
    tx: &Transaction<'_>,
    id: i64,
    message: &str,
    action: &str,
    now: &str,
) -> Result<(), Error> {
    let key = format!("purchase-vat-preflight:{id}");
    tx.execute("INSERT INTO exceptions(type,severity,status,related_document_id,message,required_action,resolution_key,created_at) SELECT 'PURCHASE_VAT_PREFLIGHT','high','open',?1,?2,?3,?4,?5 WHERE NOT EXISTS(SELECT 1 FROM exceptions WHERE type='PURCHASE_VAT_PREFLIGHT' AND status='open' AND related_document_id=?1 AND message=?2 AND required_action=?3 AND resolution_key=?4)",params![id,message,action,key,now]).map_err(sql)?;
    Ok(())
}
pub fn finish_apply(
    tx: &Transaction<'_>,
    prepared: Prepared,
    provider_result: Option<ProviderResult>,
    actor: &str,
    now: &str,
) -> Result<Value, Error> {
    crate::idempotency::writer(tx)?;
    author(actor)?;
    vies::expiry(now)?;
    let id = prepared.document_id;
    if actor != prepared.actor
        || facts(tx, id)? != prepared.facts
        || prepared.provider_vat.is_some() != provider_result.is_some()
    {
        return Err(fail("preflight admission or immutable source changed"));
    }
    let latest: Option<i64> = tx
        .query_row(
            "SELECT MAX(id) FROM vat_validation_events WHERE document_id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if latest != prepared.history_id {
        return Err(fail("preflight evidence changed during provider wait"));
    }
    let snapshot = inspect(tx, id, now)?;
    let Some(result) = provider_result else {
        if snapshot["classification"] == "CONFLICT" {
            exception(
                tx,
                id,
                snapshot["errors"][0]
                    .as_str()
                    .unwrap_or("Supplier evidence requires resolution"),
                "Resolve supplier country and typed identifier, then retry VAT preflight",
                now,
            )?;
            event(
                tx,
                id,
                "preflight_failed",
                "CONFLICT",
                "conflict",
                actor,
                now,
                None,
                None,
            )?;
        }
        return inspect(tx, id, now);
    };
    if !["valid", "invalid", "inconclusive", "unavailable"].contains(&result.status.as_str()) {
        return Err(fail("invalid provider observation status"));
    }
    let expires = vies::expiry(now)?;
    event(
        tx,
        id,
        "provider_result",
        "EU",
        &result.status,
        actor,
        now,
        Some(&result),
        if result.status == "valid" {
            Some(&expires)
        } else {
            None
        },
    )?;
    if result.status == "valid" {
        let vat = vies::foreign(
            prepared
                .provider_vat
                .as_deref()
                .ok_or_else(|| fail("missing provider identity"))?,
        )?;
        let record = vies::record(
            &json!({"countryCode":vat.country_code,"vatNumber":vat.vat_number,"normalized":vat.normalized,"valid":true,"name":result.name,"address":result.address,"rawResponse":result.raw_response}),
            now,
        )?;
        crate::vies::store(tx, &record, actor)?;
        event(
            tx,
            id,
            "preflight_passed",
            "EU",
            "valid",
            actor,
            now,
            Some(&result),
            Some(&expires),
        )?;
    } else {
        let message = format!(
            "EU VAT provider returned {}; purchase posting is blocked until VAT preflight succeeds",
            result.status
        );
        exception(
            tx,
            id,
            &message,
            "Retry VAT preflight when authoritative supplier evidence is available",
            now,
        )?;
        event(
            tx,
            id,
            "preflight_failed",
            "EU",
            &result.status,
            actor,
            now,
            Some(&result),
            None,
        )?;
    }
    inspect(tx, id, now)
}

#[cfg(test)]
mod tests;
