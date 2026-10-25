//! Deterministic offline HTML. Evidence status is not an assertion of legal compliance.
use crate::Error;
use serde_json::Value;
pub fn fingerprint(s: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
pub fn render(input: &Value) -> Result<String, Error> {
    if input["format"] != "rentemester-company-compliance/1"
        || !input["company"].is_object()
        || !input["audit"].is_object()
        || !input["coverage"].is_object()
    {
        return Err(Error::new(
            "compliance_invalid",
            "unsupported or incomplete company report",
        ));
    }
    let mut html = format!(
        "<!doctype html>\n<html lang=\"da\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Compliance-rapport — {}</title><style>body{{font:16px system-ui;max-width:960px;margin:2em auto;padding:1em}}pre{{white-space:pre-wrap;overflow-wrap:anywhere}}section{{border-top:1px solid #aaa}}@media print{{section{{break-inside:avoid}}}}</style></head><body><h1>Compliance-rapport</h1><p>Forretningsmæssigt overblik: partial evidence, not legal approval. Citation traceability and legacy references do not establish native execution. No report signature or independently trusted legal approval is asserted.</p>\n",
        escape(input["company"]["name"].as_str().unwrap_or(""))
    );
    for (title, key) in [
        ("Company / asOf", "company"),
        ("Regnskabsår", "fiscalYear"),
        (
            "1. Integritet af bogføringen — native audit status",
            "audit",
        ),
        ("2. Opbevaring og backup — unavailable details", "backup"),
        ("3. Opbevaringsfrist (5 år)", "retention"),
        ("4. Periode-lukning", "periods"),
        ("5. GDPR — persondata (read-only summary)", "gdpr"),
        (
            "6. Regulatorisk dækning / 7. Regler og deres lovhjemmel",
            "coverage",
        ),
    ] {
        let data = serde_json::to_string_pretty(&input[key])
            .map_err(|_| Error::new("compliance_invalid", "report serialization failed"))?;
        html += &format!(
            "<section><h2>{}</h2><pre>{}</pre></section>\n",
            escape(title),
            escape(&data)
        );
    }
    html += &format!(
        "<footer><p>As of: {}. {}</p><p>8. Myndighedsudlevering og SAF-T: not collected by this report. Consult separately authorized native exports. Unavailable evidence is not a passing control. Final legal assessment remains the responsibility of the company and its advisers.</p></footer></body></html>\n",
        escape(input["asOf"].as_str().unwrap_or("")),
        escape(input["stateNotice"].as_str().unwrap_or(""))
    );
    Ok(html)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_escaped_and_no_remote_assets() {
        let v = serde_json::json!({"format":"rentemester-company-compliance/1","company":{"name":"<script>\"&'"},"audit":{"ok":false},"coverage":{}});
        let a = render(&v).unwrap();
        assert_eq!(a, render(&v).unwrap());
        assert!(!a.contains("<script>"));
        assert!(a.contains("&lt;script&gt;"));
        assert!(!a.contains("https://"));
        assert!(render(&Value::Null).is_err());
    }
}

pub mod original716;
