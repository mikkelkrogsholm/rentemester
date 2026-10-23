//! One admitted, current-authorized company card. No connection opens, discovery or writes.
use crate::invalid;
use rentemester_engine::{
    Error, fiscal_year::Date, money::Money, sales_invoice::wire, trial_balance::Period, vat_period,
};
use rusqlite::Transaction;
use serde_json::{Value, json};
mod invoices;
#[cfg(test)]
mod tests;
fn availability(status: &str, reason: Option<&str>) -> Value {
    json!({"status":status,"reason":reason})
}
pub fn company(tx: &Transaction<'_>, slug: &str, as_of: &str) -> Result<Value, Error> {
    Date::parse(as_of)?;
    if slug.is_empty() || slug.len() > 128 || slug.chars().any(char::is_control) {
        return Err(invalid("company slug required"));
    }
    crate::journal_preview::require_current(tx)?;
    crate::migration::integrity(tx)?;
    let audit = crate::journal_hash::verify(tx)?;
    if !audit.ok {
        return Err(invalid("portfolio journal integrity differs"));
    }
    let (name, cvr, currency, cadence): (String, Option<String>, String, Option<String>) = tx
        .query_row(
            "SELECT name,cvr,currency,vat_period_type FROM companies WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(invalid)?;
    if currency != "DKK" {
        return Err(invalid(
            "portfolio requires verified DKK functional currency",
        ));
    }
    let years = crate::cockpit::live_years(tx, as_of)?;
    let year = years
        .as_array()
        .ok_or_else(|| invalid("native fiscal inventory unavailable"))?
        .iter()
        .find(|year| {
            year["start"].as_str().is_some_and(|start| start <= as_of)
                && year["end"].as_str().is_some_and(|end| end >= as_of)
        })
        .ok_or_else(|| invalid("native fiscal period containing cutoff required"))?;
    let fiscal_start = year["start"]
        .as_str()
        .ok_or_else(|| invalid("fiscal start required"))?;
    // Tampered opening proof is an error, never a misleading unavailable/empty card.
    let opening = crate::opening_balance::validate_history(tx)?;
    let mut start = fiscal_start.to_owned();
    let basis = crate::opening_balance::require_report_as_of(tx, as_of);
    let (revenue, result, financial_availability) = match basis {
        Ok(_) => {
            if let Some(opening) = opening.as_ref() {
                let cut = opening.cut_over_date().to_string();
                if cut.as_str() > start.as_str() {
                    start = cut;
                }
            }
            let trial = crate::trial_balance_owned(tx, Period::new(&start, as_of)?)?;
            let pnl = rentemester_engine::financial_statements::profit_and_loss(&trial);
            (
                wire(&pnl.income.total)?,
                wire(&pnl.result)?,
                if start != fiscal_start {
                    availability("partial", Some("native_history_begins_at_explicit_cutover"))
                } else {
                    availability("complete", None)
                },
            )
        }
        Err(error) if error.code == "archive_report_unsupported" => (
            Value::Null,
            Value::Null,
            availability("unavailable", Some("archive_or_cutover_basis_unavailable")),
        ),
        Err(error) => return Err(error),
    };
    let bank = crate::bank::statement_balance(tx, as_of)?;
    let mut card = json!({"slug":slug,"name":name,"cvr":cvr,"asOf":as_of,"currency":currency,"archived":false,"ledgerMissing":false,"fiscalYear":year["label"],"fiscalPeriodStart":fiscal_start,"fiscalPeriodEnd":year["end"],"periodStart":start,"periodEnd":as_of,"resultat":result,"omsaetning":revenue,"actualBankBalance":bank.balance.as_ref().map(wire).transpose()?,"bankStatementStatus":bank.status,"bankStatementProblem":bank.problem,"auditChainOk":audit.ok,"audit":{"status":"verified","scope":"current_journal_hash_chain","entriesChecked":audit.entries_checked},"availability":{"financials":financial_availability,"bank":availability(if bank.balance.is_some(){"complete"}else{"unavailable"},if bank.balance.is_some(){None}else{Some(bank.status)})},"limitations":["Opgaver viser nuværende åben status for kilder frem til skæringsdatoen; historisk opgavestatus er ikke rekonstrueret.","Auditkontrollen omfatter journalens hashkæde; fysisk bilagskontrol er en særskilt kontrol."]});
    let financial_unavailable = card["availability"]["financials"]["status"] == "unavailable";
    if financial_unavailable {
        card["vat"] = Value::Null;
        card["netVatPayable"] = Value::Null;
        card["availability"]["vat"] =
            availability("unavailable", Some("archive_or_cutover_basis_unavailable"));
    } else if let Some(cadence) = cadence {
        let window = vat_period::window(as_of, &cadence)?;
        let report = crate::vat_report::range_report(tx, &window.start, as_of)?;
        card["vat"] = json!({"payable":wire(&report.payable)?,"deadline":window.deadline,"daysRemaining":(vat_period::date(&window.deadline)?-vat_period::date(as_of)?).whole_days(),"periodStart":window.start,"periodEnd":window.end,"cutoff":as_of,"errors":report.errors,"warnings":report.warnings});
        card["netVatPayable"] = wire(&report.payable)?;
        let partial_cutover = opening
            .as_ref()
            .is_some_and(|value| value.cut_over_date().to_string() > window.start);
        card["availability"]["vat"] = if partial_cutover {
            availability("partial", Some("native_history_begins_at_explicit_cutover"))
        } else if report.errors.is_empty() {
            availability("complete", None)
        } else {
            availability("partial", Some("vat_reconciliation_errors"))
        };
    } else {
        card["vat"] = Value::Null;
        card["netVatPayable"] = json!(0);
        card["availability"]["vat"] = availability("not_registered", None);
    }
    card["moms"] = card["vat"].clone();
    let invoices = if financial_unavailable {
        json!({"openInvoiceCount":null,"openInvoiceTotal":null,"overdueInvoiceCount":null})
    } else {
        invoices::collect(tx, as_of)?
    };
    for (key, value) in invoices
        .as_object()
        .ok_or_else(|| invalid("invoice projection"))?
    {
        card[key] = value.clone();
    }
    let imported_receivables: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM imported_receivable_headers)",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    card["availability"]["invoices"] = if card["openInvoiceTotal"].is_null() {
        availability("unavailable", Some("unvalued_or_adjusted_receivable"))
    } else if imported_receivables {
        card["limitations"].as_array_mut().ok_or_else(|| invalid("portfolio limitations"))?.push(json!("Fakturatallene omfatter kun fakturaer udstedt i Rentemester; importerede tilgodehavender vises i deres separate oversigt."));
        availability(
            "partial",
            Some("native_invoices_only_imported_receivables_separate"),
        )
    } else {
        availability("complete", None)
    };
    let unlinked:i64=tx.query_row("SELECT count(*) FROM bank_transactions b WHERE b.transaction_date<=?1 AND NOT EXISTS(SELECT 1 FROM bank_journal_reconciliations r WHERE r.bank_transaction_id=b.id)",[as_of],|r|r.get(0)).map_err(invalid)?;
    card["unlinkedBankCount"] = json!(unlinked);
    // Status is explicitly current, dates only delimit the source population.
    let mut query=tx.prepare("SELECT e.type,e.severity,count(*) FROM exceptions e LEFT JOIN bank_transactions b ON b.id=e.related_bank_transaction_id LEFT JOIN documents d ON d.id=e.related_document_id WHERE e.status='open' AND substr(e.created_at,1,10)<=?1 AND (COALESCE(b.transaction_date,d.invoice_date) IS NULL OR COALESCE(b.transaction_date,d.invoice_date)<=?1) GROUP BY e.type,e.severity ORDER BY e.type,e.severity").map_err(invalid)?;
    let groups=query.query_map([as_of],|r|Ok(json!({"type":r.get::<_,String>(0)?,"severity":r.get::<_,String>(1)?,"count":r.get::<_,i64>(2)?,"label":"Åbne bilags- og bogføringsopgaver","link":format!("/companies/{slug}/exceptions")}))).map_err(invalid)?.collect::<Result<Vec<_>,_>>().map_err(invalid)?;
    let count = groups.iter().try_fold(0i64, |sum, g| {
        sum.checked_add(g["count"].as_i64().ok_or_else(|| invalid("task count"))?)
            .ok_or_else(|| invalid("task count overflow"))
    })?;
    card["openTaskCount"] = json!(count);
    card["openExceptionCount"] = json!(count);
    card["taskGroups"] = json!(groups);
    card["openTasks"] = card["taskGroups"].clone();
    card["availability"]["tasks"] = availability(
        "current_state",
        Some("current_open_state_for_sources_through_cutoff"),
    );
    Ok(card)
}

/// Canonical native invoice projection with explicit payment/credit/refund cutoff.
pub(crate) fn dated_invoices(tx: &Transaction<'_>, as_of: &str) -> Result<Value, Error> {
    invoices::snapshot(tx, as_of)
}
