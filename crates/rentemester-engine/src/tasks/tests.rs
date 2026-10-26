use super::*;
use serde_json::{Value, json};
const NOW: &str = "2026-10-26T11:00:00.000Z";
fn draft_input() -> Value {
    json!({"taskId":"task-synthetic","title":"Kontrollér perioden","scope":{"kind":"company","companySlug":"synthetic"}})
}
fn task() -> Task {
    create(&draft_input(), "unused-id", NOW, &[]).unwrap()
}
fn reference() -> Value {
    json!({"kind":"document","ref":"doc:synthetic:7","companySlug":"synthetic"})
}
fn source_input() -> Value {
    let mut v = draft_input();
    v["source"] = json!({"kind":"vat_filing","identity":"vat:2026:1","companySlug":"synthetic","ref":"period:2026:1","state":"open","contentHash":"a".repeat(64),"href":"/companies/synthetic/vat?runId=42"});
    v
}
fn err<T: std::fmt::Debug>(result: Result<T, Error>, code: &str) {
    assert_eq!(result.unwrap_err().code, code);
}

#[test]
fn documented_manual_task_flow_matches_current_main() {
    let corpus: Value = serde_json::from_str(include_str!("main-flow.json")).unwrap();
    for (source, bytes) in [
        (
            "src/core/tasks.ts",
            include_bytes!("../../../../src/core/tasks.ts").as_slice(),
        ),
        (
            "src/core/tasks-types.ts",
            include_bytes!("../../../../src/core/tasks-types.ts").as_slice(),
        ),
        (
            "src/core/canonical-json.ts",
            include_bytes!("../../../../src/core/canonical-json.ts").as_slice(),
        ),
        (
            "src/core/workspace-migrations/0034-tasks-v34.json",
            include_bytes!("../../../../src/core/workspace-migrations/0034-tasks-v34.json")
                .as_slice(),
        ),
    ] {
        assert_eq!(
            crate::sha256_bytes(bytes),
            corpus["sources"][source].as_str().unwrap(),
            "Changed main source requires regenerating the real source oracle: {source}"
        );
    }
    let mut prior: Option<Task> = None;
    for row in corpus["steps"].as_array().unwrap() {
        let op = row["operation"].as_str().unwrap();
        let input = &row["input"];
        let version = row["expectedVersion"].as_u64();
        let now = row["now"].as_str().unwrap();
        let result = match op {
            "create" => create(input, "unused-id", now, &[]),
            "update" => update(prior.as_ref().unwrap(), input, version, now, &[]),
            "move" => move_task(prior.as_ref().unwrap(), input, version, now, &[]),
            "complete" => complete(
                prior.as_ref().unwrap(),
                input,
                version,
                now,
                "user:synthetic",
                None,
                &[],
            ),
            "reopen" => reopen(prior.as_ref().unwrap(), input, version, now, &[]),
            _ => panic!("Unknown oracle operation"),
        }
        .unwrap();
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            row["expected"],
            "{op}"
        );
        prior = Some(result);
    }
    let last = prior.unwrap();
    assert_eq!(last.version, 5);
    assert!(last.completion.is_none());
    assert_eq!(last.status, "open");
}
#[test]
fn privacy_guards_preserve_product_references_and_utf16_limits() {
    for private in [
        "CPR 010190-1234",
        "Betal DK50 0040 0440 1162 43",
        "konto: 1234 56789012",
    ] {
        let mut v = draft_input();
        v["description"] = json!(private);
        err(create(&v, "unused-id", NOW, &[]), "private_data");
    }
    let mut v = draft_input();
    v["title"] = json!("😀".repeat(121));
    err(create(&v, "unused-id", NOW, &[]), "invalid_input");
    v["title"] = json!("😀".repeat(120));
    assert!(create(&v, "unused-id", NOW, &[]).is_ok());
    for bad in ["\u{7f}", "\u{0}"] {
        v["title"] = json!(bad);
        err(create(&v, "unused-id", NOW, &[]), "invalid_input");
    }
    v["title"] = json!("\u{feff} Godkend \u{feff}");
    assert_eq!(draft(&v).unwrap()["title"], "Godkend");
    for bad in [
        "../private",
        "https://example.test/secret",
        "opaque/../file",
    ] {
        let mut r = reference();
        r["ref"] = json!(bad);
        err(references(&json!([r]), &task().scope), "invalid_input");
    }
}
#[test]
fn scope_references_and_source_cannot_widen_access() {
    let s = scope(&json!({"kind":"workspace","companySlugs":["beta","alpha","beta"]})).unwrap();
    assert_eq!(s.companies(), vec!["alpha", "beta"]);
    err(
        scope(&json!({"kind":"company","companySlug":"synthetic","local":true})),
        "invalid_input",
    );
    let mut r = reference();
    r["companySlug"] = json!("outside");
    err(references(&json!([r]), &task().scope), "invalid_input");
    let t = create(&source_input(), "unused-id", NOW, &[]).unwrap();
    err(
        update(
            &t,
            &json!({"scope":{"kind":"company","companySlug":"outside"}}),
            Some(1),
            NOW,
            &[],
        ),
        "invalid_input",
    );
    for href in [
        "/companies/outside/vat",
        "/companies/synthetic/../secret",
        "/companies/synthetic/vat#unsafe",
        "/companies/synthetic/vat?credential=1",
    ] {
        let mut v = source_input();
        v["source"]["href"] = json!(href);
        err(create(&v, "unused-id", NOW, &[]), "invalid_input");
    }
}
#[test]
fn completed_state_requires_note_evidence_and_explicit_reopen() {
    let mut v = draft_input();
    v["evidenceRequired"] = json!(true);
    let t = create(&v, "unused-id", NOW, &[]).unwrap();
    err(
        move_task(&t, &json!({"status":"done"}), Some(1), NOW, &[]),
        "completion_required",
    );
    err(
        complete(
            &t,
            &json!({"outcome":"completed","note":""}),
            Some(1),
            NOW,
            "user:synthetic",
            None,
            &[],
        ),
        "invalid_input",
    );
    err(
        complete(
            &t,
            &json!({"outcome":"completed","note":"Kontrolleret"}),
            Some(1),
            NOW,
            "user:synthetic",
            None,
            &[],
        ),
        "evidence_required",
    );
    let done=complete(&t,&json!({"outcome":"completed","note":"Kontrolleret","references":[reference(),reference()]}),Some(1),NOW,"user:synthetic",None,&[]).unwrap();
    let c = done.completion.as_ref().unwrap();
    assert_eq!(c.references.len(), 1);
    assert_eq!(c.assurance, "user_reported");
    err(
        move_task(&done, &json!({"status":"open"}), Some(2), NOW, &[]),
        "reopen_required",
    );
    err(
        reopen(&done, &json!(" "), Some(2), NOW, &[]),
        "invalid_input",
    );
    assert_eq!(
        reopen(&done, &json!("Nyt bilag kræver kontrol"), Some(2), NOW, &[])
            .unwrap()
            .version,
        3
    );
    err(
        complete(
            &done,
            &json!({"outcome":"completed","note":"igen"}),
            Some(2),
            NOW,
            "user:synthetic",
            None,
            &[],
        ),
        "already_completed",
    );
}
#[test]
fn source_resolution_is_fresh_authoritative_and_evidenced() {
    let t = create(&source_input(), "unused-id", NOW, &[]).unwrap();
    let input =
        json!({"outcome":"completed","note":"Kilden kontrolleret","references":[reference()]});
    err(
        complete(&t, &input, Some(1), NOW, "user:synthetic", None, &[]),
        "source_unresolved",
    );
    let mut check = SourceCheck {
        state: "resolved".into(),
        observed_at: "2026-10-26T10:58:59.999Z".into(),
        evidence: vec![],
    };
    err(
        complete(
            &t,
            &input,
            Some(1),
            NOW,
            "user:synthetic",
            Some(&check),
            &[],
        ),
        "source_check_stale",
    );
    check.observed_at = "2026-10-26T11:00:05.001Z".into();
    err(
        complete(
            &t,
            &input,
            Some(1),
            NOW,
            "user:synthetic",
            Some(&check),
            &[],
        ),
        "source_check_stale",
    );
    check.observed_at = NOW.into();
    let reported = complete(
        &t,
        &input,
        Some(1),
        NOW,
        "user:synthetic",
        Some(&check),
        &[],
    )
    .unwrap();
    assert_eq!(reported.completion.unwrap().assurance, "user_reported");
    check.evidence = references(&json!([reference()]), &t.scope).unwrap();
    let verified = complete(
        &t,
        &input,
        Some(1),
        NOW,
        "user:synthetic",
        Some(&check),
        &[],
    )
    .unwrap();
    assert_eq!(verified.completion.unwrap().assurance, "product_verified");
    assert_eq!(verified.source.unwrap().state, "resolved");
    check.evidence[0].company_slug = Some("outside".into());
    err(
        complete(
            &t,
            &input,
            Some(1),
            NOW,
            "user:synthetic",
            Some(&check),
            &[],
        ),
        "invalid_input",
    );
    // An explicit exception is documented but is never product verification.
    let exception = complete(
        &t,
        &json!({"outcome":"exception","note":"Dokumenteret undtagelse"}),
        Some(1),
        NOW,
        "user:synthetic",
        None,
        &[],
    )
    .unwrap();
    assert_eq!(exception.completion.unwrap().assurance, "user_reported");
}
#[test]
fn unknown_relevance_external_verification_and_statutory_basis_stay_guarded() {
    let mut v = draft_input();
    v["relevance"] = json!("unknown");
    let t = create(&v, "unused-id", NOW, &[]).unwrap();
    assert!(t.verification_required);
    err(
        complete(
            &t,
            &json!({"outcome":"completed","note":"færdig"}),
            Some(1),
            NOW,
            "user:synthetic",
            None,
            &[],
        ),
        "relevance_unknown",
    );
    err(
        update(
            &t,
            &json!({"verificationRequired":false}),
            Some(1),
            NOW,
            &[],
        ),
        "invalid_input",
    );
    let clarified=update(&t,&json!({"verificationRequired":false,"relevance":"relevant","description":"Ekstern kvittering gennemgået","references":[reference()]}),Some(1),NOW,&[]).unwrap();
    assert!(!clarified.verification_required);
    let d =
        json!({"date":"2026-11-01","kind":"statutory","certainty":"confirmed","basis":"Regel A"});
    v = draft_input();
    v["deadline"] = d;
    let t = create(&v, "unused-id", NOW, &[]).unwrap();
    err(
        update(&t, &json!({"deadline":null}), Some(1), NOW, &[]),
        "invalid_input",
    );
    let mut revised = t.deadline.clone();
    revised["date"] = json!("2026-11-02");
    err(
        update(&t, &json!({"deadline":revised}), Some(1), NOW, &[]),
        "invalid_input",
    );
    revised["basis"] = json!("Ny regel B");
    assert!(update(&t, &json!({"deadline":revised}), Some(1), NOW, &[]).is_ok());
    let done = complete(
        &task(),
        &json!({"outcome":"not_relevant","note":"Ikke omfattet"}),
        Some(1),
        NOW,
        "user:synthetic",
        None,
        &[],
    )
    .unwrap();
    let reopened = reopen(&done, &json!("Nye forhold"), Some(2), NOW, &[]).unwrap();
    assert_eq!(reopened.relevance, "unknown");
    assert!(reopened.verification_required);
}
#[test]
fn version_input_calendar_and_reminder_validation_fail_closed() {
    err(
        update(&task(), &json!({"title":"Ændret"}), None, NOW, &[]),
        "version_required",
    );
    err(
        update(&task(), &json!({"title":"Ændret"}), Some(2), NOW, &[]),
        "version_conflict",
    );
    for key in ["actor", "completion", "version", "sourceCheck", "local"] {
        err(
            update(&task(), &json!({key:true}), Some(1), NOW, &[]),
            "invalid_input",
        );
    }
    for day in ["2026-02-29", "2026-2-01", "2026-04-31"] {
        err(date(&json!(day)), "invalid_input");
    }
    assert_eq!(
        instant("2026-10-26T24:00:00Z").unwrap().0,
        "2026-10-27T00:00:00.000Z"
    );
    err(instant("2026-10-26T24:00:01Z"), "invalid_input");
    let reminder = json!({"reminderId":"reminder-one","recipientId":"member-one","enabled":true,"kind":"deadline","daysBefore":366,"frequency":"once","timeZone":"Europe/Copenhagen"});
    let mut v = draft_input();
    v["reminders"] = json!([reminder.clone()]);
    assert!(create(&v, "unused-id", NOW, &[]).is_ok());
    v["reminders"] = json!([reminder.clone(), reminder]);
    err(create(&v, "unused-id", NOW, &[]), "invalid_input");
    v["reminders"].as_array_mut().unwrap().truncate(1);
    v["reminders"][0]["timeZone"] = json!("Invalid/Unknown");
    err(create(&v, "unused-id", NOW, &[]), "invalid_input");
}
#[test]
fn company_board_ambiguity_requires_choice_and_workspace_projection_follows_status() {
    let mut columns = STATUSES
        .iter()
        .map(|s| Column {
            column_id: s.to_string(),
            name: s.to_string(),
            status: s.to_string(),
            is_default: true,
        })
        .collect::<Vec<_>>();
    columns.push(Column {
        column_id: "review".into(),
        name: "Review".into(),
        status: "in_progress".into(),
        is_default: false,
    });
    let boards = vec![Board {
        board_id: "company:synthetic".into(),
        version: 1,
        scope: task().scope,
        updated_at: NOW.into(),
        columns,
    }];
    err(
        move_task(
            &task(),
            &json!({"status":"in_progress"}),
            Some(1),
            NOW,
            &boards,
        ),
        "column_choice_required",
    );
    let moved = move_task(
        &task(),
        &json!({"status":"in_progress","columnId":"review","workspaceColumnId":"in_progress"}),
        Some(1),
        NOW,
        &boards,
    )
    .unwrap();
    assert_eq!(moved.column_id, "review");
    err(
        move_task(
            &moved,
            &json!({"status":"waiting","columnId":"review"}),
            Some(2),
            NOW,
            &boards,
        ),
        "column_invalid",
    );
    let done = complete(
        &moved,
        &json!({"outcome":"completed","note":"Kontrolleret"}),
        Some(2),
        NOW,
        "user:synthetic",
        None,
        &boards,
    )
    .unwrap();
    assert_eq!(done.workspace_column_id.as_deref(), Some("done"));
}
#[test]
fn canonical_hash_matches_main_including_utf16_key_order_and_js_numbers() {
    let corpus: Value = serde_json::from_str(include_str!("main-flow.json")).unwrap();
    for row in corpus["hashes"].as_array().unwrap() {
        assert_eq!(
            canonical_json(&row["input"]),
            row["canonical"].as_str().unwrap()
        );
        assert_eq!(
            payload_hash(&row["input"]).unwrap(),
            row["sha256"].as_str().unwrap()
        );
    }
    err(payload_hash(&json!("a".repeat(1_048_576))), "invalid_input");
}
