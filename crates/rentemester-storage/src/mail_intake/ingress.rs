use super::*;
fn observation(o: &MailObservation, anchor: &str) -> Result<(), Error> {
    ensure(
        o.company_slug == anchor && o.uid > 0 && o.uid_validity > 0,
        "mail observation company/UID differs",
    )?;
    for s in [&o.company_slug, &o.config_epoch, &o.mailbox] {
        ensure(
            !s.trim().is_empty() && s.len() <= 512 && !s.chars().any(char::is_control),
            "mail observation text invalid",
        )?
    }
    Ok(())
}
fn insert(
    tx: &Transaction<'_>,
    anchor: &str,
    key: &str,
    bytes: &[u8],
    filename: &str,
    mime: &str,
    meta: Value,
    p: &Provenance<'_>,
) -> Result<(String, bool), Error> {
    let id = format!("mail-{}", hash(key.as_bytes()));
    if let Some(prior) = tx
        .query_row(
            "SELECT source_id FROM rm_workspace_inbox_sources WHERE idempotency_key=?",
            [key],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(sql)?
    {
        let (old, b, _, a) = source(tx, &prior)?;
        ensure(
            a == anchor && b == bytes && old == meta,
            "mail stable identity conflicts with retained original",
        )?;
        return Ok((prior, true));
    }
    let sha = hash(bytes);
    let identity = format!("mail:{}", hash(key.as_bytes()));
    let metadata = canonical(&json!({"nativeMail":meta}))?;
    tx.execute("INSERT INTO rm_workspace_inbox_sources(source_id,visibility_anchor_slug,idempotency_key,original_bytes,sha256,filename,mime_type,transport,transport_identity,received_at,metadata_json,created_by,created_at)VALUES(?,?,?,?,?,?,?,'mail',?,?,?,?,?)",params![id,anchor,key,bytes,sha,filename,mime,identity,p.now,metadata,p.actor,p.now]).map_err(sql)?;
    event(
        tx,
        &id,
        "ingested",
        json!({"sourceHash":sha,"filename":filename,"mimeType":mime,"transport":"mail","transportIdentity":identity}),
        p.actor,
        p.now,
    )?;
    event(
        tx,
        &id,
        "candidate_resolved",
        json!({"candidates":[{"companySlug":anchor,"evidenceKind":"account","evidence":"explicit configured mailbox company"}]}),
        p.actor,
        p.now,
    )?;
    Ok((id, false))
}
fn observed(
    tx: &Transaction<'_>,
    id: &str,
    o: &MailObservation,
    eml_id: Option<&str>,
    eml_sha: Option<&str>,
    message_id: Option<&str>,
    p: &Provenance<'_>,
) -> Result<bool, Error> {
    let payload = json!({"version":1,"observation":o,"emlSourceId":eml_id,"emlSha256":eml_sha,"messageId":message_id});
    let h = hash(canonical(&payload)?.as_bytes());
    let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM rm_workspace_inbox_events WHERE source_id=? AND event_type='mail_observed' AND payload_hash=?)",params![id,h],|r|r.get(0)).map_err(sql)?;
    if !exists {
        event(tx, id, "mail_observed", payload, p.actor, p.now)?
    }
    Ok(exists)
}
fn exception(tx: &Transaction<'_>, id: &str, code: &str, p: &Provenance<'_>) -> Result<(), Error> {
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM rm_workspace_inbox_exceptions WHERE source_id=?)",
            [id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if !exists {
        tx.execute("INSERT INTO rm_workspace_inbox_exceptions(source_id,code,required_action,opened_at)VALUES(?,?,'review_mail',?)",params![id,code,p.now]).map_err(sql)?;
        event(
            tx,
            id,
            "exception_opened",
            json!({"code":code,"candidates":[{"companySlug":source(tx,id)?.3,"evidenceKind":"account","evidence":"explicit configured mailbox company"}],"requiredAction":"review_mail"}),
            p.actor,
            p.now,
        )?;
    }
    Ok(())
}
pub fn retain(
    tx: &Transaction<'_>,
    anchor: &str,
    raw: &[u8],
    parsed: &ParsedMail,
    o: &MailObservation,
    p: Provenance<'_>,
) -> Result<RetainedMail, Error> {
    observation(o, anchor)?;
    let actual = rentemester_engine::mail_intake::parse(raw)
        .map_err(|_| refused("mail parse result unavailable"))?;
    ensure(
        &actual == parsed,
        "mail parser output differs from exact original",
    )?;
    let sha = hash(raw);
    let eml_meta = json!({"version":VERSION,"role":"eml","messageId":parsed.message_id,"sender":parsed.sender,"subject":parsed.subject,"date":parsed.date});
    let key = format!("mail-eml:{}:{sha}", hash(anchor.as_bytes()));
    let (eml, eml_existing) = insert(
        tx,
        anchor,
        &key,
        raw,
        "message.eml",
        "message/rfc822",
        eml_meta,
        &p,
    )?;
    let mut duplicate = true;
    observed(
        tx,
        &eml,
        o,
        Some(&eml),
        Some(&sha),
        parsed.message_id.as_deref(),
        &p,
    )?;
    let mut ids = vec![eml.clone()];
    if parsed.message_id.is_none() {
        duplicate = eml_existing;
        exception(tx, &eml, "MAIL_INTAKE_MISSING_MESSAGE_ID", &p)?
    } else if parsed.attachments.is_empty() {
        duplicate = eml_existing;
        exception(tx, &eml, "MAIL_INTAKE_NO_ATTACHMENT", &p)?
    } else {
        for a in &parsed.attachments {
            let identity = canonical(&json!([anchor, parsed.message_id, a.sha256]))?;
            let key = format!("mail-attachment:{}", hash(identity.as_bytes()));
            let prior = tx
                .query_row(
                    "SELECT source_id FROM rm_workspace_inbox_sources WHERE idempotency_key=?",
                    [&key],
                    |r| r.get::<_, String>(0),
                )
                .optional()
                .map_err(sql)?;
            let id = if let Some(id) = prior {
                let (meta, b, _, s) = source(tx, &id)?;
                ensure(
                    s == anchor
                        && b == a.bytes
                        && meta["messageId"] == json!(parsed.message_id)
                        && meta["attachmentSha256"] == a.sha256,
                    "mail attachment identity conflicts",
                )?;
                id
            } else {
                let meta = json!({"version":VERSION,"role":"attachment","messageId":parsed.message_id,"emlSourceId":eml,"emlSha256":sha,"attachmentSha256":a.sha256,"partIndex":a.part_index,"originalFilename":a.original_filename,"sender":parsed.sender,"subject":parsed.subject,"date":parsed.date});
                duplicate = false;
                insert(
                    tx,
                    anchor,
                    &key,
                    &a.bytes,
                    &a.filename,
                    &a.mime_type,
                    meta,
                    &p,
                )?
                .0
            };
            observed(
                tx,
                &id,
                o,
                Some(&eml),
                Some(&sha),
                parsed.message_id.as_deref(),
                &p,
            )?;
            ids.push(id);
        }
    }
    ids.sort();
    ids.dedup();
    Ok(RetainedMail {
        source_ids: ids,
        duplicate,
    })
}
pub fn retain_refusal(
    tx: &Transaction<'_>,
    anchor: &str,
    o: &MailObservation,
    size: u64,
    code: &str,
    p: Provenance<'_>,
) -> Result<RetainedMail, Error> {
    observation(o, anchor)?;
    ensure(code == "MAIL_INTAKE_OVERSIZED", "unsupported mail refusal")?;
    let body = json!({"version":1,"companySlug":anchor,"mailbox":o.mailbox,"uidValidity":o.uid_validity,"uid":o.uid,"declaredSize":size,"code":code});
    let bytes = canonical(&body)?.into_bytes();
    let key = format!("mail-refusal:{}", hash(&bytes));
    let meta = json!({"version":VERSION,"role":"refusal","messageId":null,"emlSourceId":null,"emlSha256":null,"receipt":body});
    let (id, _) = insert(
        tx,
        anchor,
        &key,
        &bytes,
        "mail-refusal.json",
        "application/json",
        meta,
        &p,
    )?;
    let duplicate = observed(tx, &id, o, None, None, None, &p)?;
    if !duplicate {
        event(
            tx,
            &id,
            "mail_refused",
            json!({"version":1,"code":code,"declaredSize":size}),
            p.actor,
            p.now,
        )?
    }
    exception(tx, &id, code, &p)?;
    Ok(RetainedMail {
        source_ids: vec![id],
        duplicate,
    })
}
pub fn cursor(
    tx: &Transaction<'_>,
    company: &str,
    epoch: &str,
    mailbox: &str,
) -> Result<Option<(u32, u32)>, Error> {
    let mut q=tx.prepare("SELECT source_id FROM rm_workspace_inbox_events WHERE event_type='mail_observed' ORDER BY id").map_err(sql)?;
    let ids = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(sql)?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(sql)?;
    let mut observations = vec![];
    for id in ids {
        for e in events(tx, &id)? {
            if e["kind"] != "mail_observed" {
                continue;
            }
            let o: MailObservation = serde_json::from_value(e["payload"]["observation"].clone())
                .map_err(|_| refused("mail observation invalid"))?;
            if o.company_slug == company && o.config_epoch == epoch && o.mailbox == mailbox {
                observations.push((
                    e["id"]
                        .as_i64()
                        .ok_or_else(|| refused("mail event ID invalid"))?,
                    o,
                ))
            }
        }
    }
    observations.sort_by_key(|e| e.0);
    let Some(validity) = observations.last().map(|e| e.1.uid_validity) else {
        return Ok(None);
    };
    let uid = observations
        .iter()
        .filter(|e| e.1.uid_validity == validity)
        .map(|e| e.1.uid)
        .max()
        .unwrap();
    Ok(Some((validity, uid)))
}

