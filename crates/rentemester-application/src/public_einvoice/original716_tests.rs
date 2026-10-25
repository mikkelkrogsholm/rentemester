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
        .execute(
            "UPDATE companies SET cvr='12345678',name='Synthetic Seller',country='DK' WHERE id=1",
            [],
        )
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

fn local(f: &Fixture) {
    f.company.execute("INSERT INTO digisense_companies(company_key,company_type,participant_id,company_name) VALUES('company_key','DK:CVR','DK12345678','Synthetic Seller')", []).unwrap();
}
fn pending(n: Next) -> Admission {
    match n {
        Next::Pending(a) => *a,
        _ => panic!("pending required"),
    }
}
fn begin(f: &mut Fixture, sender: bool) -> Admission {
    pending(
        start(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            context(&f.cookie),
            Intent::Original716TestRegistration {
                sender,
                network: Network::Nemhandel,
            },
            "test",
        )
        .unwrap()
        .value,
    )
}
fn advance(f: &mut Fixture, a: Admission, o: Outcome) -> Result<Committed<Next>, Failure> {
    finish(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        context(&f.cookie),
        a,
        o,
    )
}
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};
struct ProviderFixture {
    dir: tempfile::TempDir,
    child: Child,
    port: u16,
    ca: String,
}
impl Drop for ProviderFixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn provider_fixture(san: &str) -> ProviderFixture {
    let dir = tempfile::tempdir().unwrap();
    let cert = dir.path().join("ca.pem");
    let key = dir.path().join("key.pem");
    let ca = dir.path().join("root.pem");
    let ca_key = dir.path().join("root.key");
    let csr = dir.path().join("leaf.csr");
    let ext = dir.path().join("leaf.ext");
    std::fs::write(&ext,format!("basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName={san}\n")).unwrap();
    let cmds: Vec<Vec<String>> = vec![
        vec![
            "req".into(),
            "-x509".into(),
            "-newkey".into(),
            "rsa:2048".into(),
            "-nodes".into(),
            "-days".into(),
            "1".into(),
            "-subj".into(),
            "/CN=Synthetic DigiSense test CA".into(),
            "-addext".into(),
            "basicConstraints=critical,CA:TRUE".into(),
            "-keyout".into(),
            ca_key.to_str().unwrap().into(),
            "-out".into(),
            ca.to_str().unwrap().into(),
        ],
        vec![
            "req".into(),
            "-new".into(),
            "-newkey".into(),
            "rsa:2048".into(),
            "-nodes".into(),
            "-subj".into(),
            "/CN=localhost".into(),
            "-keyout".into(),
            key.to_str().unwrap().into(),
            "-out".into(),
            csr.to_str().unwrap().into(),
        ],
        vec![
            "x509".into(),
            "-req".into(),
            "-in".into(),
            csr.to_str().unwrap().into(),
            "-CA".into(),
            ca.to_str().unwrap().into(),
            "-CAkey".into(),
            ca_key.to_str().unwrap().into(),
            "-CAcreateserial".into(),
            "-days".into(),
            "1".into(),
            "-extfile".into(),
            ext.to_str().unwrap().into(),
            "-out".into(),
            cert.to_str().unwrap().into(),
        ],
    ];
    for args in cmds {
        let out = Command::new("openssl").args(args).output().unwrap();
        assert!(out.status.success());
    }
    let mut child = Command::new("python3")
        .args([
            "-u",
            "-c",
            include_str!("../../../rentemester-runtime/src/public_einvoice/fixture.py"),
        ])
        .arg(cert)
        .arg(key)
        .arg(dir.path().join("trace.jsonl"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    ProviderFixture {
        port: line.trim().parse().unwrap(),
        ca: std::fs::read_to_string(ca).unwrap(),
        dir,
        child,
    }
}
fn config(value: Value) -> rentemester_runtime::public_einvoice::Config {
    serde_json::from_value(value).unwrap()
}
fn client(f: &ProviderFixture, mode: &str) -> rentemester_runtime::public_einvoice::Client {
    rentemester_runtime::public_einvoice::Client::new(config(json!({"apiLicenseKey":"synthetic-private-api-key-74","environment":"test","baseUrl":format!("https://127.0.0.1:{}/{mode}/ap/api/rest",f.port),"trustedCaPem":f.ca,"timeoutMs":250}))).unwrap()
}
fn trace(f: &ProviderFixture) -> Vec<Value> {
    std::fs::read_to_string(f.dir.path().join("trace.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|v| serde_json::from_str(v).unwrap())
        .collect()
}

#[test]
fn original716_real_application_fake_provider_positive_and_generic_audit() {
    let provider = provider_fixture("IP:127.0.0.1");
    let client = client(&provider, "normal");
    for sender in [false, true] {
        let mut f = fixture("owner");
        local(&f);
        let a = begin(&mut f, sender);
        assert!(matches!(a.request(), Request::Auth));
        let a = pending(
            advance(
                &mut f,
                a,
                Outcome::Auth(Some(client.validate_auth().ok().unwrap())),
            )
            .unwrap()
            .value,
        );
        let outcome = match a.request() {
            Request::Original716TestRegistration {
                authorization,
                company_key,
                sender_cvr,
                network,
            } => {
                let r = client
                    .original716_test_registration(authorization, company_key, sender_cvr, network)
                    .unwrap();
                Outcome::Original716TestRegistration(Some(r.webhook_registered))
            }
            _ => panic!("registration required"),
        };
        let result = advance(&mut f, a, outcome).unwrap();
        assert!(
            matches!(result.value, Next::Complete(ref v) if *v == json!({"ok":true,"registered":true}))
        );
        let rows: i64 = f
            .company
            .query_row("SELECT COUNT(*) FROM digisense_participants", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(rows, if sender { 1 } else { 0 });
        let mut q = f
            .company
            .prepare("SELECT message FROM audit_log WHERE event_type LIKE 'digisense_test_%'")
            .unwrap();
        let messages = q
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(messages.len(), 2);
        for m in messages {
            for secret in [
                "5790000000000",
                "company_key",
                "signatureSecret",
                "apiLicenseKey",
            ] {
                assert!(!m.contains(secret));
            }
        }
    }
    let calls = trace(&provider);
    let posts: Vec<_> = calls
        .iter()
        .filter(|v| v["path"].as_str().unwrap().contains("register-participant"))
        .collect();
    assert_eq!(posts.len(), 2);
    assert_eq!(posts[0]["body"]["participantId"], "5790000000000");
    assert_eq!(posts[0]["body"]["direction"], "inbound");
    assert_eq!(posts[1]["body"]["participantId"], "12345678");
    assert_eq!(posts[1]["body"]["documentProfiles"], "default-peppol");
    assert!(posts[1]["path"].as_str().unwrap().ends_with("/peppol"));
}
#[test]
fn original716_current_auth_revoked_and_local_config_changed_block_next_phase() {
    let provider = provider_fixture("IP:127.0.0.1");
    let client = client(&provider, "normal");
    for change in ["revoke", "company"] {
        let mut f = fixture("owner");
        local(&f);
        let a = begin(&mut f, true);
        let auth = client.validate_auth().ok().unwrap();
        if change == "revoke" {
            f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','revoke',NULL,'user:u')",[]).unwrap();
        } else {
            f.company
                .execute(
                    "UPDATE digisense_companies SET company_key='changed_key'",
                    [],
                )
                .unwrap();
        }
        assert!(advance(&mut f, a, Outcome::Auth(Some(auth))).is_err());
    }
    assert!(
        !trace(&provider)
            .iter()
            .any(|v| v["path"].as_str().unwrap().contains("register-participant"))
    );
}
#[test]
fn original716_malformed_license_and_failure_never_save_participant() {
    let provider = provider_fixture("IP:127.0.0.1");
    for mode in ["auth-missing-fields", "auth-wrong", "auth-constrained"] {
        let client = client(&provider, mode);
        let mut f = fixture("owner");
        local(&f);
        let a = begin(&mut f, true);
        let n = advance(&mut f, a, Outcome::Auth(client.validate_auth().ok()))
            .unwrap()
            .value;
        assert!(matches!(n,Next::Complete(ref v) if v["ok"] == false));
    }
    let client = client(&provider, "normal");
    let mut f = fixture("owner");
    local(&f);
    let a = begin(&mut f, true);
    let a = pending(
        advance(&mut f, a, Outcome::Auth(client.validate_auth().ok()))
            .unwrap()
            .value,
    );
    assert!(
        matches!(advance(&mut f,a,Outcome::Original716TestRegistration(None)).unwrap().value,Next::Complete(ref v) if v["ok"] == false)
    );
    assert_eq!(
        f.company
            .query_row("SELECT COUNT(*) FROM digisense_participants", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(f.company.query_row("SELECT COUNT(*) FROM audit_log WHERE event_type='digisense_test_sender_registration_failed'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
}
#[test]
fn original716_exactly_one_company_and_test_environment_required() {
    let mut f = fixture("owner");
    for env in ["test", "production"] {
        let n = start(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            context(&f.cookie),
            Intent::Original716TestRegistration {
                sender: false,
                network: Network::Nemhandel,
            },
            env,
        )
        .unwrap()
        .value;
        assert!(matches!(n,Next::Complete(ref v) if v["ok"] == false));
    }
    local(&f);
    f.company.execute("INSERT INTO digisense_companies(company_key,company_type,participant_id,company_name)VALUES('other_key','DK:CVR','DK87654321','Other Synthetic')",[]).unwrap();
    assert!(test_company(&f.company).is_err());
}

#[test]
fn original716_revoked_after_provider_effect_is_unknown_not_false_success() {
    let provider = provider_fixture("IP:127.0.0.1");
    let client = client(&provider, "normal");
    let mut f = fixture("owner");
    local(&f);
    let a = begin(&mut f, true);
    let a = pending(
        advance(&mut f, a, Outcome::Auth(client.validate_auth().ok()))
            .unwrap()
            .value,
    );
    let r = match a.request() {
        Request::Original716TestRegistration {
            authorization,
            company_key,
            sender_cvr,
            network,
        } => client
            .original716_test_registration(authorization, company_key, sender_cvr, network)
            .unwrap(),
        _ => panic!("registration required"),
    };
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','revoke',NULL,'user:u')",[]).unwrap();
    let failure = advance(
        &mut f,
        a,
        Outcome::Original716TestRegistration(Some(r.webhook_registered)),
    )
    .err()
    .unwrap();
    assert!(failure.stage == FailureStage::OutcomeUnknown);
    assert_eq!(
        f.company
            .query_row("SELECT COUNT(*) FROM digisense_participants", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        trace(&provider)
            .iter()
            .filter(|v| v["path"].as_str().unwrap().contains("register-participant"))
            .count(),
        1
    );
}
