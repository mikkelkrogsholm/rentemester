use super::*;
use rentemester_application::exceptions::Action;
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    let company_slug = request
        .uri()
        .path()
        .split('/')
        .nth(3)
        .unwrap_or("")
        .to_owned();
    let mutation = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::NOT_FOUND, "company_not_found");
    }
    let (status, include_archived) = match selectors(request.uri().query(), mutation) {
        Ok(selectors) => selectors,
        Err(()) => return refused(StatusCode::BAD_REQUEST, "invalid_query"),
    };
    let id = if mutation {
        request
            .uri()
            .path()
            .rsplit('/')
            .nth(1)
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|id| *id > 0 && *id <= 9_007_199_254_740_991)
    } else {
        None
    };
    let input = if mutation {
        if id.is_none()
            || request.uri().query().is_some()
            || request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let Ok(input) = serde_json::from_slice::<rentemester_storage::exceptions::Resolve>(&bytes)
        else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        Some(input)
    } else {
        None
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut owner = state.native.lock_company(&company_slug).map_err(|_| ())?;
        let action = match &input {
            Some(input) => Action::Resolve(id.unwrap(), input),
            None => Action::ListWithArchived(status.as_deref().unwrap_or("open"), include_archived),
        };
        owner
            .exceptions(&cookie, &SystemClock, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(attempt)) => match super::documents::outcome(attempt) {
            Ok(value) => response(
                StatusCode::OK,
                json!({"ok":true,(if mutation{"exception"}else{"exceptions"}):value}),
            ),
            Err(r) => *r,
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unconfirmed"),
    }
}

fn selectors(query: Option<&str>, mutation: bool) -> Result<(Option<String>, bool), ()> {
    if mutation && query.is_some() {
        return Err(());
    }
    let query = query.unwrap_or("");
    if query.len() > 256
        || query.bytes().enumerate().any(|(i, b)| {
            b == b'%'
                && (i + 2 >= query.len()
                    || !query.as_bytes()[i + 1].is_ascii_hexdigit()
                    || !query.as_bytes()[i + 2].is_ascii_hexdigit())
        })
    {
        return Err(());
    }
    let mut status = None;
    let mut include_archived = None;
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        match key.as_ref() {
            "status"
                if status.is_none() && ["open", "resolved", "all"].contains(&value.as_ref()) =>
            {
                status = Some(value.into_owned());
            }
            "includeArchived" if include_archived.is_none() => {
                include_archived = Some(match value.as_ref() {
                    "true" => true,
                    "false" => false,
                    _ => return Err(()),
                });
            }
            _ => return Err(()),
        }
    }
    Ok((status, include_archived.unwrap_or(false)))
}
#[cfg(test)]
mod tests {
    use super::selectors;
    #[test]
    fn exact_unique_read_selectors_and_no_mutation_query() {
        assert_eq!(selectors(None, false), Ok((None, false)));
        assert_eq!(
            selectors(Some("status=all&includeArchived=true"), false),
            Ok((Some("all".into()), true))
        );
        assert_eq!(
            selectors(Some("includeArchived=false&status=resolved"), false),
            Ok((Some("resolved".into()), false))
        );
        for query in [
            "status=all&status=open",
            "includeArchived=true&includeArchived=false",
            "includeArchived=TRUE",
            "includeArchived=1",
            "includeArchived=",
            "includeArchived",
            "status=ignored",
            "actor=user:x",
            "includeArchived=true&unknown=true",
            "status=%",
            "status=open%GG",
            "includeArchived=%FF",
        ] {
            assert!(selectors(Some(query), false).is_err(), "{query}");
        }
        assert!(selectors(Some(&"x".repeat(257)), false).is_err());
        assert!(selectors(Some(""), true).is_err());
        assert!(selectors(Some("includeArchived=true"), true).is_err());
        assert_eq!(selectors(None, true), Ok((None, false)));
    }
}
