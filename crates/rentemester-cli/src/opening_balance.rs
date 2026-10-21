//! Explicit read/plan/apply; no generated source facts, actor, key or retry.
use rentemester_client::{ApiClient, MAX_OPENING_BALANCE_BODY};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
pub const COMMANDS: [&str; 3] = [
    "opening-balance",
    "opening-balance-plan",
    "opening-balance-apply",
];
pub fn accepts(name: &str) -> bool {
    COMMANDS.contains(&name)
}
pub fn help() -> Value {
    json!({"commands":COMMANDS,"usage":"Explicit --server/--cookie-file/--company only. opening-balance reads current attested opening; opening-balance-plan reads exact64KiB native stdin Input, readonly with no --confirm; opening-balance-apply requires bare --confirm plus exact {input,planHash,idempotencyKey,confirm:true}. Review server plan/account amounts/cutover/citations first. Server owns current human authority/atomic entry/proof/receipt, no inferred archive amounts, actor/defaults/keys/retry. Read canonical opening/journal after outcome_unknown; retain exact original input/hash/key for deliberate authorized receipt recovery. For original synthetic CSV use --file PATH on plan; apply also requires --plan-hash HASH --idempotency-key KEY and --confirm. Raw UTF-8 is sent unchanged for server parsing, exact source retained with opening; no client accounting. No year/account/id/source selectors.","planSchema":rentemester_client::opening_balance_body_schema(false),"applySchema":rentemester_client::opening_balance_body_schema(true)})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    let invalid = |m| (2, Error::new("invalid_input", m));
    let apply = command == "opening-balance-apply";
    if confirmed != apply
        || preview
        || flags.keys().any(|k| {
            ![
                "--server",
                "--cookie-file",
                "--company",
                "--file",
                "--plan-hash",
                "--idempotency-key",
            ]
            .contains(k)
        })
    {
        return Err(invalid(
            "exact opening selectors only; confirmation belongs exclusively to apply",
        ));
    }
    if (flags.contains_key("--file") && command == "opening-balance")
        || ((!apply || !flags.contains_key("--file"))
            && (flags.contains_key("--plan-hash") || flags.contains_key("--idempotency-key")))
    {
        return Err(invalid(
            "CSV selectors belong only to source plan or explicitly keyed source apply",
        ));
    }
    let required = |k| {
        flags
            .get(k)
            .copied()
            .ok_or_else(|| invalid("explicit server/cookie/company required"))
    };
    if apply
        && flags.contains_key("--file")
        && (!flags.contains_key("--plan-hash") || !flags.contains_key("--idempotency-key"))
    {
        return Err(invalid(
            "CSV apply requires explicit --plan-hash and --idempotency-key",
        ));
    }
    let cookie =
        rentemester_client::read_cookie_file(std::path::Path::new(required("--cookie-file")?))
            .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(required("--server")?, allow_http, Some(cookie))
        .map_err(crate::remote::client_error)?;
    let company = required("--company")?;
    let rt = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let value = if command == "opening-balance" {
        rt.block_on(api.opening_balance(company))
    } else {
        let mut bytes = vec![];
        if let Some(path) = flags.get("--file") {
            let source = csv_source(std::path::Path::new(path)).map_err(&invalid)?;
            bytes = if apply {
                serde_json::to_vec(&json!({"input":source,"planHash":required("--plan-hash")?,"idempotencyKey":required("--idempotency-key")?,"confirm":true}))
            } else { serde_json::to_vec(&source) }.map_err(|_| invalid("could not encode CSV source"))?;
        } else {
            std::io::stdin().take(MAX_OPENING_BALANCE_BODY as u64 + 1).read_to_end(&mut bytes)
                .map_err(|_| invalid("could not read native opening JSON"))?;
        }
        if apply {
            rt.block_on(api.opening_balance_apply(company, bytes, confirmed))
        } else {
            rt.block_on(api.opening_balance_plan(company, bytes))
        }
    }
    .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}

fn csv_source(path: &std::path::Path) -> Result<Value, &'static str> {
    let mut file = std::fs::File::open(path).map_err(|_| "could not open CSV source")?;
    if !file
        .metadata()
        .map_err(|_| "could not inspect CSV source")?
        .is_file()
    {
        return Err("CSV source must be a regular file");
    }
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(48 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "could not read CSV source")?;
    if bytes.is_empty() || bytes.len() > 48 * 1024 {
        return Err("CSV source maximum48KiB");
    }
    let content = String::from_utf8(bytes).map_err(|_| "CSV source must be UTF-8")?;
    Ok(json!({"sourceSystem":"synthetic-csv","content":content}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_csv_snapshot_preserves_utf8_bom_crlf_and_refuses_invalid_or_oversized_text() {
        let path =
            std::env::temp_dir().join(format!("rentemester-synthetic-csv-{}", std::process::id()));
        let source = "\u{feff}# cutOverDate: 2024-07-01\r\nsection,accountNo,name,debit,credit\r\n";
        std::fs::write(&path, source).unwrap();
        assert_eq!(csv_source(&path).unwrap()["content"], source);
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(csv_source(&path).is_err());
        std::fs::write(&path, vec![b'x'; 48 * 1024 + 1]).unwrap();
        assert!(csv_source(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
