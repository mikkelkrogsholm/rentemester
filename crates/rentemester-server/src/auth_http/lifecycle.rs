//! Optional owner-only native lifecycle routes, never a provider or legacy fallback.
use super::*;
use rentemester_application::service_lifecycle::{Operation as Action, Receipt};
pub struct ServiceLifecycleInput {
    proof: VerifiedCookie,
    operation: Action,
}
impl ServiceLifecycleInput {
    pub fn proof(&self) -> &VerifiedCookie {
        &self.proof
    }
    pub fn operation(&self) -> &Action {
        &self.operation
    }
}
pub type ServiceLifecycleHandler =
    Box<dyn Fn(ServiceLifecycleInput) -> Result<Receipt, ContinuationFailure> + Send + Sync>;
pub(super) fn routes(router: Router<Arc<StateData>>) -> Router<Arc<StateData>> {
    router
        .route(
            "/api/workspace/service-principals",
            get(list).post(create).head(|| async {
                let mut r = refused(StatusCode::METHOD_NOT_ALLOWED);
                r.headers_mut()
                    .insert("allow", HeaderValue::from_static("GET, POST"));
                r
            }),
        )
        .route("/api/workspace/service-principals/rotate", post(rotate))
        .route("/api/workspace/service-principals/revoke", post(revoke))
        .route("/api/workspace/service-principals/recover", post(recover))
}
async fn list(s: State<Arc<StateData>>, r: Request) -> Response {
    execute(s, r, 0).await
}
async fn create(s: State<Arc<StateData>>, r: Request) -> Response {
    execute(s, r, 1).await
}
async fn rotate(s: State<Arc<StateData>>, r: Request) -> Response {
    execute(s, r, 2).await
}
async fn revoke(s: State<Arc<StateData>>, r: Request) -> Response {
    execute(s, r, 3).await
}
async fn recover(s: State<Arc<StateData>>, r: Request) -> Response {
    execute(s, r, 4).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Wire {
    confirm: bool,
    #[serde(default, deserialize_with = "string")]
    display_name: Option<String>,
    #[serde(default, deserialize_with = "string")]
    service_account_id: Option<String>,
    #[serde(default, deserialize_with = "string")]
    credential_id: Option<String>,
    #[serde(default, deserialize_with = "string")]
    operation_id: Option<String>,
    #[serde(default, deserialize_with = "number")]
    expires_in_seconds: Option<u64>,
}
fn string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
fn number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    u64::deserialize(d).map(Some)
}
fn action(w: Wire, kind: u8) -> Option<Action> {
    if !w.confirm
        || w.operation_id
            .as_deref()
            .is_some_and(|v| rentemester_storage::service_lifecycle::uuid(v).is_err())
        || w.expires_in_seconds
            .is_some_and(|s| !(86400..=31536000).contains(&s))
    {
        return None;
    }
    let id = w.operation_id;
    match kind {
        1 if w.service_account_id.is_none() && w.credential_id.is_none() => Some(Action::Create {
            display_name: w.display_name?,
            operation_id: id,
            expires_in_seconds: w.expires_in_seconds,
        }),
        2 if w.display_name.is_none() => Some(Action::Rotate {
            subject: w.service_account_id?,
            credential: w.credential_id?,
            operation_id: id,
            expires_in_seconds: w.expires_in_seconds,
        }),
        3 if w.display_name.is_none() && w.expires_in_seconds.is_none() => Some(Action::Revoke {
            subject: w.service_account_id?,
            credential: w.credential_id?,
            operation_id: id,
        }),
        4 if w.display_name.is_none()
            && w.service_account_id.is_none()
            && w.credential_id.is_none()
            && w.expires_in_seconds.is_none() =>
        {
            Some(Action::Recover { operation_id: id? })
        }
        _ => None,
    }
}
async fn execute(State(state): State<Arc<StateData>>, request: Request, kind: u8) -> Response {
    if auth(&state).service_lifecycle.is_none() {
        return refused(StatusCode::NOT_FOUND);
    }
    if let Err(s) = gates(
        &request,
        &state,
        if kind == 0 {
            Operation::ServiceLifecycleList
        } else {
            Operation::ServiceLifecycle
        },
    ) {
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
    let bytes = match body_bytes(request).await {
        Ok(b) => b,
        Err(s) => return refused(s),
    };
    let operation = if kind == 0 {
        if !bytes.is_empty() {
            return refused(StatusCode::BAD_REQUEST);
        }
        Action::List
    } else {
        if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
            return refused(StatusCode::BAD_REQUEST);
        }
        let w: Wire = match serde_json::from_slice(&bytes) {
            Ok(w) => w,
            Err(_) => return refused(StatusCode::BAD_REQUEST),
        };
        match action(w, kind) {
            Some(a) => a,
            None => return refused(StatusCode::BAD_REQUEST),
        }
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (auth(&state)
            .service_lifecycle
            .as_ref()
            .expect("registered lifecycle"))(ServiceLifecycleInput { proof, operation })
    })
    .await;
    match result {
        Ok(Ok(receipt)) => {
            let Ok(status) = StatusCode::from_u16(receipt.status()) else {
                return uncertain();
            };
            let mut r = response(status, "");
            *r.body_mut() = axum::body::Body::from(receipt.as_json().to_owned());
            r
        }
        Ok(Err(e))
            if e.stage == FailureStage::NotCommitted && e.error.code == "invalid_request" =>
        {
            refused(StatusCode::BAD_REQUEST)
        }
        Ok(Err(e))
            if e.stage == FailureStage::NotCommitted && e.error.code == "operation_reused" =>
        {
            response(StatusCode::CONFLICT, r#"{"code":"OPERATION_REUSED"}"#)
        }
        Ok(Err(e)) => native_failure(e),
        _ => uncertain(),
    }
}
