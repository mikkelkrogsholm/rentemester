#![cfg(target_os = "macos")]
use super::*;
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier, VerifiedCookie},
    namespace::{NamespaceGuard, NamespaceProfile},
};
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1767225600000)
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    control: Connection,
    company: Connection,
    guard: NamespaceGuard,
    cookie: VerifiedCookie,
}
fn fixture(role: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap().join("workspace");
    let uid = String::from_utf8(
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .parse()
    .unwrap();
    let guard = NamespaceGuard::create_fresh_with_profile(
        &root,
        uid,
        "w",
        "a",
        "Synthetic CVR",
        "2026-01-01T00:00:00.000Z",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    let control_path = root.join(".rentemester/workspace-control.sqlite");
    let company_path = root.join("a/data/ledger.sqlite");
    let mut control = Connection::open(&control_path).unwrap();
    let mut company = Connection::open(&company_path).unwrap();
    for db in [&control, &company] {
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
            .unwrap();
    }
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut company,
        "Synthetic Local",
    )
    .unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');").unwrap();
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant',?1,'user:u')",[role]).unwrap();
    company
        .execute("UPDATE companies SET cvr='87654321' WHERE id=1", [])
        .unwrap();
    let verifier = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let issued = verifier.issue_session("synthetic-token").unwrap();
    let cookie = verifier
        .verify(Some(issued.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    Fixture {
        _dir: dir,
        control,
        company,
        guard,
        cookie,
    }
}
fn context(cookie: &VerifiedCookie) -> Context<'_> {
    Context {
        cookie,
        clock: &Fixed,
        selection: control_fence::CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        },
    }
}
fn request(body: &[u8]) -> Request<'_> {
    Request {
        body,
        query: None,
        valid_transport: true,
    }
}
#[test]
fn posting_rules_original_current_authority_precedes_all_parsing() {
    let mut f = fixture("reader");
    for op in [
        Operation::Propose,
        Operation::Approve,
        Operation::Disable,
        Operation::Supersede,
    ] {
        for body in [
            b"not json".as_slice(),
            br#"{"confirm":true,"confirm":true}"#,
        ] {
            let failure = execute(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                context(&f.cookie),
                request(body),
                op,
            )
            .err()
            .unwrap();
            assert_eq!(failure.error.code, "access_denied");
        }
    }
    // Both read-only POSTs admit CompanyRead, not CompanyReview; flat reads keep
    // human_decision as a successful result and must not invent an ok property.
    for op in [Operation::Explain, Operation::Test] {
        let value = execute(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            context(&f.cookie),
            request(br#"{"context":{}}"#),
            op,
        )
        .unwrap()
        .value;
        assert_eq!(value["decision"], "human_decision");
        assert!(value.get("ok").is_none());
        assert!(value.get("errors").is_none());
        assert_eq!(
            execute(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                context(&f.cookie),
                request(b"not json"),
                op
            )
            .err()
            .unwrap()
            .error
            .code,
            "invalid_input"
        );
    }
    f.control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')").unwrap();
    for op in [Operation::Explain, Operation::Test] {
        assert_eq!(
            execute(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                context(&f.cookie),
                request(b"not json"),
                op
            )
            .err()
            .unwrap()
            .error
            .code,
            "access_denied"
        );
    }
}
#[test]
fn posting_rules_original_strict_recursive_input_and_typed_assertions() {
    let body = br#"{"proposal":{"ruleId":"synthetic","effectiveFrom":"2026-01-01","conditions":{"amount":{"min":1,"max":2}},"outcome":{},"rationale":"Synthetic","provenance":"Synthetic"},"confirm":true}"#;
    let base: Value = serde_json::from_slice(body).unwrap();
    assert!(parse(&request(body), Operation::Propose, "user:u").is_ok());
    for actor in ["user:u", "service_account:u"] {
        let mut value = base.clone();
        value["actor"] = json!(actor);
        value["principal"] = json!(actor);
        let bytes = serde_json::to_vec(&value).unwrap();
        let parsed = parse(&request(&bytes), Operation::Propose, actor).unwrap();
        assert!(parsed.get("actor").is_none() && parsed.get("principal").is_none());
        assert_eq!(
            parse(&request(&bytes), Operation::Propose, "user:other")
                .unwrap_err()
                .code,
            "access_denied"
        );
    }
    for (pointer, field_value) in [
        ("/actor", Value::Null),
        ("/confirm", json!(false)),
        ("/proposal/createdAt", json!("2026-01-01")),
        ("/proposal/companyId", json!(1)),
        ("/proposal/conditions/company", json!(1)),
        ("/proposal/conditions/amount/unknown", json!(1)),
        ("/proposal/outcome/unknown", json!(1)),
    ] {
        let mut value = base.clone();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        value
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.into(), field_value);
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(
            parse(&request(&bytes), Operation::Propose, "user:u").is_err(),
            "{pointer}"
        );
    }
    for body in [
        br#"{"context":{"attributes":{"bankText":"a","bankText":"b"}}}"#.as_slice(),
        br#"{"context":{},"actor":"user:u"}"#,
        br#"{"context":{},"principal":"user:u"}"#,
        br#"{"context":{"companyId":1}}"#,
        br#"{"context":{},"unknown":1}"#,
    ] {
        for op in [Operation::Explain, Operation::Test] {
            assert!(parse(&request(body), op, "user:u").is_err());
        }
    }
    assert!(
        parse(
            &Request {
                body,
                query: Some(""),
                valid_transport: true
            },
            Operation::Propose,
            "user:u"
        )
        .is_err()
    );
    assert!(
        parse(
            &Request {
                body,
                query: None,
                valid_transport: false
            },
            Operation::Propose,
            "user:u"
        )
        .is_err()
    );
    assert!(
        parse(
            &request(&vec![b' '; MAX_BODY + 1]),
            Operation::Propose,
            "user:u"
        )
        .is_err()
    );
}

