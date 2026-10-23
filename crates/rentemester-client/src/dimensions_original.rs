//! Frozen source dimensions boundary. Identity assertions never select an identity; no retries.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DimensionsOriginalOperation {
    Define,
    Member,
    DefinitionLifecycle,
    MemberLifecycle,
    Definitions,
    Members,
    Plan,
    Apply,
    Replace,
    Supersede,
    List,
    BudgetPlan,
    BudgetApply,
    Budgets,
}
use DimensionsOriginalOperation::*;
impl DimensionsOriginalOperation {
    fn write(self) -> bool {
        matches!(
            self,
            Define
                | Member
                | DefinitionLifecycle
                | MemberLifecycle
                | Apply
                | Replace
                | Supersede
                | BudgetApply
        )
    }
    fn suffix(self) -> &'static str {
        match self {
            Define => "/define",
            Member => "/member",
            DefinitionLifecycle => "/definition-lifecycle",
            MemberLifecycle => "/member-lifecycle",
            Definitions => "/definitions",
            Members => "/members",
            Plan => "/plan",
            Apply => "/apply",
            Replace => "/replace",
            Supersede => "/supersede",
            List => "/assignments",
            BudgetPlan => "/budget-plan",
            BudgetApply => "/budget-apply",
            Budgets => "/budgets",
        }
    }
}
fn text(max: usize) -> Value {
    json!({"type":"string","minLength":1,"maxLength":max})
}
fn integer() -> Value {
    json!({"type":"integer","minimum":1,"maximum":9007199254740991u64})
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
pub fn original_dimensions_schema(op: DimensionsOriginalOperation) -> Value {
    let mut p = json!({});
    let mut r = vec![];
    match op {
        Define | Member | DefinitionLifecycle | MemberLifecycle => {
            p["dimensionId"] = text(64);
            r.push("dimensionId");
            if matches!(op, Member | MemberLifecycle) {
                p["memberId"] = text(64);
                r.push("memberId");
            }
            p["name"] = text(160);
            if matches!(op, Define | Member) {
                r.push("name");
            } else {
                p["action"] = json!({"enum":["activate","deactivate","rename","supersede"]});
                r.push("action");
                p["supersedesId"] = text(64);
            }
            if op == Define {
                p["kind"] = text(64);
                r.push("kind");
            }
            if op == Member {
                p["status"] = json!({"enum":["active","inactive"]});
                for k in ["effectiveFrom", "effectiveTo"] {
                    p[k] = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
                }
            }
        }
        Members => {
            p["dimensionId"] = text(64);
        }
        List => {
            p["journalLineId"] = integer();
            r.push("journalLineId");
        }
        Plan | Apply | Replace | BudgetPlan | BudgetApply => {
            let budget = matches!(op, BudgetPlan | BudgetApply);
            let mut allocation = json!({"dimensionId":text(64),"memberId":text(64)});
            let fields = if budget {
                allocation["amount"] = json!({"type":"number","minimum":0});
                vec!["dimensionId", "memberId", "amount"]
            } else {
                allocation["amountMinor"] = integer();
                allocation["currency"] = json!({"type":"string","minLength":3,"maxLength":3});
                vec!["dimensionId", "memberId", "amountMinor", "currency"]
            };
            p["allocations"] = json!({"type":"array","minItems":1,"maxItems":10000,"items":object(allocation,&fields)});
            r.push("allocations");
            p["sourceRef"] = text(500);
            if budget {
                p["accountNo"] = text(64);
                p["period"] = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}$"});
                r.extend(["accountNo", "period", "sourceRef"]);
            } else {
                p["journalLineId"] = integer();
                r.push("journalLineId");
                p["source"] = json!({"enum":["reviewed","imported"]});
                p["reviewedImport"] = json!({"type":"boolean"});
            }
            if matches!(op, Apply | Replace | BudgetApply) {
                p["planHash"] = json!({"type":"string","pattern":"^[a-f0-9]{64}$"});
                p["idempotencyKey"] = text(200);
                r.push("planHash");
            }
            if op == Replace {
                p["expectedAssignmentId"] = integer();
                p["reason"] = text(1000);
                r.extend(["expectedAssignmentId", "reason"]);
            }
        }
        Supersede => {
            p["assignmentId"] = integer();
            p["reason"] = text(1000);
            r.extend(["assignmentId", "reason"]);
        }
        Definitions | Budgets => {}
    }
    if op.write() {
        p["confirm"] = json!({"const":true});
        r.push("confirm");
    }
    object(p, &r)
}
fn hash(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn schema_valid(s: &Value, v: &Value) -> bool {
    if let Some(c) = s.get("const") {
        return c == v;
    }
    if let Some(e) = s["enum"].as_array() {
        return e.contains(v);
    }
    match s["type"].as_str() {
        Some("object") => v.as_object().is_some_and(|o| {
            o.keys().all(|k| s["properties"].get(k).is_some())
                && s["required"]
                    .as_array()
                    .is_some_and(|r| r.iter().all(|k| o.contains_key(k.as_str().unwrap_or(""))))
                && o.iter().all(|(k, v)| schema_valid(&s["properties"][k], v))
        }),
        Some("array") => v.as_array().is_some_and(|a| {
            !a.is_empty() && a.len() <= 10000 && a.iter().all(|v| schema_valid(&s["items"], v))
        }),
        Some("integer") => positive(v),
        Some("number") => minor(v).is_some(),
        Some("boolean") => v.is_boolean(),
        Some("string") => v.as_str().is_some_and(|t| {
            !t.trim().is_empty()
                && !t.chars().any(char::is_control)
                && t.chars().count() >= s["minLength"].as_u64().unwrap_or(1) as usize
                && t.chars().count() <= s["maxLength"].as_u64().unwrap_or(2000) as usize
                && match s["pattern"].as_str() {
                    Some("^[a-f0-9]{64}$") => hash(v),
                    Some("^[0-9]{4}-[0-9]{2}-[0-9]{2}$") => date(t),
                    Some("^[0-9]{4}-[0-9]{2}$") => date(&format!("{t}-01")),
                    _ => true,
                }
        }),
        _ => false,
    }
}
fn date(t: &str) -> bool {
    time::format_description::parse_borrowed::<2>("[year]-[month]-[day]")
        .is_ok_and(|f| t.len() == 10 && time::Date::parse(t, &f).is_ok())
}
fn positive(v: &Value) -> bool {
    v.as_u64().is_some_and(|n| n > 0 && n <= 9007199254740991)
}
// Decimal spelling, not floating epsilon: unsupported subcent input is never rounded.
fn minor(v: &Value) -> Option<u64> {
    if !v.is_number() {
        return None;
    }
    let s = v.to_string();
    let (whole, fraction) = s.split_once('.').unwrap_or((&s, ""));
    if fraction.len() > 2 || s.starts_with('-') {
        return None;
    }
    let n = whole
        .parse::<u64>()
        .ok()?
        .checked_mul(100)?
        .checked_add(if fraction.is_empty() {
            0
        } else {
            fraction.parse::<u64>().ok()? * if fraction.len() == 1 { 10 } else { 1 }
        })?;
    (n <= 9007199254740991).then_some(n)
}
pub fn validate_original_dimensions(
    op: DimensionsOriginalOperation,
    input: &Value,
) -> Result<(), Error> {
    if serde_json::to_vec(input)
        .map_err(|_| Error::InvalidInput("invalid JSON"))?
        .len()
        > 66560
        || !schema_valid(&original_dimensions_schema(op), input)
    {
        return Err(Error::InvalidInput("invalid dimensions input"));
    }
    Ok(())
}
fn exact(v: &Value, keys: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.len() == keys.len() && keys.iter().all(|k| o.contains_key(*k)))
}
fn nonblank(v: &Value) -> bool {
    v.as_str().is_some_and(|s| !s.trim().is_empty())
}
fn failure(v: &Value) -> bool {
    exact(v, &["ok", "errors"])
        && v["ok"] == false
        && v["errors"]
            .as_array()
            .is_some_and(|a| !a.is_empty() && a.iter().all(nonblank))
}
fn allocations_equal(a: &Value, b: &Value, budget: bool) -> bool {
    let normalize = |v: &Value| -> Option<Vec<String>> {
        let mut out = vec![];
        for row in v.as_array()? {
            let mut row = row.clone();
            for key in ["dimensionId", "memberId"] {
                row[key] = json!(row[key].as_str()?.trim());
            }
            if budget {
                row["amount"] = json!(minor(&row["amount"])?);
            }
            out.push(row.to_string());
        }
        out.sort();
        Some(out)
    };
    normalize(a).is_some() && normalize(a) == normalize(b)
}
fn canonical(v: &Value) -> String {
    match v {
        Value::Object(o) => {
            let mut keys = o.keys().collect::<Vec<_>>();
            keys.sort();
            format!(
                "{{{}}}",
                keys.iter()
                    .map(|k| format!("{}:{}", json!(k), canonical(&o[*k])))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        Value::Number(_) => {
            let s = v.to_string();
            if let Some((w, f)) = s.split_once('.') {
                let f = f.trim_end_matches('0');
                if f.is_empty() {
                    w.to_owned()
                } else {
                    format!("{w}.{f}")
                }
            } else {
                s
            }
        }
        _ => v.to_string(),
    }
}
fn bound_hash(p: &Value) -> bool {
    use sha2::{Digest, Sha256};
    let mut unsigned = p.clone();
    let Some(o) = unsigned.as_object_mut() else {
        return false;
    };
    o.remove("planHash");
    p["planHash"] == format!("{:x}", Sha256::digest(canonical(&unsigned).as_bytes()))
}
fn plan(p: &Value, input: &Value, budget: bool) -> bool {
    if !bound_hash(p)
        || !hash(&p["planHash"])
        || !allocations_equal(&p["allocations"], &input["allocations"], budget)
    {
        return false;
    }
    if budget {
        exact(
            p,
            &[
                "schemaVersion",
                "accountNo",
                "period",
                "accountBudget",
                "allocations",
                "sourceRef",
                "planHash",
            ],
        ) && p["schemaVersion"] == "rentemester-dimension-budget-v1"
            && p["accountNo"].as_str() == input["accountNo"].as_str().map(str::trim)
            && p["period"] == input["period"]
            && p["sourceRef"].as_str() == input["sourceRef"].as_str().map(str::trim)
            && minor(&p["accountBudget"]).is_some_and(|total| {
                p["allocations"].as_array().and_then(|a| {
                    a.iter()
                        .try_fold(0u64, |sum, a| sum.checked_add(minor(&a["amount"])?))
                }) == Some(total)
            })
    } else {
        exact(
            p,
            &[
                "schemaVersion",
                "journalLineId",
                "journalEntryId",
                "journalEntryHash",
                "lineCurrency",
                "lineAmountMinor",
                "allocations",
                "source",
                "sourceRef",
                "planHash",
            ],
        ) && p["schemaVersion"] == "rentemester-accounting-dimensions-v1"
            && p["journalLineId"] == input["journalLineId"]
            && positive(&p["journalEntryId"])
            && hash(&p["journalEntryHash"])
            && positive(&p["lineAmountMinor"])
            && p["lineCurrency"].as_str().is_some_and(|s| s.len() == 3)
            && p["source"] == input.get("source").cloned().unwrap_or(json!("reviewed"))
            && if p["source"] == "imported" {
                input["reviewedImport"] == true
                    && p["sourceRef"].as_str() == input["sourceRef"].as_str().map(str::trim)
            } else {
                p["sourceRef"].is_null()
            }
    }
}
fn row(op: DimensionsOriginalOperation, r: &Value, input: &Value) -> bool {
    let keys: &[&str] = match op {
        Definitions => &[
            "id",
            "dimension_id",
            "kind",
            "name",
            "status",
            "event_type",
            "supersedes_dimension_id",
            "actor",
            "principal",
            "created_at",
        ],
        Members => &[
            "id",
            "dimension_id",
            "member_id",
            "name",
            "status",
            "effective_from",
            "effective_to",
            "event_type",
            "supersedes_member_id",
            "actor",
            "principal",
            "created_at",
        ],
        List => &[
            "id",
            "journal_line_id",
            "journal_entry_id",
            "journal_entry_hash",
            "line_currency",
            "line_amount_minor",
            "allocations_json",
            "source",
            "plan_hash",
            "event_type",
            "supersedes_assignment_id",
            "idempotency_key",
            "reason",
            "actor",
            "principal",
            "created_at",
            "source_ref",
        ],
        Budgets => &[
            "id",
            "dimension_id",
            "member_id",
            "account_no",
            "period",
            "amount",
            "source_ref",
            "plan_hash",
            "event_type",
            "supersedes_budget_id",
            "idempotency_key",
            "reason",
            "actor",
            "principal",
            "created_at",
        ],
        _ => return false,
    };
    exact(r, keys)
        && positive(&r["id"])
        && nonblank(&r["actor"])
        && r["actor"] == r["principal"]
        && nonblank(&r["created_at"])
        && match op {
            Definitions | Members => {
                nonblank(&r["dimension_id"])
                    && nonblank(&r["name"])
                    && ["active", "inactive", "superseded"]
                        .contains(&r["status"].as_str().unwrap_or(""))
                    && [
                        "defined",
                        "activated",
                        "deactivated",
                        "renamed",
                        "superseded",
                    ]
                    .contains(&r["event_type"].as_str().unwrap_or(""))
                    && (op != Members
                        || (nonblank(&r["member_id"])
                            && (input.get("dimensionId").is_none()
                                || r["dimension_id"] == input["dimensionId"])))
            }
            List => {
                r["journal_line_id"] == input["journalLineId"]
                    && positive(&r["journal_entry_id"])
                    && hash(&r["journal_entry_hash"])
                    && hash(&r["plan_hash"])
                    && ["assigned", "superseded"].contains(&r["event_type"].as_str().unwrap_or(""))
                    && r["allocations_json"]
                        .as_str()
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        .is_some_and(|a| a.is_array())
            }
            Budgets => {
                nonblank(&r["dimension_id"])
                    && nonblank(&r["member_id"])
                    && nonblank(&r["account_no"])
                    && minor(&r["amount"]).is_some()
                    && hash(&r["plan_hash"])
                    && r["event_type"] == "allocated"
            }
            _ => false,
        }
}
fn valid(op: DimensionsOriginalOperation, v: &Value, input: &Value) -> bool {
    if v["ok"] == false {
        return failure(v);
    }
    if v["ok"] != true {
        return false;
    }
    match op {
        Plan | BudgetPlan => exact(v, &["ok", "plan"]) && plan(&v["plan"], input, op == BudgetPlan),
        Definitions | Members | List | Budgets => {
            let key = match op {
                Definitions => "definitions",
                Members => "members",
                List => "assignments",
                _ => "budgets",
            };
            exact(v, &["ok", key])
                && v[key]
                    .as_array()
                    .is_some_and(|a| a.len() <= 100000 && a.iter().all(|r| row(op, r, input)))
        }
        BudgetApply => {
            exact(v, &["ok", "ids", "idempotent", "errors"])
                && v["errors"] == json!([])
                && v["idempotent"].is_boolean()
                && v["ids"].as_array().is_some_and(|a| {
                    a.len() == input["allocations"].as_array().map_or(0, Vec::len)
                        && a.iter().all(positive)
                        && a.iter().enumerate().all(|(i, id)| !a[..i].contains(id))
                })
        }
        Apply | Replace => {
            let shape = if op == Apply {
                exact(v, &["ok", "id", "idempotent", "planHash"])
            } else {
                exact(v, &["ok", "id", "supersededId", "idempotent", "planHash"])
                    && positive(&v["supersededId"])
                    || v["idempotent"] == true && exact(v, &["ok", "id", "idempotent", "planHash"])
            };
            shape
                && positive(&v["id"])
                && v["idempotent"].is_boolean()
                && v["planHash"] == input["planHash"]
                && hash(&v["planHash"])
        }
        _ => {
            exact(v, &["ok", "id", "idempotent"])
                && positive(&v["id"])
                && v["idempotent"].is_boolean()
        }
    }
}
// Reject duplicate keys recursively before trusting a receipt.
struct UniqueJson(Value);
impl<'de> serde::Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<UniqueJson, E> {
                Ok(UniqueJson(json!(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<UniqueJson, E> {
                Ok(UniqueJson(json!(v)))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<UniqueJson, E> {
                Ok(UniqueJson(json!(v)))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<UniqueJson, E> {
                Ok(UniqueJson(json!(v)))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<UniqueJson, E> {
                Ok(UniqueJson(json!(v)))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<UniqueJson, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<UniqueJson, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<UniqueJson, A::Error> {
                let mut out = vec![];
                while let Some(v) = a.next_element::<UniqueJson>()? {
                    out.push(v.0);
                }
                Ok(UniqueJson(Value::Array(out)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<UniqueJson, A::Error> {
                let mut out = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if out.contains_key(&k) {
                        return Err(serde::de::Error::custom("duplicate key"));
                    }
                    let v = a.next_value::<UniqueJson>()?;
                    out.insert(k, v.0);
                }
                Ok(UniqueJson(Value::Object(out)))
            }
        }
        d.deserialize_any(Visitor)
    }
}
impl ApiClient {
    pub async fn original_dimensions(
        &self,
        company: &str,
        operation: DimensionsOriginalOperation,
        input: &Value,
        actor: Option<&str>,
        principal: Option<&str>,
    ) -> Result<Value, Error> {
        validate_original_dimensions(operation, input)?;
        for assertion in [actor, principal] {
            if assertion.is_some_and(|a| {
                a.trim().is_empty() || a.len() > 256 || a.chars().any(char::is_control)
            }) || (assertion.is_some() && !operation.write())
            {
                return Err(Error::InvalidInput(
                    "identity is a write credential equality assertion only",
                ));
            }
        }
        let write = operation.write();
        let ambiguous = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        };
        let post = !matches!(operation, Definitions | Members | List | Budgets);
        let mut wire = input.clone();
        if let Some(actor) = actor {
            wire["actor"] = json!(actor);
        }
        if let Some(principal) = principal {
            wire["principal"] = json!(principal);
        }
        let query: Vec<(String, String)> = if post {
            vec![]
        } else {
            input
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        v.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v.to_string()),
                    )
                })
                .collect()
        };
        let mut response = self
            .send(
                if post { Method::POST } else { Method::GET },
                &company_path(
                    company,
                    &format!("original/dimensions{}", operation.suffix()),
                )?,
                &query
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect::<Vec<_>>(),
                if post {
                    Some(
                        serde_json::to_vec(&wire)
                            .map_err(|_| Error::InvalidInput("invalid JSON"))?,
                    )
                } else {
                    None
                },
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
        if status >= 500 || status == 408 {
            return Err(if write {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            });
        }
        if status != 200 && status != 422 {
            return Err(Error::Refused { status });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| ambiguous())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(ambiguous());
            }
            bytes.extend_from_slice(&chunk);
        }
        let v = serde_json::from_slice::<UniqueJson>(&bytes)
            .map_err(|_| {
                if status == 422 {
                    Error::Refused { status }
                } else {
                    ambiguous()
                }
            })?
            .0;
        if status == 422 {
            return if failure(&v) {
                Ok(v)
            } else {
                Err(Error::Refused { status })
            };
        }
        if valid(operation, &v, input) {
            Ok(v)
        } else {
            Err(ambiguous())
        }
    }
}
