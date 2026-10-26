use super::*;
#[test]
fn current_main_oracle_matches_preview_hashes_consequences_and_refusals() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("main-boards.json")).unwrap();
    assert_eq!(cases.len(), 9);
    for c in cases {
        let prior: Option<Board> = serde_json::from_value(c["prior"].clone()).unwrap();
        let tasks: Vec<Task> = serde_json::from_value(c["tasks"].clone()).unwrap();
        let boards: Vec<Board> = serde_json::from_value(c["boards"].clone()).unwrap();
        let changes = changes(&c["input"], prior.as_ref(), &tasks).unwrap();
        assert_eq!(
            json!(changes.preview),
            c["preview"],
            "{} preview",
            c["mode"]
        );
        let actual = apply(
            &changes,
            &boards,
            c["expectedVersion"].as_u64(),
            c["now"].as_str().unwrap(),
        );
        if let Some(error) = c["expected"]["error"].as_str() {
            assert_eq!(actual.unwrap_err().code, error, "{}", c["mode"]);
        } else {
            let (board, updated) = actual.unwrap();
            let all: Vec<_> = tasks
                .iter()
                .map(|t| updated.iter().find(|n| n.task_id == t.task_id).unwrap_or(t))
                .collect();
            assert_eq!(
                json!({"board":board,"tasks":all}),
                c["expected"],
                "{}",
                c["mode"]
            );
        }
    }
}
