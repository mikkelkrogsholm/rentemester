//! Current human access observations under an already held control writer lock.
use rentemester_engine::Error;
use rusqlite::{MAIN_DB, OptionalExtension, Transaction, TransactionState, types::ValueRef};

#[derive(Debug)]
pub struct HumanControlFacts {
    pub user_exists: bool,
    pub email_verified: bool,
    pub mfa_enabled: bool,
    pub session_current: bool,
    pub workspace_role: Option<String>,
    pub company_role: Option<String>,
}
fn refused(_: impl std::fmt::Display) -> Error {
    Error::new(
        "invalid_control",
        "current control facts are malformed or unsupported",
    )
}
fn flag(value: ValueRef<'_>, nullable: bool) -> rusqlite::Result<bool> {
    match value {
        ValueRef::Integer(0) => Ok(false),
        ValueRef::Integer(1) => Ok(true),
        ValueRef::Null if nullable => Ok(false),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}
/// Reads exact IDs, no normalization or provider/session-expiry verification.
/// The caller must hold the control lock until its protected company commit.
pub fn observe_human(
    control: &Transaction<'_>,
    subject: &str,
    session: &str,
    company: &str,
) -> Result<HumanControlFacts, Error> {
    if control.transaction_state(Some(MAIN_DB)).map_err(refused)? != TransactionState::Write {
        return Err(Error::new(
            "control_fence_required",
            "control main writer lock required",
        ));
    }
    crate::control_migration::validate_current_control(control)
        .map_err(|_| refused("admission"))?;
    let user = control
        .query_row(
            "SELECT emailVerified,twoFactorEnabled FROM main.user WHERE id=?1",
            [subject],
            |row| Ok((flag(row.get_ref(0)?, false)?, flag(row.get_ref(1)?, true)?)),
        )
        .optional()
        .map_err(refused)?;
    let session_current: bool = control
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM main.session WHERE id=?1 AND userId=?2)",
            [session, subject],
            |row| row.get(0),
        )
        .map_err(refused)?;
    let access: Option<(String,Option<String>)> = control.query_row(
        "SELECT event_type,workspace_role FROM main.rm_workspace_user_access_events WHERE user_id=?1 ORDER BY id DESC LIMIT 1",
        [subject], |r| Ok((r.get(0)?,r.get(1)?)),
    ).optional().map_err(refused)?;
    let membership: Option<(String,Option<String>)> = control.query_row(
        "SELECT event_type,company_role FROM main.rm_company_membership_events WHERE user_id=?1 AND company_slug=?2 ORDER BY id DESC LIMIT 1",
        [subject,company], |r| Ok((r.get(0)?,r.get(1)?)),
    ).optional().map_err(refused)?;
    fn role(
        row: Option<(String, Option<String>)>,
        positive: &str,
        negative: &str,
        roles: &[&str],
    ) -> Result<Option<String>, Error> {
        match row {
            None => Ok(None),
            Some((event, None)) if event == negative => Ok(None),
            Some((event, Some(role))) if event == positive && roles.contains(&role.as_str()) => {
                Ok(Some(role))
            }
            _ => Err(refused("access event")),
        }
    }
    Ok(HumanControlFacts {
        user_exists: user.is_some(),
        email_verified: user.is_some_and(|u| u.0),
        mfa_enabled: user.is_some_and(|u| u.1),
        session_current,
        workspace_role: role(
            access,
            "activate",
            "disable",
            &["workspace_owner", "member"],
        )?,
        company_role: role(
            membership,
            "grant",
            "revoke",
            &["owner", "bookkeeper", "reviewer", "reader"],
        )?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, TransactionBehavior};
    #[test]
    fn deferred_snapshot_is_not_a_control_writer_fence() {
        let mut db = Connection::open_in_memory().unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .unwrap();
        tx.query_row("SELECT count(*) FROM sqlite_schema", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap();
        assert_eq!(
            observe_human(&tx, "u", "s", "a").unwrap_err().code,
            "control_fence_required"
        );
        tx.rollback().unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(
            observe_human(&tx, "u", "s", "a").unwrap_err().code,
            "invalid_control"
        );
        tx.rollback().unwrap();
    }
}
