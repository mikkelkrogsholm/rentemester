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
            "/CN=Synthetic backup verifier CA".into(),
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
import http.server,ssl,json,sys,time,hashlib,urllib.parse
content=b'Synthetic signed native backup'
class H(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  parsed=urllib.parse.urlparse(self.path);kind=parsed.path.rsplit('/',1)[-1];media=urllib.parse.parse_qs(parsed.query).get('alt')==['media']
  with open(sys.argv[3],'a') as f:f.write(json.dumps({'path':self.path,'method':'GET','authOk':self.headers.get('authorization')=='Bearer synthetic-drive-secret','encoding':self.headers.get('accept-encoding'),'referer':self.headers.get('referer')})+'\n')
  if kind=='slow':time.sleep(.4)
  code=200;headers={};data=content
  metadata={'id':kind,'name':'synthetic.native-backup.json','parents':['folder-a'],'size':str(len(content)),'sha256Checksum':hashlib.sha256(content).hexdigest(),'trashed':False}
  if not media:
   if kind=='wrong-id':metadata['id']='other-id'
   if kind=='wrong-name':metadata['name']='other.native-backup.json'
   if kind=='wrong-parent':metadata['parents']=['other-folder']
   if kind=='two-parents':metadata['parents'].append('other-folder')
   if kind=='number-size':metadata['size']=len(content)
   if kind=='bad-size':metadata['size']=str(len(content)+1)
   if kind=='trashed':metadata['trashed']=True
   if kind=='missing-sha':metadata.pop('sha256Checksum')
   if kind=='bad-hash':metadata['sha256Checksum']='0'*64
   data=json.dumps(metadata).encode()
   if kind=='bad-json':data=b'synthetic-drive-secret-never-return'
   if kind in ['large-meta','stream-large-meta']:data=b'x'*(65536+1)
   if kind=='redirect':code,headers,data=302,{'location':'/drive/v3/files/clean'},b''
   if kind=='compressed':headers['content-encoding']='gzip'
   if kind=='revoked':code,data=403,b'synthetic-drive-secret-never-return'
  else:
   if kind=='bad-content':data=b'X'*len(content)
   if kind=='media-long':data=content+b'X'
   if kind=='media-redirect':code,headers,data=307,{'location':'/drive/v3/files/clean?alt=media'},b''
  self.send_response(code);self.send_header('content-type','application/octet-stream' if media else 'application/json')
  for k,v in headers.items():self.send_header(k,v)
  if kind!='stream-large-meta' and not (kind=='media-long' and media):self.send_header('content-length',str(len(data)))
  self.send_header('connection','close');self.end_headers()
  try:self.wfile.write(data)
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
    let port: u16 = line
        .trim()
        .parse()
        .expect("synthetic TLS Drive verifier ready");
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
        bearer_token: "synthetic-drive-secret",
        timeout_ms,
    }
}
fn expected(id: &str) -> Expected {
    Expected {
        object_id: id.into(),
        folder_id: "folder-a".into(),
        file_name: "synthetic.native-backup.json".into(),
        archive_sha256: format!("{:x}", Sha256::digest(b"Synthetic signed native backup")),
        archive_size_bytes: b"Synthetic signed native backup".len() as u64,
    }
}
fn provider(f: &Fixture, timeout_ms: u64) -> GoogleDriveVerifier {
    GoogleDriveVerifier::with_trusted_ca(config(&format!("{}/drive/v3", f.base), timeout_ms), &f.ca)
        .unwrap()
}
fn requests(f: &Fixture) -> Vec<Value> {
    std::fs::read_to_string(f.dir.path().join("requests.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, crate::auth::AuthError> {
        Ok(1767225600000)
    }
}
struct Sequence(std::sync::Mutex<std::collections::VecDeque<i64>>);
impl Sequence {
    fn new(values: &[i64]) -> Self {
        Self(std::sync::Mutex::new(values.iter().copied().collect()))
    }
}
impl Clock for Sequence {
    fn now_unix_ms(&self) -> Result<i64, crate::auth::AuthError> {
        self.0
            .lock()
            .unwrap()
            .pop_front()
            .ok_or(crate::auth::AuthError::InvalidClock)
    }
}
#[test]
fn exact_trusted_tls_metadata_and_independent_bytes_produce_only_bound_observation() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<GoogleDriveVerifier>();
    let fixture = fixture();
    let provider = provider(&fixture, 15000);
    let e = expected("clean");
    let observation = provider.verify(&e, &Fixed).unwrap();
    assert_eq!(observation.object_id, e.object_id);
    assert_eq!(observation.folder_id, e.folder_id);
    assert_eq!(observation.file_name, e.file_name);
    assert_eq!(observation.archive_sha256, e.archive_sha256);
    assert_eq!(observation.archive_size_bytes, e.archive_size_bytes);
    assert_eq!(observation.observed_at_ms, 1767225600000);
    let calls = requests(&fixture);
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0]["path"],
        "/drive/v3/files/clean?fields=id%2Cname%2Cparents%2Csize%2Csha256Checksum%2Ctrashed&supportsAllDrives=true"
    );
    assert_eq!(
        calls[1]["path"],
        "/drive/v3/files/clean?alt=media&supportsAllDrives=true"
    );
    for call in calls {
        assert_eq!(call["method"], "GET");
        assert_eq!(call["authOk"], true);
        assert_eq!(call["encoding"], "identity");
        assert!(call["referer"].is_null());
    }
    assert!(
        !serde_json::to_string(&observation)
            .unwrap()
            .contains("secret")
    );
}
#[test]
fn malformed_metadata_wrong_identity_size_hash_encoding_and_redirects_never_retry() {
    let fixture = fixture();
    let provider = provider(&fixture, 15000);
    for id in [
        "wrong-id",
        "wrong-name",
        "wrong-parent",
        "two-parents",
        "number-size",
        "bad-size",
        "trashed",
        "missing-sha",
        "bad-hash",
        "bad-json",
        "large-meta",
        "stream-large-meta",
        "redirect",
        "compressed",
        "revoked",
    ] {
        let before = requests(&fixture).len();
        let e = provider.verify(&expected(id), &Fixed).unwrap_err();
        assert!(["backup_remote_refused", "backup_remote_unavailable"].contains(&e.code));
        assert!(!format!("{e:?}").contains("secret"));
        assert_eq!(requests(&fixture).len(), before + 1, "{id}");
    }
    for id in ["bad-content", "media-long", "media-redirect"] {
        let before = requests(&fixture).len();
        assert!(provider.verify(&expected(id), &Fixed).is_err());
        assert_eq!(requests(&fixture).len(), before + 2, "{id}");
    }
}
#[test]
fn invalid_expected_and_clock_failures_do_not_start_unnecessary_provider_reads() {
    let fixture = fixture();
    let provider = provider(&fixture, 15000);
    for size in [0, MAX_WIRE_BYTES as u64 + 1] {
        let mut e = expected("clean");
        e.archive_size_bytes = size;
        assert!(provider.verify(&e, &Fixed).is_err());
    }
    let mut e = expected("bad/id");
    assert!(provider.verify(&e, &Fixed).is_err());
    e = expected("clean");
    e.archive_sha256 = "not-a-hash".into();
    assert!(provider.verify(&e, &Fixed).is_err());
    assert!(
        provider
            .verify(&expected("clean"), &Sequence::new(&[-1]))
            .is_err()
    );
    assert!(requests(&fixture).is_empty());
    for values in [
        &[1000, 999][..],
        &[1000, 301001][..],
        &[1000, -1][..],
        &[1000, 8_640_000_000_000_001][..],
        &[1000][..],
    ] {
        let before = requests(&fixture).len();
        assert!(
            provider
                .verify(&expected("clean"), &Sequence::new(values))
                .is_err()
        );
        assert_eq!(requests(&fixture).len(), before + 2);
    }
    assert_eq!(
        provider
            .verify(&expected("clean"), &Sequence::new(&[1000, 301000]))
            .unwrap()
            .observed_at_ms,
        301000
    );
}
#[test]
fn endpoints_credentials_and_private_ca_hostname_policy_are_fail_closed() {
    assert!(endpoint(ENDPOINT, false).is_ok());
    for raw in [
        "http://www.googleapis.com/drive/v3",
        "https://www.googleapis.com/drive/v3/",
        "https://www.googleapis.com/drive/v3?x=1",
        "https://www.googleapis.com.evil.invalid/drive/v3",
        "https://localhost/drive/v3",
        "https://127.0.0.1/drive/v3",
        "https://user@www.googleapis.com/drive/v3",
    ] {
        assert!(endpoint(raw, false).is_err());
    }
    assert!(endpoint("https://127.0.0.1:4443/drive/v3", true).is_ok());
    for raw in [
        "https://localhost/drive/v3",
        "https://192.168.1.1/drive/v3",
        "http://127.0.0.1/drive/v3",
        "https://127.0.0.1/wrong",
        "https://user@127.0.0.1/drive/v3",
        "https://127.0.0.1/drive/v3?q=1",
    ] {
        assert!(endpoint(raw, true).is_err());
    }
    for value in ["", "a/b", "a?b", "a%b", "a b", "æ", "."] {
        assert!(!valid_object_id(value));
    }
    assert!(valid_object_id("a_B-1"));
    assert!(!valid_object_id(&"a".repeat(161)));
    let fixture = fixture();
    for token in ["", "has space", "has\nnewline"] {
        assert!(
            GoogleDriveVerifier::with_trusted_ca(
                GatewayConfig {
                    endpoint: &format!("{}/drive/v3", fixture.base),
                    bearer_token: token,
                    timeout_ms: 15000
                },
                &fixture.ca
            )
            .is_err()
        );
    }
    let mismatch = fixture_with_san("DNS:localhost");
    let before = requests(&mismatch).len();
    assert_eq!(
        provider(&mismatch, 15000)
            .verify(&expected("clean"), &Fixed)
            .unwrap_err()
            .code,
        "backup_remote_unavailable"
    );
    assert_eq!(requests(&mismatch).len(), before);
}
#[test]
fn total_timeout_is_bounded_and_does_not_repeat_requests() {
    let fixture = fixture();
    let provider = provider(&fixture, 100);
    let start = Instant::now();
    assert!(provider.verify(&expected("slow"), &Fixed).is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
    assert_eq!(requests(&fixture).len(), 1);
}
