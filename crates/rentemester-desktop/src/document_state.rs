//! Private unresolved upload marker and safe exact-original downloads; never a replay queue.
use serde_json::Value;
use sha2::{Digest, Sha256};
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
    let m = fs::symlink_metadata(path).map_err(|_| "Privat bilagsstatus kunne ikke åbnes")?;
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
            "Bilagsstatus kræver din egen private mappe0700/fil0600 uden symbolske links.".into(),
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
        return Err("Privat bilagsstatus må ikke have udvidede adgangslister.".into());
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
    let path = dir.join("pending-document.json");
    if !path
        .try_exists()
        .map_err(|_| "Bilagsstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Bilagsstatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Bilagsstatus overskrider grænsen".into());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Bilagsstatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret bilagsforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    value["format"] == "rentemester-desktop-document/1"
        && value.as_object().is_some_and(|object| {
            object.keys().all(|key| {
                [
                    "format",
                    "origin",
                    "company",
                    "principal",
                    "ledger",
                    "localIdentity",
                    "byteCount",
                    "mime",
                    "sha256Hash",
                    "beforeIds",
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
        return Err("Ugyldig bilagsstatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-document.json"))
        .map_err(|_| "Et bilagsforsøg kræver afklaring før ny upload")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Bilagsstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Bilagsstatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Bilagsstatus ændrede sig og bevares".into());
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
            .open(dir.join(format!("document-manually-resolved-{suffix}.json")))
            .map_err(|_| "Bilagsafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Bilagsafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-document.json"))
        .map_err(|_| "Bilagsstatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Bilagsstatus kunne ikke synkroniseres".into())
}
pub fn extension(mime: &str) -> Option<&'static str> {
    Some(match mime {
        "application/pdf" => "pdf",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "text/plain" => "txt",
        "application/json" => "json",
        "application/xml" => "xml",
        _ => return None,
    })
}
pub fn can_open(mime: &str) -> bool {
    matches!(
        mime,
        "application/pdf" | "image/png" | "image/jpeg" | "text/plain"
    )
}
pub fn download(
    value: Value,
    id: i64,
    expected_mime: &str,
    expected_hash: &str,
) -> Result<PathBuf, String> {
    let mime = value["original"]["contentType"]
        .as_str()
        .ok_or("Originalens type mangler")?;
    if mime != expected_mime {
        return Err("Originalens type afviger fra kanonisk metadata; intet åbnes".into());
    }
    let extension =
        extension(mime).ok_or("Originaltypen understøttes ikke i denne native visning")?;
    let values = value["original"]["bytes"]
        .as_array()
        .ok_or("Originalens bytes mangler")?;
    if id <= 0 || values.is_empty() || values.len() > 25 * 1024 * 1024 {
        return Err("Originalens id eller størrelse er ugyldig".into());
    }
    let bytes = values
        .iter()
        .map(|value| {
            value
                .as_u64()
                .filter(|n| *n <= 255)
                .map(|n| n as u8)
                .ok_or("Originalen indeholder ugyldige bytes")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if (mime == "application/pdf" && !bytes.starts_with(b"%PDF-"))
        || (mime == "image/png" && !bytes.starts_with(b"\x89PNG\r\n\x1a\n"))
        || (mime == "image/jpeg" && !bytes.starts_with(b"\xff\xd8\xff"))
    {
        return Err("Originalens indhold stemmer ikke med filtypen".into());
    }
    if format!("{:x}", Sha256::digest(&bytes)) != expected_hash {
        return Err(
            "Originalens SHA-256 stemmer ikke med kanonisk identitet; filen åbnes ikke".into(),
        );
    }
    let mut random = [0; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(|_| "Privat filnavn kunne ikke dannes")?;
    let suffix = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let path = ensure()?.join(format!("original-{id}-{suffix}.{extension}"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|_| "Privat original kunne ikke oprettes")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Privat original kunne ikke gemmes")?;
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn upload_marker_cannot_retain_name_metadata_or_original_bytes() {
        let mut value = json!({"format":"rentemester-desktop-document/1","origin":"https://synthetic.example","company":"synthetic","principal":"owner","ledger":"immutable","localIdentity":null,"byteCount":12,"mime":"text/plain","beforeIds":[]});
        assert!(valid_record(&value));
        value["fileName"] = json!("Private.txt");
        assert!(!valid_record(&value));
        value.as_object_mut().unwrap().remove("fileName");
        value["body"] = json!("must not persist");
        assert!(!valid_record(&value));
    }
    #[test]
    fn viewer_types_never_include_active_structured_formats() {
        for mime in ["application/pdf", "image/png", "image/jpeg", "text/plain"] {
            assert!(can_open(mime));
        }
        for mime in [
            "application/json",
            "application/xml",
            "text/html",
            "application/octet-stream",
        ] {
            assert!(!can_open(mime));
        }
        assert!(extension("text/html").is_none());
    }
    #[test]
    fn unknown_marker_survives_reload_and_cannot_be_overwritten_or_silently_deleted() {
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "rm-document-state-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let record = json!({"format":"rentemester-desktop-document/1","origin":"https://synthetic.example","company":"synthetic","principal":"owner","ledger":"immutable","localIdentity":null,"byteCount":12,"mime":"text/plain","sha256Hash":"a".repeat(64),"beforeIds":[]});
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
    #[test]
    fn download_rejects_hash_mime_and_signature_mismatch_before_writing_or_opening() {
        let value = json!({"ok":true,"original":{"contentType":"text/plain","bytes":[65,66]}});
        assert!(download(value.clone(), 1, "text/plain", &"0".repeat(64)).is_err());
        assert!(download(value, 1, "application/pdf", &"0".repeat(64)).is_err());
        assert!(
            download(
                json!({"original":{"contentType":"application/pdf","bytes":[65,66]}}),
                1,
                "application/pdf",
                &"0".repeat(64)
            )
            .is_err()
        );
        assert!(
            download(
                json!({"original":{"contentType":"text/html","bytes":[65,66]}}),
                1,
                "text/html",
                &"0".repeat(64)
            )
            .is_err()
        );
    }
}
