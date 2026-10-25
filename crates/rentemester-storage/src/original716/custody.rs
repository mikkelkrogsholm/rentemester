//! Source rotation audit on the admitted company transaction, never an independent writer.
pub fn audit_rotation(
    tx: &rusqlite::Transaction<'_>,
    actor: &str,
    at: &str,
    reason: &str,
    old: &str,
    new: &str,
) -> Result<(), rentemester_engine::Error> {
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at) VALUES('backup_keypair_rotated','backup',?,?,?,?)", rusqlite::params![new,format!("Ed25519 backup signing key rotated. Old fingerprint {old}, new fingerprint {new}. Reason: {}. (rule DK-BOOKKEEPING-BACKUP-KEY-ROTATE-001)",reason.trim()),format!("user:{actor}"),at])
        .map_err(|_| rentemester_engine::Error::new("backup_custody_unavailable", "Backup custody audit unavailable; reconcile before another deliberate attempt"))?;
    Ok(())
}
