//! Reviewed purchase-rule proofs. No VAT calculation, journal write or implied fallback.
use crate::{
    accounting_drafts, invalid,
    posting_rules::{Conditions, Outcome},
};
use rentemester_engine::{Error, document, fiscal_year::Date, money::Money};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::{Value, json};
fn refused(message: &str) -> Error {
    Error::new("purchase_rule_refused", message)
}
fn hash(value: &Value) -> Result<String, Error> {
    crate::idempotency::canonical_payload_hash(value)
        .map_err(|_| refused("purchase rule proof exceeds canonical bounds or numeric domain"))
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
fn at(as_of: &str) -> Result<(String, String), Error> {
    if as_of.len() == 10 {
        Date::parse(as_of)?;
        Ok((as_of.into(), format!("{as_of}T23:59:59.999Z")))
    } else {
        crate::cookie_session::timestamp(as_of)?;
        Ok((as_of[..10].into(), as_of.into()))
    }
}
struct Rule {
    id: i64,
    rule: String,
    version: i64,
    from: String,
    to: Option<String>,
    conditions: String,
    outcome: String,
    provenance: String,
    rationale: String,
    creator: String,
    payload_hash: String,
}
fn rules(tx: &Connection, company: i64) -> Result<Vec<Rule>, Error> {
    let mut q=tx.prepare("SELECT id,rule_id,version,effective_from,effective_to,conditions_json,outcome_json,provenance,rationale,created_by,payload_hash FROM posting_rule_versions WHERE company_id=?1 ORDER BY rule_id,version,id LIMIT 1001").map_err(invalid)?;
    let rows = q
        .query_map([company], |r| {
            Ok(Rule {
                id: r.get(0)?,
                rule: r.get(1)?,
                version: r.get(2)?,
                from: r.get(3)?,
                to: r.get(4)?,
                conditions: r.get(5)?,
                outcome: r.get(6)?,
                provenance: r.get(7)?,
                rationale: r.get(8)?,
                creator: r.get(9)?,
                payload_hash: r.get(10)?,
            })
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if rows.len() > 1000 {
        return Err(refused("rule universe exceeds native purchase proof bound"));
    }
    Ok(rows)
}
fn approved(tx: &Connection, r: &Rule, cutoff: &str) -> Result<bool, Error> {
    let mut q=tx.prepare("SELECT state,actor,approver,expected_payload_hash,effective_at FROM posting_rule_lifecycle_events WHERE rule_version_id=?1 AND effective_at<=?2 ORDER BY effective_at,id").map_err(invalid)?;
    let mut rows = q.query(params![r.id, cutoff]).map_err(invalid)?;
    let mut state = None;
    while let Some(row) = rows.next().map_err(invalid)? {
        let next: String = row.get(0).map_err(invalid)?;
        let actor: String = row.get(1).map_err(invalid)?;
        let approver: Option<String> = row.get(2).map_err(invalid)?;
        let payload: String = row.get(3).map_err(invalid)?;
        let time: String = row.get(4).map_err(invalid)?;
        at(&time)?;
        if payload != r.payload_hash
            || !text(&actor, 256)
            || !match (state.as_deref(), next.as_str()) {
                (None, "proposed") => actor == r.creator,
                (Some("proposed"), "approved") => {
                    actor != r.creator && approver.as_deref() == Some(actor.as_str())
                }
                (Some("proposed"), "rejected") => true,
                (Some("approved"), "disabled" | "superseded") => true,
                _ => false,
            }
        {
            return Err(refused("invalid reviewed rule lifecycle or payload proof"));
        }
        state = Some(next);
    }
    Ok(state.as_deref() == Some("approved"))
}
fn outcome(tx: &Transaction<'_>, o: &Outcome, date: &str) -> Result<Value, Error> {
    let treatment=o.vat_treatment.as_deref().filter(|s|["standard","exempt","non_deductible","representation","reverse_charge","eu_goods_acquisition"].contains(s)).ok_or_else(||refused("rule has no supported explicit purchase VAT outcome; bank hints are not purchase authority"))?;
    let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND type='expense' AND active=1)",[&o.account],|r|r.get(0)).map_err(invalid)?;
    if !active {
        return Err(refused("rule expense account is unavailable or inactive"));
    }
    let dims = o.dimensions.clone().unwrap_or_default();
    if dims.len() > 12 {
        return Err(refused("unsupported dimension count"));
    }
    for (dimension, member) in &dims {
        let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM current_accounting_dimension_definitions d JOIN current_accounting_dimension_members m ON m.dimension_id=d.dimension_id WHERE d.dimension_id=?1 AND m.member_id=?2 AND m.status='active' AND (m.effective_from IS NULL OR m.effective_from<=?3) AND (m.effective_to IS NULL OR m.effective_to>=?3))",params![dimension,member,date],|r|r.get(0)).map_err(invalid)?;
        if !valid {
            return Err(refused(
                "rule dimension member is not currently active for this date",
            ));
        }
    }
    Ok(json!({"account":o.account,"vatTreatment":treatment,"dimensions":dims}))
}
/// A pure projection over the held company transaction. Physical original admission belongs to caller.
pub fn inspect(
    tx: &Transaction<'_>,
    document_id: i64,
    bank_transaction_id: i64,
    as_of: &str,
) -> Result<Value, Error> {
    crate::validate(tx)?;
    if document_id <= 0 || bank_transaction_id <= 0 {
        return Err(refused("positive document and bank IDs required"));
    }
    let (day, cutoff) = at(as_of)?;
    let companies: Vec<i64> = tx
        .prepare("SELECT id FROM companies ORDER BY id")
        .map_err(invalid)?
        .query_map([], |r| r.get(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let company = match companies.as_slice() {
        [id] => *id,
        _ => return Err(refused("selected ledger must contain exactly one company")),
    };
    let (payload,kind,currency,name,country,vat):(Option<String>,String,String,Option<String>,Option<String>,Option<String>)=tx.query_row("SELECT payload_json,document_type,currency,sender_name,supplier_country_code,sender_vat_cvr FROM documents WHERE id=?1",[document_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(invalid)?;
    let (bank_text, bank_currency): (String, String) = tx
        .query_row(
            "SELECT text,currency FROM bank_transactions WHERE id=?1",
            [bank_transaction_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(invalid)?;
    let source_hash = accounting_drafts::evidence(
        tx,
        &json!({"documentId":document_id,"sourceBankTransactionId":bank_transaction_id}),
    )?;
    let mut reasons = vec![];
    let mut explanations = vec![];
    let mut matches = vec![];
    let metadata = payload
        .as_deref()
        .and_then(|p| serde_json::from_str::<document::Metadata>(p).ok());
    let mut vat_amount = None;
    match metadata {
        Some(ref m) if m.validate().is_ok() => {
            vat_amount = m.vat_amount.as_ref().map(document::money).transpose()?;
            if m.kind() != kind || m.currency() != currency {
                reasons.push(
                    "document metadata and canonical classification/currency disagree".to_owned(),
                )
            }
        }
        _ => reasons.push("valid original purchase metadata is unavailable".into()),
    }
    if !["purchase_sale", "cash_register_receipt"].contains(&kind.as_str())
        || rentemester_engine::currency::Currency::parse(&currency).is_err()
        || rentemester_engine::currency::Currency::parse(&bank_currency).is_err()
    {
        reasons.push(
            "explicit canonical currency and purchase invoice/receipt evidence required".into(),
        )
    }
    if let Err(error) = crate::invoice_extraction::require_completed_or_absent(tx, document_id) {
        if error.code != "EXTRACTION_NEEDS_RESOLUTION" {
            return Err(error);
        }
        reasons.push(error.message);
    }
    for r in rules(tx, company)? {
        if r.id <= 0 || r.version <= 0 {
            return Err(refused("invalid canonical rule identity"));
        }
        let mut mismatch = vec![];
        let parsed = (
            serde_json::from_str::<Conditions>(&r.conditions),
            serde_json::from_str::<Outcome>(&r.outcome),
        );
        let mut selected = None;
        if r.from.len() != 10
            || Date::parse(&r.from).is_err()
            || r.to
                .as_ref()
                .is_some_and(|to| to.len() != 10 || Date::parse(to).is_err() || to < &r.from)
        {
            mismatch.push("unsupported or invalid rule effective date".to_owned());
            reasons.push("rule effective date coverage is unsupported or invalid".to_owned());
        }
        if r.from > day || r.to.as_ref().is_some_and(|to| to < &day) {
            mismatch.push("rule is outside its effective dates".into())
        }
        if !approved(tx, &r, &cutoff)? {
            mismatch.push("rule is not independently approved at asOf".into())
        }
        let applicable = mismatch.is_empty();
        match parsed {
            (Ok(c), Ok(o)) => {
                let cv: Value = serde_json::from_str(&r.conditions).map_err(invalid)?;
                let ov: Value = serde_json::from_str(&r.outcome).map_err(invalid)?;
                let bound = json!({"ruleId":r.rule,"companyId":company,"effectiveFrom":r.from,"effectiveTo":r.to,"conditions":cv,"outcome":ov,"provenance":r.provenance,"rationale":r.rationale});
                if hash(&bound)? != r.payload_hash {
                    mismatch.push("rule canonical payload hash is unsupported or corrupt".into());
                    if applicable {
                        reasons.push(
                            "an approved rule has unsupported or corrupt canonical payload proof"
                                .into(),
                        );
                    }
                }
                if c.company != Some(company) || c.currency != currency {
                    mismatch.push(
                        "rule company/currency does not match canonical selected source".into(),
                    )
                }
                for (label, expected, actual) in [
                    (
                        "supplierIdentity",
                        c.supplier_identity.as_deref(),
                        name.as_deref(),
                    ),
                    (
                        "documentType",
                        c.document_type.as_deref(),
                        Some(kind.as_str()),
                    ),
                    (
                        "supplierCountry",
                        c.supplier_country.as_deref(),
                        country.as_deref(),
                    ),
                    ("supplierVat", c.supplier_vat.as_deref(), vat.as_deref()),
                ] {
                    if expected.is_some() && expected != actual {
                        mismatch.push(format!("{label} did not match exactly"))
                    }
                }
                if c.reverse_charge_wording.is_some()
                    && c.reverse_charge_wording
                        != metadata
                            .as_ref()
                            .and_then(|m| m.reverse_charge_wording_confirmed)
                {
                    mismatch.push(
                        "reverseChargeWording did not match immutable metadata exactly".into(),
                    );
                }
                if c.attributes
                    .as_ref()
                    .is_some_and(|a| a.bank_text != bank_text)
                {
                    mismatch.push("bankText did not match exactly".into())
                }
                if let Some(condition) = &c.vat {
                    if applicable && !["zero", "positive"].contains(&condition.as_str()) {
                        reasons.push("an approved rule has an unsupported VAT condition".into());
                    }
                    let valid = match (condition.as_str(), vat_amount.as_ref()) {
                        ("zero", Some(amount)) => *amount == Money::default(),
                        ("positive", Some(amount)) => {
                            !amount.is_negative() && *amount != Money::default()
                        }
                        _ => false,
                    };
                    if !valid {
                        mismatch.push("explicit VAT evidence did not match rule".into())
                    }
                }
                match outcome(tx, &o, &day) {
                    Ok(v) => selected = Some(v),
                    Err(e) => {
                        if mismatch.is_empty() && o.vat_treatment.is_some() {
                            reasons.push(
                                "an otherwise matching purchase rule has an unavailable outcome"
                                    .into(),
                            );
                        }
                        mismatch.push(e.message);
                    }
                }
            }
            _ => {
                if applicable {
                    reasons.push("an approved effective rule has unsupported conditions/outcomes; no fallback is allowed".into());
                }
                mismatch.push("unsupported rule conditions/outcomes require human review".into());
            }
        }
        let matched = mismatch.is_empty();
        explanations.push(json!({"ruleId":r.rule,"version":r.version,"ruleVersionId":r.id,"payloadHash":r.payload_hash,"matched":matched,"reasons":mismatch}));
        if matched {
            matches.push((
                r.id,
                r.payload_hash,
                selected.ok_or_else(|| refused("matched rule outcome unavailable"))?,
            ))
        }
    }
    if matches.len() != 1 {
        reasons.push(if matches.is_empty() {
            "no applicable independently approved purchase rule".into()
        } else {
            "multiple applicable independently approved purchase rules".into()
        })
    }
    let ready = reasons.is_empty();
    let selected = if ready { matches.pop() } else { None };
    let mut value = match selected {
        Some((id, payload, outcome)) => {
            json!({"ok":true,"decision":"proposed","ruleVersionId":id,"payloadHash":payload,"outcome":outcome})
        }
        None => {
            json!({"ok":false,"decision":"human_decision","ruleVersionId":null,"payloadHash":null,"outcome":null})
        }
    };
    value["explanation"] = json!({"reasons":reasons,"rules":explanations});
    value["source"] = json!({"documentId":document_id,"bankTransactionId":bank_transaction_id,"sourceHash":source_hash,"asOf":as_of});
    value["proofHash"] = json!(hash(&value)?);
    Ok(value)
}
/// Recheck the exact reviewed proof before writing one real immutable application inside caller tx.
#[allow(clippy::too_many_arguments)]
pub fn apply(
    tx: &Transaction<'_>,
    document_id: i64,
    bank_transaction_id: i64,
    as_of: &str,
    expected: &Value,
    application_key: &str,
    actor: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !text(application_key, 512) || !text(actor, 256) {
        return Err(refused(
            "bounded application key and authenticated actor required",
        ));
    }
    let current = inspect(tx, document_id, bank_transaction_id, as_of)?;
    if &current != expected || current["ok"] != true || current["decision"] != "proposed" {
        return Err(refused(
            "purchase rule proof changed or requires human decision",
        ));
    }
    let company: i64 = tx
        .query_row("SELECT id FROM companies", [], |r| r.get(0))
        .map_err(invalid)?;
    let existing:Option<(i64,i64,i64,i64,String,String,String)>=tx.query_row("SELECT id,company_id,document_id,rule_version_id,rule_payload_hash,decision,explanation_json FROM posting_rule_applications WHERE application_key=?1",[application_key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional().map_err(invalid)?;
    let (id, duplicate) = if let Some((id, c, d, r, h, decision, raw)) = existing {
        if c != company
            || d != document_id
            || Some(r) != current["ruleVersionId"].as_i64()
            || current["payloadHash"] != h
            || decision != "proposed"
            || serde_json::from_str::<Value>(&raw).map_err(invalid)? != current
        {
            return Err(refused(
                "application key already belongs to another exact rule/source proof",
            ));
        }
        (id, true)
    } else {
        let id=tx.query_row("INSERT INTO posting_rule_applications(application_key,company_id,document_id,rule_version_id,rule_payload_hash,decision,explanation_json,created_at) VALUES(?1,?2,?3,?4,?5,'proposed',?6,strftime('%Y-%m-%dT%H:%M:%fZ','now')) RETURNING id",params![application_key,company,document_id,current["ruleVersionId"].as_i64(),current["payloadHash"].as_str(),serde_json::to_string(&current).map_err(invalid)?],|r|r.get::<_,i64>(0)).map_err(invalid)?;
        tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('posting_rule_applied','posting_rule_application',?1,?2,?3)",params![id.to_string(),serde_json::to_string(&json!({"applicationKey":application_key,"proofHash":current["proofHash"]})).map_err(invalid)?,actor]).map_err(invalid)?;
        (id, false)
    };
    let mut result = current;
    result["postingRuleApplicationId"] = json!(id);
    result["duplicate"] = json!(duplicate);
    Ok(result)
}
/// Validate persisted evidence, never today's rule/account/dimension eligibility.
pub fn validate_backup(db: &Connection) -> Result<(), Error> {
    let companies = db
        .prepare("SELECT id FROM companies ORDER BY id")
        .map_err(invalid)?
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    for company in companies {
        let versions = rules(db, company)?;
        for r in &versions {
            let conditions: Value = serde_json::from_str(&r.conditions).map_err(invalid)?;
            let outcome: Value = serde_json::from_str(&r.outcome).map_err(invalid)?;
            let payload = json!({"ruleId":r.rule,"companyId":company,"effectiveFrom":r.from,"effectiveTo":r.to,"conditions":conditions,"outcome":outcome,"provenance":r.provenance,"rationale":r.rationale});
            at(&r.from)?;
            if let Some(to) = &r.to {
                at(to)?;
            }
            if r.id <= 0
                || r.version <= 0
                || conditions["company"] != company
                || r.to.as_ref().is_some_and(|to| to < &r.from)
                || !conditions.is_object()
                || !outcome.is_object()
                || hash(&payload)? != r.payload_hash
            {
                return Err(refused("stored rule canonical payload differs"));
            }
            // Validate every transition, including disabled/superseded histories.
            approved(db, r, "9999-12-31T23:59:59.999Z")?;
            let count: i64 = db
                .query_row(
                    "SELECT count(*) FROM posting_rule_lifecycle_events WHERE rule_version_id=?1",
                    [r.id],
                    |row| row.get(0),
                )
                .map_err(invalid)?;
            if count == 0 {
                return Err(refused("stored rule has no proposal history"));
            }
        }
        let mut q = db.prepare("SELECT id,document_id,rule_version_id,rule_payload_hash,decision,explanation_json,application_key,created_at FROM posting_rule_applications WHERE company_id=?1 ORDER BY id").map_err(invalid)?;
        let mut applications = q.query([company]).map_err(invalid)?;
        while let Some(row) = applications.next().map_err(invalid)? {
            let id: i64 = row.get(0).map_err(invalid)?;
            let doc: Option<i64> = row.get(1).map_err(invalid)?;
            let version: Option<i64> = row.get(2).map_err(invalid)?;
            let payload_hash: Option<String> = row.get(3).map_err(invalid)?;
            let decision: String = row.get(4).map_err(invalid)?;
            let raw: String = row.get(5).map_err(invalid)?;
            let key: String = row.get(6).map_err(invalid)?;
            let created: String = row.get(7).map_err(invalid)?;
            at(&created)?;
            let proof: Value = serde_json::from_str(&raw).map_err(invalid)?;
            if !text(&key, 512) || proof["decision"] != decision {
                return Err(refused("stored rule application identity differs"));
            }
            if let Some(doc) = doc {
                let exists: bool = db
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
                        [doc],
                        |r| r.get(0),
                    )
                    .map_err(invalid)?;
                if !exists {
                    return Err(refused("stored rule application document missing"));
                }
            }
            if decision == "proposed" {
                let r = versions
                    .iter()
                    .find(|r| Some(r.id) == version)
                    .ok_or_else(|| refused("stored rule application version missing"))?;
                if payload_hash.as_deref() != Some(r.payload_hash.as_str())
                    || proof["ruleVersionId"] != r.id
                    || proof["payloadHash"] != r.payload_hash
                {
                    return Err(refused("stored rule application payload differs"));
                }
                let original: Value = serde_json::from_str(&r.outcome).map_err(invalid)?;
                if proof.get("proofHash").is_some() || proof.get("source").is_some() {
                    let mut bound = proof.clone();
                    bound
                        .as_object_mut()
                        .ok_or_else(|| refused("stored purchase proof object required"))?
                        .remove("proofHash");
                    let (day, cutoff) = at(proof["source"]["asOf"]
                        .as_str()
                        .ok_or_else(|| refused("stored purchase proof date missing"))?)?;
                    let bank = proof["source"]["bankTransactionId"]
                        .as_i64()
                        .filter(|n| *n > 0)
                        .ok_or_else(|| refused("stored purchase proof bank missing"))?;
                    let bank_exists: bool = db
                        .query_row(
                            "SELECT EXISTS(SELECT 1 FROM bank_transactions WHERE id=?1)",
                            [bank],
                            |row| row.get(0),
                        )
                        .map_err(invalid)?;
                    let expected = json!({"account":original["account"],"vatTreatment":original["vatTreatment"],"dimensions":original.get("dimensions").cloned().unwrap_or_else(|| json!({}))});
                    // The native audit sequence disambiguates same-millisecond transitions.
                    // Later disable/supersession must not invalidate a historical application.
                    let audits: Vec<(i64,String)> = db.prepare("SELECT id,message FROM audit_log WHERE event_type='posting_rule_applied' AND entity_type='posting_rule_application' AND entity_id=?1 ORDER BY id").map_err(invalid)?
                        .query_map([id.to_string()], |row| Ok((row.get(0)?,row.get(1)?))).map_err(invalid)?
                        .collect::<Result<_,_>>().map_err(invalid)?;
                    let [(audit_id, message)] = audits.as_slice() else {
                        return Err(refused(
                            "stored purchase application audit missing or conflicting",
                        ));
                    };
                    let message: Value = serde_json::from_str(message).map_err(invalid)?;
                    let before: Option<String> = db.query_row("SELECT message FROM audit_log WHERE event_type='posting_rule_transition' AND entity_type='posting_rule' AND entity_id=?1 AND id<?2 ORDER BY id DESC LIMIT 1", params![r.id.to_string(),audit_id], |row| row.get(0)).optional().map_err(invalid)?;
                    let reviewed: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM posting_rule_lifecycle_events WHERE rule_version_id=?1 AND state='approved' AND effective_at<=?2 AND effective_at<=?3)", params![r.id,cutoff,created], |row| row.get(0)).map_err(invalid)?;
                    if before.as_deref() != Some("approved")
                        || message["applicationKey"] != key
                        || message["proofHash"] != proof["proofHash"]
                    {
                        return Err(refused("stored purchase application audit proof differs"));
                    }
                    if hash(&bound)? != proof["proofHash"]
                        || proof["ok"] != true
                        || doc != proof["source"]["documentId"].as_i64()
                        || !bank_exists
                        || !proof["source"]["sourceHash"].as_str().is_some_and(|s| {
                            s.len() == 64
                                && s.bytes()
                                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                        })
                        || proof["outcome"] != expected
                        || r.from > day
                        || r.to.as_ref().is_some_and(|to| to < &day)
                        || !reviewed
                    {
                        return Err(refused("stored purchase application proof differs"));
                    }
                } else if proof["outcome"] != original {
                    return Err(refused("stored legacy rule application outcome differs"));
                }
            } else if decision != "human_decision" || version.is_some() || payload_hash.is_some() {
                return Err(refused("stored rule application decision unsupported"));
            }
            // Native batch review freezes the complete inspect proof, not just its hash columns.
            let mut links = db.prepare("SELECT r.plan_json FROM bookkeeping_batch_applied_links l JOIN bookkeeping_batch_revisions r ON r.run_id=l.run_id WHERE l.posting_rule_application_id=?1").map_err(invalid)?;
            let plans = links
                .query_map([id], |row| row.get::<_, String>(0))
                .map_err(invalid)?;
            for plan in plans {
                let plan: Value = serde_json::from_str(&plan.map_err(invalid)?).map_err(invalid)?;
                if !plan["items"].as_array().is_some_and(|items| {
                    items.iter().any(|item| {
                        item["documentId"].as_i64() == doc && item["detail"]["rule"] == proof
                    })
                }) {
                    return Err(refused(
                        "stored rule application differs from reviewed batch",
                    ));
                }
            }
        }
    }
    Ok(())
}
#[cfg(test)]
pub(crate) mod tests;
