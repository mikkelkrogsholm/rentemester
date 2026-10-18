use super::*;
use rentemester_runtime::auth::{AuthError, CookieProfile};
use serde_json::{Value, json};
struct FixedClock(i64);
impl Clock for FixedClock {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.0)
    }
}
#[test]
fn pending_bearer_is_not_a_session_and_binds_signer_profile_key_workspace() {
    let key = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
    let v = CookieVerifier::new("w", CookieProfile::Secure, key).unwrap();
    let token = new_challenge_token().unwrap();
    let cookie = v.issue_challenge(token.as_str()).unwrap();
    assert!(cookie.set_cookie().contains("; Secure"));
    assert!(
        cookie
            .set_cookie()
            .contains("HttpOnly; SameSite=Lax; Max-Age=600")
    );
    assert!(v.verify(Some(cookie.request_header()), None).is_err());
    let proof = v
        .verify_challenge(Some(cookie.request_header()), None)
        .unwrap();
    assert!(v.accepts_challenge(&proof));
    assert!(
        !CookieVerifier::new("other", CookieProfile::Secure, key)
            .unwrap()
            .accepts_challenge(&proof)
    );
    assert!(
        !CookieVerifier::new("w", CookieProfile::Development, key)
            .unwrap()
            .accepts_challenge(&proof)
    );
    assert!(
        !CookieVerifier::new(
            "w",
            CookieProfile::Secure,
            "YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXoxMjM0NTY"
        )
        .unwrap()
        .accepts_challenge(&proof)
    );
    assert!(
        v.verify_challenge(Some(cookie.request_header()), Some(""))
            .is_err()
    );
    let duplicate = format!("{}; {}", cookie.request_header(), cookie.request_header());
    assert!(v.verify_challenge(Some(&duplicate), None).is_err());
}
#[test]
#[ignore = "synthetic enrolled MFA login process helper selected by independent harness"]
fn probe() {
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let request: Value = serde_json::from_str(&input).unwrap();
    let mut db = Connection::open(request["control"].as_str().unwrap()).unwrap();
    // Opening/configuring concurrent synthetic handles can need a schema read.
    // Only fixture setup waits; the operation under test retains zero wait.
    db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;")
        .unwrap();
    db.busy_timeout(std::time::Duration::ZERO).unwrap();
    if let Some(sql) = request["preflight"].as_str() {
        db.execute_batch(sql).unwrap();
    }
    let key = request["key"].as_str().unwrap();
    let cookies = CookieVerifier::new(
        request["workspace"].as_str().unwrap_or("w"),
        if request["development"] == true {
            CookieProfile::Development
        } else {
            CookieProfile::Secure
        },
        key,
    )
    .unwrap();
    let keys = MfaKeyring::new(&[(1, key)], None).unwrap();
    let clock = FixedClock(request["now"].as_i64().unwrap());
    let runtime = Runtime {
        cookies: &cookies,
        keys: &keys,
        clock: &clock,
        audit_key_version: 1,
    };
    if matches!(
        request["mode"].as_str(),
        Some("email-issue" | "email-complete")
    ) {
        let signer = rentemester_runtime::email_token::EmailTokenSigner::new(
            request["workspace"].as_str().unwrap_or("w"),
            key,
        )
        .unwrap();
        let runtime = crate::email_verification::Runtime {
            signer: &signer,
            clock: &clock,
            audit_key_version: 1,
        };
        let result=if request["mode"]=="email-issue" {
            crate::email_verification::issue(&mut db,&runtime,request["email"].as_str().unwrap_or("u@example.invalid")).map(|token|json!({"ok":true,"token":token.as_ref().map(|t|t.as_str())}))
        } else {
            crate::email_verification::complete(&mut db,&runtime,request["token"].as_str().unwrap_or("")).map(|()|json!({"ok":true}))
        }.unwrap_or_else(|f|json!({"ok":false,"code":f.error.code,"stage":format!("{:?}",f.stage),"reusable":f.connection_reusable}));
        println!("RM_MFA_LOGIN_RESULT={result}");
        return;
    }
    if matches!(
        request["mode"].as_str(),
        Some("reset-request" | "reset-callback")
    ) {
        use rentemester_runtime::recovery::{
            DeliveryUnconfirmed, RecoveryLinks, ResetMessage, ResetSender,
        };
        struct Sender {
            enabled: bool,
            fail: bool,
            path: String,
            marker: Option<String>,
            sent: Vec<Value>,
        }
        impl ResetSender for Sender {
            fn enabled(&self) -> bool {
                self.enabled
            }
            fn send(&mut self, message: &ResetMessage<'_>) -> Result<(), DeliveryUnconfirmed> {
                let other = Connection::open(&self.path).unwrap();
                other.busy_timeout(std::time::Duration::ZERO).unwrap();
                other
                    .execute_batch("BEGIN IMMEDIATE;ROLLBACK")
                    .expect("mail called while writer held");
                self.sent.push(json!({"recipient":message.recipient(),"token":message.token(),"url":message.action_url()}));
                if let Some(marker) = &self.marker {
                    std::fs::write(marker, "called").unwrap();
                }
                if self.fail {
                    Err(DeliveryUnconfirmed)
                } else {
                    Ok(())
                }
            }
        }
        let origins: Vec<&str> = request["trustedOrigins"]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_str().unwrap()).collect())
            .unwrap_or(vec!["https://example.invalid"]);
        let links = match RecoveryLinks::new(
            request["baseOrigin"]
                .as_str()
                .unwrap_or("https://example.invalid"),
            &origins,
            if request["development"] == true {
                CookieProfile::Development
            } else {
                CookieProfile::Secure
            },
        ) {
            Ok(l) => l,
            Err(e) => {
                println!(
                    "RM_MFA_LOGIN_RESULT={}",
                    json!({"ok":false,"code":e.code(),"stage":"NotCommitted","reusable":true,"sent":[]})
                );
                return;
            }
        };
        let runtime = crate::reset_request::Runtime {
            cookies: &cookies,
            clock: &clock,
            links: &links,
            audit_key_version: request["auditVersion"].as_u64().unwrap_or(1),
        };
        let mut sender = Sender {
            enabled: request["disabled"] != true,
            fail: request["deliveryFail"] == true,
            path: request["control"].as_str().unwrap().to_owned(),
            marker: request["mailMarker"].as_str().map(str::to_owned),
            sent: vec![],
        };
        let mut output = if request["mode"] == "reset-request" {
            crate::reset_request::execute(&mut db,&runtime,&mut sender,request["email"].as_str().unwrap_or("u@example.invalid"),request["redirect"].as_str()).map(|()|json!({"ok":true})).unwrap_or_else(|f|json!({"ok":false,"code":f.error.code,"stage":format!("{:?}",f.stage),"reusable":f.connection_reusable}))
        } else {
            crate::reset_request::callback(&mut db,&runtime,request["token"].as_str().unwrap_or(""),request["redirect"].as_str()).map(|url|json!({"ok":true,"url":url.as_str()})).unwrap_or_else(|f|json!({"ok":false,"code":f.error.code,"stage":format!("{:?}",f.stage),"reusable":f.connection_reusable}))
        };
        output["sent"] = json!(sender.sent);
        println!("RM_MFA_LOGIN_RESULT={output}");
        return;
    }
    let result = if request["mode"] == "password-reset" {
        let runtime = crate::password_reset::Runtime {
            clock: &clock,
            audit_key_version: request["auditVersion"].as_u64().unwrap_or(1),
        };
        crate::password_reset::execute(
            &mut db,
            &runtime,
            request["token"].as_str().unwrap_or(""),
            request["newPassword"].as_str().unwrap_or(""),
        )
        .map(|()| json!({"ok":true}))
    } else if request["mode"]
        .as_str()
        .is_some_and(|s| s.starts_with("session-"))
    {
        let signer = CookieVerifier::new(
            request["proofWorkspace"].as_str().unwrap_or("w"),
            if request["proofDevelopment"] == true {
                CookieProfile::Development
            } else {
                CookieProfile::Secure
            },
            request["proofKey"].as_str().unwrap_or(key),
        )
        .unwrap();
        match signer.verify(request["cookie"].as_str(), request["service"].as_str()) {
            Err(e) => Err(failure(error(e.code()), true)),
            Ok(proof) => {
                use crate::session_control::{self as sessions, Operation};
                let op = match request["mode"].as_str().unwrap() {
                    "session-list" => Operation::List,
                    "session-out" => Operation::SignOut,
                    "session-one" => Operation::RevokeOne(request["target"].as_str().unwrap_or("")),
                    "session-others" => Operation::RevokeOthers,
                    "session-all" => Operation::RevokeAll,
                    _ => panic!("unknown test operation"),
                };
                let runtime = sessions::Runtime {
                    cookies: &cookies,
                    clock: &clock,
                    audit_key_version: 1,
                };
                sessions::execute(&mut db,&runtime,&proof,op).map(|o|json!({"ok":true,"sessions":o.sessions,"deleted":o.deleted,"clearCookies":o.clear_cookies}))
            }
        }
    } else if request["mode"] == "codes-view" || request["mode"] == "codes-regenerate" {
        let signer = CookieVerifier::new(
            request["proofWorkspace"].as_str().unwrap_or("w"),
            if request["proofDevelopment"] == true {
                CookieProfile::Development
            } else {
                CookieProfile::Secure
            },
            request["proofKey"].as_str().unwrap_or(key),
        )
        .unwrap();
        match signer.verify(request["cookie"].as_str(), request["service"].as_str()) {
            Err(e) => Err(failure(error(e.code()), true)),
            Ok(proof) => crate::backup_management::execute(
                &mut db,
                &runtime,
                &proof,
                request["password"]
                    .as_str()
                    .unwrap_or("Synthetic password 123!"),
                if request["mode"] == "codes-view" {
                    crate::backup_management::Operation::View
                } else {
                    crate::backup_management::Operation::Regenerate
                },
            )
            .map(|codes| json!({"ok":true,"codes":codes.iter().collect::<Vec<_>>()})),
        }
    } else if request["mode"] == "enroll-begin" {
        crate::mfa_enrollment::begin(&mut db,&runtime,request["email"].as_str().unwrap_or("u@example.invalid"),request["password"].as_str().unwrap_or("Synthetic password 123!"),request["issuer"].as_str().unwrap_or("Rentemester"))
            .map(|s|json!({"ok":true,"pending":true,"cookie":s.cookie.request_header(),"uri":s.uri.as_str()}))
    } else if request["mode"] == "enroll-confirm" {
        match cookies.verify_challenge(request["cookie"].as_str(),request["service"].as_str()) {
            Err(e)=>Err(failure(error(e.code()),true)),
            Ok(proof)=>crate::mfa_enrollment::confirm(&mut db,&runtime,&proof,request["code"].as_str().unwrap_or(""))
                .map(|o|match o {
                    crate::mfa_enrollment::ConfirmOutcome::Completed{cookie,codes}=>{
                        let proof=cookies.verify(Some(cookie.request_header()),None).unwrap();
                        let tx=db.transaction_with_behavior(TransactionBehavior::Immediate).unwrap();
                        let s=rentemester_storage::cookie_session::resolve(&tx,proof.token(),clock.0,rentemester_storage::cookie_session::SessionRequirement::Fresh).unwrap();
                        assert_eq!(s.subject,"u");tx.rollback().unwrap();
                        json!({"ok":true,"session":true,"nativeCurrentFresh":true,"cookie":cookie.request_header(),"codes":codes.iter().collect::<Vec<_>>()})
                    },
                    crate::mfa_enrollment::ConfirmOutcome::Rejected(r)=>json!({"ok":true,"rejected":format!("{r:?}")}),
                })
        }
    } else if request["mode"] == "begin" {
        begin(&mut db,&runtime,request["email"].as_str().unwrap_or("u@example.invalid"),request["password"].as_str().unwrap_or("Synthetic password 123!")).map(|o|match o{BeginOutcome::Challenge(c)=>json!({"ok":true,"cookie":c.request_header(),"setCookie":c.set_cookie(),"pending":true}),BeginOutcome::EnrollmentRequired=>json!({"ok":true,"enrollmentRequired":true})})
    } else {
        match cookies.verify_challenge(request["cookie"].as_str(),request["service"].as_str()){
   Err(e)=>Err(failure(error(e.code()),true)),
   Ok(proof)=>(if request["backup"]==true { verify_backup(&mut db,&runtime,&proof,request["code"].as_str().unwrap_or("")) } else { verify(&mut db,&runtime,&proof,request["code"].as_str().unwrap_or("")) }).map(|o|match o{VerifyOutcome::Session(c)=>{
    let proof=cookies.verify(Some(c.request_header()),None).unwrap();let tx=db.transaction_with_behavior(TransactionBehavior::Immediate).unwrap();let resolved=rentemester_storage::cookie_session::resolve(&tx,proof.token(),clock.0,rentemester_storage::cookie_session::SessionRequirement::Fresh).unwrap();assert!(resolved.subject=="u");tx.rollback().unwrap();
    json!({"ok":true,"cookie":c.request_header(),"setCookie":c.set_cookie(),"session":true,"nativeCurrentFresh":true})},VerifyOutcome::Rejected(r)=>json!({"ok":true,"rejected":format!("{r:?}")})})
  }
    };
    let output=result.unwrap_or_else(|f|json!({"ok":false,"code":f.error.code,"stage":format!("{:?}",f.stage),"reusable":f.connection_reusable}));
    println!("RM_MFA_LOGIN_RESULT={output}");
}
