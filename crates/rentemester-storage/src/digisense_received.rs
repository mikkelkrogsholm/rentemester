//! Append-only receive bookkeeping on an existing authorized transaction.
//! No download, authority grant, or standalone commit is provided here.
use rentemester_engine::Error;
use rusqlite::{Transaction, params};
use serde_json::Value;
fn refused() -> Error {
    Error::new(
        "digisense_received_storage",
        "Received document bookkeeping unavailable",
    )
}
fn field<'a>(record: &'a Value, key: &str) -> Option<&'a str> {
    record.get(key).and_then(Value::as_str)
}
pub fn internal_id(record: &Value) -> Result<&str, Error> {
    field(record, "internalId")
        .filter(|s| !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control))
        .ok_or_else(refused)
}
/// Includes terminal quarantine rows whose document_id is NULL.
pub fn known(tx: &Transaction<'_>, internal_id: &str) -> Result<bool, Error> {
    tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM digisense_received_documents WHERE internal_id=?1)",
        [internal_id],
        |r| r.get(0),
    )
    .map_err(|_| refused())
}
/// Append after ordinary original registration in the SAME caller-owned transaction.
/// `None` is terminal quarantine; reasons are fixed safe codes, never provider/XML text.
pub fn append(
    tx: &Transaction<'_>,
    company_key: &str,
    record: &Value,
    document_id: Option<i64>,
    reason: Option<TerminalReason>,
    actor: &str,
) -> Result<(), Error> {
    let id = internal_id(record)?;
    if company_key.trim().is_empty()
        || actor.trim().is_empty()
        || document_id.is_some() == reason.is_some()
        || document_id.is_some_and(|id| id <= 0)
    {
        return Err(refused());
    }
    if let Some(document) = document_id {
        let eligible:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1 AND source='digisense_modtag' AND document_type='purchase_sale' AND mime_type='application/xml')",[document],|r|r.get(0)).map_err(|_|refused())?;
        if !eligible {
            return Err(refused());
        }
    }
    let code = reason.map(TerminalReason::code);
    tx.execute("INSERT INTO digisense_received_documents(internal_id,company_key,document_id,skip_reason,digisense_document_id,source_network,sender_participant_id,sender_name,received_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id,company_key,document_id,code,field(record,"documentId"),field(record,"sourceNetwork"),field(record,"senderParticipantId"),field(record,"senderName"),field(record,"receivedAt")]).map_err(|_|refused())?;
    let event = if document_id.is_some() {
        "digisense_document_received"
    } else {
        "digisense_document_quarantined"
    };
    // JSON escaping prevents untrusted stable IDs from forging audit lines.
    let message = serde_json::json!({"internalId":id,"reason":code}).to_string();
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'document',?2,?3,?4)",params![event,document_id,message,actor]).map_err(|_|refused())?;
    Ok(())
}
#[derive(Clone, Copy)]
pub enum TerminalReason {
    InvalidXml,
    InvalidMetadata,
    DuplicateContent,
    DuplicateLogicalIdentity,
}
impl TerminalReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidXml => "invalid_xml",
            Self::InvalidMetadata => "invalid_metadata",
            Self::DuplicateContent => "duplicate_content",
            Self::DuplicateLogicalIdentity => "duplicate_logical_identity",
        }
    }
}

#[cfg(test)]
mod tests;
