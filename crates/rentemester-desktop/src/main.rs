//! Native GPUI consumer. All company data comes through the shared HTTP client.
//! Build: cargo +stable build -p rentemester-desktop (Rust >=1.95; verified with 1.99.1).
use gpui_kit::{
    component::{
        ActiveTheme, Disableable,
        button::{Button, ButtonVariants},
        checkbox::Checkbox,
        input::{Input, InputState},
        radio::RadioGroup,
    },
    *,
};
mod bank;
mod bank_state;
mod contacts;
mod document_state;
mod documents;
mod invoice;
mod invoice_state;
mod journal;
mod journal_attempt;
mod local_backend;
mod login;
mod reporting;
mod opening_balance;
mod opening_balance_state;
mod opening_balance_attempt;
mod company_creation;
mod company_creation_state;
mod company_creation_attempt;
mod archive_import;
mod archive_import_state;
mod archive_import_attempt;
mod annual;
mod backup_verify;
mod backup_verify_state;
mod payables;
mod payables_state;
mod mileage;
mod mileage_state;
mod scheduled_recurring;
mod scheduled_recurring_state;
mod scheduled_dispatcher;
mod scheduled_dispatcher_state;
mod eliminations;
mod eliminations_state;
mod consolidation;
mod consolidation_state;
mod intercompany;
mod intercompany_state;
mod ownership;
mod ownership_state;
mod corporate_records;
mod corporate_records_state;
mod workspace_inbox;
mod workspace_inbox_state;
mod commitments;
mod commitments_state;
mod parties;
mod parties_state;
mod company_knowledge;
mod company_knowledge_state;
mod company_setup;
mod company_setup_state;
mod budget;
mod budget_state;
mod periods;
mod periods_state;
mod backup;
mod backup_state;
use rentemester_client::{ApiClient, Report};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc};

actions!(workflows, [OpenInvoices, OpenCompanySetup, OpenOpeningBalance, OpenCompanyCreation, OpenArchiveImport, OpenAnnual, OpenBackupVerify, BackToOverview, ScrollWorkflowDown, ScrollWorkflowUp]);

