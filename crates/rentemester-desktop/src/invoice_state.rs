//! A single private unresolved invoice operation. Never retry these writes automatically.
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
    let m = fs::symlink_metadata(path).map_err(|_| "Privat fakturastatus kunne ikke åbnes")?;
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
            "Fakturastatus kræver din egen private mappe0700/fil0600 uden symbolske links.".into(),
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
        return Err("Privat fakturastatus må ikke have udvidede adgangslister.".into());
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
    let path = dir.join("pending-invoice.json");
    if !path
        .try_exists()
        .map_err(|_| "Fakturastatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(524289).read_to_end(&mut bytes))
        .map_err(|_| "Fakturastatus kunne ikke læses")?;
    if bytes.len() > 524288 {
        return Err("Fakturastatus overskrider grænsen".into());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Fakturastatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret fakturaforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}
pub fn is_lifecycle(value: &Value) -> bool {
    matches!(
        value["operation"].as_str(),
        Some("receipt" | "credit" | "refund")
    )
}
fn valid_record(value: &Value) -> bool {
    if value["format"] != "rentemester-desktop-invoice/1" {
        return false;
    }
    if is_lifecycle(value) {
        value["invoiceDocumentId"].as_i64().is_some_and(|id| id > 0)
            && value.as_object().is_some_and(|object| {
                object.keys().all(|key| {
                    [
                        "format",
                        "operation",
                        "invoiceDocumentId",
                        "bankTransactionId",
                        "grossAmount",
                        "issueDate",
                        "origin",
                        "company",
                        "principal",
                        "ledger",
                        "localIdentity",
                    ]
                    .contains(&key.as_str())
                })
            })
    } else {
        matches!(value["operation"].as_str(), Some("issue" | "post"))
            && value["body"].as_str().is_some()
    }
}
pub fn resolve_lifecycle(expected: &Value) -> Result<(), String> {
    if !is_lifecycle(expected) {
        return Err("Udstedelse eller bogføring kræver sin eksisterende originalkontrol".into());
    }
    let dir = ensure()?;
    if load_at(&dir)?.as_ref() != Some(expected) {
        return Err("Fakturastatus ændrede sig; den bevares".into());
    }
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "Systemtid kunne ikke læses")?
        .as_nanos();
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join(format!("invoice-lifecycle-manually-resolved-{suffix}.json")))
        .map_err(|_| "Manuel afklaring kunne ikke arkiveres")?;
    file.write_all(expected.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Manuel afklaring kunne ikke gemmes")?;
    finish_at(&dir, expected)?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Manuel afklaring kunne ikke synkroniseres".into())
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) {
        return Err("Ugyldigt fakturaforsøg; intet sendt".into());
    }
    if value.to_string().len() > 524288 {
        return Err("Det private fakturaforsøg overskrider grænsen; intet er sendt.".into());
    }
    let mut f=OpenOptions::new().write(true).create_new(true).mode(0o600).open(dir.join("pending-invoice.json")).map_err(|_|"Et fakturaforsøg er allerede uafklaret. Afklar det før en ny udstedelse eller bogføring.")?;
    f.write_all(value.to_string().as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(|_| "Fakturaforsøget kunne ikke gemmes; intet er sendt")?;
    fs::File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(|_| "Fakturastatus kunne ikke synkroniseres")?;
    Ok(())
}
pub fn finish(expected: &Value) -> Result<(), String> {
    finish_at(&directory(), expected)
}
fn finish_at(dir: &Path, expected: &Value) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Fakturastatus har ændret sig og bevares.".into());
    }
    fs::remove_file(dir.join("pending-invoice.json"))
        .map_err(|_| "Fakturastatus kunne ikke afsluttes".into())
}
pub fn pdf(value: &Value, key: &str) -> Result<PathBuf, String> {
    let data = value[key]["bytes"]
        .as_array()
        .ok_or("Serveren returnerede ikke PDF-data")?;
    if data.len() > 16 * 1024 * 1024 {
        return Err("PDF overskrider grænsen".into());
    }
    let bytes = data
        .iter()
        .map(|v| {
            v.as_u64()
                .filter(|n| *n <= 255)
                .map(|n| n as u8)
                .ok_or("Ugyldige PDF-bytes")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !bytes.starts_with(b"%PDF-") {
        return Err("Serveren returnerede ikke en PDF".into());
    }
    let mut random = [0; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut random))
        .map_err(|_| "PDF-filnavn kunne ikke oprettes")?;
    let suffix: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let path = ensure()?.join(format!("invoice-{suffix}.pdf"));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|_| "Privat PDF kunne ikke gemmes")?;
    f.write_all(&bytes)
        .and_then(|_| f.sync_all())
        .map_err(|_| "Privat PDF kunne ikke gemmes")?;
    Ok(path)
}
pub fn file_url(path: &Path) -> String {
    let mut result = "file://".to_owned();
    for b in path.as_os_str().as_encoded_bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(b) {
            result.push(*b as char)
        } else {
            result.push_str(&format!("%{b:02X}"));
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn uncertain_invoice_cannot_be_replaced_or_silently_cleared() {
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("rm-native-invoice-state-{}", std::process::id()));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let value = json!({"format":"rentemester-desktop-invoice/1","operation":"issue","body":"exact input bytes","origin":"https://synthetic.invalid"});
        begin_at(&dir, &value).unwrap();
        assert!(begin_at(&dir, &value).is_err());
        assert_eq!(load_at(&dir).unwrap(), Some(value.clone()));
        assert!(finish_at(&dir, &json!({})).is_err());
        finish_at(&dir, &value).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn pdf_url_encodes_spaces_and_non_ascii() {
        assert_eq!(
            file_url(Path::new("/private/Invoice Æ.pdf")),
            "file:///private/Invoice%20%C3%86.pdf"
        );
    }
    #[test]
    fn lifecycle_marker_has_no_persisted_identity_reason_or_replay_body() {
        let mut value = json!({"format":"rentemester-desktop-invoice/1","operation":"credit","invoiceDocumentId":7,"grossAmount":125,"origin":"https://synthetic.example"});
        assert!(valid_record(&value));
        assert!(is_lifecycle(&value));
        value["body"] = json!("must not persist");
        assert!(!valid_record(&value));
        value.as_object_mut().unwrap().remove("body");
        value["reason"] = json!("must not persist");
        assert!(!valid_record(&value));
        assert!(!valid_record(
            &json!({"format":"rentemester-desktop-invoice/1","operation":"refund","invoiceDocumentId":0})
        ));
    }
}
