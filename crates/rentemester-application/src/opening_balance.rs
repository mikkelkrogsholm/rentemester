//! One admitted human opening-balance transaction, including source assurance and durable receipt.
use crate::{
    access::{
        self, AccessFacts, CompanyFacts, IdentityFacts, IdentityKind, MembershipFacts, Permission,
        WorkspaceAccessFacts,
    },
    control_fence::{self, Committed, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{
    Error,
    fiscal_year::Date,
    opening_balance::{Apply, Input},
};
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::{
    idempotency::{self, Operation, Outcome, PrincipalKind},
    opening_balance as store,
};
use rusqlite::Connection;
use serde_json::{Value, json};
pub enum Action<'a> {
    Read,
    OriginalRun(&'a rentemester_storage::synthetic_import::Input, bool),
    Plan(&'a Input),
    Apply(&'a Apply),
    SyntheticPlan(&'a rentemester_storage::synthetic_import::Input),
    SyntheticApply(&'a rentemester_storage::synthetic_import::Apply),
}
fn unavailable() -> Error {
    Error::new(
        "opening_balance_source_unavailable",
        "Opening-balance sources are unavailable or differ from retained proof",
    )
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let readonly = !matches!(
        action,
        Action::Apply(_) | Action::SyntheticApply(_) | Action::OriginalRun(..)
    );
    let cookie = context.cookie;
    let clock = context.clock;
    let selection = context.selection;
    control_fence::execute_authorized_with_backup(
        control,
        company,
        (!readonly).then_some(control_fence::BackupCheck {
            workspace: selection.workspace,
            slug: selection.slug,
            clock,
        }),
        |tx| {
            let denied = || {
                Error::new(
                    "access_denied",
                    "Current human opening-balance authority refused",
                )
            };
            if cookie.workspace() != selection.workspace {
                return Err(denied());
            }
            let now = clock
                .now_unix_ms()
                .map_err(|e| Error::new(e.code(), "Trusted authorization clock unavailable"))?;
            let current = rentemester_storage::cookie_session::resolve(
                tx,
                cookie.token(),
                now,
                SessionRequirement::Current,
            )?;
            rentemester_storage::session_control::authorize(tx, &current)?;
            let f = rentemester_storage::control_access::observe_human(
                tx,
                &current.subject,
                &current.session_id,
                selection.slug,
            )?;
            let workspace_role = f
                .workspace_role
                .map(|r| serde_json::from_value(json!(r)))
                .transpose()
                .map_err(|_| denied())?;
            let company_role = f
                .company_role
                .map(|r| serde_json::from_value(json!(r)))
                .transpose()
                .map_err(|_| denied())?;
            let facts = AccessFacts {
                identity: f.user_exists.then_some(IdentityFacts {
                    workspace: selection.workspace,
                    subject: &current.subject,
                    authenticated: f.session_current,
                    kind: IdentityKind::Human {
                        email_verified: f.email_verified,
                        mfa_enabled: f.mfa_enabled,
                    },
                }),
                workspace_access: workspace_role.map(|role| WorkspaceAccessFacts {
                    workspace: selection.workspace,
                    subject: &current.subject,
                    active: true,
                    role,
                }),
                company: Some(CompanyFacts {
                    workspace: selection.workspace,
                    slug: selection.slug,
                    registered: selection.registered,
                    archived: selection.archived,
                    membership: company_role.map(|role| MembershipFacts {
                        workspace: selection.workspace,
                        subject: &current.subject,
                        company: selection.slug,
                        active: true,
                        role,
                    }),
                }),
            };
            let required = if readonly {
                &[Permission::CompanyRead][..]
            } else {
                &[Permission::CompanyAdmin, Permission::CompanyLedgerPost][..]
            };
            for permission in required {
                if !access::authorize(
                    access::Request {
                        workspace: selection.workspace,
                        company: Some(selection.slug),
                        permission: *permission,
                    },
                    &facts,
                )
                .allowed
                {
                    return Err(denied());
                }
            }
            if !readonly {
                rentemester_storage::workspace_transfer::require_business_write(tx)?;
            }
            Ok(current.subject)
        },
        |tx, subject| {
            // Historical/physical assurance precedes even exact receipt replay. Mutable planning
            // eligibility belongs only to the fresh-effect callback below.
            let before = tx.total_changes();
            let verified = crate::archive_import::verify_physical(tx, guard)?;
            store::validate_history(tx)?;
            if matches!(action, Action::Read) {
                let result = store::read(tx)?;
                if tx.total_changes() != before {
                    return Err(Error::new(
                        "readonly_operation_changed_data",
                        "Opening-balance read changed data",
                    ));
                }
                return Ok(result);
            }
            let now = clock
                .now_unix_ms()
                .map_err(|e| Error::new(e.code(), "Trusted opening-balance clock unavailable"))?;
            let latest = Date::parse(
                &rentemester_runtime::posting_time::latest_date(now, 45).map_err(|e| {
                    Error::new(e.code(), "Trusted opening-balance horizon unavailable")
                })?,
            )?;
            match action {
                Action::Read => unreachable!(),
                Action::Plan(input) => {
                    let result = json!({"ok":true,"plan":store::prepare(tx,input,&verified,latest)?.review(),"errors":[]});
                    if tx.total_changes() != before {
                        return Err(Error::new(
                            "readonly_operation_changed_data",
                            "Opening-balance review changed data",
                        ));
                    }
                    Ok(result)
                }
                Action::SyntheticPlan(input) => {
                    let result = rentemester_storage::synthetic_import::prepare(
                        tx, input, &verified, latest,
                    )?;
                    if tx.total_changes() != before {
                        return Err(Error::new(
                            "readonly_operation_changed_data",
                            "Synthetic source review changed data",
                        ));
                    }
                    Ok(result)
                }
                Action::OriginalRun(input, dry_run) => {
                    tx.execute_batch("SAVEPOINT original_import94")
                        .map_err(|_| unavailable())?;
                    let actor = format!("user:{subject}");
                    let result = rentemester_storage::synthetic_import::run_original(
                        tx,
                        input,
                        &verified,
                        latest,
                        rentemester_storage::journal_post::Actor {
                            subject: &actor,
                            program: store::PROGRAM,
                        },
                        now,
                    );
                    // Both business refusals and dry runs remove all business effects.
                    if dry_run || result.is_err() {
                        tx.execute_batch("ROLLBACK TO original_import94")
                            .map_err(|_| unavailable())?;
                    }
                    tx.execute_batch("RELEASE original_import94")
                        .map_err(|_| unavailable())?;
                    match result {
                        Ok(mut value) => {
                            if dry_run {
                                value["dryRun"] = json!(true);
                            }
                            Ok(value)
                        }
                        Err(error)
                            if matches!(
                                error.code,
                                "synthetic_import_refused"
                                    | "opening_balance_refused"
                                    | "company_setup_refused"
                            ) =>
                        {
                            Ok(
                                json!({"ok":false,"sourceSystem":"synthetic-csv","openingBalanceLineCount":rentemester_engine::synthetic_import::parse(input.content.as_bytes()).map_or(0, |s| s.opening_balances.len()),"historicalEntriesSkipped":0,
                                "auditTrail":["Import transaction rolled back"],"appliedRules":["DK-BOOKKEEPING-BALANCED-001"],"errors":[error.message]}),
                            )
                        }
                        Err(error) => Err(error),
                    }
                }
                Action::SyntheticApply(input) => {
                    if !input.confirm {
                        return Err(Error::new(
                            "opening_balance_invalid",
                            "Explicit confirmation required",
                        ));
                    }
                    idempotency::validate_key(&input.idempotency_key)?;
                    let encoded = serde_json::to_string(&input.input).map_err(|_| unavailable())?;
                    let payload = json!({"syntheticSourceSha256":rentemester_storage::intercompany::sha(&encoded),"planHash":input.plan_hash,"confirm":input.confirm});
                    let actor = format!("user:{subject}");
                    match idempotency::execute(
                        tx,
                        idempotency::Input {
                            key: &input.idempotency_key,
                            operation: Operation::OpeningBalancePost,
                            principal_kind: PrincipalKind::User,
                            subject,
                            payload: &payload,
                            created_by: &actor,
                            created_by_program: store::PROGRAM,
                            now_ms: now,
                        },
                        |tx| {
                            rentemester_storage::synthetic_import::apply(
                                tx,
                                input,
                                &verified,
                                latest,
                                rentemester_storage::journal_post::Actor {
                                    subject: &actor,
                                    program: store::PROGRAM,
                                },
                                now,
                            )
                        },
                    )
                    .map_err(|f| f.error)?
                    {
                        Outcome::Accepted {
                            mut result,
                            receipt,
                        } => {
                            result["idempotency"] =
                                serde_json::to_value(receipt).map_err(|_| unavailable())?;
                            Ok(result)
                        }
                        Outcome::Refused { code } => Err(Error::new(
                            code,
                            "Source request key refused; read canonical opening and journal before another action",
                        )),
                    }
                }
                Action::Apply(input) => {
                    if !input.confirm {
                        return Err(Error::new(
                            "opening_balance_invalid",
                            "Explicit confirmation required",
                        ));
                    }
                    idempotency::validate_key(&input.idempotency_key)?;
                    let encoded = serde_json::to_string(&input.input).map_err(|_| unavailable())?;
                    let payload = json!({"inputSha256":rentemester_storage::intercompany::sha(&encoded),"planHash":input.plan_hash,"confirm":input.confirm});
                    let actor = format!("user:{subject}");
                    match idempotency::execute(
                        tx,
                        idempotency::Input {
                            key: &input.idempotency_key,
                            operation: Operation::OpeningBalancePost,
                            principal_kind: PrincipalKind::User,
                            subject,
                            payload: &payload,
                            created_by: &actor,
                            created_by_program: store::PROGRAM,
                            now_ms: now,
                        },
                        |tx| {
                            store::apply(
                                tx,
                                input,
                                &verified,
                                latest,
                                rentemester_storage::journal_post::Actor {
                                    subject: &actor,
                                    program: store::PROGRAM,
                                },
                                now,
                            )
                        },
                    )
                    .map_err(|f| f.error)?
                    {
                        Outcome::Accepted {
                            mut result,
                            receipt,
                        } => {
                            result["idempotency"] =
                                serde_json::to_value(receipt).map_err(|_| unavailable())?;
                            Ok(result)
                        }
                        Outcome::Refused { code } => Err(Error::new(
                            code,
                            "Opening-balance request key refused; read the retained opening balance before another action",
                        )),
                    }
                }
            }
        },
    )
}
