//! Original eight document commands, local snapshots and the common authenticated SDK.
use crate::dated_reports::Output;
use rentemester_client::{DocumentsOriginalMutation as Mutation, documents_original_file};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal, path::Path};
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "documents")
        && args.get(1).is_some_and(|s| {
            matches!(
                s.as_str(),
                "ingest"
                    | "list"
                    | "enrich"
                    | "set-company-context"
                    | "parse"
                    | "parse-pending"
                    | "parse-status"
                    | "parsed-text"
            )
        })
}
pub fn help() -> Value {
    json!({"ok":true,"commands":["documents ingest --file PATH --metadata PATH [--vendor-id ID] [--force]","documents list","documents enrich --document-id ID --metadata PATH --confirm yes","documents set-company-context --document-id ID --source-reference TEXT --business-use-reason TEXT --confirm yes","documents parse --document-id ID --confirm yes","documents parse-pending [--limit N --cursor N] --confirm yes","documents parse-status --document-id ID","documents parsed-text --document-id ID [--offset N --limit N]"],"connection":"--company SLUG; --server ORIGIN or --local-profile PATH; --cookie-file PATH or --service-key-file PATH; --allow-loopback-http; --json or --format json|human","native":crate::invoice_extraction::help(),"authority":"Ingest invocation confirms. Other writes require exact --confirm yes. Metadata is a bounded local JSON file; no stdin, actor, key or automatic retry."})
}
fn usage(s: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{s}\n"),
        exit: 2,
    }
}
fn parse(args: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    while cursor < args.len() {
        let (key, attached) = crate::original_args::option(&args[cursor]);
        let global = matches!(
            key,
            "--company"
                | "--server"
                | "--cookie-file"
                | "--service-key-file"
                | "--local-profile"
                | "--format"
                | "--json"
                | "--help"
                | "--allow-loopback-http"
        );
        let domain = match args[1].as_str() {
            "ingest" => matches!(key, "--file" | "--metadata" | "--vendor-id" | "--force"),
            "enrich" => matches!(key, "--document-id" | "--metadata" | "--confirm"),
            "set-company-context" => matches!(
                key,
                "--document-id" | "--source-reference" | "--business-use-reason" | "--confirm"
            ),
            "parse" => matches!(key, "--document-id" | "--confirm"),
            "parse-pending" => matches!(key, "--cursor" | "--limit" | "--confirm"),
            "parse-status" => key == "--document-id",
            "parsed-text" => matches!(key, "--document-id" | "--offset" | "--limit"),
            _ => false,
        };
        if !global && !domain {
            return Err(usage(&format!("Unknown documents flag {key}")));
        }
        let (raw, next) = if matches!(
            key,
            "--json" | "--help" | "--force" | "--allow-loopback-http"
        ) {
            if attached.is_some() {
                return Err(usage("Boolean flag takes no value"));
            }
            ("true", cursor + 1)
        } else {
            crate::original_args::value(args, cursor).ok_or_else(|| usage("Missing flag value"))?
        };
        if flags.insert(key, raw).is_some() {
            return Err(usage("Duplicate documents flag"));
        }
        cursor = next;
    }
    if flags
        .get("--format")
        .is_some_and(|s| !matches!(*s, "json" | "human"))
    {
        return Err(usage("--format must be json or human"));
    }
    if flags.contains_key("--help") {
        return Ok(flags);
    }
    let required: &[&str] = match args[1].as_str() {
        "ingest" => &["--file", "--metadata"],
        "enrich" => &["--document-id", "--metadata"],
        "set-company-context" => &[
            "--document-id",
            "--source-reference",
            "--business-use-reason",
        ],
        "parse" | "parse-status" | "parsed-text" => &["--document-id"],
        _ => &[],
    };
    if required
        .iter()
        .any(|k| flags.get(k).is_none_or(|s| s.trim().is_empty()))
    {
        return Err(usage(
            "Missing required documents flag; see documents --help",
        ));
    }
    if matches!(
        args[1].as_str(),
        "enrich" | "set-company-context" | "parse" | "parse-pending"
    ) && flags.get("--confirm") != Some(&"yes")
    {
        return Err(usage("documents write requires exact --confirm yes"));
    }
    for key in [
        "--document-id",
        "--vendor-id",
        "--cursor",
        "--offset",
        "--limit",
    ] {
        if let Some(s) = flags.get(key) {
            let max = if key == "--limit" {
                if args[1] == "parse-pending" { 100 } else { 10 }
            } else {
                9_007_199_254_740_991
            };
            let min = if matches!(key, "--offset" | "--cursor") {
                0
            } else {
                1
            };
            if s.parse::<u64>().ok().is_none_or(|n| n < min || n > max) {
                return Err(usage(&format!("Invalid bounded integer {key}")));
            }
        }
    }
    for key in ["--source-reference", "--business-use-reason"] {
        if flags.get(key).is_some_and(|s| s.len() > 2000) {
            return Err(usage("Context text exceeds 2000 bytes"));
        }
    }
    Ok(flags)
}
pub fn preflight(args: &[String]) -> Result<(), Output> {
    parse(args).map(|_| ())
}
const FIELDS: &[(&str, &str)] = &[
    ("id", "id"),
    ("document_no", "documentNo"),
    ("source", "source"),
    ("original_filename", "originalFilename"),
    ("document_type", "documentType"),
    ("invoice_date", "invoiceDate"),
    ("amount_inc_vat", "amountIncVat"),
    ("currency", "currency"),
    ("status", "status"),
    ("stored_path", "storedPath"),
    ("sender_vat_cvr", "senderVatOrCvr"),
    ("supplier_country_code", "supplierCountryCode"),
    ("supplier_identifier_kind", "supplierIdentifierKind"),
    ("supplier_identity_status", "supplierIdentityStatus"),
    ("source_bank_transaction_id", "sourceBankTransactionId"),
    ("accounting_rationale", "accountingRationale"),
    ("prepared_by", "preparedBy"),
    ("prepared_by_program", "preparedByProgram"),
    ("purchase_vat_lines", "purchaseVatLines"),
];
fn display(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        s.to_owned()
    } else if v.is_null() {
        "—".into()
    } else {
        v.to_string()
    }
}
fn human_list(rows: &[Value]) -> String {
    let mut lines = vec![format!("Bilag ({})", rows.len())];
    if rows.is_empty() {
        lines.push("Ingen bilag gemt.".into());
    }
    for r in rows {
        lines.extend([
            String::new(),
            format!(
                "#{} — {}",
                display(&r["documentNo"]),
                display(&r["originalFilename"])
            ),
            format!(
                "  Bilagsdato: {} | Kilde: {}",
                display(&r["invoiceDate"]),
                display(&r["source"])
            ),
        ]);
        if r["documentType"] == "internal_voucher" {
            lines.push(format!(
                "  Internt bilag: bankpost #{} | Udarbejdet af: {}",
                display(&r["sourceBankTransactionId"]),
                display(&r["preparedBy"])
            ));
            lines.push(format!(
                "  Begrundelse: {}",
                display(&r["accountingRationale"])
            ));
        }
        if [
            "supplierCountryCode",
            "supplierIdentifierKind",
            "supplierIdentityStatus",
        ]
        .iter()
        .any(|k| !r[*k].is_null())
        {
            lines.push(format!(
                "  Leverandøridentitet: {} · {} · {}",
                display(&r["supplierCountryCode"]),
                display(&r["supplierIdentifierKind"]),
                display(&r["supplierIdentityStatus"])
            ));
        }
        let amount = r["amountIncVat"].as_f64().map_or("—".into(), kroner);
        let currency = r["currency"].as_str().unwrap_or("DKK").to_uppercase();
        lines.push(format!(
            "  Beløb (inkl. moms): {amount}{}",
            if currency == "DKK" {
                String::new()
            } else {
                format!(" {currency}")
            }
        ));
        lines.push(format!("  Status: {}", display(&r["status"])));
    }
    lines.join("\n")
}
pub fn execute(args: &[String]) -> Output {
    let flags = match parse(args) {
        Ok(f) => f,
        Err(o) => return o,
    };
    if flags.contains_key("--help") {
        return Output {
            stdout: format!("{}\n", help()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let json_mode = flags.contains_key("--json")
        || flags.get("--format") == Some(&"json")
        || (flags.get("--format") != Some(&"human") && !std::io::stdout().is_terminal());
    for key in ["--server", "--company"] {
        if flags.get(key).is_none_or(|s| s.is_empty()) {
            return usage(&format!("Explicit {key} required"));
        }
    }
    let mut body = json!({"confirm":true});
    // All confirmation/argv checks above precede any metadata/file/credential access.
    if matches!(args[1].as_str(), "ingest" | "enrich") {
        let (_, bytes) = match documents_original_file(Path::new(flags["--metadata"])) {
            Ok(v) => v,
            Err(_) => return usage("Could not snapshot bounded regular metadata file"),
        };
        if bytes.len() > 262_144 {
            return usage("Metadata exceeds 262144 bytes");
        }
        body["metadata"] = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => return usage("Metadata file must contain JSON"),
        };
    }
    if args[1] == "ingest" {
        let (name, bytes) = match documents_original_file(Path::new(flags["--file"])) {
            Ok(v) => v,
            Err(_) => return usage("Could not snapshot bounded regular original file"),
        };
        // Base64 is exposed by the existing client-independent utility below.
        body["fileName"] = json!(name);
        body["fileBase64"] = json!(encode(&bytes));
        if let Some(id) = flags.get("--vendor-id") {
            body["vendorId"] = json!(id.parse::<i64>().unwrap());
        }
        if flags.contains_key("--force") {
            body["force"] = json!(true);
        }
    }
    if args[1] == "set-company-context" {
        body["sourceReference"] = json!(flags["--source-reference"]);
        body["businessUseReason"] = json!(flags["--business-use-reason"]);
    }
    let api = match crate::remote::credential_client(
        flags["--server"],
        flags.contains_key("--allow-loopback-http"),
        &flags,
        false,
    ) {
        Ok(a) => a,
        Err((exit, e)) => {
            return failure(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
            );
        }
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => {
            return failure(
                json!({"ok":false,"errors":["HTTP runtime unavailable"]}),
                json_mode,
                1,
            );
        }
    };
    let id = flags
        .get("--document-id")
        .map_or(0, |s| s.parse::<i64>().unwrap());
    let number = |key, default| {
        flags
            .get(key)
            .map_or(default, |s| s.parse::<u64>().unwrap())
    };
    let result = runtime.block_on(async {
        let c = flags["--company"];
        match args[1].as_str() {
            "list" => api.documents_original_list(c).await,
            "parse" => api.document_parse(c, id, true).await,
            "parse-status" => api.document_parse_status(c, id).await,
            "parsed-text" => {
                api.document_parsed_text(c, id, number("--offset", 0), number("--limit", 10) as u8)
                    .await
            }
            "parse-pending" => {
                api.documents_parse_pending(
                    c,
                    number("--cursor", 0),
                    number("--limit", 100) as u8,
                    true,
                )
                .await
            }
            op => {
                api.documents_original_mutation(
                    c,
                    match op {
                        "ingest" => Mutation::Ingest,
                        "enrich" => Mutation::Enrich(id),
                        _ => Mutation::CompanyContext(id),
                    },
                    serde_json::to_vec(&body).unwrap(),
                    true,
                )
                .await
            }
        }
    });
    let value = match result {
        Ok(v) => v,
        Err(e) => {
            let (exit, e) = crate::remote::client_error(e);
            return failure(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
            );
        }
    };
    if value["ok"] == false {
        return failure(value, json_mode, 1);
    }
    let rendered = if args[1] == "list" {
        let rows = value["documents"].as_array().unwrap();
        if json_mode {
            let rows: Vec<Value> = rows
                .iter()
                .map(|r| {
                    FIELDS
                        .iter()
                        .map(|(a, b)| ((*a).to_owned(), r[*b].clone()))
                        .collect::<serde_json::Map<_, _>>()
                        .into()
                })
                .collect();
            serde_json::to_string_pretty(&rows).unwrap()
        } else {
            human_list(rows)
        }
    } else if json_mode {
        serde_json::to_string_pretty(&value).unwrap()
    } else {
        format!(
            "✔ {}",
            value["message"]
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned)
        )
    };
    Output {
        stdout: format!("{rendered}\n"),
        stderr: String::new(),
        exit: 0,
    }
}
fn failure(v: Value, json_mode: bool, exit: u8) -> Output {
    if exit == 2 {
        return usage(&display(&v["errors"]));
    }
    Output {
        stdout: if json_mode {
            format!("{v}\n")
        } else {
            format!("✘ {}\n", display(&v["errors"]))
        },
        stderr: String::new(),
        exit,
    }
}
fn encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(TABLE[(n >> 18) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn kroner(n: f64) -> String {
    // Source rounds the shortest decimal representation half-up before formatting.
    let raw = n.abs().to_string();
    let (whole, fraction) = raw.split_once('.').unwrap_or((&raw, ""));
    let mut ore = whole.parse::<u128>().unwrap_or(0).saturating_mul(100);
    let mut digits = fraction.bytes().chain(std::iter::repeat(b'0'));
    ore = ore.saturating_add(
        u128::from(digits.next().unwrap() - b'0') * 10 + u128::from(digits.next().unwrap() - b'0'),
    );
    if digits.next().unwrap() >= b'5' {
        ore = ore.saturating_add(1);
    }
    let whole = (ore / 100).to_string();
    let mut grouped = String::new();
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(c);
    }
    format!(
        "{}{grouped},{:02} kr.",
        if n < 0.0 && ore > 0 { "-" } else { "" },
        ore % 100
    )
}
