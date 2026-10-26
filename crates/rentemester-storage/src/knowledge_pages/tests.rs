use super::*;
pub(crate) fn populated() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::control_migration::initialize_empty(&mut db).unwrap();
    let v: Value = serde_json::from_str(include_str!(
        "../../../rentemester-engine/src/knowledge_pages/main-pages.json"
    ))
    .unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    for name in ["first", "second", "target", "relation", "rel2"] {
        let row = &v[name];
        let mut input = row["input"].clone();
        for k in ["actor", "principal", "expectedVersion", "expectedEventHash"] {
            input.as_object_mut().unwrap().remove(k);
        }
        let ctx = Context {
            actor: "user:synthetic",
            principal: "synthetic-member",
            now: row["at"].as_str().unwrap(),
        };
        let expected = row["input"]["expectedVersion"]
            .as_u64()
            .map(|n| (n, row["input"]["expectedEventHash"].as_str().unwrap()));
        let result = if matches!(name, "relation" | "rel2") {
            append_relation(
                &tx,
                &serde_json::from_value(input).unwrap(),
                "generated",
                expected,
                &ctx,
            )
        } else {
            append_page(
                &tx,
                &serde_json::from_value(input).unwrap(),
                "generated",
                expected,
                &ctx,
            )
        }
        .unwrap();
        assert_eq!(result, row["result"], "{name}");
    }
    tx.commit().unwrap();
    db
}
#[test]
fn actual_main_history_interval_relations_backlinks_metrics_and_read_only_scope() {
    let db = populated();
    let v: Value = serde_json::from_str(include_str!(
        "../../../rentemester-engine/src/knowledge_pages/main-pages.json"
    ))
    .unwrap();
    let scope = Scope::Company {
        company_slug: "synthetic".into(),
    };
    let before = db.total_changes();
    assert_eq!(
        json!(pages(&db, &scope, Some("2026-10-26")).unwrap()),
        v["pages"]
    );
    assert_eq!(
        json!(pages(&db, &scope, Some("2027-01-01")).unwrap()),
        v["expired"]
    );
    assert_eq!(
        json!(
            page_history(&db, &scope, "page-synthetic")
                .unwrap()
                .iter()
                .map(|p| p.output(false).unwrap())
                .collect::<Vec<_>>()
        ),
        v["history"]
    );
    assert_eq!(json!(relations(&db, &scope, None).unwrap()), v["relations"]);
    assert_eq!(
        json!(backlinks(&db, &scope, "another-page", None).unwrap()),
        v["backlinks"]
    );
    assert_eq!(
        metric(&db, "knowledge-page-count", &scope, Some("2026-10-26"), 1).unwrap(),
        v["metric"]
    );
    assert_eq!(
        read_page(&db, &scope, "playbook", "2026-10-26")
            .unwrap()
            .unwrap(),
        v["second"]["result"]
    );
    assert!(
        read_page(&db, &Scope::Workspace, "playbook", "2026-10-26")
            .unwrap()
            .is_none()
    );
    assert!(
        read_page(&db, &scope, "playbook", "2027-01-01")
            .unwrap()
            .is_none()
    );
    assert_eq!(db.total_changes(), before);
    validate(&db, Some(&["synthetic".into()])).unwrap();
    assert!(validate(&db, Some(&["different".into()])).is_err());
}
#[test]
fn version_hash_scope_endpoints_and_appendonly_refuse_without_partial_effects() {
    let mut db = populated();
    let v: Value = serde_json::from_str(include_str!(
        "../../../rentemester-engine/src/knowledge_pages/main-pages.json"
    ))
    .unwrap();
    let mut input = v["second"]["input"].clone();
    for k in ["actor", "principal", "expectedVersion", "expectedEventHash"] {
        input.as_object_mut().unwrap().remove(k);
    }
    let i: PageInput = serde_json::from_value(input).unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let ctx = Context {
        actor: "user:synthetic",
        principal: "synthetic-member",
        now: "2026-10-26",
    };
    let count: i64 = tx
        .query_row("SELECT count(*) FROM rm_knowledge_page_events", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        append_page(&tx, &i, "unused", Some((1, "wrong")), &ctx)
            .unwrap_err()
            .code,
        "version_conflict"
    );
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM rm_knowledge_page_events", [], |r| r
            .get(0))
            .unwrap(),
        count
    );
    assert!(
        tx.execute("UPDATE rm_knowledge_page_events SET title='altered'", [])
            .is_err()
    );
    assert!(
        tx.execute("DELETE FROM rm_knowledge_relation_events", [])
            .is_err()
    );
    let mut relation_input=v["relation"]["input"].clone();for key in ["actor","principal"]{relation_input.as_object_mut().unwrap().remove(key);}let mut relation:RelationInput=serde_json::from_value(relation_input).unwrap();relation.relation_id=Some("relation-missing".into());relation.object.r#ref="missing-page".into();assert!(append_relation(&tx,&relation,"unused",None,&ctx).is_err());relation.object.r#ref="another-page".into();relation.scope=Scope::Workspace;assert!(append_relation(&tx,&relation,"unused",None,&ctx).is_err());assert_eq!(tx.query_row::<i64,_,_>("SELECT count(*) FROM rm_knowledge_relation_events",[],|r|r.get(0)).unwrap(),2);
    tx.rollback().unwrap();
}
/// Existing Rust fixture seam only. No product endpoint or authority is created.
#[test]
#[ignore = "requires an explicitly owned stopped disposable native fixture"]
fn provision_owned_synthetic_knowledge_fixture() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::PathBuf;
    let root = PathBuf::from(std::env::var("RM_TASK_WORKSPACE").expect("explicit fixture"));
    let home = PathBuf::from(std::env::var("HOME").unwrap());
    let base = root.parent().unwrap();
    assert_eq!(
        base.parent().unwrap(),
        home.join("Library/Application Support")
    );
    assert!(
        base.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("Rentemester-Synthetic-Tasks-")
    );
    assert_eq!(root.canonicalize().unwrap(), root);
    assert_eq!(
        std::fs::metadata(base).unwrap().uid(),
        std::fs::metadata(home).unwrap().uid()
    );
    assert_eq!(
        std::fs::metadata(base).unwrap().permissions().mode() & 0o077,
        0
    );
    let file = root.join(".rentemester/workspace-control.sqlite");
    assert!(
        !std::fs::symlink_metadata(&file)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let mut db = Connection::open(&file).unwrap();
    crate::control_migration::validate_current_control(&db).unwrap();
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT count(*) FROM rm_knowledge_page_events", [], |r| r
            .get(0))
            .unwrap(),
        0
    );
    let before: i64 = db
        .query_row("SELECT count(*) FROM user", [], |r| r.get(0))
        .unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    for (id, scope, title, from, to) in [
        (
            "page-company",
            json!({"kind":"company","companySlug":"synthetic"}),
            "Synthetic company playbook",
            "2026-01-01",
            "2027-01-01",
        ),
        (
            "page-workspace",
            json!({"kind":"workspace"}),
            "Synthetic workspace playbook",
            "2026-01-01",
            "2027-01-01",
        ),
        (
            "page-expired",
            json!({"kind":"company","companySlug":"synthetic"}),
            "Synthetic expired playbook",
            "2025-01-01",
            "2026-01-01",
        ),
    ] {
        let input:PageInput=serde_json::from_value(json!({"pageId":id,"scope":scope,"slug":id,"title":title,"bodyMarkdown":"<script>window.syntheticInjected=true</script> Synthetic protected instructions [[related-page]]","provenance":{"kind":"user","ref":"Synthetic stopped fixture"},"effectiveFrom":from,"effectiveToExclusive":to})).unwrap();
        append_page(
            &tx,
            &input,
            "unused",
            None,
            &Context {
                actor: "system:synthetic-fixture",
                principal: "historical-source-label-not-grant",
                now: "2026-01-01",
            },
        )
        .unwrap();
    }
    tx.commit().unwrap();
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT count(*) FROM user", [], |r| r.get(0))
            .unwrap(),
        before
    );
    println!(
        "PASS native programmatic page authoring in stopped owned fixture; no identity/grant seeded"
    );
}
