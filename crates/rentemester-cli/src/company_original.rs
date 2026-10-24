//! Original company CLI, joined only to the current authenticated common server.
use crate::dated_reports::Output;
use rentemester_client::{ApiClient, CompanySetupMutation, Error};
use serde_json::{Value, json};
use std::collections::BTreeMap;
const OPS: &[&str] = &["add", "list", "sync-cvr", "set-profile", "profile"];
const IDENTITY: &[(&str, &str)] = &[
    ("--name", "name"),
    ("--cvr", "cvr"),
    ("--address", "address"),
    ("--postal-code", "postalCode"),
    ("--city", "city"),
    ("--payment-terms", "paymentTermsDays"),
];
const BANK: &[(&str, &str)] = &[
    ("--bank-name", "bankName"),
    ("--bank-reg", "registrationNo"),
    ("--bank-account", "accountNo"),
    ("--iban", "iban"),
];
fn usage(s: impl AsRef<str>) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{}\n", s.as_ref()),
        exit: 2,
    }
}
pub fn accepts(a: &[String]) -> bool {
    a.first().is_some_and(|s| s == "company")
}
pub fn needs_company(a: &[String]) -> bool {
    !a.get(1)
        .is_some_and(|s| matches!(s.as_str(), "add" | "list"))
}
pub fn help() -> Value {
    json!({"commands":["company add --name TEXT [--slug SLUG] [--cvr TEXT --address TEXT --postal-code TEXT --city TEXT --payment-terms 0-365] [--bank-name TEXT --bank-reg TEXT --bank-account TEXT --iban TEXT] [--fiscal-year-start-month 1-12 --fiscal-year-label-strategy end-year|start-year|span]","company list","company sync-cvr --company SLUG|PATH","company set-profile --company SLUG|PATH [--name TEXT --cvr TEXT --address TEXT --postal-code TEXT --city TEXT --payment-terms 0-365 --vat-period month|quarter|half-year|none --no-vat --bank-name TEXT --bank-reg TEXT --bank-account TEXT --iban TEXT]","company profile --company SLUG|PATH"],"connection":"--server ORIGIN or --local-profile ABS_PRIVATE_PROFILE; --cookie-file PRIVATE_FILE; --workspace is exact verified local workspace equality only; --json or --format human|json; --allow-loopback-http","example":"company set-profile --company <sti-eller-slug> --bank-name <bank> --bank-reg <regnr> --bank-account <kontonr> --iban <IBAN>. Only supplied fields change. Pass --example for this template.","controls":"Current human cookie; optional --actor user:ID equality assertion. Explicit write command is intent, native confirmation is retained. Current Admin/WorkspaceManage, backup and writer gates remain. Bank append-only and VAT activity guards are server-owned. No retry: outcome_unknown requires companies/profile readback before another explicit action.","blockers":"Connected creation is NOT first-workspace bootstrap: provision an existing operator/runtime separately. No autostart or local database fallback."})
}
fn parse(a: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    if a == ["company"] || a == ["company", "--help"] {
        return Ok(BTreeMap::from([("--help", "true")]));
    }
    let op = a
        .get(1)
        .map(String::as_str)
        .filter(|s| OPS.contains(s))
        .ok_or_else(|| usage("Unknown company command; see company --help"))?;
    let mut f = BTreeMap::new();
    let mut i = 2;
    while i < a.len() {
        let (k, attached) = crate::original_args::option(&a[i]);
        let allowed = matches!(
            k,
            "--server"
                | "--local-profile"
                | "--workspace"
                | "--cookie-file"
                | "--company"
                | "--actor"
                | "--json"
                | "--format"
                | "--help"
                | "--allow-loopback-http"
        ) || op == "set-profile"
            && matches!(k, "--vat-period" | "--no-vat" | "--example")
            || matches!(op, "add" | "set-profile")
                && (IDENTITY.iter().chain(BANK).any(|(flag, _)| *flag == k))
            || op == "add"
                && matches!(
                    k,
                    "--slug" | "--fiscal-year-start-month" | "--fiscal-year-label-strategy"
                );
        if !allowed {
            return Err(usage(format!("Unknown company flag {k}")));
        }
        let (v, next) = if matches!(
            k,
            "--json" | "--help" | "--example" | "--no-vat" | "--allow-loopback-http"
        ) {
            if attached.is_some() {
                return Err(usage("Boolean flag takes no value"));
            }
            ("true", i + 1)
        } else {
            crate::original_args::value(a, i)
                .ok_or_else(|| usage(format!("Missing value for {k}")))?
        };
        if f.insert(k, v).is_some() {
            return Err(usage(format!("Duplicate {k}")));
        }
        i = next;
    }
    if f.get("--format")
        .is_some_and(|v| !matches!(*v, "human" | "json"))
    {
        return Err(usage("--format must be human or json"));
    }
    if f.contains_key("--help") || f.contains_key("--example") {
        return Ok(f);
    }
    let local = f.contains_key("--local-profile");
    if local && (f.contains_key("--server") || f.contains_key("--allow-loopback-http"))
        || !local && f.contains_key("--workspace")
    {
        return Err(usage(
            "--workspace requires verified --local-profile; explicit transports are exclusive",
        ));
    }
    if needs_company(a) && f.get("--company").is_none_or(|s| s.is_empty()) {
        return Err(usage("Explicit --company required"));
    }
    if !local && !f.contains_key("--server") {
        return Err(usage(
            "Existing runtime required: explicit --server or verified --local-profile; first-workspace bootstrap is a separate root operator prerequisite (no autostart)",
        ));
    }
    if !f.contains_key("--cookie-file") {
        return Err(usage("Explicit private human --cookie-file required"));
    }
    if op == "add" && f.get("--name").is_none_or(|s| s.trim().is_empty()) {
        return Err(usage("company add requires --name <text>"));
    }
    if f.get("--actor")
        .is_some_and(|s| !s.starts_with("user:") || s.len() <= 5)
    {
        return Err(usage("--actor only asserts current cookie user:ID"));
    }
    if let Some(s) = f.get("--server") {
        ApiClient::new(s, f.contains_key("--allow-loopback-http"), None)
            .map_err(|_| usage("Invalid explicit server origin"))?;
    }
    Ok(f)
}
pub fn preflight(a: &[String]) -> Result<bool, Output> {
    parse(a).map(|f| f.contains_key("--help") || f.contains_key("--example"))
}
pub fn binding_refusal(a: &[String]) -> Output {
    emit(
        json!({"ok":false,"errors":["Verified runtime/owner/company binding refused; no operation dispatched"]}),
        machine(a),
        a.get(1).map(String::as_str).unwrap_or("profile"),
    )
}
fn machine(a: &[String]) -> bool {
    a.iter().any(|s| s == "--json" || s == "--format=json")
        || a.windows(2).any(|w| w == ["--format", "json"])
}
fn label(op: &str) -> &str {
    match op {
        "add" => "Opretter en ny virksomhed i workspacet (opretter workspacet ved første kørsel)",
        "list" => "Lister virksomheder i workspacet.",
        "sync-cvr" => {
            "Henter virksomhedens stamdata fra CVR-registret og opdaterer navn, adresse, branche, virksomhedsform og status. Regnskabsåret røres aldrig — et afvigende regnskabsår rapporteres kun."
        }
        "set-profile" => {
            "Retter virksomhedens egen profil efter init: navn, CVR, adresse, betalingsfrist, momsperiode og betalingsoplysninger (bankkonto/IBAN). Hver efterfølgende udstedt faktura og dens PDF arver de nye værdier automatisk — du indtaster aldrig din egen stamdata på en faktura."
        }
        _ => {
            "Viser virksomhedens nuværende profil: navn, CVR, adresse, standard betalingsfrist og momsperiode. Ren læsning — ændrer intet."
        }
    }
}
fn emit(v: Value, machine: bool, op: &str) -> Output {
    let failed = v["ok"] == false;
    if machine {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&v).unwrap()),
            stderr: String::new(),
            exit: u8::from(failed),
        };
    }
    if failed {
        let mut s = format!("✘ {} failed\n", label(op));
        for e in v["errors"].as_array().into_iter().flatten() {
            s.push_str(&format!(
                "  → {}\n",
                e.as_str().unwrap_or("Company operation refused")
            ));
        }
        return Output {
            stdout: String::new(),
            stderr: s,
            exit: 1,
        };
    }
    let first = label(op).split(". ").next().unwrap();
    let heading = if first.chars().count() > 60 {
        format!("{}…", first.chars().take(57).collect::<String>())
    } else {
        first.to_owned()
    };
    let mut s = format!("✔ {heading}\n");
    let data = v.get("profile").unwrap_or(&v);
    for (k, l) in [
        ("message", "Message"),
        ("count", "Antal"),
        ("name", "Navn"),
        ("cvr", "CVR"),
        ("address", "Adresse"),
        ("postalCode", "Postnummer"),
        ("city", "By"),
        ("paymentTermsDays", "Betalingsfrist (dage)"),
        ("vatPeriodType", "Momsperiode"),
        ("vatPeriodLabel", "Momsstatus"),
        ("vatRegistered", "Momsregistreret"),
        ("slug", "Slug"),
    ] {
        let val = &data[k];
        if val.is_null() || val.is_array() || val.is_object() || val == "" {
            continue;
        }
        let text = val
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| val.to_string());
        let text = match (k, text.as_str()) {
            ("vatPeriodType", "month") => "måned",
            ("vatPeriodType", "quarter") => "kvartal",
            ("vatPeriodType", "half-year") => "halvår",
            ("vatRegistered", "true") => "Ja",
            ("vatRegistered", "false") => "Nej",
            _ => &text,
        };
        s.push_str(&format!("  {l}: {text}\n"));
    }
    Output {
        stdout: s,
        stderr: String::new(),
        exit: 0,
    }
}
fn profile(p: &Value) -> Value {
    let mut v = json!({});
    for k in [
        "name",
        "cvr",
        "address",
        "postalCode",
        "city",
        "paymentTermsDays",
        "vatPeriodType",
    ] {
        v[k] = p[k].clone();
    }
    let cadence = p["vatPeriodType"].as_str();
    v["vatRegistered"] = json!(cadence.is_some());
    v["vatPeriodLabel"] = json!(match cadence {
        Some("month") => "måned",
        Some("quarter") => "kvartal",
        Some("half-year") => "halvår",
        _ => "ikke momsregistreret",
    });
    v
}
fn valid_profile(v: &Value) -> bool {
    v.is_object()
        && v["name"].is_string()
        && v["paymentTermsDays"].is_u64()
        && (v["vatPeriodType"].is_null()
            || v["vatPeriodType"]
                .as_str()
                .is_some_and(|s| matches!(s, "month" | "quarter" | "half-year")))
}
// Bind only supplied intent to the canonical native producer. No defaults or keys
// are inferred here; a mismatched post-write receipt is an unknown outcome.
fn matches_intent(body: &Value, company: &Value) -> bool {
    body.as_object().unwrap().iter().all(|(key, expected)| {
        let actual = &company[key];
        if key == "slug" {
            return true;
        } // immutable selector checked on creation receipt
        if key == "payment" {
            return expected.as_object().unwrap().iter().all(|(field, value)| {
                let text = value.as_str().unwrap().trim();
                text.is_empty() || company["payment"][field] == text
            });
        }
        if key == "fiscalYearStartMonth" {
            return expected.as_str().and_then(|s| s.trim().parse::<u64>().ok()) == actual.as_u64();
        }
        if key == "cvr" {
            let text = expected.as_str().unwrap().trim().to_ascii_uppercase();
            return if text.is_empty() {
                actual.is_null()
            } else {
                actual == &json!(format!("DK{}", text.strip_prefix("DK").unwrap_or(&text)))
            };
        }
        if let Some(text) = expected.as_str() {
            let text = text.trim();
            if text.is_empty() {
                actual.is_null()
            } else {
                actual == text
            }
        } else {
            actual == expected
        }
    })
}
pub fn execute(a: &[String]) -> Output {
    let f = match parse(a) {
        Ok(f) => f,
        Err(o) => return o,
    };
    if f.contains_key("--help") || f.contains_key("--example") {
        return Output {
            stdout: format!("{}\n", help()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let op = a[1].as_str();
    let machine = machine(a);
    let fail = |message: String| emit(json!({"ok":false,"errors":[message]}), machine, op);
    let mut body = json!({});
    for (flag, key) in IDENTITY {
        if let Some(v) = f.get(flag) {
            body[*key] = if *flag == "--payment-terms" {
                match v.parse::<u32>() {
                    Ok(n) if n <= 365 => json!(n),
                    _ => {
                        return fail(
                            "paymentTermsDays must be an integer between 0 and 365".into(),
                        );
                    }
                }
            } else {
                json!(v)
            };
        }
    }
    for (flag, key) in [
        ("--slug", "slug"),
        ("--fiscal-year-start-month", "fiscalYearStartMonth"),
        ("--fiscal-year-label-strategy", "fiscalYearLabelStrategy"),
    ] {
        if let Some(v) = f.get(flag) {
            if key == "slug" {
                let slug = v.trim();
                if !slug.is_empty() {
                    body[key] = json!(slug);
                }
            } else {
                body[key] = json!(v);
            }
        }
    }
    let mut payment = json!({});
    for (flag, key) in BANK {
        if let Some(v) = f.get(flag) {
            payment[*key] = json!(v);
        }
    }
    if !payment.as_object().unwrap().is_empty() {
        body["payment"] = payment;
    }
    if f.contains_key("--no-vat") && f.get("--vat-period").is_some_and(|s| *s != "none") {
        return fail("--no-vat kan ikke kombineres med --vat-period <cadence>: vælg enten en momsperiode (month/quarter/half-year) eller --no-vat for en ikke-momsregistreret virksomhed".into());
    }
    if let Some(v) = f.get("--vat-period") {
        if !matches!(*v, "month" | "quarter" | "half-year" | "none") {
            return fail("--vat-period must be one of month, quarter, half-year, or 'none' (alias for --no-vat)".into());
        }
        body["vatPeriodType"] = if *v == "none" { Value::Null } else { json!(v) };
    }
    if f.contains_key("--no-vat") {
        body["vatPeriodType"] = Value::Null;
    }
    let api = match crate::remote::credential_client(
        f["--server"],
        f.contains_key("--allow-loopback-http"),
        &f,
        false,
    ) {
        Ok(api) => api,
        Err((_, e)) => return fail(format!("{}: {}", e.code, e.message)),
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return fail("Runtime unavailable".into()),
    };
    let result=rt.block_on(async {
 if let Some(actor)=f.get("--actor"){let me=api.me().await?;let id=me["user"]["id"].as_str().ok_or(Error::Protocol)?;if *actor!=format!("user:{id}"){return Err(Error::InvalidInput("actor differs from current cookie user"));}}
 match op {
 "list"=>{
 let v=api.companies().await?;let rows=v["companies"].as_array().filter(|_|v["ok"]==true).ok_or(Error::Protocol)?;
 if v["count"].as_u64()!=Some(rows.len() as u64){return Err(Error::Protocol);}
 let mut out=vec![];
 for r in rows {if !r["slug"].is_string()||!r["name"].is_string()||!r["archived"].is_boolean(){return Err(Error::Protocol);}let mut item=json!({"slug":r["slug"],"name":r["name"],"archived":r["archived"]});if let Some(at)=r.get("createdAt"){if !at.is_string(){return Err(Error::Protocol);}item["createdAt"]=at.clone();}out.push(item);}
 let slugs=out.iter().map(|r|r["slug"].as_str().unwrap()).collect::<Vec<_>>().join(", ");
 let message=if out.is_empty(){"Workspace has no companies yet".to_owned()}else{format!("{} {}: {slugs}",out.len(),if out.len()==1{"company"}else{"companies"})};
 // The authenticated API does not expose a canonical filesystem root. Null is an explicit native-profile boundary, never a fabricated remote/local path.
 Ok(json!({"ok":true,"message":message,"workspace":null,"count":out.len(),"companies":out}))},
 "add"=>{let v=api.create_company(serde_json::to_vec(&body).unwrap(),true).await?;if v["company"]["name"].as_str()!=f.get("--name").map(|s|s.trim())||f.get("--slug").filter(|slug|!slug.trim().is_empty()).is_some_and(|slug|v["company"]["slug"].as_str()!=Some(slug.trim())){return Err(Error::OutcomeUnknown);}let p=api.company_profile(v["company"]["slug"].as_str().unwrap()).await.map_err(|_|Error::OutcomeUnknown)?;if p["ok"]!=true||!valid_profile(&p["company"])||!matches_intent(&body,&p["company"]){return Err(Error::OutcomeUnknown);}let has_payment=p["company"]["payment"].as_object().is_some_and(|p|p.values().any(|v|v.as_str().is_some_and(|s|!s.is_empty())));Ok(json!({"hasPaymentDetails":has_payment,"ok":true,"message":format!("Created company '{}'",v["company"]["slug"].as_str().unwrap()),"workspaceCreated":false,"slug":v["company"]["slug"],"name":v["company"]["name"]}))},
 "sync-cvr"=>api.sync_company_cvr(f["--company"],true).await,
 "profile"=>{let v=api.company_profile(f["--company"]).await?;if v["ok"]!=true||!valid_profile(&v["company"]){return Err(Error::Protocol);}Ok(json!({"ok":true,"profile":profile(&v["company"])}))},
 _=>{let before=api.company_profile(f["--company"]).await?;if before["ok"]!=true||!valid_profile(&before["company"]){return Err(Error::Protocol);}let v=api.company_setup_mutation(f["--company"],CompanySetupMutation::UpdateProfile,serde_json::to_vec(&body).unwrap(),true).await?;if v["ok"]!=true||!valid_profile(&v["company"]){return Err(Error::OutcomeUnknown);}if !matches_intent(&body,&v["company"]){return Err(Error::OutcomeUnknown);}
 let mut updated=vec![];for(_,key)in IDENTITY{if body.get(*key).is_some()&&before["company"][*key]!=v["company"][*key]{updated.push(*key);}}if body.get("payment").and_then(Value::as_object).is_some_and(|p|p.values().any(|v|v.as_str().is_some_and(|s|!s.trim().is_empty()))){updated.push("payment");}if body.get("vatPeriodType").is_some(){updated.push("vatPeriodType");}let mut settings=v["company"].clone();settings.as_object_mut().unwrap().remove("payment");settings.as_object_mut().unwrap().remove("createdAt");Ok(json!({"ok":true,"settings":settings,"updatedFields":updated,"errors":[],"vatPeriodType":v["company"]["vatPeriodType"],"vatRegistered":!v["company"]["vatPeriodType"].is_null()}))}
 }
 });
    match result {
        Ok(v) => {
            let warn = op == "add" && v["hasPaymentDetails"] == false;
            let mut out = emit(v, machine, op);
            if warn {
                out.stderr="ADVARSEL — ingen betalingsoplysninger:\n  Du har ikke angivet bank/IBAN. Fakturaer udstedt nu får INGEN\n  betalingsanvisning (BETALING-blok) på PDF'en — kunden kan ikke se,\n  hvor pengene skal hen. Sæt dem med 'rentemester company set-profile'\n  (--bank-name --bank-reg --bank-account --iban), før du sender fakturaer.\n".into();
            }
            out
        }
        Err(e) => {
            let (_, e) = crate::remote::client_error(e);
            fail(format!(
                "{}: {}; no automatic retry; read company list/profile before another explicit write",
                e.code, e.message
            ))
        }
    }
}
