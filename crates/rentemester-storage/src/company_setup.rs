//! Company profile and explicit chart setup, on the admitted transaction only.
use rentemester_engine::{Error, journal_rules::VAT_CODES};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
pub mod source_chart;
const ROLES: [(&str, &str, &str); 8] = [
    ("bank", "asset", "debit"),
    ("debtors", "asset", "debit"),
    ("creditors", "liability", "credit"),
    ("output_vat", "vat", "credit"),
    ("input_vat", "vat", "debit"),
    ("reverse_charge_vat", "vat", "credit"),
    ("vat_settlement", "liability", "credit"),
    ("operational_default", "expense", "debit"),
];
const OPTIONAL_ROLES: [(&str, &str, &str); 2] = [
    ("fx_gain", "income", "credit"),
    ("fx_loss", "expense", "debit"),
];
fn fail(s: impl Into<String>) -> Error {
    Error::new("company_setup_refused", s)
}
fn sql(_: rusqlite::Error) -> Error {
    fail("company setup storage unavailable; abort transaction")
}
fn audit(
    tx: &Transaction<'_>,
    event: &str,
    entity: &str,
    message: &str,
    actor: &str,
) -> Result<(), Error> {
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'company_setup',?2,?3,?4)",params![event,entity,message,format!("user:{actor}")]).map_err(sql)?;
    Ok(())
}
fn expected(role: &str) -> Result<(&'static str, &'static str), Error> {
    ROLES
        .iter()
        .chain(OPTIONAL_ROLES.iter())
        .find(|r| r.0 == role)
        .map(|r| (r.1, r.2))
        .ok_or_else(|| fail("unsupported account role"))
}
fn compatible(tx: &Transaction<'_>, role: &str, account: &str) -> Result<(), Error> {
    let (kind, balance) = expected(role)?;
    let yes:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND type=?2 AND normal_balance=?3 AND active=1)",params![account,kind,balance],|r|r.get(0)).map_err(sql)?;
    if !yes {
        return Err(fail(
            "role requires an active account with compatible type and normal balance",
        ));
    }
    Ok(())
}
fn resolution(tx: &Transaction<'_>, role: &str) -> Result<Value, Error> {
    let row: Option<(String, i64, String, String)> = if OPTIONAL_ROLES.iter().any(|r| r.0 == role) {
        crate::native_fx_roles::resolve(tx, role)?
    } else {
        tx.query_row("SELECT account_no,version,confirmed_by,confirmation_source FROM account_role_mappings WHERE role=?1 AND status='confirmed'",[role],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(sql)?
    };
    match row {
        Some((account, version, actor, source)) if compatible(tx, role, &account).is_ok() => Ok(
            json!({"ok":true,"role":role,"accountNo":account,"version":version,"confirmedBy":actor,"confirmationSource":source}),
        ),
        _ => Ok(
            json!({"ok":false,"role":role,"error":"Aktiv, kompatibel konto kræver eksplicit bekræftelse"}),
        ),
    }
}
pub fn accounts(tx: &Transaction<'_>, slug: &str) -> Result<Value, Error> {
    crate::validate(tx)?;
    let mut q=tx.prepare("SELECT account_no,name,type,normal_balance,default_vat_code,active,EXISTS(SELECT 1 FROM journal_lines jl WHERE jl.account_id=a.id) FROM accounts a ORDER BY account_no").map_err(sql)?;
    let rows=q.query_map([],|r|Ok(json!({"accountNo":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"type":r.get::<_,String>(2)?,"normalBalance":r.get::<_,String>(3)?,"defaultVatCode":r.get::<_,Option<String>>(4)?,"active":r.get::<_,i64>(5)?==1,"hasPostings":r.get::<_,i64>(6)?==1}))).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?;
    let mut counts = serde_json::Map::new();
    for row in &rows {
        let key = row["type"].as_str().unwrap();
        let count = counts.get(key).and_then(Value::as_u64).unwrap_or(0);
        counts.insert(key.into(), json!(count + 1));
    }
    let mut resolutions = Vec::new();
    let mut missing = Vec::new();
    let mut reasons = Vec::new();
    for (role, _, _) in ROLES.into_iter().chain(OPTIONAL_ROLES) {
        let value = resolution(tx, role)?;
        if value["ok"] != true && ROLES.iter().any(|r| r.0 == role) {
            missing.push(role);
            reasons.push(json!({"role":role,"reason":value["error"]}));
        }
        resolutions.push(value);
    }
    let mut q=tx.prepare("SELECT role,account_no,source,proposed_at FROM account_role_proposals WHERE status='proposed' ORDER BY id").map_err(sql)?;
    let proposals=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?))).map_err(sql)?.collect::<Result<Vec<_>,_>>().map_err(sql)?.into_iter().map(|(role,account,source,at)|{let valid=compatible(tx,&role,&account).is_ok();json!({"role":role,"accountNo":account,"source":source,"proposedAt":at,"compatible":valid})}).collect::<Vec<_>>();
    let ambiguous: Vec<_> = missing
        .iter()
        .filter(|role| {
            proposals
                .iter()
                .filter(|p| p["role"] == **role && p["compatible"] == true)
                .map(|p| p["accountNo"].as_str().unwrap())
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                > 1
        })
        .copied()
        .collect();
    let company = profile(tx)?;
    Ok(
        json!({"slug":slug,"company":company,"accounts":rows,"byType":counts,"nativeSetup":true,"accountRoles":{"status":if !ambiguous.is_empty(){"ambiguous"}else if missing.is_empty(){"complete"}else{"incomplete"},"missing":missing,"ambiguous":ambiguous,"resolutions":resolutions,"reasons":reasons,"proposals":proposals,"candidates":proposals}}),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Account {
    pub account_no: String,
    pub name: String,
    pub r#type: String,
    pub normal_balance: String,
    pub default_vat_code: Option<String>,
}
pub fn create_account(tx: &Transaction<'_>, input: &Account, actor: &str) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let no = input.account_no.trim();
    let name = input.name.trim();
    if no.is_empty()
        || no.len() > 64
        || name.is_empty()
        || name.len() > 256
        || no.chars().chain(name.chars()).any(char::is_control)
        || !["asset", "liability", "equity", "income", "expense", "vat"]
            .contains(&input.r#type.as_str())
        || !["debit", "credit"].contains(&input.normal_balance.as_str())
        || input
            .default_vat_code
            .as_deref()
            .is_some_and(|c| !VAT_CODES.contains(&c))
    {
        return Err(fail(
            "valid account number, name, type, normal balance and VAT code required",
        ));
    }
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1)",
            [no],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if exists {
        return Err(fail(
            "account number already exists; historical accounts are not overwritten",
        ));
    }
    tx.execute("INSERT INTO accounts(account_no,name,type,normal_balance,default_vat_code) VALUES(?1,?2,?3,?4,?5)",params![no,name,input.r#type,input.normal_balance,input.default_vat_code]).map_err(sql)?;
    audit(
        tx,
        "account_created",
        no,
        &format!("Created account {no}: {name}"),
        actor,
    )?;
    Ok(json!({"accountNo":no}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Role {
    pub role: String,
    pub account_no: String,
    pub confirm: bool,
}
pub fn confirm_role(tx: &Transaction<'_>, input: &Role, actor: &str) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm {
        return Err(fail("explicit role confirmation required"));
    }
    compatible(tx, &input.role, &input.account_no)?;
    if OPTIONAL_ROLES.iter().any(|r| r.0 == input.role) {
        crate::native_fx_roles::confirm(tx, &input.role, &input.account_no, actor)?;
        audit(
            tx,
            "account_role_confirmed",
            &input.role,
            &format!("Confirmed {} -> {}", input.role, input.account_no),
            actor,
        )?;
        return resolution(tx, &input.role);
    }
    let current = resolution(tx, &input.role)?;
    if current["ok"] == true && current["accountNo"] == input.account_no {
        return Ok(current);
    }
    let previous: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(version),0) FROM account_role_mappings WHERE role=?1",
            [&input.role],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let version = previous
        .checked_add(1)
        .filter(|v| *v <= 9_007_199_254_740_991)
        .ok_or_else(|| fail("role version exhausted"))?;
    tx.execute(
        "UPDATE account_role_mappings SET status='superseded' WHERE role=?1 AND status='confirmed'",
        [&input.role],
    )
    .map_err(sql)?;
    tx.execute("INSERT INTO account_role_mappings(role,account_no,version,confirmed_by,confirmation_source) VALUES(?1,?2,?3,?4,'explicit')",params![input.role,input.account_no,version,format!("user:{actor}")]).map_err(sql)?;
    tx.execute("UPDATE account_role_proposals SET status=CASE WHEN account_no=?2 THEN 'accepted' ELSE 'rejected' END WHERE role=?1 AND status='proposed'",params![input.role,input.account_no]).map_err(sql)?;
    audit(
        tx,
        "account_role_confirmed",
        &input.role,
        &format!(
            "Confirmed {} -> {} version {version}",
            input.role, input.account_no
        ),
        actor,
    )?;
    resolution(tx, &input.role)
}
pub fn profile(tx: &Transaction<'_>) -> Result<Value, Error> {
    let mut company=tx.query_row("SELECT id,name,country,currency,cvr,fiscal_year_start_month,fiscal_year_label_strategy,address,postal_code,city,company_form,industry_code,industry_text,cvr_status,audit_waived,cvr_synced_at,vat_period_type,payment_terms_days,created_at FROM companies WHERE id=1",[],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"name":r.get::<_,String>(1)?,"country":r.get::<_,String>(2)?,"currency":r.get::<_,String>(3)?,"cvr":r.get::<_,Option<String>>(4)?,"fiscalYearStartMonth":r.get::<_,i32>(5)?,"fiscalYearLabelStrategy":r.get::<_,String>(6)?,"address":r.get::<_,Option<String>>(7)?,"postalCode":r.get::<_,Option<String>>(8)?,"city":r.get::<_,Option<String>>(9)?,"companyForm":r.get::<_,Option<String>>(10)?,"industryCode":r.get::<_,Option<String>>(11)?,"industryText":r.get::<_,Option<String>>(12)?,"cvrStatus":r.get::<_,Option<String>>(13)?,"auditWaived":r.get::<_,Option<bool>>(14)?,"cvrSyncedAt":r.get::<_,Option<String>>(15)?,"vatPeriodType":r.get::<_,Option<String>>(16)?,"paymentTermsDays":r.get::<_,i32>(17)?,"createdAt":r.get::<_,String>(18)?}))).map_err(sql)?;
    company["payment"]=tx.query_row("SELECT bank_name,registration_no,account_no,iban FROM bank_accounts WHERE active=1 AND currency=?1 ORDER BY (slug='primaer') DESC,id LIMIT 1",[company["currency"].as_str().unwrap()],|r|Ok(json!({"bankName":r.get::<_,Option<String>>(0)?,"registrationNo":r.get::<_,Option<String>>(1)?,"accountNo":r.get::<_,Option<String>>(2)?,"iban":r.get::<_,Option<String>>(3)?}))).optional().map_err(sql)?.unwrap_or(Value::Null);
    Ok(company)
}
fn cadence(tx: &Transaction<'_>, next: Option<&str>, actor: &str) -> Result<(), Error> {
    if next.is_some_and(|s| !["month", "quarter", "half-year"].contains(&s)) {
        return Err(fail("invalid VAT cadence"));
    }
    let before: Option<String> = tx
        .query_row(
            "SELECT vat_period_type FROM companies WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if before.as_deref() == next {
        return Ok(());
    }
    let mut q=tx.prepare("SELECT id,period_start,period_end,status FROM accounting_periods WHERE kind IN ('vat_period','vat_quarter')").map_err(sql)?;
    let periods = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    let vat_activity:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id JOIN journal_entries je ON je.id=jl.journal_entry_id WHERE je.status='posted' AND (a.type='vat' OR COALESCE(trim(jl.vat_code),'')<>'' OR a.account_no IN (SELECT account_no FROM account_role_mappings WHERE status='confirmed' AND role IN ('input_vat','output_vat','reverse_charge_vat'))))",[],|r|r.get(0)).map_err(sql)?;
    for (_, start, end, _) in &periods {
        if !rentemester_engine::calendar::valid_iso_date(start)
            || !rentemester_engine::calendar::valid_iso_date(end)
        {
            return Err(fail("invalid historical VAT period"));
        }
    }
    if let Some(next) = next {
        if before.is_some() && (!periods.is_empty() || vat_activity)
            || before.is_none() && periods.is_empty() && vat_activity
        {
            return Err(fail(
                "VAT cadence change requires dated migration after VAT activity or VAT periods",
            ));
        }
        let span = match next {
            "month" => 1,
            "quarter" => 3,
            _ => 6,
        };
        for (_, start, end, _) in &periods {
            let month = start[5..7]
                .parse::<u32>()
                .map_err(|_| fail("invalid historical VAT period"))?;
            let year = &start[..4];
            let last = (month - 1) / span * span + span;
            let mut day = 31;
            while !rentemester_engine::calendar::valid_iso_date(&format!(
                "{year}-{last:02}-{day:02}"
            )) {
                day -= 1;
                if day < 28 {
                    return Err(fail("invalid historical VAT period"));
                }
            }
            if &start[8..] != "01"
                || !(month - 1).is_multiple_of(span)
                || end != &format!("{year}-{last:02}-{day:02}")
            {
                return Err(fail("existing VAT periods have another cadence"));
            }
        }
    } else if vat_activity {
        let mut covered = Vec::new();
        for (id, start, end, mut state) in periods {
            let mut events=tx.prepare("SELECT event_type FROM audit_log WHERE entity_type='accounting_period' AND entity_id=?1 ORDER BY id").map_err(sql)?;
            for event in events
                .query_map([id.to_string()], |r| r.get::<_, String>(0))
                .map_err(sql)?
            {
                let event = event.map_err(sql)?;
                if state != "reported" {
                    match event.as_str() {
                        "period_reopen" => state = "open".into(),
                        "period_close" => state = "closed".into(),
                        "period_report" => state = "reported".into(),
                        _ => {}
                    }
                }
            }
            if state == "closed" || state == "reported" {
                covered.push((start, end));
            }
        }
        let mut dates=tx.prepare("SELECT DISTINCT je.transaction_date FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id JOIN journal_entries je ON je.id=jl.journal_entry_id WHERE je.status='posted' AND (a.type='vat' OR COALESCE(trim(jl.vat_code),'')<>'' OR a.account_no IN (SELECT account_no FROM account_role_mappings WHERE status='confirmed' AND role IN ('input_vat','output_vat','reverse_charge_vat')))").map_err(sql)?;
        for date in dates
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(sql)?
        {
            let date = date.map_err(sql)?;
            if !covered
                .iter()
                .any(|(start, end)| date >= *start && date <= *end)
            {
                return Err(fail(
                    "open VAT activity prevents deregistration; settle its VAT period first",
                ));
            }
        }
    }
    tx.execute("UPDATE companies SET vat_period_type=?1 WHERE id=1", [next])
        .map_err(sql)?;
    audit(
        tx,
        if next.is_none() {
            "company_vat_deregistered"
        } else if before.is_none() {
            "company_vat_registered"
        } else {
            "company_vat_period_changed"
        },
        "1",
        &format!("VAT cadence {before:?} -> {next:?}"),
        actor,
    )
}
pub fn update_profile(
    tx: &Transaction<'_>,
    input: &serde_json::Map<String, Value>,
    actor: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let before = profile(tx)?;
    for (key, value) in input {
        match key.as_str() {
            "cvr" => {
                let value = value.as_str().ok_or_else(|| fail("CVR must be a string"))?;
                let text = value.trim().to_ascii_uppercase();
                let digits = text.strip_prefix("DK").unwrap_or(&text);
                if !text.is_empty()
                    && (digits.len() != 8 || !digits.bytes().all(|c| c.is_ascii_digit()))
                {
                    return Err(fail(
                        "CVR must be eight digits, optionally prefixed with DK",
                    ));
                }
                let normalized = (!text.is_empty()).then(|| format!("DK{digits}"));
                tx.execute("UPDATE companies SET cvr=?1 WHERE id=1", [normalized])
                    .map_err(sql)?;
            }
            "vatPeriodType" => {
                let next = if value.is_null() {
                    None
                } else {
                    Some(value.as_str().ok_or_else(|| fail("invalid VAT cadence"))?)
                };
                cadence(tx, next, actor)?;
            }
            "address" | "postalCode" | "city" => {
                let text = value
                    .as_str()
                    .ok_or_else(|| fail("profile field must be a string"))?
                    .trim();
                if text.len() > 1024 || text.chars().any(char::is_control) {
                    return Err(fail("invalid profile text"));
                }
                let column = match key.as_str() {
                    "postalCode" => "postal_code",
                    "address" => "address",
                    _ => "city",
                };
                tx.execute(
                    &format!("UPDATE companies SET {column}=?1 WHERE id=1"),
                    [(!text.is_empty()).then_some(text)],
                )
                .map_err(sql)?;
            }
            "paymentTermsDays" => {
                let days = value
                    .as_i64()
                    .filter(|n| (0..=365).contains(n))
                    .ok_or_else(|| fail("payment terms must be 0–365 days"))?;
                tx.execute(
                    "UPDATE companies SET payment_terms_days=?1 WHERE id=1",
                    [days],
                )
                .map_err(sql)?;
            }
            "payment" => {
                let values = value
                    .as_object()
                    .ok_or_else(|| fail("payment must be an object"))?;
                for (field, value) in values {
                    if !["bankName", "registrationNo", "accountNo", "iban"]
                        .contains(&field.as_str())
                        || !value.is_string()
                    {
                        return Err(fail("unsupported payment field"));
                    }
                }
                let nonempty = values
                    .values()
                    .any(|s| !s.as_str().unwrap().trim().is_empty());
                if nonempty {
                    if !before["payment"].is_null() {
                        for (key, value) in values {
                            let stored = before["payment"][key].as_str().unwrap_or("");
                            if stored != value.as_str().unwrap().trim() {
                                return Err(fail(
                                    "editing an existing payment profile requires the bank account update flow",
                                ));
                            }
                        }
                    } else {
                        let bank:crate::bank::AccountInput=serde_json::from_value(json!({"name":"Driftskonto","slug":"primaer","currency":before["currency"],"bankName":values.get("bankName"),"registrationNo":values.get("registrationNo"),"accountNo":values.get("accountNo"),"iban":values.get("iban")})).map_err(|_|fail("invalid payment profile"))?;
                        crate::bank::create_account(tx, &bank, actor)?;
                    }
                }
            }
            _ => return Err(fail("unsupported company profile field")),
        }
    }
    audit(
        tx,
        "company_profile_update",
        "1",
        "Updated company profile",
        actor,
    )?;
    profile(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn company() -> rusqlite::Connection {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut company = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic Setup").unwrap();
        company
    }
    #[test]
    fn fx_roles_are_optional_and_require_compatible_confirmed_accounts() {
        let mut db = company();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('G77','Synthetic gain','income','credit'),('L77','Synthetic loss','expense','debit');").unwrap();
        let before = accounts(&tx, "synthetic").unwrap();
        assert!(
            !before["accountRoles"]["missing"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r == "fx_gain" || r == "fx_loss")
        );
        for (role, account) in [("fx_gain", "G77"), ("fx_loss", "L77")] {
            let input = Role {
                role: role.into(),
                account_no: account.into(),
                confirm: true,
            };
            assert_eq!(confirm_role(&tx, &input, "synthetic").unwrap()["ok"], true);
        }
        let wrong = Role {
            role: "fx_gain".into(),
            account_no: "L77".into(),
            confirm: true,
        };
        assert!(confirm_role(&tx, &wrong, "synthetic").is_err());
        assert_eq!(resolution(&tx, "fx_gain").unwrap()["accountNo"], "G77");
    }
    #[test]
    fn account_and_role_setup_is_explicit_compatible_and_retry_safe() {
        let mut db = company();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let account: Account = serde_json::from_value(
            json!({"accountNo":"V","name":"Synthetic VAT","type":"vat","normalBalance":"debit"}),
        )
        .unwrap();
        create_account(&tx, &account, "synthetic").unwrap();
        assert!(create_account(&tx, &account, "synthetic").is_err());
        let mut role = Role {
            role: "input_vat".into(),
            account_no: "V".into(),
            confirm: false,
        };
        assert!(confirm_role(&tx, &role, "synthetic").is_err());
        role.confirm = true;
        let confirmed = confirm_role(&tx, &role, "synthetic").unwrap();
        assert_eq!(confirmed["version"], 1);
        assert_eq!(confirm_role(&tx, &role, "synthetic").unwrap(), confirmed);
        role.role = "output_vat".into();
        assert!(confirm_role(&tx, &role, "synthetic").is_err());
        let state = accounts(&tx, "synthetic").unwrap();
        assert_eq!(state["accounts"].as_array().unwrap().len(), 1);
        assert!(
            state["accountRoles"]["resolutions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["role"] == "input_vat" && r["accountNo"] == "V" && r["ok"] == true)
        );
    }
    #[test]
    fn profile_cvr_is_canonical_audited_and_invalid_values_preserve_identity() {
        let mut db = company();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let input = json!({"cvr":" dk87654321 "});
        let updated = update_profile(&tx, input.as_object().unwrap(), "synthetic").unwrap();
        assert_eq!(updated["cvr"], "DK87654321");
        for invalid in [
            json!({"cvr":"1234"}),
            json!({"cvr":"8765432x"}),
            json!({"cvr":87654321}),
        ] {
            assert!(update_profile(&tx, invalid.as_object().unwrap(), "synthetic").is_err());
            assert_eq!(profile(&tx).unwrap()["cvr"], "DK87654321");
        }
        let empty = json!({"cvr":""});
        assert!(
            update_profile(&tx, empty.as_object().unwrap(), "synthetic").unwrap()["cvr"].is_null()
        );
        let events: i64 = tx.query_row("SELECT COUNT(*) FROM audit_log WHERE event_type='company_profile_update' AND actor='user:synthetic'", [], |r|r.get(0)).unwrap();
        assert_eq!(events, 2);
    }
    #[test]
    fn vat_cadence_preserves_history_and_reopened_activity_blocks_deregistration() {
        let mut db = company();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        cadence(&tx, None, "synthetic").unwrap();
        cadence(&tx, Some("quarter"), "synthetic").unwrap();
        tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('V','Synthetic VAT','vat','debit'); INSERT INTO journal_entries(id,entry_no,transaction_date,text,entry_hash,rule_version) VALUES(1,'SYN-1','2026-01-10','Synthetic','synthetic','synthetic'); INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(1,1,25,0);").unwrap();
        assert!(cadence(&tx, None, "synthetic").is_err());
        assert!(cadence(&tx, Some("month"), "synthetic").is_err());
        tx.execute_batch("INSERT INTO accounting_periods(id,period_start,period_end,kind,status) VALUES(1,'2026-01-01','2026-03-31','vat_period','closed'); INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('period_reopen','accounting_period','1','Synthetic reopen','user:synthetic');").unwrap();
        assert!(cadence(&tx, None, "synthetic").is_err());
        tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('period_close','accounting_period','1','Synthetic close','user:synthetic')", []).unwrap();
        cadence(&tx, None, "synthetic").unwrap();
        assert!(cadence(&tx, Some("month"), "synthetic").is_err());
        cadence(&tx, Some("quarter"), "synthetic").unwrap();
        assert_eq!(profile(&tx).unwrap()["vatPeriodType"], "quarter");
    }
}
