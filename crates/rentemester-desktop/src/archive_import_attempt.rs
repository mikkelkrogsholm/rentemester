//! One small durable import intent. CSV bytes and the full request are never persisted.
use crate::archive_import_state::{self as state, Review, Scope};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
const FILE: &str = "pending-archive-import.json";
const MAX_RECORD: usize = 256 * 1024;
pub fn directory() -> PathBuf {
    crate::journal_attempt::directory()
}
fn fail() -> String {
    "Det private arkivforsøg kunne ikke verificeres; originalen bevares og intet sendes".into()
}
#[derive(Clone, Debug, PartialEq)]
pub struct Attempt {
    pub scope: Scope,
    pub idempotency_key: String,
    pub plan_hash: String,
    pub source_sha256: String,
    pub body_sha256: String,
    pub summary: Value,
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
impl Attempt {
    pub fn new(review: &Review) -> Result<Self, String> {
        let value = Self {
            scope: review.scope().clone(),
            idempotency_key: review.idempotency_key().into(),
            plan_hash: review.plan()["planHash"].as_str().ok_or_else(fail)?.into(),
            source_sha256: review.package().sha256().into(),
            body_sha256: state::sha(review.body()),
            summary: review.summary(),
        };
        value.record()?;
        Ok(value)
    }
    fn record(&self) -> Result<Value, String> {
        if !state::valid_plan(&self.summary)
            || self.summary["planHash"] != self.plan_hash
            || self.summary["source"]["sha256"] != self.source_sha256
            || !state::hex(&self.body_sha256, 64)
            || self.idempotency_key.trim().is_empty()
            || self.idempotency_key.encode_utf16().count() > 128
            || self.idempotency_key.chars().any(char::is_control)
            || ![
                &self.scope.origin,
                &self.scope.company,
                &self.scope.principal,
            ]
            .iter()
            .all(|s| state::text(s))
            || !state::hex(&self.scope.ledger_uuid, 32)
            || self
                .scope
                .local_identity
                .as_ref()
                .is_some_and(|s| !state::text(s))
        {
            return Err(fail());
        }
        let v = json!({"format":"rentemester-desktop-archive-import/1","origin":self.scope.origin,"company":self.scope.company,"principal":self.scope.principal,"ledgerUuid":self.scope.ledger_uuid,"localIdentity":self.scope.local_identity,"idempotencyKey":self.idempotency_key,"planHash":self.plan_hash,"sourceSha256":self.source_sha256,"bodySha256":self.body_sha256,"summary":self.summary});
        if v.to_string().len() > MAX_RECORD {
            return Err(fail());
        }
        Ok(v)
    }
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let record = self.record()?;
        check_dir(dir, true)?;
        if let Some(current) = Self::load(dir)? {
            if current.equivalent(self) {
                return Ok(());
            }
            return Err("Et tidligere arkivforsøg kræver afklaring; det overskrives ikke".into());
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.join(FILE))
            .map_err(|_| fail())?;
        file.write_all(record.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| fail())?;
        private(&dir.join(FILE), false)?;
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
            .and_then(|f| f.take(MAX_RECORD as u64 + 1).read_to_end(&mut bytes))
            .map_err(|_| fail())?;
        if bytes.len() > MAX_RECORD {
            return Err(fail());
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| fail())?;
        let keys = [
            "format",
            "origin",
            "company",
            "principal",
            "ledgerUuid",
            "localIdentity",
            "idempotencyKey",
            "planHash",
            "sourceSha256",
            "bodySha256",
            "summary",
        ];
        if v["format"] != "rentemester-desktop-archive-import/1"
            || !v.as_object().is_some_and(|o| {
                o.len() == keys.len() && keys.iter().all(|key| o.contains_key(*key))
            })
            || !(v["localIdentity"].is_null() || v["localIdentity"].is_string())
        {
            return Err(fail());
        }
        let text = |key| v[key].as_str().map(str::to_owned).ok_or_else(fail);
        let value = Self {
            scope: Scope {
                origin: text("origin")?,
                company: text("company")?,
                principal: text("principal")?,
                ledger_uuid: text("ledgerUuid")?,
                local_identity: v["localIdentity"].as_str().map(str::to_owned),
                can_apply: false,
            },
            idempotency_key: text("idempotencyKey")?,
            plan_hash: text("planHash")?,
            source_sha256: text("sourceSha256")?,
            body_sha256: text("bodySha256")?,
            summary: v["summary"].clone(),
        };
        value.record()?;
        Ok(Some(value))
    }
    fn equivalent(&self, other: &Self) -> bool {
        self.scope.same_identity(&other.scope)
            && self.idempotency_key == other.idempotency_key
            && self.plan_hash == other.plan_hash
            && self.source_sha256 == other.source_sha256
            && self.body_sha256 == other.body_sha256
            && self.summary == other.summary
    }
    pub fn matches(&self, scope: &Scope) -> bool {
        self.scope.same_identity(scope)
    }
    pub fn can_rebind(&self, scope: &Scope) -> bool {
        self.scope.same_profile(scope) && self.scope.local_identity.is_some()
    }
    /// Call only after separate human review of the old/new endpoints. No remote adoption.
    pub fn rebind(&self, dir: &Path, scope: &Scope) -> Result<Self, String> {
        if !self.can_rebind(scope) || !Self::load(dir)?.is_some_and(|v| v.equivalent(self)) {
            return Err(fail());
        }
        let mut next = self.clone();
        next.scope = scope.clone();
        let record = next.record()?;
        let temporary = dir.join("pending-archive-import-rebind.json");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_| fail())?;
        file.write_all(record.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| fail())?;
        private(&temporary, false)?;
        fs::rename(temporary, dir.join(FILE)).map_err(|_| fail())?;
        sync(dir)?;
        Ok(next)
    }
    /// Only a verified receipt or reviewed definite noncommit may clear a pending attempt.
    pub fn finish(&self, dir: &Path) -> Result<(), String> {
        if !Self::load(dir)?.is_some_and(|v| v.equivalent(self)) {
            return Err(fail());
        }
        fs::remove_file(dir.join(FILE)).map_err(|_| fail())?;
        sync(dir)
    }
}
#[cfg(test)]
mod tests;
