//! Work-Issue: #717. Synthetic HTTP contract evidence; no provider calls.
mod common;
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
use std::io::{Read, Write};
fn input(name: &str) -> Value {
    match name {
        "system_restore_backup" => {
            json!({"backupDir":"/synthetic/b","targetCompany":"/synthetic/t","confirm":true})
        }
        "system_backup_place" => {
            json!({"company":"synthetic","archivePath":"/synthetic/a.tar","destinationId":"cold","actorKind":"human","confirm":true})
        }
        "system_backup_verify_remote_placement" => {
            json!({"company":"synthetic","destinationId":"cold","backupId":"b1","remoteObjectId":"object1","confirm":true})
        }
        "mail_intake_ingest" => {
            json!({"company":"synthetic","source":"/synthetic/nonexistent.eml","confirm":true})
        }
        "efaktura_konfigurer" => {
            json!({"company":"synthetic","apiLicenseKey":"synthetic-private","confirm":true})
        }
        _ => json!({"company":"synthetic","confirm":true}),
    }
}
async fn wire(name: &str, args: Value, status: u16, reply: Value) -> Result<Value, Error> {
    let listener = common::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let expected = args.clone();
    let name = name.to_owned();
    let operation = name.clone();
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut b = [0; 4096];
            let n = stream.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(p) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..p]).to_lowercase();
                let len = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() >= p + 4 + len {
                    assert!(headers.starts_with("post /api/companies/synthetic/original-"));
                    assert!(!headers.contains("idempotency-key:"));
                    assert!(!headers.contains("authorization:"));
                    assert!(headers.contains("cookie: session=synthetic"));
                    assert!(headers.contains("x-rentemester-protocol:"));
                    let body: Value = serde_json::from_slice(&bytes[p + 4..]).unwrap();
                    let mut expected = expected;
                    expected.as_object_mut().unwrap().remove("company");
                    if name == "mail_intake_ingest" {
                        expected.as_object_mut().unwrap().remove("source");
                        assert!(body["messages"].is_array());
                        let mut actual = body.clone();
                        actual.as_object_mut().unwrap().remove("messages");
                        assert_eq!(actual, expected);
                    } else {
                        assert_eq!(body, expected);
                    }
                    break;
                }
            }
        }
        let reply = reply.to_string();
        write!(stream,"HTTP/1.1 {status} Synthetic\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",reply.len()).unwrap();
    });
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    let result = api.original717(&operation, &args).await;
    worker.join().unwrap();
    result
}
async fn exercise(name: &str, success: Value) {
    let a = input(name);
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    let mut bad = a.clone();
    bad["confirm"] = json!("true");
    assert!(matches!(
        api.original717(name, &bad).await,
        Err(Error::InvalidInput(_))
    ));
    bad = a.clone();
    bad["unexpected"] = json!(true);
    assert!(matches!(
        api.original717(name, &bad).await,
        Err(Error::InvalidInput(_))
    ));
    bad = a.clone();
    bad["confirm"] = json!(false);
    assert!(matches!(
        api.original717(name, &bad).await,
        Err(Error::InvalidInput(_))
    ));
    if name == "system_restore_backup" {
        assert!(matches!(
            api.original717(name, &a).await,
            Err(Error::InvalidInput(_))
        ));
        return;
    }
    let mut a = a;
    let file = std::env::temp_dir().join(format!("original717-{}-mail.eml", std::process::id()));
    if name == "mail_intake_ingest" {
        std::fs::write(&file, b"Message-ID: <synthetic>\r\n\r\nsynthetic").unwrap();
        a["source"] = json!(file.to_str().unwrap());
    }
    assert_eq!(
        wire(name, a.clone(), 200, json!({"ok":true,"result":success}))
            .await
            .unwrap(),
        success
    );
    let duplicate = if name == "invoice_send_email" {
        json!({"ok":true,"duplicate":true,"invoiceNumber":"S1","kind":"invoice","subject":"Synthetic invoice","recipient":"synthetic@example.invalid","messageId":"synthetic","errors":[]})
    } else {
        success.clone()
    };
    assert_eq!(
        wire(name, a.clone(), 200, json!({"ok":true,"result":duplicate}))
            .await
            .unwrap(),
        duplicate
    );
    let refusal = json!({"ok":false,"errors":["synthetic domain refusal"]});
    assert_eq!(
        wire(name, a.clone(), 200, json!({"ok":true,"result":refusal}))
            .await
            .unwrap(),
        refusal
    );
    assert_eq!(
        wire(
            name,
            a.clone(),
            200,
            json!({"ok":true,"result":{"ok":true}})
        )
        .await,
        Err(Error::Protocol)
    );
    assert_eq!(
        wire(name, a.clone(), 403, json!({"ok":false})).await,
        Err(Error::Refused { status: 403 })
    );
    assert_eq!(
        wire(name, a.clone(), 503, json!({"ok":false})).await,
        Err(Error::OutcomeUnknown)
    );
    let mut outside = a;
    outside["company"] = json!("/outside/synthetic");
    assert!(matches!(
        api.original717(name, &outside).await,
        Err(Error::InvalidInput(_))
    ));
    if name == "mail_intake_ingest" {
        std::fs::remove_file(file).unwrap();
    }
}
#[tokio::test]
async fn original717_system_backup() {
    exercise("system_backup",json!({"ok":true,"backupId":"b1","backupDir":"backups/b1","manifestPath":"backups/b1/manifest.json","dbSnapshotPath":"backups/b1/ledger.sqlite","errors":[]})).await;
}
#[tokio::test]
async fn original717_system_backup_archive() {
    exercise("system_backup_archive",json!({"ok":true,"backupId":"b1","archivePath":"b1.tar","archiveSha256":"a".repeat(64),"sha256Path":"b1.tar.sha256","archiveSizeBytes":1024,"errors":[]})).await;
}
#[tokio::test]
async fn original717_system_backup_place() {
    exercise("system_backup_place",json!({"ok":true,"placement":{"backupId":"b1","actorKind":"human"},"copiedTo":"cold/b1.tar","errors":[]})).await;
}
#[tokio::test]
async fn original717_system_backup_verify_remote_placement() {
    exercise(
        "system_backup_verify_remote_placement",
        json!({"ok":true,"placement":{"backupId":"b1","verified":true},"errors":[]}),
    )
    .await;
}
#[tokio::test]
async fn original717_system_restore_backup() {
    exercise("system_restore_backup", json!({})).await;
}
#[tokio::test]
async fn original717_mail_intake_ingest() {
    exercise("mail_intake_ingest",json!({"ok":true,"messagesProcessed":1,"attachmentsIngested":1,"attachmentsSkipped":0,"exceptionsCreated":0,"documents":[]})).await;
}
#[tokio::test]
async fn original717_imap_intake_poll() {
    exercise("imap_intake_poll",json!({"ok":true,"messagesFetched":1,"messagesProcessed":1,"attachmentsIngested":1,"attachmentsSkipped":0,"exceptionsCreated":0,"documents":[]})).await;
}
#[tokio::test]
async fn original717_invoice_send_email() {
    exercise("invoice_send_email",json!({"ok":true,"duplicate":false,"invoiceNumber":"S1","kind":"invoice","subject":"Synthetic invoice","recipient":"synthetic@example.invalid","messageId":"synthetic","errors":[]})).await;
}
#[tokio::test]
async fn original717_efaktura_konfigurer() {
    exercise(
        "efaktura_konfigurer",
        json!({"ok":true,"configPath":"config/digisense.json","environment":"test"}),
    )
    .await;
}
#[cfg(unix)]
#[tokio::test]
async fn original717_system_restore_backup_local_operator_wire_and_reconnect() {
    use std::os::unix::{
        fs::{DirBuilderExt, PermissionsExt},
        net::UnixListener,
    };
    let home = std::fs::canonicalize(std::env::var_os("HOME").unwrap()).unwrap();
    let path = home.join(format!(".rm717-{}", std::process::id()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&path)
        .unwrap();
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(path.join("workspace"))
        .unwrap();
    let listener = UnixListener::bind(path.join("r.sock")).unwrap();
    std::fs::set_permissions(path.join("r.sock"), std::fs::Permissions::from_mode(0o600)).unwrap();
    let descriptor = json!({"format":"rentemester-local-runtime/1","origin":"http://127.0.0.1:18717","socket":"r.sock","workspaceRoot":path.join("workspace"),"workspaceId":"synthetic-workspace"});
    std::fs::write(path.join("r.json"), descriptor.to_string()).unwrap();
    std::fs::set_permissions(path.join("r.json"), std::fs::Permissions::from_mode(0o600)).unwrap();
    let local = rentemester_client::LocalRuntime::load(&path).unwrap();
    let api = ApiClient::new_local(&local, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.original717_company_selector(path.join("workspace/synthetic").to_str().unwrap())
            .unwrap(),
        "synthetic"
    );
    let cwd = std::env::current_dir().unwrap();
    let target = cwd.join("synthetic-restored");
    let a = json!({"backupDir":"./synthetic-backup","targetCompany":"./unused/../synthetic-restored","verifyKey":"./synthetic-key","confirm":true,"confirmText":"RESTORE ./unused/../synthetic-restored"});
    let mut expected = a.clone();
    expected["backupDir"] = json!(cwd.join("synthetic-backup"));
    expected["targetCompany"] = json!(target);
    expected["verifyKey"] = json!(cwd.join("synthetic-key"));
    expected["confirmText"] = json!(format!("RESTORE {}", target.display()));
    let mut wrong_confirmation = a.clone();
    wrong_confirmation["confirmText"] = json!("RESTORE another-target");
    assert!(matches!(
        api.original717("system_restore_backup", &wrong_confirmation)
            .await,
        Err(Error::InvalidInput(_))
    ));
    let reply = json!({"ok":true,"backupId":"b1","restoredAt":"2026-01-01T00:00:00Z","targetCompanyRoot":target,"restoredDbPath":target.join("data/ledger.sqlite"),"restoredFiles":{"documentsOriginals":0,"invoicesIssued":0,"config":0},"errors":[]});
    let r = reply.clone();
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = common::unix_accept(&listener).unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut b = [0; 4096];
            let n = stream.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(p) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..p]).to_lowercase();
                let len = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() >= p + 4 + len {
                    assert!(headers.starts_with("post /api/original-system-restore-backup "));
                    assert!(!headers.contains("idempotency-key:"));
                    assert_eq!(
                        serde_json::from_slice::<Value>(&bytes[p + 4..]).unwrap(),
                        expected
                    );
                    break;
                }
            }
        }
        let body = json!({"ok":true,"result":r}).to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    });
    assert_eq!(
        api.original717("system_restore_backup", &a).await.unwrap(),
        reply
    );
    worker.join().unwrap();
    // Replaced logical server descriptor is not a permission to reconnect elsewhere.
    let mut changed = descriptor;
    changed["origin"] = json!("http://127.0.0.1:18718");
    std::fs::write(path.join("r.json"), changed.to_string()).unwrap();
    assert!(api.original717("system_restore_backup", &a).await.is_err());
    assert!(
        api.original717_company_selector(path.join("workspace/synthetic").to_str().unwrap())
            .is_err()
    );
    std::fs::remove_dir_all(path).unwrap();
}
