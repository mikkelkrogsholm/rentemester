//! Native journal headers across all years, with server-owned filtering and pagination.
use crate::{ApiClient, Error, company_path};
use reqwest::Method;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
const SAFE: u64 = 9_007_199_254_740_991;
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JournalHistoryQuery {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub from: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub to: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub status: Option<String>,
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
}
impl JournalHistoryQuery {
    pub fn validate(&self) -> Result<(), Error> {
        if self
            .from
            .as_deref()
            .is_some_and(|s| !crate::valid_commitment_start(s))
            || self
                .to
                .as_deref()
                .is_some_and(|s| !crate::valid_commitment_start(s))
            || self
                .status
                .as_deref()
                .is_some_and(|s| !matches!(s, "all" | "posted" | "reversed"))
            || self.limit.is_some_and(|n| !(1..=5000).contains(&n))
            || self.offset.is_some_and(|n| n > SAFE)
        {
            return Err(Error::InvalidInput(
                "journal history requires real from/to dates, status all|posted|reversed, limit1..5000 and safe nonnegative offset",
            ));
        }
        // A reversed interval deliberately remains a valid empty server query.
        Ok(())
    }
}
pub fn journal_history_query_schema() -> Value {
    json!({"type":"object","properties":{"from":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},"to":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},"status":{"enum":["all","posted","reversed"],"default":"all"},"limit":{"type":"integer","minimum":1,"maximum":5000,"default":500},"offset":{"type":"integer","minimum":0,"maximum":SAFE,"default":0}},"additionalProperties":false})
}
const FIELDS: [&str; 12] = [
    "id",
    "entryNo",
    "transactionDate",
    "text",
    "currency",
    "amountForeign",
    "amountDkk",
    "fxRateToDkk",
    "documentId",
    "sourceBankTransactionId",
    "status",
    "reversalOfEntryId",
];
fn page(value: Value, query: &JournalHistoryQuery, all: bool) -> Result<Value, Error> {
    if value["ok"] != true {
        return Err(Error::Protocol);
    }
    let value = value.get("journalHistory").ok_or(Error::Protocol)?;
    let rows = value["entries"].as_array().ok_or(Error::Protocol)?;
    let total = value["total"]
        .as_u64()
        .filter(|n| *n <= SAFE)
        .ok_or(Error::Protocol)?;
    let count = value["count"].as_u64().ok_or(Error::Protocol)?;
    let limit = value["limit"].as_u64().ok_or(Error::Protocol)?;
    let offset = value["offset"].as_u64().ok_or(Error::Protocol)?;
    let next = offset.checked_add(count).ok_or(Error::Protocol)?;
    if count != rows.len() as u64
        || count > limit
        || total < count
        || offset != if all { 0 } else { query.offset.unwrap_or(0) }
        || (!all && limit != query.limit.unwrap_or(500))
        || (all && (count != total || value["hasMore"] != false))
        || value["hasMore"] != (next < total)
        || (next < total && value["nextOffset"] != next)
        || (next >= total && value.get("nextOffset").is_some())
    {
        return Err(Error::Protocol);
    }
    let mut prior = None;
    for row in rows {
        let object = row.as_object().ok_or(Error::Protocol)?;
        let id = row["id"]
            .as_u64()
            .filter(|id| *id > 0 && *id <= SAFE)
            .ok_or(Error::Protocol)?;
        if !(object.len() == 12 || object.len() == 13 && object.contains_key("bookedDebitTotal"))
            || FIELDS.iter().any(|k| !object.contains_key(*k))
            || prior.is_some_and(|prior| id >= prior)
            || !row["entryNo"].is_string()
            || !row["text"].is_string()
            || !row["transactionDate"]
                .as_str()
                .is_some_and(crate::valid_commitment_start)
            || !matches!(row["status"].as_str(), Some("posted" | "reversed"))
            || !(row["currency"].is_null() || row["currency"].is_string())
        {
            return Err(Error::Protocol);
        }
        for key in ["amountForeign", "amountDkk", "fxRateToDkk"] {
            if !(row[key].is_null() || row[key].is_number()) {
                return Err(Error::Protocol);
            }
        }
        for key in ["documentId", "sourceBankTransactionId", "reversalOfEntryId"] {
            if !(row[key].is_null() || row[key].as_u64().is_some_and(|n| n > 0 && n <= SAFE)) {
                return Err(Error::Protocol);
            }
        }
        if let Some(amount) = row.get("bookedDebitTotal") {
            if !(amount.is_number()
                || amount.as_str().is_some_and(|s| {
                    !s.is_empty()
                        && s.len() <= 128
                        && s.chars()
                            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-'))
                }))
            {
                return Err(Error::Protocol);
            }
        } else if all {
            return Err(Error::Protocol);
        }
        prior = Some(id);
    }
    let mut page = value.clone();
    if !all {
        for row in page["entries"].as_array_mut().ok_or(Error::Protocol)? {
            row.as_object_mut()
                .ok_or(Error::Protocol)?
                .remove("bookedDebitTotal");
        }
    }
    Ok(page)
}
impl ApiClient {
    /// One bounded authenticated snapshot. `all` never loops over separately observed pages.
    pub async fn journal_history(
        &self,
        company: &str,
        query: &JournalHistoryQuery,
        all: bool,
    ) -> Result<Value, Error> {
        query.validate()?;
        if all && (query.limit.is_some() || query.offset.is_some()) {
            return Err(Error::InvalidInput(
                "all cannot be combined with limit/offset",
            ));
        }
        let mut owned = Vec::new();
        for (key, value) in [
            ("from", query.from.as_ref()),
            ("to", query.to.as_ref()),
            ("status", query.status.as_ref()),
        ] {
            if let Some(value) = value {
                owned.push((key, value.clone()));
            }
        }
        if all {
            owned.push(("all", "true".into()));
        } else {
            owned.push(("limit", query.limit.unwrap_or(500).to_string()));
            owned.push(("offset", query.offset.unwrap_or(0).to_string()));
        }
        let pairs: Vec<_> = owned.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let mut response = self
            .send(
                Method::GET,
                &company_path(company, "journal/entries")?,
                &pairs,
                None,
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await?;
        if !response.status().is_success() {
            return Err(Error::Refused {
                status: response.status().as_u16(),
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if bytes.len().saturating_add(chunk.len()) > 96 * 1024 * 1024 {
                return Err(Error::Protocol);
            }
            bytes.extend_from_slice(&chunk);
        }
        page(
            serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?,
            query,
            all,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},

        thread,
    };
    fn row(id: u64) -> Value {
        json!({"id":id,"entryNo":format!("SYN-{id}"),"transactionDate":"2026-10-20","text":"Synthetic <&> historical header","currency":"EUR","amountForeign":50.05,"amountDkk":371.98,"fxRateToDkk":7.43208,"documentId":12,"sourceBankTransactionId":9,"status":"posted","reversalOfEntryId":null,"bookedDebitTotal":"375.38"})
    }
    fn wire(rows: Vec<Value>, limit: u64, offset: u64, total: u64) -> Value {
        let count = rows.len() as u64;
        let mut result = json!({"ok":true,"journalHistory":{"entries":rows,"limit":limit,"offset":offset,"count":count,"total":total,"hasMore":offset+count<total}});
        if offset + count < total {
            result["journalHistory"]["nextOffset"] = json!(offset + count);
        }
        result
    }
    fn server(reply: Value, status: u16) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            while !bytes.ends_with(b"\r\n\r\n") {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            let reply = reply.to_string();
            write!(socket,"HTTP/1.1 {status} Test\r\nContent-Type:application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",reply.len()).unwrap();
            String::from_utf8(bytes).unwrap()
        });
        (origin, task)
    }
    #[tokio::test]
    async fn paged_selectors_preserve_exact_twelve_fields_and_server_metadata() {
        let reply = wire(vec![row(7), row(6)], 2, 1, 4);
        let (origin, task) = server(reply, 200);
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let query = JournalHistoryQuery {
            from: Some("2026-10-21".into()),
            to: Some("2026-10-19".into()),
            status: Some("posted".into()),
            limit: Some(2),
            offset: Some(1),
        };
        let value = api
            .journal_history("synthetic", &query, false)
            .await
            .unwrap();
        assert_eq!(value["entries"][0]["amountForeign"], 50.05);
        assert_eq!(value["entries"][0]["amountDkk"], 371.98);
        assert_eq!(value["entries"][0]["reversalOfEntryId"], Value::Null);
        assert_eq!(value["entries"][0].as_object().unwrap().len(), 12);
        assert_eq!(value["nextOffset"], 3);
        let request = task.join().unwrap();
        assert!(request.starts_with("GET /api/companies/synthetic/journal/entries?from=2026-10-21&to=2026-10-19&status=posted&limit=2&offset=1 HTTP/1.1"));
        assert!(request.to_lowercase().contains("cookie: session=synthetic"));
    }
    #[tokio::test]
    async fn all_is_one_request_without_paging_or_default_truncation() {
        let rows = (1..=501).rev().map(row).collect();
        let (origin, task) = server(wire(rows, 501, 0, 501), 200);
        let api = ApiClient::new(&origin, true, None).unwrap();
        let value = api
            .journal_history("synthetic", &JournalHistoryQuery::default(), true)
            .await
            .unwrap();
        assert_eq!(value["entries"].as_array().unwrap().len(), 501);
        assert_eq!(value["entries"][0]["bookedDebitTotal"], "375.38");
        assert!(
            task.join()
                .unwrap()
                .starts_with("GET /api/companies/synthetic/journal/entries?all=true HTTP/1.1")
        );
        let (origin, task) = server(wire(vec![], 500, 0, 0), 200);
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(
            api.journal_history("synthetic", &JournalHistoryQuery::default(), false)
                .await
                .unwrap()["count"],
            0
        );
        assert!(
            task.join()
                .unwrap()
                .contains("?limit=500&offset=0 HTTP/1.1")
        );
    }
    #[tokio::test]
    async fn invalid_selectors_protocol_and_refusal_are_distinct() {
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        for value in [
            json!({"limit":0}),
            json!({"limit":5001}),
            json!({"from":"2026-02-30"}),
            json!({"status":"effective"}),
        ] {
            let q: JournalHistoryQuery = serde_json::from_value(value).unwrap();
            assert!(matches!(
                api.journal_history("synthetic", &q, false).await,
                Err(Error::InvalidInput(_))
            ));
        }
        for value in [
            json!({"limit":null}),
            json!({"offset":-1}),
            json!({"all":true}),
            json!({"from":null}),
        ] {
            assert!(serde_json::from_value::<JournalHistoryQuery>(value).is_err());
        }
        let q = JournalHistoryQuery {
            limit: Some(500),
            ..Default::default()
        };
        assert!(matches!(
            api.journal_history("synthetic", &q, true).await,
            Err(Error::InvalidInput(_))
        ));
        let (origin, t) = server(json!({"ok":false,"code":"access_denied"}), 403);
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(
            api.journal_history("synthetic", &Default::default(), false)
                .await,
            Err(Error::Refused { status: 403 })
        );
        t.join().unwrap();
        let mut invalid = wire(vec![row(2)], 500, 0, 1);
        invalid["journalHistory"]["entries"][0]["amountDkk"] = json!("371.98");
        assert_eq!(
            page(invalid, &Default::default(), false),
            Err(Error::Protocol)
        );
        let mut invalid = wire(vec![row(2)], 500, 0, 1);
        invalid["journalHistory"]["count"] = json!(2);
        assert_eq!(
            page(invalid, &Default::default(), false),
            Err(Error::Protocol)
        );
    }
}

#[cfg(test)]
use crate::test_transport::TcpListener;
