use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};
const HASH: &str = "d157b0f801b28bbb6a6fe392fe76d5dcdfd0e2e17e7c50b8c5494d9ef7e195dd";
// Passive standalone manifest with the two frozen additions and canonical manifestId.
const MANIFEST: &str = r#"{"format":"rentemester-http-protocol/1","protocolMajor":1,"wireProfiles":["legacy-react-v1","rust-http-v1"],"operations":[{"method":"POST","path":"/api/companies/{slug}/workspace-parties/{id}/approve-merge","capability":"POST /api/companies/{slug}/workspace-parties/{id}/approve-merge"},{"method":"POST","path":"/api/companies/{slug}/workspace-parties/{id}/propose-merge","capability":"POST /api/companies/{slug}/workspace-parties/{id}/propose-merge"},{"method":"GET","path":"/api/me","capability":"GET /api/me"}],"manifestId":"f0a51ed7fa95da80416d27bf8ddbd6720d2a77e65417f786e514c12673a0b319"}"#;
fn party() -> Value {
    json!({"partyId":"source","kind":"organization","name":"Synthetic","source":"fixture","observedAt":"2026-01-01T00:00:00Z","reviewAssertion":"Reviewed","identifiers":[],"aliases":[],"assertions":[],"roles":[],"history":[
 {"event_type":"created","payload_hash":"a".repeat(64),"actor":"user:synthetic","created_at":"2026-01-01T00:00:00Z"},
 {"event_type":"proposed_merge","payload_hash":HASH,"actor":"user:synthetic","created_at":"2026-01-01T00:00:00Z"},
 {"event_type":"approved_merge","payload_hash":HASH,"actor":"user:synthetic","created_at":"2026-01-01T00:00:00Z"},
 {"event_type":"superseded","payload_hash":HASH,"actor":"user:synthetic","created_at":"2026-01-01T00:00:00Z"}]})
}
fn reply(op: &str) -> Value {
    if op == "propose" {
        json!({"ok":true,"proposalHash":HASH})
    } else {
        json!({"ok":true,"party":party()})
    }
}
fn body(op: &str) -> Value {
    if op == "propose" {
        json!({"intoPartyId":"target","reviewAssertion":"Reviewed\nsynthetic","confirm":true})
    } else {
        json!({"proposalHash":HASH,"confirm":true})
    }
}
fn fixture(
    response: Value,
    status: u16,
    protocol: bool,
) -> (String, thread::JoinHandle<Vec<(String, Value)>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    l.set_nonblocking(true).unwrap();
    let worker = thread::spawn(move || {
        let mut writes = Vec::new();
        let mut idle = Instant::now();
        while idle.elapsed() < Duration::from_secs(5) {
            let (mut s, _) = match l.accept() {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            idle = Instant::now();
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut bytes = Vec::new();
            while !bytes.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                s.read_exact(&mut b).unwrap();
                bytes.push(b[0]);
            }
            let header = String::from_utf8(bytes.clone()).unwrap();
            let lower = header.to_lowercase();
            let n = lower
                .lines()
                .find_map(|l| {
                    l.strip_prefix("content-length:")
                        .map(|n| n.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            let mut data = vec![0; n];
            s.read_exact(&mut data).unwrap();
            let (code, text) = if header.starts_with("GET /api/protocol ") {
                assert!(
                    !lower.contains("cookie:")
                        && !lower.contains("authorization:")
                        && !lower.contains("x-rentemester-protocol:")
                );
                (
                    200,
                    if protocol {
                        MANIFEST.to_owned()
                    } else {
                        "{}".into()
                    },
                )
            } else {
                assert!(lower.contains("cookie: session=synthetic"));
                assert!(lower.contains("x-rentemester-manifest:"));
                assert!(
                    !lower.contains("authorization:")
                        && !lower.contains("x-rentemester-service-key:")
                );
                if header.starts_with("GET /api/me ") {
                    (
                        200,
                        json!({"ok":true,"user":{"id":"synthetic"}}).to_string(),
                    )
                } else {
                    writes.push((
                        header.lines().next().unwrap().into(),
                        serde_json::from_slice(&data).unwrap(),
                    ));
                    (status, response.to_string())
                }
            };
            write!(s,"HTTP/1.1 {code} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",text.len()).unwrap();
        }
        writes
    });
    (origin, worker)
}
use rentemester_client::{ApiClient, Error, PartyMutation};
fn action(op: &str) -> PartyMutation {
    if op == "propose" {
        PartyMutation::ProposeMerge("source".into())
    } else {
        PartyMutation::ApproveMerge("source".into())
    }
}
#[tokio::test]
async fn exact_wire_passive_negotiation_and_current_cookie() {
    for op in ["propose", "approve"] {
        let (origin, server) = fixture(reply(op), 200, true);
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            api.mutate_workspace_party(
                "synthetic",
                action(op),
                body(op).to_string().into_bytes(),
                true
            )
            .await
            .unwrap(),
            reply(op)
        );
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].0,
            format!("POST /api/companies/synthetic/workspace-parties/source/{op}-merge HTTP/1.1")
        );
        assert_eq!(requests[0].1, body(op));
    }
}
#[tokio::test]
async fn strict_shape_confirm_utf16_and_hidden_authority_before_network() {
    let api = ApiClient::new("http://127.0.0.1:1", true, Some("session=synthetic".into())).unwrap();
    for op in ["propose", "approve"] {
        let valid = body(op);
        assert!(matches!(
            api.mutate_workspace_party(
                "synthetic",
                action(op),
                valid.to_string().into_bytes(),
                false
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
        for key in [
            "actor",
            "observedAt",
            "authority",
            "principal",
            "idempotencyKey",
            "unexpected",
        ] {
            let mut b = valid.clone();
            b[key] = json!("caller");
            assert!(matches!(
                api.mutate_workspace_party(
                    "synthetic",
                    action(op),
                    b.to_string().into_bytes(),
                    true
                )
                .await,
                Err(Error::InvalidInput(_))
            ));
        }
        for confirm in [json!(false), json!("true"), json!(null)] {
            let mut b = valid.clone();
            b["confirm"] = confirm;
            assert!(matches!(
                api.mutate_workspace_party(
                    "synthetic",
                    action(op),
                    b.to_string().into_bytes(),
                    true
                )
                .await,
                Err(Error::InvalidInput(_))
            ));
        }
    }
    for review in ["😀".repeat(251), " ".into()] {
        let mut b = body("propose");
        b["reviewAssertion"] = json!(review);
        assert!(matches!(
            api.mutate_workspace_party(
                "synthetic",
                action("propose"),
                b.to_string().into_bytes(),
                true
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    for h in ["a".repeat(63), "A".repeat(64), "g".repeat(64)] {
        let b = json!({"confirm":true,"proposalHash":h});
        assert!(matches!(
            api.mutate_workspace_party(
                "synthetic",
                action("approve"),
                b.to_string().into_bytes(),
                true
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
}
#[tokio::test]
async fn no_retry_and_no_fabricated_receipt_or_protocol_bypass() {
    for (op, status, response, protocol) in [
        ("propose", 500, reply("propose"), true),
        (
            "propose",
            200,
            json!({"ok":true,"proposalHash":"a".repeat(64)}),
            true,
        ),
        (
            "approve",
            200,
            json!({"ok":true,"party":{"partyId":"source"}}),
            true,
        ),
        (
            "approve",
            200,
            {
                let mut r = reply("approve");
                r["party"]["partyId"] = json!("wrong");
                r
            },
            true,
        ),
        (
            "approve",
            200,
            {
                let mut r = reply("approve");
                r["party"]["history"][3]["payload_hash"] = json!("a".repeat(64));
                r
            },
            true,
        ),
        ("propose", 200, reply("propose"), false),
    ] {
        let (origin, server) = fixture(response, status, protocol);
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let r = api
            .mutate_workspace_party(
                "synthetic",
                action(op),
                body(op).to_string().into_bytes(),
                true,
            )
            .await;
        assert!(if protocol {
            matches!(r, Err(Error::OutcomeUnknown))
        } else {
            matches!(r, Err(Error::Protocol))
        });
        assert_eq!(server.join().unwrap().len(), usize::from(protocol));
    }
}
