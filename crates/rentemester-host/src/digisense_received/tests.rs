//! Physical current-role checks between provider discovery and registration.
//! Synthetic unit fixture only; no provider I/O or ordinary-login qualification.
use super::*;
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
    owner.held.0.as_mut().unwrap().company.as_mut().unwrap().execute("UPDATE companies SET cvr='12345678',name='Synthetic Public Company',address='Synthetic 1',country='DK' WHERE id=1",[]).unwrap();
    {
        let db = owner.held.0.as_mut().unwrap().company.as_mut().unwrap();
        let tx = db.transaction().unwrap();
        let i = rentemester_storage::public_einvoice::identity(&tx).unwrap();
        rentemester_storage::public_einvoice::save_key(
            &tx,
            &i,
            "test",
            "synthetic-key",
            "u",
            "2026-01-01T00:00:00.000Z",
        )
        .unwrap();
        tx.commit().unwrap();
    }
    (dir, owner, cookies, proof, uid)
}

const XML:&[u8]=br#"<Invoice><ID>IN-711</ID><IssueDate>2026-01-01</IssueDate><AccountingSupplierParty><Party><Name>Synthetic Supplier</Name><StreetName>Synthetic 2</StreetName><CompanyID>DK87654321</CompanyID></Party></AccountingSupplierParty><AccountingCustomerParty><Party><Name>Synthetic Public Company</Name><StreetName>Synthetic 1</StreetName><CompanyID>DK12345678</CompanyID></Party></AccountingCustomerParty><PayableAmount>125.00</PayableAmount><TaxAmount>25.00</TaxAmount></Invoice>"#;
fn prepare(o: &mut Owner, p: &VerifiedCookie) -> Admission {
    o.prepare_digisense_received(p, &Fixed(NOW), "test", [1; 32], None)
        .unwrap()
        .outcome
        .unwrap()
        .value
}
fn check(o: &mut Owner, p: &VerifiedCookie, a: Admission, id: &str) -> Checked {
    o.check_digisense_received(p, &Fixed(NOW), a, "test", [1; 32], json!({"internalId":id}))
        .unwrap()
        .outcome
        .unwrap()
        .value
}
fn finish(o: &mut Owner, p: &VerifiedCookie, a: Admission, xml: &[u8]) -> Finished {
    o.finish_digisense_received(
        p,
        &Fixed(NOW),
        a,
        "test",
        [1; 32],
        xml.to_vec(),
        Value::Null,
        false,
    )
    .unwrap()
    .outcome
    .unwrap()
    .value
}
fn count(o: &Owner, table: &str) -> i64 {
    o.held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
#[test]
fn admitted_original_exact_dedup_quarantine_and_reopen() {
    let (_dir, mut o, _cookies, p, uid) = setup();
    let a = prepare(&mut o, &p);
    let c = check(&mut o, &p, a, "one");
    assert!(!c.known);
    let f = finish(&mut o, &p, c.admission, XML);
    assert_eq!(f.document["status"], "ingested");
    assert!(
        f.document["documentNo"]
            .as_str()
            .unwrap()
            .starts_with("DOC-2026-")
    );
    let db = o.held.0.as_ref().unwrap().company.as_ref().unwrap();
    let (path, sha, payload): (String, String, String) = db
        .query_row(
            "SELECT stored_path,sha256_hash,payload_json FROM documents",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    let file =
        rentemester_runtime::document::Prepared::new("received.xml".into(), XML.to_vec()).unwrap();
    assert_eq!(sha, file.hash());
    assert_eq!(std::fs::read(o.root.join("a").join(path)).unwrap(), XML);
    let meta: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(meta["source"], "digisense_modtag");
    assert_eq!(meta["invoiceNo"], "IN-711");
    assert_eq!(meta["vatAmount"], 25.0);
    let c = check(&mut o, &p, f.admission, "one");
    assert!(c.known);
    let c = check(&mut o, &p, c.admission, "duplicate-content");
    let f = finish(&mut o, &p, c.admission, XML);
    assert_eq!(f.document["status"], "quarantined");
    let c = check(&mut o, &p, f.admission, "malformed");
    let f = finish(&mut o, &p, c.admission, b"<Invoice>");
    assert_eq!(f.document["status"], "quarantined");
    let c = check(&mut o, &p, f.admission, "missing");
    let f = finish(&mut o, &p, c.admission, b"<Invoice/>");
    assert_eq!(f.document["status"], "quarantined");
    let c = check(&mut o, &p, f.admission, "malformed");
    assert!(c.known);
    let c = check(&mut o, &p, c.admission, "transient");
    drop(c);
    assert_eq!(count(&o, "digisense_received_documents"), 4);
    assert_eq!(count(&o, "documents"), 1);
    assert_eq!(count(&o, "journal_entries"), 0);
    let db = o.held.0.as_ref().unwrap().company.as_ref().unwrap();
    let audits:i64=db.query_row("SELECT count(*) FROM audit_log WHERE event_type IN ('digisense_document_received','digisense_document_quarantined')",[],|r|r.get(0)).unwrap();
    assert_eq!(audits, 4);
    let noncanonical: i64 = db.query_row("SELECT count(*) FROM audit_log WHERE event_type IN ('document_ingest','digisense_document_received','digisense_document_quarantined') AND actor!='user:u'", [], |r| r.get(0)).unwrap();
    assert_eq!(
        noncanonical, 0,
        "document and receive/quarantine audits bind the same canonical current user"
    );
    let held = prepare(&mut o, &p);
    let root = o.root.clone();
    drop(o);
    let mut o =
        Owner::open_with_profile(&root, uid, "w", "a", NamespaceProfile::LocalUserMacos).unwrap();
    assert!(matches!(
        o.check_digisense_received(
            &p,
            &Fixed(NOW),
            held,
            "test",
            [1; 32],
            json!({"internalId":"one"})
        ),
        Err(OwnerError::Context)
    ));
    let a = prepare(&mut o, &p);
    assert!(check(&mut o, &p, a, "one").known);
    let a = prepare(&mut o, &p);
    assert!(check(&mut o, &p, a, "malformed").known);
    assert_eq!(count(&o, "documents"), 1);
}
#[test]
fn held_generation_legal_identity_and_authority_refuse_without_quarantine() {
    for mode in [
        "generation",
        "generation-back",
        "identity",
        "identity-back",
        "revoke",
        "wrong-recipient",
        "backup",
        "wrong-owner",
    ] {
        let (_dir, mut o, _cookies, p, _uid) = setup();
        let a = prepare(&mut o, &p);
        let c = check(&mut o, &p, a, "held");
        let mut generation = [1; 32];
        match mode {
            "generation" | "generation-back" => generation = [3; 32], // new live-client epoch, even when config values return to A
            "identity" => {
                o.held
                    .0
                    .as_ref()
                    .unwrap()
                    .company
                    .as_ref()
                    .unwrap()
                    .execute("UPDATE companies SET name='Changed'", [])
                    .unwrap();
            }
            "identity-back" => {
                o.held.0.as_ref().unwrap().company.as_ref().unwrap().execute_batch("UPDATE companies SET name='Changed'; UPDATE companies SET name='Synthetic Public Company';").unwrap();
            }
            "revoke" => {
                o.held.0.as_ref().unwrap().control.as_ref().unwrap().execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')",[]).unwrap();
            }
            "backup" => {
                o.backup_status(
                    &p,
                    &Fixed(NOW),
                    rentemester_application::backup_status::Action::Configure {
                        enforced: true,
                        grace_days: 0,
                    },
                )
                .unwrap()
                .outcome
                .unwrap();
            }
            "wrong-owner" => {
                o.received_epoch = std::sync::Arc::new(());
            }
            _ => {}
        }
        let xml = if mode == "wrong-recipient" {
            String::from_utf8(XML.to_vec())
                .unwrap()
                .replace("DK12345678", "DK11111111")
                .into_bytes()
        } else {
            XML.to_vec()
        };
        let r = o.finish_digisense_received(
            &p,
            &Fixed(NOW),
            c.admission,
            "test",
            generation,
            xml,
            Value::Null,
            false,
        );
        assert!(r.is_err() || r.unwrap().outcome.is_err(), "{mode}");
        assert_eq!(count(&o, "documents"), 0, "{mode}");
        assert_eq!(count(&o, "digisense_received_documents"), 0, "{mode}");
        assert_eq!(count(&o, "journal_entries"), 0, "{mode}");
    }
}

#[test]
fn before_download_current_authority_and_generation_are_required() {
    for mode in ["generation", "revoke", "backup"] {
        let (_dir, mut o, _cookies, p, _uid) = setup();
        let a = prepare(&mut o, &p);
        if mode == "revoke" {
            o.held.0.as_ref().unwrap().control.as_ref().unwrap().execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')",[]).unwrap();
        }
        if mode == "backup" {
            o.backup_status(
                &p,
                &Fixed(NOW),
                rentemester_application::backup_status::Action::Configure {
                    enforced: true,
                    grace_days: 0,
                },
            )
            .unwrap()
            .outcome
            .unwrap();
        }
        let generation = if mode == "generation" {
            [2; 32]
        } else {
            [1; 32]
        };
        assert!(
            o.check_digisense_received(
                &p,
                &Fixed(NOW),
                a,
                "test",
                generation,
                json!({"internalId":"not-downloaded"})
            )
            .unwrap()
            .outcome
            .is_err()
        );
        assert_eq!(count(&o, "digisense_received_documents"), 0);
        assert_eq!(count(&o, "documents"), 0);
        assert_eq!(count(&o, "journal_entries"), 0);
    }
}

#[test]
fn workspace_receive_preflight_is_configuration_independent_readonly_and_current() {
    for mode in ["allowed", "revoked", "backup"] {
        let (_dir, mut o, _cookies, p, _uid) = setup();
        // Host preflight takes no operator client, generation, or registration input.
        if mode == "revoked" {
            o.held.0.as_ref().unwrap().control.as_ref().unwrap().execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')",[]).unwrap();
        }
        if mode == "backup" {
            o.backup_status(
                &p,
                &Fixed(NOW),
                rentemester_application::backup_status::Action::Configure {
                    enforced: true,
                    grace_days: 0,
                },
            )
            .unwrap()
            .outcome
            .unwrap();
        }
        let company_changes = o
            .held
            .0
            .as_ref()
            .unwrap()
            .company
            .as_ref()
            .unwrap()
            .total_changes();
        let control_changes = o
            .held
            .0
            .as_ref()
            .unwrap()
            .control
            .as_ref()
            .unwrap()
            .total_changes();
        let a = o.preflight_digisense_received(&p, &Fixed(NOW)).unwrap();
        assert!(!a.retired);
        assert_eq!(a.outcome.is_ok(), mode == "allowed", "{mode}");
        assert_eq!(
            o.held
                .0
                .as_ref()
                .unwrap()
                .company
                .as_ref()
                .unwrap()
                .total_changes(),
            company_changes
        );
        assert_eq!(
            o.held
                .0
                .as_ref()
                .unwrap()
                .control
                .as_ref()
                .unwrap()
                .total_changes(),
            control_changes
        );
        assert_eq!(count(&o, "documents"), 0);
        assert_eq!(count(&o, "journal_entries"), 0);
    }
}
