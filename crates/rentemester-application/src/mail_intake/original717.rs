//! Source-compatible immediate intake, distinct from native reviewed handoff.
use super::*;
use rentemester_engine::document::Metadata;
use rentemester_runtime::document::Prepared;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Message {
    pub source: String,
    pub base64: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub messages: Vec<Message>,
    pub metadata: Option<Value>,
    pub metadata_per_message: Option<std::collections::BTreeMap<String, Value>>,
    #[serde(default)]
    pub force: bool,
    pub confirm: bool,
}
fn exception(tx: &Transaction<'_>, kind: &str, key: &str) -> Result<usize, Error> {
    tx.execute("INSERT INTO exceptions(type,severity,message,required_action,source_evidence,resolution_key) SELECT ?1,'medium','Mail intake needs metadata review','Review mail intake evidence',?2,?3 WHERE NOT EXISTS(SELECT 1 FROM exceptions WHERE type=?1 AND resolution_key=?3)",params![kind,json!({"identity":key}).to_string(),key]).map_err(|_|refused())
}
pub fn ingest(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    cookies: &CookieVerifier,
    context: Context<'_>,
    input: &Input,
) -> Result<Committed<Value>, Failure> {
    fenced(control, company, |ct, lt| {
        rentemester_storage::workspace_transfer::require_business_write(ct)?;
        let subject = human(ct, cookies, &context, Permission::CompanyDocumentsUpload)?;
        crate::backup_status::require_write(
            ct,
            lt,
            context.selection.workspace,
            context.selection.slug,
            context.clock,
        )?;
        if !input.confirm {
            return Ok(json!({"ok":false,"errors":["confirm: true is required"]}));
        }
        if input.messages.len() > 32 {
            return Err(refused());
        }
        let mut processed = 0;
        let mut ingested = 0;
        let mut skipped = 0;
        let mut exceptions = 0;
        let mut documents = vec![];
        let at = stamp(context.clock)?.1;
        let today = &at[..10];
        let actor = format!("user:{subject}");
        for message in &input.messages {
            // Source is an untrusted MCP-local label, NEVER opened by the backend.
            if message.source.is_empty() || message.source.len() > 4096 {
                return Err(refused());
            }
            let raw = decode(&message.base64)?;
            let parsed = rentemester_engine::mail_intake::parse(&raw).map_err(|_| refused())?;
            processed += 1;
            let Some(id) = parsed.message_id.as_deref() else {
                exceptions += exception(lt, "MAIL_INTAKE_AMBIGUOUS_METADATA", &sha(&raw))?;
                continue;
            };
            if parsed.attachments.is_empty() {
                exceptions += exception(lt, "MAIL_INTAKE_NO_ATTACHMENT", id)?;
                continue;
            }
            let metadata = input
                .metadata_per_message
                .as_ref()
                .and_then(|m| m.get(id))
                .or(input.metadata.as_ref());
            for attachment in parsed.attachments {
                let known:bool=lt.query_row("SELECT EXISTS(SELECT 1 FROM mail_intake_messages WHERE message_id=? AND attachment_sha256=?)",params![id,attachment.sha256],|r|r.get(0)).map_err(|_|refused())?;
                if known {
                    skipped += 1;
                    continue;
                }
                let key = format!("{id}:{}", attachment.sha256);
                let Some(mut meta) = metadata.cloned() else {
                    exceptions += exception(lt, "MAIL_INTAKE_AMBIGUOUS_METADATA", &key)?;
                    continue;
                };
                let Some(fields) = meta.as_object_mut() else {
                    return Err(refused());
                };
                fields.insert("source".into(), json!(format!("mail-intake:{id}")));
                // Original passthrough fields are accepted, but never used as authority.
                let known = [
                    "source",
                    "documentType",
                    "issueDate",
                    "invoiceNo",
                    "deliveryDescription",
                    "amountIncVat",
                    "currency",
                    "sender",
                    "recipient",
                    "vatAmount",
                    "purchaseVatLines",
                    "reverseChargeWordingConfirmed",
                    "danishSimplifiedPurchaseInvoice",
                    "exemptionCode",
                    "sourceBankTransactionId",
                    "paymentDetails",
                    "accountingRationale",
                ];
                fields.retain(|k, _| known.contains(&k.as_str()));
                let metadata: Metadata = match serde_json::from_value(meta) {
                    Ok(m) => m,
                    Err(_) => {
                        exceptions += exception(lt, "MAIL_INTAKE_INGEST_BLOCKED", &key)?;
                        continue;
                    }
                };
                if metadata.validate().is_err() {
                    exceptions += exception(lt, "MAIL_INTAKE_INGEST_BLOCKED", &key)?;
                    continue;
                }
                let file = Prepared::new(attachment.filename.clone(), attachment.bytes)
                    .map_err(|_| refused())?;
                if rentemester_storage::documents_original::intake_duplicate(
                    lt,
                    file.hash(),
                    &metadata,
                    input.force,
                )?
                .is_some()
                {
                    exceptions += exception(lt, "MAIL_INTAKE_INGEST_BLOCKED", &key)?;
                    continue;
                }
                let path = guard.publish_document(&file).map_err(|_| refused())?;
                let receipt = rentemester_storage::documents::register_original(
                    lt,
                    rentemester_storage::documents::Original {
                        filename: file.filename(),
                        hash: file.hash(),
                        path: &path,
                        mime: file.mime(),
                    },
                    &metadata,
                    &subject,
                    today,
                    None,
                    rentemester_storage::idempotency::PrincipalKind::User,
                    input.force,
                )?;
                lt.execute("INSERT INTO mail_intake_messages(message_id,attachment_sha256,attachment_filename,document_id,sender,subject,mail_date) VALUES(?,?,?,?,?,?,?)",params![id,attachment.sha256,attachment.filename,receipt["document"]["id"].as_i64(),parsed.sender,parsed.subject,parsed.date]).map_err(|_|refused())?;
                ingested += 1;
                documents.push(json!({"messageId":id,"documentNo":receipt["document"]["documentNo"],"sha256":attachment.sha256}));
            }
        }
        rentemester_storage::unmatched_bank::sync(lt)?;
        Ok(
            json!({"ok":true,"messagesProcessed":processed,"attachmentsIngested":ingested,"attachmentsSkipped":skipped,"exceptionsCreated":exceptions,"documents":documents}),
        )
    })
}
fn decode(text: &str) -> Result<Vec<u8>, Error> {
    imap::original717::decode_eml(text).map_err(|_| refused())
}

