//! Thin explicit inbound adapter. All durable intake and authorization stay in the shared core.
use super::*;
use rentemester_host::digisense_received::{Admission, ReceiveAttempt};
use rentemester_runtime::{auth::VerifiedCookie, public_einvoice as provider};
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IngestOptions {
    #[serde(default)]
    force_duplicate_logical_identity: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    confirm: bool,
    company_key: Option<String>,
    limit: Option<u32>,
    max_timestamp: Option<String>,
    metadata: Option<Value>,
    ingest_options: Option<IngestOptions>,
    max_pages: Option<u32>,
}
fn valid_metadata(v: &Value) -> bool {
    let Some(fields) = v.as_object() else {
        return false;
    };
    fields.iter().all(|(k, v)| match k.as_str() {
        "source"
        | "issueDate"
        | "invoiceNo"
        | "deliveryDescription"
        | "currency"
        | "paymentDetails"
        | "accountingRationale" => v.as_str().is_some_and(|s| s.len() <= 4096),
        "documentType" => v == "purchase_sale",
        "amountIncVat" | "vatAmount" => v.is_number(),
        "reverseChargeWordingConfirmed" | "danishSimplifiedPurchaseInvoice" => v.is_boolean(),
        "exemptionCode" => v.is_null() || v == "FOREIGN_PHYSICAL_ONLY",
        "sourceBankTransactionId" => v
            .as_i64()
            .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n)),
        "sender" | "recipient" => v.as_object().is_some_and(|o| {
            o.iter().all(|(k, v)| {
                matches!(
                    k.as_str(),
                    "name" | "address" | "vatOrCvr" | "countryCode" | "identifierKind"
                ) && v.as_str().is_some_and(|s| s.len() <= 4096)
            })
        }),
        "purchaseVatLines" => v.as_array().is_some_and(|a| {
            !a.is_empty()
                && a.len() <= 100
                && a.iter().all(|v| {
                    v.as_object().is_some_and(|o| {
                        o.keys().all(|k| {
                            matches!(k.as_str(), "classification" | "netAmount" | "vatAmount")
                        }) && matches!(
                            v["classification"].as_str(),
                            Some("dk_purchase_25" | "exempt")
                        ) && v["netAmount"].is_number()
                            && o.get("vatAmount").is_none_or(Value::is_number)
                    })
                })
        }),
        _ => false,
    })
}
fn valid(input: &Input) -> bool {
    input.confirm
        && (1..=100).contains(&input.limit.unwrap_or(100))
        && (1..=100).contains(&input.max_pages.unwrap_or(100))
        && [&input.company_key, &input.max_timestamp]
            .into_iter()
            .zip([512, 128])
            .all(|(v, max)| {
                v.as_ref().is_none_or(|s| {
                    !s.is_empty()
                        && s.trim() == s
                        && s.len() <= max
                        && !s.chars().any(char::is_control)
                })
            })
        && input.metadata.as_ref().is_none_or(valid_metadata)
}
fn environment(client: &provider::Client) -> &'static str {
    match client.environment() {
        provider::Environment::Test => "test",
        provider::Environment::Production => "production",
    }
}
fn unwrap_phase<T>(a: ReceiveAttempt<T>) -> Result<T, &'static str> {
    let a = a.map_err(|_| "owner_unavailable")?;
    if a.retired {
        return Err("outcome_unknown");
    }
    match a.outcome {
        Ok(c) if c.control_reusable => Ok(c.value),
        Ok(_) => Err("outcome_unknown"),
        Err(f) if f.stage == FailureStage::OutcomeUnknown => Err("outcome_unknown"),
        Err(_) => Err("receive_authority_unavailable"),
    }
}
fn prepare(
    state: &Product,
    slug: &str,
    cookie: &VerifiedCookie,
    assertion: Option<&str>,
    client: &provider::Client,
) -> Result<Admission, &'static str> {
    let mut owner = state
        .native
        .lock_company(slug)
        .map_err(|_| "owner_unavailable")?;
    unwrap_phase(owner.prepare_digisense_received(
        cookie,
        &SystemClock,
        environment(client),
        client.credential_generation(),
        assertion,
    ))
}
fn batch_error(result: &mut Value, code: &str) {
    result["ok"] = json!(false);
    result["errors"].as_array_mut().unwrap().push(json!(code));
}
fn add(result: &mut Value, document: Value) {
    let field = match document["status"].as_str() {
        Some("ingested") => Some("documentsIngested"),
        Some("skipped-duplicate" | "quarantined") => Some("documentsSkipped"),
        _ => None,
    };
    if let Some(field) = field {
        result[field] = json!(result[field].as_u64().unwrap() + 1);
    }
    if document["status"] == "quarantined" {
        result["documentsQuarantined"] =
            json!(result["documentsQuarantined"].as_u64().unwrap() + 1);
    }
    if let Some(errors) = document["errors"].as_array() {
        result["errors"]
            .as_array_mut()
            .unwrap()
            .extend(errors.iter().cloned());
    }
    result["documents"].as_array_mut().unwrap().push(document);
}
fn poll(state: &Product, slug: &str, cookie: &VerifiedCookie, input: &Input) -> Value {
    let mut result = json!({"ok":true,"pagesFetched":0,"documentsListed":0,"documentsIngested":0,"documentsSkipped":0,"documentsQuarantined":0,"documents":[],"errors":[]});
    let run = (|| -> Result<(), &'static str> {
        let saved = super::public_einvoice::original717::secret(state, slug, cookie)
            .map_err(|_| "receive_authority_unavailable")?;
        let owned = saved
            .as_ref()
            .map(|bytes| provider::original717::client_from_admitted_secret(&bytes.0))
            .transpose()
            .map_err(|_| "public_einvoice_config_required")?;
        let client = owned
            .as_ref()
            .or_else(|| state.native.public_einvoice.get(slug))
            .ok_or("public_einvoice_config_required")?;
        let current = || -> Result<(), &'static str> {
            if super::public_einvoice::original717::secret(state, slug, cookie)
                .map_err(|_| "receive_authority_unavailable")?
                != saved
            {
                return Err("receive_authority_unavailable");
            }
            Ok(())
        };

        let mut admission;
        let mut offset = 0;
        let limit = input.limit.unwrap_or(100);
        // Even transient rows repeated within this invocation are not downloaded again.
        let mut seen = std::collections::HashSet::new();
        for _ in 0..input.max_pages.unwrap_or(100) {
            // Fresh current authorization precedes every listing, including empty pages.
            current()?;
            admission = prepare(state, slug, cookie, input.company_key.as_deref(), client)?;
            let page = client
                .list_received_documents(
                    admission.company_key(),
                    limit,
                    offset,
                    input.max_timestamp.as_deref(),
                )
                .map_err(|_| "receive_list_failed")?;
            result["pagesFetched"] = json!(result["pagesFetched"].as_u64().unwrap() + 1);
            let count = page.documents.len();
            result["documentsListed"] =
                json!(result["documentsListed"].as_u64().unwrap() + count as u64);
            for record in page.documents {
                let id = record.internal_id().to_owned();
                let checked = {
                    current()?;
                    let mut owner = state
                        .native
                        .lock_company(slug)
                        .map_err(|_| "owner_unavailable")?;
                    unwrap_phase(owner.check_digisense_received(
                        cookie,
                        &SystemClock,
                        admission,
                        environment(client),
                        client.credential_generation(),
                        record.metadata(),
                    ))?
                };
                admission = checked.admission;
                if checked.known {
                    add(
                        &mut result,
                        json!({"internalId":id,"status":"skipped-duplicate"}),
                    );
                    continue;
                }
                if !seen.insert(id.clone()) {
                    // Drop the record-bound admission; no attempt to reuse it for another ID.
                    current()?;
                    admission = prepare(state, slug, cookie, input.company_key.as_deref(), client)?;
                    add(
                        &mut result,
                        json!({"internalId":id,"status":"skipped-duplicate"}),
                    );
                    continue;
                }
                let xml = match client.download_received_document(&record) {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        drop(admission);
                        current()?;
                        admission =
                            prepare(state, slug, cookie, input.company_key.as_deref(), client)?;
                        add(
                            &mut result,
                            json!({"internalId":id,"status":"error","errors":["receive_download_failed"]}),
                        );
                        continue;
                    }
                };
                let finished = {
                    current()?;
                    let mut owner = state
                        .native
                        .lock_company(slug)
                        .map_err(|_| "owner_unavailable")?;
                    unwrap_phase(
                        owner.finish_digisense_received(
                            cookie,
                            &SystemClock,
                            admission,
                            environment(client),
                            client.credential_generation(),
                            xml,
                            input.metadata.clone().unwrap_or_else(|| json!({})),
                            input
                                .ingest_options
                                .as_ref()
                                .is_some_and(|o| o.force_duplicate_logical_identity),
                        ),
                    )?
                };
                admission = finished.admission;
                add(&mut result, finished.document);
            }
            if !page.has_next || count == 0 || count < limit as usize {
                break;
            }
            offset += count as u32;
        }
        Ok(())
    })();
    if let Err(code) = run {
        batch_error(&mut result, code);
    }
    result
}
fn workspace(state: &Product, cookie: &VerifiedCookie, input: &Input) -> Response {
    let catalog = {
        let Ok(mut owner) = state.native.owner.lock() else {
            return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
        };
        let Ok(a) = owner.digisense_received_catalog(&state.native.cookies, cookie, &SystemClock)
        else {
            return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
        };
        if a.retired {
            return refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown");
        }
        match a.outcome {
            Ok(v) => v,
            Err(_) => return refused(StatusCode::FORBIDDEN, "workspace_access_denied"),
        }
    };
    let Some(entries) = catalog["companies"].as_array() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "catalog_unavailable");
    };
    // Complete every active initialized authority+backup preflight before ANY network/effect.
    for entry in entries
        .iter()
        .filter(|e| e["archived"] != true && e["initialized"] == true)
    {
        let Some(slug) = entry["slug"].as_str() else {
            return refused(StatusCode::SERVICE_UNAVAILABLE, "catalog_unavailable");
        };
        let Ok(mut owner) = state.native.lock_company(slug) else {
            return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
        };
        if unwrap_phase(owner.preflight_digisense_received(cookie, &SystemClock)).is_err() {
            return refused(StatusCode::FORBIDDEN, "workspace_receive_preflight_denied");
        }
    }
    let mut companies = vec![];
    for entry in entries {
        let Some(slug) = entry["slug"].as_str() else {
            return refused(StatusCode::SERVICE_UNAVAILABLE, "catalog_unavailable");
        };
        let reason = if entry["archived"] == true {
            Some("archived")
        } else if entry["initialized"] != true {
            Some("not initialized")
        } else if state.native.public_einvoice.get(slug).is_none() {
            Some("not configured")
        } else {
            None
        };
        if let Some(reason) = reason {
            companies.push(json!({"slug":slug,"status":"skipped","reason":reason}));
            continue;
        }
        let value = poll(state, slug, cookie, input);
        companies.push(if value["ok"]==true {json!({"slug":slug,"status":"polled","documentsIngested":value["documentsIngested"]})} else {json!({"slug":slug,"status":"failed","reason":"receive failed; inspect authoritative documents and dedup before any further action"})});
    }
    response(StatusCode::OK, json!({"ok":true,"companies":companies}))
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    if service_company::requested(&request) {
        return refused(StatusCode::UNAUTHORIZED, "human_cookie_required");
    }
    let cookie = match proof(&state, &request, true) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let is_workspace = request.uri().path() == "/api/workspace/efaktura/modtag";
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    if !is_workspace && !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || !one(request.headers(), "content-type")
            .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    if is_workspace && serde_json::from_slice::<Value>(&bytes).ok() != Some(json!({"confirm":true}))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(input) = serde_json::from_slice::<Input>(&bytes) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    if !valid(&input) {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if is_workspace {
            workspace(&state, &cookie, &input)
        } else {
            response(StatusCode::OK, poll(&state, &slug, &cookie, &input))
        }
    })
    .await
    {
        Ok(r) => r,
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn digisense_received_strict_input_and_counters() {
        for v in [
            json!({"confirm":false}),
            json!({"confirm":true,"limit":0}),
            json!({"confirm":true,"maxPages":101}),
            json!({"confirm":true,"metadata":{"apiKey":"x"}}),
        ] {
            assert!(!valid(&serde_json::from_value::<Input>(v).unwrap()));
        }
        assert!(serde_json::from_value::<Input>(json!({"confirm":true,"apiKey":"x"})).is_err());
        let mut v = json!({"documentsIngested":0,"documentsSkipped":0,"documentsQuarantined":0,"documents":[],"errors":[]});
        add(
            &mut v,
            json!({"internalId":"bad","status":"quarantined","errors":["invalid_xml"]}),
        );
        add(
            &mut v,
            json!({"internalId":"good","status":"ingested","documentNo":"B1"}),
        );
        assert_eq!(v["documentsSkipped"], 1);
        assert_eq!(v["documentsQuarantined"], 1);
        assert_eq!(v["documentsIngested"], 1);
    }
}
