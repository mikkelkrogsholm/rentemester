use super::*;
const NOW: &str = "2026-10-20T12:00:00.000Z";
pub(super) fn fixture() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic dispositions").unwrap();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let p=serde_json::from_value(json!({"partyId":"synthetic-party","kind":"organization","name":"Synthetic counterparty","source":"synthetic","observedAt":NOW,"reviewAssertion":"Explicit synthetic review","role":"related_company"})).unwrap();
    crate::parties::create(&tx, &p, "unused", "user:author", NOW).unwrap();
    let i=serde_json::from_value(json!({"recordId":"synthetic-record","type":"intercompany_agreement","bytesBase64":"unused","filename":"synthetic.txt","source":"synthetic","receivedAt":NOW,"uploader":"synthetic"})).unwrap();
    crate::corporate_records::ingest(
        &tx,
        &i,
        b"synthetic agreement",
        "unused",
        &[],
        "user:author",
    )
    .unwrap();
    tx.commit().unwrap();
    c
}
fn disposition(id: &str) -> Disposition {
    serde_json::from_value(json!({"dispositionId":id,"type":"loan","economicDate":"2026-10-20","amount":12.34,"currency":"DKK","partyIds":["synthetic-party"],"evidenceRecordIds":["synthetic-record"],"left":{"companySlug":"a","role":"lender","expectedSide":"receivable"},"right":{"companySlug":"b","role":"borrower","expectedSide":"payable"}})).unwrap()
}
fn snapshots() -> Vec<JournalSnapshot> {
    ["a", "b"]
        .into_iter()
        .enumerate()
        .map(|(n, s)| JournalSnapshot {
            company_slug: s.into(),
            ledger_uuid: format!("{n:032x}"),
            currency: "DKK".into(),
            ledger_head_hash: Some(format!("{:064x}", n + 1)),
            entries: vec![JournalEntry {
                id: 1,
                entry_no: "SYN-1".into(),
                transaction_date: "2026-10-20".into(),
                entry_hash: format!("{:064x}", n + 1),
                status: "posted".into(),
                reversal_of_entry_id: None,
                reversed: false,
                lines: vec![JournalLine {
                    debit: Money::parse(if n == 0 { "12.34" } else { "0" }).unwrap(),
                    credit: Money::parse(if n == 0 { "0" } else { "12.34" }).unwrap(),
                }],
            }],
        })
        .collect()
}
fn run(
    tx: &Transaction<'_>,
    mut a: Action,
    user: &str,
    s: &[JournalSnapshot],
) -> Result<Value, Error> {
    prepare(tx, &mut a)?;
    execute(
        tx,
        &a,
        s,
        &format!("display:{user}"),
        &Principal {
            kind: "user".into(),
            id: user.into(),
        },
        NOW,
    )
}
fn exact(v: &Value) -> Exact {
    Exact {
        disposition_id: v["disposition"]["dispositionId"].as_str().unwrap().into(),
        payload_hash: v["payloadHash"].as_str().unwrap().into(),
        confirm: true,
    }
}
fn proposed(tx: &Transaction<'_>, id: &str, s: &[JournalSnapshot]) -> Value {
    run(
        tx,
        Action::Propose(Propose {
            disposition: disposition(id),
            confirm: true,
        }),
        "author",
        s,
    )
    .unwrap()
}
fn approve(tx: &Transaction<'_>, v: &Value, s: &[JournalSnapshot]) -> Value {
    run(tx, Action::Approve(exact(v)), "reviewer", s).unwrap()
}
fn link(tx: &Transaction<'_>, v: &Value, side: &str, s: &[JournalSnapshot]) -> Value {
    let i = exact(v);
    run(
        tx,
        Action::Link(Link {
            disposition_id: i.disposition_id,
            payload_hash: i.payload_hash,
            confirm: true,
            side: side.into(),
            journal_entry_id: 1,
            ledger_head_hash: s[if side == "left" { 0 } else { 1 }]
                .ledger_head_hash
                .clone(),
        }),
        "reviewer",
        s,
    )
    .unwrap()
}
#[test]
fn source_lifecycle_exact_replays_stale_reads_and_rollback_have_no_financial_effects() {
    let mut c = fixture();
    let s = snapshots();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let changes = tx.total_changes();
    let plan = run(
        &tx,
        Action::Plan(Plan {
            disposition: disposition("synthetic-main"),
        }),
        "author",
        &s,
    )
    .unwrap();
    assert_eq!(
        plan["journalCandidates"]["left"][0]["journalEntryNo"],
        "SYN-1"
    );
    assert_eq!(tx.total_changes(), changes);
    let p = proposed(&tx, "synthetic-main", &s);
    assert_eq!(proposed(&tx, "synthetic-main", &s), p);
    assert!(run(&tx, Action::Approve(exact(&p)), "author", &s).is_err());
    let a = approve(&tx, &p, &s);
    assert_eq!(approve(&tx, &a, &s), a);
    let first = link(&tx, &a, "left", &s);
    assert_eq!(first["status"], "partly_posted");
    assert_eq!(link(&tx, &first, "left", &s), first);
    let posted = link(&tx, &first, "right", &s);
    assert_eq!(posted["status"], "posted");
    assert_eq!(posted["lifecycleEvents"].as_array().unwrap().len(), 3);
    let mut wrong = exact(&posted);
    wrong.payload_hash = "f".repeat(64);
    assert!(run(&tx, Action::Approve(wrong), "reviewer", &s).is_err());
    let i = exact(&posted);
    let settle = Settle {
        disposition_id: i.disposition_id.clone(),
        payload_hash: i.payload_hash.clone(),
        confirm: true,
        settlement_evidence_record_ids: vec!["synthetic-record".into(), "synthetic-record".into()],
    };
    let settled = run(&tx, Action::Settle(settle.clone()), "reviewer", &s).unwrap();
    assert_eq!(settled["status"], "settled");
    let mut duplicate = settle;
    duplicate.settlement_evidence_record_ids = vec!["does-not-exist".into()];
    assert_eq!(
        run(&tx, Action::Settle(duplicate), "reviewer", &s).unwrap(),
        settled
    );
    let reopened = run(
        &tx,
        Action::Reopen(Reopen {
            disposition_id: i.disposition_id.clone(),
            payload_hash: i.payload_hash.clone(),
            confirm: true,
            reason: "Reviewed reopening".into(),
        }),
        "reviewer",
        &s,
    )
    .unwrap();
    assert_eq!(reopened["status"], "posted");
    let mut stale = s.clone();
    stale[0].ledger_head_hash = Some("c".repeat(64));
    let status = run(
        &tx,
        Action::Status {
            disposition_id: i.disposition_id,
            as_of: "2026-10-21".into(),
        },
        "author",
        &stale,
    )
    .unwrap();
    assert_eq!(status["exceptions"][0]["kind"], "stale_or_reversed_journal");
    assert_eq!(status["balanced"], false);
    assert_eq!(validate_history(&tx).unwrap().len(), 2);
    tx.rollback().unwrap();
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM rm_intercompany_dispositions",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
#[test]
fn supersession_requires_replacement_and_keeps_original_links_and_principals() {
    let mut c = fixture();
    let s = snapshots();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let first = proposed(&tx, "synthetic-old", &s);
    let replacement = proposed(&tx, "synthetic-new", &s);
    let e = exact(&first);
    let value = run(
        &tx,
        Action::Supersede(Supersede {
            disposition_id: e.disposition_id.clone(),
            payload_hash: e.payload_hash.clone(),
            confirm: true,
            replacement_disposition_id: replacement["disposition"]["dispositionId"]
                .as_str()
                .unwrap()
                .into(),
            reason: "Reviewed replacement".into(),
        }),
        "reviewer",
        &s,
    )
    .unwrap();
    assert_eq!(value["status"], "superseded");
    let reopened = run(
        &tx,
        Action::Reopen(Reopen {
            disposition_id: e.disposition_id,
            payload_hash: e.payload_hash,
            confirm: true,
            reason: "Reviewed reopening".into(),
        }),
        "reviewer",
        &s,
    )
    .unwrap();
    assert_eq!(reopened["status"], "approved");
    assert_eq!(reopened["events"][0]["principal_id"], "author");
    tx.commit().unwrap();
}
#[test]
fn source_canonical_golden_and_normalization_are_exact() {
    let mut d = disposition("synthetic-golden");
    d.left.role = " Cafe\u{301} ".into();
    let d = d.normalize().unwrap();
    assert_eq!(d.left.role, "Café");
    let value = json!({"\u{e000}":"x","😀":"y","amount":12.34});
    assert_eq!(
        canonical(&value),
        "{\"amount\":12.34,\"😀\":\"y\",\"\u{e000}\":\"x\"}"
    );
    // Independently computed with the TypeScript canonical/Node SHA256 source algorithm.
    assert_eq!(
        hash(
            &json!({"previousHash":null,"dispositionId":"synthetic-golden","type":"approved","payloadHash":"a".repeat(64),"actor":"display:reviewer","principal":{"kind":"user","id":"reviewer"},"at":NOW})
        ),
        "bf9cd17ce69f68e8f83a35895db466df697f98276a4120add96343a9001352ab"
    );
    let mut bad = d.clone();
    bad.amount = "12.345".parse().unwrap();
    assert!(bad.normalize().is_err());
    let mut exact = d;
    exact.amount = "1234e-2".parse().unwrap();
    assert!(exact.normalize().is_ok());
}
#[test]
fn candidate_overflow_and_wrong_heads_refuse_without_partial_events() {
    let mut c = fixture();
    let mut s = snapshots();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let p = proposed(&tx, "synthetic-link", &s);
    let a = approve(&tx, &p, &s);
    let e = exact(&a);
    let before = tx.total_changes();
    assert!(
        run(
            &tx,
            Action::Link(Link {
                disposition_id: e.disposition_id,
                payload_hash: e.payload_hash,
                confirm: true,
                side: "left".into(),
                journal_entry_id: 1,
                ledger_head_hash: Some("f".repeat(64))
            }),
            "reviewer",
            &s
        )
        .is_err()
    );
    assert_eq!(tx.total_changes(), before);
    for id in 2..=201 {
        let mut entry = s[0].entries[0].clone();
        entry.id = id;
        s[0].entries.push(entry);
    }
    assert!(
        run(
            &tx,
            Action::Plan(Plan {
                disposition: disposition("overflow")
            }),
            "author",
            &s
        )
        .is_err()
    );
    tx.rollback().unwrap();
}

#[test]
fn reopened_proposed_supersession_can_link_and_two_connections_keep_one_approval() {
    let mut c = fixture();
    let s = snapshots();
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let p = proposed(&tx, "synthetic-reopened", &s);
    proposed(&tx, "synthetic-replacement", &s);
    let e = exact(&p);
    run(
        &tx,
        Action::Supersede(Supersede {
            disposition_id: e.disposition_id.clone(),
            payload_hash: e.payload_hash.clone(),
            confirm: true,
            replacement_disposition_id: "synthetic-replacement".into(),
            reason: "Reviewed replacement".into(),
        }),
        "reviewer",
        &s,
    )
    .unwrap();
    let reopened = run(
        &tx,
        Action::Reopen(Reopen {
            disposition_id: e.disposition_id,
            payload_hash: e.payload_hash,
            confirm: true,
            reason: "Reviewed reopening".into(),
        }),
        "reviewer",
        &s,
    )
    .unwrap();
    assert_eq!(link(&tx, &reopened, "left", &s)["status"], "partly_posted");
    tx.commit().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("concurrent.sqlite");
    c.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    drop(c);
    let mut first = Connection::open(&path).unwrap();
    let mut second = Connection::open(path).unwrap();
    second.busy_timeout(std::time::Duration::ZERO).unwrap();
    let a = first
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let value = approve(&a, &proposed(&a, "synthetic-concurrent", &s), &s);
    assert!(
        second
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .is_err()
    );
    a.commit().unwrap();
    let b = second
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let changes = b.total_changes();
    assert_eq!(approve(&b, &value, &s), value);
    assert_eq!(b.total_changes(), changes);
    b.commit().unwrap();
}
