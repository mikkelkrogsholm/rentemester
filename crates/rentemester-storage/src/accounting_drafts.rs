//! Append-only four-eyes drafts. The caller owns the fenced transaction and commit.
use crate::{invalid, journal_preview};
use rentemester_engine::{
    Error,
    fiscal_year::Date,
    manual_journal::{self, ManualJournal},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub fn refused(message: impl Into<String>) -> Error {
    Error::new("draft_refused", message)
}
pub fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub draft_id: Option<String>,
    pub payload: Option<Value>,
    pub expected_event_hash: Option<String>,
    pub reason: Option<String>,
    pub confirm: Option<bool>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    previous_hash: Option<String>,
    id: i64,
    draft_id: String,
    version: i64,
    event_type: String,
    payload_hash: String,
    canonical_payload: String,
    reason: Option<String>,
    journal_entry_id: Option<i64>,
    actor_id: String,
    actor_program: String,
    created_at: String,
    #[serde(skip)]
    event_hash: String,
}
fn events(tx: &Transaction<'_>) -> Result<Vec<Event>, Error> {
    let mut stmt=tx.prepare("SELECT previous_hash,id,draft_id,version,event_type,payload_hash,canonical_payload,reason,journal_entry_id,actor_id,actor_program,created_at,event_hash FROM accounting_draft_events ORDER BY id").map_err(invalid)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Event {
                previous_hash: r.get(0)?,
                id: r.get(1)?,
                draft_id: r.get(2)?,
                version: r.get(3)?,
                event_type: r.get(4)?,
                payload_hash: r.get(5)?,
                canonical_payload: r.get(6)?,
                reason: r.get(7)?,
                journal_entry_id: r.get(8)?,
                actor_id: r.get(9)?,
                actor_program: r.get(10)?,
                created_at: r.get(11)?,
                event_hash: r.get(12)?,
            })
        })
        .map_err(invalid)?;
    let mut result = vec![];
    let mut previous = None;
    for row in rows {
        let e = row.map_err(invalid)?;
        if e.previous_hash != previous
            || hash(&e.canonical_payload) != e.payload_hash
            || hash(&serde_json::to_string(&e).map_err(invalid)?) != e.event_hash
        {
            return Err(refused("draft evidence hash-chain invalid"));
        }
        previous = Some(e.event_hash.clone());
        result.push(e);
    }
    Ok(result)
}
fn state(e: &Event) -> Result<Value, Error> {
    Ok(
        json!({"id":e.draft_id,"version":e.version,"status":e.event_type,"payloadHash":e.payload_hash,"eventHash":e.event_hash,"payload":serde_json::from_str::<Value>(&e.canonical_payload).map_err(invalid)?,"actorId":e.actor_id,"reason":e.reason,"journalEntryId":e.journal_entry_id}),
    )
}
pub fn list(tx: &Transaction<'_>) -> Result<Value, Error> {
    crate::validate(tx)?;
    let mut latest = std::collections::BTreeMap::new();
    for e in events(tx)? {
        latest.insert(e.draft_id.clone(), e);
    }
    Ok(json!(
        latest.values().map(state).collect::<Result<Vec<_>, _>>()?
    ))
}
fn normalize(value: &Value) -> Result<ManualJournal, Error> {
    let mut v = value.clone();
    let obj = v
        .as_object_mut()
        .ok_or_else(|| refused("payload object required"))?;
    if obj.contains_key("createdByProgram") {
        return Err(refused("caller attribution forbidden"));
    }
    obj.entry("currency").or_insert(json!("DKK"));
    for key in ["documentId", "sourceBankTransactionId"] {
        if let Some(value) = obj.get_mut(key)
            && !value.is_null()
        {
            let id = value
                .as_i64()
                .filter(|id| *id > 0 && *id <= 9_007_199_254_740_991)
                .ok_or_else(|| refused("positive exact numeric source id required"))?;
            *value = json!(id.to_string());
        }
    }
    for key in ["amountForeign", "amountDkk"] {
        if let Some(n) = obj.get_mut(key)
            && let Some(number) = n.as_number()
        {
            *n = json!(rentemester_engine::document::money(number)?.decimal());
        }
    }
    if let Some(rate) = obj.get_mut("fxRateToDkk")
        && let Some(n) = rate.as_number()
    {
        *rate = json!(n.to_string());
    }
    let lines = obj
        .get_mut("lines")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| refused("lines required"))?;
    for line in lines {
        for key in ["debitAmount", "creditAmount"] {
            if let Some(n) = line.get_mut(key)
                && let Some(number) = n.as_number()
            {
                *n = json!(rentemester_engine::document::money(number)?.decimal());
            }
        }
    }
    serde_json::from_value(v).map_err(|_| refused("unsupported journal payload"))
}
fn validate(tx: &Transaction<'_>, value: &Value, latest: Date) -> Result<ManualJournal, Error> {
    let request = normalize(value)?;
    let rules = journal_preview::compiled_rules()?;
    let facts = journal_preview::collect_facts(tx, &request, latest)?;
    if let Some(bank) = request.source_bank_transaction_id {
        let used: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM journal_entries WHERE source_bank_transaction_id=?1)",
                [bank.value()],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        if used {
            return Err(refused("bank source already posted"));
        }
        let (amount,currency,account):(rentemester_engine::money::Money,String,Option<String>)=tx.query_row("SELECT b.amount,b.currency,a.ledger_account_no FROM bank_transactions b LEFT JOIN bank_accounts a ON a.id=b.bank_account_id WHERE b.id=?1",[bank.value()],|r|Ok((crate::legacy_money(r.get_ref(0)?).map_err(|_|rusqlite::Error::InvalidQuery)?,r.get(1)?,r.get(2)?))).map_err(invalid)?;
        let account =
            account.ok_or_else(|| refused("source bank requires a mapped ledger account"))?;
        if currency != "DKK" || request.currency.as_str() != "DKK" {
            return Err(refused("native bank draft review currently requires DKK"));
        }
        let mut movement = rentemester_engine::money::Money::default();
        for line in &request.lines {
            if line.account_no.as_str() == account {
                movement.add(&line.debit_amount.subtract(&line.credit_amount));
            }
        }
        if movement != amount {
            return Err(refused(
                "draft bank movement must exactly equal current source amount",
            ));
        }
    }
    let checked = manual_journal::validate(&request, &facts, &rules, rules.version())?;
    if !checked.ok {
        return Err(refused(
            checked
                .errors
                .iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    Ok(request)
}
// Persist a separate immutable audit record, leaving the legacy draft payload/hash format intact.
// Full source row values bind reference, buyer/VAT/currency and account mapping changes too.
fn source_row(tx: &Transaction<'_>, table: &str, id: i64) -> Result<Value, Error> {
    let mut q = tx
        .prepare(&format!("SELECT * FROM {table} WHERE id=?1"))
        .map_err(invalid)?;
    let names = q
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut rows = q.query([id]).map_err(invalid)?;
    let r = rows
        .next()
        .map_err(invalid)?
        .ok_or_else(|| refused("source evidence missing"))?;
    let mut map = serde_json::Map::new();
    for (i, name) in names.into_iter().enumerate() {
        use rusqlite::types::ValueRef;
        let v = match r.get_ref(i).map_err(invalid)? {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(n) => json!(n),
            ValueRef::Real(n) => json!(n),
            ValueRef::Text(s) => json!(std::str::from_utf8(s).map_err(invalid)?),
            ValueRef::Blob(_) => return Err(refused("binary source unsupported")),
        };
        map.insert(name, v);
    }
    Ok(Value::Object(map))
}
pub fn evidence(tx: &Transaction<'_>, value: &Value) -> Result<String, Error> {
    let mut sources = vec![];
    for (key, table) in [
        ("sourceBankTransactionId", "bank_transactions"),
        ("documentId", "documents"),
    ] {
        if let Some(id) = value[key].as_i64() {
            let row = source_row(tx, table, id)?;
            if table == "bank_transactions"
                && let Some(account) = row["bank_account_id"].as_i64()
            {
                sources.push(
                    json!({"kind":"bank_accounts","row":source_row(tx,"bank_accounts",account)?}),
                );
            }
            sources.push(json!({"kind":table,"row":row}));
        }
    }
    if let Some(lines) = value["lines"].as_array() {
        for line in lines {
            let account = line["accountNo"]
                .as_str()
                .ok_or_else(|| refused("account evidence required"))?;
            let id: i64 = tx
                .query_row(
                    "SELECT id FROM accounts WHERE account_no=?1",
                    [account],
                    |r| r.get(0),
                )
                .map_err(invalid)?;
            sources.push(json!({"kind":"accounts","row":source_row(tx,"accounts",id)?}));
        }
    }
    sources.push(
        json!({"kind":"manual_rules","version":journal_preview::manual_journal_rule_version()?}),
    );
    Ok(hash(&serde_json::to_string(&sources).map_err(invalid)?))
}
fn check_evidence(tx: &Transaction<'_>, e: &Event) -> Result<(), Error> {
    let saved:Option<String>=tx.query_row("SELECT message FROM audit_log WHERE event_type='native_draft_evidence' AND entity_type='accounting_draft' AND entity_id=?1 ORDER BY id DESC LIMIT 1",[&e.event_hash],|r|r.get(0)).optional().map_err(invalid)?;
    let value = serde_json::from_str(&e.canonical_payload).map_err(invalid)?;
    if saved.as_deref() != Some(&evidence(tx, &value)?) {
        return Err(refused(
            "source evidence changed or native review evidence unavailable; revise draft",
        ));
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)] // Keep the immutable event columns explicit.
fn append(
    tx: &Transaction<'_>,
    all: &[Event],
    id: &str,
    version: i64,
    status: &str,
    payload: &str,
    actor: &str,
    reason: Option<&str>,
    journal: Option<i64>,
) -> Result<Value, Error> {
    let at: String = tx
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
            r.get(0)
        })
        .map_err(invalid)?;
    let mut e = Event {
        previous_hash: all.last().map(|e| e.event_hash.clone()),
        id: all.last().map_or(1, |e| e.id + 1),
        draft_id: id.into(),
        version,
        event_type: status.into(),
        payload_hash: hash(payload),
        canonical_payload: payload.into(),
        reason: reason.map(str::to_owned),
        journal_entry_id: journal,
        actor_id: actor.into(),
        actor_program: "rentemester-accounting-draft".into(),
        created_at: at,
        event_hash: String::new(),
    };
    e.event_hash = hash(&serde_json::to_string(&e).map_err(invalid)?);
    tx.execute("INSERT INTO accounting_draft_events(previous_hash,id,draft_id,version,event_type,payload_hash,canonical_payload,reason,journal_entry_id,actor_id,actor_program,created_at,event_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",params![e.previous_hash,e.id,e.draft_id,e.version,e.event_type,e.payload_hash,e.canonical_payload,e.reason,e.journal_entry_id,e.actor_id,e.actor_program,e.created_at,e.event_hash]).map_err(invalid)?;
    let source = evidence(tx, &serde_json::from_str(payload).map_err(invalid)?)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('native_draft_evidence','accounting_draft',?1,?2,?3)",params![e.event_hash,source,actor]).map_err(invalid)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'accounting_draft',?2,?3,?4)",params![format!("accounting_draft_{status}"),id,format!("Version {version}: {status}"),actor]).map_err(invalid)?;
    state(&e)
}
pub fn change(
    tx: &Transaction<'_>,
    id: &str,
    action: &str,
    input: &Input,
    actor: &str,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if id.is_empty()
        || id.len() > 64
        || !id.as_bytes()[0].is_ascii_lowercase()
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(refused("lowercase stable draft id required"));
    }
    let all = events(tx)?;
    let current = all.iter().rev().find(|e| e.draft_id == id);
    if action == "create" {
        if current.is_some() {
            return Err(refused("draft id already exists"));
        }
        let p = input
            .payload
            .as_ref()
            .ok_or_else(|| refused("payload required"))?;
        validate(tx, p, latest)?;
        let canonical = serde_json::to_string(p).map_err(invalid)?;
        if canonical.len() > 262144 {
            return Err(refused("payload too large"));
        }
        return append(tx, &all, id, 1, "created", &canonical, actor, None, None);
    }
    let e = current.ok_or_else(|| refused("draft not found"))?;
    if action != "revise" && input.payload.is_some() {
        return Err(refused("review must use stored payload"));
    }
    // Same exact reviewed request may read its durable effect after a lost response.
    if action == "approve-and-post" && e.event_type == "approved_posted" {
        let submitted = all
            .iter()
            .rev()
            .find(|s| s.draft_id == id && s.event_type == "submitted");
        if input.confirm == Some(true)
            && e.actor_id == actor
            && submitted.is_some_and(|s| Some(&s.event_hash) == input.expected_event_hash.as_ref())
        {
            return state(e);
        }
    }
    if Some(&e.event_hash) != input.expected_event_hash.as_ref() {
        return Err(refused("stale draft version"));
    }
    if action == "revise" {
        if !["created", "revised", "rejected"].contains(&e.event_type.as_str()) {
            return Err(refused("draft not editable"));
        }
        let p = input
            .payload
            .as_ref()
            .ok_or_else(|| refused("payload required"))?;
        validate(tx, p, latest)?;
        return append(
            tx,
            &all,
            id,
            e.version + 1,
            "revised",
            &serde_json::to_string(p).map_err(invalid)?,
            actor,
            None,
            None,
        );
    }
    if input.payload.is_some() {
        return Err(refused("review must use stored payload"));
    }
    let value: Value = serde_json::from_str(&e.canonical_payload).map_err(invalid)?;
    if action == "submit" {
        if !["created", "revised"].contains(&e.event_type.as_str()) {
            return Err(refused("editable draft required"));
        }
        check_evidence(tx, e)?;
        validate(tx, &value, latest)?;
        return append(
            tx,
            &all,
            id,
            e.version,
            "submitted",
            &e.canonical_payload,
            actor,
            None,
            None,
        );
    }
    if e.event_type != "submitted" {
        return Err(refused("submitted draft required"));
    }
    let author = all
        .iter()
        .rev()
        .find(|s| {
            s.draft_id == id
                && s.version == e.version
                && ["created", "revised"].contains(&s.event_type.as_str())
        })
        .ok_or_else(|| refused("author unavailable"))?;
    if e.actor_id == actor || author.actor_id == actor {
        return Err(refused(
            "independent reviewer required: author and submitter cannot review",
        ));
    }
    if action == "reject" {
        let reason = input.reason.as_deref().unwrap_or("").trim();
        if reason.is_empty() || reason.len() > 1000 {
            return Err(refused("rejection reason required"));
        }
        return append(
            tx,
            &all,
            id,
            e.version,
            "rejected",
            &e.canonical_payload,
            actor,
            Some(reason),
            None,
        );
    }
    if action != "approve-and-post" || input.confirm != Some(true) {
        return Err(refused("explicit approval required"));
    }
    check_evidence(tx, e)?;
    let request = validate(tx, &value, latest)?;
    let version = journal_preview::manual_journal_rule_version()?;
    let result = crate::journal_post::post(
        tx,
        &request,
        &version,
        latest,
        crate::journal_post::Actor {
            subject: actor,
            program: "rentemester-accounting-draft",
        },
    )?;
    if result["ok"] != true {
        return Err(refused(
            "posting refused; entire draft approval rolled back",
        ));
    }
    let journal = result["entryId"]
        .as_i64()
        .ok_or_else(|| refused("posting receipt unavailable"))?;
    append(
        tx,
        &all,
        id,
        e.version,
        "approved_posted",
        &e.canonical_payload,
        actor,
        None,
        Some(journal),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, TransactionBehavior};
    fn fixture(path: &std::path::Path) -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        let mut db = Connection::open(path).unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut db, "Synthetic bank review").unwrap();
        db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('B','Synthetic bank','asset','debit'),('E','Synthetic capital','equity','credit');INSERT INTO bank_accounts(id,slug,name,currency,ledger_account_no) VALUES(1,'synthetic','Synthetic bank','DKK','B');INSERT INTO bank_transactions(id,transaction_date,text,amount,currency,bank_account_id) VALUES(1,'2026-01-01','Synthetic capital',100.01,'DKK',1);").unwrap();
        db
    }
    fn latest() -> Date {
        Date::parse("2026-02-15").unwrap()
    }
    fn change_db(
        db: &mut Connection,
        id: &str,
        action: &str,
        v: Value,
        actor: &str,
    ) -> Result<Value, Error> {
        let input: Input = serde_json::from_value(v).unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let result = change(&tx, id, action, &input, actor, latest());
        if result.is_ok() {
            tx.commit().unwrap();
        } else {
            tx.rollback().unwrap();
        }
        result
    }
    fn create(db: &mut Connection) -> Value {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let h = evidence(&tx, &json!({"sourceBankTransactionId":1})).unwrap();
        let i=serde_json::from_value(json!({"bankTransactionId":1,"expectedSourceHash":h,"accountNo":"E","draftId":"bank-one","confirm":true})).unwrap();
        let result = crate::bookkeeping_queue::create(&tx, &i, "author", latest()).unwrap();
        tx.commit().unwrap();
        result
    }
    fn count(db: &Connection, table: &str) -> i64 {
        db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }
    #[test]
    fn four_eyes_exact_replay_restart_and_atomic_rollback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("company.sqlite");
        let mut db = fixture(&path);
        let draft = create(&mut db);
        assert!(
            change_db(
                &mut db,
                "bank-one",
                "submit",
                json!({"expectedEventHash":"stale"}),
                "author"
            )
            .is_err()
        );
        let submitted = change_db(
            &mut db,
            "bank-one",
            "submit",
            json!({"expectedEventHash":draft["eventHash"]}),
            "submitter",
        )
        .unwrap();
        let input = json!({"expectedEventHash":submitted["eventHash"],"confirm":true});
        for actor in ["author", "submitter"] {
            assert!(
                change_db(
                    &mut db,
                    "bank-one",
                    "approve-and-post",
                    input.clone(),
                    actor
                )
                .is_err()
            );
        }
        {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            change(
                &tx,
                "bank-one",
                "approve-and-post",
                &serde_json::from_value(input.clone()).unwrap(),
                "reviewer",
                latest(),
            )
            .unwrap();
            tx.rollback().unwrap();
        }
        assert_eq!(count(&db, "journal_entries"), 0);
        let posted = change_db(
            &mut db,
            "bank-one",
            "approve-and-post",
            input.clone(),
            "reviewer",
        )
        .unwrap();
        assert_eq!(posted["status"], "approved_posted");
        assert_eq!(count(&db, "journal_entries"), 1);
        assert_eq!(count(&db, "journal_lines"), 2);
        assert!(crate::journal_hash::verify(&db).unwrap().ok);
        drop(db);
        let mut db = Connection::open(path).unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
            .unwrap();
        assert_eq!(
            change_db(&mut db, "bank-one", "approve-and-post", input, "reviewer").unwrap(),
            posted
        );
        assert_eq!(count(&db, "journal_entries"), 1);
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(
            crate::bookkeeping_queue::read(&tx).unwrap()["rows"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
    #[test]
    fn changed_sources_closed_period_and_tampered_payload_refuse_without_posting() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = fixture(&dir.path().join("c"));
        let draft = create(&mut db);
        db.execute(
            "UPDATE bank_transactions SET text='Changed source' WHERE id=1",
            [],
        )
        .unwrap();
        assert!(
            change_db(
                &mut db,
                "bank-one",
                "submit",
                json!({"expectedEventHash":draft["eventHash"]}),
                "author"
            )
            .is_err()
        );
        let mut wrong = draft["payload"].clone();
        wrong["lines"][0]["debitAmount"] = json!(200);
        wrong["lines"][1]["creditAmount"] = json!(200);
        assert!(
            change_db(
                &mut db,
                "bank-one",
                "revise",
                json!({"expectedEventHash":draft["eventHash"],"payload":wrong}),
                "author"
            )
            .is_err()
        );
        let revised = change_db(
            &mut db,
            "bank-one",
            "revise",
            json!({"expectedEventHash":draft["eventHash"],"payload":draft["payload"]}),
            "author",
        )
        .unwrap();
        let submitted = change_db(
            &mut db,
            "bank-one",
            "submit",
            json!({"expectedEventHash":revised["eventHash"]}),
            "author",
        )
        .unwrap();
        assert!(change_db(&mut db,"bank-one","approve-and-post",json!({"expectedEventHash":submitted["eventHash"],"payload":draft["payload"],"confirm":true}),"reviewer").is_err());
        db.execute_batch("INSERT INTO accounting_periods(period_start,period_end,kind,status,reference) VALUES('2026-01-01','2026-01-31','vat_period','closed','synthetic-lock')").unwrap();
        assert!(
            change_db(
                &mut db,
                "bank-one",
                "approve-and-post",
                json!({"expectedEventHash":submitted["eventHash"],"confirm":true}),
                "reviewer"
            )
            .is_err()
        );
        assert_eq!(count(&db, "journal_entries"), 0);
    }
    #[test]
    fn immutable_draft_corruption_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = fixture(&dir.path().join("c"));
        create(&mut db);
        assert!(
            db.execute(
                "UPDATE accounting_draft_events SET payload_hash='changed'",
                []
            )
            .is_err()
        );
        let guard: String = db
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE name='accounting_draft_events_no_update'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        db.execute_batch("DROP TRIGGER accounting_draft_events_no_update;UPDATE accounting_draft_events SET canonical_payload='{}';").unwrap();
        db.execute_batch(&guard).unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(list(&tx).is_err());
    }
    #[test]
    fn rule_author_cannot_approve_exact_hint_and_ambiguity_stays_manual() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = fixture(&dir.path().join("c"));
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        for id in ["one", "two"] {
            let input=serde_json::from_value(json!({"confirm":true,"proposal":{"ruleId":id,"effectiveFrom":"2025-01-01","conditions":{"currency":"DKK","attributes":{"bankText":"Synthetic capital"}},"outcome":{"account":"E"},"rationale":"Synthetic rule","provenance":"synthetic"}})).unwrap();
            let proposed =
                crate::posting_rules::change(&tx, "propose", &input, "author", "2026-01-01")
                    .unwrap();
            let approval=serde_json::from_value(json!({"confirm":true,"ruleId":id,"version":1,"expectedPayloadHash":proposed["payloadHash"],"rationale":"Reviewed evidence","provenance":"synthetic"})).unwrap();
            assert!(
                crate::posting_rules::change(&tx, "approve", &approval, "author", "2026-01-01")
                    .is_err()
            );
            crate::posting_rules::change(&tx, "approve", &approval, "reviewer", "2026-01-01")
                .unwrap();
            if id == "one" {
                assert_eq!(
                    crate::posting_rules::hint(&tx, "Synthetic capital", "DKK", "2026-01-01")
                        .unwrap()
                        .as_deref(),
                    Some("E")
                );
            }
        }
        assert!(
            crate::posting_rules::hint(&tx, "Synthetic capital", "DKK", "2026-01-01")
                .unwrap()
                .is_none()
        );
        assert!(
            crate::posting_rules::hint(&tx, "Different", "DKK", "2026-01-01")
                .unwrap()
                .is_none()
        );
        assert_eq!(count(&tx, "journal_entries"), 0);
    }
    #[test]
    #[ignore = "synthetic concurrent exact-approval process helper"]
    fn review_child() {
        let path = std::env::var("RM_REVIEW_COMPANY").unwrap();
        let mut db = Connection::open(path).unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
            .unwrap();
        db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
        let input = serde_json::from_str(&std::env::var("RM_REVIEW_INPUT").unwrap()).unwrap();
        let result = change_db(&mut db, "bank-one", "approve-and-post", input, "reviewer");
        std::fs::write(
            std::env::var("RM_REVIEW_OUTPUT").unwrap(),
            serde_json::to_vec(&json!({"ok":result.is_ok(),"result":result.ok()})).unwrap(),
        )
        .unwrap();
    }
    #[test]
    fn concurrent_exact_approval_has_one_journal_effect() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("company");
        let mut db = fixture(&path);
        let draft = create(&mut db);
        let submitted = change_db(
            &mut db,
            "bank-one",
            "submit",
            json!({"expectedEventHash":draft["eventHash"]}),
            "author",
        )
        .unwrap();
        let input = json!({"expectedEventHash":submitted["eventHash"],"confirm":true}).to_string();
        drop(db);
        let mut children = vec![];
        for n in 0..2 {
            let output = dir.path().join(format!("result-{n}"));
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "accounting_drafts::tests::review_child",
                    "--ignored",
                ])
                .env("RM_REVIEW_COMPANY", &path)
                .env("RM_REVIEW_INPUT", &input)
                .env("RM_REVIEW_OUTPUT", &output)
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap();
            children.push((child, output));
        }
        let mut results = vec![];
        for (mut child, output) in children {
            assert!(child.wait().unwrap().success());
            let result: Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
            assert_eq!(result["ok"], true);
            results.push(result["result"].clone());
        }
        assert_eq!(results[0], results[1]);
        let db = Connection::open(path).unwrap();
        assert_eq!(count(&db, "journal_entries"), 1);
        assert_eq!(count(&db, "accounting_draft_events"), 3);
        assert!(crate::journal_hash::verify(&db).unwrap().ok);
    }
}
