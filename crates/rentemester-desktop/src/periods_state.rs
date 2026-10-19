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
    let m = fs::symlink_metadata(path).map_err(|_| "Privat periodensstatus kunne ikke åbnes")?;
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
            "Periodensstatus kræver din egen private mappe0700/fil0600 uden symbolske links."
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
        return Err("Privat periodensstatus må ikke have udvidede adgangslister.".into());
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
    let path = dir.join("pending-periods.json");
    if !path
        .try_exists()
        .map_err(|_| "Periodensstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Periodensstatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Periodensstatus overskrider grænsen".into());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Periodensstatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret periodensforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    value["format"] == "rentemester-desktop-periods/1"
        && value.as_object().is_some_and(|object| {
            object.len() == 12
                && object.keys().all(|key| {
                    [
                        "format",
                        "origin",
                        "company",
                        "principal",
                        "ledger",
                        "localIdentity",
                        "operation",
                        "periodStart",
                        "periodEnd",
                        "periodId",
                        "packetHash",
                        "reviewId",
                    ]
                    .contains(&key.as_str())
                })
        })
        && [
            "origin",
            "company",
            "principal",
            "ledger",
            "periodStart",
            "periodEnd",
        ]
        .iter()
        .all(|key| {
            value[*key].as_str().is_some_and(|v| {
                !v.trim().is_empty() && v.len() <= 2048 && !v.chars().any(char::is_control)
            })
        })
        && matches!(
            value["operation"].as_str(),
            Some("review" | "close" | "reopen" | "report")
        )
        && (value["localIdentity"].is_null()
            || value["localIdentity"]
                .as_str()
                .is_some_and(|v| !v.is_empty()))
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) || value.to_string().len() > 65536 {
        return Err("Ugyldig periodensstatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-periods.json"))
        .map_err(|_| "Et periodensforsøg kræver afklaring før ny bogføring")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Periodensstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Periodensstatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Periodensstatus ændrede sig og bevares".into());
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
            .open(dir.join(format!("periods-manually-resolved-{suffix}.json")))
            .map_err(|_| "Periodeafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Periodeafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-periods.json"))
        .map_err(|_| "Periodensstatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Periodensstatus kunne ikke synkroniseres".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn pending_period_marker_excludes_receipt_reason_financial_values_and_replay() {
        let mut value = json!({"format":"rentemester-desktop-periods/1","origin":"https://synthetic.invalid","company":"synthetic","principal":"owner","ledger":"immutable","localIdentity":null,"operation":"close","periodStart":"2026-07-01","periodEnd":"2026-09-30","periodId":2,"packetHash":"a".repeat(64),"reviewId":3});
        assert!(valid_record(&value));
        assert!(!valid_record(
            &json!({"format":"rentemester-desktop-periods/1"})
        ));
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove("principal");
        assert!(!valid_record(&missing));
        for key in ["reference", "reason", "body", "vat", "evidence"] {
            value[key] = json!("not persisted");
            assert!(!valid_record(&value));
            value.as_object_mut().unwrap().remove(key);
        }
    }
    #[test]
    fn marker_survives_restart_and_explicit_manual_resolution_only() {
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "rm-native-periods-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let value = json!({"format":"rentemester-desktop-periods/1","origin":"https://synthetic.invalid","company":"synthetic","principal":"owner","ledger":"immutable","localIdentity":null,"operation":"review","periodStart":"2026-07-01","periodEnd":"2026-09-30","periodId":null,"packetHash":"a".repeat(64),"reviewId":null});
        begin_at(&dir, &value).unwrap();
        assert_eq!(load_at(&dir).unwrap(), Some(value.clone()));
        assert!(begin_at(&dir, &value).is_err());
        assert!(finish_at(&dir, &json!({"format":"other"}), false).is_err());
        finish_at(&dir, &value, true).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        assert!(fs::read_dir(&dir).unwrap().any(|p| {
            p.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("periods-manually-resolved-")
        }));
        fs::remove_dir_all(dir).unwrap();
    }
}
