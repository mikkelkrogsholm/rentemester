use super::*;
const NOW: &str = "2026-01-01T00:00:00.000Z";
// Synthetic examples from tests/unit/task-series.test.ts; no company ledger is needed.
fn input() -> Value {
    json!({"seriesId":"monthly","title":"Månedlig afstemning","scope":{"kind":"company","companySlug":"synthetic"},
        "template":{"title":"Månedlig afstemning","scope":{"kind":"company","companySlug":"synthetic"}},
        "cadence":"month","every":1,"anchor":"calendar","startDate":"2026-01-01","endDate":null,
        "fiscalYearStartMonth":1,"workDayOffset":0,"deadlineDayOffset":14,"relevance":"relevant","active":true})
}
fn saved() -> Series {
    save_series(None, &input(), Some(0), NOW).unwrap()
}
fn query(from: &str, to: &str) -> ProjectionQuery {
    ProjectionQuery {
        from: from.into(),
        to: to.into(),
        company_slugs: None,
        include_workspace: false,
    }
}
fn projections(history: Vec<Series>, tasks: &[Task], from: &str, to: &str) -> Vec<Projection> {
    project_series(&[history], tasks, &query(from, to)).unwrap()
}
fn concrete(series: &Series, projection: &Projection) -> Task {
    super::super::create(
        &materialization_draft(series, projection).unwrap(),
        "unused",
        NOW,
        &[],
    )
    .unwrap()
}
fn err<T: std::fmt::Debug>(result: Result<T, Error>, code: &str) {
    assert_eq!(result.unwrap_err().code, code);
}

