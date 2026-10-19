use super::*;
fn db() -> rusqlite::Connection {
    let mut c = rusqlite::Connection::open_in_memory().unwrap();
    let mut l = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic ownership").unwrap();
    c
}
fn input(id: &str, owner: &str, bp: u16, from: &str) -> Propose {
    serde_json::from_value(json!({"snapshotId":id,"source":"synthetic","observedAt":"2026-01-01","facts":[{"owner":{"kind":"company","companySlug":owner},"ownedCompanySlug":"child","validFrom":from,"economicBasisPoints":bp,"controlType":"equity","jurisdiction":"DK","evidenceRefs":["synthetic"]}]})).unwrap()
}
fn proposal(tx: &Transaction<'_>, i: &Propose) -> Value {
    propose(
        tx,
        i,
        &normalized(&i.facts).unwrap(),
        "unused",
        "user:maker",
        "maker",
        "2026-01-01T00:00:00.000Z",
    )
    .unwrap()
}
fn approved_apply(tx: &Transaction<'_>, s: &Value) -> Apply {
    let id = s["snapshotId"].as_str().unwrap();
    review(
        tx,
        &Review {
            snapshot_id: id.into(),
            decision: "approved".into(),
        },
        "user:reviewer",
        "reviewer",
        "2026-01-02T00:00:00.000Z",
    )
    .unwrap();
    let a = Apply {
        snapshot_id: id.into(),
        snapshot_hash: s["snapshotHash"].as_str().unwrap().into(),
        diff_hash: s["diffHash"].as_str().unwrap().into(),
    };
    apply(
        tx,
        &a,
        "user:reviewer",
        "reviewer",
        "2026-01-02T00:00:00.000Z",
    )
    .unwrap();
    a
}
#[test]
fn independent_review_hashes_immutable_temporal_history_and_replay() {
    let mut c = db();
    let tx = c.transaction().unwrap();
    let i = input("initial", "parent", 6000, "2026-01-01");
    let s = proposal(&tx, &i);
    // Golden hashes produced independently by the existing TypeScript core.
    assert_eq!(
        s["snapshotHash"],
        "8cb0b80ece5389a086c06a8cdf9167917fe6c993ee343ed34ee0703cd4ca88e8"
    );
    assert_eq!(
        s["diffHash"],
        "4b1a648a6ad9d49db2c58de0217baf18c933cffa39b436fa17a383418da22992"
    );
    assert_eq!(proposal(&tx, &i), s);
    let selfreview = Review {
        snapshot_id: "initial".into(),
        decision: "approved".into(),
    };
    assert!(review(&tx, &selfreview, "user:renamed", "maker", "2026-01-02").is_err());
    let mut a = approved_apply(&tx, &s);
    assert_eq!(
        apply(&tx, &a, "user:reviewer", "reviewer", "2026-01-02").unwrap()["status"],
        "unchanged"
    );
    a.diff_hash = "0".repeat(64);
    assert!(apply(&tx, &a, "user:reviewer", "reviewer", "2026-01-02").is_err());
    let later = proposal(&tx, &input("later", "parent", 7000, "2026-06-01"));
    assert_eq!(later["diff"]["endings"].as_array().unwrap().len(), 1);
    approved_apply(&tx, &later);
    let rows = approved(&tx).unwrap();
    assert_eq!(
        rows.iter()
            .filter(|(_, f)| f.active("2026-05-31"))
            .map(|(_, f)| f.economic_basis_points.unwrap())
            .collect::<Vec<_>>(),
        [6000]
    );
    assert_eq!(
        rows.iter()
            .filter(|(_, f)| f.active("2026-06-01"))
            .map(|(_, f)| f.economic_basis_points.unwrap())
            .collect::<Vec<_>>(),
        [7000]
    );
    assert!(
        tx.execute("UPDATE rm_ownership_facts SET owner_id='changed'", [])
            .is_err()
    );
    assert!(
        tx.execute("DELETE FROM rm_ownership_fact_events", [])
            .is_err()
    );
    assert_eq!(
        load(&tx, "initial").unwrap().public["history"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
#[test]
fn concurrent_reviewed_proposals_cannot_overallocate_or_reuse_changed_identity() {
    let mut c = db();
    let tx = c.transaction().unwrap();
    let first = proposal(&tx, &input("first", "a", 7000, "2026-01-01"));
    let second = proposal(&tx, &input("second", "b", 7000, "2026-01-01"));
    review(
        &tx,
        &Review {
            snapshot_id: "second".into(),
            decision: "approved".into(),
        },
        "user:reviewer",
        "reviewer",
        "2026-01-02",
    )
    .unwrap();
    approved_apply(&tx, &first);
    let a = Apply {
        snapshot_id: "second".into(),
        snapshot_hash: second["snapshotHash"].as_str().unwrap().into(),
        diff_hash: second["diffHash"].as_str().unwrap().into(),
    };
    assert!(apply(&tx, &a, "user:reviewer", "reviewer", "2026-01-02").is_err());
    assert_eq!(approved(&tx).unwrap().len(), 1);
    assert_eq!(load(&tx, "second").unwrap().public["state"], "approved");
    assert!(
        propose(
            &tx,
            &input("second", "b", 6000, "2026-01-01"),
            &normalized(&input("second", "b", 6000, "2026-01-01").facts).unwrap(),
            "unused",
            "user:maker",
            "maker",
            "2026-01-02"
        )
        .is_err()
    );
}
#[test]
fn corrupted_canonical_fact_or_snapshot_refuses_instead_of_projection() {
    let mut c = db();
    let tx = c.transaction().unwrap();
    let s = proposal(&tx, &input("initial", "parent", 6000, "2026-01-01"));
    approved_apply(&tx, &s);
    tx.execute_batch("DROP TRIGGER rm_ownership_facts_no_update;UPDATE rm_ownership_facts SET canonical_fact='{}'").unwrap();
    assert!(approved(&tx).is_err());
    tx.execute_batch("DROP TRIGGER rm_ownership_source_snapshots_no_update;UPDATE rm_ownership_source_snapshots SET diff_json='{}'").unwrap();
    assert!(load(&tx, "initial").is_err());
}
