//! Separate human-attributed interest registration, posting and causal correction.
use crate::{invalid, legacy_money};
use rentemester_engine::{
    Error, fiscal_year::Date, invoice_interest as engine, money::Money, sales_invoice::wire,
};
use rusqlite::{Transaction, params};
use serde::Deserialize;
use serde_json::{Number, Value, json};
use std::collections::BTreeMap;
pub(crate) mod history;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Calculate {
    pub invoice_document_id: i64,
    pub as_of_date: String,
    pub reference_rate_percent: Option<Number>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Register {
    pub invoice_document_id: i64,
    pub as_of_date: String,
    pub reference_rate_percent: Option<Number>,
    pub note: Option<String>,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Post {
    pub invoice_document_id: i64,
    pub claim_id: Option<i64>,
    pub transaction_date: Option<String>,
    pub receivable_account_no: String,
    pub interest_income_account_no: String,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Correction {
    pub invoice_document_id: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PostCorrection {
    pub invoice_document_id: i64,
    pub transaction_date: Option<String>,
    pub reason: String,
    pub confirm: bool,
}
fn refused(message: impl Into<String>) -> Error {
    Error::new("interest_refused", message)
}
fn positive(id: i64) -> Result<(), Error> {
    if (1..=9007199254740991).contains(&id) {
        Ok(())
    } else {
        Err(refused("positive exact identifier required"))
    }
}
fn actor(subject: &str) -> Result<(), Error> {
    if subject.trim().is_empty() || subject.len() > 256 || subject.chars().any(char::is_control) {
        Err(refused("trusted bounded human actor required"))
    } else {
        Ok(())
    }
}
fn basis(m: &Money) -> Result<u32, Error> {
    m.decimal()
        .replace('.', "")
        .parse()
        .map_err(|_| refused("nonnegative bounded rate at exact two decimals required"))
}
fn rate(n: &Number) -> Result<u32, Error> {
    let value = Value::Number(n.clone());
    basis(&crate::invoice_fx::amount(&value)?)
}
fn audit(
    tx: &Transaction<'_>,
    event: &str,
    entity: &str,
    id: i64,
    subject: &str,
    message: &str,
) -> Result<(), Error> {
    actor(subject)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES(?1,?2,?3,?4,?5)",params![event,entity,id.to_string(),message,format!("user:{subject}")]).map_err(invalid)?;
    Ok(())
}
fn engine_wire(mut value: Value) -> Result<Value, Error> {
    fn walk(v: &mut Value) -> Result<(), Error> {
        match v {
            Value::Object(o) => {
                for (k, v) in o {
                    if [
                        "principalOpenBalance",
                        "referenceRatePercent",
                        "annualInterestRatePercent",
                        "accruedInterestAmount",
                        "priorClaimedInterest",
                        "totalInterestToDate",
                        "overClaimedInterest",
                        "postedInterest",
                        "lawfulInterest",
                        "alreadyCorrected",
                        "overClaimedAmount",
                    ]
                    .contains(&k.as_str())
                    {
                        *v = wire(&Money::parse(v.as_str().ok_or_else(|| {
                            refused("engine monetary representation differs")
                        })?)?)?;
                    } else {
                        walk(v)?;
                    }
                }
            }
            Value::Array(a) => {
                for v in a {
                    walk(v)?
                }
            }
            _ => {}
        }
        Ok(())
    }
    walk(&mut value)?;
    Ok(value)
}
fn claims(tx: &Transaction<'_>, h: &engine::History) -> Result<Vec<Value>, Error> {
    h.claims.iter().map(|c|{let no:Option<String>=match c.posted_journal_id{Some(j)=>tx.query_row("SELECT entry_no FROM journal_entries WHERE id=?1",[j],|r|r.get(0)).map_err(invalid)?,None=>None};Ok(json!({"id":c.id,"claimDate":c.claim_date,"referenceRatePercent":wire(&Money::parse(&format!("{}.{:02}",c.reference_basis_points/100,c.reference_basis_points%100))?)?,"annualInterestRatePercent":wire(&Money::parse(&format!("{}.{:02}",c.annual_basis_points/100,c.annual_basis_points%100))?)?,"referenceRateSource":c.rate_source,"interestAmount":wire(&c.amount)?,"postedJournalEntryId":c.posted_journal_id,"postedJournalEntryNo":no}))}).collect()
}
fn checked(tx: &Transaction<'_>, id: i64) -> Result<history::Loaded, Error> {
    let loaded = history::load(tx, id)?;
    engine::correction(&loaded.history)?;
    for c in &loaded.history.corrections {
        crate::audit_invoice::interest_correction_effects(tx, c.id)?;
    }
    Ok(loaded)
}
pub fn calculate(tx: &Transaction<'_>, input: &Calculate) -> Result<Value, Error> {
    crate::journal_preview::require_current(tx)?;
    let loaded = checked(tx, input.invoice_document_id)?;
    crate::reminder_claims::principal_open(tx, input.invoice_document_id)?;
    let calculation = engine::calculate(
        &loaded.history,
        &engine::Calculate {
            as_of: Date::parse(&input.as_of_date)?,
            reference_basis_points: input
                .reference_rate_percent
                .as_ref()
                .map(rate)
                .transpose()?,
        },
    )?;
    let mut out = engine_wire(serde_json::to_value(calculation).map_err(invalid)?)?;
    let o = out.as_object_mut().unwrap();
    o.insert("ok".into(), json!(true));
    o.insert("invoiceDocumentId".into(), json!(input.invoice_document_id));
    o.insert("invoiceNumber".into(), json!(loaded.no));
    o.insert("principalCurrency".into(), json!(loaded.history.currency));
    o.insert("claims".into(), json!(claims(tx, &loaded.history)?));
    Ok(out)
}
pub fn register(tx: &Transaction<'_>, input: &Register, subject: &str) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    actor(subject)?;
    if !input.confirm {
        return Err(refused(
            "explicit interest registration confirmation required",
        ));
    }
    if input.note.as_ref().is_some_and(|s| s.len() > 4096) {
        return Err(refused("interest note exceeds4096 bytes"));
    }
    let req = Calculate {
        invoice_document_id: input.invoice_document_id,
        as_of_date: input.as_of_date.clone(),
        reference_rate_percent: input.reference_rate_percent.clone(),
    };
    let before = calculate(tx, &req)?;
    let duplicate:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM invoice_interest_claims WHERE invoice_document_id=?1 AND claim_date=?2 AND reference_rate_percent=?3)",params![input.invoice_document_id,input.as_of_date,before["referenceRatePercent"].to_string()],|r|r.get(0)).map_err(invalid)?;
    if duplicate {
        return Err(refused(
            "interest already registered for date and reference rate; read canonical status",
        ));
    }
    let amount = crate::invoice_fx::amount(&before["accruedInterestAmount"])?;
    if amount.is_negative() || amount == Money::default() {
        return Err(refused(
            "positive incremental interest required before registration",
        ));
    }
    let id:i64=tx.query_row("INSERT INTO invoice_interest_claims(invoice_document_id,claim_date,reference_rate_percent,annual_interest_rate_percent,reference_rate_source,overdue_days,principal_open_balance,amount_dkk,note)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)RETURNING id",params![input.invoice_document_id,input.as_of_date,before["referenceRatePercent"].to_string(),before["annualInterestRatePercent"].to_string(),before["referenceRateSource"].as_str(),before["claimableDays"].as_i64(),before["principalOpenBalance"].to_string(),amount.decimal(),input.note],|r|r.get(0)).map_err(invalid)?;
    audit(
        tx,
        "invoice_interest_register",
        "invoice_interest_claim",
        id,
        subject,
        "Registered incremental late interest",
    )?;
    let mut out = calculate(tx, &req)?;
    out["claimId"] = json!(id);
    Ok(out)
}
fn account(tx: &Transaction<'_>, no: &str, kind: &str, normal: &str) -> Result<(), Error> {
    let yes:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND type=?2 AND normal_balance=?3 AND active=1 AND allow_direct_posting=1)",params![no,kind,normal],|r|r.get(0)).map_err(invalid)?;
    if yes {
        Ok(())
    } else {
        Err(refused(
            "explicit active directly-postable compatible accounts required",
        ))
    }
}
pub fn post(
    tx: &Transaction<'_>,
    input: &Post,
    subject: &str,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    actor(subject)?;
    if !input.confirm {
        return Err(refused(
            "explicit separate interest posting confirmation required",
        ));
    }
    let loaded = checked(tx, input.invoice_document_id)?;
    let claim = if let Some(id) = input.claim_id {
        positive(id)?;
        loaded.history.claims.iter().find(|c| c.id == id)
    } else {
        loaded
            .history
            .claims
            .iter()
            .find(|c| c.posted_journal_id.is_none())
    }
    .ok_or_else(|| refused("unposted interest claim not found for invoice"))?;
    if claim.posted_journal_id.is_some() {
        return Err(refused(
            "interest claim already posted; read canonical status",
        ));
    }
    let date = input
        .transaction_date
        .clone()
        .unwrap_or_else(|| claim.claim_date.to_string());
    if Date::parse(&date)? < claim.claim_date {
        return Err(refused("interest posting cannot predate registered claim"));
    }
    account(tx, &input.receivable_account_no, "asset", "debit")?;
    account(tx, &input.interest_income_account_no, "income", "credit")?;
    let result = crate::reminder_claims::post_journal(
        tx,
        input.invoice_document_id,
        None,
        &date,
        format!("Late interest {}", loaded.no),
        vec![
            crate::reminder_claims::line(
                &input.receivable_account_no,
                claim.amount.clone(),
                Money::default(),
                "Interest receivable",
            )?,
            crate::reminder_claims::line(
                &input.interest_income_account_no,
                Money::default(),
                claim.amount.clone(),
                "Interest income",
            )?,
        ],
        (subject, latest),
    )?;
    tx.execute(
        "INSERT INTO invoice_interest_postings(interest_claim_id,journal_entry_id)VALUES(?1,?2)",
        params![claim.id, result["entryId"].as_i64()],
    )
    .map_err(invalid)?;
    audit(
        tx,
        "invoice_interest_post",
        "invoice_interest_claim",
        claim.id,
        subject,
        "Posted registered late interest",
    )?;
    let mut out = calculate(
        tx,
        &Calculate {
            invoice_document_id: input.invoice_document_id,
            as_of_date: latest.to_string(),
            reference_rate_percent: None,
        },
    )?;
    out["claimId"] = json!(claim.id);
    out["journalEntryId"] = result["entryId"].clone();
    out["journalEntryNo"] = result["entryNo"].clone();
    Ok(out)
}
#[derive(Default)]
pub(crate) struct Capacity {
    pub certain: BTreeMap<String, Money>,
    pub income: BTreeMap<String, Money>,
    pub possible: Money,
    pub ambiguous: Money,
}
/// Generic receipts on a shared account do not identify a claim family.
pub(crate) fn capacity(
    tx: &Transaction<'_>,
    id: i64,
    balances: &BTreeMap<String, Money>,
) -> Result<Capacity, Error> {
    let mut interest = BTreeMap::<String, Money>::new();
    let mut other = BTreeMap::<String, Money>::new();
    let mut income = BTreeMap::<String, Money>::new();
    for (sql, is_interest) in [
        (
            "SELECT p.journal_entry_id FROM invoice_interest_postings p JOIN invoice_interest_claims c ON c.id=p.interest_claim_id WHERE c.invoice_document_id=?1",
            true,
        ),
        (
            "SELECT p.journal_entry_id FROM invoice_reminder_postings p JOIN invoice_reminders c ON c.id=p.reminder_id WHERE c.invoice_document_id=?1 UNION ALL SELECT p.journal_entry_id FROM invoice_compensation_postings p JOIN invoice_compensation_claims c ON c.id=p.compensation_claim_id WHERE c.invoice_document_id=?1",
            false,
        ),
    ] {
        let mut q = tx.prepare(sql).map_err(invalid)?;
        let ids = q
            .query_map([id], |r| r.get::<_, i64>(0))
            .map_err(invalid)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(invalid)?;
        for j in ids {
            let (assets, incomes) = origin_accounts(tx, j)?;
            for (a, m) in assets {
                if is_interest {
                    interest.entry(a).or_default().add(&m);
                } else {
                    other.entry(a).or_default().add(&m);
                }
            }
            if is_interest {
                for (a, m) in incomes {
                    income.entry(a).or_default().add(&m);
                }
            }
        }
    }
    let mut q=tx.prepare("SELECT journal_entry_id FROM invoice_interest_corrections WHERE invoice_document_id=?1 ORDER BY journal_entry_id").map_err(invalid)?;
    let ids = q
        .query_map([id], |r| r.get::<_, i64>(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    for j in ids {
        let mut q=tx.prepare("SELECT a.account_no,a.type,l.debit_amount,l.credit_amount FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1").map_err(invalid)?;
        let mut rs = q.query([j]).map_err(invalid)?;
        while let Some(r) = rs.next().map_err(invalid)? {
            let a: String = r.get(0).map_err(invalid)?;
            let dr = legacy_money(r.get_ref(2).map_err(invalid)?)?;
            let cr = legacy_money(r.get_ref(3).map_err(invalid)?)?;
            let is_asset = r.get::<_, String>(1).map_err(invalid)? == "asset";
            let delta = if is_asset {
                cr.subtract(&dr)
            } else {
                dr.subtract(&cr)
            };
            let target = if is_asset { &mut interest } else { &mut income };
            let old = target.entry(a).or_default();
            *old = old.subtract(&delta);
        }
    }
    let mut out = Capacity {
        income,
        ..Default::default()
    };
    for (a, i) in interest {
        if i.is_negative() {
            return Err(refused("interest origin over-corrected"));
        }
        let aggregate = balances.get(&a).cloned().unwrap_or_default();
        let possible = min(&i, &aggregate);
        let certain = min(
            &i,
            &nonnegative(aggregate.subtract(&other.get(&a).cloned().unwrap_or_default())),
        );
        out.possible.add(&possible);
        let certain_old = out.certain.entry(a).or_default();
        certain_old.add(&certain);
        out.ambiguous.add(&possible.subtract(&certain));
    }
    Ok(out)
}
fn min(a: &Money, b: &Money) -> Money {
    if a.subtract(b).is_negative() {
        a.clone()
    } else {
        b.clone()
    }
}
fn nonnegative(m: Money) -> Money {
    if m.is_negative() { Money::default() } else { m }
}
type AccountAmounts = BTreeMap<String, Money>;
fn origin_accounts(
    tx: &Transaction<'_>,
    journal: i64,
) -> Result<(AccountAmounts, AccountAmounts), Error> {
    let mut assets = BTreeMap::<String, Money>::new();
    let mut income = BTreeMap::<String, Money>::new();
    let mut q=tx.prepare("SELECT a.account_no,a.type,l.debit_amount,l.credit_amount FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1 ORDER BY a.account_no,l.id").map_err(invalid)?;
    let mut rs = q.query([journal]).map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        let no: String = r.get(0).map_err(invalid)?;
        if r.get::<_, String>(1).map_err(invalid)? == "asset" {
            assets
                .entry(no)
                .or_default()
                .add(&legacy_money(r.get_ref(2).map_err(invalid)?)?);
        } else {
            income
                .entry(no)
                .or_default()
                .add(&legacy_money(r.get_ref(3).map_err(invalid)?)?);
        }
    }
    Ok((assets, income))
}
#[derive(Default)]
struct Plan {
    claims: Vec<(i64, String, Money, Money)>,
    receivables: BTreeMap<String, Money>,
    incomes: BTreeMap<String, Money>,
}
fn allocate_origins(
    origins: &mut BTreeMap<String, Money>,
    amount: &Money,
) -> Result<BTreeMap<String, Money>, Error> {
    rentemester_engine::reminder_claims::allocate(origins, amount)
}
fn plan(tx: &Transaction<'_>, proposal: &engine::CorrectionProposal) -> Result<Plan, Error> {
    let mut left = proposal.over_claimed_amount.clone();
    let mut out = Plan::default();
    for c in &proposal.claim_overclaims {
        let mut used = Money::default();
        let mut q=tx.prepare("SELECT p.amount_dkk FROM invoice_interest_correction_plan_claims p JOIN invoice_interest_corrections c ON c.journal_entry_id=p.journal_entry_id WHERE p.interest_claim_id=?1 ORDER BY c.journal_entry_id").map_err(invalid)?;
        let mut rs = q.query([c.claim_id]).map_err(invalid)?;
        while let Some(r) = rs.next().map_err(invalid)? {
            used.add(&legacy_money(r.get_ref(0).map_err(invalid)?)?);
        }
        let available = nonnegative(c.over_claimed_amount.subtract(&used));
        let take = min(&left, &available);
        if take == Money::default() {
            continue;
        }
        let (mut assets, mut incomes) = origin_accounts(tx, c.journal_entry_id)?;
        // Prior corrections consumed the original account order, not arbitrary new accounts.
        for origins in [&mut assets, &mut incomes] {
            if used != Money::default() {
                let allocated = allocate_origins(origins, &used)?;
                for (a, m) in allocated {
                    let old = origins.get_mut(&a).unwrap();
                    *old = old.subtract(&m);
                }
            }
        }
        for (a, m) in allocate_origins(&mut assets, &take)? {
            out.receivables.entry(a).or_default().add(&m);
        }
        for (a, m) in allocate_origins(&mut incomes, &take)? {
            out.incomes.entry(a).or_default().add(&m);
        }
        out.claims.push((
            c.claim_id,
            c.claim_date.to_string(),
            take.clone(),
            c.over_claimed_amount.clone(),
        ));
        left = left.subtract(&take);
        if left == Money::default() {
            break;
        }
    }
    if left != Money::default() {
        return Err(refused(
            "new correction exceeds remaining authenticated causal claim capacity",
        ));
    }
    Ok(out)
}
fn sum(values: impl Iterator<Item = Money>) -> Money {
    let mut total = Money::default();
    for v in values {
        total.add(&v);
    }
    total
}
pub fn correction(tx: &Transaction<'_>, input: &Correction) -> Result<Value, Error> {
    crate::journal_preview::require_current(tx)?;
    let loaded = checked(tx, input.invoice_document_id)?;
    let proposal = engine::correction(&loaded.history)?;
    let claims = crate::reminder_claims::validated_claims(tx, input.invoice_document_id)?;
    let cap = capacity(tx, input.invoice_document_id, &claims.balances)?;
    let safe = sum(cap.certain.values().cloned());
    let requires_refund = !proposal.over_claimed_amount.subtract(&safe).is_negative()
        && proposal.over_claimed_amount != safe;
    let mut reason = None;
    let mut proposed = vec![];
    if !proposal.has_proposal {
        reason = Some("No over-claimed interest remains".to_string());
    } else if requires_refund {
        reason=Some("Interest was paid or its receipt allocation is ambiguous; resolve allocation or use a refund".into());
    } else {
        match plan(tx, &proposal) {
            Ok(plan) => {
                for (a, m) in &plan.receivables {
                    if cap
                        .certain
                        .get(a)
                        .is_none_or(|v| v.subtract(m).is_negative())
                    {
                        reason = Some(
                            "Causal receivable is not provably unpaid on its original account"
                                .into(),
                        );
                    }
                }
                for (a, m) in &plan.incomes {
                    if cap
                        .income
                        .get(a)
                        .is_none_or(|v| v.subtract(m).is_negative())
                    {
                        reason = Some("Original interest income is already reversed".into());
                    }
                }
                if reason.is_none() {
                    for (a, m) in &plan.receivables {
                        proposed.push(json!({"accountNo":a,"debitAmount":wire(&Money::default())?,"creditAmount":wire(m)?}));
                    }
                    for (a, m) in &plan.incomes {
                        proposed.push(json!({"accountNo":a,"debitAmount":wire(m)?,"creditAmount":wire(&Money::default())?}));
                    }
                }
            }
            Err(e) => reason = Some(e.message),
        }
    }
    let mut out = engine_wire(serde_json::to_value(&proposal).map_err(invalid)?)?;
    out["ok"] = json!(true);
    out["invoiceDocumentId"] = json!(input.invoice_document_id);
    out["invoiceNumber"] = json!(loaded.no);
    out["principalCurrency"] = json!(loaded.history.currency);
    out["canPost"] = json!(proposal.has_proposal && reason.is_none());
    out["reason"] = json!(reason);
    out["requiresRefund"] = json!(requires_refund);
    out["proposedLines"] = json!(proposed);
    out["outstandingClaimBalance"] = wire(&safe)?;
    out["possibleOutstandingInterestReceivable"] = wire(&cap.possible)?;
    out["ambiguousInterestReceivable"] = wire(&cap.ambiguous)?;
    Ok(out)
}
pub fn post_correction(
    tx: &Transaction<'_>,
    input: &PostCorrection,
    subject: &str,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    actor(subject)?;
    if !input.confirm || input.reason.trim().is_empty() || input.reason.len() > 4096 {
        return Err(refused(
            "explicit confirmation and bounded nonblank correction reason required",
        ));
    }
    let before = correction(
        tx,
        &Correction {
            invoice_document_id: input.invoice_document_id,
        },
    )?;
    if before["canPost"] != true {
        return Err(refused(
            before["reason"]
                .as_str()
                .unwrap_or("correction cannot be posted"),
        ));
    }
    let loaded = checked(tx, input.invoice_document_id)?;
    let proposal = engine::correction(&loaded.history)?;
    let plan = plan(tx, &proposal)?;
    let date = input
        .transaction_date
        .clone()
        .unwrap_or_else(|| proposal.through_date.unwrap().to_string());
    let parsed = Date::parse(&date)?;
    if plan.claims.iter().any(|(_, d, _, _)| d > &date) || date < loaded.date {
        return Err(refused(
            "correction cannot predate its causal claims/invoice",
        ));
    }
    let mut lines = vec![];
    for (a, m) in &plan.receivables {
        account(tx, a, "asset", "debit")?;
        lines.push(crate::reminder_claims::line(
            a,
            Money::default(),
            m.clone(),
            "Interest correction receivable",
        )?);
    }
    for (a, m) in &plan.incomes {
        account(tx, a, "income", "credit")?;
        lines.push(crate::reminder_claims::line(
            a,
            m.clone(),
            Money::default(),
            "Interest correction income",
        )?);
    }
    let result = crate::reminder_claims::post_journal(
        tx,
        input.invoice_document_id,
        None,
        &parsed.to_string(),
        format!("Late interest correction {}", loaded.no),
        lines,
        (subject, latest),
    )?;
    let j = result["entryId"]
        .as_i64()
        .ok_or_else(|| refused("correction journal identity missing"))?;
    tx.execute("INSERT INTO invoice_interest_correction_plans(journal_entry_id,invoice_document_id,correction_date,amount_dkk)VALUES(?1,?2,?3,?4)",params![j,input.invoice_document_id,date,proposal.over_claimed_amount.decimal()]).map_err(invalid)?;
    for (id, date, amount, ceiling) in &plan.claims {
        tx.execute("INSERT INTO invoice_interest_correction_plan_claims(journal_entry_id,interest_claim_id,claim_date,amount_dkk,claim_ceiling_dkk)VALUES(?1,?2,?3,?4,?5)",params![j,id,date,amount.decimal(),ceiling.decimal()]).map_err(invalid)?;
    }
    for (a, m) in &plan.receivables {
        tx.execute("INSERT INTO invoice_interest_correction_plan_lines(journal_entry_id,account_no,debit_amount,credit_amount)VALUES(?1,?2,0,?3)",params![j,a,m.decimal()]).map_err(invalid)?;
    }
    for (a, m) in &plan.incomes {
        tx.execute("INSERT INTO invoice_interest_correction_plan_lines(journal_entry_id,account_no,debit_amount,credit_amount)VALUES(?1,?2,?3,0)",params![j,a,m.decimal()]).map_err(invalid)?;
    }
    let id:i64=tx.query_row("INSERT INTO invoice_interest_corrections(invoice_document_id,correction_date,amount_dkk,reason,journal_entry_id)VALUES(?1,?2,?3,?4,?5)RETURNING id",params![input.invoice_document_id,date,proposal.over_claimed_amount.decimal(),input.reason,j],|r|r.get(0)).map_err(invalid)?;
    audit(
        tx,
        "invoice_interest_correction",
        "invoice_interest_correction",
        id,
        subject,
        &input.reason,
    )?;
    let mut out = correction(
        tx,
        &Correction {
            invoice_document_id: input.invoice_document_id,
        },
    )?;
    out["correctionId"] = json!(id);
    out["journalEntryId"] = json!(j);
    out["journalEntryNo"] = result["entryNo"].clone();
    Ok(out)
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod refusals;
