//! Flat source journal transport. Explicit caller keys only; no preflight reads or retries.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::Value;

#[derive(Clone, Copy)]
enum Operation {
    Post,
    DryRun,
    Reverse,
    KeyedPost,
    KeyedReverse,
}
impl Operation {
    fn suffix(self) -> &'static str {
        match self {
            Self::Post => "post",
            Self::DryRun => "dry-run",
            Self::Reverse => "reverse",
            Self::KeyedPost => "keyed-post",
            Self::KeyedReverse => "keyed-reverse",
        }
    }
    fn durable(self) -> bool {
        !matches!(self, Self::DryRun)
    }
}
fn text(v: &Value) -> bool {
    v.as_str().is_some_and(|s| !s.trim().is_empty())
}
fn strings(v: &Value) -> bool {
    v.as_array().is_some_and(|a| a.iter().all(Value::is_string))
}
fn id(v: &Value) -> bool {
    v.as_u64()
        .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
}
fn hash(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}
fn result(v: &Value, op: Operation, strict: bool) -> bool {
    let Some(fields) = v.as_object() else {
        return false;
    };
    if !fields.keys().all(|k| {
        matches!(
            k.as_str(),
            "ok" | "code"
                | "errors"
                | "appliedRules"
                | "entryId"
                | "entryNo"
                | "entryHash"
                | "originalEntryId"
                | "previousHash"
                | "accountEffects"
        )
    }) {
        return false;
    }
    if v["ok"] == false {
        return strings(&v["errors"])
            && v["errors"].as_array().is_some_and(|a| !a.is_empty())
            && v.get("code").is_none_or(text)
            && (!strict || text(&v["code"]))
            && v.get("appliedRules").is_none_or(strings)
            && [
                "entryId",
                "entryNo",
                "entryHash",
                "originalEntryId",
                "previousHash",
                "accountEffects",
            ]
            .iter()
            .all(|k| v.get(*k).is_none());
    }
    if strict
        || v["ok"] != true
        || !strings(&v["appliedRules"])
        || !v["errors"].as_array().is_some_and(Vec::is_empty)
        || !id(&v["entryId"])
        || !text(&v["entryNo"])
        || !hash(&v["entryHash"])
        || v.get("code").is_some()
    {
        return false;
    }
    if matches!(op, Operation::Reverse) {
        if !id(&v["originalEntryId"]) {
            return false;
        }
    } else if v.get("originalEntryId").is_some() {
        return false;
    }
    if !matches!(op, Operation::DryRun) {
        return v.get("previousHash").is_none() && v.get("accountEffects").is_none();
    }
    if v["previousHash"] != "GENESIS" && !hash(&v["previousHash"]) {
        return false;
    }
    let Some(effects) = v["accountEffects"].as_array() else {
        return false;
    };
    let mut previous = None;
    for effect in effects {
        if !text(&effect["accountNo"])
            || !effect["accountName"].is_string()
            || !["balanceBefore", "balanceAfter", "delta"]
                .iter()
                .all(|k| effect[*k].is_number())
            || effect.as_object().is_none_or(|o| o.len() != 5)
        {
            return false;
        }
        let account = effect["accountNo"].as_str().unwrap();
        if previous.is_some_and(|p| p >= account) {
            return false;
        }
        previous = Some(account);
    }
    !effects.is_empty()
}
fn keyed_result(v: &Value, op: Operation, strict: bool) -> bool {
    let mut core = v.clone();
    let Some(fields) = core.as_object_mut() else {
        return false;
    };
    let metadata = fields.remove("idempotency");
    let core_op = if matches!(op, Operation::KeyedReverse) {
        Operation::Reverse
    } else {
        Operation::Post
    };
    if !result(&core, core_op, strict) {
        return false;
    }
    if core["ok"] == false {
        return metadata.is_none();
    }
    let Some(meta) = metadata else {
        return false;
    };
    if meta.as_object().is_none_or(|o| o.len() != 4)
        || !meta["replayed"].is_boolean()
        || !id(&meta["receiptId"])
    {
        return false;
    }
    let timestamp = |key: &str| {
        meta[key].as_str().and_then(|s| {
            time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
        })
    };
    matches!((timestamp("createdAt"), timestamp("expiresAt")), (Some(created), Some(expires)) if expires > created)
}
impl ApiClient {
    pub async fn journal_original_keyed_post(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        journal_request(self, company, bytes, Operation::KeyedPost, confirmed).await
    }
    pub async fn journal_original_keyed_reverse(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        journal_request(self, company, bytes, Operation::KeyedReverse, confirmed).await
    }

