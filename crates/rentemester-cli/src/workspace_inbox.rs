use rentemester_client::{
    ApiClient, MAX_WORKSPACE_INBOX_BODY, WorkspaceInboxMutation, workspace_inbox_body_schema,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "workspace-inbox"
            | "workspace-inbox-source"
            | "workspace-inbox-original"
            | "review-workspace-inbox"
            | "ingest-workspace-inbox"
            | "assign-workspace-inbox"
            | "complete-workspace-inbox"
    )
}
pub fn help() -> Value {
    json!({"usage":"workspace-inbox --company ANCHOR [--cursor N] [--limit 1..100]; workspace-inbox-source --company ANCHOR --source-id ID; workspace-inbox-original additionally --output ABSOLUTE creates one new0600file, verifies SHA header and never overwrites; review-workspace-inbox --source-id ID --confirm < {companySlug,sourceHash,metadata,confirm:true}; ingest-workspace-inbox --company ANCHOR --confirm < native.json; assign-workspace-inbox/complete-workspace-inbox additionally --source-id ID and exact {companySlug,confirm:true} stdin. Explicit server/private MFA cookie required.","policy":"Anchor never implies destination. Native35MiB JSON, decoded25MiB original; caller supplies factual metadata/explicit upload, original supported idempotencyKey preserved. Review source/candidates, explicitly assign then independently confirm completion. Creates target document only, no journal/automatic processing/keys/retries. Original download writes only an explicitly chosen new private output file, no overwrite or local source-path reads. Unknown outcomes require source/assignments and canonical target documents/original reads before any explicit replay.","bodySchemas":{"ingest":workspace_inbox_body_schema(&WorkspaceInboxMutation::Ingest),"review":workspace_inbox_body_schema(&WorkspaceInboxMutation::Review(String::new())),"destination":workspace_inbox_body_schema(&WorkspaceInboxMutation::Assign(String::new()))}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let list = command == "workspace-inbox";
    let original = command == "workspace-inbox-original";
    let read = list || command == "workspace-inbox-source" || original;
    let id = command != "workspace-inbox" && command != "ingest-workspace-inbox";
    if confirm == read
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k)
                || id && *k == "--source-id"
                || list && ["--cursor", "--limit"].contains(k)
                || original && *k == "--output")
        })
    {
        return Err(invalid(
            "inbox selectors/body/confirmation do not match command; see remote help",
        ));
    }
    let id = if id {
        Some(
            *flags
                .get("--source-id")
                .ok_or_else(|| invalid("explicit source-id required"))?,
        )
    } else {
        None
    };
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("explicit company required"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private cookie file required"))?;
    let parse = |key, default| -> Result<usize, Failure> {
        flags.get(key).map_or(Ok(default), |v| {
            if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid("canonical nonnegative cursor/limit required"));
            }
            v.parse().map_err(|_| invalid("cursor/limit out of range"))
        })
    };
    let cursor = parse("--cursor", 0)?;
    let limit = parse("--limit", 25)?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = Vec::new();
    if !read {
        std::io::stdin()
            .take(MAX_WORKSPACE_INBOX_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read inbox body"))?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("HTTP runtime unavailable"))?;
    let key = if command == "ingest-workspace-inbox" && bytes.len() <= MAX_WORKSPACE_INBOX_BODY {
        serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|v| v["idempotencyKey"].as_str().map(str::to_owned))
    } else {
        None
    };
    let outcome = rt.block_on(async {
        if list {
            api.workspace_inbox(company, cursor, limit).await
        } else if original {
            let output=flags.get("--output").ok_or(rentemester_client::Error::InvalidInput("absolute create-new --output required"))?;
            let original=api.workspace_inbox_original(company,id.unwrap()).await?;
            save_original(std::path::Path::new(output),&original.bytes).map_err(|_|rentemester_client::Error::InvalidInput("original output must be absolute, canonical-parent, create-new private file; existing files are never overwritten"))?;
            Ok(json!({"ok":true,"original":{"contentType":original.content_type,"sha256":original.sha256,"size":original.bytes.len(),"path":output}}))
        } else if read {
            api.workspace_inbox_source(company, id.unwrap()).await
        } else {
            let action = match command {
                "ingest-workspace-inbox" => WorkspaceInboxMutation::Ingest,
                "assign-workspace-inbox" => WorkspaceInboxMutation::Assign(id.unwrap().into()),
                "review-workspace-inbox" => WorkspaceInboxMutation::Review(id.unwrap().into()),
                _ => WorkspaceInboxMutation::Complete(id.unwrap().into()),
            };
            api.mutate_workspace_inbox(company, action, bytes, confirm)
                .await
        }
    });
    if outcome == Err(rentemester_client::Error::OutcomeUnknown) {
        return Ok(
            json!({"ok":false,"code":"outcome_unknown","idempotencyKey":key,"errors":["Inbox outcome unknown; inspect source/assignments and target documents/original. Preserve original request/key/current principal; never retry automatically."]}),
        );
    }
    let result = outcome.map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}

fn save_original(path: &std::path::Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    use std::io::Write;
    if !path.is_absolute()
        || path.components().any(|c| {
            !matches!(
                c,
                std::path::Component::RootDir | std::path::Component::Normal(_)
            )
        })
    {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    let parent = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    if parent.canonicalize()? != parent {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    #[cfg(not(unix))]
    {
        let _ = bytes;
        Err(std::io::ErrorKind::Unsupported.into())
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        if file.metadata()?.permissions().mode() & 0o777 != 0o600 {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn original_output_is_private_exclusive_and_rejects_symlink_traversal() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("rm-inbox-output-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("original.eml");
        save_original(&path, b"exact original").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"exact original");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(save_original(&path, b"replacement").is_err());
        let link = dir.join("link.eml");
        symlink(&path, &link).unwrap();
        assert!(save_original(&link, b"replacement").is_err());
        assert!(save_original(&dir.join("../escape.eml"), b"no").is_err());
        assert!(save_original(std::path::Path::new("relative.eml"), b"no").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"exact original");
        std::fs::remove_file(link).unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
