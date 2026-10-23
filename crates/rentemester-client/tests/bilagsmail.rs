mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, BilagsmailMutation as Action, Error, MAX_BILAGSMAIL_BODY};
use serde_json::json;
use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};
fn fixture(
    method: &'static str,
    path: &'static str,
    body: Vec<u8>,
    status: &'static str,
    mime: &'static str,
    headers: String,
    reply: Vec<u8>,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let h = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let size = h
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .map(|s| s.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + size {
                    assert_eq!(&bytes[end + 4..], body);
                    break;
                }
            }
        }
        assert!(bytes.starts_with(format!("{method} {path} HTTP/1.1").as_bytes()));
        write!(stream,"HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\n{headers}Connection:close\r\n\r\n",reply.len()).unwrap();
        stream.write_all(&reply).unwrap();
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(60));
        assert!(listener.accept().is_err(), "never retry mailbox work");
    });
    (origin, task)
}
#[tokio::test]
async fn mailbox_operations_preserve_exact_source_methods_and_secret_body() {
    for (action, method, path, body) in [
        (
            Action::Configure,
            "POST",
            "/api/companies/synthetic/bilagsmail/imap-config",
            r#"{ "host":"imap.example.invalid", "port":993,"username":"synthetic","password":"synthetic-private-password", "secure":true,"mailbox":"INBOX" }"#,
        ),
        (
            Action::DeleteConfig,
            "DELETE",
            "/api/companies/synthetic/bilagsmail/imap-config",
            "",
        ),
        (
            Action::Alias,
            "PATCH",
            "/api/companies/synthetic/bilagsmail/alias",
            r#"{ "alias":null }"#,
        ),
        (
            Action::Poll,
            "POST",
            "/api/companies/synthetic/bilagsmail/poll",
            r#"{ "confirm":true }"#,
        ),
    ] {
        let (origin, task) = fixture(
            method,
            path,
            body.as_bytes().to_vec(),
            "200 OK",
            "application/json",
            String::new(),
            br#"{"ok":true,"future":{"retained":true}}"#.to_vec(),
        );
        let api = ApiClient::new(&origin, true, None).unwrap();
        let reply = api
            .mutate_bilagsmail("synthetic", action, body.as_bytes().to_vec(), true)
            .await
            .unwrap();
        assert_eq!(reply["future"]["retained"], true);
        assert!(!reply.to_string().contains("synthetic-private-password"));
        task.join().unwrap();
    }
}
#[tokio::test]
async fn mailbox_confirmation_unknown_fields_and_bounds_fail_before_network() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for (action, body, confirmed) in [
        (Action::Poll, json!({"confirm":true}), false),
        (Action::Poll, json!({"confirm":false}), true),
        (Action::Poll, json!({"confirm":true,"actor":"other"}), true),
        (
            Action::Configure,
            json!({"host":"h","port":993,"username":"u","password":"p","serviceKey":"forbidden"}),
            true,
        ),
        (Action::Alias, json!({}), true),
        (Action::DeleteConfig, json!({}), true),
    ] {
        assert!(matches!(
            api.mutate_bilagsmail(
                "synthetic",
                action,
                body.to_string().into_bytes(),
                confirmed
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.mutate_bilagsmail(
            "synthetic",
            Action::Configure,
            vec![b'x'; MAX_BILAGSMAIL_BODY + 1],
            true
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
}
#[tokio::test]
async fn mailbox_reads_are_pure_and_delete_5xx_remains_unknown() {
    let(origin,task)=fixture("GET","/api/companies/synthetic/bilagsmail",vec![],"200 OK","application/json",String::new(),br#"{"ok":true,"bilagsmail":{"imapConfigured":false,"polling":{"enabled":false,"state":"disabled"}}}"#.to_vec());
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.bilagsmail("synthetic").await.unwrap()["bilagsmail"]["imapConfigured"],
        false
    );
    task.join().unwrap();
    for status in ["503 Unavailable", "408 Timeout"] {
        let (origin, task) = fixture(
            "DELETE",
            "/api/companies/synthetic/bilagsmail/imap-config",
            vec![],
            status,
            "application/json",
            String::new(),
            br#"{"ok":false}"#.to_vec(),
        );
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(
            api.mutate_bilagsmail("synthetic", Action::DeleteConfig, vec![], true)
                .await
                .err(),
            Some(Error::OutcomeUnknown)
        );
        task.join().unwrap();
    }
}
#[tokio::test]
async fn inbox_original_verifies_binary_sha_and_bound_source_path() {
    let data = b"abc".to_vec();
    let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    for (headers, valid) in [
        (format!("x-content-sha256: {hash}\r\n"), true),
        (format!("x-content-sha256: {}\r\n", "0".repeat(64)), false),
        (String::new(), false),
    ] {
        let (origin, task) = fixture(
            "GET",
            "/api/companies/synthetic/workspace-inbox/source%2Bid/original",
            vec![],
            "200 OK",
            "message/rfc822",
            headers,
            data.clone(),
        );
        let api = ApiClient::new(&origin, true, None).unwrap();
        let result = api.workspace_inbox_original("synthetic", "source+id").await;
        if valid {
            let r = result.unwrap();
            assert_eq!(r.bytes, data);
            assert_eq!(r.sha256, hash);
            assert_eq!(r.content_type, "message/rfc822");
        } else {
            assert_eq!(result.err(), Some(Error::Protocol));
        }
        task.join().unwrap();
    }
}
