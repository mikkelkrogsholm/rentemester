//! Receive-only IMAP admission. Provider I/O belongs strictly between prepare and finish.
use crate::{
    access::{self, CompanyRole, Permission, WorkspaceRole},
    control_fence::{self, Committed, CompanySelection, Failure, FailureStage},
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::{
    auth::{Clock, CookieVerifier, new_token},
    imap,
    namespace::NamespaceGuard,
    service_key::ServiceKey,
};
use rentemester_storage::{
    mail_intake as store,
    workspace_context::{Catalog, RegisteredCompany},
    workspace_inbox::Provenance,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub enum Action<'a> {
    Status,
    Configure(&'a Value),
    DeleteConfig,
    Alias(Option<&'a str>),
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Grant {
    epoch: String,
    file_hash: String,
    status: Value,
    grantor: String,
    authority: String,
    service: Option<ServiceGrant>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ServiceGrant {
    subject: String,
    credential: String,
    authority: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    epoch: String,
    nonce: String,
    config: Value,
}
/// Owned private config and authority observation. Never serialized or accepted from request JSON.
pub struct Admission {
    workspace: String,
    company: String,
    ledger: String,
    subject: String,
    authority: String,
    grant: Grant,
    config: imap::Config,
    cursor: Option<imap::Cursor>,
    service: bool,
}
impl Admission {
    pub fn config(&self) -> &imap::Config {
        &self.config
    }
    pub fn cursor(&self) -> Option<imap::Cursor> {
        self.cursor
    }
    pub fn company(&self) -> &str {
        &self.company
    }
}
fn refused() -> Error {
    Error::new(
        "mail_intake_refused",
        "Mail intake unavailable; inspect current configuration and authority",
    )
}
fn denied() -> Error {
    Error::new("access_denied", "Current mail authority required")
}
fn failure(error: Error, reusable: bool) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error,
        control_reusable: reusable,
        company_reusable: reusable,
    }
}
fn sha(bytes: &[u8]) -> String {
    rentemester_storage::invoice_email::sha(bytes)
}
fn ledger(db: &Connection) -> Result<String, Error> {
    db.query_row(
        "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
        [],
        |r| r.get(0),
    )
    .map_err(|_| refused())
}
fn stamp(clock: &dyn Clock) -> Result<(i64, String), Error> {
    let n = clock.now_unix_ms().map_err(|_| refused())?;
    Ok((n, rentemester_engine::company_knowledge::timestamp(n)?))
}
fn catalog(s: &CompanySelection<'_>) -> Catalog {
    Catalog {
        workspace: s.workspace.into(),
        companies: if s.registered {
            vec![RegisteredCompany {
                slug: s.slug.into(),
                name: s.slug.into(),
                archived: s.archived,
            }]
        } else {
            vec![]
        },
    }
}
fn permission(
    tx: &Transaction<'_>,
    subject: &str,
    s: &CompanySelection<'_>,
    p: Permission,
    human: bool,
) -> Result<(), Error> {
    if !s.registered || s.archived {
        return Err(denied());
    }
    let facts = rentemester_storage::control_access::observe_human(tx, subject, "", s.slug)?;
    let wr: WorkspaceRole = serde_json::from_value(json!(facts.workspace_role.ok_or_else(denied)?))
        .map_err(|_| denied())?;
    let cr: CompanyRole = serde_json::from_value(json!(facts.company_role.ok_or_else(denied)?))
        .map_err(|_| denied())?;
    if !facts.user_exists
        || !access::workspace_role_allows(wr, Permission::WorkspaceRead)
        || !access::company_role_allows(cr, p)
    {
        return Err(denied());
    }
    let service: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM rm_workspace_service_principals WHERE user_id=?1)",
            [subject],
            |r| r.get(0),
        )
        .map_err(|_| refused())?;
    if human && (service || !facts.email_verified || !facts.mfa_enabled) {
        return Err(denied());
    }
    if human {
        let verified: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM twoFactor WHERE userId=?1 AND verified=1)",
                [subject],
                |r| r.get(0),
            )
            .map_err(|_| refused())?;
        if !verified {
            return Err(denied());
        }
    }
    Ok(())
}
fn human(
    tx: &Transaction<'_>,
    cookies: &CookieVerifier,
    c: &Context<'_>,
    p: Permission,
) -> Result<String, Error> {
    if !cookies.accepts_session(c.cookie) || c.cookie.workspace() != c.selection.workspace {
        return Err(denied());
    }
    let current = rentemester_storage::cookie_session::resolve(
        tx,
        c.cookie.token(),
        stamp(c.clock)?.0,
        rentemester_storage::cookie_session::SessionRequirement::Current,
    )?;
    rentemester_storage::session_control::authorize(tx, &current)?;
    permission(tx, &current.subject, &c.selection, p, true)?;
    Ok(current.subject)
}
/// All durable authority/security anchors; quotas and session expiry are deliberately excluded.
fn authority(
    tx: &Transaction<'_>,
    subject: &str,
    s: &CompanySelection<'_>,
    human: bool,
) -> Result<String, Error> {
    permission(
        tx,
        subject,
        s,
        if human {
            Permission::CompanyAdmin
        } else {
            Permission::CompanyDocumentsUpload
        },
        human,
    )?;
    let workspace: i64 = tx
        .query_row(
            "SELECT COALESCE(max(id),0) FROM rm_workspace_user_access_events WHERE user_id=?1",
            [subject],
            |r| r.get(0),
        )
        .map_err(|_| refused())?;
    let company:i64=tx.query_row("SELECT COALESCE(max(id),0) FROM rm_company_membership_events WHERE user_id=?1 AND company_slug=?2",[subject,s.slug],|r|r.get(0)).map_err(|_|refused())?;
    let updated: String = tx
        .query_row("SELECT updatedAt FROM user WHERE id=?1", [subject], |r| {
            r.get(0)
        })
        .map_err(|_| refused())?;
    let security:i64=tx.query_row("SELECT COALESCE(max(id),0) FROM workspace_audit WHERE actor=?1 AND entity_type IN ('native_password_reset_v1','native_mfa_backup_management_v1')",[format!("user:{subject}")],|r|r.get(0)).map_err(|_|refused())?;
    let factor: Option<String> = tx
        .query_row(
            "SELECT id FROM twoFactor WHERE userId=?1 AND verified=1",
            [subject],
            |r| r.get(0),
        )
        .optional()
        .map_err(|_| refused())?;
    let credential:i64=tx.query_row("SELECT COALESCE(max(id),0) FROM rm_workspace_service_principal_events WHERE user_id=?1",[subject],|r|r.get(0)).map_err(|_|refused())?;
    Ok(sha(json!({"workspace":s.workspace,"company":s.slug,"subject":subject,"workspaceEvent":workspace,"companyEvent":company,"updatedAt":updated,"security":security,"factor":factor,"credentialEvent":credential}).to_string().as_bytes()))
}
fn service(
    tx: &Transaction<'_>,
    key: &ServiceKey,
    clock: &dyn Clock,
    s: &CompanySelection<'_>,
) -> Result<ServiceGrant, Error> {
    if key.workspace() != s.workspace {
        return Err(denied());
    }
    let o = rentemester_storage::service_context::observe(
        tx,
        key.lookup_hash(),
        stamp(clock)?.0,
        &catalog(s),
    )?;
    Ok(ServiceGrant {
        subject: o.subject().into(),
        credential: o.credential_id().into(),
        authority: authority(tx, o.subject(), s, false)?,
    })
}
fn audit(
    tx: &Transaction<'_>,
    kind: &str,
    value: Value,
    subject: &str,
    at: &str,
) -> Result<(), Error> {
    audit_actor(tx, kind, value, &format!("user:{subject}"), at)
}
fn audit_actor(
    tx: &Transaction<'_>,
    kind: &str,
    value: Value,
    actor: &str,
    at: &str,
) -> Result<(), Error> {
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at) VALUES(?1,'native_mail_config_v1','imap',?2,?3,?4)",params![kind,value.to_string(),actor,at]).map_err(|_|refused())?;
    Ok(())
}

