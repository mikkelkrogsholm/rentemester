//! Rust host for the same web interface, served directly by the selected Rust backend.
#[allow(dead_code)]
mod local_backend;
use local_backend::{Managed, Start};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, State, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

#[derive(Clone)]
struct Options {
    local: bool,
    create: bool,
    profile: PathBuf,
    server_binary: PathBuf,
    web: PathBuf,
    server: Option<String>,
}
impl Options {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let (server_binary, web) = local_backend::bundled_paths();
        let mut value = Self {
            local: false,
            create: false,
            profile: local_backend::default_profile(),
            server_binary,
            web,
            server: None,
        };
        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--local" => value.local = true,
                "--create-local" => {
                    value.local = true;
                    value.create = true;
                }
                "--local-workspace" => {
                    value.profile = args.next().ok_or("--local-workspace kræver en sti")?.into()
                }
                "--server-binary" => {
                    value.server_binary = args.next().ok_or("--server-binary kræver en sti")?.into()
                }
                "--web-dir" => value.web = args.next().ok_or("--web-dir kræver en sti")?.into(),
                "--server" => {
                    value.server = Some(args.next().ok_or("--server kræver en HTTPS-adresse")?)
                }
                _ => return Err(format!("Ukendt argument: {flag}")),
            }
        }
        if value.local && value.server.is_some() {
            return Err("Vælg enten lokal drift eller en ekstern server".into());
        }
        if let Some(server) = &value.server {
            remote_origin(server)?;
        }
        Ok(value)
    }
}
struct ShellState {
    options: Options,
    managed: Mutex<Option<Managed>>,
    connecting: AtomicBool,
}
fn readable(error: rentemester_client::Error) -> String {
    match error {
        rentemester_client::Error::Refused { status: 401 } => {
            "Log ind igen og gennemfør tofaktorbekræftelse.".into()
        }
        rentemester_client::Error::Refused { status: 403 } => {
            "Serveren afviste din aktuelle adgang.".into()
        }
        rentemester_client::Error::Transport => {
            "Serveren kunne ikke nås. Kontrollér adresse, netværk og TLS-certifikat.".into()
        }
        error => error.to_string(),
    }
}
fn remote_origin(value: &str) -> Result<Url, String> {
    if value.len() > 4096
        || value.trim() != value
        || value.contains('\\')
        || value.chars().any(char::is_control)
    {
        return Err("Angiv serverens HTTPS-adresse uden sti eller loginoplysninger".into());
    }
    let url = Url::parse(value).map_err(|_| "Serveradressen er ugyldig")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err("Angiv serverens HTTPS-adresse uden sti eller loginoplysninger".into());
    }
    Ok(url)
}
fn workspace_navigation(url: &Url, origin: &Url) -> bool {
    url.origin() == origin.origin()
        || url.scheme() == "blob"
            && url
                .as_str()
                .strip_prefix("blob:")
                .and_then(|s| Url::parse(s).ok())
                .is_some_and(|inner| inner.origin() == origin.origin())
}
fn trusted_launcher(window: &WebviewWindow) -> bool {
    window.label() == "launcher"
        && window.url().is_ok_and(|url| {
            url.scheme() == "tauri" && url.host_str() == Some("localhost")
                || matches!(url.scheme(), "http" | "https")
                    && url.host_str() == Some("tauri.localhost")
        })
}
#[tauri::command]
async fn launch(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, Arc<ShellState>>,
    mode: String,
    profile: String,
    server: String,
    name: String,
) -> Result<(), String> {
    if !trusted_launcher(&window) {
        return Err("Denne handling er kun tilgængelig fra den lokale startside".into());
    }
    connect(app, state.inner().clone(), mode, profile, server, name).await
}
async fn connect(
    app: AppHandle,
    state: Arc<ShellState>,
    mode: String,
    profile: String,
    server: String,
    name: String,
) -> Result<(), String> {
    if state.connecting.swap(true, Ordering::AcqRel) {
        return Err("En forbindelse er allerede ved at blive åbnet".into());
    }
    let result = connect_inner(&app, &state, mode, profile, server, name).await;
    state.connecting.store(false, Ordering::Release);
    result
}
async fn connect_inner(
    app: &AppHandle,
    state: &Arc<ShellState>,
    mode: String,
    profile: String,
    server: String,
    name: String,
) -> Result<(), String> {
    if app.get_webview_window("workspace").is_some() {
        return Err("Regnskabsvinduet er allerede åbent".into());
    }
    let url = if mode == "remote" {
        remote_origin(&server)?
    } else if mode == "local" || mode == "create" {
        if name.len() > 256 {
            return Err("Navnet er for langt".into());
        }
        let input = Start {
            profile: if profile.is_empty() {
                state.options.profile.clone()
            } else {
                profile.into()
            },
            create: mode == "create",
            company_name: if name.trim().is_empty() {
                "Mit regnskab".into()
            } else {
                name
            },
            server_binary: state.options.server_binary.clone(),
            web: state.options.web.clone(),
        };
        let managed = tauri::async_runtime::spawn_blocking(move || Managed::start(input))
            .await
            .map_err(|_| "Den lokale server kunne ikke startes")??;
        let url =
            Url::parse(&managed.browser_url()).map_err(|_| "Lokal serveradresse er ugyldig")?;
        *state
            .managed
            .lock()
            .map_err(|_| "Lokal drift er utilgængelig")? = Some(managed);
        url
    } else {
        return Err("Vælg lokal drift eller en server".into());
    };
    let origin = url.clone();
    let popup_origin = origin.clone();
    let download_origin = origin.clone();
    let built = WebviewWindowBuilder::new(app, "workspace", WebviewUrl::External(url))
        .title("Rentemester")
        .inner_size(1280.0, 850.0)
        .min_inner_size(800.0, 600.0)
        .disable_drag_drop_handler()
        .on_navigation(move |url| workspace_navigation(url, &origin))
        .on_download(move |window, event| match event {
            // WK download actions bypass ordinary navigation admission. Keep the
            // selected-origin boundary; the authenticated bytes stay in WebKit.
            tauri::webview::DownloadEvent::Requested { url, destination } => {
                if !workspace_navigation(&url, &download_origin) {
                    return false;
                }
                #[cfg(target_os = "macos")]
                {
                    // A native save panel grants access to the user's chosen
                    // location. An implicit Downloads write can otherwise block
                    // WebKit's main thread on macOS folder-privacy admission.
                    // RFD runs the panel directly when already on the main thread.
                    let name = destination
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy();
                    let Some(selected) = rfd::FileDialog::new()
                        .set_parent(&window.window())
                        .set_title("Gem fil")
                        .set_file_name(name)
                        .save_file()
                    else {
                        return false;
                    };
                    // WKDownload requires a destination that does not exist.
                    // Preserve existing exports rather than removing them before
                    // a download that may still fail or be cancelled.
                    if selected.exists() {
                        show_info(
                            window.app_handle(),
                            "Filen findes allerede",
                            "Vælg et nyt filnavn, når du henter filen igen.".into(),
                        );
                        return false;
                    }
                    *destination = selected;
                }
                #[cfg(not(target_os = "macos"))]
                let _ = (window, destination);
                true
            }
            // macOS does not provide the completed path. Never log source URLs
            // or claim a filename from an unavailable completion path.
            tauri::webview::DownloadEvent::Finished { .. } => true,
            _ => false,
        })
        .on_new_window(move |url, _| {
            if workspace_navigation(&url, &popup_origin) {
                tauri::webview::NewWindowResponse::Allow
            } else {
                tauri::webview::NewWindowResponse::Deny
            }
        })
        .build();
    if built.is_err() {
        if let Ok(mut guard) = state.managed.lock() {
            guard.take();
        }
        return Err("Regnskabsvinduet kunne ikke åbnes".into());
    }
    if let Some(launcher) = app.get_webview_window("launcher") {
        let _ = launcher.hide();
    }
    Ok(())
}
fn show_info(app: &AppHandle, title: &str, message: String) {
    if let Some(window) = app.get_webview_window("local-info") {
        let _ = window.close();
    }
    let title = serde_json::to_string(title).unwrap_or_default();
    let message = serde_json::to_string(&message).unwrap_or_default();
    let script = format!(
        "document.getElementById('title').textContent={title};document.getElementById('message').textContent={message};"
    );
    let _ = WebviewWindowBuilder::new(app, "local-info", WebviewUrl::App("local-info.html".into()))
        .title("Rentemester · Lokal drift")
        .inner_size(650.0, 380.0)
        .on_navigation(|url| {
            matches!(url.scheme(), "tauri") || url.host_str() == Some("tauri.localhost")
        })
        .on_page_load(move |window, _| {
            let _ = window.eval(&script);
        })
        .build();
}
fn local_menu(app: &AppHandle, id: &str) {
    if !matches!(id, "local-verification" | "local-backup-key") {
        return;
    }
    let state = app.state::<Arc<ShellState>>();
    let Ok(guard) = state.managed.lock() else {
        return;
    };
    let Some(managed) = guard.as_ref() else {
        show_info(
            app,
            "Lokal drift",
            "Denne handling kræver den administrerede lokale Rust-server.".into(),
        );
        return;
    };
    match id {
        "local-verification" => match managed
            .verification_url()
            .and_then(|url| Url::parse(&url).map_err(|_| "Bekræftelseslinket er ugyldigt".into()))
        {
            Ok(url) => {
                if let Some(window) = app.get_webview_window("workspace") {
                    let _ = window.navigate(url);
                }
            }
            Err(error) => show_info(app, "Bekræft lokal e-mail", error),
        },
        "local-backup-key" => match managed.backup_public_key() {
            Ok((path, key)) => show_info(
                app,
                "Offentlig backupnøgle",
                format!(
                    "{}\n\n{}\n\nDette er den offentlige nøgle. Den private signeringsnøgle forlader ikke den lokale profil.",
                    path.display(),
                    key.iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>()
                ),
            ),
            Err(error) => show_info(app, "Offentlig backupnøgle", error),
        },
        _ => {}
    }
}
fn main() {
    // Held until the managed child has stopped and the application exits.
    let _package_lease = match rentemester_runtime::package_update::current_executable_lease() {
        Ok(lease) => lease,
        Err(error) => {
            eprintln!("App-pakken kan ikke åbnes under en opdatering: {error}");
            std::process::exit(1);
        }
    };
    if std::env::args().any(|arg| arg == "--help") {
        println!(
            "Rentemester Desktop\n  --local | --create-local [--local-workspace PATH]\n  --server HTTPS_ORIGIN\n  Development: --server-binary PATH --web-dir PATH\nLogin og MFA foregår i webinterfacet. Lokal data og nøgler forbliver i den private profil."
        );
        return;
    }
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let state = Arc::new(ShellState {
        options: options.clone(),
        managed: Mutex::new(None),
        connecting: AtomicBool::new(false),
    });
    let app = tauri::Builder::default()
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![launch])
        .setup(move |app| {
            let handle = app.handle();
            let verify = MenuItem::with_id(
                handle,
                "local-verification",
                "Bekræft lokal e-mail",
                true,
                None::<&str>,
            )?;
            let backup = MenuItem::with_id(
                handle,
                "local-backup-key",
                "Vis offentlig backupnøgle",
                true,
                None::<&str>,
            )?;
            let local = Submenu::with_items(handle, "Lokal drift", true, &[&verify, &backup])?;
            let edit = Submenu::with_items(
                handle,
                "Rediger",
                true,
                &[
                    &PredefinedMenuItem::undo(handle, None)?,
                    &PredefinedMenuItem::redo(handle, None)?,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::cut(handle, None)?,
                    &PredefinedMenuItem::copy(handle, None)?,
                    &PredefinedMenuItem::paste(handle, None)?,
                    &PredefinedMenuItem::select_all(handle, None)?,
                ],
            )?;
            let application = Submenu::with_items(
                handle,
                "Rentemester",
                true,
                &[
                    &PredefinedMenuItem::about(handle, None, None)?,
                    &PredefinedMenuItem::quit(handle, None)?,
                ],
            )?;
            app.set_menu(Menu::with_items(handle, &[&application, &edit, &local])?)?;
            WebviewWindowBuilder::new(app, "launcher", WebviewUrl::App("index.html".into()))
                .title("Rentemester")
                .inner_size(600.0, 760.0)
                .resizable(false)
                .on_navigation(|url| {
                    url.scheme() == "tauri" && url.host_str() == Some("localhost")
                        || matches!(url.scheme(), "http" | "https")
                            && url.host_str() == Some("tauri.localhost")
                })
                .build()?;
            if options.local || options.server.is_some() {
                let app = app.handle().clone();
                let state = state.clone();
                let mode = if options.local {
                    if options.create { "create" } else { "local" }
                } else {
                    "remote"
                }
                .to_owned();
                let profile = options.profile.to_string_lossy().into_owned();
                let server = options.server.clone().unwrap_or_default();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = connect(
                        app.clone(),
                        state,
                        mode,
                        profile,
                        server,
                        "Mit regnskab".into(),
                    )
                    .await
                    {
                        show_info(&app, "Forbindelsen kunne ikke åbnes", error);
                    }
                });
            }
            Ok(())
        })
        .on_menu_event(|app, event| local_menu(app, event.id().as_ref()))
        .on_window_event(|window, event| {
            if window.label() == "workspace" && matches!(event, tauri::WindowEvent::Destroyed) {
                let app = window.app_handle();
                let state = app.state::<Arc<ShellState>>();
                if let Ok(mut guard) = state.managed.lock() {
                    guard.take();
                }
                if let Some(launcher) = app.get_webview_window("launcher") {
                    let _ = launcher.show();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("Rentemester desktop host could not start");
    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            let state = app.state::<Arc<ShellState>>();
            if let Ok(mut guard) = state.managed.lock() {
                guard.take();
            }
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_connections_require_exact_https_origin_and_same_origin_navigation() {
        for value in [
            "http://127.0.0.1:80",
            "https://example.invalid/path",
            "https://user:password@example.invalid/",
            "https://example.invalid/?token=x",
        ] {
            assert!(remote_origin(value).is_err());
        }
        let origin = remote_origin("https://example.invalid").unwrap();
        assert!(workspace_navigation(
            &Url::parse("https://example.invalid/companies/a").unwrap(),
            &origin
        ));
        assert!(workspace_navigation(
            &Url::parse("blob:https://example.invalid/fixture").unwrap(),
            &origin
        ));
        assert!(!workspace_navigation(
            &Url::parse("blob:https://other.invalid/fixture").unwrap(),
            &origin
        ));
        assert!(!workspace_navigation(
            &Url::parse("tauri://localhost/index.html").unwrap(),
            &origin
        ));
        assert!(!workspace_navigation(
            &Url::parse("https://other.invalid").unwrap(),
            &origin
        ));
    }
    #[test]
    fn startup_does_not_mix_local_and_remote_or_accept_credentials() {
        assert!(
            Options::parse([
                "--local".into(),
                "--server".into(),
                "https://example.invalid".into()
            ])
            .is_err()
        );
        assert!(Options::parse(["--cookie-file".into(), "private".into()]).is_err());
        assert!(Options::parse(["--create-local".into()]).unwrap().create);
    }
}
