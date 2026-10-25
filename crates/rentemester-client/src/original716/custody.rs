//! One-shot source custody transport. Local output paths are NEVER sent to the server.
use crate::{ApiClient, Error, Method, company_path};
use serde_json::{Value, json};
impl ApiClient {
    pub async fn original716_custody(&self, command: &str, args: &Value) -> Result<Value, Error> {
        let mut body = args.clone();
        let object = body
            .as_object_mut()
            .ok_or(Error::InvalidInput("custody arguments must be an object"))?;
        let company = object
            .remove("company")
            .and_then(|v| v.as_str().map(str::to_owned));
        let out = object
            .remove("out")
            .and_then(|v| v.as_str().map(str::to_owned));
        let endpoint = match command {
            "backup" if object.remove("signWithEd25519") == Some(json!(true)) => {
                "original-system-backup-signed"
            }
            "export-public-key" => "original-system-export-public-key",
            "rotate-backup-keypair" => "original-system-rotate-backup-keypair",
            "verify-backup-signature" => "original-system-verify-backup-signature",
            _ => return Err(Error::InvalidInput("unknown source custody command")),
        };
        let path = if command == "verify-backup-signature" {
            format!("/api/{endpoint}")
        } else {
            company_path(
                company
                    .as_deref()
                    .ok_or(Error::InvalidInput("company required"))?,
                endpoint,
            )?
        };
        let value = self
            .request(
                Method::POST,
                &path,
                &[],
                Some(serde_json::to_vec(&body).map_err(|_| Error::Protocol)?),
            )
            .await?;
        if value["ok"] != true || !value["result"]["ok"].is_boolean() {
            return Err(Error::Protocol);
        }
        let mut result = value["result"].clone();
        if command == "export-public-key" && result["ok"] == true {
            let out = out.ok_or(Error::InvalidInput("Missing required --out <file>"))?;
            let pem = result["publicKeyPem"]
                .as_str()
                .filter(|v| v.len() <= 1024 && v.starts_with("-----BEGIN PUBLIC KEY-----\n"))
                .ok_or(Error::Protocol)?;
            write_public_new(std::path::Path::new(&out), pem.as_bytes())?;
            result
                .as_object_mut()
                .ok_or(Error::Protocol)?
                .remove("publicKeyPem");
            result["outPath"] = json!(out);
        }
        Ok(result)
    }
}
fn write_public_new(path: &std::path::Path, bytes: &[u8]) -> Result<(), Error> {
    use std::{fs, io::Write};
    let unavailable =
        || Error::InvalidInput("local public-key export failed; no automatic retry or overwrite");
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    // Walk parents without following symlinks. No arbitrary server-side output path exists.
    let absolute = if parent.is_absolute() {
        parent.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|_| unavailable())?
            .join(parent)
    };
    let mut checked = std::path::PathBuf::new();
    for part in absolute.components() {
        if matches!(part, std::path::Component::ParentDir) {
            return Err(unavailable());
        }
        checked.push(part);
        match fs::symlink_metadata(&checked) {
            Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut builder = fs::DirBuilder::new();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                builder.create(&checked).map_err(|_| unavailable())?;
            }
            _ => return Err(unavailable()),
        }
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| unavailable())?;
    file.write_all(bytes).map_err(|_| unavailable())?;
    file.sync_all().map_err(|_| unavailable())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original716_public_export_is_new_public_only_and_refuses_symlink_or_overwrite() {
        let root =
            std::env::temp_dir().join(format!("original716-public-export-{}", std::process::id()));
        // Fresh synthetic fixture; never overwrite an existing directory.
        std::fs::create_dir(&root).unwrap();
        let target = root.join("auditor/company.pub");
        let bytes =
            b"-----BEGIN PUBLIC KEY-----\nsynthetic-public-fixture\n-----END PUBLIC KEY-----\n";
        write_public_new(&target, bytes).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), bytes);
        assert!(write_public_new(&target, bytes).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("auditor"), root.join("alias")).unwrap();
            assert!(write_public_new(&root.join("alias/another.pub"), bytes).is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