/// A downloaded but unparsable message still retains the exact bounded raw EML.
pub fn retain_invalid(
    tx: &Transaction<'_>,
    anchor: &str,
    raw: &[u8],
    o: &MailObservation,
    code: &str,
    p: Provenance<'_>,
) -> Result<RetainedMail, Error> {
    observation(o, anchor)?;
    ensure(
        code == "MAIL_INTAKE_INVALID_MESSAGE"
            && !raw.is_empty()
            && raw.len() <= rentemester_engine::mail_intake::MAX_EML,
        "invalid-message original bound/code differs",
    )?;
    ensure(
        rentemester_engine::mail_intake::parse(raw).is_err(),
        "valid parsed mail cannot be retained as invalid",
    )?;
    let sha = hash(raw);
    let meta = json!({"version":VERSION,"role":"eml","messageId":null,"sender":null,"subject":null,"date":null,"parseError":code});
    let key = format!("mail-eml:{}:{sha}", hash(anchor.as_bytes()));
    let (id, _) = insert(
        tx,
        anchor,
        &key,
        raw,
        "message.eml",
        "message/rfc822",
        meta,
        &p,
    )?;
    let duplicate = observed(tx, &id, o, Some(&id), Some(&sha), None, &p)?;
    exception(tx, &id, code, &p)?;
    Ok(RetainedMail {
        source_ids: vec![id],
        duplicate,
    })
}