    pub async fn journal_original_post(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        journal_request(self, company, bytes, Operation::Post, confirmed).await
    }
    pub async fn journal_original_dry_run(
        &self,
        company: &str,
        bytes: Vec<u8>,
    ) -> Result<Value, Error> {
        journal_request(self, company, bytes, Operation::DryRun, false).await
    }
    pub async fn journal_original_reverse(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        journal_request(self, company, bytes, Operation::Reverse, confirmed).await
    }
}
async fn journal_request(
    api: &ApiClient,
    company: &str,
    bytes: Vec<u8>,
    op: Operation,
    confirmed: bool,
) -> Result<Value, Error> {
    if bytes.len() > 65536 {
        return Err(Error::InvalidInput("journal request exceeds65536 bytes"));
    }
    let input: Value = serde_json::from_slice(&bytes)
        .map_err(|_| Error::InvalidInput("expected journal JSON object"))?;
    let Some(fields) = input.as_object() else {
        return Err(Error::InvalidInput("expected journal JSON object"));
    };
    if op.durable() && (!confirmed || input["confirm"] != true) {
        return Err(Error::InvalidInput("explicit journal invocation required"));
    }
    let keyed = matches!(op, Operation::KeyedPost | Operation::KeyedReverse);
    if keyed
        && !input["idempotencyKey"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && (1..=128).contains(&s.encode_utf16().count()))
    {
        return Err(Error::InvalidInput("invalid journal idempotency key"));
    }
    let valid = match op {
        Operation::Post => fields.len() == 2 && input["input"].is_object(),
        Operation::KeyedPost => fields.len() == 3 && input["input"].is_object(),
        Operation::DryRun => fields.len() == 1 && input["input"].is_object(),
        Operation::Reverse | Operation::KeyedReverse => fields.keys().all(|k| {
            (keyed && k == "idempotencyKey")
                || matches!(
                    k.as_str(),
                    "entryId"
                        | "entryNo"
                        | "matchText"
                        | "matchDate"
                        | "matchDocumentId"
                        | "transactionDate"
                        | "reason"
                        | "confirm"
                )
        }),
    };
    if !valid {
        return Err(Error::InvalidInput("invalid journal wire body"));
    }
    let path = company_path(company, &format!("journal/original/{}", op.suffix()))?;
    let mut response = api
        .send(
            Method::POST,
            &path,
            &[],
            Some(bytes),
            api.cookie.as_ref().map(|v| v.as_str()),
        )
        .await
        .map_err(|e| {
            if !op.durable() && e == Error::OutcomeUnknown {
                Error::Transport
            } else {
                e
            }
        })?;
    let status = response.status().as_u16();
    if op.durable() && (status >= 500 || status == 408) {
        return Err(Error::OutcomeUnknown);
    }
    if !matches!(status, 200 | 422) {
        return Err(Error::Refused { status });
    }
    let protocol = || {
        if status == 422 {
            Error::Refused { status }
        } else if op.durable() {
            Error::OutcomeUnknown
        } else {
            Error::Protocol
        }
    };
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| protocol())? {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
            return Err(protocol());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| protocol())?;
    if if keyed {
        keyed_result(&value, op, status == 422)
    } else {
        result(&value, op, status == 422)
    } {
        Ok(value)
    } else {
        Err(protocol())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn posted() -> Value {
        json!({"ok":true,"entryId":17,"entryNo":"JE-000017","entryHash":"a".repeat(64),"appliedRules":["DK-BOOKKEEPING-APPEND-ONLY-001"],"errors":[]})
    }
    #[test]
    fn source_dtos_are_operation_specific_and_numeric() {
        let post = posted();
        assert!(result(&post, Operation::Post, false));
        assert!(!result(
            &json!({"ok":true,"data":post}),
            Operation::Post,
            false
        ));
        for (key, value) in [
            ("entryId", json!("17")),
            ("entryHash", json!("invalid")),
            ("entryId", json!(9_007_199_254_740_992_u64)),
            ("errors", json!(["unexpected"])),
        ] {
            let mut bad = post.clone();
            bad[key] = value;
            assert!(!result(&bad, Operation::Post, false));
        }
        let mut reverse = post.clone();
        reverse["originalEntryId"] = json!(16);
        assert!(result(&reverse, Operation::Reverse, false));
        assert!(!result(&reverse, Operation::Post, false));
        assert!(!result(&post, Operation::Reverse, false));
        let mut dry = post;
        dry["previousHash"] = json!("GENESIS");
        dry["accountEffects"] = json!([
            {"accountNo":"1000","accountName":"Synthetic asset","balanceBefore":0,"balanceAfter":10.5,"delta":10.5},
            {"accountNo":"2000","accountName":"Synthetic bank","balanceBefore":0,"balanceAfter":-10.5,"delta":-10.5}
        ]);
        assert!(result(&dry, Operation::DryRun, false));
        assert!(!result(&dry, Operation::Post, false));
        dry["accountEffects"][0]["delta"] = json!("10.50");
        assert!(!result(&dry, Operation::DryRun, false));
        dry["accountEffects"][0]["delta"] = json!(10.5);
        dry["accountEffects"].as_array_mut().unwrap().reverse();
        assert!(!result(&dry, Operation::DryRun, false));
    }
    #[test]
    fn refusal_cannot_forge_success_or_usage_with_untyped_errors() {
        let mut refusal = json!({"ok":false,"code":"journal_selection_usage","errors":["Multiple journal entries matched"],"appliedRules":[]});
        assert!(result(&refusal, Operation::Reverse, true));
        refusal["errors"] = json!([42]);
        assert!(!result(&refusal, Operation::Reverse, true));
        refusal["errors"] = json!(["refused"]);
        refusal["entryId"] = json!(17);
        assert!(!result(&refusal, Operation::Reverse, true));
    }
    #[tokio::test]
    async fn no_confirmation_api_for_dry_run_and_unconfirmed_writes_do_not_send() {
        let api =
            ApiClient::new("http://127.0.0.1:1", true, Some("session=synthetic".into())).unwrap();
        let bytes = serde_json::to_vec(&json!({"input":{},"confirm":true})).unwrap();
        assert!(matches!(
            api.journal_original_post("synthetic", bytes.clone(), false)
                .await,
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            api.journal_original_reverse("synthetic", bytes.clone(), false)
                .await,
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            api.journal_original_dry_run("synthetic", bytes).await,
            Err(Error::InvalidInput(_))
        ));
    }
}
