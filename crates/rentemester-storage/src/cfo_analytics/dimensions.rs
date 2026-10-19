use super::*;
/// Historical allocation proof binds exact immutable line facts; current master eligibility is not rewritten.
pub(super) fn read(
    tx: &Transaction<'_>,
    line: i64,
    entry: i64,
    entry_hash: &str,
    currency: &str,
) -> Result<Vec<Dimension>, Error> {
    let mut q=tx.prepare("SELECT journal_entry_id,journal_entry_hash,line_currency,line_amount_minor,allocations_json,source,source_ref,plan_hash FROM current_accounting_dimension_assignments WHERE journal_line_id=? ORDER BY id").map_err(sql)?;
    let mut rs = q.query([line]).map_err(sql)?;
    let Some(r) = rs.next().map_err(sql)? else {
        return Ok(vec![]);
    };
    let minor: i64 = r.get(3).map_err(sql)?;
    let allocations: Vec<crate::dimensions::Allocation> =
        serde_json::from_str(&r.get::<_, String>(4).map_err(sql)?).map_err(sql)?;
    let plan = json!({"schemaVersion":"rentemester-accounting-dimensions-v1","journalLineId":line,"journalEntryId":r.get::<_,i64>(0).map_err(sql)?,"journalEntryHash":r.get::<_,String>(1).map_err(sql)?,"lineCurrency":r.get::<_,String>(2).map_err(sql)?,"lineAmountMinor":minor,"allocations":allocations,"source":r.get::<_,String>(5).map_err(sql)?,"sourceRef":r.get::<_,Option<String>>(6).map_err(sql)?});
    let mut amounts = tx
        .prepare("SELECT debit_amount,credit_amount FROM journal_lines WHERE id=?")
        .map_err(sql)?;
    let mut line_rows = amounts.query([line]).map_err(sql)?;
    let line_row = line_rows
        .next()
        .map_err(sql)?
        .ok_or_else(|| sql("line missing"))?;
    let actual = (
        crate::legacy_money(line_row.get_ref(0).map_err(sql)?)?,
        crate::legacy_money(line_row.get_ref(1).map_err(sql)?)?,
    );
    let magnitude = actual
        .0
        .subtract(&actual.1)
        .decimal()
        .trim_start_matches('-')
        .replace('.', "")
        .parse::<i64>()
        .map_err(sql)?;
    if !(1..=9_007_199_254_740_991).contains(&minor)
        || entry != r.get::<_, i64>(0).map_err(sql)?
        || entry_hash != r.get::<_, String>(1).map_err(sql)?
        || currency != r.get::<_, String>(2).map_err(sql)?
        || minor != magnitude
        || crate::idempotency::canonical_payload_hash(&plan)?
            != r.get::<_, String>(7).map_err(sql)?
    {
        return Err(sql("dimension source proof"));
    }
    let mut sums = BTreeMap::<&str, i64>::new();
    let mut seen = BTreeSet::new();
    for a in &allocations {
        if a.amount_minor <= 0
            || a.currency != currency
            || !seen.insert((&a.dimension_id, &a.member_id))
        {
            return Err(sql("allocation facts"));
        }
        let sum = sums.entry(&a.dimension_id).or_default();
        *sum = sum
            .checked_add(a.amount_minor)
            .ok_or_else(|| sql("allocation sum"))?;
    }
    if allocations.is_empty()
        || sums.values().any(|n| *n != minor)
        || rs.next().map_err(sql)?.is_some()
    {
        return Err(sql("dimension allocation sum"));
    }
    Ok(allocations
        .into_iter()
        .map(|a| Dimension {
            dimension_id: a.dimension_id,
            member_id: a.member_id,
            amount_minor: a.amount_minor,
            currency: a.currency,
        })
        .collect())
}
