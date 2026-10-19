//! Native atomic_v1 batches. Every write below belongs to the caller's ONE company transaction.
//! Legacy unversioned revisions remain inspectable and are never resumed by this writer.
use rentemester_engine::{
    Error,
    bookkeeping_batch::{self as wire, Exact, Persist, Scope},
    fiscal_year::Date,
    money::Money,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params, types::ValueRef};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
mod checks;
#[cfg(test)]
mod tests;
mod workbench;
pub use checks::final_checks;
pub use workbench::read as workbench;
fn fail(s: impl Into<String>) -> Error {
    wire::refused(s)
}
fn sql(_: rusqlite::Error) -> Error {
    fail("batch storage unavailable; abort entire operation")
}
const MAX_ROWS: usize = 5000;
const MAX_PLAN: usize = 8 * 1024 * 1024;
pub(crate) fn canonical(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut keys = m.keys().collect::<Vec<_>>();
            keys.sort();
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|k| format!("{}:{}", json!(k), canonical(&m[k])))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        Value::Number(n) => {
            if let Some(n) = n.as_i64() {
                n.to_string()
            } else if let Some(n) = n.as_u64() {
                n.to_string()
            } else {
                ryu_js::Buffer::new().format(n.as_f64().unwrap()).to_owned()
            }
        }
        _ => v.to_string(),
    }
}
pub(crate) fn hash(v: &Value) -> String {
    format!("{:x}", Sha256::digest(canonical(v)))
}
/// Complete rows or refusal; never hash a truncated population as complete.
pub(crate) fn rows(
    tx: &Connection,
    sql_text: &str,
    args: impl rusqlite::Params,
) -> Result<Vec<Value>, Error> {
    let mut q = tx.prepare(sql_text).map_err(sql)?;
    let names = q
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut rs = q.query(args).map_err(sql)?;
    let mut out = vec![];
    let mut byte_count = 0usize;
    while let Some(r) = rs.next().map_err(sql)? {
        if out.len() == MAX_ROWS {
            return Err(fail(
                "candidate universe exceeds5000 rows; use narrower explicit scope",
            ));
        }
        let mut row = serde_json::Map::new();
        for (i, n) in names.iter().enumerate() {
            let v = match r.get_ref(i).map_err(sql)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(i) => json!(i),
                ValueRef::Real(f) => serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .ok_or_else(|| fail("nonfinite stored source"))?,
                ValueRef::Text(s) => {
                    json!(std::str::from_utf8(s).map_err(|_| fail("invalid source text"))?)
                }
                ValueRef::Blob(_) => return Err(fail("unexpected binary batch source")),
            };
            row.insert(n.clone(), v);
        }
        let row = Value::Object(row);
        byte_count = byte_count
            .checked_add(canonical(&row).len())
            .ok_or_else(|| fail("batch source byte bound exceeded"))?;
        if byte_count > MAX_PLAN {
            return Err(fail("complete source exceeds8MiB; narrow scope"));
        }
        out.push(row);
    }
    Ok(out)
}
fn number(v: &Value, key: &str) -> Result<i64, Error> {
    let n = v[key]
        .as_i64()
        .ok_or_else(|| fail(format!("missing {key}")))?;
    wire::id(n)?;
    Ok(n)
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, Error> {
    v[key]
        .as_str()
        .ok_or_else(|| fail(format!("missing {key}")))
}
fn money(v: &Value) -> Result<Money, Error> {
    Money::parse(&canonical(v))
}
fn checked_plan(v: Value) -> Result<Value, Error> {
    if canonical(&v).len() > MAX_PLAN {
        Err(fail("complete plan exceeds8MiB; use narrower scope"))
    } else {
        Ok(v)
    }
}
fn universe(tx: &Transaction<'_>, s: &Scope) -> Result<Value, Error> {
    let banks = rows(
        tx,
        "SELECT bt.*,br.reconciliation_id,br.journal_entry_id,ba.ledger_account_no,ba.active AS bank_account_active,ba.name AS bank_account_name FROM bank_transactions bt LEFT JOIN bank_journal_reconciliations br ON br.bank_transaction_id=bt.id LEFT JOIN bank_accounts ba ON ba.id=bt.bank_account_id WHERE bt.transaction_date BETWEEN ?1 AND ?2 ORDER BY bt.id",
        params![s.bank_from, s.bank_to],
    )?;
    let docs = rows(
        tx,
        "SELECT d.* FROM documents d WHERE d.document_type='purchase_sale' AND d.currency='DKK' AND NOT EXISTS(SELECT 1 FROM journal_entries je WHERE je.document_id=d.id AND je.status='posted') ORDER BY d.id",
        [],
    )?;
    let ledger = rows(
        tx,
        "SELECT * FROM journal_entries WHERE transaction_date BETWEEN ?1 AND ?2 OR source_bank_transaction_id IN(SELECT id FROM bank_transactions WHERE transaction_date BETWEEN ?3 AND ?4) ORDER BY id",
        params![s.accounting_from, s.accounting_to, s.bank_from, s.bank_to],
    )?;
    let mut out = json!({"banks":banks,"documents":docs,"ledger":ledger});
    // Include current configuration and append-only source enrichment, not incidental clock/audit heads.
    for (name, q) in [
        ("company", "SELECT * FROM companies ORDER BY id"),
        ("accounts", "SELECT * FROM accounts ORDER BY id"),
        ("roles", "SELECT * FROM account_role_mappings ORDER BY role"),
        ("periods", "SELECT * FROM accounting_periods ORDER BY id"),
        ("rules", "SELECT * FROM posting_rule_versions ORDER BY id"),
        (
            "enrichments",
            "SELECT * FROM document_metadata_enrichments ORDER BY document_id",
        ),
        (
            "contexts",
            "SELECT * FROM document_company_contexts ORDER BY document_id",
        ),
        (
            "dimensions",
            "SELECT * FROM current_accounting_dimension_members ORDER BY dimension_id,member_id",
        ),
        (
            "partyLinks",
            "SELECT * FROM current_document_party_links ORDER BY id",
        ),
        (
            "partyResolutions",
            "SELECT * FROM current_document_party_resolution_events ORDER BY id",
        ),
    ] {
        out[name] = json!(rows(tx, q, [])?)
    }
    Ok(out)
}
fn date_distance_near(a: &str, b: &str) -> bool {
    fn day(s: &str) -> Option<i64> {
        let y = s.get(..4)?.parse::<i64>().ok()?;
        let m = s.get(5..7)?.parse::<i64>().ok()?;
        let d = s.get(8..10)?.parse::<i64>().ok()?;
        let y = y - i64::from(m <= 2);
        let era = y.div_euclid(400);
        let yo = y - era * 400;
        let mp = m + if m > 2 { -3 } else { 9 };
        Some(era * 146097 + yo * 365 + yo / 4 - yo / 100 + (153 * mp + 2) / 5 + d - 1)
    }
    match (day(a), day(b)) {
        (Some(a), Some(b)) => (a - b).abs() <= 7,
        _ => false,
    }
}
pub fn plan(tx: &Transaction<'_>, s: &Scope, latest: Date, today: &str) -> Result<Value, Error> {
    s.validate()?;
    crate::validate(tx)?;
    let company: i64 = tx
        .query_row("SELECT id FROM companies", [], |r| r.get(0))
        .map_err(sql)?;
    let count: i64 = tx
        .query_row("SELECT count(*) FROM companies", [], |r| r.get(0))
        .map_err(sql)?;
    if count != 1 || s.company_id.is_some_and(|n| n != company) {
        return Err(fail("companyId must match selected admitted company"));
    }
    let sources = universe(tx, s)?;
    let banks = sources["banks"].as_array().unwrap();
    let docs = sources["documents"].as_array().unwrap();
    let mut choices = vec![];
    for bank in banks.iter().filter(|b| b["reconciliation_id"].is_null()) {
        let mut matches = vec![];
        if bank["currency"] == "DKK" && money(&bank["amount"])?.is_negative() {
            for doc in docs {
                let Ok(gross) = money(&doc["amount_inc_vat"]) else {
                    continue;
                };
                if gross != Money::default().subtract(&money(&bank["amount"])?) {
                    continue;
                }
                let bank_text = ["text", "reference", "counterparty_name", "message"]
                    .iter()
                    .filter_map(|k| bank[*k].as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                let score = wire::match_score(
                    &bank_text,
                    doc["sender_name"].as_str().unwrap_or(""),
                    doc["invoice_no"].as_str().unwrap_or(""),
                    doc["payment_details"].as_str().unwrap_or(""),
                    date_distance_near(
                        bank["transaction_date"].as_str().unwrap_or(""),
                        doc["invoice_date"].as_str().unwrap_or(""),
                    ),
                );
                if score >= 50 {
                    matches.push((score, doc));
                }
            }
        }
        matches.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(a.1["id"].as_i64().cmp(&b.1["id"].as_i64()))
        });
        // Equal best scores and one document selected by multiple banks require a human choice.
        let chosen = if matches.len() > 1 && matches[0].0 == matches[1].0 {
            None
        } else {
            matches.first().map(|m| m.1)
        };
        choices.push((bank, chosen));
    }
    let mut items = vec![];
    for (bank, doc) in &choices {
        let bank_id = number(bank, "id")?;
        let mut item = json!({"actionKey":format!("bank:{bank_id}"),"bankTransactionId":bank_id,"partition":"missingDocument","evidenceHash":hash(bank),"detail":{"reason":"No unique corroborated original purchase match"}});
        if let Some(doc) = doc {
            let doc_id = number(doc, "id")?;
            let action = format!("purchase:{doc_id}:bank:{bank_id}");
            let unique = choices
                .iter()
                .filter(|(_, d)| d.is_some_and(|d| d["id"] == doc["id"]))
                .count()
                == 1;
            let as_of = today;
            let rule = crate::purchase_rules::inspect(tx, doc_id, bank_id, as_of)?;
            let vat = if rule["decision"] == "proposed" {
                crate::purchase_vat::inspect(
                    tx,
                    doc_id,
                    bank_id,
                    text(&rule["outcome"], "account")?,
                    text(&rule["outcome"], "vatTreatment")?,
                    latest,
                )?
            } else {
                json!({"ok":false,"ready":false,"errors":["Explicit applicable independently reviewed purchase/VAT rule required"]})
            };
            let ready = unique && rule["decision"] == "proposed" && vat["ready"] == true;
            item = json!({"actionKey":action,"bankTransactionId":bank_id,"documentId":doc_id,"partition":if ready{"ready"}else{"humanDecision"},"evidenceHash":hash(&json!({"document":doc,"bank":bank})),"ruleApplication":if rule["decision"]=="proposed"{json!({"ruleVersionId":rule["ruleVersionId"],"payloadHash":rule["payloadHash"]})}else{Value::Null},"detail":{"rule":rule,"vat":vat,"vatReady":vat["ready"],"uniqueMatch":unique,"asOf":as_of}});
        }
        item["detail"]["bank"] = json!({"id":bank["id"],"date":bank["transaction_date"],"text":bank["text"],"amount":bank["amount"],"currency":bank["currency"]});
        if let Some(doc) = doc {
            item["detail"]["document"] = json!({"id":doc["id"],"invoiceNo":doc["invoice_no"],"senderName":doc["sender_name"],"amountIncVat":doc["amount_inc_vat"],"currency":doc["currency"]});
            item["detail"]["proposed"] = item["detail"]["rule"]["outcome"].clone();
        }
        items.push(item);
    }
    items.sort_by(|a, b| a["actionKey"].as_str().cmp(&b["actionKey"].as_str()));
    let candidate = hash(&sources);
    let mut p = json!({"executionPolicy":wire::EXECUTION_POLICY,"companyId":company,"accountingFrom":s.accounting_from,"accountingTo":s.accounting_to,"bankFrom":s.bank_from,"bankTo":s.bank_to,"items":items,"sourceIdentities":{"candidateUniverse":sources},"candidateSetHash":candidate});
    p["planHash"] = json!(hash(&p));
    checked_plan(p)
}
fn event(
    tx: &Transaction<'_>,
    run: i64,
    kind: &str,
    hash: &str,
    actor: &str,
    detail: &Value,
) -> Result<(), Error> {
    tx.execute("INSERT INTO bookkeeping_batch_events(run_id,event_type,plan_hash,actor,detail_json,created_at) VALUES(?1,?2,?3,?4,?5,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![run,kind,hash,actor,canonical(detail)]).map_err(sql)?;
    Ok(())
}
pub fn persist(
    tx: &Transaction<'_>,
    i: &Persist,
    subject: &str,
    latest: Date,
    today: &str,
) -> Result<Value, Error> {
    i.validate()?;
    let p = plan(tx, &i.scope(), latest, today)?;
    if p["planHash"] != i.expected_plan_hash {
        return Err(fail(
            "reviewed plan changed; read and review current complete plan",
        ));
    }
    let prior: Option<(i64, String)> = tx
        .query_row(
            "SELECT id,plan_hash FROM bookkeeping_batch_runs WHERE run_key=?1",
            [&i.run_key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(sql)?;
    if let Some((run, h)) = prior {
        if h != i.expected_plan_hash {
            return Err(fail("runKey binds another plan"));
        }
        return Ok(
            json!({"ok":true,"dryRun":true,"runId":run,"duplicate":true,"plan":p,"state":status(tx,run)?}),
        );
    }
    let actor = format!("user:{subject}");
    tx.execute("INSERT INTO bookkeeping_batch_runs(run_key,company_id,accounting_from,accounting_to,bank_from,bank_to,plan_hash,plan_json,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![i.run_key,p["companyId"].as_i64(),i.accounting_from,i.accounting_to,i.bank_from,i.bank_to,i.expected_plan_hash,canonical(&p)]).map_err(sql)?;
    let run = tx.last_insert_rowid();
    tx.execute("INSERT INTO bookkeeping_batch_revisions(run_id,revision_hash,candidate_set_hash,plan_json,planner_kind,planner_subject_id,planner_actor,created_at) VALUES(?1,?2,?3,?4,'user',?5,?6,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![run,i.expected_plan_hash,p["candidateSetHash"].as_str(),canonical(&p),subject,actor]).map_err(sql)?;
    event(
        tx,
        run,
        "planned",
        &i.expected_plan_hash,
        &actor,
        &json!({"executionPolicy":wire::EXECUTION_POLICY}),
    )?;
    Ok(
        json!({"ok":true,"dryRun":true,"runId":run,"duplicate":false,"plan":p,"state":status(tx,run)?}),
    )
}
fn revision(tx: &Transaction<'_>, i: &Exact) -> Result<Value, Error> {
    i.validate()?;
    rows(
        tx,
        "SELECT * FROM bookkeeping_batch_revisions WHERE run_id=?1 AND revision_hash=?2",
        params![i.run_id, i.plan_hash],
    )?
    .into_iter()
    .next()
    .ok_or_else(|| fail("exact run revision not found"))
}
fn saved(rev: &Value) -> Result<Value, Error> {
    let p: Value =
        serde_json::from_str(text(rev, "plan_json")?).map_err(|_| fail("invalid stored plan"))?;
    if p["executionPolicy"] != wire::EXECUTION_POLICY {
        return Err(fail(
            "Legacy partial/resumable revision is inspect-only; reconcile and review a new atomic_v1 plan",
        ));
    }
    let scope = Scope {
        accounting_from: text(&p, "accountingFrom")?.into(),
        accounting_to: text(&p, "accountingTo")?.into(),
        bank_from: text(&p, "bankFrom")?.into(),
        bank_to: text(&p, "bankTo")?.into(),
        company_id: Some(number(&p, "companyId")?),
    };
    scope.validate()?;
    let universe = &p["sourceIdentities"]["candidateUniverse"];
    for key in ["banks", "documents", "ledger"] {
        if !universe[key].is_array() {
            return Err(fail("invalid stored complete source universe"));
        }
    }
    if p["candidateSetHash"] != hash(universe) {
        return Err(fail("stored candidate universe hash mismatch"));
    }
    let items = p["items"]
        .as_array()
        .ok_or_else(|| fail("invalid stored item set"))?;
    let mut keys = std::collections::BTreeSet::new();
    for item in items {
        let key = text(item, "actionKey")?;
        if key.is_empty() || key.len() > 200 || !keys.insert(key) {
            return Err(fail("invalid/duplicate stored action key"));
        }
        number(item, "bankTransactionId")?;
        wire::hash(text(item, "evidenceHash")?)?;
        if ![
            "ready",
            "suggestedMatch",
            "missingDocument",
            "humanDecision",
        ]
        .contains(&text(item, "partition")?)
        {
            return Err(fail("unknown stored item partition"));
        }
        if item["partition"] == "ready" {
            number(item, "documentId")?;
            if item["detail"]["rule"]["decision"] != "proposed"
                || item["detail"]["vat"]["ready"] != true
            {
                return Err(fail("stored ready item lacks actual rule/VAT proofs"));
            }
        }
    }
    let mut check = p.clone();
    let h = check
        .as_object_mut()
        .ok_or_else(|| fail("stored plan must be object"))?
        .remove("planHash")
        .ok_or_else(|| fail("stored plan hash missing"))?;
    if json!(hash(&check)) != h || h != rev["revision_hash"] {
        return Err(fail("stored plan hash mismatch"));
    }
    Ok(p)
}
pub fn approve(tx: &Transaction<'_>, i: &Exact, subject: &str) -> Result<Value, Error> {
    let rev = revision(tx, i)?;
    saved(&rev)?;
    if rev["planner_kind"] == "user" && rev["planner_subject_id"] == subject {
        return Err(fail(
            "Independent current human reviewer required; planner cannot approve own revision",
        ));
    }
    let actor = format!("user:{subject}");
    let n=tx.execute("INSERT INTO bookkeeping_batch_revision_approvals(revision_id,principal_kind,principal_subject_id,actor,created_at) SELECT ?1,'user',?2,?3,strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE NOT EXISTS(SELECT 1 FROM bookkeeping_batch_revision_approvals WHERE revision_id=?1 AND principal_kind='user' AND principal_subject_id=?2)",params![number(&rev,"id")?,subject,actor]).map_err(sql)?;
    if n > 0 {
        event(
            tx,
            i.run_id,
            "approved",
            &i.plan_hash,
            &actor,
            &json!({"principal":{"kind":"user","subjectId":subject}}),
        )?
    }
    Ok(json!({"ok":true,"state":status(tx,i.run_id)?}))
}
pub fn status(tx: &Connection, run: i64) -> Result<Value, Error> {
    wire::id(run)?;
    let r=rows(tx,"SELECT id AS runId,run_key AS runKey,plan_hash AS planHash,plan_json AS plan,created_at AS createdAt FROM bookkeeping_batch_runs WHERE id=?1",[run])?.into_iter().next().ok_or_else(||Error::new("bookkeeping_batch_not_found","batch run not found"))?;
    let revisions = rows(
        tx,
        "SELECT r.id AS revisionId,r.revision_hash AS planHash,r.candidate_set_hash AS candidateSetHash,r.planner_kind AS plannerKind,r.planner_subject_id AS plannerSubjectId,r.planner_actor AS plannerActor,r.created_at AS createdAt,a.principal_kind AS approverKind,a.principal_subject_id AS approverSubjectId,a.actor AS approverActor,a.created_at AS approvedAt FROM bookkeeping_batch_revisions r LEFT JOIN bookkeeping_batch_revision_approvals a ON a.revision_id=r.id WHERE r.run_id=?1 ORDER BY r.id,a.id",
        [run],
    )?;
    let attempts = rows(
        tx,
        "SELECT a.id AS attemptId,a.plan_hash AS planHash,a.principal_kind AS principalKind,a.principal_subject_id AS principalSubjectId,a.actor,a.started_at AS startedAt,e.event_type AS eventType,e.action_key AS actionKey,e.detail_json AS detail,e.created_at AS createdAt FROM bookkeeping_batch_apply_attempts_v2 a LEFT JOIN bookkeeping_batch_apply_events_v2 e ON e.apply_attempt_id=a.id WHERE a.revision_id IN(SELECT id FROM bookkeeping_batch_revisions WHERE run_id=?1) ORDER BY a.id,e.id",
        [run],
    )?;
    let receipts = rows(
        tx,
        "SELECT action_key AS actionKey,receipt_json AS receipt,created_at AS createdAt FROM bookkeeping_batch_item_receipts WHERE run_id=?1 ORDER BY id",
        [run],
    )?;
    let checks = rows(
        tx,
        "SELECT apply_attempt_id AS attemptId,check_name AS name,ok,detail_json AS detail,created_at AS createdAt FROM bookkeeping_batch_final_checks_v2 WHERE apply_attempt_id IN(SELECT id FROM bookkeeping_batch_apply_attempts_v2 WHERE revision_id IN(SELECT id FROM bookkeeping_batch_revisions WHERE run_id=?1)) ORDER BY id",
        [run],
    )?;
    checked_plan(
        json!({"run":r,"revisions":revisions,"attempts":attempts,"receipts":receipts,"finalChecks":checks,"legacyEvents":rows(tx,"SELECT * FROM bookkeeping_batch_events WHERE run_id=?1 ORDER BY id",[run])?,"legacyFinalChecks":rows(tx,"SELECT * FROM bookkeeping_batch_final_checks WHERE run_id=?1 ORDER BY id",[run])?}),
    )
}
pub fn stored_plan(tx: &Transaction<'_>, i: &Exact) -> Result<Value, Error> {
    saved(&revision(tx, i)?)
}
/// Validate complete immutable receipts before allowing replay. No partial-resume path exists.
fn replay(
    tx: &Connection,
    i: &Exact,
    p: &Value,
    check_current: bool,
) -> Result<Option<Value>, Error> {
    let receipts = rows(
        tx,
        "SELECT * FROM bookkeeping_batch_item_receipts WHERE run_id=?1 ORDER BY action_key",
        [i.run_id],
    )?;
    let attempts = rows(
        tx,
        "SELECT a.* FROM bookkeeping_batch_apply_attempts_v2 a JOIN bookkeeping_batch_revisions r ON r.id=a.revision_id WHERE r.run_id=?1 AND a.plan_hash=?2 ORDER BY a.id",
        params![i.run_id, i.plan_hash],
    )?;
    if receipts.is_empty() && attempts.is_empty() {
        let partial:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM bookkeeping_batch_applied_links WHERE run_id=?1 UNION ALL SELECT 1 FROM bookkeeping_batch_item_attempts WHERE run_id=?1)",[i.run_id],|r|r.get(0)).map_err(sql)?;
        if partial {
            return Err(fail("partial native atomic history; no automatic resume"));
        }
        return Ok(None);
    }
    let ready = p["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["partition"] == "ready")
        .collect::<Vec<_>>();
    if ready.is_empty() || receipts.len() != ready.len() || attempts.len() != 1 {
        return Err(fail(
            "Incomplete/conflicting atomic receipt set; no automatic resume",
        ));
    }
    let mut results = vec![];
    for item in ready {
        let r = receipts
            .iter()
            .find(|r| r["action_key"] == item["actionKey"])
            .ok_or_else(|| fail("missing exact receipt"))?;
        let receipt: Value =
            serde_json::from_str(text(r, "receipt_json")?).map_err(|_| fail("corrupt receipt"))?;
        if receipt["actionKey"] != item["actionKey"]
            || receipt["evidenceHash"] != item["evidenceHash"]
            || receipt["planHash"] != i.plan_hash
        {
            return Err(fail("receipt binding mismatch"));
        }
        let links = rows(
            tx,
            "SELECT * FROM bookkeeping_batch_applied_links WHERE run_id=?1 AND action_key=?2",
            params![i.run_id, text(item, "actionKey")?],
        )?;
        let l = links.first().ok_or_else(|| fail("receipt link missing"))?;
        for (a, b) in [
            ("document_id", "documentId"),
            ("bank_transaction_id", "bankTransactionId"),
            ("journal_entry_id", "journalEntryId"),
            ("vat_preflight_id", "vatPreflightId"),
            ("posting_rule_application_id", "postingRuleApplicationId"),
            ("purchase_application_id", "purchaseApplicationId"),
        ] {
            if l[a] != receipt[b] {
                return Err(fail("receipt link/proof mismatch"));
            }
        }
        let je = rows(
            tx,
            "SELECT entry_hash FROM journal_entries WHERE id=?1",
            [number(&receipt, "journalEntryId")?],
        )?;
        if je.first().map(|v| &v["entry_hash"]) != Some(&receipt["journalEntryHash"]) {
            return Err(fail("receipt journal proof mismatch"));
        }
        if receipt["documentId"] != item["documentId"]
            || receipt["bankTransactionId"] != item["bankTransactionId"]
            || l["evidence_hash"] != item["evidenceHash"]
        {
            return Err(fail("receipt source identity mismatch"));
        }
        let proof:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM purchase_posting_applications p JOIN posting_rule_applications r ON r.id=?1 JOIN vat_validation_events v ON v.id=?2 WHERE p.id=?3 AND p.document_id=?4 AND p.bank_transaction_id=?5 AND p.journal_entry_id=?6 AND p.status='posted' AND r.document_id=?4 AND r.rule_version_id=?7 AND r.rule_payload_hash=?8 AND v.document_id=?4 AND v.event_type='preflight_passed')",params![number(&receipt,"postingRuleApplicationId")?,number(&receipt,"vatPreflightId")?,number(&receipt,"purchaseApplicationId")?,number(&receipt,"documentId")?,number(&receipt,"bankTransactionId")?,number(&receipt,"journalEntryId")?,number(&item["detail"]["rule"],"ruleVersionId")?,text(&item["detail"]["rule"],"payloadHash")?],|r|r.get(0)).map_err(sql)?;
        if !proof {
            return Err(fail(
                "real rule/VAT/purchase receipt proof missing or conflicting",
            ));
        }
        let vat_json: String = tx
            .query_row(
                "SELECT detail_json FROM vat_validation_events WHERE id=?1",
                [number(&receipt, "vatPreflightId")?],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let vat: Value = serde_json::from_str(&vat_json)
            .map_err(|_| fail("invalid stored VAT receipt proof"))?;
        if vat != item["detail"]["vat"] {
            return Err(fail("stored VAT proof differs from reviewed source"));
        }
        let current:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM bank_journal_reconciliations WHERE bank_transaction_id=?1 AND journal_entry_id=?2)",params![number(&receipt,"bankTransactionId")?,number(&receipt,"journalEntryId")?],|r|r.get(0)).map_err(sql)?;
        if check_current && !current {
            return Err(fail(
                "receipt no longer matches canonical bank relation; reconcile before replay",
            ));
        }
        results
            .push(json!({"actionKey":item["actionKey"],"outcome":"duplicate","receipt":receipt}));
    }
    let checks = rows(
        tx,
        "SELECT * FROM bookkeeping_batch_final_checks_v2 WHERE apply_attempt_id=?1 ORDER BY check_name",
        [number(&attempts[0], "id")?],
    )?;
    if checks.len() != 4 || checks.iter().any(|c| c["ok"] != 1) {
        return Err(fail("missing successful atomic final checks"));
    }
    let completed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM bookkeeping_batch_apply_events_v2 WHERE apply_attempt_id=?1 AND event_type='completed')",[number(&attempts[0],"id")?],|r|r.get(0)).map_err(sql)?;
    if !completed {
        return Err(fail("atomic completion marker missing"));
    }
    let projected_checks=checks.iter().map(|c|Ok(json!({"name":c["check_name"],"ok":c["ok"]==1,"detail":serde_json::from_str::<Value>(text(c,"detail_json")?).map_err(|_|fail("invalid final-check receipt"))?}))).collect::<Result<Vec<_>,Error>>()?;
    Ok(Some(
        json!({"ok":true,"runId":i.run_id,"planHash":i.plan_hash,"results":results,"checks":projected_checks,"applyAttemptId":attempts[0]["id"],"duplicate":true,"state":status(tx,i.run_id)?}),
    ))
}
pub fn apply(
    tx: &Transaction<'_>,
    i: &Exact,
    subject: &str,
    latest: Date,
    today: &str,
) -> Result<Value, Error> {
    let rev = revision(tx, i)?;
    let p = saved(&rev)?;
    let approved:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM bookkeeping_batch_revision_approvals WHERE revision_id=?1 AND principal_kind='user' AND principal_subject_id<>?2)",params![number(&rev,"id")?,text(&rev,"planner_subject_id")?],|r|r.get(0)).map_err(sql)?;
    if !approved {
        return Err(fail("exact revision requires independent approval"));
    }
    if let Some(replay) = replay(tx, i, &p, true)? {
        return Ok(replay);
    }
    let scope = Scope {
        accounting_from: text(&p, "accountingFrom")?.into(),
        accounting_to: text(&p, "accountingTo")?.into(),
        bank_from: text(&p, "bankFrom")?.into(),
        bank_to: text(&p, "bankTo")?.into(),
        company_id: p["companyId"].as_i64(),
    };
    let current = plan(tx, &scope, latest, today)?;
    if current != p {
        return Err(fail(
            "Complete candidate universe or reviewed evidence changed; persist and independently review a new revision",
        ));
    }
    let ready = p["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["partition"] == "ready")
        .collect::<Vec<_>>();
    if ready.is_empty() {
        return Err(fail("no ready items to apply"));
    }
    let actor = format!("user:{subject}");
    tx.execute("INSERT INTO bookkeeping_batch_apply_attempts_v2(revision_id,plan_hash,principal_kind,principal_subject_id,actor,started_at) VALUES(?1,?2,'user',?3,?4,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![number(&rev,"id")?,i.plan_hash,subject,actor]).map_err(sql)?;
    let attempt = tx.last_insert_rowid();
    apply_event(
        tx,
        attempt,
        "started",
        None,
        &json!({"executionPolicy":wire::EXECUTION_POLICY}),
    )?;
    event(
        tx,
        i.run_id,
        "apply_started",
        &i.plan_hash,
        &actor,
        &json!({"applyAttemptId":attempt}),
    )?;
    let mut results = vec![];
    for item in ready {
        let action = text(item, "actionKey")?;
        let document = number(item, "documentId")?;
        let bank = number(item, "bankTransactionId")?;
        let key = format!("batch:{}:{action}", i.plan_hash);
        let rule = crate::purchase_rules::apply(
            tx,
            document,
            bank,
            text(&item["detail"], "asOf")?,
            &item["detail"]["rule"],
            &format!("{key}:rule"),
            &actor,
        )?;
        let outcome = &rule["outcome"];
        let input = crate::expense::Input {
            document_id: document,
            bank_transaction_id: bank,
            expense_account_no: text(outcome, "account")?.into(),
            vat_treatment: Some(text(outcome, "vatTreatment")?.into()),
            payment_account_no: None,
            transaction_date: None,
            text: None,
        };
        let posted = crate::purchase_vat::apply(
            tx,
            &input,
            &item["detail"]["vat"],
            &format!("{key}:purchase"),
            &actor,
            latest,
        )?;
        let entry = number(&posted, "entryId")?;
        let dimensions = assign_dimensions(
            tx,
            entry,
            text(outcome, "account")?,
            &outcome["dimensions"],
            &actor,
        )?;
        let entry_hash: String = tx
            .query_row(
                "SELECT entry_hash FROM journal_entries WHERE id=?1",
                [entry],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let receipt = json!({"actionKey":action,"planHash":i.plan_hash,"evidenceHash":item["evidenceHash"],"outcome":"applied","documentId":document,"bankTransactionId":bank,"journalEntryId":entry,"journalEntryHash":entry_hash,"vatPreflightId":posted["vatPreflightId"],"postingRuleApplicationId":rule["postingRuleApplicationId"],"purchaseApplicationId":posted["purchaseApplicationId"],"dimensionAssignmentIds":dimensions});
        tx.execute("INSERT INTO bookkeeping_batch_applied_links(run_id,action_key,document_id,journal_entry_id,bank_transaction_id,vat_preflight_id,posting_rule_application_id,purchase_application_id,evidence_hash,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![i.run_id,action,document,entry,bank,number(&posted,"vatPreflightId")?,number(&rule,"postingRuleApplicationId")?,number(&posted,"purchaseApplicationId")?,text(item,"evidenceHash")?]).map_err(sql)?;
        tx.execute("INSERT INTO bookkeeping_batch_item_receipts(run_id,action_key,receipt_json,created_at) VALUES(?1,?2,?3,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![i.run_id,action,canonical(&receipt)]).map_err(sql)?;
        tx.execute("INSERT INTO bookkeeping_batch_item_attempts(run_id,action_key,evidence_hash,outcome,created_at) VALUES(?1,?2,?3,'applied',strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![i.run_id,action,text(item,"evidenceHash")?]).map_err(sql)?;
        apply_event(tx, attempt, "item_applied", Some(action), &receipt)?;
        results.push(json!({"actionKey":action,"outcome":"applied","receipt":receipt}));
        #[cfg(test)]
        if results.len() == 1 {
            tests::fault(1)?;
        }
    }
    #[cfg(test)]
    tests::fault(2)?;
    let checks = final_checks(tx, &scope)?;
    for c in checks.as_array().unwrap() {
        if c["ok"] != true {
            return Err(fail(format!(
                "mandatory {} final check failed; entire batch rolled back: {}",
                text(c, "name")?,
                canonical(&c["detail"])
            )));
        }
        tx.execute("INSERT INTO bookkeeping_batch_final_checks_v2(apply_attempt_id,check_name,ok,detail_json,created_at) VALUES(?1,?2,1,?3,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![attempt,text(c,"name")?,canonical(&c["detail"])]).map_err(sql)?;
    }
    apply_event(tx, attempt, "final_checks", None, &checks)?;
    apply_event(
        tx,
        attempt,
        "completed",
        None,
        &json!({"count":results.len()}),
    )?;
    event(tx, i.run_id, "final_checks", &i.plan_hash, &actor, &checks)?;
    event(
        tx,
        i.run_id,
        "completed",
        &i.plan_hash,
        &actor,
        &json!({"applyAttemptId":attempt}),
    )?;
    Ok(
        json!({"ok":true,"runId":i.run_id,"planHash":i.plan_hash,"results":results,"checks":checks,"state":status(tx,i.run_id)?,"applyAttemptId":attempt,"duplicate":false}),
    )
}
fn apply_event(
    tx: &Transaction<'_>,
    attempt: i64,
    event: &str,
    action: Option<&str>,
    detail: &Value,
) -> Result<(), Error> {
    tx.execute("INSERT INTO bookkeeping_batch_apply_events_v2(apply_attempt_id,event_type,action_key,detail_json,created_at) VALUES(?1,?2,?3,?4,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![attempt,event,action,canonical(detail)]).map_err(sql)?;
    Ok(())
}
fn assign_dimensions(
    tx: &Transaction<'_>,
    entry: i64,
    account: &str,
    dims: &Value,
    actor: &str,
) -> Result<Value, Error> {
    let mut allocations = vec![];
    if let Some(dims) = dims.as_object() {
        if dims.is_empty() {
            return Ok(json!([]));
        }
        let (id,debit,credit,currency)=tx.query_row("SELECT jl.id,jl.debit_amount,jl.credit_amount,jl.currency FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id WHERE jl.journal_entry_id=?1 AND a.account_no=?2 ORDER BY jl.id LIMIT 1",params![entry,account],|r|Ok((r.get::<_,i64>(0)?,crate::legacy_money(r.get_ref(1)?).map_err(|e|rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,crate::legacy_money(r.get_ref(2)?).map_err(|e|rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,r.get::<_,String>(3)?))).map_err(sql)?;
        let amount = debit
            .subtract(&credit)
            .decimal()
            .trim_start_matches('-')
            .replace('.', "")
            .parse::<i64>()
            .map_err(|_| fail("dimension allocation amount outside exact minor-unit bounds"))?;
        for (d, m) in dims {
            allocations.push(crate::dimensions::Allocation {
                dimension_id: d.clone(),
                member_id: m
                    .as_str()
                    .ok_or_else(|| fail("invalid dimension member"))?
                    .into(),
                amount_minor: amount,
                currency: currency.clone(),
            })
        }
        let mut input = crate::dimensions::AssignmentInput {
            journal_line_id: id,
            allocations,
            source: Some("reviewed".into()),
            reviewed_import: None,
            source_ref: None,
            confirm: Some(true),
            plan_hash: None,
            idempotency_key: None,
            expected_assignment_id: None,
            reason: None,
        };
        let p = crate::dimensions::plan(tx, &input)?;
        input.plan_hash = Some(text(&p["plan"], "planHash")?.into());
        let applied = crate::dimensions::apply(tx, &input, false, actor)?;
        Ok(json!([applied["id"]]))
    } else {
        Err(fail("rule dimensions must be explicit object"))
    }
}

pub fn status_by_key(tx: &Transaction<'_>, key: &str) -> Result<Value, Error> {
    if key.trim().is_empty() || key.len() > 1024 || key.chars().any(char::is_control) {
        return Err(fail("invalid exact runKey"));
    }
    let run: Option<i64> = tx
        .query_row(
            "SELECT id FROM bookkeeping_batch_runs WHERE run_key=?1",
            [key],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    status(
        tx,
        run.ok_or_else(|| Error::new("bookkeeping_batch_not_found", "batch run not found"))?,
    )
}

/// Full signed SQLite snapshot validation. Unversioned source histories stay inspect-only.
/// No grants/principals are adopted; historical identity is retained as source provenance.
pub fn validate_backup(db: &Connection) -> Result<(), Error> {
    let revisions = rows(
        db,
        "SELECT r.*,b.plan_json AS run_plan_json,b.plan_hash AS run_plan_hash FROM bookkeeping_batch_revisions r JOIN bookkeeping_batch_runs b ON b.id=r.run_id ORDER BY r.id",
        [],
    )?;
    for rev in revisions {
        let raw: Value = serde_json::from_str(text(&rev, "plan_json")?)
            .map_err(|_| fail("invalid batch history JSON"))?;
        if raw["executionPolicy"].is_null() {
            continue;
        }
        let p = saved(&rev)?;
        if rev["run_plan_json"] != rev["plan_json"] || rev["run_plan_hash"] != rev["revision_hash"]
        {
            return Err(fail("native revision/run binding mismatch"));
        }
        let exact = Exact {
            run_id: number(&rev, "run_id")?,
            plan_hash: text(&rev, "revision_hash")?.into(),
            confirm: true,
        };
        let self_review:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM bookkeeping_batch_revision_approvals WHERE revision_id=?1 AND principal_kind=?2 AND principal_subject_id=?3)",params![number(&rev,"id")?,text(&rev,"planner_kind")?,text(&rev,"planner_subject_id")?],|r|r.get(0)).map_err(sql)?;
        if self_review {
            return Err(fail("native batch contains self approval"));
        }
        if replay(db, &exact, &p, false)?.is_some() {
            let approvals:i64=db.query_row("SELECT count(*) FROM bookkeeping_batch_revision_approvals WHERE revision_id=?1",[number(&rev,"id")?],|r|r.get(0)).map_err(sql)?;
            if approvals == 0 {
                return Err(fail("applied native batch lacks independent approval"));
            }
        }
    }
    Ok(())
}