#[test]
fn monthly_future_projection_is_pure_and_deadline_is_unconfirmed() {
    let series = saved();
    let before = json!(series);
    let p = projections(vec![series.clone()], &[], "2026-01-01", "2026-12-31");
    assert_eq!(p.len(), 12);
    assert_eq!(
        json!(p[1]),
        json!({"projectionId":occurrence_id("monthly",&p[1].period),"seriesId":"monthly","title":"Månedlig afstemning",
        "scope":{"kind":"company","companySlug":"synthetic"},"period":{"from":"2026-02-01","to":"2026-02-28","label":"2026-02-01 – 2026-02-28"},
        "workDate":"2026-02-01","deadline":{"date":"2026-02-15","kind":"internal","basis":"Den valgte rutines frist","certainty":"unconfirmed"},"relevance":"relevant","concreteTaskId":null})
    );
    assert_eq!(json!(series), before);
    // SHA-256 of the original seriesId:from:to bytes, not title or completion date.
    assert_eq!(
        p[0].projection_id,
        format!(
            "occurrence-{}",
            &crate::sha256_bytes(b"monthly:2026-01-01:2026-01-31")[..32]
        )
    );
}
#[test]
fn unfinished_periods_are_independent_and_duplicate_materialization_is_existing() {
    let series = saved();
    let p = projections(vec![series.clone()], &[], "2026-01-01", "2026-02-10");
    let tasks = p
        .iter()
        .filter(|p| p.work_date.as_str() <= "2026-02-10")
        .map(|p| concrete(&series, p))
        .collect::<Vec<_>>();
    assert_eq!(tasks.len(), 2);
    assert!(
        tasks
            .iter()
            .all(|t| t.status == "open" && t.completion.is_none())
    );
    assert_eq!(tasks[0].period["from"], "2026-01-01");
    assert_eq!(tasks[1].period["from"], "2026-02-01");
    let repeated = projections(vec![series.clone()], &tasks, "2026-01-01", "2026-02-10");
    assert_eq!(repeated.len(), 2);
    assert!(repeated.iter().all(|p| p.concrete_task_id.is_some()));
    err(
        materialization_draft(&series, &repeated[0]),
        "invalid_input",
    );
}
#[test]
fn fiscal_quarters_custom_intervals_and_leap_year_have_stable_anchors() {
    let mut v = input();
    v["cadence"] = json!("quarter");
    v["anchor"] = json!("fiscal");
    v["startDate"] = json!("2026-07-01");
    v["fiscalYearStartMonth"] = json!(7);
    let series = save_series(None, &v, None, NOW).unwrap();
    let p = projections(vec![series], &[], "2026-07-01", "2027-06-30");
    assert_eq!(
        p.iter()
            .map(|p| (p.period.from.as_str(), p.period.to.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("2026-07-01", "2026-09-30"),
            ("2026-10-01", "2026-12-31"),
            ("2027-01-01", "2027-03-31"),
            ("2027-04-01", "2027-06-30")
        ]
    );
    let mut v = input();
    v["cadence"] = json!("custom");
    v["every"] = json!(2);
    let p = projections(
        vec![save_series(None, &v, None, NOW).unwrap()],
        &[],
        "2026-01-01",
        "2026-06-30",
    );
    assert_eq!(
        p.iter().map(|p| p.period.from.as_str()).collect::<Vec<_>>(),
        vec!["2026-01-01", "2026-03-01", "2026-05-01"]
    );
    let mut v = input();
    v["startDate"] = json!("2028-01-01");
    let p = projections(
        vec![save_series(None, &v, None, NOW).unwrap()],
        &[],
        "2028-02-01",
        "2028-02-29",
    );
    assert_eq!(p[0].period.to, "2028-02-29");
}
#[test]
fn offsets_end_date_and_statutory_basis_are_preserved() {
    let mut v = input();
    v["startDate"] = json!("2026-01-10");
    v["workDayOffset"] = json!(9);
    v["deadlineDayOffset"] = json!(-1);
    v["endDate"] = json!("2026-02-01");
    v["template"]["deadline"] = json!({"date":"2026-01-01","kind":"statutory","basis":"Synthetic rule","certainty":"unconfirmed","ruleId":"rule:synthetic"});
    let p = projections(
        vec![save_series(None, &v, None, NOW).unwrap()],
        &[],
        "2026-01-01",
        "2026-04-30",
    );
    assert_eq!(p.len(), 2);
    assert_eq!(p[0].work_date, "2026-01-10");
    assert_eq!(p[0].deadline.as_ref().unwrap().date, "2025-12-31");
    assert_eq!(
        p[0].deadline.as_ref().unwrap().rule_id.as_deref(),
        Some("rule:synthetic")
    );
    assert_eq!(p[1].deadline.as_ref().unwrap().basis, "Synthetic rule");
    v["template"]["deadline"]["basis"] = json!(" ");
    err(validate_series(&v), "invalid_input");
    let mut v = input();
    v["workDayOffset"] = json!(3660);
    v["deadlineDayOffset"] = json!(-3660);
    assert!(validate_series(&v).is_ok());
    v["workDayOffset"] = json!(3661);
    err(validate_series(&v), "invalid_input");
}
#[test]
fn pause_preserves_concrete_tasks_and_evidence_without_new_periods() {
    let series = saved();
    let p = projections(vec![series.clone()], &[], "2026-01-01", "2026-02-01");
    let mut tasks = p.iter().map(|p| concrete(&series, p)).collect::<Vec<_>>();
    tasks[0].title = "Konkret titel".into();
    tasks[0].work_date = None;
    tasks[0].deadline = Value::Null;
    tasks[0].references = vec![super::super::Reference {
        kind: "document".into(),
        reference: "synthetic-proof".into(),
        company_slug: Some("synthetic".into()),
    }];
    let before = json!(tasks);
    let mut v = input();
    v["active"] = json!(false);
    v["title"] = json!("Ny titel");
    v["workDayOffset"] = json!(9);
    let paused = save_series(Some(&series), &v, Some(1), NOW).unwrap();
    let p = projections(vec![series, paused], &tasks, "2026-01-01", "2026-04-30");
    assert_eq!(p.len(), 2);
    assert_eq!(p[0].title, "Konkret titel");
    assert_eq!(p[0].work_date, "2026-01-01");
    assert!(p[0].deadline.is_none());
    assert_eq!(json!(tasks), before);
}
#[test]
fn future_revision_preserves_backlog_and_old_concrete_cadence() {
    let original = saved();
    let mut v = input();
    v["title"] = json!("Ny rutine");
    v["workDayOffset"] = json!(9);
    let revised = save_series(Some(&original), &v, Some(1), "2026-03-01T00:00:00.000Z").unwrap();
    let p = projections(
        vec![original.clone(), revised.clone()],
        &[],
        "2026-01-01",
        "2026-04-30",
    );
    assert_eq!(p.len(), 4);
    assert_eq!(p[0].title, "Månedlig afstemning");
    assert_eq!(p[2].title, "Ny rutine");
    assert_eq!(p[2].work_date, "2026-03-10");
    let old = concrete(&original, &p[0]);
    v["cadence"] = json!("quarter");
    v["title"] = json!("Kvartalsrutine");
    let quarter = save_series(Some(&revised), &v, Some(2), "2026-04-01T00:00:00.000Z").unwrap();
    let p = projections(
        vec![original, revised, quarter],
        &[old.clone()],
        "2026-01-01",
        "2026-06-30",
    );
    assert!(p.iter().any(
        |p| p.concrete_task_id.as_deref() == Some(old.task_id.as_str()) && p.title == old.title
    ));
    assert!(
        p.iter()
            .any(|p| p.period.from == "2026-04-01" && p.period.to == "2026-06-30")
    );
}
#[test]
fn activity_and_unknown_materialize_as_unverified_open_tasks() {
    for relevance in ["activity", "unknown", "relevant"] {
        let mut v = input();
        v["relevance"] = json!(relevance);
        let series = save_series(None, &v, None, NOW).unwrap();
        let p = projections(vec![series.clone()], &[], "2026-01-01", "2026-01-31");
        assert_eq!(p[0].relevance, relevance);
        let task = concrete(&series, &p[0]);
        assert_eq!(
            task.relevance,
            if relevance == "relevant" {
                "relevant"
            } else {
                "unknown"
            }
        );
        assert_eq!(task.verification_required, relevance != "relevant");
        assert_eq!(task.status, "open");
        assert!(task.completion.is_none());
        assert!(task.reminders.is_empty());
    }
}
#[test]
fn partial_and_empty_company_selection_cannot_leak_shared_or_workspace_routines() {
    let mut shared = input();
    shared["seriesId"] = json!("shared");
    shared["scope"] = json!({"kind":"workspace","companySlugs":["synthetic","other"]});
    shared["template"]["scope"] = shared["scope"].clone();
    let mut workspace = shared.clone();
    workspace["seriesId"] = json!("workspace");
    workspace["scope"] = json!({"kind":"workspace","companySlugs":[]});
    workspace["template"]["scope"] = workspace["scope"].clone();
    let histories = vec![
        vec![save_series(None, &shared, None, NOW).unwrap()],
        vec![save_series(None, &workspace, None, NOW).unwrap()],
    ];
    let mut q = query("2026-01-01", "2026-01-31");
    q.company_slugs = Some(vec!["synthetic".into()]);
    assert!(project_series(&histories, &[], &q).unwrap().is_empty());
    q.company_slugs = Some(vec!["synthetic".into(), "other".into()]);
    assert_eq!(
        project_series(&histories, &[], &q).unwrap()[0].series_id,
        "shared"
    );
    q.company_slugs = Some(vec![]);
    assert!(project_series(&histories, &[], &q).unwrap().is_empty());
    q.include_workspace = true;
    let p = project_series(&histories, &[], &q).unwrap();
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].series_id, "workspace");
    let mut task = concrete(
        &histories[0][0],
        &projections(histories[0].clone(), &[], "2026-01-01", "2026-01-31")[0],
    );
    // Concrete task scope is selected independently, including old completed periods.
    task.scope = Scope::Company {
        company_slug: "other".into(),
    };
    q.company_slugs = Some(vec!["synthetic".into()]);
    q.include_workspace = false;
    assert!(project_series(&histories, &[task], &q).unwrap().is_empty());
}
#[test]
fn strict_versions_scope_template_identity_privacy_and_activation_guards() {
    let series = saved();
    for expected in [None, Some(0), Some(8)] {
        err(
            save_series(Some(&series), &input(), expected, NOW),
            "version_conflict",
        );
    }
    err(
        save_series(None, &input(), Some(1), NOW),
        "version_conflict",
    );
    assert_eq!(
        save_series(Some(&series), &input(), Some(1), NOW)
            .unwrap()
            .version,
        2
    );
    let mut v = input();
    v["scope"] = json!({"kind":"company","companySlug":"other"});
    err(validate_series(&v), "invalid_input");
    v["template"]["scope"] = v["scope"].clone();
    err(
        save_series(Some(&series), &v, Some(1), NOW),
        "invalid_input",
    );
    for field in ["taskId", "source", "seriesId", "occurrenceKey"] {
        let mut v = input();
        v["template"][field] = json!("identity");
        assert!(validate_series(&v).is_err(), "{field}");
    }
    for field in ["title", "description"] {
        let mut v = input();
        v["template"][field] = json!("CPR 010190-1234");
        err(validate_series(&v), "private_data");
    }
    let mut v = input();
    v["title"] = json!("CPR 010190-1234");
    err(validate_series(&v), "private_data");
    for field in ["every", "fiscalYearStartMonth"] {
        let mut v = input();
        v[field] = json!(0);
        err(validate_series(&v), "invalid_input");
    }
    let mut v = input();
    v["template"]["status"] = json!("done");
    err(validate_series(&v), "invalid_input");
    let mut v = input();
    v["template"]["reminders"] = json!([{"reminderId":"r","recipientId":"u","enabled":true,"kind":"deadline","daysBefore":0,"frequency":"once","timeZone":"Europe/Copenhagen"}]);
    err(validate_series(&v), "invalid_input");
    v["template"]["reminders"][0]["enabled"] = json!(false);
    assert!(validate_series(&v).is_ok());
    let mut v = input();
    v.as_object_mut().unwrap().remove("endDate");
    err(validate_series(&v), "invalid_input");
    let mut v = input();
    v["title"] = json!("  Trimmet titel  ");
    assert_eq!(
        save_series(None, &v, None, NOW).unwrap().title,
        "Trimmet titel"
    );
}
#[test]
fn invalid_ranges_and_cross_scope_materialization_fail_closed() {
    let series = saved();
    for (from, to) in [
        ("2026-02-30", "2026-03-01"),
        ("2026-03-01", "2026-01-01"),
        ("2026-01-01", "2126-02-01"),
    ] {
        err(
            project_series(&[vec![series.clone()]], &[], &query(from, to)),
            "invalid_input",
        );
    }
    let mut p = projections(vec![series.clone()], &[], "2026-01-01", "2026-01-31").remove(0);
    p.scope = Scope::Company {
        company_slug: "other".into(),
    };
    err(materialization_draft(&series, &p), "invalid_input");
    p.scope = series.scope.clone();
    p.projection_id = "forged".into();
    err(materialization_draft(&series, &p), "invalid_input");
}
#[test]
fn actual_main_source_series_examples_match_exact_json() {
    let corpus: Value = serde_json::from_str(include_str!("main-series.json")).unwrap();
    for (source, bytes) in [
        (
            "src/core/task-series.ts",
            include_bytes!("../../../../../src/core/task-series.ts").as_slice(),
        ),
        (
            "src/core/tasks-types.ts",
            include_bytes!("../../../../../src/core/tasks-types.ts").as_slice(),
        ),
        (
            "src/core/dates.ts",
            include_bytes!("../../../../../src/core/dates.ts").as_slice(),
        ),
    ] {
        assert_eq!(
            crate::sha256_bytes(bytes),
            corpus["sources"][source].as_str().unwrap(),
            "Regenerate source oracle after changing {source}"
        );
    }
    let rows = corpus["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 11);
    for row in rows {
        let histories: Vec<Vec<Series>> = serde_json::from_value(row["histories"].clone()).unwrap();
        let tasks: Vec<Task> = serde_json::from_value(row["tasks"].clone()).unwrap();
        let query: ProjectionQuery = serde_json::from_value(row["query"].clone()).unwrap();
        assert_eq!(
            json!(project_series(&histories, &tasks, &query).unwrap()),
            row["expected"],
            "Source oracle case: {}",
            row["name"]
        );
    }
}
