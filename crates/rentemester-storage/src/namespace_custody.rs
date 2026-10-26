//! Physical native custody, deliberately absent from portable business tables.
use rentemester_engine::Error;
use rusqlite::{Connection, OptionalExtension, params};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity { pub device: u64, pub inode: u64 }
fn refused() -> Error { Error::new("namespace_custody_refused", "physical namespace custody missing or different") }
pub fn verify(db: &Connection, identity: Identity) -> Result<(), Error> {
    let row: Option<(String,String)> = db.query_row("SELECT device,inode FROM rm_namespace_custody WHERE id=1", [], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(|_| refused())?;
    if row != Some((identity.device.to_string(), identity.inode.to_string())) { return Err(refused()); }
    Ok(())
}
pub(crate) fn insert_or_verify(db: &Connection, identity: Identity) -> Result<(), Error> {
    let count: i64 = db.query_row("SELECT count(*) FROM rm_namespace_custody", [], |r| r.get(0)).map_err(|_| refused())?;
    if count == 0 {
        db.execute("INSERT INTO rm_namespace_custody(id,device,inode) VALUES(1,?1,?2)", params![identity.device.to_string(),identity.inode.to_string()]).map_err(|_| refused())?;
    }
    verify(db, identity)
}
/// Only trusted construction or explicit stopped configured operator maintenance.
pub fn initialize_private(db: &mut Connection, identity: Identity) -> Result<(), Error> { crate::control_migration::bind_namespace(db, identity, false) }
pub fn enroll_stopped(db: &mut Connection, identity: Identity) -> Result<(), Error> { crate::control_migration::bind_namespace(db, identity, true) }
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn namespace_custody_exact_missing_mismatch_never_overwrites() {
        let mut db = Connection::open_in_memory().unwrap();
        crate::control_migration::initialize_empty(&mut db).unwrap();
        let id = Identity { device: 1, inode: 2 };
        assert!(verify(&db,id).is_err());
        initialize_private(&mut db,id).unwrap();
        verify(&db,id).unwrap();
        let changes=db.total_changes();
        assert!(initialize_private(&mut db,Identity{device:1,inode:3}).is_err());
        assert_eq!(changes,db.total_changes());
        verify(&db,id).unwrap();
        assert!(db.execute("DELETE FROM rm_namespace_custody",[]).is_err());
        assert!(db.execute("UPDATE rm_namespace_custody SET inode='3'",[]).is_err());
    }
}
