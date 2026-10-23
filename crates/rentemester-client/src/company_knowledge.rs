//! Thin source-backed advisory knowledge adapter; server owns authority/state/dates/conflicts.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_KNOWLEDGE_BODY: usize = 256 * 1024;
#[derive(Clone, Copy)]
pub enum KnowledgeMutation {
    Propose,
    Review,
    Supersede,
}
impl KnowledgeMutation {
    fn suffix(self) -> &'static str {
        match self {
            Self::Propose => "propose",
            Self::Review => "review",
            Self::Supersede => "supersede",
        }
    }
}
pub fn knowledge_body_schema(action: KnowledgeMutation) -> Value {
    let text = json!({"type":"string","minLength":1});
    let mut proposal = object(
        json!({"assertionId":{"type":"string","minLength":3,"maxLength":80},"predicate":{"type":"string","enum":["business_description","operating_status","products_services","revenue_models","markets","customer_types","industry_tags","operational_systems","document_sources","reporting_dimensions","responsible_party","canonical_reference"]},"value":{},"source":object(json!({"kind":{"type":"string","enum":["user","registry","party","corporate_record","external_snapshot","canonical_setting"]},"ref":{"type":"string","minLength":1,"maxLength":512}}),&["kind","ref"]),"validFrom":text,"validToExclusive":{"type":["string","null"]},"certainty":{"type":"string","enum":["confirmed","probable","uncertain"]}}),
        &["predicate", "value", "source", "validFrom"],
    );
    match action {
        KnowledgeMutation::Propose => {
            proposal["properties"]["confirm"] = json!({"const":true});
            proposal["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("confirm"));
            proposal
        }
        KnowledgeMutation::Review => object(
            json!({"assertionId":text,"decision":{"type":"string","enum":["approved","rejected"]},"reason":{"type":["string","null"]},"confirm":{"const":true}}),
            &["assertionId", "decision", "confirm"],
        ),
        KnowledgeMutation::Supersede => object(
            json!({"assertionId":text,"replacement":proposal,"confirm":{"const":true}}),
            &["assertionId", "replacement", "confirm"],
        ),
    }
}
impl ApiClient {
    pub async fn company_knowledge(
        &self,
        company: &str,
        as_of: Option<&str>,
        include_proposed: bool,
    ) -> Result<Value, Error> {
        let company = crate::knowledge_company_selector(self.local.as_ref(), company)?;
        let mut query = vec![(
            "includeProposed",
            if include_proposed { "true" } else { "false" },
        )];
        if let Some(at) = as_of {
            if at.is_empty() || at.len() > 64 {
                return Err(Error::InvalidInput(
                    "bounded explicit knowledge asOf required",
                ));
            }
            query.push(("asOf", at));
        }
        self.get(&company_path(&company, "knowledge")?, &query).await
    }
    /// Sends exact explicitly confirmed JSON once. Unknown outcomes require canonical history reads.
    pub async fn mutate_company_knowledge(
        &self,
        company: &str,
        action: KnowledgeMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        let company = crate::knowledge_company_selector(self.local.as_ref(), company)?;
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit knowledge confirmation required",
            ));
        }
        if bytes.len() > MAX_KNOWLEDGE_BODY {
            return Err(Error::InvalidInput("knowledge exceeds native256KiB"));
        }
        validate_body(&bytes, action)?;
        self.request(
            Method::POST,
            &company_path(&company, &format!("knowledge/{}", action.suffix()))?,
            &[],
            Some(bytes),
        )
        .await
    }
}

// Validate fixed envelope fields using shared contracts, leaving arbitrary JSON values
// to the server's canonical-size/hash policy. Only this validation copy is altered.
fn validate_body(bytes: &[u8], action: KnowledgeMutation) -> Result<(), Error> {
    let mut value: Value = serde_json::from_slice(bytes)
        .map_err(|_| Error::InvalidInput("expected native knowledge JSON"))?;
    let mut schema = knowledge_body_schema(action);
    let (proposal, shape) = match action {
        KnowledgeMutation::Propose => (Some(&mut value), Some(&mut schema)),
        KnowledgeMutation::Supersede => (
            value.get_mut("replacement"),
            schema
                .get_mut("properties")
                .and_then(|v| v.get_mut("replacement")),
        ),
        KnowledgeMutation::Review => (None, None),
    };
    if let (Some(proposal), Some(shape)) = (proposal, shape) {
        let object = proposal
            .as_object_mut()
            .ok_or(Error::InvalidInput("knowledge proposal must be an object"))?;
        if object.remove("value").is_none() {
            return Err(Error::InvalidInput("knowledge proposal requires value"));
        }
        shape["properties"].as_object_mut().unwrap().remove("value");
        // Shared validator counts UTF-8 bytes; authoritative source bounds are UTF-16.
        shape["properties"]["source"]["properties"]["ref"]["maxLength"] = json!(2048);
        shape["properties"]["assertionId"]["maxLength"] = json!(320);
        shape["required"]
            .as_array_mut()
            .unwrap()
            .retain(|v| v != "value");
    }
    body(
        &serde_json::to_vec(&value).map_err(|_| Error::InvalidInput("invalid knowledge JSON"))?,
        &schema,
    )
}
