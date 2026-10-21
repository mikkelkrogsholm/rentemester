//! Explicit latest-year Dinero commands; exact sources and keys remain caller-owned.
use rentemester_client::{ApiClient, MAX_DINERO_IMPORT_BODY};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
mod local;
pub const COMMANDS: [&str; 5] = [
    "dinero-import-plan",
    "dinero-import-apply",
    "dinero-imports",
    "dinero-import-original",
    "imported-receivables",
];
pub fn accepts(name: &str) -> bool {
    COMMANDS.contains(&name)
}
pub fn help() -> Value {
    json!({"commands":COMMANDS,"input":"Explicit --server/--cookie-file/--company. dinero-import-plan/apply optionally --file PATH (local ZIP exact bytes or regular-file directory snapshot). File apply requires explicit --plan-hash HASH --idempotency-key KEY --confirm, no automatic planning or retries; file plan refuses write flags. No local paths go to the server. Without --file, dinero-import-plan reads exact native35MiB stdin JSON {fileName,contentBase64}; dinero-import-apply additionally --confirm and exact body planHash/idempotencyKey/confirm:true. Server atomically imports reviewed latest-year journals and source schedules. imported-receivables requires --as-of YYYY-MM-DD; dinero-import-original --attempt-id ID returns exact original application/json or application/zip integer bytes. No parsing/math/generatedkeys/automatic retry; unknown apply requires canonical dinero-imports/source-hash reconciliation before deliberate exact-key action.","planSchema":rentemester_client::dinero_import_body_schema(false),"applySchema":rentemester_client::dinero_import_body_schema(true)})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    let invalid = |s| (2, Error::new("invalid_input", s));
    let (apply, source, file_mode) =
        admit(command, flags, confirmed, preview).map_err(|e| (2, e))?;
    let required = |k| {
        flags.get(k).copied().ok_or_else(|| {
            invalid("explicit server/cookie/company and documented selector required")
        })
    };
    let server = required("--server")?;
    let company = required("--company")?;
    let cookie =
        rentemester_client::read_cookie_file(std::path::Path::new(required("--cookie-file")?))
            .map_err(crate::remote::client_error)?;
    let api =
        ApiClient::new(server, allow_http, Some(cookie)).map_err(crate::remote::client_error)?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let value = if source {
        let bytes = if file_mode {
            let (name, raw) =
                local::source(std::path::Path::new(required("--file")?)).map_err(|e| (2, e))?;
            ApiClient::dinero_import_source_body(
                &name,
                &raw,
                if apply {
                    Some((required("--plan-hash")?, required("--idempotency-key")?))
                } else {
                    None
                },
            )
            .map_err(crate::remote::client_error)?
        } else {
            let mut bytes = vec![];
            std::io::stdin()
                .take(MAX_DINERO_IMPORT_BODY as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| invalid("could not read native dinero JSON"))?;
            bytes
        };
        runtime.block_on(api.dinero_import(company, bytes, apply, confirmed))
    } else if command == "dinero-imports" {
        runtime.block_on(api.dinero_imports(company))
    } else if command == "imported-receivables" {
        runtime.block_on(api.imported_receivables(company, required("--as-of")?))
    } else {
        let id = required("--attempt-id")?
            .parse::<i64>()
            .map_err(|_| invalid("positive safe import ID required"))?;
        runtime.block_on(api.dinero_import_original(company, id))
    }
    .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}

fn admit(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    confirmed: bool,
    preview: bool,
) -> Result<(bool, bool, bool), Error> {
    let apply = command == "dinero-import-apply";
    let source = command == "dinero-import-plan" || apply;
    let file_mode = flags.contains_key("--file");
    if confirmed != apply
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k)
                || *k == "--as-of" && command == "imported-receivables"
                || *k == "--attempt-id" && command == "dinero-import-original"
                || *k == "--file" && source
                || ["--plan-hash", "--idempotency-key"].contains(k) && apply && file_mode)
        })
    {
        return Err(Error::new(
            "invalid_input",
            "exact documented Dinero selectors; only apply requires confirmation",
        ));
    }

    if apply && file_mode {
        let hash = flags.get("--plan-hash").copied().unwrap_or("");
        let key = flags.get("--idempotency-key").copied().unwrap_or("");
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || key.is_empty()
        {
            return Err(Error::new(
                "invalid_input",
                "file apply requires exact64hex reviewed plan hash and nonempty caller key",
            ));
        }
    }
    Ok((apply, source, file_mode))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_selector_admission_keeps_plan_readonly_and_apply_explicit() {
        let file = BTreeMap::from([("--file", "/local/source.zip")]);
        assert_eq!(
            admit("dinero-import-plan", &file, false, false).unwrap(),
            (false, true, true)
        );
        assert!(admit("dinero-import-plan", &file, true, false).is_err());
        assert!(admit("dinero-import-apply", &file, true, false).is_err());
        let hash = "a".repeat(64);
        let apply = BTreeMap::from([
            ("--file", "/local/source.zip"),
            ("--plan-hash", hash.as_str()),
            ("--idempotency-key", "caller"),
        ]);
        assert_eq!(
            admit("dinero-import-apply", &apply, true, false).unwrap(),
            (true, true, true)
        );
        assert!(admit("dinero-import-plan", &apply, false, false).is_err());
        assert!(admit("dinero-imports", &file, false, false).is_err());
        let key_without_file =
            BTreeMap::from([("--plan-hash", "reviewed"), ("--idempotency-key", "caller")]);
        assert!(admit("dinero-import-apply", &key_without_file, true, false).is_err());
        assert_eq!(
            admit("dinero-import-apply", &BTreeMap::new(), true, false).unwrap(),
            (true, true, false)
        );
    }
}
