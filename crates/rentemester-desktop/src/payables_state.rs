//! One private unresolved payable attempt; no payload persistence or replay.
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
    let m =
        fs::symlink_metadata(path).map_err(|_| "Privat leverandørgældsstatus kunne ikke åbnes")?;
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
            "Leverandørgældsstatus kræver din egen private mappe0700/fil0600 uden symbolske links."
                .into(),
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
        return Err("Privat leverandørgældsstatus må ikke have udvidede adgangslister.".into());
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
    let path = dir.join("pending-payables.json");
    if !path
        .try_exists()
        .map_err(|_| "Leverandørgældsstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Leverandørgældsstatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Leverandørgældsstatus overskrider grænsen".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Leverandørgældsstatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret leverandørgældsforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    value["format"] == "rentemester-desktop-payables/1"
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
                    "operation",
                    "payableId",
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
        return Err("Ugyldig leverandørgældsstatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-payables.json"))
        .map_err(|_| "Et leverandørgældsforsøg kræver afklaring før ny bogføring")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Leverandørgældsstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Leverandørgældsstatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Leverandørgældsstatus ændrede sig og bevares".into());
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
            .open(dir.join(format!("payables-manually-resolved-{suffix}.json")))
            .map_err(|_| "Gældsafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Gældsafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-payables.json"))
        .map_err(|_| "Leverandørgældsstatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Leverandørgældsstatus kunne ikke synkroniseres".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn private_marker_never_persists_financial_payload_or_supplier_identity() {
        let mut v = json!({"format":"rentemester-desktop-payables/1","origin":"https://synthetic.invalid","company":"synthetic","principal":"owner","ledger":"immutable","localIdentity":null,"operation":"register","documentId":7,"documentHash":"a".repeat(64),"payableId":null,"bankTransactionId":null});
        assert!(valid_record(&v));
        v["amount"] = json!(125);
        assert!(!valid_record(&v));
        v.as_object_mut().unwrap().remove("amount");
        v["supplierName"] = json!("not stored");
        assert!(!valid_record(&v));
    }
    #[test]
    fn uncertainty_persists_refuses_overwrite_and_only_explicitly_archives() {
        let dir = tempfile_dir();
        let v = json!({"format":"rentemester-desktop-payables/1","documentId":7});
        begin_at(&dir, &v).unwrap();
        assert_eq!(load_at(&dir).unwrap(), Some(v.clone()));
        assert!(begin_at(&dir, &v).is_err());
        assert!(finish_at(&dir, &json!({"format":"other"}), false).is_err());
        finish_at(&dir, &v, true).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        assert!(fs::read_dir(&dir).unwrap().any(|p| {
            p.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("payables-manually-resolved-")
        }));
        fs::remove_dir_all(dir).unwrap();
    }
    fn tempfile_dir() -> PathBuf {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "rm-native-payables-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        path
    }
}
