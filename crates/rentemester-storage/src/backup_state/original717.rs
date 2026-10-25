//! Compile-time retained regulatory corpus and actual admitted schema identity.
use super::*;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
pub fn provenance(company: &Connection) -> Result<Value, Error> {
    let (version,baseline):(i64,String)=company.query_row("SELECT max(id),(SELECT checksum FROM schema_migrations WHERE id=1) FROM schema_migrations",[],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|invalid())?;
    let mut hash = Sha256::new();
    let bytes = include_bytes!("../../../../rules/dk/annual-report.yaml");
    hash.update(b"rules/dk/annual-report.yaml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../rules/dk/bank.yaml");
    hash.update(b"rules/dk/bank.yaml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../rules/dk/bookkeeping.yaml");
    hash.update(b"rules/dk/bookkeeping.yaml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../rules/dk/documents.yaml");
    hash.update(b"rules/dk/documents.yaml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../rules/dk/email.yaml");
    hash.update(b"rules/dk/email.yaml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../rules/dk/gdpr.yaml");
    hash.update(b"rules/dk/gdpr.yaml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../rules/dk/invoices.yaml");
    hash.update(b"rules/dk/invoices.yaml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../rules/dk/mail-intake.yaml");
    hash.update(b"rules/dk/mail-intake.yaml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../rules/dk/mileage.yaml");
    hash.update(b"rules/dk/mileage.yaml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../rules/dk/peppol.yaml");
    hash.update(b"rules/dk/peppol.yaml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../rules/dk/tax-return.yaml");
    hash.update(b"rules/dk/tax-return.yaml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../rules/dk/vat.yaml");
    hash.update(b"rules/dk/vat.yaml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../sources/downloaded/DK-BILAG-OPBEVARING-2023-1383.xml");
    hash.update(b"sources/downloaded/DK-BILAG-OPBEVARING-2023-1383.xml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes =
        include_bytes!("../../../../sources/downloaded/DK-BILAG-OPBEVARING-AMEND-2025-302.xml");
    hash.update(b"sources/downloaded/DK-BILAG-OPBEVARING-AMEND-2025-302.xml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../sources/downloaded/DK-BOGFORINGSLOVEN-2022-700.xml");
    hash.update(b"sources/downloaded/DK-BOGFORINGSLOVEN-2022-700.xml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!(
        "../../../../sources/downloaded/DK-DIGITAL-BOGFORING-NONREGISTERED-2024-205.xml"
    );
    hash.update(b"sources/downloaded/DK-DIGITAL-BOGFORING-NONREGISTERED-2024-205.xml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes =
        include_bytes!("../../../../sources/downloaded/DK-DIGITAL-STANDARD-BOGFORING-2023-97.xml");
    hash.update(b"sources/downloaded/DK-DIGITAL-STANDARD-BOGFORING-2023-97.xml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes =
        include_bytes!("../../../../sources/downloaded/DK-MOMSBEKENDTGORELSEN-2023-1435.xml");
    hash.update(b"sources/downloaded/DK-MOMSBEKENDTGORELSEN-2023-1435.xml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../sources/downloaded/DK-MOMSLOVEN-2024-209.xml");
    hash.update(b"sources/downloaded/DK-MOMSLOVEN-2024-209.xml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../sources/downloaded/DK-RENTELOVEN-2014-459.xml");
    hash.update(b"sources/downloaded/DK-RENTELOVEN-2014-459.xml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!(
        "../../../../sources/downloaded/DK-UDENRETLIGE-INDDRIVELSESOMKOSTNINGER-AMEND-2013-105.xml"
    );
    hash.update(b"sources/downloaded/DK-UDENRETLIGE-INDDRIVELSESOMKOSTNINGER-AMEND-2013-105.xml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../sources/downloaded/README.md");
    hash.update(b"sources/downloaded/README.md\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../sources/downloaded/index.json");
    hash.update(b"sources/downloaded/index.json\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../sources/legal-sources.json");
    hash.update(b"sources/legal-sources.json\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    let bytes = include_bytes!("../../../../sources/scope.yaml");
    hash.update(b"sources/scope.yaml\0");
    hash.update(bytes.len().to_string().as_bytes());
    hash.update(b"\0");
    hash.update(bytes);
    hash.update(b"\0");
    Ok(
        json!({"product":{"version":env!("CARGO_PKG_VERSION"),"gitCommit":null,"builtAt":null,"bunVersion":null,"baseImageDigest":null,"runtime":"rust"},"schema":{"version":version,"baselineChecksum":baseline},"rules":{"digest":format!("sha256:{:x}",hash.finalize())}}),
    )
}

pub fn placement(
    company: &Transaction<'_>,
    destination: &str,
    placement: &Value,
    actor: &str,
    at: &str,
) -> Result<(), Error> {
    company.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES('original717_backup_placement','backup_destination',?,?,?,?)",params![destination,serde_json::to_string(placement).map_err(|_|invalid())?,actor,at]).map_err(|_|invalid())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original717_system_backup_v2_truthful_rust_provenance() {
        let mut c = Connection::open_in_memory().unwrap();
        let mut l = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic provenance").unwrap();
        let p = provenance(&l).unwrap();
        assert_eq!(p["product"]["runtime"], "rust");
        assert!(p["product"]["bunVersion"].is_null());
        assert_eq!(p["schema"]["baselineChecksum"].as_str().unwrap().len(), 64);
        assert!(p["schema"]["version"].as_u64().unwrap() >= 36);
        assert_eq!(p["rules"]["digest"].as_str().unwrap().len(), 71);
    }
}

/// Validate a retained SQLite byte image, never a live pathname or caller-supplied receipt.
/// tempfile owns a new 0600 inode, so closing this connection cannot release live writer locks.
pub fn validate_image(
    bytes: &[u8],
    uuid: &str,
    files: &std::collections::BTreeMap<String, Vec<u8>>,
) -> Result<[i64; 3], Error> {
    use std::io::Write;
    if bytes.is_empty() || bytes.len() > 64 * 1024 * 1024 {
        return Err(invalid());
    }
    let mut file = tempfile::NamedTempFile::new().map_err(|_| invalid())?;
    file.write_all(bytes).map_err(|_| invalid())?;
    let mut db = Connection::open_with_flags(
        file.path(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| invalid())?;
    db.execute_batch("PRAGMA trusted_schema=OFF;PRAGMA foreign_keys=ON;")
        .map_err(|_| invalid())?;
    let validated = crate::portable_backup::company(&db)?;
    if validated.uuid != uuid {
        return Err(invalid());
    }
    for (path, expected) in validated.originals {
        let path = path.replacen("documents/originals/", "documents-originals/", 1);
        let bytes = files.get(&path).ok_or_else(invalid)?;
        if format!("{:x}", Sha256::digest(bytes)) != expected {
            return Err(invalid());
        }
    }
    let tx = db.transaction().map_err(|_| invalid())?;
    activity_ids(&tx)
}

/// Source receipts live in the company audit, not the native workspace receipt stream.
pub fn placements(company: &Transaction<'_>, destination: &str) -> Result<Vec<Value>, Error> {
    let mut q = company.prepare("SELECT message FROM audit_log WHERE event_type='original717_backup_placement' AND entity_type='backup_destination' AND entity_id=? ORDER BY id").map_err(|_|invalid())?;
    let mut out = Vec::new();
    for row in q
        .query_map([destination], |r| r.get::<_, String>(0))
        .map_err(|_| invalid())?
    {
        let wire = row.map_err(|_| invalid())?;
        if wire.len() > 16384 {
            return Err(invalid());
        }
        let p: Value = serde_json::from_str(&wire).map_err(|_| invalid())?;
        if !p["backupId"].is_string()
            || !p["archiveSha256"].is_string()
            || !p["verified"].is_boolean()
            || !p["placedAt"].is_string()
        {
            return Err(invalid());
        }
        out.push(p);
    }
    Ok(out)
}
