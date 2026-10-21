//! CompanyRead audit inspection. Pagination is a new observation on each request.
use crate::{ApiClient, Error, company_path, domain::object};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
const SAFE: u64 = 9_007_199_254_740_991;
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AuditLogQuery {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<u64>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub offset: Option<u64>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub from_date: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub to_date: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub event_type_like: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub actor_like: Option<String>,
}
fn invalid() -> Error {
    Error::InvalidInput(
        "invalid audit filters: limit1..5000, safe nonnegative offset, ordered real dates/RFC3339 and bounded control-free LIKE text; query at most4096 bytes",
    )
}
fn bound(value: Option<&str>, upper: bool) -> Result<Option<i128>, Error> {
    let Some(value) = value.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if value.len() > 64 {
        return Err(invalid());
    }
    let at = if crate::valid_commitment_start(value) {
        let year = value[..4].parse::<i32>().map_err(|_| invalid())?;
        let month = time::Month::try_from(value[5..7].parse::<u8>().map_err(|_| invalid())?)
            .map_err(|_| invalid())?;
        let day = value[8..10].parse::<u8>().map_err(|_| invalid())?;
        let at = time::Date::from_calendar_date(year, month, day)
            .map_err(|_| invalid())?
            .midnight()
            .assume_utc()
            .unix_timestamp_nanos();
        if upper {
            at + 86_400_000_000_000 - 1
        } else {
            at
        }
    } else {
        time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
            .map_err(|_| invalid())?
            .unix_timestamp_nanos()
    };
    Ok(Some(at))
}
impl AuditLogQuery {
    fn pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = Vec::new();
        if let Some(n) = self.limit {
            pairs.push(("limit", n.to_string()))
        }
        if let Some(n) = self.offset {
            pairs.push(("offset", n.to_string()))
        }
        for (name, value) in [
            ("fromDate", &self.from_date),
            ("toDate", &self.to_date),
            ("eventTypeLike", &self.event_type_like),
            ("actorLike", &self.actor_like),
        ] {
            if let Some(value) = value.as_ref().filter(|s| !s.is_empty()) {
                pairs.push((name, value.clone()))
            }
        }
        pairs
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.limit.is_some_and(|n| !(1..=5000).contains(&n))
            || self.offset.is_some_and(|n| n > SAFE)
        {
            return Err(invalid());
        }
        for text in [&self.event_type_like, &self.actor_like]
            .into_iter()
            .flatten()
        {
            if text.chars().count() > 256 || text.chars().any(char::is_control) {
                return Err(invalid());
            }
        }
        let from = bound(self.from_date.as_deref(), false)?;
        let to = bound(self.to_date.as_deref(), true)?;
        if matches!((from,to),(Some(a),Some(b)) if a>b) {
            return Err(invalid());
        }
        let mut url = reqwest::Url::parse("https://audit.invalid/").map_err(|_| invalid())?;
        url.query_pairs_mut().extend_pairs(self.pairs());
        if url.query().is_some_and(|q| q.len() > 4096) {
            return Err(invalid());
        }
        Ok(())
    }
}
pub fn audit_log_query_schema() -> Value {
    object(
        json!({"limit":{"type":"integer","minimum":1,"maximum":5000,"default":500},"offset":{"type":"integer","minimum":0,"maximum":SAFE,"default":0},"fromDate":{"type":"string","maxLength":64,"description":"Bare YYYY-MM-DD starts at UTC midnight; otherwise RFC3339 with explicit offset."},"toDate":{"type":"string","maxLength":64,"description":"Bare YYYY-MM-DD includes the full UTC day; otherwise RFC3339 with explicit offset."},"eventTypeLike":{"type":"string","maxLength":256,"description":"Case-insensitive SQL LIKE substring; % and _ remain wildcards. ANDed with other filters."},"actorLike":{"type":"string","maxLength":256,"description":"Case-insensitive SQL LIKE substring; % and _ remain wildcards. ANDed with other filters."}}),
        &[],
    )
}
fn response(value: Value, company: &str, query: &AuditLogQuery) -> Result<Value, Error> {
    let fail = || Error::Protocol;
    let rows = value["rows"].as_array().ok_or_else(fail)?;
    let total = value["total"].as_u64().ok_or_else(fail)?;
    let count = value["count"].as_u64().ok_or_else(fail)?;
    let offset = value["offset"].as_u64().ok_or_else(fail)?;
    let limit = value["limit"].as_u64().ok_or_else(fail)?;
    let next = offset.checked_add(count).ok_or_else(fail)?;
    if value["ok"] != true
        || value["slug"] != company
        || count != rows.len() as u64
        || count > limit
        || total < count
        || offset != query.offset.unwrap_or(0)
        || limit != query.limit.unwrap_or(500)
        || value["hasMore"] != (next < total)
    {
        return Err(fail());
    }
    if next < total {
        if value["nextOffset"].as_u64() != Some(next) {
            return Err(fail());
        }
    } else if value.get("nextOffset").is_some() {
        return Err(fail());
    }
    for row in rows {
        if row["id"].as_i64().is_none_or(|n| n <= 0)
            || !["eventType", "entityType", "message", "actor", "createdAt"]
                .iter()
                .all(|f| row[*f].is_string())
            || !row
                .get("entityId")
                .is_some_and(|v| v.is_null() || v.is_string())
        {
            return Err(fail());
        }
    }
    Ok(value)
}
impl ApiClient {
    pub async fn audit_log(&self, company: &str, query: &AuditLogQuery) -> Result<Value, Error> {
        let path = company_path(company, "audit-log")?;
        query.validate()?;
        let pairs = query.pairs();
        let borrowed = pairs
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect::<Vec<_>>();
        response(self.get(&path, &borrowed).await?, company, query)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_filters_dates_and_closed_shape() {
        for bad in [
            r#"{"limit":null}"#,
            r#"{"offset":-1}"#,
            r#"{"limit":1.5}"#,
            r#"{"limit":1,"limit":2}"#,
            r#"{"actorLike":null}"#,
            r#"{"from_date":"2026-01-01"}"#,
        ] {
            assert!(serde_json::from_str::<AuditLogQuery>(bad).is_err(), "{bad}")
        }
        for value in [
            json!({"limit":0}),
            json!({"limit":5001}),
            json!({"offset":SAFE+1}),
            json!({"fromDate":"2026-02-30"}),
            json!({"fromDate":"2026-02-01T00:00:00"}),
            json!({"fromDate":"2026-10-22","toDate":"2026-10-21"}),
            json!({"actorLike":"bad\nactor"}),
        ] {
            assert!(
                serde_json::from_value::<AuditLogQuery>(value)
                    .unwrap()
                    .validate()
                    .is_err()
            )
        }
        let q:AuditLogQuery=serde_json::from_value(json!({"fromDate":"2026-10-21T01:00:00+02:00","toDate":"2026-10-20T23:00:00Z","eventTypeLike":"%Post_","actorLike":""})).unwrap();
        assert!(q.validate().is_ok());
        assert!(!q.pairs().iter().any(|(key, _)| *key == "actorLike"));
        assert_eq!(
            bound(Some("2026-10-21"), true).unwrap().unwrap()
                - bound(Some("2026-10-21"), false).unwrap().unwrap(),
            86_400_000_000_000 - 1
        );
    }
    #[test]
    fn pagination_and_exact_text_are_server_values() {
        let q = AuditLogQuery {
            limit: Some(1),
            ..Default::default()
        };
        let v = json!({"ok":true,"slug":"synthetic","rows":[{"id":2,"eventType":"post","entityType":"journal","entityId":"1","message":"<script>literal</script>","actor":"user:synthetic","createdAt":"2026-10-21 12:00:00"}],"total":2,"count":1,"limit":1,"offset":0,"hasMore":true,"nextOffset":1});
        assert_eq!(response(v.clone(), "synthetic", &q).unwrap(), v);
        let mut bad = v;
        bad["nextOffset"] = json!(2);
        assert!(response(bad, "synthetic", &q).is_err());
    }
    #[tokio::test]
    async fn cookie_and_service_reads_encode_filters_and_preserve_server_pagination() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        for service in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let reply = json!({"ok":true,"slug":"synthetic","rows":[],"total":0,"count":0,"limit":2,"offset":0,"hasMore":false});
            let expected = reply.clone();
            let reply = reply.to_string();
            let worker = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut buf = [0; 4096];
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                    if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type:application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",reply.len()).unwrap();
                bytes
            });
            let api = if service {
                ApiClient::with_service_key(&origin, true, "synthetic-service".into()).unwrap()
            } else {
                ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap()
            };
            let q = AuditLogQuery {
                limit: Some(2),
                from_date: Some("2026-10-21T01:00:00+02:00".into()),
                event_type_like: Some("%Post_".into()),
                actor_like: Some("agent:synthetic owner".into()),
                ..Default::default()
            };
            assert_eq!(api.audit_log("synthetic", &q).await.unwrap(), expected);
            let request = String::from_utf8(worker.join().unwrap()).unwrap();
            let target = request
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap();
            let url = reqwest::Url::parse(&format!("http://audit.invalid{target}")).unwrap();
            assert_eq!(url.path(), "/api/companies/synthetic/audit-log");
            let pairs = url
                .query_pairs()
                .collect::<std::collections::BTreeMap<_, _>>();
            assert_eq!(pairs["eventTypeLike"], "%Post_");
            assert_eq!(pairs["actorLike"], "agent:synthetic owner");
            assert_eq!(pairs["fromDate"], "2026-10-21T01:00:00+02:00");
            assert_eq!(request.split("\r\n\r\n").nth(1), Some(""));
            if service {
                assert!(
                    request
                        .to_ascii_lowercase()
                        .contains("x-rentemester-service-key: synthetic-service")
                );
                assert!(!request.to_ascii_lowercase().contains("cookie:"))
            } else {
                assert!(
                    request
                        .to_ascii_lowercase()
                        .contains("cookie: session=synthetic")
                )
            }
        }
    }
}
