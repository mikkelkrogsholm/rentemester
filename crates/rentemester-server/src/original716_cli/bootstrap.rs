//! Private process operator entry. One native Owner; no HTTP actor/path authority.
use rentemester_runtime::{
    auth::{CookieProfile, CookieVerifier, SystemClock},
    bootstrap::{Operator, ReservationKey},
    email_token::EmailTokenSigner,
    recovery::RecoveryLinks,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::Command,
};
use zeroize::Zeroizing;
type Error = Box<dyn std::error::Error>;
type Flags<'a> = BTreeMap<&'a str, &'a str>;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    root: PathBuf,
    broker_uid: u32,
    workspace: String,
    company_slug: String,
    #[serde(default)]
    runtime_profile: rentemester_runtime::namespace::NamespaceProfile,
    cookie_key: String,
    mfa_key: String,
    setup_actor: Option<String>,
    #[serde(default)]
    operator_actors: Vec<String>,
    #[serde(default)]
    auth_mail: Option<crate::auth_delivery::Config>,
}
impl Drop for Config {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.cookie_key.zeroize();
        self.mfa_key.zeroize();
    }
}
fn config(path: &Path) -> Result<Config, Error> {
    #[cfg(target_os = "macos")]
    let nofollow = 0x100;
    #[cfg(target_os = "linux")]
    let nofollow = 0x20000;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nofollow)
        .open(path)?;
    let m = file.metadata()?;
    if !m.is_file()
        || m.mode() & 0o7777 != 0o600
        || m.nlink() != 1
        || m.uid() != rentemester_runtime::original716::bootstrap::private_process()
        || m.len() > 1024 * 1024
    {
        return Err(
            "runtime config requires a current-user single-link private0600 regular file".into(),
        );
    }
    let mut text = Zeroizing::new(String::new());
    file.take(1024 * 1024 + 1).read_to_string(&mut text)?;
    let value: Value = serde_json::from_str(&text).map_err(|_| "invalid private runtime config")?;
    if value.get("firstOwner").is_some_and(|v| !v.is_null()) {
        return Err("operator command config must not contain firstOwner credentials; use bootstrap-first with private password-file".into());
    }
    Ok(serde_json::from_value(value).map_err(|_| "invalid private runtime config")?)
}
fn flags(args: &[String], start: usize) -> Result<Flags<'_>, Error> {
    let mut f = BTreeMap::new();
    let mut i = start;
    while i < args.len() {
        let (k, a) = args[i]
            .split_once('=')
            .map_or((args[i].as_str(), None), |(k, v)| (k, Some(v)));
        if !k.starts_with("--") {
            return Err("unexpected positional argument".into());
        }
        let boolean = matches!(k, "--no-vat" | "--no-open" | "--json" | "--help");
        let v = if boolean {
            if a.is_some() {
                return Err("boolean flag takes no value".into());
            }
            "true"
        } else if let Some(a) = a {
            a
        } else {
            i += 1;
            args.get(i)
                .filter(|s| !s.starts_with("--"))
                .ok_or("missing flag value")?
        };
        if f.insert(k, v).is_some() {
            return Err("duplicate flag".into());
        }
        i += 1;
    }
    Ok(f)
}
fn required<'a>(f: &Flags<'a>, k: &str) -> Result<&'a str, Error> {
    f.get(k)
        .copied()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("{k} is required").into())
}
fn default_config(workspace: &Path) -> Result<PathBuf, Error> {
    let parent = workspace.parent().ok_or("workspace needs a parent")?;
    let name = workspace
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("invalid workspace")?;
    Ok(parent
        .join(format!(".{name}.rentemester-runtime"))
        .join("server.json"))
}
fn workspace(f: &Flags<'_>) -> Result<PathBuf, Error> {
    let raw = f
        .get("--workspace")
        .map(|s| s.to_string())
        .or_else(|| std::env::var("RENTEMESTER_WORKSPACE").ok())
        .ok_or("--workspace is required")?;
    let p = PathBuf::from(raw);
    if !p.is_absolute()
        || p.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err("workspace must be an explicit absolute path without traversal".into());
    }
    Ok(p)
}
fn settings(
    f: &Flags<'_>,
    local: bool,
) -> Result<rentemester_storage::company_creation::Settings, Error> {
    let name = if local {
        required(f, "--company-name")?
    } else {
        f.get("--name").copied().unwrap_or("Rentemester company")
    };
    if f.contains_key("--no-vat") && f.get("--vat-period").is_some_and(|v| *v != "none") {
        return Err("--no-vat cannot be combined with --vat-period <cadence>".into());
    }
    let vat = if f.contains_key("--no-vat") || f.get("--vat-period") == Some(&"none") {
        Value::Null
    } else {
        json!(f.get("--vat-period").copied().unwrap_or("quarter"))
    };
    let payment = json!({"bankName":f.get("--bank-name"),"registrationNo":f.get("--bank-reg"),"accountNo":f.get("--bank-account"),"iban":f.get("--iban")});
    let terms = f
        .get("--payment-terms")
        .map(|s| s.parse::<i64>())
        .transpose()
        .map_err(|_| "--payment-terms must be an integer")?;
    let input = serde_json::from_value(
        json!({"name":name,"slug":f.get("--company"),"cvr":f.get("--cvr"),"fiscalYearStartMonth":f.get("--fiscal-year-start-month"),"fiscalYearLabelStrategy":f.get("--fiscal-year-label-strategy"),"vatPeriodType":vat,"address":f.get("--address"),"postalCode":f.get("--postal-code"),"city":f.get("--city"),"paymentTermsDays":terms,"payment":payment}),
    )?;
    Ok(rentemester_storage::company_creation::settings(&input)?)
}
fn generate(
    path: &Path,
    root: &Path,
    s: &rentemester_storage::company_creation::Settings,
    actor: &str,
) -> Result<(), Error> {
    if root.try_exists()? {
        return Err("fresh init refuses an existing workspace; no overwrite or adoption".into());
    }
    Operator::from_policy("fresh-validation", actor, &[actor.into()])?;
    let uid = rentemester_runtime::original716::bootstrap::private_process();
    let profile = path.parent().ok_or("config requires parent")?;
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(profile)?;
    let workspace = rentemester_runtime::auth::new_token()?;
    let cookie = rentemester_runtime::auth::new_token()?;
    let mfa = rentemester_runtime::auth::new_token()?;
    let profile = if cfg!(target_os = "macos") {
        "local-user-macos"
    } else {
        "dedicated-linux"
    };
    let value = Zeroizing::new(serde_json::to_string_pretty(
        &json!({"root":root,"brokerUid":uid,"workspace":workspace.as_str(),"companySlug":s.slug,"companyName":s.name,"cookieKey":cookie.as_str(),"mfaKey":mfa.as_str(),"runtimeProfile":profile,"setupActor":actor,"operatorActors":[actor]}),
    )?);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(value.as_bytes())?;
    file.sync_all()?;
    Ok(())
}
fn owner(c: &Config) -> Result<rentemester_host::Owner, Error> {
    Ok(rentemester_host::Owner::open_ready_with_profile(
        &c.root,
        c.broker_uid,
        &c.workspace,
        &c.company_slug,
        c.runtime_profile,
    )?)
}
fn authority(c: &Config, f: &Flags<'_>) -> Result<Operator, Error> {
    let actor = c
        .setup_actor
        .as_deref()
        .ok_or("private config has no setupActor")?;
    if f.get("--actor").is_some_and(|a| *a != actor) {
        return Err("actor differs from current private operator policy".into());
    }
    Ok(Operator::from_policy(
        &c.workspace,
        actor,
        &c.operator_actors,
    )?)
}
fn initialize(f: &Flags<'_>, local: bool, root: &Path, path: &Path) -> Result<Value, Error> {
    if !local {
        required(f, "--company")?;
    }
    let s = settings(f, local)?;
    if !path.try_exists()? {
        generate(path, root, &s, required(f, "--actor")?)?;
    }
    let c = config(path)?;
    if c.root != root || c.company_slug != s.slug {
        return Err("workspace/company differs from private runtime config".into());
    }
    let operator = authority(&c, f)?;
    let (mut owner, _) = rentemester_host::Owner::create_new_with_profile(
        root,
        c.broker_uid,
        &c.workspace,
        &s.slug,
        &s.name,
        &chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339(),
        c.runtime_profile,
    )?;
    let result = owner.initialize_original716(&operator, &s)?;
    owner
        .try_close()
        .map_err(|_| "initialized owner close pending")?;
    let p = &result["profile"];
    Ok(
        json!({"ok":true,"message":format!("Rentemester-virksomhed oprettet i {}",root.join(&s.slug).display()),"companyRoot":root.join(&s.slug),"ledger":root.join(&s.slug).join("data/ledger.sqlite"),"accountCount":result["accountCount"],"cvr":p["cvr"],"fiscalYearStartMonth":p["fiscalYearStartMonth"],"fiscalYearLabelStrategy":p["fiscalYearLabelStrategy"],"vatPeriod":p["vatPeriodType"],"hasPaymentDetails":!p["payment"].is_null(),"workspace":root,"workspaceRegistered":true,"workspaceSlug":s.slug,"runtimeConfig":path}),
    )
}
fn bootstrap(f: &Flags<'_>, root: &Path, path: &Path) -> Result<Value, Error> {
    let c = config(path)?;
    if c.root != root || required(f, "--company")? != c.company_slug {
        return Err("initial company is not the selected configured workspace company".into());
    }
    let operator = authority(&c, f)?;
    let password = rentemester_runtime::original716::read_bootstrap_password(Path::new(required(
        f,
        "--password-file",
    )?))?;
    let origin = f
        .get("--origin")
        .copied()
        .unwrap_or("http://127.0.0.1:4319");
    let profile = crate::backend::cookie_profile(origin)?;
    let cookies = CookieVerifier::new(&c.workspace, profile, &c.cookie_key)?;
    rentemester_runtime::mfa::MfaKeyring::new(&[(1, &c.mfa_key)], None)?;
    let signer = EmailTokenSigner::new(&c.workspace, &c.cookie_key)?;
    let links = RecoveryLinks::new(origin, &[origin], profile)?;
    let key = ReservationKey::new(&c.workspace, &c.cookie_key)?;
    let verification = rentemester_application::email_verification::Runtime {
        signer: &signer,
        clock: &SystemClock,
        audit_key_version: 1,
    };
    let request = rentemester_application::verification_request::Runtime {
        verification: &verification,
        cookies: &cookies,
        links: &links,
    };
    let runtime = rentemester_application::bootstrap::Runtime {
        reservation_key: &key,
        verification: &request,
    };
    let mut delivery = crate::auth_delivery::Delivery::new(
        c.auth_mail.as_ref(),
        profile == CookieProfile::Development,
        &root.join(".rentemester"),
    )?;
    let mut owner = owner(&c)?;
    let attempt = owner.bootstrap_first_identity(
        &operator,
        &runtime,
        &mut delivery,
        rentemester_application::bootstrap::Input {
            email: required(f, "--email")?,
            name: required(f, "--name")?,
            password: &password,
        },
    )?;
    let value = match attempt.outcome {
        Ok(r) => {
            json!({"ok":true,"userId":r.user_id,"reservationStatus":r.phase,"workspaceRole":"workspace_owner","companyRole":"owner","companySlug":r.company_slug,"verificationNextStep":"check_verification_email"})
        }
        Err(e) => {
            json!({"ok":false,"errors":[e.error.code],"lastCommittedPhase":e.last_committed_phase,"commitOutcomeUnknown":e.commit_outcome_unknown,"ownerRetired":attempt.retired})
        }
    };
    owner.try_close().map_err(
        |_| "bootstrap owner close pending; inspect committed phase, do not retry automatically",
    )?;
    Ok(value)
}
async fn serve(f: &Flags<'_>, local: bool, root: &Path, path: &Path) -> Result<Value, Error> {
    let c = config(path)?;
    if c.root != root {
        return Err("workspace differs from private runtime config".into());
    }
    let host = if local {
        "127.0.0.1".to_owned()
    } else {
        f.get("--host")
            .map(|v| v.to_string())
            .or_else(|| std::env::var("RENTEMESTER_APP_HOST").ok())
            .unwrap_or_else(|| "127.0.0.1".into())
    };
    let ip: std::net::IpAddr = if host == "localhost" {
        "127.0.0.1".parse()?
    } else {
        host.parse()
            .map_err(|_| "--host must be localhost or a literal IP address")?
    };
    let port = f
        .get("--port")
        .map(|v| v.to_string())
        .or_else(|| std::env::var("RENTEMESTER_APP_PORT").ok())
        .unwrap_or_else(|| "4319".into())
        .parse::<u16>()
        .map_err(|_| "--port must be an integer from 1 to 65535")?;
    if port == 0 {
        return Err("--port must be an integer from 1 to 65535".into());
    }
    let address = std::net::SocketAddr::new(ip, port);
    let origin = f
        .get("--origin")
        .map(|v| v.to_string())
        .unwrap_or_else(|| format!("http://{address}"));
    let origin = crate::canonical_origin(&origin)?;
    if local && !ip.is_loopback() {
        return Err("local start requires loopback".into());
    }
    // Existing Backend owns the single physical writer through HTTP drain. No
    // preflight alias owner and no close/reopen gap for company-count admission.
    let config_path = path.to_owned();
    let setup_origin = origin.clone();
    let native = tokio::task::spawn_blocking(move || {
        crate::backend::Backend::load(&config_path, &setup_origin, false, ip.is_loopback())
            .map_err(|e| e.to_string())
    })
    .await?
    .map_err(std::io::Error::other)?;
    if local {
        let mut owner = native.owner.lock().map_err(|_| "owner unavailable")?;
        let active = owner
            .catalog()?
            .companies
            .into_iter()
            .filter(|c| !c.archived)
            .collect::<Vec<_>>();
        if active.len() != 1 {
            return Ok(
                json!({"ok":false,"errors":["local start requires exactly one active company; use 'rentemester serve --workspace <dir>' for multi-company or hosted operation"],"workspace":root,"activeCompanyCount":active.len()}),
            );
        }
    }
    let web = std::env::current_exe()?
        .parent()
        .ok_or("server executable parent unavailable")?
        .join("app/dist");
    // A packaged UI can be selected explicitly; never invoke a build tool.
    let web = f.get("--web-dir").map(PathBuf::from).unwrap_or(web);
    if !web.join("index.html").is_file() {
        return Err("existing React cockpit build missing; pass --web-dir <built-app-dist>".into());
    }
    let listener = tokio::net::TcpListener::bind(address).await?;
    let identity = native
        .identity(&origin)
        .map_err(|_| "invalid identity HTTP configuration")?
        .merge(crate::product_http::router(native.clone(), &origin))
        .merge(crate::setup_http::router(native.clone(), &origin));
    let app = crate::cockpit_router(web, Some(identity), Some(native.clone()), None);
    let company = if local {
        let mut owner = native.owner.lock().map_err(|_| "owner unavailable")?;
        let company = owner
            .catalog()?
            .companies
            .into_iter()
            .find(|c| !c.archived)
            .ok_or("active company unavailable")?;
        json!({"slug":company.slug,"name":company.name})
    } else {
        Value::Null
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&if local {
            json!({"ok":true,"message":format!("Local cockpit listening on {origin}"),"mode":"local-single-company","url":origin,"dataLocation":root,"company":company,"host":host,"port":port,"authRequired":true,"deploymentProfile":"local","browserOpenAttempted":!f.contains_key("--no-open")})
        } else {
            json!({"ok":true,"message":format!("Cockpit backend listening on {origin}"),"url":origin,"host":host,"port":port,"workspace":root,"authRequired":true,"deploymentProfile":if ip.is_loopback(){"local"}else{"hosted"}})
        })?
    );
    if local && !f.contains_key("--no-open") {
        #[cfg(target_os = "macos")]
        let opener = "/usr/bin/open";
        #[cfg(target_os = "linux")]
        let opener = "xdg-open";
        let _ = Command::new(opener)
            .arg(&origin)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    let mut shutdown = crate::shutdown::Shutdown::new()?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown.wait().await;
    })
    .await?;
    // native retained until all requests are drained.
    drop(native);
    Ok(json!({"ok":true,"stopped":true}))
}
/// Finalwire calls before main's normal flag parser. None means not our entrypoint.
/// `--original716-operator` is private executable dispatch, not an HTTP route.
pub async fn execute(args: &[String]) -> Option<i32> {
    if args.first().map(String::as_str) != Some("--original716-operator") {
        return None;
    }
    let args = &args[1..];
    let local = args.first().map(String::as_str) == Some("local")
        && args.get(1).map(String::as_str) == Some("start");
    let boot = args.first().map(String::as_str) == Some("workspace-access")
        && args.get(1).map(String::as_str) == Some("bootstrap-first");
    let init = args.first().map(String::as_str) == Some("init");
    let serving = args.first().map(String::as_str) == Some("serve");
    if !local && !boot && !init && !serving {
        eprintln!("unknown original716 operator command");
        return Some(2);
    }
    let f = match flags(args, if local || boot { 2 } else { 1 }) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{e}");
            return Some(2);
        }
    };
    for k in f.keys() {
        let common = matches!(
            *k,
            "--workspace" | "--runtime-config" | "--actor" | "--json" | "--format" | "--help"
        );
        let allowed = if init {
            matches!(
                *k,
                "--company"
                    | "--name"
                    | "--cvr"
                    | "--address"
                    | "--postal-code"
                    | "--city"
                    | "--payment-terms"
                    | "--bank-name"
                    | "--bank-reg"
                    | "--bank-account"
                    | "--iban"
                    | "--fiscal-year-start-month"
                    | "--fiscal-year-label-strategy"
                    | "--vat-period"
                    | "--no-vat"
            )
        } else if local {
            matches!(
                *k,
                "--port" | "--web-dir" | "--company-name" | "--company" | "--confirm" | "--no-open"
            )
        } else if boot {
            matches!(
                *k,
                "--company" | "--name" | "--email" | "--password-file" | "--confirm" | "--origin"
            )
        } else {
            matches!(*k, "--host" | "--port" | "--origin" | "--web-dir")
        };
        if !common && !allowed {
            eprintln!("Unknown flag {k}");
            return Some(2);
        }
    }
    if f.get("--format")
        .is_some_and(|s| !matches!(*s, "json" | "human"))
    {
        eprintln!("--format must be either json or human");
        return Some(2);
    }
    if local && !f.contains_key("--workspace") {
        eprintln!("local start requires --workspace <dir>");
        return Some(2);
    }
    if f.contains_key("--help") {
        println!(
            "{} --workspace <absolute-dir> [--runtime-config <PRIVATE.json>]",
            args[..if local || boot { 2 } else { 1 }].join(" ")
        );
        return Some(0);
    }
    if (local && f.contains_key("--company-name") || boot)
        && f.get("--confirm")
            .is_none_or(|s| !s.trim().eq_ignore_ascii_case("yes"))
    {
        println!(
            "{}",
            json!({"ok":false,"errors":[if boot{"--confirm yes required to bootstrap the first workspace identity"}else{"local start creates a workspace and company; pass --confirm yes to continue"}]})
        );
        return Some(1);
    }
    rentemester_runtime::original716::bootstrap::private_process();
    let result: Result<Value, Error> = async {
        let root = workspace(&f)?;
        let path = f
            .get("--runtime-config")
            .map(PathBuf::from)
            .map(Ok)
            .unwrap_or_else(|| default_config(&root))?;
        if init {
            return initialize(&f, false, &root, &path);
        }
        if boot {
            return bootstrap(&f, &root, &path);
        }
        if local && !root.try_exists()? {
            initialize(&f, true, &root, &path)?;
        } else if local && f.contains_key("--company-name") {
            return Err("--company-name is only valid when creating a fresh workspace".into());
        }
        serve(&f, local, &root, &path).await
    }
    .await;
    Some(match result {
        Ok(value) => {
            let exit = if value["ok"] == true { 0 } else { 1 };
            if value.get("stopped").is_none() {
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
            }
            if init && value["hasPaymentDetails"] == false {
                eprintln!(
                    "ADVARSEL — ingen betalingsoplysninger:\n  Du har ikke angivet bank/IBAN. Fakturaer udstedt nu får INGEN\n  betalingsanvisning (BETALING-blok) på PDF'en — kunden kan ikke se,\n  hvor pengene skal hen. Sæt dem med 'rentemester company set-profile'\n  (--bank-name --bank-reg --bank-account --iban), før du sender fakturaer."
                );
            }
            exit
        }
        Err(e) => {
            if boot {
                println!(
                    "{}",
                    json!({"ok":false,"errors":["workspace bootstrap could not be completed; correct the configuration and inspect committed phase before retrying the same identity"]})
                );
                1
            } else {
                eprintln!("{e}");
                2
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn original716_settings_match_common_validation_before_creation() {
        for a in [
            args(&[
                "init",
                "--company=synthetic",
                "--fiscal-year-start-month=13",
            ]),
            args(&["init", "--company=synthetic", "--payment-terms=-1"]),
            args(&[
                "init",
                "--company=synthetic",
                "--no-vat",
                "--vat-period=quarter",
            ]),
            args(&["init", "--company=../escape"]),
        ] {
            assert!(settings(&flags(&a, 1).unwrap(), false).is_err());
        }
        let a = args(&[
            "init",
            "--company=synthetic",
            "--name=Synthetic",
            "--no-vat",
            "--address=Synthetic street",
            "--fiscal-year-start-month=4",
        ]);
        let s = settings(&flags(&a, 1).unwrap(), false).unwrap();
        assert_eq!(s.month, 4);
        assert_eq!(s.profile["vatPeriodType"], Value::Null);
    }
    #[tokio::test]
    async fn original716_cancellation_has_no_workspace_or_config_effects() {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("original716-cancel-{}", std::process::id()));
        let a = vec![
            "--original716-operator".into(),
            "local".into(),
            "start".into(),
            format!("--workspace={}", root.display()),
            "--company-name=Synthetic".into(),
            "--actor=user:synthetic".into(),
        ];
        assert_eq!(execute(&a).await, Some(1));
        assert!(!root.exists());
        assert!(!default_config(&root).unwrap().exists());
    }
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn original716_real_owner_fresh_flow_bootstrap_conflict_reopen() {
        use std::os::unix::fs::DirBuilderExt;
        rentemester_runtime::original716::bootstrap::private_process();
        let base = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "original716-flow-{}-{}",
            std::process::id(),
            rentemester_runtime::auth::new_token().unwrap().as_str()
        ));
        fs::DirBuilder::new().mode(0o700).create(&base).unwrap();
        let root = base.join("workspace");
        let path = default_config(&root).unwrap();
        let a = vec![
            "init".into(),
            format!("--workspace={}", root.display()),
            "--company=synthetic".into(),
            "--name=Synthetic".into(),
            "--actor=user:synthetic".into(),
            "--no-vat".into(),
        ];
        let f = flags(&a, 1).unwrap();
        let result = initialize(&f, false, &root, &path).unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(result["accountCount"], 48);
        assert!(initialize(&f, false, &root, &path).is_err());
        let c = config(&path).unwrap();
        let held = owner(&c).unwrap();
        assert!(owner(&c).is_err());
        held.try_close().unwrap();
        let password_path = base.join("password");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&password_path)
            .unwrap();
        file.write_all(b"Synthetic-password-716!").unwrap();
        drop(file);
        let b = vec![
            "workspace-access".into(),
            "bootstrap-first".into(),
            "--company=synthetic".into(),
            "--name=Synthetic Owner".into(),
            "--email=synthetic@example.invalid".into(),
            format!("--password-file={}", password_path.display()),
            "--actor=user:synthetic".into(),
        ];
        let f = flags(&b, 2).unwrap();
        let result = bootstrap(&f, &root, &path).unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(result["reservationStatus"], "access_completed");
        let repeat = bootstrap(&f, &root, &path).unwrap();
        assert_eq!(repeat["userId"], result["userId"]);
        let mut reopened = owner(&c).unwrap();
        assert_eq!(reopened.catalog().unwrap().companies.len(), 1);
        reopened.try_close().unwrap();
        fs::remove_dir_all(&base).unwrap();
    }
}
