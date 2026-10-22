//! Flat original period contracts. The server alone computes readiness and owns lifecycle writes.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path, domain::{body, object}};
use reqwest::Method;
use serde_json::{Value, json};

const MAX_ID: i64 = 9_007_199_254_740_991;
const RULE: &str = "DK-BOOKKEEPING-PERIOD-LOCK-001";
#[derive(Clone, Copy)]
enum Operation { Readiness, Status(i64), Review, Close, Reopen }
impl Operation {
    fn suffix(self) -> &'static str {
        match self { Self::Readiness => "readiness", Self::Status(_) => "status", Self::Review => "review", Self::Close => "close", Self::Reopen => "reopen" }
    }
    fn mutation(self) -> bool { matches!(self, Self::Review | Self::Close | Self::Reopen) }
}
fn date(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == 10 && s.bytes().enumerate().all(|(i,b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() })
            && s[..4].parse::<i32>().ok().zip(s[5..7].parse::<u8>().ok()).zip(s[8..].parse::<u8>().ok()).is_some_and(|((year, month), day)| {
                time::Month::try_from(month).ok().is_some_and(|month| time::Date::from_calendar_date(year, month, day).is_ok())
            })
    })
}
fn hash(value: &Value) -> bool { value.as_str().is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())) }
fn id(value: &Value) -> bool { value.as_i64().is_some_and(|n| (1..=MAX_ID).contains(&n)) }
fn text(value: &Value) -> bool { value.as_str().is_some_and(|s| !s.trim().is_empty()) }
fn strings(value: &Value) -> bool { value.as_array().is_some_and(|a| a.iter().all(Value::is_string)) }
fn kind(value: &Value) -> bool { matches!(value.as_str(), Some("vat_period" | "vat_quarter" | "fiscal_year" | "custom")) }
fn schema(op: Operation) -> Value {
    let date = json!({"type":"string","minLength":10,"maxLength":10});
    let hash = json!({"type":"string","minLength":64,"maxLength":64});
    let text = json!({"type":"string","maxLength":4096});
    let mut fields = json!({"periodStart":date,"periodEnd":date,"confirm":{"const":true}});
    let mut required = vec!["periodStart", "periodEnd", "confirm"];
    if !matches!(op, Operation::Review) {
        fields["kind"] = json!({"type":"string","enum":["vat_period","vat_quarter","fiscal_year","custom"]});
    }
    match op {
        Operation::Review => { fields["packetHash"] = hash; required.push("packetHash"); }
        Operation::Close => {
            fields["packetHash"] = hash;
            fields["reviewId"] = json!({"type":"integer","minimum":1,"maximum":MAX_ID});
            fields["status"] = json!({"type":"string","enum":["closed","reported"]});
            fields["reference"] = text.clone();
            fields["reason"] = text;
            // Explicit force remains a server refusal, never silently ignored or authorized here.
            fields["force"] = json!({"type":"boolean"});
        }
        Operation::Reopen => { fields["reason"] = text; required.push("reason"); }
        _ => {}
    }
    object(fields, &required)
}
fn input(op: Operation, bytes: &[u8], confirmed: bool) -> Result<Value, Error> {
    if !confirmed { return Err(Error::InvalidInput("explicit period lifecycle confirmation required")); }
    body(bytes, &schema(op))?;
    let value: Value = serde_json::from_slice(bytes).map_err(|_| Error::InvalidInput("expected JSON object"))?;
    if !date(&value["periodStart"]) || !date(&value["periodEnd"])
        || value.get("packetHash").is_some_and(|v| !hash(v))
        || (matches!(op, Operation::Reopen) && !text(&value["reason"])) {
        return Err(Error::InvalidInput("valid ISO dates, SHA256 hash and nonblank reopen reason required"));
    }
    Ok(value)
}
fn packet(v: &Value) -> bool {
    let version = v["version"] == 3 || (v["version"] == 4
        && v["format"] == "rentemester.native-period-close.v1"
        && v["sourceRevision"].is_object()
        && ["journal", "journalCount", "audit"].iter().all(|key| v["sourceRevision"][*key].is_array()));
    v.is_object() && version && date(&v["periodStart"]) && date(&v["periodEnd"])
        && date(&v["cutoff"]) && hash(&v["hash"]) && strings(&v["controlsRun"])
        && v["blockers"].as_u64().is_some() && v["warnings"].as_u64().is_some()
        && v["items"].as_array().is_some_and(|items| items.iter().all(|i| {
            text(&i["code"]) && matches!(i["status"].as_str(), Some("passed" | "warning" | "blocked" | "unavailable"))
                && i["waivable"].is_boolean() && i["count"].as_u64().is_some() && i["amount"].is_number()
                && hash(&i["sourceHash"]) && i["evidence"].as_array().is_some_and(|a| a.iter().all(Value::is_object))
        }))
}
fn review(v: &Value) -> bool {
    v.is_object() && v.get("ok").is_none() && id(&v["id"]) && packet(&v["packet"])
        && text(&v["reviewerActor"]) && text(&v["createdAt"])
        && v.get("reviewerPrincipal").is_some_and(|p| p.is_null() || (
            matches!(p["kind"].as_str(), Some("user" | "service-account" | "local-trusted")) && text(&p["subjectId"])))
}
fn refusal(v: &Value, strict: bool) -> bool {
    v["ok"] == false && v["errors"].as_array().is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
        && v.get("code").is_none_or(text) && (!strict || text(&v["code"]))
        && v.get("appliedRules").is_none_or(strings)
        && v.get("packetHash").is_none_or(hash)
        && v.get("readinessPacket").is_none_or(packet)
        && v.get("schema").is_none_or(Value::is_object)
        && v.as_object().is_some_and(|o| o.keys().all(|k| matches!(k.as_str(), "ok" | "errors" | "code" | "appliedRules" | "packetHash" | "readinessPacket" | "schema")))
}
fn success(v: &Value, op: Operation, expected: &Value) -> bool {
    match op {
        Operation::Review => review(v) && v["packet"]["hash"] == expected["packetHash"]
            && v["packet"]["periodStart"] == expected["periodStart"] && v["packet"]["periodEnd"] == expected["periodEnd"],
        Operation::Readiness => v["ok"] == true && packet(&v["packet"])
            && v["packet"]["periodStart"] == expected["periodStart"] && v["packet"]["periodEnd"] == expected["periodEnd"],
        Operation::Status(requested) => v["ok"] == true && v.get("review").is_some_and(|r| r.is_null() || (review(r) && r["id"] == requested))
            && v.get("status").is_none_or(|s| s == "unavailable" && v["review"].is_null() && v["code"] == "PERIOD_CLOSE_REVIEW_SCHEMA_UNAVAILABLE")
            && v.get("code").is_none_or(|c| c == "PERIOD_CLOSE_REVIEW_SCHEMA_UNAVAILABLE" && v["status"] == "unavailable"),
        Operation::Close | Operation::Reopen => {
            let expected_kind = expected["kind"].as_str().unwrap_or("vat_period");
            let expected_kind = if expected_kind == "vat_quarter" { "vat_period" } else { expected_kind };
            v["ok"] == true && id(&v["periodId"]) && v.get("id").is_none()
                && v["periodStart"] == expected["periodStart"] && v["periodEnd"] == expected["periodEnd"]
                && kind(&v["kind"]) && v["kind"] == expected_kind
                && v["errors"].as_array().is_some_and(Vec::is_empty)
                && v["appliedRules"] == json!([RULE])
                && v.get("reference").is_none_or(Value::is_string)
                && v.get("readinessPacket").is_none_or(packet)
                && if matches!(op, Operation::Close) {
                    v["status"] == expected["status"].as_str().unwrap_or("closed")
                } else { v["effectiveStatus"] == "open" && text(&v["reopenedBy"]) && text(&v["reason"]) }
        }
    }
}
impl ApiClient {
    pub async fn period_original_readiness(&self, company: &str, from: &str, to: &str) -> Result<Value, Error> {
        let expected = json!({"periodStart":from,"periodEnd":to});
        if !date(&expected["periodStart"]) || !date(&expected["periodEnd"]) { return Err(Error::InvalidInput("valid ISO period dates required")); }
        self.period_original_request(company, Operation::Readiness, &[("from",from),("to",to)], None, expected).await
    }
    pub async fn period_original_status(&self, company: &str, review_id: i64) -> Result<Value, Error> {
        if !(1..=MAX_ID).contains(&review_id) { return Err(Error::InvalidInput("positive safe review id required")); }
        self.period_original_request(company, Operation::Status(review_id), &[("reviewId", &review_id.to_string())], None, Value::Null).await
    }
    pub async fn period_original_review(&self, company: &str, bytes: Vec<u8>, confirmed: bool) -> Result<Value, Error> {
        self.period_original_write(company, Operation::Review, bytes, confirmed).await
    }
    pub async fn period_original_close(&self, company: &str, bytes: Vec<u8>, confirmed: bool) -> Result<Value, Error> {
        self.period_original_write(company, Operation::Close, bytes, confirmed).await
    }
    /// Reopen's explicit invocation supplies confirmation; no lifecycle precheck or second write.
    pub async fn period_original_reopen(&self, company: &str, bytes: Vec<u8>, confirmed: bool) -> Result<Value, Error> {
        self.period_original_write(company, Operation::Reopen, bytes, confirmed).await
    }
    async fn period_original_write(&self, company: &str, op: Operation, bytes: Vec<u8>, confirmed: bool) -> Result<Value, Error> {
        let expected = input(op, &bytes, confirmed)?;
        self.period_original_request(company, op, &[], Some(bytes), expected).await
    }
    // Family-local bounded parser: auth/error HTML and malformed business errors never escape.
    async fn period_original_request(&self, company: &str, op: Operation, query: &[(&str,&str)], body: Option<Vec<u8>>, expected: Value) -> Result<Value, Error> {
        if self.service_key.is_some() { return Err(Error::InvalidInput("original periods require current human authority")); }
        let path = company_path(company, &format!("periods/original/{}", op.suffix()))?;
        let mut response = self.send(if op.mutation() { Method::POST } else { Method::GET }, &path, query, body, self.cookie.as_ref().map(|v| v.as_str())).await?;
        let status = response.status();
        if op.mutation() && (status.is_server_error() || status.as_u16() == 408) { return Err(Error::OutcomeUnknown); }
        let strict = status.as_u16() == 422;
        if !status.is_success() && !strict { return Err(Error::Refused { status: status.as_u16() }); }
        let protocol = || if strict { Error::Refused { status:422 } } else if op.mutation() { Error::OutcomeUnknown } else { Error::Protocol };
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| if strict { protocol() } else if op.mutation() { Error::OutcomeUnknown } else { Error::Transport })? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE { return Err(protocol()); }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| protocol())?;
        if refusal(&value, strict) || (!strict && success(&value, op, &expected)) { Ok(value) } else { Err(protocol()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::{Read, Write}, net::TcpListener, sync::{Arc, atomic::{AtomicUsize, Ordering}}, thread, time::Duration};

    fn packet_fixture() -> Value {
        json!({"version":3,"periodStart":"2025-01-01","periodEnd":"2025-03-31","cutoff":"2025-03-31","controlsRun":["TEST"],"items":[{"code":"TEST","status":"passed","waivable":false,"count":0,"amount":0,"evidence":[],"sourceHash":"b".repeat(64)}],"blockers":0,"warnings":0,"hash":"a".repeat(64)})
    }
    fn review_fixture() -> Value {
        json!({"id":17,"packet":packet_fixture(),"reviewerActor":"user:synthetic","reviewerPrincipal":null,"createdAt":"2025-04-01 10:00:00"})
    }
    fn request() -> Vec<u8> {
        serde_json::to_vec(&json!({"periodStart":"2025-01-01","periodEnd":"2025-03-31","packetHash":"a".repeat(64),"confirm":true})).unwrap()
    }
    // Accept subsequent connections too, so the test observes unexpected retries/second writes.
    fn server(status: u16, response: String, target: &'static str, expected_body: Option<Vec<u8>>) -> (ApiClient, Arc<AtomicUsize>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        let handle = thread::spawn(move || {
            let mut last = std::time::Instant::now();
            while last.elapsed() < Duration::from_millis(400) {
                let (mut stream, _) = match listener.accept() {
                    Ok(s) => s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => { thread::sleep(Duration::from_millis(5)); continue; }
                    Err(e) => panic!("{e}"),
                };
                last = std::time::Instant::now();
                seen.fetch_add(1, Ordering::SeqCst);
                stream.set_nonblocking(false).unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut chunk = [0; 4096];
                    let n = stream.read(&mut chunk).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                    if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") { break i + 4; }
                };
                let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
                assert!(headers.starts_with(target), "{headers}");
                assert!(headers.to_lowercase().contains("cookie: session=synthetic"));
                let length = headers.lines().find_map(|line| line.to_lowercase().strip_prefix("content-length:").map(|s| s.trim().parse::<usize>().unwrap())).unwrap_or(0);
                while bytes.len() < header_end + length {
                    let mut chunk = [0;4096];
                    let n = stream.read(&mut chunk).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                }
                assert_eq!(&bytes[header_end..], expected_body.as_deref().unwrap_or_default());
                write!(stream,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
            }
        });
        (ApiClient::new(&format!("http://{address}"), true, Some("session=synthetic".into())).unwrap(), count, handle)
    }
    fn once(count: Arc<AtomicUsize>, handle: thread::JoinHandle<()>) {
        handle.join().unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn bare_review_and_exact_historical_nullable_status() {
        let value = review_fixture();
        let (client, count, handle) = server(200, value.to_string(), "POST /api/companies/synthetic/periods/original/review HTTP/1.1", Some(request()));
        assert_eq!(client.period_original_review("synthetic", request(), true).await.unwrap(), value);
        once(count, handle);
        for historical in [Value::Null, value.clone(), {
            let mut v = value;
            v["reviewerPrincipal"] = json!({"kind":"user","subjectId":"historical-user"});
            v
        }] {
            let expected = json!({"ok":true,"review":historical});
            let (client,count,handle) = server(200, expected.to_string(), "GET /api/companies/synthetic/periods/original/status?reviewId=17 HTTP/1.1", None);
            assert_eq!(client.period_original_status("synthetic",17).await.unwrap(),expected);
            once(count,handle);
        }
    }
    #[tokio::test]
    async fn readiness_and_unavailable_status_remain_flat() {
        let expected = json!({"ok":true,"packet":packet_fixture()});
        let (client,count,handle) = server(200, expected.to_string(), "GET /api/companies/synthetic/periods/original/readiness?from=2025-01-01&to=2025-03-31 HTTP/1.1",None);
        assert_eq!(client.period_original_readiness("synthetic","2025-01-01","2025-03-31").await.unwrap(),expected);
        once(count,handle);
        let expected = json!({"ok":true,"status":"unavailable","code":"PERIOD_CLOSE_REVIEW_SCHEMA_UNAVAILABLE","review":null});
        let (client,count,handle) = server(200,expected.to_string(),"GET /api/companies/synthetic/periods/original/status?reviewId=17 HTTP/1.1",None);
        assert_eq!(client.period_original_status("synthetic",17).await.unwrap(),expected);
        once(count,handle);
    }
    #[tokio::test]
    async fn stale_hash_refusal_and_strict_422() {
        for status in [200,422] {
            let mut expected = json!({"ok":false,"errors":["PERIOD_CLOSE_PACKET_STALE_OR_MISSING"],"packetHash":"c".repeat(64)});
            if status == 422 { expected["code"] = json!("PERIOD_CLOSE_PACKET_STALE_OR_MISSING"); }
            let (client,count,handle) = server(status,expected.to_string(),"POST /api/companies/synthetic/periods/original/review HTTP/1.1",Some(request()));
            assert_eq!(client.period_original_review("synthetic",request(),true).await.unwrap(),expected);
            once(count,handle);
        }
    }
    #[tokio::test]
    async fn malformed_auth_and_uncertain_writes_are_not_retried() {
        for (status,response,error) in [
            (200,"{",Error::OutcomeUnknown),
            (200,"{\"ok\":true,\"review\":{}}",Error::OutcomeUnknown),
            (200,"{\"ok\":false,\"errors\":[]}",Error::OutcomeUnknown),
            (401,"secret authentication details",Error::Refused{status:401}),
            (403,"{\"ok\":false,\"errors\":[\"private\"]}",Error::Refused{status:403}),
            (422,"{\"ok\":false,\"errors\":[\"private\"]}",Error::Refused{status:422}),
            (422,"{\"ok\":false,\"code\":\"x\",\"errors\":[42]}",Error::Refused{status:422}),
            (408,"{}",Error::OutcomeUnknown),
            (500,"{}",Error::OutcomeUnknown),
        ] {
            let (client,count,handle) = server(status,response.into(),"POST /api/companies/synthetic/periods/original/review HTTP/1.1",Some(request()));
            assert_eq!(client.period_original_review("synthetic",request(),true).await.unwrap_err(),error);
            once(count,handle);
        }
        let mut wrong = review_fixture();
        wrong["packet"]["hash"] = json!("d".repeat(64));
        let (client,count,handle) = server(200,wrong.to_string(),"POST /api/companies/synthetic/periods/original/review HTTP/1.1",Some(request()));
        assert_eq!(client.period_original_review("synthetic",request(),true).await.unwrap_err(),Error::OutcomeUnknown);
        once(count,handle);
    }
    #[tokio::test]
    async fn close_and_reopen_send_one_flat_source_write() {
        for op in [Operation::Close,Operation::Reopen] {
            let mut payload = json!({"periodStart":"2025-01-01","periodEnd":"2025-03-31","confirm":true});
            let mut result = json!({"ok":true,"periodId":19,"periodStart":"2025-01-01","periodEnd":"2025-03-31","kind":"vat_period","appliedRules":[RULE],"errors":[]});
            let target = if matches!(op,Operation::Close) {
                payload["status"] = json!("reported");
                result["status"] = json!("reported");
                result["readinessPacket"] = packet_fixture();
                "POST /api/companies/synthetic/periods/original/close HTTP/1.1"
            } else {
                payload["reason"] = json!("Correct synthetic posting");
                result["effectiveStatus"] = json!("open");
                result["reason"] = payload["reason"].clone();
                result["reopenedBy"] = json!("user:synthetic");
                "POST /api/companies/synthetic/periods/original/reopen HTTP/1.1"
            };
            let bytes = serde_json::to_vec(&payload).unwrap();
            let (client,count,handle) = server(200,result.to_string(),target,Some(bytes.clone()));
            let actual = if matches!(op,Operation::Close) { client.period_original_close("synthetic",bytes,true).await } else { client.period_original_reopen("synthetic",bytes,true).await };
            assert_eq!(actual.unwrap(),result);
            once(count,handle);
        }
    }
    #[tokio::test]
    async fn native_v4_packet_is_preserved_and_unknown_format_refuses() {
        let mut native = packet_fixture();
        native["version"] = json!(4);
        native["format"] = json!("rentemester.native-period-close.v1");
        native["sourceRevision"] = json!({"journal":[],"journalCount":[{"n":0}],"audit":[{"n":0,"head":null}]});
        let expected = json!({"ok":true,"packet":native});
        let (client,count,handle) = server(200,expected.to_string(),"GET /api/companies/synthetic/periods/original/readiness?from=2025-01-01&to=2025-03-31 HTTP/1.1",None);
        assert_eq!(client.period_original_readiness("synthetic","2025-01-01","2025-03-31").await.unwrap(),expected);
        once(count,handle);
        let mut stored = review_fixture(); stored["packet"] = native.clone();
        let (client,count,handle) = server(200,stored.to_string(),"POST /api/companies/synthetic/periods/original/review HTTP/1.1",Some(request()));
        assert_eq!(client.period_original_review("synthetic",request(),true).await.unwrap(),stored);
        once(count,handle);
        for (key,value) in [("format",json!("unknown")),("version",json!(5)),("sourceRevision",Value::Null)] {
            let mut invalid = native.clone(); invalid[key] = value;
            let (client,count,handle) = server(200,json!({"ok":true,"packet":invalid}).to_string(),"GET /api/companies/synthetic/periods/original/readiness?from=2025-01-01&to=2025-03-31 HTTP/1.1",None);
            assert_eq!(client.period_original_readiness("synthetic","2025-01-01","2025-03-31").await.unwrap_err(),Error::Protocol);
            once(count,handle);
        }
    }
    #[test]
    fn strict_input_and_operation_specific_output() {
        let valid: Value = serde_json::from_slice(&request()).unwrap();
        for (key,value) in [("actor",json!("user:untrusted")),("packetHash",json!("g".repeat(64))),("periodStart",json!("2025-02-30")),("confirm",json!(false))] {
            let mut bad = valid.clone(); bad[key] = value;
            assert!(input(Operation::Review,&serde_json::to_vec(&bad).unwrap(),true).is_err());
        }
        assert!(input(Operation::Review,&request(),false).is_err());
        let mut close = valid.clone();
        close.as_object_mut().unwrap().remove("packetHash");
        assert!(input(Operation::Close,&serde_json::to_vec(&close).unwrap(),true).is_ok());
        close["reviewId"] = json!(MAX_ID+1);
        assert!(input(Operation::Close,&serde_json::to_vec(&close).unwrap(),true).is_err());
        assert!(!success(&json!({"ok":true,"review":review_fixture()}),Operation::Status(18),&Value::Null));
        assert!(!success(&json!({"ok":true,"review":{}}),Operation::Status(17),&Value::Null));
        assert!(!success(&json!({"ok":true,"id":19,"errors":[],"appliedRules":[RULE]}),Operation::Close,&valid));
    }
}
