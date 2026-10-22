//! Source mileage receipts and caller-local, per-file atomic artifact publication.
use crate::{
    ApiClient, Error, MAX_RESPONSE, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
use std::{io::Write, path::Path};
const RULE: &str = "DK-MILEAGE-LOG-001";
const NOTE: &str = "Mileage log only. Rentemester records the trips; tax treatment is the user/advisor's responsibility.";
fn text() -> Value {
    json!({"type":"string","maxLength":65536})
}
fn number() -> Value {
    json!({"type":"number","minimum":0,"maximum":90071992547409.9f64})
}
fn strings() -> Value {
    json!({"type":"array","items":text(),"maxItems":128})
}
pub fn validate_original_mileage_shape(v: &Value, s: &Value) -> bool {
    if let Some(c) = s.get("const") {
        return v == c;
    }
    let kind = |t: &str| match t {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "null" => v.is_null(),
        "boolean" => v.is_boolean(),
        "number" => v.is_number(),
        "integer" => v.as_i64().is_some() || v.as_u64().is_some(),
        _ => false,
    };
    if !(s["type"].as_str().is_some_and(kind)
        || s["type"]
            .as_array()
            .is_some_and(|a| a.iter().any(|t| t.as_str().is_some_and(kind))))
    {
        return false;
    }
    if let Some(o) = v.as_object() {
        if s.get("properties").is_none() {
            return true;
        }
        if s["required"]
            .as_array()
            .is_some_and(|a| a.iter().any(|k| !o.contains_key(k.as_str().unwrap_or(""))))
        {
            return false;
        }
        return o.iter().all(|(k, v)| {
            s["properties"]
                .get(k)
                .is_some_and(|s| validate_original_mileage_shape(v, s))
        });
    }
    if let Some(a) = v.as_array() {
        return a.len() <= s["maxItems"].as_u64().unwrap_or(u64::MAX) as usize
            && a.iter()
                .all(|v| validate_original_mileage_shape(v, &s["items"]));
    }
    if let Some(t) = v.as_str() {
        return t.len() >= s["minLength"].as_u64().unwrap_or(0) as usize
            && t.len() <= s["maxLength"].as_u64().unwrap_or(u64::MAX) as usize;
    }
    if let Some(n) = v.as_f64() {
        return n.is_finite()
            && s["minimum"].as_f64().is_none_or(|m| n >= m)
            && s["maximum"].as_f64().is_none_or(|m| n <= m)
            && s["exclusiveMinimum"].as_f64().is_none_or(|m| n > m);
    }
    true
}
fn shape(v: &Value, s: Value) -> bool {
    validate_original_mileage_shape(v, &s)
}
pub fn original_mileage_log_schema() -> Value {
    object(
        json!({"tripDate":text(),"purpose":text(),"fromLocation":text(),"toLocation":text(),"kilometers":{"type":"number"},"vehicle":text(),"driver":text(),"ratePerKm":{"type":"number"},"rateBasis":text(),"rateSource":{"type":["string","null"]},"notes":{"type":["string","null"]}}),
        &[
            "tripDate",
            "purpose",
            "fromLocation",
            "toLocation",
            "vehicle",
            "driver",
            "rateBasis",
        ],
    )
}
fn row() -> Value {
    object(
        json!({"id":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"entryNo":text(),"tripDate":text(),"purpose":text(),"fromLocation":text(),"toLocation":text(),"kilometers":number(),"vehicle":text(),"driver":text(),"ratePerKm":number(),"amountBasis":number(),"rateBasis":text(),"rateSource":{"type":["string","null"]},"notes":{"type":["string","null"]},"createdAt":text()}),
        &[
            "id",
            "entryNo",
            "tripDate",
            "purpose",
            "fromLocation",
            "toLocation",
            "kilometers",
            "vehicle",
            "driver",
            "ratePerKm",
            "amountBasis",
            "rateBasis",
            "rateSource",
            "notes",
            "createdAt",
        ],
    )
}
fn receipt(v: &Value, op: &str, ok: bool) -> bool {
    let mut p = json!({"ok":{"const":ok},"appliedRules":{"const":[RULE]},"errors":strings()});
    let mut required = vec!["ok", "appliedRules", "errors"];
    if op == "log" {
        p["warnings"] = strings();
        required.push("warnings");
        if ok {
            p["mileageEntryId"] =
                json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
            p["entryNo"] = text();
            p["amountBasis"] = number();
            required.extend(["mileageEntryId", "entryNo", "amountBasis"]);
        }
    } else if op == "list" {
        if !ok {
            return false;
        }
        p["count"] = json!({"type":"integer","minimum":0});
        p["rows"] = json!({"type":"array","items":row()});
        required.extend(["count", "rows"]);
    } else if op == "report" || ok {
        for k in ["from", "to"] {
            p[k] = text();
            required.push(k);
        }
        for k in ["entryCount", "totalKilometers", "totalAmountBasis"] {
            p[k] = number();
            required.push(k);
        }
        if op == "report" {
            p["entries"] = json!({"type":"array","items":row()});
            required.push("entries");
        }
        if op == "export" && ok {
            p["artifacts"] = json!({"type":"object"});
            required.push("artifacts");
        }
    }
    if !shape(v, object(p, &required)) {
        return false;
    }
    if v["errors"].as_array().is_none_or(|e| e.is_empty() != ok) {
        return false;
    }
    if op == "list" {
        return v["count"].as_u64() == v["rows"].as_array().map(|a| a.len() as u64)
            && ordered(&v["rows"], false);
    }
    if op == "report" {
        let rows = v["entries"].as_array().unwrap();
        return v["entryCount"].as_u64() == Some(rows.len() as u64)
            && ordered(&v["entries"], true)
            && rows.iter().all(|r| {
                r["tripDate"].as_str() >= v["from"].as_str()
                    && r["tripDate"].as_str() <= v["to"].as_str()
            })
            && [
                ("kilometers", "totalKilometers"),
                ("amountBasis", "totalAmountBasis"),
            ]
            .iter()
            .all(|(field, total)| {
                let sum: Option<i64> = rows
                    .iter()
                    .try_fold(0i64, |n, r| n.checked_add(cents(&r[*field])?));
                sum.is_some() && sum == cents(&v[*total])
            });
    }
    true
}
fn cents(v: &Value) -> Option<i64> {
    let n = v.as_f64()?;
    let c = (n * 100.0).round();
    (n.is_finite() && n >= 0.0 && c <= 9007199254740991.0 && (n * 100.0 - c).abs() < 0.00001)
        .then_some(c as i64)
}
fn ordered(v: &Value, ascending: bool) -> bool {
    v.as_array().is_some_and(|a| {
        a.windows(2).all(|p| {
            let left = (p[0]["tripDate"].as_str(), p[0]["id"].as_u64());
            let right = (p[1]["tripDate"].as_str(), p[1]["id"].as_u64());
            if ascending {
                left < right
            } else {
                left > right
            }
        })
    })
}
fn artifacts(v: &Value) -> Result<(Vec<u8>, Vec<u8>), Error> {
    if !receipt(v, "export", true) || serde_json::to_vec(v).map_or(true, |b| b.len() > MAX_RESPONSE)
    {
        return Err(Error::Protocol);
    }
    let a = &v["artifacts"];
    if !shape(
        a,
        object(
            json!({"json":{"type":"object"},"csv":{"type":"object"}}),
            &["json", "csv"],
        ),
    ) {
        return Err(Error::Protocol);
    }
    let mut out = Vec::new();
    for (k, name, mime) in [
        (
            "json",
            "mileage-log.json",
            "application/json; charset=utf-8",
        ),
        ("csv", "mileage-log.csv", "text/csv; charset=utf-8"),
    ] {
        if !shape(
            &a[k],
            object(
                json!({"filename":{"const":name},"contentType":{"const":mime},"bytes":{"type":"array","items":{"type":"integer","minimum":0,"maximum":255}}}),
                &["filename", "contentType", "bytes"],
            ),
        ) {
            return Err(Error::Protocol);
        }
        out.push(
            a[k]["bytes"]
                .as_array()
                .ok_or(Error::Protocol)?
                .iter()
                .map(|n| n.as_u64().map(|n| n as u8).ok_or(Error::Protocol))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    let doc: Value = serde_json::from_slice(&out[0]).map_err(|_| Error::Protocol)?;
    let mut report = doc.clone();
    report
        .as_object_mut()
        .ok_or(Error::Protocol)?
        .remove("note");
    report["ok"] = json!(true);
    report["errors"] = json!([]);
    if doc["note"] != NOTE || !receipt(&report, "report", true) || !out[0].ends_with(b"\n") {
        return Err(Error::Protocol);
    }
    for k in [
        "from",
        "to",
        "entryCount",
        "totalKilometers",
        "totalAmountBasis",
        "appliedRules",
    ] {
        let matches = match k {
            "totalKilometers" | "totalAmountBasis" => cents(&doc[k])
                .zip(cents(&v[k]))
                .is_some_and(|(a, b)| a == b),
            _ => doc[k] == v[k],
        };
        if !matches {
            return Err(Error::Protocol);
        }
    }
    let mut csv = String::from(
        "entry_no,trip_date,purpose,from_location,to_location,kilometers,vehicle,driver,rate_per_km,amount_basis,rate_basis,rate_source\n",
    );
    for r in doc["entries"].as_array().ok_or(Error::Protocol)? {
        let cells = [
            "entryNo",
            "tripDate",
            "purpose",
            "fromLocation",
            "toLocation",
            "kilometers",
            "vehicle",
            "driver",
            "ratePerKm",
            "amountBasis",
            "rateBasis",
            "rateSource",
        ]
        .map(|k| {
            let s = if r[k].is_null() {
                String::new()
            } else if let Some(s) = r[k].as_str() {
                s.to_owned()
            } else {
                r[k].to_string()
            };
            if s.contains([',', '"', '\r', '\n']) {
                format!("\"{}\"", s.replace('"', "\"\""))
            } else {
                s
            }
        });
        csv.push_str(&cells.join(","));
        csv.push('\n');
    }
    if out[1] != csv.as_bytes() {
        return Err(Error::Protocol);
    }
    Ok((out.remove(0), out.remove(0)))
}
impl ApiClient {
    pub async fn original_mileage_log(
        &self,
        company: &str,
        bytes: &[u8],
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.mileage_write(company, "log", bytes, confirmed).await
    }
    pub async fn original_mileage_export(
        &self,
        company: &str,
        bytes: &[u8],
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.mileage_write(company, "export", bytes, confirmed)
            .await
    }
    pub async fn original_mileage_list(&self, company: &str) -> Result<Value, Error> {
        self.mileage_request(company, "list", None, &[]).await
    }
    pub async fn original_mileage_report(
        &self,
        company: &str,
        from: &str,
        to: &str,
    ) -> Result<Value, Error> {
        self.mileage_request(company, "report", None, &[("from", from), ("to", to)])
            .await
    }
    async fn mileage_write(
        &self,
        c: &str,
        op: &str,
        b: &[u8],
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput("confirmation required"));
        }
        let schema = if op == "log" {
            original_mileage_log_schema()
        } else {
            object(json!({"from":text(),"to":text()}), &["from", "to"])
        };
        body(b, &schema)?;
        let mut v: Value =
            serde_json::from_slice(b).map_err(|_| Error::InvalidInput("invalid mileage JSON"))?;
        v["confirm"] = json!(true);
        self.mileage_request(
            c,
            op,
            Some(serde_json::to_vec(&v).map_err(|_| Error::Protocol)?),
            &[],
        )
        .await
    }
    async fn mileage_request(
        &self,
        c: &str,
        op: &str,
        b: Option<Vec<u8>>,
        q: &[(&str, &str)],
    ) -> Result<Value, Error> {
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
                &company_path(c, &format!("original/mileage/{op}"))?,
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
        let mut bytes = Vec::new();
        while let Some(chunk) = r.chunk().await.map_err(|_| unknown())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(unknown());
            }
            bytes.extend_from_slice(&chunk);
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| unknown())?;
        if status == 422 {
            return if receipt(&v, op, false) {
                Ok(v)
            } else {
                Err(Error::Refused { status })
            };
        }
        if !receipt(&v, op, true) || (op == "export" && artifacts(&v).is_err()) {
            return Err(unknown());
        }
        Ok(v)
    }
}
/// Both artifacts are validated and staged before replacing either fixed target.
/// Replacement is atomic per file, not crash-atomic as a pair. Never repeat the POST on failure.
pub fn publish_original_mileage(value: &Value, output: &Path) -> Result<Value, Error> {
    let (json, csv) = artifacts(value)?;
    let failure = || {
        Error::InvalidInput(
            "local_export_failed: local publication failed; prepared server audit may exist; do not resubmit export",
        )
    };
    if output.as_os_str().is_empty() {
        return Err(failure());
    }
    std::fs::create_dir_all(output).map_err(|_| failure())?;
    let mut paths = Vec::new();
    let result = (|| -> std::io::Result<()> {
        for (i, bytes) in [json, csv].iter().enumerate() {
            let mut staged = None;
            for n in 0..1000 {
                let p = output.join(format!(".mileage-{}-{i}-{n}.tmp", std::process::id()));
                let mut opts = std::fs::OpenOptions::new();
                opts.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    opts.mode(0o600);
                }
                match opts.open(&p) {
                    Ok(mut f) => {
                        paths.push(p.clone());
                        f.write_all(bytes)?;
                        f.sync_all()?;
                        staged = Some(p);
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(e),
                }
            }
            if staged.is_none() {
                return Err(std::io::Error::other("no private stage available"));
            }
        }
        std::fs::rename(&paths[0], output.join("mileage-log.json"))?;
        std::fs::rename(&paths[1], output.join("mileage-log.csv"))?;
        Ok(())
    })();
    for p in paths {
        let _ = std::fs::remove_file(p);
    }
    result.map_err(|_| failure())?;
    let mut v = value.clone();
    v.as_object_mut().unwrap().remove("artifacts");
    v["jsonPath"] = json!(output.join("mileage-log.json"));
    v["csvPath"] = json!(output.join("mileage-log.csv"));
    Ok(v)
}
