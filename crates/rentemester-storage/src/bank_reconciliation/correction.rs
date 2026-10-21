//! Explicit evidence supersession. Reversal and replacement accounting must already exist.
use super::*;
use crate::idempotency::{self, Input as ReceiptInput, Operation, Outcome, PrincipalKind};
use std::collections::BTreeSet;

pub(crate) fn current(tx: &Transaction<'_>, bank: i64) -> Result<(String, i64, String), Error> {
    let mut q=tx.prepare("SELECT reconciliation_id,journal_entry_id,journal_entry_no FROM (SELECT 'direct:'||j.id AS reconciliation_id,j.id AS journal_entry_id,j.entry_no AS journal_entry_no FROM journal_entries j WHERE j.source_bank_transaction_id=?1 AND j.reversal_of_entry_id IS NULL UNION ALL SELECT 'append-only:'||l.id,j.id,j.entry_no FROM bank_journal_reconciliation_links l JOIN journal_entries j ON j.id=l.journal_entry_id WHERE l.bank_transaction_id=?1 UNION ALL SELECT 'correction:'||e.id,j.id,j.entry_no FROM bank_reconciliation_correction_events e JOIN journal_entries j ON j.id=e.replacement_journal_entry_id WHERE e.bank_transaction_id=?1) c WHERE NOT EXISTS(SELECT 1 FROM bank_reconciliation_correction_events e WHERE e.supersedes_reconciliation_id=c.reconciliation_id) ORDER BY reconciliation_id LIMIT 2").map_err(invalid)?;
    let rows = q
        .query_map([bank], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if rows.len() != 1 {
        return Err(refused("exactly one current reconciliation required"));
    }
    Ok(rows.into_iter().next().unwrap())
}
/// Full history is company-local and immutable; the plan hash binds every predecessor.
pub fn history(tx: &Transaction<'_>, bank: i64) -> Result<Value, Error> {
    let mut items = vec![];
    let mut q=tx.prepare("SELECT id,entry_hash,entry_no FROM journal_entries WHERE source_bank_transaction_id=?1 AND reversal_of_entry_id IS NULL ORDER BY id").map_err(invalid)?;
    for r in q.query_map([bank],|r|Ok(json!({"kind":"direct","id":r.get::<_,i64>(0)?,"journalHash":r.get::<_,String>(1)?,"entryNo":r.get::<_,String>(2)?}))).map_err(invalid)?{items.push(r.map_err(invalid)?);}
    let mut q=tx.prepare("SELECT id,journal_entry_id,match_method,source_reference,note,created_by,created_at,(SELECT entry_no FROM journal_entries WHERE id=journal_entry_id) FROM bank_journal_reconciliation_links WHERE bank_transaction_id=?1 ORDER BY id").map_err(invalid)?;
    for r in q.query_map([bank],|r|Ok(json!({"kind":"append-only","id":r.get::<_,i64>(0)?,"journalEntryId":r.get::<_,i64>(1)?,"matchMethod":r.get::<_,String>(2)?,"sourceReference":r.get::<_,Option<String>>(3)?,"note":r.get::<_,Option<String>>(4)?,"actor":r.get::<_,String>(5)?,"createdAt":r.get::<_,String>(6)?,"entryNo":r.get::<_,String>(7)?}))).map_err(invalid)?{items.push(r.map_err(invalid)?);}
    let mut q=tx.prepare("SELECT id,supersedes_reconciliation_id,superseded_journal_entry_id,replacement_journal_entry_id,bank_account_no,bank_amount_dkk,replacement_journal_hash,plan_hash,reason,actor,principal_kind,principal_subject_id,created_at,(SELECT entry_no FROM journal_entries WHERE id=replacement_journal_entry_id),(SELECT entry_no FROM journal_entries WHERE id=superseded_journal_entry_id) FROM bank_reconciliation_correction_events WHERE bank_transaction_id=?1 ORDER BY id").map_err(invalid)?;
    let mut rows = q.query([bank]).map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        items.push(json!({"kind":"correction","id":r.get::<_,i64>(0).map_err(invalid)?,"supersedes":r.get::<_,String>(1).map_err(invalid)?,"oldJournalEntryId":r.get::<_,i64>(2).map_err(invalid)?,"replacementJournalEntryId":r.get::<_,i64>(3).map_err(invalid)?,"bankAccountNo":r.get::<_,String>(4).map_err(invalid)?,"bankAmountDkk":wire(&signed_money(r.get_ref(5).map_err(invalid)?)?)?,"replacementJournalHash":r.get::<_,String>(6).map_err(invalid)?,"planHash":r.get::<_,String>(7).map_err(invalid)?,"reason":r.get::<_,String>(8).map_err(invalid)?,"actor":r.get::<_,String>(9).map_err(invalid)?,"principalKind":r.get::<_,String>(10).map_err(invalid)?,"principalSubjectId":r.get::<_,String>(11).map_err(invalid)?,"createdAt":r.get::<_,String>(12).map_err(invalid)?,"replacementEntryNo":r.get::<_,String>(13).map_err(invalid)?,"oldEntryNo":r.get::<_,String>(14).map_err(invalid)?}));
    }
    Ok(json!(items))
}
type Line = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
);
fn lines(tx: &Transaction<'_>, entry: i64, opposite: bool) -> Result<Vec<Line>, Error> {
    let mut q=tx.prepare("SELECT a.account_no,l.debit_amount,l.credit_amount,l.vat_code,l.text,l.currency FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1 ORDER BY l.id").map_err(invalid)?;
    let mut rows = q.query([entry]).map_err(invalid)?;
    let mut out = vec![];
    while let Some(r) = rows.next().map_err(invalid)? {
        let debit = legacy_money(r.get_ref(1).map_err(invalid)?)?.decimal();
        let credit = legacy_money(r.get_ref(2).map_err(invalid)?)?.decimal();
        let (d, c) = if opposite {
            (credit, debit)
        } else {
            (debit, credit)
        };
        out.push((
            r.get(0).map_err(invalid)?,
            d,
            c,
            r.get(3).map_err(invalid)?,
            r.get(4).map_err(invalid)?,
            r.get(5).map_err(invalid)?,
        ));
    }
    out.sort();
    Ok(out)
}
pub(crate) fn reversal(
    tx: &Transaction<'_>,
    original: i64,
) -> Result<(i64, String, String, String), Error> {
    type Proof = (String, String, Option<i64>, Option<i64>, String);
    let old:Proof=tx.query_row("SELECT status,currency,document_id,source_bank_transaction_id,entry_hash FROM journal_entries WHERE id=?1 AND reversal_of_entry_id IS NULL",[original],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(invalid)?;
    if old.0 != "posted" {
        return Err(refused(
            "linked original must remain posted with a real reversal child",
        ));
    }
    let mut q=tx.prepare("SELECT id,entry_no,entry_hash,status,currency,document_id,source_bank_transaction_id FROM journal_entries WHERE reversal_of_entry_id=?1 ORDER BY id LIMIT 2").map_err(invalid)?;
    let mut rows = q.query([original]).map_err(invalid)?;
    let r = rows.next().map_err(invalid)?.ok_or_else(|| {
        refused("first reverse the linked journal explicitly; no reversal is created here")
    })?;
    let id: i64 = r.get(0).map_err(invalid)?;
    let no: String = r.get(1).map_err(invalid)?;
    let hash: String = r.get(2).map_err(invalid)?;
    if r.get::<_, String>(3).map_err(invalid)? != "reversed"
        || r.get::<_, String>(4).map_err(invalid)? != old.1
        || r.get::<_, Option<i64>>(5).map_err(invalid)? != old.2
        || r.get::<_, Option<i64>>(6).map_err(invalid)? != old.3
    {
        return Err(refused(
            "reversal child identity does not match linked original",
        ));
    }
    if rows.next().map_err(invalid)?.is_some() {
        return Err(refused("multiple reversal children require review"));
    }
    // Native reversals annotate descriptions; compare accounting fields while the
    // complete original and reversal lines remain bound into the plan hash.
    let financial_lines = |rows: Vec<Line>| {
        let mut fields: Vec<_> = rows
            .into_iter()
            .map(|(account, debit, credit, vat, _, currency)| {
                (account, debit, credit, vat, currency)
            })
            .collect();
        fields.sort();
        fields
    };
    let original_lines = financial_lines(lines(tx, original, true)?);
    if original_lines.is_empty() || original_lines != financial_lines(lines(tx, id, false)?) {
        return Err(refused(
            "reversal child must exactly mirror every original journal line",
        ));
    }
    Ok((id, no, hash, old.4))
}
/// No transaction or accounting side effect. Caller verifies the archived statement first.
pub fn plan(tx: &Transaction<'_>, bank_id: i64, replacement: i64) -> Result<Value, Error> {
    crate::validate(tx)?;
    id(bank_id)?;
    id(replacement)?;
    let (reconciliation, old, old_no) = current(tx, bank_id)?;
    if old == replacement {
        return Err(refused(
            "replacement must be a different independently posted journal",
        ));
    }
    reversal(tx, old)?;
    let replacement_plan = super::journal_plan(tx, bank_id, replacement, false)?;
    plan_facts(
        tx,
        bank_id,
        replacement,
        (reconciliation, old, old_no),
        replacement_plan,
        history(tx, bank_id)?,
    )
}
/// Reconstruct one historical correction proof without granting mutation/candidate
/// admission. Audit supplies and separately validates the canonical predecessor prefix.
pub(crate) fn persisted_plan_facts(
    tx: &Transaction<'_>,
    bank_id: i64,
    reconciliation: &str,
    old: i64,
    replacement: i64,
    history: Value,
) -> Result<Value, Error> {
    persisted_facts_mode(
        tx,
        bank_id,
        reconciliation,
        old,
        replacement,
        history,
        false,
    )
}
pub(crate) fn persisted_audit_plan_facts(
    tx: &Transaction<'_>,
    bank_id: i64,
    reconciliation: &str,
    old: i64,
    replacement: i64,
    history: Value,
) -> Result<Value, Error> {
    persisted_facts_mode(tx, bank_id, reconciliation, old, replacement, history, true)
}
fn persisted_facts_mode(
    tx: &Transaction<'_>,
    bank_id: i64,
    reconciliation: &str,
    old: i64,
    replacement: i64,
    history: Value,
    historical: bool,
) -> Result<Value, Error> {
    let old_no: String = tx
        .query_row(
            "SELECT entry_no FROM journal_entries WHERE id=?1",
            [old],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let replacement_plan = if historical {
        super::persisted_audit_journal_facts(tx, bank_id, replacement)?
    } else {
        super::persisted_journal_facts(tx, bank_id, replacement)?
    };
    plan_facts_mode(
        tx,
        bank_id,
        replacement,
        (reconciliation.into(), old, old_no),
        replacement_plan,
        history,
        historical,
    )
}
fn plan_facts(
    tx: &Transaction<'_>,
    bank_id: i64,
    replacement: i64,
    current: (String, i64, String),
    replacement_plan: Value,
    history: Value,
) -> Result<Value, Error> {
    plan_facts_mode(
        tx,
        bank_id,
        replacement,
        current,
        replacement_plan,
        history,
        false,
    )
}
fn plan_facts_mode(
    tx: &Transaction<'_>,
    bank_id: i64,
    replacement: i64,
    current: (String, i64, String),
    replacement_plan: Value,
    history: Value,
    historical: bool,
) -> Result<Value, Error> {
    let (reconciliation, old, old_no) = current;
    if old == replacement {
        return Err(refused("replacement must differ from original"));
    }
    let (reversal_id, reversal_no, reversal_hash, old_hash) = reversal(tx, old)?;
    // Both original and replacement must move this same mapped bank by the statement amount.
    let bank = super::bank_mode(tx, bank_id, historical)?;
    let target = amount(&bank)?;
    let mut old_movement = Money::default();
    let mut q=tx.prepare("SELECT l.debit_amount,l.credit_amount FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1 AND a.account_no=?2 ORDER BY l.id").map_err(invalid)?;
    let mut rows = q.query(params![old, bank.account_no]).map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        old_movement.add(
            &legacy_money(r.get_ref(0).map_err(invalid)?)?
                .subtract(&legacy_money(r.get_ref(1).map_err(invalid)?)?),
        );
    }
    if old_movement != target {
        return Err(refused(
            "current linked journal no longer reconciles to the mapped bank amount",
        ));
    }
    let mut value = json!({"schemaVersion":"rentemester-bank-reconciliation-correction-v1","bankTransactionId":bank_id,"reconciliationId":reconciliation,"currentJournalEntryId":old,"currentJournalEntryNo":old_no,"currentJournalHash":old_hash,"reversalJournalEntryId":reversal_id,"reversalJournalEntryNo":reversal_no,"reversalJournalHash":reversal_hash,"replacementJournalEntryId":replacement,"replacementJournalEntryNo":replacement_plan["journalEntryNo"],"replacementJournalHash":replacement_plan["journalHash"],"bankAccountNo":bank.account_no,"bankAmountDkk":wire(&target)?,"replacementBankMovement":replacement_plan["journalBankMovement"],"sourceHash":bank.source,"transactionHash":bank.hash,"history":history,"replacementEvidence":replacement_plan,"originalLines":lines(tx,old,false)?,"reversalLines":lines(tx,reversal_id,false)?,"replacementLines":lines(tx,replacement,false)?,"effect":"correction_evidence_only_no_posting"});
    value["planHash"] = json!(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&value).map_err(invalid)?)
    ));
    Ok(json!({"ok":true,"plan":value,"errors":[]}))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Apply {
    pub bank_transaction_id: i64,
    pub replacement_journal_entry_id: i64,
    pub expected_reconciliation_id: String,
    pub plan_hash: String,
    pub reason: String,
    pub idempotency_key: String,
    pub confirm: bool,
}
/// Source/original verification includes the expected historical link, even on a known receipt replay.
pub fn source_journals(tx: &Transaction<'_>, input: &Apply) -> Result<Vec<i64>, Error> {
    let mut ids = BTreeSet::from([input.replacement_journal_entry_id]);
    let chain = history(tx, input.bank_transaction_id)?;
    for event in chain.as_array().unwrap() {
        let kind = event["kind"].as_str().unwrap();
        let eid = event["id"].as_i64().unwrap();
        if format!("{kind}:{eid}") == input.expected_reconciliation_id {
            let old = if kind == "direct" {
                eid
            } else if kind == "append-only" {
                event["journalEntryId"].as_i64().unwrap()
            } else {
                event["replacementJournalEntryId"].as_i64().unwrap()
            };
            ids.insert(old);
            let mut q = tx
                .prepare("SELECT id FROM journal_entries WHERE reversal_of_entry_id=?1")
                .map_err(invalid)?;
            for id in q
                .query_map([old], |r| r.get::<_, i64>(0))
                .map_err(invalid)?
            {
                ids.insert(id.map_err(invalid)?);
            }
        }
    }
    Ok(ids.into_iter().collect())
}
pub fn apply(
    tx: &Transaction<'_>,
    input: &Apply,
    subject: &str,
    now_ms: i64,
) -> Result<Value, Error> {
    apply_mode(tx, input, subject, now_ms, PrincipalKind::User, false)
}
pub(crate) fn apply_original(
    tx: &Transaction<'_>,
    input: &Apply,
    subject: &str,
    now_ms: i64,
    kind: PrincipalKind,
) -> Result<Value, Error> {
    apply_mode(tx, input, subject, now_ms, kind, true)
}
fn apply_mode(
    tx: &Transaction<'_>,
    input: &Apply,
    subject: &str,
    now_ms: i64,
    kind: PrincipalKind,
    compatibility: bool,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm
        || input.reason.trim().is_empty()
        || input.reason.trim().chars().count() > 1000
    {
        return Err(refused("explicit confirmation and reason required"));
    }
    let by = format!("user:{subject}");
    let payload = json!({"bankTransactionId":input.bank_transaction_id,"replacementJournalEntryId":input.replacement_journal_entry_id,"expectedReconciliationId":input.expected_reconciliation_id,"planHash":input.plan_hash,"reason":input.reason});
    let outcome = idempotency::execute(
        tx,
        ReceiptInput {
            key: &input.idempotency_key,
            operation: Operation::BankReconciliationCorrectionApply,
            principal_kind: kind,
            subject,
            payload: &payload,
            created_by: &by,
            created_by_program: "rentemester-native-bank-correction",
            now_ms,
        },
        |tx| {
            apply_reviewed_mode(
                tx,
                &ReviewedCorrection {
                    bank_transaction_id: input.bank_transaction_id,
                    replacement_journal_entry_id: input.replacement_journal_entry_id,
                    expected_reconciliation_id: &input.expected_reconciliation_id,
                    plan_hash: &input.plan_hash,
                    reason: &input.reason,
                },
                subject,
                kind,
                compatibility,
            )
        },
    )
    .map_err(|f| f.error)?;
    match outcome {
        Outcome::Accepted {
            mut result,
            receipt,
        } => {
            result["idempotent"] = json!(receipt.replayed);
            result["receipt"] = serde_json::to_value(receipt).map_err(invalid)?;
            Ok(result)
        }
        Outcome::Refused { code } => Err(refused(code)),
    }
}

