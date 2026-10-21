//! Company activity and explicit local lock preference. No filesystem/trust decisions.
use rentemester_engine::Error;
use rusqlite::{Transaction,params,OptionalExtension};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
fn invalid()->Error{Error::new("backup_state_unavailable","Backup activity or lock preference cannot be read")}
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields,rename_all="camelCase")]
pub struct Settings {pub version:u8,pub enforced:bool,pub grace_days:u32,pub restore_boundary:i64,pub updated_at:Option<String>,pub updated_by:Option<String>}
pub fn boundary(control:&Transaction<'_>)->Result<i64,Error>{control.query_row("SELECT COALESCE(MAX(id),0) FROM workspace_audit WHERE event_type='native_portable_restore'",[],|r|r.get(0)).map_err(|_|invalid())}
pub fn settings(control:&Transaction<'_>,company:&Transaction<'_>)->Result<Settings,Error>{
 let boundary=boundary(control)?;
 let wire:Option<String>=company.query_row("SELECT message FROM audit_log WHERE event_type='native_backup_lock' AND entity_type='backup_lock' ORDER BY id DESC LIMIT 1",[],|r|r.get(0)).optional().map_err(|_|invalid())?;
 let default=Settings{version:1,enforced:false,grace_days:0,restore_boundary:boundary,updated_at:None,updated_by:None};
 let Some(wire)=wire else{return Ok(default)};
 if wire.len()>4096{return Err(invalid());}
 let s:Settings=serde_json::from_str(&wire).map_err(|_|invalid())?;
 if s.version!=1||s.grace_days>365||s.restore_boundary<0{return Err(invalid());}
 if s.restore_boundary!=boundary{return Ok(default)}
 Ok(s)
}
pub fn configure(control:&Transaction<'_>,company:&Transaction<'_>,slug:&str,enforced:bool,grace_days:u32,actor:&str,at:&str)->Result<(),Error>{
 if grace_days>365||!actor.starts_with("user:"){return Err(invalid());}
 let s=Settings{version:1,enforced,grace_days,restore_boundary:boundary(control)?,updated_at:Some(at.into()),updated_by:Some(actor.into())};
 company.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES('native_backup_lock','backup_lock',?,?,?,?)",params![slug,serde_json::to_string(&s).map_err(|_|invalid())?,actor,at]).map_err(|_|invalid())?;Ok(())
}
fn moment(value:&str,end:bool)->Result<i64,Error>{
 use time::{OffsetDateTime,PrimitiveDateTime,Date,format_description::{self,well_known::Rfc3339}};
 let parsed=if value.len()==10{
   let fmt=format_description::parse_borrowed::<2>("[year]-[month]-[day]").map_err(|_|invalid())?;
   let d=Date::parse(value,&fmt).map_err(|_|invalid())?;
   d.with_time(if end {time::Time::from_hms_milli(23,59,59,999).map_err(|_|invalid())?}else{time::Time::MIDNIGHT}).assume_utc()
 }else if let Ok(d)=OffsetDateTime::parse(value,&Rfc3339){d}else{
   let fmt=format_description::parse_borrowed::<2>("[year]-[month]-[day] [hour]:[minute]:[second]").map_err(|_|invalid())?;
   PrimitiveDateTime::parse(value,&fmt).map_err(|_|invalid())?.assume_utc()
 };
 i64::try_from(parsed.unix_timestamp_nanos()/1_000_000).map_err(|_|invalid())
}
pub struct Activity{pub latest:Option<i64>,pub earliest:Option<i64>,pub evidence:Value}
pub fn activity(company:&Transaction<'_>)->Result<Activity,Error>{
 let mut latest=None;let mut earliest=None;let mut evidence=json!({});
 for (name,table,column) in [("latestJournalEntryAt","journal_entries","registration_datetime"),("latestDocumentAt","documents","upload_datetime"),("latestBankImportAt","bank_transactions","COALESCE(booking_date,transaction_date)")] {
  let (first,last):(Option<String>,Option<String>)=company.query_row(&format!("SELECT min({column}),max({column}) FROM {table}"),[],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|invalid())?;
  if let Some(v)=first{let n=moment(&v,false)?;earliest=Some(earliest.map_or(n,|old:i64|old.min(n)));}
  if let Some(v)=last.as_deref(){let n=moment(v,true)?;latest=Some(latest.map_or(n,|old:i64|old.max(n)));}
  evidence[name]=json!(last);
 }
 Ok(Activity{latest,earliest,evidence})
}
/// Append-only activity identities close the sub-second precision gap in legacy
/// registration timestamps. These counters are observed under the same writer.
pub fn activity_ids(company:&Transaction<'_>)->Result<[i64;3],Error>{Ok([
 company.query_row("SELECT COALESCE(MAX(id),0) FROM journal_entries",[],|r|r.get(0)).map_err(|_|invalid())?,
 company.query_row("SELECT COALESCE(MAX(id),0) FROM documents",[],|r|r.get(0)).map_err(|_|invalid())?,
 company.query_row("SELECT COALESCE(MAX(id),0) FROM bank_transactions",[],|r|r.get(0)).map_err(|_|invalid())?,
])}

#[cfg(test)]mod tests{use super::*;
#[test]fn date_only_activity_and_restore_settings_boundary(){assert_eq!(moment("2026-01-01",true).unwrap()-moment("2026-01-01",false).unwrap(),86399999);assert_eq!(moment("2026-01-01 00:00:00",false).unwrap(),moment("2026-01-01T00:00:00Z",false).unwrap());assert!(moment("invalid",false).is_err());let mut c=rusqlite::Connection::open_in_memory().unwrap();let mut l=rusqlite::Connection::open_in_memory().unwrap();crate::fresh::initialize_empty_pair(&mut c,&mut l,"Synthetic backup").unwrap();let ct=c.transaction().unwrap();let lt=l.transaction().unwrap();assert!(!settings(&ct,&lt).unwrap().enforced);configure(&ct,&lt,"synthetic",true,2,"user:synthetic","2026-01-01T00:00:00Z").unwrap();assert!(settings(&ct,&lt).unwrap().enforced);ct.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('native_portable_restore','backup','synthetic','system:portable-restore')",[]).unwrap();assert!(!settings(&ct,&lt).unwrap().enforced);assert!(configure(&ct,&lt,"synthetic",true,366,"user:synthetic","2026-01-01T00:00:00Z").is_err());}}
