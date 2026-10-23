//! Pure offline citation traceability. Legacy references are attribution, not native coverage.
use crate::Error;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
mod assets;
mod parsers;
mod provisions;
fn invalid(s: &str) -> Error {
    Error::new("regulatory_invalid", s)
}
fn sha(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}
pub fn bundled_coverage() -> Result<Value, Error> {
    compute(
        assets::CATALOG,
        assets::INDEX,
        assets::SCOPE,
        assets::XML,
        assets::RULES,
        assets::REFERENCES,
    )
}
fn compute(
    catalog: &str,
    index: &str,
    scope: &str,
    xml: &[(&str, &str)],
    bundles: &[(&str, &str)],
    refs: &str,
) -> Result<Value, Error> {
    let catalog: Value = serde_json::from_str(catalog).map_err(|_| invalid("malformed catalog"))?;
    let index: Value =
        serde_json::from_str(index).map_err(|_| invalid("malformed source index"))?;
    let references: Value =
        serde_json::from_str(refs).map_err(|_| invalid("malformed pinned references"))?;
    let catalog = catalog
        .as_array()
        .ok_or_else(|| invalid("catalog must be array"))?;
    let mut sources = BTreeMap::new();
    let mut source_errors = vec![];
    for entry in index
        .as_array()
        .ok_or_else(|| invalid("index must be array"))?
    {
        let id = s(entry, "id");
        if id.is_empty() || sources.contains_key(id) {
            return Err(invalid("missing or duplicate source id"));
        }
        let bytes = xml
            .iter()
            .find(|(i, _)| *i == id)
            .ok_or_else(|| invalid("indexed XML missing"))?
            .1;
        if !catalog.iter().any(|c| s(c, "id") == id) {
            source_errors.push(json!({"kind":"unknown_source","sourceId":id}));
        }
        if s(entry, "sha256") != sha(bytes) || entry["bytes"].as_u64() != Some(bytes.len() as u64) {
            source_errors.push(json!({"kind":"source_integrity","sourceId":id,"actualSha256":sha(bytes),"actualBytes":bytes.len()}));
        }
        sources.insert(id.to_owned(), provisions::extract(bytes, id)?);
    }
    let (scope_version, scope) = parsers::parse_scope(scope)?;
    let mut rules = vec![];
    let mut versions = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for (bundle, text) in bundles {
        let (v, r) = parsers::parse_rules(text, bundle)?;
        versions.insert(*bundle, v);
        for rule in r {
            if !ids.insert(s(&rule, "ruleId").to_owned()) {
                return Err(invalid("duplicate rule id"));
            }
            rules.push(rule);
        }
    }
    rules.sort_by_cached_key(|r| s(r, "ruleId").to_owned());
    let (mut closure, mut drift, mut scope_errors, mut uncited) = (vec![], vec![], vec![], vec![]);
    let mut cited: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for (id, ps) in &sources {
        if !scope.contains_key(id) {
            scope_errors.push(json!({"kind":"missing_source","sourceId":id}));
        }
        if let Some(Some(ranges)) = scope.get(id) {
            let existing = ps
                .iter()
                .map(|p| p["path"][0].as_str().unwrap())
                .collect::<BTreeSet<_>>();
            for (a, b) in ranges {
                for endpoint in BTreeSet::from([a, b]) {
                    if !existing.contains(endpoint.as_str()) {
                        scope_errors
                            .push(json!({"kind":"bad_endpoint","sourceId":id,"paragraf":endpoint}));
                    }
                }
            }
        }
    }
    for rule in &rules {
        let id = s(rule, "ruleId");
        let source = s(rule, "sourceId");
        let citations = rule["provisions"].as_array().unwrap();
        if (!source.is_empty() || !citations.is_empty())
            && !catalog.iter().any(|c| s(c, "id") == source)
        {
            source_errors.push(json!({"kind":"unknown_rule_source","ruleId":id,"sourceId":source}));
        }
        if citations.is_empty() {
            uncited.push(json!(id));
        }
        for c in citations {
            let r = s(c, "ref");
            let resolved = sources.get(source).and_then(|ps| provisions::find(ps, r));
            let Some(p) = resolved else {
                let other = sources
                    .iter()
                    .find(|(other, ps)| {
                        other.as_str() != source && provisions::find(ps, r).is_some()
                    })
                    .map(|(id, _)| id);
                let mut e = json!({"ruleId":id,"sourceId":source,"ref":r,"reason":if other.is_some(){"cross_source"}else{"unresolved"}});
                if let Some(o) = other {
                    e["resolvedSourceId"] = json!(o);
                }
                closure.push(e);
                continue;
            };
            if p["textHash"] != c["textHash"] {
                drift.push(json!({"ruleId":id,"sourceId":source,"ref":r,"citedTextHash":c["textHash"],"currentTextHash":p["textHash"]}));
            }
            if s(p, "kind") == "operative"
                && !parsers::in_scope(&scope, source, p["path"][0].as_str().unwrap())
            {
                scope_errors.push(
                    json!({"kind":"citation_out_of_scope","ruleId":id,"sourceId":source,"ref":r}),
                );
            }
            cited
                .entry((source.into(), r.into()))
                .or_default()
                .insert(id.into());
        }
    }
    let (mut per_source, mut uncovered, mut reverse) = (vec![], vec![], vec![]);
    let (mut op, mut ct, mut scoped, mut scoped_ct) = (0, 0, 0, 0);
    for (id, ps) in &sources {
        let (mut covered, mut missing) = (vec![], vec![]);
        let (mut count, mut sc, mut sc_ct) = (0, 0, 0);
        for p in ps.iter().filter(|p| s(p, "kind") == "operative") {
            count += 1;
            let inside = parsers::in_scope(&scope, id, p["path"][0].as_str().unwrap());
            if inside {
                sc += 1;
            }
            let r = s(p, "ref");
            if cited.contains_key(&(id.clone(), r.into())) {
                covered.push(r);
                if inside {
                    sc_ct += 1;
                }
            } else {
                missing.push(r);
                uncovered.push(json!({"sourceId":id,"ref":r}));
            }
        }
        covered.sort();
        missing.sort();
        op += count;
        ct += covered.len();
        scoped += sc;
        scoped_ct += sc_ct;
        per_source.push(json!({"sourceId":id,"operativeCount":count,"citedCount":covered.len(),"inScopeOperativeCount":sc,"inScopeCitedCount":sc_ct,"coveredRefs":covered,"uncoveredRefs":missing}));
    }
    for ((source, r), ids) in cited {
        let entries=ids.iter().map(|id|{let files=references["files"].as_object();let paths=|prefix:&str|files.into_iter().flat_map(|f|f.iter()).filter(|(p,v)|p.starts_with(prefix)&&v["ruleIds"].as_array().is_some_and(|a|a.contains(&json!(id)))).map(|(p,_)|p.clone()).collect::<Vec<_>>();json!({"ruleId":id,"codeFiles":paths("src/"),"testFiles":paths("tests/"),"evidenceKind":"legacy-typescript-reference","nativeExecutionVerified":false})}).collect::<Vec<_>>();
        reverse.push(json!({"sourceId":source,"ref":r,"rules":entries}));
    }
    for errors in [&mut scope_errors, &mut source_errors] {
        errors.sort_by_cached_key(Value::to_string);
        errors.dedup();
    }
    let errors = closure.len() + drift.len() + scope_errors.len() + source_errors.len();
    Ok(
        json!({"format":"rentemester-regulatory-coverage/1","ok":errors==0,"errorCount":errors,"scopeVersion":scope_version,"bundleVersions":versions,"sources":index,"legalSources":catalog,"provisions":sources,"rules":rules,"referenceAttribution":references,"notice":"First-draft scope, subject to owner review. Citation traceability is self-attestation, not legal approval. Legacy TypeScript references do not establish native runtime coverage. GDPR articles are not modeled by the Danish paragraf extractor.","overall":{"operativeCount":op,"citedCount":ct,"inScopeOperativeCount":scoped,"inScopeCitedCount":scoped_ct},"perSource":per_source,"closureErrors":closure,"driftErrors":drift,"scopeErrors":scope_errors,"sourceErrors":source_errors,"uncitedRules":uncited,"uncoveredProvisions":uncovered,"reverseMap":reverse}),
    )
}
fn validate(v: &Value) -> Result<(), Error> {
    if v["format"] != "rentemester-regulatory-coverage/1"
        || !v["rules"].is_array()
        || !v["provisions"].is_object()
        || !v["overall"].is_object()
    {
        return Err(invalid("unsupported coverage format or malformed coverage"));
    }
    for rule in v["rules"].as_array().unwrap() {
        if !rule["ruleId"].is_string()
            || !rule["sourceId"].is_string()
            || !rule["provisions"].is_array()
        {
            return Err(invalid("malformed coverage rule"));
        }
        for citation in rule["provisions"].as_array().unwrap() {
            if !citation["ref"].is_string() || !citation["textHash"].is_string() {
                return Err(invalid("malformed coverage citation"));
            }
        }
    }
    Ok(())
}
pub fn render_coverage(v: &Value) -> Result<String, Error> {
    validate(v)?;
    Ok(format!(
        "# Regulatory coverage\n\n{}\n\nIn-scope operative-provision coverage: {}/{}\n\nCorpus-wide operative coverage: {}/{}\n\n```json\n{}\n```\n",
        s(v, "notice"),
        v["overall"]["inScopeCitedCount"],
        v["overall"]["inScopeOperativeCount"],
        v["overall"]["citedCount"],
        v["overall"]["operativeCount"],
        serde_json::to_string_pretty(v).map_err(|_| invalid("coverage serialization"))?
    ))
}
pub fn render_citations(v: &Value) -> Result<String, Error> {
    validate(v)?;
    let mut out = format!("# Regulatory citation review\n\n{}\n", s(v, "notice"));
    for r in v["rules"].as_array().unwrap() {
        if r["provisions"].as_array().unwrap().is_empty() {
            continue;
        }
        out += &format!(
            "\n## {}\n\n- Source: {}\n- Name: {}\n- Explanation: {}\n",
            s(r, "ruleId"),
            s(r, "sourceId"),
            s(r, "name"),
            s(r, "explanation")
        );
        for c in r["provisions"].as_array().unwrap() {
            out += &format!("\n### `{}`\n\n", s(c, "ref"));
            let ps = v["provisions"][s(r, "sourceId")].as_array();
            if let Some(p) = ps.and_then(|ps| provisions::find(ps, s(c, "ref"))) {
                out += &format!("> {}\n\nText hash: {}\n", s(p, "text"), s(p, "textHash"));
            } else {
                out += "_Does not resolve in the declared source (closure error)._\n";
            }
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests;
