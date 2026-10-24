//! Finite offline diagnostics. Only protected private snapshots are opened by SQLite.
use crate::{Migration, Snapshot, company_baseline, journal_preview, migration};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{fs, path::Path};

fn directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
}
// Check every existing ancestor before any company fact or protected capture.
// Canonicalizing the selector would silently follow the very links we refuse.
fn directory_chain(path: &Path) -> bool {
    let absolute = if path.is_absolute() { path.to_path_buf() } else {
        let Ok(cwd) = std::env::current_dir() else { return false; };
        cwd.join(path)
    };
    let mut current = std::path::PathBuf::new();
    for component in absolute.components() {
        current.push(component);
        if !directory(&current) { return false; }
    }
    true
}
fn regular(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
}
fn schema(db: &Connection, expected: &[Migration]) -> Value {
    let required = expected.last().map_or(0, |m| m.id);
    // Observed numeric version is evidence only, never support or write authority.
    let current = db
        .query_row("SELECT max(id) FROM schema_migrations", [], |r| {
            r.get::<_, Option<i64>>(0)
        })
        .ok()
        .flatten()
        .unwrap_or(0);
    let fail = || {
        inspection(
            "corrupt",
            current,
            required,
            Some("Ledger schema or integrity validation failed"),
        )
    };
    if migration::integrity(db).is_err() {
        return fail();
    }
    if current > required {
        return inspection(
            "newer",
            current,
            required,
            Some("Ledger schema is newer than this runtime"),
        );
    }
    let history = match migration::strict_history(db, "schema_migrations", expected) {
        Ok(h) => h,
        Err(_) => return fail(),
    };
    if history.len() == expected.len() {
        return if journal_preview::require_current(db).is_ok() {
            inspection("current", current, required, None)
        } else {
            fail()
        };
    }
    if history.is_empty() || !company_baseline::recognizes(db, history.len()).unwrap_or(false) {
        return fail();
    }
    let mut result = inspection("pending", current, required, None);
    result["pending"] = json!(
        expected[history.len()..]
            .iter()
            .map(|m| json!({"id":m.id,"name":m.name,"checksum":m.checksum}))
            .collect::<Vec<_>>()
    );
    result
}
fn inspection(status: &str, current: i64, required: i64, error: Option<&str>) -> Value {
    let mut v =
        json!({"status":status,"currentVersion":current,"requiredVersion":required,"pending":[]});
    if let Some(error) = error {
        v["error"] = json!(error);
    }
    v
}
/// No default company, source SQL opens, migration, repair, network or write authority.
pub fn inspect_company(root: &Path) -> Value {
    let root: std::path::PathBuf = root.components().collect();
    let root_ok = directory_chain(&root);
    let data_ok = root_ok && directory(&root.join("data"));
    let ledger_path = root.join("data/ledger.sqlite");
    let ledger_ok = data_ok && regular(&ledger_path);
    let facts = [
        ("company_root", root_ok),
        ("data_dir", data_ok),
        ("ledger", ledger_ok),
        (
            "documents",
            root_ok
                && directory(&root.join("documents"))
                && directory(&root.join("documents/inbox")),
        ),
        ("config", root_ok && directory(&root.join("config"))),
    ];
    let missing: Vec<_> = facts
        .iter()
        .filter(|(_, ok)| !ok)
        .map(|(name, _)| *name)
        .collect();
    let mut errors: Vec<String> = missing
        .iter()
        .map(|name| format!("missing: {name}"))
        .collect();
    let mut result = json!({"ok":missing.is_empty(),"errors":[],"checks":facts.map(|(name,ok)|json!({"name":name,"ok":ok})),"missing":missing,"schema_outdated":false});
    // Wrong-type/link ledger entries are present but unavailable; never traverse them.
    if data_ok && fs::symlink_metadata(&ledger_path).is_ok() {
        let expected: Vec<Migration> = serde_json::from_str(include_str!("legacy-schema.json"))
            .expect("embedded migration catalogue");
        let required = expected.last().map_or(0, |m| m.id);
        let unavailable = || {
            inspection(
                "unavailable",
                0,
                required,
                Some("Protected ledger snapshot unavailable"),
            )
        };
        let report = if ledger_ok {
            match Snapshot::capture_unvalidated(&root) {
                Ok(snapshot) => schema(&snapshot.conn, &expected),
                Err(_) => unavailable(),
            }
        } else {
            unavailable()
        };
        if report["status"] != "current" {
            errors.push(format!(
                "schema_{}: current={} required={}",
                report["status"].as_str().unwrap(),
                report["currentVersion"],
                report["requiredVersion"]
            ));
        }
        result["schema_outdated"] = json!(report["status"] == "pending");
        result["schema"] = report;
    }
    result["ok"] = json!(errors.is_empty());
    result["errors"] = json!(errors);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layout() -> tempfile::TempDir {
        let dir = tempfile::tempdir_in(fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap();
        for path in ["data", "documents/inbox", "config"] {
            fs::create_dir_all(dir.path().join(path)).unwrap();
        }
        dir
    }
    fn current(root: &Path) {
        let mut db = Connection::open(root.join("data/ledger.sqlite")).unwrap();
        let mut control = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic diagnostic").unwrap();
    }
    // Reuse the existing normalized baseline images and canonical stamp builder.
    fn prefix(root: &Path) {
        let db = Connection::open(root.join("data/ledger.sqlite")).unwrap();
        for object in company_baseline::load_images().unwrap().baseline {
            db.execute_batch(&object.sql).unwrap();
        }
        let catalogue: Vec<Migration> =
            serde_json::from_str(include_str!("legacy-schema.json")).unwrap();
        company_baseline::stamp(&db, &catalogue[0]).unwrap();
    }
    fn inventory(root: &Path) -> Vec<(String, Vec<u8>)> {
        fn walk(root: &Path, at: &Path, out: &mut Vec<(String, Vec<u8>)>) {
            let mut paths = fs::read_dir(at)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect::<Vec<_>>();
            paths.sort();
            for path in paths {
                let meta = fs::symlink_metadata(&path).unwrap();
                let name = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                if meta.is_dir() {
                    out.push((name, vec![]));
                    walk(root, &path, out);
                } else if meta.is_file() {
                    out.push((name, fs::read(path).unwrap()));
                } else {
                    out.push((
                        name,
                        fs::read_link(path)
                            .unwrap()
                            .as_os_str()
                            .as_encoded_bytes()
                            .to_vec(),
                    ));
                }
            }
        }
        let mut out = Vec::new();
        walk(root, root, &mut out);
        out
    }
    fn unchanged(root: &Path) -> Value {
        let before = inventory(root);
        let report = inspect_company(root);
        assert_eq!(inventory(root), before);
        assert_eq!(inspect_company(root), report);
        assert_eq!(inventory(root), before);
        report
    }
    #[test]
    fn missing_root_and_ledger_are_truthful() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("absent");
        let report = inspect_company(&root);
        assert_eq!(
            report["missing"],
            json!(["company_root", "data_dir", "ledger", "documents", "config"])
        );
        assert!(report.get("schema").is_none());
        assert_eq!(report["schema_outdated"], false);
        assert!(!root.exists());
        let dir = layout();
        assert_eq!(unchanged(dir.path())["missing"], json!(["ledger"]));
    }
    #[test]
    fn native_current_and_real_pending_prefix_preserve_every_byte() {
        let dir = layout();
        current(dir.path());
        let report = unchanged(dir.path());
        assert_eq!(report["ok"], true, "{report}");
        assert_eq!(report["schema"]["status"], "current");
        let dir = layout();
        prefix(dir.path());
        let report = unchanged(dir.path());
        assert_eq!(report["schema"]["status"], "pending", "{report}");
        assert_eq!(report["schema_outdated"], true);
        let catalogue: Value = serde_json::from_str(include_str!("legacy-schema.json")).unwrap();
        assert_eq!(
            report["schema"]["pending"],
            json!(&catalogue.as_array().unwrap()[1..])
        );
    }
    #[test]
    fn altered_history_unknown_physical_newer_and_corrupt_fail_closed() {
        for (sql, status) in [
            (
                "UPDATE schema_migrations SET checksum='altered' WHERE id=1",
                "corrupt",
            ),
            ("CREATE TABLE invented(secret TEXT)", "corrupt"),
            ("DROP TABLE companies", "corrupt"),
            (
                "INSERT INTO schema_migrations(id,name,checksum,applied_by_version) VALUES(999,'invented','secret','synthetic')",
                "newer",
            ),
            (
                "CREATE TABLE rm_legacy_retirement_v1(secret TEXT)",
                "corrupt",
            ),
        ] {
            let dir = layout();
            prefix(dir.path());
            let db = Connection::open(dir.path().join("data/ledger.sqlite")).unwrap();
            db.execute_batch(sql).unwrap();
            drop(db);
            let report = unchanged(dir.path());
            assert_eq!(report["schema"]["status"], status, "{report}");
            assert_eq!(report["schema"]["pending"], json!([]));
            assert_eq!(report["schema_outdated"], false);
            assert!(!report.to_string().contains("secret"));
        }
        let dir = layout();
        fs::write(
            dir.path().join("data/ledger.sqlite"),
            b"corrupt synthetic bytes",
        )
        .unwrap();
        assert_eq!(unchanged(dir.path())["schema"]["status"], "unavailable");
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_wrong_types_and_sidecars_are_not_followed_or_changed() {
        use std::os::unix::fs::symlink;
        for entry in [
            "data",
            "documents",
            "documents/inbox",
            "config",
            "data/ledger.sqlite",
        ] {
            let dir = layout();
            let other = layout();
            current(other.path());
            let path = dir.path().join(entry);
            if path.is_dir() {
                fs::remove_dir_all(&path).unwrap();
            }
            symlink(other.path().join(entry), &path).unwrap();
            let before = inventory(other.path());
            let report = unchanged(dir.path());
            assert_eq!(report["ok"], false);
            assert_eq!(inventory(other.path()), before);
        }
        let dir = layout();
        current(dir.path());
        symlink(dir.path(), dir.path().join("root-link")).unwrap();
        assert_eq!(
            inspect_company(&dir.path().join("root-link"))["checks"][0]["ok"],
            false
        );
        let dir = layout();
        current(dir.path());
        let other = tempfile::NamedTempFile::new().unwrap();
        symlink(other.path(), dir.path().join("data/ledger.sqlite-wal")).unwrap();
        assert_eq!(unchanged(dir.path())["schema"]["status"], "unavailable");
        let dir = layout();
        fs::create_dir(dir.path().join("data/ledger.sqlite")).unwrap();
        assert_eq!(unchanged(dir.path())["checks"][2]["ok"], false);
    }
}
