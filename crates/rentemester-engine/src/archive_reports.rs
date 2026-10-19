//! Source-shaped closing-history reports. No invented periods, postings or classifications.
use crate::{
    Error, archive_import::ArchiveDecimal, financial_statements as statements, money::Money,
    trial_balance::Account,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[cfg(test)]
mod tests;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Journal,
    TrialBalance,
    IncomeStatement,
    Balance,
    Overview,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Posting {
    pub line_no: u32,
    pub account_no: String,
    pub account_name: String,
    pub transaction_date: String,
    pub voucher: String,
    pub voucher_type: String,
    pub text: String,
    pub vat_type: String,
    pub amount: Money,
    pub source_amount: ArchiveDecimal,
    pub running_balance: Option<Money>,
    pub source_running_balance: Option<ArchiveDecimal>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Closing {
    pub line_no: u32,
    pub account_no: String,
    pub account_name: String,
    pub amount: Money,
    pub source_amount: ArchiveDecimal,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Year {
    pub fiscal_year: u16,
    pub source_system: String,
    pub import_id: i64,
    pub source_hash: String,
    pub parser_contract: String,
    pub declared_currency: String,
    pub has_balances: bool,
    pub postings: Vec<Posting>,
    pub balances: Vec<Closing>,
}
pub struct Context {
    pub slug: String,
    pub ledger_uuid: String,
    pub company: Value,
    pub fiscal_years: Value,
}
pub struct Classification {
    pub account_no: String,
    pub kind: String,
    pub normal_balance: String,
}
fn negative(n: &Money) -> Money {
    Money::default().subtract(n)
}
fn split(n: &Money) -> (Money, Money) {
    if n.is_negative() {
        (Money::default(), negative(n))
    } else {
        (n.clone(), Money::default())
    }
}
fn valid(c: &Classification) -> bool {
    matches!(
        c.kind.as_str(),
        "asset" | "liability" | "equity" | "income" | "expense" | "vat"
    ) && matches!(c.normal_balance.as_str(), "debit" | "credit")
}
struct ClosingFigures {
    accounts: Vec<Account>,
    rows: Vec<Value>,
    unknown: Vec<Value>,
    debit: Money,
    credit: Money,
}
fn closing(year: &Year, chart: &BTreeMap<&str, &Classification>) -> ClosingFigures {
    let mut grouped: BTreeMap<Vec<u16>, (String, String, Money, Vec<Value>)> = BTreeMap::new();
    for b in &year.balances {
        let g = grouped
            .entry(b.account_no.encode_utf16().collect())
            .or_insert_with(|| {
                (
                    b.account_no.clone(),
                    b.account_name.clone(),
                    Money::default(),
                    vec![],
                )
            });
        g.2.add(&b.amount);
        g.3.push(json!({"lineNo":b.line_no,"sourceAmount":b.source_amount,"amount":b.amount}));
    }
    let mut f = ClosingFigures {
        accounts: vec![],
        rows: vec![],
        unknown: vec![],
        debit: Money::default(),
        credit: Money::default(),
    };
    for (no, name, balance, source_rows) in grouped.into_values() {
        let classification = chart.get(no.as_str()).copied();
        let (debit, credit) = split(&balance);
        f.debit.add(&debit);
        f.credit.add(&credit);
        let row = json!({"accountNo":no,"name":name,"type":classification.map(|c|&c.kind),"normalBalance":classification.map(|c|&c.normal_balance),"debit":debit,"credit":credit,"balance":balance,"amount":balance,"sourceRows":source_rows});
        if !classification.is_some_and(valid) {
            f.unknown.push(row.clone());
        }
        if let Some(c) = classification.filter(|c| valid(c)) {
            f.accounts.push(Account {
                account_no: no,
                name,
                kind: c.kind.clone(),
                normal_balance: c.normal_balance.clone(),
                debit,
                credit,
                balance,
            });
        }
        f.rows.push(row);
    }
    f
}
fn lines(section: &statements::Section, complete: bool) -> Value {
    json!(section.lines.iter().map(|l|json!({"accountNo":l.account_no,"name":l.name,"amount":if l.account_no=="—"&&!complete{Value::Null}else{json!(l.amount)},"priorAmount":null})).collect::<Vec<_>>())
}
fn section(section: &statements::Section, complete: bool) -> Value {
    json!({"lines":lines(section,complete),"total":if complete{json!(section.total)}else{Value::Null},"priorTotal":null})
}
fn posting_id(year: &Year, p: &Posting) -> String {
    format!(
        "archive:{}:{:04}:posting:{}",
        year.source_hash, year.fiscal_year, p.line_no
    )
}
fn journal(year: &Year, account: Option<&str>) -> Result<Value, Error> {
    if account.is_some_and(|a| !year.postings.iter().any(|p| p.account_no == a)) {
        return Err(Error::new(
            "invalid_account",
            "account not present in archived postings",
        ));
    }
    let mut groups: BTreeMap<String, Vec<&Posting>> = BTreeMap::new();
    for p in &year.postings {
        let key = if p.voucher.is_empty() {
            format!("line:{}", p.line_no)
        } else {
            format!("voucher:{}", p.voucher)
        };
        groups.entry(key).or_default().push(p);
    }
    let mut entries = vec![];
    for (key, ps) in groups {
        if account.is_some_and(|a| !ps.iter().any(|p| p.account_no == a)) {
            continue;
        }
        let mut total = Money::default();
        let mut wire = vec![];
        for p in &ps {
            let (debit, credit) = split(&p.amount);
            total.add(&debit);
            wire.push(json!({"journalLineId":null,"sourceId":posting_id(year,p),"lineNo":p.line_no,"accountNo":p.account_no,"accountName":p.account_name,"date":p.transaction_date,"voucher":p.voucher,"voucherType":p.voucher_type,"vatType":p.vat_type,"debit":debit,"credit":credit,"balance":p.amount,"sourceAmount":p.source_amount,"runningBalance":p.running_balance,"sourceRunningBalance":p.source_running_balance,"text":p.text}));
        }
        entries.push(json!({"id":null,"journalEntryId":null,"sourceId":format!("archive:{}:{:04}:{key}",year.source_hash,year.fiscal_year),"entryNo":if ps[0].voucher.is_empty(){key.as_str()}else{&ps[0].voucher},"date":ps.iter().map(|p|&p.transaction_date).min(),"text":ps.iter().find(|p|!p.text.is_empty()).map(|p|p.text.as_str()).unwrap_or(""),"total":total,"documentId":null,"documentNo":null,"reversalOfEntryId":null,"reversalOfEntryNo":null,"reversedByEntryId":null,"reversedByEntryNo":null,"lines":wire}));
    }
    entries.sort_by(|a, b| {
        b["date"]
            .as_str()
            .cmp(&a["date"].as_str())
            .then(b["sourceId"].as_str().cmp(&a["sourceId"].as_str()))
    });
    let filter=account.map(|a|json!({"accountNo":a,"name":year.postings.iter().find(|p|p.account_no==a).map(|p|p.account_name.as_str()).unwrap_or(a)}));
    Ok(json!({"entries":entries,"accountFilter":filter}))
}
/// Pure projection of previously verified source facts. The storage adapter owns proof coverage.
pub fn render(
    context: &Context,
    year: &Year,
    classifications: &[Classification],
    account: Option<&str>,
    resource: Resource,
) -> Result<Value, Error> {
    if account.is_some() && resource != Resource::Journal {
        return Err(Error::new(
            "invalid_account",
            "account filter is only supported for journal",
        ));
    }
    let chart: BTreeMap<_, _> = classifications
        .iter()
        .map(|c| (c.account_no.as_str(), c))
        .collect();
    let f = closing(year, &chart);
    let has_closing = year.has_balances && !year.balances.is_empty();
    let complete = has_closing && f.unknown.is_empty();
    let pnl = statements::profit_and_loss_accounts(&f.accounts);
    let sheet = statements::balance_sheet_accounts(&f.accounts);
    let status = if resource == Resource::Journal {
        "ready"
    } else if !has_closing {
        "unavailable"
    } else if resource != Resource::TrialBalance && !f.unknown.is_empty() {
        "incomplete"
    } else {
        "ready"
    };
    let reasons: Vec<&str> = if status == "ready" {
        vec![]
    } else if !year.has_balances {
        vec!["missing_closing_balances"]
    } else if year.balances.is_empty() {
        vec!["empty_closing_balances"]
    } else {
        vec!["unknown_account_classification"]
    };
    let mut body = json!({"slug":context.slug,"ledgerUuid":context.ledger_uuid,"selectedYear":format!("{:04}",year.fiscal_year),"source":"archive","selectedSource":"archive","archived":true,"archivedSource":year.source_system,"company":context.company,"companyIdentityBasis":"current","declaredCurrency":year.declared_currency,"fiscalYears":context.fiscal_years,"periodStart":null,"periodEnd":null,"asOfDate":null,"evidence":{"importId":year.import_id,"sourceHash":year.source_hash,"parserContract":year.parser_contract,"declaredCurrency":year.declared_currency,"sourceSystem":year.source_system,"classificationSource":"current_chart","periodBasis":"archived_closing_snapshot; date not supplied","observedPostingStart":year.postings.iter().map(|p|&p.transaction_date).min(),"observedPostingEnd":year.postings.iter().map(|p|&p.transaction_date).max()},"completeness":{"status":status,"reasons":reasons,"hasBalanceFile":year.has_balances,"closingBalanceCount":year.balances.len(),"unclassifiedAccounts":f.unknown},"comparison":{"status":"unavailable","reason":"archive_period_bounds_unknown"}});
    let figure = |m: &Money| if complete { json!(m) } else { Value::Null };
    match resource {
        Resource::Journal => merge(&mut body, journal(year, account)?),
        Resource::TrialBalance => merge(
            &mut body,
            json!({"basis":"closing_balance","rows":f.rows,"totalDebit":if has_closing{json!(f.debit)}else{Value::Null},"totalCredit":if has_closing{json!(f.credit)}else{Value::Null},"balanced":if has_closing{json!(f.debit==f.credit)}else{Value::Null}}),
        ),
        Resource::IncomeStatement => merge(
            &mut body,
            json!({"income":lines(&pnl.income,complete),"expense":lines(&pnl.expense,complete),"totalIncome":figure(&pnl.income.total),"totalExpense":figure(&pnl.expense.total),"result":figure(&pnl.result),"priorYearLabel":null,"priorTotalIncome":null,"priorTotalExpense":null,"priorResult":null}),
        ),
        Resource::Balance => merge(
            &mut body,
            json!({"assets":section(&sheet.assets,complete),"liabilities":section(&sheet.liabilities,complete),"equity":section(&sheet.equity,complete),"periodResult":figure(&sheet.period_result),"totalAssets":figure(&sheet.assets.total),"totalLiabilitiesAndEquity":figure(&sheet.total_liabilities_and_equity),"priorTotalLiabilitiesAndEquity":null,"priorAsOfDate":null,"balanced":if complete{json!(sheet.balanced)}else{Value::Null}}),
        ),
        Resource::Overview => merge(&mut body, overview(year, &chart, &pnl, complete)),
    }
    Ok(body)
}
fn merge(body: &mut Value, fields: Value) {
    body.as_object_mut()
        .expect("report object")
        .extend(fields.as_object().expect("fields object").clone());
}
fn overview(
    year: &Year,
    chart: &BTreeMap<&str, &Classification>,
    pnl: &statements::ProfitAndLoss,
    complete: bool,
) -> Value {
    let mut months: BTreeMap<&str, (Money, Money, bool)> = BTreeMap::new();
    let mut unknown: BTreeMap<&str, (String, Money, Vec<Value>)> = BTreeMap::new();
    for p in &year.postings {
        let month = months.entry(&p.transaction_date[..7]).or_default();
        match chart.get(p.account_no.as_str()).filter(|c| valid(c)) {
            Some(c) if c.kind == "income" => month.0.add(&negative(&p.amount)),
            Some(c) if c.kind == "expense" => month.1.add(&p.amount),
            Some(_) => {}
            None => {
                month.2 = true;
                let row = unknown
                    .entry(&p.account_no)
                    .or_insert_with(|| (p.account_name.clone(), Money::default(), vec![]));
                row.1.add(&p.amount);
                row.2.push(
                    json!({"lineNo":p.line_no,"sourceAmount":p.source_amount,"amount":p.amount}),
                );
            }
        }
    }
    let activity_unknown:Vec<_>=unknown.into_iter().map(|(no,(name,amount,rows))|json!({"accountNo":no,"name":name,"type":chart.get(no).map(|c|&c.kind),"normalBalance":chart.get(no).map(|c|&c.normal_balance),"amount":amount,"sourceRows":rows})).collect();
    let activity_complete = activity_unknown.is_empty();
    let months:Vec<_>=months.into_iter().map(|(m,(income,expense,unknown))|json!({"month":m,"label":m,"income":if unknown{Value::Null}else{json!(income)},"expense":if unknown{Value::Null}else{json!(expense)},"status":if unknown{"incomplete"}else{"ready"}})).collect();
    let mut recent: Vec<_> = year.postings.iter().collect();
    recent.sort_by(|a, b| {
        b.transaction_date
            .cmp(&a.transaction_date)
            .then(b.line_no.cmp(&a.line_no))
    });
    let recent:Vec<_>=recent.into_iter().take(8).map(|p|json!({"id":null,"sourceId":posting_id(year,p),"entryNo":p.voucher,"date":p.transaction_date,"text":p.text,"amount":p.amount,"sourceAmount":p.source_amount})).collect();
    let figure = |m: &Money| if complete { json!(m) } else { Value::Null };
    json!({"profitAndLoss":{"omsaetning":figure(&pnl.income.total),"udgifter":figure(&pnl.expense.total),"resultat":figure(&pnl.result),"months":months},"bank":{"balance":null,"actualBalance":null,"difference":null,"bankStatementStatus":"unavailable","status":"unavailable"},"receivables":{"openCount":null,"openTotal":null,"status":"unavailable"},"vat":null,"exceptions":{"count":null,"rows":[],"groups":[],"status":"unavailable"},"documents":{"unlinkedCount":null,"status":"unavailable"},"recentEntries":recent,"lastPostedDate":year.postings.iter().map(|p|&p.transaction_date).max(),"keyFigures":{"bruttomargin":null,"egenkapitalandel":null},"activity":{"status":if activity_complete{"ready"}else{"incomplete"},"unclassifiedAccounts":activity_unknown},"operationalState":"unavailable"})
}
