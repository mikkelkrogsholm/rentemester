use super::*;
use serde_json::{Value, json};
fn key() -> SigningKey {
    SigningKey(Ed25519KeyPair::from_seed_unchecked(&[7u8; 32]).unwrap())
}
fn fixture() -> (Context, Vec<Snapshot>) {
    let ctx = Context {
        workspace_id: "synthetic-stable-workspace".into(),
        created_at: "2026-01-02T03:04:05.000Z".into(),
        control_schema_version: 30,
        companies: vec![Company {
            slug: "synthetic".into(),
            ledger_uuid: "a".repeat(32),
            schema_version: 36,
        }],
        coverage: Coverage {
            included: vec![BusinessSection::WorkspaceAudit],
            unsupported: BusinessSection::ALL
                .into_iter()
                .filter(|s| *s != BusinessSection::WorkspaceAudit)
                .map(|section| UnsupportedSection {
                    section,
                    record_count: 0,
                })
                .collect(),
            excluded: Exclusion::ALL.to_vec(),
        },
    };
    let original = b"Synthetic exact original\n".to_vec();
    let statement = b"date,text,amount\n2026-01-02,Synthetic receipt,12.34\n".to_vec();
    let entries=vec![
        Snapshot{kind:EntryKind::WorkspaceManifest,path:"workspace.json".into(),bytes:br#"{"version":1,"companies":[{"slug":"synthetic","name":"Synthetic Company","createdAt":"2026-01-01","archived":false}]}"#.to_vec()},
        Snapshot{kind:EntryKind::CompanyLedger{company:"synthetic".into()},path:"synthetic/data/ledger.sqlite".into(),bytes:b"Synthetic caller-supplied snapshot bytes; no DB is opened here".to_vec()},
        Snapshot{kind:EntryKind::DocumentOriginal{company:"synthetic".into()},path:format!("synthetic/documents/originals/{}.txt",hash(&original)),bytes:original},
        Snapshot{kind:EntryKind::BankStatement{company:"synthetic".into()},path:format!("synthetic/bank/processed/{}.txt",hash(&statement)),bytes:statement},
        Snapshot{kind:EntryKind::ControlBusiness{section:BusinessSection::WorkspaceAudit},path:"control/workspace-audit.json".into(),bytes:br#"{"version":1,"events":[{"actor":"user:historical-synthetic","event":"synthetic-only"}]}"#.to_vec()},
    ];
    (ctx, entries)
}
fn artifact() -> (SigningKey, Vec<u8>) {
    let key = key();
    let (ctx, entries) = fixture();
    let wire = create(ctx, entries, &key).unwrap();
    (key, wire)
}
#[test]
fn task_format_v2_is_complete_signed_and_v1_remains_independently_readable() {
    let key=key();let (mut ctx,mut entries)=fixture();
    ctx.coverage.included.push(BusinessSection::Tasks);
    entries.push(Snapshot{kind:EntryKind::ControlBusiness{section:BusinessSection::Tasks},path:"control/tasks.json".into(),bytes:br#"{"format":"rentemester-native-tasks/1","tables":{}}"#.to_vec()});
    let encoded=create(ctx,entries,&key).unwrap();let parsed=verify(&encoded,&trusted(&key)).unwrap();
    assert_eq!(parsed.manifest().format,FORMAT_V2);
    let legacy=artifact().1;assert_eq!(verify(&legacy,&trusted(&key)).unwrap().manifest().format,FORMAT);
    let mut envelope:Value=serde_json::from_slice(&encoded).unwrap();
    envelope["entries"].as_array_mut().unwrap().retain(|e|e["path"]!="control/tasks.json");assert!(verify(&wire(envelope),&trusted(&key)).is_err());
    let mut envelope:Value=serde_json::from_slice(&encoded).unwrap();envelope["format"]=json!(FORMAT);
    signed_manifest(&mut envelope,&key,|m|m.format=FORMAT.into());assert!(verify(&wire(envelope),&trusted(&key)).is_err());
    let mut envelope:Value=serde_json::from_slice(&encoded).unwrap();
    let entry=envelope["entries"].as_array_mut().unwrap().iter_mut().find(|e|e["path"]=="control/tasks.json").unwrap();entry["bytesBase64"]=json!(STANDARD.encode(b"tampered"));assert!(verify(&wire(envelope),&trusted(&key)).is_err());
}
fn trusted(key: &SigningKey) -> TrustedPublicKey {
    TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap()
}
fn wire(v: Value) -> Vec<u8> {
    serde_json::to_vec(&v).unwrap()
}
/// Adversarial signer produces a valid signature over deliberately invalid source metadata.
fn signed_manifest(envelope: &mut Value, key: &SigningKey, mutate: impl FnOnce(&mut Manifest)) {
    let bytes = STANDARD
        .decode(envelope["manifestBase64"].as_str().unwrap())
        .unwrap();
    let mut manifest: Manifest = serde_json::from_slice(&bytes).unwrap();
    mutate(&mut manifest);
    let bytes = serde_json::to_vec(&manifest).unwrap();
    envelope["manifestBase64"] = json!(STANDARD.encode(&bytes));
    envelope["signatureBase64"] = json!(STANDARD.encode(key.0.sign(&bytes).as_ref()));
}
#[test]
fn deterministic_roundtrip_preserves_signed_bytes_identities_coverage_and_originals() {
    let key = key();
    let (ctx, entries) = fixture();
    let originals = entries
        .iter()
        .map(|s| (s.path.clone(), s.bytes.clone()))
        .collect::<BTreeMap<_, _>>();
    let first = create(ctx, entries, &key).unwrap();
    let (mut ctx, mut entries) = fixture();
    ctx.coverage.unsupported.reverse();
    entries.reverse();
    assert_eq!(first, create(ctx, entries, &key).unwrap());
    let verified = verify(&first, &trusted(&key)).unwrap();
    assert_eq!(
        verified.manifest().context.workspace_id,
        "synthetic-stable-workspace"
    );
    assert_eq!(
        verified.manifest().context.companies[0].ledger_uuid,
        "a".repeat(32)
    );
    assert_eq!(verified.entries().len(), originals.len());
    for e in verified.entries() {
        assert_eq!(e.bytes(), originals[e.path()]);
    }
    let parsed: Manifest = serde_json::from_slice(verified.manifest_bytes()).unwrap();
    assert_eq!(parsed, *verified.manifest());
    // Signature bytes are retained/verified without reserializing the input manifest.
    let mut envelope: Value = serde_json::from_slice(&first).unwrap();
    let mut exact = verified.manifest_bytes().to_vec();
    exact.push(b'\n');
    envelope["manifestBase64"] = json!(STANDARD.encode(&exact));
    envelope["signatureBase64"] = json!(STANDARD.encode(key.0.sign(&exact).as_ref()));
    assert_eq!(
        verify(&wire(envelope), &trusted(&key))
            .unwrap()
            .manifest_bytes(),
        exact
    );
}
#[test]
fn trust_is_explicit_wrong_key_and_embedded_key_cannot_establish_it() {
    let (key, bytes) = artifact();
    let other = SigningKey(Ed25519KeyPair::from_seed_unchecked(&[8u8; 32]).unwrap());
    assert!(matches!(
        verify(&bytes, &trusted(&other)),
        Err(Error::Signature)
    ));
    let mut envelope: Value = serde_json::from_slice(&bytes).unwrap();
    envelope["publicKey"] = json!(STANDARD.encode(key.public_key_bytes()));
    assert!(matches!(
        verify(&wire(envelope), &trusted(&other)),
        Err(Error::Malformed)
    ));
    assert!(matches!(
        TrustedPublicKey::from_bytes(&[0; 31]),
        Err(Error::InvalidKey)
    ));
    assert!(matches!(
        SigningKey::from_pkcs8(b"partial or missing private key"),
        Err(Error::InvalidKey)
    ));
}
#[test]
fn caller_pkcs8_is_accepted_but_secret_never_is_part_of_the_envelope() {
    let secret = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
    let key = SigningKey::from_pkcs8(secret.as_ref()).unwrap();
    let (ctx, entries) = fixture();
    let bytes = create(ctx, entries, &key).unwrap();
    verify(&bytes, &trusted(&key)).unwrap();
    assert!(
        !bytes
            .windows(secret.as_ref().len())
            .any(|window| window == secret.as_ref())
    );
    assert!(
        !String::from_utf8(bytes)
            .unwrap()
            .contains(&STANDARD.encode(secret.as_ref()))
    );
}
#[test]
fn changed_manifest_context_signature_and_payload_refuse() {
    let (key, bytes) = artifact();
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    let mut v = original.clone();
    let mut manifest: Value = serde_json::from_slice(
        &STANDARD
            .decode(v["manifestBase64"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    manifest["context"]["workspaceId"] = json!("substituted");
    v["manifestBase64"] = json!(STANDARD.encode(wire(manifest)));
    assert!(matches!(
        verify(&wire(v), &trusted(&key)),
        Err(Error::Signature)
    ));
    let mut v = original.clone();
    v["signatureBase64"] = json!(STANDARD.encode([0u8; 64]));
    assert!(matches!(
        verify(&wire(v), &trusted(&key)),
        Err(Error::Signature)
    ));
    let mut v = original;
    v["entries"][0]["bytesBase64"] = json!(STANDARD.encode(b"changed"));
    assert!(verify(&wire(v), &trusted(&key)).is_err());
}
#[test]
fn missing_unlisted_duplicate_and_link_like_entries_refuse() {
    let (key, bytes) = artifact();
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    let mut v = original.clone();
    v["entries"].as_array_mut().unwrap().pop();
    assert!(verify(&wire(v), &trusted(&key)).is_err());
    let mut v = original.clone();
    v["entries"]
        .as_array_mut()
        .unwrap()
        .push(json!({"path":"extra.txt","bytesBase64":"QQ=="}));
    assert!(verify(&wire(v), &trusted(&key)).is_err());
    let mut v = original.clone();
    let duplicate = v["entries"][0].clone();
    v["entries"].as_array_mut().unwrap().push(duplicate);
    assert!(matches!(
        verify(&wire(v), &trusted(&key)),
        Err(Error::Duplicate)
    ));
    let mut v = original;
    v["entries"][0]["linkTarget"] = json!("synthetic/data/ledger.sqlite");
    assert!(matches!(
        verify(&wire(v), &trusted(&key)),
        Err(Error::Malformed)
    ));
}
#[test]
fn even_valid_signature_cannot_authorize_unsafe_paths_or_raw_control_and_secrets() {
    let (key, bytes) = artifact();
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    for path in [
        "../ledger.sqlite",
        "/absolute",
        "synthetic//data/ledger.sqlite",
        "synthetic/./data/ledger.sqlite",
        "synthetic/../data/ledger.sqlite",
        "C:/ledger.sqlite",
        "synthetic\\data\\ledger.sqlite",
        "synthetic/data/ledger.sqlite\0",
        "synthetic/data/%2e%2e/x",
        "synthetic/.backup-signing-key.pem",
        ".rentemester/workspace-control.sqlite",
        "config/server.json",
    ] {
        let mut v = original.clone();
        signed_manifest(&mut v, &key, |m| m.inventory[0].path = path.into());
        assert!(
            matches!(verify(&wire(v), &trusted(&key)), Err(Error::UnsafeEntry)),
            "{path}"
        );
    }
    let mut v = original;
    signed_manifest(&mut v, &key, |m| {
        m.inventory[0].kind = EntryKind::CompanyLedger {
            company: "unselected".into(),
        }
    });
    assert!(matches!(
        verify(&wire(v), &trusted(&key)),
        Err(Error::UnsafeEntry)
    ));
}
#[test]
fn signed_inventory_requires_full_ledgers_audit_and_one_to_one_payload_mapping() {
    let (key, bytes) = artifact();
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    for predicate in [0, 1, 2] {
        let mut v = original.clone();
        signed_manifest(&mut v, &key, |m| {
            m.inventory.retain(|e| match predicate {
                0 => !matches!(e.kind, EntryKind::WorkspaceManifest),
                1 => !matches!(e.kind, EntryKind::CompanyLedger { .. }),
                _ => !matches!(e.kind, EntryKind::ControlBusiness { .. }),
            })
        });
        assert!(matches!(
            verify(&wire(v), &trusted(&key)),
            Err(Error::MissingEntry)
        ));
    }
    let mut v = original.clone();
    signed_manifest(&mut v, &key, |m| {
        m.inventory.push(m.inventory[0].clone());
    });
    assert!(matches!(
        verify(&wire(v), &trusted(&key)),
        Err(Error::Duplicate)
    ));
    let mut v = original;
    signed_manifest(&mut v, &key, |m| m.inventory[0].sha256 = "0".repeat(64));
    assert!(verify(&wire(v), &trusted(&key)).is_err());
}
#[test]
fn coverage_cannot_silently_omit_business_or_include_nonempty_unsupported_sections() {
    for mutation in 0..5 {
        let (mut ctx, entries) = fixture();
        match mutation {
            0 => ctx.coverage.unsupported[0].record_count = 1,
            1 => {
                ctx.coverage.unsupported.pop();
            }
            2 => ctx.coverage.included.push(BusinessSection::WorkspaceAudit),
            3 => {
                ctx.coverage.excluded.pop();
            }
            _ => {
                ctx.coverage.included.clear();
                ctx.coverage.unsupported.push(UnsupportedSection {
                    section: BusinessSection::WorkspaceAudit,
                    record_count: 0,
                });
            }
        };
        assert!(matches!(create(ctx, entries, &key()), Err(Error::Coverage)));
    }
    let (mut ctx, mut entries) = fixture();
    ctx.coverage
        .unsupported
        .retain(|s| s.section != BusinessSection::CompanyKnowledge);
    ctx.coverage
        .included
        .push(BusinessSection::CompanyKnowledge);
    assert!(matches!(
        create(
            ctx.clone(),
            entries
                .iter()
                .map(|s| Snapshot {
                    kind: s.kind.clone(),
                    path: s.path.clone(),
                    bytes: s.bytes.clone()
                })
                .collect(),
            &key()
        ),
        Err(Error::MissingEntry)
    ));
    entries.push(Snapshot {
        kind: EntryKind::ControlBusiness {
            section: BusinessSection::CompanyKnowledge,
        },
        path: "control/company-knowledge.json".into(),
        bytes: br#"{"version":1,"assertions":[]}"#.to_vec(),
    });
    assert!(create(ctx, entries, &key()).is_ok());
}
#[test]
fn schema_identity_and_created_at_cannot_be_missing_or_ambiguous() {
    for mutation in 0..7 {
        let (mut ctx, entries) = fixture();
        match mutation {
            0 => ctx.companies.push(ctx.companies[0].clone()),
            1 => ctx.companies[0].ledger_uuid = "not-a-native-id".into(),
            2 => ctx.control_schema_version = 0,
            3 => ctx.companies[0].schema_version = 0,
            4 => ctx.created_at = "2026-01-02T03:04:05+00:00".into(),
            5 => ctx.workspace_id.clear(),
            _ => ctx.companies[0].slug = "../other".into(),
        };
        assert!(matches!(create(ctx, entries, &key()), Err(Error::Identity)));
    }
}
#[test]
fn strict_json_preserves_no_ambiguous_duplicate_or_unknown_format_fields() {
    let (key, bytes) = artifact();
    let text = String::from_utf8(bytes.clone()).unwrap();
    let duplicate = text.replacen('{', "{\"format\":\"rentemester-native-backup/1\",", 1);
    assert!(matches!(
        verify(duplicate.as_bytes(), &trusted(&key)),
        Err(Error::Malformed)
    ));
    let mut v: Value = serde_json::from_slice(&bytes).unwrap();
    v["format"] = json!("legacy-workspace-backup");
    assert!(matches!(
        verify(&wire(v), &trusted(&key)),
        Err(Error::Malformed)
    ));
    let mut v: Value = serde_json::from_slice(&bytes).unwrap();
    signed_manifest(&mut v, &key, |m| m.format = "unknown/2".into());
    assert!(matches!(
        verify(&wire(v), &trusted(&key)),
        Err(Error::Malformed)
    ));
}
#[test]
fn bounds_are_refusals_never_truncation_even_under_a_valid_signature() {
    let (key, bytes) = artifact();
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    let mut v = original.clone();
    signed_manifest(&mut v, &key, |m| {
        m.inventory[0].size_bytes = MAX_FILE_BYTES as u64 + 1
    });
    assert!(matches!(
        verify(&wire(v), &trusted(&key)),
        Err(Error::Bounds)
    ));
    let mut v = original.clone();
    v["manifestBase64"] = json!("A".repeat(MAX_MANIFEST_BYTES * 4 / 3 + 8));
    assert!(matches!(
        verify(&wire(v), &trusted(&key)),
        Err(Error::Bounds)
    ));
    let mut v = original;
    v["entries"] = json!(
        (0..MAX_ENTRIES + 1)
            .map(|_| json!({"path":"workspace.json","bytesBase64":"QQ=="}))
            .collect::<Vec<_>>()
    );
    assert!(matches!(
        verify(&wire(v), &trusted(&key)),
        Err(Error::Bounds)
    ));
    assert!(matches!(
        decode("QUJD", 2),
        Err(Error::Inventory | Error::Bounds)
    ));
    let (mut ctx, entries) = fixture();
    ctx.companies = vec![ctx.companies[0].clone(); MAX_COMPANIES + 1];
    assert!(matches!(create(ctx, entries, &key), Err(Error::Identity)));
}

#[test]
fn actual_native_issued_json_and_pdf_paths_preserve_both_originals() {
    // application/sales::issue_payload publishes BOTH through document::Prepared,
    // not the legacy invoices/issued directory. Keep this inventory contract aligned.
    let (ctx, mut entries) = fixture();
    let originals = [
        crate::document::Prepared::new(
            "synthetic-invoice.json".into(),
            br#"{"invoiceNumber":"SYN-1","status":"issued"}"#.to_vec(),
        )
        .unwrap(),
        crate::document::Prepared::new(
            "synthetic-invoice.pdf".into(),
            b"%PDF-1.4\nSynthetic issued invoice original\n".to_vec(),
        )
        .unwrap(),
    ];
    for source in &originals {
        entries.push(Snapshot {
            kind: EntryKind::DocumentOriginal {
                company: "synthetic".into(),
            },
            path: format!("synthetic/{}", source.relative_path()),
            bytes: source.bytes().to_vec(),
        });
    }
    let key = key();
    let verified = verify(&create(ctx, entries, &key).unwrap(), &trusted(&key)).unwrap();
    for source in originals {
        let expected = format!("synthetic/{}", source.relative_path());
        let restored = verified
            .entries()
            .iter()
            .find(|entry| entry.path() == expected)
            .unwrap();
        assert_eq!(restored.bytes(), source.bytes());
        assert!(matches!(
            restored.kind(),
            EntryKind::DocumentOriginal { .. }
        ));
    }
}
