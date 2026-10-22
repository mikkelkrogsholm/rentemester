//! Source numeric adaptation and transaction-local original journal helpers.
//! No connection opens, authority grants, independent writer or commits.
use crate::journal_post;
use rentemester_engine::{
    Error, fx::FxRate, identifiers::JournalEntryId, manual_journal::ManualJournal, money::Money,
};
use rusqlite::{Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn invalid() -> Error {
    Error::new(
        "invalid_input",
        "original journal requires source numeric fields and server-owned attribution",
    )
}
fn failed(_: impl std::fmt::Display) -> Error {
    Error::new(
        "original_journal_storage_failure",
        "original journal evidence unavailable; abort transaction",
    )
}
fn numeric(value: &Value) -> Result<String, Error> {
    let n = value
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(invalid)?;
    Ok(ryu_js::Buffer::new().format_finite(n).to_owned())
}
fn id(value: &Value) -> Result<String, Error> {
    let n = value.as_f64().ok_or_else(invalid)?;
    if !n.is_finite() || n.fract() != 0.0 || n.abs() > 9_007_199_254_740_991.0 {
        return Err(invalid());
    }
    Ok((n as i64).to_string())
}
/// Source JS numbers are converted using the same shortest binary64 spelling and
/// exact half-away quantizers as source money.ts, not binary floating arithmetic.
pub fn adapt(mut input: Value) -> Result<ManualJournal, Error> {
    let object = input.as_object_mut().ok_or_else(invalid)?;
    // Explicit identity correction: caller attribution is never trusted, including
    // the historical-import marker. Do not silently discard attempted impersonation.
    if object.contains_key("createdBy") || object.contains_key("createdByProgram") {
        return Err(invalid());
    }
    let currency = object
        .get("currency")
        .filter(|v| !v.is_null())
        .map(|v| v.as_str().ok_or_else(invalid))
        .transpose()?
        .unwrap_or("DKK");
    object.insert(
        "currency".into(),
        json!(journal_post::trim(currency).to_uppercase()),
    );
    for key in ["documentId", "sourceBankTransactionId"] {
        if let Some(value) = object.get_mut(key).filter(|v| !v.is_null()) {
            *value = json!(id(value)?);
        }
    }
    for key in ["amountForeign", "amountDkk"] {
        if let Some(value) = object.get_mut(key).filter(|v| !v.is_null()) {
            *value = json!(Money::parse(&numeric(value)?)?.decimal());
        }
    }
    if let Some(value) = object.get_mut("fxRateToDkk").filter(|v| !v.is_null()) {
        *value = json!(FxRate::parse(&numeric(value)?)?);
    }
    if let Some(lines) = object.get_mut("lines").and_then(Value::as_array_mut) {
        for line in lines {
            let line = line.as_object_mut().ok_or_else(invalid)?;
            for key in ["debitAmount", "creditAmount"] {
                let amount = match line.get(key) {
                    None => Money::default(),
                    Some(v) => Money::parse(&numeric(v)?)?,
                };
                line.insert(key.into(), json!(amount));
            }
        }
    }
    serde_json::from_value(input).map_err(|_| invalid())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selector {
    pub entry_id: Option<Value>,
    pub entry_no: Option<String>,
    pub match_text: Option<String>,
    pub match_date: Option<String>,
    pub match_document_id: Option<Value>,
}
fn selection() -> Error {
    Error::new(
        "journal_selection_usage",
        "missing, ambiguous or unmatched journal selector",
    )
}
/// Called only inside the very same admitted writer as reverse; never a preflight read.
pub fn resolve(tx: &Transaction<'_>, selector: &Selector) -> Result<JournalEntryId, Error> {
    journal_post::require_current(tx)?;
    if let Some(value) = &selector.entry_id {
        if let Ok(value) = id(value).and_then(|n| n.parse::<i64>().map_err(|_| invalid())) {
            if value > 0 {
                let exists: bool = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM main.journal_entries WHERE id=?1)",
                        [value],
                        |r| r.get(0),
                    )
                    .map_err(failed)?;
                if !exists {
                    return Err(selection());
                }
                return Ok(JournalEntryId::from_sqlite(value));
            }
        }
    }
    if let Some(number) = selector
        .entry_no
        .as_deref()
        .map(journal_post::trim)
        .filter(|s| !s.is_empty())
    {
        let mut query = tx
            .prepare("SELECT id FROM main.journal_entries WHERE entry_no=?1 LIMIT 1")
            .map_err(failed)?;
        let mut rows = query.query([number]).map_err(failed)?;
        return rows
            .next()
            .map_err(failed)?
            .map(|row| {
                row.get::<_, i64>(0)
                    .map(JournalEntryId::from_sqlite)
                    .map_err(failed)
            })
            .transpose()?
            .ok_or_else(selection);
    }
    let text = selector
        .match_text
        .as_deref()
        .map(journal_post::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(selection)?;
    let date = selector
        .match_date
        .as_deref()
        .map(journal_post::trim)
        .filter(|s| !s.is_empty());
    let document = selector
        .match_document_id
        .as_ref()
        .map(|value| {
            let n = id(value)
                .map_err(|_| selection())?
                .parse::<i64>()
                .map_err(|_| selection())?;
            if n <= 0 {
                return Err(selection());
            }
            Ok(n)
        })
        .transpose()?;
    let mut query = tx.prepare("SELECT id FROM main.journal_entries WHERE text=?1 AND (?2 IS NULL OR transaction_date=?2) AND (?3 IS NULL OR document_id=?3) ORDER BY id DESC LIMIT 2").map_err(failed)?;
    let ids = query
        .query_map(params![text, date, document], |r| r.get::<_, i64>(0))
        .map_err(failed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(failed)?;
    if ids.len() != 1 {
        return Err(selection());
    }
    Ok(JournalEntryId::from_sqlite(ids[0]))
}
fn balances(
    tx: &Transaction<'_>,
    request: &ManualJournal,
) -> Result<BTreeMap<String, (String, Money)>, Error> {
    let mut result = BTreeMap::new();
    for line in &request.lines {
        let no = line.account_no.as_str();
        if result.contains_key(no) {
            continue;
        }
        let name: String = tx
            .query_row(
                "SELECT name FROM main.accounts WHERE account_no=?1",
                [no],
                |r| r.get(0),
            )
            .map_err(failed)?;
        let mut balance = Money::default();
        let mut query = tx.prepare("SELECT jl.debit_amount,jl.credit_amount FROM main.journal_lines jl JOIN main.accounts a ON a.id=jl.account_id WHERE a.account_no=?1 ORDER BY jl.id").map_err(failed)?;
        let mut rows = query.query([no]).map_err(failed)?;
        while let Some(row) = rows.next().map_err(failed)? {
            let debit: f64 = row.get(0).map_err(failed)?;
            let credit: f64 = row.get(1).map_err(failed)?;
            balance.add(&Money::parse(&numeric(&json!(debit))?)?);
            balance = balance.subtract(&Money::parse(&numeric(&json!(credit))?)?);
        }
        result.insert(no.to_owned(), (name, balance));
    }
    Ok(result)
}
/// Execute the real writer, capture its actual identity/hash and exact effects,
/// then undo *all* writer effects. A rollback error is never a successful preview.
pub fn dry_run(
    tx: &Transaction<'_>,
    request: &ManualJournal,
    apply: impl FnOnce() -> Result<Value, Error>,
) -> Result<Value, Error> {
    journal_post::require_current(tx)?;
    tx.execute_batch("SAVEPOINT original_journal_dry_run")
        .map_err(failed)?;
    let result = (|| {
        // Validate through the real engine before reading account snapshots, so
        // nonexistent accounts retain native business rejection semantics.
        let mut value = apply()?;
        if value.get("ok") != Some(&Value::Bool(true)) {
            return Ok(value);
        }
        let after = balances(tx, request)?;
        let entry = value
            .get("entryId")
            .and_then(Value::as_i64)
            .ok_or_else(invalid)?;
        let previous: String = tx
            .query_row(
                "SELECT previous_hash FROM main.journal_entries WHERE id=?1",
                [entry],
                |r| r.get(0),
            )
            .map_err(failed)?;
        // Derive before from the exact inserted lines, not SUM(binary64).
        let mut deltas: BTreeMap<String, Money> = BTreeMap::new();
        for line in &request.lines {
            let delta = deltas
                .entry(line.account_no.as_str().to_owned())
                .or_default();
            delta.add(&line.debit_amount);
            *delta = delta.subtract(&line.credit_amount);
        }
        let mut effects = Vec::new();
        for (no, (name, balance)) in after {
            let delta = deltas.get(&no).ok_or_else(invalid)?;
            let before = balance.subtract(delta);
            effects.push(json!({"accountNo":no,"accountName":name,"balanceBefore":journal_post::money(&before)?,"balanceAfter":journal_post::money(&balance)?,"delta":journal_post::money(delta)?}));
        }
        value["previousHash"] = json!(previous);
        value["accountEffects"] = json!(effects);
        Ok(value)
    })();
    tx.execute_batch("ROLLBACK TO original_journal_dry_run; RELEASE original_journal_dry_run")
        .map_err(failed)?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_defaults_rounding_and_identity_refusal() {
        let source = json!({"transactionDate":"2026-01-01","text":"Synthetic","lines":[{"accountNo":"1000","debitAmount":1.005},{"accountNo":"2000","creditAmount":1.01}]});
        let request = adapt(source.clone()).unwrap();
        assert_eq!(request.currency.as_str(), "DKK");
        assert_eq!(request.lines[0].debit_amount.decimal(), "1.01");
        assert_eq!(request.lines[0].credit_amount, Money::default());
        for key in ["createdBy", "createdByProgram"] {
            let mut value = source.clone();
            value[key] = json!("rentemester-import-postings");
            assert!(adapt(value).is_err());
        }
        let mut value = source.clone();
        value["lines"][0]["debitAmount"] = json!("1.01");
        assert!(adapt(value).is_err());
        let mut value = source;
        value["documentId"] = json!(9007199254740992_u64);
        assert!(adapt(value).is_err());
    }
}

#[cfg(test)]
mod writer_tests {
    use super::*;
    use rentemester_engine::fiscal_year::Date;
    use rusqlite::{Connection, TransactionBehavior};
    fn fixture() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        crate::company_baseline::rehearse(&mut db).unwrap();
        db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('2000','Synthetic bank','asset','debit'),('5000','Synthetic equity','equity','credit')").unwrap();
        db
    }
    fn request() -> ManualJournal {
        adapt(json!({"transactionDate":"2026-01-01","text":"Synthetic capital","lines":[{"accountNo":"2000","debitAmount":10},{"accountNo":"5000","creditAmount":10}]})).unwrap()
    }
    fn snapshot(db: &Connection) -> Vec<String> {
        let mut tables = db
            .prepare("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")
            .unwrap();
        let names = tables
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let mut result = Vec::new();
        for name in names {
            let mut query = db
                .prepare(&format!(
                    "SELECT * FROM \"{}\" ORDER BY rowid",
                    name.replace('"', "\"\"")
                ))
                .unwrap();
            let count = query.column_count();
            let mut rows = query.query([]).unwrap();
            while let Some(row) = rows.next().unwrap() {
                let fields = (0..count)
                    .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
                    .collect::<Vec<_>>();
                result.push(format!("{name}:{fields:?}"));
            }
        }
        result
    }
    fn post(tx: &Transaction<'_>, request: &ManualJournal) -> Result<Value, Error> {
        journal_post::post(
            tx,
            request,
            &crate::journal_preview::manual_journal_rule_version()?,
            Date::parse("2026-02-15")?,
            journal_post::Actor {
                subject: "user:synthetic",
                program: "rentemester-rust",
            },
        )
    }
    #[test]
    fn actual_dry_run_identity_effects_and_every_sql_table_rollback() {
        let mut db = fixture();
        let before = snapshot(&db);
        let request = request();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let preview = dry_run(&tx, &request, || post(&tx, &request)).unwrap();
        assert_eq!(preview["ok"], true);
        assert_eq!(preview["previousHash"], "GENESIS");
        assert_eq!(
            preview["accountEffects"],
            json!([
                {"accountNo":"2000","accountName":"Synthetic bank","balanceBefore":0.0,"balanceAfter":10.0,"delta":10.0},
                {"accountNo":"5000","accountName":"Synthetic equity","balanceBefore":0.0,"balanceAfter":-10.0,"delta":-10.0}
            ])
        );
        assert_eq!(snapshot(&tx), before);
        tx.commit().unwrap();
        assert_eq!(snapshot(&db), before);
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let actual = post(&tx, &request).unwrap();
        for key in ["entryId", "entryNo", "entryHash"] {
            assert_eq!(actual[key], preview[key]);
        }
        tx.commit().unwrap();
        assert!(crate::journal_hash::verify(&db).unwrap().ok);
    }
    #[test]
    fn late_failure_and_business_refusal_rollback_all_sql() {
        let mut db = fixture();
        let before = snapshot(&db);
        let request = request();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(
            dry_run(&tx, &request, || {
                post(&tx, &request)?;
                Err(failed("synthetic late failure"))
            })
            .is_err()
        );
        assert_eq!(snapshot(&tx), before);
        tx.commit().unwrap();
        assert_eq!(snapshot(&db), before);
        let mut request = request;
        request.lines[0].debit_amount = Money::parse("11").unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let refusal = dry_run(&tx, &request, || post(&tx, &request)).unwrap();
        assert_eq!(refusal["ok"], false);
        assert!(refusal.get("entryId").is_none());
        assert_eq!(snapshot(&tx), before);
    }
    #[test]
    fn bank_exception_resolution_and_late_failure_are_fully_rolled_back() {
        let mut db = fixture();
        db.execute_batch("INSERT INTO bank_transactions(id,transaction_date,text,amount,currency)VALUES(1,'2026-01-01','Synthetic bank capital',10,'DKK');INSERT INTO exceptions(type,related_bank_transaction_id,message)VALUES('UNMATCHED_BANK_TRANSACTION',1,'Synthetic unmatched bank')").unwrap();
        let before = snapshot(&db);
        let mut request = request();
        request.source_bank_transaction_id =
            Some(rentemester_engine::identifiers::BankTransactionId::from_sqlite(1));
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let preview = dry_run(&tx, &request, || post(&tx, &request)).unwrap();
        assert_eq!(preview["ok"], true);
        assert_eq!(snapshot(&tx), before);
        assert!(
            dry_run(&tx, &request, || {
                let result = post(&tx, &request)?;
                assert_eq!(result["ok"], true);
                let status: String = tx
                    .query_row("SELECT status FROM exceptions WHERE id=1", [], |r| r.get(0))
                    .unwrap();
                assert_eq!(status, "resolved");
                Err(failed("synthetic late failure after bank resolution"))
            })
            .is_err()
        );
        assert_eq!(snapshot(&tx), before);
        tx.commit().unwrap();
        assert_eq!(snapshot(&db), before);
    }
    #[test]
    fn native_closed_period_and_protected_vat_refusals_have_no_effects() {
        let mut db = fixture();
        db.execute_batch("INSERT INTO accounting_periods(period_start,period_end,kind,status)VALUES('2026-01-01','2026-01-31','custom','closed')").unwrap();
        let before = snapshot(&db);
        let request = request();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let refusal = dry_run(&tx, &request, || post(&tx, &request)).unwrap();
        assert_eq!(refusal["ok"], false);
        assert!(
            refusal["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_str().unwrap().contains("period"))
        );
        assert_eq!(snapshot(&tx), before);
        tx.rollback().unwrap();
        // A confirmed VAT amount role cannot be bypassed by the source adapter.
        let mut db = fixture();
        db.execute_batch("INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('output_vat','5000',1,'synthetic')").unwrap();
        let before = snapshot(&db);
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let refusal = dry_run(&tx, &request, || post(&tx, &request)).unwrap();
        assert_eq!(refusal["ok"], false);
        assert_eq!(snapshot(&tx), before);
    }
    #[test]
    fn selector_precedence_exact_match_ambiguity_and_notfound() {
        let mut db = fixture();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let request = request();
        let first = post(&tx, &request).unwrap();
        post(&tx, &request).unwrap();
        let mut selector = Selector {
            entry_id: Some(json!(1)),
            entry_no: Some("missing".into()),
            match_text: Some("Synthetic capital".into()),
            match_date: None,
            match_document_id: None,
        };
        assert_eq!(resolve(&tx, &selector).unwrap().value(), 1);
        selector.entry_id = None;
        assert_eq!(
            resolve(&tx, &selector).unwrap_err().code,
            "journal_selection_usage"
        );
        selector.entry_no = Some(format!(" {} ", first["entryNo"].as_str().unwrap()));
        assert_eq!(resolve(&tx, &selector).unwrap().value(), 1);
        selector.entry_no = None;
        assert_eq!(
            resolve(&tx, &selector).unwrap_err().code,
            "journal_selection_usage"
        );
        selector.match_date = Some("2026-02-01".into());
        assert_eq!(
            resolve(&tx, &selector).unwrap_err().code,
            "journal_selection_usage"
        );
        selector.match_text = None;
        assert_eq!(
            resolve(&tx, &selector).unwrap_err().code,
            "journal_selection_usage"
        );
        tx.rollback().unwrap();
    }
}
