//! Physical current-role checks between provider discovery and registration.
//! Synthetic unit fixture only; no provider I/O or ordinary-login qualification.
use super::*;
use serde_json::json;
use app::{Discovery, Request};
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
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
    owner.held.0.as_mut().unwrap().company.as_mut().unwrap().execute("UPDATE companies SET cvr='12345678',name='Synthetic Public Company',address='Synthetic 1',country='DK' WHERE id=1",[]).unwrap();
    (dir, owner, cookies, proof, uid)
}

fn pending(next: Next) -> Admission {
    match next {
        Next::Pending(a) => *a,
        Next::Complete(_) => panic!("pending phase required"),
    }
}
fn revoke(owner: &mut Owner) {
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')",[]).unwrap();
}
#[test]
fn revoked_company_discovery_cannot_admit_registration_post() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let phase = pending(
        owner
            .start_public_einvoice(&proof, &Fixed(NOW), Intent::Setup, "test")
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    assert!(matches!(phase.request(), Request::FindCompany { .. }));
    revoke(&mut owner);
    let result = owner
        .finish_public_einvoice(
            &proof,
            &Fixed(NOW),
            phase,
            Outcome::CompanyFound(Discovery::Missing),
        )
        .unwrap()
        .outcome;
    assert!(result.is_err());
    let count: i64 = owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM digisense_companies", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
#[test]
fn revoked_outbound_discovery_cannot_admit_participant_post() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let phase = pending(
        owner
            .start_public_einvoice(&proof, &Fixed(NOW), Intent::Setup, "test")
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    let phase = pending(
        owner
            .finish_public_einvoice(
                &proof,
                &Fixed(NOW),
                phase,
                Outcome::CompanyFound(Discovery::Found("synthetic-key".into())),
            )
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    assert!(matches!(phase.request(), Request::FindOutbound { .. }));
    revoke(&mut owner);
    assert!(
        owner
            .finish_public_einvoice(
                &proof,
                &Fixed(NOW),
                phase,
                Outcome::OutboundFound(Discovery::Missing)
            )
            .unwrap()
            .outcome
            .is_err()
    );
    let count: i64 = owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM digisense_participants", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

fn issued(owner: &mut Owner) -> (i64, String) {
    use serde_json::json;
    let p = json!({"invoiceType":"full","vatTreatment":"standard","status":"issued","invoiceNumber":"SYN-2026-1","issueDate":"2026-10-20","dueDate":"2026-11-03","currency":"DKK","seller":{"name":"Synthetic Seller","address":"Synthetic Street 1","vatOrCvr":"DK12345678"},"buyer":{"name":"Synthetic Buyer","address":"Synthetic Street 2","eanNumber":"5790000000000","publicRecipient":true},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125}});

    let json = rentemester_runtime::document::Prepared::new(
        "synthetic.json".into(),
        serde_json::to_vec_pretty(&p).unwrap(),
    )
    .unwrap();
    let pdf = rentemester_runtime::document::Prepared::new(
        "synthetic.pdf".into(),
        b"%PDF-1.4 synthetic".to_vec(),
    )
    .unwrap();
    let slots = owner.held.0.as_mut().unwrap();
    let json_path = slots.guard.publish_document(&json).unwrap();
    let pdf_path = slots.guard.publish_document(&pdf).unwrap();
    let tx = slots
        .company
        .as_mut()
        .unwrap()
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let registered = rentemester_storage::sales::register(
        &tx,
        &p,
        rentemester_storage::sales::Artifact {
            filename: json.filename(),
            path: &json_path,
            hash: json.hash(),
            mime: json.mime(),
        },
        rentemester_storage::sales::Artifact {
            filename: pdf.filename(),
            path: &pdf_path,
            hash: pdf.hash(),
            mime: pdf.mime(),
        },
        "user:u",
    )
    .unwrap();
    tx.commit().unwrap();
    (registered["documentId"].as_i64().unwrap(), json_path)
}
#[test]
fn original_artifacts_verify_physical_source_without_provider_and_preserve_duplicate() {
    use serde_json::json;
    let (dir, mut owner, _cookies, proof, _uid) = setup();
    let (id, path) = issued(&mut owner);
    let selector = json!({"documentId":id,"invoiceNumber":"wrong-number-id-wins"});
    for action in [app::OriginalAction::Preview, app::OriginalAction::OioUbl] {
        let r = owner
            .original_public_einvoice(&proof, &Fixed(NOW), action, &selector)
            .unwrap()
            .outcome
            .unwrap()
            .value;
        assert_eq!(r["ok"], true);
        assert_eq!(r["invoiceNumber"], "SYN-2026-1");
        assert!(r["xml"].as_str().unwrap().contains("Synthetic"));
    }
    let body = json!({"documentId":id,"confirm":true,"accessPoint":{"accessPointId":"synthetic","endpointUrl":"metadata:no-network","senderEndpointId":"synthetic"}});
    let first = owner
        .original_public_einvoice(&proof, &Fixed(NOW), app::OriginalAction::Prepare, &body)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(first["duplicate"], false);
    let repeat = owner
        .original_public_einvoice(&proof, &Fixed(NOW), app::OriginalAction::Prepare, &body)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(repeat["duplicate"], true);
    assert_eq!(repeat["envelope"], first["envelope"]);
    // Edit physical bytes only: database provenance alone cannot authorize export.
    std::fs::write(dir.path().join("workspace/a").join(path), b"tampered").unwrap();
    assert!(
        owner
            .original_public_einvoice(&proof, &Fixed(NOW), app::OriginalAction::Preview, &selector)
            .unwrap()
            .outcome
            .is_err()
    );
}
#[test]
fn revoked_authority_cannot_prepare_manual_ack_or_export() {
    use serde_json::json;
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let (id, _) = issued(&mut owner);
    revoke(&mut owner);
    let body = json!({"documentId":id,"confirm":true,"accessPoint":{"accessPointId":"synthetic","endpointUrl":"metadata:no-network","senderEndpointId":"synthetic"},"acknowledgement":{"transmissionId":"manual","acknowledgedAt":"now"}});
    for action in [
        app::OriginalAction::Preview,
        app::OriginalAction::OioUbl,
        app::OriginalAction::Prepare,
    ] {
        assert!(
            owner
                .original_public_einvoice(&proof, &Fixed(NOW), action, &body)
                .unwrap()
                .outcome
                .is_err()
        );
    }
}

fn register_provider(owner: &mut Owner) {
    let slots = owner.held.0.as_mut().unwrap();
    let tx = slots
        .company
        .as_mut()
        .unwrap()
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let identity = rentemester_storage::public_einvoice::identity(&tx).unwrap();
    rentemester_storage::public_einvoice::save_key(
        &tx,
        &identity,
        "test",
        "synthetic-key",
        "user:u",
        "2026-01-01T00:00:00.000Z",
    )
    .unwrap();
    rentemester_storage::public_einvoice::save_outbound(
        &tx,
        &identity,
        "test",
        "synthetic-key",
        true,
        false,
        "user:u",
        "2026-01-01T00:00:00.000Z",
    )
    .unwrap();
    tx.commit().unwrap();
}
fn ack_body(id: i64) -> serde_json::Value {
    serde_json::json!({"documentId":id,"confirm":true,"accessPoint":{"accessPointId":"manual-synthetic","endpointUrl":"metadata:no-network","senderEndpointId":"synthetic"},"acknowledgement":{"transmissionId":"manual","acknowledgedAt":"now"}})
}
#[test]
fn manual_ack_during_held_validation_blocks_delivery_and_reverse_race_is_denied() {
    use serde_json::json;
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let (id, _) = issued(&mut owner);
    register_provider(&mut owner);
    let validation = pending(
        owner
            .start_public_einvoice(
                &proof,
                &Fixed(NOW),
                Intent::OriginalSend(json!({"documentId":id})),
                "test",
            )
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    assert!(matches!(validation.request(), Request::Validate { .. }));
    owner
        .original_public_einvoice(
            &proof,
            &Fixed(NOW),
            app::OriginalAction::Prepare,
            &ack_body(id),
        )
        .unwrap()
        .outcome
        .unwrap();
    let n = owner
        .finish_public_einvoice(&proof, &Fixed(NOW), validation, Outcome::Validated(true))
        .unwrap()
        .outcome;
    assert_eq!(
        n.err().unwrap().error.code,
        "public_einvoice_redelivery_blocked"
    );
    assert!(
        owner
            .start_public_einvoice(&proof, &Fixed(NOW), Intent::Status(id), "test")
            .unwrap()
            .outcome
            .is_err()
    );
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let (id, _) = issued(&mut owner);
    register_provider(&mut owner);
    let validation = pending(
        owner
            .start_public_einvoice(
                &proof,
                &Fixed(NOW),
                Intent::OriginalSend(
                    json!({"invoiceNumber":"SYN-2026-1","companyKey":"synthetic-key"}),
                ),
                "test",
            )
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    let delivery = pending(
        owner
            .finish_public_einvoice(&proof, &Fixed(NOW), validation, Outcome::Validated(true))
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    assert!(matches!(delivery.request(), Request::Deliver { .. }));
    assert!(
        owner
            .original_public_einvoice(
                &proof,
                &Fixed(NOW),
                app::OriginalAction::Prepare,
                &ack_body(id)
            )
            .unwrap()
            .outcome
            .is_err()
    );
}
#[test]
fn original_queued_repetition_observes_only_and_delivered_repetition_is_cached() {
    use serde_json::json;
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let (id, _) = issued(&mut owner);
    register_provider(&mut owner);
    assert!(
        owner
            .start_public_einvoice(
                &proof,
                &Fixed(NOW),
                Intent::OriginalSend(json!({"documentId":id,"companyKey":"wrong-key"})),
                "test"
            )
            .unwrap()
            .outcome
            .is_err()
    );
    let validation = pending(
        owner
            .start_public_einvoice(
                &proof,
                &Fixed(NOW),
                Intent::OriginalSend(json!({"documentId":id})),
                "test",
            )
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    let delivery = pending(
        owner
            .finish_public_einvoice(&proof, &Fixed(NOW), validation, Outcome::Validated(true))
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    let n = owner
        .finish_public_einvoice(
            &proof,
            &Fixed(NOW),
            delivery,
            Outcome::Delivered(Some(app::Observation {
                document_id: Some("provider-id".into()),
                status: "queued-for-delivery".into(),
            })),
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let Next::Complete(first) = n else {
        panic!("first result")
    };
    assert_eq!(first["submission"]["duplicate"], false);
    let observe = pending(
        owner
            .start_public_einvoice(
                &proof,
                &Fixed(NOW),
                Intent::OriginalSend(json!({"documentId":id})),
                "test",
            )
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    assert!(matches!(
        observe.request(),
        Request::Observe {
            document_id: "provider-id",
            ..
        }
    ));
    owner
        .finish_public_einvoice(
            &proof,
            &Fixed(NOW),
            observe,
            Outcome::Observed(Some(app::Observation {
                document_id: Some("provider-id".into()),
                status: "delivered".into(),
            })),
        )
        .unwrap()
        .outcome
        .unwrap();
    let Next::Complete(repeat) = owner
        .start_public_einvoice(
            &proof,
            &Fixed(NOW),
            Intent::OriginalSend(json!({"documentId":id})),
            "test",
        )
        .unwrap()
        .outcome
        .unwrap()
        .value
    else {
        panic!("delivered repeat must be cached")
    };
    assert_eq!(repeat["submission"]["duplicate"], true);
    assert_eq!(repeat["submission"]["status"], "acknowledged");
}

#[test]
fn native_first_delivery_keeps_canonical_duplicate_projection() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let (id, _) = issued(&mut owner);
    register_provider(&mut owner);
    let validation = pending(
        owner
            .start_public_einvoice(&proof, &Fixed(NOW), Intent::Send(id), "test")
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    let delivery = pending(
        owner
            .finish_public_einvoice(&proof, &Fixed(NOW), validation, Outcome::Validated(true))
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    let Next::Complete(first) = owner
        .finish_public_einvoice(
            &proof,
            &Fixed(NOW),
            delivery,
            Outcome::Delivered(Some(app::Observation {
                document_id: Some("native-provider-id".into()),
                status: "unable-to-deliver".into(),
            })),
        )
        .unwrap()
        .outcome
        .unwrap()
        .value
    else {
        panic!("native outcome")
    };
    assert_eq!(first["submission"]["duplicate"], true);
    let Next::Complete(repeated) = owner
        .start_public_einvoice(&proof, &Fixed(NOW), Intent::Send(id), "test")
        .unwrap()
        .outcome
        .unwrap()
        .value
    else {
        panic!("native repeat must be cached")
    };
    assert_eq!(first, repeated);
}

fn advance_pair(
    owner: &mut Owner,
    proof: &VerifiedCookie,
    phase: Admission,
    outcome: Outcome,
) -> Next {
    owner
        .finish_public_einvoice(proof, &Fixed(NOW), phase, outcome)
        .unwrap()
        .outcome
        .unwrap()
        .value
}
#[test]
fn pair_partial_success_survives_refusal_and_repeat_discovers_only_missing_direction() {
    use rentemester_runtime::public_einvoice::{Network, onboarding::Direction};
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let phase = pending(
        owner
            .start_public_einvoice(
                &proof,
                &Fixed(NOW),
                Intent::RegisterPair(Network::Peppol),
                "test",
            )
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    assert!(matches!(phase.request(), Request::FindCompany { .. }));
    let phase = pending(advance_pair(
        &mut owner,
        &proof,
        phase,
        Outcome::CompanyFound(Discovery::Found("synthetic-key".into())),
    ));
    assert!(matches!(
        phase.request(),
        Request::FindDirection {
            network: Network::Peppol,
            direction: Direction::Outbound,
            ..
        }
    ));
    let phase = pending(advance_pair(
        &mut owner,
        &proof,
        phase,
        Outcome::DirectionFound(Discovery::Missing),
    ));
    assert!(matches!(
        phase.request(),
        Request::Direction {
            direction: Direction::Outbound,
            ..
        }
    ));
    let phase = pending(advance_pair(
        &mut owner,
        &proof,
        phase,
        Outcome::Direction(true),
    ));
    assert!(matches!(
        phase.request(),
        Request::FindDirection {
            direction: Direction::Inbound,
            ..
        }
    ));
    let phase = pending(advance_pair(
        &mut owner,
        &proof,
        phase,
        Outcome::DirectionFound(Discovery::Missing),
    ));
    assert!(
        owner
            .finish_public_einvoice(&proof, &Fixed(NOW), phase, Outcome::Direction(false))
            .unwrap()
            .outcome
            .is_err()
    );
    let phase = pending(
        owner
            .start_public_einvoice(
                &proof,
                &Fixed(NOW),
                Intent::RegisterPair(Network::Peppol),
                "test",
            )
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    assert!(matches!(
        phase.request(),
        Request::FindDirection {
            direction: Direction::Inbound,
            ..
        }
    ));
    let complete = advance_pair(
        &mut owner,
        &proof,
        phase,
        Outcome::DirectionFound(Discovery::Found(())),
    );
    match complete {
        Next::Complete(v) => assert_eq!(v["registration"]["technicalReady"], true),
        _ => panic!("complete pair expected"),
    }
    let phase = pending(
        owner
            .start_public_einvoice(
                &proof,
                &Fixed(NOW),
                Intent::RegisterPair(Network::Peppol),
                "test",
            )
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    assert!(matches!(
        phase.request(),
        Request::FindDirection {
            direction: Direction::Inbound,
            ..
        }
    ));
    // Read/discovery cannot produce a POST after authority revocation.
    revoke(&mut owner);
    assert!(
        owner
            .finish_public_einvoice(
                &proof,
                &Fixed(NOW),
                phase,
                Outcome::DirectionFound(Discovery::Missing)
            )
            .unwrap()
            .outcome
            .is_err()
    );
}
#[test]
fn pair_current_identity_and_original_outbound_consent_do_not_imply_receive_consent() {
    use rentemester_runtime::public_einvoice::{Network, onboarding::Direction};
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    register_provider(&mut owner);
    let phase = pending(
        owner
            .start_public_einvoice(
                &proof,
                &Fixed(NOW),
                Intent::RegisterPair(Network::Nemhandel),
                "test",
            )
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    assert!(matches!(
        phase.request(),
        Request::FindDirection {
            direction: Direction::Inbound,
            ..
        }
    ));
    owner
        .held
        .0
        .as_mut()
        .unwrap()
        .company
        .as_mut()
        .unwrap()
        .execute("UPDATE companies SET cvr='87654321' WHERE id=1", [])
        .unwrap();
    assert!(
        owner
            .finish_public_einvoice(
                &proof,
                &Fixed(NOW),
                phase,
                Outcome::DirectionFound(Discovery::Missing)
            )
            .unwrap()
            .outcome
            .is_err()
    );
}

#[test]
fn original_status_observe_only_binding_cache_pending_failed_and_revocation() {
    use serde_json::json;
    for terminal in ["delivered", "unable-to-deliver"] {
        let (_dir, mut owner, _cookies, proof, _uid) = setup();
        let (id, _) = issued(&mut owner);
        register_provider(&mut owner);
        assert!(
            owner
                .start_public_einvoice(
                    &proof,
                    &Fixed(NOW),
                    Intent::OriginalStatus(json!({"documentId":id,"confirm":true})),
                    "test"
                )
                .unwrap()
                .outcome
                .is_err()
        );
        let validation = pending(
            owner
                .start_public_einvoice(&proof, &Fixed(NOW), Intent::Send(id), "test")
                .unwrap()
                .outcome
                .unwrap()
                .value,
        );
        let delivery = pending(
            owner
                .finish_public_einvoice(&proof, &Fixed(NOW), validation, Outcome::Validated(true))
                .unwrap()
                .outcome
                .unwrap()
                .value,
        );
        owner
            .finish_public_einvoice(
                &proof,
                &Fixed(NOW),
                delivery,
                Outcome::Delivered(Some(app::Observation {
                    document_id: Some("provider-id".into()),
                    status: "queued-for-delivery".into(),
                })),
            )
            .unwrap()
            .outcome
            .unwrap();
        assert!(
            owner
                .start_public_einvoice(
                    &proof,
                    &Fixed(NOW),
                    Intent::OriginalStatus(
                        json!({"documentId":id,"confirm":true,"companyKey":"wrong"})
                    ),
                    "test"
                )
                .unwrap()
                .outcome
                .is_err()
        );
        for key in [None, Some(" \t "), Some(" synthetic-key ")] {
            let mut body = json!({"documentId":id,"confirm":true});
            if let Some(key) = key {
                body["companyKey"] = json!(key);
            }
            let observe = pending(
                owner
                    .start_public_einvoice(
                        &proof,
                        &Fixed(NOW),
                        Intent::OriginalStatus(body),
                        "test",
                    )
                    .unwrap()
                    .outcome
                    .unwrap()
                    .value,
            );
            assert!(matches!(
                observe.request(),
                Request::Observe {
                    company_key: "synthetic-key",
                    document_id: "provider-id"
                }
            ));
            let Next::Complete(row) = owner
                .finish_public_einvoice(
                    &proof,
                    &Fixed(NOW),
                    observe,
                    Outcome::Observed(Some(app::Observation {
                        document_id: Some("provider-id".into()),
                        status: "queued-for-delivery".into(),
                    })),
                )
                .unwrap()
                .outcome
                .unwrap()
                .value
            else {
                panic!("one observation only")
            };
            assert_eq!(row["submission"]["status"], "prepared");
        }
        let observe = pending(
            owner
                .start_public_einvoice(
                    &proof,
                    &Fixed(NOW),
                    Intent::OriginalStatus(json!({"documentId":id,"confirm":true})),
                    "test",
                )
                .unwrap()
                .outcome
                .unwrap()
                .value,
        );
        owner
            .finish_public_einvoice(
                &proof,
                &Fixed(NOW),
                observe,
                Outcome::Observed(Some(app::Observation {
                    document_id: Some("provider-id".into()),
                    status: terminal.into(),
                })),
            )
            .unwrap()
            .outcome
            .unwrap();
        let Next::Complete(cached) = owner
            .start_public_einvoice(
                &proof,
                &Fixed(NOW),
                Intent::OriginalStatus(json!({"documentId":id,"confirm":true})),
                "test",
            )
            .unwrap()
            .outcome
            .unwrap()
            .value
        else {
            panic!("terminal must cache")
        };
        assert_eq!(
            cached["submission"]["status"],
            if terminal == "delivered" {
                "acknowledged"
            } else {
                "failed"
            }
        );
    }
    for mutate_source in [false, true] {
        let (dir, mut owner, _cookies, proof, _uid) = setup();
        let (id, path) = issued(&mut owner);
        register_provider(&mut owner);
        let validation = pending(
            owner
                .start_public_einvoice(&proof, &Fixed(NOW), Intent::Send(id), "test")
                .unwrap()
                .outcome
                .unwrap()
                .value,
        );
        let delivery = pending(
            owner
                .finish_public_einvoice(&proof, &Fixed(NOW), validation, Outcome::Validated(true))
                .unwrap()
                .outcome
                .unwrap()
                .value,
        );
        owner
            .finish_public_einvoice(
                &proof,
                &Fixed(NOW),
                delivery,
                Outcome::Delivered(Some(app::Observation {
                    document_id: Some("provider-id".into()),
                    status: "queued-for-delivery".into(),
                })),
            )
            .unwrap()
            .outcome
            .unwrap();
        let observe = pending(
            owner
                .start_public_einvoice(
                    &proof,
                    &Fixed(NOW),
                    Intent::OriginalStatus(json!({"documentId":id,"confirm":true})),
                    "test",
                )
                .unwrap()
                .outcome
                .unwrap()
                .value,
        );
        if mutate_source {
            std::fs::write(dir.path().join("workspace/a").join(path), b"tampered").unwrap();
        } else {
            revoke(&mut owner);
        }
        assert!(
            owner
                .finish_public_einvoice(
                    &proof,
                    &Fixed(NOW),
                    observe,
                    Outcome::Observed(Some(app::Observation {
                        document_id: Some("provider-id".into()),
                        status: "delivered".into()
                    }))
                )
                .unwrap()
                .outcome
                .is_err()
        );
    }
}

#[test]
fn original717_digisense_profile_assertions_status_and_revocation() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let status = owner
        .original717_onboarding_status(&proof, &Fixed(NOW), None)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(status["identity"]["cvr"], "12345678");
    assert_eq!(
        status["identity"]["companyName"],
        "Synthetic Public Company"
    );
    assert_eq!(status["configured"], false);
    assert_eq!(status["ready"], false);
    assert!(!status["blockers"].as_array().unwrap().is_empty());
    let mismatch = owner
        .start_public_einvoice(
            &proof,
            &Fixed(NOW),
            Intent::OriginalRegister(
                json!({"cvr":"87654321","companyName":"Synthetic Public Company"}),
                rentemester_runtime::public_einvoice::Network::Nemhandel,
            ),
            "test",
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert!(matches!(mismatch,Next::Complete(v) if v["ok"]==false));
    let accepted = owner
        .start_public_einvoice(
            &proof,
            &Fixed(NOW),
            Intent::OriginalRegister(
                json!({"cvr":"DK12345678","companyName":" Synthetic Public Company "}),
                rentemester_runtime::public_einvoice::Network::Nemhandel,
            ),
            "test",
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert!(matches!(
        pending(accepted).request(),
        Request::FindCompany { .. }
    ));
    revoke(&mut owner);
    assert!(
        owner
            .original717_onboarding_status(&proof, &Fixed(NOW), Some("test"))
            .unwrap()
            .outcome
            .is_err()
    );
}

#[test]
fn original717_digisense_registration_pair_real_owner_phases() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let network = rentemester_runtime::public_einvoice::Network::Peppol;
    let initial = owner
        .start_public_einvoice(
            &proof,
            &Fixed(NOW),
            Intent::OriginalRegister(
                json!({"cvr":"12345678","companyName":"Synthetic Public Company"}),
                network,
            ),
            "test",
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let mut phase = pending(initial);
    // Controlled fake-provider observations cross the actual Owner workflow.
    for outcome in [
        Outcome::CompanyFound(Discovery::Missing),
        Outcome::Company(Some("synthetic-provider-key".into())),
        Outcome::DirectionFound(Discovery::Missing),
        Outcome::Direction(true),
        Outcome::DirectionFound(Discovery::Missing),
        Outcome::Direction(true),
    ] {
        let next = owner
            .finish_public_einvoice(&proof, &Fixed(NOW), phase, outcome)
            .unwrap()
            .outcome
            .unwrap()
            .value;
        match next {
            Next::Pending(a) => phase = *a,
            Next::Complete(v) => {
                assert_eq!(v["ok"], true);
                break;
            }
        }
    }
    let result = owner
        .original717_registration_result(&proof, &Fixed(NOW), "test", network)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(result["companyKey"], "synthetic-provider-key");
    assert_eq!(
        result["directionsRegistered"],
        json!(["outbound", "inbound"])
    );
    assert_eq!(result["network"], "peppol");
    let status = owner
        .original717_onboarding_status(&proof, &Fixed(NOW), Some("test"))
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(status["companyKeyConfigured"], true);
    assert_eq!(status["inboundReady"], true);
    assert_eq!(status["outboundReady"], true);
    assert_eq!(status["ready"], true);
}
