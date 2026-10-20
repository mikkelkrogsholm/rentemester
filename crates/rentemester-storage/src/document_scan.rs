//! Historical clean scan evidence. Physical original verification remains with the admitted namespace.
use rentemester_engine::{Error, document_scan::ScanEvidence};
use rusqlite::Connection;
fn refused() -> Error {
    Error::new(
        "document_scan_evidence_refused",
        "Retained document scan evidence or original SHA differs",
    )
}
/// Scanner-off history remains valid. Retained clean rows must describe the exact registered original.
pub fn validate_backup(db: &Connection) -> Result<(), Error> {
    let mut query = db.prepare("SELECT s.id,s.document_id,s.sha256_hash,s.scanner_id,s.scanner_version,s.scanned_at,s.result,s.evidence_ref,d.sha256_hash FROM document_scan_evidence s LEFT JOIN documents d ON d.id=s.document_id ORDER BY s.id").map_err(|_| refused())?;
    let mut rows = query.query([]).map_err(|_| refused())?;
    while let Some(row) = rows.next().map_err(|_| refused())? {
        let id: i64 = row.get(0).map_err(|_| refused())?;
        let document_id: i64 = row.get(1).map_err(|_| refused())?;
        let evidence = ScanEvidence {
            sha256: row.get(2).map_err(|_| refused())?,
            scanner_id: row.get(3).map_err(|_| refused())?,
            scanner_version: row.get(4).map_err(|_| refused())?,
            evidence_ref: row.get(7).map_err(|_| refused())?,
        };
        evidence.validate()?;
        let at: String = row.get(5).map_err(|_| refused())?;
        let result: String = row.get(6).map_err(|_| refused())?;
        let original_hash: Option<String> = row.get(8).map_err(|_| refused())?;
        let timestamp = if at.len() == 19 && at.as_bytes()[10] == b' ' {
            format!("{}T{}.000Z", &at[..10], &at[11..])
        } else {
            at
        };
        if id <= 0
            || document_id <= 0
            || result != "clean"
            || original_hash.as_deref() != Some(evidence.sha256.as_str())
            || crate::cookie_session::timestamp(&timestamp).is_err()
        {
            return Err(refused());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests;
