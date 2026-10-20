//! VAT display from current owned company state. No file opens or business writes.
use crate::{cockpit::number, invalid, legacy_money};
use rentemester_engine::{
    Error,
    money::Money,
    vat_period,
    vat_report::{self, Line},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn status(tx: &Transaction<'_>, start: &str, end: &str) -> Result<String, Error> {
    let row:Option<(i64,String)>=tx.query_row("SELECT id,status FROM accounting_periods WHERE kind IN ('vat_period','vat_quarter') AND period_start=?1 AND period_end=?2 ORDER BY CASE kind WHEN 'vat_period' THEN 0 ELSE 1 END,id DESC LIMIT 1",params![start,end],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(invalid)?;
    let Some((id, mut state)) = row else {
        return Ok("open".into());
    };
    let mut query=tx.prepare("SELECT event_type FROM audit_log WHERE entity_type='accounting_period' AND entity_id=?1 ORDER BY id").map_err(invalid)?;
    for event in query
        .query_map([id.to_string()], |r| r.get::<_, String>(0))
        .map_err(invalid)?
    {
        let event = event.map_err(invalid)?;
        if state != "reported" {
            match event.as_str() {
                "period_reopen" => state = "open".into(),
                "period_close" => state = "closed".into(),
                "period_report" => state = "reported".into(),
                _ => {}
            }
        }
    }
    if !["open", "closed", "reported"].contains(&state.as_str()) {
        return Err(invalid("unknown VAT period state"));
    }
    Ok(state)
}
pub(crate) fn lines(tx: &Transaction<'_>) -> Result<Vec<(String, Line)>, Error> {
    let mut roles = BTreeMap::new();
    let mut query = tx
        .prepare("SELECT role,account_no FROM account_role_mappings WHERE status='confirmed'")
        .map_err(invalid)?;
    for row in query
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(invalid)?
    {
        let (role, account) = row.map_err(invalid)?;
        roles.insert(role, account);
    }
    let mut query=tx.prepare("SELECT je.id,je.transaction_date,je.status,je.currency,je.created_by_program,a.account_no,a.type,a.normal_balance,jl.vat_code,jl.debit_amount,jl.credit_amount FROM journal_entries je JOIN journal_lines jl ON jl.journal_entry_id=je.id JOIN accounts a ON a.id=jl.account_id ORDER BY je.id,jl.id").map_err(invalid)?;
    let mut rows = query.query([]).map_err(invalid)?;
    let mut result = Vec::new();
    while let Some(r) = rows.next().map_err(invalid)? {
        let state: String = r.get(2).map_err(invalid)?;
        if !["posted", "reversed"].contains(&state.as_str()) {
            return Err(invalid("unsupported journal state in VAT report"));
        }
        let no: String = r.get(5).map_err(invalid)?;
        let kind: String = r.get(6).map_err(invalid)?;
        let normal: String = r.get(7).map_err(invalid)?;
        let is_role = |role: &str| roles.get(role) == Some(&no);
        let input = is_role("input_vat");
        let output = is_role("output_vat") || is_role("reverse_charge_vat");
        if input && output {
            return Err(invalid("conflicting VAT account roles"));
        }
        let amount_side = if input {
            Some("input")
        } else if output {
            Some("output")
        } else if kind == "vat" {
            Some(if normal == "debit" { "input" } else { "output" })
        } else {
            None
        };
        let program: String = r.get(4).map_err(invalid)?;
        // Import provenance/evidence rules are a separate migration flow. No guessed bases.
        let historical = program == "rentemester-import-postings";
        result.push((
            r.get(1).map_err(invalid)?,
            Line {
                entry_id: r.get(0).map_err(invalid)?,
                account_no: no.clone(),
                account_type: kind,
                vat_code: r.get(8).map_err(invalid)?,
                debit: legacy_money(r.get_ref(9).map_err(invalid)?)?,
                credit: legacy_money(r.get_ref(10).map_err(invalid)?)?,
                amount_side,
                dedicated_reverse_charge: is_role("reverse_charge_vat")
                    && roles.get("reverse_charge_vat") != roles.get("output_vat"),
                settlement: is_role("vat_settlement"),
                foreign: r.get::<_, String>(3).map_err(invalid)? != "DKK",
                historical,
            },
        ));
    }
    Ok(result)
}
fn numeric(value: &mut Value) -> Result<(), Error> {
    if let Some(values) = value.as_object_mut() {
        for (key, value) in values {
            if key == "errors" || key == "warnings" {
                continue;
            }
            numeric(value)?;
        }
    } else if let Some(s) = value.as_str() {
        *value = number(&Money::parse_canonical(s)?)?;
    }
    Ok(())
}
pub(crate) fn range(tx: &Transaction<'_>, start: &str, end: &str) -> Result<Value, Error> {
    let chosen = lines(tx)?
        .into_iter()
        .filter(|(date, _)| date.as_str() >= start && date.as_str() <= end)
        .map(|(_, line)| line)
        .collect::<Vec<_>>();
    let mut report = serde_json::to_value(vat_report::build(&chosen)?).map_err(invalid)?;
    numeric(&mut report)?;
    Ok(report)
}
pub fn read(
    tx: &Transaction<'_>,
    slug: &str,
    today: &str,
    year: Option<&str>,
) -> Result<Value, Error> {
    // Reuse the report's fiscal context and authorization-owned read snapshot.
    let snapshot = crate::cockpit::read(tx, slug, today, year, None)?;
    let mut base = snapshot.fiscal_years;
    base["fiscalYears"] = base["years"].take();
    base["company"] = snapshot.trial_balance["company"].clone();
    base["slug"] = json!(slug);
    base["selectedYear"] = snapshot.trial_balance["selectedYear"].clone();
    base["archived"] = json!(false);
    let cadence: Option<String> = tx
        .query_row(
            "SELECT vat_period_type FROM companies WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let Some(cadence) = cadence else {
        base["vatRegistered"] = json!(false);
        base["periodLabel"] = json!("Ikke momsregistreret");
        return Ok(base);
    };
    // The existing VAT view selects calendar-year obligations, even for shifted fiscal years.
    let label = base["selectedYear"]
        .as_str()
        .ok_or_else(|| invalid("missing year"))?;
    let year: i32 = label
        .get(..4)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| invalid("invalid selected VAT year"))?;
    let span = vat_period::span(&cadence)?;
    let lines = lines(tx)?;
    let mut windows = Vec::new();
    for month in (1..=12).step_by(span as usize) {
        let window = vat_period::window(&format!("{year:04}-{month:02}-01"), &cadence)?;
        let active = lines
            .iter()
            .any(|(date, _)| *date >= window.start && *date <= window.end);
        let state = status(tx, &window.start, &window.end)?;
        windows.push((window, active, state));
    }
    let current_year = vat_period::date(today)?.year();
    let current = vat_period::window(today, &cadence)?;
    let current_index = windows
        .iter()
        .position(|(w, _, _)| w.start == current.start)
        .unwrap_or(windows.len() - 1);
    let limit = if year == current_year {
        current_index
    } else {
        windows.len() - 1
    };
    let selected = windows
        .iter()
        .enumerate()
        .find(|(i, (_, active, state))| *i <= limit && *active && state != "reported")
        .map(|(i, _)| i)
        .unwrap_or_else(|| {
            if year == current_year {
                current_index
            } else {
                windows
                    .iter()
                    .rposition(|(_, active, _)| *active)
                    .unwrap_or(0)
            }
        });
    let (window, _, state) = &windows[selected];
    let selected_lines: Vec<_> = lines
        .into_iter()
        .filter(|(date, _)| *date >= window.start && *date <= window.end)
        .map(|(_, line)| line)
        .collect();
    let report = vat_report::build(&selected_lines)?;
    let ready = report.errors.is_empty() && ["closed", "reported"].contains(&state.as_str());
    let mut report = serde_json::to_value(report).map_err(invalid)?;
    numeric(&mut report)?;
    base.as_object_mut()
        .ok_or_else(|| invalid("report base"))?
        .extend(report.as_object().unwrap().clone());
    base["vatRegistered"] = json!(true);
    base["periodStart"] = json!(window.start);
    base["periodEnd"] = json!(window.end);
    base["periodLabel"] = json!(window.label);
    base["deadline"] = json!(window.deadline);
    base["daysRemaining"] =
        json!((vat_period::date(&window.deadline)? - vat_period::date(today)?).whole_days());
    base["periodStatus"] = json!(state);
    base["momsangivelseReady"] = json!(ready);
    base["periodActionsAvailable"] = json!(true);
    base["vatReportErrors"] = base["errors"].take();
    base["vatReportWarnings"] = base["warnings"].take();
    Ok(base)
}
