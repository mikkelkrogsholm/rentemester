use super::*;
#[test]
fn aliases_follow_source_grammar_and_empty_deletion() {
    assert_eq!(
        normalize_alias(Some("  Synthetic.Inbox-1  ")).unwrap(),
        Some("synthetic.inbox-1".into())
    );
    assert_eq!(normalize_alias(None).unwrap(), None);
    assert_eq!(normalize_alias(Some("")).unwrap(), None);
    for value in ["ab", "-abc", "abc-", "a@b", "æbc", "a\nb", "   "] {
        assert!(normalize_alias(Some(value)).is_err());
    }
}
#[test]
fn config_envelope_and_audit_proof_never_expose_config_secrets() {
    let config = json!({"host":"imap.example.invalid","port":993,"username":"synthetic","password":"private-test-password"});
    let runtime = imap::Config::parse(&serde_json::to_vec(&config).unwrap()).unwrap();
    let bytes=serde_json::to_vec(&json!({"format":"rentemester-imap-config/1","epoch":"synthetic-epoch","nonce":"a".repeat(43),"config":config})).unwrap();
    let grant = Grant {
        epoch: "synthetic-epoch".into(),
        file_hash: sha(&bytes),
        status: serde_json::to_value(runtime.status()).unwrap(),
        grantor: "synthetic-owner".into(),
        authority: "b".repeat(64),
        service: None,
    };
    let audit = serde_json::to_string(&grant).unwrap();
    assert!(!audit.contains("private-test-password"));
    assert!(!audit.contains("nonce"));
    let mut changed = bytes.clone();
    changed.push(b' ');
    assert_ne!(sha(&changed), grant.file_hash);
    let decoded: Envelope = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded.format, "rentemester-imap-config/1");
    let recovered: Grant = serde_json::from_str(&audit).unwrap();
    assert!(recovered == grant);
}
