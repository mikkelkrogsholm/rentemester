//! Exact evidence lifecycle transport. Never posts, pays, computes money or refreshes ledger heads.
use crate::{
    ApiClient, Error, MAX_RESPONSE, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_DISPOSITION_BODY: usize = 32 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispositionAction {
    Plan,
    Propose,
    Approve,
    Link,
    Settle,
    Supersede,
    Reopen,
}
impl DispositionAction {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Propose => "propose",
            Self::Approve => "approve",
            Self::Link => "link",
            Self::Settle => "settle",
            Self::Supersede => "supersede",
            Self::Reopen => "reopen",
        }
    }
    pub fn is_write(self) -> bool {
        self != Self::Plan
    }
}
pub fn disposition_body_schema(action: DispositionAction) -> Value {
    let text = json!({"type":"string","description":"Server trims/NFC-normalizes and validates source UTF16 length; exact request bytes are forwarded unchanged"});
    let date = json!({"type":"string","description":"Explicit source ISO date; server validates calendar and semantics"});
    let refs = json!({"type":"array","minItems":1,"maxItems":64,"uniqueItems":true,"items":text});
    let hash = json!({"type":"string","minLength":64,"maxLength":64,"description":"Exact server-returned lowercase SHA256, never computed or refreshed by adapter"});
    let mut schema = if matches!(action, DispositionAction::Plan | DispositionAction::Propose) {
        let side = object(
            json!({"companySlug":text,"role":text,"expectedSide":{"type":"string","enum":["receivable","payable"]}}),
            &["companySlug", "role", "expectedSide"],
        );
        let disposition = object(
            json!({"dispositionId":{"type":["string","null"],"description":"Reuse the exact normalized plan dispositionId for read-first recovery. Source-compatible omission/null/empty may create a fresh server UUID; adapters never generate or retry it."},"type":{"type":"string","enum":["invoice","reimbursement","loan","capital","dividend","repayment","other"]},"economicDate":date,"amount":{"type":"number","exclusiveMinimum":0,"description":"Positive source amount with exact cents; no adapter arithmetic or rounding"},"currency":text,"settlementDueDate":{"type":["string","null"]},"partyIds":refs,"evidenceRecordIds":refs,"left":side,"right":side}),
            &[
                "type",
                "economicDate",
                "amount",
                "currency",
                "partyIds",
                "evidenceRecordIds",
                "left",
                "right",
            ],
        );
        object(json!({"disposition":disposition}), &["disposition"])
    } else {
        object(
            json!({"dispositionId":text,"payloadHash":hash}),
            &["dispositionId", "payloadHash"],
        )
    };
    let mut add = |name: &str, value: Value| {
        schema["properties"][name] = value;
        schema["required"].as_array_mut().unwrap().push(json!(name));
    };
    match action {
        DispositionAction::Link => {
            add("side", json!({"type":"string","enum":["left","right"]}));
            add(
                "journalEntryId",
                json!({"type":"integer","minimum":1,"maximum":9007199254740991u64}),
            );
            add(
                "ledgerHeadHash",
                json!({"type":["string","null"],"description":"Exact current candidate head for new links. Source-compatible omission/null is forwarded unchanged; never inferred, generated or refreshed."}),
            );
        }
        DispositionAction::Settle => {
            let mut evidence = refs;
            evidence.as_object_mut().unwrap().remove("uniqueItems");
            add("settlementEvidenceRecordIds", evidence);
        }
        DispositionAction::Supersede => {
            add("replacementDispositionId", text.clone());
            add("reason", text);
        }
        DispositionAction::Reopen => add("reason", text),
        _ => {}
    }
    if action.is_write() {
        add("confirm", json!({"const":true}));
    }
    if action == DispositionAction::Link {
        schema["required"]
            .as_array_mut()
            .unwrap()
            .retain(|k| k != "ledgerHeadHash");
    }
    schema
}
impl ApiClient {
    pub async fn group_disposition_status(
        &self,
        id: &str,
        as_of: Option<&str>,
    ) -> Result<Value, Error> {
        if id.trim().is_empty() || matches!(id, "." | "..") {
            return Err(Error::InvalidInput("explicit disposition ID required"));
        }
        let mut url = reqwest::Url::parse(&self.origin)
            .map_err(|_| Error::InvalidInput("explicit server origin required"))?;
        url.path_segments_mut()
            .map_err(|_| Error::InvalidInput("server origin cannot carry path segments"))?
            .extend(["api", "group-dispositions", id]);
        let mut query = vec![];
        if let Some(day) = as_of {
            if day.is_empty() {
                return Err(Error::InvalidInput("explicit asOf date required"));
            }
            query.push(("asOf", day));
        }
        self.disposition_json(Method::GET, url.path(), &query, None, false)
            .await
    }
    pub async fn intercompany_disposition(
        &self,
        company: &str,
        action: DispositionAction,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if confirmed != action.is_write() || bytes.len() > MAX_DISPOSITION_BODY {
            return Err(Error::InvalidInput(
                "read-only plan forbids confirmation; writes require explicit confirmation and exact native32KiB JSON",
            ));
        }
        body(&bytes, &disposition_body_schema(action))?;
        self.disposition_json(
            Method::POST,
            &company_path(company, &format!("group-dispositions/{}", action.suffix()))?,
            &[],
            Some(bytes),
            action.is_write(),
        )
        .await
    }
    async fn disposition_json(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Vec<u8>>,
        write: bool,
    ) -> Result<Value, Error> {
        let unknown = || {
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
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await
            .map_err(|e| {
                if !write && e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
        let status = response.status();
        if write && (status.is_server_error() || status.as_u16() == 408) {
            return Err(Error::OutcomeUnknown);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unknown())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(unknown());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status.is_success() {
                unknown()
            } else {
                Error::Refused {
                    status: status.as_u16(),
                }
            }
        })?;
        if value["ok"] == false && value["code"].as_str().is_some_and(|s| !s.is_empty()) {
            if write
                && matches!(
                    value["code"].as_str(),
                    Some("internal" | "outcome_unknown" | "outcome_unconfirmed")
                )
            {
                return Err(Error::OutcomeUnknown);
            }
            return Ok(value);
        }
        if status.is_success() && value["ok"] == true {
            Ok(value)
        } else if status.is_success() {
            Err(unknown())
        } else {
            Err(Error::Refused {
                status: status.as_u16(),
            })
        }
    }
}
