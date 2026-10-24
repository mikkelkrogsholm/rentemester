//! Work-Issue: #717. Original flat public-invoice grammar; shared SDK/server authority only.
use rentemester_client::{ApiClient, Error, OriginalPublicInvoiceKind as Kind};
use serde_json::{Value, json};

pub fn accepts(name: &str) -> bool {
    matches!(name, "peppol_submit_public_invoice" | "efaktura_send")
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
pub fn tools() -> Vec<Value> {
    ["peppol_submit_public_invoice", "efaktura_send"].into_iter().map(|name| {
        let peppol = name == "peppol_submit_public_invoice";
        let text = json!({"type":"string","minLength":1});
        let mut fields = json!({"company":text,"documentId":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"invoiceNumber":{"type":"string"},"confirm":{"type":"boolean"}});
        let required = if peppol {
            fields["accessPoint"] = object(json!({"accessPointId":text,"endpointUrl":text,"senderEndpointId":text}), &["accessPointId","endpointUrl","senderEndpointId"]);
            fields["acknowledgement"] = object(json!({"transmissionId":text,"acknowledgedAt":text}), &["transmissionId","acknowledgedAt"]);
            vec!["company","accessPoint"]
        } else {
            fields["invoiceNumber"] = text.clone();
            fields["digisenseCompanyKey"] = text;
            fields["accessPoint"] = json!({"type":["null","boolean","object","array","number","string"],"description":"Any present value is business-forbidden, including null."});
            vec!["company"]
        };
        let common = "Original flat write; confirm:true required. Limits: 1MiB MCP frame, 16KiB native body, 16MiB response. Explicit company slug only: no filesystem selectors/local DB fallback or automatic actor. Authority comes only from current human cookie, fresh MFA and CompanyExternalSend; Bearer/service credentials refused. Both selectors preserved, documentId takes precedence without fallback. ok:true with status prepared or uncertain does not necessarily mean delivered. No automatic retry, status loop or readback; after unknown reconcile existing public_invoice_submissions before another deliberate action. Canonical native evidence is retained, not claimed byte-identical legacy output.";
        let specific = if peppol {
            " PEPPOL endpointUrl is metadata only: NO FETCH, credential transport or automatic delivery. Caller acknowledgement is an attestation, not a provider receipt. Natural dedup excludes URL/acknowledgement; replay returns the original submission."
        } else {
            " Digisense uses existing operator configuration/registration only; supplied digisenseCompanyKey is an equality assertion, never configuration. accessPoint is forbidden. Queued submission may make one status observation; terminal/uncertain/history NEVER cause redelivery. efaktura_status and secret configuration are outside this package and remain issue717 scope."
        };
        let mut tool = crate::tools::tool(name,&format!("{common}{specific}"),object(fields,&required),false);
        tool["annotations"] = json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":!peppol});
        tool["outputSchema"] = object(json!({"ok":{"type":"boolean"},"errors":{"type":"array","items":{"type":"string"}},"data":{"type":"object"},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}}),&["ok","errors"]);
        tool
    }).collect()
}
fn result(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":value["ok"]!=true})
}
fn refusal(code: &str, message: impl ToString) -> Value {
    result(json!({"ok":false,"code":code,"errors":[message.to_string()]}))
}
fn exact_strings(value: &Value, keys: &[&str]) -> bool {
    value.as_object().is_some_and(|o| {
        o.len() == keys.len()
            && keys.iter().all(|k| {
                o.get(*k)
                    .is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()))
            })
    })
}
fn xml_safe(s: &str) -> bool {
    !s.trim().is_empty() && s.len()<=4096 && s.chars().all(|c| matches!(c,'\t'|'\n'|'\r'|'\u{20}'..='\u{d7ff}'|'\u{e000}'..='\u{fffd}'|'\u{10000}'..='\u{10ffff}'))
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let peppol = match name {
        "peppol_submit_public_invoice" => true,
        "efaktura_send" => false,
        _ => return Err("unknown public invoice tool"),
    };
    let mut o = args
        .as_object()
        .ok_or("expected original public invoice object")?
        .clone();
    let allowed: &[&str] = if peppol {
        &[
            "company",
            "documentId",
            "invoiceNumber",
            "accessPoint",
            "acknowledgement",
            "confirm",
        ]
    } else {
        &[
            "company",
            "documentId",
            "invoiceNumber",
            "digisenseCompanyKey",
            "accessPoint",
            "confirm",
        ]
    };
    if o.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("unknown public invoice field");
    }
    let company = o
        .remove("company")
        .and_then(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned))
        .ok_or("company required")?;
    if let Some(id) = o.get("documentId") {
        let n = id
            .as_f64()
            .filter(|n| {
                n.is_finite() && n.fract() == 0.0 && *n >= 1.0 && *n <= 9_007_199_254_740_991.0
            })
            .ok_or("documentId must be a positive safe integer")?;
        o.insert("documentId".into(), json!(n as u64));
    }
    if o.get("invoiceNumber")
        .is_some_and(|v| !v.as_str().is_some_and(|s| peppol || !s.is_empty()))
    {
        return Err("invoiceNumber must be a string");
    }
    if o.get("confirm").is_some_and(|v| !v.is_boolean()) {
        return Err("confirm must be boolean");
    }
    if peppol {
        if !o.get("accessPoint").is_some_and(|v| {
            exact_strings(v, &["accessPointId", "endpointUrl", "senderEndpointId"])
        }) {
            return Err("accessPoint must contain exactly three nonempty strings");
        }
        if o.get("acknowledgement")
            .is_some_and(|v| !exact_strings(v, &["transmissionId", "acknowledgedAt"]))
        {
            return Err("acknowledgement must contain exactly two nonempty strings");
        }
    } else if o
        .get("digisenseCompanyKey")
        .is_some_and(|v| !v.as_str().is_some_and(|s| !s.is_empty()))
    {
        return Err("digisenseCompanyKey must be nonempty string");
    }
    if o.get("confirm") != Some(&json!(true)) {
        return Ok(refusal(
            "CONFIRM_REQUIRED",
            format!("confirm: true required for write tool {name}"),
        ));
    }
    if !peppol && o.contains_key("accessPoint") {
        return Ok(result(
            json!({"ok":false,"errors":["accessPoint is not allowed; Digisense identity is derived from companyKey"]}),
        ));
    }
    if let Some(key) = o.remove("digisenseCompanyKey") {
        o.insert("companyKey".into(), key);
    }
    // These are native business boundaries, not restrictions on the original grammar.
    if !company
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || o.get("invoiceNumber")
            .is_some_and(|v| !v.as_str().is_some_and(|s| s.len() <= 256 && xml_safe(s)))
        || o.get("companyKey").is_some_and(|v| {
            !v.as_str()
                .is_some_and(|s| s.len() <= 512 && !s.trim().is_empty())
        })
        || ["accessPoint", "acknowledgement"].iter().any(|k| {
            o.get(*k)
                .and_then(Value::as_object)
                .is_some_and(|m| m.values().any(|v| !xml_safe(v.as_str().unwrap())))
        })
    {
        return Ok(refusal(
            "invalid_input",
            "native public invoice identity/metadata bounds refused",
        ));
    }
    let bytes = serde_json::to_vec(&o).map_err(|_| "payload unavailable")?;
    if bytes.len() > 16 * 1024 {
        return Ok(refusal(
            "invalid_input",
            "native public invoice body exceeds 16KiB",
        ));
    }
    let kind = if peppol {
        Kind::SubmitPublicPeppol
    } else {
        Kind::TransmitDigisense
    };
    Ok(
        match api.invoice_public_original(&company, kind, bytes).await {
            Ok(mut core) => {
                let fields = core.as_object_mut().unwrap();
                let ok = fields.remove("ok").unwrap();
                let errors = fields.remove("errors").unwrap();
                let rules = fields.remove("appliedRules");
                let mut envelope = json!({"ok":ok,"errors":errors});
                if let Some(rules) = rules.filter(|v| {
                    v.as_array()
                        .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
                }) {
                    envelope["appliedRules"] = rules;
                }
                if ok == true {
                    envelope["data"] = core;
                } else if let Some(code) = core.get("code").filter(|v| v.is_string()) {
                    envelope["code"] = code.clone();
                }
                result(envelope)
            }
            Err(e) => {
                let code = match e {
                    Error::InvalidInput(_) => "invalid_input",
                    Error::Refused { .. } => "server_refused",
                    Error::Transport => "transport",
                    Error::Protocol => "protocol",
                    Error::OutcomeUnknown => "outcome_unknown",
                };
                refusal(code, e)
            }
        },
    )
}
