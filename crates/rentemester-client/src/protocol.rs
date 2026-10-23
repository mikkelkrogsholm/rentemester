//! Passive protocol admission. The manifest is fetched for each send: no stale,
//! process-global cache and no credential-bearing discovery request.
use crate::{ApiClient, Error};
use reqwest::Method;
use serde::Deserialize;
use std::collections::HashSet;

const MAX_MANIFEST: usize = 1024 * 1024;
const PROFILE: &str = "rust-http-v1";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format: String,
    protocol_major: u64,
    manifest_id: String,
    wire_profiles: Vec<String>,
    operations: Vec<Operation>,
}
#[derive(Deserialize)]
struct Operation {
    method: String,
    path: String,
    capability: String,
}

fn valid_path(path: &str) -> bool {
    path.starts_with("/api/")
        && path.split('/').skip(1).all(|segment| {
            !segment.is_empty()
                && !segment.contains(['?', '#', '*', '%', '\\'])
                && if segment.starts_with('{') && segment.ends_with('}') {
                    let name = &segment[1..segment.len() - 1];
                    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                } else {
                    !segment.contains(['{', '}'])
                }
        })
}
fn matches(template: &str, path: &str) -> bool {
    let a: Vec<_> = template.split('/').collect();
    let b: Vec<_> = path.split('/').collect();
    a.len() == b.len()
        && a.iter().zip(b).all(|(expected, actual)| {
            if expected.starts_with('{') {
                !actual.is_empty()
            } else {
                *expected == actual
            }
        })
}
impl Manifest {
    fn admit(self, method: &Method, path: &str) -> Result<String, Error> {
        if self.format != "rentemester-http-protocol/1"
            || self.protocol_major != 1
            || self.manifest_id.len() != 64
            || !self
                .manifest_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !self.wire_profiles.iter().any(|p| p == PROFILE)
            || self
                .wire_profiles
                .iter()
                .any(|p| !matches!(p.as_str(), "legacy-react-v1" | PROFILE))
        {
            return Err(Error::Protocol);
        }
        let mut seen = HashSet::new();
        let mut supported = false;
        for op in self.operations {
            if !matches!(
                op.method.as_str(),
                "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE"
            ) || !valid_path(&op.path)
                || op.capability != format!("{} {}", op.method, op.path)
                || !seen.insert((op.method.clone(), op.path.clone()))
            {
                return Err(Error::Protocol);
            }
            supported |= op.method == method.as_str() && matches(&op.path, path);
        }
        if supported {
            Ok(self.manifest_id)
        } else {
            Err(Error::Protocol)
        }
    }
}
impl ApiClient {
    pub(super) async fn negotiate(&self, method: &Method, path: &str) -> Result<String, Error> {
        // Deliberately bare: same verified Unix/TLS transport, no Cookie, service
        // key, authorization, ambient cookie jar, redirects or automatic retry.
        let url = self
            .base
            .join("/api/protocol")
            .map_err(|_| Error::Protocol)?;
        let mut response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|_| Error::Protocol)?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|n| n > MAX_MANIFEST as u64)
        {
            return Err(Error::Protocol);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Protocol)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_MANIFEST {
                return Err(Error::Protocol);
            }
            bytes.extend_from_slice(&chunk);
        }
        let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
        manifest.admit(method, path)
    }
}
