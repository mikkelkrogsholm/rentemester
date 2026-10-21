//! Standalone dashboard data composed exclusively on the caller's admitted read transaction.
use crate::invalid;
use rentemester_engine::{
    Error,
    dashboard_html::{Dashboard, Fact, Section},
    fiscal_year::Date,
    money::Money,
};
use rusqlite::Transaction;
use serde_json::Value;

fn section(id: &str, title: &str, basis: impl Into<String>) -> Section {
    Section {
        id: id.into(),
        title: title.into(),
        basis: basis.into(),
        facts: vec![],
        columns: vec![],
        rows: vec![],
        notes: vec![],
        next_action: None,
    }
}
fn text(value: &Value) -> String {
    match value {
        Value::Null => "Ikke tilgængelig".into(),
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    }
}
fn amount(value: &Value) -> Result<Money, Error> {
    crate::invoice_fx::amount(value)
}
fn dkk(value: &Value) -> Result<String, Error> {
    if value.is_null() {
        return Ok("Ikke tilgængelig".into());
    }
    Ok(format!(
        "{} kr.",
        amount(value)?.decimal().replace('.', ",")
    ))
}
fn fact(section: &mut Section, label: &str, value: impl Into<String>) {
    section.facts.push(Fact {
        label: label.into(),
        value: value.into(),
    });
}
fn array(value: &Value) -> Result<&Vec<Value>, Error> {
    value
        .as_array()
        .ok_or_else(|| invalid("dashboard reader rows unavailable"))
}
fn unavailable(section: &mut Section, reason: impl Into<String>, action: &str) {
    section
        .notes
        .push(format!("Ikke tilgængelig: {}", reason.into()));
    section.next_action = Some(action.into());
}
fn overflow(section: &mut Section, total: usize) {
    if total > 10 {
        section.notes.push(format!(
            "Viser 10 af {total} poster; se den fulde oversigt for resten."
        ));
    }
}
fn next_month(as_of: &str) -> Result<String, Error> {
    let d = rentemester_engine::vat_period::date(as_of)?;
    let (y, m) = if d.month() as u8 == 12 {
        (d.year() + 1, 1)
    } else {
        (d.year(), d.month() as u8 + 1)
    };
    let result = format!("{y:04}-{m:02}-01");
    Date::parse(&result)?;
    Ok(result)
}
fn event_label(event: &str) -> &str {
    match event {
        "journal_post" | "manual_journal_post" => "Postering bogført",
        "invoice_issue" => "Faktura udstedt",
        "invoice_post" => "Faktura bogført",
        "invoice_payment_apply" => "Fakturabetaling registreret",
        "document_register" => "Bilag registreret",
        "bank_import" => "Banktransaktioner importeret",
        "bank_reconcile" => "Bank afstemt",
        "period_close" => "Periode lukket",
        "period_reopen" => "Periode genåbnet",
        "period_report" => "Indberetningskvittering registreret",
        "budget_set" => "Budget opdateret",
        "accrual_register" => "Periodeafgrænsning registreret",
        "accrual_recognize" => "Periodeafgrænsning indregnet",
        "customer_add" => "Kunde oprettet",
        "payable_register" => "Leverandørgæld registreret",
        "payable_payment_apply" => "Leverandørbetaling registreret",
        _ => event,
    }
}

