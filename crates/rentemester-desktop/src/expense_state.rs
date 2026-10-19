//! One private unresolved paid-receipt attempt; no payload persistence or replay.
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
    let m = fs::symlink_metadata(path).map_err(|_| "Privat udgiftsstatus kunne ikke åbnes")?;
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
            "Udgiftsstatus kræver din egen private mappe0700/fil0600 uden symbolske links.".into(),
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
        return Err("Privat udgiftsstatus må ikke have udvidede adgangslister.".into());
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
    let path = dir.join("pending-expense.json");
    if !path
        .try_exists()
        .map_err(|_| "Udgiftsstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Udgiftsstatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Udgiftsstatus overskrider grænsen".into());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Udgiftsstatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret udgiftsforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    value["format"] == "rentemester-desktop-expense/1"
        && value.as_object().is_some_and(|object| {
            object.keys().all(|key| {
                [
                    "format",
                    "origin",
                    "company",
                    "principal",
                    "ledger",
                    "localIdentity",
                    "documentId",
                    "documentHash",
                    "bankTransactionId",
                    "expenseAccountNo",
                    "vatTreatment",
                ]
                .contains(&key.as_str())
            })
        })
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) || value.to_string().len() > 65536 {
        return Err("Ugyldig udgiftsstatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-expense.json"))
        .map_err(|_| "Et udgiftsforsøg kræver afklaring før ny bogføring")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Udgiftsstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Udgiftsstatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Udgiftsstatus ændrede sig og bevares".into());
    }
    if manual {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "Systemtid kunne ikke læses")?
            .as_nanos();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.join(format!("expense-manually-resolved-{suffix}.json")))
            .map_err(|_| "Bilagsafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Bilagsafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-expense.json"))
        .map_err(|_| "Udgiftsstatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Udgiftsstatus kunne ikke synkroniseres".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn uncertainty_marker_excludes_original_names_and_replay_body() {
        let mut value = json!({"format":"rentemester-desktop-expense/1","documentId":7,"documentHash":"a".repeat(64),"bankTransactionId":9,"expenseAccountNo":"X","vatTreatment":"standard","origin":"https://synthetic.example","company":"synthetic","principal":"owner","ledger":"immutable","localIdentity":null});
        assert!(valid_record(&value));
        value["body"] = json!("never persist");
        assert!(!valid_record(&value));
        value.as_object_mut().unwrap().remove("body");
        value["supplierName"] = json!("never persist");
        assert!(!valid_record(&value));
    }
    #[test]
    fn pending_attempt_survives_restart_and_refuses_replacement_or_wrong_finish() {
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "rm-expense-state-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let record = json!({"format":"rentemester-desktop-expense/1","documentId":7,"bankTransactionId":9,"documentHash":"a".repeat(64)});
        begin_at(&dir, &record).unwrap();
        assert_eq!(load_at(&dir).unwrap(), Some(record.clone()));
        assert!(begin_at(&dir, &record).is_err());
        assert!(finish_at(&dir, &json!({}), false).is_err());
        assert_eq!(load_at(&dir).unwrap(), Some(record.clone()));
        finish_at(&dir, &record, true).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        for file in fs::read_dir(&dir).unwrap() {
            fs::remove_file(file.unwrap().path()).unwrap();
        }
        fs::remove_dir(dir).unwrap();
    }
}
