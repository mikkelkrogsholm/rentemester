//! Synthetic development fixture adapter, not a product command or domain dependency.
use rentemester_engine::{
    Error,
    fiscal_year::{self, Date, LabelStrategy, StartMonth},
};
use serde_json::{Value, json};
fn execute(v: &Value) -> Result<Value, Error> {
    let date = Date::parse(v["date"].as_str().unwrap())?;
    match v["op"].as_str().unwrap() {
        "date" => Ok(serde_json::to_value(date).unwrap()),
        "deadline" => Ok(serde_json::to_value(fiscal_year::annual_report_deadline(date)?).unwrap()),
        "fiscal" => {
            let start = StartMonth::new(v["startMonth"].as_i64().unwrap().try_into().unwrap())?;
            let strategy = LabelStrategy::try_from(v["strategy"].as_str().unwrap())?;
            Ok(serde_json::to_value(fiscal_year::for_date(date, start, strategy)?).unwrap())
        }
        _ => panic!("unknown synthetic operation"),
    }
}
#[test]
#[ignore = "synthetic cross-language fixture helper selected by Bun harness"]
fn vectors() {
    let input = std::fs::read_to_string(std::env::var("RM_FISCAL_INPUT").unwrap()).unwrap();
    let mut rows = Vec::new();
    for line in input.lines() {
        let v: Value = serde_json::from_str(line).unwrap();
        let actual = execute(&v).unwrap_or_else(|e| json!({"error":e.code}));
        assert_eq!(actual, v["expected"], "{}", v["id"]);
        rows.push(actual);
    }
    std::fs::write(
        std::env::var("RM_FISCAL_OUTPUT").unwrap(),
        serde_json::to_vec(&rows).unwrap(),
    )
    .unwrap();
}

#[test]
fn serde_values_require_canonical_typed_inputs() {
    let date: Date = serde_json::from_value(json!("0099-02-28")).unwrap();
    assert_eq!(serde_json::to_value(date).unwrap(), json!("0099-02-28"));
    for value in [
        json!(" 2026-01-01"),
        json!("2023-02-29"),
        json!(20260101),
        Value::Null,
    ] {
        assert!(serde_json::from_value::<Date>(value).is_err());
    }
    let label: LabelStrategy = serde_json::from_value(json!("end-year")).unwrap();
    assert_eq!(label, LabelStrategy::EndYear);
    assert_eq!(serde_json::to_value(label).unwrap(), json!("end-year"));
    for value in [json!("END_YEAR"), json!(1), Value::Null] {
        assert!(serde_json::from_value::<LabelStrategy>(value).is_err());
    }
}
