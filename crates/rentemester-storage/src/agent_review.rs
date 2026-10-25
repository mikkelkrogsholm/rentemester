//! Transaction-local, review-only producers for the original explicit agent run.
//! The caller owns admission, authorization, commit and rollback. `actor` is the
//! already authenticated canonical audit identity, not an agent authority.
use crate::invalid;
use rentemester_engine::{
    Error,
    agent_run::{
        DEADLINE_HORIZON_DAYS, DeadlineNotice, PROGRAM, Proposal, RULE, ReviewSnapshot,
        RoutedException,
    },
    fiscal_year::{self, Date, LabelStrategy, StartMonth},
    vat_period,
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};

fn audit(
    tx: &Transaction<'_>,
    id: i64,
    event: &str,
    actor: &str,
    evidence: &Value,
) -> Result<(), Error> {
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'exception',?2,?3,?4)",params![event,id.to_string(),json!({"program":PROGRAM,"rule":RULE,"evidence":evidence}).to_string(),actor]).map_err(invalid)?;
    Ok(())
}
fn admitted(tx: &Transaction<'_>, actor: &str) -> Result<(), Error> {
    crate::journal_post::require_current(tx)?;
    if actor.trim().is_empty() {
        return Err(invalid("authenticated actor required"));
    }
    Ok(())
}
fn identity(kind: &str, ev: &Value) -> Option<Value> {
    match kind {
        "AGENT_PAYABLE_OVERDUE" if ev["payableId"].is_i64() => Some(json!([ev["payableId"]])),
        "AGENT_ACCRUAL_RECOGNITION_DUE"
            if ev["accrualId"].is_i64() && ev["periodIndex"].is_i64() =>
        {
            Some(json!([ev["accrualId"], ev["periodIndex"]]))
        }
        "AGENT_TAX_RETURN_NEEDS_REVIEW"
            if ev["fiscalYearStart"].is_string()
                && ev["fiscalYearEnd"].is_string()
                && ev["needsReviewKind"].is_string() =>
        {
            Some(json!([
                ev["fiscalYearStart"],
                ev["fiscalYearEnd"],
                ev["needsReviewKind"]
            ]))
        }
        _ => None,
    }
}
fn open_evidence(tx: &Transaction<'_>, kind: &str) -> Result<Vec<(i64, Value)>, Error> {
    let mut q = tx
        .prepare(
            "SELECT id,source_evidence FROM exceptions WHERE status='open' AND type=?1 ORDER BY id",
        )
        .map_err(invalid)?;
    q.query_map([kind], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
    })
    .map_err(invalid)?
    .map(|r| {
        let (id, ev) = r.map_err(invalid)?;
        Ok((
            id,
            ev.and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(Value::Null),
        ))
    })
    .collect()
}
/// Append a review proposal, deduplicating open source identities. No financial effect.
pub fn record_proposal(
    tx: &Transaction<'_>,
    proposal: &Proposal,
    actor: &str,
) -> Result<i64, Error> {
    admitted(tx, actor)?;
    if !proposal.kind.starts_with("AGENT_")
        || proposal.message.trim().is_empty()
        || !["low", "medium", "high"].contains(&proposal.severity.as_str())
    {
        return Err(invalid("invalid agent proposal"));
    }
    if let Some(key) = identity(&proposal.kind, &proposal.source_evidence) {
        for (id, ev) in open_evidence(tx, &proposal.kind)? {
            if identity(&proposal.kind, &ev) == Some(key.clone()) {
                return Ok(id);
            }
        }
    } else {
        let existing:Option<i64>=tx.query_row("SELECT id FROM exceptions WHERE status='open' AND type=?1 AND COALESCE(related_bank_transaction_id,0)=COALESCE(?2,0) AND COALESCE(related_document_id,0)=COALESCE(?3,0) AND message=?4 AND COALESCE(required_action,'')=COALESCE(?5,'') ORDER BY id LIMIT 1",params![proposal.kind,proposal.related_bank_transaction_id,proposal.related_document_id,proposal.message.trim(),proposal.required_action.as_deref().map(str::trim)],|r|r.get(0)).optional().map_err(invalid)?;
        if let Some(id) = existing {
            return Ok(id);
        }
    }
    tx.execute("INSERT INTO exceptions(type,severity,status,related_bank_transaction_id,related_document_id,message,required_action,source_evidence,posting_preview) VALUES(?1,?2,'open',?3,?4,?5,?6,?7,?8)",params![proposal.kind,proposal.severity,proposal.related_bank_transaction_id,proposal.related_document_id,proposal.message.trim(),proposal.required_action.as_deref().map(str::trim),proposal.source_evidence.to_string(),proposal.posting_preview.as_ref().map(Value::to_string)]).map_err(invalid)?;
    let id = tx.last_insert_rowid();
    audit(tx, id, "exception_record", actor, &proposal.source_evidence)?;
    Ok(id)
}
fn stale(
    tx: &Transaction<'_>,
    kind: &str,
    proposals: &[Proposal],
    actor: &str,
    note: &str,
    year: Option<(&str, &str)>,
) -> Result<(), Error> {
    for (id, ev) in open_evidence(tx, kind)? {
        if let Some((start, end)) = year {
            if ev["fiscalYearStart"] != start || ev["fiscalYearEnd"] != end {
                continue;
            }
        }
        if let Some(key) = identity(kind, &ev) {
            if !proposals
                .iter()
                .any(|p| identity(kind, &p.source_evidence) == Some(key.clone()))
            {
                tx.execute("UPDATE exceptions SET status='resolved',resolved_at=CURRENT_TIMESTAMP,resolved_by=?1,resolution_note=?2 WHERE id=?3 AND status='open'",params![actor,note,id]).map_err(invalid)?;
                audit(
                    tx,
                    id,
                    "exception_resolve",
                    actor,
                    &json!({"note":note,"effect":"review_only_no_posting"}),
                )?;
            }
        }
    }
    for p in proposals {
        record_proposal(tx, p, actor)?;
    }
    Ok(())
}
fn proposal(
    kind: &str,
    severity: &str,
    message: String,
    action: String,
    evidence: Value,
) -> Proposal {
    Proposal {
        kind: kind.into(),
        severity: severity.into(),
        message,
        required_action: Some(action),
        related_bank_transaction_id: None,
        related_document_id: None,
        source_evidence: evidence,
        posting_preview: None,
    }
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, Error> {
    v[key]
        .as_str()
        .ok_or_else(|| invalid(format!("missing {key}")))
}
fn integer(v: &Value, key: &str) -> Result<i64, Error> {
    v[key]
        .as_i64()
        .ok_or_else(|| invalid(format!("missing {key}")))
}
fn rows<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, Error> {
    v[key]
        .as_array()
        .ok_or_else(|| invalid(format!("missing {key}")))
}
fn kroner(v: &Value) -> Result<String, Error> {
    let m = crate::invoice_fx::amount(v)?.decimal();
    let (whole, fraction) = m.split_once('.').ok_or_else(|| invalid("invalid money"))?;
    let negative = whole.starts_with('-');
    let grouped = whole
        .trim_start_matches('-')
        .chars()
        .rev()
        .collect::<Vec<_>>()
        .chunks(3)
        .map(|c| c.iter().rev().collect::<String>())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(".");
    Ok(format!(
        "{}{grouped},{fraction} kr.",
        if negative { "-" } else { "" }
    ))
}
fn days(from: &str, to: &str) -> Result<i64, Error> {
    Ok((vat_period::date(to)? - vat_period::date(from)?).whole_days())
}
fn previous(date: &str) -> Result<String, Error> {
    Ok(vat_period::date(date)?
        .previous_day()
        .ok_or_else(|| invalid("date underflow"))?
        .to_string())
}

pub fn sync_overdue_payables(tx: &Transaction<'_>, as_of: &str, actor: &str) -> Result<(), Error> {
    admitted(tx, actor)?;
    Date::parse(as_of)?;
    let list = crate::payables::dated_rows(tx, as_of, "overdue")?;
    let mut proposals = vec![];
    for row in rows(&list, "rows")? {
        let id = integer(row, "payableId")?;
        let bill = row["billNo"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| format!("kreditorpost {s}"))
            .unwrap_or_else(|| format!("kreditorpost #{id}"));
        let mut p = proposal(
            "AGENT_PAYABLE_OVERDUE",
            if integer(row, "overdueDays")? >= 30 {
                "high"
            } else {
                "medium"
            },
            format!(
                "{bill} til {} på {} med forfald {} er overforfalden og endnu ikke betalt.",
                row["supplierName"].as_str().unwrap_or("ukendt leverandør"),
                kroner(&row["openBalance"])?,
                text(row, "dueDate")?
            ),
            "Betal kreditorposten, og afstem den udgående bankbetaling mod den med 'payable pay'."
                .into(),
            json!({"rule":"DK-PAYABLE-001","payableId":id,"dueDate":row["dueDate"],"overdueDays":row["overdueDays"],"openBalance":row["openBalance"],"agingBucket":row["agingBucket"],"asOfDate":as_of}),
        );
        p.related_document_id = Some(integer(row, "documentId")?);
        proposals.push(p);
    }
    stale(
        tx,
        "AGENT_PAYABLE_OVERDUE",
        &proposals,
        actor,
        "Erstattet automatisk: kreditorposten er ikke længere overforfalden (betalt eller udlignet)",
        None,
    )
}
/// Uses the shared canonical unmatched-bank producer (which retains its own
/// system provenance); appends the initiating authenticated actor separately.
pub fn reconcile(tx: &Transaction<'_>, actor: &str) -> Result<(), Error> {
    admitted(tx, actor)?;
    let created = crate::unmatched_bank::sync(tx)?;
    if created > 0 {
        audit(
            tx,
            0,
            "agent_reconcile",
            actor,
            &json!({"created":created,"effect":"review_only_no_posting"}),
        )?;
    }
    Ok(())
}

fn vat_notice(
    tx: &Transaction<'_>,
    as_of: &str,
    cadence: &str,
    w: vat_period::Window,
    actor: &str,
) -> Result<DeadlineNotice, Error> {
    let remaining = days(as_of, &w.deadline)?;
    let filing = crate::vat_original::filing(tx, &w.start, &w.end)?;
    let report = crate::vat_original::report(tx, &w.start, &w.end)?;
    let closed = matches!(filing["periodStatus"].as_str(), Some("closed" | "reported"));
    let ready = closed && filing["ok"] == true;
    let errors = rows(&filing, "errors")?
        .iter()
        .map(|v| v.as_str().unwrap_or("invalid VAT diagnostic"))
        .collect::<Vec<_>>();
    let note = if ready {
        format!(
            "Momsperioden er lukket — momsangivelse klar (momstilsvar {}).",
            kroner(&report["netVatPayable"])?
        )
    } else if closed {
        format!(
            "Momsperioden er lukket, men momsangivelsen er blokeret: {}",
            errors
                .first()
                .copied()
                .unwrap_or("momsrapporten kan ikke afstemmes")
        )
    } else {
        "Momsperioden er endnu ikke lukket — luk den med 'period close' før momsangivelse.".into()
    };
    if !ready && remaining <= DEADLINE_HORIZON_DAYS {
        let word = match cadence {
            "month" => "Momsmåneden",
            "half-year" => "Momshalvåret",
            _ => "Momskvartalet",
        };
        let message = if closed {
            format!(
                "{word} {}..{} er lukket, men momsangivelsen kan ikke indberettes: {}. Frist: {}.",
                w.start,
                w.end,
                errors.join("; "),
                w.deadline
            )
        } else {
            format!(
                "{word} {}..{} er endnu ikke lukket, og momsangivelsen {}",
                w.start,
                w.end,
                if remaining < 0 {
                    format!(
                        "skulle have været indberettet {} (fristen er overskredet med {} dage pr. {as_of}).",
                        w.deadline,
                        remaining.abs()
                    )
                } else {
                    format!(
                        "skal indberettes senest {} ({remaining} dage fra {as_of}).",
                        w.deadline
                    )
                }
            )
        };
        record_proposal(tx,&proposal(if closed {"AGENT_VAT_FILING_BLOCKED"} else {"AGENT_VAT_DEADLINE_OPEN"},if remaining<0 {"high"} else {"medium"},message,if closed {"Ret momsgrundlag/-konti, så 'vat momsangivelse' afstemmer uden fejl, og indberet derefter via TastSelv."} else {"Luk momsperioden med 'period close --kind vat_period', og indberet derefter momsangivelsen."}.into(),json!({"rule":RULE,"dueDate":w.deadline,"daysRemaining":remaining,"periodStatus":filing["periodStatus"],"filingErrors":filing["errors"]})),actor)?;
    }
    Ok(DeadlineNotice {
        kind: "vat_period".into(),
        period_start: w.start,
        period_end: w.end,
        due_date: w.deadline,
        days_remaining: remaining,
        ready,
        note,
    })
}

pub fn review_deadlines(
    tx: &Transaction<'_>,
    as_of: &str,
    actor: &str,
) -> Result<ReviewSnapshot, Error> {
    admitted(tx, actor)?;
    let date = Date::parse(as_of)?;
    let (cadence,month,strategy):(Option<String>,i32,String)=tx.query_row("SELECT vat_period_type,fiscal_year_start_month,fiscal_year_label_strategy FROM companies WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(invalid)?;
    let mut notices = vec![];
    if let Some(cadence) = cadence {
        let current = vat_period::window(as_of, &cadence)?;
        let prev = vat_period::window(&previous(&current.start)?, &cadence)?;
        if prev.deadline.as_str() >= as_of {
            notices.push(vat_notice(tx, as_of, &cadence, prev, actor)?);
        }
        notices.push(vat_notice(tx, as_of, &cadence, current, actor)?);
    }
    let month = StartMonth::new(month)?;
    let strategy = LabelStrategy::try_from(strategy.as_str())?;
    let fy = fiscal_year::for_date(date, month, strategy)?;
    let due = fiscal_year::annual_report_deadline(fy.end)?.to_string();
    let remaining = days(as_of, &due)?;
    notices.push(DeadlineNotice {kind:"fiscal_year".into(),period_start:fy.start.to_string(),period_end:fy.end.to_string(),due_date:due.clone(),days_remaining:remaining,ready:false,note:if remaining<=DEADLINE_HORIZON_DAYS {format!("Årsrapport for regnskabsår {} nærmer sig (frist {due}, ÅRL § 138) — forbered med 'report annual'.",fy.display_label)} else {format!("Regnskabsår {} løber — årsrapport forfalder {due} (ÅRL § 138).",fy.display_label)} });
    let accruals = crate::accruals::report(tx)?;
    let mut proposals = vec![];
    for a in rows(&accruals, "accruals")? {
        let id = integer(a, "accrualId")?;
        let preview = crate::accruals::preview(tx, id)?;
        for p in rows(&preview, "schedule")? {
            let recognition = text(p, "recognitionDate")?;
            if p["recognized"] == true || recognition > as_of {
                continue;
            }
            let index = integer(p, "periodIndex")?;
            let overdue = days(recognition, as_of)?;
            proposals.push(proposal("AGENT_ACCRUAL_RECOGNITION_DUE",if overdue>=30 {"high"} else {"medium"},format!("Periodeafgrænsningspost \"{}\" — periode {index}/{} ({}) med planlagt bogføringsdato {recognition} er forfalden og endnu ikke bogført.",text(a,"description")?,integer(a,"recognitionPeriods")?,kroner(&p["amount"])?),format!("Bogfør periode {index} af periodeafgrænsningsposten med 'accrual recognize'."),json!({"rule":"DK-BOOKKEEPING-ACCRUAL-001","accrualId":id,"accrualType":a["accrualType"],"periodIndex":index,"recognitionDate":recognition,"amount":p["amount"],"overdueDays":overdue,"asOfDate":as_of})));
        }
    }
    stale(
        tx,
        "AGENT_ACCRUAL_RECOGNITION_DUE",
        &proposals,
        actor,
        "Erstattet automatisk: periodeafgrænsnings-perioden er bogført",
        None,
    )?;
    let prev = fiscal_year::for_date(
        Date::parse(&previous(&fy.start.to_string())?)?,
        month,
        strategy,
    )?;
    let (start, end) = (prev.start.to_string(), prev.end.to_string());
    let period:Option<(i64,String)>=tx.query_row("SELECT id,status FROM accounting_periods WHERE kind='fiscal_year' AND period_start=?1 AND period_end=?2 ORDER BY id DESC LIMIT 1",params![start,end],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(invalid)?;
    if let Some((id, status)) = period {
        if matches!(
            crate::periods::effective(tx, id, status)?.as_str(),
            "closed" | "reported"
        ) {
            // Source no-ops when canonical annual tax prerequisites are unavailable.
            if let Ok(tax) = crate::annual_exports::read(tx, "", &start, &end, "tax") {
                if tax["ok"] == true {
                    let mut tax_proposals = vec![];
                    for item in rows(&tax, "needsReview")? {
                        tax_proposals.push(proposal("AGENT_TAX_RETURN_NEEDS_REVIEW","medium",format!("Oplysningsskema for regnskabsår {start}..{end}: {}. Dette tal beregner Rentemester ikke deterministisk og skal afklares før indberetning.",text(item,"label")?),text(item,"requiredAction")?.into(),json!({"rule":"DK-TAX-RETURN-CORP-001","fiscalYearStart":start,"fiscalYearEnd":end,"needsReviewKind":item["kind"]})));
                    }
                    stale(
                        tx,
                        "AGENT_TAX_RETURN_NEEDS_REVIEW",
                        &tax_proposals,
                        actor,
                        "Erstattet automatisk: needs-review-punktet udløser ikke længere for regnskabsåret",
                        Some((&start, &end)),
                    )?;
                }
            }
        }
    }
    Ok(ReviewSnapshot {
        accrual_recognitions_due: proposals.len() as u64,
        upcoming_deadlines: notices,
    })
}
/// The original report includes *all* open types, including archived subjects.
pub fn open_report_exceptions(tx: &Transaction<'_>) -> Result<Vec<RoutedException>, Error> {
    let mut q=tx.prepare("SELECT id,type,severity,message,required_action FROM exceptions WHERE status='open' ORDER BY id ASC").map_err(invalid)?;
    q.query_map([], |r| {
        Ok(RoutedException {
            exception_id: r.get(0)?,
            kind: r.get(1)?,
            severity: r.get(2)?,
            message: r.get(3)?,
            required_action: r.get(4)?,
        })
    })
    .map_err(invalid)?
    .collect::<Result<Vec<_>, _>>()
    .map_err(invalid)
}
#[cfg(test)]
mod tests;