#[derive(Default)]
struct Options {
    server: String,
    cookie: PathBuf,
    company: Option<String>,
    year: String,
    allow_http: bool,
    local: bool,
    local_create: bool,
    local_workspace: PathBuf,
    server_binary: PathBuf,
    web: PathBuf,
}
impl Options {
    fn parse() -> Result<Self, String> {
        let mut result = Self::default();
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            match flag.as_str(){
            "--local"=>result.local=true,
            "--create-local"=>{result.local=true;result.local_create=true;},
            "--local-workspace"=>result.local_workspace=args.next().ok_or("--local-workspace requires a path")?.into(),
            "--server-binary"=>result.server_binary=args.next().ok_or("--server-binary requires a path")?.into(),
            "--web-dir"=>result.web=args.next().ok_or("--web-dir requires a path")?.into(),
            "--allow-loopback-http"=>result.allow_http=true,
            "--server"=>result.server=args.next().ok_or("--server requires an origin")?,
            "--cookie-file"=>result.cookie=args.next().ok_or("--cookie-file requires a path")?.into(),
            "--company"=>result.company=Some(args.next().ok_or("--company requires a slug")?),
            "--year"=>result.year=args.next().ok_or("--year requires a fiscal label")?,
            "--help"=>return Err("Usage: rentemester-desktop [--server HTTPS_ORIGIN --cookie-file PATH] [--company SLUG] [--year LABEL] [--allow-loopback-http]\nThe native window can also connect using its fields. Log in with e-mail/password and TOTP or backup code in the native window. Existing MFA cookie files remain supported. For managed local mode use --local [--local-workspace PATH] [--create-local]. Default durable data: ~/Library/Application Support/Rentemester. The app bundle supplies its sibling server and browser assets; development overrides: --server-binary PATH --web-dir PATH. Local mode uses the explicit same-user macOS runtime profile.".into()),
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
    balanced: Option<bool>,
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
        Value::String(s) if {
            let decimal=s.strip_prefix('-').unwrap_or(s);
            decimal.split_once('.').is_some_and(|(whole,fraction)| !whole.is_empty() && whole.bytes().all(|b|b.is_ascii_digit()) && fraction.len()==2 && fraction.bytes().all(|b|b.is_ascii_digit()))
        } => Ok(s.clone()),
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
    let source=report.get("source").or_else(||report.get("selectedSource"));
    if source.is_some_and(|s| !matches!(s.as_str(),Some("live"|"archive"))) { return Err("Ukendt rapportkilde".into()); }
    let archive=source==Some(&Value::String("archive".into()));
    let shown_amount=|v:&Value,key:&str| if archive && v.get(key)==Some(&Value::Null) {Ok("Ikke tilgængeligt".into())} else {amount(v,key)};
    let period=|key:&str| if archive && report.get(key)==Some(&Value::Null) {Ok("Ikke oplyst".into())} else {field(report,key)};
    let rows = report["rows"]
        .as_array()
        .ok_or("Server response missing trial balance rows")?
        .iter()
        .map(|v| {
            Ok([
                field(v, "accountNo")?,
                field(v, "name")?,
                if archive && v.get("type")==Some(&Value::Null) {"Uklassificeret".into()} else {field(v, "type")?},
                shown_amount(v, "debit")?,
                shown_amount(v, "credit")?,
                shown_amount(v, "balance")?,
            ])
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Trial {
        title: format!(
            "{} · {} · {}",
            field(&report["company"], "name")?,
            field(report, "selectedYear")?,
            if archive {"Arkiv"} else {"Aktivt regnskab"},
        ),
        period: format!(
            "{} — {} · {}",
            period("periodStart")?,
            period("periodEnd")?,
            field(&report["company"], "currency")?
        ),
        rows,
        totals: format!(
            "Debet i alt {} · Kredit i alt {}",
            shown_amount(report, "totalDebit")?,
            shown_amount(report, "totalCredit")?
        ),
        balanced: match report.get("balanced") {
            Some(Value::Bool(value))=>Some(*value),
            Some(Value::Null) if archive=>None,
            _=>return Err("Server response missing balance check".into()),
        },
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
    workflow_focus: FocusHandle,
    close_focus: FocusHandle,
    body_scroll: ScrollHandle,
    server: Entity<InputState>,
    local_mode: bool,
    local_create: bool,
    local_workspace: Entity<InputState>,
    local_name: Entity<InputState>,
    server_binary: PathBuf,
    web: PathBuf,
    managed: Option<local_backend::Managed>,
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
    journal: Option<Entity<journal::Journal>>,
    invoice: Option<Entity<invoice::Invoice>>,
    reporting: Option<Entity<reporting::Reporting>>,
    bank: Option<Entity<bank::Bank>>,
    contacts: Option<Entity<contacts::Contacts>>,
    documents: Option<Entity<documents::Documents>>,
    payables: Option<Entity<payables::Payables>>,
    mileage: Option<Entity<mileage::Mileage>>,
    budget: Option<Entity<budget::Budget>>,
    periods: Option<Entity<periods::Periods>>,
    backup: Option<Entity<backup::Backup>>,
    scheduled_recurring: Option<Entity<scheduled_recurring::ScheduledRecurring>>,
    scheduled_dispatcher: Entity<scheduled_dispatcher::ScheduledDispatcher>,
    eliminations: Option<Entity<eliminations::Eliminations>>,
    consolidation: Option<Entity<consolidation::Consolidation>>,
    intercompany: Option<Entity<intercompany::Intercompany>>,
    ownership: Option<Entity<ownership::Ownership>>,
    corporate_records: Option<Entity<corporate_records::CorporateRecords>>,
    workspace_inbox: Option<Entity<workspace_inbox::WorkspaceInbox>>,
    commitments: Option<Entity<commitments::Commitments>>,
    parties: Option<Entity<parties::WorkspaceParties>>,
    company_knowledge: Option<Entity<company_knowledge::CompanyKnowledge>>,
    company_setup: Option<Entity<company_setup::CompanySetup>>,
    opening_balance: Option<Entity<opening_balance::OpeningBalance>>,
    company_creation: Option<Entity<company_creation::CompanyCreation>>,
    archive_import: Option<Entity<archive_import::ArchiveImport>>,
    pending_company_selection: Option<String>,
    annual: Option<Entity<annual::Annual>>,
    backup_verify: Option<Entity<backup_verify::BackupVerify>>,
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
        let local_path = if options.local_workspace.as_os_str().is_empty() {
            local_backend::default_profile()
        } else {
            options.local_workspace
        };
        let local_workspace = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Lokal datamappe");
            input.set_value(local_path.to_string_lossy().to_string(), window, cx);
            input
        });
        let local_name = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Virksomhedens navn");
            input.set_value("Min virksomhed", window, cx);
            input
        });
        let (binary, web) = local_backend::bundled_paths();
        let workflow_focus = cx.focus_handle();
        cx.on_focus_lost(window, |this, window, cx| {
            window.focus(&this.workflow_focus, cx);
        }).detach();
        window.focus(&workflow_focus, cx);
        let mut desktop = Self {
            workflow_focus,
            close_focus: cx.focus_handle(),
            body_scroll: ScrollHandle::new(),
            local_mode: options.local,
            local_create: options.local_create,
            local_workspace,
            local_name,
            server_binary: if options.server_binary.as_os_str().is_empty() {
                binary
            } else {
                options.server_binary
            },
            web: if options.web.as_os_str().is_empty() {
                web
            } else {
                options.web
            },
            managed: None,
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
            journal: None,
            invoice: None,
            reporting: None,
            bank: None,
            contacts: None,
            documents: None,
            payables: None,
            mileage: None,
            budget: None,
            periods: None,
            backup: None,
            scheduled_recurring: None,
            scheduled_dispatcher: cx.new(scheduled_dispatcher::ScheduledDispatcher::new),
            eliminations: None,
            consolidation: None,
            intercompany: None,
            ownership: None,
            corporate_records: None,
            workspace_inbox: None,
            commitments: None,
            parties: None,
            company_knowledge: None,
            company_setup: None,
            opening_balance: None,
            company_creation: None,
            archive_import: None,
            pending_company_selection: None,
            annual: None,
            backup_verify: None,
            status: "Forbind til din Rentemester-server og log ind med e-mail, adgangskode og MFA."
                .into(),
            busy: false,
        };
        if !options.local && !options.server.is_empty() && !options.cookie.as_os_str().is_empty() {
            desktop.connect(cx);
        }
        desktop
    }
    fn has_screen(&self) -> bool {
        self.journal.is_some()
            || self.invoice.is_some()
            || self.reporting.is_some()
            || self.bank.is_some()
            || self.contacts.is_some()
            || self.documents.is_some()
            || self.payables.is_some()
            || self.mileage.is_some()
            || self.budget.is_some()
            || self.periods.is_some()
            || self.backup.is_some()
            || self.scheduled_recurring.is_some()
            || self.eliminations.is_some()
            || self.consolidation.is_some()
            || self.intercompany.is_some()
            || self.ownership.is_some()
            || self.corporate_records.is_some()
            || self.workspace_inbox.is_some()
            || self.commitments.is_some()
            || self.parties.is_some()
            || self.company_knowledge.is_some()
            || self.company_setup.is_some()
            || self.opening_balance.is_some()
            || self.company_creation.is_some()
            || self.archive_import.is_some()
            || self.annual.is_some()
            || self.backup_verify.is_some()
    }
    fn open_invoices(&mut self, _: &OpenInvoices, window: &mut Window, cx: &mut Context<Self>) {
        self.open_workflow(true, window, cx);
    }
    fn open_company_setup(
        &mut self,
        _: &OpenCompanySetup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_workflow(false, window, cx);
    }
    fn open_workflow(&mut self, invoices: bool, window: &mut Window, cx: &mut Context<Self>) {
        let same_screen = if invoices {
            self.invoice.is_some()
        } else {
            self.company_setup.is_some()
        };
        match workflow_navigation(
            self.session.is_some() && self.selected.is_some(),
            self.busy,
            self.has_screen(),
            same_screen,
        ) {
            WorkflowNavigation::Unavailable => {
                self.status = "Log ind og vælg en virksomhed før navigation.".into();
                cx.notify();
                return;
            }
            WorkflowNavigation::KeepCurrent => {
                self.status = "Den åbne skærm og dens review bevares. Brug ⌘0 til at fokusere dens Luk-knap; Enter lukker eksplicit. Åbn derefter den ønskede arbejdsgang.".into();
                cx.notify();
                return;
            }
            WorkflowNavigation::Wait => return,
            WorkflowNavigation::Open | WorkflowNavigation::FocusExisting => {}
        }
        if !same_screen {
            let (Some(session), Some(company)) = (self.session.clone(), self.selected.clone())
            else {
                return;
            };
            let origin = self.connection_origin(cx);
            let local = match self
                .managed
                .as_ref()
                .map(|m| m.journal_identity())
                .transpose()
            {
                Ok(local) => local,
                Err(error) => {
                    self.status = error;
                    cx.notify();
                    return;
                }
            };
            if invoices {
                self.invoice =
                    Some(cx.new(|cx| {
                        invoice::Invoice::new(session, origin, company, local, window, cx)
                    }));
            } else {
                self.company_setup = Some(cx.new(|cx| {
                    company_setup::CompanySetup::new(session, origin, company, local, window, cx)
                }));
            }
        }
        self.body_scroll.set_offset(point(px(0.), px(0.)));
        if invoices {
            if let Some(view) = &self.invoice {
                view.update(cx, |v, cx| v.focus_entry(window, cx));
            }
        } else if let Some(view) = &self.company_setup {
            view.update(cx, |v, cx| v.focus_entry(window, cx));
        }
        cx.notify();
    }
    fn open_opening_balance(&mut self, _: &OpenOpeningBalance, window: &mut Window, cx: &mut Context<Self>) {
        match workflow_navigation(self.session.is_some() && self.selected.is_some(), self.busy, self.has_screen(), self.opening_balance.is_some()) {
            WorkflowNavigation::Unavailable => { self.status = "Log ind og vælg en virksomhed før navigation.".into(); cx.notify(); return; }
            WorkflowNavigation::KeepCurrent => { self.status = "Den åbne skærm og dens gennemgang bevares. Luk den eksplicit, før du åbner åbningsbalancen.".into(); cx.notify(); return; }
            WorkflowNavigation::Wait => return,
            WorkflowNavigation::Open | WorkflowNavigation::FocusExisting => {}
        }
        if self.opening_balance.is_none() {
            let (Some(session), Some(company)) = (self.session.clone(), self.selected.clone()) else { return; };
            let origin = self.connection_origin(cx);
            let local = match self.managed.as_ref().map(|m| m.journal_identity()).transpose() {
                Ok(local) => local,
                Err(error) => { self.status = error; cx.notify(); return; }
            };
            self.opening_balance = Some(cx.new(|cx| opening_balance::OpeningBalance::new(session, origin, company, local, window, cx)));
        }
        self.body_scroll.set_offset(point(px(0.), px(0.)));
        if let Some(view) = &self.opening_balance { view.update(cx, |v, cx| v.focus_entry(window, cx)); }
        cx.notify();
    }
    fn open_company_creation(&mut self, _: &OpenCompanyCreation, window:&mut Window, cx:&mut Context<Self>) {
        match workflow_navigation(self.session.is_some(),self.busy,self.has_screen(),self.company_creation.is_some()) {
            WorkflowNavigation::Unavailable=>{self.status="Log ind og vælg den nødvendige virksomhed før navigation.".into();cx.notify();return;},
            WorkflowNavigation::KeepCurrent=>{self.status="Den åbne skærm og dens gennemgang bevares. Luk den eksplicit før næste arbejdsgang.".into();cx.notify();return;},
            WorkflowNavigation::Wait=>return,
            WorkflowNavigation::Open|WorkflowNavigation::FocusExisting=>{}
        }
        if self.company_creation.is_none() {
            let Some(session)=self.session.clone() else {return;};
            let origin=self.connection_origin(cx);
            let local=match self.managed.as_ref().map(|m|m.journal_identity()).transpose() {Ok(local)=>local,Err(error)=>{self.status=error;cx.notify();return;}};
            self.company_creation=Some(cx.new(|cx|company_creation::CompanyCreation::new(session,origin,local,window,cx)));
        }
        self.body_scroll.set_offset(point(px(0.),px(0.)));
        if let Some(view)=&self.company_creation {view.update(cx,|v,cx|v.focus_entry(window,cx));}
        cx.notify();
    }
    fn open_archive_import(&mut self, _: &OpenArchiveImport, window:&mut Window, cx:&mut Context<Self>) {
        match workflow_navigation(self.session.is_some() && self.selected.is_some(),self.busy,self.has_screen(),self.archive_import.is_some()) {
            WorkflowNavigation::Unavailable=>{self.status="Log ind og vælg den nødvendige virksomhed før navigation.".into();cx.notify();return;},
            WorkflowNavigation::KeepCurrent=>{self.status="Den åbne skærm og dens gennemgang bevares. Luk den eksplicit før næste arbejdsgang.".into();cx.notify();return;},
            WorkflowNavigation::Wait=>return,
            WorkflowNavigation::Open|WorkflowNavigation::FocusExisting=>{}
        }
        if self.archive_import.is_none() {
            let (Some(session),Some(company))=(self.session.clone(),self.selected.clone()) else {return;};
            let origin=self.connection_origin(cx);
            let local=match self.managed.as_ref().map(|m|m.journal_identity()).transpose() {Ok(local)=>local,Err(error)=>{self.status=error;cx.notify();return;}};
            self.archive_import=Some(cx.new(|cx|archive_import::ArchiveImport::new(session,origin,company,local,window,cx)));
        }
        self.body_scroll.set_offset(point(px(0.),px(0.)));
        if let Some(view)=&self.archive_import {view.update(cx,|v,cx|v.focus_entry(window,cx));}
        cx.notify();
    }
    fn open_annual(&mut self, _: &OpenAnnual, window:&mut Window, cx:&mut Context<Self>) {
        match workflow_navigation(self.session.is_some() && self.selected.is_some(),self.busy,self.has_screen(),self.annual.is_some()) {
            WorkflowNavigation::Unavailable=>{self.status="Log ind og vælg en virksomhed før årsrapporten.".into();cx.notify();return;},
            WorkflowNavigation::KeepCurrent=>{self.status="Luk den åbne skærm før næste arbejdsgang.".into();cx.notify();return;},
            WorkflowNavigation::Wait=>return,
            WorkflowNavigation::Open|WorkflowNavigation::FocusExisting=>{}
        }
        if self.annual.is_none() {
            let (Some(session),Some(company))=(self.session.clone(),self.selected.clone()) else {return;};
            self.annual=Some(cx.new(|cx|annual::Annual::new(session,company,window,cx)));
        }
        self.body_scroll.set_offset(point(px(0.),px(0.)));
        if let Some(view)=&self.annual {view.update(cx,|v,cx|v.focus_entry(window,cx));}
        cx.notify();
    }
    fn open_backup_verify(&mut self, _: &OpenBackupVerify, window:&mut Window, cx:&mut Context<Self>) {
        match workflow_navigation(true,self.busy,self.has_screen(),self.backup_verify.is_some()) {
            WorkflowNavigation::KeepCurrent=>{self.status="Luk den åbne skærm før backupkontrollen.".into();cx.notify();return;},
            WorkflowNavigation::Wait|WorkflowNavigation::Unavailable=>return,
            WorkflowNavigation::Open|WorkflowNavigation::FocusExisting=>{}
        }
        if self.backup_verify.is_none() {
            let binary=self.server_binary.clone();
            self.backup_verify=Some(cx.new(|cx|backup_verify::BackupVerify::new(binary,window,cx)));
        }
        self.body_scroll.set_offset(point(px(0.),px(0.)));
        if let Some(view)=&self.backup_verify {view.update(cx,|v,cx|v.focus_entry(window,cx));}
        cx.notify();
    }
    fn back_to_overview(
        &mut self,
        _: &BackToOverview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.body_scroll.set_offset(point(px(0.), px(0.)));
        if self.has_screen() {
            self.status = "Luk-knappen har fokus. Enter lukker denne skærm eksplicit; afklar eller annullér først et review, hvis du vil beholde arbejdet.".into();
            window.focus(&self.close_focus, cx);
        } else {
            window.focus(&self.workflow_focus, cx);
        }
        cx.notify();
    }
    fn scroll_workflow_down(&mut self, _: &ScrollWorkflowDown, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_workflow(-1., cx);
    }
    fn scroll_workflow_up(&mut self, _: &ScrollWorkflowUp, _: &mut Window, cx: &mut Context<Self>) {
        self.scroll_workflow(1., cx);
    }
    fn scroll_workflow(&self, direction: f32, cx: &mut Context<Self>) {
        let mut offset = self.body_scroll.offset();
        offset.y += self.body_scroll.bounds().size.height * (0.8 * direction);
        self.body_scroll.set_offset(offset);
        cx.notify();
    }
    fn connection_origin(&self, cx: &App) -> String {
        if self.local_mode {
            self.managed
                .as_ref()
                .map(|m| m.origin.clone())
                .unwrap_or_default()
        } else {
            self.server.read(cx).value().to_string()
        }
    }
    fn start_local(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.managed.is_some() {
            return;
        }
        self.busy = true;
        self.status = "Starter din lokale Rust-server…".into();
        let input = local_backend::Start {
            profile: PathBuf::from(self.local_workspace.read(cx).value().to_string()),
            create: self.local_create,
            company_name: self.local_name.read(cx).value().to_string(),
            server_binary: self.server_binary.clone(),
            web: self.web.clone(),
        };
        let task = cx
            .background_executor()
            .spawn(async move { local_backend::Managed::start(input) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(managed) => {
                        this.managed = Some(managed);
                        this.local_create = false;
                        this.status = "Lokal server er klar. Ved første start: åbn opsætningen i browseren, bekræft e-mail og opsæt MFA. Log derefter ind her.".into();
                    }
                    Err(error) => this.status = error,
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn stop_local(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.reset_login(window, cx);
        let Some(mut managed) = self.managed.take() else {
            return;
        };
        self.busy = true;
        self.status = "Stopper den ejede lokale server…".into();
        let task = cx
            .background_executor()
            .spawn(async move { managed.stop() });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                this.status = result
                    .map(|_| {
                        "Den lokale server er stoppet. Data er gemt; vælg Åbn for at fortsætte."
                            .into()
                    })
                    .unwrap_or_else(|error| error);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn connect(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.scheduled_dispatcher.update(cx, |dispatcher, cx| dispatcher.invalidate(cx));
        self.busy = true;
        self.pending = None;
        self.enrollment = None;
        self.session = None;
        self.companies.clear();
        self.trial = None;
        self.journal = None;
        self.invoice = None;
        self.reporting = None;
        self.bank = None;
        self.contacts = None;
        self.documents = None;
        self.payables = None;
        self.mileage = None;
        self.budget = None;
        self.periods = None;
        self.backup = None;
        self.scheduled_recurring = None;
        self.eliminations = None;
        self.consolidation = None;
        self.intercompany = None;
        self.ownership = None;
        self.corporate_records = None;
        self.workspace_inbox = None;
        self.commitments = None;
        self.parties = None;
        self.company_knowledge = None;
        self.company_setup = None;
        self.opening_balance = None;
        self.company_creation = None;
        self.archive_import = None;
        self.pending_company_selection = None;
        self.annual = None;
        self.backup_verify = None;
        self.status = "Forbinder…".into();
        let server = self.connection_origin(cx);
        let cookie = PathBuf::from(self.cookie.read(cx).value().to_string());
        let allow_http = self.local_mode || self.allow_http;
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
        self.scheduled_dispatcher.update(cx, |dispatcher, cx| dispatcher.invalidate(cx));
        self.session = None;
        self.pending = None;
        self.enrollment = None;
        self.companies.clear();
        self.trial = None;
        self.journal = None;
        self.invoice = None;
        self.reporting = None;
        self.bank = None;
        self.contacts = None;
        self.documents = None;
        self.payables = None;
        self.mileage = None;
        self.budget = None;
        self.periods = None;
        self.backup = None;
        self.scheduled_recurring = None;
        self.eliminations = None;
        self.consolidation = None;
        self.intercompany = None;
        self.ownership = None;
        self.corporate_records = None;
        self.workspace_inbox = None;
        self.commitments = None;
        self.parties = None;
        self.company_knowledge = None;
        self.company_setup = None;
        self.opening_balance = None;
        self.company_creation = None;
        self.archive_import = None;
        self.pending_company_selection = None;
        self.annual = None;
        self.backup_verify = None;
        self.busy = true;
        self.status = "Kontrollerer login…".into();
        let origin = self.connection_origin(cx);
        let allow_http = self.local_mode || self.allow_http;
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
        self.scheduled_dispatcher.update(cx, |dispatcher, cx| dispatcher.invalidate(cx));
        self.session = Some(session.clone());
        self.busy = true;
        self.status = "Henter de aktuelt tilgængelige virksomheder…".into();
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
                        if let Some(slug) = this.pending_company_selection.take().or_else(||this.selected.clone()) {
                            if this.companies.iter().any(|c| c.slug == slug) {
                                this.load(slug, cx);
                            } else {
                                this.selected=None;
                                this.trial=None;
                                this.status="Virksomheden findes ikke i din aktuelle adgang. Vælg en tilgængelig virksomhed.".into();
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
        self.scheduled_dispatcher.update(cx, |dispatcher, cx| dispatcher.invalidate(cx));
        self.pending = None;
        self.enrollment = None;
        self.session = None;
        self.companies.clear();
        self.trial = None;
        self.journal = None;
        self.invoice = None;
        self.reporting = None;
        self.bank = None;
        self.contacts = None;
        self.documents = None;
        self.payables = None;
        self.mileage = None;
        self.budget = None;
        self.periods = None;
        self.backup = None;
        self.scheduled_recurring = None;
        self.eliminations = None;
        self.consolidation = None;
        self.intercompany = None;
        self.ownership = None;
        self.corporate_records = None;
        self.workspace_inbox = None;
        self.commitments = None;
        self.parties = None;
        self.company_knowledge = None;
        self.company_setup = None;
        self.opening_balance = None;
        self.company_creation = None;
        self.archive_import = None;
        self.pending_company_selection = None;
        self.annual = None;
        self.backup_verify = None;
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
        self.journal = None;
        self.invoice = None;
        self.reporting = None;
        self.bank = None;
        self.contacts = None;
        self.documents = None;
        self.payables = None;
        self.mileage = None;
        self.budget = None;
        self.periods = None;
        self.backup = None;
        self.scheduled_recurring = None;
        self.eliminations = None;
        self.consolidation = None;
        self.intercompany = None;
        self.ownership = None;
        self.corporate_records = None;
        self.workspace_inbox = None;
        self.commitments = None;
        self.parties = None;
        self.company_knowledge = None;
        self.company_setup = None;
        self.opening_balance = None;
        self.company_creation = None;
        self.archive_import = None;
        self.pending_company_selection = None;
        self.annual = None;
        self.backup_verify = None;
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
impl Desktop {
    fn render_body(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut content = div()
            .id("desktop-body")
            .flex()
            .flex_col()
            .gap_3()
            .p_6()
            .flex_none()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            ;
        if !self.has_screen() {
            content = content.child(div().child(if self.local_mode {
                "Skrivebord · lokal virksomhed"
            } else {
                "Skrivebord · ekstern server"
            }))
            .child(
                RadioGroup::horizontal("connection-mode")
                    .flex_none()
                    .children(["På denne Mac", "Ekstern server"])
                    .selected_index(Some(if self.local_mode { 0 } else { 1 }))
                    .disabled(
                        self.busy
                            || self.managed.is_some()
                            || self.session.is_some()
                            || self.pending.is_some(),
                    )
                    .on_change(cx.listener(|this, value, window, cx| {
                        this.local_mode = *value == 0;
                        this.reset_login(window, cx);
                    })),
            );
        content=content.child(Button::new("backup-verify-open").label("Kontrollér backup · ⌘7").disabled(self.busy)
            .on_click(cx.listener(|this,_,window,cx|this.open_backup_verify(&OpenBackupVerify,window,cx))));
        if self.local_mode {
            if let Some(managed) = &self.managed {
                let browser_url = managed.browser_url();
                content = content
                    .child(format!("Datamappe: {}", managed.profile.display()))
                    .child(
                        Button::new("local-browser")
                            .label("Åbn lokal opsætning og virksomhed i browser")
                            .disabled(self.busy)
                            .on_click(move |_, _, cx| cx.open_url(&browser_url)),
                    )
                    .child(
                        Button::new("local-verification")
                            .label("Åbn seneste lokale e-mailbekræftelse")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(managed) = &this.managed {
                                    match managed.verification_url() {
                                        Ok(url) => cx.open_url(&url),
                                        Err(error) => this.status = error,
                                    }
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("local-stop")
                            .label("Stop lokal server")
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.stop_local(window, cx)),
                            ),
                    );
            } else {
                content = content
                    .child("Lokal datamappe")
                    .child(Input::new(&self.local_workspace).disabled(self.busy))
                    .child(
                        Checkbox::new("local-create")
                            .label("Opret ny datamappe og virksomhed")
                            .checked(self.local_create)
                            .disabled(self.busy)
                            .on_change(cx.listener(|this, value, _, cx| {
                                this.local_create = *value;
                                cx.notify();
                            })),
                    );
                if self.local_create {
                    content = content
                        .child("Virksomhedens navn")
                        .child(Input::new(&self.local_name).disabled(self.busy));
                }
                content = content.child(
                    Button::new("local-start")
                        .primary()
                        .label(if self.local_create {
                            "Opret og start lokalt"
                        } else {
                            "Åbn lokalt"
                        })
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.start_local(cx))),
                );
            }
        }
        if self.session.is_none() && (!self.local_mode || self.managed.is_some()) {
            let frozen = self.busy || self.pending.is_some();
            if self.local_mode {
                content = content.child(format!("Server: {}", self.connection_origin(cx)));
            } else {
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
                    );
            }
            content = content.child(
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
        } else if self.session.is_some() {
            content = content
                .child(format!("Server: {}", self.connection_origin(cx)))
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
        if self.session.is_some() {
            content=content.child(Button::new("company-creation-open").label("Opret virksomhed · ⌘4").disabled(self.busy)
                .on_click(cx.listener(|this,_,window,cx|this.open_company_creation(&OpenCompanyCreation,window,cx))));
        }
        }
        if let Some(view)=self.backup_verify.clone() {
            return content.child(Button::new("backup-verify-close").track_focus(&self.close_focus)
                .label("Luk backupkontrol")
                .on_click(cx.listener(|this,_,_,cx|{this.backup_verify=None;cx.notify();}))).child(view);
        }
        if let Some(view)=self.annual.clone() {
            return content.child(Button::new("annual-close").track_focus(&self.close_focus)
                .label("Luk årsrapport")
                .on_click(cx.listener(|this,_,_,cx|{this.annual=None;cx.notify();}))).child(view);
        }
        if let Some(creation)=self.company_creation.clone() {
            return content.child(Button::new("company-creation-close").track_focus(&self.close_focus)
                .label("Luk og genindlæs virksomheder")
                .on_click(cx.listener(|this,_,_,cx|{
                    this.pending_company_selection=this.company_creation.as_ref().and_then(|view|view.read(cx).created_slug());
                    this.company_creation=None;
                    if let Some(session)=this.session.clone(){this.refresh(session,cx);}
                }))).child(creation);
        }
        if let Some(import)=self.archive_import.clone() {
            return content.child(Button::new("archive-import-close").track_focus(&self.close_focus)
                .label("Luk historikimport og genindlæs saldobalance")
                .on_click(cx.listener(|this,_,_,cx|{
                    this.archive_import=None;
                    if let Some(slug)=this.selected.clone(){this.load(slug,cx);}
                }))).child(import);
        }
        if let Some(scheduled_recurring) = self.scheduled_recurring.clone() {
            return content.child(Button::new("scheduled-recurring-close").track_focus(&self.close_focus)
                .label("Luk planlagte fakturaer")
                .on_click(cx.listener(|this, _, _, cx| { this.scheduled_recurring = None; cx.notify(); })))
                .child(scheduled_recurring);
        }
        if let Some(eliminations) = self.eliminations.clone() {
            return content.child(Button::new("eliminations-close").track_focus(&self.close_focus)
                .label("Luk elimineringer")
                .on_click(cx.listener(|this, _, _, cx| { this.eliminations = None; cx.notify(); })))
                .child(eliminations);
        }
        if let Some(consolidation) = self.consolidation.clone() {
            return content.child(Button::new("consolidation-close").track_focus(&self.close_focus)
                .label("Luk konsolideret rapport")
                .on_click(cx.listener(|this, _, _, cx| { this.consolidation = None; cx.notify(); })))
                .child(consolidation);
        }
        if let Some(intercompany) = self.intercompany.clone() {
            return content.child(Button::new("intercompany-close").track_focus(&self.close_focus)
                .label("Luk mellemregningsafstemning")
                .on_click(cx.listener(|this, _, _, cx| { this.intercompany = None; cx.notify(); })))
                .child(intercompany);
        }
        if let Some(ownership) = self.ownership.clone() {
            return content.child(Button::new("ownership-close").track_focus(&self.close_focus)
                .label("Luk ejer- og kontrolforhold")
                .on_click(cx.listener(|this, _, _, cx| { this.ownership = None; cx.notify(); })))
                .child(ownership);
        }
        if let Some(records) = self.corporate_records.clone() {
            return content.child(Button::new("corporate-records-close").track_focus(&self.close_focus)
                .label("Luk selskabsdokumenter")
                .on_click(cx.listener(|this, _, _, cx| { this.corporate_records = None; cx.notify(); })))
                .child(records);
        }
        if let Some(inbox) = self.workspace_inbox.clone() {
            return content.child(Button::new("workspace-inbox-close").track_focus(&self.close_focus)
                .label("Luk fælles bilagsindbakke")
                .on_click(cx.listener(|this, _, _, cx| { this.workspace_inbox = None; cx.notify(); })))
                .child(inbox);
        }
        if let Some(commitments) = self.commitments.clone() {
            return content.child(Button::new("commitments-close").track_focus(&self.close_focus)
                .label("Luk forpligtelser og likviditet")
                .on_click(cx.listener(|this, _, _, cx| { this.commitments = None; cx.notify(); })))
                .child(commitments);
        }
        if let Some(parties) = self.parties.clone() {
            return content.child(Button::new("parties-close").track_focus(&self.close_focus)
                .label("Luk workspace-parter")
                .on_click(cx.listener(|this, _, _, cx| { this.parties = None; cx.notify(); })))
                .child(parties);
        }
        if let Some(knowledge) = self.company_knowledge.clone() {
            return content.child(Button::new("knowledge-close").track_focus(&self.close_focus)
                .label("Luk virksomhedsoplysninger")
                .on_click(cx.listener(|this, _, _, cx| { this.company_knowledge = None; cx.notify(); })))
                .child(knowledge);
        }
        if let Some(setup) = self.company_setup.clone() {
            return content.child(Button::new("company-setup-close").track_focus(&self.close_focus)
                .label("Luk virksomhedsopsætning og genindlæs saldobalance")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.company_setup = None;
                    if let Some(slug) = this.selected.clone() { this.load(slug, cx); }
                }))).child(setup);
        }
        if let Some(opening) = self.opening_balance.clone() {
            return content.child(Button::new("opening-balance-close").track_focus(&self.close_focus)
                .label("Luk åbningsbalance og genindlæs saldobalance")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.opening_balance = None;
                    if let Some(slug) = this.selected.clone() { this.load(slug, cx); }
                }))).child(opening);
        }
        if let Some(backup) = self.backup.clone() {
            return content.child(Button::new("backup-close").track_focus(&self.close_focus)
                .label("Luk workspace-backup")
                .on_click(cx.listener(|this, _, _, cx| { this.backup = None; cx.notify(); })))
                .child(backup);
        }
        if let Some(periods) = self.periods.clone() {
            return content.child(Button::new("periods-close-screen")
                .label("Luk moms og perioder og genindlæs saldobalance")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.periods = None;
                    if let Some(slug) = this.selected.clone() { this.load(slug, cx); }
                }))).child(periods);
        }
        if let Some(budget) = self.budget.clone() {
            return content.child(Button::new("budget-close").track_focus(&self.close_focus)
                .label("Luk budget og genindlæs saldobalance")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.budget = None;
                    if let Some(slug) = this.selected.clone() { this.load(slug, cx); }
                }))).child(budget);
        }
        if let Some(mileage) = self.mileage.clone() {
            return content.child(Button::new("mileage-close").track_focus(&self.close_focus)
                .label("Luk kørsel og genindlæs saldobalance")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.mileage = None;
                    if let Some(slug) = this.selected.clone() { this.load(slug, cx); }
                }))).child(mileage);
        }
        if let Some(payables) = self.payables.clone() {
            return content.child(Button::new("payables-close").track_focus(&self.close_focus).label("Luk leverandørgæld og genindlæs saldobalance").on_click(cx.listener(|this,_,_,cx|{this.payables=None;if let Some(slug)=this.selected.clone(){this.load(slug,cx);}}))).child(payables);
        }
        if let Some(documents) = self.documents.clone() {
            return content
                .child(
                    Button::new("documents-close").track_focus(&self.close_focus)
                        .label("Luk bilag og genindlæs saldobalance")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.documents = None;
                            if let Some(slug) = this.selected.clone() {
                                this.load(slug, cx);
                            }
                        })),
                )
                .child(documents);
        }
        if let Some(contacts) = self.contacts.clone() {
            return content
                .child(
                    Button::new("contacts-close").track_focus(&self.close_focus)
                        .label("Luk kontakter og genindlæs saldobalance")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.contacts = None;
                            if let Some(slug) = this.selected.clone() {
                                this.load(slug, cx);
                            }
                        })),
                )
                .child(contacts);
        }
        if let Some(bank) = self.bank.clone() {
            return content
                .child(
                    Button::new("bank-close").track_focus(&self.close_focus)
                        .label("Luk bank og genindlæs saldobalance")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.bank = None;
                            if let Some(slug) = this.selected.clone() {
                                this.load(slug, cx);
                            }
                        })),
                )
                .child(bank);
        }
        if let Some(reporting) = self.reporting.clone() {
            return content
                .child(
                    Button::new("reports-close").track_focus(&self.close_focus)
                        .label("Luk rapporter og genindlæs saldobalance")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.reporting = None;
                            if let Some(slug) = this.selected.clone() {
                                this.load(slug, cx);
                            }
                        })),
                )
                .child(reporting);
        }
        if let Some(invoice) = self.invoice.clone() {
            return content
                .child(
                    Button::new("invoice-close").track_focus(&self.close_focus)
                        .label("Luk fakturaer og genindlæs saldobalance")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.invoice = None;
                            if let Some(slug) = this.selected.clone() {
                                this.load(slug, cx);
                            }
                        })),
                )
                .child(invoice);
        }
        if self.session.is_some() && self.selected.is_some() {
            if let Some(journal) = self.journal.clone() {
                content = content
                    .child(
                        Button::new("journal-close").track_focus(&self.close_focus)
                            .label("Luk postering og genindlæs saldobalance")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.journal = None;
                                if let Some(slug) = this.selected.clone() {
                                    this.load(slug, cx);
                                }
                            })),
                    )
                    .child(journal);
                return content;
            }
            content = content.child(
                Button::new("journal-open")
                    .label("Ny postering")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let (Some(session), Some(company)) =
                            (this.session.clone(), this.selected.clone())
                        {
                            let origin = this.connection_origin(cx);
                            let local_identity = match this
                                .managed
                                .as_ref()
                                .map(|m| m.journal_identity())
                                .transpose()
                            {
                                Ok(identity) => identity,
                                Err(error) => {
                                    this.status = error;
                                    cx.notify();
                                    return;
                                }
                            };
                            this.journal = Some(cx.new(|cx| {
                                journal::Journal::new(
                                    session,
                                    origin,
                                    company,
                                    local_identity,
                                    window,
                                    cx,
                                )
                            }));
                            cx.notify();
                        }
                    })),
            );
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("company-setup-open").label("Virksomhedsopsætning · ⌘2").disabled(self.busy)
                .on_click(cx.listener(|this,_,window,cx|this.open_company_setup(&OpenCompanySetup,window,cx))));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("opening-balance-open").label("Åbningsbalance · ⌘3").disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| this.open_opening_balance(&OpenOpeningBalance, window, cx))));
        }
        if self.session.is_some() && self.selected.is_some() {
            content=content.child(Button::new("archive-import-open").label("Importér regnskabshistorik · ⌘5").disabled(self.busy)
                .on_click(cx.listener(|this,_,window,cx|this.open_archive_import(&OpenArchiveImport,window,cx))));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("scheduled-recurring-open").label("Planlagte fakturaer").disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let (Some(session), Some(company)) = (this.session.clone(), this.selected.clone()) {
                        let origin = this.connection_origin(cx);
                        let local = match this.managed.as_ref().map(|m| m.journal_identity()).transpose() {
                            Ok(local) => local,
                            Err(error) => { this.status = error; cx.notify(); return; }
                        };
                        let dispatcher = this.scheduled_dispatcher.clone();
                        this.scheduled_recurring = Some(cx.new(|cx| {
                            let mut screen = scheduled_recurring::ScheduledRecurring::new(session, origin, company, local, window, cx);
                            screen.bind_dispatcher(dispatcher);
                            screen
                        }));
                        cx.notify();
                    }
                })));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("eliminations-open").label("Elimineringer").disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let (Some(session), Some(company)) = (this.session.clone(), this.selected.clone()) {
                        let origin = this.connection_origin(cx);
                        let local = match this.managed.as_ref().map(|m| m.journal_identity()).transpose() {
                            Ok(local) => local,
                            Err(error) => { this.status = error; cx.notify(); return; }
                        };
                        this.eliminations = Some(cx.new(|cx| eliminations::Eliminations::new(session, origin, company, local, window, cx)));
                        cx.notify();
                    }
                })));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("consolidation-open").label("Konsolideret rapport").disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let (Some(session), Some(company)) = (this.session.clone(), this.selected.clone()) {
                        let origin = this.connection_origin(cx);
                        let local = match this.managed.as_ref().map(|m| m.journal_identity()).transpose() {
                            Ok(local) => local,
                            Err(error) => { this.status = error; cx.notify(); return; }
                        };
                        this.consolidation = Some(cx.new(|cx| consolidation::Consolidation::new(session, origin, company, local, window, cx)));
                        cx.notify();
                    }
                })));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("intercompany-open").label("Mellemregningsafstemning").disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let (Some(session), Some(company)) = (this.session.clone(), this.selected.clone()) {
                        let origin = this.connection_origin(cx);
                        let local = match this.managed.as_ref().map(|m| m.journal_identity()).transpose() {
                            Ok(local) => local,
                            Err(error) => { this.status = error; cx.notify(); return; }
                        };
                        this.intercompany = Some(cx.new(|cx| intercompany::Intercompany::new(session, origin, company, local, window, cx)));
                        cx.notify();
                    }
                })));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("ownership-open").label("Ejer- og kontrolforhold").disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let (Some(session), Some(company)) = (this.session.clone(), this.selected.clone()) {
                        let origin = this.connection_origin(cx);
                        let local = match this.managed.as_ref().map(|m| m.journal_identity()).transpose() {
                            Ok(local) => local,
                            Err(error) => { this.status = error; cx.notify(); return; }
                        };
                        this.ownership = Some(cx.new(|cx| ownership::Ownership::new(session, origin, company, local, window, cx)));
                        cx.notify();
                    }
                })));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("corporate-records-open").label("Selskabsdokumenter").disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let (Some(session), Some(company)) = (this.session.clone(), this.selected.clone()) {
                        let origin = this.connection_origin(cx);
                        let local = match this.managed.as_ref().map(|m| m.journal_identity()).transpose() {
                            Ok(local) => local,
                            Err(error) => { this.status = error; cx.notify(); return; }
                        };
                        this.corporate_records = Some(cx.new(|cx| corporate_records::CorporateRecords::new(session, origin, company, local, window, cx)));
                        cx.notify();
                    }
                })));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("workspace-inbox-open").label("Fælles bilagsindbakke").disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let (Some(session), Some(company)) = (this.session.clone(), this.selected.clone()) {
                        let origin = this.connection_origin(cx);
                        let local = match this.managed.as_ref().map(|m| m.journal_identity()).transpose() {
                            Ok(local) => local,
                            Err(error) => { this.status = error; cx.notify(); return; }
                        };
                        this.workspace_inbox = Some(cx.new(|cx| workspace_inbox::WorkspaceInbox::new(session, origin, company, local, window, cx)));
                        cx.notify();
                    }
                })));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("commitments-open").label("Forpligtelser og likviditet").disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let (Some(session), Some(company)) = (this.session.clone(), this.selected.clone()) {
                        let origin = this.connection_origin(cx);
                        let local = match this.managed.as_ref().map(|m| m.journal_identity()).transpose() {
                            Ok(local) => local,
                            Err(error) => { this.status = error; cx.notify(); return; }
                        };
                        this.commitments = Some(cx.new(|cx| commitments::Commitments::new(session, origin, company, local, window, cx)));
                        cx.notify();
                    }
                })));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("parties-open").label("Workspace-parter").disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let (Some(session), Some(company)) = (this.session.clone(), this.selected.clone()) {
                        let origin = this.connection_origin(cx);
                        let local = match this.managed.as_ref().map(|m| m.journal_identity()).transpose() {
                            Ok(local) => local,
                            Err(error) => { this.status = error; cx.notify(); return; }
                        };
                        this.parties = Some(cx.new(|cx| parties::WorkspaceParties::new(session, origin, company, local, window, cx)));
                        cx.notify();
                    }
                })));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("knowledge-open").label("Virksomhedsoplysninger").disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let (Some(session), Some(company)) = (this.session.clone(), this.selected.clone()) {
                        let origin = this.connection_origin(cx);
                        let local = match this.managed.as_ref().map(|m| m.journal_identity()).transpose() {
                            Ok(local) => local,
                            Err(error) => { this.status = error; cx.notify(); return; }
                        };
                        this.company_knowledge = Some(cx.new(|cx| company_knowledge::CompanyKnowledge::new(session, origin, company, local, window, cx)));
                        cx.notify();
                    }
                })));
        }
        if self.session.is_some() {
            content = content.child(Button::new("backup-open").label("Workspace-backup").disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Some(session) = this.session.clone() {
                        let origin = this.connection_origin(cx);
                        let local_public_key = this.managed.as_ref().and_then(|managed| {
                            match managed.backup_public_key() {
                                Ok(key) => Some(key),
                                Err(error) => { this.status = error; None }
                            }
                        });
                        this.backup = Some(cx.new(|cx| backup::Backup::new(session, origin, local_public_key, window, cx)));
                        cx.notify();
                    }
                })));
        }
        if self.session.is_some() && self.selected.is_some() {
            content=content.child(Button::new("annual-open").label("Årsrapport og frister · ⌘6").disabled(self.busy)
                .on_click(cx.listener(|this,_,window,cx|this.open_annual(&OpenAnnual,window,cx))));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("periods-open").label("Perioder og årsafslutning").disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let (Some(session), Some(company)) = (this.session.clone(), this.selected.clone()) {
                        let origin = this.connection_origin(cx);
                        let year = this.year.read(cx).value().to_string();
                        let local = match this.managed.as_ref().map(|m| m.journal_identity()).transpose() {
                            Ok(identity) => identity,
                            Err(error) => { this.status = error; cx.notify(); return; }
                        };
                        this.periods = Some(cx.new(|cx| periods::Periods::new(session, origin, company, year, local, window, cx)));
                        cx.notify();
                    }
                })));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("budget-open").label("Budget").disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let (Some(session), Some(company)) = (this.session.clone(), this.selected.clone()) {
                        let origin = this.connection_origin(cx);
                        let year = this.year.read(cx).value().to_string();
                        let local = match this.managed.as_ref().map(|m| m.journal_identity()).transpose() {
                            Ok(identity) => identity,
                            Err(error) => { this.status = error; cx.notify(); return; }
                        };
                        this.budget = Some(cx.new(|cx| budget::Budget::new(session, origin, company, year, local, window, cx)));
                        cx.notify();
                    }
                })));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("mileage-open").label("Kørsel").disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let (Some(session), Some(company)) = (this.session.clone(), this.selected.clone()) {
                        let origin = this.connection_origin(cx);
                        let year = this.year.read(cx).value().to_string();
                        let local = match this.managed.as_ref().map(|m| m.journal_identity()).transpose() {
                            Ok(identity) => identity,
                            Err(error) => { this.status = error; cx.notify(); return; }
                        };
                        this.mileage = Some(cx.new(|cx| mileage::Mileage::new(session, origin, company, year, local, window, cx)));
                        cx.notify();
                    }
                })));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(Button::new("invoices-open").label("Fakturaer · ⌘1").disabled(self.busy)
                .on_click(cx.listener(|this,_,window,cx|this.open_invoices(&OpenInvoices,window,cx))));
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(
                Button::new("reports-open")
                    .label("Rapporter")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let (Some(session), Some(company)) =
                            (this.session.clone(), this.selected.clone())
                        {
                            let origin = this.connection_origin(cx);
                            let year = this.year.read(cx).value().to_string();
                            this.reporting = Some(cx.new(|cx| {
                                reporting::Reporting::new(
                                    session, company, origin, year, window, cx,
                                )
                            }));
                            cx.notify();
                        }
                    })),
            );
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(
                Button::new("bank-open")
                    .label("Bank")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let (Some(session), Some(company)) =
                            (this.session.clone(), this.selected.clone())
                        {
                            let origin = this.connection_origin(cx);
                            let year = this.year.read(cx).value().to_string();
                            let local = match this
                                .managed
                                .as_ref()
                                .map(|m| m.journal_identity())
                                .transpose()
                            {
                                Ok(local) => local,
                                Err(error) => {
                                    this.status = error;
                                    cx.notify();
                                    return;
                                }
                            };
                            this.bank = Some(cx.new(|cx| {
                                bank::Bank::new(session, origin, company, year, local, window, cx)
                            }));
                            cx.notify();
                        }
                    })),
            );
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(
                Button::new("contacts-open")
                    .label("Kontakter")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let (Some(session), Some(company)) =
                            (this.session.clone(), this.selected.clone())
                        {
                            let origin = this.connection_origin(cx);
                            let local = match this
                                .managed
                                .as_ref()
                                .map(|m| m.journal_identity())
                                .transpose()
                            {
                                Ok(local) => local,
                                Err(error) => {
                                    this.status = error;
                                    cx.notify();
                                    return;
                                }
                            };
                            this.contacts = Some(cx.new(|cx| {
                                contacts::Contacts::new(session, origin, company, local, window, cx)
                            }));
                            cx.notify();
                        }
                    })),
            );
        }
        if self.session.is_some() && self.selected.is_some() {
            content = content.child(
                Button::new("documents-open")
                    .label("Bilag")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let (Some(session), Some(company)) =
                            (this.session.clone(), this.selected.clone())
                        {
                            let origin = this.connection_origin(cx);
                            let local = match this
                                .managed
                                .as_ref()
                                .map(|m| m.journal_identity())
                                .transpose()
                            {
                                Ok(local) => local,
                                Err(error) => {
                                    this.status = error;
                                    cx.notify();
                                    return;
                                }
                            };
                            this.documents = Some(cx.new(|cx| {
                                documents::Documents::new(
                                    session, origin, company, local, window, cx,
                                )
                            }));
                            cx.notify();
                        }
                    })),
            );
        }
        if self.session.is_some() && self.selected.is_some() {
            content=content.child(Button::new("payables-open").label("Leverandørgæld").disabled(self.busy).on_click(cx.listener(|this,_,window,cx|{
                if let(Some(session),Some(company))=(this.session.clone(),this.selected.clone()) {
                    let origin=this.connection_origin(cx);let year=this.year.read(cx).value().to_string();
                    let local=match this.managed.as_ref().map(|m|m.journal_identity()).transpose(){Ok(local)=>local,Err(e)=>{this.status=e;cx.notify();return;}};
                    this.payables=Some(cx.new(|cx|payables::Payables::new(session,origin,company,year,local,window,cx)));cx.notify();
                }
            })));
        }
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
                .child(match report.balanced {
                    Some(true)=>"Debet og kredit stemmer.",
                    Some(false)=>"Debet og kredit stemmer ikke — gennemgå serverens ledger.",
                    None=>"Balancekontrol er ikke tilgængelig for dette arkivgrundlag.",
                });
            content = content.child(table);
        }
        content
    }
}
impl Render for Desktop {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let authenticated = self.session.is_some() && self.selected.is_some();
        let mut header = div()
            .flex_none()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .child(div().text_2xl().child("Rentemester"))
            .child(self.scheduled_dispatcher.clone());
        if authenticated {
            header = header
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(format!(
                            "Virksomhed: {}",
                            self.selected.as_deref().unwrap_or("")
                        ))
                        .child(
                            Button::new("workflow-invoices")
                                .label("Fakturaer · ⌘1")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_invoices(&OpenInvoices, window, cx)
                                })),
                        )
                        .child(
                            Button::new("workflow-company-setup")
                                .label("Virksomhedsopsætning · ⌘2")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_company_setup(&OpenCompanySetup, window, cx)
                                })),
                        )
                        .child(
                            Button::new("workflow-back")
                                .label("Tilbage: fokusér Luk · ⌘0")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.back_to_overview(&BackToOverview, window, cx)
                                })),
                        ),
                )
                .child(self.status.clone());
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .key_context("RentemesterWorkflows")
            .track_focus(&self.workflow_focus)
            .on_action(cx.listener(Self::open_invoices))
            .on_action(cx.listener(Self::open_company_setup))
                .on_action(cx.listener(Self::open_opening_balance))
            .on_action(cx.listener(Self::open_company_creation))
            .on_action(cx.listener(Self::open_archive_import))
            .on_action(cx.listener(Self::open_annual))
            .on_action(cx.listener(Self::open_backup_verify))
            .on_action(cx.listener(Self::back_to_overview))
            .on_action(cx.listener(Self::scroll_workflow_down))
            .on_action(cx.listener(Self::scroll_workflow_up))
            .child(header)
            .child(div().id("desktop-scroll").flex_1().min_h_0().overflow_y_scroll()
                .track_scroll(&self.body_scroll).child(self.render_body(window, cx)))
    }
}
#[derive(Debug, PartialEq, Eq)]
enum WorkflowNavigation {
    Unavailable,
    Wait,
    KeepCurrent,
    FocusExisting,
    Open,
}
fn workflow_navigation(
    authenticated: bool,
    busy: bool,
    has_screen: bool,
    same_screen: bool,
) -> WorkflowNavigation {
    if !authenticated {
        WorkflowNavigation::Unavailable
    } else if busy {
        WorkflowNavigation::Wait
    } else if same_screen {
        WorkflowNavigation::FocusExisting
    } else if has_screen {
        WorkflowNavigation::KeepCurrent
    } else {
        WorkflowNavigation::Open
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
            cx.bind_keys([
                KeyBinding::new("pagedown", ScrollWorkflowDown, Some("RentemesterWorkflows")),
                KeyBinding::new("pageup", ScrollWorkflowUp, Some("RentemesterWorkflows")),
                KeyBinding::new("cmd-1", OpenInvoices, Some("RentemesterWorkflows")),
                KeyBinding::new("cmd-2", OpenCompanySetup, Some("RentemesterWorkflows")),
                KeyBinding::new("cmd-3", OpenOpeningBalance, Some("RentemesterWorkflows")),
                KeyBinding::new("cmd-4", OpenCompanyCreation, Some("RentemesterWorkflows")),
                KeyBinding::new("cmd-5", OpenArchiveImport, Some("RentemesterWorkflows")),
                KeyBinding::new("cmd-6", OpenAnnual, Some("RentemesterWorkflows")),
                KeyBinding::new("cmd-7", OpenBackupVerify, Some("RentemesterWorkflows")),
                KeyBinding::new("cmd-0", BackToOverview, Some("RentemesterWorkflows")),
            ]);
            cx.set_menus([Menu::new("Rentemester"), Menu::new("Arbejdsgange").items([
                MenuItem::action("Rul arbejdsgang ned", ScrollWorkflowDown),
                MenuItem::action("Rul arbejdsgang op", ScrollWorkflowUp),
                MenuItem::action("Fakturaer", OpenInvoices),
                MenuItem::action("Virksomhedsopsætning", OpenCompanySetup),
                MenuItem::action("Åbningsbalance", OpenOpeningBalance),
                MenuItem::action("Opret virksomhed", OpenCompanyCreation),
                MenuItem::action("Importér regnskabshistorik", OpenArchiveImport),
                MenuItem::action("Årsrapport og frister", OpenAnnual),
                MenuItem::action("Kontrollér backup", OpenBackupVerify),
                MenuItem::action("Tilbage til oversigt: fokusér Luk", BackToOverview),
            ])]);

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
    use super::{companies, trial, workflow_navigation, WorkflowNavigation};
    #[test]
    fn workflow_shortcuts_preserve_open_reviews_and_require_current_connection() {
        assert_eq!(
            workflow_navigation(false, false, false, false),
            WorkflowNavigation::Unavailable
        );
        assert_eq!(
            workflow_navigation(false, false, true, true),
            WorkflowNavigation::Unavailable
        );
        assert_eq!(
            workflow_navigation(true, true, false, false),
            WorkflowNavigation::Wait
        );
        assert_eq!(
            workflow_navigation(true, false, true, false),
            WorkflowNavigation::KeepCurrent
        );
        assert_eq!(
            workflow_navigation(true, false, true, true),
            WorkflowNavigation::FocusExisting
        );
        assert_eq!(
            workflow_navigation(true, false, false, false),
            WorkflowNavigation::Open
        );
    }

    use serde_json::json;
    #[test]
    fn remote_report_requires_real_fields_and_preserves_display_amounts() {
        let source = json!({"ok":true,"trialBalance":{"company":{"name":"Synthetic Company","currency":"DKK"},"selectedYear":"2026","periodStart":"2026-01-01","periodEnd":"2026-12-31","rows":[{"accountNo":"X","name":"Synthetic expense","type":"expense","debit":12.34,"credit":0,"balance":12.34}],"totalDebit":12.34,"totalCredit":12.34,"balanced":true}});
        let report = trial(source.clone()).unwrap();
        assert_eq!(report.rows[0][3], "12.34");
        assert_eq!(report.balanced,Some(true));
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
    #[test]
    fn imported_company_overview_preserves_exact_money_and_unavailable_evidence() {
        let mut source=json!({"ok":true,"trialBalance":{"company":{"name":"Synthetic archive","currency":"EUR"},"selectedYear":"2024","source":"archive","periodStart":null,"periodEnd":null,"rows":[{"accountNo":"UNKNOWN","name":"Retained source account","type":null,"debit":"100000000000000.01","credit":"0.00","balance":"100000000000000.01"}],"totalDebit":"100000000000000.01","totalCredit":"100000000000000.01","balanced":true}});
        let report=trial(source.clone()).unwrap();
        assert_eq!(report.rows[0][3],"100000000000000.01");
        assert_eq!(report.rows[0][2],"Uklassificeret");
        assert!(report.title.contains("Arkiv"));
        assert!(report.period.contains("Ikke oplyst"));
        source["trialBalance"]["rows"][0]["balance"]=serde_json::Value::Null;
        source["trialBalance"]["balanced"]=serde_json::Value::Null;
        let report=trial(source.clone()).unwrap();
        assert_eq!(report.balanced,None);
        assert_eq!(report.rows[0][5],"Ikke tilgængeligt");
        source["trialBalance"]["source"]=json!("other");
        assert!(trial(source).is_err());
    }
}
