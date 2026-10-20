//! Standalone Rust HTTP process serving the existing cockpit on the API origin.
mod auth_delivery;
mod backend;
mod document_scanner;
mod invoice_extractor;
mod legacy_adoption_cli;
mod document_pdf;
mod hosted_worker;
mod imap_transport;
mod mail_worker;
mod portable_backup_cli;
mod product_http;
mod report_export;
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
    let mut restore_archive = None;
    let mut trusted_backup_key = None;
    let mut confirmed_target = None;
    let mut verify_archive = None;
    let mut source_unavailable = false;
    let mut restore_transfer = false;
    let mut scheduled_worker = false;
    let mut scheduled_stdin = false;
    let mut mail_worker = false;
    let mut mail_stdin = false;
    let mut upgrade_mail_schema = false;
    let mut adopt_legacy = None;
    let mut legacy_source_stopped = false;
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
            "--adopt-legacy" => adopt_legacy = Some(PathBuf::from(args.next().ok_or("--adopt-legacy requires source workspace")?)),
            "--legacy-source-stopped" => legacy_source_stopped = true,
            "--upgrade-mail-schema" => upgrade_mail_schema = true,
            "--mail-worker" => mail_worker = true,
            "--mail-service-key-stdin" => mail_stdin = true,
            "--scheduled-worker" => scheduled_worker = true,
            "--scheduled-service-key-stdin" => scheduled_stdin = true,
            "--restore-backup" => {
                restore_archive = Some(PathBuf::from(
                    args.next().ok_or("--restore-backup requires archive")?,
                ))
            }
            "--verify-backup" => {
                verify_archive = Some(PathBuf::from(
                    args.next().ok_or("--verify-backup requires archive")?,
                ))
            }
            "--trusted-backup-key" => {
                trusted_backup_key = Some(PathBuf::from(
                    args.next()
                        .ok_or("--trusted-backup-key requires separate trusted public key")?,
                ))
            }
            "--confirm-target" => {
                confirmed_target = Some(PathBuf::from(
                    args.next()
                        .ok_or("--confirm-target requires exact fresh configured root")?,
                ))
            }
            "--source-unavailable" => source_unavailable = true,
            "--restore-transfer" => restore_transfer = true,
            "--origin" => public_origin = Some(args.next().ok_or("--origin requires a URL")?),
            "--help" | "-h" => {
                println!(
                    "rentemester-server [--listen 127.0.0.1:4319] [--web-dir app/dist] [--config PRIVATE.json] [--create] [--origin http://localhost:4319]"
                );
                println!("Hosted generation: --scheduled-worker --scheduled-service-key-stdin --config PRIVATE.json (ordinary reopen only; transient stdin service key, no saved cookie/key; 32 candidates every 30 seconds; normal Copenhagen today only, no catch-up)");
                println!("Schema upgrade: --upgrade-mail-schema --config PRIVATE.json (exclusive stopped server; admits only exact v30→v31 or current, preserves business data; ordinary startup never upgrades)");
                println!("Mailbox intake: --mail-worker --mail-service-key-stdin --config PRIVATE.json (ordinary reopen only; current service key from stdin, explicitly granted mailbox configuration; credentials excluded from backups). When both workers are enabled, one stdin key is shared in memory.");
                println!("Build the cockpit first: cd app && bun run build");
                println!("Without --config, serve the cockpit and health endpoint only.");
                println!(
                    "--config defaults to runtimeProfile=dedicated-linux; run as its dedicated UID with no capabilities, no-new-privileges and umask077."
                );
                println!(
                    "macOS local operation requires config runtimeProfile=local-user-macos, equal nonroot real/effective/saved IDs, APFS, umask077 and safe owner/root ancestry (only non-inheriting deny-delete ACLs) and strict private ACL-free workspace paths. All code under that local account is trusted; it is not the Linux dedicated broker isolation profile."
                );
                println!(
                    "--create exclusively creates a new workspace. Omit it for reopen or bootstrap recovery."
                );
                println!("HTTPS origins require private config authMail gateway; local HTTP uses private verification/reset/invitation files.");
                println!(
                    "Verify independently: --verify-backup ARCHIVE --trusted-backup-key RAW_PUBLIC_KEY"
                );
                println!(
                    "Fresh restore: --config NEW_PRIVATE.json --restore-backup ARCHIVE --trusted-backup-key RAW_PUBLIC_KEY --confirm-target EXACT_CONFIG_ROOT --source-unavailable (explicit declaration, no global single-use guarantee)"
                );
                println!("Legacy adoption: --adopt-legacy SOURCE --config NEW_PRIVATE.json --confirm-target EXACT_CONFIG_ROOT --legacy-source-stopped (stop and drain every TS server, CLI, MCP and job first; two exact review hashes must be confirmed on stdin; exits after adoption, then start normally for fresh owner/MFA)");
                println!("Stage handoff: --config NEW_PRIVATE.json --restore-backup OFFERED_ARCHIVE --trusted-backup-key SOURCE_PUBLIC_KEY --confirm-target EXACT_CONFIG_ROOT --restore-transfer (requires transferTrustedKeyFile; remains read-only until reviewed source release activation)");
                return Ok(());
            }
            _ => return Err(format!("unknown option: {arg}").into()),
        }
    }
    if let Some(source) = adopt_legacy {
        if create || upgrade_mail_schema || restore_archive.is_some() || verify_archive.is_some()
            || scheduled_worker || scheduled_stdin || mail_worker || mail_stdin || restore_transfer
            || trusted_backup_key.is_some() || source_unavailable || !legacy_source_stopped {
            return Err("legacy adoption is exclusive and requires --legacy-source-stopped".into());
        }
        backend::Backend::adopt_legacy(
            config.as_deref().ok_or("legacy adoption requires --config")?,
            &source, confirmed_target.as_deref().ok_or("legacy adoption requires --confirm-target")?,
        )?;
        return Ok(());
    }
    if legacy_source_stopped { return Err("--legacy-source-stopped requires --adopt-legacy".into()); }
    if upgrade_mail_schema {
        if create || restore_archive.is_some() || verify_archive.is_some()
            || scheduled_worker || scheduled_stdin || mail_worker || mail_stdin || restore_transfer
            || trusted_backup_key.is_some() || confirmed_target.is_some() || source_unavailable {
            return Err("--upgrade-mail-schema is an exclusive configured operator action".into());
        }
        let path = config.as_ref().ok_or("--upgrade-mail-schema requires --config")?;
        backend::Backend::upgrade_mail_control(path)?;
        println!("Mail control schema is current; no mailbox network operation performed.");
        return Ok(());
    }
    hosted_worker::validate_flags(scheduled_worker, scheduled_stdin, config.is_some(), create || restore_archive.is_some() || verify_archive.is_some())?;
    mail_worker::validate_flags(mail_worker, mail_stdin, config.is_some(), create || restore_archive.is_some() || verify_archive.is_some())?;
    if let Some(archive) = verify_archive {
        if restore_archive.is_some() || create || confirmed_target.is_some() || source_unavailable || restore_transfer {
            return Err("verification flags cannot select restore/create".into());
        }
        let verified = portable_backup_cli::verified(
            &archive,
            trusted_backup_key
                .as_deref()
                .ok_or("separate trusted public key required")?,
        )?;
        rentemester_host::Owner::validate_backup(&verified)?;
        println!(
            "{{\"ok\":true,\"format\":\"rentemester-native-backup/1\",\"companies\":{},\"scope\":\"signed_inventory_current_schema_integrity_journal_chain_and_originals\"}}",
            verified.manifest().context.companies.len()
        );
        return Ok(());
    }
    let restore = if let Some(archive) = restore_archive {
        if config.is_none() {
            return Err("restore requires --config".into());
        }
        if source_unavailable == restore_transfer {
            return Err("choose exactly one: --source-unavailable recovery or --restore-transfer staged handoff".into());
        }
        Some(backend::RestoreRequest {
            archive,
            trusted_key: trusted_backup_key.ok_or("separate trusted public key required")?,
            confirmed_target: confirmed_target.ok_or("exact --confirm-target required")?,
            transfer: restore_transfer,
        })
    } else {
        if trusted_backup_key.is_some() || confirmed_target.is_some() || source_unavailable || restore_transfer {
            return Err("restore flags require --restore-backup".into());
        }
        None
    };
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
    let origin = canonical_origin(&origin)?;
    // SQLite/password initialization runs outside Tokio's asynchronous worker threads.
    let setup_origin = origin.clone();
    let mut native = tokio::task::spawn_blocking(move || {
        config
            .map(|path| {
                (if restore.is_some() {
                    backend::Backend::load_with_restore(
                        &path,
                        &setup_origin,
                        create,
                        listen.ip().is_loopback(),
                        restore.as_ref(),
                    )
                } else {
                    backend::Backend::load(&path, &setup_origin, create, listen.ip().is_loopback())
                })
                .map_err(|e| e.to_string())
            })
            .transpose()
    })
    .await?
    .map_err(|e| format!("workspace startup refused: {e}"))?;
    if scheduled_worker || mail_worker {
        let backend = native.as_mut().ok_or("scheduled worker requires configured workspace")?;
        let backend = std::sync::Arc::get_mut(backend).ok_or("scheduled worker startup refused")?;
        let key=hosted_worker::read_key(std::io::stdin().lock(),backend.cookies.workspace())?;
        let key = std::sync::Arc::new(key);
        if scheduled_worker { backend.scheduled_executor=Some(key.clone()); }
        if mail_worker { backend.mail_executor=Some(key); }
    }
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
    let worker = if scheduled_worker {
        Some(hosted_worker::Worker::start(native.as_ref().ok_or("scheduled worker requires workspace")?.clone()))
    } else { None };
    let mail = if mail_worker {
        Some(mail_worker::Worker::start(native.as_ref().ok_or("mail worker requires workspace")?.clone()))
    } else { None };
    let served = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await;
    if let Some(mail)=mail { mail.stop().await; }
    if let Some(worker)=worker { worker.stop().await; }
    served?;
    Ok(())
}

fn canonical_origin(origin: &str) -> Result<String, Box<dyn std::error::Error>> {
    backend::cookie_profile(origin)?;
    Ok(url::Url::parse(origin)?.origin().ascii_serialization())
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

#[cfg(test)]
mod origin_tests {
    #[test]
    fn public_origin_is_validated_before_shared_canonicalization() {
        assert_eq!(super::canonical_origin("HTTPS://ACCOUNTING.example.invalid:443").unwrap(),"https://accounting.example.invalid");
        assert_eq!(super::canonical_origin("http://LOCALHOST:4319").unwrap(),"http://localhost:4319");
        assert!(super::canonical_origin("https://example.invalid/untrusted-path").is_err());
    }
}
