//! Charged admission attempt followed by a fresh current service company fence.
pub(crate) mod precharged;
use crate::{
    access::{
        self, AccessFacts, CompanyFacts, IdentityFacts, IdentityKind, MembershipFacts, Permission,
        Request, WorkspaceAccessFacts,
    },
    control_fence::{self, CompanySelection, FailureStage},
    journal_mutation::{self, Command},
    journal_post, journal_reversal,
};
use rentemester_engine::Error;
use rentemester_runtime::{auth::Clock, service_key::ServiceKey};
use rentemester_storage::{
    idempotency::{PrincipalKind, Receipt},
    service_context::{self, Observation},
    workspace_context::{Catalog, RegisteredCompany},
};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use serde::Serialize;
use serde_json::Value;
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageStage {
    NotCommitted,
    OutcomeUnknown,
    Committed,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BusinessStage {
    NotAttempted,
    NotCommitted,
    OutcomeUnknown,
    Committed,
}
pub struct ServiceResult<T = Value> {
    pub usage: UsageStage,
    pub business: BusinessStage,
    pub result: T,
    pub receipt: Option<Receipt>,
    pub control_reusable: bool,
}
pub(crate) struct OperationResult<T> {
    pub result: T,
    pub receipt: Option<Receipt>,
}
pub struct ServiceFailure {
    pub usage: UsageStage,
    pub business: BusinessStage,
    pub error: Error,
    pub control_reusable: bool,
    pub company_reusable: bool,
    pub rejection: Option<Value>,
}
impl std::fmt::Debug for ServiceFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceFailure")
            .field("usage", &self.usage)
            .field("business", &self.business)
            .field("code", &self.error.code)
            .field("control_reusable", &self.control_reusable)
            .field("company_reusable", &self.company_reusable)
            .field("has_rejection", &self.rejection.is_some())
            .finish()
    }
}
/// Caller owns actual physical/catalog/lifetime/legacy-writer admission. No path opens.
pub struct Context<'a> {
    pub key: &'a ServiceKey,
    pub clock: &'a dyn Clock,
    pub selection: CompanySelection<'a>,
    pub max_future_days: u32,
}
/// Pure transport preflight; rejects malformed envelopes before opening a company attempt.
pub fn validate_post_input(input: &journal_post::Input) -> Result<(), Error> {
    Command::Post(input).validate()
}
pub fn validate_reversal_input(input: &journal_reversal::Input) -> Result<(), Error> {
    Command::Reverse(input).validate()
}
pub fn execute_post(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    input: journal_post::Input,
) -> Result<ServiceResult, ServiceFailure> {
    execute(control, company, context, Command::Post(&input))
}
pub fn execute_reversal(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    input: journal_reversal::Input,
) -> Result<ServiceResult, ServiceFailure> {
    execute(control, company, context, Command::Reverse(&input))
}
fn error(code: &'static str) -> Error {
    Error::new(
        code,
        "service company attempt refused; inspect usage/business stages",
    )
}
pub(crate) fn initial(error: Error) -> ServiceFailure {
    ServiceFailure {
        usage: UsageStage::NotCommitted,
        business: BusinessStage::NotAttempted,
        error,
        control_reusable: false,
        company_reusable: false,
        rejection: None,
    }
}
fn now(clock: &dyn Clock) -> Result<i64, Error> {
    clock.now_unix_ms().map_err(|_| error("invalid_auth_clock"))
}
fn authorize(
    tx: &Transaction<'_>,
    observed: &Observation,
    selection: &CompanySelection<'_>,
    permission: Permission,
) -> Result<(), Error> {
    let role = rentemester_storage::control_access::observe_company_role(
        tx,
        observed.subject(),
        selection.slug,
    )?;
    fn parse<T: serde::de::DeserializeOwned>(s: String) -> Result<T, Error> {
        serde::Deserialize::deserialize(serde::de::value::StringDeserializer::<
            serde::de::value::Error,
        >::new(s))
        .map_err(|_| error("invalid_control"))
    }
    let facts = AccessFacts {
        identity: Some(IdentityFacts {
            workspace: selection.workspace,
            subject: observed.subject(),
            authenticated: true,
            kind: IdentityKind::Service {
                registered: true,
                live_credential: true,
            },
        }),
        workspace_access: Some(WorkspaceAccessFacts {
            workspace: selection.workspace,
            subject: observed.subject(),
            active: true,
            role: parse(observed.workspace_role().to_owned())?,
        }),
        company: Some(CompanyFacts {
            workspace: selection.workspace,
            slug: selection.slug,
            registered: selection.registered,
            archived: selection.archived,
            membership: role
                .map(|role| {
                    Ok(MembershipFacts {
                        workspace: selection.workspace,
                        subject: observed.subject(),
                        company: selection.slug,
                        active: true,
                        role: parse(role)?,
                    })
                })
                .transpose()?,
        }),
    };
    if !access::authorize(
        Request {
            workspace: selection.workspace,
            company: Some(selection.slug),
            permission,
        },
        &facts,
    )
    .allowed
    {
        return Err(error("access_denied"));
    }
    Ok(())
}
fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    command: Command<'_>,
) -> Result<ServiceResult, ServiceFailure> {
    command.validate().map_err(initial)?;
    let clock = context.clock;
    let max_future_days = context.max_future_days;
    execute_operation(
        control,
        company,
        context,
        Permission::CompanyLedgerPost,
        true,
        |tx, subject, rejection| {
            journal_mutation::apply(
                tx,
                subject,
                PrincipalKind::ServiceAccount,
                clock,
                max_future_days,
                command,
                rejection,
            )
            .map(|value| OperationResult {
                result: value.result,
                receipt: value.receipt,
            })
        },
    )
}

