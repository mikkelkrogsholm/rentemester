use super::*;
const XML: &str = r#"<Root><DokumentIndhold><Paragraf localId="1"><Stk><Linea><Char>Text &amp; evidence.</Char></Linea></Stk></Paragraf><Paragraf localId="2"><Stk><Linea><Char>Other text.</Char></Linea></Stk></Paragraf></DokumentIndhold></Root>"#;
fn run(xml: &str, scope: &str, rule: &str) -> Result<Value, Error> {
    let index = json!([{"id":"DK-A","bytes":xml.len(),"sha256":sha(xml)}]).to_string();
    compute(
        r#"[{"id":"DK-A"}]"#,
        &index,
        scope,
        &[("DK-A", xml)],
        &[("test", rule)],
        r#"{"files":{}}"#,
    )
}
fn rule(source: &str, r: &str, hash: &str) -> String {
    format!(
        "version: test-v1\nrules:\n  - rule_id: DK-TEST-001\n    name: Test\n    source_id: {source}\n    provisions:\n      - ref: \"{r}\"\n        text_hash: \"{hash}\"\n    severity: hard_stop\n"
    )
}
const SCOPE: &str = "version: scope-v1\nsources:\n  DK-A:\n    in_scope: all\n";
#[test]
fn valid_corpus_hash_path_text_and_determinism() {
    let r = rule(
        "DK-A",
        "§ 1, stk. 1",
        &format!("sha256:{}", sha("Text & evidence.")),
    );
    let a = run(XML, SCOPE, &r).unwrap();
    assert_eq!(a["ok"], true);
    assert_eq!(a["overall"]["citedCount"], 1);
    assert_eq!(a["provisions"]["DK-A"][0]["text"], "Text & evidence.");
    assert_eq!(a["provisions"]["DK-A"][0]["path"], json!(["1", "1"]));
    assert_eq!(a, run(XML, SCOPE, &r).unwrap());
    assert!(render_citations(&a).unwrap().contains("> Text & evidence."));
    assert_eq!(
        a["reverseMap"][0]["rules"][0]["nativeExecutionVerified"],
        false
    );
}
#[test]
fn failures_are_not_omitted() {
    let r = rule("DK-A", "§ 1, stk. 1", "sha256:wrong");
    assert_eq!(
        run(XML, SCOPE, &r).unwrap()["driftErrors"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let unknown = run(XML, SCOPE, &rule("DK-UNKNOWN", "§ 1, stk. 1", "bad")).unwrap();
    assert_eq!(unknown["closureErrors"][0]["reason"], "cross_source");
    assert!(!unknown["sourceErrors"].as_array().unwrap().is_empty());
    let unresolved = run(XML, SCOPE, &rule("DK-A", "§ 999, stk. 1 junk", "bad")).unwrap();
    assert_eq!(unresolved["closureErrors"][0]["reason"], "unresolved");
    assert_eq!(unresolved["ok"], false);
    assert!(run("<malformed>", SCOPE, &r).is_err());
    assert!(run(XML, SCOPE, &r.replace("version: test-v1\n", "")).is_err());
    assert!(render_coverage(&json!({"format":"unknown"})).is_err());
}
#[test]
fn malformed_rules_fail_loudly() {
    let base = rule("DK-A", "§ 1, stk. 1", "hash");
    for bad in [
        base.replace("        text_hash: \"hash\"\n", ""),
        base.replace("- ref:", "- text_hash:"),
        base.replace("    severity:", "\tseverity:"),
        base.replace(
            "        text_hash: \"hash\"",
            "        text_hash: \"hash\"\n        text_hash: duplicate",
        ),
        base.replace("    name: Test", "    name: |\n      literal"),
        base.replace(
            "    name: Test",
            "    name: plain\n      unsupported continuation",
        ),
    ] {
        assert!(parsers::parse_rules(&bad, "test").is_err(), "{bad}");
    }
    let folded = base.replace("    name: Test", "    name: >-\n      A folded\n      name");
    assert_eq!(
        parsers::parse_rules(&folded, "test").unwrap().1[0]["name"],
        "A folded name"
    );
}
#[test]
fn scope_variants() {
    let r = rule("DK-A", "§ 2, stk. 1", "bad");
    let scope = "version: v1\nsources:\n  DK-A:\n    in_scope:\n      - \"§ 1\"\n";
    assert_eq!(
        run(XML, scope, &r).unwrap()["scopeErrors"][0]["kind"],
        "citation_out_of_scope"
    );
    let bad = scope.replace("§ 1", "§ 1-§ 9");
    assert!(
        run(XML, &bad, &r).unwrap()["scopeErrors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "bad_endpoint")
    );
    let missing = SCOPE.replace("DK-A:", "DK-B:");
    assert!(
        run(XML, &missing, &r).unwrap()["scopeErrors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "missing_source")
    );
    for bad in [
        scope.replace("version: v1\n", ""),
        scope.replace("§ 1", "§ 9-§ 3"),
        scope.replace("§ 1", "1-9"),
        scope.replace("    in_scope:", ""),
        scope.replace("  DK-A:", "\tDK-A:"),
    ] {
        assert!(parsers::parse_scope(&bad).is_err(), "{bad}");
    }
}
#[test]
fn bundled_valid_corpus_and_actual_errors_remain_visible() {
    let result = bundled_coverage().unwrap();
    eprintln!(
        "native corpus overall={} closure={} drift={} scope={} sourceErrors={}",
        result["overall"],
        result["closureErrors"].as_array().unwrap().len(),
        result["driftErrors"].as_array().unwrap().len(),
        result["scopeErrors"].as_array().unwrap().len(),
        result["sourceErrors"]
    );
    assert_eq!(
        result["overall"],
        json!({"operativeCount":1762,"citedCount":64,"inScopeOperativeCount":498,"inScopeCitedCount":64})
    );
    assert_eq!(result["closureErrors"], json!([]));
    assert_eq!(result["driftErrors"], json!([]));
    assert_eq!(result["scopeErrors"], json!([]));
    assert_eq!(result["sourceErrors"], json!([]));
    assert_eq!(result["ok"], true);
    assert_eq!(result["errorCount"], 0);
    assert_eq!(result["uncitedRules"].as_array().unwrap().len(), 10);
    assert!(result["uncitedRules"].as_array().unwrap().contains(&json!("DK-VAT-OSS-001")));
    assert_eq!(result["sources"].as_array().unwrap().len(), 9);
    assert_eq!(result["bundleVersions"].as_object().unwrap().len(), 12);
    let bad_index = assets::INDEX.replace(
        "182f07c898580e9c57da7dff8cabbc9c714dea1886b4d2497d5e7b45a7f9327a",
        "0000000000000000000000000000000000000000000000000000000000000000",
    );
    let damaged = compute(
        assets::CATALOG,
        &bad_index,
        assets::SCOPE,
        assets::XML,
        assets::RULES,
        assets::REFERENCES,
    )
    .unwrap();
    assert!(
        damaged["sourceErrors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "source_integrity")
    );
    assert_eq!(damaged["ok"], false);
    assert!(!result["rules"].as_array().unwrap().is_empty());
    assert!(
        render_coverage(&result)
            .unwrap()
            .contains("self-attestation")
    );
}

#[test]
fn deliberately_uncited_rule_is_not_an_unknown_source_but_declared_sources_validate() {
    let plain = "version: test-v1\nrules:\n  - rule_id: DK-UNCITED-001\n    name: No claimed citation\n    severity: hard_stop\n";
    let valid = run(XML, SCOPE, plain).unwrap();
    assert_eq!(valid["ok"], true);
    assert_eq!(valid["uncitedRules"], json!(["DK-UNCITED-001"]));
    let declared = plain.replace("    severity:", "    source_id: DK-UNKNOWN\n    severity:");
    let invalid = run(XML, SCOPE, &declared).unwrap();
    assert_eq!(invalid["ok"], false);
    assert_eq!(invalid["sourceErrors"][0]["kind"], "unknown_rule_source");
}
