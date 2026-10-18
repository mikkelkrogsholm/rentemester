//! Explicit live-process smoke test. Only point this at a fresh synthetic workspace.
use hmac::{Hmac, Mac};
use reqwest::{Client, Response};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn cookie(response: &Response) -> String {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| !v.contains("Max-Age=0"))
        .expect("issued native cookie")
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}
fn totp(uri: &str) -> String {
    let url = url::Url::parse(uri).unwrap();
    let secret = url
        .query_pairs()
        .find(|(k, _)| k == "secret")
        .unwrap()
        .1
        .into_owned();
    let mut decoded = Vec::new();
    let mut bits = 0u32;
    let mut count = 0;
    for c in secret.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'2'..=b'7' => c - b'2' + 26,
            _ => panic!("base32"),
        };
        bits = (bits << 5) | u32::from(v);
        count += 5;
        if count >= 8 {
            count -= 8;
            decoded.push((bits >> count) as u8);
        }
    }
    let counter = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / 30;
    let mut mac = Hmac::<sha1::Sha1>::new_from_slice(&decoded).unwrap();
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = usize::from(digest[19] & 15);
    let value = u32::from_be_bytes(digest[offset..offset + 4].try_into().unwrap()) & 0x7fff_ffff;
    format!("{:06}", value % 1_000_000)
}
#[tokio::test]
#[ignore = "requires a running Rust server with a fresh synthetic first owner"]
async fn first_owner_through_real_http() -> Result<(), Box<dyn std::error::Error>> {
    let origin = std::env::var("RM_WEB_ORIGIN")?;
    let spool = PathBuf::from(std::env::var("RM_WEB_SPOOL")?);
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let health: Value = serde_json::from_str(
        &client
            .get(format!("{origin}/api/health"))
            .send()
            .await?
            .text()
            .await?,
    )?;
    assert_eq!(health["backend"], "rust");
    assert_eq!(health["workspaceReady"], true);
    let html = client
        .get(format!("{origin}/companies/synthetic/saldobalance"))
        .send()
        .await?;
    assert_eq!(html.status(), 200);
    assert!(html.text().await?.contains("<html"));
    let anonymous = client
        .get(format!("{origin}/api/auth/get-session"))
        .send()
        .await?;
    assert_eq!(anonymous.status(), 200);
    assert_eq!(anonymous.text().await?, "null");
    let context = client.get(format!("{origin}/api/me")).send().await?;
    assert_eq!(context.status(), 401);
    let link = std::fs::read_dir(spool)?
        .filter_map(Result::ok)
        .find(|p| p.file_name().to_string_lossy().starts_with("verification-"))
        .ok_or("missing local verification handoff")?;
    let verified = client
        .get(std::fs::read_to_string(link.path())?)
        .send()
        .await?;
    assert!(verified.status().is_success() || verified.status().is_redirection());
    let credentials = json!({"email":"owner@example.invalid","password":"synthetic-password-123"});
    let sign_in = client
        .post(format!("{origin}/api/auth/sign-in/email"))
        .header("origin", &origin)
        .header("content-type", "application/json")
        .body(credentials.to_string())
        .send()
        .await?;
    assert_eq!(sign_in.status(), 409);
    let setup = client
        .post(format!("{origin}/api/auth/mfa-enrollment/begin"))
        .header("origin", &origin)
        .header("content-type", "application/json")
        .body(credentials.to_string())
        .send()
        .await?;
    assert_eq!(setup.status(), 200);
    let challenge = cookie(&setup);
    let setup: Value = serde_json::from_str(&setup.text().await?)?;
    let code = totp(setup["totpURI"].as_str().ok_or("missing TOTP setup")?);
    let confirm = client
        .post(format!("{origin}/api/auth/mfa-enrollment/confirm"))
        .header("origin", &origin)
        .header("cookie", challenge)
        .header("content-type", "application/json")
        .body(json!({"code":code}).to_string())
        .send()
        .await?;
    assert_eq!(confirm.status(), 200);
    let session_cookie = cookie(&confirm);
    let result: Value = serde_json::from_str(&confirm.text().await?)?;
    assert_eq!(result["backupCodes"].as_array().unwrap().len(), 10);
    let session = client
        .get(format!("{origin}/api/auth/get-session"))
        .header("cookie", &session_cookie)
        .send()
        .await?;
    assert_eq!(session.status(), 200);
    let session: Value = serde_json::from_str(&session.text().await?)?;
    assert_eq!(session["user"]["emailVerified"], true);
    assert_eq!(session["user"]["twoFactorEnabled"], true);
    let context = client
        .get(format!("{origin}/api/me"))
        .header("cookie", &session_cookie)
        .send()
        .await?;
    assert_eq!(context.status(), 200);
    let context: Value = serde_json::from_str(&context.text().await?)?;
    assert_eq!(context["workspaceRole"], "workspace_owner");
    assert_eq!(context["companies"][0]["slug"], "synthetic");
    let logout = client
        .post(format!("{origin}/api/auth/sign-out"))
        .header("origin", &origin)
        .header("cookie", &session_cookie)
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await?;
    assert_eq!(logout.status(), 200);
    let after = client
        .get(format!("{origin}/api/auth/get-session"))
        .header("cookie", &session_cookie)
        .send()
        .await?;
    assert_eq!(after.status(), 200);
    assert_eq!(after.text().await?, "null");
    println!(
        "Real Rust HTTP: cockpit, first owner, verification, MFA, session, company context and logout PASS"
    );
    Ok(())
}
