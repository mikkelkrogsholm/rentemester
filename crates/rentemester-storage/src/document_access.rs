//! Bounded, credential-free append-only access evidence. Legacy human events stay unchanged.
use rentemester_engine::Error;
use rusqlite::{Connection, Transaction, params};
const TABLE: &str = "native_service_document_access_events";
const DDL: &str = "CREATE TABLE native_service_document_access_events(id INTEGER PRIMARY KEY,actor TEXT NOT NULL CHECK(length(actor) BETWEEN 17 AND 4112 AND substr(actor,1,16)='service_account:'),company_slug TEXT NOT NULL CHECK(length(company_slug) BETWEEN 1 AND 120 AND substr(company_slug,1,1) GLOB '[a-z0-9]' AND company_slug NOT GLOB '*[^a-z0-9-]*'),resource_type TEXT NOT NULL CHECK(resource_type IN ('document_file','issued_invoice_pdf')),resource_id INTEGER CHECK(resource_id IS NULL OR resource_id>0),outcome TEXT NOT NULL CHECK(outcome IN ('served','denied')),reason_code TEXT NOT NULL CHECK((outcome='served' AND reason_code='authorized') OR (outcome='denied' AND reason_code='authorization_denied')),request_id TEXT CHECK(request_id IS NULL),created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);CREATE TRIGGER native_service_document_access_no_update BEFORE UPDATE ON native_service_document_access_events BEGIN SELECT RAISE(ABORT,'document access events are append-only'); END;CREATE TRIGGER native_service_document_access_no_delete BEFORE DELETE ON native_service_document_access_events BEGIN SELECT RAISE(ABORT,'document access events are append-only'); END;";
fn refused() -> Error {
    Error::new(
        "document_access_audit_unavailable",
        "document access evidence could not be persisted",
    )
}
pub(crate) fn strip_schema(
    _db: &Connection,
    actual: &[crate::migration::SchemaObject],
) -> Result<Vec<crate::migration::SchemaObject>, Error> {
    let reference = Connection::open_in_memory().map_err(|_| refused())?;
    reference.execute_batch(DDL).map_err(|_| refused())?;
    let extension = crate::migration::schema(&reference)?;
    let present = actual
        .iter()
        .any(|o| extension.iter().any(|e| e.name == o.name));
    if !present {
        return Ok(actual.to_vec());
    }
    for expected in &extension {
        if !actual.contains(expected) {
            return Err(refused());
        }
    }
    Ok(actual
        .iter()
        .filter(|o| !extension.contains(o))
        .cloned()
        .collect())
}
pub fn install(tx: &Transaction<'_>) -> Result<(), Error> {
    crate::control_migration::validate_current_control(tx)?;
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name=?1)",
            [TABLE],
            |r| r.get(0),
        )
        .map_err(|_| refused())?;
    if !exists {
        tx.execute_batch(DDL).map_err(|_| refused())?;
    }
    crate::control_migration::validate_current_control(tx)
}
pub fn append(
    tx: &Transaction<'_>,
    subject: &str,
    service: bool,
    slug: &str,
    resource: &str,
    id: i64,
    served: bool,
) -> Result<(), Error> {
    if tx.transaction_state(Some(rusqlite::MAIN_DB)).ok() != Some(rusqlite::TransactionState::Write)
    {
        return Err(refused());
    }
    let safe = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    };
    if (!service && (subject.len() > 155 || !safe(subject)))
        || (service
            && (subject.is_empty()
                || subject.len() > 4096
                || subject.chars().any(char::is_control)))
        || id <= 0
        || !["document_file", "issued_invoice_pdf"].contains(&resource)
    {
        return Err(refused());
    }
    crate::control_migration::validate_current_control(tx)?;
    let table = if service {
        install(tx)?;
        TABLE
    } else {
        "rm_workspace_document_access_events"
    };
    let exhausted: bool = tx
        .query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=9223372036854775807)"),
            [],
            |r| r.get(0),
        )
        .map_err(|_| refused())?;
    if exhausted {
        return Err(refused());
    }
    let actor = format!(
        "{}:{subject}",
        if service { "service_account" } else { "user" }
    );
    tx.execute(&format!("INSERT INTO {table}(actor,company_slug,resource_type,resource_id,outcome,reason_code,request_id) VALUES(?1,?2,?3,?4,?5,?6,NULL)"),params![actor,slug,resource,id,if served {"served"} else {"denied"},if served {"authorized"} else {"authorization_denied"}]).map_err(|_|refused())?;
    Ok(())
}
pub(crate) fn table() -> &'static str {
    TABLE
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_extension_and_append_only_service_evidence() {
        let mut c = Connection::open_in_memory().unwrap();
        let mut d = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut d, "Synthetic audit").unwrap();
        let tx = c
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        append(
            &tx,
            "synthetic-service",
            true,
            "a",
            "document_file",
            1,
            true,
        )
        .unwrap();
        tx.commit().unwrap();
        crate::control_migration::validate_current_control(&c).unwrap();
        for sql in [
            "UPDATE native_service_document_access_events SET outcome='denied'",
            "DELETE FROM native_service_document_access_events",
        ] {
            assert!(c.execute(sql, []).is_err());
        }
        c.execute_batch("DROP TRIGGER native_service_document_access_no_delete")
            .unwrap();
        assert!(crate::control_migration::validate_current_control(&c).is_err());
    }
}