/// No connections, clock, filesystem, provider calls or writes are introduced here.
pub fn read(
    tx: &Transaction<'_>,
    slug: &str,
    as_of: &str,
    generated_at: &str,
    build_revision: &str,
    rule_bundle_version: &str,
) -> Result<Dashboard, Error> {
    Date::parse(as_of)?;
    crate::validate(tx)?;
    let company = crate::company_setup::profile(tx)?;
    crate::opening_balance::validate_history(tx)?;
    let native_basis = crate::opening_balance::require_report_as_of(tx, as_of);
    if let Err(error) = &native_basis {
        if error.code != "archive_report_unsupported" {
            return Err(error.clone());
        }
    }
    let invoice = if native_basis.is_ok() {
        Some(crate::portfolio::dated_invoices(tx, as_of)?)
    } else {
        None
    };
    let exceptions = crate::exceptions::rows(tx, "open")?;
    let bank = crate::bank::read(tx, slug, as_of, None)?;
    let mut unmatched = 0u64;
    for year in array(&bank["fiscalYears"])? {
        let label = year["label"]
            .as_str()
            .ok_or_else(|| invalid("bank fiscal label unavailable"))?;
        let year_bank = crate::bank::read(tx, slug, as_of, Some(label))?;
        unmatched = unmatched
            .checked_add(
                year_bank["unmatchedCount"]
                    .as_u64()
                    .ok_or_else(|| invalid("bank unmatched count unavailable"))?,
            )
            .ok_or_else(|| invalid("dashboard count overflow"))?;
    }
    let mut metrics = section(
        "metrics",
        "Nøgletal",
        format!(
            "Fakturaer udstedt i Rentemester med dateret saldo pr. {as_of}. Bank og opgaver viser nuværende status på tværs af regnskabsår."
        ),
    );
    if let Some(v) = &invoice {
        fact(
            &mut metrics,
            "Åbne fakturaer udstedt i Rentemester",
            text(&v["openInvoiceCount"]),
        );
        fact(
            &mut metrics,
            "Åben saldo for fakturaer udstedt i Rentemester",
            dkk(&v["openInvoiceTotal"])?,
        );
        fact(
            &mut metrics,
            "Forfaldne fakturaer udstedt i Rentemester",
            text(&v["overdueInvoiceCount"]),
        );
        fact(
            &mut metrics,
            "Ældste forfald (dage)",
            text(&v["oldestOverdueDays"]),
        );
    } else {
        unavailable(
            &mut metrics,
            "Bogføringshistorikken er ikke tilgængelig før den dokumenterede overgang.",
            "Se kildearkiv og overgangsgrundlag.",
        );
    }
    fact(
        &mut metrics,
        "Uafstemte banktransaktioner",
        unmatched.to_string(),
    );
    fact(&mut metrics, "Åbne opgaver", exceptions.len().to_string());
    let mut tasks = section(
        "exceptions",
        "Åbne opgaver",
        "Nuværende åben status; historisk opgavestatus er ikke rekonstrueret.",
    );
    tasks.columns = vec![
        "Alvor".into(),
        "Type".into(),
        "Besked".into(),
        "Nødvendig handling".into(),
    ];
    tasks.rows = exceptions
        .iter()
        .map(|r| {
            vec![
                text(&r["severity"]),
                text(&r["type"]),
                text(&r["message"]),
                text(&r["requiredAction"]),
            ]
        })
        .collect();
    if !exceptions.is_empty() {
        tasks.next_action = Some("Gennemgå og løs de åbne opgaver i opgaveoversigten.".into());
    }

    let vat = crate::vat_report::dashboard_period(tx, as_of)?;
    let mut vat_section = section(
        "vat",
        "Næste momsfrist",
        format!(
            "Første periode med aktivitet frem til {as_of}, som ikke er registreret indberettet. Beregningen omfatter hele den valgte momsperiode."
        ),
    );
    if vat["vatRegistered"] == true {
        for (label, key) in [
            ("Momsperiode", "periodLabel"),
            ("Periode fra", "periodStart"),
            ("Periode til", "periodEnd"),
            ("Frist", "deadline"),
            ("Dage til frist", "daysRemaining"),
            ("Periodestatus", "periodStatus"),
        ] {
            fact(&mut vat_section, label, text(&vat[key]));
        }
        fact(
            &mut vat_section,
            "Beregnet moms til betaling",
            dkk(&vat["payable"])?,
        );
        fact(
            &mut vat_section,
            "Uindberettet aktivitet",
            if vat["hasUnreportedActivity"] == true {
                "Ja"
            } else {
                "Ingen fundet; viser den aktuelle momsperiode"
            },
        );
        for error in array(&vat["errors"])? {
            vat_section
                .notes
                .push(format!("Beregningsfejl: {}", text(error)));
        }
        vat_section.notes.push("Åben og lukket er interne periodestatusser. Indberettet betyder registreret kvittering; denne eksport indsender eller betaler ikke moms.".into());
        vat_section.next_action =
            Some("Gennemgå momsrapporten og dokumentér indberetning og betaling særskilt.".into());
    } else {
        fact(
            &mut vat_section,
            "Momspligt",
            "Virksomheden er ikke momsregistreret; ingen momsfrist beregnes.",
        );
    }

    let mut invoices = section(
        "invoices",
        "Åbne fakturaer",
        format!(
            "Kun fakturaer udstedt i Rentemester til og med {as_of}; betalinger, kreditnotaer og refunderinger afgrænses efter deres dokumenterede dato. Importerede tilgodehavender er særskilte."
        ),
    );
    invoices.columns = vec![
        "Kilde".into(),
        "Faktura".into(),
        "Kunde".into(),
        "Åben saldo".into(),
        "Forfald".into(),
        "Dage over frist".into(),
    ];
    if let Some(v) = &invoice {
        if let Some(rows) = v["rows"].as_array() {
            for r in rows.iter().take(10) {
                invoices.rows.push(vec![
                    "Rentemester".into(),
                    text(&r["invoiceNo"]),
                    text(&r["buyerName"]),
                    dkk(&r["openBalance"])?,
                    text(&r["dueDate"]),
                    if r["dueDate"].is_null() {
                        "Ukendt".into()
                    } else {
                        text(&r["overdueDays"])
                    },
                ]);
            }
            overflow(&mut invoices, rows.len());
            if rows.iter().any(|r| r["dueDate"].is_null()) {
                invoices.notes.push("Fakturaer uden dokumenteret forfaldsdato indgår i åben saldo, men deres forfald og overtrædelsesdage er ukendte.".into());
            }
        } else {
            unavailable(
                &mut invoices,
                "Dateret opgørelse understøtter endnu ikke alle valutaer eller tabskorrektioner.",
                "Se fakturaoversigt og korrektionsgrundlag.",
            );
        }
    } else {
        unavailable(
            &mut invoices,
            "Valgt dato ligger før den dokumenterede bogføringshistorik.",
            "Se kildearkivet.",
        );
    }
    let imported = crate::imported_receivables::read(tx, as_of)?;
    fact(
        &mut invoices,
        "Importeret tilgodehavendesaldo (særskilt)",
        dkk(&imported["totalOpen"])?,
    );
    invoices.notes.push("Importerede saldi lægges ikke til de nøgletal for fakturaer udstedt i Rentemester; se den separate importerede tilgodehavendeoversigt.".into());

    let pay = crate::payables::list(tx, slug, as_of, "open")?;
    let mut payables = section(
        "payables",
        "Åbne kreditorposter",
        format!(
            "Registrerede leverandørfakturaer og betalinger til og med {as_of}; ingen antaget historisk status."
        ),
    );
    fact(&mut payables, "Åbne poster", text(&pay["count"]));
    fact(&mut payables, "Åben saldo", dkk(&pay["totalOpenBalance"])?);
    payables.columns = vec![
        "Leverandør".into(),
        "Faktura".into(),
        "Åben saldo".into(),
        "Forfald".into(),
        "Dage over frist".into(),
    ];
    let rows = array(&pay["rows"])?;
    for r in rows.iter().take(10) {
        payables.rows.push(vec![
            text(&r["supplierName"]),
            text(&r["billNo"]),
            dkk(&r["openBalance"])?,
            text(&r["dueDate"]),
            text(&r["overdueDays"]),
        ]);
    }
    overflow(&mut payables, rows.len());

    let acc = crate::accruals::list(tx, slug)?;
    let mut accruals = section(
        "accruals",
        "Periodeafgrænsningsposter",
        format!(
            "Nuværende register og indregningsstatus; manglende indregninger med planlagt dato til og med {as_of}."
        ),
    );
    let mut active = 0;
    let mut due_count = 0;
    let mut due_amount = Money::default();
    for r in array(&acc["report"]["accruals"])? {
        if r["fullyRecognized"] != true {
            active += 1;
        }
        let id = r["accrualId"]
            .as_i64()
            .ok_or_else(|| invalid("accrual identity unavailable"))?;
        let preview = crate::accruals::preview(tx, id)?;
        for period in array(&preview["schedule"])? {
            if period["recognized"] != true
                && period["recognitionDate"]
                    .as_str()
                    .is_some_and(|d| d <= as_of)
            {
                due_count += 1;
                due_amount.add(&amount(&period["amount"])?);
            }
        }
    }
    fact(
        &mut accruals,
        "Resterende eksponering",
        dkk(&acc["report"]["totals"]["remainingAmount"])?,
    );
    fact(&mut accruals, "Aktive afgrænsninger", active.to_string());
    fact(
        &mut accruals,
        "Forfaldne indregninger",
        due_count.to_string(),
    );
    fact(
        &mut accruals,
        "Forfaldent beløb",
        format!("{} kr.", due_amount.decimal().replace('.', ",")),
    );
    if due_count > 0 {
        accruals.next_action =
            Some("Gennemgå og indregn de forfaldne perioder i afgrænsningsoversigten.".into());
    }

    let month = &as_of[..7];
    let plan = crate::budget::read(tx, slug, as_of, None, true)?;
    let mut budget = section(
        "budget",
        "Budget",
        format!(
            "Kalendermåneden {month}, hele månedens bogførte bevægelser og seneste budgetrevisioner. Ikke et budget afgrænset til dagen."
        ),
    );
    let mut total_budget = Money::default();
    let mut actual = Money::default();
    let mut variance = Money::default();
    budget.columns = vec![
        "Konto".into(),
        "Navn".into(),
        "Budget".into(),
        "Faktisk".into(),
        "Gunstig afvigelse".into(),
    ];
    for r in array(&plan["lines"])? {
        if r["period"] == month {
            total_budget.add(&amount(&r["budget"])?);
            actual.add(&amount(&r["actual"])?);
            variance.add(&amount(&r["variance"])?);
            budget.rows.push(vec![
                text(&r["accountNo"]),
                text(&r["accountName"]),
                dkk(&r["budget"])?,
                dkk(&r["actual"])?,
                dkk(&r["variance"])?,
            ]);
        }
    }
    for (label, value) in [
        ("Budget", total_budget),
        ("Faktisk", actual),
        ("Samlet gunstig afvigelse", variance),
    ] {
        fact(
            &mut budget,
            label,
            format!("{} kr.", value.decimal().replace('.', ",")),
        );
    }
    budget.notes.push("Gunstig afvigelse er beregnet pr. konto: faktisk minus budget for indtægter, budget minus faktisk for udgifter.".into());

    let start = next_month(as_of)?;
    let mut liquidity = section(
        "liquidity",
        "Likviditetsprognose",
        format!(
            "Tre hele kalendermåneder fra {start}. Nuværende planlægningsgrundlag; fremtidige indbetalinger og udbetalinger er en prognose."
        ),
    );
    match crate::supplier_commitments::dashboard_forecast(tx, slug, &start) {
        Ok(f) => {
            fact(
                &mut liquidity,
                "Laveste likviditet",
                dkk(&f["lowestPoint"])?,
            );
            fact(&mut liquidity, "Åbningslikviditet", dkk(&f["openingCash"])?);
            liquidity.columns = vec![
                "Fra".into(),
                "Til".into(),
                "Slutlikviditet".into(),
                "Med momsscenarier".into(),
            ];
            for p in array(&f["periods"])? {
                liquidity.rows.push(vec![
                    text(&p["periodStart"]),
                    text(&p["periodEnd"]),
                    dkk(&p["closingCash"])?,
                    dkk(&p["scenarioClosingCash"])?,
                ]);
            }
            if let Some(last) = array(&f["periods"])?.last() {
                fact(
                    &mut liquidity,
                    "Likviditet ved slut",
                    dkk(&last["closingCash"])?,
                );
            }
            for label in ["included", "excluded"] {
                for note in array(&f["completeness"][label])? {
                    liquidity.notes.push(text(note));
                }
            }
            liquidity.notes.push("Prognosen er ufuldstændig og dokumenterer hverken gennemførte betalinger eller en garanteret banksaldo.".into());
        }
        Err(e) => unavailable(
            &mut liquidity,
            format!("Prognosens datagrundlag kunne ikke valideres ({}).", e.code),
            "Gennemgå likviditetsprognosens datagrundlag.",
        ),
    }
    let mut tax = section(
        "tax",
        "Skat",
        format!("Valgt regnskabsår: {}", text(&plan["selectedYear"])),
    );
    unavailable(
        &mut tax,
        "Skatteopgørelsen er ikke tilgængelig i dette overblik; et lukket regnskabsår dokumenterer ikke i sig selv beregnet selskabsskat.",
        "Skatteopgørelsen skal udarbejdes og gennemgås særskilt før manuel indberetning.",
    );

    let mut eu = section(
        "eu",
        "EU-salg og OSS",
        "Samme valgte momsperiode som momsafsnittet; ingen eksterne opslag eller indberetning.",
    );
    if vat["vatRegistered"] == true {
        let from = Date::parse(
            vat["periodStart"]
                .as_str()
                .ok_or_else(|| invalid("VAT start unavailable"))?,
        )?;
        let to = Date::parse(
            vat["periodEnd"]
                .as_str()
                .ok_or_else(|| invalid("VAT end unavailable"))?,
        )?;
        let sales = crate::eu_sales::read(tx, from, to)?;
        let oss = crate::oss_report::read(tx, from, to)?;
        if sales["ok"] == true && oss["ok"] == true {
            let base = amount(&sales["totalValue"])?;
            let oss_base = amount(&oss["ossConsumerSalesBase"])?;
            if base != Money::default() || oss_base != Money::default() {
                fact(&mut eu, "EU-salg", dkk(&sales["totalValue"])?);
                fact(
                    &mut eu,
                    "EU-kunder",
                    array(&sales["customers"])?.len().to_string(),
                );
                fact(
                    &mut eu,
                    "OSS forbrugergrundlag",
                    dkk(&oss["ossConsumerSalesBase"])?,
                );
                eu.next_action=Some("Gennemgå EU-salgslisten og OSS-grundlaget; separat indberetning kan være nødvendig.".into());
            } else {
                eu.notes
                    .push("Ingen EU-salgs- eller OSS-aktivitet i den valgte momsperiode.".into());
            }
            for warning in array(&oss["warnings"])? {
                eu.notes.push(text(warning));
            }
        } else {
            unavailable(
                &mut eu,
                "EU-/OSS-grundlaget har beregningsfejl; der vises ingen antaget nulsaldo.",
                "Gennemgå EU-/OSS-rapporternes fejl.",
            );
        }
    } else {
        eu.notes.push(
            "Ikke momsregistreret; EU-/OSS-indberetningsgrundlaget er ikke tilgængeligt.".into(),
        );
    }

    let history = crate::audit_log::read(
        tx,
        slug,
        &crate::audit_log::Query {
            limit: Some(10),
            ..Default::default()
        },
    )?;
    let mut activity = section(
        "activity",
        "Seneste aktivitet",
        "De seneste 10 registrerede hændelser i den nuværende auditlog, også efter den valgte dato.",
    );
    activity.columns = vec![
        "Tid".into(),
        "Hændelse".into(),
        "Aktør".into(),
        "Objekt".into(),
        "Besked".into(),
    ];
    for r in array(&history["rows"])? {
        let event = r["eventType"]
            .as_str()
            .ok_or_else(|| invalid("audit event unavailable"))?;
        activity.rows.push(vec![
            text(&r["createdAt"]),
            event_label(event).into(),
            text(&r["actor"]),
            format!("{} {}", text(&r["entityType"]), text(&r["entityId"])),
            text(&r["message"]),
        ]);
    }
    let audit = crate::journal_hash::verify(tx)?;
    let mut system = section(
        "system",
        "Systemstatus",
        "Nuværende journal-hashkæde; dette er ikke en fuld kontrol af auditlog eller fysisk bilagsintegritet.",
    );
    fact(
        &mut system,
        "Journalens hashkæde",
        if audit.ok {
            "Verificeret"
        } else {
            "Kontrol fejlede"
        },
    );
    fact(
        &mut system,
        "Kontrollerede posteringer",
        audit.entries_checked.to_string(),
    );
    system.notes.extend(audit.errors);
    unavailable(
        &mut system,
        "Backupoplysninger kræver selskabsadministrator; vises særskilt ved adgang og fysisk kontrol af bevarede backupfiler.",
        "Åbn backupstatus med virksomhedsadministratoradgang.",
    );
    Ok(Dashboard {
        company_name: company["name"]
            .as_str()
            .ok_or_else(|| invalid("company name unavailable"))?
            .into(),
        cvr: company["cvr"].as_str().map(str::to_owned),
        as_of_date: as_of.into(),
        generated_at: generated_at.into(),
        build_revision: build_revision.into(),
        rule_bundle_version: rule_bundle_version.into(),
        sections: vec![
            metrics,
            tasks,
            vat_section,
            invoices,
            payables,
            accruals,
            budget,
            liquidity,
            tax,
            eu,
            activity,
            system,
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    fn fixture() -> Connection {
        let mut control = Connection::open_in_memory().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic <Dashboard> & Co")
            .unwrap();
        db.execute("UPDATE companies SET vat_period_type=NULL WHERE id=1", [])
            .unwrap();
        db
    }
    fn find<'a>(dashboard: &'a Dashboard, id: &str) -> &'a Section {
        dashboard.sections.iter().find(|s| s.id == id).unwrap()
    }
    fn value<'a>(section: &'a Section, label: &str) -> &'a str {
        &section
            .facts
            .iter()
            .find(|f| f.label == label)
            .unwrap()
            .value
    }
    fn snapshot(tx: &Transaction<'_>, date: &str) -> Result<Dashboard, Error> {
        read(
            tx,
            "synthetic",
            date,
            "2026-10-21T12:00:00Z",
            "synthetic-revision",
            "synthetic-rules",
        )
    }
    #[test]
    fn all_sections_date_truth_determinism_and_no_effects() {
        let mut db = fixture();
        let changes = db.total_changes();
        let tx = db.transaction().unwrap();
        let d = snapshot(&tx, "2026-10-21").unwrap();
        assert_eq!(
            d.sections.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec![
                "metrics",
                "exceptions",
                "vat",
                "invoices",
                "payables",
                "accruals",
                "budget",
                "liquidity",
                "tax",
                "eu",
                "activity",
                "system"
            ]
        );
        assert_eq!(
            value(find(&d, "metrics"), "Åbne fakturaer udstedt i Rentemester"),
            "0"
        );
        assert!(
            find(&d, "vat").facts[0]
                .value
                .contains("ikke momsregistreret")
        );
        assert!(find(&d, "tax").notes[0].contains("Skatteopgørelsen er ikke tilgængelig"));
        let liquidity = find(&d, "liquidity");
        assert_eq!(liquidity.rows.len(), 3);
        assert_eq!(&liquidity.rows[0][..2], &["2026-11-01", "2026-11-30"]);
        assert_eq!(&liquidity.rows[2][..2], &["2027-01-01", "2027-01-31"]);
        assert_eq!(value(liquidity, "Laveste likviditet"), "Ikke tilgængelig");
        assert_eq!(snapshot(&tx, "2026-10-21").unwrap(), d);
        let html = rentemester_engine::dashboard_html::render(&d).unwrap();
        assert!(html.contains("Synthetic &lt;Dashboard&gt; &amp; Co"));
        assert!(snapshot(&tx, "2026-02-30").is_err());
        tx.commit().unwrap();
        assert_eq!(db.total_changes(), changes);
    }
    #[test]
    fn invoice_rows_keep_cutoff_old_year_sort_and_unknown_due_truth() {
        let mut db = fixture();
        db.execute_batch(r#"INSERT INTO documents(source,document_type,sha256_hash,invoice_no,invoice_date,amount_inc_vat,currency,payload_json) VALUES
        ('synthetic','issued_invoice','synthetic-1','Z-2','2024-09-01',100.01,'DKK','{"currency":"DKK","buyer":{"name":"Synthetic <Buyer>"},"dueDate":"2024-10-01"}'),
        ('synthetic','issued_invoice','synthetic-2','A-1','2026-10-01',200.02,'DKK','{"currency":"DKK","buyer":{"name":"Synthetic Buyer 2"},"dueDate":"2026-11-01"}'),
        ('synthetic','issued_invoice','synthetic-3','FUTURE','2027-01-01',999,'DKK','{"currency":"DKK","dueDate":"2027-02-01"}'),
        ('synthetic','issued_invoice','synthetic-4','UNKNOWN','2026-10-01',0.03,'DKK','{"currency":"DKK"}');"#).unwrap();
        let changes = db.total_changes();
        let tx = db.transaction().unwrap();
        let d = snapshot(&tx, "2026-10-21").unwrap();
        let metrics = find(&d, "metrics");
        assert_eq!(value(metrics, "Åbne fakturaer udstedt i Rentemester"), "3");
        assert_eq!(
            value(metrics, "Åben saldo for fakturaer udstedt i Rentemester"),
            "300,06 kr."
        );
        assert_eq!(
            value(metrics, "Forfaldne fakturaer udstedt i Rentemester"),
            "1"
        );
        let invoices = find(&d, "invoices");
        assert_eq!(invoices.rows.len(), 3);
        assert_eq!(invoices.rows[0][1], "Z-2");
        assert_eq!(invoices.rows[2][5], "Ukendt");
        assert!(
            invoices
                .notes
                .iter()
                .any(|n| n.contains("uden dokumenteret"))
        );
        assert_eq!(
            value(
                find(&snapshot(&tx, "2024-09-15").unwrap(), "metrics"),
                "Åbne fakturaer udstedt i Rentemester"
            ),
            "1"
        );
        tx.commit().unwrap();
        assert_eq!(db.total_changes(), changes);
    }
    #[test]
    fn month_budget_sums_favourable_line_variances_and_vat_uses_calendar_activity_year() {
        let mut db = fixture();
        db.execute_batch("UPDATE companies SET fiscal_year_start_month=7,fiscal_year_label_strategy='span',vat_period_type='quarter' WHERE id=1;
        INSERT INTO accounts(id,account_no,name,type,normal_balance) VALUES(1,'B','Synthetic bank','asset','debit'),(2,'I','Synthetic income','income','credit'),(3,'X','Synthetic expense','expense','debit');
        INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES(1,'S-1','2025-10-01','Synthetic historical activity','fixture','synthetic-1'),(2,'S-2','2026-10-01','Synthetic current income','fixture','synthetic-2'),(3,'S-3','2026-10-02','Synthetic current expense','fixture','synthetic-3');
        INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(1,1,1,0),(1,2,0,1),(2,1,110.01,0),(2,2,0,110.01),(3,3,30.02,0),(3,1,0,30.02);
        INSERT INTO budget_lines(account_no,period,amount) VALUES('I','2026-10',100),('X','2026-10',40),('I','2026-11',999);").unwrap();
        let tx = db.transaction().unwrap();
        let d = snapshot(&tx, "2026-10-21").unwrap();
        let b = find(&d, "budget");
        assert_eq!(value(b, "Budget"), "140,00 kr.");
        assert_eq!(value(b, "Faktisk"), "140,03 kr.");
        assert_eq!(value(b, "Samlet gunstig afvigelse"), "19,99 kr.");
        assert_eq!(value(find(&d, "vat"), "Periode fra"), "2025-10-01");
        assert_eq!(value(find(&d, "vat"), "Periodestatus"), "open");
    }
    #[test]
    fn accrual_due_uses_canonical_schedule_and_current_recognition_truth() {
        use serde_json::json;
        let mut db = fixture();
        let tx = db.transaction().unwrap();
        tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('P','Synthetic prepaid','asset','debit'),('E','Synthetic expense','expense','debit'),('B','Synthetic bank','asset','debit');").unwrap();
        let meta=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-01-01","invoiceNo":"SYN-ACCRUAL","deliveryDescription":"Synthetic service","amountIncVat":1000,"vatAmount":0,"sender":{"name":"Synthetic Supplier","address":"Synthetic2","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic Company","address":"Synthetic1","vatOrCvr":"DK87654321"}})).unwrap();
        let doc = crate::documents::register(
            &tx,
            crate::documents::Original {
                filename: "synthetic.txt",
                path: "synthetic.txt",
                hash: &"a".repeat(64),
                mime: "text/plain",
            },
            &meta,
            "synthetic",
            "2026-10-21",
        )
        .unwrap()["document"]["id"]
            .as_i64()
            .unwrap();
        let input=serde_json::from_value(json!({"accrualType":"prepaid_expense","description":"Synthetic service","totalAmount":1000,"recognitionPeriods":3,"firstRecognitionDate":"2026-01-31","registrationDate":"2026-01-01","periodStepMonths":1,"balanceAccountNo":"P","resultAccountNo":"E","settlementAccountNo":"B","documentId":doc,"confirm":true})).unwrap();
        let registered =
            crate::accruals::register(&tx, &input, "synthetic", Date::parse("2026-10-21").unwrap())
                .unwrap();
        let before = snapshot(&tx, "2026-02-28").unwrap();
        let a = find(&before, "accruals");
        assert_eq!(value(a, "Forfaldne indregninger"), "2");
        assert_eq!(value(a, "Forfaldent beløb"), "666,66 kr.");
        assert_eq!(value(a, "Resterende eksponering"), "1000,00 kr.");
        let recognition = serde_json::from_value(
            json!({"periodIndex":1,"transactionDate":"2026-03-01","confirm":true}),
        )
        .unwrap();
        crate::accruals::recognize(
            &tx,
            registered["accrualId"].as_i64().unwrap(),
            &recognition,
            "synthetic",
            Date::parse("2026-10-21").unwrap(),
        )
        .unwrap();
        let after = snapshot(&tx, "2026-02-28").unwrap();
        let a = find(&after, "accruals");
        assert_eq!(value(a, "Forfaldne indregninger"), "1");
        assert_eq!(value(a, "Forfaldent beløb"), "333,33 kr.");
        assert!(a.basis.contains("Nuværende"));
    }
}
