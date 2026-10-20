use super::*;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};
struct Fixture {
    dir: tempfile::TempDir,
    child: Child,
    port: u16,
    ca: String,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn fixture(san: &str) -> Fixture {
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
        .args(["-u", "-c", include_str!("fixture.py")])
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
    Fixture {
        port: line.trim().parse().unwrap(),
        ca: std::fs::read_to_string(ca).unwrap(),
        dir,
        child,
    }
}
fn config(value: Value) -> Config {
    serde_json::from_value(value).unwrap()
}
fn client(f: &Fixture, mode: &str) -> Client {
    Client::new(config(json!({"apiLicenseKey":"synthetic-private-api-key-74","environment":"test","baseUrl":format!("https://127.0.0.1:{}/{mode}/ap/api/rest",f.port),"trustedCaPem":f.ca,"timeoutMs":250}))).unwrap()
}
fn trace(f: &Fixture) -> Vec<Value> {
    std::fs::read_to_string(f.dir.path().join("trace.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|v| serde_json::from_str(v).unwrap())
        .collect()
}
#[test]
fn config_environment_and_operator_tls_boundaries() {
    for (environment, host) in [
        ("test", "test-api.digisense.dk"),
        ("production", "api.digisense.dk"),
    ] {
        let c = Client::new(config(
            json!({"apiLicenseKey":"synthetic-key","environment":environment}),
        ))
        .unwrap();
        assert_eq!(c.base.host_str(), Some(host));
        assert_eq!(
            c.url(&["deliver-document"]).unwrap().path(),
            "/ap/api/rest/deliver-document"
        );
    }
    for extra in [
        json!({"baseUrl":"http://127.0.0.1"}),
        json!({"baseUrl":"https://user:secret@example.invalid"}),
        json!({"baseUrl":"https://example.invalid/?leak=x"}),
        json!({"baseUrl":"https://127.0.0.1"}),
        json!({"timeoutMs":99}),
        json!({"timeoutMs":30001}),
        json!({"apiLicenseKey":"key\r\nother"}),
    ] {
        let mut v = json!({"apiLicenseKey":"synthetic-key","environment":"test"});
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert!(Client::new(config(v)).is_err());
    }
    assert!(
        serde_json::from_value::<Config>(json!({"apiLicenseKey":"key","environment":"sandbox"}))
            .is_err()
    );
    assert!(
        serde_json::from_value::<Config>(
            json!({"apiLicenseKey":"key","environment":"test","callerActor":"invented"})
        )
        .is_err()
    );
}
#[test]
fn real_tls_explicit_registration_validation_delivery_and_status() {
    let f = fixture("DNS:localhost,IP:127.0.0.1");
    let c = client(&f, "normal");
    assert_eq!(c.environment(), Environment::Test);
    assert_eq!(
        c.register_company("DK12345678", "Synthetic Legal Entity")
            .unwrap()
            .company_key,
        "company_key"
    );
    let registered = c
        .register_outbound("company_key", "DK12345678", Network::Nemhandel)
        .unwrap();
    assert!(registered.registered_on_network);
    assert!(!registered.webhook_registered);
    c.register_outbound("company_key", "DK12345678", Network::Peppol)
        .unwrap();
    assert!(c.validate("<Invoice>synthetic</Invoice>").unwrap().valid);
    let DeliveryOutcome::Observed(observation) =
        c.deliver("company_key", "<Invoice>synthetic</Invoice>")
    else {
        panic!("queued ID must be retained")
    };
    assert_eq!(observation.document_id.as_deref(), Some("doc-1"));
    assert_eq!(observation.status, "queued-for-delivery");
    assert!(observation.public_url.is_none());
    assert_eq!(
        c.status("company_key", "doc-1").unwrap().status,
        "delivered"
    );
    let calls = trace(&f);
    assert_eq!(calls.len(), 6);
    assert!(
        calls
            .iter()
            .all(|v| v["authOk"] == true && v["accept"] == "application/json")
    );
    assert_eq!(
        calls[0]["body"],
        json!({"companyType":{"type":"DK:CVR","id":"DK12345678"},"companyName":"Synthetic Legal Entity"})
    );
    assert_eq!(
        calls[1]["body"],
        json!({"direction":"outbound","participantType":"DK:CVR","participantId":"DK12345678","companyKey":"company_key","webhookUrl":null,"documentProfiles":"default-nemhandel"})
    );
    assert_eq!(calls[2]["body"]["documentProfiles"], "default-peppol");
    assert_eq!(calls[4]["body"], "<Invoice>synthetic</Invoice>");
    assert_eq!(calls[4]["query"], json!({"companyKey":["company_key"]}));
}
#[test]
fn validation_rejection_and_explicit_delivery_never_hide_extra_calls() {
    let f = fixture("IP:127.0.0.1");
    let c = client(&f, "invalid");
    assert!(!c.validate("<Invoice/>").unwrap().valid);
    assert_eq!(trace(&f).len(), 1);
    // Caller owns the gate. This explicitly requested delivery has no hidden validate.
    let DeliveryOutcome::Observed(o) = c.deliver("company_key", "<Invoice/>") else {
        panic!("terminal accepted identity must survive")
    };
    assert_eq!(o.status, "document-not-valid");
    assert_eq!(o.document_id.as_deref(), Some("doc-1"));
    assert_eq!(trace(&f).len(), 2);
    assert_eq!(
        c.register_company("DK12345678", "Synthetic").unwrap_err(),
        Error::Refused { status: 409 }
    );
    assert_eq!(
        c.register_outbound("company_key", "DK12345678", Network::Nemhandel)
            .unwrap_err(),
        Error::Refused { status: 409 }
    );
}
#[test]
fn uncertain_delivery_is_bounded_and_never_retried_or_redirected() {
    let f = fixture("IP:127.0.0.1");
    for mode in [
        "malformed",
        "no-id",
        "unknown-status",
        "redirect",
        "large",
        "hang",
    ] {
        let c = client(&f, mode);
        let before = trace(&f).len();
        assert_eq!(
            c.deliver("company_key", "<Invoice/>"),
            DeliveryOutcome::Unknown,
            "{mode}"
        );
        assert_eq!(trace(&f).len(), before + 1, "one POST only {mode}");
    }
    let DeliveryOutcome::Observed(accepted) =
        client(&f, "secret-url").deliver("company_key", "<Invoice/>")
    else {
        panic!("ancillary URL cannot discard accepted identity")
    };
    assert_eq!(accepted.document_id.as_deref(), Some("doc-1"));
    assert!(accepted.public_url.is_none());
    let c = client(&f, "normal");
    let before = trace(&f).len();
    assert_eq!(
        c.deliver("invalid key", "<Invoice/>"),
        DeliveryOutcome::Unknown
    );
    assert_eq!(
        c.validate(&"x".repeat(MAX_XML + 1)).unwrap_err(),
        Error::InvalidInput
    );
    assert_eq!(trace(&f).len(), before);
    let observation = c.status("company_key", "doc-1").unwrap();
    assert!(!observation.message.contains("synthetic-private-api-key-74"));
    assert_eq!(
        client(&f, "wrong-id")
            .status("company_key", "doc-1")
            .unwrap_err(),
        Error::Protocol
    );
}
#[test]
fn private_ca_never_disables_hostname_or_root_verification() {
    let f = fixture("DNS:other.invalid");
    assert_eq!(
        client(&f, "normal").deliver("company_key", "<Invoice/>"),
        DeliveryOutcome::Unknown
    );
    assert!(trace(&f).is_empty());
    let trusted = fixture("IP:127.0.0.1");
    let other = fixture("IP:127.0.0.1");
    let c=Client::new(config(json!({"apiLicenseKey":"synthetic-private-api-key-74","environment":"test","baseUrl":format!("https://127.0.0.1:{}/normal/ap/api/rest",other.port),"trustedCaPem":trusted.ca,"timeoutMs":250}))).unwrap();
    assert_eq!(
        c.deliver("company_key", "<Invoice/>"),
        DeliveryOutcome::Unknown
    );
    assert!(trace(&other).is_empty());
}
