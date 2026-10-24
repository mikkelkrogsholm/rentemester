//! Actual binary, actual embedded corpus; no runtime/server/credential fixture is involved.
use rentemester_engine::regulatory;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::{Command, Output},
};
fn cli(args: &[&str], dir: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rentemester-rust"))
        .args(args)
        .current_dir(dir)
        .env("RENTEMESTER_COMPANY", "/nonexistent/no-company")
        .env("RENTEMESTER_WORKSPACE", "/nonexistent/no-workspace")
        .output()
        .unwrap()
}
fn fixture() -> PathBuf {
    let p = std::env::temp_dir().join(format!("original-reg716-{}", std::process::id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn json_output(o: &Output) -> Value {
    assert!(
        o.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}
#[test]
fn original_reg716_current_corpus_streams_files_and_passive_refusals() {
    let dir = fixture();
    let coverage = regulatory::bundled_coverage().unwrap();
    assert_eq!(coverage["sourceErrors"], json!([]));
    let mut expected = json!({"ok":true,"closureErrors":0,"driftErrors":0,"scopeErrors":0,"uncitedRules":coverage["uncitedRules"].as_array().unwrap().len()});
    for (a, b) in [
        ("inScopeOperativeProvisions", "inScopeOperativeCount"),
        ("inScopeCitedProvisions", "inScopeCitedCount"),
        ("operativeProvisions", "operativeCount"),
        ("citedProvisions", "citedCount"),
    ] {
        expected[a] = coverage["overall"][b].clone();
    }
    expected["perSource"] = coverage["perSource"].as_array().unwrap().iter().map(|s|json!({"sourceId":s["sourceId"],"inScopeOperativeProvisions":s["inScopeOperativeCount"],"inScopeCitedProvisions":s["inScopeCitedCount"],"operativeProvisions":s["operativeCount"],"citedProvisions":s["citedCount"]})).collect();
    for args in [
        vec!["reg", "coverage"],
        vec!["reg", "coverage", "--json"],
        vec!["--format=json", "reg", "coverage"],
        vec!["reg", "--json", "coverage", "--format=human"],
        vec!["reg", "coverage", "--format=human", "--format=json"],
    ] {
        let o = cli(&args, &dir);
        assert_eq!(o.status.code(), Some(0));
        assert_eq!(json_output(&o), expected);
    }
    let mut human = format!(
        "✔ reg coverage\n  In-scope provisions cited: {}/{}\n  Corpus-wide (incl. out of scope): {}/{}\n  Closure errors: 0\n  Drift errors: 0\n  Scope errors: 0\n  Uncited rules: {}\n",
        expected["inScopeCitedProvisions"],
        expected["inScopeOperativeProvisions"],
        expected["citedProvisions"],
        expected["operativeProvisions"],
        expected["uncitedRules"]
    );
    let per_source = expected["perSource"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            format!(
                "    {}: {}/{}\n",
                s["sourceId"].as_str().unwrap(),
                s["inScopeCitedProvisions"],
                s["inScopeOperativeProvisions"]
            )
        })
        .collect::<String>();
    human.push_str("  Per source (in-scope cited / in-scope operative):\n");
    human.push_str(&per_source);
    let o = cli(&["reg", "coverage", "--format=human"], &dir);
    assert_eq!(o.status.code(), Some(0));
    assert!(o.stderr.is_empty());
    assert_eq!(o.stdout, human.as_bytes());
    let review = regulatory::render_citations(&coverage).unwrap();
    for command in ["coverage", "citations"] {
        let report = if command == "coverage" {
            regulatory::render_coverage(&coverage).unwrap()
        } else {
            review.clone()
        };
        for format in ["json", "human"] {
            for attached in [false, true] {
                let path = format!("{command}-{format}-{attached}.md");
                let out = format!("--out={path}");
                let mut args = vec!["reg", command, "--format", format];
                if attached {
                    args.push(&out);
                } else {
                    args.extend(["--out", &path]);
                }
                let o = cli(&args, &dir);
                assert_eq!(o.status.code(), Some(0));
                assert!(o.stderr.is_empty());
                assert_eq!(std::fs::read(dir.join(&path)).unwrap(), report.as_bytes());
                if format == "json" {
                    let mut v = if command == "coverage" {
                        expected.clone()
                    } else {
                        json!({"ok":true})
                    };
                    v["reportPath"] = json!(path);
                    assert_eq!(json_output(&o), v);
                } else if command == "citations" {
                    assert_eq!(
                        o.stdout,
                        format!("✔ reg citations\n  Review: {path}\n").as_bytes()
                    );
                } else {
                    assert_eq!(
                        o.stdout,
                        human
                            .replace("  Per source", &format!("  Report: {path}\n  Per source"))
                            .as_bytes()
                    );
                }
            }
            let o = cli(
                &[
                    "reg",
                    command,
                    "--format",
                    format,
                    "--out",
                    "missing/report.md",
                ],
                &dir,
            );
            assert_eq!(o.status.code(), Some(1));
            assert!(o.stdout.is_empty());
            assert!(
                String::from_utf8_lossy(&o.stderr).contains("could not write regulatory report")
            );
            assert!(!dir.join("missing").exists());
        }
        for args in [
            vec!["reg", command, "--out", "never.md", "--help"],
            vec!["reg", command, "--help", "--out=never.md"],
        ] {
            let o = cli(&args, &dir);
            assert_eq!(o.status.code(), Some(0));
            assert_eq!(json_output(&o)["offline"], true);
            assert!(!dir.join("never.md").exists());
        }
        // Source Map semantics: last duplicate --out wins, including attached flags.
        let o = cli(&["reg", command, "--out=never.md", "--out=last.md"], &dir);
        assert_eq!(o.status.code(), Some(0));
        assert!(!dir.join("never.md").exists());
        assert_eq!(
            std::fs::read(dir.join("last.md")).unwrap(),
            report.as_bytes()
        );
        for bad in [
            vec!["--unknown=x"],
            vec!["--company=x"],
            vec!["--out"],
            vec!["--out", "--json"],
            vec!["--format=bad"],
            vec!["--json=false"],
            vec!["unexpected"],
            vec!["--", "--json"],
        ] {
            let mut args = vec!["reg", command, "--out=never.md"];
            args.extend(bad);
            let o = cli(&args, &dir);
            assert_eq!(o.status.code(), Some(2), "{args:?}");
            assert!(o.stdout.is_empty());
            assert!(!o.stderr.is_empty());
            assert!(!dir.join("never.md").exists());
        }
    }
    let o = cli(&["reg", "citations", "--json"], &dir);
    assert_eq!(json_output(&o), json!({"ok":true,"review":review}));
    let o = cli(&["reg", "citations", "--format=human"], &dir);
    assert_eq!(o.stdout, format!("{review}\n").as_bytes());
    assert!(o.stderr.is_empty());
    assert_eq!(o.status.code(), Some(0));
    for args in [vec!["reg", "unknown"], vec!["reg"]] {
        assert_eq!(cli(&args, &dir).status.code(), Some(2));
    }
    assert_eq!(cli(&["reg", "--help"], &dir).status.code(), Some(0));
    // Empty/whitespace --out is absent, as source trimToNull specifies.
    let o = cli(&["reg", "coverage", "--out=  "], &dir);
    assert_eq!(json_output(&o), expected);
    std::fs::remove_dir_all(dir).unwrap();
}
