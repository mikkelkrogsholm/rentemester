use super::*;
use ring::signature::{Ed25519KeyPair, KeyPair};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
fn policy() -> Policy {
    Policy {
        platform: "macos-aarch64".into(),
        api_epoch: 1,
        control_schema: 32,
        company_schema: 36,
    }
}
struct Fixture {
    temp: tempfile::TempDir,
    key: Ed25519KeyPair,
    manifest: Manifest,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let mut files = Vec::new();
        for path in REQUIRED
            .iter()
            .copied()
            .chain(["Contents/Resources/native-artifact-manifest.json"])
        {
            let target = temp.path().join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(&target, b"synthetic artifact").unwrap();
            let executable = path.starts_with("Contents/MacOS/");
            #[cfg(unix)]
            fs::set_permissions(
                &target,
                fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
            )
            .unwrap();
            files.push(FileEntry {
                path: path.into(),
                size: 18,
                sha256: format!("{:x}", Sha256::digest(b"synthetic artifact")),
                executable,
            });
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let manifest = Manifest {
            format: "rentemester-native-update/1".into(),
            bundle_id: "dk.rentemester.desktop".into(),
            version: "1.2.3".into(),
            sequence: 2,
            platform: "macos-aarch64".into(),
            api_epoch: 1,
            control_schema: 32,
            company_schema: 36,
            source_revision: "a".repeat(40),
            toolchains: BTreeMap::from([("server".into(), "1.94.1".into())]),
            files,
        };
        let f = Self {
            temp,
            key,
            manifest,
        };
        f.sign();
        f
    }
    fn sign(&self) {
        let raw = serde_json::to_vec(&self.manifest).unwrap();
        fs::write(self.temp.path().join(MANIFEST), &raw).unwrap();
        fs::write(
            self.temp.path().join(SIGNATURE),
            self.key.sign(&raw).as_ref(),
        )
        .unwrap();
    }
    fn verify(&self) -> Result<VerifiedPackage, Error> {
        verify(self.temp.path(), self.key.public_key().as_ref(), &policy())
    }
}
#[test]
#[cfg(unix)]
fn signed_package_exact_inventory_and_external_trust_anchor() {
    let f = Fixture::new();
    let verified = f.verify().unwrap();
    assert_eq!(verified.manifest().sequence, 2);
    assert_eq!(verified.root(), f.temp.path());
    assert_eq!(verified.manifest_sha256().len(), 64);
    let wrong = Ed25519KeyPair::from_seed_unchecked(&[8; 32]).unwrap();
    assert!(verify(f.temp.path(), wrong.public_key().as_ref(), &policy()).is_err());
    fs::write(f.temp.path().join(SIGNATURE), [0; 64]).unwrap();
    assert!(f.verify().is_err());
}
#[test]
#[cfg(unix)]
fn signed_package_tamper_missing_unlisted_and_mode_refused() {
    for kind in 0..4 {
        let f = Fixture::new();
        let target = f.temp.path().join(REQUIRED[0]);
        match kind {
            0 => fs::write(target, b"tampered").unwrap(),
            1 => fs::remove_file(target).unwrap(),
            2 => fs::write(f.temp.path().join("Contents/extra"), b"extra").unwrap(),
            _ => fs::set_permissions(target, fs::Permissions::from_mode(0o644)).unwrap(),
        };
        assert!(f.verify().is_err(), "case {kind}");
    }
}
#[test]
#[cfg(unix)]
fn signed_package_policy_paths_and_bounds_refused_even_with_valid_signature() {
    for kind in 0..8 {
        let mut f = Fixture::new();
        match kind {
            0 => f.manifest.control_schema += 1,
            1 => f.manifest.company_schema += 1,
            2 => f.manifest.api_epoch += 1,
            3 => f.manifest.platform = "linux-x86_64".into(),
            4 => f.manifest.files[0].path = "Contents/../workspace.db".into(),
            5 => f.manifest.sequence = 0,
            6 => f.manifest.version = "01.2.3".into(),
            _ => f.manifest.files[0].path = "workspace.db".into(),
        };
        f.sign();
        assert!(f.verify().is_err(), "case {kind}");
    }
}
#[test]
#[cfg(unix)]
fn signed_package_links_and_unsafe_permissions_refused() {
    for kind in 0..4 {
        let f = Fixture::new();
        let path = f.temp.path().join("Contents/extra");
        match kind {
            0 => std::os::unix::fs::symlink(f.temp.path().join(REQUIRED[0]), path).unwrap(),
            1 => fs::hard_link(f.temp.path().join(REQUIRED[0]), path).unwrap(),
            2 => fs::set_permissions(
                f.temp.path().join(REQUIRED[0]),
                fs::Permissions::from_mode(0o777),
            )
            .unwrap(),
            _ => {
                let alias = f.temp.path().join("alias");
                std::os::unix::fs::symlink(f.temp.path(), &alias).unwrap();
                assert!(verify(&alias, f.key.public_key().as_ref(), &policy()).is_err());
                continue;
            }
        };
        assert!(f.verify().is_err());
    }
}
