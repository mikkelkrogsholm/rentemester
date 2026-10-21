//! Primitive dated original-currency evidence. Never calls principal_open/validated_claims.
use super::*;
use rentemester_engine::invoice_interest::{
    EventKind, History, InterestClaim, InterestCorrection, PrincipalEvent, RateSource,
};
#[derive(Clone)]
pub(crate) struct Loaded {
    pub no: String,
    pub date: String,
    pub history: History,
}
pub(crate) fn load(tx: &Transaction<'_>, id: i64) -> Result<Loaded, Error> {
    positive(id)?;
    let (no,date,currency,raw,hash):(String,String,String,String,String)=tx.query_row("SELECT invoice_no,invoice_date,currency,payload_json,sha256_hash FROM documents WHERE id=?1 AND document_type='issued_invoice'",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(invalid)?;
    if currency != "DKK" {
        return Err(Error::new(
            "unsupported_foreign_interest",
            "interest actions require a DKK invoice",
        ));
    }
    let p: Value = serde_json::from_str(&raw).map_err(invalid)?;
    if crate::invoice_email::sha(raw.as_bytes()) != hash
        || p["invoiceNumber"] != no
        || p["issueDate"] != date
        || p["currency"] != currency
    {
        return Err(refused("immutable invoice identity/hash differs"));
    }
    crate::audit_invoice::interest_principal_evidence(tx, id)?;
    let due =
        rentemester_engine::reminder_claims::effective_due_date(&date, p["dueDate"].as_str())?;
    let mut history = History {
        currency,
        gross: rentemester_engine::sales_invoice::project(&p)?.gross,
        effective_due_date: Date::parse(&due)?,
        events: vec![],
        claims: vec![],
        corrections: vec![],
    };
    for (table, datecol, amount, kind) in [
        (
            "invoice_payments",
            "payment_date",
            "amount",
            EventKind::Payment,
        ),
        (
            "invoice_refunds",
            "refund_date",
            "amount",
            EventKind::Refund,
        ),
        (
            "invoice_bad_debt_writeoffs",
            "writeoff_date",
            "gross_amount",
            EventKind::WriteOff,
        ),
    ] {
        let sql = format!(
            "SELECT id,{datecol},{amount} FROM {table} WHERE invoice_document_id=?1 ORDER BY id"
        );
        let mut q = tx.prepare(&sql).map_err(invalid)?;
        let mut rs = q.query([id]).map_err(invalid)?;
        while let Some(r) = rs.next().map_err(invalid)? {
            history.events.push(PrincipalEvent {
                source_id: r.get(0).map_err(invalid)?,
                effective_date: Date::parse(&r.get::<_, String>(1).map_err(invalid)?)?,
                amount: legacy_money(r.get_ref(2).map_err(invalid)?)?,
                kind,
            });
        }
    }
    let mut q=tx.prepare("SELECT c.id,c.invoice_date,c.amount_inc_vat FROM credit_note_postings p JOIN documents c ON c.id=p.credit_note_document_id WHERE p.original_invoice_document_id=?1 ORDER BY c.id").map_err(invalid)?;
    let mut rs = q.query([id]).map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        history.events.push(PrincipalEvent {
            source_id: r.get(0).map_err(invalid)?,
            effective_date: Date::parse(&r.get::<_, String>(1).map_err(invalid)?)?,
            amount: legacy_money(r.get_ref(2).map_err(invalid)?)?,
            kind: EventKind::Credit,
        });
    }
    let mut q=tx.prepare("SELECT c.id,c.claim_date,c.reference_rate_percent,c.annual_interest_rate_percent,c.reference_rate_source,c.amount_dkk,p.journal_entry_id,c.overdue_days,c.principal_open_balance FROM invoice_interest_claims c LEFT JOIN invoice_interest_postings p ON p.interest_claim_id=c.id WHERE c.invoice_document_id=?1 ORDER BY c.claim_date,c.id").map_err(invalid)?;
    let mut rs = q.query([id]).map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        let ref_rate = legacy_money(r.get_ref(2).map_err(invalid)?)?;
        let annual = legacy_money(r.get_ref(3).map_err(invalid)?)?;
        let days: i64 = r.get(7).map_err(invalid)?;
        let principal = legacy_money(r.get_ref(8).map_err(invalid)?)?;
        if days <= 0 || principal.is_negative() || principal == Money::default() {
            return Err(refused(
                "positive registered interest principal/window required",
            ));
        }
        history.claims.push(InterestClaim {
            id: r.get(0).map_err(invalid)?,
            claim_date: Date::parse(&r.get::<_, String>(1).map_err(invalid)?)?,
            reference_basis_points: basis(&ref_rate)?,
            annual_basis_points: basis(&annual)?,
            rate_source: match r.get::<_, String>(4).map_err(invalid)?.as_str() {
                "statutory-table" => RateSource::StatutoryTable,
                "manual-override" => RateSource::ManualOverride,
                _ => return Err(refused("unknown frozen reference rate source")),
            },
            amount: legacy_money(r.get_ref(5).map_err(invalid)?)?,
            posted_journal_id: r.get(6).map_err(invalid)?,
        });
    }
    let mut q=tx.prepare("SELECT id,correction_date,amount_dkk,journal_entry_id FROM invoice_interest_corrections WHERE invoice_document_id=?1 ORDER BY journal_entry_id,id").map_err(invalid)?;
    let mut rs = q.query([id]).map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        history.corrections.push(InterestCorrection {
            id: r.get(0).map_err(invalid)?,
            correction_date: Date::parse(&r.get::<_, String>(1).map_err(invalid)?)?,
            amount: legacy_money(r.get_ref(2).map_err(invalid)?)?,
            journal_id: r.get(3).map_err(invalid)?,
        });
    }
    history
        .events
        .sort_by_key(|e| (e.effective_date, e.kind, e.source_id));
    Ok(Loaded { no, date, history })
}
fn event_journal(tx: &Transaction<'_>, event: &PrincipalEvent) -> Result<i64, Error> {
    let sql = match event.kind {
        EventKind::Payment => "SELECT journal_entry_id FROM invoice_payments WHERE id=?1",
        EventKind::Refund => "SELECT journal_entry_id FROM invoice_refunds WHERE id=?1",
        EventKind::Credit => {
            "SELECT journal_entry_id FROM credit_note_postings WHERE credit_note_document_id=?1"
        }
        EventKind::WriteOff => {
            "SELECT journal_entry_id FROM invoice_bad_debt_writeoffs WHERE id=?1"
        }
    };
    tx.query_row(sql, [event.source_id], |r| r.get(0))
        .map_err(invalid)
}
/// Authenticate saved ceilings in the append-only context that existed before
/// the correction journal, not against today's potentially smaller overclaim.
pub(crate) fn verify_historical_plan(tx: &Transaction<'_>, row: &Value) -> Result<(), Error> {
    let id = row["invoice_document_id"]
        .as_i64()
        .ok_or_else(|| refused("correction invoice missing"))?;
    let journal = row["journal_entry_id"]
        .as_i64()
        .ok_or_else(|| refused("correction journal missing"))?;
    let mut h = load(tx, id)?.history;
    h.events = h
        .events
        .into_iter()
        .filter_map(|e| match event_journal(tx, &e) {
            Ok(j) if j < journal => Some(Ok(e)),
            Ok(_) => None,
            Err(e) => Some(Err(e)),
        })
        .collect::<Result<_, _>>()?;
    let cutoff: Option<i64> = tx
        .query_row(
            "SELECT min(id) FROM audit_log WHERE entity_type='journal_entry' AND entity_id=?1",
            [journal.to_string()],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let mut claims = vec![];
    for mut c in h.claims {
        if c.posted_journal_id.is_some_and(|j| j < journal) {
            claims.push(c);
            continue;
        }
        let registered:Option<i64>=tx.query_row("SELECT min(id) FROM audit_log WHERE event_type='invoice_interest_register' AND entity_type='invoice_interest_claim' AND entity_id=?1",[c.id.to_string()],|r|r.get(0)).map_err(invalid)?;
        match (registered, cutoff) {
            (Some(a), Some(b)) if a < b => {
                c.posted_journal_id = None;
                claims.push(c);
            }
            (Some(_), Some(_)) => {}
            _ => {
                return Err(refused(
                    "historical interest registration order cannot be authenticated",
                ));
            }
        }
    }
    h.claims = claims;
    h.corrections.retain(|c| c.journal_id < journal);
    let proposal = engine::correction(&h)?;
    let saved_amount = crate::invoice_fx::amount(&row["amount_dkk"])?;
    if saved_amount.is_negative()
        || saved_amount == Money::default()
        || proposal
            .over_claimed_amount
            .subtract(&saved_amount)
            .is_negative()
    {
        return Err(refused(
            "saved correction exceeds historical remaining global ceiling",
        ));
    }
    let mut q=tx.prepare("SELECT interest_claim_id,claim_ceiling_dkk,amount_dkk FROM invoice_interest_correction_plan_claims WHERE journal_entry_id=?1 ORDER BY interest_claim_id").map_err(invalid)?;
    let mut rs = q.query([journal]).map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        let claim: i64 = r.get(0).map_err(invalid)?;
        let ceiling = legacy_money(r.get_ref(1).map_err(invalid)?)?;
        let amount = legacy_money(r.get_ref(2).map_err(invalid)?)?;
        let authorized = proposal
            .claim_overclaims
            .iter()
            .find(|c| c.claim_id == claim)
            .ok_or_else(|| refused("saved correction lacks historical causal claim"))?;
        if ceiling != authorized.over_claimed_amount || ceiling.subtract(&amount).is_negative() {
            return Err(refused("saved correction historical ceiling differs"));
        }
    }
    Ok(())
}
