use super::*;
/// Server-issued, control-history-bound metadata. Never deserialized from a caller.
pub struct ApprovedAttachment {
    pub(super) source_id: String,
    pub(super) mail: Value,
    pub(super) review: Value,
    pub(super) hash: String,
}
impl ApprovedAttachment {
    pub fn metadata(&self) -> &Value {
        &self.review["metadata"]
    }
}
fn metadata(v: &Value, id: &str) -> Result<(), Error> {
    let m: rentemester_engine::document::Metadata = serde_json::from_value(v.clone())
        .map_err(|_| refused("mail document classification invalid"))?;
    m.validate()?;
    ensure(
        v["source"] == format!("workspace-inbox:{id}")
            && serde_json::to_value(m)
                .map_err(|_| refused("mail metadata serialization failed"))?
                == *v,
        "mail classification must be canonical and source-bound",
    )
}
pub fn review(
    tx: &Transaction<'_>,
    anchor: &str,
    id: &str,
    input: &ReviewInput,
    ledger: &str,
    visible: &BTreeSet<String>,
    p: Provenance<'_>,
) -> Result<Value, Error> {
    validate_control(tx)?;
    let (mail, _, sha, a) = source(tx, id)?;
    ensure(
        a == anchor && mail["role"] == "attachment" && sha == input.source_hash,
        "only exact retained mail attachment can be reviewed",
    )?;
    ensure(
        ledger.len() == 32
            && ledger
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "target ledger UUID invalid",
    )?;
    metadata(&input.metadata, id)?;
    let claims: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM rm_workspace_inbox_handoff_claims WHERE source_id=?",
            [id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    ensure(
        claims == 0,
        "mail review frozen after handoff claim; inspect recovery",
    )?;
    crate::workspace_inbox::assign(tx, anchor, id, &input.company_slug, visible, p.actor, p.now)?;
    let h = events(tx, id)?;
    let mh = hash(canonical(&input.metadata)?.as_bytes());
    let same = h
        .iter()
        .rev()
        .find(|e| e["kind"] == "metadata_reviewed")
        .is_some_and(|e| {
            let r = &e["payload"];
            r["companySlug"] == input.company_slug
                && r["ledgerUuid"] == ledger
                && r["sourceHash"] == sha
                && r["metadataHash"] == mh
                && r["metadata"] == input.metadata
        });
    if !same {
        let last = h
            .last()
            .ok_or_else(|| refused("mail ingress history missing"))?;
        event(
            tx,
            id,
            "metadata_reviewed",
            json!({"version":1,"companySlug":input.company_slug,"ledgerUuid":ledger,"sourceHash":sha,"metadataHash":mh,"metadata":input.metadata,"priorEventCount":h.len(),"priorEventHeadHash":last["hash"]}),
            p.actor,
            p.now,
        )?;
    }
    crate::workspace_inbox::inspect(tx, anchor, id, visible)
}
pub fn approved_attachment(
    db: &Connection,
    id: &str,
    target: &str,
    ledger: &str,
) -> Result<Option<ApprovedAttachment>, Error> {
    validate_control(db)?;
    let (mail, _, sha, _) = source(db, id)?;
    if mail.is_null() {
        return Ok(None);
    }
    ensure(
        mail["role"] == "attachment",
        "mail container/refusal cannot become a document",
    )?;
    let (_, r) = projection(db, id)?;
    ensure(
        !r.is_null()
            && r["companySlug"] == target
            && r["ledgerUuid"] == ledger
            && r["sourceHash"] == sha,
        "mail attachment requires current target-bound metadata review",
    )?;
    let assigned:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM rm_workspace_inbox_assignments WHERE source_id=? AND company_slug=? AND state IN ('approved','completed'))",[id,target],|r|r.get(0)).map_err(sql)?;
    ensure(assigned, "mail review destination not approved")?;
    Ok(Some(ApprovedAttachment {
        source_id: id.into(),
        mail,
        review: r,
        hash: sha,
    }))
}
/// Validates immutable provenance and historical reviews, never today's eligibility.
pub(crate) fn validate_control(db: &Connection) -> Result<(), Error> {
    let mut q=db.prepare("SELECT source_id FROM rm_workspace_inbox_sources WHERE transport='mail' ORDER BY source_id").map_err(sql)?;
    let ids = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    for id in ids {
        let (mail, bytes, sha, anchor) = source(db, &id)?;
        if mail.is_null() {
            continue;
        }
        let role = required(&mail, "role")?;
        let h = events(db, &id)?;
        ensure(
            h.len() >= 3 && h[0]["kind"] == "ingested" && h[1]["kind"] == "candidate_resolved",
            "mail ingress proof missing",
        )?;
        let mut observations = 0;
        let mut reviews = 0;
        let (filename,mime,transport_id):(String,String,String)=db.query_row("SELECT filename,mime_type,transport_identity FROM rm_workspace_inbox_sources WHERE source_id=?",[&id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(sql)?;
        ensure(
            h[0]["payload"]
                == json!({"sourceHash":sha,"filename":filename,"mimeType":mime,"transport":"mail","transportIdentity":transport_id}),
            "mail ingress row proof differs",
        )?;
        ensure(
            h[1]["payload"]
                == json!({"candidates":[{"companySlug":anchor,"evidenceKind":"account","evidence":"explicit configured mailbox company"}]}),
            "mail configured destination provenance differs",
        )?;
        for (index, e) in h.iter().enumerate() {
            let p = &e["payload"];
            match e["kind"].as_str().unwrap_or("") {
                "mail_observed" => {
                    ensure(p["version"] == 1, "unsupported mail observation version")?;
                    let o: MailObservation = serde_json::from_value(p["observation"].clone())
                        .map_err(|_| refused("invalid mail observation"))?;
                    ensure(
                        o.company_slug == anchor
                            && o.uid > 0
                            && o.uid_validity > 0
                            && !o.config_epoch.is_empty()
                            && !o.mailbox.is_empty(),
                        "mail observation identity differs",
                    )?;
                    ensure(
                        p["messageId"] == mail["messageId"],
                        "mail observation Message-ID differs",
                    )?;
                    if role == "refusal" {
                        ensure(
                            p["emlSourceId"].is_null() && p["emlSha256"].is_null(),
                            "refusal pretends to retain EML",
                        )?
                    } else {
                        let eml = required(p, "emlSourceId")?;
                        let (em, raw, eh, ea) = source(db, eml)?;
                        ensure(
                            em["role"] == "eml" && ea == anchor && p["emlSha256"] == eh,
                            "mail observation EML provenance differs",
                        )?;
                        if role == "eml" && mail["parseError"] == "MAIL_INTAKE_INVALID_MESSAGE" {
                            ensure(
                                eml == id
                                    && raw == bytes
                                    && rentemester_engine::mail_intake::parse(&raw).is_err(),
                                "invalid-mail raw provenance differs",
                            )?;
                            observations += 1;
                            continue;
                        }
                        let parsed = rentemester_engine::mail_intake::parse(&raw)
                            .map_err(|_| refused("retained EML cannot be parsed"))?;
                        ensure(
                            json!(parsed.message_id) == mail["messageId"],
                            "retained EML Message-ID differs",
                        )?;
                        if role == "attachment" {
                            ensure(
                                parsed.message_id.is_some()
                                    && parsed
                                        .attachments
                                        .iter()
                                        .any(|a| a.sha256 == sha && a.bytes == bytes),
                                "mail attachment not present in cited EML",
                            )?
                        } else {
                            ensure(eml == id && raw == bytes, "mail container identity differs")?
                        }
                    }
                    observations += 1;
                }
                "metadata_reviewed" => {
                    ensure(
                        role == "attachment"
                            && p["version"] == 1
                            && p["sourceHash"] == sha
                            && p["priorEventCount"] == index
                            && index > 0
                            && p["priorEventHeadHash"] == h[index - 1]["hash"],
                        "mail review historical head/source differs",
                    )?;
                    metadata(&p["metadata"], &id)?;
                    ensure(
                        p["metadataHash"] == hash(canonical(&p["metadata"])?.as_bytes()),
                        "mail reviewed metadata hash differs",
                    )?;
                    let ledger = required(p, "ledgerUuid")?;
                    ensure(
                        ledger.len() == 32
                            && ledger
                                .bytes()
                                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                        "review ledger identity invalid",
                    )?;
                    let assigned = h[..index].iter().find(|e| e["kind"] == "assigned");
                    ensure(
                        assigned.is_some_and(|e| e["payload"]["companySlug"] == p["companySlug"]),
                        "mail review lacks explicit target assignment",
                    )?;
                    ensure(
                        !h[..index].iter().any(|e| e["kind"] == "handoff_completed"),
                        "mail review changed after completion",
                    )?;
                    reviews += 1;
                }
                "mail_refused" => {
                    ensure(
                        role == "refusal"
                            && p["version"] == 1
                            && p["code"] == "MAIL_INTAKE_OVERSIZED"
                            && p["declaredSize"] == mail["receipt"]["declaredSize"],
                        "mail refusal proof differs",
                    )?;
                }
                "assigned" | "handoff_completed" => {
                    ensure(role == "attachment", "mail container has a handoff")?
                }
                "ingested" | "candidate_resolved" | "exception_opened" | "reassignment_denied" => {}
                _ => return Err(refused("unsupported mail event codec")),
            }
        }
        ensure(
            observations > 0,
            "mail source lacks observed transport provenance",
        )?;
        match role {
            "eml" if mail["parseError"] == "MAIL_INTAKE_INVALID_MESSAGE" => {
                ensure(
                    rentemester_engine::mail_intake::parse(&bytes).is_err(),
                    "invalid EML provenance differs",
                )?;
            }
            "eml" => {
                let parsed = rentemester_engine::mail_intake::parse(&bytes)
                    .map_err(|_| refused("retained EML invalid"))?;
                ensure(
                    json!(parsed.message_id) == mail["messageId"]
                        && json!(parsed.sender) == mail["sender"]
                        && json!(parsed.subject) == mail["subject"]
                        && json!(parsed.date) == mail["date"],
                    "mail container header provenance differs",
                )?;
            }
            "attachment" => {
                ensure(
                    mail["attachmentSha256"] == sha,
                    "attachment hash provenance differs",
                )?;
                let eml = required(&mail, "emlSourceId")?;
                let (_, raw, eh, ea) = source(db, eml)?;
                ensure(
                    ea == anchor && mail["emlSha256"] == eh,
                    "attachment initial EML differs",
                )?;
                let parsed = rentemester_engine::mail_intake::parse(&raw)
                    .map_err(|_| refused("initial EML invalid"))?;
                ensure(
                    json!(parsed.sender) == mail["sender"]
                        && json!(parsed.subject) == mail["subject"]
                        && json!(parsed.date) == mail["date"]
                        && parsed.attachments.iter().any(|a| {
                            a.sha256 == sha
                                && a.filename == filename
                                && a.mime_type == mime
                                && json!(a.part_index) == mail["partIndex"]
                                && json!(a.original_filename) == mail["originalFilename"]
                        }),
                    "attachment original part provenance differs",
                )?;
            }
            "refusal" => {
                ensure(
                    canonical(&mail["receipt"])?.as_bytes() == bytes
                        && mail["receipt"]["code"] == "MAIL_INTAKE_OVERSIZED"
                        && mail["emlSha256"].is_null(),
                    "mail refusal receipt differs",
                )?;
            }
            _ => return Err(refused("unsupported native mail role")),
        }
        let claims: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM rm_workspace_inbox_handoff_claims WHERE source_id=?",
                [&id],
                |r| r.get(0),
            )
            .map_err(sql)?;
        ensure(
            claims == 0 || reviews > 0,
            "mail claim lacks metadata review",
        )?;
        if claims > 0 {
            let first_claim:String=db.query_row("SELECT MIN(created_at) FROM rm_workspace_inbox_handoff_claims WHERE source_id=?",[&id],|r|r.get(0)).map_err(sql)?;
            let last_review = h
                .iter()
                .rev()
                .find(|e| e["kind"] == "metadata_reviewed")
                .ok_or_else(|| refused("mail review missing"))?;
            ensure(
                required(last_review, "at")? <= first_claim.as_str(),
                "mail metadata review occurred after claim",
            )?;
        }
    }
    Ok(())
}
