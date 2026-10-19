//! Exact workspace party HTTP contracts. The server owns visibility, identity and provenance policy.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_PARTY_BODY: usize = 256 * 1024;
#[derive(Clone)]
pub enum PartyMutation {
    Create,
    Role(String),
    Assertion(String),
    Alias(String),
}
pub fn party_body_schema(action: &PartyMutation) -> Value {
    let text = json!({"type":"string","minLength":1});
    let role = json!({"type":"string","enum":["issuer","supplier","customer","recipient","payer","payee","processor","acquirer","related_company","establishment","location","payment_descriptor","vendor","owner","adviser","employee","authority","bank"]});
    let defaults = object(
        json!({"account":{},"vat":{},"currency":{},"paymentTermsDays":{}}),
        &[],
    );
    let review = json!({"type":"string","enum":["proposed","approved","rejected"]});
    match action {
        PartyMutation::Create => object(
            json!({"partyId":{"type":["string","null"],"maxLength":64},"kind":{"type":"string","enum":["person","organization","public_authority","financial_institution"]},"name":{"type":"string","minLength":1,"maxLength":320},"aliases":{"type":"array","maxItems":16,"items":{"type":"string","minLength":1,"maxLength":160}},"identifiers":{"type":"array","items":object(json!({"country":text,"identifier":{"type":["string","null"]},"identifierKind":{"type":"string","enum":["dk_cvr","eu_vat","non_eu"]}}),&["country","identifierKind"])},"source":text,"observedAt":text,"reviewAssertion":text,"role":role,"defaults":defaults,"confirm":{"const":true}}),
            &[
                "kind",
                "name",
                "source",
                "observedAt",
                "reviewAssertion",
                "role",
                "confirm",
            ],
        ),
        PartyMutation::Role(_) => object(
            json!({"role":role,"defaults":defaults,"confirm":{"const":true}}),
            &["role", "confirm"],
        ),
        PartyMutation::Assertion(_) => object(
            json!({"field":text,"value":text,"source":text,"observedAt":text,"reviewState":review,"confirm":{"const":true}}),
            &[
                "field",
                "value",
                "source",
                "observedAt",
                "reviewState",
                "confirm",
            ],
        ),
        PartyMutation::Alias(_) => object(
            json!({"alias":text,"source":text,"observedAt":text,"reviewState":review,"confirm":{"const":true}}),
            &["alias", "source", "observedAt", "reviewState", "confirm"],
        ),
    }
}
fn id_segment(id: &str) -> Result<String, Error> {
    if id.is_empty()
        || id.encode_utf16().count() > 64
        || matches!(id, "." | "..")
        || id.chars().any(char::is_control)
    {
        return Err(Error::InvalidInput("bounded partyId required"));
    }
    Ok(id
        .as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(b) {
                char::from(*b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect())
}
impl ApiClient {
    pub async fn workspace_parties(
        &self,
        company: &str,
        query: Option<&str>,
        cursor: usize,
        limit: usize,
    ) -> Result<Value, Error> {
        if query.is_some_and(|s| s.encode_utf16().count() > 512) || !(1..=100).contains(&limit) {
            return Err(Error::InvalidInput("party query max512, limit1..100"));
        }
        let c = cursor.to_string();
        let l = limit.to_string();
        let mut q = vec![("cursor", c.as_str()), ("limit", l.as_str())];
        if let Some(query) = query {
            q.push(("query", query));
        }
        self.get(&company_path(company, "workspace-parties")?, &q)
            .await
    }
    pub async fn workspace_party(&self, company: &str, id: &str) -> Result<Value, Error> {
        self.get(
            &company_path(company, &format!("workspace-parties/{}", id_segment(id)?))?,
            &[],
        )
        .await
    }
    /// Exact confirmed bytes once, no generated keys or retries. Read visible party/history after uncertainty.
    pub async fn mutate_workspace_party(
        &self,
        company: &str,
        action: PartyMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || bytes.len() > MAX_PARTY_BODY {
            return Err(Error::InvalidInput(
                "explicit party confirmation and native256KiB body required",
            ));
        }
        let mut value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidInput("native party JSON object required"))?;
        let mut schema = party_body_schema(&action);
        // Defaults are arbitrary source JSON under a fixed whitelist; never interpret accounting defaults.
        if let Some(defaults) = value
            .as_object_mut()
            .filter(|_| matches!(action, PartyMutation::Create | PartyMutation::Role(_)))
            .and_then(|v| v.remove("defaults"))
            && !defaults.is_null()
            && (!defaults.is_object()
                || defaults.as_object().unwrap().keys().any(|k| {
                    !["account", "vat", "currency", "paymentTermsDays"].contains(&k.as_str())
                }))
        {
            return Err(Error::InvalidInput(
                "unsupported company-scoped party defaults",
            ));
        }
        schema["properties"]
            .as_object_mut()
            .unwrap()
            .remove("defaults");
        relax_utf8(&mut schema);
        body(
            &serde_json::to_vec(&value).map_err(|_| Error::InvalidInput("invalid party JSON"))?,
            &schema,
        )?;
        let suffix = match action {
            PartyMutation::Create => "workspace-parties".into(),
            PartyMutation::Role(id) => format!("workspace-parties/{}/role", id_segment(&id)?),
            PartyMutation::Assertion(id) => {
                format!("workspace-parties/{}/assertion", id_segment(&id)?)
            }
            PartyMutation::Alias(id) => format!("workspace-parties/{}/alias", id_segment(&id)?),
        };
        self.request(
            Method::POST,
            &company_path(company, &suffix)?,
            &[],
            Some(bytes),
        )
        .await
    }
}
// Shared shape validator counts UTF-8; native text bounds are UTF-16 and remain server-owned.
fn relax_utf8(v: &mut Value) {
    match v {
        Value::Object(m) => {
            if let Some(n) = m.get("maxLength").and_then(Value::as_u64) {
                m.insert("maxLength".into(), json!(n * 4));
            }
            for v in m.values_mut() {
                relax_utf8(v);
            }
        }
        Value::Array(a) => {
            for v in a {
                relax_utf8(v);
            }
        }
        _ => {}
    }
}
