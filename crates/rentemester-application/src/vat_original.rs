//! Source VAT readers retain current authority and physical source assurance.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
    service_company,
};
use rentemester_engine::Error;
use rentemester_runtime::namespace::NamespaceGuard;
use rusqlite::{Connection, Transaction};
use serde_json::Value;

#[derive(Clone, Copy)]
pub enum Operation {
    Report,
    EuSalesList,
    OssReport,
    Filing,
}
impl Operation {
    fn permission(self) -> Permission {
        match self {
            Self::EuSalesList => Permission::CompanyExternalLookup,
            _ => Permission::CompanyRead,
        }
    }
}
fn apply(
    tx: &Transaction<'_>,
    guard: &mut NamespaceGuard,
    operation: Operation,
    parse: impl FnOnce() -> Result<(String, String), Error>,
) -> Result<Value, Error> {
    // Transport parsing is deliberately inside the current authorization fence.
    let (from, to) = parse()?;
    if !crate::periods::verify_sources(tx, guard)?.is_empty() {
        return Err(match operation {
            Operation::EuSalesList => Error::new(
                "eu_sales_source_unavailable",
                "EU sales source assurance differs",
            ),
            Operation::OssReport => Error::new(
                "oss_report_source_unavailable",
                "OSS report source assurance differs",
            ),
            _ => Error::new(
                "vat_report_source_unavailable",
                "VAT report source assurance differs",
            ),
        });
    }
    crate::archive_import::verify_physical(tx, guard)?;
    match operation {
        Operation::Report => rentemester_storage::vat_original::report(tx, &from, &to),
        Operation::EuSalesList => rentemester_storage::vat_original::eu_list(tx, &from, &to),
        Operation::OssReport => rentemester_storage::vat_original::oss(tx, &from, &to),
        Operation::Filing => rentemester_storage::vat_original::filing(tx, &from, &to),
    }
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    operation: Operation,
    parse: impl FnOnce() -> Result<(String, String), Error>,
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
        operation.permission(),
        true,
        |tx, _| apply(tx, guard, operation, parse),
    )
}
pub fn execute_service(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: service_company::Context<'_>,
    operation: Operation,
    parse: impl FnOnce() -> Result<(String, String), Error>,
) -> Result<service_company::ServiceResult<Value>, service_company::ServiceFailure> {
    service_company::execute_operation(
        control,
        company,
        context,
        operation.permission(),
        false,
        |tx, _, _| {
            Ok(service_company::OperationResult {
                result: apply(tx, guard, operation, parse)?,
                receipt: None,
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_eu_retains_external_lookup_permission() {
        assert!(matches!(
            Operation::EuSalesList.permission(),
            Permission::CompanyExternalLookup
        ));
        for operation in [Operation::Report, Operation::OssReport, Operation::Filing] {
            assert!(matches!(operation.permission(), Permission::CompanyRead));
        }
    }
}
