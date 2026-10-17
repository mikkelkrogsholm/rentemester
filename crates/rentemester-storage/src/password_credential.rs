//! Exact current credential candidates. No proof of password, MFA or session authority.
use rentemester_engine::Error;
use rusqlite::{MAIN_DB, OptionalExtension, Transaction, TransactionState, types::ValueRef};
use zeroize::Zeroizing;

/// Sensitive transient read snapshot; deliberately no Debug, Clone or serialization.
#[derive(PartialEq, Eq)]
pub struct CredentialSnapshot {
    subject: String,
    email: String,
    email_verified: bool,
    mfa_enabled: bool,
    user_updated: String,
    account_id: String,
    account_updated: String,
    password: Zeroizing<String>,
    access_event: i64,
    workspace_role: String,
}
/// First-factor routing only. Neither case establishes an authenticated session.
#[derive(Debug, PartialEq, Eq)]
pub enum NextFactor {
    Challenge,
    Enrollment,
}
pub struct FirstFactor {
    pub subject: String,
    pub next: NextFactor,
}
pub fn denied() -> Error {
    Error::new("invalid_credentials", "credential continuation refused")
}
fn malformed(_: impl std::fmt::Display) -> Error {
    Error::new("invalid_control", "current credential facts refused")
}
fn flag(value: ValueRef<'_>, nullable: bool) -> rusqlite::Result<bool> {
    match value {
        ValueRef::Integer(0) => Ok(false),
        ValueRef::Integer(1) => Ok(true),
        ValueRef::Null if nullable => Ok(false),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}
fn text(value: String) -> rusqlite::Result<String> {
    if value.trim().is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(value)
}
/// Source practical ASCII email grammar, with an explicit strict end-of-input correction.
/// No Unicode lowercasing, whitespace trimming or SQL collation normalization.
pub fn lookup_email(email: &str) -> Result<String, Error> {
    if email.len() > 4096 || !email.is_ascii() || email.bytes().any(|b| b.is_ascii_control()) {
        return Err(denied());
    }
    let (local, domain) = email.split_once('@').ok_or_else(denied)?;
    let local_char = |b: u8| b.is_ascii_alphanumeric() || b"_'+-.".contains(&b);
    if local.is_empty()
        || local.starts_with('.')
        || local.contains("..")
        || !local.bytes().all(local_char)
        || !local
            .bytes()
            .last()
            .is_some_and(|b| local_char(b) && !b"'.".contains(&b))
    {
        return Err(denied());
    }
    let (labels, tld) = domain.rsplit_once('.').ok_or_else(denied)?;
    if tld.len() < 2
        || !tld.bytes().all(|b| b.is_ascii_alphabetic())
        || !labels.split('.').all(|s| {
            s.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric())
                && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        return Err(denied());
    }
    Ok(email.to_ascii_lowercase())
}
/// Capture one fully admitted read snapshot. Missing/inactive candidates are deliberately absent.
pub fn capture(
    control: &Transaction<'_>,
    email: &str,
) -> Result<Option<CredentialSnapshot>, Error> {
    if control
        .transaction_state(Some(MAIN_DB))
        .map_err(malformed)?
        == TransactionState::None
    {
        return Err(malformed("snapshot required"));
    }
    crate::control_migration::validate_current_control(control).map_err(malformed)?;
    let mut query = control.prepare(
        "SELECT u.id,u.email,u.emailVerified,u.twoFactorEnabled,u.updatedAt,a.id,a.updatedAt,a.password FROM main.user u JOIN main.account a ON a.userId=u.id AND a.accountId=u.id AND a.providerId='credential' COLLATE BINARY AND a.issuer='local:credential' COLLATE BINARY WHERE u.email=?1 COLLATE BINARY LIMIT 2",
    ).map_err(malformed)?;
    let mut rows = query.query([email]).map_err(malformed)?;
    let Some(row) = rows.next().map_err(malformed)? else {
        return Ok(None);
    };
    let mut candidate = CredentialSnapshot {
        subject: text(row.get(0).map_err(malformed)?).map_err(malformed)?,
        email: text(row.get(1).map_err(malformed)?).map_err(malformed)?,
        email_verified: flag(row.get_ref(2).map_err(malformed)?, false).map_err(malformed)?,
        mfa_enabled: flag(row.get_ref(3).map_err(malformed)?, true).map_err(malformed)?,
        user_updated: text(row.get(4).map_err(malformed)?).map_err(malformed)?,
        account_id: text(row.get(5).map_err(malformed)?).map_err(malformed)?,
        account_updated: text(row.get(6).map_err(malformed)?).map_err(malformed)?,
        password: Zeroizing::new(
            row.get::<_, Option<String>>(7)
                .map_err(malformed)?
                .unwrap_or_default(),
        ),
        access_event: 0,
        workspace_role: String::new(),
    };
    if rows.next().map_err(malformed)?.is_some() {
        return Err(denied());
    }
    let access: Option<(i64,String,Option<String>)> = control.query_row(
        "SELECT id,event_type,workspace_role FROM main.rm_workspace_user_access_events WHERE user_id=?1 ORDER BY id DESC LIMIT 1",
        [&candidate.subject], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).optional().map_err(malformed)?;
    match access {
        None => return Ok(None),
        Some((_, event, None)) if event == "disable" => return Ok(None),
        Some((id, event, Some(role)))
            if id > 0
                && event == "activate"
                && ["member", "workspace_owner"].contains(&role.as_str()) =>
        {
            candidate.access_event = id;
            candidate.workspace_role = role;
        }
        _ => return Err(malformed("access event")),
    }
    Ok(Some(candidate))
}
pub fn capture_subject(
    control: &Transaction<'_>,
    subject: &str,
) -> Result<Option<CredentialSnapshot>, Error> {
    let email: Option<String> = control
        .query_row("SELECT email FROM main.user WHERE id=?1", [subject], |r| {
            r.get(0)
        })
        .optional()
        .map_err(malformed)?;
    let Some(email) = email else {
        return Ok(None);
    };
    Ok(capture(control, &email)?.filter(|s| s.subject == subject))
}
impl CredentialSnapshot {
    pub fn eligible_mfa(&self) -> bool {
        self.email_verified && self.mfa_enabled
    }
    /// Private pending-auth binding only. Never export or log credential-derived fingerprints.
    pub fn binding_fingerprint(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        let event = self.access_event.to_string();
        for value in [
            "rm-native-credential-binding-v1",
            &self.subject,
            &self.email,
            if self.email_verified { "1" } else { "0" },
            if self.mfa_enabled { "1" } else { "0" },
            &self.user_updated,
            &self.account_id,
            &self.account_updated,
            self.password.as_str(),
            &event,
            &self.workspace_role,
        ] {
            hash.update((value.len() as u64).to_be_bytes());
            hash.update(value.as_bytes());
        }
        hash.finalize().into()
    }
    /// Sensitive value for the trusted native KDF only; never persist or log this snapshot.
    pub fn password_hash(&self) -> &str {
        self.password.as_str()
    }
    /// Consume the snapshot inside the same admitted connection's current writer transaction.
    /// Caller must have verified its password and hold the lock through pending-factor commit.
    pub fn recheck(self, control: &Transaction<'_>) -> Result<FirstFactor, Error> {
        if control
            .transaction_state(Some(MAIN_DB))
            .map_err(malformed)?
            != TransactionState::Write
        {
            return Err(Error::new(
                "control_fence_required",
                "control main writer lock required",
            ));
        }
        let current = capture(control, &self.email)?.ok_or_else(denied)?;
        if self != current || !current.email_verified {
            return Err(denied());
        }
        Ok(FirstFactor {
            subject: current.subject,
            next: if current.mfa_enabled {
                NextFactor::Challenge
            } else {
                NextFactor::Enrollment
            },
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn practical_email_is_ascii_case_folded_without_trimming() {
        for email in [
            "A_B+tag@Example.INVALID",
            "a'b@a-.invalid",
            "a@b.co",
            "a@a.b.example",
        ] {
            assert_eq!(lookup_email(email).unwrap(), email.to_ascii_lowercase());
        }
        for email in [
            "",
            " a@b.co",
            "a@b.co\n",
            "a..b@b.co",
            ".a@b.co",
            "a.@b.co",
            "a'@b.co",
            "a@b.c",
            "a@-b.co",
            "a@b..co",
            "æ@b.co",
            "a@@b.co",
            "a@b.123",
        ] {
            assert!(lookup_email(email).is_err());
        }
    }
}
