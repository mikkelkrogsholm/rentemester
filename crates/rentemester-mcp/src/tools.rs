use rentemester_client::{ApiClient, Confirmation, DomainMutation, Error, JournalMutation, Report};
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
    for (name, description) in [
        (
            "contacts",
            "List native customer and vendor master data in contacts.customers and contacts.vendors",
        ),
        (
            "accounts",
            "Read explicit chart accounts and current confirmed posting-role mappings before selecting vendor defaults or invoice posting accounts",
        ),
        ("documents", "List native document metadata"),
        (
            "invoices",
            "List issued invoices, with optional server fiscal year selector",
        ),
    ] {
        let mut schema = json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false});
        if name == "invoices" {
            schema["properties"]["year"] = json!({"type":"string","maxLength":128});
        }
        tools.push(tool(name, description, schema, true));
    }
    tools.push(tool("preview_invoice","Nonmutating native invoice PDF preview. Review PDF bytes (JSON integer array) and preserve identical body for issue_invoice. No invoice number is reserved; preview errors never imply a mutation.",json!({"type":"object","required":["company","body"],"properties":{"company":company_schema(),"body":native_body(rentemester_client::invoice_body_schema())},"additionalProperties":false}),true));
    for (name, operation, description) in [
        (
            "create_customer",
            DomainMutation::CreateCustomer,
            "Create customer master data",
        ),
        (
            "update_customer",
            DomainMutation::UpdateCustomer(1),
            "Patch one active customer; omitted fields survive, explicit null clears optional fields",
        ),
        (
            "create_vendor",
            DomainMutation::CreateVendor,
            "Create supplier master data, including optional typed identity and validated expense account default",
        ),
        (
            "update_vendor",
            DomainMutation::UpdateVendor(1),
            "Patch one active supplier; source documents remain immutable",
        ),
        (
            "delete_customer",
            DomainMutation::DeleteCustomer(1),
            "Delete an unused customer after confirmation; open or overpaid invoices refuse; immutable invoice snapshots survive",
        ),
        (
            "delete_vendor",
            DomainMutation::DeleteVendor(1),
            "Delete an unused supplier after confirmation; obligations and historical foreign keys refuse; original evidence survives",
        ),
        (
            "issue_invoice",
            DomainMutation::IssueInvoice,
            "Issue immutable native invoice/PDF from the identical reviewed preview input; numbering remains server-authoritative",
        ),
        (
            "post_invoice",
            DomainMutation::PostInvoice,
            "Post an issued invoice using explicit revenue account, confirmed debtor and output-VAT roles, original evidence and open posting period",
        ),
    ] {
        let mut schema = json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":native_body(rentemester_client::domain_body_schema(operation)),"confirm":{"const":true}},"additionalProperties":false});
        if matches!(
            operation,
            DomainMutation::UpdateCustomer(_)
                | DomainMutation::UpdateVendor(_)
                | DomainMutation::DeleteCustomer(_)
                | DomainMutation::DeleteVendor(_)
        ) {
            schema["required"].as_array_mut().unwrap().push(json!("id"));
            schema["properties"]["id"] = json!({"type":"integer","minimum":1});
        }
        if matches!(operation, DomainMutation::IssueInvoice) {
            schema["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("previewReviewed"));
            schema["properties"]["previewReviewed"] = json!({"const":true,"description":"Caller attests reviewing preview_invoice for this identical body; this is not a server preview token"});
        }
        tools.push(tool(name,&format!("{description}. Native flat body is forwarded exactly once. No server idempotency keys: do not add an envelope/key, do not automatically retry. On outcome_unknown reconcile contacts/invoices/documents/journal before further action. Native issuance supports DKK/25% VAT."),schema,false));
    }
    tools.extend(crate::bank_payables::tools());
    tools.extend(crate::periods::tools());
    tools.extend(crate::receivables::tools());
    tools.extend(crate::commitments::tools());
    tools.extend(crate::annual_reports::tools());
    tools.extend(crate::reviewed_bookkeeping::tools());
    json!({"tools":tools})
}
pub(crate) fn native_body(schema: Value) -> Value {
    json!({"type":"string","minLength":2,"maxLength":262144,"contentMediaType":"application/json","contentSchema":schema,"description":"Exact UTF-8 native JSON body string, forwarded byte-for-byte; shape is specified by contentSchema. No defaults, confirmations or keys are injected."})
}
pub(crate) fn company_schema() -> Value {
    json!({"type":"string","minLength":1,"pattern":"^[A-Za-z0-9_-]+$","description":"Explicit company slug from companies"})
}
pub(crate) fn tool(name: &str, description: &str, schema: Value, read_only: bool) -> Value {
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DomainReadArgs {
    company: String,
    year: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewArgs {
    company: String,
    body: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DomainWriteArgs {
    company: String,
    body: String,
    confirm: bool,
    id: Option<i64>,
    #[serde(rename = "previewReviewed")]
    preview_reviewed: Option<bool>,
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
        name if crate::reviewed_bookkeeping::accepts(name) => crate::reviewed_bookkeeping::call(api,name,args).await?,
        "annual_report" => crate::annual_reports::call(api, args).await?,
        name if crate::receivables::accepts(name) => {
            crate::receivables::call(api, name, args).await?
        }
        name if crate::commitments::accepts(name) => {
            crate::commitments::call(api, name, args).await?
        }
        name if crate::periods::accepts(name) => crate::periods::call(api, name, args).await?,
        name if crate::bank_payables::accepts(name) => {
            crate::bank_payables::call(api, name, args).await?
        }
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
        "accounts" | "contacts" | "documents" | "invoices" => {
            if name != "invoices" && args.get("year").is_some() {
                return Err("year allowed only on invoices");
            }
            let args: DomainReadArgs =
                serde_json::from_value(args).map_err(|_| "invalid domain read arguments")?;
            if args.year.as_ref().is_some_and(|s| s.len() > 128) {
                return Err("year selector too long");
            }
            match name {
                "accounts" => api.accounts(&args.company).await,
                "contacts" => api.contacts(&args.company).await,
                "documents" => api.documents(&args.company).await,
                _ => api.invoices(&args.company, args.year.as_deref()).await,
            }
        }
        "preview_invoice" => {
            let args: PreviewArgs =
                serde_json::from_value(args).map_err(|_| "invalid preview arguments")?;
            api.invoice_preview(&args.company, args.body.into_bytes())
                .await
        }
        "create_customer" | "update_customer" | "create_vendor" | "update_vendor"
        | "delete_customer" | "delete_vendor" | "issue_invoice" | "post_invoice" => {
            let update = matches!(
                name,
                "update_customer" | "update_vendor" | "delete_customer" | "delete_vendor"
            );
            if args.get("id").is_some() != update
                || args
                    .get("id")
                    .is_some_and(|v| v.as_i64().is_none_or(|n| n <= 0))
                || args.get("previewReviewed").is_some() != (name == "issue_invoice")
            {
                return Err(
                    "updates/deletions require positive id; issue_invoice requires previewReviewed:true",
                );
            }
            let args: DomainWriteArgs =
                serde_json::from_value(args).map_err(|_| "invalid domain mutation arguments")?;
            if !args.confirm || (name == "issue_invoice" && args.preview_reviewed != Some(true)) {
                return Err("explicit confirmation and reviewed invoice preview required");
            }
            let operation = match name {
                "create_customer" => DomainMutation::CreateCustomer,
                "update_customer" => DomainMutation::UpdateCustomer(args.id.unwrap()),
                "create_vendor" => DomainMutation::CreateVendor,
                "update_vendor" => DomainMutation::UpdateVendor(args.id.unwrap()),
                "delete_customer" => DomainMutation::DeleteCustomer(args.id.unwrap()),
                "delete_vendor" => DomainMutation::DeleteVendor(args.id.unwrap()),
                "issue_invoice" => DomainMutation::IssueInvoice,
                _ => DomainMutation::PostInvoice,
            };
            api.domain_mutation(
                &args.company,
                operation,
                args.body.into_bytes(),
                Confirmation {
                    confirm: args.confirm,
                    invoice_preview_reviewed: args.preview_reviewed.unwrap_or(false),
                },
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
