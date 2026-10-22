//! Typed provenance supplied by the held native authorization boundary.
use crate::idempotency::PrincipalKind;
use rentemester_engine::Error;
use serde::{Deserialize, Serialize};
const SCHEMA: &str = "rentemester.claim-principal.v1";
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ServiceProof {
    schema: String,
    principal_kind: String,
    subject_id: String,
    message: String,
}
pub(crate) fn program(kind: PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::User => "rentemester-rust",
        PrincipalKind::ServiceAccount => "service-principal",
    }
}
pub(crate) fn audit(
    subject: &str,
    message: &str,
    kind: PrincipalKind,
) -> Result<(String, String), Error> {
    match kind {
        // Preserve existing human evidence byte for byte.
        PrincipalKind::User => Ok((format!("user:{subject}"), message.into())),
        PrincipalKind::ServiceAccount => {
            if !valid_subject(subject) {
                return Err(super::refused("trusted bounded service subject required"));
            }
            let proof = ServiceProof {
                schema: SCHEMA.into(),
                principal_kind: "service-account".into(),
                subject_id: subject.into(),
                message: message.into(),
            };
            Ok((
                format!("service_account:{subject}"),
                serde_json::to_string(&proof).map_err(crate::invalid)?,
            ))
        }
    }
}
fn valid_subject(subject: &str) -> bool {
    !subject.trim().is_empty() && subject.len() <= 256 && !subject.chars().any(char::is_control)
}
/// A prefix alone is never service evidence. Bind the fixed version/kind and exact subject.
pub(super) fn service_subject(actor: &str, message: &str) -> Option<String> {
    let proof: ServiceProof = serde_json::from_str(message).ok()?;
    (proof.schema == SCHEMA
        && proof.principal_kind == "service-account"
        && valid_subject(&proof.subject_id)
        && actor == format!("service_account:{}", proof.subject_id))
    .then_some(proof.subject_id)
}
