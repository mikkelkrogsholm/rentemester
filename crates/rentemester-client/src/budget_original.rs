//! Source budget DTOs, bounded single-shot transport and arithmetic checks.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};
fn object(p: Value, required: &[&str]) -> Value {
    crate::domain::object(p, required)
}
fn text() -> Value {
    json!({"type":"string"})
}
fn number() -> Value {
    json!({"type":"number"})
}
fn nullable() -> Value {
    json!({"type":["string","null"]})
}
fn array(s: Value) -> Value {
    json!({"type":"array","items":s})
}
fn shape(p: Value) -> Value {
    let keys: Vec<String> = p.as_object().unwrap().keys().cloned().collect();
    let required: Vec<&str> = keys.iter().map(String::as_str).collect();
    object(p, &required)
}
pub fn original_budget_set_schema() -> Value {
    object(
        json!({"accountNo":text(),"period":text(),"amount":number(),"notes":text()}),
        &["accountNo", "period"],
    )
}
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.005
}
fn n(v: &Value, k: &str) -> f64 {
    v[k].as_f64().unwrap_or(f64::NAN)
}
fn date(s: &str) -> Option<time::Date> {
    if s.len() != 10 || !s.is_ascii() || s.as_bytes()[4] != b'-' || s.as_bytes()[7] != b'-' {
        return None;
    }
    let year = s[..4].parse::<i32>().ok()?;
    if !(1..=9999).contains(&year) {
        return None;
    }
    time::Date::from_calendar_date(
        year,
        time::Month::try_from(s[5..7].parse::<u8>().ok()?).ok()?,
        s[8..].parse().ok()?,
    )
    .ok()
}
/// Exact source fields and lowercase rules. No native metadata or DK-rule substitution.
pub fn validate_original_budget_result(op: &str, v: &Value, success: bool) -> bool {
    let strings = array(text());
    let mut p = json!({"ok":{"type":"boolean"},"errors":strings});
    let mut optional: Vec<&str> = vec![];
    match op {
        "set" => {
            p["budgetLineId"] = json!({"type":"integer","minimum":1});
            p["accountNo"] = text();
            p["period"] = text();
            p["amount"] = number();
            if !success {
                optional.extend(["budgetLineId", "accountNo", "period", "amount"]);
            }
        }
        "list" => {
            p["count"] = json!({"type":"integer","minimum":0});
            p["rows"] = array(shape(
                json!({"id":{"type":"integer","minimum":1},"accountNo":text(),"accountName":nullable(),"period":text(),"amount":number(),"notes":nullable(),"createdAt":text()}),
            ));
        }
        "vs-actual" => {
            for k in ["periodStart", "periodEnd"] {
                p[k] = text();
            }
            for k in ["totalBudget", "totalActual", "totalVariance"] {
                p[k] = number();
            }
            p["lines"] = array(shape(
                json!({"accountNo":text(),"accountName":nullable(),"accountType":nullable(),"period":text(),"budget":number(),"actual":number(),"variance":number()}),
            ));
        }
        "forecast" => {
            p["startDate"] = text();
            p["months"] = json!({"type":"integer"});
            if !success {
                optional.extend(["startDate", "months"]);
            }
            for k in ["openingBalance", "closingBalance"] {
                p[k] = number();
            }
            p["periods"] = array(shape(
                json!({"period":text(),"openingBalance":number(),"invoiceInflow":number(),"recurringInflow":number(),"budgetedCostOutflow":number(),"netChange":number(),"closingBalance":number()}),
            ));
        }
        "forecast-13-week" => {
            p["startDate"] = text();
            if !success {
                optional.push("startDate");
            }
            for k in ["openingCash", "lowestPoint"] {
                p[k] = number();
            }
            p["completeness"] = shape(json!({"included":strings,"excluded":strings}));
            let mut row = json!({"weekStart":text()});
            for k in [
                "openingCash",
                "receivables",
                "payables",
                "commitments",
                "budgets",
                "undatedBudgetAssumptions",
                "scenarios",
                "obligations",
                "intercompany",
                "closingCash",
                "scenarioClosingCash",
            ] {
                row[k] = number();
            }
            row["excluded"] = array(object(
                json!({"source":text(),"amount":number(),"currency":text(),"reason":text()}),
                &["source", "amount", "currency"],
            ));
            row["sources"] = array(object(
                json!({"source":text(),"amount":number(),"reference":text(),"assumption":{"type":"boolean"},"settlementStatus":{"const":"unknown"}}),
                &["source", "amount", "reference"],
            ));
            p["periods"] = array(shape(row));
        }
        _ => return false,
    }
    if op != "list" {
        p["appliedRules"] = strings;
    }
    let required: Vec<String> = p
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| !optional.contains(&k.as_str()))
        .cloned()
        .collect();
    let required: Vec<&str> = required.iter().map(String::as_str).collect();
    if !crate::validate_original_mileage_shape(v, &object(p, &required)) || v["ok"] != success {
        return false;
    }
    let errors = v["errors"].as_array().unwrap();
    if success != errors.is_empty() {
        return false;
    }
    let rule = match op {
        "set" => "budget:set",
        "vs-actual" => "budget:vs-actual",
        "forecast" => "liquidity-forecast",
        "forecast-13-week" => {
            if success {
                "liquidity-forecast-13-week-v3"
            } else {
                // Opening refusal retains the valid-input v3 rule.
                if v["appliedRules"] == json!(["liquidity-forecast-13-week-v3"]) {
                    "liquidity-forecast-13-week-v3"
                } else {
                    "liquidity-forecast-13-week-v1"
                }
            }
        }
        _ => "",
    };
    if op != "list" && v["appliedRules"] != json!([rule]) {
        return false;
    }
    if op == "list" {
        return v["count"].as_u64() == Some(v["rows"].as_array().unwrap().len() as u64);
    }
    if op == "set" {
        return v
            .get("amount")
            .is_none_or(|v| v.as_f64().is_some_and(|n| n >= 0.));
    }
    if op == "vs-actual" {
        let rows = v["lines"].as_array().unwrap();
        return rows.iter().all(|r| {
            close(
                n(r, "variance"),
                if r["accountType"] == "expense" {
                    n(r, "budget") - n(r, "actual")
                } else {
                    n(r, "actual") - n(r, "budget")
                },
            )
        }) && close(
            n(v, "totalBudget"),
            rows.iter().map(|r| n(r, "budget")).sum(),
        ) && close(
            n(v, "totalActual"),
            rows.iter().map(|r| n(r, "actual")).sum(),
        ) && close(
            n(v, "totalVariance"),
            n(v, "totalActual") - n(v, "totalBudget"),
        );
    }
    let rows = v["periods"].as_array().unwrap();
    if op == "forecast" {
        if success {
            let Some(start) = v["startDate"].as_str().and_then(date) else {
                return false;
            };
            let Some(months) = v["months"].as_u64().filter(|m| (1..=18).contains(m)) else {
                return false;
            };
            if rows.len() != months as usize {
                return false;
            }
            let first = start.year() * 12 + start.month() as i32 - 1;
            for (i, row) in rows.iter().enumerate() {
                let month = first + i as i32;
                if row["period"] != format!("{:04}-{:02}", month / 12, month % 12 + 1) {
                    return false;
                }
            }
        }
        let mut balance = n(v, "openingBalance");
        for r in rows {
            let change =
                n(r, "invoiceInflow") + n(r, "recurringInflow") - n(r, "budgetedCostOutflow");
            if !close(balance, n(r, "openingBalance"))
                || !close(change, n(r, "netChange"))
                || !close(balance + change, n(r, "closingBalance"))
            {
                return false;
            }
            balance = n(r, "closingBalance");
        }
        return close(balance, n(v, "closingBalance"));
    }
    if success {
        let Some(start) = v["startDate"].as_str().and_then(date) else {
            return false;
        };
        if !(1..=13).contains(&rows.len()) {
            return false;
        }
        for (i, row) in rows.iter().enumerate() {
            let Some(expected) = start.checked_add(time::Duration::days(i as i64 * 7)) else {
                return false;
            };
            if row["weekStart"] != expected.to_string() {
                return false;
            }
        }
    }
    let mut base = n(v, "openingCash");
    let mut scenario = base;
    let mut lowest = base;
    for r in rows {
        let change =
            n(r, "receivables") - n(r, "payables") - n(r, "commitments") - n(r, "obligations");
        if !close(base, n(r, "openingCash")) {
            return false;
        }
        base += change;
        scenario += change + n(r, "budgets") + n(r, "scenarios") + n(r, "intercompany");
        lowest = lowest.min(base);
        if !close(base, n(r, "closingCash")) || !close(scenario, n(r, "scenarioClosingCash")) {
            return false;
        }
    }
    close(lowest, n(v, "lowestPoint"))
}
impl ApiClient {
    pub async fn original_budget_set(
        &self,
        company: &str,
        bytes: &[u8],
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput("explicit budget confirmation required"));
        }
        if bytes.len() > 16 * 1024 {
            return Err(Error::InvalidInput("budget exceeds 16KiB"));
        }
        crate::domain::body(bytes, &original_budget_set_schema())?;
        let mut v: Value = serde_json::from_slice(bytes)
            .map_err(|_| Error::InvalidInput("invalid budget JSON"))?;
        v["confirm"] = json!(true);
        let b = serde_json::to_vec(&v).map_err(|_| Error::Protocol)?;
        if b.len() > 16 * 1024 {
            return Err(Error::InvalidInput("budget exceeds 16KiB"));
        }
        self.original_budget_request(company, "set", Some(b), &[])
            .await
    }
    pub async fn original_budget_list(
        &self,
        c: &str,
        period: Option<&str>,
        account_no: Option<&str>,
    ) -> Result<Value, Error> {
        let mut q = vec![];
        if let Some(v) = period {
            q.push(("period", v));
        }
        if let Some(v) = account_no {
            q.push(("accountNo", v));
        }
        self.original_budget_request(c, "list", None, &q).await
    }
    pub async fn original_budget_vs_actual(
        &self,
        c: &str,
        from: &str,
        to: &str,
    ) -> Result<Value, Error> {
        self.original_budget_request(c, "vs-actual", None, &[("from", from), ("to", to)])
            .await
    }
    pub async fn original_budget_forecast(
        &self,
        c: &str,
        start_date: &str,
        months_raw: &str,
    ) -> Result<Value, Error> {
        self.original_budget_request(
            c,
            "forecast",
            None,
            &[("startDate", start_date), ("months", months_raw)],
        )
        .await
    }
    pub async fn original_budget_forecast_13_week(
        &self,
        c: &str,
        start_date: &str,
        weeks_raw: Option<&str>,
    ) -> Result<Value, Error> {
        let mut q = vec![("startDate", start_date)];
        if let Some(v) = weeks_raw {
            q.push(("weeks", v));
        }
        self.original_budget_request(c, "forecast-13-week", None, &q)
            .await
    }
    async fn original_budget_request(
        &self,
        c: &str,
        op: &str,
        b: Option<Vec<u8>>,
        q: &[(&str, &str)],
    ) -> Result<Value, Error> {
        if q.iter()
            .any(|(_, v)| v.len() > 4096 || v.chars().any(char::is_control))
        {
            return Err(Error::InvalidInput("bounded budget selector required"));
        }
        let write = b.is_some();
        let unknown = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        };
        let mut r = self
            .send(
                if write { Method::POST } else { Method::GET },
                &company_path(c, &format!("original/budget/{op}"))?,
                q,
                b,
                self.cookie.as_ref().map(|s| s.as_str()),
            )
            .await
            .map_err(|e| {
                if !write && e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
        let status = r.status().as_u16();
        if status == 408 || status >= 500 {
            return Err(if write {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            });
        }
        if status != 200 && status != 422 {
            return Err(Error::Refused { status });
        }
        let mut bytes = vec![];
        while let Some(chunk) = r.chunk().await.map_err(|_| unknown())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(unknown());
            }
            bytes.extend_from_slice(&chunk);
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| unknown())?;
        if !validate_original_budget_result(op, &v, status == 200) {
            return Err(if status == 422 {
                Error::Refused { status }
            } else {
                unknown()
            });
        }
        Ok(v)
    }
}
