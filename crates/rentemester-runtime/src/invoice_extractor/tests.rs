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
            "/CN=Synthetic invoice provider CA".into(),
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
  with open(sys.argv[3],'a') as f:f.write(json.dumps({'path':self.path,'method':'GET'})+'\n')
  self.send_response(200);self.end_headers()
 def do_POST(self):
  data=self.rfile.read(int(self.headers.get('content-length','0')))
  with open(sys.argv[3],'a') as f:f.write(json.dumps({'path':self.path,'method':'POST','body':json.loads(data),'authOk':self.headers.get('authorization')=='Bearer synthetic-extractor-secret','accept':self.headers.get('accept'),'encoding':self.headers.get('accept-encoding'),'referer':self.headers.get('referer')})+'\n')
  if self.path.startswith('/hang'):time.sleep(.5)
  code=200;headers={}
  body=json.dumps({'fields':[{'key':'invoiceNumber','value':'SYN-70','confidence':.95,'page':1,'sourceText':'Invoice SYN-70'}],'privateDiagnostics':'synthetic-extractor-secret-never-return'}).encode()
  if self.path.startswith('/raw-fields'):body=json.dumps({'fields':[None,{'key':'unknown','value':{'opaque':'retained-for-validation'}}]}).encode()
  elif self.path.startswith('/empty'):body=b'{"fields":[]}'
  elif self.path.startswith('/http-error'):code,body=503,b'synthetic-extractor-secret-private-details'
  elif self.path.startswith('/redirect'):code,body,headers=307,b'',{'location':'/clean'}
  elif self.path.startswith('/malformed'):body=b'synthetic-extractor-secret-not-json'
  elif self.path.startswith('/no-fields'):body=b'{"fields":null}'
  elif self.path.startswith('/compressed'):headers={'content-encoding':'gzip'}
  elif self.path.startswith('/large'):body=b'x'*(1024*1024+1)
  self.send_response(code);self.send_header('content-type','application/json')
  for k,v in headers.items():self.send_header(k,v)
  if not self.path.startswith('/stream-large'):self.send_header('content-length',str(len(body)))
  else:body=b'x'*(1024*1024+1);self.send_header('connection','close')
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
    let port: u16 = line.trim().parse().expect("synthetic TLS extractor ready");
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
        bearer_token: "synthetic-extractor-secret",
        timeout_ms,
    }
}
fn requests(fixture: &Fixture) -> Vec<Value> {
    std::fs::read_to_string(fixture.dir.path().join("requests.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn bytes() -> Vec<u8> {
    b"%PDF-1.4\nSynthetic invoice extraction fixture\n".to_vec()
}
fn input<'a>(bytes: &'a [u8], hash: &'a str) -> Input<'a> {
    Input {
        sha256: hash,
        pdf_bytes: bytes,
    }
}
#[test]
fn trusted_tls_exact_input_fixed_identity_and_raw_fields_leave_validation_to_engine() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<HttpJsonExtractor>();
    let fixture = fixture();
    let data = bytes();
    let hash = format!("{:x}", Sha256::digest(&data));
    let provider = HttpJsonExtractor::with_trusted_ca(
        config(&format!("{}/clean", fixture.base), DEFAULT_TIMEOUT_MS),
        &fixture.ca,
    )
    .unwrap();
    assert_eq!(provider.identity().id, "http-json-v1");
    assert_eq!(provider.identity().version, "1");
    provider.identity().validate().unwrap();
    let result = provider.extract(input(&data, &hash)).unwrap();
    assert_eq!(result.fields.len(), 1);
    assert_eq!(result.fields[0]["value"], "SYN-70");
    assert!(result.fields[0].get("privateDiagnostics").is_none());
    let calls = requests(&fixture);
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call["authOk"], true);
    assert_eq!(call["accept"], "application/json");
    assert!(call["encoding"].is_null());
    assert!(call["referer"].is_null());
    assert_eq!(call["body"].as_object().unwrap().len(), 2);
    assert_eq!(call["body"]["sha256"], hash);
    assert_eq!(
        STANDARD
            .decode(call["body"]["pdfBytesBase64"].as_str().unwrap())
            .unwrap(),
        data
    );
    let provider = HttpJsonExtractor::with_trusted_ca(
        config(&format!("{}/raw-fields", fixture.base), 15000),
        &fixture.ca,
    )
    .unwrap();
    let raw = provider.extract(input(&data, &hash)).unwrap();
    assert!(raw.fields[0].is_null());
    assert_eq!(raw.fields[1]["key"], "unknown");
    let provider = HttpJsonExtractor::with_trusted_ca(
        config(&format!("{}/empty", fixture.base), 15000),
        &fixture.ca,
    )
    .unwrap();
    assert!(
        provider
            .extract(input(&data, &hash))
            .unwrap()
            .fields
            .is_empty()
    );
}
#[test]
fn malformed_size_encoding_rejections_and_redirects_are_curated_without_follow_or_retry() {
    let fixture = fixture();
    let data = bytes();
    let hash = format!("{:x}", Sha256::digest(&data));
    for path in [
        "/malformed",
        "/no-fields",
        "/large",
        "/stream-large",
        "/compressed",
        "/http-error",
        "/redirect",
    ] {
        let count = requests(&fixture).len();
        let provider = HttpJsonExtractor::with_trusted_ca(
            config(&format!("{}{path}", fixture.base), 15000),
            &fixture.ca,
        )
        .unwrap();
        let error = provider.extract(input(&data, &hash)).unwrap_err();
        assert_eq!(error, ProviderFailure::Unavailable);
        assert_eq!(error.as_str(), "EXTRACTION_PROVIDER_UNAVAILABLE");
        assert!(!format!("{error:?}").contains("secret"));
        let calls = requests(&fixture);
        assert_eq!(calls.len(), count + 1, "{path}");
        assert_eq!(calls.last().unwrap()["method"], "POST");
    }
}
#[test]
fn total_timeout_and_invalid_source_stop_without_extra_provider_execution() {
    let fixture = fixture();
    let data = bytes();
    let hash = format!("{:x}", Sha256::digest(&data));
    let provider = HttpJsonExtractor::with_trusted_ca(
        config(&format!("{}/hang", fixture.base), 100),
        &fixture.ca,
    )
    .unwrap();
    let now = Instant::now();
    assert_eq!(
        provider.extract(input(&data, &hash)).unwrap_err(),
        ProviderFailure::Unavailable
    );
    assert!(now.elapsed() < Duration::from_secs(2));
    let count = requests(&fixture).len();
    for invalid in [
        Input {
            sha256: &hash,
            pdf_bytes: &[],
        },
        Input {
            sha256: &"0".repeat(64),
            pdf_bytes: &data,
        },
        Input {
            sha256: "malformed",
            pdf_bytes: &data,
        },
    ] {
        assert_eq!(
            provider.extract(invalid).unwrap_err(),
            ProviderFailure::Unavailable
        )
    }
    let huge = vec![0; MAX_PDF_BYTES + 1];
    let huge_hash = format!("{:x}", Sha256::digest(&huge));
    assert!(provider.extract(input(&huge, &huge_hash)).is_err());
    assert_eq!(requests(&fixture).len(), count);
}
#[test]
fn operator_config_and_verified_hostname_never_downgrade_or_expose_bearer() {
    for endpoint in [
        "http://example.invalid/extract",
        "https://localhost/extract",
        "https://a.localhost/extract",
        "https://0.0.0.0/extract",
        "https://user:secret@example.invalid/extract",
        "https://example.invalid/#fragment",
        " https://example.invalid",
        "https://127.0.0.1/extract",
    ] {
        assert!(
            HttpJsonExtractor::new(config(endpoint, 15000)).is_err(),
            "{endpoint}"
        );
    }
    for timeout in [0, 99, 120001] {
        assert!(
            HttpJsonExtractor::new(config("https://example.invalid/extract", timeout)).is_err()
        );
    }
    let bad = GatewayConfig {
        endpoint: "https://example.invalid/extract",
        bearer_token: "synthetic-extractor-secret\n",
        timeout_ms: 15000,
    };
    let error = HttpJsonExtractor::new(bad).err().unwrap();
    assert!(!error.message.contains("secret"));
    assert!(
        HttpJsonExtractor::with_trusted_ca(config("https://127.0.0.1", 15000), b"not a CA")
            .is_err()
    );
    let fixture = fixture_with_san("DNS:wrong-host.invalid");
    let data = bytes();
    let hash = format!("{:x}", Sha256::digest(&data));
    let provider =
        HttpJsonExtractor::with_trusted_ca(config(&fixture.base, 1000), &fixture.ca).unwrap();
    assert!(provider.extract(input(&data, &hash)).is_err());
    assert!(requests(&fixture).is_empty());
}
