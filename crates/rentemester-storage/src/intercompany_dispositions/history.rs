use super::*;
#[derive(Clone, Debug)]
pub struct HistoricalLink {
    pub disposition: Disposition,
    pub side: String,
    pub journal_entry_id: i64,
    pub journal_entry_no: String,
    pub journal_entry_hash: String,
    pub ledger_head_hash: Option<String>,
}
pub(super) struct State {
    pub disposition: Disposition,
    pub payload_hash: String,
    pub status: String,
    pub events: Vec<Value>,
    pub lifecycle: Vec<Value>,
    pub links: Vec<Value>,
    witnesses: BTreeSet<String>,
}
impl State {
    pub fn value(&self) -> Value {
        json!({"ok":true,"disposition":self.disposition,"payloadHash":self.payload_hash,"status":self.status,"events":self.events,"lifecycleEvents":self.lifecycle,"links":self.links})
    }
}
fn rows(db: &Connection, table: &str, order: &str) -> Result<Vec<Value>, Error> {
    let mut q = db
        .prepare(&format!("SELECT * FROM {table} ORDER BY {order}"))
        .map_err(sql)?;
    let columns = q
        .column_names()
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>();
    let mut cursor = q.query([]).map_err(sql)?;
    let mut result = vec![];
    while let Some(row) = cursor.next().map_err(sql)? {
        let mut object = serde_json::Map::new();
        for (i, name) in columns.iter().enumerate() {
            use rusqlite::types::ValueRef;
            let value = match row.get_ref(i).map_err(sql)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Text(s) => json!(std::str::from_utf8(s).map_err(sql)?),
                _ => return Err(refused("invalid disposition storage cell")),
            };
            object.insert(name.clone(), value);
        }
        result.push(Value::Object(object));
        if result.len() > 100000 {
            return Err(refused("disposition history bound exceeded"));
        }
    }
    Ok(result)
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str, Error> {
    v[key]
        .as_str()
        .ok_or_else(|| refused("invalid disposition history field"))
}
fn body(row: &Value) -> Result<Value, Error> {
    let raw = string(row, "canonical_payload")?;
    let value: Value = serde_json::from_str(raw).map_err(sql)?;
    if !value.is_object()
        || canonical(&value) != raw
        || hash(&value) != string(row, "payload_hash")?
    {
        return Err(refused("disposition canonical payload or hash differs"));
    }
    Ok(value)
}
fn chain(db: &Connection, table: &str, allowed: &[&str]) -> Result<Vec<Value>, Error> {
    let mut previous: Option<String> = None;
    let events = rows(db, table, "id")?;
    for row in &events {
        let kind = string(row, "event_type")?;
        let actor = string(row, "actor")?;
        let principal = string(row, "principal_id")?;
        let pk = string(row, "principal_kind")?;
        let at = string(row, "created_at")?;
        if !allowed.contains(&kind)
            || !["user", "service"].contains(&pk)
            || text(actor, 160)? != actor
            || text(principal, 160)? != principal
            || row["previous_hash"] != json!(previous)
        {
            return Err(refused("invalid disposition global event chain"));
        }
        crate::cookie_session::timestamp(at)?;
        body(row)?;
        let expected = hash(
            &json!({"previousHash":previous,"dispositionId":row["disposition_id"],"type":kind,"payloadHash":row["payload_hash"],"actor":actor,"principal":{"kind":pk,"id":principal},"at":at}),
        );
        if row["event_hash"] != expected {
            return Err(refused("disposition event hash differs"));
        }
        previous = Some(expected);
    }
    Ok(events)
}
pub(super) fn states(db: &Connection) -> Result<BTreeMap<String, State>, Error> {
    let mut result = BTreeMap::new();
    for row in rows(db, "rm_intercompany_dispositions", "disposition_id")? {
        let v = body(&row)?;
        let d: Disposition = serde_json::from_value(v.clone()).map_err(sql)?;
        let normalized = d.clone().normalize()?;
        if serde_json::to_value(&normalized).map_err(sql)? != v
            || row["disposition_id"] != d.disposition_id
        {
            return Err(refused("disposition normalization or ID differs"));
        }
        crate::cookie_session::timestamp(string(&row, "created_at")?)?;
        references(db, &d)?;
        result.insert(
            d.disposition_id.clone(),
            State {
                disposition: d,
                payload_hash: string(&row, "payload_hash")?.into(),
                status: "proposed".into(),
                events: vec![],
                lifecycle: vec![],
                links: vec![],
                witnesses: BTreeSet::new(),
            },
        );
    }
    for row in chain(
        db,
        EVENTS,
        &[
            "proposed",
            "approved",
            "rejected",
            "cancelled",
            "settled",
            "superseded",
            "exception",
        ],
    )? {
        let id = string(&row, "disposition_id")?;
        let s = result
            .get_mut(id)
            .ok_or_else(|| refused("disposition event has missing parent"))?;
        let payload = body(&row)?;
        let kind = string(&row, "event_type")?;
        if s.events.is_empty() {
            if kind != "proposed" || payload != serde_json::to_value(&s.disposition).map_err(sql)? {
                return Err(refused("disposition must begin with exact proposal"));
            }
        } else if kind == "proposed" {
            return Err(refused("duplicate disposition proposal event"));
        } else if kind == "approved" {
            let proposer = &s.events[0];
            if s.status != "proposed"
                || payload != json!({"payloadHash":s.payload_hash})
                || (proposer["principal_kind"] == row["principal_kind"]
                    && proposer["principal_id"] == row["principal_id"])
            {
                return Err(refused("invalid independent disposition approval"));
            }
            s.status = "approved".into();
        } else if payload
            .get("payloadHash")
            .is_some_and(|h| h != &s.payload_hash)
        {
            return Err(refused(
                "legacy disposition event payload reference differs",
            ));
        }
        if kind == "exception" && payload["kind"] == "one_sided_posting" {
            let side = payload["side"]
                .as_str()
                .filter(|s| ["left", "right"].contains(s))
                .ok_or_else(|| refused("invalid legacy disposition link exception"))?;
            if s.status != "approved" || !s.witnesses.insert(side.into()) {
                return Err(refused("invalid legacy disposition link ordering"));
            }
        }
        s.events.push(row);
    }
    let mut used = BTreeSet::new();
    for row in rows(db, "rm_intercompany_disposition_journal_links", "id")? {
        let s = result
            .get_mut(string(&row, "disposition_id")?)
            .ok_or_else(|| refused("disposition journal link has no parent"))?;
        let side = string(&row, "side")?;
        let expected = match side {
            "left" => &s.disposition.left,
            "right" => &s.disposition.right,
            _ => return Err(refused("invalid disposition link side")),
        };
        let company = string(&row, "company_slug")?;
        let id = row["journal_entry_id"]
            .as_i64()
            .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991)
            .ok_or_else(|| refused("invalid linked journal ID"))?;
        if company != expected.company_slug
            || !hex(string(&row, "journal_entry_hash")?)
            || (!row["ledger_head_hash"].is_null()
                && !row["ledger_head_hash"].as_str().is_some_and(hex))
            || s.links.iter().any(|l| l["side"] == side)
            || !used.insert((company.to_owned(), id))
        {
            return Err(refused("invalid or reused disposition journal link"));
        }
        text(string(&row, "journal_entry_no")?, 160)?;
        text(string(&row, "actor")?, 160)?;
        text(string(&row, "principal_id")?, 160)?;
        if !["user", "service"].contains(&string(&row, "principal_kind")?) {
            return Err(refused("unsupported historical link principal"));
        }
        crate::cookie_session::timestamp(string(&row, "linked_at")?)?;
        s.links.push(row);
    }
    for row in chain(
        db,
        LIFECYCLE,
        &[
            "settled",
            "superseded",
            "reopened",
            "exception",
            "exception_resolved",
        ],
    )? {
        let payload = body(&row)?;
        let id = string(&row, "disposition_id")?;
        if let Some(replacement) = payload["replacementDispositionId"].as_str()
            && (replacement == id || !result.contains_key(replacement))
        {
            return Err(refused("supersession replacement missing or self"));
        }
        let s = result
            .get_mut(id)
            .ok_or_else(|| refused("disposition lifecycle has no parent"))?;
        if payload["payloadHash"] != s.payload_hash {
            return Err(refused("disposition lifecycle payload reference differs"));
        }
        match string(&row, "event_type")? {
            "exception" => {
                let side = payload["side"]
                    .as_str()
                    .ok_or_else(|| refused("link exception side missing"))?;
                if !s.witnesses.insert(side.to_owned()) {
                    return Err(refused("duplicate disposition link exception"));
                }
                if payload["kind"] != "one_sided_posting"
                    || !["approved"].contains(&s.status.as_str())
                    || !s.links.iter().any(|l| l["side"] == payload["side"])
                {
                    return Err(refused("invalid disposition link exception"));
                }
            }
            "exception_resolved" => {
                if payload["kind"] != "one_sided_posting"
                    || s.status != "approved"
                    || s.witnesses.len() != 2
                    || s.links.len() != 2
                {
                    return Err(refused("invalid disposition exception resolution"));
                }
            }
            "settled" => {
                if s.status != "approved" || s.links.len() != 2 {
                    return Err(refused("invalid disposition settlement history"));
                }
                let ids: Vec<String> =
                    serde_json::from_value(payload["settlementEvidenceRecordIds"].clone())
                        .map_err(sql)?;
                if refs(&ids)? != ids {
                    return Err(refused("settlement references are not canonical"));
                }
                records(db, &ids)?;
                s.status = "settled".into();
            }
            "superseded" => {
                if ["settled", "superseded"].contains(&s.status.as_str())
                    || payload["replacementDispositionId"].as_str().is_none()
                {
                    return Err(refused("invalid disposition supersession history"));
                }
                text(
                    payload["reason"]
                        .as_str()
                        .ok_or_else(|| refused("supersession reason missing"))?,
                    1000,
                )?;
                s.status = "superseded".into();
            }
            "reopened" => {
                if !["settled", "superseded"].contains(&s.status.as_str()) {
                    return Err(refused("invalid disposition reopening history"));
                }
                text(
                    payload["reason"]
                        .as_str()
                        .ok_or_else(|| refused("reopen reason missing"))?,
                    1000,
                )?;
                s.status = "approved".into();
            }
            _ => unreachable!(),
        }
        s.lifecycle.push(row);
    }
    for s in result.values_mut() {
        if s.links.iter().any(|l| {
            !l["side"]
                .as_str()
                .is_some_and(|side| s.witnesses.contains(side))
        }) {
            return Err(refused(
                "disposition link has no approved lifecycle evidence",
            ));
        }
        if s.events.is_empty() {
            return Err(refused("disposition has no proposal history"));
        }
        if s.status == "approved" {
            s.status = match s.links.len() {
                0 => "approved",
                1 => "partly_posted",
                2 => "posted",
                _ => return Err(refused("too many disposition links")),
            }
            .into();
        }
    }
    Ok(result)
}
/// Verifies immutable control evidence only. Host must check actual signed company ledgers,
/// original inventories and historical head prefixes; current readiness is deliberately separate.
pub fn validate_history(tx: &Transaction<'_>) -> Result<Vec<HistoricalLink>, Error> {
    let mut links = vec![];
    for s in states(tx)?.into_values() {
        for l in s.links {
            links.push(HistoricalLink {
                disposition: s.disposition.clone(),
                side: string(&l, "side")?.into(),
                journal_entry_id: l["journal_entry_id"].as_i64().unwrap(),
                journal_entry_no: string(&l, "journal_entry_no")?.into(),
                journal_entry_hash: string(&l, "journal_entry_hash")?.into(),
                ledger_head_hash: l["ledger_head_hash"].as_str().map(str::to_owned),
            });
        }
    }
    Ok(links)
}
