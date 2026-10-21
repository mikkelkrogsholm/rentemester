//! Original juxtaposed visible-company overview, composed only by the admitted server.
use crate::{ApiClient, Error, valid_commitment_start};
use serde_json::Value;
impl ApiClient {
    pub async fn portfolio_overview(&self, as_of: Option<&str>) -> Result<Value, Error> {
        if as_of.is_some_and(|date| !valid_commitment_start(date)) {
            return Err(Error::InvalidInput("asOf must be a real YYYY-MM-DD date"));
        }
        let query = as_of.map(|date| vec![("asOf", date)]).unwrap_or_default();
        let response = self.get("/api/portfolio/overview", &query).await?;
        let overview = response
            .get("overview")
            .filter(|v| v.is_object())
            .ok_or(Error::Protocol)?;
        let companies = overview["companies"].as_array().ok_or(Error::Protocol)?;
        if response["ok"] != true
            || overview["companyCount"].as_u64() != Some(companies.len() as u64)
        {
            return Err(Error::Protocol);
        }
        Ok(overview.clone())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
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
            let body = reply.to_string();
            write!(socket,"HTTP/1.1 {status} Test\r\nContent-Type:application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",body.len()).unwrap();
            String::from_utf8(bytes).unwrap()
        });
        (origin, task)
    }
    #[tokio::test]
    async fn exact_server_companies_unavailable_values_and_cutoff_are_preserved() {
        let overview = json!({"asOf":"2026-10-21","companyCount":2,"companies":[{"slug":"synthetic-a","name":"Synthetic A","cvr":null,"ok":true,"vat":{"netVatPayable":"9007199254740993.01","canCompute":false,"errors":["unavailable"]},"openReceivables":{"totalOpenBalance":null,"basis":"dated-native-principal"},"backup":{"backupDue":null,"available":false},"audit":{"ok":false,"firstError":"synthetic"},"openExceptions":{"count":2,"highSeverityCount":1}},{"slug":"synthetic-b","name":"Synthetic B","ok":false,"error":"company source unavailable","code":"source_unavailable"}]});
        let (origin, t) = server(json!({"ok":true,"overview":overview}), 200);
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            api.portfolio_overview(Some("2026-10-21")).await.unwrap(),
            overview
        );
        let request = t.join().unwrap();
        assert!(request.starts_with("GET /api/portfolio/overview?asOf=2026-10-21 HTTP/1.1"));
        assert!(request.to_lowercase().contains("cookie: session=synthetic"));
    }
    #[tokio::test]
    async fn default_date_is_server_owned_and_empty_visible_scope_stays_empty() {
        let overview = json!({"asOf":"2026-10-22","companyCount":0,"companies":[]});
        let (origin, t) = server(json!({"ok":true,"overview":overview}), 200);
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(api.portfolio_overview(None).await.unwrap(), overview);
        assert!(
            t.join()
                .unwrap()
                .starts_with("GET /api/portfolio/overview HTTP/1.1")
        );
    }
    #[tokio::test]
    async fn invalid_dates_source_refusals_and_incomplete_shapes_fail_explicitly() {
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        assert!(matches!(
            api.portfolio_overview(Some("2026-02-30")).await,
            Err(Error::InvalidInput(_))
        ));
        let (origin, t) = server(json!({"ok":false,"code":"access_denied"}), 403);
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(
            api.portfolio_overview(None).await,
            Err(Error::Refused { status: 403 })
        );
        t.join().unwrap();
        for reply in [
            json!({"ok":true,"overview":{"companyCount":1,"companies":[]}}),
            json!({"ok":true,"overview":{"companyCount":0,"companies":null}}),
            json!({"ok":true,"overview":[]}),
        ] {
            let (origin, t) = server(reply, 200);
            let api = ApiClient::new(&origin, true, None).unwrap();
            assert_eq!(api.portfolio_overview(None).await, Err(Error::Protocol));
            t.join().unwrap();
        }
    }
}
