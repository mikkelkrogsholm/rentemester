//! Synthetic source-ABI blackbox checks, compiled after root dispatch wiring.
#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::Command,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn call(args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .env_remove("RENTEMESTER_COMPANY")
        .output()
        .unwrap()
}
#[test]
fn restore_refuses_before_credentials_or_destructive_io() {
    for confirmation in [None, Some("no"), Some("true")] {
        let mut args = vec![
            "system",
            "restore-backup",
            "--backup-dir",
            "/synthetic/not-read.tar",
            "--target-company",
            "/synthetic/not-written",
            "--json",
        ];
        if let Some(v) = confirmation {
            args.extend(["--confirm", v]);
        }
        let o = call(&args);
        assert_eq!(o.status.code(), Some(1));
        assert!(o.stderr.is_empty());
        let v: Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v["ok"], false);
        assert!(
            v["errors"][0]
                .as_str()
                .unwrap()
                .contains("Re-run with --confirm yes")
        );
    }
    let o = call(&[
        "system",
        "restore-backup",
        "--backup-dir",
        "/synthetic",
        "--target-company",
        "/synthetic-target",
        "--confirm",
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(o.stdout.is_empty());
}
#[test]
fn source_parse_errors_precede_transport() {
    for tail in [
        vec!["backup-lock"],
        vec!["backup-lock", "--enforce", "yes"],
        vec!["backup-lock", "--grace-days", "NaN"],
        vec!["backup-add-destination", "--region-eu", "1"],
        vec![
            "backup-place",
            "--archive-file",
            "x",
            "--destination",
            "d",
            "--actor-kind",
            "system",
        ],
        vec!["backup-confirm-placement", "--destination", "d"],
        vec!["backup-guide"],
        vec!["export-public-key"],
    ] {
        let mut args = vec!["system"];
        args.extend(tail);
        let o = call(&args);
        assert_eq!(
            o.status.code(),
            Some(2),
            "{args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(o.stdout.is_empty());
    }
}
fn read_request(stream: &mut impl Read) -> (String, Value) {
    let mut bytes = vec![];
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut b = [0];
        stream.read_exact(&mut b).unwrap();
        bytes.push(b[0]);
    }
    let h = String::from_utf8(bytes).unwrap();
    let n = h
        .lines()
        .find_map(|l| {
            l.to_lowercase()
                .strip_prefix("content-length:")
                .and_then(|s| s.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    let mut b = vec![0; n];
    stream.read_exact(&mut b).unwrap();
    (
        h,
        if n == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&b).unwrap()
        },
    )
}
#[test]
fn destinations_uses_passive_protocol_then_source_flat_route() {
    let listener = common::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let cookie = std::env::temp_dir().join(format!("backup716-cookie-{}", std::process::id()));
    std::fs::write(&cookie, "session=synthetic").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cookie, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let (h, b) = read_request(&mut stream);
        assert!(
            h.starts_with(
                "GET /api/companies/synthetic/original-system-backup-destination-list HTTP/1.1"
            ),
            "{h}"
        );
        assert!(b.is_null());
        let response =
            json!({"ok":true,"result":{"ok":true,"destinationCount":0,"destinations":[]}})
                .to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
    });
    let o = call(&[
        "--server",
        &origin,
        "system",
        "--company=synthetic",
        "backup-destinations",
        "--cookie-file",
        cookie.to_str().unwrap(),
        "--allow-loopback-http",
        "--json",
    ]);
    server.join().unwrap();
    std::fs::remove_file(cookie).unwrap();
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(o.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap(),
        json!({"ok":true,"destinationCount":0,"destinations":[]})
    );
}
#[test]
fn literal_values_are_not_rewritten_and_missing_transport_remains_usage() {
    for args in [
        vec![
            "--company=synthetic",
            "system",
            "backup",
            "--sign-with-ed25519",
            "--json",
        ],
        vec![
            "system",
            "backup-add-destination",
            "--label=--backup=a=b",
            "--region-eu=false",
            "--actor=user:forged",
            "--json",
        ],
    ] {
        let o = call(&args);
        assert_eq!(o.status.code(), Some(2));
        assert!(o.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&o.stderr).contains("--server or --local-profile required")
        );
    }
}
#[test]
fn signed_backup_and_public_key_export_use_common_source_routes() {
    let temp = std::env::temp_dir().join(format!("backup716-public-{}", std::process::id()));
    std::fs::create_dir(&temp).unwrap();
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let temp = std::fs::canonicalize(temp).unwrap();
    let out = temp.join("public.pem");
    let out_flag = format!("--out={}", out.display());
    let key = rentemester_runtime::original716::custody::CompanyKey::generate().unwrap();
    let pem = key.public_pem().to_owned();
    for (cmd, extra, suffix, result) in [
        (
            "backup",
            vec!["--sign-with-ed25519"],
            "original-system-backup-signed",
            json!({"ok":true,"backupId":"synthetic","backupDir":"backups/synthetic","manifestPath":"backups/synthetic/manifest.json","dbSnapshotPath":"backups/synthetic/ledger.sqlite"}),
        ),
        (
            "export-public-key",
            vec![out_flag.as_str()],
            "original-system-export-public-key",
            json!({"ok":true,"publicKeyPem":pem,"publicKeyHint":key.hint()}),
        ),
    ] {
        let listener = common::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let cookie =
            std::env::temp_dir().join(format!("backup716-key-cookie-{}-{cmd}", std::process::id()));
        std::fs::write(&cookie, "session=synthetic").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&cookie, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let mut expected = result.clone();
        if cmd == "export-public-key" {
            expected.as_object_mut().unwrap().remove("publicKeyPem");
            expected["outPath"] = json!(out);
        }
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let (h, b) = read_request(&mut stream);
            assert!(
                h.starts_with(&format!("POST /api/companies/synthetic/{suffix} HTTP/1.1")),
                "{h}"
            );
            if cmd == "backup" {
                assert_eq!(b["signWithEd25519"], true);
                assert_eq!(b["confirm"], true);
            }
            assert!(b.get("privateKey").is_none());
            let response = json!({"ok":true,"result":result}).to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
        });
        let mut args = vec!["system", cmd];
        args.extend(extra);
        args.extend([
            "--company=synthetic",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--json",
        ]);
        let o = call(&args);
        server.join().unwrap();
        std::fs::remove_file(cookie).unwrap();
        assert!(o.status.success(), "{o:?}");
        assert_eq!(
            serde_json::from_slice::<Value>(&o.stdout).unwrap(),
            expected
        );
        if cmd == "export-public-key" {
            assert_eq!(std::fs::read_to_string(&out).unwrap(), pem);
        }
    }
    std::fs::remove_dir_all(temp).unwrap();
}
