//! Read-only annual preparation. Period review/close/reopen stays in the existing period UI.
use crate::{Session, readable};
use gpui_kit::{
    component::{
        Disableable,
        button::Button,
        select::{SearchableVec, Select, SelectState},
    },
    *,
};
use rentemester_client::{Report, valid_annual_bounds};
use serde_json::Value;
use std::sync::Arc;
type Choices = SelectState<SearchableVec<SharedString>>;
#[derive(Clone, Debug, PartialEq, Eq)]
struct Year {
    label: String,
    start: String,
    end: String,
}
impl Year {
    fn caption(&self) -> String {
        format!("{} · {} — {}", self.label, self.start, self.end)
    }
}
fn text(value: &Value, key: &str) -> Result<String, String> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Serverens rapport mangler {key}. Genlæs grundlaget."))
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    value[key]
        .as_array()
        .ok_or_else(|| format!("Serverens rapport mangler {key}. Genlæs grundlaget."))
}
fn years(value: &Value, company: &str) -> Result<Vec<Year>, String> {
    if value["ok"] != true || value["journal"]["slug"] != company {
        return Err("Serveren bekræftede ikke den valgte virksomhed.".into());
    }
    let mut years = Vec::new();
    let mut labels = std::collections::BTreeSet::new();
    for row in array(&value["journal"], "fiscalYears")? {
        if row["source"] != "live" || row["selectable"] == false {
            continue;
        }
        let year = Year {
            label: text(row, "label")?,
            start: text(row, "start")?,
            end: text(row, "end")?,
        };
        if !valid_annual_bounds(&year.start, &year.end)
            || year.label.len() > 128
            || year.label.chars().any(char::is_control)
            || !labels.insert(year.label.clone())
        {
            return Err("Serverens aktive regnskabsår kunne ikke skelnes sikkert. Ingen periode udledes lokalt.".into());
        }
        years.push(year);
    }
    if years.is_empty() {
        return Err("Ingen aktive regnskabsår med bekræftede start- og slutdatoer blev returneret. Arkivår bruges ikke som aktivt årsrapportgrundlag.".into());
    }
    Ok(years)
}
/// Keep exact decimal strings and already received JSON numbers; never convert through f64.
fn amount(value: &Value, key: &str, currency: &str) -> Result<String, String> {
    let raw = match value.get(key) {
        Some(Value::String(s)) if decimal(s) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Null) => return Ok("Ikke tilgængeligt".into()),
        _ => return Err(format!("Serveren bekræftede ikke beløbet {key}.")),
    };
    Ok(format!("{raw} {currency}"))
}
fn decimal(value: &str) -> bool {
    let value = value.strip_prefix('-').unwrap_or(value);
    let Some((whole, cents)) = value.split_once('.') else {
        return false;
    };
    !whole.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit())
        && cents.len() == 2
        && cents.bytes().all(|b| b.is_ascii_digit())
}
struct Panel {
    title: String,
    rows: Vec<(String, String)>,
    notes: Vec<String>,
}
impl Panel {
    fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            rows: vec![],
            notes: vec![],
        }
    }
}
struct Prepared {
    year: Year,
    panels: Vec<Panel>,
    ready: bool,
}
fn section(title: &str, section: &Value, currency: &str) -> Result<Panel, String> {
    let mut panel = Panel::new(title);
    for line in array(section, "lines")? {
        panel.rows.push((
            format!("{} · {}", text(line, "accountNo")?, text(line, "name")?),
            amount(line, "amount", currency)?,
        ));
    }
    panel.rows.push((
        "I alt · serverens beregning".into(),
        amount(section, "total", currency)?,
    ));
    Ok(panel)
}
fn parse(
    year: Year,
    company: &str,
    annual: &Value,
    obligations: &Value,
    journal_response: &Value,
) -> Result<Prepared, String> {
    let body = &annual["annualReport"];
    let report = &body["report"];
    let owed = &obligations["obligations"];
    let journal = &journal_response["journal"];
    if journal_response["ok"] != true
        || journal["slug"] != company
        || journal["selectedYear"] != year.label
        || journal["periodStart"] != year.start
        || journal["periodEnd"] != year.end
        || journal["source"] != "live"
        || annual["ok"] != true
        || body["slug"] != company
        || body["fiscalYearStart"] != year.start
        || body["fiscalYearEnd"] != year.end
        || report["fiscalYearStart"] != year.start
        || report["fiscalYearEnd"] != year.end
        || !report["ok"].is_boolean()
        || obligations["ok"] != true
        || owed["slug"] != company
        || owed["selectedYear"] != year.label
        || owed["periodStart"] != year.start
        || owed["periodEnd"] != year.end
        || [
            "name",
            "cvr",
            "country",
            "currency",
            "fiscalYearStartMonth",
            "fiscalYearLabelStrategy",
        ]
        .iter()
        .any(|key| {
            owed["company"].get(*key).is_none() || owed["company"][*key] != report["company"][*key]
        })
    {
        return Err("Årsrapport og forpligtelser bekræftede ikke samme virksomhed og regnskabsår. Genlæs; ingen tal genbruges fra en anden periode.".into());
    }
    let currency = text(&report["company"], "currency")?;
    let mut panels = Vec::new();
    let mut identity = Panel::new(format!("Årsrapport · {}", year.label));
    identity
        .rows
        .push(("Virksomhed".into(), text(&report["company"], "name")?));
    identity.rows.push((
        "CVR".into(),
        report["company"]["cvr"]
            .as_str()
            .unwrap_or("Ikke registreret")
            .to_owned(),
    ));
    identity.rows.push((
        "Regnskabsår".into(),
        format!("{} — {}", year.start, year.end),
    ));
    identity.rows.push(("Valuta".into(), currency.clone()));
    identity.notes.push("Materialet forbereder en årsrapport. Ejer eller revisor gennemgår og er ansvarlig for den eksterne indberetning. Der sendes eller godkendes intet her.".into());
    panels.push(identity);
    let mut coverage = Panel::new("Aktivt regnskabsgrundlag og dækning");
    let basis = &journal["openingBalanceBasis"];
    if let Some(date) = basis["cutOverDate"].as_str() {
        coverage.notes.push(format!("Aktivt regnskab fra udtrykkelig åbningsbalance pr. {date}. Arkivbeløb er ikke medregnet."));
        if let Some(id) = basis["openingJournalEntryId"].as_i64() {
            coverage
                .rows
                .push(("Åbningsbalance · journalpost".into(), id.to_string()));
        }
    } else if basis["basis"] == "live" && basis.get("cutOverDate") == Some(&Value::Null) {
        coverage.notes.push(
            "Kun aktive posteringer. Arkivhistorik er ikke overført som åbningsbalance.".into(),
        );
    } else {
        coverage
            .notes
            .push("Oplysning om åbningsbalance er ikke tilgængelig i serverens grundlag.".into());
    }
    let scope = &journal["liveCoverage"];
    match scope["status"].as_str() {
        Some("partial") => coverage.notes.push(format!("Delvis aktiv dækning: {} til {} har kun aktive posteringer fra {}. Tallene dokumenterer ikke hele året før skæringsdatoen.", text(scope,"periodStart")?, text(scope,"periodEnd")?,text(scope,"cutOverDate")?)),
        Some("complete") => coverage.notes.push("Serveren angiver aktiv dækning af den valgte periode. Dette er ikke en erklæring om fuldstændig årsrapport eller indberetning.".into()),
        _ => coverage.notes.push("Serveren har ikke bekræftet aktiv dækning af hele det valgte regnskabsår.".into()),
    }
    panels.push(coverage);
    let mut tasks = Panel::new("Forpligtelser og næste manuelle handling");
    tasks.notes.push(format!(
        "Serverens opgørelsesdato: {}",
        text(owed, "asOfDate")?
    ));
    tasks.rows.push((
        "Samlet beløb i serverens grundlag".into(),
        amount(owed, "totalOwed", &currency)?,
    ));
    tasks.notes.push("Beløbet er ikke en bekræftet betalingsliste. Bogført gæld og beregnet moms kan have ukendt betalingsstatus.".into());
    if owed["coverage"]["complete"] != true {
        tasks.notes.push(
            "Opgørelsen dækker ikke alle mulige forpligtelser. Læs begrænsningerne nedenfor."
                .into(),
        );
    }
    for note in array(&owed["coverage"], "notes")? {
        tasks.notes.push(
            note.as_str()
                .ok_or("Serverens dækningsoplysninger kunne ikke læses")?
                .to_owned(),
        );
    }
    for row in array(owed, "obligations")? {
        let label = text(row, "label")?;
        let due = row["dueDate"]
            .as_str()
            .map(|date| format!("Frist: {date}"))
            .unwrap_or_else(|| "Frist ikke fastlagt".into());
        let value = if row["kind"] == "annual-report" {
            "Indberetning · ikke gæld".into()
        } else {
            amount(row, "amount", &currency)?
        };
        tasks.rows.push((format!("{label} · {due}"), value));
        if row["provisional"] == true {
            tasks.notes.push(format!("{label}: Foreløbigt grundlag."));
        }
        if row["settlementStatus"] == "unknown" {
            tasks.notes.push(format!("{label}: Betalingsstatus er ukendt. Registreret indberetning er ikke dokumentation for betaling."));
        }
        if let Some(note) = row["note"].as_str() {
            tasks.notes.push(format!("{label}: {note}"));
        }
        if let Some(status) = row["periodStatus"].as_str() {
            tasks.notes.push(format!(
                "{label}: {}",
                match status {
                    "open" => "Åben periode",
                    "closed" => "Lukket periode",
                    "reported" => "Ekstern indberetning registreret",
                    _ => "Periodestatus ikke genkendt",
                }
            ));
        }
    }
    panels.push(tasks);
    let ready = report["ok"] == true;
    let errors = array(report, "errors")?;
    if !ready {
        let mut blockers = Panel::new("Årsrapporten kan endnu ikke forberedes");
        for error in errors {
            blockers.notes.push(
                error
                    .as_str()
                    .ok_or("Serverens blokeringer kunne ikke læses")?
                    .to_owned(),
            );
        }
        blockers.notes.push("Luk denne side og vælg Virksomhedsopsætning for at rette fx CVR. Vælg Moms og perioder for at gennemgå og lukke det valgte regnskabsår. Løs blokerende kontroller dér, og vend tilbage for at forberede årsrapporten igen.".into());
        panels.push(blockers);
        return Ok(Prepared {
            year,
            panels,
            ready,
        });
    }
    if !errors.is_empty() {
        return Err(
            "Serveren returnerede både en klar årsrapport og uafklarede fejl. Genlæs grundlaget."
                .into(),
        );
    }
    let pl = &report["profitAndLoss"];
    let balance = &report["balanceSheet"];
    panels.push(section("Indtægter", &pl["income"], &currency)?);
    panels.push(section("Omkostninger", &pl["expense"], &currency)?);
    let mut result = Panel::new("Årets resultat");
    result.rows.push((
        "Resultat · serverens beregning".into(),
        amount(pl, "result", &currency)?,
    ));
    panels.push(result);
    for (title, key) in [
        ("Aktiver", "assets"),
        ("Gæld", "liabilities"),
        ("Egenkapital", "equity"),
    ] {
        panels.push(section(title, &balance[key], &currency)?);
    }
    let mut totals = Panel::new(format!("Balance ultimo {}", year.end));
    for (label, key) in [
        ("Aktiver i alt", "totalAssets"),
        ("Gæld og egenkapital i alt", "totalLiabilitiesAndEquity"),
        ("Periodens resultat", "periodResult"),
    ] {
        totals
            .rows
            .push((label.into(), amount(balance, key, &currency)?));
    }
    totals.notes.push(
        match balance["balanced"].as_bool() {
            Some(true) => "Serveren bekræfter, at balancen stemmer.",
            Some(false) => "Serveren har ikke bekræftet, at balancen stemmer.",
            None => return Err("Serverens balancestatus mangler.".into()),
        }
        .into(),
    );
    panels.push(totals);
    let comparison = &report["comparison"];
    let mut prior = Panel::new(format!(
        "Sammenligning · {} — {}",
        text(comparison, "fiscalYearStart")?,
        text(comparison, "fiscalYearEnd")?
    ));
    match comparison["available"].as_bool() {
        Some(true) => {
            for (label, key) in [
                ("Indtægter i alt", "totalIncome"),
                ("Omkostninger i alt", "totalExpense"),
                ("Årets resultat", "aretsResultat"),
                ("Aktiver i alt", "totalAssets"),
                ("Gæld og egenkapital i alt", "totalLiabilitiesAndEquity"),
                ("Egenkapital i alt", "equity"),
            ] {
                prior
                    .rows
                    .push((label.into(), amount(comparison, key, &currency)?));
            }
        }
        Some(false) => prior.notes.push(
            "Serveren har ikke markeret sammenligningstal som tilgængelige. De vises ikke som nul."
                .into(),
        ),
        None => return Err("Serverens status for sammenligningstal mangler.".into()),
    };
    panels.push(prior);
    for note in array(report, "notes")? {
        let mut panel = Panel::new(text(note, "title")?);
        panel.notes.push(text(note, "body")?);
        if note["placeholder"] == true {
            panel
                .notes
                .push("Skabelon · udfyldes og gennemgås af ejer eller revisor.".into());
        }
        panels.push(panel);
    }
    let management = &report["ledelsespategning"];
    let mut panel = Panel::new("Ledelsespåtegning");
    panel.notes.push(text(management, "text")?);
    if management["placeholder"] == true {
        panel.notes.push("Skabelon · ledelsen indsætter dato, sted og underskrifter. Dette er ikke en underskrevet eller indberettet årsrapport.".into());
    }
    panels.push(panel);
    if let Some(disclaimer) = report["disclaimer"].as_str() {
        let mut notice = Panel::new("Ansvar og videre arbejde");
        notice.notes.push(disclaimer.into());
        panels.push(notice);
    }
    Ok(Prepared {
        year,
        panels,
        ready,
    })
}
pub struct Annual {
    session: Arc<Session>,
    company: String,
    entry_focus: FocusHandle,
    entry_focus_requested: bool,
    year: Entity<Choices>,
    years: Vec<Year>,
    data: Option<Prepared>,
    busy: bool,
    status: String,
}
impl Annual {
    pub fn new(
        session: Arc<Session>,
        company: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let year = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(Vec::<SharedString>::new()),
                None,
                window,
                cx,
            )
        });
        let mut this = Self {
            session,
            company,
            entry_focus: cx.focus_handle(),
            entry_focus_requested: false,
            year,
            years: vec![],
            data: None,
            busy: false,
            status: String::new(),
        };
        this.load(window, cx);
        this
    }
    pub fn focus_entry(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.entry_focus_requested = true;
        cx.notify();
    }
    fn chosen(&self, cx: &App) -> Option<Year> {
        self.year.read(cx).selected_value().and_then(|selected| {
            self.years
                .iter()
                .find(|year| year.caption() == selected.as_ref())
                .cloned()
        })
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.data = None;
        self.years.clear();
        self.status = "Henter aktive regnskabsår fra serverens journal…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let task = cx.background_executor().spawn(async move {
            let response = session
                .runtime
                .block_on(session.client.report_source(
                    &company,
                    Report::Journal,
                    None,
                    None,
                    Some("live"),
                ))
                .map_err(readable)?;
            years(&response, &company)
        });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,window,cx|{this.busy=false;match result{Ok(years)=>{this.years=years;let labels=SearchableVec::new(this.years.iter().map(|year|SharedString::from(year.caption())).collect::<Vec<_>>());this.year.update(cx,|state,cx|state.set_items(labels,window,cx));this.status="Vælg et aktivt regnskabsår. Datoer, tal og forpligtelser kommer fra serveren; der lukkes eller indberettes intet her.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn prepare(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(year) = self.chosen(cx) else {
            self.status = "Vælg et aktivt regnskabsår fra serverens liste.".into();
            cx.notify();
            return;
        };
        self.busy = true;
        self.data = None;
        self.status = "Henter forpligtelser og forbereder årsrapportens aktuelle grundlag…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let task = cx.background_executor().spawn(async move {
            let obligations = session
                .runtime
                .block_on(session.client.obligations(&company, Some(&year.label)))
                .map_err(readable)?;
            let journal = session
                .runtime
                .block_on(session.client.report_source(
                    &company,
                    Report::Journal,
                    Some(&year.label),
                    None,
                    Some("live"),
                ))
                .map_err(readable)?;
            let annual = session
                .runtime
                .block_on(
                    session
                        .client
                        .annual_report(&company, &year.start, &year.end),
                )
                .map_err(readable)?;
            parse(year, &company, &annual, &obligations, &journal)
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok(data)=>{this.status=if data.ready{"Årsrapportens grundlag er forberedt. Noter og ledelsespåtegning kræver stadig ejerens eller revisors gennemgang; intet er indberettet."}else{"Serveren returnerede forudsætninger, der skal rettes før årsrapporten kan forberedes."}.into();this.data=Some(data);},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
}
impl Render for Annual {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.entry_focus_requested && !self.busy {
            self.entry_focus_requested = false;
            let focus = self.entry_focus.clone();
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        }
        let mut view=div().flex().flex_col().gap_3().child(div().text_xl().child("Årsrapport og forpligtelser"))
            .child(format!("Virksomhed: {}",self.company)).child(self.status.clone())
            .child(Button::new("annual-years").track_focus(&self.entry_focus).label("Genlæs aktive regnskabsår").disabled(self.busy).on_click(cx.listener(|this,_,window,cx|this.load(window,cx))))
            .child(Select::new(&self.year).accessibility_label("Aktivt regnskabsår til årsrapport").placeholder("Vælg regnskabsår og bekræftede datoer").disabled(self.busy||self.years.is_empty()))
            .child(Button::new("annual-prepare").label("Forbered eller opdater årsrapport og forpligtelser").disabled(self.busy||self.chosen(cx).is_none()).on_click(cx.listener(|this,_,_,cx|this.prepare(cx))))
            .child("Luk denne side og vælg Moms og perioder for separat gennemgang, lukning eller genåbning af regnskabsåret. Vend derefter tilbage og opdater rapporten. Ekstern indberetning foretages af ejer eller revisor.");
        let Some(data) = &self.data else {
            return view;
        };
        if self.chosen(cx).as_ref() != Some(&data.year) {
            return view.child("Årsvalget er ændret. Hent det nye grundlag, før der vises tal for dette regnskabsår.");
        }
        for panel in &data.panels {
            let mut block = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().text_lg().child(panel.title.clone()));
            for (label, value) in &panel.rows {
                block = block.child(
                    div()
                        .flex()
                        .justify_between()
                        .gap_4()
                        .child(label.clone())
                        .child(value.clone()),
                );
            }
            for note in &panel.notes {
                block = block.child(note.clone());
            }
            view = view.child(block);
        }
        view
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture(ready: bool) -> (Year, Value, Value) {
        let year = Year {
            label: "2025/26".into(),
            start: "2025-07-01".into(),
            end: "2026-06-30".into(),
        };
        let company = json!({"name":"Synthetic Annual","cvr":"DK11223344","country":"DK","currency":"DKK","fiscalYearStartMonth":7,"fiscalYearLabelStrategy":"span"});
        let zero = json!({"total":"0.00","lines":[]});
        let annual = json!({"ok":true,"annualReport":{"slug":"synthetic","fiscalYearStart":year.start,"fiscalYearEnd":year.end,"report":{"ok":ready,"fiscalYearStart":year.start,"fiscalYearEnd":year.end,"company":company,"errors":if ready{vec![]}else{vec!["Regnskabsåret er ikke lukket"]},"profitAndLoss":{"income":zero,"expense":zero,"result":"0.00"},"balanceSheet":{"assets":zero,"liabilities":zero,"equity":zero,"totalAssets":"0.00","totalLiabilitiesAndEquity":"0.00","periodResult":"0.00","balanced":true},"comparison":{"available":false,"fiscalYearStart":"2024-07-01","fiscalYearEnd":"2025-06-30","totalIncome":0,"totalExpense":0,"aretsResultat":0,"totalAssets":0,"totalLiabilitiesAndEquity":0,"equity":0},"notes":[{"title":"Regnskabspraksis","body":"Udfyldes manuelt","placeholder":true}],"ledelsespategning":{"text":"Ledelsens manuelle tekst","placeholder":true},"disclaimer":"Forberedelse, ikke indberetning"}}});
        let obligations = json!({"ok":true,"obligations":{"slug":"synthetic","selectedYear":year.label,"periodStart":year.start,"periodEnd":year.end,"company":company,"asOfDate":"2026-06-30","totalOwed":"0.00","coverage":{"complete":false,"notes":["Årsrapportens frist er ikke fastlagt"]},"obligations":[{"kind":"annual-report","label":"Årsrapport","amount":0,"dueDate":null,"note":"Indberetning, ikke gæld"}]}});
        (year, annual, obligations)
    }
    fn journal(year: &Year) -> Value {
        json!({"ok":true,"journal":{"slug":"synthetic","selectedYear":year.label,"periodStart":year.start,"periodEnd":year.end,"source":"live"}})
    }
    #[test]
    fn selected_coverage_is_authoritative_and_missing_is_not_complete() {
        let (year, annual, owed) = fixture(true);
        let mut response = journal(&year);
        let missing = parse(year.clone(), "synthetic", &annual, &owed, &response).unwrap();
        assert!(missing.panels.iter().any(|p| {
            p.notes
                .iter()
                .any(|n| n.contains("ikke bekræftet aktiv dækning"))
        }));
        response["journal"]["openingBalanceBasis"] = json!({"basis":"live","cutOverDate":"2025-08-01","openingJournalEntryId":7,"archiveAmountsIncluded":false});
        response["journal"]["liveCoverage"] = json!({"status":"partial","periodStart":year.start,"periodEnd":year.end,"cutOverDate":"2025-08-01"});
        let partial = parse(year.clone(), "synthetic", &annual, &owed, &response).unwrap();
        assert!(partial.panels.iter().any(|p| {
            p.notes
                .iter()
                .any(|n| n.contains("dokumenterer ikke hele året"))
        }));
        response["journal"]["selectedYear"] = json!("other");
        assert!(parse(year, "synthetic", &annual, &owed, &response).is_err());
    }
    #[test]
    fn shifted_years_are_exact_server_bounds_and_archive_is_not_a_live_choice() {
        let response = json!({"ok":true,"journal":{"slug":"synthetic","fiscalYears":[{"label":"2025/26","source":"live","start":"2025-07-01","end":"2026-06-30"},{"label":"2025/26","source":"archive","start":null,"end":null}]}});
        let result = years(&response, "synthetic").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].start, "2025-07-01");
        assert_eq!(result[0].end, "2026-06-30");
        assert!(years(&response, "other").is_err());
    }
    #[test]
    fn money_strings_are_never_reparsed_as_floats_or_defaulted_to_zero() {
        assert_eq!(
            amount(&json!({"x":"9007199254740993000.01"}), "x", "DKK").unwrap(),
            "9007199254740993000.01 DKK"
        );
        assert_eq!(
            amount(&json!({"x":null}), "x", "DKK").unwrap(),
            "Ikke tilgængeligt"
        );
        assert!(amount(&json!({}), "x", "DKK").is_err());
    }
    #[test]
    fn nested_blocker_is_valid_and_unknown_deadline_does_not_become_a_date_or_debt() {
        let (year, annual, owed) = fixture(false);
        let data = parse(year.clone(), "synthetic", &annual, &owed, &journal(&year)).unwrap();
        assert!(!data.ready);
        assert!(
            data.panels
                .iter()
                .any(|p| p.notes.iter().any(|n| n == "Regnskabsåret er ikke lukket"))
        );
        assert!(data.panels.iter().any(|p| {
            p.rows.iter().any(|(label, value)| {
                label.contains("Frist ikke fastlagt") && value == "Indberetning · ikke gæld"
            })
        }));
        assert!(!data.panels.iter().any(|p| p.title == "Aktiver"));
    }
    #[test]
    fn optional_profile_details_do_not_break_scope_and_unavailable_comparisons_stay_unavailable() {
        let (year, mut annual, owed) = fixture(true);
        annual["annualReport"]["report"]["company"]["paymentTermsDays"] = json!(30);
        let data = parse(year.clone(), "synthetic", &annual, &owed, &journal(&year)).unwrap();
        assert!(data.ready);
        let prior = data
            .panels
            .iter()
            .find(|p| p.title.starts_with("Sammenligning"))
            .unwrap();
        assert!(prior.rows.is_empty());
        assert!(!prior.notes.is_empty());
        assert!(
            data.panels.iter().any(|p| p.title == "Ledelsespåtegning"
                && p.notes.iter().any(|n| n.contains("Skabelon")))
        );
        assert!(parse(year.clone(), "other", &annual, &owed, &journal(&year)).is_err());
    }
}
