//! Immutable source bytes and source-bound server review. No CSV parsing or accounting math.
use crate::archive_import_attempt::Attempt;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
pub const MAX_FILE: usize = 16 * 1024 * 1024;
pub const MAX_TOTAL: usize = 18 * 1024 * 1024;
pub const MAX_PACKAGE: usize = 25 * 1024 * 1024;
pub const MAX_BODY: usize = 35 * 1024 * 1024;
pub(crate) fn fail() -> String {
    "Arkivkilde, gennemgang eller forsøgets identitet kunne ikke verificeres; intet sendes".into()
}
pub fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn hex(s: &str, n: usize) -> bool {
    s.len() == n
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn text(s: &str) -> bool {
    !s.is_empty() && s.len() <= 2048 && !s.chars().any(char::is_control)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FileKind {
    Postings,
    Balances,
}
impl FileKind {
    fn name(self) -> &'static str {
        match self {
            Self::Postings => "Posteringer.csv",
            Self::Balances => "SaldoBalance.csv",
        }
    }
}
#[derive(Clone, Debug)]
pub struct SourceFile {
    pub year: u16,
    pub kind: FileKind,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct Package {
    bytes: Vec<u8>,
    hash: String,
    currency: String,
    years: Vec<u16>,
}
impl Package {
    pub fn build(currency: &str, mut files: Vec<SourceFile>) -> Result<Self, String> {
        if currency.len() != 3
            || !currency.bytes().all(|b| b.is_ascii_uppercase())
            || files.is_empty()
            || files.len() > 32
        {
            return Err(
                "Vælg historisk valuta med tre store bogstaver og højst32 CSV-filer".into(),
            );
        }
        files.sort_by_key(|file| (file.year, file.kind));
        let mut paths = BTreeSet::new();
        let mut years = BTreeSet::new();
        let mut total = 0usize;
        let mut encoded = Vec::new();
        for file in &files {
            if !(1000..=9999).contains(&file.year)
                || file.bytes.len() > MAX_FILE
                || std::str::from_utf8(&file.bytes).is_err()
                || !paths.insert((file.year, file.kind))
            {
                return Err(
                    "Vælg entydige firecifrede år og originale UTF-8 CSV-filer på højst16MiB"
                        .into(),
                );
            }
            total = total.checked_add(file.bytes.len()).ok_or_else(fail)?;
            if total > MAX_TOTAL {
                return Err("CSV-filerne må samlet højst være18MiB".into());
            }
            if file.kind == FileKind::Postings {
                years.insert(file.year);
            }
            encoded.push(json!({"path":format!("{}/{}",file.year,file.kind.name()),"contentBase64":STANDARD.encode(&file.bytes)}));
        }
        if years.len() < 2 || files.iter().any(|file| !years.contains(&file.year)) {
            return Err(
                "Vælg Posteringer.csv for mindst to år, inklusive det højeste kontrolår".into(),
            );
        }
        let bytes=serde_json::to_vec(&json!({"format":"rentemester-dinero-archive-source/1","currency":currency,"files":encoded})).map_err(|_|fail())?;
        if bytes.len() > MAX_PACKAGE {
            return Err("Kildepakken overstiger25MiB".into());
        }
        let hash = sha(&bytes);
        Ok(Self {
            bytes,
            hash,
            currency: currency.into(),
            years: years.into_iter().collect(),
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn sha256(&self) -> &str {
        &self.hash
    }
    pub fn size(&self) -> usize {
        self.bytes.len()
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub fn plan_body(&self) -> Vec<u8> {
        serde_json::to_vec(&self.source()).expect("source is JSON")
    }
    fn source(&self) -> Value {
        json!({"fileName":"dinero-archive-source.json","contentBase64":STANDARD.encode(&self.bytes)})
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    pub origin: String,
    pub company: String,
    pub principal: String,
    pub ledger_uuid: String,
    pub local_identity: Option<String>,
    pub can_apply: bool,
}
impl Scope {
    pub fn parse(
        origin: &str,
        company: &str,
        local: Option<&str>,
        me: &Value,
        journal: &Value,
    ) -> Result<Self, String> {
        let principal = me["user"]["id"].as_str().ok_or_else(fail)?;
        let ledger = journal["journal"]["ledgerUuid"].as_str().ok_or_else(fail)?;
        if me["ok"] != true
            || journal["ok"] != true
            || journal["journal"]["slug"] != company
            || ![origin, company, principal].iter().all(|s| text(s))
            || !hex(ledger, 32)
            || local.is_some_and(|s| !text(s))
        {
            return Err(fail());
        }
        let companies = me["companies"].as_array().ok_or_else(fail)?;
        let selected = companies
            .iter()
            .find(|c| c["slug"] == company && c["archived"] != true)
            .ok_or_else(fail)?;
        Ok(Self {
            origin: origin.into(),
            company: company.into(),
            principal: principal.into(),
            ledger_uuid: ledger.into(),
            local_identity: local.map(str::to_owned),
            can_apply: selected["role"] == "owner",
        })
    }
    pub fn same_identity(&self, other: &Self) -> bool {
        self.origin == other.origin && self.same_profile(other)
    }
    pub fn same_profile(&self, other: &Self) -> bool {
        self.company == other.company
            && self.principal == other.principal
            && self.ledger_uuid == other.ledger_uuid
            && self.local_identity == other.local_identity
    }
}
fn key(s: &str) -> bool {
    !s.trim().is_empty() && s.encode_utf16().count() <= 128 && !s.chars().any(char::is_control)
}
fn integer(v: &Value) -> Option<u64> {
    v.as_u64().filter(|n| *n <= 9_007_199_254_740_991)
}
pub(crate) fn valid_plan(plan: &Value) -> bool {
    let Some(years) = plan["years"].as_array() else {
        return false;
    };
    let mut seen = BTreeSet::new();
    plan["format"] == "rentemester-native-closed-history/1"
        && plan["status"] == "ready"
        && plan["planHash"].as_str().is_some_and(|s| hex(s, 64))
        && plan["source"]["sha256"]
            .as_str()
            .is_some_and(|s| hex(s, 64))
        && integer(&plan["source"]["sizeBytes"]).is_some_and(|n| n > 0 && n <= MAX_PACKAGE as u64)
        && plan["declaredCurrency"]
            .as_str()
            .is_some_and(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase()))
        && plan["currencyDeclaration"] == "operator"
        && integer(&plan["excludedLatestYear"]).is_some_and(|n| (1000..=9999).contains(&n))
        && !years.is_empty()
        && years.len() < 32
        && years.iter().all(|y| {
            integer(&y["fiscalYear"]).is_some_and(|n| {
                n >= 1000 && n < plan["excludedLatestYear"].as_u64().unwrap_or(0) && seen.insert(n)
            }) && integer(&y["postingCount"]).is_some()
                && integer(&y["balanceCount"]).is_some()
                && y["hasBalances"].is_boolean()
                && matches!(y["status"].as_str(), Some("new" | "retained"))
        })
        && plan["limitations"]
            .as_array()
            .is_some_and(|v| v.iter().all(Value::is_string))
}
#[derive(Clone, Debug)]
pub struct Review {
    package: Package,
    scope: Scope,
    plan: Value,
    key: String,
    body: Vec<u8>,
}
impl Review {
    pub fn prepare(
        package: Package,
        scope: Scope,
        response: Value,
        idempotency_key: String,
    ) -> Result<Self, String> {
        if response["ok"] != true {
            return Err(fail());
        }
        Self::from_plan(package, scope, response["plan"].clone(), idempotency_key)
    }
    fn from_plan(
        package: Package,
        scope: Scope,
        plan: Value,
        idempotency_key: String,
    ) -> Result<Self, String> {
        let expected = &package.years[..package.years.len() - 1];
        if !valid_plan(&plan)
            || !key(&idempotency_key)
            || plan["source"]["sha256"] != package.hash
            || plan["source"]["sizeBytes"] != package.size()
            || plan["declaredCurrency"] != package.currency
            || plan["excludedLatestYear"] != *package.years.last().ok_or_else(fail)?
            || plan["years"].as_array().ok_or_else(fail)?.len() != expected.len()
            || expected.iter().any(|year| {
                !plan["years"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row["fiscalYear"] == *year)
            })
        {
            return Err(fail());
        }
        let mut value = package.source();
        value["planHash"] = plan["planHash"].clone();
        value["idempotencyKey"] = json!(idempotency_key);
        value["confirm"] = json!(true);
        let body = serde_json::to_vec(&value).map_err(|_| fail())?;
        if body.len() > MAX_BODY {
            return Err("Anmodningen overstiger35MiB".into());
        }
        Ok(Self {
            package,
            scope,
            plan,
            key: idempotency_key,
            body,
        })
    }
    pub fn recover(package: Package, scope: Scope, attempt: &Attempt) -> Result<Self, String> {
        if !attempt.matches(&scope) {
            return Err(fail());
        }
        let review = Self::from_plan(
            package,
            scope,
            attempt.summary.clone(),
            attempt.idempotency_key.clone(),
        )?;
        if sha(review.body()) != attempt.body_sha256
            || review.package.sha256() != attempt.source_sha256
            || review.plan["planHash"] != attempt.plan_hash
        {
            return Err(fail());
        }
        Ok(review)
    }
    pub fn body(&self) -> &[u8] {
        &self.body
    }
    pub fn plan(&self) -> &Value {
        &self.plan
    }
    pub fn summary(&self) -> Value {
        self.plan.clone()
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn package(&self) -> &Package {
        &self.package
    }
    pub fn idempotency_key(&self) -> &str {
        &self.key
    }
    pub fn current(&self, scope: &Scope) -> bool {
        self.scope.same_identity(scope) && scope.can_apply
    }
}
/// Only documented definite noncommits. Conflict/expired/restored-key outcomes stay pending.
pub fn known_refusal(v: &Value) -> bool {
    v["ok"] == false
        && matches!(
            v["code"].as_str(),
            Some(
                "archive_plan_changed"
                    | "archive_import_refused"
                    | "archive_import_invalid"
                    | "invalid_input"
                    | "access_denied"
                    | "invalid_credentials"
                    | "archive_import_too_large"
            )
        )
}
pub fn receipt(review: &Review, response: &Value) -> Result<i64, String> {
    let r = &response["archiveImport"];
    let id = r["importId"]
        .as_i64()
        .filter(|n| (1..=9_007_199_254_740_991).contains(n))
        .ok_or_else(fail)?;
    if !response["idempotency"]["receiptId"]
        .as_i64()
        .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
        || !response["idempotency"]["replayed"].is_boolean()
        || ["createdAt", "expiresAt"]
            .iter()
            .any(|key| !response["idempotency"][*key].as_str().is_some_and(text))
    {
        return Err(fail());
    }
    let years = r["years"].as_array().ok_or_else(fail)?;
    let planned = review.plan["years"].as_array().ok_or_else(fail)?;
    if response["ok"] != true
        || r["sourceSha256"] != review.plan["source"]["sha256"]
        || r["planHash"] != review.plan["planHash"]
        || r["excludedLatestYear"] != review.plan["excludedLatestYear"]
        || r["declaredCurrency"] != review.plan["declaredCurrency"]
        || years.len() != planned.len()
        || planned.iter().any(|p| {
            years
                .iter()
                .filter(|r| {
                    ["fiscalYear", "postingCount", "balanceCount"]
                        .iter()
                        .all(|k| r[*k] == p[*k])
                })
                .count()
                != 1
        })
    {
        return Err(fail());
    }
    Ok(id)
}
fn rows(archives: &Value) -> Result<&Vec<Value>, String> {
    if archives["ok"] != true || archives["errors"].as_array().is_none_or(|v| !v.is_empty()) {
        return Err(fail());
    }
    archives["years"].as_array().ok_or_else(fail)
}
fn original(value: &Value, expected: &str) -> Result<(), String> {
    if value["ok"] != true || value["contentType"] != "application/json" {
        return Err(fail());
    }
    let array = value["bytes"]
        .as_array()
        .filter(|v| v.len() <= MAX_PACKAGE)
        .ok_or_else(fail)?;
    let bytes: Vec<u8> = array
        .iter()
        .map(|v| {
            v.as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .ok_or_else(fail)
        })
        .collect::<Result<_, _>>()?;
    if sha(&bytes) != expected {
        return Err(fail());
    }
    Ok(())
}
fn matched<'a>(review: &Review, archives: &'a Value) -> Result<Vec<&'a Value>, String> {
    let rows = rows(archives)?;
    let mut output = Vec::new();
    for plan in review.plan["years"].as_array().ok_or_else(fail)? {
        let found = rows
            .iter()
            .filter(|r| r["fiscalYear"] == plan["fiscalYear"])
            .collect::<Vec<_>>();
        if found.len() > 1 {
            return Err(fail());
        }
        if let Some(row) = found.first() {
            if ["postingCount", "balanceCount", "hasBalances"]
                .iter()
                .any(|key| row[*key] != plan[*key])
                || row["sourceSystem"] != "dinero"
                || row["declaredCurrency"] != review.plan["declaredCurrency"]
                || row["parserContract"] != "rentemester-native-closed-history/1"
                || !row["sourceHash"].as_str().is_some_and(|s| hex(s, 64))
                || !row["importId"]
                    .as_i64()
                    .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
                || (plan["status"] == "new" && row["sourceHash"] != review.plan["source"]["sha256"])
            {
                return Err(fail());
            }
            output.push(*row);
        }
    }
    Ok(output)
}
pub fn reconciliation_original_ids(review: &Review, archives: &Value) -> Result<Vec<i64>, String> {
    Ok(matched(review, archives)?
        .iter()
        .map(|row| row["importId"].as_i64().unwrap())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}
pub fn verify_reconciliation(
    review: &Review,
    archives: &Value,
    originals: &[(i64, Value)],
) -> Result<(), String> {
    for row in matched(review, archives)? {
        let id = row["importId"].as_i64().ok_or_else(fail)?;
        let found = originals
            .iter()
            .filter(|(i, _)| *i == id)
            .collect::<Vec<_>>();
        if found.len() != 1 {
            return Err(fail());
        }
        original(&found[0].1, row["sourceHash"].as_str().ok_or_else(fail)?)?;
    }
    Ok(())
}
pub fn verify_receipt(
    review: &Review,
    response: &Value,
    archives: &Value,
    source: &Value,
) -> Result<Value, String> {
    receipt(review, response)?;
    if matched(review, archives)?.len() != review.plan["years"].as_array().ok_or_else(fail)?.len() {
        return Err(fail());
    }
    original(source, review.package.sha256())?;
    Ok(response["archiveImport"].clone())
}
#[cfg(test)]
pub(crate) mod tests;
