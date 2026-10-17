//! Synthetic adapter only; no product command or posting authority.
use rentemester_engine::{
    Error,
    identifiers::{AccountNo, DocumentId},
    journal_rules::{ManualJournalRules, RuleBundle},
    manual_journal::{self, JournalFacts, ManualJournal},
};
use serde_json::{Value, json};
macro_rules! decode {
    ($v:expr) => {
        serde_json::from_value($v)
            .map_err(|_| Error::new("invalid_input", "strict synthetic input refused"))
    };
}
fn execute(v: &Value, bundles: &Value, version: &str) -> Result<Value, Error> {
    match v["op"].as_str().unwrap() {
        "identifier" => Ok(json!(
            DocumentId::parse(v["input"].as_str().unwrap())?.to_string()
        )),
        "account" => Ok(json!(
            AccountNo::parse(v["input"].as_str().unwrap())?.as_str()
        )),
        "manual" | "rules" => {
            let bundles: Vec<RuleBundle> = decode!(v.get("bundles").unwrap_or(bundles).clone())?;
            let rules = ManualJournalRules::from_bundles(bundles)?;
            let version = v["expectedVersion"].as_str().unwrap_or(version);
            if v["op"] == "rules" {
                rules.require_version(version)?;
                return Ok(json!(rules.version()));
            }
            let request: ManualJournal = decode!(v["request"].clone())?;
            let facts: JournalFacts = decode!(v["facts"].clone())?;
            Ok(
                serde_json::to_value(manual_journal::validate(&request, &facts, &rules, version)?)
                    .unwrap(),
            )
        }
        _ => panic!("unknown synthetic operation"),
    }
}
fn projection(mut v: Value) -> Value {
    if let Some(errors) = v.get_mut("errors").and_then(Value::as_array_mut) {
        for e in errors {
            e.as_object_mut().unwrap().remove("message");
        }
    }
    v
}
#[test]
#[ignore = "synthetic pure manual journal corpus selected by Bun harness"]
fn vectors() {
    let input = std::fs::read_to_string(std::env::var("RM_MANUAL_INPUT").unwrap()).unwrap();
    let bundles: Value = serde_json::from_str(
        &std::fs::read_to_string(std::env::var("RM_MANUAL_BUNDLES").unwrap()).unwrap(),
    )
    .unwrap();
    let mut sorted = bundles.as_array().unwrap().clone();
    sorted.sort_by_key(|b| b["name"].as_str().unwrap().to_owned());
    let version = sorted
        .iter()
        .map(|b| {
            format!(
                "{}={}",
                b["name"].as_str().unwrap(),
                b["version"].as_str().unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    let mut output = Vec::new();
    for line in input.lines() {
        let v: Value = serde_json::from_str(line).unwrap();
        let actual = execute(&v, &bundles, &version).unwrap_or_else(|e| json!({"error":e.code}));
        assert_eq!(projection(actual.clone()), v["expected"], "{}", v["id"]);
        if let Some(messages) = v["expectedMessages"].as_array() {
            for message in messages {
                assert!(
                    actual["errors"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|e| e["message"] == *message),
                    "{} expected blocker message",
                    v["id"]
                );
            }
        }
        output.push(actual);
    }
    std::fs::write(
        std::env::var("RM_MANUAL_OUTPUT").unwrap(),
        serde_json::to_vec(&output).unwrap(),
    )
    .unwrap();
}
#[test]
fn row_identity_wire_is_exact_and_nominal() {
    for input in [
        json!(1),
        json!(1.5),
        json!(null),
        json!("01"),
        json!("1e0"),
        json!("9223372036854775808"),
    ] {
        assert!(serde_json::from_value::<DocumentId>(input).is_err());
    }
    for s in [
        "0",
        "-1",
        "9007199254740993",
        "-9223372036854775808",
        "9223372036854775807",
    ] {
        let id: DocumentId = serde_json::from_value(json!(s)).unwrap();
        assert_eq!(serde_json::to_value(id).unwrap(), json!(s));
    }
}
