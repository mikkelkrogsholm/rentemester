//! Audited handoff download and bounded regular-file USTAR materialization, never a general extractor.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
pub const MAX_HANDOFF_BODY: usize = 4096;
pub const MAX_HANDOFF_PACKAGE: usize = 256 * 1024 * 1024;
pub fn handoff_body_schema() -> Value {
    let date = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    object(
        json!({"periodStart":date,"periodEnd":date,"confirm":{"const":true},"requestedAt":{"type":"string"},"requester":{"type":"string"}}),
        &["periodStart", "periodEnd", "confirm"],
    )
}
impl ApiClient {
    /// Exactly one confirmed POST; lost/bad responses can follow an export audit commit.
    pub async fn handoff_export(
        &self,
        company: &str,
        profile: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit handoff export confirmation required",
            ));
        }
        if bytes.len() > MAX_HANDOFF_BODY {
            return Err(Error::InvalidInput("handoff body exceeds4096 bytes"));
        }
        body(&bytes, &handoff_body_schema())?;
        let route = match profile {
            "authority" => "authority-export",
            "accountant_handoff" => "accountant-export",
            _ => {
                return Err(Error::InvalidInput(
                    "handoff profile must be authority|accountant_handoff",
                ));
            }
        };
        let mut response = self
            .send(
                Method::POST,
                &company_path(company, route)?,
                &[],
                Some(bytes),
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        if response.status().is_server_error() {
            return Err(Error::OutcomeUnknown);
        }
        if !response.status().is_success() {
            return Err(Error::Refused {
                status: response.status().as_u16(),
            });
        }
        if response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            != Some("application/x-tar")
            || response
                .content_length()
                .is_some_and(|n| n > MAX_HANDOFF_PACKAGE as u64)
        {
            return Err(Error::OutcomeUnknown);
        }
        let mut data = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::OutcomeUnknown)? {
            if data.len().saturating_add(chunk.len()) > MAX_HANDOFF_PACKAGE {
                return Err(Error::OutcomeUnknown);
            }
            data.extend_from_slice(&chunk);
        }
        if data.len() < 1024 || !data.len().is_multiple_of(512) {
            return Err(Error::OutcomeUnknown);
        }
        Ok(json!({"ok":true,"export":{"contentType":"application/x-tar","bytes":data}}))
    }
}
fn protocol<T>() -> Result<T, Error> {
    Err(Error::Protocol)
}
fn path(value: &str) -> bool {
    !value.is_empty()
        && !value.contains(['\\', ':'])
        && !value.chars().any(char::is_control)
        && !value
            .split('/')
            .any(|s| s.is_empty() || matches!(s, "." | ".."))
}
fn text(field: &[u8]) -> Result<&str, Error> {
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    if field[end..].iter().any(|b| *b != 0) {
        return protocol();
    }
    std::str::from_utf8(&field[..end]).map_err(|_| Error::Protocol)
}
fn octal(field: &[u8]) -> Result<usize, Error> {
    let s = std::str::from_utf8(field)
        .map_err(|_| Error::Protocol)?
        .trim_matches(|c| c == '\0' || c == ' ');
    if s.is_empty() || !s.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return protocol();
    }
    usize::from_str_radix(s, 8).map_err(|_| Error::Protocol)
}
fn archive(bytes: &[u8]) -> Result<BTreeMap<String, &[u8]>, Error> {
    if bytes.len() < 1024 || bytes.len() > MAX_HANDOFF_PACKAGE || !bytes.len().is_multiple_of(512) {
        return protocol();
    }
    let mut files = BTreeMap::new();
    let mut cursor = 0;
    loop {
        let header = bytes.get(cursor..cursor + 512).ok_or(Error::Protocol)?;
        if header.iter().all(|b| *b == 0) {
            if bytes.len() - cursor < 1024 || bytes[cursor..].iter().any(|b| *b != 0) {
                return protocol();
            }
            break;
        }
        if &header[257..263] != b"ustar\0"
            || &header[263..265] != b"00"
            || !matches!(header[156], 0 | b'0')
            || header[157..257].iter().any(|b| *b != 0)
        {
            return protocol();
        }
        let checksum: usize = header
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if (148..156).contains(&i) {
                    b' ' as usize
                } else {
                    *b as usize
                }
            })
            .sum();
        if octal(&header[148..156])? != checksum {
            return protocol();
        }
        let name = text(&header[..100])?;
        let prefix = text(&header[345..500])?;
        let name = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}/{name}")
        };
        if !path(&name) || !name.contains('/') {
            return protocol();
        }
        let size = octal(&header[124..136])?;
        let begin = cursor.checked_add(512).ok_or(Error::Protocol)?;
        let end = begin.checked_add(size).ok_or(Error::Protocol)?;
        let next = end
            .checked_add((512 - size % 512) % 512)
            .ok_or(Error::Protocol)?;
        let data = bytes.get(begin..end).ok_or(Error::Protocol)?;
        if bytes
            .get(end..next)
            .ok_or(Error::Protocol)?
            .iter()
            .any(|b| *b != 0)
            || files.insert(name, data).is_some()
        {
            return protocol();
        }
        cursor = next;
    }
    for name in files.keys() {
        let mut parent = Path::new(name).parent();
        while let Some(p) = parent {
            if files.contains_key(p.to_str().ok_or(Error::Protocol)?) {
                return protocol();
            }
            parent = p.parent();
        }
    }
    Ok(files)
}
pub(crate) fn matches_output(value: &Value, bytes: &[u8]) -> bool {
    value["sizeBytes"].as_u64() == Some(bytes.len() as u64)
        && value["sha256"].as_str() == Some(format!("{:x}", Sha256::digest(bytes)).as_str())
}
fn validate_manifest(files: &BTreeMap<String, &[u8]>) -> Result<(String, Value), Error> {
    let mut roots = BTreeSet::new();
    for name in files.keys() {
        roots.insert(name.split('/').next().ok_or(Error::Protocol)?);
    }
    if roots.len() != 1 {
        return protocol();
    }
    let root = roots.first().ok_or(Error::Protocol)?.to_string();
    let manifest: Value = serde_json::from_slice(
        files
            .get(&format!("{root}/manifest.json"))
            .ok_or(Error::Protocol)?,
    )
    .map_err(|_| Error::Protocol)?;
    let valid_profile = match manifest["packageType"].as_str() {
        Some("authority_export") => root.starts_with("myndigheds-eksport-"),
        Some("accountant_handoff_export") => root.starts_with("revisor-eksport-"),
        _ => false,
    };
    if !valid_profile
        || manifest["manifestVersion"] != 2
        || !manifest["periodStart"].is_string()
        || !manifest["periodEnd"].is_string()
        || !manifest["generatedAt"].is_string()
        || !(manifest["deadlineAt"].is_null() || manifest["deadlineAt"].is_string())
        || !manifest["appliedRules"]
            .as_array()
            .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
    {
        return protocol();
    }
    for key in ["journalEntries", "bankTransactions", "documents"] {
        if !manifest["counts"][key].is_u64() {
            return protocol();
        }
    }
    let outputs = manifest["outputs"].as_array().ok_or(Error::Protocol)?;
    let mut declared = BTreeSet::new();
    for output in outputs {
        let relative = output["path"].as_str().ok_or(Error::Protocol)?;
        if !path(relative) || relative == "manifest.json" || !declared.insert(relative.to_owned()) {
            return protocol();
        }
        let bytes = files
            .get(&format!("{root}/{relative}"))
            .ok_or(Error::Protocol)?;
        if !matches_output(output, bytes) {
            return protocol();
        }
    }
    if outputs.len() + 1 != files.len() {
        return protocol();
    }
    for (key, value) in manifest["files"].as_object().ok_or(Error::Protocol)? {
        let relative = value.as_str().ok_or(Error::Protocol)?;
        if !path(relative)
            || if key == "readableDocumentsDir" {
                relative != "documents-readable"
            } else {
                !declared.contains(relative)
            }
        {
            return protocol();
        }
    }
    let documents_path = manifest["files"]["documents"]
        .as_str()
        .ok_or(Error::Protocol)?;
    let documents: Value = serde_json::from_slice(
        files
            .get(&format!("{root}/{documents_path}"))
            .ok_or(Error::Protocol)?,
    )
    .map_err(|_| Error::Protocol)?;
    let documents = documents.as_array().ok_or(Error::Protocol)?;
    let copies = manifest["copiedDocuments"]
        .as_array()
        .ok_or(Error::Protocol)?;
    let mut copied = BTreeSet::new();
    for copy in copies {
        let relative = copy["exportedPath"].as_str().ok_or(Error::Protocol)?;
        let source = copy["sourcePathRelativeToCompany"]
            .as_str()
            .ok_or(Error::Protocol)?;
        if !path(relative)
            || !relative.starts_with("documents-readable/")
            || !path(source)
            || !copied.insert(relative.to_owned())
        {
            return protocol();
        }
        let bytes = files
            .get(&format!("{root}/{relative}"))
            .ok_or(Error::Protocol)?;
        if !matches_output(copy, bytes) {
            return protocol();
        }
        let document = documents
            .iter()
            .find(|d| d["id"] == copy["documentId"])
            .ok_or(Error::Protocol)?;
        if document["sha256"] != copy["sha256"]
            || document["storedPathRelativeToCompany"] != source
            || document["exportedReadablePath"] != relative
        {
            return protocol();
        }
    }
    for document in documents {
        if let Some(relative) = document["exportedReadablePath"].as_str()
            && !copied.contains(relative)
        {
            return protocol();
        }
    }
    for relative in &declared {
        if relative.starts_with("documents-readable/") && !copied.contains(relative) {
            return protocol();
        }
    }
    if manifest["counts"]["copiedReadableDocuments"].as_u64() != Some(copies.len() as u64) {
        return protocol();
    }
    Ok((root, manifest))
}
fn mkdir(path: &Path, recursive: bool) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(recursive);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}
/// Validate every byte/path/hash before filesystem effects; publish only a fresh package root.
/// The confirmed server export audit may already exist on local failure. Never auto-export again.
pub(crate) fn write_package(
    value: &Value,
    out: &Path,
    validate: impl FnOnce(&BTreeMap<String, &[u8]>) -> Result<(String, Value), Error>,
) -> Result<(PathBuf, Value), Error> {
    if value["ok"] != true || value["export"]["contentType"] != "application/x-tar" {
        return protocol();
    }
    let array = value["export"]["bytes"]
        .as_array()
        .filter(|a| a.len() <= MAX_HANDOFF_PACKAGE)
        .ok_or(Error::Protocol)?;
    let bytes = array
        .iter()
        .map(|v| {
            v.as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .ok_or(Error::Protocol)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let files = archive(&bytes)?;
    let (root, manifest) = validate(&files)?;
    mkdir(out, true).map_err(|_| Error::InvalidInput("local export parent unavailable"))?;
    let parent = fs::canonicalize(out)
        .map_err(|_| Error::InvalidInput("local export parent unavailable"))?;
    let target = parent.join(&root);
    mkdir(&target,false).map_err(|_|Error::InvalidInput("local export package already exists or cannot be created; never overwrite/re-export automatically"))?;
    let result = (|| {
        for (name, bytes) in &files {
            let relative = name
                .strip_prefix(&format!("{root}/"))
                .ok_or(Error::Protocol)?;
            let file = target.join(relative);
            mkdir(file.parent().ok_or(Error::Protocol)?, true)
                .map_err(|_| Error::InvalidInput("local package directory write failed"))?;
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut output = options
                .open(&file)
                .map_err(|_| Error::InvalidInput("local package file write failed"))?;
            output
                .write_all(bytes)
                .and_then(|_| output.sync_all())
                .map_err(|_| Error::InvalidInput("local package file write failed"))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&target);
    }
    result?;
    Ok((target, manifest))
}
/// Verified authority/accountant package; preserves the original flat result.
pub fn write_handoff_package(value: &Value, out: &Path) -> Result<Value, Error> {
    let (target, manifest) = write_package(value, out, validate_manifest)?;
    let manifest_path: PathBuf = target.join("manifest.json");
    Ok(
        json!({"ok":true,"exportDir":target,"manifestPath":manifest_path,"periodStart":manifest["periodStart"],"periodEnd":manifest["periodEnd"],"generatedAt":manifest["generatedAt"],"deadlineAt":manifest["deadlineAt"],"journalEntryCount":manifest["counts"]["journalEntries"],"bankTransactionCount":manifest["counts"]["bankTransactions"],"documentCount":manifest["counts"]["documents"],"appliedRules":manifest["appliedRules"],"errors":[]}),
    )
}