/// Service reads use the same current credential and company fence as posting.
pub(crate) fn execute_read(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    apply: impl FnOnce(&Transaction<'_>) -> Result<Value, Error>,
) -> Result<ServiceResult, ServiceFailure> {
    execute_operation(
        control,
        company,
        context,
        Permission::CompanyRead,
        false,
        |tx, _, _| {
            Ok(OperationResult {
                result: apply(tx)?,
                receipt: None,
            })
        },
    )
}

pub(crate) fn execute_operation<T>(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    permission: Permission,
    mutation: bool,
    apply: impl FnOnce(&Transaction<'_>, &str, &mut Option<Value>) -> Result<OperationResult<T>, Error>,
) -> Result<ServiceResult<T>, ServiceFailure> {
    execute_operation_both(control, company, context, permission, mutation, |_, tx, subject, rejection| apply(tx, subject, rejection))
}
pub(crate) fn execute_recovery<T>(control:&mut Connection,company:&mut Connection,context:Context<'_>,permission:Permission,apply:impl FnOnce(&Transaction<'_>,&str,&mut Option<Value>)->Result<OperationResult<T>,Error>)->Result<ServiceResult<T>,ServiceFailure>{
    execute_operation_mode(control,company,context,permission,true,true,|_, tx, subject, rejection| apply(tx, subject, rejection))
}
/// Narrow backup policy recovery: normal transfer/auth guards and usage remain enforced.
pub(crate) fn execute_recovery_both<T>(control:&mut Connection,company:&mut Connection,context:Context<'_>,permission:Permission,apply:impl FnOnce(&Transaction<'_>,&Transaction<'_>,&str,&mut Option<Value>)->Result<OperationResult<T>,Error>)->Result<ServiceResult<T>,ServiceFailure>{
 execute_operation_mode(control,company,context,permission,true,true,apply)
}
/// Same single-charge admission and live recheck, retaining both held transactions.
pub(crate) fn execute_operation_both<T>(
    control: &mut Connection, company: &mut Connection, context: Context<'_>,
    permission: Permission, mutation: bool,
    apply: impl FnOnce(&Transaction<'_>, &Transaction<'_>, &str, &mut Option<Value>) -> Result<OperationResult<T>, Error>,
) -> Result<ServiceResult<T>, ServiceFailure> {
    execute_operation_mode(control, company, context, permission, mutation, false, apply)
}
#[allow(clippy::too_many_arguments)]
fn execute_operation_mode<T>(control:&mut Connection,company:&mut Connection,context:Context<'_>,permission:Permission,mutation:bool,backup_recovery:bool,apply:impl FnOnce(&Transaction<'_>,&Transaction<'_>,&str,&mut Option<Value>)->Result<OperationResult<T>,Error>)->Result<ServiceResult<T>,ServiceFailure>{
    control_fence::connection_ready(control).map_err(initial)?;
    control_fence::connection_ready(company).map_err(initial)?;
    let Context {
        key,
        clock,
        selection,
        ..
    } = context;
    if key.workspace() != selection.workspace {
        return Err(initial(error("invalid_credentials")));
    }
    let catalog = Catalog {
        workspace: selection.workspace.to_owned(),
        companies: if selection.registered {
            vec![RegisteredCompany {
                slug: selection.slug.to_owned(),
                name: selection.slug.to_owned(),
                archived: selection.archived,
            }]
        } else {
            vec![]
        },
    };
    rentemester_storage::workspace_context::validate_catalog(&catalog).map_err(initial)?;
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| initial(error("control_busy_or_unavailable")))?;
    let mut charge_usage = false;
    let charged = (|| {
        charge_usage = rentemester_storage::workspace_transfer::current(&tx)?.mode == "active";
        if mutation {
            rentemester_storage::workspace_transfer::require_business_write(&tx)?;
        }
        let time = now(clock)?;
        let observed = service_context::observe(&tx, key.lookup_hash(), time, &catalog)?;
        authorize(&tx, &observed, &selection, permission)?;
        let charged = if charge_usage {
            let charged = service_context::charge(&tx, key.lookup_hash(), time, &catalog)?;
            if !charged.same_credential(&observed) {
                return Err(error("invalid_credentials"));
            }
            charged
        } else {
            observed
        };
        #[cfg(test)]
        if std::env::var("RM_SERVICE_FAIL").ok().as_deref() == Some("usage") {
            tx.execute(
                "INSERT INTO rm_service_synthetic_missing_table VALUES(1)",
                [],
            )
            .map_err(|_| error("invalid_control"))?;
        }
        #[cfg(test)]
        checkpoint("usage-precommit");
        Ok(charged)
    })();
    let charged = match charged {
        Ok(charged) => charged,
        Err(error) => {
            let clean = tx.rollback().is_ok() && control_fence::connection_ready(control).is_ok();
            return Err(ServiceFailure {
                usage: if clean {
                    UsageStage::NotCommitted
                } else {
                    UsageStage::OutcomeUnknown
                },
                control_reusable: clean,
                company_reusable: true,
                ..initial(error)
            });
        }
    };
    if tx.commit().is_err() {
        return Err(ServiceFailure {
            usage: UsageStage::OutcomeUnknown,
            company_reusable: true,
            ..initial(error("usage_commit_outcome_unknown"))
        });
    }
    let usage = if charge_usage {
        UsageStage::Committed
    } else {
        UsageStage::NotCommitted
    };
    #[cfg(test)]
    checkpoint("usage-committed");
    let mut rejection = None;
    let mut attempted = false;
    let done = control_fence::execute_authorized_both_with_backup(
        control,
        company,
        (mutation && !backup_recovery).then_some(control_fence::BackupCheck{workspace:selection.workspace,slug:selection.slug,clock}),
        |tx| {
            if mutation {
                rentemester_storage::workspace_transfer::require_business_write(tx)?;
            } else if (rentemester_storage::workspace_transfer::current(tx)?.mode == "active")
                != charge_usage
            {
                return Err(error("service_admission_changed"));
            }
            let current = service_context::observe(tx, key.lookup_hash(), now(clock)?, &catalog)?;
            if !current.same_credential(&charged) {
                return Err(error("invalid_credentials"));
            }
            authorize(tx, &current, &selection, permission)?;
            Ok(current.subject().to_owned())
        },
        |control_tx, tx, subject| {
            attempted = true;
            let before = tx.total_changes();
            let value = apply(control_tx, tx, subject, &mut rejection)?;
            if !mutation && tx.total_changes() != before {
                return Err(error("readonly_operation_changed_data"));
            }
            #[cfg(test)]
            if std::env::var("RM_SERVICE_FAIL").ok().as_deref() == Some("company") {
                tx.execute(
                    "INSERT INTO rm_service_synthetic_missing_table VALUES(1)",
                    [],
                )
                .map_err(|_| error("synthetic_company_failure"))?;
            }
            Ok(value)
        },
    );
    match done {
        Ok(done) => Ok(ServiceResult {
            usage,
            business: BusinessStage::Committed,
            result: done.value.result,
            receipt: done.value.receipt,
            control_reusable: done.control_reusable,
        }),
        Err(f) => Err(ServiceFailure {
            usage,
            business: if f.stage == FailureStage::OutcomeUnknown {
                BusinessStage::OutcomeUnknown
            } else if attempted {
                BusinessStage::NotCommitted
            } else {
                BusinessStage::NotAttempted
            },
            error: f.error,
            control_reusable: f.control_reusable,
            company_reusable: f.company_reusable,
            rejection,
        }),
    }
}
#[cfg(test)]
fn checkpoint(point: &str) {
    if std::env::var("RM_SERVICE_POINT").ok().as_deref() == Some(point) {
        std::fs::write(std::env::var("RM_SERVICE_MARKER").unwrap(), point).unwrap();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
    }
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod paired_admission_tests {
    use super::*;
    #[test]
    fn paired_service_keeps_workspace_check_before_callback_or_charge() {
        let dir = tempfile::tempdir().unwrap();
        let mut control = Connection::open(dir.path().join("control.sqlite")).unwrap();
        let mut company = Connection::open(dir.path().join("company.sqlite")).unwrap();
        for db in [&control, &company] {
            db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL").unwrap();
        }
        let key = ServiceKey::new("other", "synthetic-secret").unwrap();
        let context = Context {
            key: &key, clock: &rentemester_runtime::auth::SystemClock,
            selection: CompanySelection { workspace: "w", slug: "a", registered: true, archived: false },
            max_future_days: 45,
        };
        let failure = execute_operation_both::<Value>(&mut control, &mut company, context, Permission::CompanyRead, false,
            |_, _, _, _| panic!("unadmitted callback must not run")).err().unwrap();
        assert_eq!(failure.error.code, "invalid_credentials");
        assert_eq!(failure.usage, UsageStage::NotCommitted);
        assert_eq!(failure.business, BusinessStage::NotAttempted);
        assert_eq!(control.total_changes(), 0);
        assert_eq!(company.total_changes(), 0);
    }
}
