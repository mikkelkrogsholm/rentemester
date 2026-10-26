use super::*;
use rentemester_runtime::auth::{AuthError, CookieProfile};
use rentemester_storage::workspace_context::RegisteredCompany;
struct Fixed(i64);
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.0)
    }
}
const NOW: i64 = 1767225600000;
fn fixture() -> (
    tempfile::TempDir,
    Connection,
    Connection,
    CookieVerifier,
    VerifiedCookie,
    Catalog,
) {
    let dir = tempfile::tempdir().unwrap();
    let mut control = Connection::open(dir.path().join("control.sqlite")).unwrap();
    let mut ledger = Connection::open(dir.path().join("ledger.sqlite")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut ledger,
        "Synthetic diagnostics",
    )
    .unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','diagnostic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','b','grant','reader','user:u');").unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let issued = cookies.issue_session("diagnostic-token").unwrap();
    let proof = cookies
        .verify(Some(issued.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    let catalog = Catalog {
        workspace: "w".into(),
        companies: ["a", "b"]
            .into_iter()
            .map(|slug| RegisteredCompany {
                slug: slug.into(),
                name: format!("Synthetic {slug}"),
                purpose: Default::default(), archived: false,
            })
            .collect(),
    };
    (dir, control, ledger, cookies, proof, catalog)
}
#[test]
fn original_diagnostics118_retention_dates_projection_no_backfill_and_current_auth() {
    let (_dir, mut c, mut l, _, p, _) = fixture();
    l.execute_batch("INSERT INTO journal_entries(entry_no,transaction_date,text,rule_version,entry_hash,retain_until)VALUES('SYN-1','2020-01-01','Synthetic retained entry','synthetic','synthetic-hash','2025-12-31');").unwrap();
    let before = l.total_changes();
    for (date, expired) in [("2025-12-31", 0), ("2026-01-01", 1), ("2035-01-01", 1)] {
        let ctx = Context {
            cookie: &p,
            clock: &Fixed(NOW),
            selection: control_fence::CompanySelection {
                workspace: "w",
                slug: "a",
                registered: true,
                archived: false,
            },
        };
        let core = retention_human(&mut c, &mut l, ctx, date).unwrap().value;
        assert_eq!(core["asOf"], date);
        assert_eq!(core["rows"][1]["expired"], expired);
        assert!(core["rows"][1].get("unknown").is_none());
    }
    for date in ["", "2026-02-30", "invalid"] {
        let ctx = Context {
            cookie: &p,
            clock: &Fixed(NOW),
            selection: control_fence::CompanySelection {
                workspace: "w",
                slug: "a",
                registered: true,
                archived: false,
            },
        };
        let core = retention_human(&mut c, &mut l, ctx, date).unwrap().value;
        assert_eq!(
            core,
            json!({"ok":false,"asOf":date,"appliedRules":["DK-BOOKKEEPING-RETENTION-001"],"rows":[],"errors":["asOf must be YYYY-MM-DD"]})
        );
    }
    assert_eq!(l.total_changes(), before);
    // A historical business date cannot revive an expired session.
    let ctx = Context {
        cookie: &p,
        clock: &Fixed(NOW + 86_400_000),
        selection: control_fence::CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        },
    };
    assert!(retention_human(&mut c, &mut l, ctx, "2020-01-01").is_err());
}
#[test]
fn original_diagnostics118_control_only_health_missing_secondary_and_current_scope() {
    let (_dir, mut c, _, cookies, p, cat) = fixture();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    for slug in ["a", "b"] {
        let core = health(
            &mut c,
            &Fixed(NOW),
            Credential::Human(&cookies, &p),
            &cat,
            slug,
            |registered| rentemester_storage::healthcheck::inspect_company(&root.join(registered)),
        )
        .unwrap();
        assert_eq!(core["ok"], false);
        assert!(core["checks"].is_array());
        assert!(core["errors"].is_array());
    }
    let secondary = root.join("b");
    std::fs::create_dir_all(secondary.join("data")).unwrap();
    let ledger = secondary.join("data/ledger.sqlite");
    std::fs::write(&ledger, b"corrupt synthetic SQLite").unwrap();
    let core = health(
        &mut c,
        &Fixed(NOW),
        Credential::Human(&cookies, &p),
        &cat,
        "b",
        |registered| rentemester_storage::healthcheck::inspect_company(&root.join(registered)),
    )
    .unwrap();
    assert_eq!(core["ok"], false);
    assert!(core["schema"].is_object());
    #[cfg(unix)]
    {
        std::fs::remove_file(&ledger).unwrap();
        let outside = root.join("not-ledger");
        std::fs::write(&outside, b"must not follow").unwrap();
        std::os::unix::fs::symlink(&outside, &ledger).unwrap();
        let core = health(
            &mut c,
            &Fixed(NOW),
            Credential::Human(&cookies, &p),
            &cat,
            "b",
            |registered| rentemester_storage::healthcheck::inspect_company(&root.join(registered)),
        )
        .unwrap();
        assert_eq!(core["ok"], false);
        assert_eq!(core["schema"]["status"], "unavailable");
        assert_eq!(std::fs::read(outside).unwrap(), b"must not follow");
    }
    for slug in ["../escape", "unregistered"] {
        assert!(
            health(
                &mut c,
                &Fixed(NOW),
                Credential::Human(&cookies, &p),
                &cat,
                slug,
                |_| panic!("must not inspect unregistered path")
            )
            .is_err()
        );
    }
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','b','revoke',NULL,'user:u');").unwrap();
    assert!(
        health(
            &mut c,
            &Fixed(NOW),
            Credential::Human(&cookies, &p),
            &cat,
            "b",
            |_| panic!("revoked")
        )
        .is_err()
    );
    assert!(
        health(
            &mut c,
            &Fixed(NOW + 86_400_000),
            Credential::Human(&cookies, &p),
            &cat,
            "a",
            |_| panic!("expired")
        )
        .is_err()
    );
    c.execute("DELETE FROM session WHERE id='s'", []).unwrap();
    let stale_codes: Vec<_> = ["a", "unregistered"].into_iter().map(|slug| {
        health(&mut c, &Fixed(NOW), Credential::Human(&cookies, &p), &cat,
            slug, |_| panic!("stale session must not inspect any target"))
            .unwrap_err().error.code
    }).collect();
    assert_eq!(stale_codes[0], stale_codes[1], "stale credentials cannot probe target registration");
    assert!(
        health(
            &mut c,
            &Fixed(NOW),
            Credential::Human(&cookies, &p),
            &cat,
            "a",
            |_| panic!("revoked session")
        )
        .is_err()
    );
    let wrong = CookieVerifier::new(
        "other",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    assert!(
        health(
            &mut c,
            &Fixed(NOW),
            Credential::Human(&wrong, &p),
            &cat,
            "a",
            |_| panic!("wrong verifier")
        )
        .is_err()
    );
}

#[test]
fn original_diagnostics118_service_health_exact_scope_charge_revocation_and_quota() {
    let (_dir, mut c, _, _, _, cat) = fixture();
    let key = ServiceKey::new("w", "synthetic-diagnostic-service-key").unwrap();
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('svc','Synthetic service','svc@example.invalid',1,0,'2026-01-01','2026-01-01');INSERT INTO rm_workspace_service_principals(user_id,display_name,created_at)VALUES('svc','Synthetic service','2026-01-01T00:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('svc','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('svc','b','grant','reader','user:u');").unwrap();
    c.execute("INSERT INTO apikey(id,configId,name,start,referenceId,prefix,key,enabled,rateLimitEnabled,rateLimitTimeWindow,rateLimitMax,requestCount,remaining,createdAt,updatedAt) VALUES('diagnostic-key','workspace-service-principal','Synthetic','rms_sy','svc','rms_',?1,1,1,60000,600,0,2,'2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z')",[key.lookup_hash()]).unwrap();
    assert!(
        health(
            &mut c,
            &Fixed(NOW),
            Credential::Service(&key),
            &cat,
            "a",
            |_| panic!("wrong scope")
        )
        .is_err()
    );
    let usage = |c: &Connection| {
        c.query_row(
            "SELECT remaining,requestCount FROM apikey WHERE id='diagnostic-key'",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
        .unwrap()
    };
    assert_eq!(usage(&c), (2, 0));
    for remaining in [1, 0] {
        let core=health(&mut c,&Fixed(NOW),Credential::Service(&key),&cat,"b",|s|{assert_eq!(s,"b");json!({"ok":false,"errors":["synthetic missing ledger"],"checks":[],"missing":["ledger"],"schema_outdated":false})}).unwrap();
        assert_eq!(core["ok"], false);
        assert_eq!(usage(&c), (remaining, 2 - remaining));
    }
    assert!(
        health(
            &mut c,
            &Fixed(NOW),
            Credential::Service(&key),
            &cat,
            "b",
            |_| panic!("exhausted quota")
        )
        .is_err()
    );
    assert_eq!(usage(&c), (0, 2));
    c.execute(
        "UPDATE apikey SET remaining=2,enabled=0 WHERE id='diagnostic-key'",
        [],
    )
    .unwrap();
    assert!(
        health(
            &mut c,
            &Fixed(NOW),
            Credential::Service(&key),
            &cat,
            "b",
            |_| panic!("revoked key")
        )
        .is_err()
    );
    let wrong = ServiceKey::new("other", "synthetic-diagnostic-service-key").unwrap();
    assert!(
        health(
            &mut c,
            &Fixed(NOW),
            Credential::Service(&wrong),
            &cat,
            "b",
            |_| panic!("wrong workspace")
        )
        .is_err()
    );
}
