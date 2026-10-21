//! Finite SAF-T artifact contract, using the existing bounded regular-file writer.
use crate::{
    Error,
    handoff_export::{matches_output, write_package},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
const PROFILE: &str = "rentemester-dk-saft-v3-ledger-sales-purchases-masterfiles";
fn validate(files: &BTreeMap<String, &[u8]>) -> Result<(String, Value), Error> {
    if files.len() != 3 {
        return Err(Error::Protocol);
    }
    let root = files
        .keys()
        .next()
        .and_then(|s| s.split('/').next())
        .filter(|s| s.starts_with("saft-export-"))
        .ok_or(Error::Protocol)?
        .to_owned();
    for name in ["manifest.json", "README.txt", "saft.xml"] {
        if !files.contains_key(&format!("{root}/{name}")) {
            return Err(Error::Protocol);
        }
    }
    let manifest: Value = serde_json::from_slice(files[&format!("{root}/manifest.json")])
        .map_err(|_| Error::Protocol)?;
    if manifest["manifestVersion"] != 2
        || manifest["packageType"] != "saft_export"
        || manifest["profileId"] != PROFILE
        || ![
            "periodStart",
            "periodEnd",
            "generatedAt",
            "sourceCompanyRootName",
        ]
        .iter()
        .all(|key| manifest[key].as_str().is_some_and(|s| !s.is_empty()))
        || !manifest["generatedAtExplicit"].is_boolean()
        || !matches!(
            (
                manifest["generatedAtSource"].as_str(),
                manifest["generatedAtExplicit"].as_bool()
            ),
            (Some("trusted_runtime_clock" | "period_end"), Some(false))
                | (Some("caller_supplied"), Some(true))
        )
        || manifest["files"] != json!({"saftXml":"saft.xml","readme":"README.txt"})
        || !manifest["appliedRules"]
            .as_array()
            .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
    {
        return Err(Error::Protocol);
    }
    for key in [
        "accounts",
        "journalEntries",
        "salesInvoices",
        "purchaseInvoices",
        "vatSummaryCodes",
        "documentReferences",
        "customers",
        "suppliers",
    ] {
        if !manifest["counts"][key].is_u64() {
            return Err(Error::Protocol);
        }
    }
    let outputs = manifest["outputs"]
        .as_array()
        .filter(|v| v.len() == 2)
        .ok_or(Error::Protocol)?;
    let mut declared = BTreeSet::new();
    for output in outputs {
        let name = output["path"]
            .as_str()
            .filter(|s| matches!(*s, "README.txt" | "saft.xml"))
            .ok_or(Error::Protocol)?;
        if !declared.insert(name) || !matches_output(output, files[&format!("{root}/{name}")]) {
            return Err(Error::Protocol);
        }
    }
    let source = &manifest["sourceContext"];
    if !["journalEntries", "documents", "auditLog"]
        .iter()
        .all(|key| {
            source[key]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_object))
        })
        || !source["trialBalance"].is_object()
        || source["trialBalance"]["ok"] != true
        || source["trialBalance"]["balanced"] != true
        || !source["trialBalance"]["accounts"].is_array()
        || !source["trialBalance"]["totalDebit"].is_string()
        || !source["trialBalance"]["totalCredit"].is_string()
        || !source["journalHashChain"].is_object()
        || source["journalHashChain"]["ok"] != true
        || !source["journalHashChain"]["entriesChecked"].is_u64()
        || !source["journalHashChain"]["errors"]
            .as_array()
            .is_some_and(Vec::is_empty)
    {
        return Err(Error::Protocol);
    }
    Ok((root, manifest))
}
/// Write only a fully verified three-file package beneath a fresh out/root.
/// Audit may already be committed on local failure; never automatically re-export.
pub fn write_saft_package(value: &Value, out: &Path) -> Result<Value, Error> {
    let (target, manifest) = write_package(value, out, validate)?;
    Ok(
        json!({"ok":true,"exportDir":target,"manifestPath":target.join("manifest.json"),"saftXmlPath":target.join("saft.xml"),"generatedAt":manifest["generatedAt"],"periodStart":manifest["periodStart"],"periodEnd":manifest["periodEnd"],"journalEntryCount":manifest["counts"]["journalEntries"],"salesInvoiceCount":manifest["counts"]["salesInvoices"],"purchaseInvoiceCount":manifest["counts"]["purchaseInvoices"],"customerCount":manifest["counts"]["customers"],"supplierCount":manifest["counts"]["suppliers"],"appliedRules":manifest["appliedRules"],"errors":[]}),
    )
}
