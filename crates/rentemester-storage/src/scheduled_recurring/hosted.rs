//! Typed V3 hosted delegation. These stored facts are historical intent, never live control authority.
use super::*;
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub service_account_id: String,
    pub credential_id: String,
    pub grantor_user_id: String,
    pub authority_hash: String,
    pub policy: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Proof {
    kind: String,
    principal_kind: String,
    pub binding: Binding,
    consent_hash: Option<String>,
    reason: Option<String>,
    reviewed_plan: Option<Value>,
}
#[derive(Clone)]
pub(super) struct Recorded {
    pub binding: Binding,
    pub consent_hash: String,
    pub active: bool,
}
fn valid(b: &Binding) -> bool {
    [&b.service_account_id, &b.credential_id, &b.grantor_user_id]
        .iter()
        .all(|v| !v.trim().is_empty() && v.len() <= 4096 && !v.chars().any(char::is_control))
        && engine::hash(&b.authority_hash)
        && b.policy == "normal_due_only"
}
pub(super) fn mode(s: &State) -> &'static str {
    if s.hosted.as_ref().is_some_and(|h| h.active) {
        "hosted"
    } else {
        "app"
    }
}
pub(super) fn descriptor(s: &State) -> Value {
    s.hosted.as_ref().map(|h|json!({"active":h.active,"serviceAccountId":h.binding.service_account_id,"credentialId":h.binding.credential_id,"grantorUserId":h.binding.grantor_user_id,"authorityHash":h.binding.authority_hash,"policy":h.binding.policy,"consentHash":h.consent_hash})).unwrap_or(Value::Null)
}
pub(super) fn actor(s: &State) -> Option<&str> {
    if let Some(h) = s.hosted.as_ref().filter(|h| h.active) {
        Some(&h.binding.service_account_id)
    } else {
        s.owner.as_deref()
    }
}
fn quiet(s: &State, p: &Value) -> bool {
    s.error.is_none()
        && s.queued.is_empty()
        && !s.restore.as_ref().is_some_and(|r| r.required)
        && p["missedCount"] == 0
        && p["truncated"] == false
        && p["queued"].as_array().is_some_and(Vec::is_empty)
}
fn review_hash(v: &Value) -> Result<(), Error> {
    let mut c = v.clone();
    let h = c
        .as_object_mut()
        .ok_or_else(refused)?
        .remove("planHash")
        .ok_or_else(refused)?;
    if h != sha(c.to_string().as_bytes()) {
        return Err(refused());
    }
    Ok(())
}
pub(super) fn validate_event(
    db: &Connection,
    id: i64,
    hash: &str,
    s: &State,
    e: &Event,
) -> Result<(), Error> {
    let p = &e.payload;
    if p.version != 3 {
        if mode(s) == "hosted" && matches!(p.kind.as_str(), "attempted" | "succeeded" | "failed") {
            return Err(refused());
        }
        return Ok(());
    }
    let h = p.hosted.as_ref().ok_or_else(refused)?;
    if !valid(&h.binding) {
        return Err(refused());
    }
    if p.kind == "enabled" {
        let review = h.reviewed_plan.as_ref().ok_or_else(refused)?;
        review_hash(review)?;
        let schedule = &review["schedule"];
        review_hash(schedule)?;
        if h.kind != "consent"
            || h.principal_kind != "user"
            || p.actor != h.binding.grantor_user_id
            || h.consent_hash.is_some()
            || h.reason
                .as_ref()
                .is_none_or(|r| engine::reason(r).ok() != Some(r.as_str()))
            || !matches!(s.state, "enabled" | "paused")
            || !quiet(s, schedule)
            || !p.data.occurrences.is_empty()
            || p.data.reason.is_some()
            || p.data.reviewed_plan.is_some()
            || p.data.generation.is_some()
            || p.data.error.is_some()
            || review["templateId"] != id
            || review["templateHash"] != hash
            || review["stateHash"] != s.hash
            || review["authorityHash"] != h.binding.authority_hash
            || review["executor"]["serviceAccountId"] != h.binding.service_account_id
            || review["executor"]["credentialId"] != h.binding.credential_id
            || review["policy"] != "normal_due_only"
            || schedule["stateHash"] != s.hash
            || schedule["templateHash"] != hash
            || review["template"] != recurring::scheduled_spec(db, id)?["specification"]
        {
            return Err(refused());
        }
    } else {
        let active = s.hosted.as_ref().filter(|h| h.active).ok_or_else(refused)?;
        if h.kind != "execution"
            || h.principal_kind != "service_account"
            || h.binding != active.binding
            || h.consent_hash.as_deref() != Some(active.consent_hash.as_str())
            || p.actor != active.binding.service_account_id
            || h.reason.is_some()
            || h.reviewed_plan.is_some()
            || !matches!(p.kind.as_str(), "attempted" | "succeeded" | "failed")
        {
            return Err(refused());
        }
        if p.kind == "attempted" {
            let review = p.data.reviewed_plan.as_ref().ok_or_else(refused)?;
            if !quiet(s, review)
                || p.data.occurrences.len() != 1
                || review["normalDue"] != json!(p.data.occurrences[0])
                || review["asOfDate"] != p.data.occurrences[0].issue_date
            {
                return Err(refused());
            }
        }
    }
    Ok(())
}
/// Read current delegated source; caller must independently verify live authority.
pub fn bindings(db: &Connection) -> Result<Vec<(i64, Binding)>, Error> {
    let all = events(db)?;
    let ids: BTreeSet<_> = all.iter().map(|e| e.payload.template_id).collect();
    let mut out = vec![];
    for id in ids {
        let s = state(db, id, &spec(db, id)?.0, &all)?;
        if let Some(h) = s.hosted.filter(|h| h.active) {
            out.push((id, h.binding));
        }
    }
    Ok(out)
}
pub fn plan(
    tx: &Transaction<'_>,
    id: i64,
    b: &Binding,
    display: &str,
    today: &str,
) -> Result<Value, Error> {
    if !valid(b) {
        return Err(refused());
    }
    let (hash, _) = spec(tx, id)?;
    let s = state(tx, id, &hash, &events(tx)?)?;
    let schedule = super::plan(tx, id, today)?;
    let mut v = json!({"templateId":id,"templateHash":hash,"stateHash":s.hash,"template":recurring::scheduled_spec(tx,id)?["specification"],"schedule":schedule,"executor":{"serviceAccountId":b.service_account_id,"credentialId":b.credential_id,"displayName":display},"authorityHash":b.authority_hash,"policy":"normal_due_only"});
    v["planHash"] = json!(sha(v.to_string().as_bytes()));
    Ok(v)
}
pub fn consent(
    tx: &Transaction<'_>,
    i: &engine::HostedConsent,
    b: &Binding,
    display: &str,
    actor: &str,
    today: &str,
    at: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let p = plan(tx, i.template_id, b, display, today)?;
    let (hash, active) = spec(tx, i.template_id)?;
    let s = state(tx, i.template_id, &hash, &events(tx)?)?;
    if !i.confirm
        || !active
        || i.service_account_id != b.service_account_id
        || i.credential_id != b.credential_id
        || actor != b.grantor_user_id
        || p["planHash"] != i.plan_hash
        || !matches!(s.state, "enabled" | "paused")
        || !quiet(&s, &p["schedule"])
    {
        return Err(refused());
    }
    append_payload(
        tx,
        Payload {
            version: 3,
            template_id: i.template_id,
            template_hash: hash,
            kind: "enabled".into(),
            actor: actor.into(),
            at: at.into(),
            data: Data::empty(),
            restored_consent: None,
            hosted: Some(Proof {
                kind: "consent".into(),
                principal_kind: "user".into(),
                binding: b.clone(),
                consent_hash: None,
                reason: Some(engine::reason(&i.reason)?.into()),
                reviewed_plan: Some(p),
            }),
        },
    )?;
    current(tx, i.template_id, today)
}
fn execution(s: &State) -> Result<Proof, Error> {
    let h = s.hosted.as_ref().filter(|h| h.active).ok_or_else(refused)?;
    Ok(Proof {
        kind: "execution".into(),
        principal_kind: "service_account".into(),
        binding: h.binding.clone(),
        consent_hash: Some(h.consent_hash.clone()),
        reason: None,
        reviewed_plan: None,
    })
}
pub fn due(tx: &Transaction<'_>, id: i64, b: &Binding, today: &str) -> Result<bool, Error> {
    let (hash, active) = spec(tx, id)?;
    let s = state(tx, id, &hash, &events(tx)?)?;
    let p = super::plan(tx, id, today)?;
    Ok(active
        && s.state == "enabled"
        && s.hosted
            .as_ref()
            .is_some_and(|h| h.active && h.binding == *b)
        && quiet(&s, &p)
        && !p["normalDue"].is_null())
}
pub fn begin(
    tx: &Transaction<'_>,
    id: i64,
    b: &Binding,
    today: &str,
    at: &str,
) -> Result<Occurrence, Error> {
    if !due(tx, id, b, today)? {
        return Err(refused());
    }
    let hash = spec(tx, id)?.0;
    let s = state(tx, id, &hash, &events(tx)?)?;
    let p = super::plan(tx, id, today)?;
    let o: Occurrence = serde_json::from_value(p["normalDue"].clone()).map_err(invalid)?;
    let mut data = Data::empty();
    data.occurrences.push(o.clone());
    data.reviewed_plan = Some(p);
    append_payload(
        tx,
        Payload {
            version: 3,
            template_id: id,
            template_hash: hash,
            kind: "attempted".into(),
            actor: b.service_account_id.clone(),
            at: at.into(),
            data,
            restored_consent: None,
            hosted: Some(execution(&s)?),
        },
    )?;
    Ok(o)
}
pub fn finish(
    tx: &Transaction<'_>,
    id: i64,
    o: Occurrence,
    result: Result<Value, String>,
    b: &Binding,
    today: &str,
    at: &str,
) -> Result<Value, Error> {
    let hash = spec(tx, id)?.0;
    let s = state(tx, id, &hash, &events(tx)?)?;
    let proof = execution(&s)?;
    if proof.binding != *b {
        return Err(refused());
    }
    let mut data = Data::empty();
    data.occurrences.push(o);
    let (kind, mut response) = match result {
        Ok(g) => {
            data.generation = Some(g.clone());
            (
                "succeeded",
                json!({"generation":g,"dispatch":{"status":"succeeded"}}),
            )
        }
        Err(e) => {
            data.error = Some(e.clone());
            ("failed", json!({"dispatch":{"status":"failed","error":e}}))
        }
    };
    append_payload(
        tx,
        Payload {
            version: 3,
            template_id: id,
            template_hash: hash,
            kind: kind.into(),
            actor: b.service_account_id.clone(),
            at: at.into(),
            data,
            restored_consent: None,
            hosted: Some(proof),
        },
    )?;
    response["scheduledRecurring"] = current(tx, id, today)?;
    Ok(response)
}

/// Trusted host milliseconds rendered for source event evidence.
pub fn timestamp(now: i64) -> Result<String, Error> {
    crate::mfa_login::date(now)
}
#[cfg(test)]
mod tests;
