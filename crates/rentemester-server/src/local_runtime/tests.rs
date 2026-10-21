use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture {
    path: PathBuf,
    identity: Identity,
}
impl Fixture {
    fn new() -> Self {
        let (stream, _other) = UnixStream::pair().unwrap();
        let uid = stream.peer_cred().unwrap().uid();
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "rm138-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(path.join("workspace")).unwrap();
        let config = path.join("server.json");
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&config)
            .unwrap()
            .write_all(b"{}")
            .unwrap();
        Self {
            identity: Identity {
                config,
                root: path.join("workspace"),
                workspace: "synthetic-local".into(),
                uid,
            },
            path,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
#[tokio::test]
async fn publication_busy_stale_reopen_and_owned_cleanup() {
    let f = Fixture::new();
    let mut runtime = Runtime::bind(&f.path, &f.identity, "http://127.0.0.1:40101")
        .await
        .unwrap();
    let record = fs::read(f.path.join("r.json")).unwrap();
    assert!(
        Runtime::bind(&f.path, &f.identity, "http://127.0.0.1:40102")
            .await
            .is_err()
    );
    assert_eq!(fs::read(f.path.join("r.json")).unwrap(), record);
    let listener = runtime.take_listener().unwrap();
    drop(listener);
    runtime.socket = None;
    runtime.descriptor = None;
    drop(runtime);
    let runtime = Runtime::bind(&f.path, &f.identity, "http://127.0.0.1:40102")
        .await
        .unwrap();
    let (d, _) = descriptor(&f.path.join("r.json"), f.identity.uid).unwrap();
    assert_eq!(d.origin, "http://127.0.0.1:40102");
    assert_eq!(d.workspace_root, f.identity.root);
    assert_eq!(d.socket, "r.sock");
    drop(runtime);
    assert!(!f.path.join("r.json").exists());
    assert!(!f.path.join("r.sock").exists());
}
#[tokio::test]
async fn unsafe_or_mismatched_entries_are_preserved() {
    let f = Fixture::new();
    let socket = f.path.join("r.sock");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&socket)
        .unwrap();
    file.write_all(b"unrelated file").unwrap();
    assert!(
        Runtime::bind(&f.path, &f.identity, "http://127.0.0.1:40101")
            .await
            .is_err()
    );
    assert_eq!(fs::read(&socket).unwrap(), b"unrelated file");
    fs::remove_file(&socket).unwrap();
    std::os::unix::fs::symlink(f.path.join("server.json"), &socket).unwrap();
    assert!(
        Runtime::bind(&f.path, &f.identity, "http://127.0.0.1:40101")
            .await
            .is_err()
    );
    assert!(
        fs::symlink_metadata(&socket)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fs::remove_file(&socket).unwrap();
    let d = Descriptor {
        format: "rentemester-local-runtime/1".into(),
        origin: "http://127.0.0.1:40101".into(),
        socket: "r.sock".into(),
        workspace_root: f.identity.root.clone(),
        workspace_id: "wrong-workspace".into(),
    };
    let bytes = serde_json::to_vec(&d).unwrap();
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(f.path.join("r.json"))
        .unwrap()
        .write_all(&bytes)
        .unwrap();
    assert!(
        Runtime::bind(&f.path, &f.identity, "http://127.0.0.1:40101")
            .await
            .is_err()
    );
    assert_eq!(fs::read(f.path.join("r.json")).unwrap(), bytes);
}
#[tokio::test]
async fn replacement_cleanup_and_unsafe_profile_never_remove_unowned_files() {
    let f = Fixture::new();
    let mut runtime = Runtime::bind(&f.path, &f.identity, "http://127.0.0.1:40101")
        .await
        .unwrap();
    fs::rename(f.path.join("r.json"), f.path.join("original.json")).unwrap();
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(f.path.join("r.json"))
        .unwrap()
        .write_all(b"replacement")
        .unwrap();
    assert!(runtime.cleanup().is_err());
    drop(runtime);
    assert_eq!(fs::read(f.path.join("r.json")).unwrap(), b"replacement");
    fs::set_permissions(&f.path, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(preflight(&f.path, &f.identity.config).is_err());
    fs::set_permissions(&f.path, fs::Permissions::from_mode(0o700)).unwrap();
    let long = f.path.join("x".repeat(100));
    fs::create_dir(&long).unwrap();
    fs::set_permissions(&long, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(preflight(&long, &long.join("server.json")).is_err());
}
