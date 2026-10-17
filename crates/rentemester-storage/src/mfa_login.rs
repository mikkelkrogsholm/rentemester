//! Native enrolled-login rows on a trusted admitted control writer transaction only.
use rentemester_engine::Error;
use rusqlite::{MAIN_DB, Transaction, TransactionState, params};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;
const CONTEXT: &str = "rm-native-mfa-v1:";
const ENROLLMENT: &str = "rm-native-enroll-v1:";
#[derive(Clone, Copy)]
enum Stage {
    Login,
    Enrollment,
}
impl Stage {
    fn namespace(self) -> &'static str {
        match self {
            Self::Login => CONTEXT,
            Self::Enrollment => ENROLLMENT,
        }
    }
}
const REPLAY: &str = "rm-native-totp-step-v1:";
pub const MAX_NOW: i64 = 253_402_300_799_999 - 43_200_000;
fn invalid(_: impl std::fmt::Display) -> Error {
    Error::new("invalid_control", "native MFA control facts refused")
}
fn denied() -> Error {
    Error::new("invalid_credentials", "native MFA challenge refused")
}
fn writer(tx: &Transaction<'_>) -> Result<(), Error> {
    if tx.transaction_state(Some(MAIN_DB)).map_err(invalid)? != TransactionState::Write {
        return Err(invalid("writer required"));
    }
    crate::control_migration::validate_current_control(tx).map_err(invalid)
}
pub fn check_now(now: i64) -> Result<(), Error> {
    if !(0..=MAX_NOW).contains(&now) {
        Err(Error::new("invalid_auth_clock", "native MFA clock refused"))
    } else {
        Ok(())
    }
}
pub(crate) fn date(now: i64) -> Result<String, Error> {
    let t = time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(now) * 1_000_000)
        .map_err(invalid)?;
    if !(1..=9999).contains(&t.year()) {
        return Err(invalid("date range"));
    }
    Ok(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
        t.millisecond()
    ))
}
fn stamp(value: &str) -> Result<i64, Error> {
    crate::cookie_session::timestamp(value).map_err(invalid)
}
fn bounded(value: String) -> Result<String, Error> {
    if value.trim().is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
        Err(invalid("text"))
    } else {
        Ok(value)
    }
}
struct Record {
    id: String,
    value: Zeroizing<String>,
    created: String,
    updated: String,
    expires: String,
}
fn record(tx: &Transaction<'_>, identifier: &str) -> Result<Option<Record>, Error> {
    let mut q=tx.prepare("SELECT id,value,createdAt,updatedAt,expiresAt FROM main.verification WHERE identifier=?1 COLLATE BINARY LIMIT 2").map_err(invalid)?;
    let mut rows = q.query([identifier]).map_err(invalid)?;
    let Some(r) = rows.next().map_err(invalid)? else {
        return Ok(None);
    };
    let result = Record {
        id: bounded(r.get(0).map_err(invalid)?)?,
        value: Zeroizing::new(r.get(1).map_err(invalid)?),
        created: r.get(2).map_err(invalid)?,
        updated: r.get(3).map_err(invalid)?,
        expires: r.get(4).map_err(invalid)?,
    };
    if result.value.len() > 8192 || rows.next().map_err(invalid)?.is_some() {
        return Err(invalid("ambiguous record"));
    }
    Ok(Some(result))
}
fn insert(
    tx: &Transaction<'_>,
    id: &str,
    identifier: &str,
    value: &str,
    created: &str,
    expires: &str,
) -> Result<(), Error> {
    if record(tx, identifier)?.is_some() {
        return Err(invalid("existing identifier"));
    }
    tx.execute("INSERT INTO main.verification(id,identifier,value,createdAt,updatedAt,expiresAt) VALUES(?1,?2,?3,?4,?4,?5)",params![id,identifier,value,created,expires]).map_err(invalid)?;
    Ok(())
}
struct Factor {
    id: String,
    secret: Zeroizing<String>,
    backup: Zeroizing<String>,
    failures: i64,
    locked: Option<i64>,
}
impl Factor {
    fn locked(&self, now: i64) -> bool {
        self.locked.is_some_and(|until| until > now)
    }
    fn count(&self, now: i64) -> i64 {
        if self.locked.is_some_and(|until| until <= now) {
            0
        } else {
            self.failures
        }
    }
}
fn factor(tx: &Transaction<'_>, subject: &str, stage: Stage) -> Result<Factor, Error> {
    let mut q=tx.prepare("SELECT id,secret,verified,failedVerificationCount,lockedUntil,backupCodes FROM main.twoFactor WHERE userId=?1 COLLATE BINARY LIMIT 2").map_err(invalid)?;
    let mut rows = q.query([subject]).map_err(invalid)?;
    let Some(r) = rows.next().map_err(invalid)? else {
        return Err(denied());
    };
    let verified: Option<i64> = r.get(2).map_err(invalid)?;
    if verified
        != Some(match stage {
            Stage::Login => 1,
            Stage::Enrollment => 0,
        })
    {
        return Err(denied());
    }
    let failures: Option<i64> = r.get(3).map_err(invalid)?;
    let locked: Option<String> = r.get(4).map_err(invalid)?;
    let result = Factor {
        id: bounded(r.get(0).map_err(invalid)?)?,
        secret: Zeroizing::new(r.get(1).map_err(invalid)?),
        backup: Zeroizing::new(r.get(5).map_err(invalid)?),
        failures: failures.unwrap_or(0),
        locked: locked.as_deref().map(stamp).transpose()?,
    };
    if result.secret.len() > 8294
        || result.backup.len() > 8294
        || !(0..=10).contains(&result.failures)
        || result.failures == 10 && result.locked.is_none()
        || rows.next().map_err(invalid)?.is_some()
    {
        return Err(invalid("factor state"));
    }
    Ok(result)
}
fn context(
    tx: &Transaction<'_>,
    subject: &str,
    workspace: &str,
    factor: &Factor,
    stage: Stage,
) -> Result<String, Error> {
    let credential =
        crate::password_credential::capture_subject(tx, subject)?.ok_or_else(denied)?;
    if !match stage {
        Stage::Login => credential.eligible_mfa(),
        Stage::Enrollment => credential.eligible_enrollment(),
    } {
        return Err(denied());
    }
    // Rows are immutable; COUNT advances even after SQLite random ROWID allocation.
    let epoch:i64=tx.query_row("SELECT COUNT(*) FROM main.rm_workspace_auth_state_events WHERE user_id=?1 AND state_transition IN ('credential_updated','email_verified','mfa_enabled','session_deleted')",[subject],|r|r.get(0)).map_err(invalid)?;
    if epoch < 0 {
        return Err(invalid("epoch"));
    }
    let fingerprint = credential.binding_fingerprint();
    let mut hash = Sha256::new();
    for value in [
        match stage {
            Stage::Login => b"rm-native-mfa-binding-v1".as_slice(),
            Stage::Enrollment => b"rm-native-enroll-binding-v1".as_slice(),
        },
        workspace.as_bytes(),
        subject.as_bytes(),
        &fingerprint,
        factor.id.as_bytes(),
        factor.secret.as_bytes(),
        &epoch.to_be_bytes(),
    ] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value);
    }
    if matches!(stage, Stage::Enrollment) {
        if record(tx, &format!("{REPLAY}{}", factor.id))?.is_some() {
            return Err(invalid("unverified replay state"));
        }
        hash.update((factor.backup.len() as u64).to_be_bytes());
        hash.update(factor.backup.as_bytes());
    }
    Ok(format!("v1:{:x}", hash.finalize()))
}
pub struct PendingIds<'a> {
    pub challenge: &'a str,
    pub attempts: &'a str,
    pub context: &'a str,
}
pub fn create(
    tx: &Transaction<'_>,
    subject: &str,
    workspace: &str,
    token: &str,
    ids: PendingIds<'_>,
    now: i64,
) -> Result<(), Error> {
    create_inner(tx, subject, workspace, token, ids, now, Stage::Login)
}
pub fn create_enrollment(
    tx: &Transaction<'_>,
    subject: &str,
    workspace: &str,
    token: &str,
    ids: PendingIds<'_>,
    now: i64,
) -> Result<(), Error> {
    create_inner(tx, subject, workspace, token, ids, now, Stage::Enrollment)
}
/// Replace only an unverified setup, retaining the account-wide failure/lock budget.
pub fn prepare_enrollment(
    tx: &Transaction<'_>,
    subject: &str,
    id: &str,
    secret: &str,
    backup: &str,
    now: i64,
) -> Result<(), Error> {
    writer(tx)?;
    check_now(now)?;
    if !crate::password_credential::capture_subject(tx, subject)?
        .is_some_and(|c| c.eligible_enrollment())
    {
        return Err(denied());
    }
    let count: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM main.twoFactor WHERE userId=?1 COLLATE BINARY",
            [subject],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if count == 0 {
        if record(tx, &format!("{REPLAY}{id}"))?.is_some() {
            return Err(invalid("unverified replay state"));
        }
        tx.execute("INSERT INTO main.twoFactor(id,userId,secret,backupCodes,verified,failedVerificationCount,lockedUntil) VALUES(?1,?2,?3,?4,0,0,NULL)",params![id,subject,secret,backup]).map_err(invalid)?;
    } else if count == 1 {
        let current = factor(tx, subject, Stage::Enrollment)?;
        if current.locked(now) {
            return Err(denied());
        }
        if record(tx, &format!("{REPLAY}{}", current.id))?.is_some() {
            return Err(invalid("unverified replay state"));
        }
        one_change(
            tx.execute(
                "UPDATE main.twoFactor SET secret=?1,backupCodes=?2 WHERE id=?3 AND verified=0",
                params![secret, backup, current.id],
            )
            .map_err(invalid)?,
        )?;
    } else {
        return Err(invalid("ambiguous factor"));
    }
    Ok(())
}
fn create_inner(
    tx: &Transaction<'_>,
    subject: &str,
    workspace: &str,
    token: &str,
    ids: PendingIds<'_>,
    now: i64,
    stage: Stage,
) -> Result<(), Error> {
    writer(tx)?;
    check_now(now)?;
    let factor = factor(tx, subject, stage)?;
    if factor.locked(now) {
        return Err(denied());
    }
    let binding = context(tx, subject, workspace, &factor, stage)?;
    let created = date(now)?;
    let expires = date(now + 600_000)?;
    insert(tx, ids.challenge, token, subject, &created, &expires)?;
    insert(
        tx,
        ids.attempts,
        &format!("2fa-attempts-{token}"),
        "0",
        &created,
        &expires,
    )?;
    insert(
        tx,
        ids.context,
        &format!("{}{token}", stage.namespace()),
        &binding,
        &created,
        &expires,
    )
}
/// Sensitive private state, deliberately neither Debug nor serializable.
pub struct Attempt {
    subject: String,
    challenge: Record,
    attempts: Record,
    context: Record,
    count: i64,
    factor: Factor,
    stage: Stage,
}
pub fn resolve(
    tx: &Transaction<'_>,
    workspace: &str,
    token: &str,
    now: i64,
) -> Result<Attempt, Error> {
    resolve_inner(tx, workspace, token, now, Stage::Login)
}
pub fn resolve_enrollment(
    tx: &Transaction<'_>,
    workspace: &str,
    token: &str,
    now: i64,
) -> Result<Attempt, Error> {
    resolve_inner(tx, workspace, token, now, Stage::Enrollment)
}
fn resolve_inner(
    tx: &Transaction<'_>,
    workspace: &str,
    token: &str,
    now: i64,
    stage: Stage,
) -> Result<Attempt, Error> {
    writer(tx)?;
    check_now(now)?;
    let challenge = record(tx, token)?.ok_or_else(denied)?;
    let attempts = record(tx, &format!("2fa-attempts-{token}"))?.ok_or_else(denied)?;
    let binding = record(tx, &format!("{}{token}", stage.namespace()))?.ok_or_else(denied)?;
    let created = stamp(&challenge.created)?;
    let expires = stamp(&challenge.expires)?;
    if created > now
        || expires <= now
        || expires <= created
        || expires - created > 600_000
        || challenge.updated != challenge.created
    {
        return Err(denied());
    }
    for row in [&attempts, &binding] {
        if row.created != challenge.created
            || row.expires != challenge.expires
            || stamp(&row.updated)? < created
            || stamp(&row.updated)? > now
        {
            return Err(denied());
        }
    }
    if binding.updated != binding.created {
        return Err(denied());
    }
    let count = attempts.value.parse::<i64>().map_err(invalid)?;
    if !(0..5).contains(&count) || count.to_string() != attempts.value.as_str() {
        return Err(denied());
    }
    let subject = bounded(challenge.value.as_str().to_owned())?;
    let factor = factor(tx, &subject, stage)?;
    if context(tx, &subject, workspace, &factor, stage)? != binding.value.as_str() {
        return Err(denied());
    }
    Ok(Attempt {
        subject,
        challenge,
        attempts,
        context: binding,
        count,
        factor,
        stage,
    })
}
fn one_change(count: usize) -> Result<(), Error> {
    if count != 1 {
        Err(invalid("row changed"))
    } else {
        Ok(())
    }
}
fn telemetry(
    tx: &Transaction<'_>,
    endpoint: &str,
    outcome: &str,
    subject: &str,
    session: Option<&str>,
    version: u64,
) -> Result<(), Error> {
    if version > 9_007_199_254_740_991 {
        return Err(invalid("audit version"));
    }
    tx.execute("INSERT INTO main.rm_workspace_auth_telemetry_events(endpoint,outcome,user_id,session_id,identity_key_version) VALUES(?1,?2,?3,?4,?5)",params![endpoint,outcome,subject,session,version as i64]).map_err(invalid)?;
    Ok(())
}
pub fn audit_begin(tx: &Transaction<'_>, subject: &str, version: u64) -> Result<(), Error> {
    telemetry(tx, "sign-in-email", "accepted", subject, None, version)
}
pub fn audit_enrollment_begin(
    tx: &Transaction<'_>,
    subject: &str,
    version: u64,
) -> Result<(), Error> {
    audit_begin(tx, subject, version)?;
    telemetry(tx, "two-factor-enable", "accepted", subject, None, version)
}
/// Current verified factor pool, not a reusable identity grant; caller holds authorization lock.
pub struct ManagedPool {
    subject: String,
    factor: Factor,
}
pub fn managed_pool(tx: &Transaction<'_>, subject: &str) -> Result<ManagedPool, Error> {
    writer(tx)?;
    if !crate::password_credential::capture_subject(tx, subject)?.is_some_and(|c| c.eligible_mfa())
    {
        return Err(denied());
    }
    Ok(ManagedPool {
        subject: subject.to_owned(),
        factor: factor(tx, subject, Stage::Login)?,
    })
}
impl ManagedPool {
    pub fn encrypted(&self) -> &str {
        &self.factor.backup
    }
    pub fn finish(
        &self,
        tx: &Transaction<'_>,
        replacement: Option<&str>,
        session: &str,
        version: u64,
        now: i64,
    ) -> Result<(), Error> {
        let event = if let Some(value) = replacement {
            one_change(tx.execute("UPDATE main.twoFactor SET backupCodes=?1 WHERE id=?2 AND backupCodes=?3 COLLATE BINARY",params![value,self.factor.id,self.factor.backup.as_str()]).map_err(invalid)?)?;
            "mfa_backup_codes_regenerated"
        } else {
            "mfa_backup_codes_viewed"
        };
        if version > 9_007_199_254_740_991 {
            return Err(invalid("audit version"));
        }
        let entity = serde_json::to_string(&(&self.subject, session, version)).map_err(invalid)?;
        tx.execute("INSERT INTO main.workspace_audit(event_type,entity_type,entity_id,actor,created_at) VALUES(?1,'native_mfa_backup_management_v1',?2,?3,?4)",params![event,entity,format!("user:{}",self.subject),date(now)?]).map_err(invalid)?;
        Ok(())
    }
}
#[derive(Clone, Copy)]
pub enum Method {
    Totp,
    Backup,
}
impl Method {
    fn endpoint(self) -> &'static str {
        match self {
            Self::Totp => "two-factor-verify",
            Self::Backup => "two-factor-verify-backup-code",
        }
    }
}
pub enum Completion<'a> {
    Totp(u64),
    Backup(&'a str),
}
pub struct SessionIds<'a> {
    pub id: &'a str,
    pub token: &'a str,
    pub replay: &'a str,
}
impl Attempt {
    pub fn locked(&self, now: i64) -> bool {
        self.factor.locked(now)
    }
    pub fn encrypted_secret(&self) -> &str {
        &self.factor.secret
    }
    pub fn encrypted_backup_codes(&self) -> &str {
        &self.factor.backup
    }
    fn burn(&self, tx: &Transaction<'_>) -> Result<(), Error> {
        for row in [&self.challenge, &self.attempts, &self.context] {
            one_change(
                tx.execute("DELETE FROM main.verification WHERE id=?1", [&row.id])
                    .map_err(invalid)?,
            )?;
        }
        Ok(())
    }
    pub fn audit_locked(
        &self,
        tx: &Transaction<'_>,
        version: u64,
        method: Method,
    ) -> Result<(), Error> {
        telemetry(
            tx,
            method.endpoint(),
            "rejected",
            &self.subject,
            None,
            version,
        )
    }
    pub fn reject(
        &self,
        tx: &Transaction<'_>,
        now: i64,
        version: u64,
        method: Method,
    ) -> Result<(), Error> {
        let failures = self.factor.count(now) + 1;
        let locked = if failures >= 10 {
            Some(date(now + 900_000)?)
        } else {
            None
        };
        one_change(
            tx.execute(
                "UPDATE main.twoFactor SET failedVerificationCount=?1,lockedUntil=?2 WHERE id=?3",
                params![failures, locked, self.factor.id],
            )
            .map_err(invalid)?,
        )?;
        if self.count + 1 >= 5 {
            self.burn(tx)?;
        } else {
            one_change(
                tx.execute(
                    "UPDATE main.verification SET value=?1,updatedAt=?2 WHERE id=?3",
                    params![(self.count + 1).to_string(), date(now)?, self.attempts.id],
                )
                .map_err(invalid)?,
            )?;
        }
        telemetry(
            tx,
            method.endpoint(),
            "rejected",
            &self.subject,
            None,
            version,
        )
    }
    pub fn replayed(&self, tx: &Transaction<'_>, counter: u64, now: i64) -> Result<bool, Error> {
        let Some(row) = record(tx, &format!("{REPLAY}{}", self.factor.id))? else {
            return Ok(false);
        };
        let step = row.value.parse::<u64>().map_err(invalid)?;
        if step > 9_007_199_254_740_991
            || step.to_string() != row.value.as_str()
            || stamp(&row.created)? > now
            || stamp(&row.updated)? > now
            || row.expires != "9999-12-31T23:59:59.999Z"
        {
            return Err(invalid("replay record"));
        }
        Ok(counter <= step)
    }
    pub fn accept_enrollment(
        &self,
        tx: &Transaction<'_>,
        counter: u64,
        now: i64,
        ids: SessionIds<'_>,
        version: u64,
    ) -> Result<(), Error> {
        if !matches!(self.stage, Stage::Enrollment) {
            return Err(denied());
        }
        one_change(
            tx.execute(
                "UPDATE main.twoFactor SET verified=1 WHERE id=?1 AND verified=0",
                [&self.factor.id],
            )
            .map_err(invalid)?,
        )?;
        // The canonical trigger revokes ALL previous sessions. Its legacy actor label
        // is a compatibility value, not an assertion of the executing runtime.
        one_change(tx.execute("UPDATE main.user SET twoFactorEnabled=1,updatedAt=?1 WHERE id=?2 AND COALESCE(twoFactorEnabled,0)=0",params![date(now)?,self.subject]).map_err(invalid)?)?;
        self.accept_inner(tx, Completion::Totp(counter), now, ids, version)
    }
    pub fn accept(
        &self,
        tx: &Transaction<'_>,
        completion: Completion<'_>,
        now: i64,
        ids: SessionIds<'_>,
        version: u64,
    ) -> Result<(), Error> {
        if !matches!(self.stage, Stage::Login) {
            return Err(denied());
        }
        self.accept_inner(tx, completion, now, ids, version)
    }
    fn accept_inner(
        &self,
        tx: &Transaction<'_>,
        completion: Completion<'_>,
        now: i64,
        ids: SessionIds<'_>,
        version: u64,
    ) -> Result<(), Error> {
        let created = date(now)?;
        let method = match completion {
            Completion::Backup(replacement) => {
                one_change(tx.execute("UPDATE main.twoFactor SET backupCodes=?1 WHERE id=?2 AND backupCodes=?3 COLLATE BINARY", params![replacement,self.factor.id,self.factor.backup.as_str()]).map_err(invalid)?)?;
                Method::Backup
            }
            Completion::Totp(counter) => {
                let identifier = format!("{REPLAY}{}", self.factor.id);
                if let Some(row) = record(tx, &identifier)? {
                    one_change(
                        tx.execute(
                            "UPDATE main.verification SET value=?1,updatedAt=?2 WHERE id=?3",
                            params![counter.to_string(), created, row.id],
                        )
                        .map_err(invalid)?,
                    )?;
                } else {
                    insert(
                        tx,
                        ids.replay,
                        &identifier,
                        &counter.to_string(),
                        &created,
                        "9999-12-31T23:59:59.999Z",
                    )?;
                }
                Method::Totp
            }
        };
        one_change(
            tx.execute(
                "UPDATE main.twoFactor SET failedVerificationCount=0,lockedUntil=NULL WHERE id=?1",
                [&self.factor.id],
            )
            .map_err(invalid)?,
        )?;
        self.burn(tx)?;
        tx.execute("INSERT INTO main.session(id,token,userId,createdAt,updatedAt,expiresAt) VALUES(?1,?2,?3,?4,?4,?5)",params![ids.id,ids.token,self.subject,created,date(now+43_200_000)?]).map_err(invalid)?;
        telemetry(
            tx,
            method.endpoint(),
            "accepted",
            &self.subject,
            Some(ids.id),
            version,
        )
    }
}
