mod common;
use rentemester_client::{ApiClient, Error, system_backup_lock_original122_body_schema};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};

fn status_core() -> Value {
    json!({"ok":true,"appliedRules":["DK-BOOKKEEPING-BACKUP-001"],"latestBackupAt":null,"latestBackupId":null,"backupDue":false,"hasActivitySinceBackup":false,"daysSinceLatestBackup":null,"requiredBy":null,"checkedAt":"2026-10-25T07:00:00.000Z","backupsFound":0,"evidence":{"latestJournalEntryAt":null,"latestDocumentAt":null,"latestBankImportAt":null},"errors":[]})
}
fn lock_core() -> Value {
    json!({"ok":true,"appliedRules":["DK-BOOKKEEPING-BACKUP-001"],"config":{"version":"backup-lock-v1","enforced":false,"graceDays":0,"updatedAt":"2026-01-01T00:00:00.000Z","updatedBy":"user:synthetic"},"errors":[]})
}
async fn invoke(
    lock: bool,
    date: Option<&str>,
    bytes: Vec<u8>,
    status: u16,
    response: Value,
    service: bool,
) -> (Result<Value, Error>, String) {
    let listener = common::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut wire = Vec::new();
        while !wire.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            s.read_exact(&mut b).unwrap();
            wire.push(b[0]);
        }
        let header = String::from_utf8(wire.clone()).unwrap();
        let len: usize = header
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|n| n.trim().parse().unwrap())
            })
            .unwrap_or(0);
        let mut body = vec![0; len];
        s.read_exact(&mut body).unwrap();
        wire.extend(body);
        let response = response.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Reply\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{response}",
            response.len()
        )
        .unwrap();
        drop(s);
        listener.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(50));
        assert!(listener.accept().is_err(), "no automatic backup retries");
        String::from_utf8(wire).unwrap()
    });
    let api = if service {
        ApiClient::with_service_key(&origin, true, "synthetic-key".into()).unwrap()
    } else {
        ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap()
    };
    let result = if lock {
        api.system_backup_lock_original122("synthetic", bytes, true)
            .await
    } else {
        api.system_backup_status_original122("synthetic", date)
            .await
    };
    (result, task.join().unwrap())
}
#[tokio::test]
async fn backup_original122_status_exact_query_current_credentials_and_business_false() {
    for service in [false, true] {
        for date in [
            None,
            Some("2026-01-01"),
            Some("2026-01-01T01:00:00+01:00"),
            Some("bad&actor=forged"),
            Some(""),
        ] {
            let mut core = status_core();
            if date.is_some_and(|s| s.is_empty() || s.starts_with("bad")) {
                core["ok"] = json!(false);
                core["errors"] = json!(["invalid asOf"]);
            }
            let (got, wire) = invoke(
                false,
                date,
                vec![],
                200,
                json!({"ok":true,"backupStatus":core}),
                service,
            )
            .await;
            assert_eq!(got.unwrap(), core);
            let target = wire
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap();
            let url = reqwest::Url::parse(&format!("http://localhost{target}")).unwrap();
            assert_eq!(url.path(), "/api/companies/synthetic/system-backup-status");
            assert_eq!(
                url.query_pairs().collect::<Vec<_>>(),
                date.map(|s| vec![("asOf".into(), s.into())])
                    .unwrap_or_default()
            );
            assert!(wire.ends_with("\r\n\r\n"));
            assert_eq!(wire.to_lowercase().contains("cookie:"), !service);
            assert_eq!(
                wire.to_lowercase().contains("x-rentemester-service-key:"),
                service
            );
        }
    }
}
#[tokio::test]
async fn backup_original122_lock_exact_optional_body_and_domain_refusal() {
    for bytes in [
        br#"{ "confirm":true }"#.to_vec(),
        br#"{"confirm":true,"graceDays":0,"enforced":false}"#.to_vec(),
        br#"{"confirm":true,"graceDays":366}"#.to_vec(),
        br#"{"confirm":true,"graceDays":4294967295}"#.to_vec(),
        br#"{"confirm":true,"graceDays":-1.5,"at":"invalid"}"#.to_vec(),
    ] {
        let mut core = lock_core();
        if bytes.windows(4).any(|s| s == b"-1.5") {
            core.as_object_mut().unwrap().remove("config");
            core["ok"] = json!(false);
            core["errors"] = json!(["graceDays must be a non-negative integer"]);
        }
        let (got, wire) = invoke(
            true,
            None,
            bytes.clone(),
            200,
            json!({"ok":true,"backupLock":core}),
            true,
        )
        .await;
        assert_eq!(got.unwrap(), core);
        assert!(wire.starts_with("POST /api/companies/synthetic/system-backup-lock HTTP/1.1"));
        assert_eq!(wire.split_once("\r\n\r\n").unwrap().1.as_bytes(), bytes);
    }
}
#[tokio::test]
async fn backup_original122_protocol_refusal_and_unknown_writes_never_retry() {
    for lock in [false, true] {
        for response in [
            json!({}),
            json!({"ok":true,"backupStatus":{"ok":true}}),
            json!({"ok":false}),
        ] {
            let got = invoke(
                lock,
                None,
                br#"{"confirm":true}"#.to_vec(),
                200,
                response,
                false,
            )
            .await
            .0;
            assert!(if lock {
                matches!(got, Err(Error::OutcomeUnknown))
            } else {
                matches!(got, Err(Error::Protocol))
            });
        }
        for status in [401, 403, 409, 429, 503] {
            let got = invoke(
                lock,
                None,
                br#"{"confirm":true}"#.to_vec(),
                status,
                json!({"ok":false}),
                false,
            )
            .await
            .0;
            if lock && status == 503 {
                assert!(matches!(got, Err(Error::OutcomeUnknown)));
            } else {
                assert!(matches!(got,Err(Error::Refused{status:s}) if s==status));
            }
        }
    }
}
#[tokio::test]
async fn backup_original122_structural_guards_refuse_before_network() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let api = ApiClient::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        true,
        Some("session=synthetic".into()),
    )
    .unwrap();
    for company in [
        "",
        "../synthetic",
        "synthetic/nested",
        "/unverified/synthetic",
        "synthetic?at=x",
    ] {
        assert!(matches!(
            api.system_backup_status_original122(company, None).await,
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            api.system_backup_lock_original122(company, br#"{"confirm":true}"#.to_vec(), true)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    for bytes in [
        br#"{"confirm":false}"#.to_vec(),
        br#"{"confirm":true,"actor":"forged"}"#.to_vec(),
        br#"{"confirm":true,"graceDays":"3"}"#.to_vec(),
        br#"{"confirm":true,"asOf":"2026-01-01"}"#.to_vec(),
        vec![b' '; 4097],
    ] {
        assert!(matches!(
            api.system_backup_lock_original122("synthetic", bytes, true)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.system_backup_lock_original122("synthetic", br#"{"confirm":true}"#.to_vec(), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(
        system_backup_lock_original122_body_schema()["required"],
        json!(["confirm"])
    );
    listener.set_nonblocking(true).unwrap();
    assert!(listener.accept().is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn backup_original122_verified_local_selectors_use_unix_and_reverify_root() {
    use rentemester_client::LocalRuntime;
    use std::{
        fs,
        os::unix::{
            fs::{DirBuilderExt, PermissionsExt},
            net::UnixListener,
        },
    };
    let root = fs::canonicalize(std::env::var_os("HOME").unwrap())
        .unwrap()
        .join(format!(".rm122-backup-{}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(root.join("workspace"))
        .unwrap();
    let listener = UnixListener::bind(root.join("r.sock")).unwrap();
    fs::set_permissions(root.join("r.sock"), fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(root.join("r.json"),json!({"format":"rentemester-local-runtime/1","origin":"http://127.0.0.1:9","socket":"r.sock","workspaceRoot":root.join("workspace"),"workspaceId":"synthetic-workspace"}).to_string()).unwrap();
    fs::set_permissions(root.join("r.json"), fs::Permissions::from_mode(0o600)).unwrap();
    let local = LocalRuntime::load(&root).unwrap();
    let api = ApiClient::new_local(&local, Some("session=synthetic".into())).unwrap();
    for suffix in [
        "workspace/../synthetic",
        "workspace/nested/synthetic",
        "elsewhere/synthetic",
        "workspace//synthetic",
        "workspace/synthetic/",
    ] {
        assert!(matches!(
            api.system_backup_status_original122(root.join(suffix).to_str().unwrap(), None)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    listener.set_nonblocking(true).unwrap();
    assert!(listener.accept().is_err());
    listener.set_nonblocking(false).unwrap();
    let expected = status_core();
    let reply = json!({"ok":true,"backupStatus":expected}).to_string();
    let task = thread::spawn(move || {
        let (mut stream, _) = common::unix_accept(&listener).unwrap();
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            stream.read_exact(&mut b).unwrap();
            bytes.push(b[0]);
        }
        assert!(
            String::from_utf8(bytes)
                .unwrap()
                .starts_with("GET /api/companies/synthetic/system-backup-status HTTP/1.1")
        );
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        )
        .unwrap();
    });
    // The secondary company need not exist locally: only the server inspects it.
    assert_eq!(
        api.system_backup_status_original122(
            root.join("workspace/synthetic").to_str().unwrap(),
            None
        )
        .await
        .unwrap(),
        expected
    );
    task.join().unwrap();
    fs::set_permissions(root.join("workspace"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        api.system_backup_status_original122(
            root.join("workspace/synthetic").to_str().unwrap(),
            None
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn backup_original122_overdue_business_false_with_empty_errors_is_real_status() {
 let mut core=status_core();core["ok"]=json!(false);core["backupDue"]=json!(true);core["hasActivitySinceBackup"]=json!(true);core["errors"]=json!([]);
 for service in [false,true] {
  let (got,_)=invoke(false,None,vec![],200,json!({"ok":true,"backupStatus":core}),service).await;
  assert_eq!(got.unwrap(),core);
 }
}
