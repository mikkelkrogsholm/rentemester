use super::*;
pub(crate) fn snapshot() -> Snapshot {
    Snapshot::parse(
        json!({"ok":true,"user":{"id":"synthetic-owner"}}),
        json!({"ok":true,"companies":[{"slug":"first","name":"Synthetic original"}]}),
    )
    .unwrap()
}
pub(crate) fn review() -> Review {
    let values = [
        ("name", "Synthetic new"),
        ("fiscalYearStartMonth", "7"),
        ("fiscalYearLabelStrategy", "span"),
        ("vatPeriodType", "none"),
        ("cvr", "12345678"),
    ]
    .map(|(k, v)| (k.into(), v.into()));
    Review::new(snapshot(), body(&values).unwrap()).unwrap()
}
pub(crate) fn receipt() -> Value {
    json!({"ok":true,"company":{"slug":"synthetic-new","name":"Synthetic new"}})
}
pub(crate) fn profile() -> Value {
    json!({"ok":true,"company":{"id":1,"name":"Synthetic new","fiscalYearStartMonth":7,"fiscalYearLabelStrategy":"span","vatPeriodType":null,"cvr":"DK12345678","payment":null}})
}
pub(crate) fn catalog() -> Value {
    json!({"ok":true,"companies":[{"slug":"first","name":"Synthetic original"},{"slug":"synthetic-new","name":"Synthetic new"}]})
}
#[test]
fn explicit_wire_minimal201_and_canonical_reads_required() {
    let r = review();
    let wire: Value = serde_json::from_slice(&r.bytes).unwrap();
    assert_eq!(wire["fiscalYearStartMonth"], "7");
    assert!(wire["vatPeriodType"].is_null());
    for k in ["confirm", "idempotencyKey", "company", "payment"] {
        assert!(wire.get(k).is_none());
    }
    assert_eq!(r.receipt(&receipt()).unwrap(), "synthetic-new");
    assert_eq!(
        r.canonical(&receipt(), &catalog(), &profile()).unwrap(),
        "synthetic-new"
    );
    assert!(
        r.receipt(&json!({"ok":true,"companies":catalog()["companies"]}))
            .is_err()
    );
    assert!(
        r.canonical(&receipt(), &r.before.catalog, &profile())
            .is_err()
    );
    let mut bad = profile();
    bad["company"]["fiscalYearStartMonth"] = json!(1);
    assert!(r.canonical(&receipt(), &catalog(), &bad).is_err());
    let mut now = r.before.clone();
    now.principal = "other".into();
    assert!(!r.current(&now));
    let mut now = r.before.clone();
    now.catalog = catalog();
    assert!(!r.current(&now));
}
#[test]
fn form_choices_are_required_no_actor_key_or_hidden_defaults() {
    assert!(body(&[("name".into(), "Synthetic".into())]).is_err());
    let r = review();
    let mut v: Value = serde_json::from_slice(&r.bytes).unwrap();
    v["confirm"] = json!(true);
    assert!(Review::new(snapshot(), serde_json::to_vec(&v).unwrap()).is_err());
    for field in ["actor", "idempotencyKey", "company"] {
        let mut v: Value = serde_json::from_slice(&r.bytes).unwrap();
        v[field] = json!("invented");
        assert!(validate_body(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    let values = [
        ("name", "Synthetic"),
        ("fiscalYearStartMonth", "1"),
        ("fiscalYearLabelStrategy", "end-year"),
        ("vatPeriodType", "quarter"),
        ("bankName", "Own Bank"),
        ("accountNo", "000001"),
    ]
    .map(|(k, v)| (k.into(), v.into()));
    let wire: Value = serde_json::from_slice(&body(&values).unwrap()).unwrap();
    assert_eq!(
        wire["payment"],
        json!({"bankName":"Own Bank","accountNo":"000001"})
    );
}
