//! Original tax_return_prepare keeps the legacy errors[]/appliedRules envelope.
use crate::tools::tool;
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let mut t = tool(
        "tax_return_prepare",
        "Prepare closed-year corporate taxable income and deterministic adjustments;22% for ApS, other forms and non-deterministic items need owner/advisor review. Current company.read and existing annual CVR/balance/closed-year prerequisites apply. Original company/from/to strings; connected explicit company slug, filesystem/default launcher compatibility remains pending. Read-only preparation, no TastSelv submission, files, actor or confirmation.",
        json!({"type":"object","required":["company","from","to"],"properties":{"company":{"type":"string","minLength":1},"from":{"type":"string","minLength":1},"to":{"type":"string","minLength":1}},"additionalProperties":false}),
        true,
    );
    t["annotations"]["openWorldHint"] = json!(false);
    t["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object","additionalProperties":true},"errors":{"type":"array","items":{"type":"string"}},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"additionalProperties":false});
    vec![t]
}
fn result(envelope: Value) -> Value {
    json!({"isError":envelope["ok"]==false,"content":[{"type":"text","text":envelope.to_string()}],"structuredContent":envelope})
}
pub async fn call(api: &ApiClient, args: Value) -> Result<Value, &'static str> {
    let object = args
        .as_object()
        .ok_or("tax_return_prepare requires company/from/to")?;
    if object.len() != 3
        || object
            .keys()
            .any(|k| !matches!(k.as_str(), "company" | "from" | "to"))
    {
        return Err("tax_return_prepare requires only company/from/to");
    }
    let string = |key| {
        object
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or("tax_return_prepare requires nonempty company/from/to strings")
    };
    let (company, from, to) = (string("company")?, string("from")?, string("to")?);
    Ok(match api.annual_export(company, from, to, "tax").await {
        Ok(report) => {
            let mut envelope = json!({"ok":report["ok"],"errors":report["errors"]});
            if report["appliedRules"]
                .as_array()
                .is_some_and(|a| !a.is_empty())
            {
                envelope["appliedRules"] = report["appliedRules"].clone();
            }
            if report["ok"] == true {
                let mut data = report;
                let object = data.as_object_mut().unwrap();
                for key in ["ok", "errors", "appliedRules"] {
                    object.remove(key);
                }
                envelope["data"] = data;
            }
            result(envelope)
        }
        Err(error) => {
            let code = match error {
                Error::InvalidInput(_) => "invalid_input",
                Error::Transport => "transport",
                Error::Refused { .. } => "server_refused",
                Error::Protocol => "protocol",
                Error::OutcomeUnknown => "outcome_unknown",
            };
            result(json!({"ok":false,"errors":[error.to_string()],"code":code}))
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_required_inputs_and_legacy_output_schema() {
        let t = tools().remove(0);
        assert_eq!(t["name"], "tax_return_prepare");
        assert_eq!(
            t["inputSchema"]["required"],
            json!(["company", "from", "to"])
        );
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        assert_eq!(t["outputSchema"]["required"], json!(["ok", "errors"]));
        assert_eq!(t["annotations"]["openWorldHint"], false);
        assert_eq!(
            crate::tools::list()["tools"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|t| t["name"] == "tax_return_prepare")
                .count(),
            1
        );
    }
    #[test]
    fn unknown_null_and_empty_inputs_are_protocol_errors_calendar_errors_are_business() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        for args in [
            json!({"company":"synthetic","from":null,"to":"2020-12-31"}),
            json!({"company":"synthetic","from":"","to":"2020-12-31"}),
            json!({"company":"synthetic","from":"2020-01-01","to":"2020-12-31","confirm":true}),
        ] {
            assert!(
                rt.block_on(crate::tools::call(&api, "tax_return_prepare", args))
                    .is_err()
            );
        }
        let value = rt
            .block_on(crate::tools::call(
                &api,
                "tax_return_prepare",
                json!({"company":"synthetic","from":"2020-02-30","to":"2020-12-31"}),
            ))
            .unwrap();
        assert_eq!(value["isError"], true);
        assert!(value["structuredContent"]["errors"].is_array());
        assert!(value["structuredContent"].get("data").is_none());
    }
    #[test]
    fn tax_payload_is_unwrapped_and_business_failure_has_no_data() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        for ok in [true, false] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let task = thread::spawn(move || {
                let (mut s, _) = listener.accept().unwrap();
                s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = vec![];
                while !request.ends_with(b"\r\n\r\n") {
                    let mut b = [0; 4096];
                    let n = s.read(&mut b).unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&b[..n]);
                }
                let body=json!({"ok":true,"annualExport":{"ok":ok,"errors":if ok{json!([])}else{json!(["year is open"])},"appliedRules":["DK-TAX-RETURN-CORP-001"],"corporateTax":null,"needsReview":[{"kind":"company_form_out_of_scope"}],"taxableIncome":-25.05}}).to_string();
                write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                String::from_utf8(request).unwrap()
            });
            let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
            let response = rt
                .block_on(crate::tools::call(
                    &api,
                    "tax_return_prepare",
                    json!({"company":"synthetic","from":"2020-01-01","to":"2020-12-31"}),
                ))
                .unwrap();
            assert_eq!(response["isError"], !ok);
            let envelope = &response["structuredContent"];
            assert_eq!(envelope["appliedRules"], json!(["DK-TAX-RETURN-CORP-001"]));
            if ok {
                assert_eq!(envelope["errors"], json!([]));
                assert_eq!(envelope["data"]["corporateTax"], Value::Null);
                assert_eq!(envelope["data"]["taxableIncome"], -25.05);
                for key in ["ok", "errors", "appliedRules"] {
                    assert!(envelope["data"].get(key).is_none());
                }
            } else {
                assert!(envelope.get("data").is_none());
                assert_eq!(envelope["errors"], json!(["year is open"]));
            }
            assert!(task.join().unwrap().starts_with("GET /api/companies/synthetic/annual-exports?fiscalYearStart=2020-01-01&fiscalYearEnd=2020-12-31&kind=tax HTTP/1.1"));
        }
    }
}
