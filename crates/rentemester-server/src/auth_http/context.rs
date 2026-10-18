//! Human-only workspace display context; optional native handler owns /api/me.
use super::*;
use rentemester_application::workspace_context::WorkspaceContext;
pub type WorkspaceContextHandler =
    Box<dyn Fn(VerifiedCookie) -> Result<WorkspaceContext, ContinuationFailure> + Send + Sync>;
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
async fn current(State(state): State<Arc<StateData>>, request: Request) -> Response {
    if auth(&state).workspace_context.is_none() {
        return refused(StatusCode::NOT_FOUND);
    }
    if let Err(s) = gates(&request, &state, Operation::WorkspaceContext) {
        return refused(s);
    }
    if request.headers().contains_key("content-encoding") {
        return refused(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    let proof = match auth(&state)
        .cookies
        .verify_optional(one(request.headers(), "cookie"), None)
    {
        Ok(Some(p)) => p,
        _ => return denied(),
    };
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
        (auth(&state)
            .workspace_context
            .as_ref()
            .expect("registered context"))(proof)
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
