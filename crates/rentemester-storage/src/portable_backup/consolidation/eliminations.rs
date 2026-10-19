//! Historical mapping evidence binds payload identity; current ledger freshness is a report gate.
use super::*;
use rentemester_engine::{eliminations::BalancePayload, intercompany::Mapping};
use std::collections::BTreeMap;
fn payloads(tx: &Transaction<'_>) -> Result<Vec<BalancePayload>, Error> {
    let mut q = tx
        .prepare("SELECT canonical_payload FROM rm_consolidation_elimination_events ORDER BY id")
        .map_err(sql)?;
    q.query_map([], |r| r.get::<_, String>(0))
        .map_err(sql)?
        .map(|r| {
            serde_json::from_str(&r.map_err(sql)?)
                .map_err(|_| refused("elimination payload malformed"))
        })
        .collect()
}
pub(super) fn companies(tx: &Transaction<'_>) -> Result<BTreeSet<String>, Error> {
    let mut refs = BTreeSet::new();
    for p in payloads(tx)? {
        refs.extend(p.companies());
    }
    Ok(refs)
}
pub(super) fn validate(tx: &Transaction<'_>) -> Result<(), Error> {
    crate::eliminations::states(tx)
        .map_err(|_| refused("elimination lifecycle, payload or hash chain refused"))?;
    let mut q=tx.prepare("SELECT mapping_hash,canonical_mapping FROM rm_intercompany_mapping_events WHERE event_type='approved' ORDER BY id").map_err(sql)?;
    let mappings = q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(sql)?
        .map(|r| {
            let (h, c) = r.map_err(sql)?;
            let m: Mapping = serde_json::from_str(&c).map_err(sql)?;
            Ok(((m.id.clone(), h), m))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    for p in payloads(tx)? {
        let m = mappings
            .get(&(p.mapping_id.clone(), p.mapping_hash.clone()))
            .ok_or_else(|| refused("elimination lacks authenticated approved mapping history"))?;
        ensure(
            p.group_id == m.group_id
                && p.left.company_slug == m.left_company_slug
                && p.right.company_slug == m.right_company_slug
                && p.left.account_nos == m.left_account_nos
                && p.right.account_nos == m.right_account_nos
                && m.interval.active(&p.as_of),
            "elimination mapping identity, accounts or date differ",
        )?;
        // V1's known historical orientation bug remains authentic historical evidence.
        // Current report validation must still refuse unsafe V1 rather than rewrite it.
        if p.version == 2 {
            let a = p.adjustments()?;
            ensure(
                a[0].position == m.left_position && a[1].position == m.right_position,
                "V2 elimination position differs from mapping source",
            )?;
        }
    }
    Ok(())
}
