//! Bounded MIME interpretation. All mail text is untrusted evidence, never instructions or accounting metadata.
use mail_parser::{HeaderName, Message, MessageParser, MimeHeaders};
use sha2::{Digest, Sha256};
pub const MAX_EML: usize = 25 * 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailFailure {
    Invalid,
    TooLarge,
    ResourceLimit,
}
impl MailFailure {
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooLarge => "MAIL_INTAKE_OVERSIZED",
            _ => "MAIL_INTAKE_INVALID_MESSAGE",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMail {
    pub message_id: Option<String>,
    pub sender: Option<String>,
    pub subject: Option<String>,
    pub date: Option<String>,
    pub attachments: Vec<Attachment>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub part_index: u32,
    pub original_filename: Option<String>,
    pub filename: String,
    pub mime_type: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
}
fn header(value: Option<&str>) -> Option<String> {
    value
        .map(|v| v.lines().map(str::trim).collect::<Vec<_>>().join(" "))
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}
fn collect(
    message: &Message<'_>,
    depth: usize,
    part_index: &mut u32,
    attachments: &mut Vec<Attachment>,
    total: &mut usize,
) -> Result<(), MailFailure> {
    if depth > 8 || message.parts.len() > 1024 {
        return Err(MailFailure::ResourceLimit);
    }
    for part in message.attachments() {
        if let Some(nested) = part.message() {
            collect(nested, depth + 1, part_index, attachments, total)?;
            continue;
        }
        *part_index = part_index
            .checked_add(1)
            .ok_or(MailFailure::ResourceLimit)?;
        if *part_index > 1024 {
            return Err(MailFailure::ResourceLimit);
        }
        let original = part.attachment_name().map(str::to_owned);
        if original.as_ref().is_some_and(|s| s.len() > 8192) {
            return Err(MailFailure::ResourceLimit);
        }
        let extension = original
            .as_deref()
            .and_then(|v| v.rsplit('.').next())
            .map(str::to_ascii_lowercase);
        let extension = if let Some(ext) = extension.as_deref() {
            match ext {
                "pdf" | "jpg" | "jpeg" | "png" => ext,
                _ => continue,
            }
        } else {
            match part.content_type().map(|c| (c.ctype(), c.subtype())) {
                Some(("application", Some("pdf"))) => "pdf",
                Some(("image", Some("jpeg"))) => "jpg",
                Some(("image", Some("png"))) => "png",
                _ => continue,
            }
        };
        let bytes = part.contents();
        let mime = match extension {
            "pdf" if bytes.starts_with(b"%PDF-") => "application/pdf",
            "png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => "image/png",
            "jpg" | "jpeg" if bytes.starts_with(b"\xff\xd8\xff") => "image/jpeg",
            _ => continue,
        };
        *total = total
            .checked_add(bytes.len())
            .ok_or(MailFailure::ResourceLimit)?;
        if bytes.is_empty() || bytes.len() > MAX_EML || *total > MAX_EML || attachments.len() >= 128
        {
            return Err(MailFailure::ResourceLimit);
        }
        let filename = original
            .as_deref()
            .and_then(|s| s.rsplit(['/', '\\']).next())
            .filter(|s| !s.is_empty() && s.len() <= 255 && !s.chars().any(char::is_control))
            .map(str::to_owned)
            .unwrap_or_else(|| format!("attachment-{part_index}.{extension}"));
        attachments.push(Attachment {
            part_index: *part_index,
            original_filename: original.clone(),
            filename,
            mime_type: mime.into(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
            bytes: bytes.to_vec(),
        });
    }
    Ok(())
}
fn metadata_budget(header_cost: usize, filename_cost: usize) -> Result<(), MailFailure> {
    // Native fixed keys, source IDs, hashes and numeric fields occupy <1 KiB.
    if header_cost
        .saturating_add(filename_cost)
        .saturating_add(1024)
        > 64 * 1024
    {
        return Err(MailFailure::ResourceLimit);
    }
    Ok(())
}
pub fn parse(raw: &[u8]) -> Result<ParsedMail, MailFailure> {
    if raw.is_empty() {
        return Err(MailFailure::Invalid);
    }
    if raw.len() > MAX_EML {
        return Err(MailFailure::TooLarge);
    }
    let Some(message) = MessageParser::default().parse(raw) else {
        return Ok(ParsedMail {
            message_id: None,
            sender: None,
            subject: None,
            date: None,
            attachments: vec![],
        });
    };
    let message_id = header(message.header_raw(HeaderName::MessageId));
    let sender = header(message.header_raw(HeaderName::From));
    let subject = header(message.subject());
    let date = header(message.header_raw(HeaderName::Date));
    if [&message_id, &sender, &subject, &date]
        .into_iter()
        .flatten()
        .any(|s| s.len() > 8192)
    {
        return Err(MailFailure::ResourceLimit);
    }
    // Bound the actual escaped JSON, not just decoded header lengths. These
    // strings become immutable per-source metadata; rejection must happen before
    // retention so an oversized envelope cannot poison later reads or backups.
    let header_cost = serde_json::to_vec(&(&message_id, &sender, &subject, &date))
        .map_err(|_| MailFailure::ResourceLimit)?
        .len();
    metadata_budget(header_cost, 0)?;
    let mut attachments = vec![];
    collect(&message, 0, &mut 0, &mut attachments, &mut 0)?;
    for attachment in &attachments {
        let filename_cost = serde_json::to_vec(&attachment.original_filename)
            .map_err(|_| MailFailure::ResourceLimit)?
            .len();
        metadata_budget(header_cost, filename_cost)?;
    }
    attachments.sort_by(|a, b| {
        a.filename
            .encode_utf16()
            .cmp(b.filename.encode_utf16())
            .then_with(|| a.sha256.cmp(&b.sha256))
            .then_with(|| a.part_index.cmp(&b.part_index))
    });
    Ok(ParsedMail {
        message_id,
        sender,
        subject,
        date,
        attachments,
    })
}
#[cfg(test)]
mod tests;
