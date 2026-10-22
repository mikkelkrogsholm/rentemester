//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_application::recurring_workspace::Action;
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
use serde_json::json;
struct Fixed(i64);
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.0)
    }
}
const NOW: i64 = 1767225600000;
fn setup() -> (
    tempfile::TempDir,
    Owner,
    CookieVerifier,
    VerifiedCookie,
    u32,
) {
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
        "Synthetic A",
        "2026-01-01T00:00:00.000Z",
        &[],
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','owner-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');").unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let cookie = cookies.issue_session("owner-token").unwrap();
    let proof = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    (dir, owner, cookies, proof, uid)
}

const AT: &str = "2026-01-01T00:00:00Z";
fn input() -> rentemester_storage::recurring_invoices::original::Create {
    serde_json::from_value(json!({"name":"Synthetic mixed recurring","interval":"monthly","deliveryChannel":"digisense","firstIssueDate":"2025-11-30","deliveryPeriodMode":"interval_window","invoice":{"invoiceType":"full","seller":{"name":"Synthetic frozen seller","address":"Frozen1","vatOrCvr":"DK87654321"},"buyer":{"name":"Synthetic frozen buyer","address":"Frozen2"},"lines":[{"description":"Taxed","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100,"taxClassification":"taxable","vatRate":0.25},{"description":"Exempt","quantity":1,"unitPriceExVat":20,"lineTotalExVat":20,"taxClassification":"exempt"}],"totals":{"netAmount":120,"vatAmount":25,"grossAmount":145}},"confirm":true})).unwrap()
}
fn call(
    owner: &mut Owner,
    proof: &VerifiedCookie,
    action: Action<'_>,
) -> Result<serde_json::Value, rentemester_application::control_fence::Failure> {
    owner
        .recurring_workspace(proof, &Fixed(NOW), action, "2026-01-01", AT)
        .unwrap()
        .outcome
        .map(|v| v.value)
}
fn prepare_profile(owner: &mut Owner) {
    owner.held.0.as_mut().unwrap().company.as_mut().unwrap().execute_batch("UPDATE companies SET address='Synthetic1',cvr='87654321',vat_period_type='quarter' WHERE id=1").unwrap();
}

fn create(owner: &mut Owner, proof: &VerifiedCookie, channel: &str) -> i64 {
    let mut input = input();
    input.delivery_channel = Some(channel.to_owned());
    owner
        .recurring_invoices(
            proof,
            &Fixed(NOW),
            rentemester_application::recurring_invoices::Action::OriginalCreate(&input),
            "2026-01-01",
            AT,
        )
        .unwrap()
        .outcome
        .unwrap()
        .value["templateId"]
        .as_i64()
        .unwrap()
}
#[test]
fn oldest_gaps_are_capped_exact_and_use_same_original_numbering() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    prepare_profile(&mut owner);
    let catalog = owner
        .recurring_workspace_catalog(&cookies, &proof, &Fixed(NOW), "w")
        .unwrap();
    assert!(!catalog.retired);
    assert_eq!(
        catalog.outcome.unwrap()["companies"][0]["initialized"],
        true
    );
    assert!(
        owner
            .recurring_workspace_catalog(
                &cookies,
                &proof,
                &Fixed(NOW),
                "/not-the-configured-workspace"
            )
            .unwrap()
            .outcome
            .is_err()
    );
    let id = create(&mut owner, &proof, "manual");
    let plan = call(
        &mut owner,
        &proof,
        Action::Plan {
            as_of: "2026-01-01",
            max: 1,
        },
    )
    .unwrap();
    assert_eq!(plan["initialMissing"], 2);
    assert_eq!(plan["missing"], json!([{"templateId":id,"periodIndex":0}]));
    let first = call(
        &mut owner,
        &proof,
        Action::Generate {
            template_id: id,
            period_index: 0,
            as_of: "2026-01-01",
        },
    )
    .unwrap();
    assert_eq!(first["created"], true);
    let plan = call(
        &mut owner,
        &proof,
        Action::Plan {
            as_of: "2026-01-01",
            max: 1,
        },
    )
    .unwrap();
    assert_eq!(plan["initialMissing"], 1);
    assert_eq!(plan["missing"][0]["periodIndex"], 1);
    let second = call(
        &mut owner,
        &proof,
        Action::Generate {
            template_id: id,
            period_index: 1,
            as_of: "2026-01-01",
        },
    )
    .unwrap();
    assert_eq!(second["created"], true);
    assert_ne!(first["invoiceNumber"], second["invoiceNumber"]);
    assert!(
        call(
            &mut owner,
            &proof,
            Action::Generate {
                template_id: id,
                period_index: 2,
                as_of: "2026-01-01"
            }
        )
        .is_err()
    );
    assert_eq!(
        call(
            &mut owner,
            &proof,
            Action::Plan {
                as_of: "2026-01-01",
                max: 1
            }
        )
        .unwrap()["initialMissing"],
        0
    );
    assert_eq!(
        call(
            &mut owner,
            &proof,
            Action::Generate {
                template_id: id,
                period_index: 0,
                as_of: "2026-01-01"
            }
        )
        .unwrap()["created"],
        false
    );
    let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM documents", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        4
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')",[]).unwrap();
    assert!(
        call(
            &mut owner,
            &proof,
            Action::Plan {
                as_of: "2026-01-01",
                max: 1
            }
        )
        .is_err()
    );
}
#[test]
fn delivery_attempts_are_typed_nonreplayable_and_public_ack_cannot_be_forged() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    prepare_profile(&mut owner);
    let id = create(&mut owner, &proof, "email");
    let generated = call(
        &mut owner,
        &proof,
        Action::Generate {
            template_id: id,
            period_index: 0,
            as_of: "2026-01-01",
        },
    )
    .unwrap();
    let plan = call(
        &mut owner,
        &proof,
        Action::Plan {
            as_of: "2026-01-01",
            max: 1,
        },
    )
    .unwrap();
    let generation = plan["deliveries"][0]["generationId"].as_i64().unwrap();
    assert_eq!(plan["deliveries"][0]["documentId"], generated["documentId"]);
    assert_eq!(
        plan["deliveries"][0]["emailRecipient"],
        serde_json::Value::Null
    );
    call(
        &mut owner,
        &proof,
        Action::Reserve {
            generation_id: generation,
        },
    )
    .unwrap();
    assert!(
        call(
            &mut owner,
            &proof,
            Action::Reserve {
                generation_id: generation
            }
        )
        .is_err()
    );
    assert!(
        call(
            &mut owner,
            &proof,
            Action::Event {
                generation_id: generation,
                event_type: "acknowledged",
                provider_id: None,
                message: "email acknowledged"
            }
        )
        .is_err()
    );
    call(
        &mut owner,
        &proof,
        Action::Event {
            generation_id: generation,
            event_type: "acknowledged",
            provider_id: None,
            message: "email dry-run completed; no actual delivery",
        },
    )
    .unwrap();
    let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    rentemester_storage::recurring_workspace::validate_history(db).unwrap();
    assert_eq!(db.query_row("SELECT count(*) FROM recurring_invoice_delivery_events WHERE message LIKE 'rentemester-recurring/1: %'",[],|r|r.get::<_,i64>(0)).unwrap(),2);
    let id = create(&mut owner, &proof, "digisense");
    call(
        &mut owner,
        &proof,
        Action::Generate {
            template_id: id,
            period_index: 0,
            as_of: "2026-01-01",
        },
    )
    .unwrap();
    let plan = call(
        &mut owner,
        &proof,
        Action::Plan {
            as_of: "2026-01-01",
            max: 2,
        },
    )
    .unwrap();
    let generation = plan["deliveries"][0]["generationId"].as_i64().unwrap();
    assert_eq!(plan["deliveries"][0]["canObserve"], false);
    call(
        &mut owner,
        &proof,
        Action::Reserve {
            generation_id: generation,
        },
    )
    .unwrap();
    assert!(
        call(
            &mut owner,
            &proof,
            Action::Event {
                generation_id: generation,
                event_type: "acknowledged",
                provider_id: Some("forged-provider-id"),
                message: "public acknowledged"
            }
        )
        .is_err()
    );
    assert!(
        call(
            &mut owner,
            &proof,
            Action::Reserve {
                generation_id: generation
            }
        )
        .is_err()
    );
    let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM peppol_submissions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn admitted_catalog_preserves_archived_and_uninitialized_without_creating_them() {
    let (dir, owner, cookies, proof, uid) = setup();
    let root = dir.path().canonicalize().unwrap().join("workspace");
    owner.try_close().unwrap();
    let manifest = root.join("workspace.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    value["companies"].as_array_mut().unwrap().extend([
        json!({"slug":"b-archived","name":"Synthetic archived","createdAt":"2026-01-01T00:00:00.000Z","archived":true}),
        json!({"slug":"c-missing","name":"Synthetic uninitialized","createdAt":"2026-01-01T00:00:00.000Z","archived":false}),
    ]);
    std::fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
    let mut owner =
        Owner::open_with_profile(&root, uid, "w", "a", NamespaceProfile::LocalUserMacos).unwrap();
    let attempt = owner
        .recurring_workspace_catalog(&cookies, &proof, &Fixed(NOW), "w")
        .unwrap();
    assert!(!attempt.retired);
    let companies = attempt.outcome.unwrap()["companies"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(companies.len(), 3);
    for (slug, archived, initialized) in [
        ("a", false, true),
        ("b-archived", true, false),
        ("c-missing", false, false),
    ] {
        let row = companies.iter().find(|row| row["slug"] == slug).unwrap();
        assert_eq!(row["archived"], archived);
        assert_eq!(row["initialized"], initialized);
    }
    assert!(!root.join("b-archived").exists());
    assert!(!root.join("c-missing").exists());
}

#[test]
fn opted_in_overdue_backup_blocks_plan_generation_and_delivery_before_effects() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    prepare_profile(&mut owner);
    let template = create(&mut owner, &proof, "digisense");
    call(
        &mut owner,
        &proof,
        Action::Generate {
            template_id: template,
            period_index: 0,
            as_of: "2026-01-01",
        },
    )
    .unwrap();
    let generation = {
        let slots = owner.held.0.as_mut().unwrap();
        let control = slots.control.as_mut().unwrap().transaction().unwrap();
        let company = slots.company.as_mut().unwrap().transaction().unwrap();
        company.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('B','Synthetic bank','asset','debit');INSERT INTO bank_accounts(id,slug,name,currency,ledger_account_no)VALUES(1,'synthetic','Synthetic','DKK','B');INSERT INTO bank_transactions(transaction_date,text,amount,currency,bank_account_id)VALUES('2025-12-01','Synthetic old activity',1,'DKK',1)").unwrap();
        rentemester_storage::backup_state::configure(
            &control, &company, "a", true, 0, "user:u", AT,
        )
        .unwrap();
        let generation: i64 = company
            .query_row(
                "SELECT id FROM recurring_invoice_generations WHERE template_id=?1",
                [template],
                |row| row.get(0),
            )
            .unwrap();
        company.commit().unwrap();
        control.commit().unwrap();
        generation
    };
    let counts = |owner: &Owner| {
        let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
        [
            "documents",
            "recurring_invoice_generations",
            "recurring_invoice_generation_claims",
            "recurring_invoice_delivery_events",
            "journal_entries",
            "audit_log",
        ]
        .map(|table| {
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
        })
    };
    let before = counts(&owner);
    for action in [
        Action::Plan {
            as_of: "2026-01-01",
            max: 2,
        },
        Action::Generate {
            template_id: template,
            period_index: 1,
            as_of: "2026-01-01",
        },
        Action::Reserve {
            generation_id: generation,
        },
        Action::Event {
            generation_id: generation,
            event_type: "preflight_failed",
            provider_id: None,
            message: "synthetic preflight",
        },
    ] {
        assert_eq!(
            call(&mut owner, &proof, action).unwrap_err().error.code,
            "backup_overdue"
        );
        assert_eq!(counts(&owner), before);
    }
}
