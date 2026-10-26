//! Native ledger-local source facts. The admitted host supplies the read snapshot and physical proofs.
use super::*;
use rentemester_engine::{
    bookkeeping_batch::WorkbenchInput,
    fiscal_year::Date,
    tasks::{Reference, Source, SourceCheck, sources::Observation},
    vat_period,
};
use std::collections::BTreeSet;
pub const COVERAGE: &[&str] = &[
    "Åbne undtagelser, inklusive tidligere og lukkede perioder; bankrelaterede undtagelser samles med bankarbejdet.",
    "Alle sider af uafsluttet bogføringsarbejde fra workbench samt autoritativ bankafstemning; importeret cut-over respekteres.",
    "Registrerede regnskabsperioder og ikke-beståede kontroller for tidligere perioder; overlappende bank-/undtagelseskontroller samles.",
    "Momsangivelse for eksplicit registreret måned, kvartal eller halvår, også nul-/refusionsperioder. Afslutning kræver indberettet periode og reference.",
    "Seneste bogføringsrevision, der afventer godkendelse. Godkendelse læses fra batchens egne godkendelsesbeviser.",
    "Årsafslutning følger selskabets regnskabsår. Årsrapportpligt og frist kræver afklaring eller en eksplicit konfigureret rutine; ingen generel lovfrist antages.",
    "Momsfrister bruger produktets ML §57-regel, verificeret mod SKATs fristoversigt 2026/27 den 8. oktober 2026. Andre myndighedsfrister kræver særskilt vedligeholdt regelgrundlag.",
];
const VAT_BASIS: &str = "Momsloven §57; SKAT: https://skat.dk/erhverv/moms/frister-indberet-og-betal-moms (verificeret 2026-10-08)";
pub struct ReadContext<'a> {
    pub slug: &'a str,
    pub as_of: &'a str,
    pub now: &'a str,
    pub today: &'a str,
    pub latest: Date,
    pub integrity: Result<Vec<Value>, Error>,
}
fn reference(slug: &str, kind: &str, value: &str) -> Reference {
    Reference {
        kind: kind.into(),
        reference: value.into(),
        company_slug: Some(slug.into()),
    }
}
fn checked(state: &str, now: &str, evidence: Vec<Reference>) -> SourceCheck {
    SourceCheck {
        state: state.into(),
        observed_at: now.into(),
        evidence,
    }
}
fn uuid(db: &Connection) -> Result<String, Error> {
    let value: String = db
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if value.len() != 32
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || ('a'..='f').contains(&(b as char)))
    {
        return Err(invalid("Task source ledger identity unavailable"));
    }
    Ok(value)
}
fn period(from: &str, to: &str) -> Value {
    json!({"from":from,"to":to,"label":format!("{from} – {to}")})
}
fn observation(
    slug: &str,
    uuid: &str,
    kind: &str,
    value: &str,
    title: String,
    next: &str,
    content: Value,
    check: SourceCheck,
) -> Result<Observation, Error> {
    let route = match kind {
        "bank_work" | "batch_approval" => "batchbogfoering",
        "exception" => "undtagelser",
        "vat_filing" | "vat_registration" => "moms",
        _ => "periodelas",
    };
    let href = format!(
        "/companies/{slug}/{route}{}",
        if kind == "batch_approval" {
            format!("?runId={value}")
        } else {
            String::new()
        }
    );
    let source = Source {
        kind: kind.into(),
        identity: format!("{uuid}:{kind}:{value}"),
        company_slug: slug.into(),
        reference: value.into(),
        state: check.state.clone(),
        content_hash: tasks::payload_hash(&content)?,
        href,
    };
    Ok(Observation {
        draft: json!({"title":title,"scope":{"kind":"company","companySlug":slug},"nextAction":next,"type":"ad_hoc","origin":"system","source":source,"period":null,"references":[],"deadline":null,"evidenceRequired":true,"relevance":"relevant","verificationRequired":check.state=="unknown"}),
        check,
    })
}
fn numeric(value: &str) -> Option<i64> {
    (!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
}
fn check_bank(
    tx: &Transaction<'_>,
    slug: &str,
    value: &str,
    now: &str,
) -> Result<SourceCheck, Error> {
    let Some(id) = numeric(value) else {
        return Ok(checked("unknown", now, vec![]));
    };
    let date: Option<String> = tx
        .query_row(
            "SELECT transaction_date FROM bank_transactions WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(invalid)?;
    let Some(date) = date.filter(|d| vat_period::date(d).is_ok()) else {
        return Ok(checked("unknown", now, vec![]));
    };
    let report = crate::bank_queries::report(
        tx,
        &crate::bank_queries::ReportQuery {
            from: date.clone(),
            to: date,
            filters: Default::default(),
        },
    )?;
    let open:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM exceptions WHERE related_bank_transaction_id=?1 AND status='open')",[id],|r|r.get(0)).map_err(invalid)?;
    let matched = report["ok"] == true
        && report["data"]["matched"]
            .as_array()
            .is_some_and(|a| a.iter().any(|r| r["bankTransactionId"] == id));
    Ok(if matched && !open {
        checked(
            "resolved",
            now,
            vec![reference(slug, "bank_transaction", value)],
        )
    } else {
        checked("open", now, vec![])
    })
}
fn period_rows(tx: &Transaction<'_>, slug: &str) -> Result<Vec<Value>, Error> {
    crate::periods::list(tx, slug)?["periods"]
        .as_array()
        .cloned()
        .ok_or_else(|| invalid("Period source unavailable"))
}
fn check_vat(
    tx: &Transaction<'_>,
    slug: &str,
    value: &str,
    now: &str,
) -> Result<SourceCheck, Error> {
    let parts: Vec<_> = value.split(':').collect();
    if parts.len() != 2 || parts.iter().any(|s| vat_period::date(s).is_err()) {
        return Ok(checked("unknown", now, vec![]));
    }
    let mut rows = period_rows(tx, slug)?
        .into_iter()
        .filter(|r| {
            r["periodStart"] == parts[0]
                && r["periodEnd"] == parts[1]
                && ["vat_period", "vat_quarter"].contains(&r["kind"].as_str().unwrap_or(""))
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|r| {
        (
            if r["kind"] == "vat_period" { 0 } else { 1 },
            std::cmp::Reverse(r["id"].as_i64().unwrap_or(0)),
        )
    });
    if let Some(r) = rows.first().filter(|r| r["effectiveStatus"] == "reported") {
        return Ok(
            if r["reference"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty())
            {
                checked("resolved", now, vec![reference(slug, "period", value)])
            } else {
                checked("unknown", now, vec![])
            },
        );
    }
    let cadence: Option<String> = tx
        .query_row(
            "SELECT vat_period_type FROM companies ORDER BY id LIMIT 1",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    Ok(checked(
        if cadence
            .as_deref()
            .is_some_and(|s| vat_period::span(s).is_ok())
        {
            "open"
        } else {
            "unknown"
        },
        now,
        vec![],
    ))
}
fn check_batch(
    tx: &Transaction<'_>,
    slug: &str,
    value: &str,
    now: &str,
) -> Result<SourceCheck, Error> {
    let Some(id) = numeric(value) else {
        return Ok(checked("unknown", now, vec![]));
    };
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bookkeeping_batch_runs WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if !exists {
        return Ok(checked("unknown", now, vec![]));
    }
    let batch = crate::bookkeeping_batch::status(tx, id)?;
    let revisions = batch["revisions"]
        .as_array()
        .ok_or_else(|| invalid("Batch source unavailable"))?;
    let Some(latest) = revisions.last().and_then(|r| r["revisionId"].as_i64()) else {
        return Ok(checked("unknown", now, vec![]));
    };
    Ok(
        if revisions.iter().any(|r| {
            r["revisionId"] == latest && r["approvedAt"].as_str().is_some_and(|s| !s.is_empty())
        }) {
            checked(
                "resolved",
                now,
                vec![reference(
                    slug,
                    "approval",
                    &format!("bookkeeping-batch:{value}:{latest}"),
                )],
            )
        } else {
            checked("open", now, vec![])
        },
    )
}
fn check_period(
    tx: &Transaction<'_>,
    slug: &str,
    kind: &str,
    value: &str,
    now: &str,
    integrity: Result<Vec<Value>, Error>,
) -> Result<SourceCheck, Error> {
    let p: Vec<_> = value.split(':').collect();
    if p.len() < 2 || p[..2].iter().any(|s| vat_period::date(s).is_err()) {
        return Ok(checked("unknown", now, vec![]));
    }
    if kind == "period_control" {
        let packet = crate::periods::compute(tx, p[0], p[1], integrity)?;
        let item = packet["items"].as_array().and_then(|a| {
            a.iter()
                .find(|c| Some(c["code"].as_str().unwrap_or("")) == p.get(2).copied())
        });
        return Ok(match item.and_then(|c| c["status"].as_str()) {
            Some("passed") => checked("resolved", now, vec![reference(slug, "period", value)]),
            None | Some("unavailable") => checked("unknown", now, vec![]),
            _ => checked("open", now, vec![]),
        });
    }
    let mut rows = period_rows(tx, slug)?
        .into_iter()
        .filter(|r| {
            r["periodStart"] == p[0]
                && r["periodEnd"] == p[1]
                && (kind != "annual_reporting" || r["kind"] == "fiscal_year")
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|r| std::cmp::Reverse(r["id"].as_i64().unwrap_or(0)));
    let Some(row) = rows.first() else {
        return Ok(checked("unknown", now, vec![]));
    };
    let resolved = if kind == "annual_reporting" {
        row["effectiveStatus"] == "reported"
            && row["reference"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty())
    } else {
        ["closed", "reported"].contains(&row["effectiveStatus"].as_str().unwrap_or(""))
    };
    Ok(if resolved {
        checked("resolved", now, vec![reference(slug, "period", value)])
    } else {
        checked(
            if kind == "annual_reporting" {
                "unknown"
            } else {
                "open"
            },
            now,
            vec![],
        )
    })
}
pub fn check(
    tx: &Transaction<'_>,
    slug: &str,
    task: &Task,
    now: &str,
    integrity: Result<Vec<Value>, Error>,
) -> Result<SourceCheck, Error> {
    if !integrity.as_ref().is_ok_and(|errors| errors.is_empty()) {
        return Ok(checked("unknown", now, vec![]));
    }
    let Some(source) = &task.source else {
        return Ok(checked("unknown", now, vec![]));
    };
    let uuid = uuid(tx)?;
    if source.company_slug != slug
        || !task.scope.companies().contains(&slug)
        || source.identity != format!("{uuid}:{}:{}", source.kind, source.reference)
    {
        return Ok(checked("unknown", now, vec![]));
    }
    match source.kind.as_str() {
        "bank_work" => check_bank(tx, slug, &source.reference, now),
        "vat_filing" => check_vat(tx, slug, &source.reference, now),
        "batch_approval" => check_batch(tx, slug, &source.reference, now),
        "period_close" | "period_control" | "annual_reporting" => {
            check_period(tx, slug, &source.kind, &source.reference, now, integrity)
        }
        "exception" => {
            let id = numeric(&source.reference);
            let status = id
                .map(|id| {
                    tx.query_row::<String, _, _>(
                        "SELECT status FROM exceptions WHERE id=?1",
                        [id],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(invalid)
                })
                .transpose()?
                .flatten();
            Ok(checked(
                match status.as_deref() {
                    Some("resolved") => "resolved",
                    Some(_) => "open",
                    None => "unknown",
                },
                now,
                vec![],
            ))
        }
        _ => Ok(checked("unknown", now, vec![])),
    }
}
fn add(values: &mut Vec<Observation>, item: Observation) {
    let identity = &item.draft["source"]["identity"];
    if let Some(i) = values
        .iter()
        .position(|v| &v.draft["source"]["identity"] == identity)
    {
        values[i] = item;
    } else {
        values.push(item);
    }
}
pub fn collect<F>(
    tx: &Transaction<'_>,
    ctx: ReadContext<'_>,
    mut verify_plan: F,
) -> Result<Vec<Observation>, Error>
where
    F: FnMut(&Value) -> Result<(), Error>,
{
    tasks::date(&json!(ctx.as_of))?;
    if !ctx.integrity.as_ref().is_ok_and(|errors| errors.is_empty()) {
        return Err(invalid(
            "Native source originals are unavailable or changed",
        ));
    }
    let slug = ctx.slug;
    let uuid = uuid(tx)?;
    let mut out = vec![];
    let earliest: Option<String> = tx
        .query_row(
            "SELECT MIN(transaction_date) FROM bank_transactions WHERE transaction_date<=?1",
            [ctx.as_of],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if let Some(from) = &earliest {
        let mut input: WorkbenchInput =
            serde_json::from_value(json!({"from":from,"to":ctx.as_of,"limit":100}))
                .map_err(invalid)?;
        let plan =
            crate::bookkeeping_batch::plan_at(tx, &input.scope(), ctx.latest, ctx.today, ctx.now)?;
        verify_plan(&plan)?;
        let packet = crate::periods::compute(tx, from, ctx.as_of, ctx.integrity.clone())?;
        let close = if let Some(n) = packet["blockers"].as_u64() {
            json!({"status":"available","blockers":n,"hash":packet["hash"]})
        } else {
            json!({"status":"unavailable","blockers":null})
        };
        loop {
            let page = crate::bookkeeping_batch::workbench_at(
                tx,
                &input,
                ctx.latest,
                ctx.today,
                close.clone(),
                ctx.now,
            )?;
            let rows = page["workbench"]["rows"]
                .as_array()
                .ok_or_else(|| invalid("Complete workbench source unavailable"))?;
            if page["workbench"]["state"] == "unavailable" {
                return Err(invalid("Complete workbench source unavailable"));
            }
            for row in rows {
                let id = row["bankTransactionId"]
                    .as_i64()
                    .ok_or_else(|| invalid("Workbench source identity unavailable"))?
                    .to_string();
                let check = check_bank(tx, slug, &id, ctx.now)?;
                let mut o = observation(
                    slug,
                    &uuid,
                    "bank_work",
                    &id,
                    format!("Bogfør og afstem bankpost #{id}"),
                    if row["status"] == "missingDocument" {
                        "Find bilaget på den beskyttede kildeside."
                    } else {
                        "Gennemgå den aktuelle bogføringsplan og dens dokumentation."
                    },
                    json!({"sourceHash":row["sourceHash"],"state":check.state}),
                    check,
                )?;
                let mut refs = vec![reference(slug, "bank_transaction", &id)];
                if let Some(id) = row["document"]["id"].as_i64() {
                    refs.push(reference(slug, "document", &id.to_string()));
                }
                if let Some(id) = row["document"]["party"]["id"].as_str() {
                    refs.push(reference(slug, "party", id));
                }
                o.draft["references"] = json!(refs);
                let date = row["date"]
                    .as_str()
                    .ok_or_else(|| invalid("Workbench date unavailable"))?;
                o.draft["period"] = period(date, date);
                add(&mut out, o);
            }
            let Some(next) = page["workbench"]["page"]["nextCursor"].as_u64() else {
                break;
            };
            let next = usize::try_from(next).map_err(invalid)?;
            if input.cursor.is_some_and(|c| next <= c) {
                return Err(invalid("Incomplete workbench cursor"));
            }
            input.cursor = Some(next);
        }
    }
    let exceptions = crate::exceptions::list_with_archived(tx, slug, "open", true)?;
    for e in exceptions["rows"]
        .as_array()
        .ok_or_else(|| invalid("Exception sources unavailable"))?
    {
        let bank = e["relatedBankTransactionId"]
            .as_i64()
            .filter(|id| *id != 0)
            .map(|id| id.to_string());
        if let Some(bank) = &bank {
            if let Some(existing) = out
                .iter_mut()
                .find(|o| o.draft["source"]["identity"] == format!("{uuid}:bank_work:{bank}"))
            {
                existing.draft["source"]["contentHash"] = json!(tasks::payload_hash(&json!([
                    existing.draft["source"]["contentHash"],
                    e["id"]
                ]))?);
                continue;
            }
        }
        let id = e["id"]
            .as_i64()
            .ok_or_else(|| invalid("Exception id unavailable"))?
            .to_string();
        let value = bank.as_deref().unwrap_or(&id);
        let kind = if bank.is_some() {
            "bank_work"
        } else {
            "exception"
        };
        let check = if bank.is_some() {
            check_bank(tx, slug, value, ctx.now)?
        } else {
            checked("open", ctx.now, vec![])
        };
        let mut o = observation(
            slug,
            &uuid,
            kind,
            value,
            if bank.is_some() {
                format!("Bogfør og afstem bankpost #{value}")
            } else {
                format!("Afklar undtagelse #{id}")
            },
            "Læs undtagelsen og dens dokumentation på den beskyttede kildeside.",
            json!({"id":e["id"],"status":e["status"],"severity":e["severity"]}),
            check,
        )?;
        let mut refs = vec![];
        if let Some(id) = e["relatedDocumentId"].as_i64() {
            refs.push(reference(slug, "document", &id.to_string()));
        }
        if let Some(id) = bank {
            refs.push(reference(slug, "bank_transaction", &id));
        }
        o.draft["references"] = json!(refs);
        add(&mut out, o);
    }
    let rows = period_rows(tx, slug)?
        .into_iter()
        .filter(|r| r["periodStart"].as_str().is_some_and(|s| s <= ctx.as_of))
        .collect::<Vec<_>>();
    for row in &rows {
        let from = row["periodStart"]
            .as_str()
            .ok_or_else(|| invalid("Period date unavailable"))?;
        let to = row["periodEnd"]
            .as_str()
            .ok_or_else(|| invalid("Period date unavailable"))?;
        let value = format!("{from}:{to}");
        let range = period(from, to);
        if row["effectiveStatus"] == "open" {
            let raw = json!({"id":row["id"],"period_start":from,"period_end":to,"kind":row["kind"],"status":row["rowStatus"],"reference":row["reference"]});
            let mut o = observation(
                slug,
                &uuid,
                "period_close",
                &value,
                format!(
                    "Afslut regnskabsperioden {}",
                    range["label"].as_str().unwrap()
                ),
                "Gennemgå og afslut perioden gennem periodens kontroller.",
                json!({"row":raw,"effectiveState":"open"}),
                checked("open", ctx.now, vec![]),
            )?;
            o.draft["period"] = range.clone();
            o.draft["references"] = json!([reference(slug, "period", &value)]);
            add(&mut out, o);
        }
        let packet = crate::periods::compute(tx, from, to, ctx.integrity.clone())?;
        for control in packet["items"]
            .as_array()
            .ok_or_else(|| invalid("Period controls unavailable"))?
        {
            let code = control["code"]
                .as_str()
                .ok_or_else(|| invalid("Period control code unavailable"))?;
            if control["status"] == "passed"
                || ["BANK_UNRECONCILED", "EXCEPTIONS_OPEN", "PERIOD_LIFECYCLE"].contains(&code)
            {
                continue;
            }
            let mut o = observation(
                slug,
                &uuid,
                "period_control",
                &format!("{value}:{code}"),
                format!("Afklar periodekontrol: {code}"),
                "Gennemgå den konkrete kontrol og dens dokumentation på periodens kildeside.",
                json!({"code":code,"sourceHash":control["sourceHash"],"status":control["status"]}),
                checked(
                    if control["status"] == "unavailable" {
                        "unknown"
                    } else {
                        "open"
                    },
                    ctx.now,
                    vec![],
                ),
            )?;
            o.draft["period"] = range.clone();
            o.draft["references"] = json!([reference(slug, "period", &value)]);
            add(&mut out, o);
        }
    }
    let (cadence, start_month): (Option<String>, i32) = tx
        .query_row(
            "SELECT vat_period_type,fiscal_year_start_month FROM companies ORDER BY id LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(invalid)?;
    if cadence
        .as_deref()
        .is_some_and(|s| vat_period::span(s).is_err())
    {
        let mut o = observation(
            slug,
            &uuid,
            "vat_registration",
            "registration",
            "Afklar momsregistrering og frister".into(),
            "Bekræft selskabets momsregistrering og afregningsperiode.",
            json!({"cadence":"unknown"}),
            checked("unknown", ctx.now, vec![]),
        )?;
        o.draft["relevance"] = json!("unknown");
        add(&mut out, o);
    }
    if let Some(cadence) = cadence.as_deref().filter(|s| vat_period::span(s).is_ok()) {
        let first = earliest
            .iter()
            .map(String::as_str)
            .chain(rows.iter().filter_map(|r| r["periodStart"].as_str()))
            .min()
            .map(String::from)
            .unwrap_or_else(|| format!("{}-01-01", &ctx.as_of[..4]));
        let start: i32 = first[..4].parse().map_err(invalid)?;
        let end: i32 = ctx.as_of[..4].parse().map_err(invalid)?;
        if end - start > 100 {
            return Err(invalid("Task source period range unsupported"));
        }
        for year in start..=end {
            for month in (1..=12).step_by(vat_period::span(cadence)? as usize) {
                let window = vat_period::window(&format!("{year:04}-{month:02}-01"), cadence)?;
                if window.end < first || window.start.as_str() > ctx.as_of {
                    continue;
                }
                let value = format!("{}:{}", window.start, window.end);
                let check = check_vat(tx, slug, &value, ctx.now)?;
                if check.state == "resolved" {
                    continue;
                }
                let w = json!({"start":window.start,"end":window.end,"filingDeadline":window.deadline,"vatPeriodType":cadence});
                let mut o = observation(
                    slug,
                    &uuid,
                    "vat_filing",
                    &value,
                    format!("Indberet moms: {}", window.label),
                    "Afstem perioden og gem den faktiske indberetningskvittering gennem momsforløbet.",
                    json!({"cadence":cadence,"window":w,"state":check.state}),
                    check,
                )?;
                o.draft["type"] = json!("obligation");
                o.draft["period"] = period(&window.start, &window.end);
                o.draft["deadline"] = json!({"date":window.deadline,"kind":"statutory","basis":VAT_BASIS,"certainty":"confirmed","ruleId":"DK-VAT-ML57"});
                o.draft["references"] = json!([reference(slug, "period", &value)]);
                add(&mut out, o);
            }
        }
    }
    let mut ranges = rows
        .iter()
        .filter(|r| r["kind"] == "fiscal_year")
        .map(|r| {
            (
                r["periodStart"].as_str().unwrap().to_owned(),
                r["periodEnd"].as_str().unwrap().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    rentemester_engine::fiscal_year::StartMonth::new(start_month)?;
    let year: i32 = ctx.as_of[..4].parse().map_err(invalid)?;
    let month: i32 = ctx.as_of[5..7].parse().map_err(invalid)?;
    let start_year = year - i32::from(month < start_month);
    let from = format!("{start_year:04}-{start_month:02}-01");
    let to = (vat_period::date(&format!("{:04}-{start_month:02}-01", start_year + 1))?
        - time::Duration::days(1))
    .to_string();
    if !ranges.iter().any(|r| r.0 == from && r.1 == to) {
        ranges.push((from, to));
    }
    for (from, to) in ranges {
        let value = format!("{from}:{to}");
        let check = check_period(
            tx,
            slug,
            "annual_reporting",
            &value,
            ctx.now,
            ctx.integrity.clone(),
        )?;
        if check.state == "resolved" {
            continue;
        }
        let range = period(&from, &to);
        let mut o = observation(
            slug,
            &uuid,
            "annual_reporting",
            &value,
            format!(
                "Afklar årsrapport og årsafslutning: {}",
                range["label"].as_str().unwrap()
            ),
            "Bekræft årsrapportpligt, frist og dokumentation for det valgte regnskabsår.",
            json!({"ref":value,"state":check.state}),
            check,
        )?;
        o.draft["type"] = json!("obligation");
        o.draft["origin"] = json!("proposal");
        o.draft["relevance"] = json!("unknown");
        o.draft["period"] = range;
        o.draft["references"] = json!([reference(slug, "period", &value)]);
        add(&mut out, o);
    }
    let mut q = tx
        .prepare("SELECT id FROM bookkeeping_batch_runs ORDER BY id")
        .map_err(invalid)?;
    for id in q.query_map([], |r| r.get::<_, i64>(0)).map_err(invalid)? {
        let id = id.map_err(invalid)?;
        let value = id.to_string();
        let check = check_batch(tx, slug, &value, ctx.now)?;
        if check.state == "resolved" {
            continue;
        }
        let state = crate::bookkeeping_batch::status(tx, id)?;
        let mut o = observation(
            slug,
            &uuid,
            "batch_approval",
            &value,
            format!("Godkend bogføringsplan #{id}"),
            "Gennemgå den seneste præcise revision i det beskyttede godkendelsesforløb.",
            json!({"revisions":state["revisions"],"state":check.state}),
            check,
        )?;
        o.draft["references"] = json!([reference(
            slug,
            "approval",
            &format!("bookkeeping-batch:{id}")
        )]);
        add(&mut out, o);
    }
    Ok(out)
}
/// Recheck every known source even when collection is unavailable, archived or no longer lists it.
pub fn observe<F>(
    tx: &Transaction<'_>,
    ctx: ReadContext<'_>,
    existing: &[Task],
    discover: bool,
    verify: F,
) -> (Vec<Observation>, bool)
where
    F: FnMut(&Value) -> Result<(), Error>,
{
    let integrity = ctx.integrity.clone();
    let slug = ctx.slug;
    let now = ctx.now;
    let (mut out, failed) = if discover {
        match collect(tx, ctx, verify) {
            Ok(v) => (v, false),
            Err(_) => (vec![], true),
        }
    } else {
        (vec![], false)
    };
    let observed = out
        .iter()
        .filter_map(|o| o.draft["source"]["identity"].as_str())
        .map(String::from)
        .collect::<BTreeSet<_>>();
    for task in existing.iter().filter(|t| {
        t.source
            .as_ref()
            .is_some_and(|s| s.company_slug == slug && !observed.contains(&s.identity))
    }) {
        let check = check(tx, slug, task, now, integrity.clone())
            .unwrap_or_else(|_| checked("unknown", now, vec![]));
        let mut source = task.source.clone().unwrap();
        if source.state != check.state {
            source.content_hash = tasks::payload_hash(
                &json!({"previousContent":source.content_hash,"state":check.state}),
            )
            .unwrap_or(source.content_hash);
        }
        source.state = check.state.clone();
        out.push(Observation{draft:json!({"title":task.title,"scope":task.scope,"source":source,"verificationRequired":check.state=="unknown"}),check});
    }
    (out, failed)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceError {
    pub company_slug: String,
    pub reason: String,
}
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub created: usize,
    pub updated: usize,
    pub resolved: usize,
    pub reopened: usize,
    pub unknown: usize,
    pub errors: Vec<SourceError>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncResult {
    pub scope: tasks::Scope,
    pub includes_workspace: bool,
    pub sync: Summary,
}
fn source_task(db: &Connection, source: &Source) -> Result<Option<Task>, Error> {
    let raw:Option<String>=db.query_row("SELECT payload_json FROM rm_current_tasks WHERE json_extract(payload_json,'$.source.companySlug')=?1 AND json_extract(payload_json,'$.source.kind')=?2 AND json_extract(payload_json,'$.source.identity')=?3",params![source.company_slug,source.kind,source.identity],|r|r.get(0)).optional().map_err(invalid)?;
    raw.map(|s| serde_json::from_str(&s).map_err(invalid))
        .transpose()
}
pub fn synchronize_in_tx<A>(
    tx: &Transaction<'_>,
    input: &Value,
    ctx: &Context<'_>,
    mut authorize: A,
) -> Result<Task, Error>
where
    A: FnMut(&Task) -> Result<(), Error>,
{
    super::series::fence(tx, ctx)?;
    let value = tasks::draft(input)?;
    let source: Source = serde_json::from_value(value["source"].clone()).map_err(invalid)?;
    let current = source_task(tx, &source)?;
    let generated = format!(
        "source-{}",
        &tasks::payload_hash(&json!([source.company_slug, source.kind, source.identity]))?[..32]
    );
    let candidate = current
        .clone()
        .map(Ok)
        .unwrap_or_else(|| tasks::prepare_create(&value, &generated, ctx.now))?;
    authorize(&candidate)?;
    if let Some(result) = super::series::prior(tx, ctx, "task-source-sync", input)? {
        let historical: Task = serde_json::from_value(result).map_err(invalid)?;
        let live = super::read(tx, &historical.task_id)?
            .ok_or_else(|| invalid("Source task unavailable"))?;
        authorize(&live)?;
        authorize(&historical)?;
        return Ok(historical);
    }
    let (task, event) = tasks::sources::synchronize(
        current.as_ref(),
        input,
        ctx.expected_version,
        ctx.now,
        &super::boards(tx)?,
    )?;
    authorize(&task)?;
    if let Some(event) = event {
        if current.is_none() && super::read(tx, &task.task_id)?.is_some() {
            return Err(Error::new(
                "task_exists",
                "Source task reference already exists",
            ));
        }
        let raw = tasks::canonical_json(&json!(task));
        tx.execute("INSERT INTO rm_task_events(task_id,version,operation,payload_json,actor,principal,created_at,reason)VALUES(?1,?2,?3,?4,?5,?6,?7,NULL)",params![task.task_id,i64::try_from(task.version).map_err(invalid)?,event,raw,ctx.actor,ctx.principal,task.updated_at]).map_err(invalid)?;
    }
    super::series::append_receipt(tx, ctx, "task-source-sync", input, &json!(task))?;
    Ok(task)
}
pub fn sync_in_tx<A, R>(
    tx: &Transaction<'_>,
    payload: &Value,
    companies: &[String],
    ctx: &Context<'_>,
    mut authorize: A,
    mut read: R,
) -> Result<SyncResult, Error>
where
    A: FnMut(&Task) -> Result<(), Error>,
    R: FnMut(&str, &[Task]) -> Result<(Vec<Observation>, bool), Error>,
{
    super::series::fence(tx, ctx)?;
    if let Some(value) = super::series::prior(tx, ctx, "sources-sync", payload)? {
        return serde_json::from_value(value).map_err(invalid);
    }
    let mut summary = Summary::default();
    for slug in companies {
        let all = super::list(tx, |_| Ok(true))?
            .into_iter()
            .filter(|t| t.source.as_ref().is_some_and(|s| s.company_slug == *slug))
            .collect::<Vec<_>>();
        let outside = all
            .iter()
            .filter(|t| {
                t.scope.companies().is_empty()
                    || !t
                        .scope
                        .companies()
                        .iter()
                        .all(|s| companies.iter().any(|c| c == s))
            })
            .filter_map(|t| t.source.as_ref().map(|s| s.identity.clone()))
            .collect::<BTreeSet<_>>();
        let existing = all
            .into_iter()
            .filter(|t| {
                t.source
                    .as_ref()
                    .is_some_and(|s| !outside.contains(&s.identity))
            })
            .collect::<Vec<_>>();
        for t in &existing {
            authorize(t)?;
        }
        let (observations, failed) = read(slug, &existing)?;
        if failed {
            summary.errors.push(SourceError {
                company_slug: slug.clone(),
                reason:
                    "Kilder kunne ikke gennemlæses fuldstændigt; eksisterende arbejde er bevaret."
                        .into(),
            });
        }
        for o in observations.into_iter().filter(|o| {
            !o.draft["source"]["identity"]
                .as_str()
                .is_some_and(|id| outside.contains(id))
        }) {
            let source: Source =
                serde_json::from_value(o.draft["source"].clone()).map_err(invalid)?;
            if source.company_slug != *slug || source.state != o.check.state {
                return Err(invalid(
                    "Native source observation differs from requested scope/check",
                ));
            }
            let before = existing.iter().find(|t| {
                t.source
                    .as_ref()
                    .is_some_and(|s| s.identity == source.identity)
            });
            let key = format!(
                "source:{}",
                tasks::payload_hash(&json!([
                    source.identity,
                    source.content_hash,
                    o.check.state,
                    before.map(|t| t.version).unwrap_or(0)
                ]))?
            );
            let mut after = synchronize_in_tx(
                tx,
                &o.draft,
                &Context {
                    actor: ctx.actor,
                    principal: ctx.principal,
                    idempotency_key: &key,
                    expected_version: before.map(|t| t.version),
                    now: ctx.now,
                    source_check: Some(&o.check),
                },
                &mut authorize,
            )?;
            if after.status != "done"
                && after.relevance != "unknown"
                && before
                    .and_then(|t| t.source.as_ref())
                    .is_none_or(|s| s.state != "resolved")
                && o.check.state == "resolved"
                && !o.check.evidence.is_empty()
            {
                let key = format!(
                    "source-complete:{}",
                    tasks::payload_hash(&json!([after.task_id, after.version, o.check.evidence]))?
                );
                let input = json!({"outcome":"completed","note":"Kildens aktuelle resultat er registreret.","references":o.check.evidence});
                after = super::mutate_in_tx_checked(
                    tx,
                    Command::Complete {
                        id: &after.task_id,
                        input: &input,
                    },
                    Context {
                        actor: ctx.actor,
                        principal: ctx.principal,
                        idempotency_key: &key,
                        expected_version: Some(after.version),
                        now: ctx.now,
                        source_check: Some(&o.check),
                    },
                    |_, t| authorize(t),
                    |_, _| Ok(()),
                )?
                .task;
            }
            if let Some(before) = before {
                if after.version != before.version {
                    summary.updated += 1;
                }
            } else {
                summary.created += 1;
            }
            if before.is_none_or(|t| t.status != "done") && after.status == "done" {
                summary.resolved += 1;
            }
            if before.is_some_and(|t| t.status == "done") && after.status != "done" {
                summary.reopened += 1;
            }
            if o.check.state == "unknown" {
                summary.unknown += 1;
            }
        }
    }
    let result = SyncResult {
        scope: tasks::Scope::Workspace {
            company_slugs: companies.to_vec(),
        },
        includes_workspace: false,
        sync: summary,
    };
    super::series::append_receipt(tx, ctx, "sources-sync", payload, &json!(result))?;
    Ok(result)
}

#[cfg(test)]
mod tests;
