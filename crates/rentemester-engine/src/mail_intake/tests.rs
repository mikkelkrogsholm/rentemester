use super::*;
const EML:&[u8]=b"Message-ID: <synthetic73@example.invalid>\r\nFrom: Synthetic <s@example.invalid>\r\nSubject: =?utf-8?Q?Synthetic_=C3=A6?=\r\nContent-Type: multipart/mixed; boundary=parts\r\n\r\n--parts\r\nContent-Type: application/pdf\r\nContent-Disposition: attachment; filename=\"../../invoice.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQKU3ludGhldGljCg==\r\n--parts--\r\n";
#[test]
fn exact_raw_identity_decoded_pdf_and_safe_filename() {
    let parsed = parse(EML).unwrap();
    assert_eq!(
        parsed.message_id.as_deref(),
        Some("<synthetic73@example.invalid>")
    );
    assert_eq!(parsed.subject.as_deref(), Some("Synthetic æ"));
    assert_eq!(parsed.attachments.len(), 1);
    let a = &parsed.attachments[0];
    assert_eq!(a.original_filename.as_deref(), Some("../../invoice.pdf"));
    assert_eq!(a.filename, "invoice.pdf");
    assert_eq!(a.bytes, b"%PDF-1.4\nSynthetic\n");
    assert_eq!(parse(EML).unwrap(), parsed);
}
#[test]
fn missing_identity_no_attachment_and_magic_mismatch_remain_explicit() {
    assert!(
        parse(b"Subject: Body only\r\n\r\nBody")
            .unwrap()
            .message_id
            .is_none()
    );
    assert!(
        parse(b"Message-ID: <empty@example.invalid>\r\n\r\nNo attachment")
            .unwrap()
            .attachments
            .is_empty()
    );
    let invalid = String::from_utf8(EML.to_vec())
        .unwrap()
        .replace("JVBERi0xLjQKU3ludGhldGljCg==", "bm90LXBkZg==");
    assert!(parse(invalid.as_bytes()).unwrap().attachments.is_empty());
    assert_eq!(parse(&[]).unwrap_err(), MailFailure::Invalid);
}

#[test]
fn combined_escaped_headers_cannot_exceed_retained_metadata_bound() {
    let escaped = "\"".repeat(8192);
    for name in ["Message-ID", "From", "Subject", "Date"] {
        let raw = format!("{name}: {escaped}\r\n\r\nSynthetic body");
        let parsed = parse(raw.as_bytes()).unwrap();
        let retained = match name {
            "Message-ID" => parsed.message_id,
            "From" => parsed.sender,
            "Subject" => parsed.subject,
            _ => parsed.date,
        };
        assert_eq!(retained.as_deref(), Some(escaped.as_str()));
    }
    let raw = format!(
        "Message-ID: {escaped}\r\nFrom: {escaped}\r\nSubject: {escaped}\r\nDate: {escaped}\r\n\r\nSynthetic body"
    );
    assert_eq!(
        parse(raw.as_bytes()).unwrap_err(),
        MailFailure::ResourceLimit
    );
}

#[test]
fn attachment_filename_escaping_is_part_of_each_source_metadata_budget() {
    // Four individually bounded headers fit until the attachment's escaped
    // original filename is included. No truncation or lossy header replacement.
    let header = Some("\"".repeat(7500));
    let base = serde_json::to_vec(&(&header, &header, &header, &header))
        .unwrap()
        .len();
    assert!(metadata_budget(base, 0).is_ok());
    let filename = Some("\\".repeat(3000));
    let encoded = serde_json::to_vec(&filename).unwrap().len();
    assert_eq!(
        metadata_budget(base, encoded),
        Err(MailFailure::ResourceLimit)
    );
    assert!(
        metadata_budget(
            base,
            serde_json::to_vec(&Some("invoice.pdf")).unwrap().len()
        )
        .is_ok()
    );
}
