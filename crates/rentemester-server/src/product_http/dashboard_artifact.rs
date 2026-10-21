//! Download the shared standalone dashboard; local files and opening belong to the CLI.
use super::*;

fn date(query: Option<&str>, today: &str) -> Result<String, ()> {
    let query = query.unwrap_or("");
    if query.len() > 256 {
        return Err(());
    }
    let mut as_of = None;
    let mut format = false;
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        match key.as_ref() {
            "format" if !format && value == "html" => format = true,
            "asOf"
                if as_of.is_none()
                    && rentemester_engine::fiscal_year::Date::parse(&value).is_ok() =>
            {
                as_of = Some(value.into_owned());
            }
            _ => return Err(()),
        }
    }
    Ok(as_of.unwrap_or_else(|| today.to_owned()))
}

pub(super) async fn read(State(state): State<Product>, request: Request) -> Response {
    let slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let cookie = match proof(&state, &request, false) {
        Ok(cookie) => cookie,
        Err(reply) => return *reply,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let now = chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now());
    // The original dashboard CLI defaults to the UTC date, independently of the
    // Danish date used by other read adapters. Explicit asOf always wins.
    let today = now.format("%Y-%m-%d").to_string();
    let as_of = match date(request.uri().query(), &today) {
        Ok(value) => value,
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
    };
    if request.headers().contains_key("content-encoding")
        || request.headers().contains_key("transfer-encoding")
        || request.headers().contains_key("idempotency-key")
        || to_bytes(request.into_body(), 0).await.is_err()
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let generated_at = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let commit = option_env!("RENTEMESTER_BUILD_REVISION").unwrap_or("unknown");
    let rules = option_env!("RENTEMESTER_RULE_BUNDLE_VERSION").unwrap_or("unknown");
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let export_date = as_of.clone();
    let export_time = generated_at.clone();
    let filename = format!(
        "dashboard-{}-{as_of}.html",
        slug.chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            })
            .collect::<String>()
    );
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        owner
            .dashboard_artifact(
                &cookie,
                &SystemClock,
                rentemester_application::dashboard_artifact::Request {
                    as_of: &export_date,
                    generated_at: &export_time,
                    build_revision: commit,
                    rule_bundle_version: rules,
                },
            )
            .map_err(|_| ())
    })
    .await;
    let export = match result {
        Ok(Ok(attempt)) => match documents::outcome(attempt) {
            Ok(export) => export,
            Err(reply) => return *reply,
        },
        _ => return refused(StatusCode::SERVICE_UNAVAILABLE, "read_unconfirmed"),
    };
    if export.html.len() > 8 * 1024 * 1024 {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "export_too_large");
    }
    let mut reply = (StatusCode::OK, export.html).into_response();
    let render_ms = format!("{:.3}", export.render_ms);
    let disposition = format!("attachment; filename=\"{filename}\"");
    for (key, value) in [
        ("content-type", "text/html; charset=utf-8"),
        ("content-disposition", disposition.as_str()),
        ("cache-control", "private, no-store"),
        ("x-content-type-options", "nosniff"),
        (
            "content-security-policy",
            "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; frame-ancestors 'none'",
        ),
        ("x-rentemester-as-of", as_of.as_str()),
        ("x-rentemester-generated-at", generated_at.as_str()),
        ("x-rentemester-commit-sha", commit),
        ("x-rentemester-rule-bundle-version", rules),
        ("x-rentemester-render-ms", render_ms.as_str()),
    ] {
        let Ok(value) = axum::http::HeaderValue::from_str(value) else {
            return refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "export_metadata_unavailable",
            );
        };
        reply.headers_mut().insert(key, value);
    }
    reply
}

#[cfg(test)]
mod tests {
    use super::date;
    #[test]
    fn selectors_preserve_explicit_or_utc_date_and_refuse_ambiguous_inputs() {
        assert_eq!(date(None, "2026-01-01").unwrap(), "2026-01-01");
        assert_eq!(
            date(Some("asOf=2024-02-29&format=html"), "2026-01-01").unwrap(),
            "2024-02-29"
        );
        for query in [
            "asOf=2025-02-29",
            "format=pdf",
            "format=html&format=html",
            "asOf=2026-01-01&asOf=2026-01-01",
            "confirm=true",
            "out=/tmp/file.html",
            "source=archive",
            "asOf=",
        ] {
            assert!(date(Some(query), "2026-01-01").is_err(), "{query}");
        }
    }
}
