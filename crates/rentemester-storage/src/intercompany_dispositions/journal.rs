use super::*;
pub(super) fn snapshot<'a>(
    snapshots: &'a [JournalSnapshot],
    company: &str,
) -> Result<&'a JournalSnapshot, Error> {
    let mut found = snapshots.iter().filter(|s| s.company_slug == company);
    let s = found
        .next()
        .ok_or_else(|| refused("admitted company snapshot unavailable"))?;
    if found.next().is_some() {
        return Err(refused("duplicate admitted company snapshot"));
    }
    Ok(s)
}
pub(super) fn matching(
    d: &Disposition,
    side: &Side,
    s: &JournalSnapshot,
    e: &JournalEntry,
    current: bool,
) -> Result<(), Error> {
    if s.currency != d.currency
        || e.transaction_date != d.economic_date
        || !hex(&e.entry_hash)
        || e.status != "posted"
        || (current && (e.reversed || e.reversal_of_entry_id.is_some()))
    {
        return Err(refused(
            "journal identity/date/currency/active evidence differs",
        ));
    }
    let amount = d.amount_money()?;
    if !e.lines.iter().any(|l| {
        if side.expected_side == "receivable" {
            l.debit == amount
        } else {
            l.credit == amount
        }
    }) {
        return Err(refused(
            "journal does not contain exact expected side amount",
        ));
    }
    Ok(())
}
pub(super) fn current_links(
    s: &history::State,
    snapshots: &[JournalSnapshot],
) -> Result<(), Error> {
    for l in &s.links {
        let snap = snapshot(snapshots, l["company_slug"].as_str().unwrap())?;
        let entry = snap
            .entries
            .iter()
            .find(|e| Some(e.id) == l["journal_entry_id"].as_i64())
            .ok_or_else(|| refused("linked journal missing"))?;
        let side = if l["side"] == "left" {
            &s.disposition.left
        } else {
            &s.disposition.right
        };
        matching(&s.disposition, side, snap, entry, true)?;
        if entry.entry_hash != l["journal_entry_hash"]
            || json!(snap.ledger_head_hash) != l["ledger_head_hash"]
        {
            return Err(refused("linked journal/head changed; review required"));
        }
    }
    Ok(())
}
pub(super) fn candidates(d: &Disposition, snapshots: &[JournalSnapshot]) -> Result<Value, Error> {
    let mut value = json!({"left":[],"right":[]});
    for (name, side) in [("left", &d.left), ("right", &d.right)] {
        let s = snapshot(snapshots, &side.company_slug)?;
        let mut items = vec![];
        let mut seen = BTreeSet::new();
        for e in &s.entries {
            if !seen.insert(e.id) {
                return Err(refused("duplicate journal in admitted snapshot"));
            }
            if matching(d, side, s, e, true).is_ok() {
                items.push(json!({"journalEntryId":e.id,"journalEntryNo":e.entry_no,"transactionDate":e.transaction_date,"currency":s.currency,"amount":d.amount,"expectedSide":side.expected_side,"journalEntryHash":e.entry_hash,"ledgerHeadHash":s.ledger_head_hash}));
            }
        }
        if items.len() > 200 {
            return Err(refused(
                "more than 200 matching journals; narrow source evidence",
            ));
        }
        value[name] = json!(items);
    }
    Ok(value)
}
pub(super) fn status(
    s: &history::State,
    snapshots: &[JournalSnapshot],
    as_of: &str,
) -> Result<Value, Error> {
    let mut value = s.value();
    let mut exceptions = vec![];
    for l in &s.links {
        let snap = snapshot(snapshots, l["company_slug"].as_str().unwrap())?;
        let side = if l["side"] == "left" {
            &s.disposition.left
        } else {
            &s.disposition.right
        };
        let valid = snap
            .entries
            .iter()
            .find(|e| Some(e.id) == l["journal_entry_id"].as_i64())
            .is_some_and(|e| {
                matching(&s.disposition, side, snap, e, true).is_ok()
                    && e.entry_hash == l["journal_entry_hash"]
                    && json!(snap.ledger_head_hash) == l["ledger_head_hash"]
            });
        if !valid {
            exceptions.push(
                json!({"kind":"stale_or_reversed_journal","side":l["side"],"waivable":false}),
            );
        }
    }
    if s.status == "partly_posted" {
        exceptions
            .push(json!({"kind":"one_sided_posting","side":s.links[0]["side"],"waivable":false}));
    }
    if s.status != "settled"
        && s.disposition
            .settlement_due_date
            .as_ref()
            .is_some_and(|d| d.as_str() < as_of)
    {
        exceptions.push(json!({"kind":"overdue_settlement","dueDate":s.disposition.settlement_due_date,"waivable":false}));
    }
    value["balanced"] = json!(s.status == "posted" && exceptions.is_empty());
    value["exceptions"] = json!(exceptions);
    value["scope"] = json!("intercompany-disposition-status");
    value["journalCandidates"] = candidates(&s.disposition, snapshots)?;
    Ok(value)
}
