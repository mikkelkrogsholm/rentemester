//! Native bank master data and imported statement rows on the admitted transaction.
use rentemester_engine::{Error, bank::Row, currency::Currency, money::Money};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
fn fail(message: impl Into<String>) -> Error {
    Error::new("bank_refused", message)
}
fn sql(_: rusqlite::Error) -> Error {
    fail("bank storage unavailable; abort transaction")
}
fn audit(
    tx: &Transaction<'_>,
    event: &str,
    id: &str,
    message: &str,
    actor: &str,
) -> Result<(), Error> {
    audit_with_principal(
        tx,
        event,
        id,
        message,
        actor,
        crate::idempotency::PrincipalKind::User,
    )
}
fn audit_with_principal(
    tx: &Transaction<'_>,
    event: &str,
    id: &str,
    message: &str,
    actor: &str,
    principal: crate::idempotency::PrincipalKind,
) -> Result<(), Error> {
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'bank',?2,?3,?4)",params![event,id,message,format!("{}:{actor}", match principal {crate::idempotency::PrincipalKind::User=>"user",crate::idempotency::PrincipalKind::ServiceAccount=>"service_account"})]).map_err(sql)?;
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AccountInput {
    pub name: String,
    pub slug: Option<String>,
    pub bank_name: Option<String>,
    pub registration_no: Option<String>,
    pub account_no: Option<String>,
    pub iban: Option<String>,
    pub bic: Option<String>,
    pub account_owner: Option<String>,
    pub customer_no: Option<String>,
    pub currency: Option<String>,
    pub ledger_account_no: Option<String>,
}
pub fn create_account(
    tx: &Transaction<'_>,
    input: &AccountInput,
    actor: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let name = input.name.trim();
    if name.is_empty() || name.len() > 256 {
        return Err(fail("bank account name required (max 256 bytes)"));
    }
    let slug = input
        .slug
        .as_deref()
        .unwrap_or(name)
        .trim()
        .to_lowercase()
        .replace('æ', "ae")
        .replace('ø', "o")
        .replace('å', "a")
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() || slug.len() > 128 {
        return Err(fail("a valid bank account slug is required"));
    }
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bank_accounts WHERE slug=?1)",
            [&slug],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if exists {
        return Err(fail("bank account slug already exists"));
    }
    let currency = input
        .currency
        .as_deref()
        .unwrap_or("DKK")
        .trim()
        .to_uppercase();
    Currency::parse(&currency)?;
    let ledger = input
        .ledger_account_no
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(no) = ledger {
        let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND type='asset' AND active=1)",[no],|r|r.get(0)).map_err(sql)?;
        if !valid {
            return Err(fail("bank ledger account must be an active asset account"));
        }
    }
    let clean = |s: &Option<String>| {
        s.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let id:i64=tx.query_row("INSERT INTO bank_accounts(slug,name,bank_name,registration_no,account_no,iban,bic,account_owner,customer_no,currency,ledger_account_no) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) RETURNING id",params![slug,name,clean(&input.bank_name),clean(&input.registration_no),clean(&input.account_no),clean(&input.iban),clean(&input.bic),clean(&input.account_owner),clean(&input.customer_no),currency,ledger],|r|r.get(0)).map_err(sql)?;
    audit(
        tx,
        "bank_account_add",
        &id.to_string(),
        &format!("Added bank account '{slug}' ({name})"),
        actor,
    )?;
    Ok(json!({"id":id,"slug":slug}))
}
fn company(tx: &Transaction<'_>) -> Result<Value, Error> {
    tx.query_row("SELECT name,cvr,country,currency FROM companies WHERE id=1",[],|r|Ok(json!({"name":r.get::<_,String>(0)?,"cvr":r.get::<_,Option<String>>(1)?,"country":r.get::<_,String>(2)?,"currency":r.get::<_,String>(3)?}))).map_err(sql)
}
pub(crate) fn account_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    Ok(
        json!({"id":r.get::<_,i64>(0)?,"slug":r.get::<_,String>(1)?,"name":r.get::<_,String>(2)?,"bankName":r.get::<_,Option<String>>(3)?,"registrationNo":r.get::<_,Option<String>>(4)?,"accountNo":r.get::<_,Option<String>>(5)?,"iban":r.get::<_,Option<String>>(6)?,"bic":r.get::<_,Option<String>>(7)?,"accountOwner":r.get::<_,Option<String>>(8)?,"customerNo":r.get::<_,Option<String>>(9)?,"currency":r.get::<_,String>(10)?,"ledgerAccountNo":r.get::<_,Option<String>>(11)?,"active":r.get::<_,i64>(12)?==1,"createdAt":r.get::<_,String>(13)?}),
    )
}
pub fn accounts(tx: &Transaction<'_>, slug: &str) -> Result<Value, Error> {
    let mut q=tx.prepare("SELECT id,slug,name,bank_name,registration_no,account_no,iban,bic,account_owner,customer_no,currency,ledger_account_no,active,created_at FROM bank_accounts ORDER BY id").map_err(sql)?;
    let rows = q
        .query_map([], account_row)
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(
        json!({"slug":slug,"company":company(tx)?,"accounts":rows,"profiles":rentemester_engine::bank::profiles()}),
    )
}
pub(crate) fn signed_money(value: rusqlite::types::ValueRef<'_>) -> Result<Money, Error> {
    use rusqlite::types::ValueRef;
    match value {
        ValueRef::Real(value) if value < 0.0 => {
            Ok(Money::default().subtract(&crate::legacy_money(ValueRef::Real(-value))?))
        }
        ValueRef::Integer(value) if value < 0 => {
            let positive = value
                .checked_neg()
                .ok_or_else(|| fail("bank amount exceeds supported numeric domain"))?;
            Ok(Money::default().subtract(&crate::legacy_money(ValueRef::Integer(positive))?))
        }
        value => crate::legacy_money(value),
    }
}
fn quoted(s: &str) -> String {
    serde_json::to_string(s).expect("string encoding")
}
fn numeric(m: &Money) -> Result<String, Error> {
    Ok(ryu_js::Buffer::new()
        .format_finite(crate::journal_post::money(m)?)
        .to_owned())
}
fn optional_string(s: Option<&str>) -> String {
    s.map(quoted).unwrap_or_else(|| "null".into())
}
pub(crate) fn fingerprint(
    row: &Row,
    account: Option<i64>,
    occurrence: usize,
) -> Result<String, Error> {
    let rate = row
        .rate
        .as_ref()
        .map(crate::journal_post::rate)
        .transpose()?
        .map(|n| ryu_js::Buffer::new().format_finite(n).to_owned())
        .unwrap_or_else(|| "null".into());
    // Exact existing JS field order and number spelling keep re-import identity stable.
    let wire = format!(
        "{{\"bank_account_id\":{},\"transaction_date\":{},\"booking_date\":{},\"text\":{},\"amount\":{},\"currency\":{},\"reference\":{},\"amount_dkk\":{},\"fx_rate_to_dkk\":{},\"occurrence\":{}}}",
        account
            .map(|n| n.to_string())
            .unwrap_or_else(|| "null".into()),
        quoted(&row.date),
        optional_string(row.booking_date.as_deref()),
        quoted(&row.text),
        numeric(&row.amount)?,
        quoted(&row.currency),
        optional_string(row.reference.as_deref()),
        row.amount_dkk
            .as_ref()
            .map(numeric)
            .transpose()?
            .unwrap_or_else(|| "null".into()),
        rate,
        occurrence
    );
    Ok(format!("{:x}", Sha256::digest(wire.as_bytes())))
}
fn lookup_import_account(
    tx: &Transaction<'_>,
    key: &str,
    numeric: Option<i64>,
) -> Result<Option<(i64, String, i64)>, Error> {
    tx.query_row("SELECT id,slug,active FROM bank_accounts WHERE id=?1 OR slug=?2 ORDER BY (id=?1) DESC LIMIT 1",params![numeric,key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(sql)
}
fn resolve_import_account(
    tx: &Transaction<'_>,
    account: Option<&str>,
) -> Result<Option<(i64, String, i64)>, Error> {
    account
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            let row = lookup_import_account(tx, s, s.parse::<i64>().ok())?
                .ok_or_else(|| fail("unknown bank account"))?;
            if row.2 != 1 {
                return Err(fail("bank account is inactive"));
            }
            Ok(row)
        })
        .transpose()
}
/// Compatibility input resolution is explicit: trimmed exact slug or positive id,
/// omitted/blank stays unassigned. Ordinary refusals precede source publication.
pub fn validate_original_import_account(
    tx: &Transaction<'_>,
    account: Option<&str>,
) -> Result<(), Error> {
    let Some(raw) = account.filter(|s| !s.trim().is_empty()) else {
        return Ok(());
    };
    let key = raw.trim();
    let numeric = key.parse::<i64>().ok().filter(|n| *n > 0);
    let row = lookup_import_account(tx, key, numeric)?.ok_or_else(|| {
        Error::new(
            "bank_import_account_not_found",
            format!("bank account '{raw}' does not exist"),
        )
    })?;
    if row.2 != 1 {
        return Err(Error::new(
            "bank_import_account_inactive",
            format!(
                "bank account '{}' is inactive and cannot receive imports",
                row.1
            ),
        ));
    }
    Ok(())
}
/// Validate the selected account before publishing a physical statement original.
pub fn validate_import_account(tx: &Transaction<'_>, account: Option<&str>) -> Result<(), Error> {
    resolve_import_account(tx, account).map(|_| ())
}
/// Read the declared parser from immutable import provenance; never infer date order.
pub fn source_profile(tx: &Transaction<'_>, bank_id: i64) -> Result<Option<String>, Error> {
    let (source,batch,account):(String,String,Option<i64>)=tx.query_row("SELECT source_file_hash,import_batch_id,bank_account_id FROM bank_transactions WHERE id=?1",[bank_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(sql)?;
    let mut q=tx.prepare("SELECT event_type,message FROM audit_log WHERE event_type IN ('bank_import','bank_import_profile') AND entity_type='bank' AND entity_id=?1 ORDER BY id").map_err(sql)?;
    let mut profile = None;
    let mut required = false;
    for row in q
        .query_map([&batch], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(sql)?
    {
        let (kind, message) = row.map_err(sql)?;
        let parsed = serde_json::from_str::<Value>(&message);
        if kind == "bank_import"
            && parsed
                .as_ref()
                .ok()
                .and_then(|v| v["version"].as_str())
                .is_none_or(|v| !v.starts_with("rentemester-native-bank-import/"))
        {
            continue;
        }
        let value = parsed.map_err(|_| fail("bank import profile proof is invalid"))?;
        let source_hash = value["sourceFileHash"]
            .as_str()
            .ok_or_else(|| fail("bank import profile source hash unavailable"))?;
        if crate::intercompany_dispositions::canonical(&value) != message
            || value["importBatchId"] != batch
            || source_hash.len() != 64
            || !source_hash
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            || !(value["bankAccountId"].is_null()
                || value["bankAccountId"].as_i64().is_some_and(|id| id > 0))
        {
            return Err(fail("bank import profile proof is inconsistent"));
        }
        let matches = source_hash == source && value["bankAccountId"] == json!(account);
        if kind == "bank_import" {
            if value.as_object().is_none_or(|m| m.len() != 8)
                || value["version"] != "rentemester-native-bank-import/1"
                || value["requiredProfile"] != "danske-bank"
                || value["imported"].as_u64().is_none_or(|n| n == 0)
                || value["skippedDuplicates"].as_u64().is_none()
                || value["original"].as_str().is_none_or(str::is_empty)
            {
                return Err(fail("bank import marker is unsupported"));
            }
            if matches {
                if required {
                    return Err(fail("conflicting bank import markers"));
                }
                required = true;
            }
        } else {
            if value.as_object().is_none_or(|m| m.len() != 5)
                || value["version"] != "rentemester-native-bank-profile/1"
                || value["profile"] != "danske-bank"
            {
                return Err(fail("bank import profile proof is unsupported"));
            }
            if matches {
                if profile.is_some() {
                    return Err(fail("conflicting bank import profile proof"));
                }
                profile = Some("danske-bank".into());
            }
        }
    }
    if required != profile.is_some() {
        return Err(fail(
            "named bank import requires its matching immutable profile proof",
        ));
    }
    Ok(profile)
}
pub fn import(
    tx: &Transaction<'_>,
    content: &str,
    rows: &[Row],
    account: Option<&str>,
    actor: &str,
    now: &str,
    archive: &str,
) -> Result<Value, Error> {
    import_bytes(tx, content.as_bytes(), rows, account, actor, now, archive)
}
/// Preserve the exact statement bytes in its source hash and archive lineage.
pub fn import_bytes(
    tx: &Transaction<'_>,
    content: &[u8],
    rows: &[Row],
    account: Option<&str>,
    actor: &str,
    now: &str,
    archive: &str,
) -> Result<Value, Error> {
    import_bytes_with_principal(
        tx,
        content,
        rows,
        account,
        actor,
        now,
        archive,
        crate::idempotency::PrincipalKind::User,
    )
}
#[allow(clippy::too_many_arguments)]
pub fn import_bytes_with_principal(
    tx: &Transaction<'_>,
    content: &[u8],
    rows: &[Row],
    account: Option<&str>,
    actor: &str,
    now: &str,
    archive: &str,
    principal: crate::idempotency::PrincipalKind,
) -> Result<Value, Error> {
    import_bytes_with_receipt(
        tx, content, rows, account, actor, now, archive, principal, false, None,
    )
}
/// Original receipt uses the same importer and transaction-local unmatched sync count.
#[allow(clippy::too_many_arguments)]
pub fn import_original_bytes_with_principal(
    tx: &Transaction<'_>,
    content: &[u8],
    rows: &[Row],
    account: Option<&str>,
    actor: &str,
    now: &str,
    archive: &str,
    principal: crate::idempotency::PrincipalKind,
) -> Result<Value, Error> {
    let account = account.map(str::trim).filter(|s| !s.is_empty());
    validate_original_import_account(tx, account)?;
    import_bytes_with_receipt(
        tx, content, rows, account, actor, now, archive, principal, true, None,
    )
}
/// Same native importer, with an explicitly declared generic chronology.
#[allow(clippy::too_many_arguments)]
pub fn import_ordered_bytes_with_principal(tx: &Transaction<'_>, content: &[u8], rows: &[Row], account: Option<&str>, actor: &str, now: &str, archive: &str, principal: crate::idempotency::PrincipalKind, original: bool, statement_order: Option<&str>) -> Result<Value, Error> {
    if original { validate_original_import_account(tx, account)?; }
    import_bytes_with_receipt(tx,content,rows,account.map(str::trim).filter(|s|!s.is_empty()),actor,now,archive,principal,original,statement_order)
}
#[allow(clippy::too_many_arguments)]
fn import_bytes_with_receipt(
    tx: &Transaction<'_>,
    content: &[u8],
    rows: &[Row],
    account: Option<&str>,
    actor: &str,
    now: &str,
    archive: &str,
    principal: crate::idempotency::PrincipalKind,
    original_receipt: bool,
    statement_order: Option<&str>,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let profile = rows.first().and_then(|row| row.profile.as_deref());
    let direction = crate::bank_statement_order::direction(profile, statement_order)?;
    if direction.is_some() && !crate::bank_statement_order::installed(tx)? { return Err(fail("explicit bank statement order schema upgrade required")); }
    let resolved = resolve_import_account(tx, account)?;
    let account_id = resolved.as_ref().map(|r| r.0);
    let source_hash = format!("{:x}", Sha256::digest(content));
    let batch = format!(
        "BANK-{}-{}",
        now.chars()
            .filter(char::is_ascii_digit)
            .take(14)
            .collect::<String>(),
        &source_hash[..8]
    );
    let month: i32 = tx
        .query_row(
            "SELECT fiscal_year_start_month FROM companies WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let mut occurrence = HashMap::new();
    let mut imported = 0;
    let mut fresh = Vec::new();
    let mut skipped = Vec::new();
    for (source_index,row) in rows.iter().enumerate() {
        let content_hash = fingerprint(row, account_id, 0)?;
        let count = occurrence.entry(content_hash).or_insert(0);
        let hash = fingerprint(row, account_id, *count)?;
        *count += 1;
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM bank_transactions WHERE transaction_hash=?1)",
                [&hash],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if exists {
            if let Some(balance) = &row.balance_after {
                let stored: Option<Money> = tx
                    .query_row(
                        "SELECT balance_after FROM bank_transactions WHERE transaction_hash=?1",
                        [&hash],
                        |r| match r.get_ref(0)? {
                            rusqlite::types::ValueRef::Null => Ok(None),
                            value => signed_money(value)
                                .map(Some)
                                .map_err(|_| rusqlite::Error::InvalidQuery),
                        },
                    )
                    .map_err(sql)?;
                if stored.as_ref() != Some(balance) {
                    return Err(fail(
                        "duplicate bank transaction has different or previously missing statement balance; source evidence requires explicit correction",
                    ));
                }
            }
            skipped.push(format!(
                "{} {} {} {}",
                row.date,
                row.text,
                row.amount.decimal(),
                row.currency
            ));
            continue;
        }
        let retain = rentemester_engine::calendar::retention_deadline(
            row.booking_date.as_deref().unwrap_or(&row.date),
            month as u32,
        )?;
        tx.execute("INSERT INTO bank_transactions(transaction_date,booking_date,text,amount,currency,reference,amount_dkk,fx_rate_to_dkk,source_file_hash,import_batch_id,transaction_hash,status,retain_until,bank_account_id,balance_after,counterparty_name,counterparty_account,message,archive_reference,customer_reference,raw_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'imported',?12,?13,?14,?15,?16,?17,?18,?19,?20)",params![row.date,row.booking_date,row.text,crate::journal_post::money(&row.amount)?,row.currency,row.reference,row.amount_dkk.as_ref().map(crate::journal_post::money).transpose()?,row.rate.as_ref().map(crate::journal_post::rate).transpose()?,source_hash,batch,hash,retain,account_id,row.balance_after.as_ref().map(crate::journal_post::money).transpose()?,row.counterparty_name,row.counterparty_account,row.message,row.archive_reference,row.customer_reference,row.raw.as_ref().map(serde_json::to_string).transpose().map_err(|_|fail("bank raw source cannot be represented"))?]).map_err(sql)?;
        if let Some(direction)=direction {crate::bank_statement_order::record(tx,tx.last_insert_rowid(),source_index,&source_hash,profile,direction)?;}
        fresh.push(BalanceRow {
            id: tx.last_insert_rowid(),
            row,
        });
        imported += 1;
    }
    let profile = rows.first().and_then(|row| row.profile.as_deref());
    let message = if imported > 0 && profile.is_some() {
        crate::intercompany_dispositions::canonical(
            &json!({"version":"rentemester-native-bank-import/1","importBatchId":batch,"sourceFileHash":source_hash,"bankAccountId":account_id,"requiredProfile":profile,"imported":imported,"skippedDuplicates":skipped.len(),"original":archive}),
        )
    } else {
        format!(
            "Imported {imported} bank rows; skipped {} duplicates; source {source_hash}; original {archive}",
            skipped.len()
        )
    };
    audit_with_principal(tx, "bank_import", &batch, &message, actor, principal)?;
    if imported > 0
        && let Some(profile) = profile
    {
        let proof = json!({"version":"rentemester-native-bank-profile/1","importBatchId":batch,"sourceFileHash":source_hash,"bankAccountId":account_id,"profile":profile});
        audit_with_principal(
            tx,
            "bank_import_profile",
            &batch,
            &crate::intercompany_dispositions::canonical(&proof),
            actor,
            principal,
        )?;
    }
    let continuity = verify_balance_continuity(tx, &fresh)?;
    let unmatched = crate::unmatched_bank::sync(tx)?;
    let mut result = json!({"importBatchId":batch,"sourceFileHash":source_hash,"bankAccountId":account_id,"imported":imported,"skippedDuplicates":skipped.len(),"skippedDuplicateRows":skipped,"bankAccountSlug":resolved.map(|r|r.1),"profile":rows.first().and_then(|row|row.profile.as_deref()),"balanceWarnings":continuity.warnings,"exceptionsCreated":if original_receipt {unmatched}else{continuity.created + unmatched}});
    if let Some(first) = continuity.first {
        result["firstBalance"] = json!(crate::journal_post::money(&first)?);
    }
    if let Some(last) = continuity.last {
        result["lastBalance"] = json!(crate::journal_post::money(&last)?);
    }
    if original_receipt {
        for key in ["bankAccountId", "bankAccountSlug", "profile"] {
            if result[key].is_null() {
                result.as_object_mut().expect("receipt object").remove(key);
            }
        }
    }
    Ok(result)
}
#[derive(Clone, Copy)]
struct BalanceRow<'a> {
    id: i64,
    row: &'a Row,
}
struct Continuity {
    warnings: Vec<String>,
    created: usize,
    first: Option<Money>,
    last: Option<Money>,
}
fn order_day<'a>(cluster: &[BalanceRow<'a>], incoming: Option<&Money>) -> Vec<BalanceRow<'a>> {
    if cluster.len() < 2 {
        return cluster.to_vec();
    }
    let balance = |row: BalanceRow<'a>| {
        row.row
            .balance_after
            .as_ref()
            .expect("filtered statement balance")
    };
    let mut running = if let Some(incoming) = incoming {
        incoming.clone()
    } else {
        let after: std::collections::HashSet<_> =
            cluster.iter().map(|r| balance(*r).decimal()).collect();
        let Some(first) = cluster
            .iter()
            .find(|r| !after.contains(&balance(**r).subtract(&r.row.amount).decimal()))
        else {
            return cluster.to_vec();
        };
        balance(*first).subtract(&first.row.amount)
    };
    // This implements the source's first-match greedy walk without a quadratic scan.
    let mut next: HashMap<String, std::collections::VecDeque<usize>> = HashMap::new();
    for (index, row) in cluster.iter().enumerate() {
        next.entry(balance(*row).subtract(&row.row.amount).decimal())
            .or_default()
            .push_back(index);
    }
    let mut ordered = vec![];
    for _ in 0..cluster.len() {
        let Some(index) = next
            .get_mut(&running.decimal())
            .and_then(|indices| indices.pop_front())
        else {
            return cluster.to_vec();
        };
        let row = cluster[index];
        running = balance(row).clone();
        ordered.push(row);
    }
    ordered
}
fn verify_balance_continuity(
    tx: &Transaction<'_>,
    fresh: &[BalanceRow<'_>],
) -> Result<Continuity, Error> {
    let mut rows = fresh
        .iter()
        .filter(|r| r.row.balance_after.is_some())
        .copied()
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| (&a.row.date, a.id).cmp(&(&b.row.date, b.id)));
    let mut ordered: Vec<BalanceRow<'_>> = vec![];
    let mut start = 0;
    while start < rows.len() {
        let mut end = start + 1;
        while end < rows.len() && rows[end].row.date == rows[start].row.date {
            end += 1;
        }
        let incoming = ordered.last().and_then(|r| r.row.balance_after.as_ref());
        ordered.extend(order_day(&rows[start..end], incoming));
        start = end;
    }
    let mut warnings = vec![];
    let mut created = 0;
    for pair in ordered.windows(2) {
        let previous = pair[0]
            .row
            .balance_after
            .as_ref()
            .expect("filtered balance");
        let current = pair[1];
        let statement = current
            .row
            .balance_after
            .as_ref()
            .expect("filtered balance");
        let mut expected = previous.clone();
        expected.add(&current.row.amount);
        if expected == *statement {
            continue;
        }
        let drift = statement.subtract(&expected);
        let warning = format!(
            "balance break before bank transaction {} ({} '{}'): expected {} from previous balance {} + amount {}, statement shows {} (drift {})",
            current.id,
            current.row.date,
            current.row.text.trim(),
            numeric(&expected)?,
            numeric(previous)?,
            numeric(&current.row.amount)?,
            numeric(statement)?,
            numeric(&drift)?
        );
        let action = "Check the statement for missing, duplicated or misordered rows around this transaction; re-import the complete export if needed.";
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM exceptions WHERE type='BANK_BALANCE_GAP' AND status='open' AND related_bank_transaction_id=?1 AND message=?2 AND required_action=?3 AND resolution_key IS NULL)",params![current.id,warning,action],|r|r.get(0)).map_err(sql)?;
        if !exists {
            let evidence = json!({"ruleId":"DK-BANK-BALANCE-CONTINUITY-001","bankTransactionId":current.id,"transactionDate":current.row.date,"previousBalance":crate::journal_post::money(previous)?,"amount":crate::journal_post::money(&current.row.amount)?,"expectedBalance":crate::journal_post::money(&expected)?,"statementBalance":crate::journal_post::money(statement)?,"drift":crate::journal_post::money(&drift)?});
            tx.execute("INSERT INTO exceptions(type,severity,related_bank_transaction_id,message,required_action,source_evidence) VALUES('BANK_BALANCE_GAP','medium',?1,?2,?3,?4)",params![current.id,warning,action,serde_json::to_string(&evidence).map_err(|_|fail("bank balance evidence unavailable"))?]).map_err(sql)?;
            created += 1;
        }
        warnings.push(warning);
    }
    Ok(Continuity {
        warnings,
        created,
        first: ordered.first().and_then(|r| r.row.balance_after.clone()),
        last: ordered.last().and_then(|r| r.row.balance_after.clone()),
    })
}
pub fn read(
    tx: &Transaction<'_>,
    slug: &str,
    today: &str,
    selected: Option<&str>,
) -> Result<Value, Error> {
    use rentemester_engine::fiscal_year::{self, Date, LabelStrategy, StartMonth};
    let (month, strategy): (i32, String) = tx
        .query_row(
            "SELECT fiscal_year_start_month,fiscal_year_label_strategy FROM companies WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(sql)?;
    let month = StartMonth::new(month)?;
    let strategy = LabelStrategy::try_from(strategy.as_str())?;
    let current = fiscal_year::for_date(Date::parse(today)?, month, strategy)?;
    let current_label = current.display_label.clone();
    let mut years = std::collections::BTreeMap::new();
    years.insert(current_label.clone(), current);
    let mut dates=tx.prepare("SELECT DISTINCT transaction_date FROM bank_transactions UNION SELECT DISTINCT transaction_date FROM journal_entries").map_err(sql)?;
    for date in dates
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(sql)?
    {
        let year = fiscal_year::for_date(Date::parse(&date.map_err(sql)?)?, month, strategy)?;
        years.insert(year.display_label.clone(), year);
    }
    let label = selected.unwrap_or(&current_label);
    let year = years
        .get(label)
        .ok_or_else(|| fail("unknown fiscal year"))?;
    let start = year.start.to_string();
    let end = year.end.to_string();
    let all_accounts = accounts(tx, slug)?;
    let mut q=tx.prepare("SELECT bt.id,bt.transaction_date,bt.text,COALESCE(bt.amount_dkk,bt.amount),br.journal_entry_no,bt.balance_after,bt.currency,ba.currency,EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=br.journal_entry_id),br.reconciliation_id,COALESCE(c.reason,link.note),COALESCE(c.actor,link.created_by),COALESCE(c.plan_hash,link.source_reference),bt.booking_date,bt.reference,bt.counterparty_name,bt.counterparty_account,bt.message,bt.archive_reference,bt.customer_reference,bt.balance_after,bt.raw_json,bt.source_file_hash,bt.import_batch_id FROM bank_transactions bt LEFT JOIN bank_accounts ba ON ba.id=bt.bank_account_id LEFT JOIN bank_journal_reconciliations br ON br.bank_transaction_id=bt.id LEFT JOIN bank_journal_reconciliation_links link ON link.bank_transaction_id=bt.id AND br.link_kind='append-only' LEFT JOIN bank_reconciliation_correction_events c ON br.reconciliation_id='correction:'||c.id WHERE bt.transaction_date BETWEEN ?1 AND ?2 ORDER BY bt.transaction_date,bt.id").map_err(sql)?;
    let mut transactions=q.query_map(params![start,end],|r|{let journal:Option<String>=r.get(4)?;let reversed:bool=r.get(8)?;Ok(json!({"id":r.get::<_,i64>(0)?,"date":r.get::<_,String>(1)?,"text":r.get::<_,String>(2)?,"amount":r.get::<_,f64>(3)?,"runningBalance":if r.get::<_,Option<String>>(7)?.as_deref().unwrap_or(&r.get::<_,String>(6)?)=="DKK" {r.get::<_,Option<f64>>(5)?}else{None},"reconciliationStatus":if journal.is_some() && !reversed{"matched"}else{"unmatched"},"reconciliationProblem":if reversed{Some("Tilknyttet postering er tilbageført; særskilt linkkorrektion kræves.")}else{None},"journalEntryNo":journal,"reconciliationId":r.get::<_,Option<String>>(9)?,"reconciliationNote":r.get::<_,Option<String>>(10)?,"reconciliationActor":r.get::<_,Option<String>>(11)?,"reconciliationProof":r.get::<_,Option<String>>(12)?,"bookingDate":r.get::<_,Option<String>>(13)?,"reference":r.get::<_,Option<String>>(14)?,"counterpartyName":r.get::<_,Option<String>>(15)?,"counterpartyAccount":r.get::<_,Option<String>>(16)?,"message":r.get::<_,Option<String>>(17)?,"archiveReference":r.get::<_,Option<String>>(18)?,"customerReference":r.get::<_,Option<String>>(19)?,"balanceAfter":r.get::<_,Option<f64>>(20)?,"raw":r.get::<_,Option<String>>(21)?.map(|raw|serde_json::from_str::<Value>(&raw).map_err(|_|rusqlite::Error::InvalidQuery)).transpose()?,"sourceFileHash":r.get::<_,Option<String>>(22)?,"importBatchId":r.get::<_,Option<String>>(23)?}))}).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?;
    let source_order_installed=crate::bank_statement_order::installed(tx)?;
    for row in &mut transactions {
        row["reconciliationHistory"] =
            crate::bank_reconciliation::correction::history(tx, row["id"].as_i64().unwrap())?;
        let order=if source_order_installed {tx.query_row("SELECT source_row_index,direction,provenance FROM native_bank_statement_order_v1 WHERE transaction_id=?1",[row["id"].as_i64().unwrap()],|r| Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).optional().map_err(sql)?} else {None};
        row["statementRowIndex"]=json!(order.as_ref().map(|v|v.0));row["statementOrder"]=json!(order.as_ref().map(|v|&v.1));row["statementOrderProvenance"]=json!(order.as_ref().map(|v|&v.2));
    }
    let matched = transactions
        .iter()
        .filter(|v| v["reconciliationStatus"] == "matched")
        .count();
    let mut booked = Money::default();
    let mut balances=tx.prepare("SELECT jl.debit_amount,jl.credit_amount FROM journal_lines jl JOIN journal_entries je ON je.id=jl.journal_entry_id JOIN accounts a ON a.id=jl.account_id WHERE je.status IN ('posted','reversed') AND je.transaction_date<=?1 AND a.account_no IN (SELECT ledger_account_no FROM bank_accounts WHERE ledger_account_no IS NOT NULL UNION SELECT account_no FROM account_role_mappings WHERE role='bank' AND status='confirmed')").map_err(sql)?;
    let mut rows = balances.query([&end]).map_err(sql)?;
    while let Some(r) = rows.next().map_err(sql)? {
        booked.add(&crate::legacy_money(r.get_ref(0).map_err(sql)?)?);
        booked.add(&Money::default().subtract(&crate::legacy_money(r.get_ref(1).map_err(sql)?)?));
    }
    let statement = statement_balance(tx, &end)?;
    let difference = statement
        .balance
        .as_ref()
        .map(|balance| crate::cockpit::number(&booked.subtract(balance)))
        .transpose()?;
    let fiscal_years:Vec<_>=years.values().map(|y|json!({"label":y.display_label,"start":y.start.to_string(),"end":y.end.to_string(),"source":"live"})).collect();
    Ok(
        json!({"slug":slug,"selectedYear":label,"archived":false,"manualReconciliationAvailable":true,"reconciliationCorrectionAvailable":true,"company":all_accounts["company"],"fiscalYears":fiscal_years,"periodStart":start,"periodEnd":end,"accounts":all_accounts["accounts"],"bookedBalance":crate::cockpit::number(&booked)?,"actualBalance":statement.balance.as_ref().map(crate::cockpit::number).transpose()?,"difference":difference,"bankStatementStatus":statement.status,"bankStatementProblem":statement.problem,"matchedCount":matched,"unmatchedCount":transactions.len()-matched,"transactions":transactions}),
    )
}

/// Independent statement evidence; never synthesize a closing balance from the ledger.
pub(crate) struct StatementBalance {
    pub balance: Option<Money>,
    pub status: &'static str,
    pub problem: Option<String>,
}
pub(crate) fn statement_balance(
    tx: &Transaction<'_>,
    cutoff: &str,
) -> Result<StatementBalance, Error> {
    let missing = |problem: String| StatementBalance {
        balance: None,
        status: "unavailable",
        problem: Some(problem),
    };
    let any: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bank_transactions WHERE transaction_date<=?1)",
            [cutoff],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if !any {
        return Ok(StatementBalance {
            balance: None,
            status: "none",
            problem: None,
        });
    }
    let balances:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM bank_transactions WHERE transaction_date<=?1 AND balance_after IS NOT NULL)",[cutoff],|r|r.get(0)).map_err(sql)?;
    if !balances {
        return Ok(StatementBalance {
            balance: None,
            status: "no-balance-column",
            problem: None,
        });
    }
    let mut q=tx.prepare("SELECT id,name,currency FROM bank_accounts WHERE ledger_account_no IS NOT NULL OR EXISTS(SELECT 1 FROM bank_transactions t WHERE t.bank_account_id=bank_accounts.id AND t.transaction_date<=?1) ORDER BY id").map_err(sql)?;
    let mut streams = q
        .query_map([cutoff], |r| {
            Ok((
                Some(r.get::<_, i64>(0)?),
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let unassigned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM bank_transactions WHERE bank_account_id IS NULL AND transaction_date<=?1)",[cutoff],|r|r.get(0)).map_err(sql)?;
    if unassigned {
        if !streams.is_empty() {
            return Ok(missing("Kontoudtoget indeholder både tilknyttede og ikke-tilknyttede bankkonti. Saldoernes sammenhæng skal afklares.".into()));
        }
        streams.push((None, "Ikke tilknyttet bankkonto".into(), "DKK".into()));
    }
    if streams.is_empty() {
        return Ok(missing(
            "Kontoudtogets bankkonto kan ikke bestemmes.".into(),
        ));
    }
    let mut total = Money::default();
    for (account, name, currency) in streams {
        if currency != "DKK" {
            return Ok(missing(format!(
                "Kontoudtogets saldo for {name} er i {currency}; en uafhængig DKK-slutsaldo mangler."
            )));
        }
        let latest:Option<String>=tx.query_row("SELECT MAX(transaction_date) FROM bank_transactions WHERE bank_account_id IS ?1 AND transaction_date<=?2",params![account,cutoff],|r|r.get(0)).map_err(sql)?;
        let Some(_latest) = latest else {
            return Ok(missing(format!("Der mangler et kontoudtog for {name}.")));
        };
        // Native source chronology is optional on historical rows; a same-day
        // cluster without one exact declaration is unavailable, never inferred.
        let order_join = if crate::bank_statement_order::installed(tx)? {
            "LEFT JOIN native_bank_statement_order_v1 o ON o.transaction_id=b.id"
        } else { "LEFT JOIN (SELECT NULL AS transaction_id,NULL AS source_row_index,NULL AS direction,NULL AS provenance) o ON o.transaction_id=b.id" };
        let mut q=tx.prepare(&format!("SELECT b.id,b.transaction_date,b.amount,b.balance_after,b.currency,o.source_row_index,o.direction,o.provenance FROM bank_transactions b {order_join} WHERE b.bank_account_id IS ?1 AND b.transaction_date<=?2 ORDER BY b.transaction_date,b.id")).map_err(sql)?;
        let mut rs=q.query(params![account,cutoff]).map_err(sql)?;
        let mut dates=std::collections::BTreeMap::<String,Vec<(Money,Money,Option<i64>,Option<String>,Option<String>)>>::new();
        while let Some(r)=rs.next().map_err(sql)? {
            if r.get::<_,String>(4).map_err(sql)?!="DKK" { return Ok(missing(format!("Bankkontoens saldovaluta for {name} skal afklares før en DKK-sammenligning."))); }
            if r.get_ref(3).map_err(sql)?==rusqlite::types::ValueRef::Null {return Ok(missing(format!("Der mangler saldi på bankposter for {name}.")));}
            dates.entry(r.get(1).map_err(sql)?).or_default().push((signed_money(r.get_ref(2).map_err(sql)?)?,signed_money(r.get_ref(3).map_err(sql)?)?,r.get(5).map_err(sql)?,r.get(6).map_err(sql)?,r.get(7).map_err(sql)?));
        }
        let mut previous: Option<Money>=None;
        for (date,mut cluster) in dates {
            if cluster.len()>1 {
                let direction=cluster[0].3.clone();let provenance=cluster[0].4.clone();
                let indices:std::collections::BTreeSet<_>=cluster.iter().map(|r|r.2).collect();
                if direction.is_none() || provenance.is_none() || indices.contains(&None) || indices.len()!=cluster.len() || cluster.iter().any(|r|r.3!=direction || r.4!=provenance) {
                    return Ok(missing(format!("Rækkefølgen i kontoudtoget for {name} den {date} er ikke dokumenteret entydigt.")));
                }
                cluster.sort_by_key(|r|r.2);if direction.as_deref()==Some("descending") {cluster.reverse();}
            }
            for (amount,balance,_,_,_) in cluster {
                if let Some(ref prior)=previous {let mut expected=prior.clone();expected.add(&amount);if expected!=balance {return Ok(missing(format!("Saldoerne i kontoudtoget for {name} den {date} hænger ikke sammen i ét forløb.")));}}
                previous=Some(balance);
            }
        }
        total.add(&previous.ok_or_else(||fail("statement closing balance unavailable"))?);
    }
    Ok(StatementBalance {
        balance: Some(total),
        status: "known",
        problem: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_statement_balance_ignores_import_order_and_refuses_missing_or_ambiguous_evidence()
     {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Bank").unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic bank','asset','debit');").unwrap();
        let account = |name: &str| {
            serde_json::from_value::<AccountInput>(json!({"name":name,"ledgerAccountNo":"A"}))
                .unwrap()
        };
        create_account(&tx, &account("First"), "synthetic").unwrap();
        // The ending row comes first: insertion order cannot pick the balance.
        let csv = "date;text;amount;saldo\n2026-10-18;Synthetic outgoing;-250;750\n2026-10-18;Synthetic incoming;1000;1000\n";
        let rows = rentemester_engine::bank::parse(csv).unwrap();
        import_ordered_bytes_with_principal(&tx,csv.as_bytes(),&rows,Some("first"),"synthetic","2026-10-18T12:00:00Z","synthetic-original",crate::idempotency::PrincipalKind::User,false,Some("descending"))
        .unwrap();
        let value = read(&tx, "synthetic", "2026-10-18", None).unwrap();
        assert_eq!(value["actualBalance"].as_f64(), Some(750.0));
        assert_eq!(value["difference"].as_f64(), Some(-750.0));
        assert_eq!(
            value["transactions"][0]["runningBalance"].as_f64(),
            Some(750.0)
        );
        assert_eq!(
            import(
                &tx,
                csv,
                &rows,
                Some("first"),
                "synthetic",
                "2026-10-18T12:00:00Z",
                "synthetic-original"
            )
            .unwrap()["imported"],
            0
        );
        let changed = csv.replace(";750\n", ";751\n");
        assert!(
            import(
                &tx,
                &changed,
                &rentemester_engine::bank::parse(&changed).unwrap(),
                Some("first"),
                "synthetic",
                "2026-10-18T12:00:00Z",
                "synthetic-original"
            )
            .is_err()
        );
        create_account(&tx, &account("Second"), "synthetic").unwrap();
        assert_eq!(
            read(&tx, "synthetic", "2026-10-18", None).unwrap()["bankStatementStatus"],
            "unavailable"
        );
        let second =
            "date;text;amount;balance_after\n2026-10-18;Synthetic second account;100;100\n";
        import(
            &tx,
            second,
            &rentemester_engine::bank::parse(second).unwrap(),
            Some("second"),
            "synthetic",
            "2026-10-18T12:00:00Z",
            "synthetic-original",
        )
        .unwrap();
        assert_eq!(
            read(&tx, "synthetic", "2026-10-18", None).unwrap()["actualBalance"].as_f64(),
            Some(850.0)
        );
        let ambiguous = "date;text;amount;balance_after\n2026-10-19;Synthetic ambiguous first;100;100\n2026-10-19;Synthetic ambiguous second;50;250\n";
        import(
            &tx,
            ambiguous,
            &rentemester_engine::bank::parse(ambiguous).unwrap(),
            Some("first"),
            "synthetic",
            "2026-10-19T12:00:00Z",
            "synthetic-original",
        )
        .unwrap();
        let value = read(&tx, "synthetic", "2026-10-19", None).unwrap();
        assert_eq!(value["actualBalance"], Value::Null);
        assert_eq!(value["difference"], Value::Null);
        assert_eq!(value["bankStatementStatus"], "unavailable");
        assert!(
            value["bankStatementProblem"]
                .as_str()
                .unwrap()
                .contains("entydig")
        );
        let disconnected = "date;text;amount;balance_after\n2026-10-20;Synthetic cycle one;100;100\n2026-10-20;Synthetic cycle two;-100;0\n2026-10-20;Synthetic disconnected;50;250\n";
        import(
            &tx,
            disconnected,
            &rentemester_engine::bank::parse(disconnected).unwrap(),
            Some("first"),
            "synthetic",
            "2026-10-20T12:00:00Z",
            "synthetic-original",
        )
        .unwrap();
        let value = read(&tx, "synthetic", "2026-10-20", None).unwrap();
        assert_eq!(value["actualBalance"], Value::Null);
        assert!(
            value["bankStatementProblem"]
                .as_str()
                .unwrap()
                .contains("entydig")
        );
    }
    #[test]
    fn repeated_identical_payments_keep_occurrences_and_account_identity() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut company = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic Bank").unwrap();
        let tx = company
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic cash','asset','debit');").unwrap();
        let account = |name: &str| {
            serde_json::from_value::<AccountInput>(json!({"name":name,"ledgerAccountNo":"A"}))
                .unwrap()
        };
        create_account(&tx, &account("First"), "synthetic").unwrap();
        create_account(&tx, &account("Second"), "synthetic").unwrap();
        let csv = "date;text;amount\n2026-10-18;Synthetic payment;-250\n2026-10-18;Synthetic payment;-250\n";
        let rows = rentemester_engine::bank::parse(csv).unwrap();
        assert_eq!(
            fingerprint(&rows[0], Some(1), 0).unwrap(),
            "5b9e36c21c0a56c3bba25efcf79fb01d2493cd311c43196b55298538a21ee1b0"
        );
        let first = import(
            &tx,
            csv,
            &rows,
            Some("first"),
            "synthetic",
            "2026-10-18T12:00:00Z",
            "synthetic-original",
        )
        .unwrap();
        assert_eq!(first["imported"], 2);
        let repeated = import(
            &tx,
            csv,
            &rows,
            Some("first"),
            "synthetic",
            "2026-10-18T12:01:00Z",
            "synthetic-original",
        )
        .unwrap();
        assert_eq!(repeated["imported"], 0);
        assert_eq!(repeated["skippedDuplicates"], 2);
        assert_eq!(
            import(
                &tx,
                csv,
                &rows,
                Some("second"),
                "synthetic",
                "2026-10-18T12:02:00Z",
                "synthetic-original"
            )
            .unwrap()["imported"],
            2
        );
        assert!(
            import(
                &tx,
                csv,
                &rows,
                Some("unknown"),
                "synthetic",
                "2026-10-18T12:03:00Z",
                "synthetic-original"
            )
            .is_err()
        );
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM bank_transactions", [], |r| r.get(0))
                .unwrap(),
            4
        );
    }
}

#[cfg(test)]
mod profile_tests;

#[cfg(test)]
mod byte_tests;

#[cfg(test)]
#[path = "bank/original_tests.rs"]
mod original_tests;
