//! Current company backup evidence and opt-in bookkeeping lock; recovery stays available.
use crate::{access::Permission,control_fence::{self,Committed,CookieAuthorization,Failure,SessionRequirement},documents::Context};
use rentemester_engine::Error;
use rentemester_runtime::auth::Clock;
use rusqlite::{Connection,Transaction};
use serde_json::{Value,json};
fn invalid()->Error{Error::new("backup_status_unavailable","Current retained backup evidence is unavailable")}
pub enum Action {Read,Configure{enforced:bool,grace_days:u32}}
fn timestamp(ms:i64)->Result<String,Error>{rentemester_runtime::backup_keys::timestamp(ms).map_err(|_|invalid())}
pub(crate) fn data(control:&Transaction<'_>,company:&Transaction<'_>,workspace:&str,slug:&str,clock:&dyn Clock,all:bool)->Result<Value,Error>{
 let now=clock.now_unix_ms().map_err(|_|invalid())?;
 data_at(control,company,workspace,slug,now,all)
}
/// Observation time is independent of the credential/write enforcement clock.
pub(crate) fn data_at(control:&Transaction<'_>,company:&Transaction<'_>,workspace:&str,slug:&str,now:i64,all:bool)->Result<Value,Error>{
 let settings=rentemester_storage::backup_state::settings(control,company)?;
 let activity=rentemester_storage::backup_state::activity(company)?;
 let uuid:String=company.query_row("SELECT ledger_uuid FROM ledger_identity WHERE id=1",[],|r|r.get(0)).map_err(|_|invalid())?;
 let mut inventory=rentemester_storage::backup_inventory::for_company(control,workspace,slug,&uuid)?;
 inventory.sort_by_key(|s|std::cmp::Reverse(s.created_at_ms));
 let path:String=control.query_row("SELECT file FROM pragma_database_list WHERE name='main'",[],|r|r.get(0)).map_err(|_|invalid())?;
 let mut latest=None;let mut found=0;let mut errors=vec![];let mut seen=std::collections::BTreeSet::new();
 for snapshot in inventory {
  if !seen.insert(snapshot.archive_sha256.clone()){continue;}
  let qualified=(||{
   let key:[u8;32]=snapshot.signing_public_key.as_deref().ok_or(())?.try_into().map_err(|_|())?;
   let key=rentemester_runtime::backup_artifact::TrustedPublicKey::from_bytes(&key).map_err(|_|())?;
   let receipt=rentemester_runtime::backup_destination::verify_retained(std::path::Path::new(&path),&snapshot.archive_sha256,&key,workspace).map_err(|_|())?;
   if receipt["archiveSha256"]!=snapshot.archive_sha256 || receipt["archiveSizeBytes"]!=snapshot.archive_size_bytes || receipt["backupCreatedAt"]!=snapshot.created_at {return Err(());}
   Ok(())
  })();
  if qualified.is_ok(){found+=1;if latest.is_none(){latest=Some(snapshot);}
   if !all{break;}}
  else{errors.push("En registreret lokal backup kunne ikke genlæses og verificeres.".to_string());}
 }
 let current_ids=rentemester_storage::backup_state::activity_ids(company)?;
 let changed=latest.as_ref().and_then(|s|s.companies.iter().find(|c|c.slug==slug)).and_then(|c|c.activity_max_ids).is_some_and(|old|old!=current_ids);
 let policy=rentemester_engine::backup_policy::evaluate_observed(now,latest.as_ref().map(|s|s.created_at_ms),activity.latest,activity.earliest,settings.enforced,settings.grace_days,changed)?;
 let deadline=policy.lock_deadline_ms.map(timestamp).transpose()?;
 let required=policy.required_by_ms.map(timestamp).transpose()?;
 Ok(json!({"ok":true,"slug":slug,"backup":{"available":true,"ok":!policy.backup_due && errors.is_empty(),"latestBackupAt":latest.as_ref().map(|s|&s.created_at),"latestBackupId":latest.as_ref().map(|s|&s.archive_sha256),"backupDue":policy.backup_due,"hasActivitySinceBackup":policy.has_activity_since_backup,"daysSinceLatestBackup":policy.days_since_latest_backup,"backupsFound":found,"requiredBy":required,"checkedAt":timestamp(now)?,"evidence":{"timestamps":activity.evidence,"identityChangedSinceBackup":changed},"errors":errors,"scope":"local-retained-snapshots","remoteSyncVerified":false},"lock":{"enforced":settings.enforced,"graceDays":settings.grace_days,"locked":policy.locked,"lockDeadline":deadline,"updatedAt":settings.updated_at,"updatedBy":settings.updated_by}}))
}
pub fn require_write(control:&Transaction<'_>,company:&Transaction<'_>,workspace:&str,slug:&str,clock:&dyn Clock)->Result<(),Error>{
 if !rentemester_storage::backup_state::settings(control,company)?.enforced{return Ok(());}
 if data(control,company,workspace,slug,clock,false)?["lock"]["locked"]==true{return Err(Error::new("backup_overdue","Bookkeeping is locked by the explicitly enabled backup policy; create a backup or review the lock setting before a new action"));}
 Ok(())
}
pub fn execute(control:&mut Connection,company:&mut Connection,context:Context<'_>,action:Action)->Result<Committed<Value>,Failure>{
 let workspace=context.selection.workspace;let slug=context.selection.slug;let clock=context.clock;
 let read=matches!(action,Action::Read);
 control_fence::execute_cookie_both(control,company,CookieAuthorization{cookie:context.cookie,clock,requirement:SessionRequirement::Current},context.selection,Permission::CompanyAdmin,read,true,|control,company,subject|{
  if let Action::Configure{enforced,grace_days}=action {rentemester_storage::backup_state::configure(control,company,slug,enforced,grace_days,&format!("user:{subject}"),&timestamp(clock.now_unix_ms().map_err(|_|invalid())?)?)?;}
  data(control,company,workspace,slug,clock,true)
 })
}


/// Trusted host observation of all affected ledgers under the current control writer.
/// Implementations must use admitted company handles and restore the owner's selection.
pub trait CompanyScopes {
    fn require(&mut self, control: &Transaction<'_>, clock: &dyn Clock, slugs: &[String]) -> Result<(), Error>;
}
#[cfg(test)]
pub(crate) struct UnlockedTestScopes;
#[cfg(test)]
impl CompanyScopes for UnlockedTestScopes {
    fn require(&mut self, _: &Transaction<'_>, _: &dyn Clock, _: &[String]) -> Result<(), Error> { Ok(()) }
}

#[cfg(test)]mod tests;
