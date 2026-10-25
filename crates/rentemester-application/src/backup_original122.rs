//! Source-compatible backup projections on the common retained-artifact producer.
use crate::{access::Permission, backup_status, control_fence::{self,Committed,CookieAuthorization,Failure,SessionRequirement}, documents::Context, service_company};
use rentemester_engine::Error;
use rentemester_runtime::auth::Clock;
use rusqlite::{Connection,Transaction};
use serde::Deserialize;
use serde_json::{Value,json};
const RULE:&str="DK-BOOKKEEPING-BACKUP-001";
#[derive(Deserialize)]
#[serde(deny_unknown_fields,rename_all="camelCase")]
pub struct Input {
 pub confirm:bool,
 #[serde(default,deserialize_with="present")] pub enforced:Option<bool>,
 #[serde(default,deserialize_with="present")] pub grace_days:Option<serde_json::Number>,
 #[serde(default,deserialize_with="present")] pub at:Option<String>
}
fn present<'de,D,T>(deserializer:D)->Result<Option<T>,D::Error> where D:serde::Deserializer<'de>,T:Deserialize<'de>{T::deserialize(deserializer).map(Some)}
pub enum Action {Status(Option<String>),Configure(Input)}
fn invalid()->Error {Error::new("backup_status_unavailable","Backup observation unavailable")}
fn timestamp(ms:i64)->Result<String,Error>{rentemester_runtime::backup_keys::timestamp(ms).map_err(|_|invalid())}
fn refusal(message:&str)->Value{json!({"ok":false,"appliedRules":[RULE],"errors":[message]})}
fn apply(c:&Transaction<'_>,l:&Transaction<'_>,workspace:&str,slug:&str,clock:&dyn Clock,actor:&str,action:Action)->Result<Value,Error>{
 let now=clock.now_unix_ms().map_err(|_|invalid())?;
 match action {
 Action::Status(as_of)=>{
 let observed=match as_of.as_deref(){None=>Some(now),Some(v)=>rentemester_storage::backup_state::original122_moment(v).ok()};
 let Some(observed)=observed else{return Ok(json!({"ok":false,"appliedRules":[RULE],"latestBackupAt":null,"latestBackupId":null,"backupDue":false,"hasActivitySinceBackup":false,"daysSinceLatestBackup":null,"requiredBy":null,"checkedAt":as_of.unwrap_or_default(),"backupsFound":0,"evidence":{"latestJournalEntryAt":null,"latestDocumentAt":null,"latestBankImportAt":null},"errors":["asOf must be a valid ISO-8601 datetime when provided"]}));};
 let native=backup_status::data_at(c,l,workspace,slug,observed,true)?;
 let mut core=native["backup"].clone();
 core.as_object_mut().ok_or_else(invalid)?.retain(|k,_|matches!(k.as_str(),"ok"|"latestBackupAt"|"latestBackupId"|"backupDue"|"hasActivitySinceBackup"|"daysSinceLatestBackup"|"requiredBy"|"checkedAt"|"backupsFound"|"errors"));
 core["evidence"]=native["backup"]["evidence"]["timestamps"].clone();core["appliedRules"]=json!([RULE]);Ok(core)
 },
 Action::Configure(input)=>{
 if !input.confirm{return Ok(refusal("confirm must be true"));}
 let at=match input.at.as_deref(){None=>now,Some(v)=>match rentemester_storage::backup_state::original122_moment(v){Ok(n)=>n,Err(_)=>return Ok(refusal("at must be a valid ISO-8601 datetime when provided"))}};
 let current=rentemester_storage::backup_state::settings(c,l)?;
 let grace=match input.grace_days{None=>if current.updated_at.is_some(){current.grace_days}else{3},Some(n)=>match n.as_f64(){Some(n) if n.is_finite() && n>=0.0 && n.fract()==0.0 && n<=u32::MAX as f64=>n as u32,_=>return Ok(refusal("graceDays must be a non-negative integer"))}};
 // Invoke the one pure policy validator, including Date deadline bounds even
 // when enforcement is being disabled or the company has no activity yet.
 let activity=rentemester_storage::backup_state::activity(l)?;
 if rentemester_engine::backup_policy::evaluate(now,None,Some(now),Some(now),true,grace).is_err() || rentemester_engine::backup_policy::evaluate(now,None,activity.latest,activity.earliest,true,grace).is_err(){return Ok(refusal("graceDays exceeds the supported backup deadline range"));}
 let enforced=input.enforced.unwrap_or(current.enforced);
 let at=timestamp(at)?;
 rentemester_storage::backup_state::configure_original122(c,l,slug,enforced,grace,actor,&at,&timestamp(now)?)?;
 Ok(json!({"ok":true,"config":{"version":"backup-lock-v1","enforced":enforced,"graceDays":grace,"updatedAt":at,"updatedBy":actor},"appliedRules":[RULE],"errors":[]}))
 }
 }
}
pub fn human(c:&mut Connection,l:&mut Connection,ctx:Context<'_>,action:Action)->Result<Committed<Value>,Failure>{
 let read=matches!(action,Action::Status(_));let workspace=ctx.selection.workspace;let slug=ctx.selection.slug;let clock=ctx.clock;
 control_fence::execute_cookie_both(c,l,CookieAuthorization{cookie:ctx.cookie,clock,requirement:SessionRequirement::Current},ctx.selection,if read{Permission::CompanyRead}else{Permission::CompanyAdmin},read,true,|c,l,subject|apply(c,l,workspace,slug,clock,&format!("user:{subject}"),action))
}
pub fn service(c:&mut Connection,l:&mut Connection,ctx:service_company::Context<'_>,action:Action)->Result<service_company::ServiceResult<Value>,service_company::ServiceFailure>{
 let read=matches!(action,Action::Status(_));let workspace=ctx.selection.workspace;let slug=ctx.selection.slug;let clock=ctx.clock;
 let callback=|c:&Transaction<'_>,l:&Transaction<'_>,subject:&str,_:&mut Option<Value>|Ok(service_company::OperationResult{result:apply(c,l,workspace,slug,clock,&format!("user:{subject}"),action)?,receipt:None});
 if read{service_company::execute_operation_both(c,l,ctx,Permission::CompanyRead,false,callback)}else{service_company::execute_recovery_both(c,l,ctx,Permission::CompanyAdmin,callback)}
}
#[cfg(test)]
#[path="backup_original122_tests.rs"]mod tests;
pub mod original717;
