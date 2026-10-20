//! Read-only financial reports; all periods, signs and totals come from the server.
use crate::{Session, readable};
use gpui_kit::{
    component::{Disableable, button::Button},
    *,
};
use rentemester_client::Report;
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Copy)]
enum Kind {
    Trial,
    Income,
    Balance,
    Journal,
}
impl Kind {
    fn contract(self) -> (&'static str, &'static str, Report) {
        match self {
            Self::Trial => ("Saldobalance", "trialBalance", Report::TrialBalance),
            Self::Income => (
                "Resultatopgørelse",
                "incomeStatement",
                Report::IncomeStatement,
            ),
            Self::Balance => ("Balance", "balance", Report::Balance),
            Self::Journal => ("Posteringer", "journal", Report::Journal),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Year {
    label: String,
    source: Option<String>,
    selectable: bool,
}
impl Year {
    fn title(&self) -> String {
        format!(
            "{} · {}",
            self.label,
            match self.source.as_deref() {
                Some("archive") => "Arkiv",
                Some("live") => "Aktivt regnskab",
                _ => "Serverens regnskab",
            }
        )
    }
}
struct Sheet {
    title: String,
    period: String,
    selected: Year,
    years: Vec<Year>,
    lines: Vec<String>,
}
fn text(v: &Value, key: &str) -> Result<String, String> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Rapporten mangler {key}"))
}
fn optional_text(v: &Value, key: &str) -> String {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or("Ikke oplyst")
        .into()
}
/// Preserve canonical decimal text. Never parse money as a float or calculate totals.
fn decimal(s: &str) -> bool {
    let value = s.strip_prefix('-').unwrap_or(s);
    let Some((whole, fraction)) = value.split_once('.') else {
        return false;
    };
    !whole.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit())
        && fraction.len() == 2
        && fraction.bytes().all(|b| b.is_ascii_digit())
}
fn amount(v: &Value, key: &str) -> Result<String, String> {
    match v.get(key) {
        Some(Value::String(s)) if decimal(s) => Ok(s.clone()),
        Some(Value::Number(n)) => Ok(n.to_string()),
        Some(Value::Null) => Ok("Ikke tilgængeligt".into()),
        _ => Err(format!("Rapporten mangler et gyldigt beløb: {key}")),
    }
}
fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    v[key]
        .as_array()
        .ok_or_else(|| format!("Rapporten mangler {key}"))
}
fn statement(lines: &mut Vec<String>, rows: &[Value]) -> Result<(), String> {
    for row in rows {
        let prior = if row.get("priorAmount") == Some(&Value::Null) {
            "—".into()
        } else {
            amount(row, "priorAmount")?
        };
        lines.push(format!(
            "{} · {}    {}    (forrige: {})",
            text(row, "accountNo")?,
            text(row, "name")?,
            amount(row, "amount")?,
            prior
        ));
    }
    Ok(())
}
fn source(v: &Value) -> Result<Option<String>, String> {
    let selected = v.get("selectedSource").or_else(|| v.get("source"));
    let source = match selected {
        None => None,
        Some(Value::String(s)) if matches!(s.as_str(), "live" | "archive") => Some(s.clone()),
        _ => return Err("Rapportens kilde er ugyldig".into()),
    };
    if let Some(other) = v.get("source")
        && source.as_deref() != other.as_str()
    {
        return Err("Rapportens kilder er modstridende".into());
    }
    Ok(source)
}
fn notices(v: &Value, lines: &mut Vec<String>) -> Result<(), String> {
    let opening = &v["openingBalanceBasis"];
    if let Some(date) = opening["cutOverDate"].as_str() {
        lines.push(format!("Aktivt regnskab fra udtrykkelig åbningsbalance pr. {date}. Arkivbeløb er ikke medregnet."));
        if let Some(id) = opening["openingJournalEntryId"].as_i64() {
            lines.push(format!(
                "Åbningsbalance · journalpost {id}. Se Posteringer i det aktive regnskab."
            ));
        }
    } else if v["ledgerBasis"] == "native_ledger_only" {
        lines.push(
            "Kun aktive posteringer. Arkivhistorik er ikke overført som åbningsbalance.".into(),
        );
    }
    let coverage = &v["liveCoverage"];
    if coverage["status"] == "partial" {
        lines.push(format!("Delvis aktiv dækning: {} til {} har kun aktive posteringer fra {}. Tallene dokumenterer ikke hele året før skæringsdatoen.", optional_text(coverage,"periodStart"),optional_text(coverage,"periodEnd"),optional_text(coverage,"cutOverDate")));
    }
    if v["comparison"]["status"] == "unavailable" {
        lines.push(format!(
            "Sammenligningstal er ikke tilgængelige: {}.",
            if v["comparison"]["reason"] == "prior_native_year_not_present" {
                "Der findes ikke et foregående bogført år i det aktive regnskab"
            } else {
                "Der er ikke dokumenteret en sammenlignelig periode"
            }
        ));
    }
    if v["source"] == "archive" || v["selectedSource"] == "archive" {
        let evidence = &v["evidence"];
        lines.push("Arkiveret afslutningssnapshot. Regnskabsårets grænser og afslutningsdato er ikke oplyst. Virksomhedsoplysninger og kontoklassifikation er fra den aktuelle opsætning.".into());
        lines.push(format!("Historisk erklæret valuta {} · observerede posteringsdatoer {} til {} (ikke regnskabsårets grænser).",text(v,"declaredCurrency")?,optional_text(evidence,"observedPostingStart"),optional_text(evidence,"observedPostingEnd")));
        lines.push(format!(
            "Original arkivkilde · import {} · SHA256 {} · parser {}",
            evidence["importId"],
            text(evidence, "sourceHash")?,
            text(evidence, "parserContract")?
        ));
    }
    let complete = &v["completeness"];
    if let Some(status) = complete["status"].as_str() {
        lines.push(format!(
            "Rapportgrundlag: {}",
            match status {
                "ready" => "tilgængeligt",
                "incomplete" => "ufuldstændigt",
                "unavailable" => "ikke tilgængeligt",
                _ => return Err("Ukendt rapportgrundlag".into()),
            }
        ));
        for reason in array(complete, "reasons")? {
            lines.push(match reason.as_str() {
                Some("missing_closing_balances") => {
                    "SaldoBalance-filen mangler; afslutningssaldi kan ikke opgøres.".into()
                }
                Some("empty_closing_balances") => {
                    "SaldoBalance-filen har ingen afslutningssaldi.".into()
                }
                Some("unknown_account_classification") => {
                    "Konti mangler gyldig klassifikation i den aktuelle kontoplan.".into()
                }
                Some(value) => format!("Kildebegrænsning: {value}"),
                None => return Err("Ugyldig kildebegrænsning".into()),
            });
        }
        for account in array(complete, "unclassifiedAccounts")? {
            lines.push(format!(
                "Uklassificeret konto {} · {} · {}",
                text(account, "accountNo")?,
                text(account, "name")?,
                amount(account, "amount")?
            ));
            for row in array(account, "sourceRows")? {
                lines.push(format!(
                    "Original kilderække {} · originalbeløb {} · rapportbeløb {}",
                    row["lineNo"],
                    text(row, "sourceAmount")?,
                    amount(row, "amount")?
                ));
            }
        }
    }
    Ok(())
}
fn sheet(
    root: Value,
    kind: Kind,
    company: &str,
    requested: Option<&Year>,
) -> Result<Sheet, String> {
    if root["ok"] != true {
        return Err("Serveren bekræftede ikke rapporten".into());
    }
    let v = &root[kind.contract().1];
    if text(v, "slug")? != company {
        return Err("Rapporten tilhører en anden virksomhed".into());
    }
    let selected = Year {
        label: text(v, "selectedYear")?,
        source: source(v)?,
        selectable: true,
    };
    if let Some(requested) = requested
        && (requested.label != selected.label
            || requested.source.is_some() && requested.source != selected.source)
    {
        return Err(
            "Serveren returnerede et andet regnskabsår eller en anden kilde. Ingen rapport vises."
                .into(),
        );
    }
    let currency = if selected.source.as_deref() == Some("archive") {
        text(v, "declaredCurrency")?
    } else {
        text(&v["company"], "currency")?
    };
    let mut lines = Vec::new();
    notices(v, &mut lines)?;
    match kind {
        Kind::Trial => {
            if v["basis"] == "closing_balance" {
                lines.push(
                    "Grundlag: arkiverede afslutningssaldi; ikke periodens bevægelser.".into(),
                );
            }
            lines.push("Konto · Navn    Debet    Kredit    Saldo".into());
            for r in array(v, "rows")? {
                lines.push(format!(
                    "{} · {}    {}    {}    {}",
                    text(r, "accountNo")?,
                    text(r, "name")?,
                    amount(r, "debit")?,
                    amount(r, "credit")?,
                    amount(r, "balance")?
                ));
                if r.get("type") == Some(&Value::Null) {
                    lines.push("Kontoklassifikation ikke oplyst.".into());
                }
                if let Some(rows) = r["sourceRows"].as_array() {
                    for row in rows {
                        lines.push(format!(
                            "Original kilderække {} · originalbeløb {} · rapportbeløb {}",
                            row["lineNo"],
                            text(row, "sourceAmount")?,
                            amount(row, "amount")?
                        ));
                    }
                }
            }
            lines.push(format!(
                "I alt · Debet {} · Kredit {}",
                amount(v, "totalDebit")?,
                amount(v, "totalCredit")?
            ));
        }
        Kind::Income => {
            lines.push("Indtægter · beløb (forrige dokumenterede regnskabsår)".into());
            statement(&mut lines, array(v, "income")?)?;
            lines.push(format!("Indtægter i alt {}", amount(v, "totalIncome")?));
            lines.push("Omkostninger".into());
            statement(&mut lines, array(v, "expense")?)?;
            lines.push(format!("Omkostninger i alt {}", amount(v, "totalExpense")?));
            lines.push(format!(
                "Resultat {} · Forrige resultat {}",
                amount(v, "result")?,
                amount(v, "priorResult")?
            ));
        }
        Kind::Balance => {
            lines.push(format!("Pr. {}", optional_text(v, "asOfDate")));
            for (key, title) in [
                ("assets", "Aktiver"),
                ("liabilities", "Forpligtelser"),
                ("equity", "Egenkapital"),
            ] {
                lines.push(title.into());
                statement(&mut lines, array(&v[key], "lines")?)?;
                lines.push(format!("{title} i alt {}", amount(&v[key], "total")?));
            }
            lines.push(format!(
                "Aktiver {} · Passiver {}",
                amount(v, "totalAssets")?,
                amount(v, "totalLiabilitiesAndEquity")?
            ));
        }
        Kind::Journal => {
            if selected.source.as_deref() == Some("archive") {
                lines.push("Originale arkivposter; ingen aktive journal-/bilagsidentiteter og ingen bogføringshandlinger.".into());
            }
            for entry in array(v, "entries")? {
                lines.push(format!(
                    "{} · {} · {}",
                    text(entry, "entryNo")?,
                    text(entry, "date")?,
                    text(entry, "text")?
                ));
                if let Some(id) = entry["sourceId"].as_str() {
                    lines.push(format!("Original kildepost {id}"));
                }
                if let Some(no) = entry["documentNo"].as_str() {
                    lines.push(format!("Bilag {no}"));
                }
                for r in array(entry, "lines")? {
                    lines.push(format!(
                        "{} · {}    Debet {} · Kredit {}",
                        text(r, "accountNo")?,
                        text(r, "accountName")?,
                        amount(r, "debit")?,
                        amount(r, "credit")?
                    ));
                    if let Some(raw) = r["sourceAmount"].as_str() {
                        lines.push(format!("Originalbeløb: {raw}"));
                    }
                    if let Some(id) = r["sourceId"].as_str() {
                        lines.push(format!("Original kilderække {id}"));
                    }
                }
                if let Some(no) = entry["reversalOfEntryNo"].as_str() {
                    lines.push(format!("Tilbagefører {no}"));
                }
                if let Some(no) = entry["reversedByEntryNo"].as_str() {
                    lines.push(format!("Tilbageført af {no}"));
                }
            }
            if array(v, "entries")?.is_empty() {
                lines.push("Ingen posteringer i perioden.".into());
            }
        }
    }
    if matches!(kind, Kind::Trial | Kind::Balance) {
        lines.push(
            match v.get("balanced") {
                Some(Value::Bool(true)) => "Stemmer ifølge serveren.",
                Some(Value::Bool(false)) => "Stemmer ikke — gennemgå kildeposteringerne.",
                Some(Value::Null) => {
                    "Balancekontrol ikke tilgængelig; manglende grundlag er ikke nul."
                }
                _ => return Err("Rapporten mangler balancekontrol".into()),
            }
            .into(),
        );
    }
    let mut years = Vec::new();
    for year in array(v, "fiscalYears")? {
        let entry = Year {
            label: text(year, "label")?,
            source: source(year)?,
            selectable: year["selectable"] != false,
        };
        if years
            .iter()
            .any(|other: &Year| other.label == entry.label && other.source == entry.source)
        {
            return Err("Dubleret regnskabsår og kilde".into());
        }
        years.push(entry);
    }
    if !years.iter().any(|year| {
        year.label == selected.label && year.source == selected.source && year.selectable
    }) {
        return Err(
            "Det valgte regnskabsår og kilde findes ikke i serverens tilgængelige regnskaber"
                .into(),
        );
    }
    Ok(Sheet {
        title: format!(
            "{} · {} · {} · {currency}",
            kind.contract().0,
            text(&v["company"], "name")?,
            selected.title()
        ),
        period: format!(
            "{} til {}",
            optional_text(v, "periodStart"),
            optional_text(v, "periodEnd")
        ),
        selected,
        years,
        lines,
    })
}

