//! Operator-selected local/synced directories. No cloud upload, inferred residency or restore authority.
use crate::backup_artifact::{self as artifact, TrustedPublicKey};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Config {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub directory: PathBuf,
    pub non_related_party: bool,
    pub in_eea_or_eu: bool,
    pub country: String,
    pub meets_recognised_standards: bool,
    pub attested_by: String,
    pub attested_at: String,
}
pub fn valid_id(id: &str) -> bool {
    (8..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
impl Config {
    pub fn view(&self) -> Value {
        json!({"id":self.id,"label":self.label,"kind":self.kind,
            "nonRelatedParty":self.non_related_party,"regionAttestation":{"inEeaOrEu":self.in_eea_or_eu,"country":self.country,"attestedBy":self.attested_by,"attestedAt":self.attested_at},
            "itSecurityAttestation":{"meetsRecognisedStandards":self.meets_recognised_standards,"attestedBy":self.attested_by,"attestedAt":self.attested_at},
            "offsiteAttested":self.non_related_party && self.in_eea_or_eu && self.meets_recognised_standards,
            "attestationIndependentlyVerified":false,"transfer":"local-directory-copy","remoteSyncVerified":false})
    }
    pub fn validate(&self, workspace: &std::path::Path, uid: u32) -> Result<(), &'static str> {
        if !valid_id(&self.id)
            || self.label.trim().is_empty()
            || self.label.len() > 128
            || !["local-folder", "dropbox", "google-drive"].contains(&self.kind.as_str())
            || self.attested_by.trim().is_empty()
            || self.attested_by.len() > 128
            || self.country.len() != 2
            || !self.country.bytes().all(|b| b.is_ascii_uppercase())
            || chrono::DateTime::parse_from_rfc3339(&self.attested_at).is_err()
            || !self.directory.is_absolute()
            || self.directory.starts_with(workspace)
            || workspace.starts_with(&self.directory)
        {
            return Err("backup_destination_invalid_configuration");
        }
        open_directory(self, uid).map(|_| ())
    }
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn open_directory(config: &Config, uid: u32) -> Result<std::fs::File, &'static str> {
    crate::namespace::backup_directory(&config.directory, uid)
        .map_err(|_| "backup_destination_unsafe_directory")
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn open_directory(_: &Config, _: u32) -> Result<std::fs::File, &'static str> {
    Err("backup_destination_unsupported_platform")
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn verify(
    config: &Config,
    uid: u32,
    operation: &str,
    trusted: &TrustedPublicKey,
    workspace: &str,
) -> Result<Value, &'static str> {
    use rustix::fs::{FileType, Mode, OFlags, fstat, openat};
    use std::{fs::File, io::Read};
    if !valid_id(operation) {
        return Err("backup_destination_invalid_operation");
    }
    let directory = open_directory(config, uid)?;
    let mut file = File::from(
        openat(
            &directory,
            format!("{operation}.native-backup.json").as_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| "backup_destination_file_unavailable")?,
    );
    let before = fstat(&file).map_err(|_| "backup_destination_file_unavailable")?;
    if FileType::from_raw_mode(before.st_mode) != FileType::RegularFile
        || before.st_uid != uid
        || before.st_nlink != 1
        || before.st_mode & 0o7777 != 0o600
        || before.st_size <= 0
        || before.st_size as usize > artifact::MAX_WIRE_BYTES
    {
        return Err("backup_destination_unsafe_file");
    }
    crate::namespace::backup_file_acl(&file).map_err(|_| "backup_destination_unsafe_file")?;
    let mut wire = Vec::new();
    (&mut file)
        .take(artifact::MAX_WIRE_BYTES as u64 + 1)
        .read_to_end(&mut wire)
        .map_err(|_| "backup_destination_read_failed")?;
    let after = fstat(&file).map_err(|_| "backup_destination_read_failed")?;
    if wire.len() != before.st_size as usize
        || before.st_dev != after.st_dev
        || before.st_ino != after.st_ino
        || before.st_ctime != after.st_ctime
        || before.st_ctime_nsec != after.st_ctime_nsec
    {
        return Err("backup_destination_changed");
    }
    let verified = artifact::verify(&wire, trusted)
        .map_err(|_| "backup_destination_signature_or_inventory_refused")?;
    if verified.manifest().context.workspace_id != workspace {
        return Err("backup_destination_wrong_workspace");
    }
    let current = open_directory(config, uid)?;
    let now = fstat(&current).map_err(|_| "backup_destination_changed")?;
    let original = fstat(&directory).map_err(|_| "backup_destination_changed")?;
    if now.st_dev != original.st_dev || now.st_ino != original.st_ino {
        return Err("backup_destination_changed");
    }
    Ok(
        json!({"format":"rentemester-native-backup-placement/1","destinationId":config.id,"operationId":operation,
        "archiveSha256":crate::backup_keys::digest(&wire),"archiveSizeBytes":wire.len(),"backupCreatedAt":verified.manifest().context.created_at,
        "verified":true,"verifyMethod":"sha256-reread","signatureVerified":true,"remoteSyncVerified":false,"destination":config.view()}),
    )
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn place(
    config: &Config,
    uid: u32,
    operation: &str,
    wire: &[u8],
    trusted: &TrustedPublicKey,
    workspace: &str,
) -> Result<Value, &'static str> {
    use rustix::fs::{AtFlags, Mode, OFlags, RenameFlags, openat, renameat_with, unlinkat};
    use std::{fs::File, io::Write};
    if !valid_id(operation) {
        return Err("backup_destination_invalid_operation");
    }
    let verified = artifact::verify(wire, trusted)
        .map_err(|_| "backup_destination_signature_or_inventory_refused")?;
    if verified.manifest().context.workspace_id != workspace {
        return Err("backup_destination_wrong_workspace");
    }
    let directory = open_directory(config, uid)?;
    let temporary = format!(
        ".backup-{}",
        crate::auth::new_token()
            .map_err(|_| "backup_destination_random_unavailable")?
            .as_str()
    );
    let result = (|| {
        let mut file = File::from(
            openat(
                &directory,
                temporary.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            )
            .map_err(|_| "backup_destination_write_failed")?,
        );
        file.write_all(wire)
            .and_then(|_| file.sync_all())
            .map_err(|_| "backup_destination_write_failed")?;
        renameat_with(
            &directory,
            temporary.as_str(),
            &directory,
            format!("{operation}.native-backup.json").as_str(),
            RenameFlags::NOREPLACE,
        )
        .map_err(|e| {
            if e == rustix::io::Errno::EXIST {
                "backup_destination_exists_read_back"
            } else {
                "backup_destination_publication_unknown"
            }
        })?;
        directory
            .sync_all()
            .map_err(|_| "backup_destination_publication_unknown")?;
        let receipt = verify(config, uid, operation, trusted, workspace)
            .map_err(|_| "backup_destination_publication_unknown")?;
        if receipt["archiveSha256"] != crate::backup_keys::digest(wire) {
            return Err("backup_destination_publication_unknown");
        }
        Ok(receipt)
    })();
    let _ = unlinkat(&directory, temporary.as_str(), AtFlags::empty());
    result
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn verify(
    _: &Config,
    _: u32,
    _: &str,
    _: &TrustedPublicKey,
    _: &str,
) -> Result<Value, &'static str> {
    Err("backup_destination_unsupported_platform")
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn place(
    _: &Config,
    _: u32,
    _: &str,
    _: &[u8],
    _: &TrustedPublicKey,
    _: &str,
) -> Result<Value, &'static str> {
    Err("backup_destination_unsupported_platform")
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;
    use artifact::{
        BusinessSection, Company, Context, Coverage, EntryKind, Exclusion, SigningKey, Snapshot,
        UnsupportedSection,
    };
    use std::os::unix::fs::PermissionsExt;
    fn wire() -> (SigningKey, Vec<u8>) {
        let pkcs8 =
            ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
                .unwrap();
        let key = SigningKey::from_pkcs8(pkcs8.as_ref()).unwrap();
        let context = Context {
            workspace_id: "synthetic-workspace".into(),
            created_at: "2026-01-02T03:04:05.000Z".into(),
            control_schema_version: 31,
            companies: vec![Company {
                slug: "synthetic".into(),
                ledger_uuid: "a".repeat(32),
                schema_version: 36,
            }],
            coverage: Coverage {
                included: vec![BusinessSection::WorkspaceAudit],
                unsupported: BusinessSection::ALL
                    .into_iter()
                    .filter(|s| *s != BusinessSection::WorkspaceAudit)
                    .map(|section| UnsupportedSection {
                        section,
                        record_count: 0,
                    })
                    .collect(),
                excluded: Exclusion::ALL.to_vec(),
            },
        };
        let entries = vec![
            Snapshot {
                kind: EntryKind::WorkspaceManifest,
                path: "workspace.json".into(),
                bytes: b"Synthetic namespace; runtime does not validate SQLite semantics".to_vec(),
            },
            Snapshot {
                kind: EntryKind::CompanyLedger {
                    company: "synthetic".into(),
                },
                path: "synthetic/data/ledger.sqlite".into(),
                bytes: b"Synthetic signed opaque unit bytes".to_vec(),
            },
            Snapshot {
                kind: EntryKind::ControlBusiness {
                    section: BusinessSection::WorkspaceAudit,
                },
                path: "control/workspace-audit.json".into(),
                bytes: b"{}".to_vec(),
            },
        ];
        let wire = artifact::create(context, entries, &key).unwrap();
        (key, wire)
    }
    fn fixture() -> (tempfile::TempDir, Config, u32) {
        let root = tempfile::Builder::new()
            .prefix(".rm-destination-test-")
            .tempdir_in(if cfg!(target_os = "macos") {
                PathBuf::from(std::env::var_os("HOME").unwrap()).join("Library/Application Support")
            } else {
                PathBuf::from(std::env::var_os("HOME").unwrap())
            })
            .unwrap();
        let path = root.path().canonicalize().unwrap().join("destination");
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = Config {
            id: "synthetic-destination".into(),
            label: "Synthetic copy".into(),
            kind: "local-folder".into(),
            directory: path,
            non_related_party: false,
            in_eea_or_eu: false,
            country: "DK".into(),
            meets_recognised_standards: false,
            attested_by: "Synthetic operator".into(),
            attested_at: "2026-01-01T00:00:00Z".into(),
        };
        (root, config, rustix::process::getuid().as_raw())
    }
    #[test]
    fn exclusive_signed_copy_reread_tamper_and_wrong_workspace() {
        let (root, config, uid) = fixture();
        config
            .validate(&root.path().join("workspace"), uid)
            .unwrap();
        let (key, wire) = wire();
        let trusted = TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
        let receipt = place(
            &config,
            uid,
            "synthetic-operation",
            &wire,
            &trusted,
            "synthetic-workspace",
        )
        .unwrap();
        assert_eq!(receipt["archiveSha256"], crate::backup_keys::digest(&wire));
        assert_eq!(receipt["verified"], true);
        assert_eq!(receipt["destination"]["offsiteAttested"], false);
        assert_eq!(receipt["remoteSyncVerified"], false);
        assert_eq!(
            place(
                &config,
                uid,
                "synthetic-operation",
                &wire,
                &trusted,
                "synthetic-workspace"
            )
            .unwrap_err(),
            "backup_destination_exists_read_back"
        );
        assert_eq!(
            verify(
                &config,
                uid,
                "synthetic-operation",
                &trusted,
                "other-workspace"
            )
            .unwrap_err(),
            "backup_destination_wrong_workspace"
        );
        let path = config
            .directory
            .join("synthetic-operation.native-backup.json");
        assert_eq!(std::fs::read(&path).unwrap(), wire);
        std::fs::write(&path, b"tampered").unwrap();
        assert!(
            verify(
                &config,
                uid,
                "synthetic-operation",
                &trusted,
                "synthetic-workspace"
            )
            .is_err()
        );
    }
    #[test]
    fn unsafe_paths_links_modes_and_client_path_ids_refuse() {
        let (root, config, uid) = fixture();
        let (key, wire) = wire();
        let trusted = TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
        assert!(
            place(
                &config,
                uid,
                "../escape",
                &wire,
                &trusted,
                "synthetic-workspace"
            )
            .is_err()
        );
        let linked = config
            .directory
            .join("synthetic-symlink.native-backup.json");
        std::os::unix::fs::symlink(root.path().join("outside"), &linked).unwrap();
        assert!(
            verify(
                &config,
                uid,
                "synthetic-symlink",
                &trusted,
                "synthetic-workspace"
            )
            .is_err()
        );
        place(
            &config,
            uid,
            "synthetic-operation",
            &wire,
            &trusted,
            "synthetic-workspace",
        )
        .unwrap();
        let path = config
            .directory
            .join("synthetic-operation.native-backup.json");
        let hard = config.directory.join("hard-link");
        std::fs::hard_link(&path, &hard).unwrap();
        assert!(
            verify(
                &config,
                uid,
                "synthetic-operation",
                &trusted,
                "synthetic-workspace"
            )
            .is_err()
        );
        std::fs::remove_file(hard).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(
            verify(
                &config,
                uid,
                "synthetic-operation",
                &trusted,
                "synthetic-workspace"
            )
            .is_err()
        );
        std::fs::set_permissions(&config.directory, std::fs::Permissions::from_mode(0o755))
            .unwrap();
        assert!(
            config
                .validate(&root.path().join("workspace"), uid)
                .is_err()
        );
    }
}

/// Qualify a fixed cache file from an admitted control connection's filename.
/// No caller-provided root, remote locator, or embedded archive key is accepted.
#[cfg(any(target_os="linux", target_os="macos"))]
pub fn verify_retained(control_path:&std::path::Path,operation:&str,trusted:&TrustedPublicKey,workspace:&str)->Result<Value,&'static str>{
    if control_path.file_name().and_then(|v|v.to_str())!=Some("workspace-control.sqlite") || control_path.parent().and_then(|v|v.file_name()).and_then(|v|v.to_str())!=Some(".rentemester") {return Err("backup_retained_namespace_unavailable");}
    let root=control_path.parent().and_then(|v|v.parent()).ok_or("backup_retained_namespace_unavailable")?;
    let uid=rustix::process::geteuid().as_raw();
    let config=Config{id:"native-local-backups".into(),label:"Local retained snapshots".into(),kind:"local-folder".into(),directory:root.join("native-backups"),non_related_party:false,in_eea_or_eu:false,country:"ZZ".into(),meets_recognised_standards:false,attested_by:"system:local-retention".into(),attested_at:"1970-01-01T00:00:00Z".into()};
    verify(&config,uid,operation,trusted,workspace)
}
#[cfg(not(any(target_os="linux", target_os="macos")))]
pub fn verify_retained(_: &std::path::Path,_:&str,_:&TrustedPublicKey,_:&str)->Result<Value,&'static str>{Err("backup_destination_unsupported_platform")}

