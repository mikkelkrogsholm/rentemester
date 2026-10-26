use super::*;
#[test]
fn current_main_source_transition_snapshots_match() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("main-sources.json")).unwrap();
    assert_eq!(cases.len(), 9);
    for c in cases {
        let prior: Option<Task> = serde_json::from_value(c["prior"].clone()).unwrap();
        let result = synchronize(
            prior.as_ref(),
            &c["input"],
            c["expectedVersion"].as_u64(),
            c["now"].as_str().unwrap(),
            &[],
        );
        if let Some(error) = c["expected"]["error"].as_str() {
            assert_eq!(result.unwrap_err().code, error, "{}", c["mode"]);
        } else {
            assert_eq!(
                json!(result.unwrap().0),
                c["expected"]["task"],
                "{}",
                c["mode"]
            );
        }
    }
}
