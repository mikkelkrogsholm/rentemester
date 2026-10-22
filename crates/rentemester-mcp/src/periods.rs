//! Explicit review and commitment over existing server period controls.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, Error, PeriodMutation, period_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "vat"
            | "periods"
            | "period_close_readiness"
            | "period_close_status"
            | "period_list"
            | "period_close_review"
            | "period_close"
            | "review_period"
            | "close_period"
            | "reopen_period"
            | "report_period"
    )
}
pub fn tools() -> Vec<Value> {
    let company = company_schema();
    let mut result = vec![
        tool(
            "vat",
            "Read server-calculated VAT totals, registered cadence, period windows and filing state; optional fiscal year",
            json!({"type":"object","required":["company"],"properties":{"company":company,"year":{"type":"string","maxLength":128}},"additionalProperties":false}),
            true,
        ),
        tool(
            "periods",
            "List effective period status, original closePacketHash and immutable receipt reference; read before reconciling uncertain writes",
            json!({"type":"object","required":["company"],"properties":{"company":company},"additionalProperties":false}),
            true,
        ),

    ];
    for (name, op, description) in [
        (
            "review_period",
            PeriodMutation::Review,
            "Append explicit review of current readiness controls. Returns review.id and review.packet.hash; inspect controls before close",
        ),
        (
            "close_period",
            PeriodMutation::Close,
            "Close using caller-selected current reviewId and packetHash; stale proof, blockers, future bounds and overlaps refused. Forced close unavailable. Lock prevents dated posting",
        ),
        (
            "reopen_period",
            PeriodMutation::Reopen,
            "Reopen closed unreported period with nonblank reason; appends audit without rewriting original close",
        ),
        (
            "report_period",
            PeriodMutation::Report,
            "Record receipt of externally completed filing using original closePacketHash from periods; reference immutable, reported period cannot reopen. No filing, email or network submission",
        ),
    ] {
        result.push(tool(name,&format!("{description}. Company-review authority. Exact body bytes, confirm:true inside body and outer confirm:true. No generic idempotency keys or automatic retries; reconcile outcome_unknown with periods/readiness before further action."),json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company,"body":native_body(period_body_schema(op)),"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    result.extend(original_tools());
    result
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
    year: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    company: String,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    Ok(match name {
        "vat" | "periods" => {
            if args.get("year").is_some_and(|v| !v.is_string())
                || (name == "periods" && args.get("year").is_some())
            {
                return Err("only VAT accepts string year");
            }
            let a: Read =
                serde_json::from_value(args).map_err(|_| "invalid period read arguments")?;
            if name == "vat" {
                api.vat(&a.company, a.year.as_deref()).await
            } else {
                api.periods(&a.company).await
            }
        }
        _ => {
            let a: Write = serde_json::from_value(args)
                .map_err(|_| "period write requires company, exact native body and confirm:true")?;
            if !a.confirm {
                return Err("explicit confirmation required");
            }
            let op = match name {
                "review_period" => PeriodMutation::Review,
                "close_period" => PeriodMutation::Close,
                "reopen_period" => PeriodMutation::Reopen,
                "report_period" => PeriodMutation::Report,
                _ => return Err("unknown period tool"),
            };
            api.period_mutation(&a.company, op, a.body.into_bytes(), a.confirm)
                .await
        }
    })
}

/// The original five use structured source arguments, not the native body-string aliases.
pub fn original(name: &str) -> bool {
    matches!(name, "period_close_readiness" | "period_close_status" | "period_list" | "period_close_review" | "period_close")
}
fn original_schema(name: &str) -> Value {
    let date = json!({"type":"string","minLength":10,"maxLength":10,"format":"date"});
    let positive = json!({"type":"integer","minimum":1,"maximum":9007199254740991_u64});
    let mut properties = json!({"company":company_schema()});
    let mut required = vec!["company"];
    if matches!(name, "period_close_readiness" | "period_close_review" | "period_close") {
        properties["from"] = date.clone(); properties["to"] = date;
        required.extend(["from", "to"]);
    }
    if name == "period_close_status" { properties["reviewId"] = positive.clone(); required.push("reviewId"); }
    if matches!(name, "period_close_review" | "period_close") { properties["confirm"] = json!({"type":"boolean"}); }
    if name == "period_close" {
        properties["kind"] = json!({"type":"string","enum":["vat_period","vat_quarter","fiscal_year","custom"],"default":"vat_period"});
        properties["status"] = json!({"type":"string","enum":["closed","reported"],"default":"closed"});
        properties["reference"] = json!({"type":"string"});
        properties["reason"] = json!({"type":"string","minLength":1});
        properties["force"] = json!({"type":"boolean","default":false});
        properties["packetHash"] = json!({"type":"string","minLength":64,"maxLength":64,"pattern":"^[a-fA-F0-9]{64}$"});
        properties["reviewId"] = positive;
    }
    json!({"type":"object","required":required,"properties":properties,"additionalProperties":false})
}
fn original_tools() -> Vec<Value> {
    ["period_close_readiness", "period_close_status", "period_list", "period_close_review", "period_close"].into_iter().map(|name| {
        let read = !matches!(name, "period_close_review" | "period_close");
        let mut value = tool(name, "Original period source contract. Current cookie or service CompanyRead for reads; CompanyReview for writes and live CompanyPeriodForceClose additionally for force. Review computes and persists the current packet atomically, without caller hash. Close requires exact persisted proof. No keys or automatic retries; reconcile unknown writes with status/list/readiness before further effects.", original_schema(name), read);
        value["annotations"] = json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read,"openWorldHint":false});
        value["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"errors":{"type":"array","items":{"type":"string"}},"data":{"type":"object"},"code":{"type":"string"},"appliedRules":{"type":"array","items":{"type":"string"}}},"additionalProperties":false});
        value
    }).collect()
}
fn valid_original(value: &Value, schema: &Value) -> bool {
    if schema.get("enum").is_some_and(|values| !values.as_array().unwrap().contains(value)) { return false; }
    match schema["type"].as_str().unwrap() {
        "object" => value.as_object().is_some_and(|fields| {
            schema["required"].as_array().unwrap().iter().all(|k| fields.contains_key(k.as_str().unwrap()))
                && fields.iter().all(|(k,v)| schema["properties"].get(k).is_some_and(|s| valid_original(v,s)))
        }),
        "boolean" => value.is_boolean(),
        "integer" => value.as_u64().is_some_and(|n| (1..=9007199254740991_u64).contains(&n)),
        "string" => value.as_str().is_some_and(|s| {
            let n = s.chars().count() as u64;
            schema.get("minLength").is_none_or(|v| n >= v.as_u64().unwrap())
                && schema.get("maxLength").is_none_or(|v| n <= v.as_u64().unwrap())
                && (schema.get("pattern").is_none() || if schema.get("maxLength") == Some(&json!(64)) { s.bytes().all(|b| b.is_ascii_hexdigit()) } else { s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') })
                && (schema["format"] != "date" || valid_date(s))
        }),
        _ => false,
    }
}
fn valid_date(s: &str) -> bool {
    // The client performs the calendar check too; this rejects malformed schema arguments before confirmation.
    let bytes = s.as_bytes();
    if bytes.len() != 10 || !bytes.iter().enumerate().all(|(i,b)| if i == 4 || i == 7 { *b == b'-' } else { b.is_ascii_digit() }) { return false; }
    let year: u32 = s[..4].parse().unwrap(); let month: u32 = s[5..7].parse().unwrap(); let day: u32 = s[8..].parse().unwrap();
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month { 1|3|5|7|8|10|12 => 31, 4|6|9|11 => 30, 2 => if leap {29} else {28}, _ => 0 };
    day > 0 && day <= days
}
fn source_result(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":value["ok"]!=true})
}
fn source_wrap(mut core: Value) -> Value {
    let fields = core.as_object_mut().unwrap();
    let ok = fields.remove("ok").unwrap();
    let errors = fields.remove("errors").unwrap_or_else(|| json!([]));
    let rules = fields.remove("appliedRules");
    let mut envelope = json!({"ok":ok,"errors":errors});
    if let Some(rules) = rules { envelope["appliedRules"] = rules; }
    if ok == true { envelope["data"] = core; }
    else if let Some(code) = core.get("code") { envelope["code"] = code.clone(); }
    else {
        let errors = envelope["errors"].as_array().unwrap().iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" ").to_lowercase();
        let code = if errors.contains("falls in closed period") || errors.contains("falls in reported period") || errors.contains("in a closed period") || errors.contains("perioden er lukket") || errors.contains("periode er lukket") { Some("PERIOD_CLOSED") }
            else if errors.contains("forudsætning ikke opfyldt") || errors.contains("precondition not met") || errors.contains("precondition missing") || errors.contains("must be posted") || errors.contains("skal bogf") || errors.contains("skal være bogf") { Some("PRECONDITION_MISSING") }
            else if errors.contains("does not exist") || errors.contains("findes ikke") || errors.contains("no such") || errors.contains("kunne ikke findes") { Some("NOT_FOUND") }
            else if errors.contains("ikke momsregistreret") { Some("NOT_VAT_REGISTERED") } else { None };
        if let Some(code) = code { envelope["code"] = json!(code); }
    }
    source_result(envelope)
}
pub async fn call_original(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    if !original(name) || !valid_original(&args, &original_schema(name)) { return Err("invalid original period arguments"); }
    let company = args["company"].as_str().unwrap();
    let write = matches!(name, "period_close_review" | "period_close");
    if write && args["confirm"] != true {
        return Ok(source_result(json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":[format!("confirm: true required for write tool {name}")]})));
    }
    let outcome = match name {
        "period_close_readiness" => api.period_mcp_readiness(company, args["from"].as_str().unwrap(), args["to"].as_str().unwrap()).await,
        "period_close_status" => api.period_mcp_status(company, args["reviewId"].as_i64().unwrap()).await,
        "period_list" => api.period_mcp_list(company).await,
        _ => {
            let mut wire = args.as_object().unwrap().clone(); wire.remove("company");
            let from = wire.remove("from").unwrap(); let to = wire.remove("to").unwrap();
            wire.insert("periodStart".into(), from); wire.insert("periodEnd".into(), to);
            let bytes = serde_json::to_vec(&wire).map_err(|_| "period serialization failed")?;
            if bytes.len() > 65536 { return Err("period request exceeds65536 bytes"); }
            if name == "period_close_review" { api.period_mcp_review(company,bytes,true).await } else { api.period_mcp_close(company,bytes,true).await }
        }
    };
    Ok(match outcome {
        Ok(core) => source_wrap(core),
        Err(error) => {
            let code = match error { Error::InvalidInput(_) => "invalid_input", Error::Refused{..} => "server_refused", Error::Transport => "transport", Error::Protocol => "protocol", Error::OutcomeUnknown => "outcome_unknown" };
            source_result(json!({"ok":false,"code":code,"errors":[error.to_string()]}))
        }
    })
}
