//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
use rentemester_runtime::contact_import::{Lookup, LookupError};
use serde_json::Value;
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
    let input = serde_json::from_value(json!({"slug":"b","name":"Synthetic B"})).unwrap();
    assert!(
        owner
            .create_company(&cookies, &Fixed(NOW), &proof, &input)
            .unwrap()
            .outcome
            .is_ok()
    );
    owner.select_company("a").unwrap();
    (dir, owner, cookies, proof, uid)
}
struct Provider {
    calls: usize,
    control: std::path::PathBuf,
    company: std::path::PathBuf,
    revoke: bool,
    fail: bool,
}
impl Lookup for Provider {
    fn lookup(&mut self, _: &str) -> Result<Value, LookupError> {
        self.calls += 1;
        // Trusted unit fixture only: separate handles prove neither SQL writer spans provider I/O.
        for path in [&self.control, &self.company] {
            let db = Connection::open(path).unwrap();
            db.busy_timeout(std::time::Duration::from_millis(50))
                .unwrap();
            db.execute_batch("BEGIN IMMEDIATE;ROLLBACK").unwrap();
        }
        if self.revoke {
            let db = Connection::open(&self.control).unwrap();
            db.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:synthetic-revoker');").unwrap();
        }
        if self.fail {
            return Err(LookupError::MissingConfiguration);
        }
        Ok(
            json!({"cvrNummer":12345678,"virksomhedMetadata":{"nyesteNavn":{"navn":"Synthetic provider company"},"nyesteBeliggenhedsadresse":{"vejnavn":"Synthetic road","husnummerFra":3,"postnummer":1234,"postdistrikt":"Synthetic town"}},"elektroniskPost":[{"kontaktoplysning":"synthetic@example.invalid","periode":{"gyldigTil":null}}]}),
        )
    }
}
fn provider(o: &Owner) -> Provider {
    Provider {
        calls: 0,
        control: o.root.join(".rentemester/workspace-control.sqlite"),
        company: o.root.join("a/data/ledger.sqlite"),
        revoke: false,
        fail: false,
    }
}
fn input(content: &str, enrich: bool) -> rentemester_engine::contact_import::Input {
    serde_json::from_value(
        json!({"fileName":"synthetic.csv","content":content,"enrichCvr":enrich,"confirm":true}),
    )
    .unwrap()
}
fn counts(o: &Owner) -> (i64, i64, i64, i64) {
    let db = o.held.0.as_ref().unwrap().company.as_ref().unwrap();
    let n = |table: &str| {
        db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    };
    (
        n("customers"),
        n("vendors"),
        n("cvr_lookups"),
        n("journal_entries"),
    )
}
#[test]
fn authenticated_provider_is_sql_free_csv_wins_fresh_cache_and_exact_repeat_skip() {
    let (_dir, mut o, _cookies, p, _) = setup();
    let mut lookup = provider(&o);
    let content = "\u{feff}Kontaktnavn;CVR-nummer;Kontakttype;Landekode;Total salg;Total køb;Betalingsfrist i dage;EAN-nummer\r\n\"Synthetic; both\";12345678;;DK;1;1;400;1234567890123\r\n";
    let request = input(content, true);
    let result = o
        .import_contacts(&p, &Fixed(NOW), &request, &mut lookup)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(lookup.calls, 1);
    assert_eq!(result["import"]["summary"]["customersCreated"], 1);
    assert_eq!(result["import"]["summary"]["vendorsCreated"], 1);
    assert_eq!(counts(&o), (1, 1, 1, 0));
    let db = o.held.0.as_ref().unwrap().company.as_ref().unwrap();
    let name: String = db
        .query_row("SELECT name FROM customers", [], |r| r.get(0))
        .unwrap();
    assert_eq!(name, "Synthetic; both");
    let doubled: i64 = db
        .query_row(
            "SELECT count(*) FROM audit_log WHERE actor LIKE 'user:user:%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(doubled, 0);
    let audit: i64 = db
        .query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0))
        .unwrap();
    let repeated = o
        .import_contacts(&p, &Fixed(NOW), &request, &mut lookup)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(repeated["import"]["summary"]["skipped"], 2);
    assert_eq!(lookup.calls, 1);
    assert_eq!(counts(&o), (1, 1, 1, 0));
    let after: i64 = o
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(audit, after);
    let cached = input(
        "Kontaktnavn;CVR-nummer;Kontakttype;Landekode;Total salg\nSynthetic other;12345678;;DK;1\n",
        true,
    );
    o.import_contacts(&p, &Fixed(NOW), &cached, &mut lookup)
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(lookup.calls, 1);
    assert_eq!(counts(&o), (2, 1, 1, 0));
}
#[test]
fn current_revocation_after_provider_and_foreign_temp_schema_refuse_without_mutation() {
    let (_dir, mut o, _cookies, p, _) = setup();
    let mut lookup = provider(&o);
    lookup.revoke = true;
    let request = input(
        "Kontaktnavn;CVR-nummer;Kontakttype;Landekode;Total salg\nSynthetic revoke;12345678;;DK;1\n",
        true,
    );
    let failure = o
        .import_contacts(&p, &Fixed(NOW), &request, &mut lookup)
        .unwrap()
        .outcome
        .unwrap_err();
    assert_eq!(failure.error.code, "access_denied");
    assert_eq!(lookup.calls, 1);
    assert_eq!(counts(&o), (0, 0, 0, 0));
    o.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:synthetic');").unwrap();
    lookup.revoke = false;
    let db = o.held.0.as_ref().unwrap().company.as_ref().unwrap();
    let before: i64 = db
        .query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0))
        .unwrap();
    db.execute_batch("CREATE TEMP TRIGGER synthetic_contact_storage_fault BEFORE INSERT ON customers WHEN NEW.name='Synthetic fail' BEGIN SELECT RAISE(ABORT,'synthetic database failure');END;").unwrap();
    let request = input(
        "Kontaktnavn;CVR-nummer;Kontakttype;Total salg\nSynthetic good;;;1\nSynthetic fail;;;1\n",
        false,
    );
    let failure = o
        .import_contacts(&p, &Fixed(NOW), &request, &mut lookup)
        .unwrap()
        .outcome
        .unwrap_err();
    // Owner preflight rejects foreign TEMP objects before any publication. The storage
    // suite separately injects an unexpected second-row INSERT error to test SQL rollback.
    assert_eq!(failure.error.code, "connection_refused");
    assert_eq!(counts(&o), (0, 0, 0, 0));
    let after: i64 = o
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(lookup.calls, 1);
}
#[test]
fn missing_provider_configuration_keeps_original_csv_and_counts_attempted_roles() {
    let (_dir, mut o, _cookies, p, _) = setup();
    let mut lookup = provider(&o);
    lookup.fail = true;
    let request = input(
        "Kontaktnavn;CVR-nummer;Kontakttype;Landekode;Total salg;Total køb\nSynthetic fallback;12345678;;DK;1;1\n",
        true,
    );
    let result = o
        .import_contacts(&p, &Fixed(NOW), &request, &mut lookup)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(result["import"]["summary"]["enrichmentFailures"], 2);
    assert_eq!(result["import"]["summary"]["enriched"], 0);
    assert!(
        result["import"]["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e.as_str().unwrap().contains("CVR_USERNAME"))
    );
    assert_eq!(counts(&o), (1, 1, 0, 0));
}
