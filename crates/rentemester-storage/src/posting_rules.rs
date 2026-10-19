//! Bounded manual rule proposals. Unknown conditions/outcomes refuse; never auto-post.
use crate::{accounting_drafts::refused, invalid};
use rentemester_engine::{Error, fiscal_year::Date};
use rusqlite::{Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub confirm: bool,
    pub proposal: Option<Proposal>,
    pub rule_id: Option<String>,
    pub version: Option<i64>,
    pub expected_payload_hash: Option<String>,
    pub rationale: Option<String>,
    pub provenance: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Proposal {
    pub rule_id: String,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub conditions: Conditions,
    pub outcome: Outcome,
    pub provenance: String,
    pub rationale: String,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Conditions {
    pub currency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Attributes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supplier_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supplier_country: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supplier_vat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vat: Option<String>,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Attributes {
    pub bank_text: String,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Outcome {
    pub account: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vat_treatment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<std::collections::BTreeMap<String, String>>,
}
fn text(s: &str, max: usize) -> Result<(), Error> {
    if s.trim().is_empty() || s.len() > max {
        Err(refused("bounded rule text required"))
    } else {
        Ok(())
    }
}
pub fn list(tx: &Transaction<'_>) -> Result<Value, Error> {
    crate::validate(tx)?;
    let mut q=tx.prepare("SELECT v.rule_id,v.version,v.payload_hash,v.provenance,v.rationale,v.created_by,v.created_at,v.conditions_json,v.outcome_json,v.effective_from,v.effective_to,(SELECT state FROM posting_rule_lifecycle_events WHERE rule_version_id=v.id AND effective_at<=strftime('%Y-%m-%dT%H:%M:%fZ','now') ORDER BY effective_at DESC,id DESC LIMIT 1) FROM posting_rule_versions v WHERE v.company_id=(SELECT id FROM companies ORDER BY id LIMIT 1) ORDER BY v.rule_id,v.version").map_err(invalid)?;
    let rows=q.query_map([],|r|Ok(json!({"ruleId":r.get::<_,String>(0)?,"version":r.get::<_,i64>(1)?,"payloadHash":r.get::<_,String>(2)?,"provenance":r.get::<_,String>(3)?,"rationale":r.get::<_,String>(4)?,"createdBy":r.get::<_,String>(5)?,"createdAt":r.get::<_,String>(6)?,"conditions":r.get::<_,String>(7)?,"outcome":r.get::<_,String>(8)?,"effectiveFrom":r.get::<_,String>(9)?,"effectiveTo":r.get::<_,Option<String>>(10)?,"state":r.get::<_,Option<String>>(11)?}))).map_err(invalid)?;
    let mut out = vec![];
    for r in rows {
        let mut v = r.map_err(invalid)?;
        for k in ["conditions", "outcome"] {
            v[k] = serde_json::from_str(v[k].as_str().unwrap()).map_err(invalid)?;
        }
        out.push(v);
    }
    Ok(json!(out))
}
pub fn change(
    tx: &Transaction<'_>,
    action: &str,
    input: &Input,
    actor: &str,
    today: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm {
        return Err(refused("confirmation required"));
    }
    let company: i64 = tx
        .query_row("SELECT id FROM companies ORDER BY id LIMIT 1", [], |r| {
            r.get(0)
        })
        .map_err(invalid)?;
    let at: String = tx
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
            r.get(0)
        })
        .map_err(invalid)?;
    let (id, expected, rationale, provenance, state) = if action == "propose" {
        let p = input
            .proposal
            .as_ref()
            .ok_or_else(|| refused("proposal required"))?;
        text(&p.rule_id, 128)?;
        text(&p.provenance, 256)?;
        text(&p.rationale, 2000)?;
        if let Some(attributes) = &p.conditions.attributes {
            text(&attributes.bank_text, 512)?;
        }
        if p.conditions.company.is_some_and(|id| id != company) {
            return Err(refused("condition company must equal selected ledger"));
        }
        for v in [&p.conditions.supplier_identity, &p.conditions.supplier_vat]
            .into_iter()
            .flatten()
        {
            text(v, 256)?;
        }
        if p.conditions
            .document_type
            .as_deref()
            .is_some_and(|s| !["purchase_sale", "cash_register_receipt"].contains(&s))
            || p.conditions
                .supplier_country
                .as_deref()
                .is_some_and(|s| s != "DK")
            || p.conditions
                .vat
                .as_deref()
                .is_some_and(|s| !["zero", "positive"].contains(&s))
        {
            return Err(refused(
                "unsupported purchase rule condition; explicit DKK domestic purchase review required",
            ));
        }
        if p.outcome
            .vat_treatment
            .as_deref()
            .is_some_and(|s| !["standard", "exempt", "non_deductible"].contains(&s))
        {
            return Err(refused("unsupported purchase VAT treatment"));
        }
        if let Some(dimensions) = &p.outcome.dimensions {
            if dimensions.len() > 12 {
                return Err(refused("bounded dimensions required"));
            }
            for (dimension, member) in dimensions {
                text(dimension, 128)?;
                text(member, 256)?;
            }
        }
        Date::parse(&p.effective_from)?;
        if let Some(to) = &p.effective_to {
            Date::parse(to)?;
            if to < &p.effective_from {
                return Err(refused("ordered dates required"));
            }
        }
        if p.conditions.currency != "DKK" {
            return Err(refused("native bank rule hints currently support DKK only"));
        }
        let valid: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND active=1)",
                [&p.outcome.account],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        if p.outcome.vat_treatment.is_some() && !tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND type='expense' AND active=1)", [&p.outcome.account], |r| r.get::<_,bool>(0)).map_err(invalid)? {
            return Err(refused("purchase rule requires an active expense account"));
        }
        if !valid {
            return Err(refused("active account required"));
        }
        let mut conditions = serde_json::to_value(&p.conditions).map_err(invalid)?;
        conditions["company"] = json!(company);
        let payload = json!({"ruleId":p.rule_id,"companyId":company,"effectiveFrom":p.effective_from,"effectiveTo":p.effective_to,"conditions":conditions,"outcome":p.outcome,"provenance":p.provenance,"rationale":p.rationale});
        let expected = crate::idempotency::canonical_payload_hash(&payload)?;
        let version:i64=tx.query_row("SELECT coalesce(max(version),0)+1 FROM posting_rule_versions WHERE company_id=?1 AND rule_id=?2",params![company,p.rule_id],|r|r.get(0)).map_err(invalid)?;
        let id:i64=tx.query_row("INSERT INTO posting_rule_versions(rule_id,company_id,version,effective_from,effective_to,conditions_json,outcome_json,provenance,rationale,created_by,payload_hash,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) RETURNING id",params![p.rule_id,company,version,p.effective_from,p.effective_to,conditions.to_string(),serde_json::to_string(&p.outcome).map_err(invalid)?,p.provenance,p.rationale,actor,expected,at],|r|r.get(0)).map_err(invalid)?;
        (
            id,
            expected,
            p.rationale.clone(),
            p.provenance.clone(),
            "proposed",
        )
    } else {
        let rule = input
            .rule_id
            .as_deref()
            .ok_or_else(|| refused("rule id required"))?;
        let version = input.version.ok_or_else(|| refused("version required"))?;
        let (id,expected,creator):(i64,String,String)=tx.query_row("SELECT id,payload_hash,created_by FROM posting_rule_versions WHERE company_id=?1 AND rule_id=?2 AND version=?3",params![company,rule,version],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(invalid)?;
        if Some(&expected) != input.expected_payload_hash.as_ref() {
            return Err(refused("exact rule payload required"));
        }
        let rationale = input.rationale.as_deref().unwrap_or("");
        let provenance = input.provenance.as_deref().unwrap_or("");
        text(rationale, 2000)?;
        text(provenance, 256)?;
        let current:String=tx.query_row("SELECT state FROM posting_rule_lifecycle_events WHERE rule_version_id=?1 ORDER BY effective_at DESC,id DESC LIMIT 1",[id],|r|r.get(0)).map_err(invalid)?;
        let state = match action {
            "approve" if current == "proposed" && creator != actor => "approved",
            "disable" if current == "approved" => "disabled",
            _ => {
                return Err(refused(
                    "independent reviewer and valid rule lifecycle required",
                ));
            }
        };
        (id, expected, rationale.into(), provenance.into(), state)
    };
    let _ = today;
    tx.execute("INSERT INTO posting_rule_lifecycle_events(rule_version_id,state,effective_at,actor,approver,rationale,provenance,expected_payload_hash,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?3)",params![id,state,at,actor,if state=="approved"{Some(actor)}else{None},rationale,provenance,expected]).map_err(invalid)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('posting_rule_transition','posting_rule',?1,?2,?3)",params![id.to_string(),state,actor]).map_err(invalid)?;
    Ok(json!({"ok":true,"payloadHash":expected,"state":state}))
}
/// Exact text/currency hints only. Unsupported legacy conditions remain a human decision.
pub fn hint(
    tx: &Transaction<'_>,
    bank_text: &str,
    currency: &str,
    date: &str,
) -> Result<Option<String>, Error> {
    let rules = list(tx)?;
    Ok(hint_from(&rules, bank_text, currency, date))
}
pub fn hint_from(rules: &Value, bank_text: &str, currency: &str, date: &str) -> Option<String> {
    let mut matches = vec![];
    for r in rules.as_array().unwrap() {
        let c = &r["conditions"];
        if r["state"] != "approved"
            || r["effectiveFrom"].as_str().unwrap_or("9999") > date
            || r["effectiveTo"].as_str().is_some_and(|to| to < date)
        {
            continue;
        }
        if c.as_object().is_some_and(|o| {
            o.keys()
                .all(|k| ["company", "currency", "attributes"].contains(&k.as_str()))
        }) && c["currency"] == currency
            && c["attributes"] == json!({"bankText":bank_text})
            && r["outcome"].as_object().is_some_and(|o| o.len() == 1)
            && let Some(account) = r["outcome"]["account"].as_str()
        {
            matches.push(account.to_owned());
        }
    }
    if matches.len() == 1 {
        matches.pop()
    } else {
        None
    }
}
