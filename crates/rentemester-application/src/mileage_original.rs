//! Original mileage projections under current, typed company authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::auth::Clock;
use rentemester_storage::{idempotency::PrincipalKind, mileage_original as store};
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};
pub use store::Log;

/// The serialized internal packet, including byte arrays, is bounded before commit.
pub const MAX_EXPORT_PACKET_BYTES: usize = 16 * 1024 * 1024;
pub enum Action<'a> {
    Log(&'a Log),
    List,
    Report(&'a str, &'a str),
    Export(&'a str, &'a str),
    Invalid(Permission),
}
fn permission(action: &Action<'_>) -> Permission {
    match action {
        Action::Log(_) => Permission::CompanyDraftWrite,
        Action::Export(..) => Permission::CompanyExport,
        Action::Invalid(p) => *p,
        _ => Permission::CompanyRead,
    }
}
fn readonly(action: &Action<'_>) -> bool {
    matches!(permission(action), Permission::CompanyRead)
}
fn bounded(value: &Value) -> Result<(), Error> {
    struct Limit(usize);
    impl std::io::Write for Limit {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.0 {
                return Err(std::io::Error::other("export packet limit"));
            }
            self.0 -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Limit(MAX_EXPORT_PACKET_BYTES), value).map_err(|_| {
        Error::new(
            "mileage_export_too_large",
            "Mileage export exceeds the delivery limit",
        )
    })
}
fn apply(
    tx: &Transaction<'_>,
    clock: &dyn Clock,
    subject: &str,
    kind: PrincipalKind,
    action: Action<'_>,
    expected: Option<&str>,
) -> Result<Value, Error> {
    let actor = format!(
        "{}:{subject}",
        if matches!(kind, PrincipalKind::ServiceAccount) {
            "service_account"
        } else {
            "user"
        }
    );
    if expected.is_some_and(|v| v != actor || v.len() > 160 || v.chars().any(char::is_control)) {
        return Err(Error::new(
            "access_denied",
            "Current actor assertion did not match",
        ));
    }
    match action {
        Action::Invalid(_) => Err(Error::new(
            "invalid_input",
            "Invalid original mileage request",
        )),
        Action::Log(input) => store::log(tx, input, subject, kind),
        Action::List => store::list(tx),
        Action::Report(from, to) => store::report(tx, from, to),
        Action::Export(from, to) => {
            let now = clock
                .now_unix_ms()
                .map_err(|_| Error::new("clock_unavailable", "Mileage clock unavailable"))?;
            let today = rentemester_runtime::posting_time::latest_date(now, 0)
                .map_err(|_| Error::new("clock_unavailable", "Mileage date unavailable"))?;
            match store::export(tx, from, to, subject, kind, Date::parse(&today)?)? {
                store::ExportOutcome::Refused(value) => Ok(value),
                store::ExportOutcome::Ready(artifacts) => {
                    let mut value = artifacts.summary;
                    value["artifacts"] = json!({
                        "json": {"filename":"mileage-log.json", "contentType":"application/json; charset=utf-8", "bytes":artifacts.json_bytes},
                        "csv": {"filename":"mileage-log.csv", "contentType":"text/csv; charset=utf-8", "bytes":artifacts.csv_bytes}
                    });
                    // Any failure rolls back the caller-owned transaction, including
                    // the domain's prepared-artifact audit. No publication occurs here.
                    bounded(&value)?;
                    Ok(value)
                }
            }
        }
    }
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    action: Action<'_>,
    expected: Option<&str>,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_subject_mode(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission(&action),
        readonly(&action),
        |tx, subject| {
            apply(
                tx,
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
    context: service_company::Context<'_>,
    action: Action<'_>,
    expected: Option<&str>,
) -> Result<service_company::ServiceResult<Value>, service_company::ServiceFailure> {
    let clock = context.clock;
    service_company::execute_operation(
        control,
        company,
        context,
        permission(&action),
        !readonly(&action),
        |tx, subject, _| {
            Ok(service_company::OperationResult {
                result: apply(
                    tx,
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
    #[test]
    fn permissions_and_fixed_serialized_limit() {
        assert_eq!(permission(&Action::List), Permission::CompanyRead);
        assert_eq!(permission(&Action::Report("", "")), Permission::CompanyRead);
        assert_eq!(
            permission(&Action::Export("", "")),
            Permission::CompanyExport
        );
        assert_eq!(
            permission(&Action::Invalid(Permission::CompanyDraftWrite)),
            Permission::CompanyDraftWrite
        );
        assert!(bounded(&json!({"ok":true})).is_ok());
        let exact = Value::String("x".repeat(MAX_EXPORT_PACKET_BYTES - 2));
        assert!(bounded(&exact).is_ok());
        assert!(bounded(&Value::String("x".repeat(MAX_EXPORT_PACKET_BYTES - 1))).is_err());
        // JSON escaping, not raw artifact size, determines the delivery limit.
        assert!(bounded(&Value::String("\n".repeat(MAX_EXPORT_PACKET_BYTES / 2))).is_err());
    }
}
