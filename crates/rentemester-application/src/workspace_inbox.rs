//! Current authority and durable cross-database document handoff under one Owner mutex.
use crate::{
    access::{self, CompanyRole, Permission, WorkspaceRole},
    control_fence::{self, Committed, Failure, FailureStage},
    documents::Context,
};
use rentemester_engine::{Error, document::Metadata};
use rentemester_runtime::{auth::CookieVerifier, document::Prepared, namespace::NamespaceGuard};
use rentemester_storage::{workspace_context::Catalog, workspace_inbox as storage};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub use storage::{Destination, Ingest};
pub enum Action<'a> {
    List { cursor: usize, limit: usize },
    Inspect(&'a str),
    Ingest(&'a Ingest, &'a Prepared),
    Assign(&'a str, &'a Destination),
    Complete(&'a str, &'a Destination),
}
pub struct Runtime<'a> {
    pub cookies: &'a CookieVerifier,
    pub catalog: &'a Catalog,
    pub anchor: &'a str,
}
fn error(code: &'static str) -> Error {
    Error::new(
        code,
        "workspace inbox action refused; inspect source and current permissions",
    )
}
fn failure(e: Error, reusable: bool) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error: e,
        control_reusable: reusable,
        company_reusable: true,
    }
}
fn transact<T>(
    db: &mut Connection,
    work: impl FnOnce(&Transaction<'_>) -> Result<T, Error>,
) -> Result<T, Failure> {
    control_fence::connection_ready(db).map_err(|e| failure(e, false))?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    match work(&tx) {
        Ok(v) => {
            tx.commit().map_err(|_| Failure {
                stage: FailureStage::OutcomeUnknown,
                error: error("commit_outcome_unknown"),
                control_reusable: false,
                company_reusable: true,
            })?;
            Ok(v)
        }
        Err(e) => {
            let reuse = tx.rollback().is_ok();
            Err(failure(e, reuse))
        }
    }
}
/// Permissions are observed from current events on the held control writer. The
/// workspace role never supplies implicit company permissions or hidden targets.
fn authorize(
    tx: &Transaction<'_>,
    r: &Runtime<'_>,
    c: &Context<'_>,
    write: bool,
    target: Option<&str>,
) -> Result<(String, BTreeSet<String>, String), Error> {
    if !r.cookies.accepts_session(c.cookie)
        || c.cookie.workspace() != r.catalog.workspace
        || c.selection.workspace != r.catalog.workspace
    {
        return Err(error("invalid_credentials"));
    }
    rentemester_storage::workspace_context::validate_catalog(r.catalog)?;
    let now = c
        .clock
        .now_unix_ms()
        .map_err(|_| error("invalid_auth_clock"))?;
    let current = rentemester_storage::cookie_session::resolve(
        tx,
        c.cookie.token(),
        now,
        rentemester_storage::cookie_session::SessionRequirement::Current,
    )?;
    rentemester_storage::session_control::authorize(tx, &current)?;
    rentemester_storage::workspace_members::company(r.catalog, r.anchor, !write)?;
    let facts = rentemester_storage::control_access::observe_human(
        tx,
        &current.subject,
        &current.session_id,
        r.anchor,
    )?;
    let wr: WorkspaceRole = serde_json::from_value(json!(
        facts.workspace_role.ok_or_else(|| error("access_denied"))?
    ))
    .map_err(|_| error("invalid_control"))?;
    if !access::workspace_role_allows(wr, Permission::WorkspaceRead) {
        return Err(error("access_denied"));
    }
    let role: CompanyRole = serde_json::from_value(json!(
        facts.company_role.ok_or_else(|| error("access_denied"))?
    ))
    .map_err(|_| error("invalid_control"))?;
    if !access::company_role_allows(
        role,
        if write {
            Permission::CompanyDocumentsUpload
        } else {
            Permission::CompanyDocumentsRead
        },
    ) {
        return Err(error("access_denied"));
    }
    let mut visible = BTreeSet::new();
    let mut writable = BTreeSet::new();
    for company in &r.catalog.companies {
        if let Some(role) = rentemester_storage::control_access::observe_company_role(
            tx,
            &current.subject,
            &company.slug,
        )? {
            let role: CompanyRole =
                serde_json::from_value(json!(role)).map_err(|_| error("invalid_control"))?;
            if access::company_role_allows(role, Permission::CompanyDocumentsRead) {
                visible.insert(company.slug.clone());
            }
            if !company.archived
                && access::company_role_allows(role, Permission::CompanyDocumentsUpload)
            {
                writable.insert(company.slug.clone());
            }
        }
    }
    if let Some(t) = target {
        rentemester_storage::workspace_members::company(r.catalog, t, false)?;
        if !writable.contains(t) {
            return Err(error("access_denied"));
        }
    }
    Ok((
        current.subject,
        visible,
        rentemester_engine::company_knowledge::timestamp(now)?,
    ))
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    runtime: Runtime<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let (id, destination) = if let Action::Complete(id, d) = action {
        (id, d)
    } else {
        let value = transact(control, |tx| {
            let (subject, visible, now) = authorize(
                tx,
                &runtime,
                &context,
                !matches!(action, Action::List { .. } | Action::Inspect(_)),
                if let Action::Assign(_, d) = &action {
                    Some(d.company_slug.as_str())
                } else {
                    None
                },
            )?;
            let actor = format!("user:{subject}");
            match action {
                Action::List { cursor, limit } => {
                    storage::list(tx, runtime.anchor, &visible, cursor, limit)
                }
                Action::Inspect(id) => {
                    storage::inspect(tx, runtime.anchor, id, &visible).map(|s| json!({"source":s}))
                }
                Action::Ingest(i, file) => {
                    if file.mime() != i.mime_type {
                        return Err(storage::refused(
                            "declared MIME differs from verified original",
                        ));
                    }
                    let generated = format!(
                        "inbox-{}",
                        rentemester_runtime::auth::new_token()
                            .map_err(|_| error("entropy_unavailable"))?
                            .as_str()
                    );
                    storage::ingest(
                        tx,
                        runtime.anchor,
                        i,
                        file.bytes(),
                        &generated,
                        &visible,
                        storage::Provenance {
                            actor: &actor,
                            now: &now,
                        },
                    )
                    .map(|s| json!({"source":s}))
                }
                Action::Assign(id, d) => storage::assign(
                    tx,
                    runtime.anchor,
                    id,
                    &d.company_slug,
                    &visible,
                    &actor,
                    &now,
                )
                .map(|s| json!({"source":s})),
                Action::Complete(..) => unreachable!(),
            }
        })?;
        return Ok(Committed {
            value,
            control_reusable: true,
        });
    };
    if context.selection.slug != destination.company_slug {
        return Err(failure(error("company_selection_mismatch"), true));
    }
    // Commit the claim before any company mutation. No method exposes owned handles;
    // the host mutex remains held across all three database transactions.
    let (source, file, metadata, claim, completed) = transact(control, |tx| {
        let (_, visible, now) = authorize(
            tx,
            &runtime,
            &context,
            true,
            Some(&destination.company_slug),
        )?;
        let source = storage::inspect(tx, runtime.anchor, id, &visible)?;
        let bytes = storage::original(tx, id)?;
        let file = Prepared::new(
            source["filename"]
                .as_str()
                .ok_or_else(|| error("invalid_control"))?
                .into(),
            bytes,
        )
        .map_err(storage::refused)?;
        if source["mimeType"] != file.mime() {
            return Err(storage::refused("stored MIME differs from original"));
        }
        let mut metadata = source["metadata"].clone();
        if !metadata.is_object() {
            return Err(storage::refused("metadata object required"));
        }
        metadata["source"] = json!(format!("workspace-inbox:{id}"));
        let metadata: Metadata = serde_json::from_value(metadata).map_err(|_| {
            storage::refused(
                "explicit supported document classification and metadata required before handoff",
            )
        })?;
        metadata.validate()?;
        let assignment = source["assignments"]
            .as_array()
            .and_then(|a| {
                a.iter()
                    .find(|a| a["companySlug"] == destination.company_slug)
            })
            .ok_or_else(|| storage::refused("source has no approved target assignment"))?;
        let completed = assignment["state"] == "completed";
        let claim = format!(
            "inbox-claim-{}",
            rentemester_runtime::auth::new_token()
                .map_err(|_| error("entropy_unavailable"))?
                .as_str()
        );
        if !completed {
            let now_ms = context
                .clock
                .now_unix_ms()
                .map_err(|_| error("invalid_auth_clock"))?;
            let expires = rentemester_engine::company_knowledge::timestamp(
                now_ms
                    .checked_add(120_000)
                    .ok_or_else(|| error("invalid_auth_clock"))?,
            )?;
            storage::claim(tx, id, &destination.company_slug, &claim, &now, &expires)?;
        }
        Ok((source, file, metadata, claim, completed))
    })?;
    let today = rentemester_runtime::posting_time::latest_date(
        context
            .clock
            .now_unix_ms()
            .map_err(|_| failure(error("invalid_auth_clock"), true))?,
        0,
    )
    .map_err(|_| failure(error("invalid_auth_clock"), true))?;
    let document = control_fence::execute_cookie_subject(
        control,
        company,
        control_fence::CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: control_fence::SessionRequirement::Current,
        },
        control_fence::CompanySelection {
            workspace: context.selection.workspace,
            slug: context.selection.slug,
            registered: context.selection.registered,
            archived: context.selection.archived,
        },
        Permission::CompanyDocumentsUpload,
        |tx, subject| {
            if let Some(document) =
                storage::recovered_document(tx, &metadata.source, file.hash(), &metadata)?
            {
                let id = document["id"]
                    .as_i64()
                    .ok_or_else(|| error("invalid_company"))?;
                let evidence = rentemester_storage::documents::evidence(tx, id)?;
                let bytes = guard
                    .read_document(&evidence.path, &evidence.hash)
                    .map_err(|_| storage::refused("recovered original unavailable"))?;
                if bytes != file.bytes() {
                    return Err(storage::refused("recovered original differs"));
                }
                return Ok(document);
            }
            if completed {
                return Err(storage::refused(
                    "completed handoff document missing; reconciliation required",
                ));
            }
            let path = guard
                .publish_document(&file)
                .map_err(|_| storage::refused("protected source publication unavailable"))?;
            let registered = rentemester_storage::documents::register(
                tx,
                rentemester_storage::documents::Original {
                    filename: file.filename(),
                    hash: file.hash(),
                    path: &path,
                    mime: file.mime(),
                },
                &metadata,
                subject,
                &today,
            )?;
            Ok(registered["document"].clone())
        },
    )?;
    if completed {
        let assigned = source["assignments"]
            .as_array()
            .and_then(|a| {
                a.iter()
                    .find(|a| a["companySlug"] == destination.company_slug)
            })
            .ok_or_else(|| failure(error("invalid_control"), true))?;
        if assigned["documentId"] != document.value["id"]
            || assigned["documentNo"] != document.value["documentNo"]
        {
            return Err(failure(
                storage::refused("completed assignment document identity differs"),
                true,
            ));
        }
        return Ok(Committed {
            value: json!({"source":source}),
            control_reusable: document.control_reusable,
        });
    }
    let result = transact(control, |tx| {
        let (subject, visible, now) = authorize(
            tx,
            &runtime,
            &context,
            true,
            Some(&destination.company_slug),
        )?;
        storage::complete(
            tx,
            id,
            &destination.company_slug,
            &claim,
            document.value["id"]
                .as_i64()
                .ok_or_else(|| error("invalid_company"))?,
            document.value["documentNo"]
                .as_str()
                .ok_or_else(|| error("invalid_company"))?,
            storage::Provenance {
                actor: &format!("user:{subject}"),
                now: &now,
            },
        )?;
        storage::inspect(tx, runtime.anchor, id, &visible).map(|s| json!({"source":s}))
    });
    match result {
        Ok(value) => Ok(Committed {
            value,
            control_reusable: document.control_reusable,
        }),
        Err(mut f) => {
            f.stage = FailureStage::OutcomeUnknown;
            f.error = storage::refused(
                "company handoff may have committed; inspect source and recover the same claim after expiry",
            );
            Err(f)
        }
    }
}
