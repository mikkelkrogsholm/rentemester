use super::*;
use rentemester_runtime::auth::{AuthError,Clock,CookieProfile,CookieVerifier};
struct Fixed;impl Clock for Fixed{fn now_unix_ms(&self)->Result<i64,AuthError>{Ok(1768867200000)}}
#[test]
fn explicit_lock_refuses_before_effects_but_read_and_recovery_and_disable_remain_available(){
 let dir=tempfile::tempdir().unwrap();let mut c=Connection::open(dir.path().join("control.sqlite")).unwrap();let mut l=Connection::open(dir.path().join("company.sqlite")).unwrap();rentemester_storage::fresh::initialize_empty_pair(&mut c,&mut l,"Synthetic lock").unwrap();
 c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-20T00:00:00.000Z','2026-01-20T00:00:00.000Z','2026-01-20T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');").unwrap();
 l.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('B','Synthetic bank','asset','debit');INSERT INTO bank_accounts(id,slug,name,currency,ledger_account_no)VALUES(1,'synthetic','Synthetic','DKK','B');INSERT INTO bank_transactions(transaction_date,text,amount,currency,bank_account_id)VALUES('2026-01-01','Synthetic',1,'DKK',1)").unwrap();
 let cookies=CookieVerifier::new("w",CookieProfile::Secure,"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap();let issued=cookies.issue_session("synthetic-token").unwrap();let proof=cookies.verify(Some(issued.request_header()),None).unwrap();
 let context=||Context{cookie:&proof,clock:&Fixed,selection:control_fence::CompanySelection{workspace:"w",slug:"a",registered:true,archived:false}};
 assert_eq!(execute(&mut c,&mut l,context(),Action::Read).unwrap().value["lock"]["enforced"],false);
 let value=execute(&mut c,&mut l,context(),Action::Configure{enforced:true,grace_days:0}).unwrap().value;assert_eq!(value["lock"]["locked"],true);let changes=l.total_changes();
 let auth=||CookieAuthorization{cookie:&proof,clock:&Fixed,requirement:SessionRequirement::Current};
 let failed=control_fence::execute_cookie_subject(&mut c,&mut l,auth(),context().selection,Permission::CompanyLedgerPost,|tx,_|{tx.execute("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('N','Must not exist','asset','debit')",[]).unwrap();Ok(())}).unwrap_err();assert_eq!(failed.error.code,"backup_overdue");assert_eq!(failed.stage,control_fence::FailureStage::NotCommitted);assert_eq!(l.total_changes(),changes);
 assert_eq!(execute(&mut c,&mut l,context(),Action::Read).unwrap().value["backup"]["backupDue"],true);
 control_fence::execute_cookie_recovery(&mut c,&mut l,auth(),context().selection,Permission::CompanyExternalSend,|_,_|Ok(())).unwrap();
 execute(&mut c,&mut l,context(),Action::Configure{enforced:false,grace_days:0}).unwrap();control_fence::execute_cookie_subject(&mut c,&mut l,auth(),context().selection,Permission::CompanyLedgerPost,|_,_|Ok(())).unwrap();
 c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u')").unwrap();assert_eq!(execute(&mut c,&mut l,context(),Action::Configure{enforced:true,grace_days:0}).unwrap_err().error.code,"access_denied");
}
