//! Explicit native-only session persistence. Secrets deliberately implement no Debug.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{Url, webview::Cookie};

const REFUSED: &str = "Det gemte login kunne ikke bekræftes. Log ind på ny.";
const UNAVAILABLE: &str =
    "OS-nøgleringen er utilgængelig eller låst. Intet login blev gemt eller gendannet.";

#[derive(Clone, PartialEq, Eq)]
pub struct Binding {
    pub origin: String,
    pub email: String,
}
impl Binding {
    pub fn new(origin: &Url, email: &str) -> Result<Self, String> {
        super::remote_origin(origin.as_str())?;
        let email = email.trim().to_lowercase();
        if email.is_empty()
            || email.len() > 4096
            || !email.contains('@')
            || email.chars().any(char::is_control)
        {
            return Err(REFUSED.into());
        }
        Ok(Self {
            origin: origin.origin().ascii_serialization(),
            email,
        })
    }
    fn service(&self) -> String {
        format!("dk.rentemester.desktop.session.v1:{}", self.origin)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    version: u8,
    origin: String,
    user_id: String,
    email: String,
    cookie_name: String,
    cookie_value: String,
}
impl Session {
    pub fn from_cookie(origin: &Url, cookie: &Cookie<'_>, current: &Value) -> Result<Self, String> {
        if !allowed_cookie(origin, cookie) {
            return Err(REFUSED.into());
        }
        let (user_id, email) = identity(current)?;
        let binding = Binding::new(origin, &email)?;
        Ok(Self {
            version: 1,
            origin: binding.origin,
            user_id,
            email: binding.email,
            cookie_name: cookie.name().into(),
            cookie_value: cookie.value().into(),
        })
    }
    pub fn binding(&self) -> Binding {
        Binding {
            origin: self.origin.clone(),
            email: self.email.clone(),
        }
    }
    pub fn header(&self) -> String {
        format!("{}={}", self.cookie_name, self.cookie_value)
    }
    pub fn cookie(&self) -> Result<Cookie<'static>, String> {
        let origin = super::remote_origin(&self.origin)?;
        Ok(
            Cookie::build((self.cookie_name.clone(), self.cookie_value.clone()))
                .domain(origin.host_str().ok_or(REFUSED)?.to_owned())
                .path("/")
                .http_only(true)
                .secure(true)
                .same_site(tauri::webview::cookie::SameSite::Lax)
                .build(),
        )
    }
    fn validate(&self, binding: &Binding) -> Result<(), String> {
        let origin = super::remote_origin(&binding.origin)?;
        if self.version != 1
            || self.binding() != *binding
            || self.user_id.is_empty()
            || self.user_id.len() > 4096
            || self.user_id.chars().any(char::is_control)
            || Binding::new(&origin, &self.email)? != *binding
            || !allowed_cookie(&origin, &self.cookie()?)
        {
            return Err(REFUSED.into());
        }
        Ok(())
    }
    pub async fn verify(&self) -> Result<(), String> {
        self.validate(&self.binding())?;
        let current = current(&self.origin, self.header()).await?;
        self.matches_current(&current)
    }
    fn matches_current(&self, current: &Value) -> Result<(), String> {
        let (id, email) = identity(current)?;
        if id != self.user_id || email != self.email {
            return Err(REFUSED.into());
        }
        Ok(())
    }
}
fn identity(current: &Value) -> Result<(String, String), String> {
    if current["ok"] != true
        || current["user"]["emailVerified"] != true
        || current["user"]["twoFactorEnabled"] != true
    {
        return Err(REFUSED.into());
    }
    let id = current["user"]["id"]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 4096 && !v.chars().any(char::is_control))
        .ok_or(REFUSED)?;
    let email = current["user"]["email"]
        .as_str()
        .ok_or(REFUSED)?
        .trim()
        .to_lowercase();
    Ok((id.into(), email))
}
pub async fn current(origin: &str, header: String) -> Result<Value, String> {
    rentemester_client::ApiClient::new(origin, false, Some(header))
        .map_err(|_| REFUSED.to_owned())?
        .me()
        .await
        .map_err(|_| REFUSED.into())
}
pub fn allowed_cookie(origin: &Url, cookie: &Cookie<'_>) -> bool {
    origin.scheme() == "https"
        && matches!(
            cookie.name(),
            "__Secure-better-auth.session_token" | "better-auth.session_token"
        )
        && cookie.http_only() == Some(true)
        && cookie.secure() == Some(true)
        && cookie.domain() == origin.host_str()
        && cookie.path() == Some("/")
        && !cookie.value().is_empty()
        && cookie.value().len() <= 8192 - cookie.name().len() - 1
        && cookie
            .value()
            .bytes()
            .all(|b| b.is_ascii_graphic() && !matches!(b, b';' | b',' | b'"' | b'\\'))
        && cookie
            .expires_datetime()
            .is_none_or(|expires| expires > tauri::webview::cookie::time::OffsetDateTime::now_utc())
}
pub fn select_cookie(
    origin: &Url,
    cookies: Vec<Cookie<'static>>,
) -> Result<Cookie<'static>, String> {
    let mut selected = cookies.into_iter().filter(|c| allowed_cookie(origin, c));
    let cookie = selected.next().ok_or(REFUSED)?;
    if selected.next().is_some() {
        return Err(REFUSED.into());
    }
    Ok(cookie)
}

