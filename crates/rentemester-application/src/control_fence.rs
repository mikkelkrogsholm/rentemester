//! Internal admitted-connection orchestration, not a product writer or authentication provider.
use crate::access::{
    self, AccessFacts, CompanyFacts, IdentityFacts, IdentityKind, MembershipFacts, Permission,
    Request, WorkspaceAccessFacts,
};
use rentemester_engine::Error;
use rusqlite::{Connection, MAIN_DB, Transaction, TransactionBehavior, TransactionState};

/// Verified by the provider/runtime, not deserializable caller claims. Expiry is a provider duty.
pub struct HumanSession<'a> {
    pub workspace: &'a str,
    pub subject: &'a str,
    pub session_id: &'a str,
    pub provider_authenticated: bool,
}
/// Caller must hold manifest/lifetime admission; SQLite cannot fence workspace.json.
pub struct CompanySelection<'a> {
    pub workspace: &'a str,
    pub slug: &'a str,
    pub registered: bool,
    pub archived: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureStage {
    NotCommitted,
    OutcomeUnknown,
}
#[derive(Debug)]
pub struct Failure {
    pub stage: FailureStage,
    pub error: Error,
    pub control_reusable: bool,
    pub company_reusable: bool,
}
#[derive(Debug)]
pub struct Committed<T> {
    pub value: T,
    /// False means retire this control connection; the company commit still succeeded.
    pub control_reusable: bool,
}
fn error(code: &'static str) -> Error {
    Error::new(
        code,
        "fenced company transaction refused; inspect stage and connection reuse status",
    )
}
fn initial(error: Error) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error,
        control_reusable: false,
        company_reusable: false,
    }
}
fn connection_ready(db: &Connection) -> Result<(), Error> {
    if db
        .is_readonly(MAIN_DB)
        .map_err(|_| error("connection_refused"))?
    {
        return Err(error("connection_refused"));
    }
    if !db.is_autocommit() {
        return Err(error("active_transaction"));
    }
    let journal: String = db
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .map_err(|_| error("connection_refused"))?;
    if !["wal", "delete"].contains(&journal.as_str()) {
        return Err(error("connection_refused"));
    }
    let attachments: i64 = db
        .query_row(
            "SELECT count(*) FROM pragma_database_list WHERE name NOT IN ('main','temp')",
            [],
            |r| r.get(0),
        )
        .map_err(|_| error("connection_refused"))?;
    let temporary: i64 = db
        .query_row("SELECT count(*) FROM sqlite_temp_schema", [], |r| r.get(0))
        .map_err(|_| error("connection_refused"))?;
    let fk: i64 = db
        .pragma_query_value(None, "foreign_keys", |r| r.get(0))
        .map_err(|_| error("connection_refused"))?;
    let sync: i64 = db
        .pragma_query_value(None, "synchronous", |r| r.get(0))
        .map_err(|_| error("connection_refused"))?;
    let readonly: i64 = db
        .pragma_query_value(None, "query_only", |r| r.get(0))
        .map_err(|_| error("connection_refused"))?;
    if attachments != 0 || temporary != 0 || fk != 1 || ![2, 3].contains(&sync) || readonly != 0 {
        return Err(error("connection_refused"));
    }
    Ok(())
}
/// Control→company is the only supported lock order. Both physical DB identities and
/// operation→permission binding must already be admitted by the runtime. Callback is trusted,
/// synchronous, transaction-local Rust code; SQL transaction escapes are NOT sandboxed.
/// No source paths are opened, and no real writer is enabled by this internal primitive.
pub fn execute<T>(
    control: &mut Connection,
    company: &mut Connection,
    principal: HumanSession<'_>,
    selection: CompanySelection<'_>,
    permission: Permission,
    apply: impl FnOnce(&Transaction<'_>) -> Result<T, Error>,
) -> Result<Committed<T>, Failure> {
    if !permission.is_company()
        || !principal.provider_authenticated
        || principal.workspace.is_empty()
        || principal.subject.is_empty()
        || principal.session_id.is_empty()
        || principal.workspace != selection.workspace
    {
        return Err(initial(error("access_denied")));
    }
    connection_ready(control).map_err(initial)?;
    connection_ready(company).map_err(initial)?;
    let control_tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| initial(error("control_busy_or_unavailable")))?;
    let authorize = (|| {
        let observed = rentemester_storage::control_access::observe_human(
            &control_tx,
            principal.subject,
            principal.session_id,
            selection.slug,
        )?;
        let workspace_role = observed
            .workspace_role
            .map(|r| {
                serde::Deserialize::deserialize(serde::de::value::StringDeserializer::<
                    serde::de::value::Error,
                >::new(r))
            })
            .transpose()
            .map_err(|_| error("invalid_control"))?;
        let company_role = observed
            .company_role
            .map(|r| {
                serde::Deserialize::deserialize(serde::de::value::StringDeserializer::<
                    serde::de::value::Error,
                >::new(r))
            })
            .transpose()
            .map_err(|_| error("invalid_control"))?;
        let facts = AccessFacts {
            identity: observed.user_exists.then_some(IdentityFacts {
                workspace: principal.workspace,
                subject: principal.subject,
                authenticated: observed.session_current,
                kind: IdentityKind::Human {
                    email_verified: observed.email_verified,
                    mfa_enabled: observed.mfa_enabled,
                },
            }),
            workspace_access: workspace_role.map(|role| WorkspaceAccessFacts {
                workspace: principal.workspace,
                subject: principal.subject,
                active: true,
                role,
            }),
            company: Some(CompanyFacts {
                workspace: selection.workspace,
                slug: selection.slug,
                registered: selection.registered,
                archived: selection.archived,
                membership: company_role.map(|role| MembershipFacts {
                    workspace: principal.workspace,
                    subject: principal.subject,
                    company: selection.slug,
                    active: true,
                    role,
                }),
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
    })();
    if let Err(error) = authorize {
        let reusable = control_tx.rollback().is_ok();
        return Err(Failure {
            control_reusable: reusable,
            ..initial(error)
        });
    }
    #[cfg(test)]
    checkpoint("authorized");
    let company_tx = match company.transaction_with_behavior(TransactionBehavior::Immediate) {
        Ok(tx) => tx,
        Err(_) => {
            let reusable = control_tx.rollback().is_ok();
            return Err(Failure {
                control_reusable: reusable,
                ..initial(error("company_busy_or_unavailable"))
            });
        }
    };
    if company_tx.transaction_state(Some(MAIN_DB)).ok() != Some(TransactionState::Write) {
        let company_reusable = company_tx.rollback().is_ok();
        let control_reusable = control_tx.rollback().is_ok();
        return Err(Failure {
            stage: FailureStage::NotCommitted,
            error: error("company_write_lock_required"),
            control_reusable,
            company_reusable,
        });
    }
    let changes_before = control_tx.total_changes();
    let result = apply(&company_tx);
    if company_tx.is_autocommit()
        || control_tx.is_autocommit()
        || control_tx.total_changes() != changes_before
    {
        let _ = company_tx.rollback();
        let _ = control_tx.rollback();
        return Err(Failure {
            stage: FailureStage::OutcomeUnknown,
            error: error("transaction_escaped"),
            control_reusable: false,
            company_reusable: false,
        });
    }
    if let Err(error) = result {
        let company_reusable = company_tx.rollback().is_ok();
        let control_reusable = control_tx.rollback().is_ok();
        return Err(Failure {
            stage: FailureStage::NotCommitted,
            error,
            control_reusable,
            company_reusable,
        });
    }
    let value = result.expect("error branch returned");
    #[cfg(test)]
    checkpoint("precommit");
    if company_tx.commit().is_err() {
        let control_reusable = control_tx.rollback().is_ok();
        return Err(Failure {
            stage: FailureStage::OutcomeUnknown,
            error: error("commit_outcome_unknown"),
            control_reusable,
            company_reusable: false,
        });
    }
    #[cfg(test)]
    checkpoint("postcommit");
    Ok(finish_committed(value, control_tx))
}
fn finish_committed<T>(value: T, control: Transaction<'_>) -> Committed<T> {
    Committed {
        value,
        control_reusable: control.rollback().is_ok(),
    }
}
#[cfg(test)]
fn checkpoint(point: &str) {
    if std::env::var("RM_FENCE_POINT").ok().as_deref() == Some(point) {
        std::fs::write(std::env::var("RM_FENCE_MARKER").unwrap(), point).unwrap();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
    }
}
#[cfg(test)]
mod tests;
