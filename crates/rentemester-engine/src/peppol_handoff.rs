//! Deterministic original handoff artifact. Metadata never authorizes a network request.
use crate::{Error, public_einvoice::Export};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessPoint {
    pub access_point_id: String,
    pub endpoint_url: String,
    pub sender_endpoint_id: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Acknowledgement {
    pub transmission_id: String,
    pub acknowledged_at: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Handoff {
    pub submission_reference: String,
    pub idempotency_key: String,
    pub oioubl_sha256: String,
    pub envelope_sha256: String,
    pub envelope: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transmission_id: Option<String>,
}
fn refused(message: &str) -> Error {
    Error::new("peppol_handoff_refused", message)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 4096 && s.chars().all(|c| matches!(c,'\t'|'\n'|'\r'|'\u{20}'..='\u{d7ff}'|'\u{e000}'..='\u{fffd}'|'\u{10000}'..='\u{10ffff}'))
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
/// Identity deliberately excludes endpoint URL and supplied acknowledgement, as in the original contract.
pub fn prepare(
    invoice_number: &str,
    export: &Export,
    access: &AccessPoint,
    acknowledgement: Option<&Acknowledgement>,
) -> Result<Handoff, Error> {
    for s in [
        invoice_number,
        &access.access_point_id,
        &access.endpoint_url,
        &access.sender_endpoint_id,
    ] {
        if !valid(s) {
            return Err(refused(
                "nonempty bounded XML-safe invoice/access-point metadata required",
            ));
        }
    }
    if let Some(a) = acknowledgement
        && (!valid(&a.transmission_id) || !valid(&a.acknowledged_at))
    {
        return Err(refused(
            "nonempty bounded acknowledgement metadata required",
        ));
    }
    let digits = &export.receiver_endpoint_id;
    let receiver = format!("GLN:{digits}");
    if digits.len() != 13
        || !digits.bytes().all(|b| b.is_ascii_digit())
        || export.sha256 != hash(export.xml.as_bytes())
    {
        return Err(refused(
            "exact OIOUBL artifact hash and GLN receiver required",
        ));
    }
    let idempotency_key = hash(
        [
            invoice_number,
            &export.sha256,
            access.access_point_id.trim(),
            access.sender_endpoint_id.trim(),
            &receiver,
        ]
        .join("|")
        .as_bytes(),
    );
    let submission_reference = format!("PEPPOL-{invoice_number}-{}", &idempotency_key[..12]);
    let status = if acknowledgement.is_some() {
        "acknowledged"
    } else {
        "prepared"
    };
    let mut lines = vec![
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_owned(),
        "<PeppolSubmission xmlns=\"urn:rentemester:dk:peppol-submission:v1\">".into(),
    ];
    let tag = |name: &str, value: &str, indent: &str| {
        format!("{indent}<{name}>{}</{name}>", escape(value))
    };
    lines.extend([
        tag("SubmissionReference", &submission_reference, "  "),
        tag("IdempotencyKey", &idempotency_key, "  "),
        tag("Status", status, "  "),
        "  <Document>".into(),
        tag("InvoiceNumber", invoice_number, "    "),
        tag("Format", "OIOUBL-2.02", "    "),
        tag("Profile", "Procurement-BilSim-1.0", "    "),
        tag("HandoffArtifactSha256", &export.sha256, "    "),
        "  </Document>".into(),
        "  <AccessPoint>".into(),
        tag("AccessPointId", &access.access_point_id, "    "),
        tag("EndpointUrl", &access.endpoint_url, "    "),
        tag("SenderEndpointId", &access.sender_endpoint_id, "    "),
        tag("ReceiverEndpointId", &receiver, "    "),
        "  </AccessPoint>".into(),
    ]);
    if let Some(a) = acknowledgement {
        lines.extend([
            "  <Acknowledgement>".into(),
            tag("TransmissionId", &a.transmission_id, "    "),
            tag("AcknowledgedAt", &a.acknowledged_at, "    "),
            "  </Acknowledgement>".into(),
        ]);
    }
    lines.push("</PeppolSubmission>".into());
    let envelope = lines.join("\n");
    Ok(Handoff {
        submission_reference,
        idempotency_key,
        oioubl_sha256: export.sha256.clone(),
        envelope_sha256: hash(envelope.as_bytes()),
        envelope,
        status: status.into(),
        transmission_id: acknowledgement.map(|a| a.transmission_id.clone()),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn exported() -> Export {
        Export {
            xml: "<Invoice/>".into(),
            sha256: hash(b"<Invoice/>"),
            receiver_endpoint_id: "5790000000000".into(),
        }
    }
    fn access() -> AccessPoint {
        AccessPoint {
            access_point_id: " synthetic & point ".into(),
            endpoint_url: "https://metadata.invalid/a?x=1&y=2".into(),
            sender_endpoint_id: " DK12345678 ".into(),
        }
    }
    #[test]
    fn original_source_envelope_and_identity_match_exactly() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("peppol_handoff/source_cases.json")).unwrap();
        for case in cases {
            let access: AccessPoint = serde_json::from_value(case["accessPoint"].clone()).unwrap();
            let ack: Option<Acknowledgement> =
                serde_json::from_value(case["acknowledgement"].clone()).unwrap();
            let ex = Export {
                xml: case["export"]["xml"].as_str().unwrap().into(),
                sha256: case["export"]["sha256"].as_str().unwrap().into(),
                receiver_endpoint_id: case["export"]["receiverEndpointId"]
                    .as_str()
                    .unwrap()
                    .into(),
            };
            let result = prepare(
                case["invoiceNumber"].as_str().unwrap(),
                &ex,
                &access,
                ack.as_ref(),
            )
            .unwrap();
            assert_eq!(serde_json::to_value(result).unwrap(), case["expected"]);
        }
    }
    #[test]
    fn identity_is_stable_but_artifact_retains_endpoint_and_attestation() {
        let ex = exported();
        let a = access();
        let first = prepare("SYN-153", &ex, &a, None).unwrap();
        assert_eq!(first.status, "prepared");
        assert!(first.envelope.contains("synthetic &amp; point"));
        assert!(!first.envelope.ends_with('\n'));
        let mut changed = access();
        changed.endpoint_url = "metadata:elsewhere".into();
        changed.access_point_id = changed.access_point_id.trim().into();
        let ack = Acknowledgement {
            transmission_id: "MANUAL-153".into(),
            acknowledged_at: "2026-10-22T00:00:00Z".into(),
        };
        let attested = prepare("SYN-153", &ex, &changed, Some(&ack)).unwrap();
        assert_eq!(first.idempotency_key, attested.idempotency_key);
        assert_ne!(first.envelope_sha256, attested.envelope_sha256);
        assert_eq!(attested.status, "acknowledged");
        assert_eq!(attested.transmission_id.as_deref(), Some("MANUAL-153"));
    }
    #[test]
    fn malformed_metadata_and_artifact_are_not_accepted() {
        let mut ex = exported();
        ex.xml.push(' ');
        assert!(prepare("SYN-153", &ex, &access(), None).is_err());
        let mut a = access();
        a.endpoint_url = "\u{0}".into();
        assert!(prepare("SYN-153", &exported(), &a, None).is_err());
        let ack = Acknowledgement {
            transmission_id: "".into(),
            acknowledged_at: "now".into(),
        };
        assert!(prepare("SYN-153", &exported(), &access(), Some(&ack)).is_err());
    }
}
