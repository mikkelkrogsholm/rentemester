//! Original invoice claims transport. Amounts, rates, dates, selection and authority remain server-owned.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginalInvoiceClaimKind {
    Remind,
    PostReminder,
    Compensation,
    ClaimCompensation,
    PostCompensation,
    Interest,
    ClaimInterest,
    PostInterest,
    InterestCorrection,
    PostInterestCorrection,
    SettleClaimBank,
}
impl OriginalInvoiceClaimKind {
    pub const ALL: [Self; 11] = [
        Self::Remind,
        Self::PostReminder,
        Self::Compensation,
        Self::ClaimCompensation,
        Self::PostCompensation,
        Self::Interest,
        Self::ClaimInterest,
        Self::PostInterest,
        Self::InterestCorrection,
        Self::PostInterestCorrection,
        Self::SettleClaimBank,
    ];
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Remind => "remind",
            Self::PostReminder => "post-reminder",
            Self::Compensation => "compensation",
            Self::ClaimCompensation => "claim-compensation",
            Self::PostCompensation => "post-compensation",
            Self::Interest => "interest",
            Self::ClaimInterest => "claim-interest",
            Self::PostInterest => "post-interest",
            Self::InterestCorrection => "interest-correction",
            Self::PostInterestCorrection => "post-interest-correction",
            Self::SettleClaimBank => "settle-claim-bank",
        }
    }
    pub fn is_read_only(self) -> bool {
        matches!(
            self,
            Self::Compensation | Self::Interest | Self::InterestCorrection
        )
    }
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
pub fn invoice_claims_original_schema(kind: OriginalInvoiceClaimKind) -> Value {
    use OriginalInvoiceClaimKind::*;
    let id = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    let string = json!({"type":"string"});
    let number = json!({"type":"number"});
    let mut fields = json!({"documentId":id,"invoiceNumber":string});
    let mut required = vec![];
    match kind {
        Remind => {
            fields["date"] = string.clone();
            fields["fee"] = number.clone();
            fields["note"] = string.clone();
            required.push("date");
        }
        PostReminder => {
            fields["reminderId"] = id.clone();
            fields["date"] = string.clone();
            fields["receivableAccountNo"] = string.clone();
            fields["reminderIncomeAccountNo"] = string.clone();
            required.extend(["receivableAccountNo", "reminderIncomeAccountNo"]);
        }
        Compensation | ClaimCompensation => {
            fields["asOf"] = string.clone();
            fields["amountDkk"] = number.clone();
            required.push("asOf");
            if kind == ClaimCompensation {
                fields["note"] = string.clone();
            }
        }
        PostCompensation => {
            fields["date"] = string.clone();
            fields["receivableAccountNo"] = string.clone();
            fields["compensationIncomeAccountNo"] = string.clone();
            required.extend(["receivableAccountNo", "compensationIncomeAccountNo"]);
        }
        Interest | ClaimInterest => {
            fields["asOf"] = string.clone();
            fields["referenceRate"] = number.clone();
            required.push("asOf");
            if kind == ClaimInterest {
                fields["note"] = string.clone();
            }
        }
        PostInterest => {
            fields["claimId"] = id.clone();
            fields["date"] = string.clone();
            fields["receivableAccountNo"] = string.clone();
            fields["interestIncomeAccountNo"] = string.clone();
            required.extend(["receivableAccountNo", "interestIncomeAccountNo"]);
        }
        InterestCorrection => {}
        PostInterestCorrection => {
            fields["date"] = string.clone();
            fields["reason"] = json!({"type":"string","minLength":1,"maxLength":4096});
            required.push("reason");
        }
        SettleClaimBank => {
            fields = json!({"payload":object(json!({"invoiceDocumentId":id,"invoiceNumber":string,"bankTransactionId":id,"bankTransactionReference":string,"paymentDate":string,"amount":number,"bankAccountNo":string,"receivableAccountNo":string}),&[])});
            required.push("payload");
        }
    }
    if !kind.is_read_only() {
        fields["confirm"] = json!({"const":true});
        required.push("confirm");
    }
    object(fields, &required)
}
pub fn validate_invoice_claims_original(
    kind: OriginalInvoiceClaimKind,
    body: &[u8],
) -> Result<(), Error> {
    crate::domain::body(body, &invoice_claims_original_schema(kind))
}
impl ApiClient {
    pub async fn invoice_claims_original(
        &self,
        company: &str,
        kind: OriginalInvoiceClaimKind,
        body: Vec<u8>,
    ) -> Result<Value, Error> {
        validate_invoice_claims_original(kind, &body)?;
        let render = kind.is_read_only();
        let action = kind.suffix();
        let path = company_path(company, &format!("invoices/original/{action}"))?;
        let unknown = || {
            if render {
                Error::Transport
            } else {
                Error::OutcomeUnknown
            }
        };
        let mut response = self
            .send(
                Method::POST,
                &path,
                &[],
                Some(body),
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await
            .map_err(|e| {
                if render && e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
        let status = response.status();
        if !render && (status.is_server_error() || status.as_u16() == 408) {
            return Err(Error::OutcomeUnknown);
        }
        if !status.is_success() {
            return Err(Error::Refused {
                status: status.as_u16(),
            });
        }
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| unknown())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(unknown());
            }
            bytes.extend_from_slice(&chunk);
        }
        let protocol = || {
            if render {
                Error::Protocol
            } else {
                Error::OutcomeUnknown
            }
        };
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| protocol())?;
        let valid = value["ok"].is_boolean()
            && value["errors"]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string))
            && value.get("code").is_none_or(Value::is_string)
            && value
                .get("appliedRules")
                .is_none_or(|v| v.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
            && if value["ok"] == true {
                value["data"].is_object() && value["errors"].as_array().is_some_and(Vec::is_empty)
            } else {
                value.get("data").is_none()
                    && value["errors"].as_array().is_some_and(|a| !a.is_empty())
            };
        if !valid {
            return Err(protocol());
        }
        Ok(value)
    }
}
