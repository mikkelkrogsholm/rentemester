use super::*;
use std::io::{BufRead, BufReader as StdReader};
use std::process::{Child, Command, Stdio};
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
fn fixture(secure: bool, mode: &str, san: &str) -> Fixture {
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
            "/CN=Synthetic IMAP CA".into(),
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
    for args in cmds.into_iter().filter(|_| secure) {
        let out = Command::new("openssl").args(args).output().unwrap();
        assert!(out.status.success());
    }
    let script = include_str!("fixture.py");
    let mut child = Command::new("python3")
        .args(["-u", "-c", script])
        .arg(cert)
        .arg(key)
        .arg(dir.path().join("trace.jsonl"))
        .arg(if secure { "tls" } else { "plain" })
        .arg(mode)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    StdReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let port = line.trim().parse().expect("synthetic IMAP listening");
    let ca = if secure {
        std::fs::read_to_string(ca).unwrap()
    } else {
        String::new()
    };
    Fixture {
        dir,
        child,
        port,
        ca,
    }
}
fn config(f: &Fixture, secure: bool) -> Config {
    Config::parse(&serde_json::to_vec(&serde_json::json!({"host":"127.0.0.1","port":f.port,"username":"synthetic-user","password":"synthetic-password","secure":secure})).unwrap()).unwrap()
}
fn client(f: &Fixture) -> NetworkClient {
    NetworkClient::new(OperatorConfig {
        trusted_ca_pem: (!f.ca.is_empty()).then(|| f.ca.clone()),
        ..Default::default()
    })
    .unwrap()
}
fn trace(f: &Fixture) -> Vec<String> {
    std::fs::read_to_string(f.dir.path().join("trace.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}
#[test]
fn config_is_strict_defaults_secure_and_never_exposes_password() {
    let c = Config::parse(
        br#"{"host":" mail.example.test ","port":993,"username":" owner ","password":" secret "}"#,
    )
    .unwrap();
    assert!(c.status().secure);
    assert_eq!(c.status().mailbox, "INBOX");
    assert_eq!(c.password.as_str(), " secret ");
    let status = serde_json::to_string(&c.status()).unwrap();
    assert!(!status.contains("secret"));
    for raw in [
        r#"{"host":"x","port":0,"username":"a","password":"b"}"#,
        r#"{"host":"x","port":993,"username":"a\r\nBAD","password":"b"}"#,
        r#"{"host":"x","port":993,"username":"a","password":"b","trustedCaPem":"x"}"#,
        r#"{"host":"x","port":993,"port":143,"username":"a","password":"b"}"#,
    ] {
        assert!(Config::parse(raw.as_bytes()).is_err());
    }
    assert!(Config::parse(&vec![b' '; 16385]).is_err());
    assert!(
        NetworkClient::new(OperatorConfig {
            timeout_ms: 30001,
            ..Default::default()
        })
        .is_err()
    );
}
#[test]
fn explicit_plaintext_preserves_binary_and_uidvalidity_resets_old_cursor() {
    let f = fixture(false, "normal", "DNS:localhost,IP:127.0.0.1");
    let c = config(&f, false);
    let mut network = client(&f);
    let first = network
        .fetch(
            &c,
            Some(Cursor {
                uid_validity: 6,
                last_uid: 999,
            }),
        )
        .unwrap();
    assert_eq!(first.uid_validity, 7);
    assert_eq!(first.messages.len(), 2);
    assert!(!first.has_more);
    assert_eq!(
        first.messages[0].raw,
        b"Message-ID: <synthetic>\r\n\r\nraw\x00\xff\r\n"
    );
    let next = network
        .fetch(
            &c,
            Some(Cursor {
                uid_validity: 7,
                last_uid: 1,
            }),
        )
        .unwrap();
    assert_eq!(next.messages.len(), 1);
    assert_eq!(next.messages[0].uid, 2);
    let t = trace(&f).join("\n");
    assert!(t.contains("UID SEARCH UID 1:*"));
    assert!(t.contains("UID SEARCH UID 2:*"));
    assert!(t.contains("BODY.PEEK[]"));
    for forbidden in [
        "STORE",
        "EXPUNGE",
        "DELETE",
        "STARTTLS",
        "synthetic-password",
    ] {
        assert!(!t.contains(forbidden));
    }
}
#[test]
fn tls_private_ca_retains_hostname_and_trust_validation() {
    let f = fixture(true, "normal", "DNS:localhost,IP:127.0.0.1");
    let c = config(&f, true);
    assert_eq!(client(&f).fetch(&c, None).unwrap().messages.len(), 2);
    assert_eq!(
        NetworkClient::new(OperatorConfig::default())
            .unwrap()
            .fetch(&c, None)
            .unwrap_err(),
        Failure::Unavailable
    );
    let wrong = fixture(true, "normal", "DNS:other.invalid");
    assert_eq!(
        client(&wrong)
            .fetch(&config(&wrong, true), None)
            .unwrap_err(),
        Failure::Unavailable
    );
}
#[test]
fn oversize_is_explicit_and_wrong_uid_never_advances_cursor() {
    let f = fixture(false, "oversize", "DNS:localhost");
    let b = client(&f).fetch(&config(&f, false), None).unwrap();
    assert!(b.messages.is_empty());
    assert!(b.has_more);
    assert_eq!(b.refusals.len(), 1);
    assert_eq!(b.refusals[0].uid, 1);
    assert_eq!(b.refusals[0].declared_size, MAX_MESSAGE as u64 + 1);
    assert_eq!(b.refusals[0].code, "imap_message_too_large");
    let bad = fixture(false, "wrong-uid", "DNS:localhost");
    assert_eq!(
        client(&bad).fetch(&config(&bad, false), None).unwrap_err(),
        Failure::Protocol
    );
}
#[test]
fn pass_count_timeout_auth_and_line_limits_are_real() {
    let f = fixture(false, "many", "DNS:localhost");
    let b = client(&f).fetch(&config(&f, false), None).unwrap();
    assert_eq!(b.messages.len(), 32);
    assert!(b.has_more);
    for (mode, expected) in [
        ("auth", Failure::Authentication),
        ("long-line", Failure::Limit),
        ("malformed", Failure::Protocol),
    ] {
        let f = fixture(false, mode, "DNS:localhost");
        assert_eq!(
            client(&f).fetch(&config(&f, false), None).unwrap_err(),
            expected
        );
        assert_eq!(
            trace(&f).iter().filter(|s| s.as_str() == "CONNECT").count(),
            1
        );
    }
    let f = fixture(false, "hang", "DNS:localhost");
    let mut c = NetworkClient::new(OperatorConfig {
        timeout_ms: 50,
        ..Default::default()
    })
    .unwrap();
    let before = std::time::Instant::now();
    assert_eq!(
        c.fetch(&config(&f, false), None).unwrap_err(),
        Failure::Timeout
    );
    assert!(before.elapsed() < Duration::from_secs(2));
}

#[test]
fn aggregate_remaining_budget_stops_before_reading_literal() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let (client, mut server) = tokio::io::duplex(1024);
        let writer = tokio::spawn(async move {
            let mut request = [0; 128];
            let _ = server.read(&mut request).await.unwrap();
            // A valid-sized message that cannot fit the remaining aggregate budget.
            server
                .write_all(b"* 1 FETCH (UID 4 BODY[] {123}\r\n")
                .await
                .unwrap();
        });
        let r = command(
            &mut BufReader::new(client),
            4,
            "UID FETCH 4 (UID BODY.PEEK[])",
            100,
            Some(4),
        )
        .await
        .unwrap();
        assert!(r.budget);
        assert!(r.literal.is_none());
        assert!(r.oversized.is_none());
        writer.await.unwrap();
    });
}