pub struct PollAdmission {
    workspace: String,
    slug: String,
    ledger: String,
    subject: String,
    authority: String,
    config: imap::Config,
}
impl PollAdmission {
    pub fn config(&self) -> &imap::Config {
        &self.config
    }
}
pub fn prepare(
    control: &mut Connection,
    company: &mut Connection,
    cookies: &CookieVerifier,
    context: Context<'_>,
    config: imap::Config,
) -> Result<Committed<PollAdmission>, Failure> {
    fenced(control, company, |ct, lt| {
        rentemester_storage::workspace_transfer::require_business_write(ct)?;
        let subject = human(ct, cookies, &context, Permission::CompanyAdmin)?;
        crate::backup_status::require_write(
            ct,
            lt,
            context.selection.workspace,
            context.selection.slug,
            context.clock,
        )?;
        Ok(PollAdmission {
            workspace: context.selection.workspace.into(),
            slug: context.selection.slug.into(),
            ledger: ledger(lt)?,
            authority: authority(ct, &subject, &context.selection, true)?,
            subject,
            config,
        })
    })
}
pub fn finish(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    cookies: &CookieVerifier,
    context: Context<'_>,
    admission: PollAdmission,
    input: &Input,
) -> Result<Committed<Value>, Failure> {
    let checked = fenced(control, company, |ct, lt| {
        let subject = human(ct, cookies, &context, Permission::CompanyAdmin)?;
        if admission.workspace != context.selection.workspace
            || admission.slug != context.selection.slug
            || admission.subject != subject
            || admission.ledger != ledger(lt)?
            || admission.authority != authority(ct, &subject, &context.selection, true)?
        {
            return Err(denied());
        }
        Ok(())
    })?;
    if !checked.control_reusable {
        return Err(failure(refused(), false));
    }
    ingest(control, company, guard, cookies, context, input)
}
