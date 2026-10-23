use super::*;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Evidence {
    pub kind: String,
    pub id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Match {
    pub commitment_id: String,
    pub occurrence_date: String,
    pub evidence: Evidence,
    pub confirm: bool,
}
pub fn evidence_document(tx: &Transaction<'_>, input: &Match) -> Result<Option<i64>, Error> {
    let id = input
        .evidence
        .id
        .parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| refused("positive canonical evidence id required"))?;
    match input.evidence.kind.as_str() {
        "canonical_document" => Ok(Some(id)),
        "payable" => tx
            .query_row("SELECT document_id FROM payables WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(invalid)?
            .map(Some)
            .ok_or_else(|| refused("payable evidence not found")),
        "bank_transaction" => Ok(None),
        _ => Err(refused("unsupported canonical evidence kind")),
    }
}
fn canonical_evidence(tx: &Transaction<'_>, input: &Match) -> Result<Value, Error> {
    let id = input
        .evidence
        .id
        .parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| refused("positive canonical evidence id required"))?;
    let document = evidence_document(tx, input)?;
    let (date, amount, currency, source_hash, mut party): (
        String,
        Money,
        String,
        String,
        Option<String>,
    ) = match input.evidence.kind.as_str() {
        "canonical_document" => {
            let row:Option<(String,Money,String,String)>=tx.query_row("SELECT invoice_date,amount_inc_vat,currency,sha256_hash FROM documents WHERE id=?1 AND document_type IN ('purchase_sale','cash_register_receipt')",[id],|r|Ok((r.get(0)?,legacy_money(r.get_ref(1)?).map_err(|_|rusqlite::Error::InvalidQuery)?,r.get(2)?,r.get(3)?))).optional().map_err(invalid)?;
            let (d, a, c, h) =
                row.ok_or_else(|| refused("dated supplier document evidence required"))?;
            (d, a, c, h, None)
        }
        "payable" => {
            let row:Option<(String,Money,String,String,Option<i64>)>=tx.query_row("SELECT p.due_date,p.gross_amount,p.currency,d.sha256_hash,p.vendor_id FROM payables p JOIN documents d ON d.id=p.document_id WHERE p.id=?1",[id],|r|Ok((r.get(0)?,legacy_money(r.get_ref(1)?).map_err(|_|rusqlite::Error::InvalidQuery)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(invalid)?;
            let (d, a, c, h, v) = row.ok_or_else(|| refused("payable evidence required"))?;
            (d, a, c, h, v.map(|n| format!("vendor:{n}")))
        }
        "bank_transaction" => {
            let row:Option<(String,Money,String,String)>=tx.query_row("SELECT transaction_date,amount,currency,transaction_hash FROM bank_transactions WHERE id=?1",[id],|r|Ok((r.get(0)?,crate::bank::signed_money(r.get_ref(1)?).map_err(|_|rusqlite::Error::InvalidQuery)?,r.get(2)?,r.get(3)?))).optional().map_err(invalid)?;
            let (d, a, c, h) = row.ok_or_else(|| refused("bank evidence required"))?;
            if !a.is_negative() {
                return Err(refused("supplier match requires outgoing bank evidence"));
            }
            (d, Money::default().subtract(&a), c, h, None)
        }
        _ => return Err(refused("unsupported evidence kind")),
    };
    Date::parse(&date)?;
    if party.is_none()
        && let Some(doc) = document
    {
        party=tx.query_row("SELECT party_id FROM current_document_party_links WHERE document_id=?1 AND party_role IN ('supplier','vendor') ORDER BY id DESC LIMIT 1",[doc],|r|r.get(0)).optional().map_err(invalid)?;
    }
    Ok(
        json!({"kind":input.evidence.kind,"id":id.to_string(),"date":date,"amount":wire(&amount)?,"currency":currency,"partyId":party,"documented":document.is_some(),"sourceHash":source_hash,"documentId":document}),
    )
}
pub fn match_occurrence(
    tx: &Transaction<'_>,
    input: &Match,
    actor: &str,
    now: &str,
) -> Result<Value, Error> {
    match_identity(tx, input, &format!("user:{actor}"), actor, now)
}
pub fn match_occurrence_principal(
    tx: &Transaction<'_>,
    input: &Match,
    subject: &str,
    kind: crate::idempotency::PrincipalKind,
    now: &str,
) -> Result<Value, Error> {
    let actor = principal_actor(subject, kind)?;
    match_identity(tx, input, &actor, &actor, now)
}
fn match_identity(
    tx: &Transaction<'_>,
    input: &Match,
    actor: &str,
    principal: &str,
    now: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm {
        return Err(refused("match confirmation required"));
    }
    Date::parse(&input.occurrence_date)?;
    let last = latest(tx, &input.commitment_id)?.ok_or_else(|| refused("commitment not found"))?;
    if last["event"] != "approved"
        || last["payload"]["status"].as_str().unwrap_or("active") != "active"
    {
        return Err(refused("active commitment required for matching"));
    }
    let p = &last["payload"];
    let first = p["nextDate"]
        .as_str()
        .ok_or_else(|| refused("missing stored schedule"))?;
    let frequency = p["frequency"]
        .as_str()
        .ok_or_else(|| refused("missing stored cadence"))?;
    let mut found = false;
    let first_index = nearby_index(first, frequency, &input.occurrence_date)?;
    for index in first_index..first_index + 3 {
        let date = occurrence(first, frequency, index)?;
        if date > input.occurrence_date || p["endDate"].as_str().is_some_and(|e| date.as_str() > e)
        {
            break;
        }
        if date == input.occurrence_date {
            found = true;
            break;
        }
    }
    if !found {
        return Err(refused("expected occurrence not found"));
    }
    let evidence = canonical_evidence(tx, input)?;
    let variance = variance(
        p,
        &evidence,
        &input.occurrence_date,
        p["vendorPartyId"].as_str(),
    )?;
    let payload = json!({"commitmentId":input.commitment_id,"occurrenceDate":input.occurrence_date,"expected":{"date":input.occurrence_date,"amount":p["amount"],"currency":p["currency"],"payloadHash":last["payloadHash"]},"evidence":evidence,"variance":variance});
    let digest = hash(&payload);
    let old: Vec<String> = {
        let mut q=tx.prepare("SELECT payload_hash FROM supplier_commitment_occurrence_matches WHERE (commitment_id=?1 AND occurrence_date=?2) OR (evidence_kind=?3 AND evidence_id=?4)").map_err(invalid)?;
        q.query_map(
            params![
                input.commitment_id,
                input.occurrence_date,
                input.evidence.kind,
                evidence["id"].as_str()
            ],
            |r| r.get(0),
        )
        .map_err(invalid)?
        .collect::<Result<_, _>>()
        .map_err(invalid)?
    };
    if !old.is_empty() {
        if old.iter().all(|h| *h == digest) {
            return Ok(json!({"idempotent":true,"payloadHash":digest,"variance":variance}));
        }
        return Err(refused("OCCURRENCE_OR_EVIDENCE_ALREADY_MATCHED"));
    }
    // Prevent the same underlying document from being consumed once as document and again as payable.
    if let Some(doc) = evidence["documentId"].as_i64() {
        let mut q = tx
            .prepare("SELECT canonical_payload FROM supplier_commitment_occurrence_matches")
            .map_err(invalid)?;
        for raw in q
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(invalid)?
        {
            let prior: Value = serde_json::from_str(&raw.map_err(invalid)?).map_err(invalid)?;
            if prior["evidence"]["documentId"] == doc {
                return Err(refused(
                    "source document already consumed by another occurrence",
                ));
            }
        }
    }
    tx.execute("INSERT INTO supplier_commitment_occurrence_matches(commitment_id,occurrence_date,expected_payload_hash,evidence_kind,evidence_id,canonical_payload,payload_hash,actor,principal,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![input.commitment_id,input.occurrence_date,last["payloadHash"].as_str(),input.evidence.kind,evidence["id"].as_str(),canonical(&payload),digest,actor,principal,now]).map_err(invalid)?;
    Ok(json!({"idempotent":false,"payloadHash":digest,"variance":variance}))
}

pub(super) fn variance(
    expected: &Value,
    evidence: &Value,
    occurrence_date: &str,
    party: Option<&str>,
) -> Result<Value, Error> {
    let same_currency = evidence["currency"] == expected["currency"];
    let actual = money(
        evidence["amount"]
            .as_number()
            .ok_or_else(|| refused("invalid evidence amount"))?,
    )?;
    let amount = money(
        expected["amount"]
            .as_number()
            .ok_or_else(|| refused("invalid expected amount"))?,
    )?;
    let days = (rentemester_engine::vat_period::date(
        evidence["date"]
            .as_str()
            .ok_or_else(|| refused("invalid evidence date"))?,
    )? - rentemester_engine::vat_period::date(occurrence_date)?)
    .whole_days();
    Ok(
        json!({"amount":if same_currency{wire(&actual.subtract(&amount))?}else{Value::Null},"dateDays":days,"late":days>0,"currency":{"expected":expected["currency"],"actual":evidence["currency"],"status":if same_currency{"same"}else{"unsupported_fx"}},"party":{"expected":party,"actual":evidence["partyId"],"status":if evidence["partyId"].is_null(){"missing"}else if Some(evidence["partyId"].as_str().unwrap_or(""))==party{"same"}else{"mismatch"}},"documentation":if evidence["documented"]==true{"present"}else{"missing"},"comparability":if same_currency{"native_currency"}else{"unsupported_fx"}}),
    )
}
