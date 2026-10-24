//! Original unkeyed source invocation. No planning, accounting or retries in this client.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};
pub const MAX_ORIGINAL_IMPORT_BODY: usize = 35 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Wire {
    source_system: String,
    file_name: String,
    content_base64: String,
    dry_run: bool,
    confirm: bool,
}
fn validate(bytes: &[u8]) -> Result<Wire, Error> {
    let invalid = || Error::InvalidInput("expected exact bounded original source import body");
    if bytes.len() > MAX_ORIGINAL_IMPORT_BODY {
        return Err(invalid());
    }
    let w: Wire = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if !w.confirm
        || w.file_name.is_empty()
        || w.file_name.len() > 255
        || w.file_name.contains(['/', '\\'])
        || w.file_name.chars().any(char::is_control)
        || w.content_base64.len() > 4 * (25 * 1024 * 1024usize).div_ceil(3)
    {
        return Err(invalid());
    }
    let raw = STANDARD.decode(&w.content_base64).map_err(|_| invalid())?;
    if raw.is_empty() || raw.len() > 25 * 1024 * 1024 || STANDARD.encode(&raw) != w.content_base64 {
        return Err(invalid());
    }
    match w.source_system.as_str() {
        "synthetic-csv" if raw.len() <= 48 * 1024 && std::str::from_utf8(&raw).is_ok() => (),
        "dinero"
            if w.file_name.to_ascii_lowercase().ends_with(".zip")
                || w.file_name == "dinero-directory-source.json" =>
        {
            ()
        }
        _ => return Err(invalid()),
    }
    Ok(w)
}
/// Builds exactly the frozen wire; source bytes are never parsed as accounting here.
pub fn original_import_body(
    system: &str,
    file_name: &str,
    raw: &[u8],
    dry_run: bool,
) -> Result<Vec<u8>, Error> {
    let bytes = serde_json::to_vec(&json!({"sourceSystem":system,"fileName":file_name,"contentBase64":STANDARD.encode(raw),"dryRun":dry_run,"confirm":true})).map_err(|_| Error::InvalidInput("source serialization failed"))?;
    validate(&bytes)?;
    Ok(bytes)
}
fn strings(v: &Value) -> bool {
    v.as_array().is_some_and(|a| a.iter().all(Value::is_string))
}
fn count(v: &Value) -> bool {
    v.as_u64().is_some_and(|n| n <= 9_007_199_254_740_991)
}
fn id(v: &Value) -> bool {
    count(v) && v.as_u64() != Some(0)
}
fn keys(v: &Value, fields: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.keys().all(|k| fields.contains(&k.as_str())))
}
fn receipt(v: &Value, w: &Wire) -> bool {
    if !keys(
        v,
        &[
            "ok",
            "dryRun",
            "sourceSystem",
            "cutOverDate",
            "entryId",
            "entryNo",
            "entryHash",
            "openingBalanceLineCount",
            "historicalEntriesSkipped",
            "historicalEntriesPosted",
            "chart",
            "company",
            "accountRoleProposals",
            "bilag",
            "migrationOpenItems",
            "archiveIntegrity",
            "auditTrail",
            "appliedRules",
            "errors",
        ],
    ) || !v["ok"].is_boolean()
        || v["sourceSystem"] != w.source_system
        || !count(&v["openingBalanceLineCount"])
        || !count(&v["historicalEntriesSkipped"])
        || !["auditTrail", "appliedRules", "errors"]
            .iter()
            .all(|k| strings(&v[*k]))
        || v.get("dryRun")
            .is_some_and(|d| !d.is_boolean() || d.as_bool() != Some(w.dry_run))
        || (w.dry_run && v["ok"] == true && v["dryRun"] != true)
        || (v["ok"] == false && v["errors"].as_array().is_none_or(Vec::is_empty))
    {
        return false;
    }
    for k in ["cutOverDate", "entryNo", "entryHash"] {
        if v.get(k)
            .is_some_and(|x| !x.is_string() || x.as_str() == Some(""))
        {
            return false;
        }
    }
    if v.get("entryId").is_some_and(|x| !id(x)) {
        return false;
    }
    if let Some(chart) = v.get("chart") {
        let fields = [
            "created",
            "existing",
            "updated",
            "differences",
            "conflicts",
            "unmappedVatCodes",
        ];
        if !keys(chart, &fields) || !fields.iter().all(|k| strings(&chart[*k])) {
            return false;
        }
    }
    if let Some(c) = v.get("company") {
        if !keys(c, &["updatedFields", "notes"])
            || !strings(&c["updatedFields"])
            || !strings(&c["notes"])
        {
            return false;
        }
    }
    if let Some(a) = v.get("historicalEntriesPosted") {
        if !a.as_array().is_some_and(|a| {
            a.iter().all(|p| {
                keys(p, &["voucherRef", "entryId", "entryNo", "transactionDate"])
                    && id(&p["entryId"])
                    && ["voucherRef", "entryNo", "transactionDate"]
                        .iter()
                        .all(|k| p[*k].is_string())
            })
        }) {
            return false;
        }
    }
    if let Some(a) = v.get("accountRoleProposals") {
        if !a.as_array().is_some_and(|a| {
            a.iter().all(|p| {
                keys(p, &["role", "accountNo", "source"])
                    && ["role", "accountNo", "source"]
                        .iter()
                        .all(|k| p[*k].is_string())
            })
        }) {
            return false;
        }
    }
    if let Some(b) = v.get("bilag") {
        let fields = [
            "linkedCount",
            "unmatchedCount",
            "duplicateCount",
            "unbookedCount",
        ];
        if !keys(b, &fields) || !fields.iter().all(|k| count(&b[*k])) {
            return false;
        }
    }
    if let Some(b) = v.get("migrationOpenItems") {
        if !keys(b, &["batchCount", "receivableAmount", "payableAmount"])
            || !count(&b["batchCount"])
            || !["receivableAmount", "payableAmount"]
                .iter()
                .all(|k| b[*k].as_f64().is_some_and(|n| n.is_finite() && n >= 0.0))
        {
            return false;
        }
    }
    if let Some(a) = v.get("archiveIntegrity") {
        if !keys(
            a,
            &[
                "archiveSha256",
                "archiveEntryCount",
                "extractedEntryCount",
                "importedEntryCount",
                "archiveListingSha256",
                "extractedManifestSha256",
            ],
        ) || ![
            "archiveEntryCount",
            "extractedEntryCount",
            "importedEntryCount",
        ]
        .iter()
        .all(|k| count(&a[*k]))
            || ![
                "archiveSha256",
                "archiveListingSha256",
                "extractedManifestSha256",
            ]
            .iter()
            .all(|k| {
                a[*k]
                    .as_str()
                    .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            })
        {
            return false;
        }
    }
    true
}
impl ApiClient {
    /// One explicit unkeyed transaction; an unknown result must be reconciled before further action.
    pub async fn import_original(&self, company: &str, bytes: Vec<u8>) -> Result<Value, Error> {
        let wire = validate(&bytes)?;
        let mut response = self
            .send(
                Method::POST,
                &company_path(company, "import-original")?,
                &[],
                Some(bytes),
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await?;
        let status = response.status();
        if status.is_server_error() || status.as_u16() == 408 {
            return Err(Error::OutcomeUnknown);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::OutcomeUnknown)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(Error::OutcomeUnknown);
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::OutcomeUnknown)?;
        if receipt(&value, &wire) && (status.is_success() || value["ok"] == false) {
            return Ok(value);
        }
        if !status.is_success()
            && value["ok"] == false
            && value["code"].as_str().is_some_and(|s| !s.is_empty())
        {
            if matches!(
                value["code"].as_str(),
                Some("internal" | "outcome_unknown" | "outcome_unconfirmed")
            ) {
                return Err(Error::OutcomeUnknown);
            }
            return Err(Error::Refused {
                status: status.as_u16(),
            });
        }
        Err(Error::OutcomeUnknown)
    }
}
