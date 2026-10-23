//! Durable keyed receipts inside an admitted caller-owned write transaction.
//! Every Err requires whole-transaction rollback. No authentication or commit authority.
use rentemester_engine::Error;
use rusqlite::{Connection, MAIN_DB, Transaction, TransactionState, params};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_BYTES: usize = 65_536;
const SAFE_ID: i64 = 9_007_199_254_740_991;
const RETENTION_MS: i64 = 30 * 86_400_000;
#[derive(Clone, Copy)]
pub enum Operation {
    OpeningBalancePost,
    ClosedHistoryArchiveImport,
    DineroImport,
    JournalPost,
    AssetDispose,
    JournalReverse,
    ExpenseBook,
    PayableRegister,
    PayablePay,
    BankReconciliationCorrectionApply,
    DirectBankPurchasePayableCorrectionApply,
}
impl Operation {
    pub fn name(self) -> &'static str {
        match self {
            Self::OpeningBalancePost => "opening_balance_post",
            Self::ClosedHistoryArchiveImport => "closed_history_archive_import",
            Self::DineroImport => "dinero_import",
            Self::JournalPost => "journal_post",
            Self::AssetDispose => "asset_dispose",
            Self::JournalReverse => "journal_reverse",
            Self::ExpenseBook => "expense_book",
            Self::PayableRegister => "payable_register",
            Self::PayablePay => "payable_pay",
            Self::BankReconciliationCorrectionApply => "bank_reconciliation_correction_apply",
            Self::DirectBankPurchasePayableCorrectionApply => {
                "direct_bank_purchase_payable_correction_apply"
            }
        }
    }
}
#[derive(Clone, Copy)]
pub enum PrincipalKind {
    User,
    ServiceAccount,
}
impl PrincipalKind {
    fn name(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::ServiceAccount => "service-account",
        }
    }
}
pub struct Input<'a> {
    pub key: &'a str,
    pub operation: Operation,
    pub principal_kind: PrincipalKind,
    pub subject: &'a str,
    pub payload: &'a Value,
    pub created_by: &'a str,
    pub created_by_program: &'a str,
    pub now_ms: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub replayed: bool,
    pub receipt_id: i64,
    pub created_at: String,
    pub expires_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconciliation: Option<Value>,
}
pub enum Outcome {
    /// Uncommitted. Release result/receipt only after the caller's known commit.
    Accepted { result: Value, receipt: Receipt },
    /// Only the diagnostic audit was written. Caller may commit it or roll it back.
    Refused { code: &'static str },
}
pub struct Failure {
    pub error: Error,
    /// Result-shaped business rejection, retained only for the authorized caller.
    pub rejection: Option<Value>,
}
impl std::fmt::Debug for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Failure")
            .field("code", &self.error.code)
            .field("has_rejection", &self.rejection.is_some())
            .finish()
    }
}
impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self {
            error,
            rejection: None,
        }
    }
}
fn storage(_: impl std::fmt::Display) -> Error {
    Error::new(
        "IDEMPOTENCY_STORAGE_FAILURE",
        "idempotency storage refused; abort caller transaction",
    )
}
pub(crate) fn writer(tx: &Transaction<'_>) -> Result<(), Error> {
    if tx.transaction_state(Some(MAIN_DB)).map_err(storage)? != TransactionState::Write {
        return Err(Error::new(
            "transaction_required",
            "idempotency requires admitted main write transaction",
        ));
    }
    Ok(())
}
fn hash(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
pub(crate) fn whitespace(c: char) -> bool {
    matches!(c,'\u{9}'..='\u{d}'|'\u{20}'|'\u{a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
pub fn validate_key(key: &str) -> Result<(), Error> {
    if key.encode_utf16().count() > 128 || key.trim_matches(whitespace).is_empty() {
        return Err(storage("key"));
    }
    Ok(())
}
fn push(out: &mut String, s: &str) -> Result<(), Error> {
    if out
        .len()
        .checked_add(s.len())
        .filter(|n| *n <= MAX_BYTES)
        .is_none()
    {
        return Err(storage("size"));
    }
    out.push_str(s);
    Ok(())
}
fn canonical(v: &Value, out: &mut String, depth: usize) -> Result<(), Error> {
    if depth > 64 {
        return Err(storage("depth"));
    }
    match v {
        Value::Null => push(out, "null"),
        Value::Bool(v) => push(out, if *v { "true" } else { "false" }),
        Value::String(v) => {
            if v.len() > MAX_BYTES {
                return Err(storage("string size"));
            }
            push(out, &serde_json::to_string(v).map_err(storage)?)
        }
        Value::Number(v) => {
            let n = v
                .as_f64()
                .filter(|n| n.is_finite())
                .ok_or_else(|| storage("number"))?;
            if n.fract() == 0.0 && n.abs() > SAFE_ID as f64 {
                return Err(storage("unsafe integer"));
            }
            push(out, ryu_js::Buffer::new().format_finite(n))
        }
        Value::Array(items) => {
            push(out, "[")?;
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    push(out, ",")?;
                }
                canonical(v, out, depth + 1)?;
            }
            push(out, "]")
        }
        Value::Object(object) => {
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            push(out, "{")?;
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    push(out, ",")?;
                }
                push(out, &serde_json::to_string(key).map_err(storage)?)?;
                push(out, ":")?;
                canonical(&object[*key], out, depth + 1)?;
            }
            push(out, "}")
        }
    }
}
pub fn canonical_payload_hash(payload: &Value) -> Result<String, Error> {
    if !payload.is_object() {
        return Err(storage("business root"));
    }
    let mut out = String::new();
    canonical(payload, &mut out, 0)?;
    Ok(hash(out.as_bytes()))
}
fn hex(s: &str, n: usize) -> bool {
    s.len() == n
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn iso(ms: i64) -> Result<String, Error> {
    let t = time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
        .map_err(storage)?;
    if !(1..=9999).contains(&t.year()) {
        return Err(storage("clock"));
    }
    Ok(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
        t.millisecond()
    ))
}
fn dates(created: &str, expires: &str) -> Result<(), Error> {
    let c = crate::cookie_session::timestamp(created).map_err(storage)?;
    let e = crate::cookie_session::timestamp(expires).map_err(storage)?;
    if iso(c)? != created || iso(e)? != expires || e.checked_sub(c) != Some(RETENTION_MS) {
        return Err(storage("dates"));
    }
    Ok(())
}
fn next_id(db: &Connection, table: &str) -> Result<i64, Error> {
    // Table is always a static module constant, never request data.
    let (min, max): (Option<i64>, Option<i64>) = db
        .query_row(
            &format!("SELECT MIN(id),MAX(id) FROM main.{table}"),
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    match (min, max) {
        (None, None) => Ok(1),
        (Some(min), Some(max)) if min > 0 && max < SAFE_ID => Ok(max + 1),
        _ => Err(storage("id exhausted")),
    }
}
fn audit(db: &Connection, event: &str, id: i64, operation: &str, actor: &str) -> Result<(), Error> {
    let message = match event {
        "idempotency_original" => format!("Recorded idempotent {operation} outcome"),
        "idempotency_replay" => format!("Replayed durable idempotency outcome for {operation}"),
        "idempotency_conflict" => format!("Rejected conflicting idempotency retry for {operation}"),
        _ => format!("Idempotency outcome expired for {operation}; key remains reserved"),
    };
    let audit_id = next_id(db, "audit_log")?;
    if db.execute("INSERT INTO main.audit_log(id,event_type,entity_type,entity_id,message,actor) VALUES(?1,?2,'idempotency_receipt',?3,?4,?5)",params![audit_id,event,id.to_string(),message,actor]).map_err(storage)?!=1 {return Err(storage("audit"));}
    Ok(())
}
pub(crate) fn outcome_json(v: &Value) -> Result<String, Error> {
    if v.get("ok") == Some(&Value::Bool(false)) {
        return Err(storage("rejected stored outcome"));
    }
    let mut bounded = String::new();
    canonical(v, &mut bounded, 0)?;
    Ok(bounded)
}
/// Trusted synchronous callback only: never await, commit, rollback, open another DB or
/// escape the transaction. This function never commits. Propagate EVERY Err to owner rollback.
pub fn execute(
    tx: &Transaction<'_>,
    input: Input<'_>,
    apply: impl FnOnce(&Transaction<'_>) -> Result<Value, Error>,
) -> Result<Outcome, Failure> {
    writer(tx)?;
    validate_key(input.key)?;
    if input.subject.is_empty() || input.subject.chars().count() > 256 {
        return Err(Error::new(
            "IDEMPOTENCY_AUTH_REQUIRED",
            "authenticated stable principal required",
        )
        .into());
    }
    let created_by = input.created_by.trim_matches(whitespace);
    let program = input.created_by_program.trim_matches(whitespace);
    if created_by.is_empty()
        || input.created_by.len() > 512
        || program.is_empty()
        || input.created_by_program.len() > 256
    {
        return Err(storage("actor").into());
    }
    let created = iso(input.now_ms)?;
    let expires = iso(input
        .now_ms
        .checked_add(RETENTION_MS)
        .ok_or_else(|| storage("clock"))?)?;
    let payload = canonical_payload_hash(input.payload)?;
    let actor = if created_by == "system" && program == "system" {
        "system".to_owned()
    } else {
        format!("{created_by} via {program}")
    };
    let mut identities = tx
        .prepare("SELECT id,ledger_uuid FROM main.ledger_identity LIMIT 2")
        .map_err(storage)?;
    let identity = identities
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage)?;
    let ledger = match identity.as_slice() {
        [(1, id)] if hex(id, 32) => id,
        _ => return Err(storage("identity").into()),
    };
    let key = hash(input.key.as_bytes());
    let op = input.operation.name();
    // A credential-free restore preserves old receipts but creates new live
    // principals. Never reinterpret an imported key as a new executable intent.
    let imported = crate::receipt_reconciliation::imported_ids(tx, &key, ledger, op)?;
    if !imported.is_empty() {
        if let [id] = imported.as_slice()
            && matches!(input.principal_kind, PrincipalKind::User)
            && let Some((result, provenance)) =
                crate::receipt_reconciliation::mapped_replay(tx, *id, input.subject, &payload)?
        {
            let (c, e): (String, String) = tx
                .query_row(
                    "SELECT created_at,expires_at FROM mutation_idempotency_tombstones WHERE id=?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(storage)?;
            dates(&c, &e)?;
            audit(tx, "idempotency_replay", *id, op, &actor)?;
            return Ok(Outcome::Accepted {
                result,
                receipt: Receipt {
                    replayed: true,
                    receipt_id: *id,
                    created_at: c,
                    expires_at: e,
                    reconciliation: Some(provenance),
                },
            });
        }
        return Err(Error::new("RESTORED_KEY_RECONCILIATION_REQUIRED","Imported request key already belongs to source history; reconcile the original accepted outcome before any new intended operation").into());
    }

    let mut q=tx.prepare("SELECT t.id,t.payload_hash,t.created_at,t.expires_at,o.outcome_json,o.created_at FROM main.mutation_idempotency_tombstones t LEFT JOIN main.mutation_idempotency_outcomes o ON o.tombstone_id=t.id WHERE t.client_key_hash=?1 AND t.ledger_uuid=?2 AND t.operation=?3 AND t.principal_kind=?4 AND t.principal_subject_id=?5 LIMIT 2").map_err(storage)?;
    let mut rows = q
        .query(params![
            key,
            ledger,
            op,
            input.principal_kind.name(),
            input.subject
        ])
        .map_err(storage)?;
    if let Some(r) = rows.next().map_err(storage)? {
        let id: i64 = r.get(0).map_err(storage)?;
        let prior: String = r.get(1).map_err(storage)?;
        let c: String = r.get(2).map_err(storage)?;
        let e: String = r.get(3).map_err(storage)?;
        let stored: Option<String> = r.get(4).map_err(storage)?;
        let outcome_created: Option<String> = r.get(5).map_err(storage)?;
        if id <= 0 || id > SAFE_ID || !hex(&prior, 64) || rows.next().map_err(storage)?.is_some() {
            return Err(storage("prior identity").into());
        }
        dates(&c, &e)?;
        let refusal = if prior != payload {
            Some(("IDEMPOTENCY_CONFLICT", "idempotency_conflict"))
        } else if stored.is_none() {
            Some(("IDEMPOTENCY_OUTCOME_EXPIRED", "idempotency_outcome_expired"))
        } else {
            None
        };
        if let Some((code, event)) = refusal {
            audit(tx, event, id, op, &actor)?;
            return Ok(Outcome::Refused { code });
        }
        let stored = stored.ok_or_else(|| storage("outcome"))?;
        if stored.len() > MAX_BYTES || outcome_created.as_deref() != Some(&c) {
            return Err(storage("stored outcome").into());
        }
        let result: Value = serde_json::from_str(&stored).map_err(storage)?;
        outcome_json(&result)?;
        audit(tx, "idempotency_replay", id, op, &actor)?;
        return Ok(Outcome::Accepted {
            result,
            receipt: Receipt {
                replayed: true,
                receipt_id: id,
                created_at: c,
                expires_at: e,
                reconciliation: None,
            },
        });
    }
    drop(rows);
    drop(q);
    drop(identities);
    let result = apply(tx)?;
    if result.get("ok") == Some(&Value::Bool(false)) {
        return Err(Failure {
            error: Error::new("business_rejection", "abort rejected mutation transaction"),
            rejection: Some(result),
        });
    }
    writer(tx)?;
    let outcome = outcome_json(&result)?;
    let id = next_id(tx, "mutation_idempotency_tombstones")?;
    if tx.execute("INSERT INTO main.mutation_idempotency_tombstones(id,client_key_hash,ledger_uuid,operation,principal_kind,principal_subject_id,payload_hash,created_at,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id,key,ledger,op,input.principal_kind.name(),input.subject,payload,created,expires]).map_err(storage)?!=1 {return Err(storage("tombstone").into());}
    if tx.execute("INSERT INTO main.mutation_idempotency_outcomes(tombstone_id,outcome_json,created_at) VALUES(?1,?2,?3)",params![id,outcome,created]).map_err(storage)?!=1 {return Err(storage("outcome insert").into());}
    audit(tx, "idempotency_original", id, op, &actor)?;
    Ok(Outcome::Accepted {
        result,
        receipt: Receipt {
            replayed: false,
            receipt_id: id,
            created_at: created,
            expires_at: expires,
            reconciliation: None,
        },
    })
}
/// Delete only replay material. Every error requires caller rollback; no key is ever unreserved.
pub fn prune(tx: &Transaction<'_>, now_ms: i64) -> Result<usize, Error> {
    writer(tx)?;
    let cutoff = iso(now_ms)?;
    let mut q=tx.prepare("SELECT t.created_at,t.expires_at FROM main.mutation_idempotency_tombstones t JOIN main.mutation_idempotency_outcomes o ON o.tombstone_id=t.id").map_err(storage)?;
    let mut rows = q.query([]).map_err(storage)?;
    while let Some(r) = rows.next().map_err(storage)? {
        dates(
            &r.get::<_, String>(0).map_err(storage)?,
            &r.get::<_, String>(1).map_err(storage)?,
        )?;
    }
    drop(rows);
    drop(q);
    tx.execute("DELETE FROM main.mutation_idempotency_outcomes WHERE tombstone_id IN (SELECT id FROM main.mutation_idempotency_tombstones WHERE expires_at<=?1)",[cutoff]).map_err(storage)
}
#[cfg(test)]
mod tests;
