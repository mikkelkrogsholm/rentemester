//! Explicit original AgentRun. Each effect enters the current Owner fence anew;
//! scanner I/O never retains an Owner mutex or SQL transaction.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use documents::Credential;
use rentemester_application::{
    control_fence::{Committed, Failure},
    documents::{OriginalOptions, ScanPolicy},
    service_company::{BusinessStage, ServiceFailure, ServiceResult, UsageStage},
};
use rentemester_engine::agent_run::{
    BankInput, Inbox, Policy, Proposal, RULE, Report, Request as RunRequest,
};
use rentemester_host::Attempt;
use rentemester_runtime::{auth::Clock, document::Prepared, document_scanner::Scanner};
#[derive(Debug)]
struct Refusal {
    code: &'static str,
    message: String,
    unknown: bool,
}
impl Refusal {
    fn unknown() -> Self {
        Self {
            code: "outcome_unknown",
            message: "Outcome unknown; inspect canonical state before another run".into(),
            unknown: true,
        }
    }
    fn authority(&self) -> bool {
        matches!(self.code, "access_denied" | "invalid_credentials")
    }
    fn response(self) -> Response {
        let status = if self.unknown {
            StatusCode::SERVICE_UNAVAILABLE
        } else if self.code == "invalid_credentials" {
            StatusCode::UNAUTHORIZED
        } else if self.code == "access_denied" {
            StatusCode::FORBIDDEN
        } else {
            StatusCode::UNPROCESSABLE_ENTITY
        };
        response(
            status,
            json!({"ok":false,"code":if self.unknown{"outcome_unknown"}else{self.code},"errors":[if self.unknown{"outcome_unknown".into()}else{self.message}]}),
        )
    }
}
fn human<T>(a: Attempt<Result<Committed<T>, Failure>>) -> Result<T, Refusal> {
    let retired = a.retired;
    match a.outcome {
        Ok(v) if !retired && v.control_reusable => Ok(v.value),
        Ok(_) => Err(Refusal::unknown()),
        Err(f) => Err(Refusal {
            code: f.error.code,
            message: f.error.message,
            unknown: retired
                || f.stage == FailureStage::OutcomeUnknown
                || !f.control_reusable
                || !f.company_reusable,
        }),
    }
}
fn service<T>(a: Attempt<Result<ServiceResult<T>, ServiceFailure>>) -> Result<T, Refusal> {
    let retired = a.retired;
    match a.outcome {
        Ok(v) if !retired && v.control_reusable => Ok(v.result),
        Ok(_) => Err(Refusal::unknown()),
        Err(f) => Err(Refusal {
            code: f.error.code,
            message: f.error.message,
            unknown: retired
                || f.usage == UsageStage::OutcomeUnknown
                || f.business == BusinessStage::OutcomeUnknown
                || !f.control_reusable
                || !f.company_reusable,
        }),
    }
}
// Value normally accepts duplicate keys. Reject them recursively before DTO decoding.
struct Strict(Value);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Strict;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("unambiguous JSON")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(self, mut m: M) -> Result<Strict, M::Error> {
                let mut out = serde_json::Map::new();
                while let Some((k, Strict(v))) = m.next_entry::<String, Strict>()? {
                    if out.insert(k, v).is_some() {
                        return Err(serde::de::Error::custom("duplicate field"));
                    }
                }
                Ok(Strict(Value::Object(out)))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
                let mut v = vec![];
                while let Some(Strict(i)) = a.next_element()? {
                    v.push(i);
                }
                Ok(Strict(Value::Array(v)))
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Strict, E> {
                Ok(Strict(json!(v)))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
        }
        d.deserialize_any(V)
    }
}
fn name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 255
        && !s.contains(['/', '\\'])
        && !matches!(s, "." | "..")
        && !s.chars().any(char::is_control)
}
fn decode(bytes: &[u8]) -> Result<RunRequest, ()> {
    let Strict(v) = serde_json::from_slice(bytes).map_err(|_| ())?;
    if !v.is_object()
        || ["expectedActor", "inbox", "bank"]
            .iter()
            .any(|k| v.get(k).is_some_and(Value::is_null))
    {
        return Err(());
    }
    let r: RunRequest = serde_json::from_value(v).map_err(|_| ())?;
    if !r.confirm
        || r.as_of.len() > 64
        || r.as_of.chars().any(char::is_control)
        || r.expected_actor.as_ref().is_some_and(|a| {
            a.is_empty() || a.trim() != a || a.len() > 160 || a.chars().any(char::is_control)
        })
    {
        return Err(());
    }
    match &r.inbox {
        Some(Inbox::Files { items })
            if items.len() > 256
                || items
                    .iter()
                    .any(|i| !name(&i.file_name) || i.file_base64.len() > 34 * 1024 * 1024) =>
        {
            return Err(());
        }
        Some(Inbox::Failed { error }) if error.len() > 4096 => return Err(()),
        _ => {}
    }
    match &r.bank {
        Some(BankInput::Csv { file_name, csv })
            if !name(file_name) || csv.len() > 25 * 1024 * 1024 =>
        {
            return Err(());
        }
        Some(BankInput::Failed { error }) if error.len() > 4096 => return Err(()),
        _ => {}
    }
    Ok(r)
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let credential = match documents::credential(&state, &request, true) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || request.headers().contains_key("idempotency-key")
        || !one(request.headers(), "content-type")
            .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let bytes = match to_bytes(request.into_body(), 35 * 1024 * 1024).await {
        Ok(v) => v,
        Err(_) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input"),
    };
    let request = match decode(&bytes) {
        Ok(r) => r,
        Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        run(state, credential, slug, request)
    })
    .await
    {
        Ok(Ok(report)) => response(StatusCode::OK, json!(report)),
        Ok(Err(f)) => f.response(),
        Err(_) => Refusal::unknown().response(),
    }
}
fn proposal(
    state: &Product,
    c: &Credential,
    slug: &str,
    r: &Report,
    p: &Proposal,
) -> Result<(), Refusal> {
    let mut o = state
        .native
        .lock_company(slug)
        .map_err(|_| Refusal::unknown())?;
    match c {
        Credential::Human(c) => o.agent_run_proposal(c, &SystemClock, p, r),
        Credential::Service(k) => o.agent_run_proposal_service(k, &SystemClock, p, r),
    }
    .map_err(|f| Refusal {
        code: f.code,
        message: f.message,
        unknown: f.unknown,
    })
}
fn ingest(
    state: &Product,
    c: &Credential,
    slug: &str,
    item: rentemester_engine::agent_run::InboxDocument,
) -> Result<Value, Refusal> {
    let error = |code, message: String| Refusal {
        code,
        message,
        unknown: false,
    };
    let bytes = STANDARD
        .decode(item.file_base64)
        .map_err(|_| error("invalid_document_file", "invalid base64".into()))?;
    let file = Prepared::new(item.file_name, bytes)
        .map_err(|s| error("invalid_document_file", s.to_string()))?;
    let metadata = serde_json::from_value(item.metadata)
        .map_err(|_| error("invalid_metadata", "Invalid document metadata".into()))?;
    let admission = {
        let mut o = state
            .native
            .lock_company(slug)
            .map_err(|_| Refusal::unknown())?;
        let policy = if state.native.document_scanner.is_some() {
            ScanPolicy::Required
        } else {
            ScanPolicy::Off
        };
        let opts = OriginalOptions {
            original_api: true,
            ..Default::default()
        };
        match c {
            Credential::Human(c) => human(
                o.prepare_documents_original(c, &SystemClock, file, metadata, policy, opts)
                    .map_err(|_| Refusal::unknown())?,
            ),
            Credential::Service(k) => service(
                o.prepare_documents_original_service(k, &SystemClock, file, metadata, policy, opts)
                    .map_err(|_| Refusal::unknown())?,
            ),
        }?
    };
    let scan = state
        .native
        .document_scanner
        .as_ref()
        .map(|s| {
            s.scan(admission.file())
                .map_err(|f| error("document_scan_refused", format!("{f:?}")))
        })
        .transpose()?;
    let mut o = state
        .native
        .lock_company(slug)
        .map_err(|_| Refusal::unknown())?;
    match c {
        Credential::Human(c) => human(
            o.finish_document_ingest_registered(c, &SystemClock, admission, scan)
                .map_err(|_| Refusal::unknown())?,
        )
        .map(|v| v.receipt),
        Credential::Service(k) => service(
            o.finish_document_ingest_service(k, &SystemClock, admission, scan)
                .map_err(|_| Refusal::unknown())?,
        ),
    }
}
fn run(
    state: Product,
    c: Credential,
    slug: String,
    request: RunRequest,
) -> Result<Report, Refusal> {
    let policy = state
        .native
        .agent_policies
        .get(&slug)
        .cloned()
        .unwrap_or_else(Policy::default);
    let mut report = {
        let mut o = state
            .native
            .lock_company(&slug)
            .map_err(|_| Refusal::unknown())?;
        match &c {
            Credential::Human(c) => o.agent_run_start(c, &SystemClock, &request, &policy),
            Credential::Service(k) => o.agent_run_start_service(k, &SystemClock, &request, &policy),
        }
        .map_err(|f| Refusal {
            code: f.code,
            message: f.message,
            unknown: f.unknown,
        })?
    };
    if !report.ok {
        return Ok(report);
    }
    report.phases.push("ingest".into());
    match request.inbox {
        Some(Inbox::Failed { error }) => {
            report.ok = false;
            report.errors.push(error);
            return Ok(report);
        }
        Some(Inbox::Files { mut items }) => {
            items.sort_by(|a, b| a.file_name.cmp(&b.file_name));
            for item in items {
                let stem = item
                    .file_name
                    .rsplit_once('.')
                    .map(|(s, _)| s)
                    .unwrap_or(&item.file_name)
                    .to_owned();
                match ingest(&state, &c, &slug, item) {
                    Ok(v) if v["ok"] != false => report.documents_ingested += 1,
                    Ok(v) => {
                        report.documents_rejected += 1;
                        let errors = v["errors"].to_string();
                        if !errors.to_lowercase().contains("duplicate") {
                            proposal(&state, &c, &slug, &report, &rejected(&stem, &errors))?;
                        }
                    }
                    Err(f) if !f.unknown && !f.authority() => {
                        report.documents_rejected += 1;
                        if !f.code.contains("duplicate")
                            && !f.message.to_lowercase().contains("duplicate")
                        {
                            proposal(&state, &c, &slug, &report, &rejected(&stem, &f.message))?;
                        }
                    }
                    Err(f) => return Err(f),
                }
            }
        }
        None => {}
    }
    match request.bank {
        Some(BankInput::Failed { error }) => {
            report.ok = false;
            report.errors.push(error);
            return Ok(report);
        }
        Some(BankInput::Csv { file_name: _, csv }) => {
            let timestamp = rentemester_runtime::backup_keys::timestamp(
                SystemClock.now_unix_ms().map_err(|_| Refusal::unknown())?,
            )
            .map_err(|_| Refusal::unknown())?;
            let result = {
                let mut o = state
                    .native
                    .lock_company(&slug)
                    .map_err(|_| Refusal::unknown())?;
                match &c {
                    Credential::Human(c) => {
                        o.agent_run_import_bank(c, &SystemClock, &csv, &timestamp, &report)
                    }
                    Credential::Service(k) => {
                        o.agent_run_import_bank_service(k, &SystemClock, &csv, &timestamp, &report)
                    }
                }
                .map_err(|f| Refusal {
                    code: f.code,
                    message: f.message,
                    unknown: f.unknown,
                })
            };
            match result {
                Ok(v) if v["ok"] != false => {
                    report.bank_transactions_imported = v["imported"]
                        .as_u64()
                        .or_else(|| v["data"]["imported"].as_u64())
                        .unwrap_or(0);
                }
                Ok(v) => {
                    report.ok = false;
                    report
                        .errors
                        .push(format!("bank import failed: {}", v["errors"]));
                    return Ok(report);
                }
                Err(f) if !f.unknown && !f.authority() => {
                    report.ok = false;
                    report
                        .errors
                        .push(format!("bank import failed: {}", f.message));
                    return Ok(report);
                }
                Err(f) => return Err(f),
            }
        }
        None => {}
    }
    let mut o = state
        .native
        .lock_company(&slug)
        .map_err(|_| Refusal::unknown())?;
    match &c {
        Credential::Human(c) => o.agent_run_finish(c, &SystemClock, &policy, report),
        Credential::Service(k) => o.agent_run_finish_service(k, &SystemClock, &policy, report),
    }
    .map_err(|f| Refusal {
        code: f.code,
        message: f.message,
        unknown: f.unknown,
    })
}
fn rejected(stem: &str, error: &str) -> Proposal {
    Proposal {
        kind: "AGENT_DOCUMENT_REJECTED".into(),
        severity: "medium".into(),
        message: format!("Bilag {stem} blev afvist af bogføringen: {error}"),
        required_action: Some(
            "Gennemgå bilagets metadata, ret fejlen, og indlæs bilaget igen manuelt.".into(),
        ),
        related_document_id: None,
        related_bank_transaction_id: None,
        source_evidence: json!({"rule":RULE,"bilag":stem,"errors":[error]}),
        posting_preview: None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agent_run_strict_wire_rejects_authority_injection_and_duplicate_fields() {
        for s in [
            r#"[]"#,
            r#"{"confirm":true,"asOf":"2026-10-25","actor":"user:forged"}"#,
            r#"{"confirm":true,"confirm":true,"asOf":"2026-10-25"}"#,
            r#"{"confirm":true,"asOf":"2026-10-25","inbox":{"state":"files","items":[{"fileName":"../x","fileBase64":"","metadata":{}}]}}"#,
        ] {
            assert!(decode(s.as_bytes()).is_err(), "{s}");
        }
        assert!(decode(br#"{"confirm":true,"asOf":"2026-10-25"}"#).is_ok());
    }
    #[test]
    fn agent_run_uncertain_authority_outcome_stays_unknown() {
        for code in ["access_denied", "invalid_credentials"] {
            assert_eq!(
                Refusal {
                    code,
                    message: "uncertain".into(),
                    unknown: true
                }
                .response()
                .status(),
                StatusCode::SERVICE_UNAVAILABLE
            );
        }
    }
    #[test]
    fn agent_run_report_is_exact_flat_fifteen_fields() {
        let r = Report::new(
            "user:synthetic".into(),
            "synthetic".into(),
            "2026-10-25".into(),
        );
        assert_eq!(json!(r).as_object().unwrap().len(), 15);
    }
}
