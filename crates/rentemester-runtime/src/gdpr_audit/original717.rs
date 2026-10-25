//! TypeScript v1 wire bytes: insertion order, UTF-8 SHA256 and detached Ed25519.
use super::*;
use serde::Serialize;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Event<'a> {
    id: &'a Value,
    occurred_at: &'a Value,
    event_type: &'a Value,
    subject_key: &'a Value,
    actor: &'a Value,
    message: &'a Value,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Payload<'a> {
    format: &'static str,
    rule_id: &'static str,
    as_of: &'a Value,
    since: &'a Value,
    until: &'a Value,
    events: Vec<Event<'a>>,
}
pub fn seal(input: &Value, sign: bool, key: Option<&SigningKey>) -> Result<Value, &'static str> {
    let events = input["events"]
        .as_array()
        .ok_or("audit events missing")?
        .iter()
        .map(|e| Event {
            id: &e["id"],
            occurred_at: &e["occurredAt"],
            event_type: &e["eventType"],
            subject_key: &e["subjectKey"],
            actor: &e["actor"],
            message: &e["message"],
        })
        .collect();
    let payload = Payload {
        format: "rentemester.gdpr-audit.v1",
        rule_id: "GDPR-AUDIT-LOG",
        as_of: &input["asOf"],
        since: &input["since"],
        until: &input["until"],
        events,
    };
    let bytes = serde_json::to_string(&payload).map_err(|_| "audit serialization refused")?;
    let mut result = input.clone();
    result["format"] = json!(payload.format);
    result["ruleId"] = json!(payload.rule_id);
    result["canonicalPayload"] = json!(bytes);
    result["fingerprint"] = json!(format!("sha256:{:x}", Sha256::digest(bytes.as_bytes())));
    if sign && input["ok"] == true {
        if let Some(key) = key {
            // RFC 8410 SubjectPublicKeyInfo DER, matching node publicKey.export(spki).
            let mut spki = vec![
                0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
            ];
            spki.extend(key.public_key_bytes());
            result["signature"] = json!({"algorithm":"ed25519","encoding":"utf8","signedField":"canonicalPayload","publicKeyHint":format!("sha256:{:x}",Sha256::digest(spki)),"base64":STANDARD.encode(key.sign_detached(bytes.as_bytes()))});
        } else {
            result["ok"] = json!(false);
            result["errors"] = json!([
                "no ed25519 backup signing key is configured; run system backup with --sign-with-ed25519 once"
            ]);
        }
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original717_privacy_canonical_unsigned() {
        let i = json!({"ok":true,"asOf":"2026-01-01","since":null,"until":null,"events":[],"errors":[]});
        let r = seal(&i, false, None).unwrap();
        assert_eq!(
            r["canonicalPayload"],
            r#"{"format":"rentemester.gdpr-audit.v1","ruleId":"GDPR-AUDIT-LOG","asOf":"2026-01-01","since":null,"until":null,"events":[]}"#
        );
        assert!(r.get("signature").is_none());
        assert_eq!(seal(&i, true, None).unwrap()["ok"], false);
    }
}

#[cfg(test)]
mod signing_tests {
    use super::*;
    #[test]
    fn original717_privacy_signs_exact_source_bytes() {
        let hex = concat!(
            "3051020101300506032b657004220420",
            "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
            "812100",
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
        );
        let der = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect::<Vec<_>>();
        let key = SigningKey::from_pkcs8(&der).unwrap();
        let input = json!({"ok":true,"asOf":"2026-07-18","since":"2026-07-18","until":"2026-07-18","events":[{"id":1,"occurredAt":"2026-07-18 12:00:00","eventType":"gdpr_export","subjectKey":"sha256:synthetic","actor":"user:synthetic","message":"Synthetic export"}],"errors":[]});
        let result = seal(&input, true, Some(&key)).unwrap();
        let bytes = result["canonicalPayload"].as_str().unwrap().as_bytes();
        let sig = STANDARD
            .decode(result["signature"]["base64"].as_str().unwrap())
            .unwrap();
        TrustedPublicKey::from_bytes(&key.public_key_bytes())
            .unwrap()
            .verify_detached(bytes, &sig)
            .unwrap();
        assert_eq!(
            result["fingerprint"],
            format!("sha256:{:x}", Sha256::digest(bytes))
        );
        assert_eq!(result["signature"]["signedField"], "canonicalPayload");
    }
}
