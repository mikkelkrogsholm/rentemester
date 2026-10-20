use super::*;
const EVENT: &str = "native_mail_document";
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailReceipt {
    pub id: i64,
    pub document_id: i64,
    pub document_no: String,
    pub message_id: String,
    pub attachment_sha256: String,
    pub duplicate: bool,
}
fn ledger(db: &Connection) -> Result<String, Error> {
    db.query_row(
        "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
        [],
        |r| r.get(0),
    )
    .map_err(sql)
}
fn document(
    db: &Connection,
    id: i64,
    no: &str,
    source: &str,
    sha: &str,
    meta: &Value,
) -> Result<(), Error> {
    let (n, s, h, p): (String, String, String, String) = db
        .query_row(
            "SELECT document_no,source,sha256_hash,payload_json FROM documents WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(sql)?;
    ensure(
        n == no
            && s == format!("workspace-inbox:{source}")
            && h == sha
            && serde_json::from_str::<Value>(&p)
                .map_err(|_| refused("mail document JSON invalid"))?
                == *meta,
        "mail document differs from reviewed original",
    )
}
pub fn record_document(
    tx: &Transaction<'_>,
    a: &ApprovedAttachment,
    id: i64,
    no: &str,
    p: Provenance<'_>,
) -> Result<MailReceipt, Error> {
    ensure(
        ledger(tx)? == a.review["ledgerUuid"],
        "mail document target ledger differs",
    )?;
    document(tx, id, no, &a.source_id, &a.hash, a.metadata())?;
    validate_mail_intake(tx)?;
    let message = required(&a.mail, "messageId")?;
    let prior:Option<(i64,Option<i64>)>=tx.query_row("SELECT id,document_id FROM mail_intake_messages WHERE message_id=? AND attachment_sha256=?",params![message,a.hash],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(sql)?;
    if let Some((row, doc)) = prior {
        ensure(
            doc == Some(id),
            "mail attachment already belongs to a different document",
        )?;
        let proof = proof(tx, row)?;
        ensure(
            proof["sourceId"] == a.source_id && proof["review"] == a.review,
            "mail recovered receipt provenance differs",
        )?;
        return Ok(MailReceipt {
            id: row,
            document_id: id,
            document_no: no.into(),
            message_id: message.into(),
            attachment_sha256: a.hash.clone(),
            duplicate: true,
        });
    }
    tx.execute("INSERT INTO mail_intake_messages(message_id,attachment_sha256,attachment_filename,document_id,sender,subject,mail_date,ingested_at)VALUES(?,?,?,?,?,?,?,?)",params![message,a.hash,a.mail["originalFilename"].as_str(),id,a.mail["sender"].as_str(),a.mail["subject"].as_str(),a.mail["date"].as_str(),p.now]).map_err(sql)?;
    let row = tx.last_insert_rowid();
    let payload = json!({"version":1,"sourceId":a.source_id,"ledgerUuid":a.review["ledgerUuid"],"companySlug":a.review["companySlug"],"sourceHash":a.hash,"messageId":message,"documentId":id,"documentNo":no,"mail":a.mail,"review":a.review,"recordedAt":p.now});
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES(?,'mail_intake_message',?,?,?)",params![EVENT,row.to_string(),canonical(&payload)?,p.actor]).map_err(sql)?;
    Ok(MailReceipt {
        id: row,
        document_id: id,
        document_no: no.into(),
        message_id: message.into(),
        attachment_sha256: a.hash.clone(),
        duplicate: false,
    })
}
pub(super) fn proof(db: &Connection, id: i64) -> Result<Value, Error> {
    let mut q=db.prepare("SELECT entity_type,message,actor FROM audit_log WHERE event_type=? AND entity_id=? ORDER BY id").map_err(sql)?;
    let rows = q
        .query_map(params![EVENT, id.to_string()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    ensure(
        rows.len() == 1,
        "mail receipt requires exactly one immutable native audit proof",
    )?;
    let (t, raw, actor) = &rows[0];
    let p: Value =
        serde_json::from_str(raw).map_err(|_| refused("mail receipt proof JSON invalid"))?;
    ensure(
        t == "mail_intake_message"
            && !actor.is_empty()
            && canonical(&p)? == *raw
            && p["version"] == 1,
        "mail receipt proof codec invalid",
    )?;
    Ok(p)
}
/// Company-local historical validation. Physical originals and control crossproof are separate.
pub fn validate_mail_intake(tx: &Connection) -> Result<(), Error> {
    validate_connection(tx)
}
pub(crate) fn validate_connection(db: &Connection) -> Result<(), Error> {
    let uuid = ledger(db)?;
    let mut q=db.prepare("SELECT id,message_id,attachment_sha256,attachment_filename,document_id,sender,subject,mail_date,ingested_at FROM mail_intake_messages ORDER BY id").map_err(sql)?;
    for r in q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, String>(8)?,
            ))
        })
        .map_err(sql)?
    {
        let (row, message, sha, file, doc, sender, subject, date, at) = r.map_err(sql)?;
        let Some(id) = doc else {
            return Err(refused("mail receipt reserved without a document"));
        };
        let source: String = db
            .query_row("SELECT source FROM documents WHERE id=?", [id], |r| {
                r.get(0)
            })
            .map_err(sql)?;
        let native: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM audit_log WHERE event_type=? AND entity_id=?)",
                params![EVENT, row.to_string()],
                |r| r.get(0),
            )
            .map_err(sql)?;
        ensure(
            !native || source.starts_with("workspace-inbox:"),
            "native mail receipt lost its original source marker",
        )?;
        if source.starts_with("workspace-inbox:") {
            let p = proof(db, row)?;
            ensure(
                p["ledgerUuid"] == uuid
                    && p["messageId"] == message
                    && p["sourceHash"] == sha
                    && p["documentId"] == id
                    && p["recordedAt"] == at
                    && p["mail"]["messageId"] == message
                    && p["mail"]["attachmentSha256"] == sha
                    && p["mail"]["originalFilename"] == json!(file)
                    && p["mail"]["sender"] == json!(sender)
                    && p["mail"]["subject"] == json!(subject)
                    && p["mail"]["date"] == json!(date),
                "native mail receipt row/provenance differs",
            )?;
            let review = &p["review"];
            let metadata: rentemester_engine::document::Metadata =
                serde_json::from_value(review["metadata"].clone())
                    .map_err(|_| refused("mail reviewed metadata invalid"))?;
            metadata.validate()?;
            ensure(
                p["mail"]["role"] == "attachment"
                    && serde_json::to_value(metadata)
                        .map_err(|_| refused("mail metadata serialization failed"))?
                        == review["metadata"],
                "mail receipt classification provenance invalid",
            )?;
            ensure(
                review["ledgerUuid"] == uuid
                    && review["sourceHash"] == sha
                    && review["metadataHash"] == hash(canonical(&review["metadata"])?.as_bytes()),
                "mail receipt reviewed context differs",
            )?;
            document(
                db,
                id,
                required(&p, "documentNo")?,
                required(&p, "sourceId")?,
                &sha,
                &review["metadata"],
            )?;
        } else {
            // Source legacy mail rows have no native provenance. Preserve their original
            // receipt semantics without reclassifying them as reviewed native inbox work.
            ensure(
                source == format!("mail-intake:{message}"),
                "unsupported legacy mail document source",
            )?;
            let hash: String = db
                .query_row("SELECT sha256_hash FROM documents WHERE id=?", [id], |r| {
                    r.get(0)
                })
                .map_err(sql)?;
            ensure(hash == sha, "legacy mail receipt original hash differs")?;
        }
    }
    let orphans:i64=db.query_row("SELECT COUNT(*) FROM audit_log a WHERE a.event_type=? AND (a.entity_type!='mail_intake_message' OR NOT EXISTS(SELECT 1 FROM mail_intake_messages m WHERE CAST(m.id AS TEXT)=a.entity_id))",[EVENT],|r|r.get(0)).map_err(sql)?;
    ensure(orphans == 0, "orphan mail document proof")?;
    Ok(())
}
pub(crate) fn cross_validate(
    control: &Connection,
    company: &Connection,
    slug: &str,
) -> Result<(), Error> {
    validate_control(control)?;
    validate_connection(company)?;
    let uuid = ledger(company)?;
    let mut q=company.prepare("SELECT id FROM mail_intake_messages WHERE document_id IN (SELECT id FROM documents WHERE substr(source,1,16)='workspace-inbox:') ORDER BY id").map_err(sql)?;
    for r in q.query_map([], |r| r.get::<_, i64>(0)).map_err(sql)? {
        let p = proof(company, r.map_err(sql)?)?;
        ensure(p["companySlug"] == slug, "mail receipt company differs")?;
        let a = approved_attachment(control, required(&p, "sourceId")?, slug, &uuid)?
            .ok_or_else(|| refused("mail receipt references nonmail source"))?;
        ensure(
            p["mail"] == a.mail && p["review"] == a.review && p["sourceHash"] == a.hash,
            "mail receipt control provenance differs",
        )?;
    }
    Ok(())
}
