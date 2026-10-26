use super::*;
#[test]
fn actual_main_reminder_snapshots_schedule_signatures_and_slots_match() {
    let rows: Vec<Value> = serde_json::from_str(include_str!("main-reminders.json")).unwrap();
    assert_eq!(rows.len(), 8);
    for row in rows {
        let prior: Task = serde_json::from_value(row["prior"].clone()).unwrap();
        let configured = configure(
            &prior,
            &row["reminder"],
            Some(1),
            "2026-01-01T00:00:00.000Z",
            &[],
        );
        if let Some(code) = row["error"].as_str() {
            assert_eq!(configured.unwrap_err().code, code, "{}", row["mode"]);
            continue;
        }
        let task = configured.unwrap();
        assert_eq!(json!(task), row["configured"], "{}", row["mode"]);
        let mut delivered = std::collections::BTreeSet::new();
        let mut records = vec![];
        for p in row["probes"].as_array().unwrap() {
            let mut added = 0;
            if let Some(n) = notification(
                &task,
                &task.reminders[0],
                p["today"].as_str().unwrap(),
                p["now"].as_str().unwrap(),
            )
            .unwrap()
            {
                if delivered.insert(n.notification_id.clone()) {
                    records.push(n);
                    added += 1;
                }
            }
            records.sort_by(|a, b| {
                b.created_at
                    .cmp(&a.created_at)
                    .then_with(|| a.notification_id.cmp(&b.notification_id))
            });
            assert_eq!(
                json!(records),
                p["notifications"],
                "{} {}",
                row["mode"],
                p["now"]
            );
            assert_eq!(json!(added), p["delivered"]);
        }
    }
}
