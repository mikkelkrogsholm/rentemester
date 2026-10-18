use super::*;
const KEY: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
struct Fixed(i64);
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.0)
    }
}
fn signer() -> EmailTokenSigner {
    EmailTokenSigner::new("w", KEY).unwrap()
}
#[test]
fn fresh_nonce_current_window_and_bound_proof() {
    let s = signer();
    let a = s.issue("u@example.invalid", &Fixed(1234)).unwrap();
    let b = s.issue("u@example.invalid", &Fixed(1234)).unwrap();
    assert!(a.as_str() != b.as_str());
    let proof = s.verify(a.as_str(), &Fixed(1000)).unwrap();
    assert!(s.accepts(&proof));
    assert_eq!(proof.issued_at_seconds(), 1);
    assert_eq!(proof.expires_at_seconds(), 3601);
    assert_eq!(proof.email(), "u@example.invalid");
    assert!(s.verify(a.as_str(), &Fixed(3_600_999)).is_ok());
    assert!(s.verify(a.as_str(), &Fixed(3_601_000)).is_err());
    assert!(s.verify(a.as_str(), &Fixed(999)).is_err());
    let other = EmailTokenSigner::new("other", KEY).unwrap();
    assert!(!other.accepts(&proof));
    assert!(other.verify(a.as_str(), &Fixed(1234)).is_err());
    let different =
        EmailTokenSigner::new("w", "AQECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8").unwrap();
    assert!(!different.accepts(&proof));
    assert!(different.verify(a.as_str(), &Fixed(1234)).is_err());
}
#[test]
fn invalid_config_clock_and_coordinate_refuse() {
    assert!(EmailTokenSigner::new("w", "bad").is_err());
    assert!(EmailTokenSigner::new(" ", KEY).is_err());
    assert!(EmailTokenSigner::new(&"w".repeat(1025), KEY).is_err());
    for email in [
        "",
        "U@example.invalid",
        "u@example.invalid\n",
        " u@example.invalid",
        "u@@example.invalid",
        "ü@example.invalid",
    ] {
        assert!(signer().issue(email, &Fixed(0)).is_err());
    }
    for t in [-1, 253_402_300_799_999, 253_402_300_800_000] {
        assert!(signer().issue("u@example.invalid", &Fixed(t)).is_err());
    }
}
#[test]
fn compact_input_is_bounded_canonical_and_authenticated() {
    let s = signer();
    let token = s.issue("u@example.invalid", &Fixed(0)).unwrap();
    for t in [
        "".to_owned(),
        "x.y.z".to_owned(),
        format!("{}.", token.as_str()),
        format!("{}=", token.as_str()),
        "x".repeat(8193),
    ] {
        assert!(s.verify(&t, &Fixed(0)).is_err());
    }
    let mut bytes = token.as_str().as_bytes().to_vec();
    let i = bytes.iter().position(|b| *b == b'.').unwrap() + 1;
    bytes[i] = if bytes[i] == b'A' { b'B' } else { b'A' };
    assert!(
        s.verify(std::str::from_utf8(&bytes).unwrap(), &Fixed(0))
            .is_err()
    );
}
#[test]
fn signed_ambiguity_and_other_purposes_are_refused() {
    let s = signer();
    let good = s.issue("u@example.invalid", &Fixed(0)).unwrap();
    let parts: Vec<_> = good.as_str().split('.').collect();
    let raw = URL_SAFE_NO_PAD.decode(parts[1]).unwrap();
    let valid: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    let sign = |header: &str, payload: &str| {
        let input = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(header),
            URL_SAFE_NO_PAD.encode(payload)
        );
        let mut mac = Hmac::<Sha256>::new_from_slice(KEY.as_bytes()).unwrap();
        mac.update(input.as_bytes());
        format!(
            "{input}.{}",
            URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
        )
    };
    for (field, value) in [
        ("exp", serde_json::json!(3601)),
        ("iat", serde_json::json!(-1)),
        ("purpose", serde_json::json!("other")),
        ("workspace", serde_json::json!("other")),
        ("jti", serde_json::json!("bad")),
        ("updateTo", serde_json::json!("other@example.invalid")),
    ] {
        let mut v = valid.clone();
        v[field] = value;
        assert!(
            s.verify(&sign(r#"{"alg":"HS256"}"#, &v.to_string()), &Fixed(0))
                .is_err()
        );
    }
    for h in [
        r#"{"alg":"none"}"#,
        r#"{"alg":"HS256","alg":"HS256"}"#,
        r#"{"alg":"HS256","kid":"1"}"#,
    ] {
        assert!(s.verify(&sign(h, &valid.to_string()), &Fixed(0)).is_err());
    }
    let duplicate =
        std::str::from_utf8(&raw)
            .unwrap()
            .replacen('{', r#"{"email":"u@example.invalid","#, 1);
    assert!(
        s.verify(&sign(r#"{"alg":"HS256"}"#, &duplicate), &Fixed(0))
            .is_err()
    );
}
#[test]
#[ignore = "synthetic private JWT protocol selected by installed-source oracle"]
fn probe() {
    use std::io::Read;
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let s = EmailTokenSigner::new(
        v["workspace"].as_str().unwrap_or("w"),
        v["key"].as_str().unwrap(),
    )
    .unwrap();
    let clock = Fixed(v["now"].as_i64().unwrap());
    let out = if v["mode"] == "issue" {
        match s.issue(v["email"].as_str().unwrap(), &clock) {
            Ok(t) => serde_json::json!({"ok":true,"token":t.as_str()}),
            Err(e) => serde_json::json!({"ok":false,"code":e.code()}),
        }
    } else {
        match s.verify(v["token"].as_str().unwrap(), &clock) {
            Ok(p) => {
                serde_json::json!({"ok":true,"email":p.email(),"nonce":p.nonce(),"iat":p.issued_at_seconds(),"exp":p.expires_at_seconds()})
            }
            Err(e) => serde_json::json!({"ok":false,"code":e.code()}),
        }
    };
    println!("RM_EMAIL_TOKEN_RESULT={out}");
}
