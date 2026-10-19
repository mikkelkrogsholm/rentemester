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
mod login;
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
            "--help"=>return Err("Usage: rentemester-desktop [--server HTTPS_ORIGIN --cookie-file PATH] [--company SLUG] [--year LABEL] [--allow-loopback-http]\nThe native window can also connect using its fields. Log in with e-mail/password and TOTP or backup code in the native window. Existing MFA cookie files remain supported. Managed local runtime is not implemented: the native server currently requires its qualified Linux runtime. This desktop connects only to explicit remote server origins.".into()),
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
        rentemester_client::Error::Refused { status: 401 } => {
            "Sessionen er udløbet eller kræver login/MFA. Log ind igen.".into()
        }
        rentemester_client::Error::Refused { status: 403 } => {
            "Serveren afviste adgang. Kontrollér din aktuelle virksomhedsrolle.".into()
        }
        rentemester_client::Error::Transport => {
            "Serveren kunne ikke nås. Kontrollér adresse, netværk og TLS-certifikat.".into()
        }
        error => error.to_string(),
    }
}
struct Desktop {
    server: Entity<InputState>,
    cookie: Entity<InputState>,
    email: Entity<InputState>,
    password: Entity<InputState>,
    code: Entity<InputState>,
    cookie_mode: bool,
    backup_code: bool,
    pending: Option<login::Pending>,
    enrollment: Option<String>,
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
        let email = cx.new(|cx| InputState::new(window, cx).placeholder("E-mail"));
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Adgangskode")
                .masked(true)
        });
        let code = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("MFA-kode")
                .masked(true)
        });
        let mut desktop = Self {
            server,
            cookie,
            year,
            email,
            password,
            code,
            cookie_mode: !options.cookie.as_os_str().is_empty(),
            backup_code: false,
            pending: None,
            enrollment: None,
            allow_http: options.allow_http,
            session: None,
            companies: vec![],
            selected: options.company,
            trial: None,
            status: "Forbind til din Rentemester-server og log ind med e-mail, adgangskode og MFA."
                .into(),
            busy: false,
        };
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
        self.pending = None;
        self.enrollment = None;
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
    fn password_login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.session = None;
        self.pending = None;
        self.enrollment = None;
        self.companies.clear();
        self.trial = None;
        self.busy = true;
        self.status = "Kontrollerer login…".into();
        let origin = self.server.read(cx).value().to_string();
        let allow_http = self.allow_http;
        let email = self.email.read(cx).value().to_string();
        let password = self.password.read(cx).value().to_string();
        self.password
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.code
            .update(cx, |input, cx| input.set_value("", window, cx));
        let task = cx
            .background_executor()
            .spawn(async move { login::begin(origin, allow_http, email, password) });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok(login::Step::Factor(pending))=>{this.pending=Some(pending);this.status="Adgangskoden er godkendt. Indtast en TOTP-kode eller en ubrugt backupkode.".into();},Ok(login::Step::Enrollment(origin))=>{this.enrollment=Some(origin);this.status="MFA skal først opsættes på serveren. Åbn serveren i browseren, gennemfør opsætningen, og log ind igen her.".into();},Err(error)=>this.status=error}cx.notify();});}).detach();
        cx.notify();
    }
    fn factor_login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(pending) = self.pending.take() else {
            return;
        };
        self.busy = true;
        self.status = "Kontrollerer MFA…".into();
        let backup = self.backup_code;
        let code = self.code.read(cx).value().to_string();
        self.code
            .update(cx, |input, cx| input.set_value("", window, cx));
        let task = cx
            .background_executor()
            .spawn(async move { login::complete(pending, backup, code) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(session) => this.refresh(Arc::new(session), cx),
                    Err(failure) => {
                        this.pending = failure.retry;
                        this.status = failure.message;
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn refresh(&mut self, session: Arc<Session>, cx: &mut Context<Self>) {
        self.session = Some(session.clone());
        self.busy = true;
        self.status = "Login godkendt. Henter virksomheder…".into();
        let task = cx.background_executor().spawn(async move {
            companies(
                session
                    .runtime
                    .block_on(session.client.companies())
                    .map_err(readable)?,
            )
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(list) => {
                        this.companies = list;
                        this.status =
                            "Forbundet. Vælg en virksomhed for at læse saldobalancen.".into();
                        if let Some(slug) = this.selected.clone() {
                            if this.companies.iter().any(|c| c.slug == slug) {
                                this.load(slug, cx);
                            }
                        }
                    }
                    Err(error) => this.status = error,
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn reset_login(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.pending = None;
        self.enrollment = None;
        self.session = None;
        self.companies.clear();
        self.trial = None;
        self.password
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.code
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.status = "Vælg server og log ind.".into();
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
            .child(div().child("Skrivebord · ekstern server"));
        if self.session.is_none() {
            let frozen = self.busy || self.pending.is_some();
            content = content
                .child("Serveradresse (HTTPS)")
                .child(Input::new(&self.server).disabled(frozen))
                .child(
                    Checkbox::new("loopback-http")
                        .label("Tillad HTTP til lokal testserver (kun loopback)")
                        .checked(self.allow_http)
                        .disabled(frozen)
                        .on_change(cx.listener(|this, value, _, cx| {
                            this.allow_http = *value;
                            cx.notify();
                        })),
                )
                .child(
                    Checkbox::new("cookie-mode")
                        .label("Brug eksisterende cookie-fil (avanceret)")
                        .checked(self.cookie_mode)
                        .disabled(frozen)
                        .on_change(cx.listener(|this, value, window, cx| {
                            this.cookie_mode = *value;
                            this.enrollment = None;
                            this.password
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            this.code
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            cx.notify();
                        })),
                );
            if self.cookie_mode {
                content = content
                    .child("Cookie-fil fra browserens MFA-session")
                    .child(Input::new(&self.cookie).disabled(self.busy))
                    .child(
                        Button::new("connect")
                            .primary()
                            .label("Forbind med cookie-fil")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.connect(cx))),
                    );
            } else if let Some(pending) = &self.pending {
                content = content
                    .child(format!("MFA til {}", pending.origin))
                    .child("MFA-kode")
                    .child(Input::new(&self.code).disabled(self.busy))
                    .child(
                        Checkbox::new("backup-code")
                            .label("Brug backupkode")
                            .checked(self.backup_code)
                            .disabled(self.busy)
                            .on_change(cx.listener(|this, value, _, cx| {
                                this.backup_code = *value;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("mfa-login")
                            .primary()
                            .label("Bekræft MFA")
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.factor_login(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("cancel-login")
                            .label("Start login forfra")
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.reset_login(window, cx)),
                            ),
                    );
            } else {
                content = content
                    .child("E-mail")
                    .child(Input::new(&self.email).disabled(self.busy))
                    .child("Adgangskode")
                    .child(Input::new(&self.password).disabled(self.busy))
                    .child(
                        Button::new("password-login")
                            .primary()
                            .label("Log ind")
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.password_login(window, cx)),
                            ),
                    );
                if let Some(origin) = self.enrollment.clone() {
                    content = content.child(
                        Button::new("enroll-browser")
                            .label("Åbn serverens MFA-opsætning i browser")
                            .on_click(move |_, _, cx| cx.open_url(&format!("{origin}/"))),
                    );
                }
            }
        } else {
            content = content
                .child(format!("Server: {}", self.server.read(cx).value()))
                .child(
                    Button::new("change-connection")
                        .label("Skift forbindelse")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, window, cx| this.reset_login(window, cx))),
                );
            if let Some(session) = self.session.clone() {
                content = content.child(
                    Button::new("refresh-companies")
                        .label("Genindlæs virksomheder")
                        .disabled(self.busy)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.refresh(session.clone(), cx)),
                        ),
                );
            }
        }
        content = content
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
