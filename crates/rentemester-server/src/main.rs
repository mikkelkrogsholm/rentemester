//! Standalone Rust HTTP process serving the existing cockpit on the API origin.
mod backend;
mod product_http;
mod setup_http;
use axum::{
    Router,
    http::{HeaderValue, StatusCode},
    routing::get,
};
use std::{net::SocketAddr, path::PathBuf};
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut listen: SocketAddr = "127.0.0.1:4319".parse()?;
    let mut web = PathBuf::from("app/dist");
    let mut config = None;
    let mut create = false;
    let mut public_origin: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => listen = args.next().ok_or("--listen requires an address")?.parse()?,
            "--web-dir" => web = PathBuf::from(args.next().ok_or("--web-dir requires a path")?),
            "--config" => {
                config = Some(PathBuf::from(
                    args.next().ok_or("--config requires a path")?,
                ))
            }
            "--create" => create = true,
            "--origin" => public_origin = Some(args.next().ok_or("--origin requires a URL")?),
            "--help" | "-h" => {
                println!(
                    "rentemester-server [--listen 127.0.0.1:4319] [--web-dir app/dist] [--config PRIVATE.json] [--create] [--origin http://localhost:4319]"
                );
                println!("Build the cockpit first: cd app && bun run build");
                println!("Without --config, serve the cockpit and health endpoint only.");
                println!(
                    "--config connects a protected Linux workspace; run as its dedicated UID with no capabilities, no-new-privileges and umask 077."
                );
                println!(
                    "--create exclusively creates a new workspace. Omit it for reopen or bootstrap recovery."
                );
                println!("Verification delivery uses private local files, not external email.");
                return Ok(());
            }
            _ => return Err(format!("unknown option: {arg}").into()),
        }
    }
    if !listen.ip().is_loopback() && public_origin.is_none() {
        return Err("this development entrypoint requires a loopback listen address".into());
    }
    let web = web
        .canonicalize()
        .map_err(|_| "cockpit build missing; run: cd app && bun run build")?;
    if !web.join("index.html").is_file() {
        return Err("cockpit index.html missing; run: cd app && bun run build".into());
    }
    let listener = tokio::net::TcpListener::bind(listen).await?;
    if create && config.is_none() {
        return Err("--create requires --config".into());
    }
    let origin =
        public_origin.unwrap_or_else(|| format!("http://{}", listener.local_addr().unwrap()));
    rentemester_runtime::recovery::RecoveryLinks::new(
        &origin,
        &[&origin],
        rentemester_runtime::auth::CookieProfile::Development,
    )?;
    // SQLite/password initialization runs outside Tokio's asynchronous worker threads.
    let setup_origin = origin.clone();
    let native = tokio::task::spawn_blocking(move || {
        config
            .map(|path| {
                backend::Backend::load(&path, &setup_origin, create).map_err(|e| e.to_string())
            })
            .transpose()
    })
    .await?
    .map_err(|e| format!("workspace startup refused: {e}"))?;
    let identity = native
        .as_ref()
        .map(|backend| {
            backend.identity(&origin).map(|identity| {
                identity
                    .merge(product_http::router(backend.clone(), &origin))
                    .merge(setup_http::router(backend.clone(), &origin))
            })
        })
        .transpose()
        .map_err(|_| "invalid identity HTTP configuration")?;
    let app = cockpit_router(web, identity, native.clone());
    println!("Rentemester Rust: http://{}", listener.local_addr()?);
    println!(
        "Cockpit served directly by Rust. Native workspace connected: {}",
        native.is_some()
    );
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

fn cockpit_router(
    web: PathBuf,
    identity: Option<Router>,
    native: Option<std::sync::Arc<backend::Backend>>,
) -> Router {
    // API misses must never return index.html with a successful status.
    let connected = identity.is_some();
    let api = Router::new()
        .route("/health", get(move || {
            let native = native.clone();
            async move {
            let body = if connected {
                serde_json::json!({"ok":true,"service":"rentemester","backend":"rust","authRequired":true,"deploymentProfile":"hosted","workspaceReady":true,"setupRequired":native.as_ref().is_some_and(|b|b.setup_pending.load(std::sync::atomic::Ordering::Acquire))}).to_string()
            } else {
                r#"{"ok":true,"backend":"rust","workspaceReady":false}"#.to_owned()
            };
            ([ ("content-type", "application/json") ], body)
        }}))
        .fallback(move || async move {
            let body = if connected {
                r#"{"ok":false,"code":"rust_endpoint_not_implemented","errors":["This endpoint is not implemented by the Rust server yet"]}"#
            } else {
                r#"{"ok":false,"code":"rust_workspace_not_connected","errors":["Rust workspace integration is not connected yet"]}"#
            };
            (StatusCode::SERVICE_UNAVAILABLE,
             [("content-type", "application/json")],
             body)
        });
    let router = Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new(&web).fallback(ServeFile::new(web.join("index.html"))));
    let router = if let Some(identity) = identity {
        router.merge(identity)
    } else {
        router
    };
    router
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
}
