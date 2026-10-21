//! Explicit remote verification of an operator-bound existing backup placement.
//! Provider credentials, paths and archive claims are never caller inputs.
use crate::{ApiClient, Error};
use reqwest::Method;
use serde_json::{Value, json};
fn id(value: &str) -> bool {
    (8..=64).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn object_id(value: &str) -> bool {
    (1..=160).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn response(value: Value, read: bool) -> Result<Value, Error> {
    if value["ok"] == true
        && (value["verification"].is_object()
            || read && value.get("verification").is_some_and(Value::is_null))
    {
        Ok(value)
    } else if read {
        Err(Error::Protocol)
    } else {
        Err(Error::OutcomeUnknown)
    }
}
impl ApiClient {
    /// Perform one confirmed metadata/download verification and retain its receipt.
    /// An uncertain reply requires GET of this exact operation ID; never auto-POST.
    pub async fn verify_backup_remote(
        &self,
        destination: &str,
        operation: &str,
        placement: &str,
        remote_object: &str,
        confirm: bool,
    ) -> Result<Value, Error> {
        if !confirm
            || !id(destination)
            || !id(operation)
            || !id(placement)
            || !object_id(remote_object)
        {
            return Err(Error::InvalidInput(
                "explicit valid destination/operation/placement/object and confirmation required",
            ));
        }
        if self.cookie.is_none() || self.service_key.is_some() {
            return Err(Error::InvalidInput(
                "current human MFA session cookie required",
            ));
        }
        let body = json!({"operationId":operation,"placementOperationId":placement,"remoteObjectId":remote_object,"confirm":true}).to_string().into_bytes();
        if body.len() > 4096 {
            return Err(Error::InvalidInput(
                "remote verification request exceeds4096 bytes",
            ));
        }
        response(
            self.request(
                Method::POST,
                &format!("/api/workspace/backup-destinations/{destination}/remote-verifications"),
                &[],
                Some(body),
            )
            .await?,
            false,
        )
    }
    /// Reconcile the original operation under current human authority. The retained
    /// observation time is historical evidence, not a claim of current cloud availability.
    pub async fn backup_remote_verification(
        &self,
        destination: &str,
        operation: &str,
    ) -> Result<Value, Error> {
        if !id(destination) || !id(operation) {
            return Err(Error::InvalidInput(
                "valid destination and operation required",
            ));
        }
        if self.cookie.is_none() || self.service_key.is_some() {
            return Err(Error::InvalidInput(
                "current human MFA session cookie required",
            ));
        }
        response(self.get(&format!("/api/workspace/backup-destinations/{destination}/remote-verifications/{operation}"),&[]).await?,true)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
        time::Duration,
    };
    fn server(
        status: &'static str,
        reply: &'static str,
    ) -> (String, mpsc::Receiver<Vec<u8>>, thread::JoinHandle<usize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = vec![];
            loop {
                let mut buffer = [0; 4096];
                let size = stream.read(&mut buffer).unwrap();
                assert!(size > 0);
                bytes.extend_from_slice(&buffer[..size]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    let len = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .map(|n| n.parse::<usize>().unwrap())
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            tx.send(bytes).unwrap();
            write!(stream,"HTTP/1.1 {status}\r\nContent-Type:application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",reply.len()).unwrap();
            drop(stream);
            listener.set_nonblocking(true).unwrap();
            thread::sleep(Duration::from_millis(60));
            1 + listener.incoming().take_while(Result::is_ok).count()
        });
        (origin, rx, worker)
    }
    #[tokio::test]
    async fn exact_small_post_and_bodyless_reconciliation_preserve_server_receipt_and_absence() {
        for read in [false, true] {
            let reply = if read {
                r#"{"ok":true,"verification":null}"#
            } else {
                r#"{"ok":true,"verification":{"operationId":"verify-0001","observedAt":"2026-01-01T00:00:00.000Z","remoteSyncVerified":true}}"#
            };
            let (origin, wire, worker) = server("200 OK", reply);
            let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
            let result = if read {
                api.backup_remote_verification("drive-0001", "verify-0001")
                    .await
            } else {
                api.verify_backup_remote(
                    "drive-0001",
                    "verify-0001",
                    "placed-0001",
                    "Drive_Object-1",
                    true,
                )
                .await
            };
            assert_eq!(
                result.unwrap(),
                serde_json::from_str::<Value>(reply).unwrap()
            );
            let bytes = wire.recv().unwrap();
            let request = String::from_utf8(bytes).unwrap();
            assert!(request.to_lowercase().contains("cookie: session=synthetic"));
            if read {
                assert!(request.starts_with("GET /api/workspace/backup-destinations/drive-0001/remote-verifications/verify-0001 HTTP/1.1"));
                assert_eq!(request.split("\r\n\r\n").nth(1).unwrap(), "");
            } else {
                assert!(request.starts_with("POST /api/workspace/backup-destinations/drive-0001/remote-verifications HTTP/1.1"));
                let body = request.split("\r\n\r\n").nth(1).unwrap();
                assert!(body.len() <= 4096);
                assert_eq!(
                    serde_json::from_str::<Value>(body).unwrap(),
                    json!({"operationId":"verify-0001","placementOperationId":"placed-0001","remoteObjectId":"Drive_Object-1","confirm":true})
                );
            }
            assert_eq!(worker.join().unwrap(), 1);
        }
    }
    #[tokio::test]
    async fn confirmation_identifiers_and_human_authority_fail_before_network() {
        let api =
            ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
        for (d, o, p, r, c) in [
            ("drive-0001", "verify-0001", "placed-0001", "Object", false),
            ("../drive", "verify-0001", "placed-0001", "Object", true),
            ("drive-0001", "VERIFY-0001", "placed-0001", "Object", true),
            ("drive-0001", "verify-0001", "bad", "Object", true),
            (
                "drive-0001",
                "verify-0001",
                "placed-0001",
                "https://provider.invalid/object",
                true,
            ),
        ] {
            assert!(matches!(
                api.verify_backup_remote(d, o, p, r, c).await,
                Err(Error::InvalidInput(_))
            ));
        }
        for api in [
            ApiClient::new("http://127.0.0.1:9", true, None).unwrap(),
            ApiClient::with_service_key("http://127.0.0.1:9", true, "synthetic-service".into())
                .unwrap(),
        ] {
            assert!(matches!(
                api.verify_backup_remote(
                    "drive-0001",
                    "verify-0001",
                    "placed-0001",
                    "Object",
                    true
                )
                .await,
                Err(Error::InvalidInput(_))
            ));
            assert!(matches!(
                api.backup_remote_verification("drive-0001", "verify-0001")
                    .await,
                Err(Error::InvalidInput(_))
            ));
        }
        assert!(object_id(&"A".repeat(160)));
        assert!(!object_id(&"A".repeat(161)));
        assert!(!object_id("æ"));
    }
    #[tokio::test]
    async fn uncertain_post_is_not_retried_and_get_keeps_current_rejection() {
        for (read, status, reply) in [
            (false, "500 Internal Server Error", r#"{"ok":false}"#),
            (false, "200 OK", r#"{"ok":true,"verification":null}"#),
            (true, "403 Forbidden", r#"{"ok":false}"#),
            (true, "200 OK", r#"{"ok":true}"#),
        ] {
            let (origin, wire, worker) = server(status, reply);
            let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
            let result = if read {
                api.backup_remote_verification("drive-0001", "verify-0001")
                    .await
            } else {
                api.verify_backup_remote("drive-0001", "verify-0001", "placed-0001", "Object", true)
                    .await
            };
            if !read {
                assert!(matches!(result, Err(Error::OutcomeUnknown)));
            } else if status.starts_with("403") {
                assert!(matches!(result, Err(Error::Refused { status: 403 })));
            } else {
                assert!(matches!(result, Err(Error::Protocol)));
            }
            wire.recv().unwrap();
            assert_eq!(worker.join().unwrap(), 1);
        }
    }
}
