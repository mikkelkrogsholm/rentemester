//! Original bank intent adapters over native source/financial proofs. No scorer
//! or posting calculations. Compatibility metadata never replaces native facts.
use crate::{bank_reconciliation as native, idempotency::PrincipalKind, invalid};
use rentemester_engine::Error;
use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
const RULE: &str = "DK-BOOKKEEPING-RECONCILIATION-001";
const NO_NOTE: &str = "Original confirmed link intent; no caller note supplied.";
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Link {
    pub confirm: bool,
    pub bank_transaction_id: i64,
    pub journal_entry_id: i64,
    pub match_method: String,
    #[serde(default, deserialize_with = "text")]
    pub source_reference: Option<String>,
    #[serde(default, deserialize_with = "text")]
    pub note: Option<String>,
}
fn text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExpectedPrincipal {
    pub kind: String,
    pub subject_id: String,
}
pub struct Correct {
    pub input: native::correction::Apply,
    pub expected_principal: Option<ExpectedPrincipal>,
}
#[derive(Clone, Copy)]
pub enum Action<'a> {
    Suggestions(&'a crate::bank_matches::Input),
    Link(&'a Link),
    Plan { bank: i64, replacement: i64 },
    Correct(&'a Correct),
}
pub fn principal_name(kind: PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::User => "user",
        PrincipalKind::ServiceAccount => "service-account",
    }
}
fn valid_subject(s: &str) -> bool {
    !s.is_empty() && s == s.trim() && s.chars().count() <= 256 && !s.chars().any(char::is_control)
}
fn methods(s: &str) -> bool {
    matches!(
        s,
        "exact-date-amount" | "settlement-lag-amount" | "source-reference" | "manual-review"
    )
}
fn optional(s: &Option<String>, max: usize) -> bool {
    s.as_deref()
        .is_none_or(|s| !s.trim().is_empty() && s.len() <= max)
}
fn fail(message: impl Into<String>) -> Error {
    Error::new("bank_original_validation", message)
}
impl Link {
    pub fn validate(&self) -> Result<(), Error> {
        if !self.confirm
            || !(1..=9_007_199_254_740_991).contains(&self.bank_transaction_id)
            || !(1..=9_007_199_254_740_991).contains(&self.journal_entry_id)
            || !methods(&self.match_method)
            || !optional(&self.source_reference, 4096)
            || !optional(&self.note, 1000)
        {
            return Err(fail(
                "confirmed positive bank/journal ids, known matchMethod and nonblank optional metadata required",
            ));
        }
        Ok(())
    }
}
impl ExpectedPrincipal {
    pub fn validate(&self) -> Result<(), Error> {
        if !matches!(self.kind.as_str(), "user" | "service-account")
            || !valid_subject(&self.subject_id)
        {
            return Err(fail(
                "exact expected principal kind and nonblank subjectId required",
            ));
        }
        Ok(())
    }
    pub fn assert_current(&self, kind: PrincipalKind, subject: &str) -> Result<(), Error> {
        self.validate()?;
        if self.kind != principal_name(kind) || self.subject_id != subject {
            return Err(Error::new(
                "access_denied",
                "expected principal does not match authenticated principal",
            ));
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LinkClaims {
    schema_version: String,
    match_method: String,
    source_reference: Option<String>,
    note: Option<String>,
    principal_kind: String,
    principal_subject_id: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CorrectionClaims {
    schema_version: String,
    principal_kind: String,
    principal_subject_id: String,
}
fn proof_error() -> Error {
    Error::new(
        "bank_original_proof_invalid",
        "native bank compatibility proof differs",
    )
}
pub(crate) fn correction_claims(kind: PrincipalKind, subject: &str) -> Value {
    json!({"schemaVersion":"rentemester-original-bank-correction/1","principalKind":principal_name(kind),"principalSubjectId":subject})
}
/// Existing v1 audit objects remain byte-for-byte structurally strict. New
/// claims bind a concrete authenticated kind/id, never an arbitrary actor prefix.
pub(crate) fn verify_link_audit(
    facts: &Value,
    note: &str,
    actor: &str,
    audit: &Value,
) -> Result<(), Error> {
    let mut expected = json!({"plan":facts,"note":note,"effect":"link_only_no_posting"});
    if let Some(value) = audit.get("compatibility") {
        let claims: LinkClaims =
            serde_json::from_value(value.clone()).map_err(|_| proof_error())?;
        if claims.schema_version != "rentemester-original-bank-link/1"
            || !methods(&claims.match_method)
            || !optional(&claims.source_reference, 4096)
            || !optional(&claims.note, 1000)
            || !matches!(claims.principal_kind.as_str(), "user" | "service-account")
            || !valid_subject(&claims.principal_subject_id)
            || actor != format!("user:{}", claims.principal_subject_id)
            || note != claims.note.as_deref().map(str::trim).unwrap_or(NO_NOTE)
        {
            return Err(proof_error());
        }
        expected["compatibility"] = serde_json::to_value(claims).map_err(invalid)?;
    } else if !actor.strip_prefix("user:").is_some_and(|s| !s.is_empty()) {
        return Err(proof_error());
    }
    if expected != *audit {
        return Err(proof_error());
    }
    Ok(())
}
pub(crate) fn verify_correction_audit(
    plan: &Value,
    reason: &str,
    actor: &str,
    kind: &str,
    subject: &str,
    audit: &Value,
) -> Result<(), Error> {
    let mut expected =
        json!({"plan":plan,"reason":reason,"effect":"correction_evidence_only_no_posting"});
    if let Some(value) = audit.get("compatibility") {
        let claims: CorrectionClaims =
            serde_json::from_value(value.clone()).map_err(|_| proof_error())?;
        if claims.schema_version != "rentemester-original-bank-correction/1"
            || claims.principal_kind != kind
            || claims.principal_subject_id != subject
            || !matches!(kind, "user" | "service-account")
            || !valid_subject(subject)
            || actor != format!("user:{subject}")
        {
            return Err(proof_error());
        }
        expected["compatibility"] = serde_json::to_value(claims).map_err(invalid)?;
    } else if kind != "user" || actor != format!("user:{subject}") {
        return Err(proof_error());
    }
    if expected != *audit {
        return Err(proof_error());
    }
    Ok(())
}
fn link_receipt(input: &Link, id: i64, planned: &Value, replayed: bool) -> Value {
    envelope(
        json!({"ok":true,"id":id,"idempotent":replayed,"bankTransactionId":input.bank_transaction_id,"journalEntryId":input.journal_entry_id,"journalEntryNo":planned["journalEntryNo"],"linkKind":"append-only","bankAmount":planned["bankAmountDkk"],"journalBankMovement":planned["journalBankMovement"],"errors":[]}),
        true,
    )
}
pub fn envelope(mut result: Value, rule: bool) -> Value {
    let ok = result["ok"] == true;
    let errors = result.get("errors").cloned().unwrap_or_else(|| json!([]));
    let mut env = json!({"ok":ok,"errors":errors});
    if rule {
        env["appliedRules"] = json!([RULE]);
    }
    if ok {
        let data = result.as_object_mut().expect("core result object");
        data.remove("ok");
        data.remove("errors");
        data.remove("appliedRules");
        env["data"] = result;
    } else if errors.as_array().is_some_and(|es| {
        es.iter()
            .any(|e| e.as_str().is_some_and(|s| s.contains("does not exist")))
    }) {
        env["code"] = json!("NOT_FOUND");
    }
    env
}
pub fn link(
    tx: &Transaction<'_>,
    input: &Link,
    subject: &str,
    kind: PrincipalKind,
) -> Result<Value, Error> {
    input.validate()?;
    crate::journal_post::require_current(tx)?;
    let mut q=tx.prepare("SELECT reconciliation_id,journal_entry_id FROM bank_journal_reconciliations WHERE bank_transaction_id=?1").map_err(invalid)?;
    let links = q
        .query_map([input.bank_transaction_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if !links.is_empty() {
        if links.len() != 1
            || links[0].1 != input.journal_entry_id
            || !links[0].0.starts_with("append-only:")
        {
            return Err(Error::new(
                "bank_original_link_conflict",
                "bank transaction already has a different reconciliation; explicit correction required",
            ));
        }
        let reversed = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM journal_entries WHERE reversal_of_entry_id=?1)",
                [input.journal_entry_id],
                |r| r.get::<_, bool>(0),
            )
            .map_err(invalid)?;
        if reversed {
            return Err(Error::new(
                "bank_original_link_conflict",
                "linked journal was reversed; explicit correction required",
            ));
        }
        let facts =
            native::persisted_journal_facts(tx, input.bank_transaction_id, input.journal_entry_id)?;
        let (id,method,reference,note,actor,program):(i64,String,String,String,String,String)=tx.query_row("SELECT id,match_method,source_reference,note,created_by,created_by_program FROM bank_journal_reconciliation_links WHERE bank_transaction_id=?1 AND journal_entry_id=?2",params![input.bank_transaction_id,input.journal_entry_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(invalid)?;
        if method != "manual-review"
            || program != "rentemester-native-bank-link"
            || reference
                != format!(
                    "native-plan:{}",
                    facts["planHash"].as_str().ok_or_else(proof_error)?
                )
        {
            return Err(proof_error());
        }
        let mut audits=tx.prepare("SELECT message,actor FROM audit_log WHERE event_type='bank_journal_reconciliation_link' AND entity_type='bank_transaction' AND entity_id=?1").map_err(invalid)?;
        let mut matches = 0;
        for row in audits
            .query_map([input.bank_transaction_id.to_string()], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(invalid)?
        {
            let (raw, by) = row.map_err(invalid)?;
            let value: Value = serde_json::from_str(&raw).map_err(invalid)?;
            if value["plan"]["planHash"] == facts["planHash"] {
                if by != actor {
                    return Err(proof_error());
                }
                verify_link_audit(&facts, &note, &actor, &value)?;
                matches += 1;
            }
        }
        if matches != 1 {
            return Err(proof_error());
        }
        return Ok(link_receipt(input, id, &facts, true));
    }
    let planned =
        native::journal_plan(tx, input.bank_transaction_id, input.journal_entry_id, true)?;
    let claims = json!({"schemaVersion":"rentemester-original-bank-link/1","matchMethod":input.match_method,"sourceReference":input.source_reference,"note":input.note,"principalKind":principal_name(kind),"principalSubjectId":subject});
    let native_input = native::Apply {
        bank_transaction_id: input.bank_transaction_id,
        journal_entry_id: input.journal_entry_id,
        plan_hash: planned["planHash"].as_str().ok_or_else(proof_error)?.into(),
        note: input.note.as_deref().unwrap_or(NO_NOTE).to_owned(),
        confirm: input.confirm,
    };
    let result = native::apply_claimed(tx, &native_input, subject, Some(claims))?;
    let ids=tx.prepare("SELECT id FROM exceptions WHERE type='UNMATCHED_BANK_TRANSACTION' AND status='open' AND related_bank_transaction_id=?1 ORDER BY id").map_err(invalid)?.query_map([input.bank_transaction_id],|r|r.get::<_,i64>(0)).map_err(invalid)?.collect::<Result<Vec<_>,_>>().map_err(invalid)?;
    for id in ids {
        crate::exceptions::resolve(
            tx,
            id,
            &crate::exceptions::Resolve {
                note: Some(
                    "Resolved by confirmed existing-journal bank reconciliation; no new posting."
                        .into(),
                ),
            },
            subject,
        )?;
    }
    Ok(link_receipt(
        input,
        result["id"].as_i64().ok_or_else(proof_error)?,
        &planned,
        false,
    ))
}
pub fn correct(
    tx: &Transaction<'_>,
    input: &Correct,
    subject: &str,
    kind: PrincipalKind,
    now: i64,
) -> Result<Value, Error> {
    if let Some(expected) = &input.expected_principal {
        expected.assert_current(kind, subject)?;
    }
    Ok(envelope(
        native::correction::apply_original(tx, &input.input, subject, now, kind)?,
        false,
    ))
}
/// Full persisted journal hashes are required before replay or new linking.
pub fn verify_journals(tx: &Transaction<'_>) -> Result<(), Error> {
    if !crate::journal_hash::verify(tx)?.errors.is_empty() {
        return Err(Error::new(
            "bank_original_source_invalid",
            "Current journal hash or source lines differ",
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests;
