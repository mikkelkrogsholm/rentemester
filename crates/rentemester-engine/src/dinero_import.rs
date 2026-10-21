//! Source-derived latest-year Dinero chart, opening, vouchers and declared controls.
//! Pure advice: a parsed plan grants no journal or source-attestation authority.
use crate::{
    Error,
    archive_import::ArchiveDecimal,
    fiscal_year::Date,
    imported_receivables::{self, ValidatedSchedule},
    money::Money,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
mod csv;
mod postings;
#[cfg(test)]
mod tests;
pub const CONTRACT: &str = "rentemester-dinero-latest-year/1";
#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: String,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedImport {
    pub parser_contract: String,
    pub declared_currency: String,
    pub latest_year: u16,
    pub cut_over_date: Option<Date>,
    pub control_date: Option<Date>,
    pub chart: Vec<Account>,
    pub company_master: Value,
    pub role_proposals: Vec<RoleProposal>,
    pub opening_lines: Vec<Line>,
    pub vouchers: Vec<Voucher>,
    pub controls: Vec<Control>,
    pub schedule: Option<ValidatedSchedule>,
    pub unmapped_vat_codes: Vec<String>,
    pub notices: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub account_no: String,
    pub name: String,
    pub kind: String,
    pub normal_balance: String,
    pub default_vat_code: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleProposal {
    pub role: String,
    pub account_no: String,
    pub source: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub account_no: String,
    pub debit_amount: Money,
    pub credit_amount: Money,
    pub vat_code: Option<String>,
    pub text: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Voucher {
    pub voucher_ref: String,
    pub transaction_date: Date,
    pub text: String,
    pub entry_type: String,
    pub lines: Vec<Line>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Control {
    pub account_no: String,
    pub kind: String,
    pub amount: Money,
    pub source_reference: String,
}
fn refused(s: impl Into<String>) -> Error {
    Error::new("dinero_import_refused", s)
}
pub(super) fn decimal(s: &str) -> Result<Money, Error> {
    ArchiveDecimal::parse_csv(s)
        .map(|v| v.money())
        .map_err(|e| refused(e.message))
}
pub(super) fn vat(s: &str) -> Option<&'static str> {
    fn mapped(s: &str) -> Option<&'static str> {
        match s {
            "U25" | "Dansk salgsmoms" => Some("DK_SALE_25"),
            "I25" | "Dansk købsmoms" => Some("DK_PURCHASE_25"),
            "IEUY" | "Ydelseskøb EU (rubrik A - ydelser)" => Some("EU_SERVICE_REVERSE_CHARGE"),
            "IVY" | "Ydelseskøb fra verden" => Some("NON_EU_SERVICE_REVERSE_CHARGE"),
            "REP" | "Repræsentation (kvartmoms)" => Some("REPRESENTATION_SPECIAL"),
            _ => None,
        }
    }
    let s = s.trim();
    mapped(s).or_else(|| mapped(s.split_once('-').map_or(s, |(code, _)| code.trim())))
}

fn classification(t: &str, no: &str) -> Option<(&'static str, &'static str)> {
    match no {
        "64000" | "64020" | "64040" => return Some(("vat", "credit")),
        "64060" => return Some(("vat", "debit")),
        _ => {}
    }
    match t.to_lowercase().as_str() {
        "aktiv" => Some(("asset", "debit")),
        "indtægt" | "indtaegt" => Some(("income", "credit")),
        "udgift" => Some(("expense", "debit")),
        "passiv" => Some((
            if no
                .parse::<u32>()
                .is_ok_and(|n| (60000..=60040).contains(&n))
            {
                "equity"
            } else {
                "liability"
            },
            "credit",
        )),
        _ => None,
    }
}
fn proposals(chart: &[Account]) -> Vec<RoleProposal> {
    let mut p = vec![];
    for a in chart {
        let mut add = |role: &str, evidence: String| {
            p.push(RoleProposal {
                role: role.into(),
                account_no: a.account_no.clone(),
                source: format!("dinero:chart:{evidence}"),
            })
        };
        let n = a.name.to_lowercase();
        let control = match a.account_no.as_str() {
            "64000" => Some("output_vat"),
            "64020" | "64040" => Some("reverse_charge_vat"),
            "64060" => Some("input_vat"),
            _ => None,
        };
        if let Some(role) = control {
            add(role, format!("control-account-{}", a.account_no));
            continue;
        }
        let words = n.split(|c: char| !c.is_alphanumeric()).collect::<Vec<_>>();
        if a.kind == "asset" && words.iter().any(|w| ["bank", "bankkonto"].contains(w)) {
            add("bank", "name-bank".into());
        }
        if a.kind == "asset"
            && (n.contains("debitor")
                || n.contains("kundetilgodehavende")
                || n.find("tilgodehavende").is_some_and(|i| {
                    let s = &n[i..];
                    s.contains("kunde") || s.contains("salg")
                }))
        {
            add("debtors", "name-debtors".into());
        }
        if a.kind == "liability" && (n.contains("kreditor") || n.contains("leverandørgæld")) {
            add("creditors", "name-creditors".into());
        }
        if a.kind == "liability" && (n.contains("skyldig moms") || n.contains("momsafregning")) {
            add("vat_settlement", "name-vat-settlement".into());
        }
        if a.kind == "expense"
            && ["drift", "administration", "kontor"]
                .iter()
                .any(|s| n.contains(s))
        {
            add("operational_default", "name-operational".into());
        }
    }
    p
}
fn year(path: &str, file: &str) -> Option<u16> {
    let (dir, name) = path.rsplit_once('/')?;
    if !name.eq_ignore_ascii_case(file) {
        return None;
    }
    let y = dir.rsplit('/').next()?;
    if y.len() != 4 {
        return None;
    }
    y.parse().ok().filter(|y| *y > 0)
}
fn latest<'a>(files: &'a [SourceFile], name: &str) -> Option<&'a SourceFile> {
    files
        .iter()
        .filter(|f| year(&f.path, name).is_some())
        .max_by(|a, b| a.path.cmp(&b.path))
}
fn master(f: Option<&SourceFile>) -> Result<Value, Error> {
    let Some(f) = f else {
        return Ok(json!({}));
    };
    let r = csv::records(&f.bytes)?;
    if r.len() != 2 {
        return Err(refused(
            "Firmaoplysninger.csv needs exactly one company row",
        ));
    }
    let mut o = serde_json::Map::new();
    for (source, key) in [
        ("Firmanavn", "name"),
        ("CvrNr", "cvr"),
        ("Adresse", "address"),
        ("Postnr", "postalCode"),
        ("By", "city"),
        ("Land", "country"),
        ("Email", "email"),
        ("Telefonnummer", "phone"),
        ("Hjemmeside", "website"),
    ] {
        if let Some(i) = r[0].iter().position(|s| s.eq_ignore_ascii_case(source))
            && let Some(v) = r[1].get(i).filter(|s| !s.is_empty())
        {
            o.insert(key.into(), json!(v));
        }
    }
    Ok(Value::Object(o))
}
/// All paths are preserved by the caller's immutable inventory. Unsupported binary files
/// are opaque originals; only explicitly named source files produce accounting facts.
pub fn parse(files: &[SourceFile], currency: &str) -> Result<ParsedImport, Error> {
    if currency != "DKK" {
        return Err(refused(
            "latest-year Dinero journal import requires declared DKK",
        ));
    }
    let mut paths = BTreeSet::new();
    for f in files {
        if f.path.is_empty()
            || f.path.starts_with('/')
            || f.path.contains('\\')
            || f.path
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == "..")
            || f.path.chars().any(char::is_control)
            || !paths.insert(&f.path)
        {
            return Err(refused("unsafe or duplicate source path"));
        }
    }
    let chart_file = latest(files, "Kontoplan.csv")
        .ok_or_else(|| refused("latest-year Kontoplan.csv missing"))?;
    let latest_year = year(&chart_file.path, "Kontoplan.csv").unwrap();
    let mut chart = vec![];
    let mut unmapped = BTreeSet::new();
    let mut accounts = BTreeSet::new();
    for r in csv::read(
        &chart_file.bytes,
        &["Nummer", "Navn", "Type", "Momstype", "Total"],
    )? {
        if !accounts.insert(r[0].clone()) || r[1].is_empty() {
            return Err(refused("duplicate account or missing account name"));
        }
        let (kind, normal) = classification(&r[2], &r[0])
            .ok_or_else(|| refused(format!("unrecognized account type {}", r[2])))?;
        let code = vat(&r[3]);
        if !r[3].is_empty() && code.is_none() {
            unmapped.insert(r[3].clone());
        }
        chart.push(Account {
            account_no: r[0].clone(),
            name: r[1].clone(),
            kind: kind.into(),
            normal_balance: normal.into(),
            default_vat_code: code.map(str::to_owned),
        });
    }
    if chart.is_empty() {
        return Err(refused("source chart has no accounts"));
    }
    let role_proposals = proposals(&chart);
    let post_file = latest(files, "Posteringer.csv");
    if post_file.is_some_and(|f| year(&f.path, "Posteringer.csv") != Some(latest_year)) {
        return Err(refused("chart and postings latest years differ"));
    }
    let mut notices = vec![];
    let (opening_lines, cut_over_date, mut vouchers) = if let Some(f) = post_file {
        postings::parse(f, &accounts)?
    } else {
        (vec![], None, vec![])
    };
    // Source framework chooses chart-only when there is no nonzero primo. Do not
    // silently replay a year's vouchers without its source opening boundary.
    if cut_over_date.is_none() {
        notices.push(
            "chart-only import: no nonzero Primobeholdning; historical vouchers are not posted"
                .into(),
        );
        vouchers.clear();
    }
    postings::classify_reverse_charge(&mut vouchers, &chart)?;
    let control_date = cut_over_date.map(|d| {
        vouchers
            .iter()
            .map(|v| v.transaction_date)
            .max()
            .map_or(d, |v| v.max(d))
    });
    let mut controls = vec![];
    if let Some(f) = post_file {
        let path = f.path[..f.path.len() - "Posteringer.csv".len()].to_owned() + "SaldoBalance.csv";
        if let Some(b) = files.iter().find(|b| b.path == path) {
            let mut balances = BTreeMap::new();
            for r in csv::read(&b.bytes, &["Konto", "Kontonavn", "Beløb"])? {
                if balances.insert(r[0].clone(), decimal(&r[2])?).is_some() {
                    return Err(refused("SaldoBalance repeats account"));
                }
            }
            for p in &role_proposals {
                if ["debtors", "creditors"].contains(&p.role.as_str())
                    && let Some(m) = balances
                        .get(&p.account_no)
                        .filter(|m| **m != Money::default())
                {
                    let credit = p.role == "creditors";
                    if m.is_negative() != credit {
                        return Err(refused("unexpected debtor/creditor control sign"));
                    }
                    controls.push(Control {
                        account_no: p.account_no.clone(),
                        kind: if credit { "payable" } else { "receivable" }.into(),
                        amount: if credit {
                            Money::default().subtract(m)
                        } else {
                            m.clone()
                        },
                        source_reference: path.clone(),
                    });
                }
            }
            controls.sort_by(|a, b| a.account_no.cmp(&b.account_no));
        }
    }
    let schedule = files
        .iter()
        .find(|f| f.path == "Rentemester-modtagerposter-v1.json")
        .map(|f| {
            let text =
                std::str::from_utf8(&f.bytes).map_err(|_| refused("schedule must be UTF8"))?;
            let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
                .map_err(|_| refused("invalid schedule JSON"))?;
            imported_receivables::validate(&value)
        })
        .transpose()?;
    if let Some(s) = &schedule {
        for i in &s.invoices {
            if !accounts.contains(&i.control_account_no) {
                return Err(refused("schedule references unknown control account"));
            }
        }
    }
    Ok(ParsedImport {
        parser_contract: CONTRACT.into(),
        declared_currency: currency.into(),
        latest_year,
        cut_over_date,
        control_date,
        chart,
        company_master: master(files.iter().find(|f| f.path == "Firmaoplysninger.csv"))?,
        role_proposals,
        opening_lines,
        vouchers,
        controls,
        schedule,
        unmapped_vat_codes: unmapped.into_iter().collect(),
        notices,
    })
}
