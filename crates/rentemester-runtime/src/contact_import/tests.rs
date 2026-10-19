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
    let dir = tempfile::tempdir().unwrap();
    let cert = dir.path().join("ca.pem");
    let key = dir.path().join("key.pem");
    let ca = dir.path().join("root.pem");
    let ca_key = dir.path().join("root.key");
    let csr = dir.path().join("leaf.csr");
    let ext = dir.path().join("leaf.ext");
    std::fs::write(&ext,"basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n").unwrap();
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
            "/CN=Synthetic CVR CA".into(),
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
import http.server,ssl,json,sys
class H(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_POST(self):
  data=self.rfile.read(int(self.headers.get('content-length','0')))
  with open(sys.argv[3],'a') as f:f.write(json.dumps({'path':self.path,'body':json.loads(data),'authOk':self.headers.get('authorization')=='Basic c3ludGhldGljOnN5bnRoZXRpYy1wcml2YXRlLXBhc3N3b3Jk'})+'\n')
  if self.path.startswith('/redirect'):
   self.send_response(302);self.send_header('location','https://127.0.0.1:1/credential-leak');self.send_header('content-length','0');self.end_headers();return
  if self.path.startswith('/failure'):code,body=503,b'synthetic-private-password'
  elif self.path.startswith('/malformed'):code,body=200,b'{invalid'
  else:code,body=200,json.dumps({'hits':{'hits':[{'_source':{'Vrvirksomhed':{'cvrNummer':12345678,'navne':[{'navn':'Synthetic register','periode':{'gyldigTil':None}}]}}}]}}).encode()
  self.send_response(code);self.send_header('content-type','application/json');self.send_header('content-length',str(4*1024*1024+1 if self.path.startswith('/large') else len(body)));self.end_headers()
  if not self.path.startswith('/large'):self.wfile.write(body)
s=http.server.HTTPServer(('127.0.0.1',0),H);c=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);c.load_cert_chain(sys.argv[1],sys.argv[2]);s.socket=c.wrap_socket(s.socket,server_side=True);print(s.server_port,flush=True);s.serve_forever()
"#;
    let mut child = Command::new("python3")
        .arg("-u")
        .arg("-c")
        .arg(script)
        .arg(&cert)
        .arg(&key)
        .arg(dir.path().join("capture.jsonl"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let port: u16 = line.trim().parse().unwrap();
    Fixture {
        ca: std::fs::read(ca).unwrap(),
        dir,
        child,
        base: format!("https://127.0.0.1:{port}"),
    }
}
#[test]
fn real_tls_query_auth_no_redirect_retry_or_provider_body_leak() {
    let f = fixture();
    let mut provider = Provider::new(
        &format!("{}/success///", f.base),
        "synthetic",
        "synthetic-private-password",
        Some(&f.ca),
    )
    .unwrap();
    assert_eq!(provider.lookup("12345678").unwrap()["cvrNummer"], 12345678);
    for (path, expected) in [
        ("redirect", LookupError::Unavailable),
        ("failure", LookupError::Unavailable),
        ("malformed", LookupError::InvalidResponse),
        ("large", LookupError::InvalidResponse),
    ] {
        let mut p = Provider::new(
            &format!("{}/{path}", f.base),
            "synthetic",
            "synthetic-private-password",
            Some(&f.ca),
        )
        .unwrap();
        let e = p.lookup("12345678").unwrap_err();
        assert_eq!(e, expected);
        assert!(!e.message().contains("synthetic-private-password"));
    }
    let lines = std::fs::read_to_string(f.dir.path().join("capture.jsonl")).unwrap();
    let requests = lines
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 5);
    assert_eq!(requests[0]["path"], "/success/virksomhed/_search");
    for r in requests {
        assert_eq!(r["authOk"], true);
        assert!(r["path"].as_str().unwrap().ends_with("/virksomhed/_search"));
        assert_eq!(r["body"], request("12345678").unwrap());
    }
    let mut untrusted = Provider::new(
        &format!("{}/success", f.base),
        "synthetic",
        "synthetic-private-password",
        None,
    )
    .unwrap();
    assert_eq!(
        untrusted.lookup("12345678").unwrap_err(),
        LookupError::Unavailable
    );
}
#[test]
fn unsafe_operator_urls_and_invalid_queries_refuse_before_io() {
    for base in [
        "http://localhost",
        "https://u:p@example.invalid",
        "https://example.invalid/#x",
        "https://example.invalid/?q=secret",
        "https://example.invalid\\evil",
    ] {
        assert!(Provider::new(base, "synthetic", "secret", None).is_err());
    }
    for cvr in ["DK12345678", "1234", "../12345678", "1234567x"] {
        assert!(request(cvr).is_err());
    }
}
#[cfg(unix)]
#[test]
fn operator_ca_file_requires_private_regular_owned_inode() {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("ca.pem");
    std::fs::write(&p, b"synthetic").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(private_ca(&p).unwrap(), b"synthetic");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(private_ca(&p).is_err());
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
    let link = d.path().join("link");
    std::os::unix::fs::symlink(&p, &link).unwrap();
    assert!(private_ca(&link).is_err());
}
