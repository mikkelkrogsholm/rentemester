use super::*;
pub(crate) async fn handle(State(state): State<Product>, request: Request) -> Response {
    if !request
        .uri()
        .path()
        .ends_with("original-efaktura-konfigurer")
    {
        return family(State(state), request).await;
    }
    let cookie = match proof(&state, &request, true) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || !one(request.headers(), "content-type")
            .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
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
    let Ok(bytes) = to_bytes(request.into_body(), 16384).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let Ok(body) = serde_json::from_slice::<Value>(&bytes) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    if !body.as_object().is_some_and(|o| {
        o.keys()
            .all(|k| matches!(k.as_str(), "apiLicenseKey" | "environment" | "confirm"))
    }) || body
        .get("environment")
        .is_some_and(|v| v != "test" && v != "production")
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let Ok(mut owner) = state.native.lock_company(&slug) else {
            return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
        };
        match owner.original717_efaktura_konfigurer(&cookie, &SystemClock, &body) {
            Ok(a) => match super::super::bilagsmail::original717::outcome(a) {
                Ok(v) => response(StatusCode::OK, json!({"ok":true,"result":v})),
                Err(r) => r,
            },
            Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
        }
    })
    .await
    {
        Ok(r) => r,
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
    }
}

pub(crate) fn secret(
    state: &Product,
    slug: &str,
    cookie: &rentemester_runtime::auth::VerifiedCookie,
) -> Result<Option<(Vec<u8>, String)>, Box<Response>> {
    let mut owner = state.native.lock_company(slug).map_err(|_| {
        Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "owner_unavailable",
        ))
    })?;
    let a = owner
        .original717_digisense_secret(cookie, &SystemClock)
        .map_err(|_| {
            Box::new(refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "owner_unavailable",
            ))
        })?;
    if a.retired {
        return Err(Box::new(refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "outcome_unknown",
        )));
    }
    match a.outcome {
        Ok(c) if c.control_reusable => Ok(c.value),
        _ => Err(Box::new(refused(
            StatusCode::FORBIDDEN,
            "public_einvoice_refused",
        ))),
    }
}

async fn family(State(state): State<Product>, request: Request) -> Response {
    let read = request
        .uri()
        .path()
        .ends_with("/original-efaktura-onboarding-status");
    if request.method()
        != if read {
            axum::http::Method::GET
        } else {
            axum::http::Method::POST
        }
    {
        return refused(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
    }
    let cookie = match proof(&state, &request, !read) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    if request.uri().query().is_some() || request.headers().contains_key("content-encoding") {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let path = request.uri().path().to_owned();
    let slug = path.split('/').nth(3).unwrap_or("").to_owned();
    let onboard = path.ends_with("/original-efaktura-onboard");
    if !read && !onboard && !path.ends_with("/original-efaktura-registrer") {
        return refused(StatusCode::NOT_FOUND, "not_found");
    }
    if !read
        && !one(request.headers(), "content-type")
            .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(bytes) = to_bytes(request.into_body(), 16384).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let body = if read {
        if !bytes.is_empty() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        json!({})
    } else {
        let Ok(body) = serde_json::from_slice::<Value>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if !valid_family(&body, onboard) {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        if body["confirm"] != true {
            return response(
                StatusCode::OK,
                json!({"ok":true,"result":{"ok":false,"errors":["confirm: true is required"],"code":"CONFIRM_REQUIRED"}}),
            );
        }
        body
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    match tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let fallback = state
            .native
            .public_einvoice
            .get(&slug)
            .map(super::environment);
        if read {
            let Ok(mut owner) = state.native.lock_company(&slug) else {
                return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
            };
            return match owner.original717_onboarding_status(&cookie, &SystemClock, fallback) {
                Ok(a) => match super::super::bilagsmail::original717::outcome(a) {
                    Ok(v) => response(StatusCode::OK, json!({"ok":true,"result":v})),
                    Err(r) => r,
                },
                Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable"),
            };
        }
        let network = if body["network"] == "peppol" {
            provider::Network::Peppol
        } else {
            provider::Network::Nemhandel
        };
        let intent = if onboard {
            app::Intent::Onboard(network)
        } else {
            app::Intent::OriginalRegister(body, network)
        };
        let value = match super::drive(&state, &slug, &cookie, intent) {
            Ok(v) => v,
            Err(r) => return *r,
        };
        if value["ok"] == false {
            return response(StatusCode::OK, json!({"ok":true,"result":value}));
        }
        let saved = match secret(&state, &slug, &cookie) {
            Ok(s) => s,
            Err(r) => return *r,
        };
        let env = match saved
            .as_ref()
            .map(|s| provider::original717::client_from_admitted_secret(&s.0))
            .transpose()
        {
            Ok(c) => c.as_ref().map(super::environment).or(fallback),
            Err(_) => None,
        };
        let Some(env) = env else {
            return refused(
                StatusCode::UNPROCESSABLE_ENTITY,
                "public_einvoice_config_required",
            );
        };
        let Ok(mut owner) = state.native.lock_company(&slug) else {
            return refused(StatusCode::SERVICE_UNAVAILABLE, "owner_unavailable");
        };
        let result =
            match owner.original717_registration_result(&cookie, &SystemClock, env, network) {
                Ok(a) => match super::super::bilagsmail::original717::outcome(a) {
                    Ok(v) => v,
                    Err(r) => return r,
                },
                Err(_) => return refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
            };
        let core = if onboard {
            let status = match owner.original717_onboarding_status(&cookie, &SystemClock, fallback)
            {
                Ok(a) => match super::super::bilagsmail::original717::outcome(a) {
                    Ok(v) => v,
                    Err(r) => return r,
                },
                Err(_) => return refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
            };
            json!({"ok":true,"companyKey":result["companyKey"],"status":status})
        } else {
            result
        };
        response(StatusCode::OK, json!({"ok":true,"result":core}))
    })
    .await
    {
        Ok(r) => r,
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
    }
}
fn valid_family(v: &Value, onboard: bool) -> bool {
    v.as_object().is_some_and(|o| {
        o.keys().all(|k| {
            if onboard {
                k == "confirm"
            } else {
                matches!(k.as_str(), "confirm" | "cvr" | "companyName" | "network")
            }
        }) && o.get("confirm").is_none_or(Value::is_boolean)
            && (onboard
                || ["cvr", "companyName"]
                    .iter()
                    .all(|k| v[*k].as_str().is_some_and(|s| !s.is_empty())))
            && o.get("network")
                .is_none_or(|v| v == "nemhandel" || v == "peppol")
    })
}
#[cfg(test)]
mod original717_provider_tests {
    use super::*;
    #[test]
    fn original717_digisense_registration_source_shape() {
        assert!(valid_family(
            &json!({"cvr":"DK12345678","companyName":"Synthetic"}),
            false
        ));
        assert!(valid_family(&json!({"confirm":false}), true));
        assert!(!valid_family(&json!({"confirm":"true"}), true));
        assert!(!valid_family(
            &json!({"confirm":true,"companyKey":"override"}),
            true
        ));
    }
}
