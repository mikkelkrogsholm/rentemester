use super::*;
const NOW: &str = "2026-10-26T11:00:00.000Z";
fn schema(db: &Connection) {
    // This exact main task-table shape is exercised in a private fixture only.
    // Native schema suffix/adoption and authenticated backup coverage must be integrated
    // before a production application caller can enable these primitives.
    let artifact: Value = serde_json::from_str(include_str!(
        "../../../../src/core/workspace-migrations/0034-tasks-v34.json"
    ))
    .unwrap();
    db.execute_batch(artifact["sql"].as_str().unwrap()).unwrap();
    db.execute_batch("CREATE TABLE synthetic_access(company TEXT PRIMARY KEY,allowed INTEGER NOT NULL); INSERT INTO synthetic_access VALUES('synthetic',1),('other',1);").unwrap();
}
fn db() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    schema(&db);
    db
}
fn input() -> Value {
    json!({"taskId":"task-synthetic","title":"Kontrollér perioden","scope":{"kind":"company","companySlug":"synthetic"}})
}
fn context<'a>(key: &'a str, version: Option<u64>) -> Context<'a> {
    Context {
        actor: "user:synthetic",
        principal: "synthetic-member",
        idempotency_key: key,
        expected_version: version,
        now: NOW,
        source_check: None,
    }
}
fn authorize(tx: &Transaction<'_>, task: &Task) -> Result<(), Error> {
    for company in task.scope.companies() {
        let allowed: Option<i64> = tx
            .query_row(
                "SELECT allowed FROM synthetic_access WHERE company=?1",
                [company],
                |r| r.get(0),
            )
            .optional()
            .map_err(invalid)?;
        if allowed != Some(1) {
            return Err(Error::new("access_denied", "Current access refused"));
        }
    }
    Ok(())
}
fn counts(db: &Connection) -> (i64, i64) {
    (
        db.query_row("SELECT count(*) FROM rm_task_events", [], |r| r.get(0))
            .unwrap(),
        db.query_row("SELECT count(*) FROM rm_task_receipts", [], |r| r.get(0))
            .unwrap(),
    )
}
fn create_task(db: &mut Connection) -> Task {
    mutate(
        db,
        Command::Create {
            input: &input(),
            generated_id: "unused-id",
        },
        context("create", None),
        authorize,
    )
    .unwrap()
    .task
}

