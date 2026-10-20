use super::*;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};
struct Fixture {
    dir: tempfile::TempDir,
    child: Child,
    base: String,
    ca: Vec<u8>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn fixture() -> Fixture {
    fixture_with_san("DNS:localhost,IP:127.0.0.1")
}
fn fixture_with_san(san: &str) -> Fixture {
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
            "/CN=Synthetic scanner CA".into(),
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
    let script = r#"
import http.server,ssl,json,sys,time
class H(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  with open(sys.argv[3],'a') as f:f.write(json.dumps({'path':self.path,'method':'GET','authOk':self.headers.get('authorization')=='Bearer synthetic-scanner-secret'})+'\n')
  body=b'{"ok":true,"scannerId":"redirect-was-followed"}'
  self.send_response(200);self.send_header('content-length',str(len(body)));self.end_headers();self.wfile.write(body)
 def do_POST(self):
  data=self.rfile.read(int(self.headers.get('content-length','0')))
  with open(sys.argv[3],'a') as f:f.write(json.dumps({'path':self.path,'body':json.loads(data),'authOk':self.headers.get('authorization')=='Bearer synthetic-scanner-secret','accept':self.headers.get('accept'),'encoding':self.headers.get('accept-encoding'),'referer':self.headers.get('referer')})+'\n')
  if self.path.startswith('/hang'):time.sleep(.5)
  code=200;headers={}
  body=json.dumps({'ok':True,'scannerId':' Synthetic A\u030a ','scannerVersion':' v1 ','evidenceRef':' source proof ','privateDiagnostics':'must never be retained'}).encode()
  if self.path.startswith('/negative'):body=b'{"ok":false,"error":"private-provider-details"}'
  elif self.path.startswith('/http-error'):code,body=503,b'private-provider-details'
  elif self.path.startswith('/redirect'):code,body,headers=302,b'',{'location':'/clean'}
  elif self.path.startswith('/malformed'):body=b'private-provider-not-json'
  elif self.path.startswith('/unsafe'):body=json.dumps({'ok':True,'scannerId':'unsafe\nname'}).encode()
  elif self.path.startswith('/optional-types'):body=json.dumps({'ok':True,'scannerId':'valid','scannerVersion':42,'evidenceRef':{'opaque':'ignored'}}).encode()
  elif self.path.startswith('/compressed'):headers={'content-encoding':'gzip'}
  elif self.path.startswith('/large'):body=b'x'*(65536+1)
  self.send_response(code);self.send_header('content-type','application/json')
  for k,v in headers.items():self.send_header(k,v)
  if not self.path.startswith('/stream-large'):self.send_header('content-length',str(len(body)))
  else:body=b'x'*(65536+1);self.send_header('connection','close')
  self.end_headers()
  try:self.wfile.write(body)
  except (BrokenPipeError,ConnectionResetError,ssl.SSLError):pass
s=http.server.ThreadingHTTPServer(('127.0.0.1',0),H);s.daemon_threads=True
c=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);c.load_cert_chain(sys.argv[1],sys.argv[2]);s.socket=c.wrap_socket(s.socket,server_side=True)
print(s.server_port,flush=True);s.serve_forever()
"#;
    let mut child = Command::new("python3")
        .arg("-u")
        .arg("-c")
        .arg(script)
        .arg(cert)
        .arg(key)
        .arg(dir.path().join("requests.jsonl"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let port: u16 = line.trim().parse().expect("synthetic TLS scanner ready");
    let pem = std::fs::read(ca).unwrap();
    Fixture {
        dir,
        child,
        base: format!("https://127.0.0.1:{port}"),
        ca: pem,
    }
}
fn config(endpoint: &str, timeout_ms: u64) -> GatewayConfig<'_> {
    GatewayConfig {
        endpoint,
        bearer_token: "synthetic-scanner-secret",
        timeout_ms,
    }
}
fn file() -> Prepared {
    Prepared::new(
        "synthetic.txt".into(),
        b"Synthetic immutable original\n".to_vec(),
    )
    .unwrap()
}
fn requests(f: &Fixture) -> Vec<Value> {
    std::fs::read_to_string(f.dir.path().join("requests.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
#[test]
fn operator_configuration_is_https_nonlocal_and_secret_safe() {
    for endpoint in [
        "http://scanner.example.invalid/scan",
        "https://localhost/scan",
        "https://localhost./scan",
        "https://x.localhost/scan",
        "https://127.0.0.1/scan",
        "https://[::1]/scan",
        "https://[::ffff:127.0.0.1]/scan",
        "https://0.0.0.0/scan",
        "https://user:private@scanner.example.invalid/scan",
        "https://@scanner.example.invalid/scan",
        "https://scanner.example.invalid/scan#fragment",
        "https://scanner.example.invalid\\scan",
    ] {
        let result = HttpJsonScanner::new(config(endpoint, 15000));
        assert!(result.is_err(), "configuration accepted");
        let error = result.err().unwrap();
        assert_eq!(error.code, "document_scanner_invalid_config");
        assert!(!format!("{error:?}").contains("synthetic-scanner-secret"));
    }
    for timeout in [0, 99, 120001] {
        assert!(
            HttpJsonScanner::new(config("https://scanner.example.invalid/scan", timeout)).is_err()
        );
    }
    for token in ["", " leading", "trailing ", "newline\n", "nonasciiø"] {
        assert!(
            HttpJsonScanner::new(GatewayConfig {
                endpoint: "https://scanner.example.invalid/scan",
                bearer_token: token,
                timeout_ms: 15000
            })
            .is_err()
        );
    }
    assert!(
        HttpJsonScanner::new(config(
            "https://scanner.example.invalid/scan?operator=opaque",
            15000
        ))
        .is_ok()
    );
    assert!(
        HttpJsonScanner::with_trusted_ca(
            config("https://127.0.0.1/scan", 15000),
            b"not a certificate"
        )
        .is_err()
    );
}
#[test]
fn private_tls_exact_copy_protocol_normalized_evidence_and_explicit_source_exception() {
    let f = fixture();
    let file = file();
    let original = file.bytes().to_vec();
    let scanner = HttpJsonScanner::with_trusted_ca(
        config(&format!("{}/clean?operator=opaque", f.base), 15000),
        &f.ca,
    )
    .unwrap();
    let proof = scanner.scan(&file).unwrap();
    assert_eq!(proof.sha256, file.hash());
    assert_eq!(proof.scanner_id, "Synthetic Å");
    assert_eq!(proof.scanner_version.as_deref(), Some("v1"));
    assert_eq!(proof.evidence_ref.as_deref(), Some("source proof"));
    proof.validate().unwrap();
    assert_eq!(file.bytes(), original);
    let sent = requests(&f);
    assert_eq!(sent.len(), 1);
    let sent = &sent[0];
    assert_eq!(sent["path"], "/clean?operator=opaque");
    assert_eq!(sent["authOk"], true);
    assert_eq!(sent["accept"], "application/json");
    assert!(sent["referer"].is_null());
    assert!(sent["encoding"].is_null());
    let mut keys: Vec<_> = sent["body"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(keys, ["bytesBase64", "filename", "mimeType", "sha256"]);
    assert_eq!(sent["body"]["sha256"], file.hash());
    assert_eq!(sent["body"]["mimeType"], file.mime());
    assert_eq!(sent["body"]["filename"], file.filename());
    assert_eq!(
        STANDARD
            .decode(sent["body"]["bytesBase64"].as_str().unwrap())
            .unwrap(),
        original
    );
    // The correct private CA is trusted and the server is reachable, but its leaf
    // certificate names a different IP. This must fail actual hostname verification.
    let wrong_name = fixture_with_san("DNS:localhost,IP:127.0.0.2");
    let scanner = HttpJsonScanner::with_trusted_ca(
        config(&format!("{}/clean", wrong_name.base), 1000),
        &wrong_name.ca,
    )
    .unwrap();
    assert_eq!(scanner.scan(&file), Err(ScanFailure::Unavailable));
    assert!(requests(&wrong_name).is_empty());
    assert!(
        HttpJsonScanner::with_trusted_ca(
            config(&f.base.replace("127.0.0.1", "localhost"), 15000),
            &f.ca
        )
        .is_err()
    );
}
#[test]
fn curated_negative_malformed_bounds_timeout_and_redirect_never_retry_or_leak() {
    let f = fixture();
    let file = file();
    for (path, error) in [
        ("negative", ScanFailure::Rejected),
        ("http-error", ScanFailure::Rejected),
        ("malformed", ScanFailure::Rejected),
        ("redirect", ScanFailure::Rejected),
        ("unsafe", ScanFailure::Unavailable),
        ("large", ScanFailure::Unavailable),
        ("stream-large", ScanFailure::Unavailable),
        ("compressed", ScanFailure::Unavailable),
    ] {
        let before = requests(&f).len();
        let scanner =
            HttpJsonScanner::with_trusted_ca(config(&format!("{}/{path}", f.base), 1000), &f.ca)
                .unwrap();
        assert_eq!(scanner.scan(&file), Err(error), "{path}");
        assert_eq!(
            requests(&f).len(),
            before + 1,
            "{path} retried or redirected"
        );
    }
    let scanner = HttpJsonScanner::with_trusted_ca(
        config(&format!("{}/optional-types", f.base), 1000),
        &f.ca,
    )
    .unwrap();
    let proof = scanner.scan(&file).unwrap();
    assert!(proof.scanner_version.is_none() && proof.evidence_ref.is_none());
    let before = requests(&f).len();
    let scanner =
        HttpJsonScanner::with_trusted_ca(config(&format!("{}/hang", f.base), 100), &f.ca).unwrap();
    let start = Instant::now();
    assert_eq!(scanner.scan(&file), Err(ScanFailure::Unavailable));
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(requests(&f).len(), before + 1);
}
