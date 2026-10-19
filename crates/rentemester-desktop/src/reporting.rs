//! Read-only financial reports; all periods, signs and totals come from the server.
use crate::{Session, readable};
use gpui_kit::{
    component::{
        Disableable,
        button::Button,
        input::{Input, InputState},
    },
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
struct Sheet {
    title: String,
    period: String,
    years: Vec<String>,
    lines: Vec<String>,
}
fn text(v: &Value, key: &str) -> Result<String, String> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Rapporten mangler {key}"))
}
fn amount(v: &Value, key: &str) -> Result<String, String> {
    v[key]
        .as_number()
        .map(ToString::to_string)
        .ok_or_else(|| format!("Rapporten mangler beløbet {key}"))
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
fn sheet(root: Value, kind: Kind, company: &str) -> Result<Sheet, String> {
    if root["ok"] != true {
        return Err("Serveren bekræftede ikke rapporten".into());
    }
    let v = &root[kind.contract().1];
    if text(v, "slug")? != company {
        return Err("Rapporten tilhører en anden virksomhed".into());
    }
    let currency = text(&v["company"], "currency")?;
    let mut lines = Vec::new();
    match kind {
        Kind::Trial => {
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
            }
            lines.push(format!(
                "I alt · Debet {} · Kredit {}",
                amount(v, "totalDebit")?,
                amount(v, "totalCredit")?
            ));
        }
        Kind::Income => {
            lines.push("Indtægter · beløb (forrige regnskabsår)".into());
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
            lines.push(format!("Pr. {}", text(v, "asOfDate")?));
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
            for entry in array(v, "entries")? {
                lines.push(format!(
                    "{} · {} · {}",
                    text(entry, "entryNo")?,
                    text(entry, "date")?,
                    text(entry, "text")?
                ));
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
            match v["balanced"]
                .as_bool()
                .ok_or("Rapporten mangler balancekontrol")?
            {
                true => "Stemmer ifølge serveren.",
                false => "Stemmer ikke — gennemgå posteringerne på serveren.",
            }
            .into(),
        );
    }
    let years = array(v, "fiscalYears")?
        .iter()
        .map(|year| text(year, "label"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Sheet {
        title: format!(
            "{} · {} · {} · {currency}",
            kind.contract().0,
            text(&v["company"], "name")?,
            text(v, "selectedYear")?
        ),
        period: format!("{} til {}", text(v, "periodStart")?, text(v, "periodEnd")?),
        years,
        lines,
    })
}

pub struct Reporting {
    session: Arc<Session>,
    company: String,
    origin: String,
    kind: Kind,
    year: Entity<InputState>,
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let year = cx.new(|cx| {
            let mut field = InputState::new(window, cx).placeholder("Regnskabsår (tomt = aktuelt)");
            field.set_value(year, window, cx);
            field
        });
        let mut this = Self {
            session,
            company,
            origin,
            kind: Kind::Trial,
            year,
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
        let year = self.year.read(cx).value().trim().to_owned();
        let task = cx.background_executor().spawn(async move {
            let value = session
                .runtime
                .block_on(session.client.report(
                    &company,
                    kind.contract().2,
                    if year.is_empty() { None } else { Some(&year) },
                    None,
                ))
                .map_err(readable)?;
            sheet(value, kind, &company)
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(data) => {
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
        view = view
            .child(kinds)
            .child(Input::new(&self.year).disabled(self.busy))
            .child(
                Button::new("report-refresh")
                    .label("Hent valgt rapport og regnskabsår")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.load(cx))),
            );
        if let Some(data) = &self.data {
            view = view
                .child(div().text_lg().child(data.title.clone()))
                .child(data.period.clone());
            let mut years = div().flex().flex_wrap().gap_2();
            for year in &data.years {
                let year = year.clone();
                years = years.child(
                    Button::new(SharedString::from(format!("report-year-{year}")))
                        .label(year.clone())
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.year
                                .update(cx, |state, cx| state.set_value(year.clone(), window, cx));
                            this.load(cx);
                        })),
                );
            }
            view = view
                .child(years)
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
        let report = sheet(fixture(), Kind::Income, "synthetic").unwrap();
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
        assert_eq!(report.years, vec!["2026"]);
    }
    #[::core::prelude::v1::test]
    fn missing_amount_does_not_become_zero() {
        let mut v = fixture();
        v["incomeStatement"]
            .as_object_mut()
            .unwrap()
            .remove("result");
        assert!(sheet(v, Kind::Income, "synthetic").is_err());
    }
    #[::core::prelude::v1::test]
    fn rejects_other_company_and_unconfirmed_response() {
        assert!(sheet(fixture(), Kind::Income, "other").is_err());
        let mut v = fixture();
        v["ok"] = json!(false);
        assert!(sheet(v, Kind::Income, "synthetic").is_err());
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
        let report = sheet(root.clone(), Kind::Balance, "synthetic").unwrap();
        assert!(
            report
                .lines
                .contains(&"— · Årets resultat    205    (forrige: —)".into())
        );
        assert!(report.lines.last().unwrap().starts_with("Stemmer ikke"));
        root["balance"].as_object_mut().unwrap().remove("balanced");
        assert!(sheet(root, Kind::Balance, "synthetic").is_err());
    }
    #[::core::prelude::v1::test]
    fn journal_keeps_original_and_reversal_references() {
        let mut root = fixture();
        let mut journal = root["incomeStatement"].take();
        journal["entries"] = json!([{"entryNo":"2026-00002","date":"2026-10-19","text":"Synthetic correction","documentNo":"B-2","reversalOfEntryNo":"2026-00001","lines":[{"accountNo":"I","accountName":"Income","debit":200,"credit":0}]}]);
        root["journal"] = journal;
        let report = sheet(root, Kind::Journal, "synthetic").unwrap();
        assert!(report.lines.contains(&"Tilbagefører 2026-00001".into()));
        assert!(
            report
                .lines
                .contains(&"I · Income    Debet 200 · Kredit 0".into())
        );
    }
}
