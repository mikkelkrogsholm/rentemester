//! Own workspace display context. Optional native handlers own /api/me without fallback.
use super::*;
use rentemester_application::workspace_context::WorkspaceContext;
use rentemester_runtime::service_key::ServiceKey;
pub type WorkspaceContextHandler =
    Box<dyn Fn(VerifiedCookie) -> Result<WorkspaceContext, ContinuationFailure> + Send + Sync>;
pub type ServiceContextHandler =
    Box<dyn Fn(ServiceKey) -> Result<WorkspaceContext, ContinuationFailure> + Send + Sync>;
pub(super) fn routes(router: Router<Arc<StateData>>) -> Router<Arc<StateData>> {
    router.route(
        "/api/me",
        get(current).head(|| async {
            let mut r = refused(StatusCode::METHOD_NOT_ALLOWED);
            r.headers_mut()
                .insert("allow", HeaderValue::from_static("GET"));
            r
        }),
    )
}
enum Input {
    Human(VerifiedCookie),
    Service(ServiceKey),
}
async fn current(State(state): State<Arc<StateData>>, request: Request) -> Response {
    let service = request.headers().contains_key("x-rentemester-service-key");
    if if service {
        auth(&state).service_context.is_none()
    } else {
        auth(&state).workspace_context.is_none()
    } {
        return refused(StatusCode::NOT_FOUND);
    }
    let input = if service {
        if request.uri().query().is_some() {
            return refused(StatusCode::BAD_REQUEST);
        }
        if let Err(s) = admission_for(&request, &state, Operation::WorkspaceContext) {
            return refused(s);
        }
        let h = request.headers();
        if h.contains_key("authorization") || h.contains_key("cookie") {
            return denied();
        }
        let Some(raw) = one(h, "x-rentemester-service-key")
            .filter(|v| v.len() <= 4096 && !v.chars().any(char::is_control))
        else {
            return denied();
        };
        match ServiceKey::new(auth(&state).cookies.workspace(), raw.trim()) {
            Ok(key) => Input::Service(key),
            Err(_) => return denied(),
        }
    } else {
        if let Err(s) = gates(&request, &state, Operation::WorkspaceContext) {
            return refused(s);
        }
        match auth(&state)
            .cookies
            .verify_optional(one(request.headers(), "cookie"), None)
        {
            Ok(Some(p)) => Input::Human(p),
            _ => return denied(),
        }
    };
    if request.headers().contains_key("content-encoding") {
        return refused(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE);
    };
    match body_bytes(request).await {
        Ok(b) if b.is_empty() => {}
        Ok(_) => return refused(StatusCode::BAD_REQUEST),
        Err(s) => return refused(s),
    }
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        match input {
            Input::Human(proof) => (auth(&state)
                .workspace_context
                .as_ref()
                .expect("registered context"))(proof),
            Input::Service(key) => (auth(&state)
                .service_context
                .as_ref()
                .expect("registered service context"))(key),
        }
    })
    .await;
    match result {
        Ok(Ok(context)) => {
            let mut r = response(StatusCode::OK, "");
            *r.body_mut() = axum::body::Body::from(context.as_json().to_owned());
            r
        }
        Ok(Err(e)) => native_failure(e),
        _ => uncertain(),
    }
}
