use super::*;
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier, VerifiedCookie};
use rentemester_storage::workspace_context::Catalog;
use rentemester_storage::workspace_context::RegisteredCompany;
struct Fixed(i64);
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.0)
    }
}
const NOW: i64 = 1767225600000;
fn fixture() -> (
    tempfile::TempDir,
    Connection,
    Connection,
    CookieVerifier,
    VerifiedCookie,
    Catalog,
) {
    let dir = tempfile::tempdir().unwrap();
    let mut control = Connection::open(dir.path().join("control.sqlite")).unwrap();
    let mut ledger = Connection::open(dir.path().join("ledger.sqlite")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut ledger,
        "Synthetic diagnostics",
    )
    .unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','diagnostic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','b','grant','reader','user:u');").unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let issued = cookies.issue_session("diagnostic-token").unwrap();
    let proof = cookies
        .verify(Some(issued.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    let catalog = Catalog {
        workspace: "w".into(),
        companies: ["a", "b"]
            .into_iter()
            .map(|slug| RegisteredCompany {
                slug: slug.into(),
                name: format!("Synthetic {slug}"),
                purpose: Default::default(), archived: false,
            })
            .collect(),
    };
    (dir, control, ledger, cookies, proof, catalog)
}

fn context<'a>(p:&'a VerifiedCookie,clock:&'a dyn Clock)->Context<'a>{Context{cookie:p,clock,selection:control_fence::CompanySelection{workspace:"w",slug:"a",registered:true,archived:false}}}
#[test]
fn backup_original122_current_read_and_expiry_never_follow_as_of(){
 let (_d,mut c,mut l,_,p,_)=fixture();
 let before=l.total_changes();
 let core=human(&mut c,&mut l,context(&p,&Fixed(NOW)),Action::Status(Some("2035-01-01".into()))).unwrap().value;
 assert_eq!(core["checkedAt"],"2035-01-01T00:00:00.000Z");assert_eq!(core["evidence"].as_object().unwrap().len(),3);assert_eq!(core.as_object().unwrap().len(),12);
 let bad=human(&mut c,&mut l,context(&p,&Fixed(NOW)),Action::Status(Some("invalid".into()))).unwrap().value;
 assert_eq!(bad["ok"],false);assert_eq!(bad["checkedAt"],"invalid");assert_eq!(l.total_changes(),before);
 assert!(human(&mut c,&mut l,context(&p,&Fixed(NOW+86400000)),Action::Status(Some("2026-01-01".into()))).is_err());
}
fn input(value:Value)->Action{Action::Configure(serde_json::from_value(value).unwrap())}
#[test]
fn backup_original122_merge_default_zero_cap_refusal_and_real_audit(){
 let (_d,mut c,mut l,_,p,_)=fixture();
 assert!(human(&mut c,&mut l,context(&p,&Fixed(NOW)),input(json!({"confirm":true}))).is_err());
 c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u')").unwrap();
 let core=human(&mut c,&mut l,context(&p,&Fixed(NOW)),input(json!({"confirm":true,"at":"2020-01-01"}))).unwrap().value;
 assert_eq!(core["config"]["graceDays"],3);assert_eq!(core["config"]["updatedBy"],"user:u");
 let audit:String=l.query_row("SELECT created_at FROM audit_log WHERE event_type='native_backup_lock' ORDER BY id DESC LIMIT 1",[],|r|r.get(0)).unwrap();assert_eq!(audit,"2026-01-01T00:00:00.000Z");
 for grace in [0,366]{assert_eq!(human(&mut c,&mut l,context(&p,&Fixed(NOW)),input(json!({"confirm":true,"graceDays":grace}))).unwrap().value["ok"],true);}
 let count:i64=l.query_row("SELECT count(*) FROM audit_log",[],|r|r.get(0)).unwrap();
 for bad in [json!({"confirm":true,"graceDays":-1}),json!({"confirm":true,"graceDays":1.2}),json!({"confirm":true,"graceDays":4294967296u64}),json!({"confirm":true,"graceDays":4294967295u64}),json!({"confirm":true,"at":"bad"})]{assert_eq!(human(&mut c,&mut l,context(&p,&Fixed(NOW)),input(bad)).unwrap().value["ok"],false);}
 assert_eq!(l.query_row("SELECT count(*) FROM audit_log",[],|r|r.get::<_,i64>(0)).unwrap(),count);
 let ct=c.transaction().unwrap();let lt=l.transaction().unwrap();assert_eq!(rentemester_storage::backup_state::settings(&ct,&lt).unwrap().grace_days,366);assert!(rentemester_storage::backup_state::configure(&ct,&lt,"a",false,366,"user:u","2026-01-01").is_err());
}

#[test]
fn backup_original122_service_admin_recovery_actor_charge_isolation_and_revocation(){
 use rentemester_runtime::service_key::ServiceKey;
 let (_d,mut c,mut l,_,_,_)=fixture();
 let key=ServiceKey::new("w","synthetic-backup-service-key").unwrap();
 c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('svc','Synthetic service','svc@example.invalid',1,0,'2026-01-01','2026-01-01');INSERT INTO rm_workspace_service_principals(user_id,display_name,created_at)VALUES('svc','Synthetic service','2026-01-01T00:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('svc','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('svc','a','grant','owner','user:u');").unwrap();
 c.execute("INSERT INTO apikey(id,configId,name,start,referenceId,prefix,key,enabled,rateLimitEnabled,rateLimitTimeWindow,rateLimitMax,requestCount,remaining,createdAt,updatedAt) VALUES('backup-key','workspace-service-principal','Synthetic','rms_sy','svc','rms_',?1,1,1,60000,600,0,8,'2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z')",[key.lookup_hash()]).unwrap();
 l.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('B','Synthetic bank','asset','debit');INSERT INTO bank_accounts(id,slug,name,currency,ledger_account_no)VALUES(1,'synthetic','Synthetic','DKK','B');INSERT INTO bank_transactions(transaction_date,text,amount,currency,bank_account_id)VALUES('2025-12-01','Synthetic',1,'DKK',1)").unwrap();
 let clock=Fixed(NOW);
 let ctx=|slug|service_company::Context{key:&key,clock:&clock,selection:control_fence::CompanySelection{workspace:"w",slug,registered:true,archived:false},max_future_days:0};
 let one=service(&mut c,&mut l,ctx("a"),input(json!({"confirm":true,"enforced":true,"graceDays":0,"at":"2020-01-01"}))).unwrap();
 assert_eq!(one.result["config"]["updatedBy"],"user:svc");
 {let ct=c.transaction().unwrap();let lt=l.transaction().unwrap();assert_eq!(backup_status::require_write(&ct,&lt,"w","a",&clock).unwrap_err().code,"backup_overdue");}
 let two=service(&mut c,&mut l,ctx("a"),input(json!({"confirm":true,"enforced":false}))).unwrap();
 assert_eq!(two.result["config"]["graceDays"],0);assert_eq!(two.result["config"]["enforced"],false);
 let audit:(String,String)=l.query_row("SELECT actor,created_at FROM audit_log WHERE event_type='native_backup_lock' ORDER BY id DESC LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();assert_eq!(audit,("user:svc".into(),"2026-01-01T00:00:00.000Z".into()));
 let count=l.total_changes();
 assert!(service(&mut c,&mut l,ctx("b"),input(json!({"confirm":true}))).is_err());
 let usage:i64=c.query_row("SELECT requestCount FROM apikey WHERE id='backup-key'",[],|r|r.get(0)).unwrap();assert_eq!(usage,2);
 c.execute("UPDATE apikey SET enabled=0 WHERE id='backup-key'",[]).unwrap();
 assert!(service(&mut c,&mut l,ctx("a"),input(json!({"confirm":true,"at":"2020-01-01"}))).is_err());assert_eq!(l.total_changes(),count);
}
