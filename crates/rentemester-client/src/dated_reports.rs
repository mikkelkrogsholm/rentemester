//! Original flat numeric report transport. Calendar/business validation stays server-owned.
use crate::{ApiClient, Error, company_path};
use reqwest::Method;
use serde_json::Value;
impl ApiClient {
    pub async fn dated_report(
        &self,
        company: &str,
        kind: &str,
        from: Option<&str>,
        to: Option<&str>,
        as_of: Option<&str>,
    ) -> Result<Value, Error> {
        let query = match (kind, from, to, as_of) {
            ("trial-balance" | "profit-loss", Some(from), Some(to), None) => {
                vec![("from", from), ("to", to)]
            }
            ("balance", None, None, Some(date)) => vec![("asOf", date)],
            _ => {
                return Err(Error::InvalidInput(
                    "dated report requires trial-balance|profit-loss with from/to, or balance with asOf",
                ));
            }
        };
        let mut response = self
            .send(
                Method::GET,
                &company_path(company, &format!("reports/{kind}"))?,
                &query,
                None,
                self.cookie.as_ref().map(|c| c.as_str()),
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
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
        validate(&value, kind)?;
        Ok(value)
    }
}
fn validate(value: &Value, kind: &str) -> Result<(), Error> {
    if !value["ok"].is_boolean()
        || !value["errors"]
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
        || !value["appliedRules"]
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
    {
        return Err(Error::Protocol);
    }
    let numeric = |key| value[key].is_number();
    let valid = match kind {
        "trial-balance" => {
            value["periodStart"].is_string()
                && value["periodEnd"].is_string()
                && value["accounts"].is_array()
                && value["balanced"].is_boolean()
                && [
                    "totalDebit",
                    "totalCredit",
                    "linesConsidered",
                    "journalEntryCount",
                ]
                .into_iter()
                .all(numeric)
        }
        "profit-loss" => {
            value["periodStart"].is_string()
                && value["periodEnd"].is_string()
                && value["income"].is_array()
                && value["expense"].is_array()
                && ["totalIncome", "totalExpense", "result"]
                    .into_iter()
                    .all(numeric)
        }
        "balance" => {
            value["asOfDate"].is_string()
                && value["balanced"].is_boolean()
                && ["assets", "liabilities", "equity"]
                    .iter()
                    .all(|k| value[k]["lines"].is_array() && value[k]["total"].is_number())
                && ["periodResult", "totalAssets", "totalLiabilitiesAndEquity"]
                    .into_iter()
                    .all(numeric)
        }
        _ => false,
    };
    if !valid {
        return Err(Error::Protocol);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        io::{Read, Write},

        thread,
    };
    fn report(kind: &str) -> Value {
        match kind {
            "trial-balance" => {
                json!({"ok":true,"appliedRules":["financial-statements:trial-balance"],"periodStart":"2025-12-31","periodEnd":"2026-01-01","accounts":[{"accountNo":"A<&>","name":"Synthetic cash","type":"asset","normalBalance":"debit","debit":12.34,"credit":0,"balance":12.34}],"totalDebit":12.34,"totalCredit":12.34,"balanced":true,"linesConsidered":2,"journalEntryCount":1,"errors":[]})
            }
            "profit-loss" => {
                json!({"ok":true,"appliedRules":["financial-statements:profit-loss"],"periodStart":"2025-12-31","periodEnd":"2026-01-01","income":[],"expense":[],"totalIncome":0,"totalExpense":0,"result":0,"errors":[]})
            }
            _ => {
                json!({"ok":true,"appliedRules":["financial-statements:balance-sheet"],"asOfDate":"2026-01-01","assets":{"lines":[],"total":0},"liabilities":{"lines":[],"total":0},"equity":{"lines":[],"total":0},"periodResult":0,"totalAssets":0,"totalLiabilitiesAndEquity":0,"balanced":true,"errors":[]})
            }
        }
    }
    fn server(reply: Value, status: u16) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = vec![];
            while !request.ends_with(b"\r\n\r\n") {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buf[..n]);
            }
            let body = reply.to_string();
            write!(socket,"HTTP/1.1 {status} Test\r\nContent-Type:application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",body.len()).unwrap();
            String::from_utf8(request).unwrap()
        });
        (origin, task)
    }
    #[tokio::test]
    async fn all_three_original_flat_reports_and_selectors_are_preserved() {
        for kind in ["trial-balance", "profit-loss", "balance"] {
            let expected = report(kind);
            let (origin, t) = server(expected.clone(), 200);
            let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
            let (from, to, as_of) = if kind == "balance" {
                (None, None, Some("2026-01-01"))
            } else {
                (Some("2025-12-31"), Some("2026-01-01"), None)
            };
            assert_eq!(
                api.dated_report("synthetic", kind, from, to, as_of)
                    .await
                    .unwrap(),
                expected
            );
            let request = t.join().unwrap();
            let query = if kind == "balance" {
                "asOf=2026-01-01"
            } else {
                "from=2025-12-31&to=2026-01-01"
            };
            assert!(request.starts_with(&format!(
                "GET /api/companies/synthetic/reports/{kind}?{query} HTTP/1.1"
            )));
            assert!(request.to_lowercase().contains("cookie: session=synthetic"));
        }
    }
    #[tokio::test]
    async fn impossible_and_inverted_dates_remain_flat_business_refusals() {
        for (from, to) in [("2026-02-30", "2026-03-01"), ("2026-10-21", "2026-10-20")] {
            let mut expected = report("trial-balance");
            expected["ok"] = json!(false);
            expected["periodStart"] = json!(from);
            expected["periodEnd"] = json!(to);
            expected["errors"] = json!(["periodStart must be before or equal to periodEnd"]);
            expected["accounts"] = json!([]);
            expected["totalDebit"] = json!(0);
            expected["totalCredit"] = json!(0);
            expected["linesConsidered"] = json!(0);
            expected["journalEntryCount"] = json!(0);
            expected["balanced"] = json!(false);
            let (origin, t) = server(expected.clone(), 200);
            let api = ApiClient::new(&origin, true, None).unwrap();
            assert_eq!(
                api.dated_report("synthetic", "trial-balance", Some(from), Some(to), None)
                    .await
                    .unwrap(),
                expected
            );
            t.join().unwrap();
        }
    }
    #[tokio::test]
    async fn selector_shape_source_refusals_and_wrong_wire_do_not_become_empty_reports() {
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        for (kind, from, to, date) in [
            ("unknown", Some("x"), Some("y"), None),
            ("balance", Some("x"), None, Some("y")),
            ("trial-balance", Some("x"), None, None),
        ] {
            assert!(matches!(
                api.dated_report("synthetic", kind, from, to, date).await,
                Err(Error::InvalidInput(_))
            ));
        }
        let (origin, t) = server(json!({"ok":false,"code":"access_denied"}), 403);
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(
            api.dated_report("synthetic", "balance", None, None, Some("2026-10-20"))
                .await,
            Err(Error::Refused { status: 403 })
        );
        t.join().unwrap();
        let mut wrong = report("trial-balance");
        wrong["totalDebit"] = json!("12.34");
        assert_eq!(validate(&wrong, "trial-balance"), Err(Error::Protocol));
        assert_eq!(
            validate(&json!({"ok":false,"code":"precision_refused"}), "balance"),
            Err(Error::Protocol)
        );
    }
}

#[cfg(test)]
use crate::test_transport::TcpListener;