#[test]
fn posting_rules_original_service_current_typed_authority_precedes_parse() {
    use rentemester_runtime::service_key::ServiceKey;
    let mut f = fixture("owner");
    f.control.execute_batch("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u')").unwrap();
    let verifier = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let receipt = crate::service_lifecycle::execute(
        &mut f.control,
        &verifier,
        &Fixed,
        &f.cookie,
        &crate::service_lifecycle::Operation::Create {
            display_name: "Synthetic posting rule reviewer".into(),
            operation_id: None,
            expires_in_seconds: None,
        },
    )
    .unwrap();
    let issued: Value = serde_json::from_str(receipt.as_json()).unwrap();
    let subject = issued["serviceAccountId"].as_str().unwrap();
    let key = ServiceKey::new("w", issued["secret"].as_str().unwrap()).unwrap();
    f.control.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')", [subject]).unwrap();
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','reader','user:u')", [subject]).unwrap();
    let service = || service_company::Context {
        key: &key,
        clock: &Fixed,
        max_future_days: 45,
        selection: control_fence::CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        },
    };
    for op in [
        Operation::Propose,
        Operation::Approve,
        Operation::Disable,
        Operation::Supersede,
    ] {
        assert_eq!(
            execute_service(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                service(),
                request(b"not json"),
                op
            )
            .err()
            .unwrap()
            .error
            .code,
            "access_denied"
        );
    }
    for op in [Operation::Explain, Operation::Test] {
        let value = execute_service(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            service(),
            request(br#"{"context":{}}"#),
            op,
        )
        .unwrap()
        .result;
        assert_eq!(value["decision"], "human_decision");
        assert!(value.get("ok").is_none());
    }
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','reviewer','user:u')", [subject]).unwrap();
    let input = serde_json::to_vec(&json!({"ruleId":"synthetic", "version":1, "expectedPayloadHash":"a".repeat(64), "rationale":"Synthetic", "provenance":"Synthetic", "confirm":true, "actor":format!("user:{subject}")})).unwrap();
    assert_eq!(
        execute_service(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            service(),
            request(&input),
            Operation::Approve
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES(?1,'a','revoke','user:u')", [subject]).unwrap();
    assert_eq!(
        execute_service(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            service(),
            request(b"not json"),
            Operation::Explain
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
}
