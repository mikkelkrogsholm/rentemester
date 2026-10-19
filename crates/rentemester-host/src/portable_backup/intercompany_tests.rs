use super::*;
use rentemester_storage::intercompany as source;
use serde_json::json;
fn signed_intercompany(wrong_uuid: bool, corrupt: bool) -> artifact::VerifiedArtifact {
    // Start with the exact pre-intercompany archive shape to retain backward acceptance.
    let old = tests::signed(false, false, false, false, false, None);
    let mut context = old.manifest().context.clone();
    let mut control = Connection::open_in_memory().unwrap();
    let mut second = Connection::open_in_memory().unwrap();
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut second,
        "Synthetic second",
    )
    .unwrap();
    context.companies.push(artifact::Company {
        slug: "synthetic-second".into(),
        ledger_uuid: if wrong_uuid {
            "f".repeat(32)
        } else {
            store::company(&second).unwrap().uuid
        },
        schema_version: 36,
    });
    let registered = context
        .companies
        .iter()
        .map(|c| c.slug.clone())
        .collect::<BTreeSet<_>>();
    let tx = control.transaction().unwrap();
    let m:rentemester_engine::intercompany::Manifest=serde_json::from_value(json!({"version":1,"groups":[{"id":"synthetic-signed","name":"Synthetic signed group","memberships":[{"id":"first","companySlug":"synthetic","validFrom":"2020-01-01"},{"id":"second","companySlug":"synthetic-second","validFrom":"2020-01-01"}],"ownership":[{"id":"owns-second","parentCompanySlug":"synthetic","childCompanySlug":"synthetic-second","basisPoints":10000,"evidenceRefs":["synthetic-source"],"validFrom":"2020-01-01"}]}]})).unwrap();
    let m = m.normalize(&registered).unwrap();
    source::apply_manifest(
        &tx,
        &m,
        None,
        "user:historical-owner",
        "2026-01-01T00:00:00.000Z",
    )
    .unwrap();
    let mapping=serde_json::from_value::<rentemester_engine::intercompany::Mapping>(json!({"id":"synthetic-reviewed","groupId":"synthetic-signed","leftCompanySlug":"synthetic","rightCompanySlug":"synthetic-second","leftAccountNos":["1400"],"rightAccountNos":["2400"],"leftPosition":"receivable","rightPosition":"payable","evidenceRefs":["synthetic-reviewed-original"],"validFrom":"2020-01-01"})).unwrap().normalize(&m).unwrap();
    for (kind, actor) in [
        ("proposed", "user:historical-maker"),
        ("approved", "user:historical-reviewer"),
    ] {
        source::append(
            &tx,
            &source::states(&tx, &m).unwrap(),
            &mapping,
            kind,
            actor,
            "2026-01-01T00:00:00.000Z",
        )
        .unwrap();
    }
    let business = store::export_control(&tx).unwrap();
    tx.rollback().unwrap();
    let mut entries = old
        .entries()
        .iter()
        .map(|e| artifact::Snapshot {
            kind: e.kind().clone(),
            path: e.path().into(),
            bytes: e.bytes().to_vec(),
        })
        .collect::<Vec<_>>();
    entries.iter_mut().find(|e|matches!(e.kind,artifact::EntryKind::WorkspaceManifest)).unwrap().bytes=serde_json::to_vec(&json!({"version":1,"companies":[{"slug":"synthetic","name":"Synthetic backup coverage","archived":false,"createdAt":"2026-01-01T00:00:00.000Z"},{"slug":"synthetic-second","name":"Synthetic second","archived":false,"createdAt":"2026-01-01T00:00:00.000Z"}]})).unwrap();
    entries
        .iter_mut()
        .find(|e| {
            matches!(
                e.kind,
                artifact::EntryKind::ControlBusiness {
                    section: artifact::BusinessSection::WorkspaceAudit
                }
            )
        })
        .unwrap()
        .bytes = business.bytes;
    let mut bytes = business.intercompany_bytes;
    if corrupt {
        let mut v: Value = serde_json::from_slice(&bytes).unwrap();
        v["format"] = json!("unsupported-intercompany");
        bytes = serde_json::to_vec(&v).unwrap();
    }
    entries.push(artifact::Snapshot {
        kind: artifact::EntryKind::CompanyLedger {
            company: "synthetic-second".into(),
        },
        path: "synthetic-second/data/ledger.sqlite".into(),
        bytes: snapshot(&second).unwrap(),
    });
    entries.push(artifact::Snapshot {
        kind: artifact::EntryKind::ControlBusiness {
            section: artifact::BusinessSection::Intercompany,
        },
        path: "control/intercompany.json".into(),
        bytes,
    });
    context
        .coverage
        .unsupported
        .retain(|s| s.section != artifact::BusinessSection::Intercompany);
    context
        .coverage
        .included
        .push(artifact::BusinessSection::Intercompany);
    // Public RFC8032 test key, never deployed/operator material.
    let der = concat!(
        "3051020101300506032b657004220420",
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        "812100",
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
    );
    let der = (0..der.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&der[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let key = artifact::SigningKey::from_pkcs8(&der).unwrap();
    let trust = artifact::TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
    artifact::verify(&artifact::create(context, entries, &key).unwrap(), &trust).unwrap()
}
#[test]
fn signed_intercompany_restores_real_source_chain_reopens_and_keeps_identity_gates() {
    let old = tests::signed(false, false, false, false, false, None);
    validate(&old).unwrap();
    let archive = signed_intercompany(false, false);
    validate(&archive).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic-control.sqlite");
    let mut db = Connection::open(&path).unwrap();
    restore_business(&mut db, &archive, &"a".repeat(64)).unwrap();
    db.close().unwrap();
    let mut db = Connection::open(path).unwrap();
    let tx = db.transaction().unwrap();
    let reg = BTreeSet::from(["synthetic".into(), "synthetic-second".into()]);
    let (m, h) = source::current(&tx, &reg).unwrap().unwrap();
    assert_eq!(m.groups[0].ownership[0].basis_points, 10000);
    assert_eq!(h.len(), 64);
    let states = source::states(&tx, &m).unwrap();
    assert_eq!(states["synthetic-reviewed"].status, "approved");
    assert_eq!(
        states["synthetic-reviewed"].actor,
        "user:historical-reviewer"
    );
    assert_eq!(
        states["synthetic-reviewed"].mapping.evidence_refs,
        ["synthetic-reviewed-original"]
    );
    assert_eq!(tx.query_row("SELECT count(*) FROM workspace_audit WHERE event_type='intercompany_mapping_approved' AND actor='user:historical-reviewer'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    for table in [
        "user",
        "session",
        "twoFactor",
        "apikey",
        "rm_company_membership_events",
        "rm_workspace_user_access_events",
    ] {
        assert_eq!(
            tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert!(
        validate(&signed_intercompany(true, false)).is_err(),
        "signed slug does not replace actual ledger UUID gate"
    );
    assert!(
        validate(&signed_intercompany(false, true)).is_err(),
        "signature does not make an unknown business codec valid"
    );
}
