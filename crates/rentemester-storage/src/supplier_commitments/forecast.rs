//! Exact weekly roll-forward. Undated budgets never become invented cash dates.
use super::*;
use rentemester_engine::fiscal_year::{self, LabelStrategy, StartMonth};
use std::collections::{BTreeMap, BTreeSet};
struct Flow {
    date: String,
    amount: Money,
    kind: &'static str,
    reference: String,
    currency: String,
    assumption: bool,
    settlement_unknown: bool,
}
fn value_money(v: &Value) -> Result<Money, Error> {
    money(
        v.as_number()
            .ok_or_else(|| refused("forecast amount unavailable"))?,
    )
}
fn opening(
    tx: &Transaction<'_>,
    cutoff: &str,
    currency: &str,
    excluded: &mut Vec<String>,
) -> Result<Option<Money>, Error> {
    if currency != "DKK" {
        excluded.push("Bogført åbningssaldo er i DKK; omregning til virksomhedens valuta er ikke tilgængelig.".into());
        return Ok(None);
    }
    let mut q=tx.prepare("SELECT DISTINCT a.id FROM accounts a WHERE a.type='asset' AND (EXISTS(SELECT 1 FROM bank_accounts b WHERE b.ledger_account_no=a.account_no AND b.currency='DKK') OR EXISTS(SELECT 1 FROM account_role_mappings r WHERE r.account_no=a.account_no AND r.role='bank' AND r.status='confirmed'))").map_err(invalid)?;
    let ids = q
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if ids.is_empty() {
        excluded.push("Ingen bekræftet bankrolle eller bogført DKK-bankkontomapping; åbningssaldo kan ikke bestemmes.".into());
        return Ok(None);
    }
    let mut total = Money::default();
    for id in ids {
        let mut q=tx.prepare("SELECT l.debit_amount,l.credit_amount FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_entry_id WHERE l.account_id=?1 AND j.transaction_date<=?2 AND j.currency='DKK' ORDER BY j.id,l.id").map_err(invalid)?;
        let mut rows = q.query(params![id, cutoff]).map_err(invalid)?;
        while let Some(r) = rows.next().map_err(invalid)? {
            total.add(
                &legacy_money(r.get_ref(0).map_err(invalid)?)?
                    .subtract(&legacy_money(r.get_ref(1).map_err(invalid)?)?),
            );
        }
    }
    let statement = crate::bank::statement_balance(tx, cutoff)?;
    if let Some(balance) = statement.balance {
        if balance != total {
            excluded.push(
                "Bogført bank og uafhængigt kontoudtog stemmer ikke ved prognosestart.".into(),
            );
        }
    } else {
        excluded.push("Uafhængig kontoudtogssaldo ved prognosestart er ikke tilgængelig; åbningssaldo er alene bogført bank.".into());
    }
    Ok(Some(total))
}
fn receivables(
    tx: &Transaction<'_>,
    slug: &str,
    start: &str,
    company: &Value,
) -> Result<Vec<Flow>, Error> {
    let month = StartMonth::new(
        company["fiscalYearStartMonth"]
            .as_i64()
            .ok_or_else(|| refused("missing fiscal setup"))? as i32,
    )?;
    let strategy = LabelStrategy::try_from(
        company["fiscalYearLabelStrategy"]
            .as_str()
            .ok_or_else(|| refused("missing fiscal setup"))?,
    )?;
    let mut years = BTreeSet::new();
    let mut q=tx.prepare("SELECT DISTINCT invoice_date FROM documents WHERE document_type='issued_invoice' AND invoice_date<=?1").map_err(invalid)?;
    for date in q
        .query_map([start], |r| r.get::<_, String>(0))
        .map_err(invalid)?
    {
        years.insert(
            fiscal_year::for_date(Date::parse(&date.map_err(invalid)?)?, month, strategy)?
                .display_label,
        );
    }
    let mut flows = vec![];
    for year in years {
        let list = crate::sales::list(tx, slug, start, Some(&year))?;
        for row in list["invoices"]
            .as_array()
            .ok_or_else(|| refused("invoice projection unavailable"))?
        {
            if row["invoiceDate"].as_str().is_none_or(|date| date > start) {
                continue;
            }
            let amount = value_money(&row["openBalance"])?;
            if !amount.is_negative()
                && amount != Money::default()
                && let Some(date) = row["effectiveDueDate"].as_str()
            {
                flows.push(Flow {
                    date: date.into(),
                    amount,
                    kind: "receivables",
                    reference: format!("invoice:{}", row["documentId"]),
                    currency: row["currency"].as_str().unwrap_or("UNAVAILABLE").into(),
                    assumption: false,
                    settlement_unknown: false,
                });
            }
        }
    }
    Ok(flows)
}
fn vat_flows(tx: &Transaction<'_>, start: &str, end: &str) -> Result<Vec<Flow>, Error> {
    let cadence: Option<String> = tx
        .query_row(
            "SELECT vat_period_type FROM companies WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let Some(cadence) = cadence else {
        return Ok(vec![]);
    };
    let span = rentemester_engine::vat_period::span(&cadence)?;
    let first = start[..4].parse::<i32>().map_err(invalid)? - 1;
    let last = end[..4].parse::<i32>().map_err(invalid)?;
    let mut out = vec![];
    for year in first..=last {
        for month in (1..=12).step_by(span as usize) {
            let w = rentemester_engine::vat_period::window(
                &format!("{year:04}-{month:02}-01"),
                &cadence,
            )?;
            if w.deadline.as_str() < start || w.deadline.as_str() > end {
                continue;
            }
            let report = crate::vat_report::range(tx, &w.start, &w.end)?;
            if report["errors"].as_array().is_none_or(|a| !a.is_empty()) {
                return Err(refused("VAT obligation preflight unavailable"));
            }
            let amount = value_money(&report["payable"])?;
            if amount.is_negative() || amount == Money::default() {
                continue;
            }
            let row:Option<(i64,String)>=tx.query_row("SELECT id,status FROM accounting_periods WHERE period_start=?1 AND period_end=?2 AND kind IN ('vat_period','vat_quarter') ORDER BY id DESC LIMIT 1",params![w.start,w.end],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(invalid)?;
            let locked = if let Some((id, status)) = row {
                matches!(
                    crate::periods::effective(tx, id, status)?.as_str(),
                    "closed" | "reported"
                )
            } else {
                false
            };
            out.push(Flow {
                date: w.deadline,
                amount,
                kind: if locked { "obligations" } else { "scenarios" },
                reference: format!("vat:{}:{}", w.start, w.end),
                currency: "DKK".into(),
                assumption: !locked,
                settlement_unknown: true,
            });
        }
    }
    Ok(out)
}
pub(super) fn build(
    tx: &Transaction<'_>,
    slug: &str,
    start: &str,
    occurrences: &[Value],
) -> Result<Value, Error> {
    build_horizon(tx, slug, start, occurrences, false)
}
pub(super) fn build_horizon(
    tx: &Transaction<'_>,
    slug: &str,
    start: &str,
    occurrences: &[Value],
    monthly: bool,
) -> Result<Value, Error> {
    let company = crate::company_setup::profile(tx)?;
    let currency = company["currency"]
        .as_str()
        .ok_or_else(|| refused("company currency unavailable"))?;
    let intervals = intervals(start, monthly)?;
    let end = intervals
        .last()
        .ok_or_else(|| refused("forecast horizon missing"))?
        .1
        .clone();
    let mut included =
        vec!["Godkendte umatchede DKK-forpligtelser i DKK-virksomheder.".to_string()];
    let mut excluded=vec!["Ikke-registrerede forpligtelser, løn, selskabsskat og eksterne betalinger er ikke fuldt dækket.".into(),"Daterede reviewede budget-/scenarie-/intercompany-antagelser er endnu ikke tilgængelige i native prognose.".into(),"Tilgodehavender bruger native aktuelle saldi; historisk betalingsstatus er ikke rekonstrueret.".into(),"Månedsbudgetter vises kun som udateret information og påvirker aldrig likviditet.".into(),"Tilgodehavender uden dokumenteret forfaldsdato er udeladt; forfaldne kilder flyttes ikke til en opfundet betalingsdato.".into()];
    let opening = opening(tx, start, currency, &mut excluded)?;
    let mut flows = vec![];
    match receivables(tx, slug, start, &company) {
        Ok(rows) => {
            flows.extend(rows);
            included.push("Native udstedte tilgodehavender med dokumenteret forfaldsdato.".into());
        }
        Err(_) => excluded.push(
            "Tilgodehavender kunne ikke indlæses; prognosen mangler disse indbetalinger.".into(),
        ),
    }
    match crate::payables::list(tx, slug, start, "open") {
        Ok(list) => {
            for p in list["rows"]
                .as_array()
                .ok_or_else(|| refused("payables projection unavailable"))?
            {
                flows.push(Flow {
                    date: p["dueDate"]
                        .as_str()
                        .ok_or_else(|| refused("payable due date unavailable"))?
                        .into(),
                    amount: value_money(&p["openBalance"])?,
                    kind: "payables",
                    reference: format!("payable:{}", p["payableId"]),
                    currency: p["currency"].as_str().unwrap_or("UNAVAILABLE").into(),
                    assumption: false,
                    settlement_unknown: false,
                });
            }
            included.push("Registrerede kreditorers åbne saldo ved prognosestart.".into());
        }
        Err(_) => excluded
            .push("Kreditorer kunne ikke indlæses; prognosen mangler disse udbetalinger.".into()),
    }
    for o in occurrences {
        flows.push(Flow {
            date: o["date"].as_str().unwrap().into(),
            amount: value_money(&o["amount"])?,
            kind: "commitments",
            reference: o["commitmentId"].as_str().unwrap().into(),
            currency: o["currency"].as_str().unwrap().into(),
            assumption: false,
            settlement_unknown: false,
        });
    }
    match vat_flows(tx, start, &end) {
        Ok(rows) => {
            flows.extend(rows);
            included.push("Lukket/indberettet moms med lovbestemt betalingsdato; betalingsstatus ukendt. Åben moms alene som scenarie.".into());
        }
        Err(_) => excluded.push("Momsforpligtelser kunne ikke valideres og er udeladt.".into()),
    }
    let mut budgets = vec![];
    let mut q=tx.prepare("SELECT b.id,b.period,b.account_no,b.amount FROM budget_lines b JOIN accounts a ON a.account_no=b.account_no WHERE a.type='expense' AND b.id IN(SELECT MAX(id) FROM budget_lines GROUP BY period,account_no) AND b.period BETWEEN ?1 AND ?2 ORDER BY b.period,b.id").map_err(invalid)?;
    let mut rs = q.query(params![&start[..7], &end[..7]]).map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        budgets.push((
            r.get::<_, i64>(0).map_err(invalid)?,
            r.get::<_, String>(1).map_err(invalid)?,
            r.get::<_, String>(2).map_err(invalid)?,
            legacy_money(r.get_ref(3).map_err(invalid)?)?,
        ));
    }
    let mut cash = opening.clone();
    let mut scenario = cash.clone();
    let mut lowest = cash.clone();
    let mut periods = vec![];
    let mut seen_budget = BTreeSet::new();
    for (index, (from, to)) in intervals.into_iter().enumerate() {
        let opening_week = cash.clone();
        let mut sums: BTreeMap<&str, Money> = [
            "receivables",
            "payables",
            "commitments",
            "budgets",
            "scenarios",
            "obligations",
            "intercompany",
            "undatedBudgetAssumptions",
        ]
        .into_iter()
        .map(|k| (k, Money::default()))
        .collect();
        let mut sources = vec![];
        let mut omitted = vec![];
        for flow in &flows {
            if flow.date.as_str() < start {
                if index == 0 {
                    omitted.push(json!({"source":flow.reference,"amount":wire(&flow.amount)?,"currency":flow.currency,"reason":"Forfalden før prognosestart; ingen ny betalingsdato antages."}));
                }
                continue;
            }
            if flow.date < from || flow.date > to {
                continue;
            }
            if flow.currency != currency || currency != "DKK" {
                omitted.push(json!({"source":flow.reference,"amount":wire(&flow.amount)?,"currency":flow.currency,"reason":"Valuta afviger; ingen dokumenteret valutakurs."}));
                continue;
            }
            sums.get_mut(flow.kind).unwrap().add(&flow.amount);
            let signed = if flow.kind == "receivables" {
                flow.amount.clone()
            } else {
                Money::default().subtract(&flow.amount)
            };
            sources.push(json!({"source":flow.kind,"amount":wire(&signed)?,"reference":flow.reference,"assumption":flow.assumption,"settlementStatus":if flow.settlement_unknown{json!("unknown")}else{Value::Null}}));
        }
        for (id, month, account, amount) in &budgets {
            if month.as_str() >= &from[..7] && month.as_str() <= &to[..7] && seen_budget.insert(*id)
            {
                sums.get_mut("undatedBudgetAssumptions")
                    .unwrap()
                    .add(&Money::default().subtract(amount));
                sources.push(json!({"source":"undated_budget","amount":wire(&Money::default().subtract(amount))?,"reference":format!("budget-revision:{id}:account:{account}"),"assumption":true}));
            }
        }
        let movement = sums["receivables"]
            .subtract(&sums["payables"])
            .subtract(&sums["commitments"])
            .subtract(&sums["obligations"]);
        if let Some(c) = &mut cash {
            c.add(&movement);
            if lowest.as_ref().is_some_and(|l| c.subtract(l).is_negative()) {
                lowest = Some(c.clone());
            }
        }
        if let Some(c) = &mut scenario {
            c.add(&movement.subtract(&sums["scenarios"]));
        }
        let mut week = json!({"weekStart":from,"openingCash":opening_week.as_ref().map(wire).transpose()?,"closingCash":cash.as_ref().map(wire).transpose()?,"scenarioClosingCash":scenario.as_ref().map(wire).transpose()?,"sources":sources,"excluded":omitted});
        if monthly {
            week["periodStart"] = json!(from);
            week["periodEnd"] = json!(to);
            week.as_object_mut().unwrap().remove("weekStart");
        }
        for (k, v) in sums {
            week[k] = wire(&v)?;
        }
        periods.push(week);
    }
    Ok(
        json!({"ok":true,"startDate":start,"currency":currency,"openingCash":opening.as_ref().map(wire).transpose()?,"openingCashBasis":if opening.is_some(){"confirmed_bank_ledger"}else{"unavailable"},"lowestPoint":lowest.as_ref().map(wire).transpose()?,"periods":periods,"completeness":{"status":"incomplete","included":included,"excluded":excluded},"errors":[],"appliedRules":[if monthly {"native-liquidity-3-calendar-month-v1"}else{"native-liquidity-13-week-v1"}]}),
    )
}

