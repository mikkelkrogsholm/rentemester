//! Queue reads and review-only decisions under current company authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    AgentSuggestions,
    AgentDecision(
        i64,
        rentemester_storage::exceptions::AgentDecision,
        Option<&'a str>,
    ),
    List(&'a str),
    ListWithArchived(&'a str, bool),
    Resolve(i64, &'a rentemester_storage::exceptions::Resolve),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    let permission = match action {
        Action::AgentSuggestions | Action::List(_) | Action::ListWithArchived(..) => {
            Permission::CompanyRead
        }
        Action::AgentDecision(..) | Action::Resolve(..) => Permission::CompanyReview,
    };
    control_fence::execute_cookie_subject_mode(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission,
        matches!(
            action,
            Action::AgentSuggestions | Action::List(_) | Action::ListWithArchived(..)
        ),
        |tx, actor| match action {
            Action::AgentSuggestions => {
                rentemester_storage::exceptions::agent_suggestions(tx, slug)
            }
            Action::AgentDecision(id, decision, note) => {
                rentemester_storage::exceptions::decide_agent_suggestion(
                    tx,
                    id,
                    decision,
                    note,
                    rentemester_storage::exceptions::Principal::Human(actor),
                )
            }
            Action::List(status) => rentemester_storage::exceptions::list(tx, slug, status),
            Action::ListWithArchived(status, include_archived) => {
                rentemester_storage::exceptions::list_with_archived(
                    tx,
                    slug,
                    status,
                    include_archived,
                )
            }
            Action::Resolve(id, input) => {
                rentemester_storage::exceptions::resolve(tx, id, input, actor)
            }
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_fence::CompanySelection;
    use rentemester_runtime::auth::{AuthError, Clock, CookieProfile, CookieVerifier};
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1_767_225_600_000)
        }
    }
    #[test]
    fn agent_suggestions_and_exceptions_require_current_review_and_preserve_revocation() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Connection::open(dir.path().join("c.sqlite")).unwrap();
        let mut l = Connection::open(dir.path().join("l.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Review")
            .unwrap();
        c.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reader','agent:synthetic');").unwrap();
        l.execute_batch("INSERT INTO exceptions(id,type,severity,message) VALUES(1,'SYNTHETIC_REVIEW_REQUIRED','high','Synthetic exception review fixture'),(2,'AGENT_UNKNOWN','low','Synthetic controlled Agent fixture');").unwrap();
        let fixture: Value = serde_json::from_str(include_str!(
            "../../rentemester-runtime/fixtures/session-cookie.json"
        ))
        .unwrap();
        let cookie =
            CookieVerifier::new("w", CookieProfile::Secure, fixture["key"].as_str().unwrap())
                .unwrap()
                .verify(fixture["vectors"][0]["header"].as_str(), None)
                .unwrap();
        let context = |slug| Context {
            cookie: &cookie,
            clock: &Fixed,
            selection: CompanySelection {
                workspace: "w",
                slug,
                registered: true,
                archived: false,
            },
        };
        assert_eq!(
            execute(&mut c, &mut l, context("a"), Action::AgentSuggestions)
                .unwrap()
                .value["count"],
            1
        );
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                Action::AgentDecision(
                    2,
                    rentemester_storage::exceptions::AgentDecision::Approve,
                    None
                )
            )
            .is_err()
        );
        let input = rentemester_storage::exceptions::Resolve { note: None };
        let before = l.total_changes();
        assert_eq!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                Action::ListWithArchived("all", true)
            )
            .unwrap()
            .value["count"],
            2
        );
        assert_eq!(l.total_changes(), before);
        assert_eq!(
            execute(&mut c, &mut l, context("a"), Action::List("open"))
                .unwrap()
                .value["count"],
            2
        );
        assert!(execute(&mut c, &mut l, context("a"), Action::Resolve(1, &input)).is_err());
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','bookkeeper','agent:synthetic');").unwrap();
        assert!(execute(&mut c, &mut l, context("a"), Action::Resolve(1, &input)).is_err());
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reviewer','agent:synthetic');").unwrap();
        assert!(execute(&mut c, &mut l, context("other"), Action::Resolve(1, &input)).is_err());
        assert_eq!(
            execute(&mut c, &mut l, context("a"), Action::Resolve(1, &input))
                .unwrap()
                .value["resolved"],
            true
        );
        assert_eq!(
            execute(&mut c, &mut l, context("a"), Action::Resolve(1, &input))
                .unwrap()
                .value["resolved"],
            false
        );
        assert_eq!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                Action::AgentDecision(
                    2,
                    rentemester_storage::exceptions::AgentDecision::Approve,
                    Some("review only")
                )
            )
            .unwrap()
            .value["resolved"],
            true
        );
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','revoke',NULL,'agent:synthetic');").unwrap();
        assert!(execute(&mut c, &mut l, context("a"), Action::Resolve(1, &input)).is_err());
        assert!(
            execute(
                &mut c,
                &mut l,
                context("a"),
                Action::ListWithArchived("all", true)
            )
            .is_err()
        );
        assert!(execute(&mut c, &mut l, context("a"), Action::List("open")).is_err());
        assert!(execute(&mut c, &mut l, context("a"), Action::AgentSuggestions).is_err());
        assert_eq!(l.query_row("SELECT count(*) FROM audit_log WHERE event_type='exception_resolve' AND actor='user:u'",[],|r|r.get::<_,i64>(0)).unwrap(),2);
        assert_eq!(
            l.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

/// Original source projection, using native effective archive classification.
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    context: crate::service_company::Context<'_>,
    action: Action<'_>,
) -> Result<crate::service_company::ServiceResult, crate::service_company::ServiceFailure> {
    let mutation = matches!(action, Action::AgentDecision(..) | Action::Resolve(..));
    let permission = if mutation {
        Permission::CompanyReview
    } else {
        Permission::CompanyRead
    };
    let slug = context.selection.slug;
    crate::service_company::execute_operation(
        control,
        company,
        context,
        permission,
        mutation,
        |tx, subject, _| {
            let result = match action {
                Action::AgentSuggestions => {
                    rentemester_storage::exceptions::agent_suggestions(tx, slug)
                }
                Action::AgentDecision(id, decision, note) => {
                    rentemester_storage::exceptions::decide_agent_suggestion(
                        tx,
                        id,
                        decision,
                        note,
                        rentemester_storage::exceptions::Principal::ServiceAccount(subject),
                    )
                }
                Action::List(status) => rentemester_storage::exceptions::list(tx, slug, status),
                Action::ListWithArchived(status, archived) => {
                    rentemester_storage::exceptions::list_with_archived(tx, slug, status, archived)
                }
                Action::Resolve(id, input) => rentemester_storage::exceptions::resolve_principal(
                    tx,
                    id,
                    input,
                    rentemester_storage::exceptions::Principal::ServiceAccount(subject),
                ),
            }?;
            Ok(crate::service_company::OperationResult {
                result,
                receipt: None,
            })
        },
    )
}