/// Read an original source artifact under explicit local operator custody. Never opens a live
/// workspace resource: only retained backups or external operator files are admitted.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn read_original717_restore(
    source: &std::path::Path,
    verify_key: Option<&std::path::Path>,
    public_key: Option<&std::path::Path>,
    root: &std::path::Path,
    uid: u32,
    _operator: &crate::bootstrap::Operator,
) -> Result<crate::backup_artifact::original717::VerifiedOriginal717, &'static str> {
    use crate::backup_artifact::original717 as codec;
    use std::{fs::File, io::Read, os::unix::fs::MetadataExt};
    fn read(
        path: &std::path::Path,
        root: &std::path::Path,
        uid: u32,
        max: usize,
        secret: bool,
    ) -> Result<Vec<u8>, &'static str> {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("restore_unsafe_source")?;
        let parent = path.parent().ok_or("restore_unsafe_source")?;
        if !path.is_absolute()
            || path.components().any(|p| {
                matches!(
                    p,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
        {
            return Err("restore_unsafe_source");
        }
        if path.starts_with(root) {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "restore_unsafe_source")?;
            let parts: Vec<_> = relative.components().collect();
            if !(parts.len() >= 3 && parts[1].as_os_str() == "backups"
                || secret && parts.len() == 2 && name == ".backup-manifest.key")
            {
                return Err("restore_live_source_refused");
            }
        }
        let dir =
            crate::namespace::backup_directory(parent, uid).map_err(|_| "restore_unsafe_source")?;
        let before = rustix::fs::statat(&dir, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| "restore_source_unavailable")?;
        if before.st_uid != uid
            || before.st_nlink != 1
            || before.st_mode & 0o7777 != 0o600
            || rustix::fs::FileType::from_raw_mode(before.st_mode)
                != rustix::fs::FileType::RegularFile
            || before.st_size < 0
            || before.st_size as u64 > max as u64
        {
            return Err("restore_unsafe_source");
        }
        let mut file = File::from(
            rustix::fs::openat(
                &dir,
                name,
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC
                    | rustix::fs::OFlags::NONBLOCK,
                rustix::fs::Mode::empty(),
            )
            .map_err(|_| "restore_source_unavailable")?,
        );
        crate::namespace::backup_file_acl(&file).map_err(|_| "restore_unsafe_source")?;
        let m = file.metadata().map_err(|_| "restore_source_unavailable")?;
        if m.dev() != before.st_dev || m.ino() != before.st_ino {
            return Err("restore_source_changed");
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(max as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "restore_source_unavailable")?;
        let after = rustix::fs::statat(&dir, name, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| "restore_source_changed")?;
        if bytes.len() > max
            || before.st_dev != after.st_dev
            || before.st_ino != after.st_ino
            || before.st_size != after.st_size
            || before.st_mtime != after.st_mtime
            || before.st_mtime_nsec != after.st_mtime_nsec
            || before.st_ctime != after.st_ctime
            || before.st_ctime_nsec != after.st_ctime_nsec
        {
            return Err("restore_source_changed");
        }
        Ok(bytes)
    }
    let key_path = verify_key
        .map(std::path::Path::to_owned)
        .or_else(|| {
            source
                .parent()
                .filter(|p| p.file_name().is_some_and(|s| s == "backups"))
                .and_then(|p| p.parent())
                .map(|p| p.join(".backup-manifest.key"))
        })
        .ok_or("HMAC verification key is required")?;
    let key_text = zeroize::Zeroizing::new(read(&key_path, root, uid, 65, true)?);
    let key = zeroize::Zeroizing::new(
        codec::symmetric_key(std::str::from_utf8(&key_text).map_err(|_| "invalid HMAC key")?)
            .map_err(|_| "invalid HMAC key")?,
    );
    let public = public_key
        .map(|p| read(p, root, uid, 1024, false))
        .transpose()?;
    let mut files = codec::Files::new();
    if source.extension().is_some_and(|s| s == "tar") {
        let bytes = read(
            source,
            root,
            uid,
            artifact::MAX_TOTAL_BYTES + (artifact::MAX_ENTRIES + 2) * 1024,
            false,
        )?;
        files = codec::read_archive(&bytes).map_err(|_| "invalid backup archive")?;
    } else {
        fn walk_source(source:&std::path::Path,current:&std::path::Path,prefix:&str,root:&std::path::Path,uid:u32,files:&mut codec::Files,total:&mut usize)->Result<(),&'static str>{
            if prefix.len()>256||prefix.split('/').count()>32{return Err("restore_bounds");}
            let dir=crate::namespace::backup_directory(current,uid).map_err(|_|"restore_unsafe_source")?;
            for entry in rustix::fs::Dir::read_from(&dir).map_err(|_|"restore_source_unavailable")?{
                let entry=entry.map_err(|_|"restore_source_unavailable")?;
                let name=entry.file_name().to_str().map_err(|_|"restore_unsafe_source")?;
                if name=="."||name==".."{continue;}
                if name.contains(['/', '\\', ':', '\0']){return Err("restore_unsafe_source");}
                let path=format!("{prefix}{name}");
                let st=rustix::fs::statat(&dir,name,rustix::fs::AtFlags::SYMLINK_NOFOLLOW).map_err(|_|"restore_source_unavailable")?;
                if rustix::fs::FileType::from_raw_mode(st.st_mode)==rustix::fs::FileType::Directory{
                    walk_source(source,&current.join(name),&format!("{path}/"),root,uid,files,total)?;
                }else{
                    let bytes=read(&source.join(&path),root,uid,artifact::MAX_FILE_BYTES,false)?;
                    *total=total.checked_add(bytes.len()).ok_or("restore_bounds")?;
                    if *total>artifact::MAX_TOTAL_BYTES||files.len()>=artifact::MAX_ENTRIES{return Err("restore_bounds");}
                    if files.insert(path,bytes).is_some(){return Err("restore_duplicate");}
                }
            }
            Ok(())
        }
        walk_source(source,source,"",root,uid,&mut files,&mut 0)?;
    }
    codec::verify(&files, &key, public.as_deref())
        .map_err(|_| "backup manifest integrity verification failed")
}

