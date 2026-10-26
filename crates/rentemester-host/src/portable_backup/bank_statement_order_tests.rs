//! Actual signed source-free SQLite restoration retains immutable bank chronology.
use super::*;
#[test]
fn signed_bank_order_restore_and_reopen_preserve_closing_balance_and_original() {
    const CSV:&[u8]=b"date,text,amount,currency,balance_after\n2026-10-24,final,-100,DKK,100\n2026-10-24,middle,100,DKK,200\n2026-10-24,opening,1,DKK,100\n";
    let old = tests::signed(false, false, false, false, false, None);
    let bytes = old
        .entries()
        .iter()
        .find(|e| matches!(e.kind(), artifact::EntryKind::CompanyLedger { .. }))
        .unwrap()
        .bytes();
    let mut source = Connection::open_in_memory().unwrap();
    source
        .deserialize_read_exact(MAIN_DB, bytes, bytes.len(), false)
        .unwrap();
    let hash = rentemester_runtime::backup_keys::digest(CSV);
    let path = format!("bank/processed/{hash}.txt");
    {
        let tx = source
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let rows = rentemester_engine::bank::parse(std::str::from_utf8(CSV).unwrap()).unwrap();
        let result = rentemester_storage::bank::import_ordered_bytes_with_principal(
            &tx,
            CSV,
            &rows,
            None,
            "synthetic",
            "2026-10-26T00:00:00Z",
            &path,
            rentemester_storage::idempotency::PrincipalKind::User,
            false,
            Some("descending"),
        )
        .unwrap();
        assert_eq!(result["imported"], 3);
        tx.commit().unwrap();
    }
    let mut entries: Vec<_> = old
        .entries()
        .iter()
        .map(|e| artifact::Snapshot {
            kind: e.kind().clone(),
            path: e.path().into(),
            bytes: if matches!(e.kind(), artifact::EntryKind::CompanyLedger { .. }) {
                source.serialize(MAIN_DB).unwrap().to_vec()
            } else {
                e.bytes().to_vec()
            },
        })
        .collect();
    entries.push(artifact::Snapshot {
        kind: artifact::EntryKind::BankStatement {
            company: "synthetic".into(),
        },
        path: format!("synthetic/{path}"),
        bytes: CSV.to_vec(),
    });
    let signed = tests::verified(old.manifest().context.clone(), entries);
    drop(source);
    validate(&signed).unwrap();
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("restored.sqlite");
    let bytes = signed
        .entries()
        .iter()
        .find(|e| matches!(e.kind(), artifact::EntryKind::CompanyLedger { .. }))
        .unwrap()
        .bytes();
    std::fs::write(&file, bytes).unwrap();
    let mut restored = Connection::open(&file).unwrap();
    let digest = rentemester_runtime::backup_keys::digest(signed.manifest_bytes());
    store::restore_boundary(&mut restored, &digest).unwrap();
    store::company(&restored).unwrap();
    for _ in 0..2 {
        let tx = restored
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let view =
            rentemester_storage::bank::read(&tx, "synthetic", "2026-10-26", Some("2026")).unwrap();
        assert_eq!(view["actualBalance"], 100.0);
        assert_eq!(view["difference"], -100.0);
        assert!(view["transactions"].as_array().unwrap().iter().all(|r| {
            r["statementOrder"] == "descending"
                && r["statementOrderProvenance"]
                    .as_str()
                    .unwrap()
                    .contains(&hash)
        }));
        tx.commit().unwrap();
        restored.close().unwrap();
        restored = Connection::open(&file).unwrap();
    }
    assert!(
        signed.entries().iter().any(|e| matches!(
            e.kind(),
            artifact::EntryKind::BankStatement { .. }
        ) && e.bytes() == CSV)
    );
}
