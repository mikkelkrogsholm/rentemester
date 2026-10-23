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
fn accounts_original_current_authority_precedes_all_parsing() {
    for role in ["reader", "bookkeeper", "reviewer"] {
        let mut f = fixture(role);
        for op in [Operation::Add, Operation::RoleConfirm] {
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
        for op in [Operation::List, Operation::RolesStatus] {
            let value = execute(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                context(&f.cookie),
                request(b""),
                op,
            )
            .unwrap()
            .value;
            assert_eq!(value["ok"], true);
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
        for op in [
            Operation::List,
            Operation::RolesStatus,
            Operation::Add,
            Operation::RoleConfirm,
        ] {
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
}
#[test]
fn accounts_original_recursive_parse_and_typed_assertions() {
    let base =
        json!({"input":{"accountNo":" 1790 ","name":" Synthetic ","type":"asset"},"confirm":true});
    for actor in ["user:u", "service_account:u"] {
        let mut value = base.clone();
        value["actor"] = json!(actor);
        value["principal"] = json!(actor);
        let bytes = serde_json::to_vec(&value).unwrap();
        let parsed = parse(&request(&bytes), Operation::Add, actor).unwrap();
        assert!(parsed.get("actor").is_none() && parsed.get("principal").is_none());
        assert_eq!(parsed["input"]["accountNo"], " 1790 ");
        assert_eq!(
            parse(&request(&bytes), Operation::Add, "user:other")
                .unwrap_err()
                .code,
            "access_denied"
        );
    }
    for body in [
        br#"{"input":{"accountNo":"1","accountNo":"2"},"confirm":true}"#.as_slice(),
        br#"{"input":{"accountNo":"1","createdByProgram":"forged"},"confirm":true}"#,
        br#"{"input":{},"confirm":true,"company":"other"}"#,
        br#"{"input":{},"confirm":false}"#,
        br#"{"input":{},"confirm":true,"actor":null}"#,
        br#"{"input":{},"confirm":true,"principal":"u"}"#,
    ] {
        assert!(parse(&request(body), Operation::Add, "user:u").is_err());
    }
    assert!(
        parse(
            &request(br#"{"role":"bank","accountNo":"1790","confirm":true}"#),
            Operation::RoleConfirm,
            "user:u"
        )
        .is_ok()
    );
    assert!(
        parse(
            &request(br#"{"role":"bank","accountNo":"1790","confirm":true,"version":1}"#),
            Operation::RoleConfirm,
            "user:u"
        )
        .is_err()
    );
    for op in [Operation::List, Operation::RolesStatus] {
        assert_eq!(parse(&request(b""), op, "user:u").unwrap(), json!({}));
        for body in [b"{}".as_slice(), br#"{"actor":"user:u"}"#] {
            assert!(parse(&request(body), op, "user:u").is_err());
        }
    }
    let bytes = serde_json::to_vec(&base).unwrap();
    let mut bounded = bytes.clone();
    bounded.resize(MAX_BODY, b' ');
    assert!(parse(&request(&bounded), Operation::Add, "user:u").is_ok());
    bounded.push(b' ');
    assert!(parse(&request(&bounded), Operation::Add, "user:u").is_err());
    for r in [
        Request {
            body: &bytes,
            query: Some(""),
            valid_transport: true,
        },
        Request {
            body: &bytes,
            query: None,
            valid_transport: false,
        },
    ] {
        assert!(parse(&r, Operation::Add, "user:u").is_err());
    }
}
#[test]
fn accounts_original_service_current_typed_authority_precedes_parse() {
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
            display_name: "Synthetic account owner".into(),
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
    for op in [Operation::Add, Operation::RoleConfirm] {
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
    for op in [Operation::List, Operation::RolesStatus] {
        let value = execute_service(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            service(),
            request(b""),
            op,
        )
        .unwrap()
        .result;
        assert_eq!(value["ok"], true);
    }
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','owner','user:u')", [subject]).unwrap();
    // An owner reaches parsing; derive the stored actor from this key, never the assertion.
    let input = serde_json::to_vec(&json!({"input":{"accountNo":"1790","name":"Synthetic bank","type":"asset"},"confirm":true,"actor":format!("user:{subject}")})).unwrap();
    assert_eq!(
        execute_service(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            service(),
            request(&input),
            Operation::Add
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
    assert_eq!(
        execute_service(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            service(),
            request(b"not json"),
            Operation::Add
        )
        .err()
        .unwrap()
        .error
        .code,
        "invalid_input"
    );
    let valid = serde_json::to_vec(&json!({"input":{"accountNo":"1790","name":"Synthetic bank","type":"asset"},"confirm":true,"principal":format!("service_account:{subject}")})).unwrap();
    assert_eq!(
        execute_service(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            service(),
            request(&valid),
            Operation::Add
        )
        .unwrap()
        .result["ok"],
        true
    );
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES(?1,'a','revoke','user:u')", [subject]).unwrap();
    assert_eq!(
        execute_service(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            service(),
            request(b"not json"),
            Operation::List
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
}

#[test]
fn accounts_original_core_false_is_exact_fence_failure_and_atomic() {
    let mut f = fixture("owner");
    let body = br#"{"role":"bank","accountNo":"missing","confirm":true}"#;
    let input: Value = serde_json::from_slice(body).unwrap();
    let expected = {
        let tx = f
            .company
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        store::execute(&tx, Operation::RoleConfirm, &input, "user:u").unwrap()
    };
    assert_eq!(expected["ok"], false);
    let failure = execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        context(&f.cookie),
        request(body),
        Operation::RoleConfirm,
    )
    .err()
    .unwrap();
    assert_eq!(failure.error.code, "accounts_original_business");
    assert_eq!(
        serde_json::from_str::<Value>(&failure.error.message).unwrap(),
        expected
    );
    assert!(
        expected["errors"]
            .as_array()
            .is_some_and(|errors| !errors.is_empty()
                && errors.iter().all(|e| e
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty() && !s.starts_with('{'))))
    );
    assert_eq!(
        f.company
            .query_row("SELECT count(*) FROM audit_log", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}
