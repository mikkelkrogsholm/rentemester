//! One current control transaction for task authority, receipts and operational state.
//! Never opens a company ledger or treats historical task labels as authority.
use crate::{
    access::{self, CompanyRole, Permission, WorkspaceRole},
    mfa_login::{error, failure},
    password_login::{Failure, FailureStage},
};
use rentemester_engine::{
    Error,
    tasks::{self as domain, Scope, Task},
};
use rentemester_runtime::{
    auth::{Clock, CookieVerifier, VerifiedCookie},
    service_key::ServiceKey,
};
use rentemester_storage::{tasks as store, workspace_context::Catalog};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use serde_json::{Value, json};
use std::collections::BTreeSet;

mod boards;
mod series;
pub enum Authority<'a> {
    Cookie {
        cookies: &'a CookieVerifier,
        proof: &'a VerifiedCookie,
    },
    Service(&'a ServiceKey),
}
pub enum Action<'a> {
    List(&'a Value),
    BoardsList(&'a Value),
    BoardsPreview(&'a Value),
    BoardsSave(&'a Value),
    SeriesList(&'a Value),
    SeriesProject(&'a Value),
    SeriesSave(&'a Value),
    SeriesMaterialize(&'a Value),
    Get(&'a str),
    History(&'a str),
    Receipt(&'a str),
    Create(&'a Value),
    Update { id: &'a str, patch: &'a Value },
    Move { id: &'a str, input: &'a Value },
    Complete { id: &'a str, input: &'a Value },
    Reopen { id: &'a str, reason: &'a Value },
}
impl Action<'_> {
    fn writes(&self) -> bool {
        matches!(
            self,
            Self::BoardsSave(_)
                | Self::SeriesSave(_)
                | Self::SeriesMaterialize(_)
                | Self::Create(_)
                | Self::Update { .. }
                | Self::Move { .. }
                | Self::Complete { .. }
                | Self::Reopen { .. }
        )
    }
}
struct Identity {
    subject: String,
    actor: String,
    principal: String,
}
struct Access {
    owner: bool,
    read: BTreeSet<String>,
    write: BTreeSet<String>,
    manage: BTreeSet<String>,
}
fn denied() -> Error {
    error("access_denied")
}
fn observe(
    tx: &Transaction<'_>,
    catalog: &Catalog,
    authority: &Authority<'_>,
    now: i64,
) -> Result<Identity, Error> {
    use rentemester_storage::{cookie_session, service_context, session_control};
    match authority {
        Authority::Cookie { cookies, proof } => {
            if !cookies.accepts_session(proof) || proof.workspace() != catalog.workspace {
                return Err(error("invalid_credentials"));
            }
            let current = cookie_session::resolve(
                tx,
                proof.token(),
                now,
                cookie_session::SessionRequirement::Current,
            )?;
            session_control::authorize(tx, &current)?;
            let subject = current.subject;
            Ok(Identity {
                actor: format!("user:{subject}"),
                principal: format!("user:{subject}"),
                subject,
            })
        }
        Authority::Service(key) => {
            if key.workspace() != catalog.workspace {
                return Err(error("invalid_credentials"));
            }
            let current = service_context::observe(tx, key.lookup_hash(), now, catalog)?;
            let subject = current.subject().to_owned();
            Ok(Identity {
                actor: format!("service_account:{subject}"),
                principal: format!("service_account:{subject}"),
                subject,
            })
        }
    }
}
fn access_for(tx: &Transaction<'_>, subject: &str, catalog: &Catalog) -> Result<Access, Error> {
    let role = rentemester_storage::workspace_members::access(tx, subject)?.ok_or_else(denied)?;
    let role: WorkspaceRole =
        serde_json::from_value(json!(role)).map_err(|_| error("invalid_control"))?;
    if !access::workspace_role_allows(role, Permission::WorkspaceTasksRead) {
        return Err(denied());
    }
    let mut sets = Access {
        owner: access::workspace_role_allows(role, Permission::WorkspaceTasksManage),
        read: BTreeSet::new(),
        write: BTreeSet::new(),
        manage: BTreeSet::new(),
    };
    // Archived Live duties retain their task access. No financial permission changes.
    for company in catalog
        .companies
        .iter()
        .filter(|c| c.purpose == rentemester_engine::workspace_manifest::Purpose::Live)
    {
        if let Some(role) =
            rentemester_storage::control_access::observe_company_role(tx, subject, &company.slug)?
        {
            let role: CompanyRole =
                serde_json::from_value(json!(role)).map_err(|_| error("invalid_control"))?;
            if access::company_role_allows(role, Permission::CompanyTasksRead) {
                sets.read.insert(company.slug.clone());
            }
            if access::company_role_allows(role, Permission::CompanyTasksWrite) {
                sets.write.insert(company.slug.clone());
            }
            if access::company_role_allows(role, Permission::CompanyTasksManage) {
                sets.manage.insert(company.slug.clone());
            }
        }
    }
    Ok(sets)
}
fn scope_allowed(access: &Access, scope: &Scope, write: bool) -> bool {
    let slugs = scope.companies();
    if slugs.is_empty() {
        access.owner
    } else {
        slugs.iter().all(|s| {
            if write {
                access.write.contains(*s)
            } else {
                access.read.contains(*s)
            }
        })
    }
}
fn authorize_task(
    _tx: &Transaction<'_>,
    _catalog: &Catalog,
    access: &Access,
    task: &Task,
    write: bool,
) -> Result<(), Error> {
    if !scope_allowed(access, &task.scope, write) {
        return Err(denied());
    }
    domain::references(&json!(task.references), &task.scope)?;
    if let Some(completion) = &task.completion {
        for refs in completion.references.chunks(100) {
            domain::references(&json!(refs), &task.scope)?;
        }
    }
    if task
        .source
        .as_ref()
        .is_some_and(|s| !task.scope.companies().contains(&s.company_slug.as_str()))
    {
        return Err(denied());
    }
    Ok(())
}
fn recipients(tx: &Transaction<'_>, catalog: &Catalog, task: &Task) -> Result<(), Error> {
    let mut recipients = Vec::new();
    if task.assignee["kind"] == "member" {
        recipients.push(
            task.assignee["userId"]
                .as_str()
                .ok_or_else(|| error("invalid_assignee"))?,
        );
    }
    for recipient in recipients {
        let ready = recipient_access(tx, recipient, catalog)?;
        if !scope_allowed(&ready, &task.scope, false) {
            return Err(error("invalid_assignee"));
        }
    }
    Ok(())
}
fn recipient_access(
    tx: &Transaction<'_>,
    subject: &str,
    catalog: &Catalog,
) -> Result<Access, Error> {
    let facts = rentemester_storage::control_access::observe_human(tx, subject, "", "")?;
    let service: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM rm_workspace_service_principals WHERE user_id=?1)",
            [subject],
            |r| r.get(0),
        )
        .map_err(|_| error("invalid_control"))?;
    if !facts.user_exists || !service && (!facts.email_verified || !facts.mfa_enabled) {
        return Err(error("invalid_assignee"));
    }
    access_for(tx, subject, catalog).map_err(|e| {
        if e.code == "access_denied" {
            error("invalid_assignee")
        } else {
            e
        }
    })
}
fn visible_history(
    tx: &Transaction<'_>,
    id: &str,
    catalog: &Catalog,
    access: &Access,
) -> Result<Vec<store::Event>, Error> {
    store::history_visible(tx, id, |t| {
        if !scope_allowed(access, &t.scope, false) {
            return Ok(false);
        }
        authorize_task(tx, catalog, access, t, false)?;
        Ok(true)
    })
}
fn manual_draft(input: &Value) -> Result<Value, Error> {
    let m = input.as_object().ok_or_else(|| error("invalid_input"))?;
    let fields = [
        "title",
        "description",
        "nextAction",
        "scope",
        "type",
        "assignee",
        "waitingOn",
        "workDate",
        "deadline",
        "period",
        "references",
        "evidenceRequired",
        "relevance",
        "verificationRequired",
    ];
    if m.keys().any(|k| !fields.contains(&k.as_str())) {
        return Err(error("invalid_input"));
    }
    let mut out = input.clone();
    out["origin"] = json!("manual");
    domain::draft(&out)?;
    Ok(out)
}
fn get_current(
    tx: &Transaction<'_>,
    id: &str,
    catalog: &Catalog,
    access: &Access,
    write: bool,
) -> Result<Task, Error> {
    store::get(tx, id, |t| authorize_task(tx, catalog, access, t, write))?
        .ok_or_else(|| error("not_found"))
}
fn view(
    tx: &Transaction<'_>,
    input: &Value,
    catalog: &Catalog,
    id: &Identity,
    access: &Access,
    now: &str,
) -> Result<Value, Error> {
    let query = input.as_object().ok_or_else(|| error("invalid_input"))?;
    let allowed = [
        "companySlugs",
        "includeArchived",
        "status",
        "type",
        "search",
        "assigneeId",
        "from",
        "to",
        "undated",
        "unassigned",
        "showDone",
    ];
    if query.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(error("invalid_input"));
    }
    let selected = if let Some(v) = query.get("companySlugs") {
        let a = v
            .as_array()
            .filter(|a| a.len() <= 100)
            .ok_or_else(|| error("invalid_input"))?;
        a.iter()
            .map(|v| {
                v.as_str()
                    .filter(|s| access.read.contains(*s))
                    .map(String::from)
                    .ok_or_else(denied)
            })
            .collect::<Result<BTreeSet<_>, _>>()?
    } else {
        access.read.clone()
    };
    for k in ["includeArchived", "undated", "unassigned", "showDone"] {
        if query.get(k).is_some_and(|v| !v.is_boolean()) {
            return Err(error("invalid_input"));
        }
    }
    for k in ["from", "to"] {
        if let Some(v) = query.get(k) {
            domain::date(v)?;
        }
    }
    if let Some(v) = query.get("status") {
        if !domain::STATUSES.contains(&v.as_str().unwrap_or("")) {
            return Err(error("invalid_input"));
        }
    }
    if let Some(v) = query.get("type") {
        if !["ad_hoc", "routine", "obligation"].contains(&v.as_str().unwrap_or("")) {
            return Err(error("invalid_input"));
        }
    }
    let search = query
        .get("search")
        .map(|v| domain::text(v, "search", 240, false))
        .transpose()?
        .unwrap_or_default()
        .to_lowercase();
    if let Some(v) = query.get("assigneeId") {
        domain::id(v)?;
    }
    let mut tasks = store::list(tx, |task| {
        if !scope_allowed(access, &task.scope, false) {
            return Ok(false);
        }
        authorize_task(tx, catalog, access, task, false)?;
        Ok(true)
    })?;
    tasks.retain(|t| {
        let companies = t.scope.companies();
        if query.contains_key("companySlugs") && !companies.iter().any(|s| selected.contains(*s)) {
            return false;
        }
        if input["includeArchived"] != true
            && catalog
                .companies
                .iter()
                .any(|c| c.archived && companies.contains(&c.slug.as_str()))
        {
            return false;
        }
        if let Some(status) = query.get("status") {
            if status != &json!(t.status) {
                return false;
            }
        } else if input["showDone"] != true && t.status == "done" {
            return false;
        }
        if query.get("type").is_some_and(|v| v != &json!(t.task_type)) {
            return false;
        }
        if query
            .get("assigneeId")
            .is_some_and(|v| t.assignee["kind"] != "member" || t.assignee["userId"] != *v)
        {
            return false;
        }
        if input["undated"] == true && (t.work_date.is_some() || !t.deadline.is_null()) {
            return false;
        }
        if input["unassigned"] == true && !t.assignee.is_null() {
            return false;
        }
        let haystack = format!(
            "{} {} {}",
            t.title,
            t.task_id,
            t.references
                .iter()
                .map(|r| r.reference.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        )
        .to_lowercase();
        search.is_empty() || haystack.contains(&search)
    });
    // Calendar ranges constrain future projections, never hide older outstanding duties.
    let stored = store::boards(tx)?;
    let mut boards = Vec::new();
    for scope in access
        .read
        .iter()
        .map(|s| Scope::Company {
            company_slug: s.clone(),
        })
        .chain(std::iter::once(Scope::Workspace {
            company_slugs: vec![],
        }))
    {
        boards.push(stored.iter().find(|b|b.board_id==scope.board_id()).map(|b|json!(b)).unwrap_or_else(||json!({"boardId":scope.board_id(),"version":0,"scope":scope,"updatedAt":"1970-01-01T00:00:00.000Z","columns":domain::STATUSES.iter().map(|s|json!({"columnId":s,"name":match *s{"open"=>"Åben","in_progress"=>"I gang","waiting"=>"Afventer",_=>"Færdig"},"status":s,"isDefault":true})).collect::<Vec<_>>()})));
    }
    let mut assignees = Vec::new();
    let mut q = tx
        .prepare("SELECT id,name FROM user ORDER BY name,id")
        .map_err(|_| error("invalid_control"))?;
    for row in q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|_| error("invalid_control"))?
    {
        let (user, name) = row.map_err(|_| error("invalid_control"))?;
        match recipient_access(tx, &user, catalog) {
            Ok(candidate)
                if if selected.is_empty() {
                    candidate.owner && access.owner
                } else {
                    selected.iter().all(|s| candidate.read.contains(s))
                } =>
            {
                assignees.push(json!({"userId":user,"name":name}))
            }
            Ok(_) => {}
            Err(e) if e.code == "invalid_assignee" => {}
            Err(e) => return Err(e),
        }
    }
    let (series, projections) = series::metadata(tx, input, catalog, access, now)?;
    let notifications: i64 = tx
        .query_row("SELECT count(*) FROM rm_task_notifications", [], |r| {
            r.get(0)
        })
        .map_err(|_| error("invalid_control"))?;
    if notifications != 0 {
        return Err(error("task_notifications_unavailable"));
    }
    Ok(
        json!({"count":tasks.len(),"tasks":tasks,"boards":boards,"series":series,"projections":projections,"companies":catalog.companies.iter().filter(|c|access.read.contains(&c.slug)).map(|c|json!({"slug":c.slug,"name":c.name,"archived":c.archived,"canWrite":access.write.contains(&c.slug),"canManage":access.manage.contains(&c.slug)})).collect::<Vec<_>>(),"currentUserId":id.subject,"canManageWorkspace":access.owner,"assignees":assignees,"runtime":{"running":false,"lastTickAt":null,"lastError":null},"notifications":[],"sourceCoverage":["Manuelle opgaver. Automatisk opfølgning er ikke aktiveret."]}),
    )
}
#[allow(clippy::too_many_arguments)]
pub fn execute(
    db: &mut Connection,
    catalog: &Catalog,
    clock: &dyn Clock,
    authority: Authority<'_>,
    action: Action<'_>,
    key: Option<&str>,
    expected: Option<u64>,
) -> Result<Value, Failure> {
    crate::control_fence::connection_ready(db).map_err(|e| failure(e, false))?;
    rentemester_storage::workspace_context::validate_catalog(catalog)
        .map_err(|e| failure(e, true))?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let mut charged = false;
    let mut savepoint_safe = true;
    let result = (|| {
        let now = clock
            .now_unix_ms()
            .map_err(|_| error("invalid_auth_clock"))?;
        let stamp = rentemester_engine::company_knowledge::timestamp(now)?;
        let identity = observe(&tx, catalog, &authority, now)?;
        let access = access_for(&tx, &identity.subject, catalog)?;
        series::preadmit(&tx, &action, &access, expected)?;
        boards::preadmit(&tx, &action, catalog, &access, expected)?;
        let writes = action.writes();
        if writes {
            rentemester_storage::workspace_transfer::require_business_write(&tx)?;
        }
        let draft = if let Action::Create(v) = &action {
            Some(manual_draft(v)?)
        } else {
            None
        };
        let generated = format!(
            "task-{}",
            rentemester_runtime::auth::new_token()
                .map_err(|_| error("entropy_unavailable"))?
                .as_str()
        );
        if let Some(draft) = &draft {
            let candidate = domain::prepare_create(draft, &generated, &stamp)?;
            authorize_task(&tx, catalog, &access, &candidate, true)?;
        }
        let existing = match &action {
            Action::Update { id, .. }
            | Action::Move { id, .. }
            | Action::Complete { id, .. }
            | Action::Reopen { id, .. } => Some(get_current(&tx, id, catalog, &access, true)?),
            _ => None,
        };
        if let Action::Update { patch, .. } = &action {
            let fields = [
                "title",
                "description",
                "nextAction",
                "scope",
                "assignee",
                "waitingOn",
                "workDate",
                "deadline",
                "period",
                "references",
                "evidenceRequired",
                "relevance",
                "verificationRequired",
            ];
            if patch
                .as_object()
                .is_none_or(|m| m.keys().any(|k| !fields.contains(&k.as_str())))
            {
                return Err(error("invalid_input"));
            }
            if let Some(scope) = patch.get("scope") {
                let scope = domain::scope(scope)?;
                if !scope_allowed(&access, &scope, true) {
                    return Err(denied());
                }
                if existing.as_ref().unwrap().source.is_some()
                    && scope != existing.as_ref().unwrap().scope
                {
                    return Err(error("invalid_scope"));
                }
            }
        }
        let read = match &action {
            Action::List(q) => Some(view(&tx, q, catalog, &identity, &access, &stamp)?),
            Action::SeriesList(_) | Action::SeriesProject(_) => {
                Some(series::read(&tx, &action, catalog, &access, &stamp)?)
            }
            Action::BoardsList(input) => Some(boards::list(&tx, input, &access)?),
            Action::BoardsPreview(input) => {
                Some(json!({"preview":boards::preview(&tx,input,catalog,&access)?.preview}))
            }
            Action::Get(id) => {
                let task = get_current(&tx, id, catalog, &access, false)?;
                let history = visible_history(&tx, id, catalog, &access)?;
                Some(json!({"task":task,"history":history}))
            }
            Action::History(id) => {
                get_current(&tx, id, catalog, &access, false)?;
                Some(json!({"history":visible_history(&tx,id,catalog,&access)?}))
            }
            Action::Receipt(key) => Some(
                json!({"receipt":boards::receipt(&tx,&identity.principal,key,catalog,&access)?}),
            ),
            _ => None,
        };
        if let Authority::Service(key) = &authority {
            if rentemester_storage::workspace_transfer::current(&tx)?.mode == "active" {
                let observed = rentemester_storage::service_context::charge(
                    &tx,
                    key.lookup_hash(),
                    now,
                    catalog,
                )?;
                if observed.subject() != identity.subject {
                    return Err(denied());
                }
                charged = true;
            }
        }
        if let Some(value) = read {
            return Ok(value);
        }
        tx.execute_batch("SAVEPOINT task_effect")
            .map_err(|_| error("connection_refused"))?;
        savepoint_safe = false;
        let ctx = store::Context {
            actor: &identity.actor,
            principal: &identity.principal,
            idempotency_key: key.ok_or_else(|| error("invalid_input"))?,
            expected_version: expected,
            now: &stamp,
            source_check: None,
        };
        let result = if let Action::BoardsSave(input) = &action {
            boards::save(&tx, input, &ctx, catalog, &access)
        } else if matches!(action, Action::SeriesSave(_) | Action::SeriesMaterialize(_)) {
            series::mutate(&tx, &action, &ctx, catalog, &access)
        } else {
            let command = match &action {
                Action::Create(_) => store::Command::Create {
                    input: draft.as_ref().unwrap(),
                    generated_id: &generated,
                },
                Action::Update { id, patch } => store::Command::Update { id, patch },
                Action::Move { id, input } => store::Command::Move { id, input },
                Action::Complete { id, input } => store::Command::Complete { id, input },
                Action::Reopen { id, reason } => store::Command::Reopen { id, reason },
                _ => unreachable!(),
            };
            store::mutate_in_tx_checked(
                &tx,
                command,
                ctx,
                |tx, t| authorize_task(tx, catalog, &access, t, true),
                |tx, t| {
                    if matches!(action, Action::Create(_) | Action::Update { .. }) {
                        recipients(tx, catalog, t)
                    } else {
                        Ok(())
                    }
                },
            )
            .map(|out| json!({"task":out.task}))
        };
        match result {
            Ok(out) => {
                tx.execute_batch("RELEASE task_effect")
                    .map_err(|_| error("connection_refused"))?;
                savepoint_safe = true;
                Ok(out)
            }
            Err(e) => {
                tx.execute_batch("ROLLBACK TO task_effect; RELEASE task_effect")
                    .map_err(|_| error("connection_refused"))?;
                savepoint_safe = true;
                Err(e)
            }
        }
    })();
    let result = if savepoint_safe {
        result
    } else {
        Err(error("connection_refused"))
    };
    if (result.is_ok() && action.writes() || charged)
        && !result
            .as_ref()
            .is_err_and(|e| e.code == "connection_refused")
    {
        tx.commit().map_err(|_| Failure {
            stage: FailureStage::OutcomeUnknown,
            error: error("commit_outcome_unknown"),
            connection_reusable: false,
        })?;
    } else if tx.rollback().is_err() {
        return Err(failure(error("connection_refused"), false));
    }
    result.map_err(|e| {
        let reusable = e.code != "connection_refused";
        failure(e, reusable)
    })
}

#[cfg(test)]
mod tests;