pub(super) fn intervals(start: &str, monthly: bool) -> Result<Vec<(String, String)>, Error> {
    let date = rentemester_engine::vat_period::date(start)?;
    if !monthly {
        return (0..13)
            .map(|index| {
                let from = add_days(start, index * 7)?;
                Ok((from.clone(), add_days(&from, 6)?))
            })
            .collect();
    }
    if date.day() != 1 {
        return Err(refused(
            "calendar forecast must start on first day of month",
        ));
    }
    let mut year = date.year();
    let mut month = date.month() as u8;
    let mut out = vec![];
    for _ in 0..3 {
        let from = format!("{year:04}-{month:02}-01");
        if month == 12 {
            year += 1;
            month = 1;
        } else {
            month += 1;
        }
        let next = format!("{year:04}-{month:02}-01");
        let to = add_days(&next, -1)?;
        out.push((from, to));
    }
    Ok(out)
}

#[cfg(test)]
mod calendar_tests {
    use super::*;
    #[test]
    fn calendar_intervals_include_day92_and_leap_february_without_changing_weekly_mode() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Calendar Forecast")
            .unwrap();
        db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('B','Synthetic bank','asset','debit'); INSERT INTO bank_accounts(slug,name,ledger_account_no,currency) VALUES('synthetic','Synthetic bank','B','DKK');").unwrap();
        let tx = db.transaction().unwrap();
        let occurrences = vec![
            json!({"commitmentId":"synthetic-end","date":"2026-09-30","amount":7.03,"currency":"DKK"}),
        ];
        let monthly = build_horizon(&tx, "synthetic", "2026-07-01", &occurrences, true).unwrap();
        assert_eq!(monthly["periods"].as_array().unwrap().len(), 3);
        assert_eq!(monthly["periods"][2]["periodEnd"], "2026-09-30");
        assert_eq!(monthly["periods"][2]["closingCash"], -7.03);
        assert_eq!(monthly["lowestPoint"], -7.03);
        let weekly = build(&tx, "synthetic", "2026-07-01", &occurrences).unwrap();
        assert_eq!(weekly["periods"].as_array().unwrap().len(), 13);
        assert_eq!(weekly["periods"][12]["closingCash"], 0.0);
        assert_eq!(
            intervals("2028-01-01", true).unwrap()[1],
            ("2028-02-01".into(), "2028-02-29".into())
        );
        assert!(intervals("2026-07-02", true).is_err());
    }
}
