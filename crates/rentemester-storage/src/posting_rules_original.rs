//! Original general rules over the native immutable version/lifecycle tables.
//! This read-only evaluator is not purchase posting authority.
use crate::{accounting_drafts::refused, invalid};
use rentemester_engine::{Error, fiscal_year::Date, money::Money};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};

#[derive(Clone, Copy)]
pub enum Operation {
    Propose,
    Approve,
    Disable,
    Supersede,
    Explain,
    Test,
}

fn object<'a>(v: &'a Value, allowed: &[&str]) -> Result<&'a serde_json::Map<String, Value>, Error> {
    let o = v.as_object().ok_or_else(|| refused("object required"))?;
    if o.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(refused("unknown posting rule field"));
    }
    Ok(o)
}
fn text(v: &Value, max: usize) -> Result<&str, Error> {
    v.as_str()
        .filter(|s| {
            !s.is_empty() && *s == s.trim() && s.len() <= max && !s.chars().any(char::is_control)
        })
        .ok_or_else(|| refused("trimmed bounded text required"))
}
fn required<'a>(v: &'a Value, k: &str, max: usize) -> Result<&'a str, Error> {
    text(&v[k], max)
}
fn iso(s: &str) -> Result<String, Error> {
    if s.len() == 10 {
        Date::parse(s)?;
        return Ok(format!("{s}T00:00:00.000Z"));
    }
    let canonical = if s.len() == 20 && s.is_ascii() && s.ends_with('Z') {
        format!("{}.000Z", &s[..19])
    } else {
        s.to_owned()
    };
    crate::cookie_session::timestamp(&canonical)
        .map_err(|_| refused("real ISO date or UTC timestamp required"))?;
    Ok(canonical)
}
fn date(v: &Value) -> Result<String, Error> {
    let s = text(v, 24)?;
    let normalized = iso(s)?;
    Ok(if s.len() == 10 { s.into() } else { normalized })
}
fn cents(v: &Value) -> Result<Money, Error> {
    if !v.is_number() {
        return Err(refused("finite exact numeric money required"));
    }
    let raw = v.to_string();
    let (mantissa, exponent) = raw
        .split_once(['e', 'E'])
        .map_or(Ok((raw.as_str(), 0i32)), |(m, e)| {
            e.parse::<i32>().map(|e| (m, e))
        })
        .map_err(|_| refused("invalid money exponent"))?;
    let negative = mantissa.starts_with('-');
    let m = mantissa.trim_start_matches('-');
    let (whole, fraction) = m.split_once('.').unwrap_or((m, ""));
    let mut digits = format!("{whole}{fraction}");
    let shift = 2 + exponent - fraction.len() as i32;
    if shift.abs() > 400 || digits.len() > 400 {
        return Err(refused("money exceeds native input bound"));
    }
    if shift < 0 {
        let remove = (-shift) as usize;
        if remove > digits.len() || !digits[digits.len() - remove..].bytes().all(|b| b == b'0') {
            return Err(refused("exact native money cents required; no rounding"));
        }
        digits.truncate(digits.len() - remove);
    } else {
        digits.extend(std::iter::repeat_n('0', shift as usize));
    }
    while digits.len() < 3 {
        digits.insert(0, '0');
    }
    let split = digits.len() - 2;
    let w = digits[..split].trim_start_matches('0');
    let canonical = format!(
        "{}{}.{}",
        if negative && digits.bytes().any(|b| b != b'0') {
            "-"
        } else {
            ""
        },
        if w.is_empty() { "0" } else { w },
        &digits[split..]
    );
    Money::parse_canonical(&canonical).map_err(|_| refused("exact native money cents required"))
}
fn greater(a: &Money, b: &Money) -> bool {
    b.subtract(a).is_negative()
}
fn flat(v: &Value, attributes: bool) -> Result<(), Error> {
    let o = v
        .as_object()
        .ok_or_else(|| refused("flat string map required"))?;
    if o.len() > 12 {
        return Err(refused("at most twelve map entries required"));
    }
    for (k, v) in o {
        text(&json!(k), if attributes { 64 } else { 128 })?;
        if attributes
            && (!k.as_bytes()[0].is_ascii_alphabetic()
                || !k
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)))
        {
            return Err(refused("invalid attribute key"));
        }
        text(v, 256)?;
    }
    Ok(())
}
fn conditions(v: &Value, stored: bool) -> Result<(), Error> {
    let mut keys = vec![
        "supplierIdentity",
        "documentType",
        "supplierCountry",
        "supplierVat",
        "currency",
        "vat",
        "amount",
        "reverseChargeWording",
        "attributes",
    ];
    if stored {
        keys.push("company");
    }
    let o = object(v, &keys)?;
    for k in [
        "supplierIdentity",
        "documentType",
        "supplierCountry",
        "supplierVat",
    ] {
        if let Some(v) = o.get(k) {
            text(v, 256)?;
        }
    }
    if let Some(v) = o.get("currency") {
        let s = text(v, 3)?;
        if s.len() != 3 || !s.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(refused("canonical three-letter currency required"));
        }
    }
    if let Some(v) = o.get("vat") {
        if !["zero", "positive"].contains(&text(v, 8)?) {
            return Err(refused("VAT must be zero or positive"));
        }
    }
    if let Some(v) = o.get("reverseChargeWording") {
        if !v.is_boolean() {
            return Err(refused("boolean required"));
        }
    }
    if let Some(v) = o.get("attributes") {
        flat(v, true)?;
    }
    if let Some(v) = o.get("amount") {
        let range = object(v, &["min", "max"])?;
        for v in range.values() {
            cents(v)?;
        }
        if let (Some(a), Some(b)) = (range.get("min"), range.get("max")) {
            if greater(&cents(a)?, &cents(b)?) {
                return Err(refused("ordered amount bounds required"));
            }
        }
    }
    Ok(())
}
fn outcome(v: &Value) -> Result<(), Error> {
    let o = object(
        v,
        &["account", "vatTreatment", "textTemplate", "dimensions"],
    )?;
    for (k, max) in [
        ("account", 128),
        ("vatTreatment", 256),
        ("textTemplate", 2000),
    ] {
        if let Some(v) = o.get(k) {
            text(v, max)?;
        }
    }
    if let Some(v) = o.get("dimensions") {
        flat(v, false)?;
    }
    Ok(())
}
fn eligibility(tx: &Transaction<'_>, o: &Value, day: &str) -> Result<(), Error> {
    if let Some(a) = o.get("account") {
        let active: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND active=1)",
                [a.as_str().unwrap()],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        if !active {
            return Err(refused("active account required"));
        }
    }
    if let Some(d) = o.get("dimensions") {
        for (dimension, member) in d.as_object().unwrap() {
            let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM current_accounting_dimension_definitions d JOIN current_accounting_dimension_members m ON m.dimension_id=d.dimension_id WHERE d.dimension_id=?1 AND m.member_id=?2 AND m.status='active' AND (m.effective_from IS NULL OR m.effective_from<=?3) AND (m.effective_to IS NULL OR m.effective_to>=?3))",params![dimension,member.as_str().unwrap(),day],|r|r.get(0)).map_err(invalid)?;
            if !valid {
                return Err(refused(
                    "current active dimension member required on effective day",
                ));
            }
        }
    }
    Ok(())
}
fn company(tx: &Transaction<'_>) -> Result<i64, Error> {
    let (n, id): (i64, Option<i64>) = tx
        .query_row("SELECT count(*),min(id) FROM companies", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(invalid)?;
    if n != 1 {
        return Err(refused("exactly one selected company required"));
    }
    Ok(id.unwrap())
}
fn append(
    tx: &Transaction<'_>,
    id: i64,
    state: &str,
    hash: &str,
    actor: &str,
    reason: &str,
    provenance: &str,
    at: &str,
    now: &str,
) -> Result<(), Error> {
    tx.execute("INSERT INTO posting_rule_lifecycle_events(rule_version_id,state,effective_at,actor,approver,rationale,provenance,expected_payload_hash,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id,state,at,actor,if state=="approved" {Some(actor)} else {None},reason,provenance,hash,now]).map_err(invalid)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at) VALUES('posting_rule_transition','posting_rule',?1,?2,?3,?4)",params![id.to_string(),state,actor,now]).map_err(invalid)?;
    Ok(())
}
/// All business refusals use the source mutation envelope; reads on success stay flat.
pub fn execute(
    tx: &Transaction<'_>,
    operation: Operation,
    input: &Value,
    actor: &str,
    now: &str,
) -> Result<Value, Error> {
    tx.execute_batch("SAVEPOINT original_posting_rule_operation")
        .map_err(invalid)?;
    match run(tx, operation, input, actor, now) {
        Ok(v) => {
            tx.execute_batch("RELEASE original_posting_rule_operation")
                .map_err(invalid)?;
            Ok(v)
        }
        Err(e) => {
            tx.execute_batch("ROLLBACK TO original_posting_rule_operation; RELEASE original_posting_rule_operation").map_err(invalid)?;
            Ok(json!({"ok":false,"errors":[e.message]}))
        }
    }
}
fn run(
    tx: &Transaction<'_>,
    op: Operation,
    input: &Value,
    actor: &str,
    now: &str,
) -> Result<Value, Error> {
    crate::validate(tx)?;
    let now = iso(now)?;
    text(&json!(actor), 256)?;
    if !["user:", "service_account:"]
        .iter()
        .any(|p| actor.strip_prefix(p).is_some_and(|s| !s.is_empty()))
    {
        return Err(refused("canonical authenticated actor required"));
    }
    let company = company(tx)?;
    // Validate immutable hash and complete lifecycle, not merely a status label.
    crate::purchase_rules::validate_backup(tx)?;
    if matches!(op, Operation::Explain | Operation::Test) {
        return evaluate(tx, input, company, &now);
    }
    crate::journal_post::require_current(tx)?;
    if input["confirm"] != true {
        return Err(refused("explicit confirmation required"));
    }
    if matches!(op, Operation::Propose) {
        object(input, &["proposal", "confirm"])?;
        let p = &input["proposal"];
        object(
            p,
            &[
                "ruleId",
                "effectiveFrom",
                "effectiveTo",
                "conditions",
                "outcome",
                "provenance",
                "rationale",
            ],
        )?;
        let rule = required(p, "ruleId", 128)?;
        let provenance = required(p, "provenance", 256)?;
        let reason = required(p, "rationale", 2000)?;
        let from = date(&p["effectiveFrom"])?;
        let to = p.get("effectiveTo").map(date).transpose()?;
        if to
            .as_ref()
            .is_some_and(|t| iso(t).unwrap() < iso(&from).unwrap())
        {
            return Err(refused("ordered effective dates required"));
        }
        conditions(&p["conditions"], false)?;
        outcome(&p["outcome"])?;
        eligibility(tx, &p["outcome"], &from[..10])?;
        let mut c = p["conditions"].clone();
        c["company"] = json!(company);
        let payload = json!({"ruleId":rule,"companyId":company,"effectiveFrom":from,"effectiveTo":to,"conditions":c,"outcome":p["outcome"],"provenance":provenance,"rationale":reason});
        let hash = crate::idempotency::canonical_payload_hash(&payload)?;
        let duplicate:Option<i64>=tx.query_row("SELECT version FROM posting_rule_versions WHERE company_id=?1 AND rule_id=?2 AND payload_hash=?3",params![company,rule,hash],|r|r.get(0)).optional().map_err(invalid)?;
        if let Some(version) = duplicate {
            return Ok(
                json!({"ok":true,"duplicate":true,"version":version,"payloadHash":hash,"errors":[]}),
            );
        }
        let version:i64=tx.query_row("SELECT coalesce(max(version),0)+1 FROM posting_rule_versions WHERE company_id=?1 AND rule_id=?2",params![company,rule],|r|r.get(0)).map_err(invalid)?;
        let id:i64=tx.query_row("INSERT INTO posting_rule_versions(rule_id,company_id,version,effective_from,effective_to,conditions_json,outcome_json,provenance,rationale,created_by,payload_hash,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) RETURNING id",params![rule,company,version,from,to,c.to_string(),p["outcome"].to_string(),provenance,reason,actor,hash,now],|r|r.get(0)).map_err(invalid)?;
        append(
            tx, id, "proposed", &hash, actor, reason, provenance, &now, &now,
        )?;
        return Ok(
            json!({"ok":true,"duplicate":false,"version":version,"payloadHash":hash,"errors":[]}),
        );
    }
    object(
        input,
        &[
            "ruleId",
            "version",
            "expectedPayloadHash",
            "rationale",
            "provenance",
            "effectiveAt",
            "confirm",
        ],
    )?;
    let rule = required(input, "ruleId", 128)?;
    let reason = required(input, "rationale", 2000)?;
    let provenance = required(input, "provenance", 256)?;
    let version = input["version"]
        .as_i64()
        .filter(|n| *n > 0)
        .ok_or_else(|| refused("positive version required"))?;
    let expected = required(input, "expectedPayloadHash", 64)?;
    let row:Option<(i64,String,String)>=tx.query_row("SELECT id,payload_hash,created_by FROM posting_rule_versions WHERE company_id=?1 AND rule_id=?2 AND version=?3",params![company,rule,version],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(invalid)?;
    let (id, hash, creator) = row.ok_or_else(|| refused("rule version not in selected company"))?;
    if expected != hash {
        return Err(refused("exact existing payload hash required"));
    }
    let effective = input
        .get("effectiveAt")
        .map(date)
        .transpose()?
        .unwrap_or_else(|| now.clone());
    let effective = iso(&effective)?;
    let (state,last):(String,String)=tx.query_row("SELECT state,effective_at FROM posting_rule_lifecycle_events WHERE rule_version_id=?1 ORDER BY effective_at DESC,id DESC LIMIT 1",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(invalid)?;
    // Appending only at/after the complete retained history prevents backdated insertions
    // from invalidating a later transition (including scheduled future transitions).
    if effective < iso(&last)? {
        return Err(refused(
            "transition cannot precede retained lifecycle history",
        ));
    }
    let next = match op {
        Operation::Approve if state == "proposed" && creator != actor => "approved",
        Operation::Disable if state == "approved" => "disabled",
        Operation::Supersede if state == "approved" => "superseded",
        _ => return Err(refused("independent approval and valid lifecycle required")),
    };
    append(
        tx, id, next, &hash, actor, reason, provenance, &effective, &now,
    )?;
    Ok(json!({"ok":true,"errors":[]}))
}
fn evaluate(tx: &Transaction<'_>, input: &Value, company: i64, now: &str) -> Result<Value, Error> {
    object(input, &["context", "at"])?;
    let x = &input["context"];
    let flags = [
        "changedBuyer",
        "changedSupplier",
        "changedVat",
        "changedCurrency",
        "unusualAmount",
        "contradictoryEvidence",
    ];
    let mut keys = vec![
        "supplierIdentity",
        "documentType",
        "supplierCountry",
        "supplierVat",
        "currency",
        "vatAmount",
        "amount",
        "reverseChargeWording",
        "attributes",
        "documentId",
    ];
    keys.extend(flags);
    let o = object(x, &keys)?;
    // Reuse exact condition validation for the common factual fields.
    let mut common = x.clone();
    for k in flags
        .into_iter()
        .chain(["documentId", "vatAmount", "amount"])
    {
        common.as_object_mut().unwrap().remove(k);
    }
    conditions(&common, false)?;
    for k in ["vatAmount", "amount"] {
        if let Some(v) = o.get(k) {
            cents(v)?;
        }
    }
    let mut reasons = vec![];
    for flag in flags {
        if let Some(v) = o.get(flag) {
            if !v.is_boolean() {
                return Err(refused("evidence flag must be boolean"));
            }
            if v == true {
                reasons.push(format!("{flag} requires human decision"));
            }
        }
    }
    if let Some(v) = o.get("documentId") {
        let id = v
            .as_i64()
            .filter(|n| *n > 0)
            .ok_or_else(|| refused("positive existing document ID required"))?;
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
                [id],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        if !exists {
            return Err(refused("document not in selected ledger"));
        }
        if let Err(e) = crate::invoice_extraction::require_completed_or_absent(tx, id) {
            if e.code != "EXTRACTION_NEEDS_RESOLUTION" {
                return Err(e);
            }
            reasons.push("unresolved extraction requires human decision".into());
        }
    }
    let at = input
        .get("at")
        .map(date)
        .transpose()?
        .unwrap_or_else(|| now.into());
    let cutoff = if at.len() == 10 {
        format!("{at}T23:59:59.999Z")
    } else {
        iso(&at)?
    };
    let mut explanations = vec![];
    let mut matched = vec![];
    let mut selected = None;
    let mut q=tx.prepare("SELECT id,rule_id,version,payload_hash,effective_from,effective_to,conditions_json,outcome_json FROM posting_rule_versions WHERE company_id=?1 ORDER BY rule_id,version").map_err(invalid)?;
    let mut rows = q.query([company]).map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        let id: i64 = r.get(0).map_err(invalid)?;
        let rule: String = r.get(1).map_err(invalid)?;
        let version: i64 = r.get(2).map_err(invalid)?;
        let hash: String = r.get(3).map_err(invalid)?;
        let from: String = r.get(4).map_err(invalid)?;
        let to: Option<String> = r.get(5).map_err(invalid)?;
        if iso(&from)? > cutoff
            || to
                .as_ref()
                .map(|t| {
                    if t.len() == 10 {
                        Ok(format!("{t}T23:59:59.999Z"))
                    } else {
                        iso(t)
                    }
                })
                .transpose()?
                .is_some_and(|t| t < cutoff)
        {
            continue;
        }
        let c: Value =
            serde_json::from_str(&r.get::<_, String>(6).map_err(invalid)?).map_err(invalid)?;
        let out: Value =
            serde_json::from_str(&r.get::<_, String>(7).map_err(invalid)?).map_err(invalid)?;
        conditions(&c, true)?;
        outcome(&out)?;
        let state:Option<String>=tx.query_row("SELECT state FROM posting_rule_lifecycle_events WHERE rule_version_id=?1 AND (CASE WHEN length(effective_at)=10 THEN effective_at||'T00:00:00.000Z' ELSE effective_at END)<=?2 ORDER BY effective_at DESC,id DESC LIMIT 1",params![id,cutoff],|r|r.get(0)).optional().map_err(invalid)?;
        let mut mismatch = vec![];
        if state.as_deref() != Some("approved") {
            mismatch.push(format!(
                "state is {}",
                state.as_deref().unwrap_or("not yet effective")
            ));
        } else {
            for k in [
                "supplierIdentity",
                "documentType",
                "supplierCountry",
                "supplierVat",
                "currency",
                "reverseChargeWording",
            ] {
                if c.get(k).is_some_and(|v| x.get(k) != Some(v)) {
                    mismatch.push(format!("{k} did not match exactly"));
                }
            }
            if let Some(v) = c.get("vat") {
                let valid = match x.get("vatAmount").map(cents).transpose()? {
                    Some(a) => {
                        if v == "zero" {
                            a == Money::default()
                        } else {
                            !a.is_negative() && a != Money::default()
                        }
                    }
                    None => false,
                };
                if !valid {
                    mismatch.push("VAT condition did not match".into());
                }
            }
            if let Some(range) = c.get("amount") {
                let valid = if let Some(v) = x.get("amount") {
                    let a = cents(v)?;
                    !range
                        .get("min")
                        .map(|v| cents(v).map(|b| greater(&b, &a)))
                        .transpose()?
                        .unwrap_or(false)
                        && !range
                            .get("max")
                            .map(|v| cents(v).map(|b| greater(&a, &b)))
                            .transpose()?
                            .unwrap_or(false)
                } else {
                    false
                };
                if !valid {
                    mismatch.push("amount range did not match".into());
                }
            }
            if let Some(attrs) = c.get("attributes") {
                for (k, v) in attrs.as_object().unwrap() {
                    if x["attributes"].get(k) != Some(v) {
                        mismatch.push(format!("attribute {k} did not match exactly"));
                    }
                }
            }
        }
        let yes = mismatch.is_empty();
        let explanation = json!({"ruleId":rule,"version":version,"payloadHash":hash,"state":state,"matched":yes,"reasons":mismatch});
        if yes {
            matched.push(explanation.clone());
            selected = Some((id, hash, out));
        }
        explanations.push(explanation);
    }
    if matched.len() != 1 {
        reasons.push(
            if matched.is_empty() {
                "no approved rule matched"
            } else {
                "multiple approved rules matched"
            }
            .into(),
        );
    }
    if reasons.is_empty() {
        let (id, hash, out) = selected.unwrap();
        Ok(
            json!({"decision":"proposed","outcome":out,"matched":matched,"explanations":explanations,"reasons":[],"ruleVersionId":id,"payloadHash":hash,"dryRun":true}),
        )
    } else {
        Ok(
            json!({"decision":"human_decision","matched":matched,"explanations":explanations,"reasons":reasons,"dryRun":true}),
        )
    }
}
#[cfg(test)]
mod tests;
