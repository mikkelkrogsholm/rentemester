//! Original public invoice flags and local artifact boundary. No accounting/provider logic.
use crate::dated_reports::Output;
use rentemester_client::{
    ApiClient, OriginalPublicInvoiceKind as Kind, public_invoice_artifact,
    validate_invoice_public_original,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::Path};
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|a| a == "invoice")
        && args
            .get(1)
            .is_some_and(|a| Kind::ALL.iter().any(|k| k.suffix() == a))
}
pub fn help() -> Value {
    json!({"commands":Kind::ALL.map(|k|k.suffix()),"usage":"invoice <command> --server ORIGIN --company SLUG --cookie-file FILE --document-id ID|--invoice-number NUMBER [--json] [--allow-loopback-http]","flags":"export-public/export-public-oioubl require --out FILE; submit-public-peppol requires --access-point FILE [--out FILE]; transmit-digisense requires --confirm yes [--digisense-company-key KEY]","policy":"Access point JSON is metadata only: accessPointId, endpointUrl, senderEndpointId, optional acknowledgement {transmissionId,acknowledgedAt}. No arbitrary URL fetch, provider credentials, local paths or keys are sent. Company key is an assertion against the server binding. Successful artifacts are SHA256 verified before local write. No retries: outcome_unknown requires inspection of canonical submissions before a separate explicit action."})
}
fn output(value: Value, json_mode: bool, exit: u8) -> Output {
    if exit == 0 || json_mode && exit != 2 {
        Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
            stderr: String::new(),
            exit,
        }
    } else {
        Output {
            stdout: String::new(),
            stderr: format!(
                "{}\n",
                value["errors"]
                    .as_array()
                    .map(|a| a
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("\n"))
                    .unwrap_or_default()
            ),
            exit,
        }
    }
}
fn fail(message: &str, json_mode: bool, exit: u8) -> Output {
    output(json!({"ok":false,"errors":[message]}), json_mode, exit)
}
fn access_point(path: &str) -> Result<Value, &'static str> {
    let mut bytes = vec![];
    std::fs::File::open(path)
        .map_err(|_| "Could not open access point JSON")?
        .take(262145)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read access point JSON")?;
    if bytes.len() > 262144 {
        return Err("Access point JSON exceeds256KiB");
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Access point file must be JSON object")?;
    if !value.as_object().is_some_and(|o| {
        o.keys().all(|k| {
            [
                "accessPointId",
                "endpointUrl",
                "senderEndpointId",
                "acknowledgement",
            ]
            .contains(&k.as_str())
        })
    }) {
        return Err("Unknown access point field or non-object JSON");
    }
    Ok(value)
}
pub fn execute(args: &[String]) -> Output {
    if args.iter().skip(2).any(|a| a == "--help") {
        return output(help(), true, 0);
    }
    let kind = Kind::ALL
        .into_iter()
        .find(|k| k.suffix() == args[1])
        .unwrap();
    let (mut json_mode, mut http) = (false, false);
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    while cursor < args.len() {
        let key = args[cursor].as_str();
        if key == "--json" && !json_mode {
            json_mode = true;
            cursor += 1;
            continue;
        }
        if key == "--allow-loopback-http" && !http {
            http = true;
            cursor += 1;
            continue;
        }
        let allowed = [
            "--server",
            "--company",
            "--cookie-file",
            "--document-id",
            "--invoice-number",
        ]
        .contains(&key)
            || key == "--out" && (kind.is_export() || kind == Kind::SubmitPublicPeppol)
            || key == "--access-point" && kind == Kind::SubmitPublicPeppol
            || ["--confirm", "--digisense-company-key"].contains(&key)
                && kind == Kind::TransmitDigisense;
        if !allowed {
            return fail(
                &format!("Unknown/duplicate public invoice flag {key}"),
                false,
                2,
            );
        }
        let Some(value) = args
            .get(cursor + 1)
            .filter(|v| !v.trim().is_empty() && !v.starts_with("--"))
        else {
            return fail(&format!("Missing value for {key}"), false, 2);
        };
        if flags.insert(key, value.as_str()).is_some() {
            return fail(&format!("Duplicate {key}"), false, 2);
        }
        cursor += 2;
    }
    let mut required = vec!["--server", "--company", "--cookie-file"];
    if kind.is_export() {
        required.push("--out");
    }
    if kind == Kind::SubmitPublicPeppol {
        required.push("--access-point");
    }
    if required.iter().any(|k| !flags.contains_key(k)) {
        return fail(&format!("Required {}", required.join(", ")), false, 2);
    }
    if kind == Kind::TransmitDigisense && flags.get("--confirm") != Some(&"yes") {
        return fail("transmit-digisense requires --confirm yes", false, 2);
    }
    let mut body = json!({});
    if let Some(id) = flags.get("--document-id") {
        let Some(id) = id
            .parse::<u64>()
            .ok()
            .filter(|n| (1..=9007199254740991).contains(n))
        else {
            return fail("Positive safe --document-id required", false, 2);
        };
        body["documentId"] = json!(id);
    }
    if let Some(number) = flags.get("--invoice-number") {
        body["invoiceNumber"] = json!(number);
    }
    if !kind.is_export() {
        body["confirm"] = json!(true);
    }
    if kind == Kind::SubmitPublicPeppol {
        let mut config = match access_point(flags["--access-point"]) {
            Ok(v) => v,
            Err(e) => return fail(e, false, 2),
        };
        if let Some(ack) = config.as_object_mut().unwrap().remove("acknowledgement") {
            body["acknowledgement"] = ack;
        }
        body["accessPoint"] = config;
    }
    if let Some(key) = flags.get("--digisense-company-key") {
        body["companyKey"] = json!(key);
    }
    let bytes = serde_json::to_vec(&body).unwrap();
    if let Err(e) = validate_invoice_public_original(kind, &bytes) {
        return fail(&e.to_string(), false, 2);
    }
    let cookie = match rentemester_client::read_cookie_file(Path::new(flags["--cookie-file"])) {
        Ok(v) => v,
        Err(e) => {
            let (exit, e) = crate::remote::client_error(e);
            return output(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
            );
        }
    };
    let api = match ApiClient::new(flags["--server"], http, Some(cookie)) {
        Ok(v) => v,
        Err(e) => {
            let (exit, e) = crate::remote::client_error(e);
            return output(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
            );
        }
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(v) => v,
        Err(_) => return fail("HTTP runtime unavailable", json_mode, 1),
    };
    match rt.block_on(api.invoice_public_original(flags["--company"], kind, bytes)) {
        Ok(mut value) => {
            if value["ok"] != true {
                return output(value, json_mode, 1);
            }
            if let Some(path) = flags.get("--out") {
                let artifact = match public_invoice_artifact(kind, &value) {
                    Ok(v) => v,
                    Err(_) => {
                        return fail(
                            "Artifact SHA256 mismatch; no local output written",
                            json_mode,
                            1,
                        );
                    }
                };
                if std::fs::write(path, artifact.as_bytes()).is_err() {
                    return fail(
                        "Server operation succeeded but local artifact could not be written; inspect canonical state before any further action",
                        json_mode,
                        1,
                    );
                }
                value["outPath"] = json!(path);
            }
            output(value, json_mode, 0)
        }
        Err(e) => {
            let (exit, e) = crate::remote::client_error(e);
            output(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
            )
        }
    }
}
