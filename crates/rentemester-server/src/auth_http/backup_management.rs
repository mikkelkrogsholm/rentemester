//! Password-bound disclosure/replacement of the current session owner's existing backup pool.
use super::*;
use rentemester_application::backup_management::Operation as BackupOperation;
use rentemester_runtime::mfa::BackupCodeList;
pub type BackupManagementHandler =
    Box<dyn Fn(BackupManagementInput) -> Result<BackupCodeList, ContinuationFailure> + Send + Sync>;
pub struct BackupManagementInput {
    proof: VerifiedCookie,
    password: Zeroizing<String>,
    operation: BackupOperation,
}
impl BackupManagementInput {
    pub fn proof(&self) -> &VerifiedCookie {
        &self.proof
    }
    pub fn password(&self) -> &str {
        &self.password
    }
    pub fn operation(&self) -> BackupOperation {
        self.operation
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    password: String,
}
pub(super) fn routes(router: Router<Arc<StateData>>) -> Router<Arc<StateData>> {
    router
        .route("/api/auth/two-factor/view-backup-codes", post(view))
        .route(
            "/api/auth/two-factor/generate-backup-codes",
            post(regenerate),
        )
}
async fn view(state: State<Arc<StateData>>, r: Request) -> Response {
    execute(state, r, BackupOperation::View).await
}
async fn regenerate(state: State<Arc<StateData>>, r: Request) -> Response {
    execute(state, r, BackupOperation::Regenerate).await
}
async fn execute(
    State(state): State<Arc<StateData>>,
    request: Request,
    operation: BackupOperation,
) -> Response {
    if auth(&state).backup_management.is_none() {
        return refused(StatusCode::NOT_FOUND);
    }
    if let Err(s) = gates(&request, &state, Operation::BackupManagement) {
        return refused(s);
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
    // Serde structs accept positional arrays; this public wire contract accepts objects only.
    if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
        return refused(StatusCode::BAD_REQUEST);
    }
    let wire: Wire = match serde_json::from_slice(&bytes) {
        Ok(w) => w,
        Err(_) => return refused(StatusCode::BAD_REQUEST),
    };
    if wire.password.len() > rentemester_runtime::password::INPUT_BYTE_LIMIT {
        return refused(StatusCode::BAD_REQUEST);
    }
    let input = BackupManagementInput {
        proof,
        password: Zeroizing::new(wire.password),
        operation,
    };
    let owner = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        (auth(&owner)
            .backup_management
            .as_ref()
            .expect("registered backup management"))(input)
    })
    .await;
    match result {
        Ok(Ok(codes)) => {
            if codes.len() > 10
                || matches!(operation, BackupOperation::Regenerate) && codes.len() != 10
            {
                return uncertain();
            }
            let values: Vec<&str> = codes.iter().collect();
            let body =
                Zeroizing::new(serde_json::json!({"status":true,"backupCodes":values}).to_string());
            let mut r = response(StatusCode::OK, "");
            *r.body_mut() = axum::body::Body::from(body.to_string());
            r
        }
        Ok(Err(e)) => native_failure(e),
        _ => uncertain(),
    }
}
