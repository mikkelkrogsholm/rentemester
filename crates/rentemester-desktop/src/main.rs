//! Native GPUI consumer. All company data comes through the shared HTTP client.
//! Build: cargo +stable build -p rentemester-desktop (Rust >=1.95; verified with 1.99.1).
use gpui_kit::{
    component::{
        ActiveTheme, Disableable,
        button::{Button, ButtonVariants},
        checkbox::Checkbox,
        input::{Input, InputState},
    },
    *,
};
use rentemester_client::{ApiClient, Report};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};

#[derive(Default)]
struct Options {
    server: String,
    cookie: PathBuf,
    company: Option<String>,
    year: String,
    allow_http: bool,
}
impl Options {
    fn parse() -> Result<Self, String> {
        let mut result = Self::default();
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            match flag.as_str(){
            "--allow-loopback-http"=>result.allow_http=true,
            "--server"=>result.server=args.next().ok_or("--server requires an origin")?,
            "--cookie-file"=>result.cookie=args.next().ok_or("--cookie-file requires a path")?.into(),
            "--company"=>result.company=Some(args.next().ok_or("--company requires a slug")?),
            "--year"=>result.year=args.next().ok_or("--year requires a fiscal label")?,
            "--help"=>return Err("Usage: rentemester-desktop [--server HTTPS_ORIGIN --cookie-file PATH] [--company SLUG] [--year LABEL] [--allow-loopback-http]\nThe native window can also connect using its fields. Complete MFA in the server browser UI; supply its Cookie request header in an owner-only file. Managed local runtime is not implemented: the native server currently requires its qualified Linux runtime. This desktop connects only to explicit remote server origins.".into()),
            _=>return Err(format!("Unknown option {flag}; use --help")),
        }
        }
        Ok(result)
    }
}
struct Session {
    client: ApiClient,
    runtime: tokio::runtime::Runtime,
}
#[derive(Clone)]
struct Company {
    slug: String,
    name: String,
}
struct Trial {
    title: String,
    period: String,
    rows: Vec<[String; 6]>,
    totals: String,
    balanced: bool,
}
fn field(value: &Value, key: &str) -> Result<String, String> {
    value[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("Server response missing {key}"))
}
fn amount(value: &Value, key: &str) -> Result<String, String> {
    match &value[key] {
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(format!("Server response missing amount {key}")),
    }
}
fn companies(value: Value) -> Result<Vec<Company>, String> {
    if value["ok"] != true {
        return Err("Server did not confirm company list".into());
    }
    value["companies"]
        .as_array()
        .ok_or("Server response missing companies")?
        .iter()
        .map(|v| {
            Ok(Company {
                slug: field(v, "slug")?,
                name: field(v, "name")?,
            })
        })
        .collect()
}
fn trial(value: Value) -> Result<Trial, String> {
    if value["ok"] != true {
        return Err("Server did not confirm trial balance".into());
    }
    let report = &value["trialBalance"];
    let rows = report["rows"]
        .as_array()
        .ok_or("Server response missing trial balance rows")?
        .iter()
        .map(|v| {
            Ok([
                field(v, "accountNo")?,
                field(v, "name")?,
                field(v, "type")?,
                amount(v, "debit")?,
                amount(v, "credit")?,
                amount(v, "balance")?,
            ])
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Trial {
        title: format!(
            "{} · {}",
            field(&report["company"], "name")?,
            field(report, "selectedYear")?
        ),
        period: format!(
            "{} — {} · {}",
            field(report, "periodStart")?,
            field(report, "periodEnd")?,
            field(&report["company"], "currency")?
        ),
        rows,
        totals: format!(
            "Debet i alt {} · Kredit i alt {}",
            amount(report, "totalDebit")?,
            amount(report, "totalCredit")?
        ),
        balanced: report["balanced"]
            .as_bool()
            .ok_or("Server response missing balance check")?,
    })
}
fn readable(error: rentemester_client::Error) -> String {
    match error {
        rentemester_client::Error::Refused{status:401}=>"Sessionen er udløbet eller kræver login/MFA. Log ind på serveren og opdatér cookie-filen.".into(),
        rentemester_client::Error::Refused{status:403}=>"Serveren afviste adgang. Kontrollér din aktuelle virksomhedsrolle.".into(),
        rentemester_client::Error::Transport=>"Serveren kunne ikke nås. Kontrollér adresse, netværk og TLS-certifikat.".into(),
        error=>error.to_string(),
    }
}
struct Desktop {
    server: Entity<InputState>,
    cookie: Entity<InputState>,
    year: Entity<InputState>,
    allow_http: bool,
    session: Option<Arc<Session>>,
    companies: Vec<Company>,
    selected: Option<String>,
    trial: Option<Trial>,
    status: String,
    busy: bool,
}
impl Desktop {
    fn new(window: &mut Window, cx: &mut Context<Self>, options: Options) -> Self {
        let server = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("https://bogfoering.example");
            input.set_value(options.server.clone(), window, cx);
            input
        });
        let cookie = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("/private/session.cookie");
            input.set_value(options.cookie.to_string_lossy().to_string(), window, cx);
            input
        });
        let year = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Aktuelt regnskabsår");
            input.set_value(options.year, window, cx);
            input
        });
        let mut desktop=Self{server,cookie,year,allow_http:options.allow_http,session:None,companies:vec![],selected:options.company,trial:None,status:"Forbind til din Rentemester-server. Log ind med MFA i browseren og vælg en cookie-fil med din session.".into(),busy:false};
        if !options.server.is_empty() && !options.cookie.as_os_str().is_empty() {
            desktop.connect(cx);
        }
        desktop
    }
    fn connect(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.session = None;
        self.companies.clear();
        self.trial = None;
        self.status = "Forbinder…".into();
        let server = self.server.read(cx).value().to_string();
        let cookie = PathBuf::from(self.cookie.read(cx).value().to_string());
        let allow_http = self.allow_http;
        let task = cx.background_executor().spawn(async move {
            let cookie = rentemester_client::read_cookie_file(&cookie).map_err(readable)?;
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|_| "HTTP-runtime kunne ikke startes".to_owned())?;
            let client = {
                let _guard = runtime.enter();
                ApiClient::new(&server, allow_http, Some(cookie)).map_err(readable)?
            };
            let list = companies(runtime.block_on(client.companies()).map_err(readable)?)?;
            Ok::<_, String>((Arc::new(Session { client, runtime }), list))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((session, list)) => {
                        this.session = Some(session);
                        this.companies = list;
                        this.status = if this.companies.is_empty() {
                            "Forbundet. Ingen tilgængelige virksomheder.".into()
                        } else {
                            "Forbundet. Vælg en virksomhed for at læse saldobalancen.".into()
                        };
                        if let Some(slug) = this.selected.clone() {
                            if this.companies.iter().any(|c| c.slug == slug) {
                                this.load(slug, cx);
                            } else {
                                this.status =
                                    "Den valgte virksomhed er ikke tilgængelig for din session."
                                        .into();
                            }
                        }
                    }
                    Err(error) => this.status = error,
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn load(&mut self, slug: String, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(session) = self.session.clone() else {
            return;
        };
        self.busy = true;
        self.trial = None;
        self.selected = Some(slug.clone());
        self.status = "Henter saldobalance…".into();
        let year = self.year.read(cx).value().to_string();
        let task = cx.background_executor().spawn(async move {
            trial(
                session
                    .runtime
                    .block_on(session.client.report(
                        &slug,
                        Report::TrialBalance,
                        if year.is_empty() { None } else { Some(&year) },
                        None,
                    ))
                    .map_err(readable)?,
            )
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(report) => {
                        this.trial = Some(report);
                        this.status = "Saldobalance læst fra serveren.".into();
                    }
                    Err(error) => this.status = error,
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
impl Render for Desktop {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut content = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_6()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(div().text_2xl().child("Rentemester"))
            .child(div().child("Skrivebord · ekstern server"))
            .child(div().child("Serveradresse (HTTPS)"))
            .child(Input::new(&self.server).disabled(self.busy))
            .child(div().child("Cookie-fil fra browserens MFA-session"))
            .child(Input::new(&self.cookie).disabled(self.busy))
            .child(
                Checkbox::new("loopback-http")
                    .label("Tillad HTTP til lokal testserver (kun loopback)")
                    .checked(self.allow_http)
                    .disabled(self.busy)
                    .on_change(cx.listener(|this, value, _, cx| {
                        this.allow_http = *value;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("connect")
                    .primary()
                    .label("Forbind / genindlæs virksomheder")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.connect(cx))),
            )
            .child(div().child(self.status.clone()))
            .child(div().child("Regnskabsår (tomt = aktuelt)"))
            .child(Input::new(&self.year).disabled(self.busy));
        let mut list = div().flex().gap_2().flex_wrap();
        for company in self.companies.clone() {
            let slug = company.slug.clone();
            list = list.child(
                Button::new(SharedString::from(format!("company-{}", company.slug)))
                    .label(company.name)
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| this.load(slug.clone(), cx))),
            );
        }
        content = content.child(list);
        if let Some(report) = &self.trial {
            let mut table = div()
                .id("trial-balance")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .gap_1()
                .child(
                    div()
                        .text_xl()
                        .child(format!("Saldobalance · {}", report.title)),
                )
                .child(report.period.clone())
                .child(row(
                    ["Konto", "Navn", "Type", "Debet", "Kredit", "Saldo"].map(str::to_owned)
                ));
            for cells in &report.rows {
                table = table.child(row(cells.clone()));
            }
            if report.rows.is_empty() {
                table = table.child("Ingen bevægelser i dette regnskabsår.");
            }
            table = table
                .child(report.totals.clone())
                .child(if report.balanced {
                    "Debet og kredit stemmer."
                } else {
                    "Debet og kredit stemmer ikke — gennemgå serverens ledger."
                });
            content = content.child(table);
        }
        content
    }
}
fn row(cells: [String; 6]) -> impl IntoElement {
    div()
        .flex()
        .gap_3()
        .children(cells.into_iter().enumerate().map(|(i, text)| {
            div()
                .w(if i == 1 { px(260.) } else { px(110.) })
                .child(text)
        }))
}
fn main() {
    let options = match Options::parse() {
        Ok(o) => o,
        Err(message) => {
            if std::env::args().any(|arg| arg == "--help") {
                println!("{message}");
                return;
            }
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            let window = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Rentemester".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::centered(size(px(1050.), px(800.)), cx)),
                ..Default::default()
            };
            if let Err(error) = gpui_kit::open_window(window, cx, |window, cx| {
                cx.new(|cx| Desktop::new(window, cx, options))
            }) {
                eprintln!("Native window unavailable: {error}");
                cx.quit();
            }
            cx.activate(true);
        });
}

#[cfg(test)]
mod tests {
    use super::{companies, trial};
    use serde_json::json;
    #[test]
    fn remote_report_requires_real_fields_and_preserves_display_amounts() {
        let source = json!({"ok":true,"trialBalance":{"company":{"name":"Synthetic Company","currency":"DKK"},"selectedYear":"2026","periodStart":"2026-01-01","periodEnd":"2026-12-31","rows":[{"accountNo":"X","name":"Synthetic expense","type":"expense","debit":12.34,"credit":0,"balance":12.34}],"totalDebit":12.34,"totalCredit":12.34,"balanced":true}});
        let report = trial(source.clone()).unwrap();
        assert_eq!(report.rows[0][3], "12.34");
        assert!(report.balanced);
        let mut incomplete = source;
        incomplete["trialBalance"]["rows"][0]["balance"] = serde_json::Value::Null;
        assert!(trial(incomplete).is_err());
        assert!(trial(json!({"ok":false})).is_err());
        assert!(companies(json!({"ok":true,"companies":[{"name":"Missing slug"}]})).is_err());
        let list = companies(
            json!({"ok":true,"companies":[{"slug":"synthetic","name":"Synthetic Company"}]}),
        )
        .unwrap();
        assert_eq!(list[0].slug, "synthetic");
    }
}
