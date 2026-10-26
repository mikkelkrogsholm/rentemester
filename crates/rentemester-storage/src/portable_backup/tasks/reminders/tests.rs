use super::*;
use crate::tasks::{self as writes, Context};
fn fresh() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::control_migration::initialize_empty(&mut db).unwrap();
    db
}
fn ctx(key: &str, version: Option<u64>) -> Context<'_> {
    Context {
        actor: "user:synthetic",
        principal: "synthetic-member",
        idempotency_key: key,
        expected_version: version,
        now: "2026-10-26T11:00:00Z",
        source_check: None,
    }
}
fn populated(frequency: &str) -> Vec<u8> {
    let mut db = crate::portable_backup::tasks::tests::populated();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let r = serde_json::json!({"reminderId":"follow","recipientId":"recipient","enabled":true,"kind":"follow_up","followUpDate":"2026-03-29","frequency":frequency,"timeZone":"Europe/Copenhagen"});
    writes::reminders::configure_in_tx(
        &tx,
        "task-synthetic",
        &r,
        &ctx("configure", Some(1)),
        |_| Ok(()),
        |_, _| Ok(()),
    )
    .unwrap();
    // Explicit retrospective asOfDate is an existing contract; it predates the task event wall clock.
    assert_eq!(
        writes::reminders::deliver(
            &tx,
            "2026-03-28T23:30:00.000Z",
            |_, _| Ok(true),
            |_| Ok("2026-03-29".into())
        )
        .unwrap(),
        1
    );
    assert_eq!(
        writes::reminders::deliver(
            &tx,
            "2026-03-28T23:30:00.000Z",
            |_, _| Ok(true),
            |_| Ok("2026-03-29".into())
        )
        .unwrap(),
        0
    );
    let result = writes::reminders::MaintenanceResult {
        scope: domain::Scope::Workspace {
            company_slugs: vec!["synthetic".into()],
        },
        includes_workspace: false,
        notification_scope: domain::Scope::Workspace {
            company_slugs: vec!["synthetic".into()],
        },
        notifications_include_workspace: false,
        run: writes::reminders::MaintenanceSummary {
            sync: Default::default(),
            materialized: writes::series::Materialized {
                created: 0,
                existing: 0,
                task_ids: vec![],
            },
            delivered: 1,
        },
    };
    writes::append_runtime_receipt(
        &tx,
        &ctx("run", None),
        &serde_json::json!({}),
        &serde_json::json!(result),
    )
    .unwrap();
    tx.commit().unwrap();
    export(&db).unwrap()
}
fn mutate_notification(bytes: &[u8], edit: impl FnOnce(&mut engine::Notification)) -> Vec<u8> {
    let mut b: Business = serde_json::from_slice(bytes).unwrap();
    let t = b.tables.get_mut("rm_task_notifications").unwrap();
    let p = t.columns.iter().position(|c| c == "payload_json").unwrap();
    let Cell::Text(raw) = &t.rows[0][p] else {
        panic!("text")
    };
    let mut n: engine::Notification = serde_json::from_str(raw).unwrap();
    edit(&mut n);
    n.notification_id = format!(
        "notification-{}",
        &domain::payload_hash(&serde_json::json!([
            n.recipient_id,
            n.task_id,
            n.reminder_id,
            n.slot_key
        ]))
        .unwrap()[..32]
    );
    for (column, value) in [
        ("notification_id", n.notification_id.clone()),
        ("slot_key", n.slot_key.clone()),
        ("created_at", n.created_at.clone()),
        (
            "payload_json",
            domain::canonical_json(&serde_json::json!(n)),
        ),
    ] {
        let i = t.columns.iter().position(|c| c == column).unwrap();
        t.rows[0][i] = Cell::Text(value);
    }
    serde_json::to_vec(&b).unwrap()
}
fn refuses(bytes: &[u8]) {
    let mut db = fresh();
    assert!(initialize(&mut db, bytes, &["synthetic".into()]).is_err());
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT count(*) FROM rm_task_events", [], |r| r.get(0))
            .unwrap(),
        0
    );
}
#[test]
fn codec5_reminder_run_slot_roundtrip_without_grants_runtime_and_exact_legacy_coverage() {
    for frequency in ["once", "daily"] {
        let bytes = populated(frequency);
        let b: Business = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(b.format, FORMAT5);
        let mut recovered = fresh();
        initialize(&mut recovered, &bytes, &["synthetic".into()]).unwrap();
        assert_eq!(export(&recovered).unwrap(), bytes);
        assert_eq!(
            recovered
                .query_row::<i64, _, _>("SELECT count(*) FROM user", [], |r| r.get(0))
                .unwrap(),
            0
        );
        assert_eq!(
            recovered
                .query_row::<Option<String>, _, _>(
                    "SELECT lease_owner FROM rm_task_runtime",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
            None
        );
        let mut downgraded: Business = serde_json::from_slice(&bytes).unwrap();
        downgraded.format = FORMAT4.into();
        refuses(&serde_json::to_vec(&downgraded).unwrap());
    }
    let old = crate::portable_backup::tasks::tests::populated();
    let bytes = export(&old).unwrap();
    let b: Business = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(b.format, FORMAT);
}
#[test]
fn codec5_refuses_reidentified_wrong_local_day_and_before_due_slots_atomically() {
    let daily = populated("daily");
    let changed = mutate_notification(&daily, |n| {
        n.slot_key = format!("{}:2026-03-30", n.slot_key.split(':').next().unwrap())
    });
    refuses(&changed);
    let once = populated("once");
    let before_due =
        mutate_notification(&once, |n| n.created_at = "2026-03-27T12:00:00.000Z".into());
    refuses(&before_due);
    let mut b: Business = serde_json::from_slice(&daily).unwrap();
    let t = b.tables.get_mut("rm_task_receipts").unwrap();
    let op = t.columns.iter().position(|s| s == "operation").unwrap();
    let principal = t.columns.iter().position(|s| s == "principal").unwrap();
    for row in &mut t.rows {
        if matches!(&row[op],Cell::Text(s)if s=="reminder-set") {
            row[principal] = Cell::Text("wrong-principal".into());
        }
    }
    refuses(&serde_json::to_vec(&b).unwrap());
}
