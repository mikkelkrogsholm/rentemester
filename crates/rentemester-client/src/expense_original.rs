//! Original expense HTTP surface. No provider, accounting, identity or retry logic lives here.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn id() -> Value {
    json!({"type":"integer","minimum":1,"maximum":9007199254740991u64})
}
fn strings() -> Value {
    json!({"type":"array","items":{"type":"string"}})
}
fn treatment(mcp: bool) -> Value {
    let mut values = vec![
        "standard",
        "reverse_charge",
        "representation",
        "exempt",
        "non_deductible",
    ];
    if !mcp {
        values.push("eu_goods_acquisition");
    }
    json!({"type":"string","enum":values})
}
/// Flat input only: confirmation and current-identity assertions are transport fields.
pub fn original_expense_book_schema(mcp: bool) -> Value {
    object(
        json!({"documentId":id(),"bankTransactionId":id(),"expenseAccount":{"type":"string","minLength":1},"vatTreatment":treatment(mcp),"paymentAccount":{"type":"string"},"date":{"type":"string"},"text":{"type":"string"},"idempotencyKey":{"type":"string","minLength":1,"maxLength":128}}),
        &["documentId", "bankTransactionId", "expenseAccount"],
    )
}
pub fn validate_original_expense_book(input: &Value, mcp: bool) -> Result<(), Error> {
    let bytes =
        serde_json::to_vec(input).map_err(|_| Error::InvalidInput("invalid expense input"))?;
    if bytes.len() > 16 * 1024 {
        return Err(Error::InvalidInput("expense input exceeds16KiB"));
    }
    crate::domain::body(&bytes, &original_expense_book_schema(mcp))?;
    if input
        .get("idempotencyKey")
        .is_some_and(|v| v.as_str().is_none_or(|s| s.trim().is_empty()))
    {
        return Err(Error::InvalidInput(
            "nonblank caller idempotencyKey required",
        ));
    }
    Ok(())
}
fn check_id(value: i64) -> Result<(), Error> {
    if (1..=9_007_199_254_740_991).contains(&value) {
        Ok(())
    } else {
        Err(Error::InvalidInput("positive safe document id required"))
    }
}
fn confirmation(confirmed: bool, actor: Option<&str>) -> Result<Value, Error> {
    if !confirmed {
        return Err(Error::InvalidInput("confirmation required"));
    }
    let mut body = json!({"confirm":true});
    if let Some(actor) = actor {
        if actor.trim().is_empty()
            || actor != actor.trim()
            || actor.encode_utf16().count() > 160
            || actor.chars().any(char::is_control)
        {
            return Err(Error::InvalidInput("invalid expected actor assertion"));
        }
        body["expectedActor"] = json!(actor);
    }
    Ok(body)
}
fn snapshot_schema() -> Value {
    let text = json!({"type":"string"});
    let nullable = json!({"type":["string","null"]});
    let region = json!({"type":"string","enum":["DK","EU","NON_EU","CONFLICT"]});
    let event = object(
        json!({"id":id(),"event_type":{"type":"string","enum":["provider_requested","provider_result","preflight_passed","preflight_failed"]},"classification":region,"provider_status":{"type":"string","enum":["requested","valid","invalid","inconclusive","unavailable","blocked","local","conflict"]},"evidence_expires_at":nullable,"created_at":text,"actor":nullable}),
        &[
            "id",
            "event_type",
            "classification",
            "provider_status",
            "evidence_expires_at",
            "created_at",
            "actor",
        ],
    );
    let mut exception = object(
        json!({"id":id(),"status":text,"severity":text,"message":text,"requiredAction":nullable,"createdAt":text}),
        &[
            "id",
            "status",
            "severity",
            "message",
            "requiredAction",
            "createdAt",
        ],
    );
    exception["type"] = json!(["object", "null"]);
    object(
        json!({"ok":{"type":"boolean"},"documentId":id(),"classification":region,"cached":{"type":"boolean"},"evidenceExpiresAt":nullable,"wouldCallProvider":{"type":"boolean"},"errors":strings(),"derivedRegion":region,"requiredValidation":{"type":["string","null"],"enum":["EU VAT provider validation",null]},"cache":object(json!({"reused":{"type":"boolean"},"freshUntil":nullable}), &["reused","freshUntil"]),"applyWouldCallProvider":{"type":"boolean"},"evidence":{"type":"array","maxItems":20,"items":event},"exception":exception}),
        &[
            "ok",
            "documentId",
            "classification",
            "cached",
            "evidenceExpiresAt",
            "wouldCallProvider",
            "errors",
            "derivedRegion",
            "requiredValidation",
            "cache",
            "applyWouldCallProvider",
            "evidence",
            "exception",
        ],
    )
}
fn book_result_schema() -> Value {
    let text = json!({"type":"string"});
    let mut fields = json!({"ok":{"type":"boolean"},"errors":strings(),"appliedRules":strings(),"entryId":id(),"entryNo":text,"entryHash":text,"documentId":id(),"bankTransactionId":id(),"vatTreatment":treatment(false),"fxRateSource":{"type":"string","enum":["imported_bank","derived_dkk_settlement"]},"idempotency":object(json!({"replayed":{"type":"boolean"},"receiptId":id(),"createdAt":text,"expiresAt":text}), &["replayed","receiptId","createdAt","expiresAt"])});
    for key in [
        "grossAmount",
        "netAmount",
        "vatAmount",
        "grossAmountForeign",
        "grossAmountDkk",
        "netAmountDkk",
        "vatAmountDkk",
        "fxRateToDkk",
        "fxReconstructionDifferenceDkk",
    ] {
        fields[key] = json!({"type":"number"});
    }
    object(fields, &["ok", "errors", "appliedRules"])
}
fn shape(value: &Value, schema: &Value) -> bool {
    serde_json::to_vec(value)
        .ok()
        .is_some_and(|b| crate::domain::body(&b, schema).is_ok())
}
fn snapshot(value: &Value, document: i64) -> bool {
    shape(value, &snapshot_schema())
        && value["documentId"] == document
        && value["classification"] == value["derivedRegion"]
        && value["cached"] == value["cache"]["reused"]
        && value["evidenceExpiresAt"] == value["cache"]["freshUntil"]
        && value["wouldCallProvider"] == value["applyWouldCallProvider"]
        && (value["classification"] == "EU") == value["requiredValidation"].is_string()
        && value["errors"]
            .as_array()
            .is_some_and(|a| a.is_empty() == (value["ok"] == true))
}
fn book_result(value: &Value, input: &Value) -> bool {
    shape(value, &book_result_schema())
        && value["errors"]
            .as_array()
            .is_some_and(|a| a.is_empty() == (value["ok"] == true))
        && (value["ok"] != true
            || (value["entryId"].is_i64()
                && value["entryNo"].is_string()
                && value["entryHash"].is_string()
                && value["documentId"] == input["documentId"]
                && value["bankTransactionId"] == input["bankTransactionId"]))
        && (value.get("idempotency").is_none() || input.get("idempotencyKey").is_some())
}
fn refusal(value: &Value) -> bool {
    shape(
        value,
        &object(
            json!({"ok":{"const":false},"errors":strings(),"code":{"type":"string","enum":["invalid_input","not_found","conflict","precondition_failed","IDEMPOTENCY_CONFLICT","IDEMPOTENCY_OUTCOME_EXPIRED","IDEMPOTENCY_STORAGE_FAILURE","BACKUP_LOCKED","PURCHASE_VAT_PREFLIGHT_FAILED","EXPENSE_BOOK_FAILED","ORIGINAL_UNAVAILABLE","ORIGINAL_HASH_MISMATCH"]},"appliedRules":strings()}),
            &["ok", "errors"],
        ),
    ) && value["errors"].as_array().is_some_and(|a| !a.is_empty())
}
impl ApiClient {
    pub async fn original_expense_vat_preflight(
        &self,
        company: &str,
        document_id: i64,
    ) -> Result<Value, Error> {
        check_id(document_id)?;
        self.original_expense_request(
            company,
            &format!("vat-preflight/{document_id}"),
            None,
            Some(document_id),
            None,
        )
        .await
    }
    pub async fn original_expense_vat_preflight_apply(
        &self,
        company: &str,
        document_id: i64,
        confirmed: bool,
        expected_actor: Option<&str>,
    ) -> Result<Value, Error> {
        check_id(document_id)?;
        let body = confirmation(confirmed, expected_actor)?;
        self.original_expense_request(
            company,
            &format!("vat-preflight/{document_id}"),
            Some(body),
            Some(document_id),
            None,
        )
        .await
    }
    pub async fn original_expense_book(
        &self,
        company: &str,
        input: &Value,
        confirmed: bool,
        expected_actor: Option<&str>,
    ) -> Result<Value, Error> {
        validate_original_expense_book(input, false)?;
        let mut body = input.clone();
        body.as_object_mut().unwrap().extend(
            confirmation(confirmed, expected_actor)?
                .as_object()
                .unwrap()
                .clone(),
        );
        self.original_expense_request(company, "book", Some(body), None, Some(input))
            .await
    }
    async fn original_expense_request(
        &self,
        company: &str,
        suffix: &str,
        body: Option<Value>,
        document: Option<i64>,
        input: Option<&Value>,
    ) -> Result<Value, Error> {
        let write = body.is_some();
        let unknown = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        };
        let transport = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            }
        };
        let bytes = body
            .map(|v| {
                serde_json::to_vec(&v).map_err(|_| Error::InvalidInput("invalid expense JSON"))
            })
            .transpose()?;
        let mut response = self
            .send(
                if write { Method::POST } else { Method::GET },
                &company_path(company, &format!("expense/original/{suffix}"))?,
                &[],
                bytes,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await
            .map_err(|e| {
                if !write && e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
        let status = response.status().as_u16();
        if write && (status >= 500 || status == 408) {
            return Err(Error::OutcomeUnknown);
        }
        if status != 200 && status != 422 {
            return Err(Error::Refused { status });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| transport())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(unknown());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status == 422 {
                Error::Refused { status }
            } else {
                unknown()
            }
        })?;
        let valid = document.map_or_else(
            || input.is_some_and(|i| book_result(&value, i)),
            |id| snapshot(&value, id),
        );
        if status == 422 {
            if valid || refusal(&value) {
                Ok(value)
            } else {
                Err(Error::Refused { status })
            }
        } else if valid || refusal(&value) {
            Ok(value)
        } else {
            Err(unknown())
        }
    }
}
