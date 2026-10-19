//! Exact native V1 manifest/mapping transport; server owns scope, dates, four-eyes review and money.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_INTERCOMPANY_BODY: usize = 256 * 1024;
#[derive(Clone, Copy)]
pub enum IntercompanyMutation {
    Manifest,
    Propose,
    Approve,
    Revoke,
}
impl IntercompanyMutation {
    fn suffix(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Propose => "propose",
            Self::Approve => "approve",
            Self::Revoke => "revoke",
        }
    }
}
pub fn intercompany_body_schema(action: IntercompanyMutation) -> Value {
    let text = json!({"type":"string","minLength":1});
    let id =
        json!({"type":"string","minLength":1,"maxLength":64,"pattern":"^[a-z][a-z0-9-]{0,63}$"});
    let date = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    let optional_date = json!({"type":["string","null"],"description":"Explicit validToExclusive; no date inferred"});
    let refs = json!({"type":"array","minItems":1,"maxItems":32,"items":{"type":"string","minLength":1,"maxLength":256}});
    match action {
        IntercompanyMutation::Manifest => {
            let membership = object(
                json!({"id":id,"companySlug":text,"validFrom":date,"validToExclusive":optional_date}),
                &["id", "companySlug", "validFrom"],
            );
            let ownership = object(
                json!({"id":id,"parentCompanySlug":text,"childCompanySlug":text,"basisPoints":{"type":"integer","minimum":1,"maximum":10000},"evidenceRefs":refs,"validFrom":date,"validToExclusive":optional_date}),
                &[
                    "id",
                    "parentCompanySlug",
                    "childCompanySlug",
                    "basisPoints",
                    "evidenceRefs",
                    "validFrom",
                ],
            );
            let group = object(
                json!({"id":id,"name":{"type":"string","minLength":1,"maxLength":160},"memberships":{"type":"array","minItems":1,"maxItems":1024,"items":membership},"ownership":{"type":"array","maxItems":1024,"items":ownership}}),
                &["id", "name", "memberships", "ownership"],
            );
            object(
                json!({"version":{"const":1},"groups":{"type":"array","minItems":1,"maxItems":128,"items":group},"confirm":{"const":true}}),
                &["version", "groups", "confirm"],
            )
        }
        IntercompanyMutation::Propose => {
            let accounts = json!({"type":"array","minItems":1,"maxItems":128,"items":{"type":"string","minLength":1,"maxLength":64}});
            let position = json!({"type":"string","enum":["receivable","payable"]});
            object(
                json!({"id":id,"groupId":id,"leftCompanySlug":text,"rightCompanySlug":text,"leftAccountNos":accounts,"rightAccountNos":accounts,"leftPosition":position,"rightPosition":position,"evidenceRefs":refs,"validFrom":date,"validToExclusive":optional_date,"confirm":{"const":true}}),
                &[
                    "id",
                    "groupId",
                    "leftCompanySlug",
                    "rightCompanySlug",
                    "leftAccountNos",
                    "rightAccountNos",
                    "leftPosition",
                    "rightPosition",
                    "evidenceRefs",
                    "validFrom",
                    "confirm",
                ],
            )
        }
        _ => object(
            json!({"mappingId":id,"mappingHash":{"type":"string","minLength":64,"maxLength":64},"confirm":{"const":true}}),
            &["mappingId", "mappingHash", "confirm"],
        ),
    }
}
fn utf8_bounds(schema: &mut Value) {
    match schema {
        Value::Object(m) => {
            if let Some(n) = m.get("maxLength").and_then(Value::as_u64) {
                m.insert("maxLength".into(), json!(n * 4));
            }
            for v in m.values_mut() {
                utf8_bounds(v);
            }
        }
        Value::Array(a) => {
            for v in a {
                utf8_bounds(v)
            }
        }
        _ => {}
    }
}
impl ApiClient {
    pub async fn intercompany(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "intercompany")?, &[]).await
    }
    /// Global membership-filtered source projection. Decimal strings are never converted or summed here.
    pub async fn group_reconciliation(&self, as_of: &str) -> Result<Value, Error> {
        if as_of.len() != 10
            || as_of.bytes().enumerate().any(|(i, b)| {
                if i == 4 || i == 7 {
                    b != b'-'
                } else {
                    !b.is_ascii_digit()
                }
            })
        {
            return Err(Error::InvalidInput(
                "explicit intercompany asOf YYYY-MM-DD required",
            ));
        }
        self.get("/api/group-reconciliation", &[("asOf", as_of)])
            .await
    }
    /// Exact body once. No actor/principal injection, generated keys/defaults or automatic retry.
    pub async fn mutate_intercompany(
        &self,
        company: &str,
        action: IntercompanyMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || bytes.len() > MAX_INTERCOMPANY_BODY {
            return Err(Error::InvalidInput(
                "explicit intercompany confirmation and native256KiB body required",
            ));
        }
        let mut schema = intercompany_body_schema(action);
        utf8_bounds(&mut schema);
        body(&bytes, &schema)?;
        self.request(
            Method::POST,
            &company_path(company, &format!("intercompany/{}", action.suffix()))?,
            &[],
            Some(bytes),
        )
        .await
    }
}
