//! Source asset operations inside the existing current company fences.
//!
//! Registration is metadata-only and uses DraftWrite, unlike the retained native
//! LedgerPost alias. The shared storage writer deliberately retains the stricter
//! unallocated-capitalisation proof although source TypeScript only checks a document.
//! Physical verification precedes all domain writes, including threshold exceptions;
//! only a positively identified absent row may bypass the physical-original check.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::{auth::Clock, namespace::NamespaceGuard};
use rentemester_storage::{asset_original as store, idempotency::PrincipalKind};
use rusqlite::{Connection, Transaction};
use serde_json::Value;
pub use store::{Depreciate, Register, WriteOff};

pub enum Action<'a> {
    Report,
    Register(&'a Register),
    Depreciate(i64, &'a Depreciate),
    WriteOff(&'a WriteOff),
    DisposalPreview(i64, &'a rentemester_storage::asset_disposal::Preview),
    Dispose(i64, &'a rentemester_storage::asset_disposal::Dispose),
    DisposalReceipt(i64, &'a str),
    /// Transport validation is evaluated only after current authority admission.
    Invalid,
    InvalidRegister,
}
fn permission(action: &Action<'_>) -> Permission {
    match action {
        Action::Report => Permission::CompanyRead,
        Action::Register(_) | Action::InvalidRegister => Permission::CompanyDraftWrite,
        _ => Permission::CompanyLedgerPost,
    }
}

fn apply(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    clock: &dyn Clock,
    subject: &str,
    kind: PrincipalKind,
    action: Action<'_>,
    expected: Option<&str>,
) -> Result<Value, Error> {
    if !matches!(action, Action::Report) {
        let actor = format!(
            "{}:{subject}",
            if matches!(kind, PrincipalKind::ServiceAccount) {
                "service_account"
            } else {
                "user"
            }
        );
        if expected.is_some_and(|v| {
            v.trim().is_empty()
                || v.trim() != v
                || v.len() > 160
                || v.chars().any(char::is_control)
                || v != actor
        }) {
            return Err(Error::new(
                "access_denied",
                "Current actor assertion did not match",
            ));
        }
    }
    if matches!(action, Action::Invalid | Action::InvalidRegister) {
        return Err(Error::new(
            "invalid_input",
            "Invalid original asset request",
        ));
    }
    rentemester_storage::bank_original::verify_journals(tx)?;
    if matches!(action, Action::Report) {
        return store::report(tx);
    }
    let now = clock
        .now_unix_ms()
        .map_err(|_| Error::new("clock_unavailable", "Asset clock unavailable"))?;
    let days = if matches!(
        action,
        Action::Register(_)
            | Action::DisposalPreview(..)
            | Action::Dispose(..)
            | Action::DisposalReceipt(..)
    ) {
        0
    } else {
        45
    };
    let latest = rentemester_runtime::posting_time::latest_date(now, days)
        .map_err(|_| Error::new("clock_unavailable", "Asset posting horizon unavailable"))?;
    let latest = Date::parse(&latest)?;
    let document = match action {
        Action::Report => return store::report(tx),
        Action::Register(input) => Some(input.document_id),
        Action::DisposalPreview(id, _)
        | Action::Dispose(id, _)
        | Action::DisposalReceipt(id, _)
        | Action::Depreciate(id, _) => {
            if !(1..=9_007_199_254_740_991).contains(&id) {
                return Err(Error::new(
                    "invalid_input",
                    "Positive safe asset ID required",
                ));
            }
            Some(rentemester_storage::assets::document_id(tx, id)?)
        }
        Action::WriteOff(input) => {
            // Validate before existence lookup: malformed input and a declined
            // inner acknowledgement must never create a documentation exception.
            validate_write_off(input)?;
            if !input.confirm_immediate_write_off {
                return Ok(
                    serde_json::json!({"ok":false,"appliedRules":["DK-ASSET-WRITEOFF-001"],"errors":["immediate write-off requires confirmImmediateWriteOff: true — the user/advisor must confirm the straksafskrivning treatment"]}),
                );
            }
            if store::purchase_document_exists(tx, input.document_id)? {
                Some(input.document_id)
            } else {
                None // Only this case may reach the durable missing-document exception.
            }
        }
        Action::Invalid | Action::InvalidRegister => unreachable!(),
    };
    if let Some(id) = document {
        if !(1..=9_007_199_254_740_991).contains(&id) {
            return Err(Error::new(
                "invalid_input",
                "Positive safe document ID required",
            ));
        }
        rentemester_storage::invoice_extraction::validate_history(tx)?;
        let original = rentemester_storage::documents::evidence(tx, id)?;
        let bytes = guard
            .read_document(&original.path, &original.hash)
            .map_err(|_| {
                Error::new(
                    "document_evidence_unavailable",
                    "Retained asset original unavailable or changed",
                )
            })?;
        for source in rentemester_storage::invoice_extraction::sources(tx)? {
            if source.document_id == id
                && (source.sha256 != original.hash || source.pdf_bytes != bytes)
            {
                return Err(Error::new(
                    "document_evidence_unavailable",
                    "Retained asset source changed",
                ));
            }
        }
    }
    if let Action::DisposalPreview(_, i) = &action {
        verify_disposal_original(tx, guard, i.document_id)?;
    }
    if let Action::Dispose(_, i) = &action {
        verify_disposal_original(tx, guard, i.document_id)?;
    }
    if let Action::DisposalReceipt(id, _) = &action {
        if let Some(record) = rentemester_storage::asset_disposal::record(tx, *id)? {
            verify_disposal_original(
                tx,
                guard,
                record["documentId"].as_i64().ok_or_else(|| {
                    Error::new("asset_refused", "Invalid retained disposal original")
                })?,
            )?;
        }
    }
    match action {
        Action::DisposalPreview(id, i) => {
            rentemester_storage::asset_disposal::preview(tx, id, i, latest)
        }
        Action::Dispose(id, i) => {
            rentemester_storage::asset_disposal::dispose(tx, id, i, subject, kind, now, latest)
        }
        Action::DisposalReceipt(id, key) => {
            rentemester_storage::asset_disposal::receipt(tx, id, key, subject, kind)
        }
        Action::Register(input) => store::register(tx, input, subject, kind, latest),
        Action::Depreciate(id, input) => store::depreciate(tx, id, input, subject, kind, latest),
        Action::WriteOff(input) => store::write_off(tx, input, subject, kind, latest),
        _ => unreachable!(),
    }
}
fn verify_disposal_original(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    id: i64,
) -> Result<(), Error> {
    if !(1..=9_007_199_254_740_991).contains(&id) {
        return Err(Error::new(
            "invalid_input",
            "positive safe disposal document required",
        ));
    }
    let original = rentemester_storage::documents::evidence(tx, id)?;
    guard
        .read_document(&original.path, &original.hash)
        .map_err(|_| {
            Error::new(
                "document_evidence_unavailable",
                "Disposal original unavailable or changed",
            )
        })?;
    Ok(())
}
fn validate_write_off(input: &WriteOff) -> Result<(), Error> {
    let text =
        |s: &str| !s.trim().is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control);
    if !text(&input.name)
        || !text(&input.category)
        || !text(&input.expense_account)
        || !text(&input.threshold_rule_source)
        || input.payment_account.as_deref().is_some_and(|s| !text(s))
        || input.note.as_deref().is_some_and(|s| {
            s.len() > 4096
                || s.chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
        })
        || !(1..=9_007_199_254_740_991).contains(&input.document_id)
        || input.cost.as_f64().is_none_or(|n| {
            !n.is_finite() || n <= 0.0 || n > (9_007_199_254_740_991_i64 as f64 / 100.0)
        })
        || Date::parse(&input.acquisition_date).is_err()
        || Date::parse(&input.date).is_err()
    {
        return Err(Error::new(
            "invalid_input",
            "Invalid original asset write-off request",
        ));
    }
    let cost = rentemester_engine::document::money(&input.cost)
        .map_err(|_| Error::new("invalid_input", "Invalid asset cost precision"))?;
    if cost == rentemester_engine::money::Money::default() || cost.is_negative() {
        return Err(Error::new("invalid_input", "Asset cost must be positive"));
    }
    Ok(())
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
    expected: Option<&str>,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission(&action),
        |tx, subject| {
            apply(
                tx,
                guard,
                context.clock,
                subject,
                PrincipalKind::User,
                action,
                expected,
            )
        },
    )
}
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: service_company::Context<'_>,
    action: Action<'_>,
    expected: Option<&str>,
) -> Result<service_company::ServiceResult<Value>, service_company::ServiceFailure> {
    let clock = context.clock;
    let mutation = !matches!(
        action,
        Action::Report | Action::DisposalPreview(..) | Action::DisposalReceipt(..)
    );
    // execute_operation commits exactly one usage charge, then rechecks the live
    // typed service key/scope/permission under its company/backup/schema fence.
    service_company::execute_operation(
        control,
        company,
        context,
        permission(&action),
        mutation,
        |tx, subject, _| {
            Ok(service_company::OperationResult {
                result: apply(
                    tx,
                    guard,
                    clock,
                    subject,
                    PrincipalKind::ServiceAccount,
                    action,
                    expected,
                )?,
                receipt: None,
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn permissions_preserve_metadata_financial_and_read_split() {
        let input: Register = serde_json::from_value(json!({"name":"Synthetic", "category":"Equipment", "acquisitionDate":"2026-01-01", "cost":125, "usefulLifeMonths":3, "documentId":1})).unwrap();
        assert_eq!(
            permission(&Action::Register(&input)),
            Permission::CompanyDraftWrite
        );
        assert_eq!(
            permission(&Action::InvalidRegister),
            Permission::CompanyDraftWrite
        );
        assert_eq!(permission(&Action::Report), Permission::CompanyRead);
        let depreciation: Depreciate =
            serde_json::from_value(json!({"period":1,"date":"2026-02-01"})).unwrap();
        assert_eq!(
            permission(&Action::Depreciate(1, &depreciation)),
            Permission::CompanyLedgerPost
        );
        let writeoff = writeoff();
        assert_eq!(
            permission(&Action::WriteOff(&writeoff)),
            Permission::CompanyLedgerPost
        );
    }
    fn writeoff() -> WriteOff {
        serde_json::from_value(json!({"name":"Synthetic", "category":"Equipment", "acquisitionDate":"2026-01-01", "date":"2026-01-01", "cost":250, "documentId":1, "expenseAccount":"2000", "thresholdRuleSource":"Explicit synthetic advisor source", "confirmImmediateWriteOff":false})).unwrap()
    }
    #[test]
    fn preflight_accepts_false_ack_but_rejects_malformed_before_document_lookup() {
        let mut input = writeoff();
        assert!(validate_write_off(&input).is_ok());
        input.document_id = 9_007_199_254_740_992;
        assert!(validate_write_off(&input).is_err());
        input.document_id = 1;
        input.cost = serde_json::Number::from_f64(0.001).unwrap();
        assert!(validate_write_off(&input).is_err());
        input.cost = 250.into();
        input.acquisition_date = "2026-02-30".into();
        assert!(validate_write_off(&input).is_err());
        input.acquisition_date = "2026-01-02".into();
        // Source accepts explicit dates independently; shared journal controls
        // still reject any actual posting beyond the admitted horizon.
        assert!(validate_write_off(&input).is_ok());
    }
    #[test]
    fn asset_disposal_permission_is_current_ledger_post_for_preview_apply_and_receipt() {
        let p = rentemester_storage::asset_disposal::Preview {
            transaction_date: "2026-01-01".into(),
            document_id: 1,
            reason: "Synthetic retirement".into(),
            loss_account_no: None,
        };
        assert_eq!(
            permission(&Action::DisposalPreview(1, &p)),
            Permission::CompanyLedgerPost
        );
        assert_eq!(
            permission(&Action::DisposalReceipt(1, "synthetic-key")),
            Permission::CompanyLedgerPost
        );
        let i = rentemester_storage::asset_disposal::Dispose {
            transaction_date: p.transaction_date.clone(),
            document_id: 1,
            reason: p.reason.clone(),
            loss_account_no: None,
            expected_plan_hash: "a".repeat(64),
            idempotency_key: "synthetic-key".into(),
            confirm: true,
        };
        assert_eq!(
            permission(&Action::Dispose(1, &i)),
            Permission::CompanyLedgerPost
        );
    }
    #[test]
    fn asset_disposal_current_authority_precedes_receipt_or_original_access() {
        use rentemester_runtime::auth::{AuthError, Clock, CookieProfile, CookieVerifier};
        struct Fixed;
        impl Clock for Fixed {
            fn now_unix_ms(&self) -> Result<i64, AuthError> {
                Ok(1_767_225_600_000)
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let mut control = Connection::open(dir.path().join("control.sqlite")).unwrap();
        let mut company = Connection::open(dir.path().join("company.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(
            &mut control,
            &mut company,
            "Synthetic disposal authority",
        )
        .unwrap();
        control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u');").unwrap();
        let verifier = CookieVerifier::new(
            "w",
            CookieProfile::Development,
            "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
        )
        .unwrap();
        let issued = verifier.issue_session("synthetic-token").unwrap();
        let cookie = verifier
            .verify(Some(issued.request_header()), None)
            .unwrap();
        let selection = || control_fence::CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        };
        let auth = || CookieAuthorization {
            cookie: &cookie,
            clock: &Fixed,
            requirement: SessionRequirement::Current,
        };
        let before = company.total_changes();
        let error = control_fence::execute_cookie_subject(
            &mut control,
            &mut company,
            auth(),
            selection(),
            permission(&Action::DisposalReceipt(1, "synthetic-key")),
            |_, _| -> Result<Value, Error> { panic!("reader must not reach receipt/original") },
        )
        .unwrap_err();
        assert_eq!(error.error.code, "access_denied");
        assert_eq!(company.total_changes(), before);
        control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:u')").unwrap();
        let error = control_fence::execute_cookie_subject(
            &mut control,
            &mut company,
            auth(),
            selection(),
            permission(&Action::DisposalReceipt(1, "synthetic-key")),
            |_, _| -> Result<Value, Error> {
                Err(Error::new(
                    "synthetic_reached",
                    "authorized source callback",
                ))
            },
        )
        .unwrap_err();
        assert_eq!(error.error.code, "synthetic_reached");
        control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES('u','a','revoke','user:u')").unwrap();
        let error = control_fence::execute_cookie_subject(
            &mut control,
            &mut company,
            auth(),
            selection(),
            permission(&Action::DisposalReceipt(1, "synthetic-key")),
            |_, _| -> Result<Value, Error> {
                panic!("revoked actor must not reach receipt/original")
            },
        )
        .unwrap_err();
        assert_eq!(error.error.code, "access_denied");
    }
}