/// Original TAR copy, not the native signed-JSON publisher. Integrity comes from the opaque
/// company-scoped proof; a configured destination is required independently of request metadata.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn place_original717(
    config: &Config,
    uid: u32,
    proof: &crate::backup_artifact::original717::VerifiedOriginal717,
) -> Result<(), &'static str> {
    use rustix::fs::{
        AtFlags, FileType, Mode, OFlags, RenameFlags, fstat, openat, renameat_with, statat,
    };
    use std::{
        fs::File,
        io::{Read, Write},
    };
    let directory = open_directory(config, uid)?;
    let bytes = crate::backup_artifact::original717::archive(proof.files())
        .map_err(|_| "backup_archive_invalid")?;
    let name = format!("{}.tar", proof.manifest().backup_id);
    let sidecar = format!(
        "{}  {name}\n",
        crate::backup_artifact::original717::sha256(&bytes)
    );
    for (leaf, expected) in [
        (name.clone(), bytes.as_slice()),
        (format!("{name}.sha256"), sidecar.as_bytes()),
    ] {
        match statat(&directory, leaf.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
            Ok(st) => {
                if st.st_uid != uid
                    || st.st_nlink != 1
                    || st.st_mode & 0o7777 != 0o600
                    || FileType::from_raw_mode(st.st_mode) != FileType::RegularFile
                    || st.st_size < 0
                    || st.st_size as usize != expected.len()
                {
                    return Err("backup_destination_existing_mismatch");
                }
            }
            Err(rustix::io::Errno::NOENT) => {
                let token = crate::auth::new_token()
                    .map_err(|_| "backup_destination_random_unavailable")?;
                let temporary = format!(".backup-{}", token.as_str());
                let mut f = File::from(
                    openat(
                        &directory,
                        temporary.as_str(),
                        OFlags::WRONLY
                            | OFlags::CREATE
                            | OFlags::EXCL
                            | OFlags::NOFOLLOW
                            | OFlags::CLOEXEC,
                        Mode::from_raw_mode(0o600),
                    )
                    .map_err(|_| "backup_destination_write_failed")?,
                );
                f.write_all(expected)
                    .and_then(|_| f.sync_all())
                    .map_err(|_| "backup_destination_publication_unknown")?;
                renameat_with(
                    &directory,
                    temporary.as_str(),
                    &directory,
                    leaf.as_str(),
                    RenameFlags::NOREPLACE,
                )
                .map_err(|_| "backup_destination_publication_unknown")?;
                directory
                    .sync_all()
                    .map_err(|_| "backup_destination_publication_unknown")?;
            }
            Err(_) => return Err("backup_destination_unsafe_file"),
        }
        let before = statat(&directory, leaf.as_str(), AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| "backup_destination_publication_unknown")?;
        let mut f = File::from(
            openat(
                &directory,
                leaf.as_str(),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| "backup_destination_publication_unknown")?,
        );
        let opened = fstat(&f).map_err(|_| "backup_destination_publication_unknown")?;
        if opened.st_dev != before.st_dev
            || opened.st_ino != before.st_ino
            || opened.st_nlink != 1
            || opened.st_mode & 0o7777 != 0o600
            || opened.st_uid != uid
            || FileType::from_raw_mode(opened.st_mode) != FileType::RegularFile
        {
            return Err("backup_destination_publication_unknown");
        }
        crate::namespace::backup_file_acl(&f)
            .map_err(|_| "backup_destination_publication_unknown")?;
        let mut actual = Vec::new();
        Read::by_ref(&mut f)
            .take(expected.len() as u64 + 1)
            .read_to_end(&mut actual)
            .map_err(|_| "backup_destination_publication_unknown")?;
        let after = statat(&directory, leaf.as_str(), AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| "backup_destination_publication_unknown")?;
        if actual != expected
            || after.st_dev != before.st_dev
            || after.st_ino != before.st_ino
            || after.st_size != before.st_size
            || after.st_mtime != before.st_mtime
            || after.st_mtime_nsec != before.st_mtime_nsec
            || after.st_ctime != before.st_ctime
            || after.st_ctime_nsec != before.st_ctime_nsec
        {
            return Err("backup_destination_publication_unknown");
        }
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn verify_original717_archive(
    path: &std::path::Path,
    uid: u32,
    expected: &[u8],
) -> Result<(), &'static str> {
    use rustix::fs::{AtFlags, FileType, Mode, OFlags, fstat, openat, statat};
    use std::{fs::File, io::Read};
    let directory =
        crate::namespace::backup_directory(path.parent().ok_or("backup_archive_invalid")?, uid)
            .map_err(|_| "backup_archive_invalid")?;
    let leaf = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("backup_archive_invalid")?;
    let sidecar = format!(
        "{}  {leaf}\n",
        crate::backup_artifact::original717::sha256(expected)
    );
    for (name, bytes) in [
        (leaf.to_owned(), expected),
        (format!("{leaf}.sha256"), sidecar.as_bytes()),
    ] {
        let before = statat(&directory, name.as_str(), AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| "canonical local backup archive is missing")?;
        if before.st_uid != uid
            || before.st_nlink != 1
            || before.st_mode & 0o7777 != 0o600
            || FileType::from_raw_mode(before.st_mode) != FileType::RegularFile
            || before.st_size < 0
            || before.st_size as usize != bytes.len()
        {
            return Err("canonical archive checksum mismatch");
        }
        let mut file = File::from(
            openat(
                &directory,
                name.as_str(),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| "canonical archive unavailable")?,
        );
        crate::namespace::backup_file_acl(&file).map_err(|_| "canonical archive unsafe")?;
        let opened = fstat(&file).map_err(|_| "canonical archive unavailable")?;
        if before.st_dev != opened.st_dev || before.st_ino != opened.st_ino {
            return Err("canonical archive changed");
        }
        let mut actual = Vec::new();
        Read::by_ref(&mut file)
            .take(bytes.len() as u64 + 1)
            .read_to_end(&mut actual)
            .map_err(|_| "canonical archive unavailable")?;
        let after = statat(&directory, name.as_str(), AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| "canonical archive changed")?;
        if actual != bytes
            || before.st_dev != after.st_dev
            || before.st_ino != after.st_ino
            || before.st_size != after.st_size
            || before.st_mtime != after.st_mtime
            || before.st_mtime_nsec != after.st_mtime_nsec
            || before.st_ctime != after.st_ctime
            || before.st_ctime_nsec != after.st_ctime_nsec
        {
            return Err("canonical archive changed");
        }
    }
    Ok(())
}