#[test]
fn append_only_manual_flow_survives_reopen_and_matches_main_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control.sqlite");
    let mut db = Connection::open(&path).unwrap();
    schema(&db);
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../rentemester-engine/src/tasks/main-flow.json"
    ))
    .unwrap();
    for (index, row) in corpus["steps"].as_array().unwrap().iter().enumerate() {
        let value = &row["input"];
        let command = match row["operation"].as_str().unwrap() {
            "create" => Command::Create {
                input: value,
                generated_id: "unused-id",
            },
            "update" => Command::Update {
                id: "task-synthetic",
                patch: value,
            },
            "move" => Command::Move {
                id: "task-synthetic",
                input: value,
            },
            "complete" => Command::Complete {
                id: "task-synthetic",
                input: value,
            },
            "reopen" => Command::Reopen {
                id: "task-synthetic",
                reason: value,
            },
            _ => unreachable!(),
        };
        let key = format!("step-{index}");
        let mut ctx = context(&key, row["expectedVersion"].as_u64());
        ctx.now = row["now"].as_str().unwrap();
        let out = mutate(&mut db, command, ctx, authorize).unwrap();
        assert!(!out.replayed);
        assert_eq!(json!(out.task), row["expected"]);
    }
    assert_eq!(counts(&db), (5, 5));
    drop(db);
    let db = Connection::open(&path).unwrap();
    assert_eq!(
        get(&db, "task-synthetic", |_| Ok(()))
            .unwrap()
            .unwrap()
            .version,
        5
    );
    let events = history(&db, "task-synthetic", |_| Ok(())).unwrap();
    assert_eq!(events.len(), 5);
    assert_eq!(
        events[4].reason.as_deref(),
        Some("Efterfølgende bilag ændrer vurderingen")
    );
    assert_eq!(list(&db, |_| Ok(true)).unwrap().len(), 1);
    assert!(list(&db, |_| Ok(false)).unwrap().is_empty());
    for sql in [
        "UPDATE rm_task_events SET reason='tampered'",
        "DELETE FROM rm_task_events",
        "UPDATE rm_task_receipts SET operation='tampered'",
        "DELETE FROM rm_task_receipts",
    ] {
        assert!(db.execute(sql, []).is_err());
    }
    assert_eq!(counts(&db), (5, 5));
}
#[test]
fn durable_receipt_replay_is_exact_and_requires_current_authority_first() {
    let mut db = db();
    let original = create_task(&mut db);
    let replay = mutate(
        &mut db,
        Command::Create {
            input: &input(),
            generated_id: "different-generated-id",
        },
        context("create", None),
        authorize,
    )
    .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.task, original);
    assert_eq!(counts(&db), (1, 1));
    let mut changed = input();
    changed["title"] = json!("Et andet payload");
    assert_eq!(
        mutate(
            &mut db,
            Command::Create {
                input: &changed,
                generated_id: "unused-id"
            },
            context("create", None),
            authorize
        )
        .unwrap_err()
        .code,
        "idempotency_conflict"
    );
    db.execute("UPDATE synthetic_access SET allowed=0", [])
        .unwrap();
    assert_eq!(
        mutate(
            &mut db,
            Command::Create {
                input: &input(),
                generated_id: "unused-id"
            },
            context("create", None),
            authorize
        )
        .unwrap_err()
        .code,
        "access_denied"
    );
    assert_eq!(counts(&db), (1, 1));
}
#[test]
fn create_receipt_replay_does_not_reapply_changed_board_placement() {
    let mut db = db();
    let mut v = input();
    v["columnId"] = json!("initial");
    for (version, column) in [(1, "initial"), (2, "renamed")] {
        let board = json!({"boardId":"company:synthetic","version":version,"scope":{"kind":"company","companySlug":"synthetic"},"updatedAt":NOW,"columns":[{"columnId":column,"name":"Open","status":"open","isDefault":true}]});
        db.execute("INSERT INTO rm_task_board_events(board_id,version,payload_json,actor,principal,created_at) VALUES('company:synthetic',?1,?2,'user:synthetic','synthetic-member',?3)",params![version,board.to_string(),NOW]).unwrap();
        if version == 1 {
            mutate(
                &mut db,
                Command::Create {
                    input: &v,
                    generated_id: "unused-id",
                },
                context("create", None),
                authorize,
            )
            .unwrap();
        }
    }
    let result = mutate(
        &mut db,
        Command::Create {
            input: &v,
            generated_id: "unused-id",
        },
        context("create", None),
        authorize,
    )
    .unwrap();
    assert!(result.replayed);
    assert_eq!(result.task.column_id, "initial");
    assert_eq!(counts(&db), (1, 1));
    assert_eq!(
        mutate(
            &mut db,
            Command::Create {
                input: &v,
                generated_id: "unused-id"
            },
            context("different-new-command", None),
            authorize
        )
        .unwrap_err()
        .code,
        "column_invalid"
    );
    assert_eq!(counts(&db), (1, 1));
}
#[test]
fn scope_change_checks_old_new_and_receipt_current_scopes() {
    let mut db = db();
    create_task(&mut db);
    let patch = json!({"scope":{"kind":"company","companySlug":"other"}});
    let mut seen = vec![];
    mutate(
        &mut db,
        Command::Update {
            id: "task-synthetic",
            patch: &patch,
        },
        context("scope", Some(1)),
        |tx, t| {
            seen.push(t.scope.companies()[0].to_owned());
            authorize(tx, t)
        },
    )
    .unwrap();
    assert_eq!(seen, vec!["synthetic", "other"]);
    db.execute(
        "UPDATE synthetic_access SET allowed=0 WHERE company='other'",
        [],
    )
    .unwrap();
    // The original create receipt belongs to the historical scope, but current scope is denied.
    let error = mutate(
        &mut db,
        Command::Create {
            input: &input(),
            generated_id: "unused-id",
        },
        context("create", None),
        authorize,
    )
    .unwrap_err();
    assert_eq!(error.code, "access_denied");
    assert_eq!(counts(&db), (2, 2));
    assert!(
        history(&db, "task-synthetic", |t| {
            if t.scope.companies().contains(&"other") {
                Err(Error::new("access_denied", "denied"))
            } else {
                Ok(())
            }
        })
        .is_err()
    );
}
#[test]
fn stale_or_denied_proposals_leave_no_partial_event_or_receipt() {
    let mut db = db();
    create_task(&mut db);
    let patch = json!({"title":"Ændret"});
    assert_eq!(
        mutate(
            &mut db,
            Command::Update {
                id: "task-synthetic",
                patch: &patch
            },
            context("stale", Some(8)),
            authorize
        )
        .unwrap_err()
        .code,
        "version_conflict"
    );
    let mut calls = 0;
    let error = mutate(
        &mut db,
        Command::Update {
            id: "task-synthetic",
            patch: &patch,
        },
        context("denied-result", Some(1)),
        |_, _| {
            calls += 1;
            if calls == 2 {
                Err(Error::new(
                    "access_denied",
                    "Proposed task recipient not allowed",
                ))
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "access_denied");
    assert_eq!(counts(&db), (1, 1));
    assert_eq!(
        read(&db, "task-synthetic").unwrap().unwrap().title,
        "Kontrollér perioden"
    );
    // A receipt insert failure rolls the preceding event back as well.
    db.execute_batch("CREATE TRIGGER synthetic_receipt_failure BEFORE INSERT ON rm_task_receipts BEGIN SELECT RAISE(ABORT,'synthetic receipt failure'); END;").unwrap();
    assert!(
        mutate(
            &mut db,
            Command::Update {
                id: "task-synthetic",
                patch: &patch
            },
            context("insert-failure", Some(1)),
            authorize
        )
        .is_err()
    );
    assert_eq!(counts(&db), (1, 1));
}
#[test]
fn two_connections_same_command_append_once_under_writer_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("control.sqlite");
    let db = Connection::open(&path).unwrap();
    schema(&db);
    drop(db);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut db = Connection::open(path).unwrap();
                db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
                barrier.wait();
                mutate(
                    &mut db,
                    Command::Create {
                        input: &input(),
                        generated_id: "unused-id",
                    },
                    context("create", None),
                    authorize,
                )
                .unwrap()
                .replayed
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|r| **r).count(), 1);
    let db = Connection::open(path).unwrap();
    assert_eq!(counts(&db), (1, 1));
}
