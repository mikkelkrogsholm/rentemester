//! Explicit local PDF inspection; the server owns originals, parsing and evidence.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};

fn id(value: i64) -> Result<(), Error> {
    if (1..=9_007_199_254_740_991).contains(&value) {
        Ok(())
    } else {
        Err(Error::InvalidInput("positive exact document id required"))
    }
}
impl ApiClient {
    pub async fn document_parse(
        &self,
        company: &str,
        document_id: i64,
        confirmed: bool,
    ) -> Result<Value, Error> {
        id(document_id)?;
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit PDF parsing confirmation required",
            ));
        }
        self.pdf_request(
            Method::POST,
            &company_path(company, &format!("documents/{document_id}/parse"))?,
            &[],
            Some(serde_json::to_vec(&json!({"confirm":true})).map_err(|_| Error::Protocol)?),
        )
        .await
    }
    pub async fn document_parse_status(
        &self,
        company: &str,
        document_id: i64,
    ) -> Result<Value, Error> {
        id(document_id)?;
        self.pdf_request(
            Method::GET,
            &company_path(company, &format!("documents/{document_id}/parse-status"))?,
            &[],
            None,
        )
        .await
    }
    pub async fn document_parsed_text(
        &self,
        company: &str,
        document_id: i64,
        offset: u64,
        limit: u8,
    ) -> Result<Value, Error> {
        id(document_id)?;
        if offset > 9_007_199_254_740_991 || !(1..=10).contains(&limit) {
            return Err(Error::InvalidInput(
                "exact nonnegative offset and limit1..10 required",
            ));
        }
        let offset = offset.to_string();
        let limit = limit.to_string();
        self.pdf_request(
            Method::GET,
            &company_path(company, &format!("documents/{document_id}/parsed-text"))?,
            &[("offset", &offset), ("limit", &limit)],
            None,
        )
        .await
    }
    pub async fn documents_parse_pending(
        &self,
        company: &str,
        cursor: u64,
        limit: u8,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || cursor > 9_007_199_254_740_991 || !(1..=100).contains(&limit) {
            return Err(Error::InvalidInput(
                "confirmation, exact nonnegative cursor and limit1..100 required",
            ));
        }
        self.pdf_request(
            Method::POST,
            &company_path(company, "documents/parse-pending")?,
            &[],
            Some(
                serde_json::to_vec(&json!({"confirm":true,"cursor":cursor,"limit":limit}))
                    .map_err(|_| Error::Protocol)?,
            ),
        )
        .await
    }
    // Preserve curated evidence/refusal codes for agent callers. A transport failure
    // after a write remains unknown, including an incomplete or oversized response.
    async fn pdf_request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Vec<u8>>,
    ) -> Result<Value, Error> {
        let write = body.is_some();
        let failure = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            }
        };
        let mut response = self
            .send(
                method,
                path,
                query,
                body,
                if self.service_key.is_some() {
                    None
                } else {
                    self.cookie.as_ref().map(|c| c.as_str())
                },
            )
            .await?;
        let status = response.status();
        if status.is_server_error() || status.as_u16() == 408 {
            return Err(failure());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| failure())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(failure());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status.is_success() {
                failure()
            } else {
                Error::Refused {
                    status: status.as_u16(),
                }
            }
        })?;
        if value["ok"] == false
            && (value["code"].is_string() || status.is_success() && value["errors"].is_array())
        {
            return Ok(value);
        }
        if status.is_success() && value["ok"] == true && valid_pdf(&value, path) {
            Ok(value)
        } else if status.is_success() {
            Err(if write {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            })
        } else {
            Err(Error::Refused {
                status: status.as_u16(),
            })
        }
    }
}

fn exact(v: &Value, fields: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.len() == fields.len() && o.keys().all(|k| fields.contains(&k.as_str())))
}
fn natural(v: &Value) -> bool {
    v.as_u64().is_some_and(|n| n <= 9_007_199_254_740_991)
}
fn positive(v: &Value) -> bool {
    natural(v) && v.as_u64() != Some(0)
}
fn hash(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
}
fn status_fields(v: &Value, summary: bool) -> bool {
    let mut fields = vec![
        "documentId",
        "status",
        "errorCode",
        "pageCount",
        "itemCount",
        "textLength",
        "resultHash",
    ];
    if summary {
        fields.push("cached");
    } else {
        fields.extend([
            "sourceSha256",
            "parserId",
            "parserVersion",
            "contractVersion",
        ]);
    }
    exact(v, &fields)
        && positive(&v["documentId"])
        && v["status"].as_str().is_some_and(|s| !s.is_empty())
        && (v["errorCode"].is_null() || v["errorCode"].is_string())
        && ["pageCount", "itemCount", "textLength"]
            .iter()
            .all(|k| natural(&v[*k]))
        && hash(&v["resultHash"])
        && if summary {
            v["cached"].is_boolean()
        } else {
            hash(&v["sourceSha256"])
                && ["parserId", "parserVersion", "contractVersion"]
                    .iter()
                    .all(|k| v[*k].as_str().is_some_and(|s| !s.is_empty()))
        }
}
fn valid_pdf(v: &Value, path: &str) -> bool {
    if path.ends_with("/parse-pending") {
        let b = &v["batch"];
        return exact(v, &["ok", "batch"])
            && exact(
                b,
                &[
                    "requested",
                    "parsed",
                    "failed",
                    "cursor",
                    "nextCursor",
                    "resume",
                ],
            )
            && ["requested", "parsed", "failed", "cursor"]
                .iter()
                .all(|k| natural(&b[*k]))
            && b["requested"].as_u64().is_some_and(|n| {
                n <= 100
                    && b["parsed"]
                        .as_u64()
                        .unwrap_or(u64::MAX)
                        .checked_add(b["failed"].as_u64().unwrap_or(u64::MAX))
                        == Some(n)
            })
            && (b["nextCursor"].is_null() || natural(&b["nextCursor"]))
            && (b["resume"].is_null()
                || exact(&b["resume"], &["documentIds"])
                    && b["resume"]["documentIds"]
                        .as_array()
                        .is_some_and(|a| a.len() <= 100 && a.iter().all(positive)));
    }
    if path.ends_with("/parsed-text") {
        return exact(
            v,
            &["ok", "parse", "pages", "offset", "limit", "nextOffset"],
        ) && (v["parse"].is_null() || status_fields(&v["parse"], false))
            && natural(&v["offset"])
            && v["limit"].as_u64().is_some_and(|n| (1..=10).contains(&n))
            && (v["nextOffset"].is_null() || natural(&v["nextOffset"]))
            && v["pages"].as_array().is_some_and(|a| {
                a.len() <= v["limit"].as_u64().unwrap_or(0) as usize
                    && a.iter().all(|p| {
                        exact(
                            p,
                            &[
                                "pageNumber",
                                "width",
                                "height",
                                "rotation",
                                "text",
                                "itemCount",
                                "layoutHash",
                            ],
                        ) && positive(&p["pageNumber"])
                            && ["width", "height", "rotation"]
                                .iter()
                                .all(|k| p[*k].is_number())
                            && p["text"].is_string()
                            && natural(&p["itemCount"])
                            && hash(&p["layoutHash"])
                    })
            });
    }
    exact(v, &["ok", "parse"])
        && if path.ends_with("/parse-status") {
            v["parse"].is_null() || status_fields(&v["parse"], false)
        } else {
            status_fields(&v["parse"], true)
        }
}
