//! Fixed party business codec. Historical actors/relationship roles never become live authority.
use super::*;
use rentemester_engine::{
    company_knowledge::iso,
    parties::{Identifier, KINDS, ROLES, bounded, identity},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
const FORMAT: &str = "rentemester-native-parties/1";
const TABLES: [&str; 6] = [
    "rm_party_events",
    "rm_party_identifiers",
    "rm_party_company_roles",
    "rm_party_alias_assertions",
    "rm_party_field_assertions",
    "rm_party_legacy_links",
];
const COLUMNS: [&[&str]; 6] = [
    &[
        "id",
        "party_id",
        "event_type",
        "canonical_kind",
        "payload_hash",
        "canonical_payload",
        "actor",
        "created_at",
    ],
    &["party_id", "jurisdiction", "identifier_kind", "identifier"],
    &["party_id", "company_slug", "role", "defaults_json"],
    &[
        "id",
        "party_id",
        "alias",
        "source",
        "observed_at",
        "review_state",
        "payload_hash",
        "actor",
        "created_at",
    ],
    &[
        "id",
        "party_id",
        "field",
        "value",
        "source",
        "observed_at",
        "review_state",
        "payload_hash",
        "actor",
        "created_at",
    ],
    &[
        "company_slug",
        "legacy_kind",
        "legacy_id",
        "party_id",
        "actor",
        "created_at",
    ],
];
fn shape(db: &Connection) -> Result<(), Error> {
    for (table, expected) in TABLES.iter().zip(COLUMNS) {
        if columns(db, table)? != expected {
            return Err(refused("party schema differs from codec v1"));
        }
    }
    Ok(())
}
pub(super) fn export(db: &Connection) -> Result<Vec<u8>, Error> {
    shape(db)?;
    let bytes = export_tables(db, &TABLES, FORMAT)?;
    facts(db)?;
    Ok(bytes)
}
/// Only source-declared slugs are references. They map to manifest ledger UUIDs,
/// which the host independently checks against each restored company snapshot.
pub fn validate_companies(db: &Connection, companies: &[String]) -> Result<(), Error> {
    let known: BTreeSet<_> = companies.iter().collect();
    for table in [TABLES[2], TABLES[5]] {
        let mut q = db
            .prepare(&format!("SELECT DISTINCT company_slug FROM {table}"))
            .map_err(sql)?;
        for slug in q.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
            if !known.contains(&slug.map_err(sql)?) {
                return Err(refused("party refers to an unregistered company"));
            }
        }
    }
    Ok(())
}
pub fn initialize(db: &mut Connection, bytes: &[u8], companies: &[String]) -> Result<(), Error> {
    crate::control_migration::validate_current_control(db)?;
    shape(db)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(refused("party codec file bound"));
    }
    let business: Business = serde_json::from_slice(bytes).map_err(sql)?;
    if business.format != FORMAT || business.tables.len() != TABLES.len() {
        return Err(refused("unsupported party codec"));
    }
    let mut text_bytes = 0usize;
    for (name, expected) in TABLES.iter().zip(COLUMNS) {
        let table = business
            .tables
            .get(*name)
            .ok_or_else(|| refused("party table missing"))?;
        if table.columns != expected || table.rows.len() > 100000 {
            return Err(refused("party codec table shape differs"));
        }
        for row in &table.rows {
            if row.len() != expected.len() {
                return Err(refused("party row shape differs"));
            }
            for (column, cell) in expected.iter().zip(row) {
                match (*column, cell) {
                    ("id", Cell::Integer(_)) => {}
                    ("id", _) => return Err(refused("party id type differs")),
                    (_, Cell::Text(s)) => {
                        text_bytes = text_bytes
                            .checked_add(s.len())
                            .ok_or_else(|| refused("party byte bound"))?;
                        if text_bytes > 16 * 1024 * 1024 {
                            return Err(refused("party byte bound"));
                        }
                    }
                    _ => return Err(refused("party cell type differs")),
                }
            }
        }
    }
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql)?;
    for table in TABLES {
        if tx
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .map_err(sql)?
            != 0
        {
            return Err(refused("fresh party target required"));
        }
    }
    insert_tables(&tx, &business, &TABLES)?;
    facts(&tx)?;
    validate_companies(&tx, companies)?;
    // SQLite affinities and explicit row ordering may not silently alter source facts.
    let actual: Value =
        serde_json::from_slice(&export_tables(&tx, &TABLES, FORMAT)?).map_err(sql)?;
    if actual != serde_json::to_value(&business).map_err(sql)? {
        return Err(refused("party codec order or types changed"));
    }
    crate::control_migration::validate_current_control(&tx)?;
    tx.commit().map_err(sql)
}
fn exact_text(s: &str, max: usize) -> Result<(), Error> {
    if bounded(s, max).map_err(sql)? != s {
        return Err(refused("noncanonical party text"));
    }
    Ok(())
}
fn stamp(s: &str) -> Result<(), Error> {
    if iso(s).map_err(sql)? != s {
        return Err(refused("noncanonical party timestamp"));
    }
    Ok(())
}
fn keys(v: &Value, expected: &[&str]) -> Result<(), Error> {
    if v.as_object()
        .is_none_or(|o| o.len() != expected.len() || !expected.iter().all(|k| o.contains_key(*k)))
    {
        return Err(refused("unsupported party source payload"));
    }
    Ok(())
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, Error> {
    v[key]
        .as_str()
        .ok_or_else(|| refused("party source field type"))
}
fn payload(raw: &str, expected_hash: &str) -> Result<Value, Error> {
    let v: Value = serde_json::from_str(raw).map_err(sql)?;
    if !v.is_object()
        || crate::parties::canonical(&v) != raw
        || crate::parties::hash(&v) != expected_hash
    {
        return Err(refused("party source hash or canonical payload differs"));
    }
    Ok(v)
}
fn defaults(v: &Value) -> Result<(), Error> {
    if v.as_object().is_none_or(|o| {
        o.keys()
            .any(|k| !["account", "vat", "currency", "paymentTermsDays"].contains(&k.as_str()))
    }) {
        return Err(refused("unsupported company-scoped party defaults"));
    }
    Ok(())
}
type Identity = (String, String, String, String);
struct Event {
    id: i64,
    party: String,
    kind: String,
    event: String,
    hash: String,
    payload: Value,
}
fn facts(db: &Connection) -> Result<(), Error> {
    let mut q=db.prepare("SELECT id,party_id,event_type,canonical_kind,payload_hash,canonical_payload,actor,created_at FROM rm_party_events ORDER BY id").map_err(sql)?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
            ))
        })
        .map_err(sql)?;
    let mut events = Vec::new();
    let mut created = BTreeMap::new();
    let mut expected_identifiers = BTreeSet::<Identity>::new();
    for row in rows {
        let (id, party, event, kind, hash, raw, actor, at) = row.map_err(sql)?;
        exact_text(&party, 64)?;
        exact_text(&actor, 160)?;
        stamp(&at)?;
        if !KINDS.contains(&kind.as_str()) {
            return Err(refused("unsupported party kind"));
        }
        let p = payload(&raw, &hash)?;
        if event == "created" {
            keys(
                &p,
                &[
                    "name",
                    "aliases",
                    "identifiers",
                    "source",
                    "observedAt",
                    "reviewAssertion",
                ],
            )?;
            exact_text(text(&p, "name")?, 320)?;
            exact_text(text(&p, "source")?, 160)?;
            exact_text(text(&p, "reviewAssertion")?, 500)?;
            if text(&p, "observedAt")? != at {
                return Err(refused("party creation observation differs"));
            }
            let aliases = p["aliases"]
                .as_array()
                .ok_or_else(|| refused("party alias array required"))?;
            let mut seen = BTreeSet::new();
            if aliases.len() > 16 {
                return Err(refused("party aliases bound"));
            }
            for alias in aliases {
                let a = alias.as_str().ok_or_else(|| refused("party alias type"))?;
                exact_text(a, 160)?;
                if !seen.insert(a) {
                    return Err(refused("duplicate created alias"));
                }
            }
            for item in p["identifiers"]
                .as_array()
                .ok_or_else(|| refused("party identifier array required"))?
            {
                keys(item, &["country", "identifier", "identifierKind"])?;
                let i = Identifier {
                    country: text(item, "country")?.into(),
                    identifier: Some(text(item, "identifier")?.into()),
                    identifier_kind: text(item, "identifierKind")?.into(),
                };
                let normalized = identity(&i).map_err(sql)?;
                if normalized.country != i.country
                    || Some(&normalized.identifier) != i.identifier.as_ref()
                    || normalized.identifier_kind != i.identifier_kind
                    || !expected_identifiers.insert((
                        party.clone(),
                        normalized.country,
                        normalized.identifier_kind,
                        normalized.identifier,
                    ))
                {
                    return Err(refused("noncanonical or duplicate party identifier"));
                }
            }
            if created.insert(party.clone(), (id, kind.clone())).is_some() {
                return Err(refused("conflicting canonical party creations"));
            }
        }
        events.push(Event {
            id,
            party,
            kind,
            event,
            hash,
            payload: p,
        });
    }
    let known = |id: &str| -> Result<(), Error> {
        if !created.contains_key(id) {
            return Err(refused("dangling party reference"));
        }
        Ok(())
    };
    let mut q=db.prepare("SELECT party_id,jurisdiction,identifier_kind,identifier FROM rm_party_identifiers ORDER BY jurisdiction,identifier_kind,identifier,party_id").map_err(sql)?;
    let actual = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(sql)?;
    if actual != expected_identifiers {
        return Err(refused(
            "party identifier index differs from source creation",
        ));
    }
    let mut roles = BTreeMap::new();
    let mut q = db
        .prepare("SELECT party_id,company_slug,role,defaults_json FROM rm_party_company_roles")
        .map_err(sql)?;
    for row in q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(sql)?
    {
        let (id, slug, role, raw) = row.map_err(sql)?;
        known(&id)?;
        exact_text(&slug, 120)?;
        if !ROLES.contains(&role.as_str()) {
            return Err(refused("unsupported party relationship role"));
        }
        let d: Value = serde_json::from_str(&raw).map_err(sql)?;
        defaults(&d)?;
        if crate::parties::canonical(&d) != raw {
            return Err(refused("noncanonical party defaults"));
        }
        roles.insert((id, slug, role), d);
    }
    let mut legacy = BTreeSet::new();
    let mut q=db.prepare("SELECT company_slug,legacy_kind,legacy_id,party_id,actor,created_at FROM rm_party_legacy_links").map_err(sql)?;
    for row in q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .map_err(sql)?
    {
        let (slug, kind, id, party, actor, at) = row.map_err(sql)?;
        known(&party)?;
        exact_text(&slug, 120)?;
        exact_text(&id, 160)?;
        exact_text(&actor, 160)?;
        stamp(&at)?;
        if !["customer", "vendor"].contains(&kind.as_str()) {
            return Err(refused("unsupported legacy party reference"));
        }
        legacy.insert((party, slug, kind, id));
    }
    let mut proposals = BTreeSet::new();
    let mut approvals = BTreeSet::new();
    for e in events {
        let (c_id, c_kind) = created
            .get(&e.party)
            .ok_or_else(|| refused("dangling party event"))?;
        if e.kind != *c_kind || e.id < *c_id {
            return Err(refused("party event predates or conflicts with creation"));
        }
        match e.event.as_str() {
            "created" => {}
            "linked" => {
                let p = &e.payload;
                if p.get("role").is_some() {
                    keys(p, &["companySlug", "role", "defaults"])?;
                    let key = (
                        e.party.clone(),
                        text(p, "companySlug")?.into(),
                        text(p, "role")?.into(),
                    );
                    if roles.get(&key) != Some(&p["defaults"]) {
                        return Err(refused("party role event/index differs"));
                    }
                } else {
                    keys(p, &["companySlug", "legacyKind", "legacyId"])?;
                    if !legacy.contains(&(
                        e.party.clone(),
                        text(p, "companySlug")?.into(),
                        text(p, "legacyKind")?.into(),
                        text(p, "legacyId")?.into(),
                    )) {
                        return Err(refused("party legacy event/index differs"));
                    }
                }
            }
            "proposed_merge" | "approved_merge" | "superseded" => {
                keys(&e.payload, &["intoPartyId", "reviewAssertion"])?;
                let target = text(&e.payload, "intoPartyId")?;
                known(target)?;
                if target == e.party {
                    return Err(refused("self party merge unsupported"));
                }
                exact_text(text(&e.payload, "reviewAssertion")?, 500)?;
                let key = (e.party, e.hash);
                match e.event.as_str() {
                    "proposed_merge" => {
                        proposals.insert(key);
                    }
                    "approved_merge" => {
                        if !proposals.contains(&key) {
                            return Err(refused("party merge approval lacks exact proposal"));
                        }
                        approvals.insert(key);
                    }
                    _ => {
                        if !approvals.contains(&key) {
                            return Err(refused("party supersession lacks exact approval"));
                        }
                    }
                }
            }
            _ => return Err(refused("unsupported party event")),
        }
    }
    for alias in [true, false] {
        let table = if alias { TABLES[3] } else { TABLES[4] };
        let cols = if alias {
            "party_id,alias,'' AS value,source,observed_at,review_state,payload_hash,actor,created_at"
        } else {
            "party_id,field,value,source,observed_at,review_state,payload_hash,actor,created_at"
        };
        let mut q = db
            .prepare(&format!("SELECT {cols} FROM {table} ORDER BY id"))
            .map_err(sql)?;
        for row in q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                ))
            })
            .map_err(sql)?
        {
            let (id, field, value, source, at, state, hash, actor, created_at) =
                row.map_err(sql)?;
            known(&id)?;
            exact_text(&field, if alias { 160 } else { 64 })?;
            exact_text(&source, 160)?;
            exact_text(&actor, 160)?;
            stamp(&at)?;
            stamp(&created_at)?;
            if !["proposed", "approved", "rejected"].contains(&state.as_str()) {
                return Err(refused("unsupported party review state"));
            }
            let p = if alias {
                if field.to_lowercase() != field {
                    return Err(refused("noncanonical alias"));
                }
                json!({"alias":field,"source":source,"observedAt":at,"reviewState":state})
            } else {
                exact_text(&value, 512)?;
                json!({"field":field,"value":value,"source":source,"observedAt":at,"reviewState":state})
            };
            if crate::parties::hash(&p) != hash {
                return Err(refused("party assertion source hash differs"));
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn pair() -> (Connection, Connection) {
        let mut c = Connection::open_in_memory().unwrap();
        let mut l = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic party backup").unwrap();
        (c, l)
    }
    fn event(db: &Connection, id: &str, kind: &str, payload: Value) {
        db.execute("INSERT INTO rm_party_events(party_id,event_type,canonical_kind,payload_hash,canonical_payload,actor,created_at) VALUES(?,?, 'organization',?,?, 'user:historical-party-owner','2026-02-01T00:00:00.000Z')",params![id,kind,crate::parties::hash(&payload),crate::parties::canonical(&payload)]).unwrap();
    }
    fn fixture(c: &mut Connection) {
        let tx = c
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let input=serde_json::from_value::<crate::parties::Create>(json!({"partyId":"party-source","kind":"organization","name":"Synthetic historical party","aliases":["Synthetic Trading"],"identifiers":[{"country":"DK","identifier":"12345678","identifierKind":"dk_cvr"}],"source":"synthetic-brief","observedAt":"2026-01-01","reviewAssertion":"Synthetic original reviewed","role":"supplier"})).unwrap();
        crate::parties::create(
            &tx,
            &input,
            "unused",
            "user:historical-party-owner",
            "2026-01-02T00:00:00.000Z",
        )
        .unwrap();
        for (slug, role, defaults) in [
            (
                "synthetic",
                "supplier",
                json!({"currency":"DKK","paymentTermsDays":14}),
            ),
            (
                "other",
                "customer",
                json!({"account":"synthetic-other-account"}),
            ),
        ] {
            crate::parties::role(
                &tx,
                "party-source",
                slug,
                &crate::parties::Role {
                    role: role.into(),
                    defaults: Some(defaults.as_object().unwrap().clone()),
                },
                "user:historical-party-owner",
                "2026-01-02T00:00:00.000Z",
            )
            .unwrap();
        }
        for (state, alias) in [
            ("proposed", "Synthetic proposed alias"),
            ("rejected", "Synthetic rejected alias"),
            ("approved", "Synthetic approved alias"),
        ] {
            crate::parties::alias(
                &tx,
                "party-source",
                &crate::parties::Alias {
                    alias: alias.into(),
                    source: "synthetic-import".into(),
                    observed_at: "2026-02-01".into(),
                    review_state: state.into(),
                },
                "user:historical-party-reviewer",
                "2026-02-02T00:00:00.000Z",
            )
            .unwrap();
        }
        crate::parties::assertion(
            &tx,
            "party-source",
            &crate::parties::Assertion {
                field: "name".into(),
                value: "Synthetic revised legal name".into(),
                source: "synthetic-registry-snapshot".into(),
                observed_at: "2026-02-01".into(),
                review_state: "approved".into(),
            },
            "user:historical-party-reviewer",
            "2026-02-02T00:00:00.000Z",
        )
        .unwrap();
        tx.commit().unwrap();
        // Source-compatible pre-v19 creation keeps alias fallback and no synthesized assertion rows.
        let p = json!({"name":"Synthetic legacy party","aliases":["Legacy Created Alias"],"identifiers":[],"source":"legacy-source","observedAt":"2026-02-01T00:00:00.000Z","reviewAssertion":"Legacy reviewed original"});
        event(c, "party-legacy", "created", p);
        c.execute("INSERT INTO rm_party_company_roles(party_id,company_slug,role,defaults_json)VALUES('party-legacy','synthetic','vendor','{}')",[]).unwrap();
        let merged =
            json!({"intoPartyId":"party-source","reviewAssertion":"Synthetic reviewed merge"});
        for kind in ["proposed_merge", "approved_merge", "superseded"] {
            event(c, "party-legacy", kind, merged.clone());
        }
        c.execute("INSERT INTO rm_party_legacy_links(company_slug,legacy_kind,legacy_id,party_id,actor,created_at)VALUES('other','vendor','legacy-7','party-source','user:historical-party-owner','2026-02-01T00:00:00.000Z')",[]).unwrap();
        event(
            c,
            "party-source",
            "linked",
            json!({"companySlug":"other","legacyKind":"vendor","legacyId":"legacy-7"}),
        );
    }
    fn exported(c: &mut Connection) -> Control {
        let tx = c
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let e = export_control(&tx).unwrap();
        tx.rollback().unwrap();
        e
    }
    #[test]
    fn party_codec_roundtrip_preserves_all_source_tables_native_projection_and_no_authority() {
        let (mut source, _) = pair();
        fixture(&mut source);
        source.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('source-auth-owner','Synthetic source owner','source-owner@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId)VALUES('source-private-session','2026-01-01T12:00:00.000Z','SYNTHETIC-EXCLUDED-SESSION','2026-01-01','2026-01-01','source-auth-owner');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('source-factor','SYNTHETIC-EXCLUDED-MFA','SYNTHETIC-EXCLUDED-RECOVERY','source-auth-owner',1);INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('source-auth-owner','activate','workspace_owner','system:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('source-auth-owner','synthetic','grant','owner','system:synthetic');").unwrap();
        let e = exported(&mut source);
        let serialized = String::from_utf8(e.parties_bytes.clone()).unwrap();
        assert!(!serialized.contains("SYNTHETIC-EXCLUDED"));
        assert_eq!(e.unsupported.len(), 4);
        assert!(!e.unsupported.iter().any(|s| s.0 == "parties"));
        let mut target = Connection::open_in_memory().unwrap();
        initialize_control(&mut target, &e.bytes, &"a".repeat(64)).unwrap();
        initialize(
            &mut target,
            &e.parties_bytes,
            &["synthetic".into(), "other".into()],
        )
        .unwrap();
        assert_eq!(
            export_tables(&target, &TABLES, FORMAT).unwrap(),
            e.parties_bytes
        );
        for names in [vec!["synthetic"], vec!["other"], vec!["synthetic", "other"]] {
            let visible = names.into_iter().map(str::to_owned).collect();
            let a = source
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            let b = target
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            for id in ["party-source", "party-legacy"] {
                assert_eq!(
                    crate::parties::inspect(&a, id, &visible).ok(),
                    crate::parties::inspect(&b, id, &visible).ok()
                );
            }
            assert_eq!(
                crate::parties::list(&a, &visible, Some("Synthetic"), 0, 25).unwrap(),
                crate::parties::list(&b, &visible, Some("Synthetic"), 0, 25).unwrap()
            );
            a.rollback().unwrap();
            b.rollback().unwrap();
        }
        for name in [
            "user",
            "session",
            "account",
            "twoFactor",
            "apikey",
            "rm_workspace_user_access_events",
            "rm_company_membership_events",
            "rm_workspace_invitation_events",
        ] {
            assert_eq!(
                target
                    .query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
        assert!(
            initialize(
                &mut target,
                &e.parties_bytes,
                &["synthetic".into(), "other".into()]
            )
            .is_err()
        );
    }
    #[test]
    fn malformed_conflicting_unknown_or_dangling_party_codec_refuses_atomically() {
        let (mut source, _) = pair();
        fixture(&mut source);
        let e = exported(&mut source);
        let original: Value = serde_json::from_slice(&e.parties_bytes).unwrap();
        for case in 0..15 {
            let mut bad = original.clone();
            match case {
                0 => bad["format"] = json!("unknown-party-codec"),
                1 => bad["tables"][TABLES[0]]["columns"][0] = json!("unknown_id"),
                2 => bad["tables"]["user"] = json!({"columns":[],"rows":[]}),
                3 => bad["tables"][TABLES[0]]["rows"][0][0] = json!({"type":"Text","value":"1"}),
                4 => bad["tables"][TABLES[0]]["rows"][0][4]["value"] = json!("f".repeat(64)),
                5 => bad["tables"][TABLES[1]]["rows"][0][0]["value"] = json!("party-missing"),
                6 => bad["tables"][TABLES[3]]["rows"][0][1]["value"] = json!("party-missing"),
                7 => bad["tables"][TABLES[4]]["rows"][0][6]["value"] = json!("f".repeat(64)),
                8 => bad["tables"][TABLES[5]]["rows"][0][3]["value"] = json!("party-missing"),
                9 => {
                    bad["tables"].as_object_mut().unwrap().remove(TABLES[3]);
                }
                10 => {
                    let rows = bad["tables"][TABLES[0]]["rows"].as_array_mut().unwrap();
                    rows.reverse();
                }
                11 => {
                    bad["tables"][TABLES[2]]["rows"][0][3]["value"] =
                        json!("{\"arbitrary_default\":true}")
                }
                12 => {
                    let rows = bad["tables"][TABLES[0]]["rows"].as_array_mut().unwrap();
                    rows.retain(|r| r[2]["value"] != "proposed_merge");
                }
                14 => {
                    let mut row = bad["tables"][TABLES[0]]["rows"][0].clone();
                    row[0]["value"] = json!(1000);
                    let mut p: Value =
                        serde_json::from_str(row[5]["value"].as_str().unwrap()).unwrap();
                    p["name"] = json!("Conflicting created identity");
                    row[4]["value"] = json!(crate::parties::hash(&p));
                    row[5]["value"] = json!(crate::parties::canonical(&p));
                    bad["tables"][TABLES[0]]["rows"]
                        .as_array_mut()
                        .unwrap()
                        .push(row);
                }
                _ => {
                    let row = &mut bad["tables"][TABLES[0]]["rows"][4];
                    let raw = row[5]["value"].as_str().unwrap();
                    let mut p: Value = serde_json::from_str(raw).unwrap();
                    p["intoPartyId"] = json!("party-missing");
                    row[4]["value"] = json!(crate::parties::hash(&p));
                    row[5]["value"] = json!(crate::parties::canonical(&p));
                }
            }
            let mut target = Connection::open_in_memory().unwrap();
            initialize_control(&mut target, &e.bytes, &"a".repeat(64)).unwrap();
            assert!(
                initialize(
                    &mut target,
                    &serde_json::to_vec(&bad).unwrap(),
                    &["synthetic".into(), "other".into()]
                )
                .is_err(),
                "case{case}"
            );
            for name in TABLES {
                assert_eq!(
                    target
                        .query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| r
                            .get::<_, i64>(0))
                        .unwrap(),
                    0,
                    "case{case}"
                );
            }
        }
    }
    #[test]
    fn company_reference_mismatch_source_corruption_and_unknown_schema_are_refused() {
        let (mut source, _) = pair();
        fixture(&mut source);
        let e = exported(&mut source);
        let mut target = Connection::open_in_memory().unwrap();
        initialize_control(&mut target, &e.bytes, &"a".repeat(64)).unwrap();
        assert!(initialize(&mut target, &e.parties_bytes, &["synthetic".into()]).is_err());
        assert_eq!(
            target
                .query_row("SELECT COUNT(*) FROM rm_party_events", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        source.execute("INSERT INTO rm_party_field_assertions(party_id,field,value,source,observed_at,review_state,payload_hash,actor,created_at)VALUES('party-source','name','Tampered','source','2026-02-01T00:00:00.000Z','approved',?,'user:historical','2026-02-01T00:00:00.000Z')",["a".repeat(64)]).unwrap();
        let tx = source
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert!(export_control(&tx).is_err());
        tx.rollback().unwrap();
        let (mut clean, _) = pair();
        clean
            .execute_batch("ALTER TABLE rm_party_company_roles ADD COLUMN unknown_future TEXT")
            .unwrap();
        let tx = clean
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert!(export_control(&tx).is_err());
        tx.rollback().unwrap();
    }
}
