//! Original archive wire projection over ApiClient's current guarded physical reads.
//! The caller supplies the engine's canonical ArchiveDecimal numeric witness check;
//! this transport crate neither rounds Money nor implements decimal arithmetic.
use crate::ApiClient;
use serde_json::{Value, json};
fn fail(message: impl Into<String>, code: Option<&str>) -> Value {
    let mut v = json!({"ok":false,"years":[],"errors":[message.into()]});
    if let Some(code) = code {
        v["code"] = json!(code);
    }
    v
}
fn success(years: Vec<Value>) -> Value {
    json!({"ok":true,"years":years,"errors":[]})
}
fn safe_integer(v: &Value) -> Option<i64> {
    let n = v.as_f64()?;
    if n.is_finite() && n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0 {
        Some(n as i64)
    } else {
        None
    }
}
fn header(v: &Value) -> Option<Value> {
    let year = safe_integer(&v["fiscalYear"])?;
    if !(1000..=9999).contains(&year) || v["sourceSystem"] != "dinero" {
        return None;
    }
    for key in ["postingCount", "balanceCount"] {
        if safe_integer(&v[key])? < 0 {
            return None;
        }
    }
    if v["importedAt"].as_str()?.is_empty() {
        return None;
    }
    let mut out = json!({});
    for key in [
        "fiscalYear",
        "sourceSystem",
        "postingCount",
        "balanceCount",
        "importedAt",
    ] {
        out[key] = v[key].clone();
    }
    Some(out)
}
// Native Money is deliberately NOT read here. Reuse the native decimal witness
// validator and numeric round-trip guard instead of rounding or scaled casts.
fn number(v: &Value, verify: fn(&str, &str) -> bool) -> Option<Value> {
    let raw = v.as_str()?;
    let n: serde_json::Number = raw.parse().ok()?;
    if !verify(raw, &n.to_string()) {
        return None;
    }
    Some(Value::Number(n))
}
fn rows(v: &Value, posting: bool, verify: fn(&str, &str) -> bool) -> Option<Vec<Value>> {
    let mut out = vec![];
    let mut last = None;
    for row in v.as_array()? {
        let line = safe_integer(&row["lineNo"])?;
        if line < 0 || last.is_some_and(|n| line <= n) {
            return None;
        }
        last = Some(line);
        let mut r = json!({"lineNo":line,"amount":number(&row["sourceAmount"], verify)?});
        let fields: &[&str] = if posting {
            &[
                "accountNo",
                "accountName",
                "transactionDate",
                "voucher",
                "voucherType",
                "text",
                "vatType",
            ]
        } else {
            &["accountNo", "accountName"]
        };
        for key in fields {
            r[*key] = json!(row[*key].as_str()?);
        }
        if posting {
            let raw = row.get("sourceRunningBalance")?;
            r["runningBalance"] = if raw.is_null() {
                Value::Null
            } else {
                number(raw, verify)?
            };
        }
        out.push(r);
    }
    Some(out)
}
async fn read(api: &ApiClient, company: &str, year: Option<u16>) -> Result<Value, Value> {
    match api.archives(company, year).await {
        Err(e) => Err(fail(
            e.to_string(),
            Some(match e {
                crate::Error::Protocol => "protocol",
                crate::Error::InvalidInput(_) => "native_binding_unavailable",
                crate::Error::Refused { .. } => "server_refused",
                _ => "transport",
            }),
        )),
        Ok(v) if v["ok"] == false => {
            if !v["errors"]
                .as_array()
                .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
                || !v["code"].is_string()
            {
                return Err(fail("Invalid native archive refusal", Some("protocol")));
            }
            Err(json!({"ok":false,"years":[],"errors":v["errors"],"code":v["code"]}))
        }
        Ok(v)
            if v["ok"] == true
                && v["errors"].as_array().is_some_and(Vec::is_empty)
                && v["years"].is_array() =>
        {
            Ok(v)
        }
        Ok(_) => Err(fail("Invalid guarded archive response", Some("protocol"))),
    }
}
pub async fn query_original_archive(
    api: &ApiClient,
    company: &str,
    source: &str,
    year: Option<f64>,
    verify: fn(&str, &str) -> bool,
) -> Value {
    project(api, company, source, year, verify)
        .await
        .expect("projection has no parser errors")
}
async fn project(
    api: &ApiClient,
    company: &str,
    source: &str,
    year: Option<f64>,
    verify: fn(&str, &str) -> bool,
) -> Result<Value, &'static str> {
    let list = match read(api, company, None).await {
        Ok(v) => v,
        Err(e) => return Ok(e),
    };
    let mut headers = vec![];
    let mut previous = 0;
    for v in list["years"].as_array().unwrap() {
        let Some(h) = header(v) else {
            return Ok(fail("Invalid native archive header", Some("protocol")));
        };
        let y = safe_integer(&h["fiscalYear"]).unwrap();
        if y <= previous {
            return Ok(fail("Invalid native archive ordering", Some("protocol")));
        }
        previous = y;
        if h["sourceSystem"] == source && year.is_none_or(|n| n == y as f64) {
            headers.push(h);
        }
    }
    if year.is_none() {
        return Ok(success(headers));
    }
    if headers.is_empty() {
        return Ok(fail(
            format!(
                "no archived {source} data for fiscal year {}",
                year.unwrap()
            ),
            None,
        ));
    }
    // Only a present guarded four-digit year can reach the native DETAIL route.
    let y = year.unwrap() as u16;
    let data = match read(api, company, Some(y)).await {
        Ok(v) => v,
        Err(e) => return Ok(e),
    };
    let years = data["years"].as_array().unwrap();
    if years.len() != 1 {
        return Ok(fail("Invalid native archive detail", Some("protocol")));
    }
    let Some(mut h) = header(&years[0]) else {
        return Ok(fail(
            "Invalid native archive detail header",
            Some("protocol"),
        ));
    };
    if h["fiscalYear"] != y || h["sourceSystem"] != source {
        return Ok(fail(
            "Native archive detail selector mismatch",
            Some("protocol"),
        ));
    }
    let Some(postings) = rows(&years[0]["postings"], true, verify) else {
        return Ok(fail(
            "Archive postings lack representable raw source witnesses",
            Some("native_source_unavailable"),
        ));
    };
    let Some(balances) = rows(&years[0]["balances"], false, verify) else {
        return Ok(fail(
            "Archive balances lack representable raw source witnesses",
            Some("native_source_unavailable"),
        ));
    };
    if h["postingCount"].as_u64() != Some(postings.len() as u64)
        || h["balanceCount"].as_u64() != Some(balances.len() as u64)
    {
        return Ok(fail(
            "Native archive detail counts differ",
            Some("protocol"),
        ));
    }
    h["postings"] = json!(postings);
    h["balances"] = json!(balances);
    Ok(success(vec![h]))
}
