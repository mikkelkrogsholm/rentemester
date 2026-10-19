//! Exact transport bytes and an explicit user destination. No archive verification or restore.
use rentemester_client::{MAX_WORKSPACE_BACKUP_BYTES, WORKSPACE_BACKUP_CONTENT_TYPE};
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path},
};
#[derive(Clone, Debug, PartialEq)]
pub struct Scope {
    pub user: String,
    pub workspace_role: String,
    pub companies: Value,
}
impl Scope {
    pub fn parse(value: Value) -> Result<Self, String> {
        if value["ok"] != true || !value["companies"].is_array() {
            return Err("Serverens aktuelle workspaceoversigt mangler".into());
        }
        let user = value["user"]["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Aktuel bruger mangler")?
            .into();
        let workspace_role = value["workspaceRole"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Aktuel workspacerolle mangler")?
            .into();
        Ok(Self {
            user,
            workspace_role,
            companies: value["companies"].clone(),
        })
    }
}
pub struct Archive {
    pub bytes: Vec<u8>,
}
impl Archive {
    pub fn from_response(value: Value) -> Result<Self, String> {
        if value["ok"] != true || value["backup"]["contentType"] != WORKSPACE_BACKUP_CONTENT_TYPE {
            return Err("Backupsvaret mangler den forventede native transporttype".into());
        }
        let values = value["backup"]["bytes"]
            .as_array()
            .ok_or("Backupsvaret mangler bytes")?;
        if values.is_empty() || values.len() > MAX_WORKSPACE_BACKUP_BYTES {
            return Err(
                "Backupsvaret overskrider den understøttede størrelsesgrænse eller er tomt".into(),
            );
        }
        let bytes = values
            .iter()
            .map(|v| {
                v.as_u64()
                    .filter(|n| *n <= 255)
                    .map(|n| n as u8)
                    .ok_or("Backupsvaret indeholder ugyldige bytes")
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { bytes })
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        save_bytes(&self.bytes, path)
    }
}
fn save_bytes(bytes: &[u8], path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path.file_name().is_none()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(
            "Vælg et nyt absolut filnavn uden . eller ..; ingen sti udledes automatisk".into(),
        );
    }
    let parent = path.parent().ok_or("Destinationsmappen mangler")?;
    if parent
        .canonicalize()
        .map_err(|_| "Destinationsmappen findes ikke")?
        != parent
    {
        return Err(
            "Destinationen må ikke gå gennem symbolske links; vælg mappens kanoniske sti".into(),
        );
    }
    let meta =
        fs::symlink_metadata(parent).map_err(|_| "Destinationsmappen kunne ikke undersøges")?;
    let uid = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .map_err(|_| "Brugeridentitet kunne ikke læses")?;
    if !meta.is_dir()
        || meta.mode() & 0o022 != 0
        || String::from_utf8_lossy(&uid.stdout)
            .trim()
            .parse::<u32>()
            .ok()
            != Some(meta.uid())
    {
        return Err("Vælg din egen mappe, som andre ikke kan skrive i".into());
    }
    // create_new refuses existing files and both ordinary/dangling final symlinks.
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| "Ny privat fil kunne ikke oprettes; eksisterende filer overskrives aldrig")?;
    let meta = file
        .metadata()
        .map_err(|_| "Destinationsfilen kunne ikke undersøges")?;
    let acl = std::process::Command::new("/bin/ls")
        .arg("-lde")
        .arg(path)
        .output()
        .map_err(|_| "Filens adgangskontrol kunne ikke undersøges")?;
    if !meta.is_file()
        || meta.mode() & 0o077 != 0
        || meta.nlink() != 1
        || !acl.status.success()
        || String::from_utf8_lossy(&acl.stdout)
            .lines()
            .skip(1)
            .any(|l| !l.trim().is_empty())
    {
        return Err(
            "Filen kræver privat0600-adgang uden ekstra adgangslister; ingen backupbytes skrevet"
                .into(),
        );
    }
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Filen kunne ikke gemmes fuldstændigt; en delvis fil kan være oprettet")?;
    fs::File::open(parent)
        .and_then(|p| p.sync_all())
        .map_err(|_| "Mappens lagring kunne ikke bekræftes; kontrollér den oprettede fil")?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::os::unix::fs::{DirBuilderExt, symlink};
    fn temp() -> std::path::PathBuf {
        let d = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "rm-desktop-backup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&d).unwrap();
        d
    }
    #[test]
    fn exact_raw_bytes_private_create_new_and_failed_save_retains_archive() {
        let d = temp();
        let a=Archive::from_response(json!({"ok":true,"backup":{"contentType":WORKSPACE_BACKUP_CONTENT_TYPE,"bytes":[0,255,10,13,123,125]}})).unwrap();
        let p = d.join("chosen.native-backup.json");
        a.save(&p).unwrap();
        assert_eq!(fs::read(&p).unwrap(), a.bytes);
        assert_eq!(fs::metadata(&p).unwrap().mode() & 0o777, 0o600);
        assert!(a.save(&p).is_err());
        assert_eq!(fs::read(&p).unwrap(), a.bytes);
        let q = d.join("different-name.json");
        a.save(&q).unwrap();
        assert_eq!(fs::read(q).unwrap(), a.bytes);
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn symlink_final_parent_and_relative_or_traversing_destinations_refused() {
        let d = temp();
        let a = Archive {
            bytes: vec![1, 2, 3],
        };
        let target = d.join("untouched");
        fs::write(&target, b"unchanged").unwrap();
        let link = d.join("alias");
        symlink(&target, &link).unwrap();
        assert!(a.save(&link).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"unchanged");
        let missing = d.join("missing-alias");
        symlink(d.join("absent"), &missing).unwrap();
        assert!(a.save(&missing).is_err());
        let parent = d.join("directory-alias");
        symlink(&d, &parent).unwrap();
        assert!(a.save(&parent.join("file")).is_err());
        assert!(a.save(Path::new("relative.json")).is_err());
        assert!(a.save(&d.join("../escape.json")).is_err());
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn transport_is_not_an_artifact_verifier_and_bad_transport_bytes_refused() {
        for v in [
            json!({"ok":false}),
            json!({"ok":true,"backup":{"contentType":"application/json","bytes":[1]}}),
            json!({"ok":true,"backup":{"contentType":WORKSPACE_BACKUP_CONTENT_TYPE,"bytes":[]}}),
            json!({"ok":true,"backup":{"contentType":WORKSPACE_BACKUP_CONTENT_TYPE,"bytes":[256]}}),
        ] {
            assert!(Archive::from_response(v).is_err());
        }
        let a=Archive::from_response(json!({"ok":true,"backup":{"contentType":WORKSPACE_BACKUP_CONTENT_TYPE,"bytes":[1,2,3]}})).unwrap();
        assert_eq!(a.bytes, [1, 2, 3]);
    }
    #[test]
    fn review_scope_contains_current_user_role_and_visible_catalog_not_a_company_selector() {
        let v = json!({"ok":true,"user":{"id":"synthetic-user"},"workspaceRole":"workspace_owner","companies":[{"slug":"one","archived":false},{"slug":"two","archived":true}]});
        let a = Scope::parse(v.clone()).unwrap();
        assert_eq!(a.companies.as_array().unwrap().len(), 2);
        let mut changed = v.clone();
        changed["user"]["id"] = json!("other");
        assert_ne!(a, Scope::parse(changed).unwrap());
        let mut incomplete = v;
        incomplete.as_object_mut().unwrap().remove("workspaceRole");
        assert!(Scope::parse(incomplete).is_err());
    }
}
