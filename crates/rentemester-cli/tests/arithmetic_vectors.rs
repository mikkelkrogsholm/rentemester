//! Explicit synthetic adapter; wire strings and legacy numeric bridges are separate.
use rentemester_engine::{Error, currency::Currency, fx::FxRate, money::Money};
use serde_json::{Value, json};
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str, Error> {
    v[key]
        .as_str()
        .ok_or_else(|| Error::new("invalid_input", "expected decimal string"))
}
fn execute(v: &Value) -> Result<Value, Error> {
    let output = match v["op"].as_str().unwrap() {
        "money" => Money::parse(string(v, "input")?)?.decimal(),
        "multiply" => Money::multiply_decimal(string(v, "left")?, string(v, "right")?)?.decimal(),
        "percent" => Money::parse(string(v, "left")?)?
            .percent(string(v, "right")?)?
            .decimal(),
        "add" => {
            let mut a = Money::parse(string(v, "left")?)?;
            a.add(&Money::parse(string(v, "right")?)?);
            a.decimal()
        }
        "subtract" => Money::parse(string(v, "left")?)?
            .subtract(&Money::parse(string(v, "right")?)?)
            .decimal(),
        "rate" => FxRate::parse(string(v, "input")?)?.decimal(),
        "fx" => {
            Currency::parse(string(v, "currency")?)?;
            FxRate::parse(string(v, "right")?)?
                .apply(&Money::parse(string(v, "left")?)?)
                .decimal()
        }
        "currency" => Currency::parse(string(v, "input")?)?.as_str().to_owned(),
        "money-wire" => serde_json::from_value::<Money>(v["input"].clone())
            .map_err(|_| Error::new("invalid_input", "noncanonical money wire"))?
            .decimal(),
        "rate-wire" => serde_json::from_value::<FxRate>(v["input"].clone())
            .map_err(|_| Error::new("invalid_rate", "noncanonical rate wire"))?
            .decimal(),
        "currency-wire" => serde_json::from_value::<Currency>(v["input"].clone())
            .map_err(|_| Error::new("invalid_currency", "noncanonical currency wire"))?
            .as_str()
            .to_owned(),
        _ => panic!("unknown synthetic operation"),
    };
    Ok(json!(output))
}
#[test]
#[ignore = "synthetic exact arithmetic corpus helper selected by Bun harness"]
fn vectors() {
    let data = std::fs::read_to_string(std::env::var("RM_ARITHMETIC_INPUT").unwrap()).unwrap();
    let mut results = Vec::new();
    for line in data.lines() {
        let v: Value = serde_json::from_str(line).unwrap();
        let actual = execute(&v).unwrap_or_else(|e| json!({"error":e.code}));
        assert_eq!(actual, v["expected"], "{}", v["id"]);
        results.push(actual);
    }
    std::fs::write(
        std::env::var("RM_ARITHMETIC_OUTPUT").unwrap(),
        serde_json::to_vec(&results).unwrap(),
    )
    .unwrap();
}
#[test]
fn canonical_serde_refuses_coercion_and_keeps_scale() {
    let amount: Money = serde_json::from_value(json!("123.45")).unwrap();
    assert_eq!(serde_json::to_value(amount).unwrap(), json!("123.45"));
    let rate: FxRate = serde_json::from_value(json!("7.460380")).unwrap();
    assert_eq!(serde_json::to_value(rate).unwrap(), json!("7.460380"));
    let currency: Currency = serde_json::from_value(json!("DKK")).unwrap();
    assert_eq!(serde_json::to_value(currency).unwrap(), json!("DKK"));
    for input in [
        json!(1),
        Value::Null,
        json!("1.005"),
        json!("1"),
        json!("1e0"),
        json!("-0.00"),
        json!("01.00"),
    ] {
        assert!(serde_json::from_value::<Money>(input).is_err());
    }
    for input in [
        json!(1),
        Value::Null,
        json!("1"),
        json!("1.0000001"),
        json!("0.000000"),
        json!("-1.000000"),
    ] {
        assert!(serde_json::from_value::<FxRate>(input).is_err());
    }
    for input in [
        json!(1),
        Value::Null,
        json!("usd"),
        json!(" USD "),
        json!("123"),
    ] {
        assert!(serde_json::from_value::<Currency>(input).is_err());
    }
}
#[test]
fn computed_values_roundtrip_beyond_raw_decimal_input_budget() {
    let raw = format!("{}e400", "9".repeat(2042));
    let amount = Money::parse(&raw).unwrap();
    let encoded = serde_json::to_value(&amount).unwrap();
    assert!(encoded.as_str().unwrap().len() > 2048);
    assert!(Money::parse(encoded.as_str().unwrap()).is_err());
    assert_eq!(serde_json::from_value::<Money>(encoded).unwrap(), amount);
    let rate = FxRate::parse(&raw).unwrap();
    let encoded = serde_json::to_value(&rate).unwrap();
    assert!(encoded.as_str().unwrap().len() > 2048);
    assert_eq!(serde_json::from_value::<FxRate>(encoded).unwrap(), rate);
    let product = Money::multiply_decimal(&raw, &raw).unwrap();
    assert_eq!(
        serde_json::from_value::<Money>(serde_json::to_value(&product).unwrap()).unwrap(),
        product
    );
    let applied = rate.apply(&amount);
    assert_eq!(
        serde_json::from_value::<Money>(serde_json::to_value(&applied).unwrap()).unwrap(),
        applied
    );
    let mut total = Money::parse(&"9".repeat(2048)).unwrap();
    total.add(&Money::parse("0.01").unwrap());
    assert_eq!(
        serde_json::from_value::<Money>(serde_json::to_value(&total).unwrap()).unwrap(),
        total
    );
}
