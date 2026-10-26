use super::*;
fn fresh() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::control_migration::initialize_empty(&mut db).unwrap();
    db
}
#[test]
fn complete_sourcefree_page_relation_history_restores_without_grants() {
    let db = crate::knowledge_pages::tests::populated();
    let bytes = export(&db).unwrap();
    let mut out = fresh();
    initialize(&mut out, &bytes, &["synthetic".into()]).unwrap();
    assert_eq!(export(&out).unwrap(), bytes);
    assert_eq!(
        out.query_row::<i64, _, _>("SELECT count(*) FROM user", [], |r| r.get(0))
            .unwrap(),
        0
    );
    assert_eq!(
        out.query_row::<i64, _, _>(
            "SELECT count(*) FROM rm_workspace_user_access_events",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        0
    );
}
#[test]
fn changed_body_hash_principal_chain_unknown_company_or_codec_refuses_atomically() {
    let db = crate::knowledge_pages::tests::populated();
    let bytes = export(&db).unwrap();
    for column in [
        "body_markdown",
        "event_hash",
        "principal",
        "prior_event_hash",
    ] {
        let mut b: Business = serde_json::from_slice(&bytes).unwrap();
        let t = b.tables.get_mut("rm_knowledge_page_events").unwrap();
        let i = t.columns.iter().position(|c| c == column).unwrap();
        t.rows[1][i] = Cell::Text("tampered".into());
        let mut out = fresh();
        assert!(
            initialize(
                &mut out,
                &serde_json::to_vec(&b).unwrap(),
                &["synthetic".into()]
            )
            .is_err(),
            "{column}"
        );
        assert_eq!(
            out.query_row::<i64, _, _>("SELECT count(*) FROM rm_knowledge_page_events", [], |r| r
                .get(0))
                .unwrap(),
            0
        );
    }
    assert!(initialize(&mut fresh(), &bytes, &["unregistered".into()]).is_err());
}
