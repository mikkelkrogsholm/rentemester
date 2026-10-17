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
    let result = if request["mode"] == "begin" {
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
