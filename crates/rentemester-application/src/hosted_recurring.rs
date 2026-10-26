//! Fixed generation-only hosted operation. Service usage precedes a fresh control/company fence.
use crate::{
    access::{
        self, AccessFacts, CompanyFacts, IdentityFacts, IdentityKind, MembershipFacts, Permission,
        Request, WorkspaceAccessFacts,
    },
    control_fence::{self, Committed, CompanySelection, Failure, FailureStage},
    documents::Context,
    service_company::{BusinessStage, ServiceFailure, ServiceResult, UsageStage},
};
use rentemester_engine::{
    Error,
    scheduled_recurring::{self as engine, refused},
};
use rentemester_runtime::{auth::Clock, namespace::NamespaceGuard, service_key::ServiceKey};
use rentemester_storage::{
    scheduled_recurring::{
        self as store,
        hosted::{self, Binding},
    },
    service_context::{self, Observation},
    workspace_context::{Catalog, RegisteredCompany},
};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap};
pub enum Action<'a> {
    Plan(i64),
    Consent(&'a engine::HostedConsent),
    Revoke(&'a engine::Change),
}
fn time(clock: &dyn Clock) -> Result<(i64, String, String), Error> {
    let n = clock.now_unix_ms().map_err(|_| refused())?;
    Ok((
        n,
        rentemester_runtime::posting_time::latest_date(n, 0).map_err(|_| refused())?,
        hosted::timestamp(n)?,
    ))
}
fn catalog(s: &CompanySelection<'_>) -> Catalog {
    Catalog {
        workspace: s.workspace.into(),
        companies: if s.registered {
            vec![RegisteredCompany {
                slug: s.slug.into(),
                name: s.slug.into(),
                purpose: Default::default(), archived: s.archived,
            }]
        } else {
            vec![]
        },
    }
}
fn parse<T: serde::de::DeserializeOwned>(s: String) -> Result<T, Error> {
    serde::Deserialize::deserialize(serde::de::value::StringDeserializer::<
        serde::de::value::Error,
    >::new(s))
    .map_err(|_| refused())
}
fn permission(
    tx: &Transaction<'_>,
    subject: &str,
    s: &CompanySelection<'_>,
    service: bool,
    p: Permission,
) -> Result<(), Error> {
    let f = rentemester_storage::control_access::observe_human(tx, subject, "", s.slug)?;
    let role = f.workspace_role.map(parse).transpose()?;
    let membership = f.company_role.map(parse).transpose()?;
    let facts = AccessFacts {
        identity: f.user_exists.then_some(IdentityFacts {
            workspace: s.workspace,
            subject,
            authenticated: true,
            kind: if service {
                IdentityKind::Service {
                    registered: true,
                    live_credential: true,
                }
            } else {
                IdentityKind::Human {
                    email_verified: f.email_verified,
                    mfa_enabled: f.mfa_enabled,
                }
            },
        }),
        workspace_access: role.map(|role| WorkspaceAccessFacts {
            workspace: s.workspace,
            subject,
            active: true,
            role,
        }),
        company: Some(CompanyFacts {
            workspace: s.workspace,
            slug: s.slug,
            registered: s.registered,
            archived: s.archived,
            membership: membership.map(|role| MembershipFacts {
                workspace: s.workspace,
                subject,
                company: s.slug,
                active: true,
                role,
            }),
        }),
    };
    if !access::authorize(
        Request {
            workspace: s.workspace,
            company: Some(s.slug),
            permission: p,
        },
        &facts,
    )
    .allowed
    {
        return Err(Error::new(
            "access_denied",
            "current hosted generation authority refused",
        ));
    }
    if !service {
        let ready: bool = tx
            .query_row(
                "SELECT count(*)=1 AND min(verified)=1 FROM twoFactor WHERE userId=?1",
                [subject],
                |r| r.get(0),
            )
            .map_err(|_| refused())?;
        let registered: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM rm_workspace_service_principals WHERE user_id=?1)",
                [subject],
                |r| r.get(0),
            )
            .map_err(|_| refused())?;
        if !ready || registered {
            return Err(Error::new(
                "access_denied",
                "current hosted generation authority refused",
            ));
        }
    }
    Ok(())
}
/// Stable live authority anchors exclude mutable quota counters and human session lifetime.
fn binding(
    tx: &Transaction<'_>,
    o: &Observation,
    grantor: &str,
    s: &CompanySelection<'_>,
) -> Result<(Binding, String), Error> {
    permission(tx, o.subject(), s, true, Permission::CompanyDraftWrite)?;
    permission(tx, grantor, s, false, Permission::CompanyAdmin)?;
    let mut anchors = vec![];
    for subject in [grantor, o.subject()] {
        let access: i64 = tx
            .query_row(
                "SELECT COALESCE(max(id),0) FROM rm_workspace_user_access_events WHERE user_id=?1",
                [subject],
                |r| r.get(0),
            )
            .map_err(|_| refused())?;
        let company:i64=tx.query_row("SELECT COALESCE(max(id),0) FROM rm_company_membership_events WHERE user_id=?1 AND company_slug=?2",[subject,s.slug],|r|r.get(0)).map_err(|_|refused())?;
        anchors.push(json!({"subject":subject,"workspaceEvent":access,"companyEvent":company}));
    }
    let (updated,factor):(String,String)=tx.query_row("SELECT u.updatedAt,t.id FROM user u JOIN twoFactor t ON t.userId=u.id WHERE u.id=?1 AND t.verified=1",[grantor],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|refused())?;
    let security:i64=tx.query_row("SELECT COALESCE(max(id),0) FROM workspace_audit WHERE actor=?1 AND entity_type IN ('native_password_reset_v1','native_mfa_backup_management_v1')",[format!("user:{grantor}")],|r|r.get(0)).map_err(|_|refused())?;
    let credential_event:i64=tx.query_row("SELECT COALESCE(max(id),0) FROM rm_workspace_service_principal_events WHERE user_id=?1",[o.subject()],|r|r.get(0)).map_err(|_|refused())?;
    let name: String = tx
        .query_row(
            "SELECT display_name FROM rm_workspace_service_principals WHERE user_id=?1",
            [o.subject()],
            |r| r.get(0),
        )
        .map_err(|_| refused())?;
    let authority=store::sha(json!({"version":1,"workspace":s.workspace,"company":s.slug,"grantor":grantor,"service":o.subject(),"credential":o.credential_id(),"anchors":anchors,"userUpdatedAt":updated,"factorId":factor,"securityEvent":security,"credentialEvent":credential_event}).to_string().as_bytes());
    Ok((
        Binding {
            service_account_id: o.subject().into(),
            credential_id: o.credential_id().into(),
            grantor_user_id: grantor.into(),
            authority_hash: authority,
            policy: "normal_due_only".into(),
        },
        name,
    ))
}
fn observed(
    tx: &Transaction<'_>,
    key: &ServiceKey,
    clock: &dyn Clock,
    s: &CompanySelection<'_>,
) -> Result<Observation, Error> {
    if key.workspace() != s.workspace {
        return Err(refused());
    }
    service_context::observe(tx, key.lookup_hash(), time(clock)?.0, &catalog(s))
}
#[allow(clippy::too_many_arguments)]
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    _guard: &mut NamespaceGuard,
    context: Context<'_>,
    key: Option<&ServiceKey>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let Context {
        cookie,
        clock,
        selection,
    } = context;
    let proof = RefCell::new(None);
    let times = RefCell::new(None);
    control_fence::execute_authorized_with_backup(
        control,
        company,
        (!matches!(action,Action::Plan(_))).then_some(control_fence::BackupCheck{workspace:selection.workspace,slug:selection.slug,clock}),
        |tx| {
            if !matches!(action,Action::Plan(_)){rentemester_storage::workspace_transfer::require_business_write(tx)?;}
            if cookie.workspace() != selection.workspace {
                return Err(refused());
            }
            let (n, today, now) = time(clock)?;
            let human = rentemester_storage::cookie_session::resolve(
                tx,
                cookie.token(),
                n,
                rentemester_storage::cookie_session::SessionRequirement::Current,
            )?;
            permission(
                tx,
                &human.subject,
                &selection,
                false,
                Permission::CompanyAdmin,
            )?;
            if !matches!(action, Action::Revoke(_)) {
                let key = key.ok_or_else(refused)?;
                let o = observed(tx, key, clock, &selection)?;
                *proof.borrow_mut() = Some(binding(tx, &o, &human.subject, &selection)?);
            }
            *times.borrow_mut() = Some((today, now));
            Ok(human.subject)
        },
        |tx, actor| {
            let times = times.borrow();
            let (today, now) = times.as_ref().ok_or_else(refused)?;
            match action {
                Action::Revoke(i) => {
                    let row = store::current(tx, i.template_id, today)?;
                    if row["executionMode"] != "hosted" {
                        return Err(refused());
                    }
                    Ok(json!({"scheduledRecurring":store::change(tx,"paused",i,actor,today,now)?}))
                }
                Action::Plan(id) => {
                    let proof = proof.borrow();
                    let (b, name) = proof.as_ref().ok_or_else(refused)?;
                    Ok(json!({"hostedPlan":hosted::plan(tx,id,b,name,today)?}))
                }
                Action::Consent(i) => {
                    let proof = proof.borrow();
                    let (b, name) = proof.as_ref().ok_or_else(refused)?;
                    Ok(json!({"scheduledRecurring":hosted::consent(tx,i,b,name,actor,today,now)?}))
                }
            }
        },
    )
}
/// Non-consuming selected-company discovery; every authority is observed under the retained control fence.
pub fn candidates(
    control: &mut Connection,
    company: &mut Connection,
    key: &ServiceKey,
    clock: &dyn Clock,
    selection: CompanySelection<'_>,
    limit: usize,
) -> Result<Committed<Value>, Failure> {
    let source = if rentemester_storage::workspace_transfer::current(control).map_err(initial)?.mode=="active" {hosted::bindings(company).map_err(initial)?}else{vec![]};
    let live = RefCell::new(BTreeMap::new());
    let today = RefCell::new(String::new());
    control_fence::execute_authorized(
        control,
        company,
        |tx| {
            let o = observed(tx, key, clock, &selection)?;
            permission(
                tx,
                o.subject(),
                &selection,
                true,
                Permission::CompanyDraftWrite,
            )?;
            for (id, b) in &source {
                if let Ok((current, _)) = binding(tx, &o, &b.grantor_user_id, &selection)
                    && current == *b
                {
                    live.borrow_mut().insert(*id, current);
                }
            }
            *today.borrow_mut() = time(clock)?.1;
            Ok(o.subject().into())
        },
        |tx, _| {
            let mut rows = vec![];
            for (id, b) in live.borrow().iter() {
                if rows.len() >= limit.min(32) {
                    break;
                }
                if hosted::due(tx, *id, b, &today.borrow())? {
                    rows.push(json!({"companySlug":selection.slug,"templateId":id}));
                }
            }
            Ok(json!({"rows":rows}))
        },
    )
}
fn initial(error: Error) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error,
        control_reusable: true,
        company_reusable: true,
    }
}
fn service_failure(
    error: Error,
    usage: UsageStage,
    business: BusinessStage,
    control_reusable: bool,
    company_reusable: bool,
) -> ServiceFailure {
    ServiceFailure {
        usage,
        business,
        error,
        control_reusable,
        company_reusable,
        rejection: None,
    }
}
#[allow(clippy::too_many_arguments)]
pub fn dispatch(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    key: &ServiceKey,
    clock: &dyn Clock,
    selection: CompanySelection<'_>,
    id: i64,
) -> Result<ServiceResult, ServiceFailure> {
    rentemester_storage::workspace_transfer::require_business_write(control).map_err(|e|service_failure(e,UsageStage::NotCommitted,BusinessStage::NotAttempted,true,true))?;
    // Definite preflight before charging; neither discovery nor a duplicate dispatch consumes quota.
    let found = candidates(
        control,
        company,
        key,
        clock,
        CompanySelection {
            workspace: selection.workspace,
            slug: selection.slug,
            registered: selection.registered,
            archived: selection.archived,
        },
        32,
    )
    .map_err(|f| {
        service_failure(
            f.error,
            UsageStage::NotCommitted,
            BusinessStage::NotAttempted,
            f.control_reusable,
            f.company_reusable,
        )
    })?;
    if !found.control_reusable {
        return Err(service_failure(
            refused(),
            UsageStage::NotCommitted,
            BusinessStage::NotAttempted,
            false,
            true,
        ));
    }
    if !found.value["rows"]
        .as_array()
        .is_some_and(|r| r.iter().any(|v| v["templateId"] == id))
    {
        return Err(service_failure(
            refused(),
            UsageStage::NotCommitted,
            BusinessStage::NotAttempted,
            true,
            true,
        ));
    }
    let source = hosted::bindings(company)
        .map_err(|e| {
            service_failure(
                e,
                UsageStage::NotCommitted,
                BusinessStage::NotAttempted,
                true,
                true,
            )
        })?
        .into_iter()
        .find(|(n, _)| *n == id)
        .ok_or_else(|| {
            service_failure(
                refused(),
                UsageStage::NotCommitted,
                BusinessStage::NotAttempted,
                true,
                true,
            )
        })?
        .1;
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| {
            service_failure(
                refused(),
                UsageStage::NotCommitted,
                BusinessStage::NotAttempted,
                false,
                true,
            )
        })?;
    let charge = (|| {
 rentemester_storage::workspace_transfer::require_business_write(&tx)?;
        let o = observed(&tx, key, clock, &selection)?;
        let (b, _) = binding(&tx, &o, &source.grantor_user_id, &selection)?;
        if b != source {
            return Err(refused());
        }
        let charged =
            service_context::charge(&tx, key.lookup_hash(), time(clock)?.0, &catalog(&selection))?;
        if !charged.same_credential(&o) {
            return Err(refused());
        }
        Ok(charged)
    })();
    let charged = match charge {
        Ok(v) => v,
        Err(e) => {
            let clean = tx.rollback().is_ok();
            return Err(service_failure(
                e,
                if clean {
                    UsageStage::NotCommitted
                } else {
                    UsageStage::OutcomeUnknown
                },
                BusinessStage::NotAttempted,
                clean,
                true,
            ));
        }
    };
    if tx.commit().is_err() {
        return Err(service_failure(
            refused(),
            UsageStage::OutcomeUnknown,
            BusinessStage::NotAttempted,
            false,
            true,
        ));
    }
    let fresh = RefCell::new(None);
    let times = RefCell::new(None);
    let mut attempted = false;
    let result = control_fence::execute_authorized_with_backup(
        control,
        company,
        Some(control_fence::BackupCheck{workspace:selection.workspace,slug:selection.slug,clock}),
        |tx| {rentemester_storage::workspace_transfer::require_business_write(tx)?;
            let o = observed(tx, key, clock, &selection)?;
            if !o.same_credential(&charged) {
                return Err(refused());
            }
            let (b, _) = binding(tx, &o, &source.grantor_user_id, &selection)?;
            if b != source {
                return Err(refused());
            }
            *fresh.borrow_mut() = Some(b);
            let (_, today, now) = time(clock)?;
            *times.borrow_mut() = Some((today, now));
            Ok(o.subject().into())
        },
        |tx, actor| {
            attempted = true;
            let fresh = fresh.borrow();
            let b = fresh.as_ref().ok_or_else(refused)?;
            if actor != b.service_account_id {
                return Err(refused());
            }
            let times = times.borrow();
            let (today, now) = times.as_ref().ok_or_else(refused)?;
            let occurrence = hosted::begin(tx, id, b, today, now)?;
            let issued = crate::scheduled_recurring::issue(
                tx,
                guard,
                id,
                &occurrence,
                actor,
                today,
                now,
                rentemester_storage::idempotency::PrincipalKind::ServiceAccount,
            )?;
            hosted::finish(tx, id, occurrence, issued, b, today, now)
        },
    );
    match result {
        Ok(c) => Ok(ServiceResult {
            usage: UsageStage::Committed,
            business: BusinessStage::Committed,
            result: c.value,
            receipt: None,
            control_reusable: c.control_reusable,
        }),
        Err(f) => Err(service_failure(
            f.error,
            UsageStage::Committed,
            if f.stage == FailureStage::OutcomeUnknown {
                BusinessStage::OutcomeUnknown
            } else if attempted {
                BusinessStage::NotCommitted
            } else {
                BusinessStage::NotAttempted
            },
            f.control_reusable,
            f.company_reusable,
        )),
    }
}