fn grant(tx: &Transaction<'_>) -> Result<Option<Grant>, Error> {
    let v:Option<(String,String,String)>=tx.query_row("SELECT event_type,message,actor FROM audit_log WHERE entity_type='native_mail_config_v1' AND event_type IN ('native_mail_config_set','native_mail_config_deleted') ORDER BY id DESC LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|_|refused())?;
    match v {
        None => Ok(None),
        Some((event, raw, actor)) => {
            if event == "native_mail_config_deleted" {
                return Ok(None);
            }
            let g: Grant = serde_json::from_str(&raw).map_err(|_| refused())?;
            if actor != format!("user:{}", g.grantor) {
                return Err(refused());
            }
            Ok(Some(g))
        }
    }
}
fn configured(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
) -> Result<Option<(Grant, imap::Config)>, Error> {
    let g = grant(tx)?;
    let bytes = guard.read_imap_config().map_err(|_| refused())?;
    match (g, bytes) {
        (None, None) | (Some(_), None) => Ok(None),
        (Some(g), Some(bytes)) => {
            let e: Envelope = serde_json::from_slice(&bytes).map_err(|_| refused())?;
            if e.format != "rentemester-imap-config/1"
                || e.epoch != g.epoch
                || e.nonce.len() != 43
                || sha(&bytes) != g.file_hash
            {
                return Err(refused());
            }
            let c = imap::Config::parse(&serde_json::to_vec(&e.config).map_err(|_| refused())?)
                .map_err(|_| refused())?;
            if serde_json::to_value(c.status()).map_err(|_| refused())? != g.status {
                return Err(refused());
            }
            Ok(Some((g, c)))
        }
        _ => Err(refused()),
    }
}
fn check_grant(tx: &Transaction<'_>, g: &Grant, s: &CompanySelection<'_>) -> Result<(), Error> {
    if authority(tx, &g.grantor, s, true)? != g.authority {
        return Err(denied());
    }
    Ok(())
}
fn fenced<T>(
    control: &mut Connection,
    company: &mut Connection,
    work: impl FnOnce(&Transaction<'_>, &Transaction<'_>) -> Result<T, Error>,
) -> Result<Committed<T>, Failure> {
    control_fence::connection_ready(control).map_err(|e| failure(e, false))?;
    control_fence::connection_ready(company).map_err(|e| failure(e, false))?;
    let ct = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(refused(), false))?;
    let lt = company
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(refused(), false))?;
    match work(&ct, &lt) {
        Err(e) => {
            let clean = lt.rollback().is_ok() & ct.rollback().is_ok();
            Err(failure(e, clean))
        }
        Ok(value) => {
            if lt.commit().is_err() {
                let _ = ct.rollback();
                return Err(Failure {
                    stage: FailureStage::OutcomeUnknown,
                    ..failure(refused(), false)
                });
            }
            ct.commit().map_err(|_| Failure {
                stage: FailureStage::OutcomeUnknown,
                ..failure(refused(), false)
            })?;
            Ok(Committed {
                value,
                control_reusable: true,
            })
        }
    }
}
fn state(
    company: &Transaction<'_>,
    slug: &str,
    g: Option<&Grant>,
    enabled: bool,
) -> Result<Value, Error> {
    let info = rentemester_storage::company_setup::profile(company)?;
    let alias: Option<String> = company
        .query_row("SELECT mail_alias FROM companies WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(|_| refused())?;
    let mut q=company.prepare("SELECT id,document_no,source,upload_datetime,sender_name,invoice_date,amount_inc_vat,retain_until FROM documents WHERE id IN (SELECT document_id FROM mail_intake_messages) ORDER BY id DESC LIMIT 50").map_err(|_|refused())?;
    let rows=q.query_map([],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"documentNo":r.get::<_,Option<String>>(1)?,"source":r.get::<_,String>(2)?,"uploadDatetime":r.get::<_,Option<String>>(3)?,"senderName":r.get::<_,Option<String>>(4)?,"invoiceDate":r.get::<_,Option<String>>(5)?,"amountIncVat":r.get::<_,Option<f64>>(6)?,"retainUntil":r.get::<_,Option<String>>(7)?}))).map_err(|_|refused())?.collect::<Result<Vec<_>,_>>().map_err(|_|refused())?;
    let last:Option<String>=company.query_row("SELECT message FROM audit_log WHERE entity_type='native_mail_config_v1' AND event_type='native_mail_poll' ORDER BY id DESC LIMIT 1",[],|r|r.get(0)).optional().map_err(|_|refused())?;
    let last = last
        .map(|raw| serde_json::from_str::<Value>(&raw).map_err(|_| refused()))
        .transpose()?;
    let mut polling = json!({"enabled":enabled,"state":if g.is_some(){if enabled{"configured"}else{"manual_only"}}else{"disabled"}});
    if let Some(last) = last.filter(|v| g.is_some_and(|g| v["epoch"] == g.epoch)) {
        if let Some(state) = last["state"].as_str() {
            polling["state"] = json!(state);
        }
        if let Some(code) = last["lastError"].as_str() {
            polling["lastError"] = json!(code);
        }
    }
    Ok(
        json!({"bilagsmail":{"slug":slug,"company":info,"mailAlias":alias,"imapConfigured":g.is_some(),"imapStatus":g.map(|g|&g.status),"inbox":rows,"polling":polling}}),
    )
}
#[allow(clippy::too_many_arguments)]
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    cookies: &CookieVerifier,
    context: Context<'_>,
    key: Option<&ServiceKey>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    fenced(control, company, |ct, lt| {
        let write = !matches!(action, Action::Status);
        if write {
            rentemester_storage::workspace_transfer::require_business_write(ct)?;
        }
        let subject = human(
            ct,
            cookies,
            &context,
            if write {
                Permission::CompanyAdmin
            } else {
                Permission::CompanyDocumentsRead
            },
        )?;
        let at = stamp(context.clock)?.1;
        match action {
            Action::Configure(value) => {
                let bytes = serde_json::to_vec(value).map_err(|_| refused())?;
                let config = imap::Config::parse(&bytes).map_err(|_| {
                    Error::new("imap_config_invalid", "Mailbox configuration refused")
                })?;
                let epoch = new_token().map_err(|_| refused())?.as_str().to_owned();
                let nonce = new_token().map_err(|_| refused())?.as_str().to_owned();
                let envelope=serde_json::to_vec(&json!({"format":"rentemester-imap-config/1","epoch":epoch,"nonce":nonce,"config":value})).map_err(|_|refused())?;
                if envelope.len() > 16384 {
                    return Err(refused());
                }
                let g = Grant {
                    epoch,
                    file_hash: sha(&envelope),
                    status: serde_json::to_value(config.status()).map_err(|_| refused())?,
                    grantor: subject.clone(),
                    authority: authority(ct, &subject, &context.selection, true)?,
                    service: key
                        .map(|k| service(ct, k, context.clock, &context.selection))
                        .transpose()?,
                };
                guard.write_imap_config(&envelope).map_err(|_| refused())?;
                audit(
                    lt,
                    "native_mail_config_set",
                    serde_json::to_value(&g).map_err(|_| refused())?,
                    &subject,
                    &at,
                )?;
            }
            Action::DeleteConfig => {
                guard.remove_imap_config().map_err(|_| refused())?;
                audit(lt, "native_mail_config_deleted", json!({}), &subject, &at)?;
            }
            Action::Alias(alias) => {
                let alias = normalize_alias(alias)?;
                lt.execute("UPDATE companies SET mail_alias=?1 WHERE id=1", [&alias])
                    .map_err(|_| refused())?;
                audit(
                    lt,
                    "native_mail_alias_set",
                    json!({"alias":alias}),
                    &subject,
                    &at,
                )?;
            }
            Action::Status => {}
        }
        let configured = configured(lt, guard)?;
        let grant = configured.as_ref().map(|(g, _)| g);
        let enabled = grant.is_some_and(|g| {
            check_grant(ct, g, &context.selection).is_ok()
                && key
                    .and_then(|key| service(ct, key, context.clock, &context.selection).ok())
                    .as_ref()
                    .is_some_and(|current| g.service.as_ref() == Some(current))
        });
        state(lt, context.selection.slug, grant, enabled)
    })
}
fn normalize_alias(alias: Option<&str>) -> Result<Option<String>, Error> {
    let Some(alias) = alias else { return Ok(None) };
    if alias.is_empty() {
        return Ok(None);
    }
    let value = alias.trim().to_ascii_lowercase();
    let b = value.as_bytes();
    if !(3..=64).contains(&b.len())
        || !b.first().is_some_and(u8::is_ascii_alphanumeric)
        || !b.last().is_some_and(u8::is_ascii_alphanumeric)
        || !b
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._-".contains(c))
    {
        return Err(Error::new(
            "invalid_input",
            "Mail alias must be 3–64 lowercase letters, digits, dots, underscores or hyphens",
        ));
    }
    Ok(Some(value))
}
#[allow(clippy::too_many_arguments)]
fn prepare_inner(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    clock: &dyn Clock,
    s: CompanySelection<'_>,
    authorize: impl FnOnce(&Transaction<'_>) -> Result<(String, String, bool), Error>,
) -> Result<Committed<Admission>, Failure> {
    fenced(control, company, |ct, lt| {
        rentemester_storage::workspace_transfer::require_business_write(ct)?;
        let (subject, binding, is_service) = authorize(ct)?;
        let (g, config) = configured(lt, guard)?
            .ok_or_else(|| Error::new("imap_not_configured", "Mailbox not configured"))?;
        check_grant(ct, &g, &s)?;
        if is_service {
            let expected = g.service.as_ref().ok_or_else(denied)?;
            if expected.subject != subject
                || serde_json::to_string(expected).map_err(|_| refused())? != binding
            {
                return Err(denied());
            }
        }
        let status = config.status();
        let cursor = store::cursor(ct, s.slug, &g.epoch, &status.mailbox)?.map(
            |(uid_validity, last_uid)| imap::Cursor {
                uid_validity,
                last_uid,
            },
        );
        let _ = clock;
        Ok(Admission {
            workspace: s.workspace.into(),
            company: s.slug.into(),
            ledger: ledger(lt)?,
            subject,
            authority: binding,
            grant: g,
            config,
            cursor,
            service: is_service,
        })
    })
}
pub fn prepare(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    cookies: &CookieVerifier,
    context: Context<'_>,
) -> Result<Committed<Admission>, Failure> {
    let s = CompanySelection {
        workspace: context.selection.workspace,
        slug: context.selection.slug,
        registered: context.selection.registered,
        archived: context.selection.archived,
    };
    prepare_inner(control, company, guard, context.clock, s, |ct| {
        let subject = human(ct, cookies, &context, Permission::CompanyAdmin)?;
        let binding = authority(ct, &subject, &context.selection, true)?;
        Ok((subject, binding, false))
    })
}
pub fn prepare_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    key: &ServiceKey,
    clock: &dyn Clock,
    s: CompanySelection<'_>,
) -> Result<Committed<Admission>, Failure> {
    let copy = CompanySelection {
        workspace: s.workspace,
        slug: s.slug,
        registered: s.registered,
        archived: s.archived,
    };
    prepare_inner(control, company, guard, clock, s, |ct| {
        let grant = service(ct, key, clock, &copy)?;
        rentemester_storage::service_context::charge(
            ct,
            key.lookup_hash(),
            stamp(clock)?.0,
            &catalog(&copy),
        )?;
        Ok((
            grant.subject.clone(),
            serde_json::to_string(&grant).map_err(|_| refused())?,
            true,
        ))
    })
}
#[allow(clippy::too_many_arguments)]
fn finish_inner(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    clock: &dyn Clock,
    s: CompanySelection<'_>,
    admission: Admission,
    outcome: Result<imap::Batch, imap::Failure>,
    authorize: impl FnOnce(&Transaction<'_>) -> Result<(String, String, bool), Error>,
) -> Result<Committed<Value>, Failure> {
    fenced(control, company, |ct, lt| {
        rentemester_storage::workspace_transfer::require_business_write(ct)?;
        let (subject, binding, is_service) = authorize(ct)?;
        let (g, _) = configured(lt, guard)?.ok_or_else(refused)?;
        check_grant(ct, &g, &s)?;
        if admission.workspace != s.workspace
            || admission.company != s.slug
            || admission.ledger != ledger(lt)?
            || admission.subject != subject
            || admission.authority != binding
            || admission.service != is_service
            || admission.grant != g
        {
            return Err(denied());
        }
        if is_service
            && g.service
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|_| refused())?
                .as_deref()
                != Some(binding.as_str())
        {
            return Err(denied());
        }
        let current_cursor =
            store::cursor(ct, s.slug, &g.epoch, &admission.config.status().mailbox)?;
        if current_cursor != admission.cursor.map(|c| (c.uid_validity, c.last_uid)) {
            return Err(Error::new(
                "mail_poll_stale",
                "Another poll advanced this mailbox; refresh before polling again",
            ));
        }
        let at = stamp(clock)?.1;
        let actor = format!(
            "{}:{subject}",
            if is_service {
                "service_account"
            } else {
                "user"
            }
        );
        let batch = match outcome {
            Ok(batch) => batch,
            Err(e) => {
                audit_actor(
                    lt,
                    "native_mail_poll",
                    json!({"epoch":g.epoch,"state":"error","lastError":e.code()}),
                    &actor,
                    &at,
                )?;
                return Ok(
                    json!({"ok":false,"code":e.code(),"polling":{"enabled":g.service.is_some(),"state":"error","lastError":e.code()}}),
                );
            }
        };
        if batch.uid_validity == 0
            || batch.messages.len() + batch.refusals.len() > 32
            || batch
                .messages
                .iter()
                .any(|m| m.raw.len() > 25 * 1024 * 1024)
            || batch.messages.iter().map(|m| m.raw.len()).sum::<usize>() > 64 * 1024 * 1024
        {
            return Err(refused());
        }
        let mut uids = BTreeSet::new();
        let floor = admission
            .cursor
            .filter(|c| c.uid_validity == batch.uid_validity)
            .map_or(0, |c| c.last_uid);
        for uid in batch
            .messages
            .iter()
            .map(|m| m.uid)
            .chain(batch.refusals.iter().map(|r| r.uid))
        {
            if uid <= floor || !uids.insert(uid) {
                return Err(refused());
            }
        }
        let observed = |uid| store::MailObservation {
            company_slug: s.slug.into(),
            config_epoch: g.epoch.clone(),
            mailbox: admission.config.status().mailbox,
            uid_validity: batch.uid_validity,
            uid,
        };
        let mut sources = Vec::new();
        let mut duplicates = 0usize;
        for message in batch.messages {
            let observation = observed(message.uid);
            let retained = match rentemester_engine::mail_intake::parse(&message.raw) {
                Ok(parsed) => store::retain(
                    ct,
                    s.slug,
                    &message.raw,
                    &parsed,
                    &observation,
                    Provenance {
                        actor: &actor,
                        now: &at,
                    },
                )?,
                Err(_) => store::retain_invalid(
                    ct,
                    s.slug,
                    &message.raw,
                    &observation,
                    "MAIL_INTAKE_INVALID_MESSAGE",
                    Provenance {
                        actor: &actor,
                        now: &at,
                    },
                )?,
            };
            duplicates += usize::from(retained.duplicate);
            sources.extend(retained.source_ids);
        }
        for rejected in batch.refusals {
            if rejected.declared_size <= 25 * 1024 * 1024 {
                return Err(refused());
            }
            // Runtime refusal codes never supply user text or pretend to retain an EML.
            let retained = store::retain_refusal(
                ct,
                s.slug,
                &observed(rejected.uid),
                rejected.declared_size,
                "MAIL_INTAKE_OVERSIZED",
                Provenance {
                    actor: &actor,
                    now: &at,
                },
            )?;
            duplicates += usize::from(retained.duplicate);
            sources.extend(retained.source_ids);
        }
        audit_actor(
            lt,
            "native_mail_poll",
            json!({"epoch":g.epoch,"state":"idle","hasMore":batch.has_more}),
            &actor,
            &at,
        )?;
        Ok(
            json!({"ok":true,"poll":{"sourceIds":sources,"duplicates":duplicates,"hasMore":batch.has_more},"polling":{"enabled":g.service.is_some(),"state":"idle"}}),
        )
    })
}
#[allow(clippy::too_many_arguments)]
pub fn finish(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    cookies: &CookieVerifier,
    context: Context<'_>,
    admission: Admission,
    outcome: Result<imap::Batch, imap::Failure>,
) -> Result<Committed<Value>, Failure> {
    let s = CompanySelection {
        workspace: context.selection.workspace,
        slug: context.selection.slug,
        registered: context.selection.registered,
        archived: context.selection.archived,
    };
    finish_inner(
        control,
        company,
        guard,
        context.clock,
        s,
        admission,
        outcome,
        |ct| {
            let subject = human(ct, cookies, &context, Permission::CompanyAdmin)?;
            let binding = authority(ct, &subject, &context.selection, true)?;
            Ok((subject, binding, false))
        },
    )
}
#[allow(clippy::too_many_arguments)]
pub fn finish_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    key: &ServiceKey,
    clock: &dyn Clock,
    s: CompanySelection<'_>,
    admission: Admission,
    outcome: Result<imap::Batch, imap::Failure>,
) -> Result<Committed<Value>, Failure> {
    let copy = CompanySelection {
        workspace: s.workspace,
        slug: s.slug,
        registered: s.registered,
        archived: s.archived,
    };
    finish_inner(
        control,
        company,
        guard,
        clock,
        s,
        admission,
        outcome,
        |ct| {
            let g = service(ct, key, clock, &copy)?;
            Ok((
                g.subject.clone(),
                serde_json::to_string(&g).map_err(|_| refused())?,
                true,
            ))
        },
    )
}
pub fn candidates(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    key: &ServiceKey,
    clock: &dyn Clock,
    s: CompanySelection<'_>,
) -> Result<Committed<Value>, Failure> {
    fenced(control, company, |ct, lt| {
        rentemester_storage::workspace_transfer::require_business_write(ct)?;
        let current = service(ct, key, clock, &s)?;
        let configured = configured(lt, guard)?;
        let mut rows = vec![];
        if let Some((g, _)) = configured {
            check_grant(ct, &g, &s)?;
            if g.service.as_ref() == Some(&current) {
                rows.push(json!({"companySlug":s.slug,"epoch":g.epoch}));
            }
        }
        Ok(json!({"rows":rows}))
    })
}
#[cfg(test)]
mod tests;
