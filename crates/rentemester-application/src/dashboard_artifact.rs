//! Standalone HTML uses the same current authority and physical sources as native reports.
use crate::{
    access::{CompanyRole, Permission, company_role_allows},
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{Error, dashboard_html::Fact};
use rentemester_runtime::namespace::NamespaceGuard;
use rusqlite::Connection;

pub struct Request<'a> {
    pub as_of: &'a str,
    pub generated_at: &'a str,
    pub build_revision: &'a str,
    pub rule_bundle_version: &'a str,
}

pub struct Export {
    pub html: String,
    /// Only pure rendering, excluding source collection and transport.
    pub render_ms: f64,
}

pub fn read(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    request: Request<'_>,
) -> Result<Committed<Export>, Failure> {
    let workspace = context.selection.workspace;
    let slug = context.selection.slug;
    let clock = context.clock;
    control_fence::execute_cookie_both(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyRead,
        true,
        false,
        |control, company, subject| {
            if !crate::periods::verify_sources(company, guard)?.is_empty() {
                return Err(Error::new(
                    "report_source_unavailable",
                    "Dashboard source assurance differs",
                ));
            }
            crate::archive_import::verify_physical(company, guard)?;
            let mut snapshot = rentemester_storage::dashboard_artifact::read(
                company,
                slug,
                request.as_of,
                request.generated_at,
                request.build_revision,
                request.rule_bundle_version,
            )?;
            // The surrounding fence already resolves current identity and CompanyRead.
            // A current role observation under that same writer gates the privileged
            // retained-backup subsection; a reader never receives admin-only details.
            let role =
                rentemester_storage::control_access::observe_company_role(control, subject, slug)?;
            let admin = role
                .map(|role| serde_json::from_value::<CompanyRole>(serde_json::Value::String(role)))
                .transpose()
                .map_err(|_| Error::new("invalid_control", "Invalid company role"))?
                .is_some_and(|role| company_role_allows(role, Permission::CompanyAdmin));
            if admin {
                let status =
                    crate::backup_status::data(control, company, workspace, slug, clock, true)?;
                let system = snapshot
                    .sections
                    .iter_mut()
                    .find(|section| section.id == "system")
                    .ok_or_else(|| Error::new("dashboard_unavailable", "Missing system section"))?;
                // Replace the storage projection's permission placeholder only
                // after checking the current admin role on this same fence.
                system.notes.retain(|note| {
                    !note.starts_with(
                        "Ikke tilgængelig: Backupoplysninger kræver selskabsadministrator;",
                    )
                });
                system.next_action = None;
                system.facts.extend([
                    Fact {
                        label: "Seneste verificerede lokale backup (aktuel status)".into(),
                        value: status["backup"]["latestBackupAt"]
                            .as_str()
                            .unwrap_or("Ingen verificeret lokal backup")
                            .into(),
                    },
                    Fact {
                        label: "Dage siden seneste verificerede backup".into(),
                        value: status["backup"]["daysSinceLatestBackup"]
                            .as_u64()
                            .map(|days| days.to_string())
                            .unwrap_or_else(|| "Ikke tilgængelig".into()),
                    },
                    Fact {
                        label: "Backup påkrævet".into(),
                        value: if status["backup"]["backupDue"] == true {
                            "Ja"
                        } else {
                            "Nej"
                        }
                        .into(),
                    },
                    Fact {
                        label: "Ændringer siden backup".into(),
                        value: if status["backup"]["hasActivitySinceBackup"] == true {
                            "Ja"
                        } else {
                            "Nej"
                        }
                        .into(),
                    },
                ]);
                if let Some(errors) = status["backup"]["errors"].as_array() {
                    system
                        .notes
                        .extend(errors.iter().filter_map(|e| e.as_str()).map(str::to_owned));
                }
                system.notes.push("Backupkontrollen gælder lokalt bevarede signerede snapshots ved genereringen; ekstern synkronisering er ikke verificeret.".into());
            }
            let start = std::time::Instant::now();
            let html = rentemester_engine::dashboard_html::render(&snapshot)?;
            let render_ms = start.elapsed().as_secs_f64() * 1000.0;
            Ok(Export { html, render_ms })
        },
    )
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::control_fence::CompanySelection;
    use rentemester_runtime::{
        auth::{AuthError, Clock, CookieProfile, CookieVerifier, VerifiedCookie},
        document::Prepared,
        namespace::NamespaceProfile,
    };
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1_767_225_600_000)
        }
    }
    fn context<'a>(cookie: &'a VerifiedCookie, slug: &'a str) -> Context<'a> {
        Context {
            cookie,
            clock: &Fixed,
            selection: CompanySelection {
                workspace: "w",
                slug,
                registered: true,
                archived: false,
            },
        }
    }
    fn request() -> Request<'static> {
        Request {
            as_of: "2026-01-01",
            generated_at: "2026-01-01T00:00:00.000Z",
            build_revision: "synthetic-build",
            rule_bundle_version: "synthetic-rules",
        }
    }
    #[test]
    fn html_preserves_read_effects_current_access_admin_backup_and_physical_evidence() {
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
        let mut guard = NamespaceGuard::create_fresh_with_profile(
            &root,
            uid,
            "w",
            "a",
            "Synthetic HTML <company>",
            "2026-01-01T00:00:00.000Z",
            NamespaceProfile::LocalUserMacos,
        )
        .unwrap();
        let mut control =
            Connection::open(root.join(".rentemester/workspace-control.sqlite")).unwrap();
        let mut company = Connection::open(root.join("a/data/ledger.sqlite")).unwrap();
        for db in [&control, &company] {
            db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
                .unwrap();
        }
        rentemester_storage::fresh::initialize_empty_pair(
            &mut control,
            &mut company,
            "Synthetic HTML <company>",
        )
        .unwrap();
        control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u');").unwrap();
        let verifier = CookieVerifier::new(
            "w",
            CookieProfile::Development,
            "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
        )
        .unwrap();
        let issued = verifier.issue_session("synthetic-token").unwrap();
        let cookie = verifier
            .verify(Some(issued.request_header()), None)
            .unwrap();
        let before = (control.total_changes(), company.total_changes());
        let first = read(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie, "a"),
            request(),
        )
        .unwrap()
        .value;
        assert!(first.html.contains("Synthetic HTML &lt;company&gt;"));
        assert!(
            !first
                .html
                .contains("Seneste verificerede lokale backup (aktuel status)")
        );
        assert_eq!(
            first.html,
            read(
                &mut control,
                &mut company,
                &mut guard,
                context(&cookie, "a"),
                request()
            )
            .unwrap()
            .value
            .html
        );
        assert_eq!(before, (control.total_changes(), company.total_changes()));
        assert_eq!(
            read(
                &mut control,
                &mut company,
                &mut guard,
                context(&cookie, "other"),
                request()
            )
            .err()
            .unwrap()
            .error
            .code,
            "access_denied"
        );
        control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u')").unwrap();
        let before = (control.total_changes(), company.total_changes());
        let owner = read(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie, "a"),
            request(),
        )
        .unwrap()
        .value;
        assert!(
            owner
                .html
                .contains("Seneste verificerede lokale backup (aktuel status)")
        );
        assert!(owner.html.contains("Ingen verificeret lokal backup"));
        assert!(!owner.html.contains("Ikke tilgængelig: Backupoplysninger"));
        assert_eq!(before, (control.total_changes(), company.total_changes()));
        let original = Prepared::new(
            "synthetic-html.txt".into(),
            b"Synthetic HTML report source".to_vec(),
        )
        .unwrap();
        let metadata = serde_json::from_value(
            serde_json::json!({"source":"synthetic-html","documentType":"cash_register_receipt"}),
        )
        .unwrap();
        let doc = crate::documents::ingest(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie, "a"),
            crate::documents::Ingest {
                file: &original,
                metadata: &metadata,
                today: "2026-01-01",
            },
        )
        .unwrap()
        .value;
        let path: String = company
            .query_row(
                "SELECT stored_path FROM documents WHERE id=?1",
                [doc["document"]["id"].as_i64().unwrap()],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            read(
                &mut control,
                &mut company,
                &mut guard,
                context(&cookie, "a"),
                request()
            )
            .is_ok()
        );
        std::fs::write(root.join("a").join(path), b"Changed synthetic source").unwrap();
        let before = (control.total_changes(), company.total_changes());
        assert_eq!(
            read(
                &mut control,
                &mut company,
                &mut guard,
                context(&cookie, "a"),
                request()
            )
            .err()
            .unwrap()
            .error
            .code,
            "report_source_unavailable"
        );
        assert_eq!(before, (control.total_changes(), company.total_changes()));
        control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES('u','a','revoke','user:u')").unwrap();
        assert_eq!(
            read(
                &mut control,
                &mut company,
                &mut guard,
                context(&cookie, "a"),
                request()
            )
            .err()
            .unwrap()
            .error
            .code,
            "access_denied"
        );
    }
}