pub struct Reporting {
    session: Arc<Session>,
    company: String,
    origin: String,
    kind: Kind,
    selected: Option<Year>,
    initial_year: String,
    years: Vec<Year>,
    data: Option<Sheet>,
    busy: bool,
    status: String,
}
impl Reporting {
    pub fn new(
        session: Arc<Session>,
        company: String,
        origin: String,
        year: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            session,
            company,
            origin,
            kind: Kind::Trial,
            selected: None,
            initial_year: year,
            years: Vec::new(),
            data: None,
            busy: false,
            status: String::new(),
        };
        this.load(cx);
        this
    }
    fn load(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.data = None;
        self.status = "Henter rapport fra serveren…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let kind = self.kind;
        let requested = self.selected.clone().or_else(|| {
            let label = self.initial_year.trim();
            (!label.is_empty()).then(|| Year {
                label: label.into(),
                source: None,
                selectable: true,
            })
        });
        let task = cx.background_executor().spawn(async move {
            let value = session
                .runtime
                .block_on(session.client.report_source(
                    &company,
                    kind.contract().2,
                    requested.as_ref().map(|year| year.label.as_str()),
                    None,
                    requested.as_ref().and_then(|year| year.source.as_deref()),
                ))
                .map_err(readable)?;
            sheet(value, kind, &company, requested.as_ref())
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(data) => {
                        this.selected = Some(data.selected.clone());
                        this.years = data.years.clone();
                        this.data = Some(data);
                        this.status = "Beløb, fortegn og periode er beregnet af serveren.".into();
                    }
                    Err(error) => this.status = error,
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
impl Render for Reporting {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut view = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_xl().child("Rapporter"))
            .child(format!("{} · {}", self.company, self.origin))
            .child(self.status.clone());
        if let Some(selected) = &self.selected {
            view = view.child(format!("Valgt regnskabsår og kilde: {}", selected.title()));
        }
        let mut kinds = div().flex().flex_wrap().gap_2();
        for (id, kind) in [
            ("report-trial", Kind::Trial),
            ("report-income", Kind::Income),
            ("report-balance", Kind::Balance),
            ("report-journal", Kind::Journal),
        ] {
            kinds = kinds.child(
                Button::new(id)
                    .label(kind.contract().0)
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.kind = kind;
                        this.load(cx);
                    })),
            );
        }
        view = view.child(kinds).child(
            Button::new("report-refresh")
                .label("Hent valgt rapport og regnskabsår")
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, _, cx| this.load(cx))),
        );
        let mut years = div().flex().flex_wrap().gap_2();
        for year in &self.years {
            let year = year.clone();
            years = years.child(
                Button::new(SharedString::from(format!(
                    "report-year-{}-{}",
                    year.source.as_deref().unwrap_or("server"),
                    year.label
                )))
                .label(year.title())
                .disabled(self.busy || !year.selectable)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.selected = Some(year.clone());
                    this.load(cx);
                })),
            );
        }
        view = view.child(years);
        if let Some(data) = &self.data {
            view = view
                .child(div().text_lg().child(data.title.clone()))
                .child(data.period.clone())
                .children(data.lines.iter().map(|line| div().child(line.clone())));
        }
        view
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> Value {
        json!({"ok":true,"incomeStatement":{"slug":"synthetic","company":{"name":"Synthetic","currency":"DKK"},"selectedYear":"2026","periodStart":"2026-01-01","periodEnd":"2026-12-31","fiscalYears":[{"label":"2026"}],"income":[{"accountNo":"I","name":"Income","amount":200,"priorAmount":0}],"expense":[{"accountNo":"X","name":"Expense","amount":-5,"priorAmount":null}],"totalIncome":200,"totalExpense":-5,"result":205,"priorResult":0}})
    }
    #[::core::prelude::v1::test]
    fn preserves_server_signs_totals_and_period() {
        let report = sheet(fixture(), Kind::Income, "synthetic", None).unwrap();
        assert!(
            report
                .lines
                .contains(&"X · Expense    -5    (forrige: —)".into())
        );
        assert!(
            report
                .lines
                .contains(&"Resultat 205 · Forrige resultat 0".into())
        );
        assert_eq!(report.period, "2026-01-01 til 2026-12-31");
        assert_eq!(report.years[0].label, "2026");
    }
    #[::core::prelude::v1::test]
    fn missing_amount_does_not_become_zero() {
        let mut v = fixture();
        v["incomeStatement"]
            .as_object_mut()
            .unwrap()
            .remove("result");
        assert!(sheet(v, Kind::Income, "synthetic", None).is_err());
    }
    #[::core::prelude::v1::test]
    fn rejects_other_company_and_unconfirmed_response() {
        assert!(sheet(fixture(), Kind::Income, "other", None).is_err());
        let mut v = fixture();
        v["ok"] = json!(false);
        assert!(sheet(v, Kind::Income, "synthetic", None).is_err());
    }
    #[::core::prelude::v1::test]
    fn balance_retains_server_equity_and_failed_balance_control() {
        let mut root = fixture();
        let mut balance = root["incomeStatement"].take();
        balance["asOfDate"] = json!("2026-12-31");
        for section in ["assets", "liabilities", "equity"] {
            balance[section] = json!({"lines":[],"total":0});
        }
        balance["equity"] = json!({"lines":[{"accountNo":"—","name":"Årets resultat","amount":205,"priorAmount":null}],"total":205});
        balance["totalAssets"] = json!(0);
        balance["totalLiabilitiesAndEquity"] = json!(205);
        balance["balanced"] = json!(false);
        root["balance"] = balance;
        let report = sheet(root.clone(), Kind::Balance, "synthetic", None).unwrap();
        assert!(
            report
                .lines
                .contains(&"— · Årets resultat    205    (forrige: —)".into())
        );
        assert!(report.lines.last().unwrap().starts_with("Stemmer ikke"));
        root["balance"].as_object_mut().unwrap().remove("balanced");
        assert!(sheet(root, Kind::Balance, "synthetic", None).is_err());
    }
    #[::core::prelude::v1::test]
    fn journal_keeps_original_and_reversal_references() {
        let mut root = fixture();
        let mut journal = root["incomeStatement"].take();
        journal["entries"] = json!([{"entryNo":"2026-00002","date":"2026-10-19","text":"Synthetic correction","documentNo":"B-2","reversalOfEntryNo":"2026-00001","lines":[{"accountNo":"I","accountName":"Income","debit":200,"credit":0}]}]);
        root["journal"] = journal;
        let report = sheet(root, Kind::Journal, "synthetic", None).unwrap();
        assert!(report.lines.contains(&"Tilbagefører 2026-00001".into()));
        assert!(
            report
                .lines
                .contains(&"I · Income    Debet 200 · Kredit 0".into())
        );
    }
    fn archive() -> Value {
        let mut root = fixture();
        let v = &mut root["incomeStatement"];
        v["source"] = json!("archive");
        v["selectedSource"] = json!("archive");
        v["declaredCurrency"] = json!("EUR");
        v["fiscalYears"] = json!([{"label":"2026","source":"live"},{"label":"2026","source":"archive","start":null,"end":null}]);
        v["periodStart"] = Value::Null;
        v["periodEnd"] = Value::Null;
        v["priorResult"] = Value::Null;
        v["evidence"] = json!({"importId":4,"sourceHash":"a".repeat(64),"parserContract":"synthetic-contract","observedPostingStart":"2026-03-04","observedPostingEnd":"2026-06-05"});
        v["completeness"] = json!({"status":"ready","reasons":[],"unclassifiedAccounts":[]});
        v["comparison"] = json!({"status":"unavailable","reason":"archive_period_bounds_unknown"});
        root
    }
    #[::core::prelude::v1::test]
    fn same_label_sources_remain_distinct_and_response_cannot_fallback() {
        let root = archive();
        let report = sheet(root.clone(), Kind::Income, "synthetic", None).unwrap();
        assert_eq!(report.years.len(), 2);
        assert_ne!(report.years[0], report.years[1]);
        assert_ne!(report.years[0].title(), report.years[1].title());
        assert!(
            sheet(
                root.clone(),
                Kind::Income,
                "synthetic",
                Some(&report.years[0])
            )
            .is_err()
        );
        assert!(
            sheet(
                root.clone(),
                Kind::Income,
                "synthetic",
                Some(&report.years[1])
            )
            .is_ok()
        );
        let mut different = report.years[1].clone();
        different.label = "2025".into();
        assert!(sheet(root, Kind::Income, "synthetic", Some(&different)).is_err());
    }
    #[::core::prelude::v1::test]
    fn exact_large_decimals_null_prior_bounds_and_immutable_currency_are_preserved() {
        let mut root = archive();
        root["incomeStatement"]["result"] = json!("100000000000000.01");
        root["incomeStatement"]["expense"][0]["amount"] = json!("-0.01");
        let report = sheet(root, Kind::Income, "synthetic", None).unwrap();
        assert!(report.title.ends_with("EUR"));
        assert_eq!(report.period, "Ikke oplyst til Ikke oplyst");
        assert!(
            report
                .lines
                .iter()
                .any(|line| line
                    == "Resultat 100000000000000.01 · Forrige resultat Ikke tilgængeligt")
        );
        assert!(report.lines.iter().any(|line| line.contains("-0.01")));
        assert!(
            report
                .lines
                .iter()
                .any(|line| line.contains("ikke regnskabsårets grænser"))
        );
        for invalid in [
            json!("1e20"),
            json!("NaN"),
            json!("12.345"),
            json!({}),
            json!(true),
        ] {
            assert!(amount(&json!({"amount":invalid}), "amount").is_err());
        }
    }
    #[::core::prelude::v1::test]
    fn unavailable_closing_balances_and_classification_are_not_zero_or_green() {
        let root = archive();
        let mut v = root["incomeStatement"].clone();
        v["rows"] = json!([]);
        v["totalDebit"] = Value::Null;
        v["totalCredit"] = Value::Null;
        v["balanced"] = Value::Null;
        v["completeness"] = json!({"status":"unavailable","reasons":["missing_closing_balances"],"unclassifiedAccounts":[]});
        let report = sheet(
            json!({"ok":true,"trialBalance":v}),
            Kind::Trial,
            "synthetic",
            None,
        )
        .unwrap();
        assert!(
            report
                .lines
                .iter()
                .any(|line| line.contains("SaldoBalance-filen mangler"))
        );
        assert!(report.lines.last().unwrap().contains("ikke tilgængelig"));
        let mut v = archive()["incomeStatement"].clone();
        v["result"] = Value::Null;
        v["completeness"] = json!({"status":"incomplete","reasons":["unknown_account_classification"],"unclassifiedAccounts":[{"accountNo":"UNKNOWN","name":"Original account","amount":"12.34","sourceRows":[{"lineNo":8,"sourceAmount":"12.345678","amount":"12.34"}]}]});
        let report = sheet(
            json!({"ok":true,"incomeStatement":v}),
            Kind::Income,
            "synthetic",
            None,
        )
        .unwrap();
        assert!(
            report
                .lines
                .iter()
                .any(|line| line.contains("Uklassificeret konto UNKNOWN"))
        );
        assert!(report.lines.iter().any(|line| line.contains("12.345678")));
        assert!(
            report
                .lines
                .iter()
                .any(|line| line.starts_with("Resultat Ikke tilgængeligt"))
        );
    }
    #[::core::prelude::v1::test]
    fn cutover_partial_coverage_and_opening_journal_are_canonical_not_full_year() {
        let mut root = fixture();
        let v = &mut root["incomeStatement"];
        v["openingBalanceBasis"] = json!({"basis":"live","cutOverDate":"2026-07-01","openingJournalEntryId":42,"archiveAmountsIncluded":false});
        v["liveCoverage"] = json!({"status":"partial","reason":"native_history_begins_at_explicit_cutover","periodStart":"2026-01-01","periodEnd":"2026-12-31","cutOverDate":"2026-07-01"});
        let report = sheet(root, Kind::Income, "synthetic", None).unwrap();
        assert!(
            report
                .lines
                .iter()
                .any(|line| line.contains("journalpost 42"))
        );
        assert!(
            report
                .lines
                .iter()
                .any(|line| line.contains("Arkivbeløb er ikke medregnet"))
        );
        assert!(
            report
                .lines
                .iter()
                .any(|line| line.contains("ikke hele året før skæringsdatoen"))
        );
    }
    #[::core::prelude::v1::test]
    fn archived_journal_retains_raw_six_decimal_source_and_has_no_native_identity() {
        let mut v = archive()["incomeStatement"].clone();
        v["entries"] = json!([{"id":null,"sourceId":"archive:hash:2026:voucher:MICRO","entryNo":"MICRO","date":"2026-03-04","text":"Original export","lines":[{"accountNo":"X","accountName":"Expense","debit":"1.23","credit":"0.00","sourceAmount":"1.234567","sourceId":"archive:hash:2026:posting:8"}]}]);
        let report = sheet(
            json!({"ok":true,"journal":v}),
            Kind::Journal,
            "synthetic",
            None,
        )
        .unwrap();
        assert!(
            report
                .lines
                .iter()
                .any(|line| line == "Originalbeløb: 1.234567")
        );
        assert!(
            report
                .lines
                .iter()
                .any(|line| line.contains("ingen aktive journal-/bilagsidentiteter"))
        );
    }
    #[::core::prelude::v1::test]
    fn unknown_conflicting_or_unavailable_source_descriptor_is_refused() {
        let mut root = archive();
        root["incomeStatement"]["selectedSource"] = json!("live");
        assert!(sheet(root, Kind::Income, "synthetic", None).is_err());
        let mut root = archive();
        root["incomeStatement"]["fiscalYears"][1]["selectable"] = json!(false);
        assert!(sheet(root, Kind::Income, "synthetic", None).is_err());
        let mut root = archive();
        root["incomeStatement"]["source"] = json!("other");
        assert!(sheet(root, Kind::Income, "synthetic", None).is_err());
    }
}
