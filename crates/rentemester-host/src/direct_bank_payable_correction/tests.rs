//! Synthetic UNIT authority rows; actual qualification uses ordinary verification/MFA.
use super::*;
use rentemester_application::direct_bank_payable_correction::Action;
use rentemester_runtime::{
    auth::{CookieProfile, CookieVerifier, SystemClock},
    document::Prepared,
};
use rentemester_storage::direct_bank_payable_correction::{ApplyInput, PlanInput};
use serde_json::{Value, json};
fn call(
    owner: &mut Owner,
    proof: &VerifiedCookie,
    action: Action<'_>,
) -> Result<Value, rentemester_application::control_fence::Failure> {
    owner
        .direct_bank_payable_correction(proof, &SystemClock, action)
        .unwrap()
        .outcome
        .map(|c| c.value)
}
#[test]
fn owned_originals_current_role_and_transfer_fence_cover_effects_and_receipt_replay() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap().join("workspace");
    let uid = String::from_utf8(
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .parse()
    .unwrap();
    let mut owner = Owner::create_with_chart_profile(
        &root,
        uid,
        "w",
        "a",
        "Synthetic correction",
        "2026-10-19T00:00:00.000Z",
        &[],
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-10-19','2026-10-19');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('u','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('u','u','synthetic-token',strftime('%Y-%m-%dT%H:%M:%fZ','now','-1 minute'),strftime('%Y-%m-%dT%H:%M:%fZ','now','-1 minute'),strftime('%Y-%m-%dT%H:%M:%fZ','now','+11 hours'));INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');").unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let c = cookies.issue_session("synthetic-token").unwrap();
    let proof = cookies
        .verify(Some(c.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    owner.held.0.as_mut().unwrap().company.as_mut().unwrap().execute_batch("UPDATE companies SET vat_period_type='quarter' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('A','Synthetic bank','asset','debit'),('X','Synthetic expense','expense','debit'),('V','Synthetic VAT','vat','debit'),('C','Synthetic creditor','liability','credit');INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by)VALUES('bank','A','confirmed',1,'synthetic'),('input_vat','V','confirmed',1,'synthetic'),('creditors','C','confirmed',1,'synthetic');INSERT INTO bank_accounts(slug,name,ledger_account_no)VALUES('synthetic','Synthetic bank','A');").unwrap();
    let file = Prepared::new(
        "synthetic-correction.txt".into(),
        b"Synthetic immutable correction original".to_vec(),
    )
    .unwrap();
    let metadata=serde_json::from_value(json!({"source":"synthetic-host-unit","documentType":"purchase_sale","issueDate":"2026-06-16","deliveryDescription":"Synthetic supplies","invoiceNo":"SYN-58","amountIncVat":125,"vatAmount":25,"sender":{"name":"Synthetic supplier","address":"Synthetic1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic buyer","address":"Synthetic2","vatOrCvr":"DK87654321"}})).unwrap();
    owner
        .ingest_document(
            &proof,
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
    let csv = "date;text;amount\n2026-07-02;Synthetic SYN-58;-125\n";
    owner
        .bank(
            &proof,
            &SystemClock,
            rentemester_application::bank::Action::Import {
                content: csv,
                profile: None,
                account: Some("synthetic"),
                now: "2026-10-19T12:00:00Z",
            },
        )
        .unwrap()
        .outcome
        .unwrap();
    // A valid named statement must not leave an orphan original when its
    // explicitly selected account is missing.
    let refused = owner.bank(&proof, &SystemClock, rentemester_application::bank::Action::Import {
        content: "Dato;Tekst;Beløb;Valuta\n03.04.2026;Synthetic unknown account;-1,00;DKK\n",
        account: Some("missing-synthetic-account"),
        profile: Some("danske-bank"),
        now: "2026-10-19T12:00:00Z",
    }).unwrap().outcome.err().unwrap();
    assert_eq!(refused.error.code, "bank_refused");
    assert_eq!(
        std::fs::read_dir(root.join("a/bank/processed"))
            .unwrap()
            .count(),
        1
    );
    let expense = rentemester_storage::expense::Input {
        document_id: 1,
        bank_transaction_id: 1,
        expense_account_no: "X".into(),
        vat_treatment: Some("standard".into()),
        payment_account_no: None,
        transaction_date: None,
        text: None,
    };
    owner
        .expense(
            &proof,
            &SystemClock,
            rentemester_application::expense::Action::Post(&expense),
        )
        .unwrap()
        .outcome
        .unwrap();
    let input = PlanInput {
        document_id: 1,
        bank_transaction_id: 1,
        bill_date: "2026-06-16".into(),
        due_date: "2026-07-16".into(),
        expense_account_no: "X".into(),
        vat_treatment: Some("standard".into()),
        vendor_id: None,
        note: None,
    };
    let plan = call(&mut owner, &proof, Action::Plan(&input)).unwrap()["plan"].clone();
    let mut body = serde_json::to_value(&input).unwrap();
    body["planHash"] = plan["planHash"].clone();
    body["reason"] = json!("Synthetic reviewed payable correction");
    body["idempotencyKey"] = json!("synthetic-host-correction");
    body["confirm"] = json!(true);
    let apply: ApplyInput = serde_json::from_value(body).unwrap();
    let document = root.join("a").join(file.relative_path());
    std::fs::write(&document, b"changed synthetic original").unwrap();
    assert_eq!(
        call(&mut owner, &proof, Action::Apply(&apply))
            .unwrap_err()
            .error
            .code,
        "document_evidence_unavailable"
    );
    std::fs::write(&document, file.bytes()).unwrap();
    let hash = owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row::<String, _, _>(
            "SELECT source_file_hash FROM bank_transactions WHERE id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let bank = root.join("a/bank/processed").join(format!("{hash}.txt"));
    std::fs::write(&bank, b"changed synthetic archive").unwrap();
    assert_eq!(
        call(&mut owner, &proof, Action::Apply(&apply))
            .unwrap_err()
            .error
            .code,
        "bank_statement_original_unavailable"
    );
    std::fs::write(&bank, csv).unwrap();
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u');").unwrap();
    assert_eq!(
        call(&mut owner, &proof, Action::Apply(&apply))
            .unwrap_err()
            .error
            .code,
        "access_denied"
    );
    let profiled_import = || rentemester_application::bank::Action::Import {
        content: "Dato;Tekst;Beløb;Valuta\n03.04.2026;Synthetic blocked import;-1,00;DKK\n",
        account: Some("synthetic"),
        profile: Some("danske-bank"),
        now: "2026-10-19T12:00:00Z",
    };
    assert_eq!(
        owner
            .bank(&proof, &SystemClock, profiled_import())
            .unwrap()
            .outcome
            .err()
            .unwrap()
            .error
            .code,
        "access_denied"
    );
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');").unwrap();
    let result = call(&mut owner, &proof, Action::Apply(&apply)).unwrap();
    assert_eq!(result["correction"]["idempotent"], false);
    assert_eq!(
        call(&mut owner, &proof, Action::Apply(&apply)).unwrap()["idempotency"]["replayed"],
        true
    );
    let status = owner
        .workspace_transfer_status(&cookies, &SystemClock, &proof)
        .unwrap()
        .outcome
        .unwrap();
    let freeze=serde_json::from_value(json!({"expectedStateHash":status["stateHash"],"destination":"https://synthetic.invalid","reason":"Synthetic correction transfer proof","confirm":true})).unwrap();
    owner
        .freeze_workspace(&cookies, &SystemClock, &proof, &freeze)
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(
        call(&mut owner, &proof, Action::Apply(&apply))
            .unwrap_err()
            .error
            .code,
        "workspace_transfer_fenced"
    );
    assert_eq!(
        owner
            .bank(&proof, &SystemClock, profiled_import())
            .unwrap()
            .outcome
            .err()
            .unwrap()
            .error
            .code,
        "workspace_transfer_fenced"
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
            .query_row::<i64, _, _>("SELECT count(*) FROM bank_transactions", [], |r| r.get(0))
            .unwrap(),
        1
    );
    assert_eq!(
        std::fs::read_dir(root.join("a/bank/processed"))
            .unwrap()
            .count(),
        1
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
        4
    );
}
