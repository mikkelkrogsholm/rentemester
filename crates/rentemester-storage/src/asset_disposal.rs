//! Capitalized-asset derecognition over the existing admitted journal and receipt writer.
use crate::{
    assets,
    idempotency::{self, PrincipalKind},
    invalid, legacy_money,
};
use rentemester_engine::{
    Error,
    currency::Currency,
    fiscal_year::Date,
    identifiers::DocumentId,
    manual_journal::{self, ManualJournal},
    money::Money,
    sales_invoice::{refused, wire},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preview {
    pub transaction_date: String,
    pub document_id: i64,
    pub reason: String,
    pub loss_account_no: Option<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Dispose {
    pub transaction_date: String,
    pub document_id: i64,
    pub reason: String,
    pub loss_account_no: Option<String>,
    pub expected_plan_hash: String,
    pub idempotency_key: String,
    pub confirm: bool,
}
impl Dispose {
    pub fn preview(&self) -> Preview {
        Preview {
            transaction_date: self.transaction_date.clone(),
            document_id: self.document_id,
            reason: self.reason.clone(),
            loss_account_no: self.loss_account_no.clone(),
        }
    }
}
pub fn validate_key(key: &str) -> Result<String, Error> {
    if !(8..=160).contains(&key.encode_utf16().count())
        || key.trim_matches(crate::idempotency::whitespace).is_empty()
    {
        return Err(Error::new(
            "invalid_input",
            "disposal key must be 8..160 UTF16 units",
        ));
    }
    Ok(format!("{:x}", Sha256::digest(key.as_bytes())))
}
fn json_money(v: &Value) -> Result<Money, Error> {
    rentemester_engine::document::money(
        v.as_number()
            .ok_or_else(|| refused("invalid retained disposal amount"))?,
    )
}
fn positive(id: i64) -> Result<(), Error> {
    if !(1..=9_007_199_254_740_991).contains(&id) {
        return Err(Error::new("invalid_input", "positive safe ID required"));
    }
    Ok(())
}
fn require(db: &Connection) -> Result<(), Error> {
    crate::journal_preview::require_current(db)?;
    if !crate::asset_disposal_schema::installed(db)? {
        return Err(Error::new(
            "unsupported_schema",
            "asset disposal migration required",
        ));
    }
    Ok(())
}
/// Also verifies immutable journal hashes; a damaged/reversed disposal must never reactivate assets.
pub fn record(db: &Connection, id: i64) -> Result<Option<Value>, Error> {
    if !crate::asset_disposal_schema::installed(db)? {
        return Ok(None);
    }
    let row: Option<(i64, String)> = db
        .query_row(
            "SELECT journal_entry_id,receipt_json FROM native_asset_disposals WHERE asset_id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(invalid)?;
    let Some((entry, raw)) = row else {
        return Ok(None);
    };
    let v: Value = serde_json::from_str(&raw).map_err(invalid)?;
    let (status,previous,hash,reversed):(String,String,String,bool)=db.query_row("SELECT status,previous_hash,entry_hash,EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) FROM journal_entries j WHERE id=?1",[entry],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(invalid)?;
    if status != "posted"
        || reversed
        || crate::journal_hash::stored_hash(db, entry, &previous)? != hash
        || v["journalEntryHash"] != hash
        || v["assetId"] != id
        || v["journalEntryId"] != entry
    {
        return Err(refused("disposal evidence inconsistent"));
    }
    let (disposal,date,document,reason,loss,cost,accumulated,carrying):(i64,String,i64,String,Option<String>,String,String,String)=db.query_row("SELECT id,transaction_date,document_id,reason,loss_account_no,cost,accumulated,carrying FROM native_asset_disposals WHERE asset_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).map_err(invalid)?;
    let cost = Money::parse(&cost)?;
    let accumulated = Money::parse(&accumulated)?;
    let carrying = Money::parse(&carrying)?;
    let registered_cost = db
        .query_row("SELECT cost FROM assets WHERE id=?1", [id], |r| {
            Ok(legacy_money(r.get_ref(0)?))
        })
        .map_err(invalid)??;
    let mut booked = Money::default();
    let mut query = db
        .prepare(
            "SELECT amount FROM asset_depreciation_entries WHERE asset_id=?1 ORDER BY period_index",
        )
        .map_err(invalid)?;
    let mut rows = query.query([id]).map_err(invalid)?;
    while let Some(row) = rows.next().map_err(invalid)? {
        booked.add(&legacy_money(row.get_ref(0).map_err(invalid)?)?);
    }
    if registered_cost != cost || booked != accumulated {
        return Err(refused("disposal source values mismatch"));
    }
    if cost.is_negative()
        || accumulated.is_negative()
        || carrying.is_negative()
        || cost.subtract(&accumulated) != carrying
        || v["disposalId"] != disposal
        || v["transactionDate"] != date
        || v["documentId"] != document
        || v["reason"] != reason
        || json_money(&v["cost"])? != cost
        || json_money(&v["accumulatedDepreciation"])? != accumulated
        || json_money(&v["carryingValueWrittenOff"])? != carrying
        || v["remainingCarryingValue"] != 0
        || v["replayed"] != false
    {
        return Err(refused("disposal retained values mismatch"));
    }
    let (asset, accum): (String, String) = db
        .query_row(
            "SELECT asset_account_no,accumulated_depreciation_account_no FROM assets WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(invalid)?;
    let basis = journal_basis(db, entry, document, &date)?;
    let mut expected = vec![line(&asset, &Money::default(), &cost)?];
    if accumulated != Money::default() {
        expected.push(line(&accum, &accumulated, &Money::default())?);
    }
    if carrying != Money::default() {
        expected.push(line(
            loss.as_deref()
                .ok_or_else(|| refused("missing retained loss account"))?,
            &carrying,
            &Money::default(),
        )?);
    }
    let mut actual = basis["lines"]
        .as_array()
        .ok_or_else(|| refused("missing disposal lines"))?
        .clone();
    for l in &mut actual {
        if !l["vatCode"].is_null() {
            return Err(refused("disposal VAT mismatch"));
        }
        l.as_object_mut().unwrap().remove("vatCode");
    }
    expected.sort_by_key(Value::to_string);
    actual.sort_by_key(Value::to_string);
    if expected != actual {
        return Err(refused("disposal journal lines mismatch"));
    }
    let number: String = db
        .query_row(
            "SELECT entry_no FROM journal_entries WHERE id=?1",
            [entry],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if v["journalEntryNo"] != number {
        return Err(refused("disposal journal number mismatch"));
    }
    Ok(Some(v))
}
pub fn ensure_active(db: &Connection, id: i64) -> Result<(), Error> {
    if record(db, id)?.is_some() {
        return Err(refused("asset is disposed"));
    }
    Ok(())
}
fn journal_basis(db: &Connection, id: i64, document: i64, date: &str) -> Result<Value, Error> {
    let (status,source,actual,previous,hash,reversal):(String,Option<i64>,String,String,String,bool)=db.query_row("SELECT status,document_id,transaction_date,previous_hash,entry_hash,reversal_of_entry_id IS NOT NULL OR EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) FROM journal_entries j WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(invalid)?;
    let dkk: bool = db
        .query_row(
            "SELECT currency='DKK' FROM journal_entries WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if status != "posted"
        || !dkk
        || source != Some(document)
        || actual != date
        || reversal
        || crate::journal_hash::stored_hash(db, id, &previous)? != hash
    {
        return Err(refused(
            "asset source journal date/source/hash/status mismatch",
        ));
    }
    let mut q=db.prepare("SELECT a.account_no,l.debit_amount,l.credit_amount,l.vat_code FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1 ORDER BY l.id").map_err(invalid)?;
    let mut rows = q.query([id]).map_err(invalid)?;
    let mut lines = vec![];
    while let Some(r) = rows.next().map_err(invalid)? {
        lines.push(json!({"accountNo":r.get::<_,String>(0).map_err(invalid)?,"debit":wire(&legacy_money(r.get_ref(1).map_err(invalid)?)?)?,"credit":wire(&legacy_money(r.get_ref(2).map_err(invalid)?)?)?,"vatCode":r.get::<_,Option<String>>(3).map_err(invalid)?}));
    }
    Ok(json!({"id":id,"date":date,"hash":hash,"lines":lines}))
}
fn line(account: &str, debit: &Money, credit: &Money) -> Result<Value, Error> {
    Ok(json!({"accountNo":account,"debit":wire(debit)?,"credit":wire(credit)?}))
}
fn basis(
    tx: &Transaction<'_>,
    id: i64,
    i: &Preview,
    latest: Date,
) -> Result<(Value, ManualJournal), Error> {
    require(tx)?;
    positive(id)?;
    positive(i.document_id)?;
    if i.reason.trim().is_empty()
        || i.reason.encode_utf16().count() > 4096
        || i.reason
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
    {
        return Err(Error::new(
            "invalid_input",
            "bounded nonblank disposal reason required",
        ));
    }
    let date = Date::parse(&i.transaction_date)?;
    ensure_active(tx, id)?;
    let a = assets::load(tx, id)?;
    if date < Date::parse(&a.acquisition)? || date > latest {
        return Err(refused("disposal date outside allowed range"));
    }
    let metadata = crate::expense::metadata(tx, a.document)?;
    if metadata.currency.as_deref().unwrap_or("DKK") != "DKK"
        || !matches!(metadata.kind(), "purchase_sale" | "cash_register_receipt")
    {
        return Err(refused(
            "asset purchase evidence requires DKK purchase original",
        ));
    }
    let asset: String = tx
        .query_row(
            "SELECT asset_account_no FROM assets WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    assets::account(tx, Some(&asset), "asset", "debit")?;
    assets::account(tx, Some(&a.accumulated), "asset", "credit")?;
    assets::account(tx, Some(&a.expense), "expense", "debit")?;
    if asset == a.accumulated || a.expense == asset || a.expense == a.accumulated {
        return Err(refused("overlapping asset mappings"));
    }
    let mut evidence = vec![];
    let mut available = Money::default();
    let mut q=tx.prepare("SELECT DISTINCT j.id,j.transaction_date FROM journal_entries j JOIN journal_lines l ON l.journal_entry_id=j.id JOIN accounts ac ON ac.id=l.account_id WHERE j.document_id=?1 AND ac.account_no=?2 AND j.transaction_date<=?3 ORDER BY j.id").map_err(invalid)?;
    let refs = q
        .query_map(params![a.document, asset, a.acquisition], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    for (entry, date) in refs {
        let v = journal_basis(tx, entry, a.document, &date)?;
        for l in v["lines"].as_array().unwrap() {
            if l["accountNo"] == asset {
                available.add(&rentemester_engine::document::money(
                    l["debit"]
                        .as_number()
                        .ok_or_else(|| refused("source debit"))?,
                )?);
                available = available.subtract(&rentemester_engine::document::money(
                    l["credit"]
                        .as_number()
                        .ok_or_else(|| refused("source credit"))?,
                )?);
            }
        }
        evidence.push(v);
    }
    let mut q=tx.prepare("SELECT cost FROM assets WHERE purchase_document_id=?1 AND asset_account_no=?2 ORDER BY id").map_err(invalid)?;
    let mut rows = q.query(params![a.document, asset]).map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        available = available.subtract(&legacy_money(r.get_ref(0).map_err(invalid)?)?);
    }
    if available.is_negative() {
        return Err(refused("capitalized acquisition evidence insufficient"));
    }
    let (_, accumulated) = assets::posted(tx, &a)?;
    let mut q=tx.prepare("SELECT journal_entry_id,transaction_date,amount FROM asset_depreciation_entries WHERE asset_id=?1 ORDER BY period_index").map_err(invalid)?;
    let mut rows = q.query([id]).map_err(invalid)?;
    while let Some(r) = rows.next().map_err(invalid)? {
        let entry = r.get::<_, i64>(0).map_err(invalid)?;
        let date = r.get::<_, String>(1).map_err(invalid)?;
        let amount = legacy_money(r.get_ref(2).map_err(invalid)?)?;
        if Date::parse(&date)? > Date::parse(&i.transaction_date)?
            || Date::parse(&date)? < Date::parse(&a.acquisition)?
        {
            return Err(refused("disposal precedes booked depreciation"));
        }
        let v = journal_basis(tx, entry, a.document, &date)?;
        let mut expected = vec![
            line(&a.expense, &amount, &Money::default())?,
            line(&a.accumulated, &Money::default(), &amount)?,
        ];
        let mut actual = v["lines"].as_array().unwrap().clone();
        for l in &actual {
            if !l["vatCode"].is_null() {
                return Err(refused("depreciation VAT evidence mismatch"));
            }
        }
        for l in &mut actual {
            l.as_object_mut().unwrap().remove("vatCode");
        }
        expected.sort_by_key(Value::to_string);
        actual.sort_by_key(Value::to_string);
        if expected != actual {
            return Err(refused("depreciation journal accounts/amounts mismatch"));
        }
        evidence.push(v);
    }
    let carrying = a.cost.subtract(&accumulated);
    if carrying.is_negative() {
        return Err(refused("negative carrying value"));
    }
    let loss = if carrying == Money::default() {
        None
    } else {
        Some(assets::account(
            tx,
            i.loss_account_no.as_deref(),
            "expense",
            "debit",
        )?)
    };
    let mut lines = vec![line(&asset, &Money::default(), &a.cost)?];
    if accumulated != Money::default() {
        lines.push(line(&a.accumulated, &accumulated, &Money::default())?);
    }
    if let Some(loss) = &loss {
        lines.push(line(loss, &carrying, &Money::default())?);
    }
    let journal = ManualJournal {
        transaction_date: date,
        text: format!("Asset disposal {}: {}", a.name, i.reason),
        currency: Currency::parse("DKK")?,
        document_id: Some(DocumentId::from_sqlite(i.document_id)),
        source_bank_transaction_id: None,
        amount_foreign: None,
        amount_dkk: None,
        fx_rate_to_dkk: None,
        created_by_program: None,
        lines: lines
            .iter()
            .map(|l| {
                crate::expense::line(
                    l["accountNo"].as_str().unwrap(),
                    rentemester_engine::document::money(l["debit"].as_number().unwrap())?,
                    rentemester_engine::document::money(l["credit"].as_number().unwrap())?,
                    None,
                    "Asset disposal".into(),
                )
            })
            .collect::<Result<_, Error>>()?,
    };
    let rules = crate::journal_preview::compiled_rules()?;
    let facts = crate::journal_preview::collect_facts(tx, &journal, latest)?;
    if !manual_journal::validate(&journal, &facts, &rules, rules.version())?.ok {
        return Err(refused(
            "disposal journal refused by current period/account controls",
        ));
    }
    let sources = [
        crate::documents::evidence(tx, a.document)?,
        crate::documents::evidence(tx, i.document_id)?,
    ];
    let mut preview = json!({"assetId":id,"transactionDate":i.transaction_date,"documentId":i.document_id,"reason":i.reason,"lossAccountNo":loss,"cost":wire(&a.cost)?,"accumulatedDepreciation":wire(&accumulated)?,"carryingValue":wire(&carrying)?,"lines":lines});
    let hash = idempotency::canonical_payload_hash(
        &json!({"intent":preview,"asset":{"acquisition":a.acquisition,"months":a.months,"expense":a.expense,"accumulated":a.accumulated},"journalEvidence":evidence,"originals":sources.iter().map(|s|json!({"path":s.path,"hash":s.hash})).collect::<Vec<_>>() }),
    )?;
    preview["planHash"] = json!(hash);
    Ok((preview, journal))
}
pub fn preview(tx: &Transaction<'_>, id: i64, i: &Preview, latest: Date) -> Result<Value, Error> {
    Ok(json!({"ok":true,"disposalPreview":basis(tx,id,i,latest)?.0}))
}
pub fn dispose(
    tx: &Transaction<'_>,
    id: i64,
    i: &Dispose,
    subject: &str,
    kind: PrincipalKind,
    now: i64,
    latest: Date,
) -> Result<Value, Error> {
    require(tx)?;
    positive(id)?;
    let key = validate_key(&i.idempotency_key)?;
    if !i.confirm
        || i.expected_plan_hash.len() != 64
        || !i
            .expected_plan_hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::new(
            "invalid_input",
            "reviewed hash and confirmation required",
        ));
    }
    let payload = json!({"assetId":id,"intent":i});
    let actor = crate::accruals::principal(subject, kind)?;
    let outcome = idempotency::execute(
        tx,
        idempotency::Input {
            key: &key,
            operation: idempotency::Operation::AssetDispose,
            principal_kind: kind,
            subject,
            payload: &payload,
            created_by: &actor,
            created_by_program: "rentemester-rust",
            now_ms: now,
        },
        |tx| {
            let (plan, journal) = basis(tx, id, &i.preview(), latest)?;
            if plan["planHash"] != i.expected_plan_hash {
                return Err(refused("stale disposal plan"));
            }
            let journal = crate::journal_post::post(
                tx,
                &journal,
                &crate::journal_preview::manual_journal_rule_version()?,
                latest,
                crate::journal_post::Actor { subject: &actor, program: "rentemester-rust" },
            )?;
            if journal["ok"] != true {
                return Err(refused("disposal journal refused"));
            }
            let disposal_id: i64 = tx.query_row(
                "SELECT COALESCE(MAX(id),0)+1 FROM native_asset_disposals", [], |r| r.get(0)
            ).map_err(invalid)?;
            positive(disposal_id)?;
            let receipt = json!({
                "disposalId": disposal_id,
                "assetId": id,
                "transactionDate": i.transaction_date,
                "documentId": i.document_id,
                "reason": i.reason,
                "cost": plan["cost"],
                "accumulatedDepreciation": plan["accumulatedDepreciation"],
                "carryingValueWrittenOff": plan["carryingValue"],
                "remainingCarryingValue": 0,
                "journalEntryId": journal["entryId"],
                "journalEntryNo": journal["entryNo"],
                "journalEntryHash": journal["entryHash"],
                "replayed": false
            });
            let receipt: Value = serde_json::from_str(&idempotency::outcome_json(&receipt)?).map_err(invalid)?;
            tx.execute(
                "INSERT INTO native_asset_disposals(id,asset_id,transaction_date,document_id,reason,loss_account_no,cost,accumulated,carrying,plan_hash,journal_entry_id,receipt_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![disposal_id,id,i.transaction_date,i.document_id,i.reason,
                    plan["lossAccountNo"].as_str(),plan["cost"].to_string(),
                    plan["accumulatedDepreciation"].to_string(),plan["carryingValue"].to_string(),
                    i.expected_plan_hash,journal["entryId"].as_i64(),receipt.to_string()],
            ).map_err(invalid)?;
            assets::audit(tx, "asset_dispose", id, &actor)?;
            Ok(json!({"ok":true,"disposal":receipt}))
        },
    ).map_err(|failure| failure.error)?;
    match outcome {
        idempotency::Outcome::Accepted {
            mut result,
            receipt,
        } => {
            verify_receipt(tx, id, &result)?;
            result["disposal"]["replayed"] = json!(receipt.replayed);
            Ok(result)
        }
        idempotency::Outcome::Refused { code } => {
            Err(Error::new(code, "disposal key conflict or expired receipt"))
        }
    }
}

pub fn receipt(
    tx: &Transaction<'_>,
    id: i64,
    key: &str,
    subject: &str,
    kind: PrincipalKind,
) -> Result<Value, Error> {
    require(tx)?;
    positive(id)?;
    let key = validate_key(key)?;
    let hash = format!("{:x}", Sha256::digest(key.as_bytes()));
    let ledger: String = tx
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let imported =
        crate::receipt_reconciliation::imported_ids(tx, &hash, &ledger, "asset_dispose")?;
    let mut result = if !imported.is_empty() {
        if imported.len() != 1 || !matches!(kind, PrincipalKind::User) {
            return Err(refused("restored key reconciliation required"));
        }
        let payload: String = tx
            .query_row(
                "SELECT payload_hash FROM mutation_idempotency_tombstones WHERE id=?1",
                [imported[0]],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        crate::receipt_reconciliation::mapped_replay(tx, imported[0], subject, &payload)?
            .ok_or_else(|| refused("restored key reconciliation required"))?
            .0
    } else {
        let raw:Option<String>=tx.query_row("SELECT o.outcome_json FROM mutation_idempotency_tombstones t JOIN mutation_idempotency_outcomes o ON o.tombstone_id=t.id WHERE t.client_key_hash=?1 AND t.ledger_uuid=?2 AND t.operation='asset_dispose' AND t.principal_kind=?3 AND t.principal_subject_id=?4",params![hash,ledger,if matches!(kind,PrincipalKind::User){"user"}else{"service-account"},subject],|r|r.get(0)).optional().map_err(invalid)?;
        serde_json::from_str(
            &raw.ok_or_else(|| Error::new("not_found", "disposal receipt not found"))?,
        )
        .map_err(invalid)?
    };
    if result["disposal"]["assetId"] != id {
        return Err(Error::new("not_found", "disposal receipt not found"));
    }
    verify_receipt(tx, id, &result)?;
    result["disposal"]["replayed"] = json!(true);
    Ok(result)
}
fn verify_receipt(db: &Connection, id: i64, result: &Value) -> Result<(), Error> {
    let retained = record(db, id)?.ok_or_else(|| refused("missing disposal history"))?;
    if idempotency::outcome_json(result)?
        != idempotency::outcome_json(&json!({"ok":true,"disposal":retained}))?
    {
        return Err(refused("disposal receipt and retained journal differ"));
    }
    Ok(())
}
pub fn validate_backup(db: &Connection) -> Result<(), Error> {
    if !crate::asset_disposal_schema::installed(db)? {
        return Ok(());
    }
    let mut q = db
        .prepare("SELECT asset_id FROM native_asset_disposals ORDER BY id")
        .map_err(invalid)?;
    let ids = q
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    for id in ids {
        record(db, id)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
