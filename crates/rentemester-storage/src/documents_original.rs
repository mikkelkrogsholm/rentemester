//! Original-document metadata evidence, on caller-owned authorized transactions.
use rentemester_engine::{Error, document::Metadata};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
fn refused(message: impl Into<String>) -> Error {
    Error::new("DOCUMENT_ORIGINAL_REFUSED", message)
}
fn sql(_: rusqlite::Error) -> Error {
    refused("original document evidence unavailable")
}
pub fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
pub fn canonical(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort();
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|key| format!("{}:{}", json!(key), canonical(&object[key])))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(array) => format!(
            "[{}]",
            array.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        _ => value.to_string(),
    }
}
fn decode(raw: &str) -> Result<Metadata, Error> {
    let metadata: Metadata =
        serde_json::from_str(raw).map_err(|_| refused("invalid document metadata JSON"))?;
    metadata.validate()?;
    Ok(metadata)
}
// Older unlinked metadata may be incomplete; only the new enrichment must meet
// current ingest requirements. Its immutable original bytes/hash remain bound.
fn original_metadata(raw: &str) -> Result<Metadata, Error> {
    serde_json::from_str(raw).map_err(|_| refused("invalid original document metadata JSON"))
}
fn subset(old: &Value, new: &Value) -> bool {
    match old {
        Value::Null => true,
        Value::String(s) if s.trim().is_empty() => true,
        Value::Object(fields) => fields.iter().all(|(k, v)| subset(v, &new[k])),
        _ => old == new,
    }
}
fn eligible(tx: &Transaction<'_>, id: i64, strict_links: bool) -> Result<String, Error> {
    let row: Option<(String, String, String)> = tx
        .query_row(
            "SELECT status,document_type,payload_json FROM documents WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(sql)?;
    let (status, kind, payload) = row.ok_or_else(|| refused("document does not exist"))?;
    if id <= 0 || status != "ingested" || kind == "internal_voucher" {
        return Err(refused(
            "document must be ingested, noninternal and unposted",
        ));
    }
    let linked: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_entries WHERE document_id=?1 UNION ALL SELECT 1 FROM import_document_links WHERE document_id=?1 AND (?2 OR journal_entry_id IS NOT NULL) UNION ALL SELECT 1 FROM dinero_import_document_links WHERE document_id=?1 AND (?2 OR journal_entry_id IS NOT NULL OR disposition='linked') UNION ALL SELECT 1 FROM payables WHERE document_id=?1)", params![id,strict_links], |r|r.get(0)).map_err(sql)?;
    if linked {
        return Err(refused("document is linked to accounting evidence"));
    }
    if strict_links {
        decode(&payload)?;
    } else {
        original_metadata(&payload)?;
    }
    Ok(payload)
}
/// Projection shared by mutation and semantic history checks, never physical PDF bytes.
fn projection(metadata: &Metadata) -> Result<Value, Error> {
    let identity = metadata.validate()?;
    let sender = metadata.sender.as_ref();
    let recipient = metadata.recipient.as_ref();
    Ok(
        json!({"source":metadata.source,"document_type":metadata.kind(),"supplier_name":sender.and_then(|p|p.name.as_deref()),"invoice_no":metadata.invoice_no,"invoice_date":metadata.issue_date,"amount_inc_vat":metadata.amount_inc_vat,"currency":metadata.currency(),"delivery_description":metadata.delivery_description,"sender_name":sender.and_then(|p|p.name.as_deref()),"sender_address":sender.and_then(|p|p.address.as_deref()),"sender_vat_cvr":identity.as_ref().and_then(|i|i.identifier.as_deref()).or_else(||sender.and_then(|p|p.vat_or_cvr.as_deref())),"supplier_country_code":identity.as_ref().map(|i|i.country.as_str()),"supplier_identifier_kind":identity.as_ref().map(|i|i.kind.as_str()),"supplier_identity_status":identity.as_ref().map(|_|"resolved"),"recipient_name":recipient.and_then(|p|p.name.as_deref()),"recipient_address":recipient.and_then(|p|p.address.as_deref()),"recipient_vat_cvr":recipient.and_then(|p|p.vat_or_cvr.as_deref()),"vat_amount":metadata.vat_amount,"payment_details":metadata.payment_details,"exemption_code":metadata.exemption_code}),
    )
}
fn facts(db: &Connection, id: i64, metadata: &Metadata, fill: bool) -> Result<(), Error> {
    for (column, incoming) in projection(metadata)?
        .as_object()
        .ok_or_else(|| refused("invalid projection"))?
    {
        let persisted: Value = db
            .query_row(
                &format!("SELECT {column} FROM documents WHERE id=?1"),
                [id],
                |r| {
                    Ok(match r.get_ref(0)? {
                        rusqlite::types::ValueRef::Null => Value::Null,
                        rusqlite::types::ValueRef::Text(s) => json!(String::from_utf8_lossy(s)),
                        rusqlite::types::ValueRef::Real(n) => json!(n),
                        rusqlite::types::ValueRef::Integer(n) => json!(n),
                        _ => Value::Null,
                    })
                },
            )
            .map_err(sql)?;
        let missing = persisted.is_null() || persisted.as_str() == Some("");
        let equal = persisted == *incoming
            || persisted
                .as_f64()
                .zip(incoming.as_f64())
                .is_some_and(|(a, b)| a == b);
        if !(equal || fill && missing) {
            return Err(refused(format!(
                "document {column} conflicts with metadata"
            )));
        }
    }
    Ok(())
}
pub(crate) fn duplicate(tx: &Transaction<'_>, id: i64, metadata: &Metadata) -> Result<(), Error> {
    let p = projection(metadata)?;
    let invoice = metadata.invoice_no.as_deref().unwrap_or("").trim();
    if metadata.kind() != "purchase_sale" || invoice.is_empty() {
        return Ok(());
    }
    let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM documents WHERE id!=?1 AND document_type='purchase_sale' AND trim(invoice_no)=?2 AND ((?3 IS NOT NULL AND sender_vat_cvr=?3) OR (?4='non_eu' AND supplier_country_code=?5 AND lower(trim(sender_name))=lower(trim(?6)) AND trim(?6)!='')))", params![id,invoice,p["sender_vat_cvr"].as_str(),p["supplier_identifier_kind"].as_str(),p["supplier_country_code"].as_str(),p["sender_name"].as_str()], |r|r.get(0)).map_err(sql)?;
    if exists {
        return Err(refused("supplier invoice identity already registered"));
    }
    Ok(())
}
/// Source enrichment defaults and recipient whitelist; raw originals stay separate.
fn normalized(metadata: &Metadata) -> Result<Value, Error> {
    let mut value = serde_json::to_value(metadata).map_err(|_| refused("invalid metadata"))?;
    value["documentType"] = json!(metadata.kind());
    value["currency"] = json!(metadata.currency());
    if let Some(fields) = value.as_object_mut() {
        fields.remove("sourceBankTransactionId");
        fields.remove("accountingRationale");
    }
    if let Some(recipient) = value.get_mut("recipient").and_then(Value::as_object_mut) {
        recipient.retain(|key, _| matches!(key.as_str(), "name" | "address" | "vatOrCvr"));
    }
    Ok(value)
}
fn identity_gate(tx: &Transaction<'_>, metadata: &Metadata) -> Result<(), Error> {
    for party in [&metadata.sender, &metadata.recipient]
        .into_iter()
        .flatten()
    {
        crate::gdpr::require_contact_identity(
            tx,
            "vendors",
            None,
            party.name.as_deref().unwrap_or(""),
            party.vat_or_cvr.as_deref(),
        )?;
    }
    Ok(())
}
pub fn enrich(
    tx: &Transaction<'_>,
    id: i64,
    metadata: &Metadata,
    actor: &str,
) -> Result<Value, Error> {
    let original = eligible(tx, id, false)?;
    validate_history(tx)?;
    metadata.validate()?;
    if metadata.kind() == "internal_voucher" {
        return Err(refused("internal voucher metadata cannot be enriched"));
    }
    identity_gate(tx, metadata)?;
    let value = normalized(metadata)?;
    let enriched = canonical(&value);
    let digest = hash(&enriched);
    let existing: Option<String> = tx.query_row("SELECT enriched_metadata_sha256 FROM document_metadata_enrichments WHERE document_id=?1",[id],|r|r.get(0)).optional().map_err(sql)?;
    if let Some(existing) = existing {
        return if existing == digest {
            Ok(json!({"ok":true,"documentId":id,"enriched":false}))
        } else {
            Err(refused(
                "document metadata was already enriched with different metadata",
            ))
        };
    }
    if !subset(&normalized(&original_metadata(&original)?)?, &value) {
        return Err(refused(
            "existing document metadata conflicts with enrichment",
        ));
    }
    facts(tx, id, metadata, true)?;
    duplicate(tx, id, metadata)?;
    let context: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM document_company_contexts WHERE document_id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if context {
        return Err(refused("company context already binds the current payload"));
    }
    for (column, value) in projection(metadata)?
        .as_object()
        .ok_or_else(|| refused("invalid projection"))?
    {
        let sql_value = match value {
            Value::Null => rusqlite::types::Value::Null,
            Value::String(s) => rusqlite::types::Value::Text(s.clone()),
            Value::Number(n) => {
                rusqlite::types::Value::Real(n.as_f64().ok_or_else(|| refused("invalid amount"))?)
            }
            _ => return Err(refused("invalid projection")),
        };
        tx.execute(
            &format!(
                "UPDATE documents SET {column}=?1 WHERE id=?2 AND ({column} IS NULL OR {column}='')"
            ),
            params![sql_value, id],
        )
        .map_err(sql)?;
    }
    tx.execute(
        "UPDATE documents SET payload_json=?1 WHERE id=?2",
        params![enriched, id],
    )
    .map_err(sql)?;
    tx.execute("INSERT INTO document_metadata_enrichments(document_id,original_payload_json,original_payload_sha256,enriched_metadata_json,enriched_metadata_sha256,actor,program) VALUES(?1,?2,?3,?4,?5,?6,'rentemester')",params![id,original,hash(&original),enriched,digest,actor]).map_err(sql)?;
    audit(tx, id, "document_metadata_enriched", actor)?;
    validate_history(tx)?;
    Ok(json!({"ok":true,"documentId":id,"enriched":true}))
}
fn audit(tx: &Transaction<'_>, id: i64, event: &str, actor: &str) -> Result<(), Error> {
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'document',?2,?1,?3)",params![event,id.to_string(),actor]).map_err(sql)?;
    Ok(())
}
fn snapshot(db: &Connection) -> Result<Value, Error> {
    let value = db.query_row("SELECT id,name,country,cvr,address,postal_code,city,vat_period_type FROM companies WHERE id=1",[],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"name":r.get::<_,String>(1)?,"country":r.get::<_,String>(2)?,"cvr":r.get::<_,Option<String>>(3)?,"address":r.get::<_,Option<String>>(4)?,"postalCode":r.get::<_,Option<String>>(5)?,"city":r.get::<_,Option<String>>(6)?,"vatPeriodType":r.get::<_,Option<String>>(7)?}))).map_err(sql)?;
    validate_snapshot(&value)?;
    Ok(value)
}
fn validate_snapshot(value: &Value) -> Result<(), Error> {
    let cvr = value["cvr"].as_str().unwrap_or("");
    if value.as_object().is_none_or(|o| o.len() != 8)
        || value["id"] != 1
        || value["country"] != "DK"
        || cvr
            .strip_prefix("DK")
            .is_none_or(|s| s.len() != 8 || !s.bytes().all(|b| b.is_ascii_digit()))
        || ["name", "address", "postalCode", "city", "vatPeriodType"]
            .iter()
            .any(|k| value[k].as_str().is_none_or(|s| s.trim().is_empty()))
    {
        return Err(refused(
            "complete VAT-registered Danish company profile required",
        ));
    }
    Ok(())
}
fn context_hash(
    document: &str,
    payload: &str,
    snapshot: &Value,
    reference: &str,
    reason: &str,
) -> String {
    hash(&canonical(
        &json!({"documentHash":document,"payloadHash":payload,"snapshot":snapshot,"sourceReference":reference,"businessUseReason":reason}),
    ))
}
fn text(value: &str) -> Result<&str, Error> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 2000 {
        Err(refused(
            "context text must be nonblank and at most 2000 characters",
        ))
    } else {
        Ok(value)
    }
}
fn qualifies(db: &Connection, id: i64, payload: &str) -> Result<(), Error> {
    let metadata = decode(payload)?;
    if metadata.danish_simplified_purchase_invoice != Some(true) {
        return Err(refused(
            "independently qualifying Danish simplified invoice required",
        ));
    }
    facts(db, id, &metadata, false)
}
pub fn company_context(
    tx: &Transaction<'_>,
    id: i64,
    reference: &str,
    reason: &str,
    actor: &str,
) -> Result<Value, Error> {
    let payload = eligible(tx, id, true)?;
    validate_history(tx)?;
    qualifies(tx, id, &payload)?;
    let reference = text(reference)?;
    let reason = text(reason)?;
    let snapshot = snapshot(tx)?;
    let document: String = tx
        .query_row("SELECT sha256_hash FROM documents WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .map_err(sql)?;
    let payload_hash = hash(&payload);
    let digest = context_hash(&document, &payload_hash, &snapshot, reference, reason);
    let old: Option<String> = tx
        .query_row(
            "SELECT context_sha256 FROM document_company_contexts WHERE document_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    if let Some(old) = old {
        return if old == digest {
            Ok(json!({"ok":true,"documentId":id,"applied":false,"errors":[]}))
        } else {
            Err(refused(
                "document company context already exists with conflicting evidence",
            ))
        };
    }
    let snapshot_json = canonical(&snapshot);
    tx.execute("INSERT INTO document_company_contexts(document_id,company_id,document_sha256,payload_sha256,company_snapshot_json,company_snapshot_sha256,source_reference,business_use_reason,context_sha256,actor,program) VALUES(?1,1,?2,?3,?4,?5,?6,?7,?8,?9,'rentemester')",params![id,document,payload_hash,snapshot_json,hash(&snapshot_json),reference,reason,digest,actor]).map_err(sql)?;
    audit(tx, id, "document_company_context_set", actor)?;
    validate_history(tx)?;
    Ok(json!({"ok":true,"documentId":id,"applied":true,"errors":[]}))
}
/// Semantic validation accepts nonempty immutable histories and rejects mismatched
/// raw-original hashes, current projections, canonical metadata and company binding.
pub fn validate_history(db: &Connection) -> Result<(), Error> {
    for table in ["document_metadata_enrichments", "document_company_contexts"] {
        let mut query = db
            .prepare(&format!("SELECT actor,program,created_at FROM {table}"))
            .map_err(sql)?;
        let mut rows = query.query([]).map_err(sql)?;
        while let Some(row) = rows.next().map_err(sql)? {
            let actor: String = row.get(0).map_err(sql)?;
            let program: String = row.get(1).map_err(sql)?;
            let at: String = row.get(2).map_err(sql)?;
            let stamp = if at.len() == 19 && at.as_bytes()[10] == b' ' {
                format!("{}T{}.000Z", &at[..10], &at[11..])
            } else {
                at
            };
            if actor.trim().is_empty()
                || program.trim().is_empty()
                || crate::cookie_session::timestamp(&stamp).is_err()
            {
                return Err(refused("document history attribution or timestamp invalid"));
            }
        }
    }
    let mut q=db.prepare("SELECT e.document_id,e.original_payload_json,e.original_payload_sha256,e.enriched_metadata_json,e.enriched_metadata_sha256,d.payload_json FROM document_metadata_enrichments e LEFT JOIN documents d ON d.id=e.document_id").map_err(sql)?;
    let mut rows = q.query([]).map_err(sql)?;
    while let Some(r) = rows.next().map_err(sql)? {
        let id: i64 = r.get(0).map_err(sql)?;
        let original: String = r.get(1).map_err(sql)?;
        let original_hash: String = r.get(2).map_err(sql)?;
        let enriched: String = r.get(3).map_err(sql)?;
        let digest: String = r.get(4).map_err(sql)?;
        let current: String = r.get(5).map_err(sql)?;
        let old = original_metadata(&original)?;
        let new = decode(&enriched)?;
        let old = normalized(&old)?;
        let value = normalized(&new)?;
        if new.kind() == "internal_voucher"
            || hash(&original) != original_hash
            || enriched != canonical(&value)
            || hash(&enriched) != digest
            || enriched != current
            || !subset(&old, &value)
        {
            return Err(refused("document enrichment history tampered"));
        }
        facts(db, id, &new, false)?;
    }
    let mut q=db.prepare("SELECT c.document_id,c.company_id,c.document_sha256,c.payload_sha256,c.company_snapshot_json,c.company_snapshot_sha256,c.source_reference,c.business_use_reason,c.context_sha256,d.sha256_hash,d.payload_json FROM document_company_contexts c LEFT JOIN documents d ON d.id=c.document_id").map_err(sql)?;
    let mut rows = q.query([]).map_err(sql)?;
    while let Some(r) = rows.next().map_err(sql)? {
        let id: i64 = r.get(0).map_err(sql)?;
        let company: i64 = r.get(1).map_err(sql)?;
        let document: String = r.get(2).map_err(sql)?;
        let payload_hash: String = r.get(3).map_err(sql)?;
        let raw: String = r.get(4).map_err(sql)?;
        let snapshot_hash: String = r.get(5).map_err(sql)?;
        let reference: String = r.get(6).map_err(sql)?;
        let reason: String = r.get(7).map_err(sql)?;
        let digest: String = r.get(8).map_err(sql)?;
        let current_hash: String = r.get(9).map_err(sql)?;
        let payload: String = r.get(10).map_err(sql)?;
        let value: Value =
            serde_json::from_str(&raw).map_err(|_| refused("invalid context snapshot"))?;
        if company != 1
            || validate_snapshot(&value).is_err()
            || raw != canonical(&value)
            || hash(&raw) != snapshot_hash
            || document != current_hash
            || payload_hash != hash(&payload)
            || text(&reference)? != reference
            || text(&reason)? != reason
            || digest != context_hash(&document, &payload_hash, &value, &reference, &reason)
        {
            return Err(refused("document company context tampered or stale"));
        }
        qualifies(db, id, &payload)?;
    }
    Ok(())
}
pub fn valid_context_for_document(
    tx: &Transaction<'_>,
    id: i64,
    metadata: &Metadata,
) -> Result<bool, Error> {
    validate_history(tx)?;
    let raw: Option<String> = tx.query_row("SELECT d.payload_json FROM documents d JOIN document_company_contexts c ON c.document_id=d.id WHERE d.id=?1",[id],|r|r.get(0)).optional().map_err(sql)?;
    let Some(raw) = raw else {
        return Ok(false);
    };
    // Decode only this exact selected source; never search for equal metadata.
    let selected = decode(&raw)?;
    if serde_json::to_value(&selected).ok() != serde_json::to_value(metadata).ok() {
        return Err(refused("selected document metadata mismatch"));
    }
    facts(tx, id, metadata, false)?;
    Ok(true)
}
/// Original surface has no journal joins, no private path and no raw payload.
pub fn list(tx: &Transaction<'_>) -> Result<Value, Error> {
    crate::validate(tx)?;
    let mut q=tx.prepare("SELECT d.id,d.document_no,d.source,d.original_filename,d.document_type,d.invoice_date,d.amount_inc_vat,d.currency,d.status,d.sender_vat_cvr,d.supplier_country_code,d.supplier_identifier_kind,d.supplier_identity_status,v.bank_transaction_id,v.accounting_rationale,v.prepared_by,v.prepared_by_program,d.payload_json FROM documents d LEFT JOIN internal_voucher_evidence v ON v.document_id=d.id ORDER BY d.id DESC").map_err(sql)?;
    let rows=q.query_map([],|r| { let payload:Option<String>=r.get(17)?; let payload=payload.and_then(|s|serde_json::from_str::<Value>(&s).ok()).unwrap_or(Value::Null); Ok(json!({"id":r.get::<_,i64>(0)?,"documentNo":r.get::<_,Option<String>>(1)?,"source":r.get::<_,String>(2)?,"originalFilename":r.get::<_,Option<String>>(3)?,"documentType":r.get::<_,String>(4)?,"invoiceDate":r.get::<_,Option<String>>(5)?,"amountIncVat":r.get::<_,Option<f64>>(6)?,"currency":r.get::<_,String>(7)?,"status":r.get::<_,String>(8)?,"storedPath":null,"senderVatOrCvr":r.get::<_,Option<String>>(9)?,"supplierCountryCode":r.get::<_,Option<String>>(10)?,"supplierIdentifierKind":r.get::<_,Option<String>>(11)?,"supplierIdentityStatus":r.get::<_,Option<String>>(12)?,"sourceBankTransactionId":r.get::<_,Option<i64>>(13)?,"accountingRationale":r.get::<_,Option<String>>(14)?,"preparedBy":r.get::<_,Option<String>>(15)?,"preparedByProgram":r.get::<_,Option<String>>(16)?,"purchaseVatLines":payload["purchaseVatLines"]})) }).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?;
    Ok(json!({"ok":true,"documents":rows,"errors":[]}))
}
/// Fill missing sender facts only. Return the exact vendor observation for the
/// admission to recheck after scanner I/O.
pub fn vendor_fill(
    tx: &Transaction<'_>,
    metadata: &mut Metadata,
    vendor_id: Option<i64>,
) -> Result<Option<String>, Error> {
    let Some(id) = vendor_id else {
        return Ok(None);
    };
    if id <= 0 {
        return Err(refused("positive vendor ID required"));
    }
    let vendor:Option<Value>=tx.query_row("SELECT name,address,vat_or_cvr,country_code,identifier_kind FROM vendors WHERE id=?1 AND archived=0",[id],|r|Ok(json!({"name":r.get::<_,String>(0)?,"address":r.get::<_,Option<String>>(1)?,"vatOrCvr":r.get::<_,Option<String>>(2)?,"countryCode":r.get::<_,Option<String>>(3)?,"identifierKind":r.get::<_,Option<String>>(4)?}))).optional().map_err(sql)?;
    let vendor = vendor.ok_or_else(|| refused("current nonarchived vendor required"))?;
    let sender = metadata.sender.get_or_insert_with(Default::default);
    for (field, key) in [
        (&mut sender.name, "name"),
        (&mut sender.address, "address"),
        (&mut sender.vat_or_cvr, "vatOrCvr"),
    ] {
        if field.as_ref().is_none_or(|s| s.trim().is_empty()) {
            *field = vendor[key]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
        }
    }
    if sender.country_code.is_none() {
        sender.country_code = vendor["countryCode"].as_str().map(str::to_owned);
    }
    if sender.identifier_kind.is_none() {
        sender.identifier_kind = vendor["identifierKind"].as_str().map(str::to_owned);
    }
    identity_gate(tx, metadata)?;
    Ok(Some(canonical(&vendor)))
}
#[cfg(test)]
mod tests;

/// A supported intake refusal is durable review evidence, deduplicated by safe
/// source identity. Never include a server path in the exception payload.
pub fn blocked(
    tx: &Transaction<'_>,
    filename: &str,
    sha256: &str,
    metadata: &Metadata,
    message: &str,
) -> Result<Value, Error> {
    let identity = json!({"fileName":filename,"sha256":sha256,"source":metadata.source,"invoiceNo":metadata.invoice_no});
    let key = format!("document-ingest-blocked:{}", hash(&canonical(&identity)));
    tx.execute("INSERT INTO exceptions(type,severity,message,required_action,source_evidence,resolution_key) SELECT 'DOCUMENT_INGEST_BLOCKED','high',?1,'Review original document intake evidence',?2,?3 WHERE NOT EXISTS(SELECT 1 FROM exceptions WHERE type='DOCUMENT_INGEST_BLOCKED' AND status='open' AND resolution_key=?3)",params![message,canonical(&identity),key]).map_err(sql)?;
    Ok(json!({"ok":false,"code":"DOCUMENT_INGEST_BLOCKED","errors":[message]}))
}
/// Check supported duplicate refusal before any physical publication. Force
/// affects the logical supplier identity check, never the content hash.
pub fn intake_duplicate(
    tx: &Transaction<'_>,
    sha256: &str,
    metadata: &Metadata,
    force: bool,
) -> Result<Option<String>, Error> {
    let number: Option<String> = tx
        .query_row(
            "SELECT document_no FROM documents WHERE sha256_hash=?1",
            [sha256],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    if let Some(number) = number {
        return Ok(Some(format!(
            "duplicate document content already ingested as {number}"
        )));
    }
    if !force && let Err(error) = duplicate(tx, 0, metadata) {
        return if error.message == "supplier invoice identity already registered" {
            Ok(Some(error.message))
        } else {
            Err(error)
        };
    }
    Ok(None)
}
