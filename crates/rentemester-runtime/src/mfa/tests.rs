use super::*;
use crate::auth::AuthError;
use serde_json::{Value, json};
use std::io::Read;
struct FixedClock(i64);
impl Clock for FixedClock {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.0)
    }
}
struct BrokenClock;
impl Clock for BrokenClock {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Err(AuthError::InvalidClock)
    }
}
fn corpus() -> Value {
    serde_json::from_str(include_str!("../../fixtures/mfa.json")).unwrap()
}
#[test]
fn independent_libsodium_vectors_and_strict_utf8() {
    for v in corpus()["aead"].as_array().unwrap() {
        let key = v["key"].as_str().unwrap();
        let ring = MfaKeyring::new(&[(7, key)], Some(key)).unwrap();
        let result = ring.decrypt(v["envelope"].as_str().unwrap());
        if v["validUtf8"] == true {
            let text = v["plaintext"].as_str().unwrap();
            assert_eq!(result.unwrap().as_str(), v["decoded"].as_str().unwrap());
            let bytes = v["nonce"].as_str().unwrap().as_bytes();
            let mut nonce = [0u8; 24];
            for (i, b) in bytes.chunks_exact(2).enumerate() {
                nonce[i] = u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap();
            }
            assert_eq!(
                ring.encrypt_with_nonce(text, &nonce).unwrap().as_str(),
                v["envelope"].as_str().unwrap()
            );
            assert_eq!(
                ring.decrypt(v["raw"].as_str().unwrap()).unwrap().as_str(),
                v["decoded"].as_str().unwrap()
            );
        } else {
            assert!(matches!(result, Err(MfaError::InvalidCredentials)));
        }
    }
}
#[test]
fn rfc_hotp_totp_and_trusted_clock_refusals() {
    let data = corpus();
    let secret = data["rfcSecret"].as_str().unwrap();
    for v in data["hotp"].as_array().unwrap() {
        assert_eq!(
            hotp(secret.as_bytes(), v["counter"].as_u64().unwrap()).as_slice(),
            v["code"].as_str().unwrap().as_bytes()
        );
    }
    for v in data["totp"].as_array().unwrap() {
        assert_eq!(
            verify_totp(
                secret,
                v["code"].as_str().unwrap(),
                &FixedClock(v["nowMs"].as_i64().unwrap())
            )
            .unwrap(),
            v["counter"].as_u64().unwrap()
        );
    }
    for now in [-1, MAX_UNIX_MS + 1] {
        assert_eq!(
            verify_totp(secret, "123456", &FixedClock(now)),
            Err(MfaError::InvalidClock)
        );
    }
    assert_eq!(
        verify_totp(secret, "123456", &BrokenClock),
        Err(MfaError::InvalidClock)
    );
    for code in ["", "12345", "1234567", " 123456", "１２３４５６", "12345\n"] {
        assert_eq!(
            verify_totp(secret, code, &FixedClock(0)),
            Err(MfaError::InvalidCredentials)
        );
    }
    assert_eq!(verify_totp(secret, "755224", &FixedClock(0)).unwrap(), 0);
    let wrapped = String::from_utf8(hotp(secret.as_bytes(), u64::MAX).to_vec()).unwrap();
    assert_eq!(
        verify_totp(secret, &wrapped, &FixedClock(0)),
        Err(MfaError::InvalidCredentials)
    );
}
#[test]
fn config_and_envelope_grammar_are_fail_closed() {
    let data = corpus();
    let key = data["aead"][0]["key"].as_str().unwrap();
    for keys in [
        vec![],
        vec![(7, key), (7, key)],
        vec![(MAX_VERSION + 1, key)],
        vec![(1, "not-a-key")],
        vec![(1, "x")],
    ] {
        assert!(matches!(
            MfaKeyring::new(&keys, None),
            Err(MfaError::InvalidConfig)
        ));
    }
    let ring = MfaKeyring::new(&[(7, key)], None).unwrap();
    let raw = data["aead"][1]["raw"].as_str().unwrap();
    for value in [
        raw.to_owned(),
        format!("$ba$07${raw}"),
        format!("$ba$7suffix${raw}"),
        format!("$ba$+7${raw}"),
        format!("$ba$8${raw}"),
        format!("$ba$7${}", raw.to_uppercase()),
        "$ba$7$00".into(),
        "x".repeat(MAX_ENVELOPE + 1),
    ] {
        assert!(matches!(
            ring.decrypt(&value),
            Err(MfaError::InvalidCredentials)
        ));
    }
    assert!(ring.encrypt(&"x".repeat(PLAINTEXT_LIMIT + 1)).is_err());
    let text = "x".repeat(PLAINTEXT_LIMIT);
    assert_eq!(
        ring.decrypt(ring.encrypt(&text).unwrap().as_str())
            .unwrap()
            .as_str(),
        text
    );
    let a = new_totp_secret().unwrap();
    let b = new_totp_secret().unwrap();
    assert_eq!(a.as_str().len(), 32);
    assert_ne!(a.as_str(), b.as_str());
    assert!(
        a.as_str()
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
    );
}
#[test]
#[ignore = "synthetic native MFA crypto/OTP probe selected by independent harness"]
fn probe() {
    let mut input = Zeroizing::new(String::new());
    std::io::stdin().read_to_string(&mut input).unwrap();
    let request: Value = serde_json::from_str(&input).unwrap();
    let outcome = (|| -> Result<Value, MfaError> {
        if request["mode"] == "totp" {
            let secret = request["secret"].as_str().unwrap();
            let code = request["code"].as_str().unwrap();
            let result = if request["clockUnavailable"] == true {
                verify_totp(secret, code, &BrokenClock)
            } else {
                verify_totp(
                    secret,
                    code,
                    &FixedClock(request["nowMs"].as_i64().unwrap()),
                )
            };
            return result.map(|counter| json!({"ok":true,"counter":counter}));
        }
        if request["mode"] == "new-secret" {
            return Ok(json!({"ok":true,"secret":new_totp_secret()?.as_str()}));
        }
        let keys: Vec<_> = request["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (v["version"].as_u64().unwrap(), v["value"].as_str().unwrap()))
            .collect();
        let ring = MfaKeyring::new(&keys, request["legacy"].as_str())?;
        if request["mode"] == "encrypt" {
            Ok(
                json!({"ok":true,"value":ring.encrypt(request["plaintext"].as_str().unwrap())?.as_str()}),
            )
        } else {
            Ok(
                json!({"ok":true,"plaintext":ring.decrypt(request["value"].as_str().unwrap())?.as_str()}),
            )
        }
    })();
    let value = outcome.unwrap_or_else(|e| json!({"ok":false,"code":e.code()}));
    println!("RM_MFA_RESULT={value}");
}
