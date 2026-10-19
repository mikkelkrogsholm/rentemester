//! Current human authority is held across source rechecks, immutable review and journal effects.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_storage::{accounting_drafts, bookkeeping_queue, posting_rules};
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    Drafts,
    Draft {
        id: &'a str,
        action: &'a str,
        input: &'a accounting_drafts::Input,
    },
    Rules,
    Rule {
        action: &'a str,
        input: &'a posting_rules::Input,
    },
    Queue,
    FromBank(&'a bookkeeping_queue::Create),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let permission = match action {
        Action::Drafts | Action::Rules | Action::Queue => Permission::CompanyRead,
        Action::Draft {
            action: "reject" | "approve-and-post",
            ..
        }
        | Action::Rule { .. } => Permission::CompanyReview,
        _ => Permission::CompanyDraftWrite,
    };
    let now = context.clock.now_unix_ms().map_err(|e| Failure {
        stage: control_fence::FailureStage::NotCommitted,
        error: rentemester_engine::Error::new(e.code(), "clock unavailable"),
        control_reusable: true,
        company_reusable: true,
    })?;
    let latest = rentemester_runtime::posting_time::latest_date(now, 45)
        .map_err(|e| rentemester_engine::Error::new(e.code(), "posting horizon unavailable"))
        .and_then(|s| rentemester_engine::fiscal_year::Date::parse(&s))
        .map_err(|error| Failure {
            stage: control_fence::FailureStage::NotCommitted,
            error,
            control_reusable: true,
            company_reusable: true,
        })?;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission,
        |tx, subject| {
            let actor = format!("user:{subject}");
            match action {
                Action::Drafts => accounting_drafts::list(tx),
                Action::Draft { id, action, input } => {
                    accounting_drafts::change(tx, id, action, input, &actor, latest)
                }
                Action::Rules => posting_rules::list(tx),
                Action::Rule { action, input } => {
                    posting_rules::change(tx, action, input, &actor, &latest.to_string())
                }
                Action::Queue => bookkeeping_queue::read(tx),
                Action::FromBank(input) => bookkeeping_queue::create(tx, input, &actor, latest),
            }
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_fence::CompanySelection;
    use rentemester_runtime::auth::{AuthError, Clock, CookieProfile, CookieVerifier};
    use serde_json::json;
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1_767_225_600_000)
        }
    }
    #[test]
    fn current_roles_distinct_sessions_revocation_and_exact_post() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Connection::open(dir.path().join("control")).unwrap();
        let mut l = Connection::open(dir.path().join("company")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic four eyes")
            .unwrap();
        for (user, token) in [("u", "synthetic-token"), ("v", "with.dots")] {
            c.execute("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES(?1,'Synthetic',?2,1,'2026-01-01','2026-01-01',1)",rusqlite::params![user,format!("{user}@example.invalid")]).unwrap();
            c.execute("INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES(?1,'2026-01-01T12:00:00.000Z',?2,'2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',?1)",rusqlite::params![user,token]).unwrap();
            c.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES(?1,'activate','member','agent:synthetic')",[user]).unwrap();
            c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?1,'a','grant','reader','agent:synthetic')",[user]).unwrap();
        }
        l.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('B','Synthetic bank','asset','debit'),('E','Synthetic capital','equity','credit');INSERT INTO bank_accounts(id,slug,name,currency,ledger_account_no) VALUES(1,'synthetic','Synthetic bank','DKK','B');INSERT INTO bank_transactions(id,transaction_date,text,amount,currency,bank_account_id) VALUES(1,'2026-01-01','Synthetic capital',100.01,'DKK',1);").unwrap();
        let fixtures: Value = serde_json::from_str(include_str!(
            "../../rentemester-runtime/fixtures/session-cookie.json"
        ))
        .unwrap();
        let verifier = CookieVerifier::new(
            "w",
            CookieProfile::Secure,
            fixtures["key"].as_str().unwrap(),
        )
        .unwrap();
        let author = verifier
            .verify(fixtures["vectors"][0]["header"].as_str(), None)
            .unwrap();
        let reviewer = verifier
            .verify(fixtures["vectors"][1]["header"].as_str(), None)
            .unwrap();
        let ctx = |cookie| Context {
            cookie,
            clock: &Fixed,
            selection: CompanySelection {
                workspace: "w",
                slug: "a",
                registered: true,
                archived: false,
            },
        };
        let queue = execute(&mut c, &mut l, ctx(&author), Action::Queue)
            .unwrap()
            .value;
        let input=serde_json::from_value(json!({"bankTransactionId":1,"expectedSourceHash":queue["rows"][0]["sourceHash"],"accountNo":"E","draftId":"bank-one","confirm":true})).unwrap();
        assert!(execute(&mut c, &mut l, ctx(&author), Action::FromBank(&input)).is_err());
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','bookkeeper','agent:synthetic'),('v','a','grant','reviewer','agent:synthetic');").unwrap();
        let draft = execute(&mut c, &mut l, ctx(&author), Action::FromBank(&input))
            .unwrap()
            .value;
        let submit =
            serde_json::from_value(json!({"expectedEventHash":draft["eventHash"]})).unwrap();
        let submitted = execute(
            &mut c,
            &mut l,
            ctx(&author),
            Action::Draft {
                id: "bank-one",
                action: "submit",
                input: &submit,
            },
        )
        .unwrap()
        .value;
        let approval = serde_json::from_value(
            json!({"expectedEventHash":submitted["eventHash"],"confirm":true}),
        )
        .unwrap();
        assert!(
            execute(
                &mut c,
                &mut l,
                ctx(&author),
                Action::Draft {
                    id: "bank-one",
                    action: "approve-and-post",
                    input: &approval
                }
            )
            .is_err()
        );
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('v','a','revoke',NULL,'agent:synthetic')").unwrap();
        assert!(
            execute(
                &mut c,
                &mut l,
                ctx(&reviewer),
                Action::Draft {
                    id: "bank-one",
                    action: "approve-and-post",
                    input: &approval
                }
            )
            .is_err()
        );
        assert_eq!(
            l.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('v','a','grant','reviewer','agent:synthetic')").unwrap();
        let posted = execute(
            &mut c,
            &mut l,
            ctx(&reviewer),
            Action::Draft {
                id: "bank-one",
                action: "approve-and-post",
                input: &approval,
            },
        )
        .unwrap()
        .value;
        assert_eq!(posted["status"], "approved_posted");
        assert_eq!(
            execute(
                &mut c,
                &mut l,
                ctx(&reviewer),
                Action::Draft {
                    id: "bank-one",
                    action: "approve-and-post",
                    input: &approval
                }
            )
            .unwrap()
            .value,
            posted
        );
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('v','a','revoke',NULL,'agent:synthetic')").unwrap();
        assert!(
            execute(
                &mut c,
                &mut l,
                ctx(&reviewer),
                Action::Draft {
                    id: "bank-one",
                    action: "approve-and-post",
                    input: &approval
                }
            )
            .is_err()
        );
        assert_eq!(
            l.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
