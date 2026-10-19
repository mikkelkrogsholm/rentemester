//! One private bank import/account attempt; manual reconciliation only after uncertainty.
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
fn directory() -> PathBuf {
    crate::journal_attempt::directory()
}
fn private(path: &Path, directory: bool) -> Result<(), String> {
    let m = fs::symlink_metadata(path).map_err(|_| "Privat bankstatus kunne ikke åbnes")?;
    let uid = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .map_err(|_| "Brugeridentitet kunne ikke læses")?;
    if m.file_type().is_symlink()
        || m.is_dir() != directory
        || (!directory && !m.is_file())
        || m.mode() & 0o077 != 0
        || String::from_utf8_lossy(&uid.stdout)
            .trim()
            .parse::<u32>()
            .ok()
            != Some(m.uid())
    {
        return Err(
            "Bankstatus kræver din egen private mappe0700/fil0600 uden symbolske links.".into(),
        );
    }
    let acl = std::process::Command::new("/bin/ls")
        .arg("-lde")
        .arg(path)
        .output()
        .map_err(|_| "Privat adgangskontrol kunne ikke undersøges")?;
    if !acl.status.success()
        || String::from_utf8_lossy(&acl.stdout)
            .lines()
            .skip(1)
            .any(|line| !line.trim().is_empty())
    {
        return Err("Privat bankstatus må ikke have udvidede adgangslister.".into());
    }
    Ok(())
}
fn ensure() -> Result<PathBuf, String> {
    let dir = directory();
    if !dir.is_absolute() {
        return Err("Vælg en absolut privat statusmappe".into());
    }
    if !dir.exists() {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .map_err(|_| "Privat statusmappe kunne ikke oprettes")?;
    }
    private(&dir, true)?;
    if dir
        .canonicalize()
        .map_err(|_| "Privat statusmappe kunne ikke åbnes")?
        != dir
    {
        return Err("Statusmappen skal være en kanonisk sti".into());
    }
    Ok(dir)
}
pub fn load() -> Result<Option<Value>, String> {
    load_at(&directory())
}
fn load_at(dir: &Path) -> Result<Option<Value>, String> {
    let path = dir.join("pending-bank.json");
    if !path
        .try_exists()
        .map_err(|_| "Bankstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(524289).read_to_end(&mut bytes))
        .map_err(|_| "Bankstatus kunne ikke læses")?;
    if bytes.len() > 524288 {
        return Err("Bankstatus overskrider grænsen".into());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Bankstatus er ugyldig")?;
    if value["format"] != "rentemester-desktop-bank/1"
        || !matches!(value["operation"].as_str(), Some("import" | "create"))
        || value["body"].as_str().is_none()
    {
        return Err("Ukendt uafklaret bankforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if value.to_string().len() > 524288 {
        return Err("Det private bankforsøg overskrider grænsen; intet er sendt.".into());
    }
    let mut f=OpenOptions::new().write(true).create_new(true).mode(0o600).open(dir.join("pending-bank.json")).map_err(|_|"Et bankforsøg er allerede uafklaret. Afklar det før en ny import eller kontooprettelse.")?;
    f.write_all(value.to_string().as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(|_| "Bankforsøget kunne ikke gemmes; intet er sendt")?;
    fs::File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(|_| "Bankstatus kunne ikke synkroniseres")?;
    Ok(())
}
pub fn finish(expected: &Value) -> Result<(), String> {
    finish_at(&directory(), expected)
}
fn finish_at(dir: &Path, expected: &Value) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Bankstatus har ændret sig og bevares.".into());
    }
    fs::remove_file(dir.join("pending-bank.json"))
        .map_err(|_| "Bankstatus kunne ikke afsluttes".into())
}

/// Manual resolution preserves the original request in an owned private archive.
pub fn resolve(expected: &Value) -> Result<(), String> {
    let dir = ensure()?;
    if load_at(&dir)?.as_ref() != Some(expected) {
        return Err("Bankforsøget har ændret sig; det bevares.".into());
    }
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "Systemtid kunne ikke læses")?
        .as_nanos();
    let archive = dir.join(format!("bank-manually-reconciled-{suffix}.json"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(archive)
        .map_err(|_| "Bankafklaring kunne ikke gemmes")?;
    let mut record = expected.clone();
    record["resolution"] =
        serde_json::json!("manually reconciled by current user; not an import receipt");
    file.write_all(record.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Bankafklaring kunne ikke synkroniseres")?;
    finish_at(&dir, expected)?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Bankafklaring kunne ikke afsluttes".into())
}
pub fn same_scope(
    record: &Value,
    origin: &str,
    company: &str,
    user: &str,
    ledger: &str,
    local: Option<&str>,
) -> bool {
    record["company"] == company
        && record["principal"] == user
        && record["ledger"] == ledger
        && (record["origin"] == origin
            || local.is_some_and(|identity| record["localIdentity"].as_str() == Some(identity)))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn scope_requires_user_company_ledger_and_remote_origin_or_owned_profile() {
        let v = json!({"origin":"https://old.example","company":"synthetic","principal":"owner","ledger":"immutable","localIdentity":"owned-profile"});
        assert!(same_scope(
            &v,
            "https://old.example",
            "synthetic",
            "owner",
            "immutable",
            None
        ));
        assert!(!same_scope(
            &v,
            "https://new.example",
            "synthetic",
            "owner",
            "immutable",
            None
        ));
        assert!(same_scope(
            &v,
            "http://127.0.0.1:1234",
            "synthetic",
            "owner",
            "immutable",
            Some("owned-profile")
        ));
        assert!(!same_scope(
            &v,
            "http://127.0.0.1:1234",
            "synthetic",
            "owner",
            "different",
            Some("owned-profile")
        ));
        assert!(!same_scope(
            &v,
            "https://old.example",
            "other",
            "owner",
            "immutable",
            None
        ));
        assert!(!same_scope(
            &v,
            "https://old.example",
            "synthetic",
            "other",
            "immutable",
            None
        ));
    }
    #[test]
    fn pending_exact_body_survives_reload_and_cannot_be_replaced() {
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "rm-bank-state-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let record = json!({"format":"rentemester-desktop-bank/1","operation":"import","body":"{ \"csvContent\":\"date;text;amount\\n2026-10-19;Synthetic;12.34\\n\", \"confirm\":true }"});
        begin_at(&dir, &record).unwrap();
        assert_eq!(load_at(&dir).unwrap(), Some(record.clone()));
        assert!(begin_at(&dir, &record).is_err());
        assert!(finish_at(&dir, &json!({})).is_err());
        assert_eq!(load_at(&dir).unwrap(), Some(record.clone()));
        finish_at(&dir, &record).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        fs::remove_dir(&dir).unwrap();
    }
}
