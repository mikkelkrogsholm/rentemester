//! Canonical parties projected through current admitted company visibility, never raw control data.
use super::*;
use rentemester_application::parties::{self as app, Action};
enum Command {
    List(Option<String>, usize, usize),
    Inspect(String),
    Create(app::Create),
    CreateIdentity(app::CreateIdentity),
    Role(String, app::Role),
    Assertion(String, app::Assertion),
    Alias(String, app::Alias),
    ProposeMerge(String, app::ProposeMerge),
    ApproveMerge(String, app::ApproveMerge),
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    // GET-backed HEAD must refuse before credentials, body parsing or mutation dispatch.
    if request.method() == axum::http::Method::HEAD {
        let mut response = refused(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
        response
            .headers_mut()
            .insert("allow", axum::http::HeaderValue::from_static("GET"));
        return response;
    }
    let path = request.uri().path().to_owned();
    let parts: Vec<_> = path.split('/').collect();
    let slug = parts.get(3).unwrap_or(&"").to_string();
    let id = parts.get(5).map(|s| {
        url::form_urlencoded::parse(format!("id={}", s.replace('+', "%2B")).as_bytes())
            .next()
            .unwrap()
            .1
            .into_owned()
    });
    let mutation = request.method() != axum::http::Method::GET;
    let cookie = match proof(&state, &request, mutation) {
        Ok(p) => p,
        Err(r) => return *r,
    };
    if !selected(&state, &request) {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let command = if mutation {
        if request.uri().query().is_some()
            || request.headers().contains_key("content-encoding")
            || !one(request.headers(), "content-type")
                .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
        {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        let Ok(bytes) = to_bytes(request.into_body(), 256 * 1024).await else {
            return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
        };
        let Ok(mut body) = serde_json::from_slice::<serde_json::Map<String, Value>>(&bytes) else {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        };
        if body.remove("confirm") != Some(json!(true)) {
            return refused(StatusCode::BAD_REQUEST, "confirmation_required");
        }
        let body = Value::Object(body);
        let c = match (id, parts.get(6).copied()) {
            (None, None) => serde_json::from_value(body).map(Command::Create),
            (Some(id), None) if id == "create-identity" => {
                serde_json::from_value(body).map(Command::CreateIdentity)
            }
            (Some(id), Some("role")) => serde_json::from_value(body).map(|b| Command::Role(id, b)),
            (Some(id), Some("assertion")) => {
                serde_json::from_value(body).map(|b| Command::Assertion(id, b))
            }
            (Some(id), Some("alias")) => {
                serde_json::from_value(body).map(|b| Command::Alias(id, b))
            }
            (Some(id), Some("propose-merge")) => {
                serde_json::from_value(body).map(|b| Command::ProposeMerge(id, b))
            }
            (Some(id), Some("approve-merge")) => {
                serde_json::from_value(body).map(|b| Command::ApproveMerge(id, b))
            }
            _ => return refused(StatusCode::NOT_FOUND, "not_found"),
        };
        match c {
            Ok(c) => c,
            Err(_) => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
        }
    } else if let Some(id) = id {
        if request.uri().query().is_some() {
            return refused(StatusCode::BAD_REQUEST, "invalid_input");
        }
        Command::Inspect(id)
    } else {
        let mut query = None;
        let mut cursor = None;
        let mut limit = None;
        for (k, v) in url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes()) {
            match k.as_ref() {
                "query" if query.is_none() && v.encode_utf16().count() <= 512 => {
                    query = Some(v.into_owned())
                }
                "cursor" if cursor.is_none() && v.bytes().all(|b| b.is_ascii_digit()) => {
                    cursor = v.parse::<usize>().ok()
                }
                "limit" if limit.is_none() && v.bytes().all(|b| b.is_ascii_digit()) => {
                    limit = v.parse::<usize>().ok()
                }
                _ => return refused(StatusCode::BAD_REQUEST, "invalid_input"),
            };
            if k == "cursor" && cursor.is_none() || k == "limit" && limit.is_none() {
                return refused(StatusCode::BAD_REQUEST, "invalid_input");
            }
        }
        Command::List(
            query,
            cursor.unwrap_or(0),
            limit.unwrap_or(25).clamp(1, 100),
        )
    };
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let action = match &command {
            Command::List(q, c, l) => Action::List {
                query: q.as_deref(),
                cursor: *c,
                limit: *l,
            },
            Command::Inspect(id) => Action::Inspect(id),
            Command::Create(i) => Action::Create(i),
            Command::CreateIdentity(i) => Action::CreateIdentity(i),
            Command::Role(id, i) => Action::Role(id, i),
            Command::Assertion(id, i) => Action::Assertion(id, i),
            Command::Alias(id, i) => Action::Alias(id, i),
            Command::ProposeMerge(id, i) => Action::ProposeMerge(id, i),
            Command::ApproveMerge(id, i) => Action::ApproveMerge(id, i),
        };
        let mut owner = state.native.lock_company(&slug).map_err(|_| ())?;
        owner
            .parties(&state.native.cookies, &SystemClock, &cookie, action)
            .map_err(|_| ())
    })
    .await;
    match result {
        Ok(Ok(a)) => match a.outcome {
            Ok(mut v) => {
                v["ok"] = json!(true);
                response(StatusCode::OK, v)
            }
            Err(f) => {
                use rentemester_application::password_login::FailureStage;
                let unknown = f.stage == FailureStage::OutcomeUnknown;
                let status = if unknown {
                    StatusCode::SERVICE_UNAVAILABLE
                } else if matches!(f.error.code, "access_denied" | "invalid_credentials") {
                    StatusCode::FORBIDDEN
                } else if f.error.code == "party_not_found" {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::UNPROCESSABLE_ENTITY
                };
                refused(status, if unknown { "internal" } else { f.error.code })
            }
        },
        _ => refused(StatusCode::SERVICE_UNAVAILABLE, "internal"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn party_merge_manifest_has_exact_two_post_routes() {
        let m: Value =
            serde_json::from_str(include_str!("../../../../protocol/http-v1.json")).unwrap();
        let routes: Vec<_> = m["operations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| {
                o["path"]
                    .as_str()
                    .is_some_and(|s| s.contains("workspace-parties") && s.ends_with("-merge"))
            })
            .collect();
        assert_eq!(routes.len(), 2);
        for suffix in ["propose-merge", "approve-merge"] {
            assert!(routes.iter().any(|r| r["method"] == "POST"
                && r["path"]
                    == format!("/api/companies/{{slug}}/workspace-parties/{{id}}/{suffix}")));
        }
    }
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn party_merge_registered_http_gates_refuse_before_effects() {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        struct Synthetic(std::path::PathBuf);
        impl Drop for Synthetic {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let base = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "party-merge-{}",
            rentemester_runtime::auth::new_token().unwrap().as_str()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&base)
            .unwrap();
        let base = Synthetic(base);
        let root = base.0.join("workspace");
        let config = base.0.join("server.json");
        let key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        std::fs::write(&config,json!({"root":root,"brokerUid":std::fs::metadata(&base.0).unwrap().uid(),"runtimeProfile":"local-user-macos","workspace":"synthetic","companySlug":"a","companyName":"Synthetic Merge","cookieKey":key,"mfaKey":key,"accounts":[]}).to_string()).unwrap();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
        let load_origin = origin.clone();
        let backend = tokio::task::spawn_blocking(move || {
            Backend::load(&config, &load_origin, true, true).unwrap()
        })
        .await
        .unwrap();
        let cookie = backend
            .cookies
            .issue_session("synthetic-merge-token")
            .unwrap()
            .set_cookie()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let router = super::super::router(backend.clone(), &origin);
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let db = rusqlite::Connection::open_with_flags(
            root.join(".rentemester/workspace-control.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        for suffix in ["propose-merge", "approve-merge"] {
            let url = format!("{origin}/api/companies/a/workspace-parties/from/{suffix}");
            let valid = if suffix == "propose-merge" {
                json!({"intoPartyId":"into","reviewAssertion":"Synthetic\nreview","confirm":true})
            } else {
                json!({"proposalHash":"a".repeat(64),"confirm":true})
            };
            for method in [
                reqwest::Method::GET,
                reqwest::Method::HEAD,
                reqwest::Method::PUT,
                reqwest::Method::DELETE,
            ] {
                assert_eq!(
                    client.request(method, &url).send().await.unwrap().status(),
                    StatusCode::METHOD_NOT_ALLOWED
                );
            }
            let post = |body: Value| {
                client
                    .post(&url)
                    .header("origin", &origin)
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(body.to_string())
            };
            assert_eq!(
                client
                    .post(&url)
                    .header("origin", &origin)
                    .header("content-type", "application/json")
                    .body(valid.to_string())
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
            assert_eq!(
                client
                    .post(&url)
                    .header("origin", "http://evil.invalid")
                    .header("cookie", &cookie)
                    .body(valid.to_string())
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                post(valid.clone())
                    .header("authorization", "Bearer synthetic")
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
            for confirm in [Value::Null, json!(false), json!("true")] {
                let mut body = valid.clone();
                body["confirm"] = confirm;
                let reply = post(body).send().await.unwrap();
                assert_eq!(reply.status(), StatusCode::BAD_REQUEST);
                assert!(
                    reply
                        .text()
                        .await
                        .unwrap()
                        .contains("confirmation_required")
                );
            }
            for field in [
                "actor",
                "observedAt",
                "principal",
                "companySlug",
                "unexpected",
            ] {
                let mut body = valid.clone();
                body[field] = json!("forged");
                assert_eq!(
                    post(body).send().await.unwrap().status(),
                    StatusCode::BAD_REQUEST
                );
            }
            assert_eq!(
                client
                    .post(format!("{url}?actor=forged"))
                    .header("origin", &origin)
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(valid.to_string())
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
            assert_eq!(
                post(valid)
                    .header("content-encoding", "gzip")
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
            assert_eq!(
                db.query_row("SELECT count(*) FROM rm_party_events", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        server.abort();
        let _ = server.await;
        drop(db);
        tokio::task::spawn_blocking(move || drop(backend))
            .await
            .unwrap();
    }
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn identity_only_registered_http_gates_refuse_before_effects() {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        struct Synthetic(std::path::PathBuf);
        impl Drop for Synthetic {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let base = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "party-merge-{}",
            rentemester_runtime::auth::new_token().unwrap().as_str()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&base)
            .unwrap();
        let base = Synthetic(base);
        let root = base.0.join("workspace");
        let config = base.0.join("server.json");
        let key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        std::fs::write(&config,json!({"root":root,"brokerUid":std::fs::metadata(&base.0).unwrap().uid(),"runtimeProfile":"local-user-macos","workspace":"synthetic","companySlug":"a","companyName":"Synthetic Merge","cookieKey":key,"mfaKey":key,"accounts":[]}).to_string()).unwrap();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
        let load_origin = origin.clone();
        let backend = tokio::task::spawn_blocking(move || {
            Backend::load(&config, &load_origin, true, true).unwrap()
        })
        .await
        .unwrap();
        let cookie = backend
            .cookies
            .issue_session("synthetic-merge-token")
            .unwrap()
            .set_cookie()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let router = super::super::router(backend.clone(), &origin);
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let db = rusqlite::Connection::open_with_flags(
            root.join(".rentemester/workspace-control.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        for suffix in ["create-identity"] {
            let url = format!("{origin}/api/companies/a/workspace-parties/{suffix}");
            let valid = json!({"partyId":"synthetic-identity","kind":"organization","name":"Synthetic identity","source":"synthetic-review","observedAt":"2026-01-01","reviewAssertion":"Synthetic reviewed","confirm":true});
            // Static POST route must preserve GET inspection for the pre-existing valid opaque ID.
            assert_eq!(client.get(&url).send().await.unwrap().status(),StatusCode::UNAUTHORIZED);
            for method in [
                reqwest::Method::HEAD,
                reqwest::Method::PUT,
                reqwest::Method::DELETE,
            ] {
                assert_eq!(
                    client.request(method, &url).send().await.unwrap().status(),
                    StatusCode::METHOD_NOT_ALLOWED
                );
            }
            let post = |body: Value| {
                client
                    .post(&url)
                    .header("origin", &origin)
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(body.to_string())
            };
            assert_eq!(
                client
                    .post(&url)
                    .header("origin", &origin)
                    .header("content-type", "application/json")
                    .body(valid.to_string())
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
            assert_eq!(
                client
                    .post(&url)
                    .header("origin", "http://evil.invalid")
                    .header("cookie", &cookie)
                    .body(valid.to_string())
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                post(valid.clone())
                    .header("authorization", "Bearer synthetic")
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
            for confirm in [Value::Null, json!(false), json!("true")] {
                let mut body = valid.clone();
                body["confirm"] = confirm;
                let reply = post(body).send().await.unwrap();
                assert_eq!(reply.status(), StatusCode::BAD_REQUEST);
                assert!(
                    reply
                        .text()
                        .await
                        .unwrap()
                        .contains("confirmation_required")
                );
            }
            for field in [
                "actor",
                "role",
                "defaults",
                "principal",
                "companySlug",
                "unexpected",
            ] {
                let mut body = valid.clone();
                body[field] = json!("forged");
                assert_eq!(
                    post(body).send().await.unwrap().status(),
                    StatusCode::BAD_REQUEST
                );
            }
            assert_eq!(
                client
                    .post(format!("{url}?actor=forged"))
                    .header("origin", &origin)
                    .header("cookie", &cookie)
                    .header("content-type", "application/json")
                    .body(valid.to_string())
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
            assert_eq!(
                post(valid)
                    .header("content-encoding", "gzip")
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
            assert_eq!(
                db.query_row("SELECT count(*) FROM rm_party_events", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        server.abort();
        let _ = server.await;
        drop(db);
        tokio::task::spawn_blocking(move || drop(backend))
            .await
            .unwrap();
    }
}
