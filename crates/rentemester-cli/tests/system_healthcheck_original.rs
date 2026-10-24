//! Compiled offline journey. Synthetic schema comes from existing embedded images.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir()).unwrap().join(format!(
            "rm-healthcheck716-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        for path in ["data", "documents/inbox", "config"] {
            fs::create_dir_all(root.join(path)).unwrap();
        }
        Self(root)
    }
    fn company(&self) -> &str {
        self.0.to_str().unwrap()
    }
    fn build(&self, current: bool) {
        let images: Value = serde_json::from_str(include_str!(
            "../../rentemester-storage/src/legacy-company-schemas.json"
        ))
        .unwrap();
        let catalogue: Value = serde_json::from_str(include_str!(
            "../../rentemester-storage/src/legacy-schema.json"
        ))
        .unwrap();
        let mut sql = String::from("PRAGMA foreign_keys=ON;BEGIN;\n");
        let mut objects = images[if current { "current" } else { "baseline" }]
            .as_array()
            .unwrap()
            .iter()
            .collect::<Vec<_>>();
        objects.sort_by_key(|o| match o["kind"].as_str().unwrap() {
            "table" => 0,
            "index" => 1,
            "view" => 2,
            _ => 3,
        });
        for object in objects {
            sql.push_str(object["sql"].as_str().unwrap());
            sql.push_str(";\n");
        }
        for m in catalogue.as_array().unwrap().iter().take(if current {
            catalogue.as_array().unwrap().len()
        } else {
            1
        }) {
            sql.push_str(&format!("INSERT INTO schema_migrations(id,name,checksum,applied_by_version) VALUES({},'{}','{}','synthetic');\n",m["id"],m["name"].as_str().unwrap().replace('\'',"''"),m["checksum"].as_str().unwrap()));
        }
        sql.push_str("COMMIT;\n");
        let mut child = Command::new("sqlite3")
            .arg(self.0.join("data/ledger.sqlite"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("existing SQLite fixture builder");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(sql.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn call(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .env("RENTEMESTER_COMPANY", "/must-not-use-environment")
        .env("RENTEMESTER_WORKSPACE", "/must-not-use-environment")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}
fn inventory(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut result = vec![];
    let mut entries = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if fs::symlink_metadata(&path).unwrap().is_dir() {
            result.push((path.clone(), vec![]));
            result.extend(inventory(&path));
        } else {
            result.push((path.clone(), fs::read(&path).unwrap()));
        }
    }
    result
}
#[test]
fn original_human_and_explicit_json_have_exact_streams_and_no_source_writes() {
    for current in [true, false] {
        let fixture = Fixture::new();
        fixture.build(current);
        let before = inventory(&fixture.0);
        let required: Value = serde_json::from_str(include_str!(
            "../../rentemester-storage/src/legacy-schema.json"
        ))
        .unwrap();
        let version = required.as_array().unwrap().last().unwrap()["id"]
            .as_i64()
            .unwrap();
        for args in [
            vec![
                "system",
                "healthcheck",
                "--company",
                fixture.company(),
                "--json",
            ],
            vec![
                "--format=json",
                "--company",
                fixture.company(),
                "system",
                "healthcheck",
            ],
            vec![
                "system",
                "--company",
                fixture.company(),
                "healthcheck",
                "--format",
                "json",
            ],
        ] {
            let out = call(&args);
            assert_eq!(
                out.status.code(),
                Some(if current { 0 } else { 1 }),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(out.stderr.is_empty());
            let value: Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(value["ok"], current, "{value}");
            assert_eq!(
                value["schema"]["status"],
                if current { "current" } else { "pending" }
            );
            assert_eq!(value["schema"]["requiredVersion"], version);
            assert_eq!(value["schema_outdated"], !current);
            if !current {
                assert_eq!(
                    value["schema"]["pending"],
                    json!(&required.as_array().unwrap()[1..])
                );
            }
            assert!(String::from_utf8(out.stdout).unwrap().starts_with("{\n  "));
        }
        for flags in [vec![], vec!["--format=human"]] {
            let mut args = vec!["system", "healthcheck", "--company", fixture.company()];
            args.extend(flags);
            let out = call(&args);
            assert_eq!(out.status.code(), Some(if current { 0 } else { 1 }));
            assert!(out.stderr.is_empty());
            let expected = format!(
                "OK company_root\nOK data_dir\nOK ledger\nOK documents\nOK config\n{} schema_{} current={} required={version}\n",
                if current { "OK" } else { "FAIL" },
                if current { "current" } else { "pending" },
                if current { version } else { 1 }
            );
            assert_eq!(String::from_utf8(out.stdout).unwrap(), expected);
        }
        assert_eq!(inventory(&fixture.0), before);
    }
}
#[test]
fn missing_and_corrupt_are_diagnostics_not_parser_errors() {
    let fixture = Fixture::new();
    let absent = fixture.0.join("absent");
    let path = absent.to_str().unwrap();
    let out = call(&["system", "healthcheck", "--company", path, "--json"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stderr.is_empty());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        report["missing"],
        json!(["company_root", "data_dir", "ledger", "documents", "config"])
    );
    assert!(report.get("schema").is_none());
    assert!(!absent.exists());
    let out = call(&["system", "healthcheck", "--company", fixture.company()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stderr.is_empty());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "OK company_root\nOK data_dir\nFAIL ledger\nOK documents\nOK config\n"
    );
    fs::write(
        fixture.0.join("data/ledger.sqlite"),
        b"synthetic broken ledger",
    )
    .unwrap();
    let before = inventory(&fixture.0);
    let out = call(&[
        "system",
        "healthcheck",
        "--company",
        fixture.company(),
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["schema"]["status"], "unavailable");
    assert_eq!(inventory(&fixture.0), before);
}
#[test]
fn help_globals_and_rejected_authority_inputs_remain_offline() {
    for args in [
        vec!["system", "healthcheck", "--help"],
        vec!["--help", "system", "healthcheck"],
        vec!["--help"],
    ] {
        let out = call(&args);
        assert_eq!(out.status.code(), Some(0));
        assert!(out.stderr.is_empty());
        let help = String::from_utf8(out.stdout).unwrap();
        assert!(help.contains("system healthcheck"));
        assert!(help.contains("aggregate-only"));
        if args == ["--help"] {
            let value: Value = serde_json::from_str(&help).unwrap();
            assert_eq!(
                value["commands"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|v| v["name"] == "system healthcheck")
                    .count(),
                1
            );
        }
    }
    for flags in [
        vec![],
        vec!["--server=https://example.invalid"],
        vec!["--local-profile=/private/secret"],
        vec!["--cookie-file=/private/secret"],
        vec!["--actor=agent:forged"],
        vec!["--repair"],
        vec!["--input=-"],
        vec!["--confirm=yes"],
        vec!["--unknown=x"],
        vec!["--company"],
        vec!["--company="],
        vec!["--format=csv"],
        vec!["--json=false"],
        vec!["--json", "--json"],
        vec!["--format=json", "--format=human"],
        vec!["--company=x", "--company=y"],
    ] {
        let mut args = vec!["system", "healthcheck"];
        args.extend(flags);
        let out = call(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&out.stderr).contains("secret"));
    }
}

#[cfg(unix)]
#[test]
fn intermediate_company_symlink_is_refused_before_target_facts() {
    let outer = Fixture::new();
    let target = Fixture::new();
    target.build(true);
    let before = inventory(&target.0);
    let link = outer.0.join("linked-parent");
    std::os::unix::fs::symlink(target.0.parent().unwrap(), &link).unwrap();
    let selected = link.join(target.0.file_name().unwrap());
    let out = call(&["system", "healthcheck", "--company", selected.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stderr.is_empty());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["missing"], json!(["company_root","data_dir","ledger","documents","config"]));
    assert!(report.get("schema").is_none());
    assert_eq!(inventory(&target.0), before);
    fs::remove_file(link).unwrap();
}
