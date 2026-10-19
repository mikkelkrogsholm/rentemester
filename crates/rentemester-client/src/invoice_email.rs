//! Explicit invoice delivery. Known refusals retain canonical evidence; ambiguous writes never replay.
use crate::{
    ApiClient, Error, MAX_RESPONSE, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_INVOICE_EMAIL_BODY: usize = 16 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvoiceEmailMutation {
    Send,
    Reconcile,
    Retry,
}
pub fn invoice_email_body_schema(action: InvoiceEmailMutation) -> Value {
    if action == InvoiceEmailMutation::Send {
        return object(
            json!({"invoiceDocumentId":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"to":{"type":"string","minLength":1,"description":"Explicit single ASCII address, max254 bytes after native trimming; no control/header injection"},"kind":{"type":["string","null"],"enum":["invoice","reminder",null]},"confirm":{"const":true}}),
            &["invoiceDocumentId", "to", "confirm"],
        );
    }
    let mut fields = json!({"messageId":{"type":"string","maxLength":128,"pattern":"^<[0-9a-f]{32}@rentemester\\.local>$"},"expectedStateHash":{"type":"string","minLength":64,"maxLength":64,"pattern":"^[0-9a-f]{64}$"},"reason":{"type":"string","minLength":1,"description":"Trimmed10..2000 UTF8 bytes, no control characters; exact input forwarded"},"confirm":{"const":true}});
    let mut required = vec!["messageId", "expectedStateHash", "reason", "confirm"];
    if action == InvoiceEmailMutation::Reconcile {
        fields["resolution"] = json!({"type":"string","enum":["accepted","not_accepted"]});
        fields["evidence"] = json!({"type":"string","minLength":1,"description":"Explicit external SMTP evidence reference, trimmed10..4000 UTF8 bytes, no control characters"});
        required.extend(["resolution", "evidence"]);
    }
    object(fields, &required)
}
fn validate(action: InvoiceEmailMutation, bytes: &[u8], confirmed: bool) -> Result<(), Error> {
    if !confirmed || bytes.len() > MAX_INVOICE_EMAIL_BODY {
        return Err(Error::InvalidInput(
            "explicit confirmation and native16KiB body required",
        ));
    }
    body(bytes, &invoice_email_body_schema(action))?;
    let v: Value = serde_json::from_slice(bytes)
        .map_err(|_| Error::InvalidInput("native invoice email JSON required"))?;
    if action == InvoiceEmailMutation::Send {
        let id = v["invoiceDocumentId"]
            .as_i64()
            .ok_or(Error::InvalidInput("positive exact document ID required"))?;
        if !(1..=9_007_199_254_740_991).contains(&id) {
            return Err(Error::InvalidInput("positive exact document ID required"));
        }
        let raw = v["to"].as_str().unwrap_or("");
        let address = raw.trim();
        if raw.chars().any(char::is_control)
            || address.is_empty()
            || address.len() > 254
            || !address.is_ascii()
            || address
                .chars()
                .any(|c| c.is_ascii_whitespace() || c.is_control() || ",;<>".contains(c))
            || address.matches('@').count() != 1
            || address.starts_with('@')
            || address.ends_with('@')
            || !address
                .split_once('@')
                .is_some_and(|(_, domain)| domain.contains('.'))
        {
            return Err(Error::InvalidInput(
                "explicit bounded single ASCII recipient required",
            ));
        }
    } else {
        let hash = v["expectedStateHash"].as_str().unwrap_or("");
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::InvalidInput("exact lowercase state SHA256 required"));
        }
        let mid = v["messageId"].as_str().unwrap_or("");
        let fingerprint = mid
            .strip_prefix('<')
            .and_then(|s| s.strip_suffix("@rentemester.local>"));
        if !fingerprint.is_some_and(|s| {
            s.len() == 32
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }) {
            return Err(Error::InvalidInput(
                "exact canonical invoice message ID required",
            ));
        }
        for (key, max) in [("reason", 2000), ("evidence", 4000)] {
            if key == "evidence" && action != InvoiceEmailMutation::Reconcile {
                continue;
            }
            let s = v[key].as_str().unwrap_or("");
            if s.chars().any(char::is_control) || !(10..=max).contains(&s.trim().len()) {
                return Err(Error::InvalidInput(
                    "explicit bounded reason/evidence without control characters required",
                ));
            }
        }
    }
    Ok(())
}
impl ApiClient {
    pub async fn invoice_email_status(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "invoices/email-deliveries")?, &[])
            .await
    }
    pub async fn invoice_email_send(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.invoice_email_write(company, InvoiceEmailMutation::Send, bytes, confirmed)
            .await
    }
    pub async fn invoice_email_reconcile(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.invoice_email_write(company, InvoiceEmailMutation::Reconcile, bytes, confirmed)
            .await
    }
    pub async fn invoice_email_retry(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.invoice_email_write(company, InvoiceEmailMutation::Retry, bytes, confirmed)
            .await
    }
    async fn invoice_email_write(
        &self,
        company: &str,
        action: InvoiceEmailMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        validate(action, &bytes, confirmed)?;
        let suffix = match action {
            InvoiceEmailMutation::Send => "invoices/send-email",
            InvoiceEmailMutation::Reconcile => "invoices/email-deliveries/reconcile",
            InvoiceEmailMutation::Retry => "invoices/email-deliveries/retry",
        };
        let mut response = self
            .send(
                Method::POST,
                &company_path(company, suffix)?,
                &[],
                Some(bytes),
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await?;
        let status = response.status().as_u16();
        if status >= 500 {
            return Err(Error::OutcomeUnknown);
        }
        if ![200, 409, 422].contains(&status) {
            return Err(Error::Refused { status });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::OutcomeUnknown)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(Error::OutcomeUnknown);
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::OutcomeUnknown)?;
        if status == 200 {
            let expected = if action == InvoiceEmailMutation::Reconcile {
                matches!(
                    value["delivery"]["state"].as_str(),
                    Some("accepted" | "refused")
                )
            } else {
                value["delivery"]["state"] == "accepted"
            };
            return if value["ok"] == true && value["delivery"].is_object() && expected {
                Ok(value)
            } else {
                Err(Error::OutcomeUnknown)
            };
        }
        let (code, state) = if status == 409 {
            ("delivery_unknown", "unknown")
        } else {
            ("delivery_refused", "refused")
        };
        if value["ok"] == false
            && value["code"] == code
            && value["delivery"].is_object()
            && value["delivery"]["state"] == state
        {
            Ok(value)
        } else if status == 409 && value["code"] == "delivery_unknown" {
            Err(Error::OutcomeUnknown)
        } else {
            Err(Error::Refused { status })
        }
    }
}
