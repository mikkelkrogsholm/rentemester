//! Synthetic identity rows are UNIT fixtures; actual acceptance uses ordinary two-user MFA.
use super::*;
use rentemester_application::bookkeeping_batch::Action;
use rentemester_engine::bookkeeping_batch::{Exact, Persist, Scope};
use rentemester_runtime::{
    auth::{CookieProfile, CookieVerifier, SystemClock},
    document::Prepared,
};
use serde_json::{Value, json};
fn call(
    o: &mut Owner,
    p: &VerifiedCookie,
    a: Action<'_>,
) -> Result<Value, rentemester_application::control_fence::Failure> {
    o.bookkeeping_batch(p, &SystemClock, a)
        .unwrap()
        .outcome
        .map(|c| c.value)
}
#[test]
fn real_namespace_originals_independent_review_current_revocation_and_transfer_gate() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap().join("workspace");
    let uid = std::process::Command::new("id").arg("-u").output().unwrap();
    let uid = String::from_utf8(uid.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let mut owner = Owner::create_with_chart_profile(
        &root,
        uid,
        "w",
        "a",
        "Synthetic batch",
        "2026-10-19T00:00:00.000Z",
        &[],
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let mut proofs = vec![];
    for name in ["author", "reviewer"] {
        owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch(&format!("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('{name}','Synthetic','{name}@example.invalid',1,1,'2026-10-19','2026-10-19');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('{name}','synthetic','[]','{name}',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('{name}','{name}','{name}-token',strftime('%Y-%m-%dT%H:%M:%fZ','now','-1 minute'),strftime('%Y-%m-%dT%H:%M:%fZ','now','-1 minute'),strftime('%Y-%m-%dT%H:%M:%fZ','now','+11 hours'));INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('{name}','activate','workspace_owner','user:{name}');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('{name}','a','grant','owner','user:{name}');")).unwrap();
        let c = cookies.issue_session(&format!("{name}-token")).unwrap();
        proofs.push(
            cookies
                .verify(Some(c.set_cookie().split(';').next().unwrap()), None)
                .unwrap(),
        );
    }
    owner.held.0.as_mut().unwrap().company.as_mut().unwrap().execute_batch("UPDATE companies SET vat_period_type='quarter' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('A','Synthetic bank','asset','debit'),('X','Synthetic expense','expense','debit'),('V','Synthetic VAT','vat','debit');INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by)VALUES('input_vat','V','confirmed',1,'synthetic');INSERT INTO bank_accounts(slug,name,ledger_account_no)VALUES('synthetic','Synthetic bank','A');").unwrap();
    for (n, amount, vat) in [(1, 125, 25), (2, 250, 50)] {
        let file = Prepared::new(
            format!("synthetic-{n}.txt"),
            format!("Synthetic immutable original {n}").into_bytes(),
        )
        .unwrap();
        let metadata=serde_json::from_value(json!({"source":"synthetic-host-unit","documentType":"purchase_sale","issueDate":"2026-10-18","deliveryDescription":"Synthetic supplies","invoiceNo":format!("SYN-{n}"),"amountIncVat":amount,"vatAmount":vat,"sender":{"name":"Synthetic supplier","address":"Synthetic street1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic buyer","address":"Synthetic street2","vatOrCvr":"DK87654321"}})).unwrap();
        owner
            .ingest_document(
                &proofs[0],
                &SystemClock,
                rentemester_application::documents::Ingest {
                    file: &file,
                    metadata: &metadata,
                    today: "2026-10-19",
                },
            )
            .unwrap()
            .outcome
            .unwrap();
    }
    let csv = "date;text;amount\n2026-10-18;Synthetic purchase SYN-1;-125\n2026-10-18;Synthetic purchase SYN-2;-250\n";
    owner
        .bank(
            &proofs[0],
            &SystemClock,
            rentemester_application::bank::Action::Import {
                content: csv,
                account: Some("synthetic"),
                now: "2026-10-19T12:00:00Z",
            },
        )
        .unwrap()
        .outcome
        .unwrap();
    let proposal=serde_json::from_value(json!({"confirm":true,"proposal":{"ruleId":"batch-unit","effectiveFrom":"2000-01-01","conditions":{"currency":"DKK","supplierIdentity":"Synthetic supplier","supplierCountry":"DK","supplierVat":"DK12345678","documentType":"purchase_sale","vat":"positive"},"outcome":{"account":"X","vatTreatment":"standard"},"provenance":"synthetic unit","rationale":"Synthetic explicit rule"}})).unwrap();
    let c = owner
        .reviewed_bookkeeping(
            &proofs[0],
            &SystemClock,
            rentemester_application::reviewed_bookkeeping::Action::Rule {
                action: "propose",
                input: &proposal,
            },
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let review=serde_json::from_value(json!({"confirm":true,"ruleId":"batch-unit","version":1,"expectedPayloadHash":c["payloadHash"],"rationale":"Independent synthetic review","provenance":"synthetic unit"})).unwrap();
    owner
        .reviewed_bookkeeping(
            &proofs[1],
            &SystemClock,
            rentemester_application::reviewed_bookkeeping::Action::Rule {
                action: "approve",
                input: &review,
            },
        )
        .unwrap()
        .outcome
        .unwrap();
    let scope = Scope {
        accounting_from: "2026-10-01".into(),
        accounting_to: "2026-10-31".into(),
        bank_from: "2026-10-01".into(),
        bank_to: "2026-10-31".into(),
        company_id: None,
    };
    let plan = call(&mut owner, &proofs[0], Action::Plan(&scope)).unwrap()["plan"].clone();
    assert_eq!(
        plan["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["partition"] == "ready")
            .count(),
        2
    );
    let input:Persist=serde_json::from_value(json!({"accountingFrom":"2026-10-01","accountingTo":"2026-10-31","bankFrom":"2026-10-01","bankTo":"2026-10-31","runKey":"host-real-proof","expectedPlanHash":plan["planHash"],"confirm":true})).unwrap();
    let run = call(&mut owner, &proofs[0], Action::Persist(&input)).unwrap();
    let exact = Exact {
        run_id: run["runId"].as_i64().unwrap(),
        plan_hash: plan["planHash"].as_str().unwrap().into(),
        confirm: true,
    };
    assert!(call(&mut owner, &proofs[0], Action::Approve(&exact)).is_err());
    // Missing physical original is a definite refusal BEFORE any item effect.
    let original = owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row::<String, _, _>("SELECT stored_path FROM documents WHERE id=2", [], |r| {
            r.get(0)
        })
        .unwrap();
    let absolute = root.join("a").join(&original);
    let bytes = std::fs::read(&absolute).unwrap();
    std::fs::write(&absolute, b"changed synthetic original").unwrap();
    assert_eq!(
        call(&mut owner, &proofs[1], Action::Approve(&exact))
            .unwrap_err()
            .error
            .code,
        "bookkeeping_batch_original_unavailable"
    );

    assert_eq!(
        call(&mut owner, &proofs[0], Action::Apply(&exact))
            .unwrap_err()
            .error
            .code,
        "bookkeeping_batch_original_unavailable"
    );
    std::fs::write(&absolute, bytes).unwrap();
    let source: String = owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row(
            "SELECT source_file_hash FROM bank_transactions WHERE id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let bank_path = root.join("a/bank/processed").join(format!("{source}.txt"));
    let bank_bytes = std::fs::read(&bank_path).unwrap();
    std::fs::write(&bank_path, b"changed synthetic bank archive").unwrap();
    assert_eq!(
        call(&mut owner, &proofs[1], Action::Approve(&exact))
            .unwrap_err()
            .error
            .code,
        "bookkeeping_batch_bank_original_unavailable"
    );

    assert_eq!(
        call(&mut owner, &proofs[0], Action::Apply(&exact))
            .unwrap_err()
            .error
            .code,
        "bookkeeping_batch_bank_original_unavailable"
    );
    std::fs::write(&bank_path, bank_bytes).unwrap();
    assert_eq!(
        owner
            .held
            .0
            .as_ref()
            .unwrap()
            .company
            .as_ref()
            .unwrap()
            .query_row::<i64, _, _>(
                "SELECT count(*) FROM bookkeeping_batch_revision_approvals",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        0
    );
    call(&mut owner, &proofs[1], Action::Approve(&exact)).unwrap();

    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('author','a','revoke',NULL,'user:reviewer');").unwrap();
    assert!(call(&mut owner, &proofs[0], Action::Apply(&exact)).is_err());
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('author','a','grant','owner','user:reviewer');").unwrap();
    let applied = call(&mut owner, &proofs[0], Action::Apply(&exact)).unwrap();
    assert_eq!(applied["results"].as_array().unwrap().len(), 2);
    let status = owner
        .workspace_transfer_status(&cookies, &SystemClock, &proofs[0])
        .unwrap()
        .outcome
        .unwrap();
    let freeze=serde_json::from_value(json!({"expectedStateHash":status["stateHash"],"destination":"https://synthetic.invalid","reason":"Synthetic transfer gate proof","confirm":true})).unwrap();
    owner
        .freeze_workspace(&cookies, &SystemClock, &proofs[0], &freeze)
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(
        call(&mut owner, &proofs[0], Action::Apply(&exact))
            .unwrap_err()
            .error
            .code,
        "workspace_transfer_fenced"
    );
    assert!(
        call(
            &mut owner,
            &proofs[0],
            Action::StatusByKey("host-real-proof")
        )
        .is_ok()
    );
    assert_eq!(
        owner
            .held
            .0
            .as_ref()
            .unwrap()
            .company
            .as_ref()
            .unwrap()
            .query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        2
    );
}
