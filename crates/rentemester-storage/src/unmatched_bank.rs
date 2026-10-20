//! Review-only bank tasks, refreshed inside admitted bank imports and document uploads.
use crate::{invalid, legacy_money};
use rentemester_engine::{Error, money::Money};
use rusqlite::{Transaction, params};
use serde_json::json;

const ACTOR: &str = "system:sync-unmatched-bank-transactions";
const KIND: &str = "UNMATCHED_BANK_TRANSACTION";
struct Bank {
    id: i64,
    date: String,
    booking_date: Option<String>,
    text: Option<String>,
    amount: Money,
    currency: String,
    reference: Option<String>,
    batch: String,
}
struct Candidate {
    id: i64,
    number: Option<String>,
    supplier: Option<String>,
}
fn candidate(tx: &Transaction<'_>, bank: &Bank) -> Result<Option<Candidate>, Error> {
    if !bank.amount.is_negative() {
        return Ok(None);
    }
    let absolute = Money::default().subtract(&bank.amount);
    let mut query = tx.prepare("SELECT d.id,d.document_no,d.supplier_name,d.amount_inc_vat FROM documents d WHERE d.document_type IN ('purchase_sale','cash_register_receipt') AND d.amount_inc_vat IS NOT NULL AND UPPER(TRIM(COALESCE(d.currency,'DKK')))=?1 AND NOT EXISTS(SELECT 1 FROM journal_entries j WHERE j.document_id=d.id AND j.status IN ('posted','reversed')) ORDER BY d.id").map_err(invalid)?;
    let mut rows = query.query([&bank.currency]).map_err(invalid)?;
    let mut found = None;
    while let Some(row) = rows.next().map_err(invalid)? {
        if legacy_money(row.get_ref(3).map_err(invalid)?)? != absolute {
            continue;
        }
        if found.is_some() {
            return Ok(None);
        }
        found = Some(Candidate {
            id: row.get(0).map_err(invalid)?,
            number: row.get(1).map_err(invalid)?,
            supplier: row.get(2).map_err(invalid)?,
        });
    }
    Ok(found)
}
fn formatted(amount: &Money, currency: &str) -> String {
    let absolute = if amount.is_negative() {
        Money::default().subtract(amount)
    } else {
        amount.clone()
    };
    let decimal = absolute.decimal();
    let (whole, fraction) = decimal.split_once('.').unwrap();
    let grouped = whole
        .chars()
        .rev()
        .collect::<Vec<_>>()
        .chunks(3)
        .map(|part| part.iter().rev().collect::<String>())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(".");
    format!(
        "{grouped},{fraction} {}",
        if currency == "DKK" { "kr." } else { currency }
    )
}
fn describe(bank: &Bank, candidate: Option<&Candidate>) -> (String, String) {
    let incoming = !bank.amount.is_negative() && bank.amount != Money::default();
    let text = bank
        .text
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("(ingen tekst)");
    let prefix = format!(
        "{} \"{text}\" den {} på {} er endnu ikke bogført.",
        if incoming {
            "Indbetalingen"
        } else {
            "Banktransaktionen"
        },
        bank.date,
        formatted(&bank.amount, &bank.currency)
    );
    if let Some(candidate) = candidate {
        let name = candidate
            .number
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|number| format!("bilag {number}"))
            .unwrap_or_else(|| format!("bilag med id {}", candidate.id));
        let supplier = candidate
            .supplier
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|name| format!(" fra {name}"))
            .unwrap_or_default();
        return (
            format!(
                "{prefix} Et muligt {name}{supplier} har samme beløb og valuta, men er endnu ikke bogført mod betalingen. Beløbet alene beviser ikke en sammenhæng."
            ),
            format!(
                "Kontrollér, at {name} hører til betalingen, og gennemgå bilaget, før du bogfører udgiften mod banktransaktionen."
            ),
        );
    }
    if incoming {
        (format!("{prefix} Der er ikke fundet et entydigt bilag til indbetalingen."),"Find fakturaen eller anden dokumentation for indbetalingen, kontrollér sammenhængen, og bogfør indtægten mod banktransaktionen.".into())
    } else {
        (format!("{prefix} Der er ikke fundet et entydigt bilag til betalingen."),"Find kvitteringen eller fakturaen for betalingen, kontrollér sammenhængen, og gennemgå bilaget, før du bogfører udgiften mod banktransaktionen.".into())
    }
}
/// Returns the number of newly appended tasks, including replacement tasks.
/// No journal, bank row or original is created or changed by synchronization.
pub fn sync(tx: &Transaction<'_>) -> Result<usize, Error> {
    crate::journal_post::require_current(tx)?;
    let mut query = tx.prepare("SELECT b.id,b.transaction_date,b.booking_date,b.text,b.amount,b.currency,b.reference,b.import_batch_id FROM bank_transactions b WHERE NOT EXISTS(SELECT 1 FROM bank_journal_reconciliations r WHERE r.bank_transaction_id=b.id) ORDER BY b.transaction_date,b.id").map_err(invalid)?;
    let mut rows = query.query([]).map_err(invalid)?;
    let mut banks = vec![];
    while let Some(row) = rows.next().map_err(invalid)? {
        banks.push(Bank {
            id: row.get(0).map_err(invalid)?,
            date: row.get(1).map_err(invalid)?,
            booking_date: row.get(2).map_err(invalid)?,
            text: row.get(3).map_err(invalid)?,
            amount: crate::bank::signed_money(row.get_ref(4).map_err(invalid)?)?,
            currency: row.get(5).map_err(invalid)?,
            reference: row.get(6).map_err(invalid)?,
            batch: row.get(7).map_err(invalid)?,
        });
    }
    let mut created = 0;
    for bank in banks {
        let candidate = candidate(tx, &bank)?;
        let (message, action) = describe(&bank, candidate.as_ref());
        let mut open = tx.prepare("SELECT id,message,required_action FROM exceptions WHERE type=?1 AND status='open' AND related_bank_transaction_id=?2 ORDER BY id").map_err(invalid)?;
        let existing = open
            .query_map(params![KIND, bank.id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(invalid)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(invalid)?;
        if existing.len() == 1
            && existing[0].1 == message
            && existing[0].2.as_deref() == Some(action.as_str())
        {
            continue;
        }
        for (id, _, _) in existing {
            tx.execute("UPDATE exceptions SET status='resolved',resolved_at=CURRENT_TIMESTAMP,resolved_by=?1,resolution_note=?2 WHERE id=?3 AND status='open'",params![ACTOR,"Erstattet automatisk: bankpostens beskrivelse og mulige bilag er opdateret.",id]).map_err(invalid)?;
            tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('exception_resolve','exception',?1,?2,?3)",params![id.to_string(),"Erstattet automatisk ved opdatering af bankpostens bilagsvejledning; ingen bogføring.",ACTOR]).map_err(invalid)?;
        }
        let evidence = json!({"ruleId":"DK-BOOKKEEPING-BANK-IMPORT-001","bankTransactionId":bank.id,"transactionDate":bank.date,"bookingDate":bank.booking_date,"text":bank.text,"amount":crate::journal_post::money(&bank.amount)?,"currency":bank.currency,"reference":bank.reference,"importBatchId":bank.batch,"candidateBilagId":candidate.as_ref().map(|v|v.id),"candidateBilagNo":candidate.as_ref().and_then(|v|v.number.as_deref()),"matchBasis":"amount_and_currency_only_requires_review"});
        let preview =
            json!({"bankTransactionId":bank.id,"candidateBilagId":candidate.as_ref().map(|v|v.id)});
        tx.execute("INSERT INTO exceptions(type,severity,status,related_bank_transaction_id,related_document_id,message,required_action,source_evidence,posting_preview) VALUES(?1,'medium','open',?2,?3,?4,?5,?6,?7)",params![KIND,bank.id,candidate.as_ref().map(|v|v.id),message,action,evidence.to_string(),preview.to_string()]).map_err(invalid)?;
        let id = tx.last_insert_rowid();
        tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('exception_record','exception',?1,?2,?3)",params![id.to_string(),evidence.to_string(),ACTOR]).map_err(invalid)?;
        created += 1;
    }
    Ok(created)
}
#[cfg(test)]
mod tests;