#[cfg(test)]
mod original_tests;

#[cfg(test)]
mod agent_suggestions_service_tests {
    use super::*;
    use crate::control_fence::CompanySelection;
    use rentemester_runtime::{
        auth::{AuthError, Clock, CookieProfile, CookieVerifier},
        service_key::ServiceKey,
    };
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1767225600000)
        }
    }
    #[test]
    fn agent_suggestions_current_service_reader_reviewer_revoked() {
        let dir = tempfile::tempdir().unwrap();
        let mut control = Connection::open(dir.path().join("control.sqlite")).unwrap();
        let mut company = Connection::open(dir.path().join("company.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(
            &mut control,
            &mut company,
            "Synthetic Exceptions",
        )
        .unwrap();
        control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');").unwrap();
        let cookies = CookieVerifier::new(
            "w",
            CookieProfile::Development,
            "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
        )
        .unwrap();
        let cookie = cookies.issue_session("synthetic-token").unwrap();
        let proof = cookies
            .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
            .unwrap();
        let receipt = crate::service_lifecycle::execute(
            &mut control,
            &cookies,
            &Fixed,
            &proof,
            &crate::service_lifecycle::Operation::Create {
                display_name: "Synthetic reader".into(),
                operation_id: None,
                expires_in_seconds: None,
            },
        )
        .unwrap();
        let issued: Value = serde_json::from_str(receipt.as_json()).unwrap();
        let subject = issued["serviceAccountId"].as_str().unwrap().to_owned();
        let credential = issued["credentialId"].as_str().unwrap().to_owned();
        let key = ServiceKey::new("w", issued["secret"].as_str().unwrap()).unwrap();
        control.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')", [&subject]).unwrap();
        control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','reader','user:u')", [&subject]).unwrap();

        company.execute_batch("INSERT INTO exceptions(id,type,severity,message) VALUES(1,'AGENT_UNKNOWN','high','Synthetic controlled Agent fixture');").unwrap();
        let context = || crate::service_company::Context {
            key: &key,
            clock: &Fixed,
            max_future_days: 45,
            selection: CompanySelection {
                workspace: "w",
                slug: "a",
                registered: true,
                archived: false,
            },
        };
        assert_eq!(
            execute_service(
                &mut control,
                &mut company,
                context(),
                Action::AgentSuggestions
            )
            .unwrap()
            .result["count"],
            1
        );
        assert!(
            execute_service(
                &mut control,
                &mut company,
                context(),
                Action::AgentDecision(
                    1,
                    rentemester_storage::exceptions::AgentDecision::Reject,
                    Some(" first service note ")
                )
            )
            .is_err()
        );
        control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?1,'a','grant','bookkeeper','user:u')", [&subject]).unwrap();
        assert!(
            execute_service(
                &mut control,
                &mut company,
                context(),
                Action::AgentDecision(
                    1,
                    rentemester_storage::exceptions::AgentDecision::Reject,
                    Some(" first service note ")
                )
            )
            .is_err()
        );
        control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?1,'a','grant','reviewer','user:u')", [&subject]).unwrap();
        assert_eq!(
            execute_service(
                &mut control,
                &mut company,
                context(),
                Action::AgentDecision(
                    1,
                    rentemester_storage::exceptions::AgentDecision::Reject,
                    Some(" first service note ")
                )
            )
            .unwrap()
            .result["resolved"],
            true
        );
        let stale = execute_service(
            &mut control,
            &mut company,
            context(),
            Action::AgentDecision(
                1,
                rentemester_storage::exceptions::AgentDecision::Reject,
                None,
            ),
        )
        .err()
        .unwrap();
        assert_eq!(stale.error.code, "agent_suggestion_stale");
        let actor: String = company
            .query_row("SELECT resolved_by FROM exceptions WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(actor, format!("service-account:{subject}"));
        control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?1,'a','revoke',NULL,'user:u')", [&subject]).unwrap();
        let changes = company.total_changes();
        assert!(
            execute_service(
                &mut control,
                &mut company,
                context(),
                Action::AgentDecision(
                    1,
                    rentemester_storage::exceptions::AgentDecision::Reject,
                    Some(" first service note ")
                )
            )
            .is_err()
        );
        assert!(
            execute_service(
                &mut control,
                &mut company,
                context(),
                Action::AgentDecision(
                    999,
                    rentemester_storage::exceptions::AgentDecision::Approve,
                    None
                )
            )
            .is_err()
        );
        assert_eq!(company.total_changes(), changes);
        assert_eq!(
            company
                .query_row(
                    "SELECT count(*) FROM audit_log WHERE event_type='exception_resolve'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?1,'a','grant','reviewer','user:u')", [&subject]).unwrap();
        control
            .execute("UPDATE apikey SET enabled=0 WHERE id=?1", [&credential])
            .unwrap();
        assert!(
            execute_service(
                &mut control,
                &mut company,
                context(),
                Action::AgentDecision(
                    1,
                    rentemester_storage::exceptions::AgentDecision::Reject,
                    Some(" first service note ")
                )
            )
            .is_err()
        );
        assert!(
            execute_service(
                &mut control,
                &mut company,
                context(),
                Action::AgentSuggestions
            )
            .is_err()
        );
        assert_eq!(company.total_changes(), changes);
    }
}
