//! Trusted producer receipts, separate from downloaded bytes, placement and offsite compliance.
//! Only an admitted host that durably retained and reread the signed artifact may append.
use rentemester_engine::Error;
use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all="camelCase")]
pub struct Company {
    pub slug: String,
    pub ledger_uuid: String,
    pub snapshot_sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all="camelCase")]
pub struct Snapshot {
    pub version: u8,
    pub workspace_id: String,
    pub archive_sha256: String,
    pub archive_size_bytes: u64,
    pub signing_public_key_sha256: String,
    pub created_at: String,
    pub created_at_ms: i64,
    pub companies: Vec<Company>,
}
fn refused() -> Error { Error::new("backup_inventory_unavailable","Trusted retained snapshot inventory is unavailable") }
fn hash(v:&str)->bool{v.len()==64 && v.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b))}
impl Snapshot {
    pub fn validate(&self)->Result<(),Error>{
        let timestamp = time::OffsetDateTime::parse(&self.created_at,&time::format_description::well_known::Rfc3339).map_err(|_|refused())?;
        let mut slugs=std::collections::BTreeSet::new();
        if self.version!=1 || self.workspace_id.is_empty() || self.workspace_id.len()>128
            || !hash(&self.archive_sha256) || !hash(&self.signing_public_key_sha256)
            || self.archive_size_bytes==0 || self.archive_size_bytes>96*1024*1024
            || self.created_at_ms<0 || timestamp.unix_timestamp_nanos()/1_000_000!=i128::from(self.created_at_ms)
            || self.companies.is_empty() || self.companies.len()>64
            || self.companies.iter().any(|c|c.slug.is_empty()||c.slug.len()>128||!slugs.insert(&c.slug)||c.ledger_uuid.is_empty()||c.ledger_uuid.len()>128||!hash(&c.snapshot_sha256))
        { return Err(refused()); }
        Ok(())
    }
}
pub fn append(tx:&Transaction<'_>,snapshot:&Snapshot,actor:&str)->Result<(),Error>{
    snapshot.validate()?;
    if !actor.starts_with("user:") || actor.len()>256 {return Err(refused());}
    let data=serde_json::to_string(snapshot).map_err(|_|refused())?;
    tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor,created_at)VALUES('native_backup_created','native_retained_snapshot',?,?,?)",params![data,actor,snapshot.created_at]).map_err(|_|refused())?;
    Ok(())
}
/// Historical receipts imported through restore/transfer are evidence, not a
/// claim that those source-local retained files exist on the new instance.
pub fn for_company(tx:&Transaction<'_>,workspace:&str,slug:&str,ledger_uuid:&str)->Result<Vec<Snapshot>,Error>{
    let mut q=tx.prepare("SELECT entity_id FROM workspace_audit WHERE event_type='native_backup_created' AND entity_type='native_retained_snapshot' AND id>COALESCE((SELECT max(id) FROM workspace_audit WHERE event_type='native_portable_restore'),0) ORDER BY id DESC").map_err(|_|refused())?;
    let mut snapshots=Vec::new();
    for row in q.query_map([],|r|r.get::<_,String>(0)).map_err(|_|refused())? {
        let wire=row.map_err(|_|refused())?;
        if wire.len()>65536{return Err(refused());}
        let s:Snapshot=serde_json::from_str(&wire).map_err(|_|refused())?;s.validate()?;
        if s.workspace_id==workspace && s.companies.iter().any(|c|c.slug==slug&&c.ledger_uuid==ledger_uuid){snapshots.push(s);}
    }
    Ok(snapshots)
}
#[cfg(test)]
mod tests {
 use super::*;
 fn value()->Snapshot{Snapshot{version:1,workspace_id:"synthetic-workspace".into(),archive_sha256:"a".repeat(64),archive_size_bytes:123,signing_public_key_sha256:"b".repeat(64),created_at:"2026-01-01T00:00:00Z".into(),created_at_ms:1767225600000,companies:vec![Company{slug:"synthetic".into(),ledger_uuid:"synthetic-uuid".into(),snapshot_sha256:"c".repeat(64)}]}}
 #[test]
 fn trusted_identity_scope_and_restore_boundary(){
  let mut c=rusqlite::Connection::open_in_memory().unwrap();let mut l=rusqlite::Connection::open_in_memory().unwrap();crate::fresh::initialize_empty_pair(&mut c,&mut l,"Synthetic backup").unwrap();
  let tx=c.transaction().unwrap();let s=value();append(&tx,&s,"user:synthetic").unwrap();assert_eq!(for_company(&tx,"synthetic-workspace","synthetic","synthetic-uuid").unwrap(),vec![s]);
  assert!(for_company(&tx,"synthetic-workspace","other","synthetic-uuid").unwrap().is_empty());assert!(for_company(&tx,"synthetic-workspace","synthetic","new-ledger").unwrap().is_empty());
  tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('native_portable_restore','backup','synthetic','system:portable-restore')",[]).unwrap();assert!(for_company(&tx,"synthetic-workspace","synthetic","synthetic-uuid").unwrap().is_empty());
  let mut s=value();s.archive_sha256="d".repeat(64);append(&tx,&s,"user:synthetic").unwrap();assert_eq!(for_company(&tx,"synthetic-workspace","synthetic","synthetic-uuid").unwrap(),vec![s]);
 }
 #[test]
 fn malformed_timestamp_duplicate_coverage_and_untrusted_actor_refuse(){let mut s=value();s.created_at_ms+=1;assert!(s.validate().is_err());let mut s=value();s.companies.push(s.companies[0].clone());assert!(s.validate().is_err());let s=value();let mut c=rusqlite::Connection::open_in_memory().unwrap();let tx=c.transaction().unwrap();assert!(append(&tx,&s,"agent:caller").is_err());}
}