pub trait Vault {
    fn load(&self, binding: &Binding) -> Result<Vec<u8>, String>;
    fn save(&self, binding: &Binding, blob: &[u8]) -> Result<(), String>;
    fn delete(&self, binding: &Binding) -> Result<(), String>;
}
pub struct OsVault;
#[cfg(target_os = "macos")]
fn keychain<T>(
    operation: impl FnOnce() -> Result<T, security_framework::base::Error>,
) -> Result<T, String> {
    // The API's interaction switch is process-global: serialize all our accesses,
    // preserve an already-disabled state, and never issue an unlock request.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _lock = LOCK.lock().map_err(|_| UNAVAILABLE.to_owned())?;
    use security_framework::os::macos::keychain::SecKeychain;
    let enabled = SecKeychain::user_interaction_allowed().map_err(|_| UNAVAILABLE.to_owned())?;
    let _interaction = if enabled {
        Some(SecKeychain::disable_user_interaction().map_err(|_| UNAVAILABLE.to_owned())?)
    } else {
        None
    };
    operation().map_err(|_| UNAVAILABLE.into())
}
#[cfg(target_os = "macos")]
impl OsVault {
    fn options(binding: &Binding) -> security_framework::passwords::PasswordOptions {
        let mut options = security_framework::passwords::PasswordOptions::new_generic_password(
            &binding.service(),
            &binding.email,
        );
        options.set_access_synchronized(Some(false));
        options
    }
}
impl Vault for OsVault {
    fn load(&self, binding: &Binding) -> Result<Vec<u8>, String> {
        #[cfg(target_os = "macos")]
        {
            keychain(|| security_framework::passwords::generic_password(Self::options(binding)))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = binding;
            Err(UNAVAILABLE.into())
        }
    }
    fn save(&self, binding: &Binding, blob: &[u8]) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            keychain(|| {
                security_framework::passwords::set_generic_password_options(
                    blob,
                    Self::options(binding),
                )
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (binding, blob);
            Err(UNAVAILABLE.into())
        }
    }
    fn delete(&self, binding: &Binding) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            keychain(|| {
                match security_framework::passwords::delete_generic_password_options(Self::options(
                    binding,
                )) {
                    Err(e) if e.code() == -25300 => Ok(()),
                    result => result,
                }
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = binding;
            Err(UNAVAILABLE.into())
        }
    }
}
pub fn save(vault: &impl Vault, session: &Session) -> Result<(), String> {
    session.validate(&session.binding())?;
    let blob = serde_json::to_vec(session).map_err(|_| REFUSED.to_owned())?;
    vault.save(&session.binding(), &blob)
}
pub fn load(vault: &impl Vault, binding: &Binding) -> Result<Session, String> {
    let blob = vault.load(binding)?;
    let session = if blob.len() <= 32768 {
        serde_json::from_slice::<Session>(&blob).ok()
    } else {
        None
    };
    match session {
        Some(session) if session.validate(binding).is_ok() => Ok(session),
        _ => {
            vault.delete(binding)?;
            Err(REFUSED.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[derive(Default)]
    struct MockVault(RefCell<std::collections::HashMap<String, Vec<u8>>>);
    impl MockVault {
        fn key(b: &Binding) -> String {
            format!("{}\n{}", b.service(), b.email)
        }
    }
    impl Vault for MockVault {
        fn load(&self, b: &Binding) -> Result<Vec<u8>, String> {
            self.0
                .borrow()
                .get(&Self::key(b))
                .cloned()
                .ok_or(UNAVAILABLE.into())
        }
        fn save(&self, b: &Binding, blob: &[u8]) -> Result<(), String> {
            self.0.borrow_mut().insert(Self::key(b), blob.into());
            Ok(())
        }
        fn delete(&self, b: &Binding) -> Result<(), String> {
            self.0.borrow_mut().remove(&Self::key(b));
            Ok(())
        }
    }
    fn fixture() -> (Url, Cookie<'static>, Value) {
        let origin = Url::parse("https://example.invalid").unwrap();
        let cookie = Cookie::build(("__Secure-better-auth.session_token", "synthetic-secret"))
            .domain("example.invalid")
            .path("/")
            .http_only(true)
            .secure(true)
            .build();
        (
            origin,
            cookie,
            serde_json::json!({"ok":true,"user":{"id":"stable-user-1","email":" Person@Example.invalid ","emailVerified":true,"twoFactorEnabled":true}}),
        )
    }
    #[test]
    fn binding_selection_and_native_cookie_constructor() {
        let (origin, cookie, current) = fixture();
        let session = Session::from_cookie(&origin, &cookie, &current).unwrap();
        assert!(allowed_cookie(&origin, &session.cookie().unwrap()));
        assert!(select_cookie(&origin, vec![cookie.clone(), cookie.clone()]).is_err());
        for name in ["better-auth.two_factor", "better-auth.enrollment", "other"] {
            let mut bad = cookie.clone();
            bad.set_name(name);
            assert!(!allowed_cookie(&origin, &bad));
        }
        for domain in ["sub.example.invalid", "other.invalid"] {
            let mut bad = cookie.clone();
            bad.set_domain(domain);
            assert!(!allowed_cookie(&origin, &bad));
        }
        let mut bad = cookie.clone();
        bad.set_http_only(false);
        assert!(!allowed_cookie(&origin, &bad));
        assert_eq!(session.email, "person@example.invalid");
        assert!(
            Binding::new(
                &Url::parse("http://example.invalid").unwrap(),
                "p@example.invalid"
            )
            .is_err()
        );
    }
    #[test]
    fn restore_requires_positive_current_stable_identity_and_mfa() {
        let (origin, cookie, current) = fixture();
        let session = Session::from_cookie(&origin, &cookie, &current).unwrap();
        assert!(session.matches_current(&current).is_ok());
        for replacement in [
            serde_json::json!({"ok":false}),
            serde_json::json!({"ok":true}),
            serde_json::json!({"ok":true,"user":{"id":"another-stable-id","email":"person@example.invalid","emailVerified":true,"twoFactorEnabled":true}}),
            serde_json::json!({"ok":true,"user":{"id":"stable-user-1","email":"another@example.invalid","emailVerified":true,"twoFactorEnabled":true}}),
            serde_json::json!({"ok":true,"user":{"id":"stable-user-1","email":"person@example.invalid","emailVerified":true,"twoFactorEnabled":false}}),
        ] {
            assert!(session.matches_current(&replacement).is_err());
        }
    }
    #[test]
    fn exact_binding_malformed_removal_and_redacted_errors() {
        let (origin, cookie, mut current) = fixture();
        let session = Session::from_cookie(&origin, &cookie, &current).unwrap();
        let vault = MockVault::default();
        let binding = session.binding();
        save(&vault, &session).unwrap();
        assert!(load(&vault, &binding).is_ok());
        let other = Binding::new(
            &Url::parse("https://other.invalid").unwrap(),
            &binding.email,
        )
        .unwrap();
        assert!(load(&vault, &other).is_err());
        let other_account = Binding::new(&origin, "other@example.invalid").unwrap();
        assert!(load(&vault, &other_account).is_err());
        vault
            .save(&other_account, b"synthetic-secret-malformed")
            .unwrap();
        let error = load(&vault, &other_account).err().unwrap();
        assert!(!error.contains("synthetic-secret"));
        assert!(vault.load(&other_account).is_err());
        assert!(load(&vault, &binding).is_ok());
        current["ok"] = Value::Bool(false);
        assert!(Session::from_cookie(&origin, &cookie, &current).is_err());
        current["ok"] = Value::Bool(true);
        current["user"]["twoFactorEnabled"] = Value::Bool(false);
        assert!(Session::from_cookie(&origin, &cookie, &current).is_err());
        vault
            .save(&other_account, &serde_json::to_vec(&session).unwrap())
            .unwrap();
        assert!(load(&vault, &other_account).is_err());
        assert!(vault.load(&other_account).is_err());
    }
}