pub(crate) struct ReviewedCorrection<'a> {
    pub bank_transaction_id: i64,
    pub replacement_journal_entry_id: i64,
    pub expected_reconciliation_id: &'a str,
    pub plan_hash: &'a str,
    pub reason: &'a str,
}
pub(crate) fn apply_reviewed(
    tx: &Transaction<'_>,
    input: &ReviewedCorrection<'_>,
    subject: &str,
) -> Result<Value, Error> {
    apply_reviewed_mode(tx, input, subject, PrincipalKind::User, false)
}
pub(crate) fn apply_reviewed_original(
    tx: &Transaction<'_>,
    input: &ReviewedCorrection<'_>,
    subject: &str,
    kind: PrincipalKind,
) -> Result<Value, Error> {
    apply_reviewed_mode(tx, input, subject, kind, true)
}
fn apply_reviewed_mode(
    tx: &Transaction<'_>,
    input: &ReviewedCorrection<'_>,
    subject: &str,
    kind: PrincipalKind,
    compatibility: bool,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if input.reason.trim().is_empty() || input.reason.trim().chars().count() > 1000 {
        return Err(refused("explicit reason required"));
    }
    let by = format!("user:{subject}");
    if current(tx, input.bank_transaction_id)?.0 != input.expected_reconciliation_id {
        return Err(refused(
            "current reconciliation changed; inspect a new correction plan",
        ));
    }
    let planned = plan(
        tx,
        input.bank_transaction_id,
        input.replacement_journal_entry_id,
    )?;
    let plan = &planned["plan"];
    if plan["planHash"] != input.plan_hash {
        return Err(refused(
            "reviewed correction proof changed; inspect a new plan",
        ));
    }
    let (link_kind, id) = input
        .expected_reconciliation_id
        .split_once(':')
        .ok_or_else(|| refused("invalid reconciliation identity"))?;
    let id: i64 = id.parse().map_err(invalid)?;
    tx.execute("INSERT INTO bank_reconciliation_correction_events(bank_transaction_id,supersedes_kind,supersedes_id,supersedes_reconciliation_id,superseded_journal_entry_id,replacement_journal_entry_id,bank_account_no,bank_amount_dkk,replacement_journal_hash,plan_hash,reason,actor,principal_kind,principal_subject_id,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![input.bank_transaction_id,link_kind,id,input.expected_reconciliation_id,plan["currentJournalEntryId"].as_i64(),input.replacement_journal_entry_id,plan["bankAccountNo"].as_str(),crate::journal_post::money(&Money::parse(&plan["bankAmountDkk"].to_string())?)?,plan["replacementJournalHash"].as_str(),input.plan_hash,input.reason.trim(),by,crate::bank_original::principal_name(kind),subject]).map_err(invalid)?;
    let id = tx.last_insert_rowid();
    let mut audit = json!({"plan":plan,"reason":input.reason.trim(),"effect":"correction_evidence_only_no_posting"});
    if compatibility {
        audit["compatibility"] = crate::bank_original::correction_claims(kind, subject);
    }
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('bank_reconciliation_corrected','bank_transaction',?1,?2,?3)",params![input.bank_transaction_id.to_string(),audit.to_string(),by]).map_err(invalid)?;
    Ok(
        json!({"ok":true,"id":id,"reconciliationId":format!("correction:{id}"),"planHash":input.plan_hash,"idempotent":false,"effect":"correction_evidence_only_no_posting","errors":[]}),
    )
}

#[cfg(test)]
#[path = "correction_tests.rs"]
mod tests;
