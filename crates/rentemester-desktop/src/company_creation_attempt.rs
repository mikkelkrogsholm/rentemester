//! A durable no-retry creation marker. Unknown outcomes require explicit read/manual reconciliation.
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
const FILE: &str = "pending-company-creation.json";
const LIMIT: usize = 64 * 1024;
#[derive(Clone, Debug, PartialEq)]
pub struct Attempt {
    pub origin: String,
    pub principal: String,
    pub local_identity: Option<String>,
    pub body: Vec<u8>,
    pub receipt: Option<Value>,
}
pub fn directory() -> PathBuf {
    crate::journal_attempt::directory()
}
fn fail() -> String {
    "Virksomhedsoprettelsen er uafklaret. Den private status bevares; ingen automatisk genafsendelse".into()
}
fn private(path: &Path, directory: bool) -> Result<(), String> {
    let m = fs::symlink_metadata(path).map_err(|_| fail())?;
    let uid = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .map_err(|_| fail())?;
    if !uid.status.success()
        || String::from_utf8_lossy(&uid.stdout)
            .trim()
            .parse::<u32>()
            .ok()
            != Some(m.uid())
        || m.file_type().is_symlink()
        || m.is_dir() != directory
        || (!directory && (!m.is_file() || m.nlink() != 1))
        || m.mode() & 0o777 != if directory { 0o700 } else { 0o600 }
    {
        return Err(fail());
    }
    let acl = std::process::Command::new("/bin/ls")
        .arg("-lde")
        .arg(path)
        .output()
        .map_err(|_| fail())?;
    if !acl.status.success()
        || String::from_utf8_lossy(&acl.stdout)
            .lines()
            .skip(1)
            .any(|l| !l.trim().is_empty())
    {
        return Err(fail());
    }
    Ok(())
}
fn check_dir(dir: &Path, create: bool) -> Result<(), String> {
    if !dir.is_absolute() {
        return Err(fail());
    }
    if create && !dir.exists() {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(dir)
            .map_err(|_| fail())?;
    }
    private(dir, true)?;
    if dir.canonicalize().map_err(|_| fail())? != dir {
        return Err(fail());
    }
    Ok(())
}
fn sync(dir: &Path) -> Result<(), String> {
    fs::File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(|_| fail())
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| fail())?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|_| fail())?;
    private(path, false)
}
fn suffix() -> Result<u128, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|n| n.as_nanos())
        .map_err(|_| fail())
}
impl Attempt {
    fn record(&self) -> Result<Value, String> {
        crate::company_creation_state::validate_body(&self.body)?;
        let text = |s: &str| !s.is_empty() && s.len() <= 2048 && !s.chars().any(char::is_control);
        if !text(&self.origin)
            || !text(&self.principal)
            || self.local_identity.as_ref().is_some_and(|s| !text(s))
        {
            return Err(fail());
        }
        if let Some(receipt) = &self.receipt {
            crate::company_creation_state::receipt_slug(&self.body, receipt)?;
        }
        let value = json!({"format":"rentemester-desktop-company-creation/1","origin":self.origin,"principal":self.principal,"localIdentity":self.local_identity,"body":String::from_utf8(self.body.clone()).map_err(|_|fail())?,"receipt":self.receipt});
        if value.to_string().len() > LIMIT {
            return Err(fail());
        }
        Ok(value)
    }
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let record = self.record()?;
        check_dir(dir, true)?;
        write_new(&dir.join(FILE), record.to_string().as_bytes())?;
        sync(dir)
    }
    pub fn load(dir: &Path) -> Result<Option<Self>, String> {
        let path = dir.join(FILE);
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(fail()),
            Ok(_) => {}
        }
        check_dir(dir, false)?;
        private(&path, false)?;
        let mut bytes = Vec::new();
        fs::File::open(&path)
            .and_then(|f| f.take(LIMIT as u64 + 1).read_to_end(&mut bytes))
            .map_err(|_| fail())?;
        if bytes.len() > LIMIT {
            return Err(fail());
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| fail())?;
        let o = value.as_object().ok_or_else(fail)?;
        if o.len() != 6
            || ![
                "format",
                "origin",
                "principal",
                "localIdentity",
                "body",
                "receipt",
            ]
            .iter()
            .all(|k| o.contains_key(*k))
            || value["format"] != "rentemester-desktop-company-creation/1"
            || !(value["localIdentity"].is_null() || value["localIdentity"].is_string())
        {
            return Err(fail());
        }
        let text = |k| value[k].as_str().map(str::to_owned).ok_or_else(fail);
        let attempt = Self {
            origin: text("origin")?,
            principal: text("principal")?,
            local_identity: value["localIdentity"].as_str().map(str::to_owned),
            body: text("body")?.into_bytes(),
            receipt: (!value["receipt"].is_null()).then(|| value["receipt"].clone()),
        };
        attempt.record()?;
        Ok(Some(attempt))
    }
    pub fn matches(&self, origin: &str, principal: &str, local: Option<&str>) -> bool {
        self.origin == origin
            && self.principal == principal
            && self.local_identity.as_deref() == local
    }
    /// Caller explicitly reviews the endpoint change. This only permits reads/acknowledgement, never POST replay.
    pub fn can_resolve_at(&self, origin: &str, principal: &str, local: Option<&str>) -> bool {
        self.matches(origin, principal, local)
            || (self.principal == principal
                && self
                    .local_identity
                    .as_deref()
                    .is_some_and(|old| Some(old) == local))
    }
    pub fn record_receipt(&self, dir: &Path, receipt: &Value) -> Result<Self, String> {
        if Self::load(dir)?.as_ref() != Some(self) {
            return Err(fail());
        }
        crate::company_creation_state::receipt_slug(&self.body, receipt)?;
        if let Some(existing) = &self.receipt {
            return if existing == receipt {
                Ok(self.clone())
            } else {
                Err(fail())
            };
        }
        let mut next = self.clone();
        next.receipt = Some(receipt.clone());
        let bytes = next.record()?.to_string();
        let tmp = dir.join("pending-company-creation-receipt.json");
        write_new(&tmp, bytes.as_bytes())?;
        fs::rename(tmp, dir.join(FILE)).map_err(|_| fail())?;
        sync(dir)?;
        Ok(next)
    }
    fn archive_and_clear(&self, dir: &Path, value: &Value, label: &str) -> Result<(), String> {
        if Self::load(dir)?.as_ref() != Some(self) {
            return Err(fail());
        }
        write_new(
            &dir.join(format!("company-creation-{label}-{}.json", suffix()?)),
            value.to_string().as_bytes(),
        )?;
        sync(dir)?;
        fs::remove_file(dir.join(FILE)).map_err(|_| fail())?;
        sync(dir)
    }
    /// Only after a known receipt plus canonical catalog/profile readback. No receipt is inferred here.
    pub fn finish(&self, dir: &Path) -> Result<(), String> {
        if self.receipt.is_none() {
            return Err(fail());
        }
        self.archive_and_clear(dir,&json!({"format":"rentemester-desktop-company-creation-completed/1","attempt":self.record()?}),"completed")
    }
    /// Only the direct response to the original POST, never a later read failure.
    pub fn definite_refusal(&self, dir: &Path, status: u16) -> Result<(), String> {
        if self.receipt.is_some() || !matches!(status, 400 | 401 | 403 | 404 | 409 | 422) {
            return Err(fail());
        }
        self.archive_and_clear(dir, &json!({"format":"rentemester-desktop-company-creation-refused/1","attempt":self.record()?,"httpStatus":status,"knownNoAcceptance":true}), "refused")
    }
    pub fn acknowledge(
        &self,
        dir: &Path,
        reason: &str,
        current: &crate::company_creation_state::Snapshot,
        origin: &str,
        local: Option<&str>,
    ) -> Result<(), String> {
        let reason = reason.trim();
        if !self.can_resolve_at(origin, &current.principal, local)
            || reason.chars().count() < 10
            || reason.len() > 2000
            || reason.chars().any(char::is_control)
        {
            return Err(fail());
        }
        self.archive_and_clear(dir,&json!({"format":"rentemester-desktop-company-creation-manual/1","attempt":self.record()?,"observedOrigin":origin,"observedPrincipal":current.principal,"observedLocalIdentity":local,"catalog":current.catalog,"reason":reason,"createdCompanyConfirmed":false}),"manual")
    }
}
#[cfg(test)]
mod tests;
