//! One private reviewed opening attempt. Loading never submits or changes its original key/body.
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
const FILE: &str = "pending-opening-balance.json";
const MAX_RECORD: usize = 256 * 1024;
#[derive(Clone, Debug, PartialEq)]
pub struct Attempt {
    pub origin: String,
    pub company: String,
    pub principal: String,
    pub body: Vec<u8>,
    pub ledger_uuid: String,
    pub local_identity: Option<String>,
    pub plan: Value,
}
pub fn directory() -> PathBuf {
    crate::journal_attempt::directory()
}
fn fail() -> String {
    "Det private primobalanceforsøg kunne ikke verificeres; filen bevares og intet sendes".into()
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
    fn record(&self) -> Result<Value, String> {
        crate::opening_balance_state::attempt_body(&self.body, &self.plan)?;
        let valid = |s: &str| !s.is_empty() && s.len() <= 2048 && !s.chars().any(char::is_control);
        if !valid(&self.origin)
            || !valid(&self.company)
            || !valid(&self.principal)
            || self.ledger_uuid.len() != 32
            || !self
                .ledger_uuid
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.local_identity.as_ref().is_some_and(|s| !valid(s))
        {
            return Err(fail());
        }
        let value = json!({"format":"rentemester-desktop-opening-balance/1","origin":self.origin,"company":self.company,"principal":self.principal,"ledgerUuid":self.ledger_uuid,"localIdentity":self.local_identity,"body":String::from_utf8(self.body.clone()).map_err(|_|fail())?,"plan":self.plan});
        if value.to_string().len() > MAX_RECORD {
            return Err(fail());
        }
        Ok(value)
    }
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        let record = self.record()?;
        check_dir(dir, true)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.join(FILE))
            .map_err(|_| "Et primobalanceforsøg kræver afklaring før et nyt forsøg".to_string())?;
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
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| fail())?;
        let Some(object) = value.as_object() else {
            return Err(fail());
        };
        if object.len() != 8
            || ![
                "format",
                "origin",
                "company",
                "principal",
                "ledgerUuid",
                "localIdentity",
                "body",
                "plan",
            ]
            .iter()
            .all(|k| object.contains_key(*k))
            || value["format"] != "rentemester-desktop-opening-balance/1"
            || !(value["localIdentity"].is_null() || value["localIdentity"].is_string())
        {
            return Err(fail());
        }
        let text = |key| value[key].as_str().map(str::to_owned).ok_or_else(fail);
        let attempt = Self {
            origin: text("origin")?,
            company: text("company")?,
            principal: text("principal")?,
            ledger_uuid: text("ledgerUuid")?,
            local_identity: value["localIdentity"].as_str().map(str::to_owned),
            body: text("body")?.into_bytes(),
            plan: value["plan"].clone(),
        };
        attempt.record()?;
        Ok(Some(attempt))
    }
    pub fn matches(
        &self,
        origin: &str,
        company: &str,
        principal: &str,
        ledger: &str,
        local: Option<&str>,
    ) -> bool {
        self.origin == origin
            && self.company == company
            && self.principal == principal
            && self.ledger_uuid == ledger
            && self.local_identity.as_deref() == local
    }
    pub fn can_rebind(
        &self,
        company: &str,
        principal: &str,
        ledger: &str,
        local: Option<&str>,
    ) -> bool {
        self.company == company
            && self.principal == principal
            && self.ledger_uuid == ledger
            && self
                .local_identity
                .as_deref()
                .is_some_and(|old| Some(old) == local)
    }
    /// Explicit UI confirmation only; preserves original plan, request bytes and principal.
    #[allow(clippy::too_many_arguments)]
    pub fn rebind(
        &self,
        dir: &Path,
        origin: &str,
        company: &str,
        principal: &str,
        ledger: &str,
        local: Option<&str>,
    ) -> Result<Self, String> {
        if !self.can_rebind(company, principal, ledger, local)
            || Self::load(dir)?.as_ref() != Some(self)
        {
            return Err(fail());
        }
        let mut next = self.clone();
        next.origin = origin.into();
        let record = next.record()?;
        let temporary = dir.join("pending-opening-balance-rebind.json");
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
        fs::rename(&temporary, dir.join(FILE)).map_err(|_| fail())?;
        sync(dir)?;
        Ok(next)
    }
    /// Caller has a verified canonical receipt or an explicitly reviewed definite refusal.
    pub fn finish(&self, dir: &Path) -> Result<(), String> {
        if Self::load(dir)?.as_ref() != Some(self) {
            return Err(fail());
        }
        fs::remove_file(dir.join(FILE)).map_err(|_| fail())?;
        sync(dir)
    }
}
#[cfg(test)]
mod tests;
