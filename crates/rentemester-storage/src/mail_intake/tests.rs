use super::*;
const NOW: &str = "2026-04-01T12:00:00.000Z";
const EML:&[u8]=b"Message-ID: <synthetic73@example.invalid>\r\nFrom: Synthetic <s@example.invalid>\r\nSubject: Synthetic mail\r\nContent-Type: multipart/mixed; boundary=p\r\n\r\n--p\r\nContent-Type: application/pdf\r\nContent-Disposition: attachment; filename=invoice.pdf\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQKU3ludGhldGljCg==\r\n--p--\r\n";
fn p() -> Provenance<'static> {
    Provenance {
        actor: "service-account:synthetic-receiver",
        now: NOW,
    }
}
fn observation(uid: u32) -> MailObservation {
    MailObservation {
        company_slug: "a".into(),
        config_epoch: "synthetic-epoch".into(),
        mailbox: "INBOX".into(),
        uid_validity: 7,
        uid,
    }
}
fn fixture() -> (Connection, Connection) {
    let mut c = Connection::open_in_memory().unwrap();
    let mut a = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut a, "Synthetic A").unwrap();
    (c, a)
}
fn attachment(tx: &Transaction<'_>) -> String {
    retain(
        tx,
        "a",
        EML,
        &rentemester_engine::mail_intake::parse(EML).unwrap(),
        &observation(1),
        p(),
    )
    .unwrap()
    .source_ids
    .into_iter()
    .find(|id| projection(tx, id).unwrap().0["role"] == "attachment")
    .unwrap()
}
fn normalized(id: &str) -> Value {
    let m:rentemester_engine::document::Metadata=serde_json::from_value(json!({"source":format!("workspace-inbox:{id}"),"documentType":"cash_register_receipt","deliveryDescription":"Explicit synthetic receipt classification"})).unwrap();
    serde_json::to_value(m).unwrap()
}
fn approve(c: &mut Connection, a: &Connection, id: &str) {
    let uuid: String = a
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let tx = c.transaction().unwrap();
    let input = ReviewInput {
        company_slug: "a".into(),
        source_hash: hash(
            &rentemester_engine::mail_intake::parse(EML)
                .unwrap()
                .attachments[0]
                .bytes,
        ),
        metadata: normalized(id),
    };
    review(
        &tx,
        "a",
        id,
        &input,
        &uuid,
        &BTreeSet::from(["a".into()]),
        Provenance {
            actor: "user:synthetic-owner",
            now: NOW,
        },
    )
    .unwrap();
    tx.commit().unwrap();
}
#[test]
fn observations_keep_raw_dedup_and_max_uid_not_last_insert() {
    let (mut c, _) = fixture();
    let tx = c.transaction().unwrap();
    let first = retain(
        &tx,
        "a",
        EML,
        &rentemester_engine::mail_intake::parse(EML).unwrap(),
        &observation(9),
        p(),
    )
    .unwrap();
    assert_eq!(first.source_ids.len(), 2);
    assert!(!first.duplicate);
    let mut o = observation(2);
    let repeat = retain(
        &tx,
        "a",
        EML,
        &rentemester_engine::mail_intake::parse(EML).unwrap(),
        &o,
        Provenance {
            actor: "user:another-observer",
            now: "2026-04-02T00:00:00.000Z",
        },
    )
    .unwrap();
    assert!(repeat.duplicate);
    assert_eq!(repeat.source_ids, first.source_ids);
    assert_eq!(
        cursor(&tx, "a", "synthetic-epoch", "INBOX").unwrap(),
        Some((7, 9))
    );
    o.uid_validity = 8;
    o.uid = 1;
    retain_refusal(&tx, "a", &o, 30 * 1024 * 1024, "MAIL_INTAKE_OVERSIZED", p()).unwrap();
    assert_eq!(
        cursor(&tx, "a", "synthetic-epoch", "INBOX").unwrap(),
        Some((8, 1))
    );
    validate_control(&tx).unwrap();
    assert_eq!(
        tx.query_row("SELECT COUNT(*) FROM rm_workspace_inbox_sources", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        3
    );
}
#[test]
fn missing_identity_and_empty_message_are_retained_nonhandoffable() {
    let (mut c, a) = fixture();
    let uuid: String = a
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let tx = c.transaction().unwrap();
    for (i, raw, code) in [
        (
            1,
            b"Subject: no id\r\n\r\nBody".as_slice(),
            "MAIL_INTAKE_MISSING_MESSAGE_ID",
        ),
        (
            2,
            b"Message-ID: <empty73@example.invalid>\r\n\r\nNo attachment".as_slice(),
            "MAIL_INTAKE_NO_ATTACHMENT",
        ),
    ] {
        let out = retain(
            &tx,
            "a",
            raw,
            &rentemester_engine::mail_intake::parse(raw).unwrap(),
            &observation(i),
            p(),
        )
        .unwrap();
        assert_eq!(out.source_ids.len(), 1);
        let id = &out.source_ids[0];
        assert_eq!(crate::workspace_inbox::original(&tx, id).unwrap(), raw);
        assert!(approved_attachment(&tx, id, "a", &uuid).is_err());
        assert_eq!(
            tx.query_row(
                "SELECT code FROM rm_workspace_inbox_exceptions WHERE source_id=?",
                [id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            code
        );
    }
    validate_control(&tx).unwrap();
}
#[test]
fn reviewed_metadata_receipt_recovery_is_atomic_and_has_no_journal_effect() {
    let (mut c, mut a) = fixture();
    let tx = c.transaction().unwrap();
    let id = attachment(&tx);
    tx.commit().unwrap();
    approve(&mut c, &a, &id);
    let uuid: String = a
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let approved = approved_attachment(&c, &id, "a", &uuid).unwrap().unwrap();
    assert!(approved_attachment(&c, &id, "a", "00000000000000000000000000000000").is_err());
    let tx = a.transaction().unwrap();
    let meta = serde_json::from_value(approved.metadata().clone()).unwrap();
    let sha = &approved.hash;
    let path = format!("documents/originals/{sha}.pdf");
    let result = crate::documents::register(
        &tx,
        crate::documents::Original {
            filename: "invoice.pdf",
            hash: sha,
            path: &path,
            mime: "application/pdf",
        },
        &meta,
        "synthetic-owner",
        "2026-04-01",
    )
    .unwrap();
    let doc = &result["document"];
    let doc_id = doc["id"].as_i64().unwrap();
    let no = doc["documentNo"].as_str().unwrap();
    assert!(
        !record_document(&tx, &approved, doc_id, no, p())
            .unwrap()
            .duplicate
    );
    assert!(
        record_document(
            &tx,
            &approved,
            doc_id,
            no,
            Provenance {
                actor: "user:restored-owner",
                now: NOW
            }
        )
        .unwrap()
        .duplicate
    );
    validate_mail_intake(&tx).unwrap();
    assert_eq!(
        tx.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    tx.commit().unwrap();
    cross_validate(&c, &a, "a").unwrap();
    // Restored current credentials never replace original receipt attribution.
    let actor: String = a
        .query_row(
            "SELECT actor FROM audit_log WHERE event_type='native_mail_document'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(actor, p().actor);
    let tx = c.transaction().unwrap();
    crate::workspace_inbox::claim(
        &tx,
        &id,
        "a",
        "synthetic-claim",
        NOW,
        "2026-04-01T12:05:00.000Z",
    )
    .unwrap();
    let input = ReviewInput {
        company_slug: "a".into(),
        source_hash: sha.clone(),
        metadata: normalized(&id),
    };
    assert!(
        review(
            &tx,
            "a",
            &id,
            &input,
            &uuid,
            &BTreeSet::from(["a".into()]),
            p()
        )
        .is_err()
    );
}
#[test]
fn rollback_does_not_reserve_message_and_tampered_original_refuses() {
    let (mut c, mut a) = fixture();
    let tx = c.transaction().unwrap();
    let id = attachment(&tx);
    tx.commit().unwrap();
    approve(&mut c, &a, &id);
    let uuid: String = a
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let approved = approved_attachment(&c, &id, "a", &uuid).unwrap().unwrap();
    let tx = a.transaction().unwrap();
    assert!(record_document(&tx, &approved, 999, "missing", p()).is_err());
    assert_eq!(
        tx.query_row("SELECT COUNT(*) FROM mail_intake_messages", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(tx);
    c.execute_batch("DROP TRIGGER IF EXISTS rm_workspace_inbox_sources_no_update;")
        .unwrap();
    let trigger:Vec<String>=c.prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name='rm_workspace_inbox_sources'").unwrap().query_map([],|r|r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
    for t in trigger {
        c.execute_batch(&format!("DROP TRIGGER {t}")).unwrap();
    }
    c.execute(
        "UPDATE rm_workspace_inbox_sources SET original_bytes=x'01' WHERE source_id=?",
        [&id],
    )
    .unwrap();
    assert!(validate_control(&c).is_err());
}
#[test]
fn legacy_upload_and_legacy_mail_are_not_native_review_authority() {
    let (mut c, _) = fixture();
    let tx = c.transaction().unwrap();
    let input:crate::workspace_inbox::Ingest=serde_json::from_value(json!({"sourceId":"legacy73","idempotencyKey":"legacy73","bytesBase64":"unused","filename":"legacy.txt","mimeType":"text/plain","transport":"upload","receivedAt":NOW,"metadata":{"documentType":"cash_register_receipt","deliveryDescription":"Synthetic legacy"},"candidates":[{"companySlug":"a","evidenceKind":"account","evidence":"explicit"}]})).unwrap();
    crate::workspace_inbox::ingest(
        &tx,
        "a",
        &input,
        b"legacy",
        "unused",
        &BTreeSet::from(["a".into()]),
        p(),
    )
    .unwrap();
    assert!(
        approved_attachment(&tx, "legacy73", "a", "unused")
            .unwrap()
            .is_none()
    );
    assert!(projection(&tx, "legacy73").unwrap().0.is_null());
    tx.commit().unwrap();
    // Existing v1 transport=mail is legacy data, never automatically a native source.
    let triggers:Vec<String>=c.prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name IN ('rm_workspace_inbox_sources','rm_workspace_inbox_events')").unwrap().query_map([],|r|r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
    for t in triggers {
        c.execute_batch(&format!("DROP TRIGGER {t}")).unwrap();
    }
    c.execute(
        "UPDATE rm_workspace_inbox_sources SET transport='mail' WHERE source_id='legacy73'",
        [],
    )
    .unwrap();
    let mut body:Value=serde_json::from_str(&c.query_row("SELECT canonical_payload FROM rm_workspace_inbox_events WHERE source_id='legacy73' AND event_type='ingested'",[],|r|r.get::<_,String>(0)).unwrap()).unwrap();
    body["transport"] = json!("mail");
    let raw = canonical(&body).unwrap();
    c.execute("UPDATE rm_workspace_inbox_events SET canonical_payload=?,payload_hash=? WHERE source_id='legacy73' AND event_type='ingested'",params![raw,hash(raw.as_bytes())]).unwrap();
    assert!(projection(&c, "legacy73").unwrap().0.is_null());
    validate_control(&c).unwrap();
}
