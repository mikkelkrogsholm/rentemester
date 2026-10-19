use rentemester_client::{ApiClient, Error, JournalMutation, Report};
use serde::Deserialize;
use serde_json::{Value, json};

pub fn list() -> Value {
    let mut tools = Vec::new();
    for (name, description) in [
        ("me", "Current authenticated user and workspace context"),
        (
            "companies",
            "Discover companies accessible to the authenticated session",
        ),
    ] {
        tools.push(tool(
            name,
            description,
            json!({"type":"object","properties":{},"additionalProperties":false}),
            true,
        ));
    }
    for (name, description) in [
        ("journal", "Read journal entries"),
        ("trial_balance", "Read trial balance"),
        ("income_statement", "Read income statement"),
        ("balance", "Read balance sheet"),
    ] {
        tools.push(tool(name, description, json!({"type":"object","required":["company"],"properties":{
            "company":company_schema(),"year":{"type":"string","maxLength":128,"description":"Server fiscal year selector"},
            "account":{"type":"string","maxLength":128,"description":"Optional server account filter"}},"additionalProperties":false}), true));
    }
    for (name, description) in [
        (
            "post_journal",
            "Post one confirmed manual journal through authenticated server authority",
        ),
        (
            "reverse_journal",
            "Reverse a journal through authenticated server authority",
        ),
    ] {
        tools.push(tool(name, description, json!({"type":"object","required":["company","envelope"],"properties":{
            "company":company_schema(),"envelope":{"type":"string","maxLength":65536,"description":"Exact server JSON envelope string: confirm:true and input with format, expectedRuleVersion, idempotencyKey and request. Preserved byte-for-byte; never generate a replacement key. Reconcile outcome_unknown using the original key before retrying."}},"additionalProperties":false}), false));
    }
    json!({"tools":tools})
}
fn company_schema() -> Value {
    json!({"type":"string","minLength":1,"pattern":"^[A-Za-z0-9_-]+$","description":"Explicit company slug from companies"})
}
fn tool(name: &str, description: &str, schema: Value, read_only: bool) -> Value {
    json!({"name":name,"description":description,"inputSchema":schema,"annotations":{"readOnlyHint":read_only,"destructiveHint":!read_only,"idempotentHint":read_only,"openWorldHint":true}})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    company: String,
    year: Option<String>,
    account: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteArgs {
    company: String,
    envelope: String,
}

/// Invalid RPC tool/arguments are protocol errors; HTTP/domain failures are tool results.
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    if !args.is_object() {
        return Err("expected arguments object");
    }
    for field in ["year", "account"] {
        if args.get(field).is_some_and(|v| !v.is_string()) {
            return Err("report selectors must be strings");
        }
    }
    let mut idempotency = None;
    let outcome = match name {
        "me" | "companies" => {
            serde_json::from_value::<Empty>(args).map_err(|_| "expected empty arguments object")?;
            if name == "me" {
                api.me().await
            } else {
                api.companies().await
            }
        }
        "journal" | "trial_balance" | "income_statement" | "balance" => {
            let args: ReadArgs =
                serde_json::from_value(args).map_err(|_| "invalid report arguments")?;
            if args.year.as_ref().is_some_and(|v| v.len() > 128)
                || args.account.as_ref().is_some_and(|v| v.len() > 128)
            {
                return Err("report selector too long");
            }
            let report = match name {
                "journal" => Report::Journal,
                "trial_balance" => Report::TrialBalance,
                "income_statement" => Report::IncomeStatement,
                _ => Report::Balance,
            };
            api.report(
                &args.company,
                report,
                args.year.as_deref(),
                args.account.as_deref(),
            )
            .await
        }
        "post_journal" | "reverse_journal" => {
            let args: WriteArgs =
                serde_json::from_value(args).map_err(|_| "invalid journal arguments")?;
            // Used only to return the original key with uncertainty, never to rewrite input.
            if args.envelope.len() <= 65_536 {
                idempotency = serde_json::from_str::<Value>(&args.envelope)
                    .ok()
                    .and_then(|v| {
                        v.pointer("/input/idempotencyKey")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    });
            }
            api.manual_journal(
                &args.company,
                if name == "post_journal" {
                    JournalMutation::Post
                } else {
                    JournalMutation::Reverse
                },
                args.envelope.into_bytes(),
            )
            .await
        }
        _ => return Err("unknown tool"),
    };
    let value = match outcome {
        Ok(data) => {
            json!({"ok":data.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":data})
        }
        Err(error) => {
            let (code, status) = match error {
                Error::InvalidInput(_) => ("invalid_input", None),
                Error::Transport => ("transport", None),
                Error::Refused { status } => ("server_refused", Some(status)),
                Error::Protocol => ("protocol", None),
                Error::OutcomeUnknown => ("outcome_unknown", None),
            };
            let mut details = json!({"code":code,"message":error.to_string()});
            if let Some(status) = status {
                details["status"] = json!(status);
            }
            if let Some(key) = idempotency {
                details["idempotencyKey"] = json!(key);
            }
            json!({"ok":false,"error":details})
        }
    };
    Ok(
        json!({"isError":value["ok"] == false,"content":[{"type":"text","text":value.to_string()}],"structuredContent":value}),
    )
}
