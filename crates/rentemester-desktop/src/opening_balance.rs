//! Native opening adoption. Root declares opening_balance/state/attempt modules and recreates
//! this entity on session/company/local-profile changes. Financial rules remain server-owned.
use crate::{Session, opening_balance_attempt::Attempt, opening_balance_state as state, readable};
use gpui_kit::{
    component::{
        Disableable,
        button::Button,
        checkbox::Checkbox,
        input::{Input, InputState},
        select::{SearchableVec, Select, SelectState},
    },
    *,
};
use rentemester_client::{Error, Report};
use serde_json::Value;
use std::{collections::BTreeSet, io::Read, sync::Arc};
type Choices = SelectState<SearchableVec<SharedString>>;
struct Row {
    id: usize,
    account: Entity<Choices>,
    debit: Entity<InputState>,
    credit: Entity<InputState>,
    text: Entity<InputState>,
}
struct Readback {
    data: state::Snapshot,
    archives: Vec<Value>,
}
fn read(session: &Session, company: &str) -> Result<Readback, String> {
    let profile = session
        .runtime
        .block_on(session.client.company_profile(company))
        .map_err(readable)?;
    let chart = session
        .runtime
        .block_on(session.client.accounts(company))
        .map_err(readable)?;
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let opening = session
        .runtime
        .block_on(session.client.opening_balance(company))
        .map_err(readable)?;
    let mut journal = session
        .runtime
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    if let Some(date) = opening["openingBalance"]["cutOverDate"].as_str() {
        let year = state::journal_year(&journal, date)
            .ok_or("Åbningsposteringens regnskabsår kunne ikke bekræftes. Genlæs virksomheden.")?;
        journal = session
            .runtime
            .block_on(
                session
                    .client
                    .report(company, Report::Journal, Some(&year), None),
            )
            .map_err(readable)?;
    }
    let archives = session
        .runtime
        .block_on(session.client.archives(company, None))
        .map_err(readable)?;
    if archives["ok"] != true {
        return Err("Arkivhenvisninger kunne ikke bekræftes fra serveren.".into());
    }
    let archives = archives["years"]
        .as_array()
        .cloned()
        .ok_or("Serverens arkivoversigt kunne ikke læses.")?;
    let data = state::Snapshot::parse(company, profile, chart, me, journal, opening)?;
    Ok(Readback { data, archives })
}
fn key() -> Result<String, String> {
    let mut bytes = [0; 24];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|_| "En sikker forsøgsnøgle kunne ikke oprettes; intet sendt.")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn message(value: &Value) -> String {
    let errors = value["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(" · ");
    if errors.is_empty() {
        value["message"].as_str().unwrap_or("Serveren afviste åbningsbalancen. Kontrollér dato, konti, beløb, perioder og rettigheder.").to_owned()
    } else {
        errors
    }
}
fn known_refusal(error: &Error) -> bool {
    matches!(
        error,
        Error::Refused {
            status: 400 | 401 | 403 | 404 | 409 | 422
        } | Error::InvalidInput(_)
    )
}
fn recovered(data: &state::Snapshot, attempt: &Attempt) -> Result<state::Review, String> {
    let body: Value = serde_json::from_slice(&attempt.body)
        .map_err(|_| "Det gemte forsøg kunne ikke læses; det bevares.")?;
    Ok(state::Review {
        before: data.clone(),
        input: body["input"].clone(),
        plan: attempt.plan.clone(),
    })
}
fn record_proven(data: &state::Snapshot, review: &state::Review, company: &str) -> bool {
    review.canonical(&serde_json::json!({"ok":true,"openingBalance":data.opening}))
        && state::journal_matches(
            &serde_json::json!({"ok":true,"journal":data.journal}),
            &data.opening,
            company,
            &data.ledger,
        )
}
struct Outcome {
    status: String,
    readback: Result<Readback, String>,
    pending: Result<Option<Attempt>, String>,
    receipt: Option<Value>,
}
pub struct OpeningBalance {
    entry_focus: FocusHandle,
    entry_focus_requested: bool,
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    data: Option<state::Snapshot>,
    archives: Vec<Value>,
    citations: BTreeSet<i64>,
    accounts: Vec<(String, String)>,
    date: Entity<InputState>,
    note: Entity<InputState>,
    rows: Vec<Row>,
    next_row: usize,
    review: Option<state::Review>,
    pending: Option<Attempt>,
    reviewed: bool,
    reconciled: bool,
    rebind_confirmed: bool,
    receipt: Option<Value>,
    busy: bool,
    status: String,
}
impl OpeningBalance {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        company: String,
        local: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let date =
            cx.new(|cx| InputState::new(window, cx).placeholder("Overtagelsesdato · ÅÅÅÅ-MM-DD"));
        let note = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Note om åbningsgrundlaget · valgfri")
        });
        let mut this = Self {
            entry_focus: cx.focus_handle(),
            entry_focus_requested: false,
            session,
            origin,
            company,
            local,
            data: None,
            archives: vec![],
            citations: BTreeSet::new(),
            accounts: vec![],
            date,
            note,
            rows: vec![],
            next_row: 1,
            review: None,
            pending: None,
            reviewed: false,
            reconciled: false,
            rebind_confirmed: false,
            receipt: None,
            busy: false,
            status: String::new(),
        };
        this.add_row(window, cx);
        this.add_row(window, cx);
        this.load(window, cx);
        this
    }
    pub fn focus_entry(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.entry_focus_requested = true;
        cx.notify();
    }
    fn add_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() || self.review.is_some() {
            return;
        }
        let labels = SearchableVec::new(
            self.accounts
                .iter()
                .map(|(label, _)| SharedString::from(label.clone()))
                .collect::<Vec<_>>(),
        );
        let account = cx.new(|cx| SelectState::new(labels, None, window, cx).searchable(true));
        let debit = cx.new(|cx| InputState::new(window, cx).placeholder("Debet · fx 100.00"));
        let credit = cx.new(|cx| InputState::new(window, cx).placeholder("Kredit · fx 100.00"));
        let text = cx.new(|cx| InputState::new(window, cx).placeholder("Linjetekst · valgfri"));
        self.rows.push(Row {
            id: self.next_row,
            account,
            debit,
            credit,
            text,
        });
        self.next_row += 1;
        cx.notify();
    }
    fn install(&mut self, readback: Readback, window: &mut Window, cx: &mut Context<Self>) {
        self.accounts = readback.data.chart["accounts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|a| a["active"] == true)
            .filter_map(|a| {
                Some((
                    format!("{} · {}", a["accountNo"].as_str()?, a["name"].as_str()?),
                    a["accountNo"].as_str()?.to_owned(),
                ))
            })
            .collect();
        for row in &self.rows {
            let labels = SearchableVec::new(
                self.accounts
                    .iter()
                    .map(|(label, _)| SharedString::from(label.clone()))
                    .collect::<Vec<_>>(),
            );
            row.account
                .update(cx, |s, cx| s.set_items(labels, window, cx));
        }
        self.archives = readback.archives;
        self.data = Some(readback.data);
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.data = None;
        self.review = None;
        self.reviewed = false;
        self.reconciled = false;
        self.rebind_confirmed = false;
        self.receipt = None;
        self.status = "Henter åbningsbalance, kontoplan, bruger og bogførte poster…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((
                read(&session, &company)?,
                Attempt::load(&crate::opening_balance_attempt::directory())?,
            ))
        });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,window,cx|{
            this.busy=false;match result {Ok((readback,pending))=>{this.install(readback,window,cx);this.pending=pending;if let(Some(data),Some(attempt))=(&this.data,&this.pending){if attempt.matches(&this.origin,&this.company,&data.principal,&data.ledger,this.local.as_deref()){match recovered(data,attempt){Ok(review)=>this.review=Some(review),Err(e)=>{this.status=e;cx.notify();return;}}}this.status="Et tidligere forsøg er uafklaret. Genlæs først udfaldet; intet sendes automatisk.".into();}else{this.status="Angiv selv overtagelsesdato og beløb. Serveren kontrollerer konti, balance og perioder før bogføring.".into();}},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn prepare(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(before) = self.data.clone() else {
            return;
        };
        let lines = self
            .rows
            .iter()
            .map(|row| state::LineInput {
                account_no: row
                    .account
                    .read(cx)
                    .selected_value()
                    .and_then(|label| {
                        self.accounts
                            .iter()
                            .find(|(name, _)| name == label.as_ref())
                            .map(|(_, no)| no.clone())
                    })
                    .unwrap_or_default(),
                debit: row.debit.read(cx).value().to_string(),
                credit: row.credit.read(cx).value().to_string(),
                text: row.text.read(cx).value().to_string(),
            })
            .collect::<Vec<_>>();
        let bytes = match state::input(
            &self.date.read(cx).value(),
            &lines,
            &self.note.read(cx).value(),
            &self.citations.iter().copied().collect::<Vec<_>>(),
            &before,
        ) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.status = e;
                cx.notify();
                return;
            }
        };
        self.busy = true;
        self.reviewed = false;
        self.status = "Serveren gennemgår de valgte åbningslinjer…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let task = cx.background_executor().spawn(async move {
            let response = session
                .runtime
                .block_on(session.client.opening_balance_plan(&company, bytes.clone()))
                .map_err(readable)?;
            state::Review::parse(before, &bytes, response)
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(review) => {
                        this.review = Some(review);
                        this.status =
                            "Gennemgå serverens plan. Der er endnu ikke bogført noget.".into();
                    }
                    Err(e) => this.status = e,
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn reconcile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(attempt) = self.pending.clone() else {
            return;
        };
        self.busy = true;
        self.reconciled = false;
        self.reviewed = false;
        self.status = "Genlæser det kanoniske udfald uden at sende igen…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let task = cx
            .background_executor()
            .spawn(async move { read(&session, &company) });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,window,cx|{this.busy=false;match result{Ok(readback)=>{
            if !attempt.matches(&this.origin,&this.company,&readback.data.principal,&readback.data.ledger,this.local.as_deref()){this.status="Forsøget tilhører en anden server, bruger eller virksomhed. Originalen bevares.".into();cx.notify();return;}
            match recovered(&readback.data,&attempt){Ok(review)=>{let present=!readback.data.opening.is_null();if present&&!record_proven(&readback.data,&review,&this.company){this.status="Den registrerede åbningsbalance svarer ikke til det gemte forsøg. Originalen bevares; der sendes ikke igen.".into();}else{this.reconciled=true;this.review=Some(review);this.status=if present{"Den bogførte åbningsbalance og journal er fundet. Bekræft særskilt hentning af den oprindelige kvittering med samme forsøg."}else{"Ingen åbningsbalance er registreret ved denne læsning. Et nyt, eksplicit forsøg bruger den oprindelige anmodning og nøgle og kan bogføre nu."}.into();}},Err(e)=>this.status=e}this.install(readback,window,cx);
        },Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.reviewed {
            return;
        }
        let Some(review) = self.review.clone() else {
            return;
        };
        let old = self.pending.clone();
        if old.is_some() && !self.reconciled {
            return;
        }
        if old.is_none()
            && (review.plan["status"] != "ready"
                || !review.plan["existing"].is_null()
                || review.plan["blockers"]
                    .as_array()
                    .is_none_or(|a| !a.is_empty()))
        {
            return;
        }
        self.busy = true;
        self.reviewed = false;
        self.receipt = None;
        self.status = "Sender det gennemgåede forsøg én gang…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let origin = self.origin.clone();
        let local = self.local.clone();
        let task=cx.background_executor().spawn(async move{
            let fresh=read(&session,&company)?;
            let was_pending=old.is_some();
            let attempt=if let Some(attempt)=old {if !attempt.matches(&origin,&company,&fresh.data.principal,&fresh.data.ledger,local.as_deref()){return Err("Forsøgets oprindelige bruger, server eller virksomhed er ændret. Intet sendt.".into());}if !fresh.data.opening.is_null()&&!record_proven(&fresh.data,&review,&company){return Err("Den aktuelle åbningsbalance matcher ikke forsøget. Intet sendt.".into());}attempt}else{
                if !review.current(&fresh.data){return Err("Virksomhedens grundlag er ændret siden planen. Genlæs og gennemgå igen; intet sendt.".into());}
                let attempt=Attempt{origin:origin.clone(),company:company.clone(),principal:fresh.data.principal.clone(),ledger_uuid:fresh.data.ledger.clone(),local_identity:local.clone(),body:review.apply_body(&key()?)?,plan:review.plan.clone()};attempt.save(&crate::opening_balance_attempt::directory())?;attempt};
            let response=session.runtime.block_on(session.client.opening_balance_apply(&company,attempt.body.clone(),true));
            let mut status="UDFALD UAFKLARET. Forsøget bevares privat. Genlæs udfaldet før en eventuel, særskilt gentagelse med samme anmodning.".to_owned();let mut receipt=None;let readback=read(&session,&company);
            match response {
                Ok(value) if review.receipt(&value)=>{if let Ok(data)=&readback{if record_proven(&data.data,&review,&company)&&value["openingBalance"]==data.data.opening{attempt.finish(&crate::opening_balance_attempt::directory())?;status="Åbningsbalancen er bogført. Kvitteringen er bekræftet mod den aktuelle åbningsbalance og journal.".into();receipt=Some(value);}}},
                Ok(value) if value["ok"]==false=>{if !was_pending{attempt.finish(&crate::opening_balance_attempt::directory())?;}status=message(&value);},
                Err(error) if !was_pending&&known_refusal(&error)=>{attempt.finish(&crate::opening_balance_attempt::directory())?;status=readable(error);},_=>{}
            }
            Ok::<_,String>(Outcome{status,readback,pending:Attempt::load(&crate::opening_balance_attempt::directory()),receipt})
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, window, cx| {
                this.busy = false;
                this.reconciled = false;
                match result {
                    Ok(outcome) => {
                        this.status = outcome.status;
                        this.receipt = outcome.receipt;
                        match outcome.pending {
                            Ok(pending) => this.pending = pending,
                            Err(e) => {
                                this.data = None;
                                this.status = e;
                                cx.notify();
                                return;
                            }
                        }
                        match outcome.readback {
                            Ok(data) => this.install(data, window, cx),
                            Err(e) => {
                                this.data = None;
                                this.status.push_str(&format!(" Genlæsning fejlede: {e}"));
                            }
                        }
                        if this.pending.is_none() {
                            this.review = None;
                        }
                    }
                    Err(e) => {
                        this.status = e;
                        match Attempt::load(&crate::opening_balance_attempt::directory()) {
                            Ok(pending) => this.pending = pending,
                            Err(e) => {
                                this.data = None;
                                this.status = e;
                            }
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
impl OpeningBalance {
    fn rebind(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.rebind_confirmed {
            return;
        }
        let Some(attempt) = self.pending.clone() else {
            return;
        };
        self.busy = true;
        let session = self.session.clone();
        let company = self.company.clone();
        let origin = self.origin.clone();
        let local = self.local.clone();
        let task = cx.background_executor().spawn(async move {
            let data = read(&session, &company)?;
            let rebound = attempt.rebind(
                &crate::opening_balance_attempt::directory(),
                &origin,
                &company,
                &data.data.principal,
                &data.data.ledger,
                local.as_deref(),
            )?;
            Ok::<_, String>((data, rebound))
        });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,window,cx|{this.busy=false;this.reconciled=false;this.reviewed=false;this.rebind_confirmed=false;match result{Ok((data,attempt))=>{this.review=recovered(&data.data,&attempt).ok();this.pending=Some(attempt);this.install(data,window,cx);this.status="Det samme lokale forsøg er knyttet til den aktuelle serveradresse. Genlæs nu udfaldet; intet er sendt.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn review_details(&self, review: &state::Review) -> Div {
        let currency = review.plan["sourceBasis"]["currency"]
            .as_str()
            .unwrap_or("Valuta ikke bekræftet");
        let mut view=div().flex().flex_col().gap_2().child(div().text_lg().child("Gennemgå åbningsbalancen"))
            .child(format!("Overtagelsesdato: {} · Valuta: {currency}",review.input["cutOverDate"].as_str().unwrap_or("ikke bekræftet")))
            .child("Åbningsbeløbene er dine valgte saldi. Arkivbeløb overføres ikke automatisk. Rapporter før overtagelsesdatoen er ikke dækket af denne åbningspostering.");
        for line in review.input["lines"].as_array().into_iter().flatten() {
            let account = line["accountNo"].as_str().unwrap_or("?");
            let name = review.before.chart["accounts"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|a| a["accountNo"] == account)
                .and_then(|a| a["name"].as_str())
                .unwrap_or("Kontonavn ikke bekræftet");
            view = view.child(format!(
                "{account} · {name} · Debet {} · Kredit {} {currency}",
                line["debitAmount"].as_str().unwrap_or("?"),
                line["creditAmount"].as_str().unwrap_or("?")
            ));
            if let Some(text) = line["text"].as_str() {
                view = view.child(format!("Linjetekst: {text}"));
            }
        }
        if let Some(note) = review.input["note"].as_str() {
            view = view.child(format!("Note: {note}"));
        }
        view = view.child(format!(
            "Serverens totaler · Debet {} · Kredit {} {currency}",
            review.plan["totals"]["debit"]
                .as_str()
                .unwrap_or("ikke bekræftet"),
            review.plan["totals"]["credit"]
                .as_str()
                .unwrap_or("ikke bekræftet")
        ));
        for source in review.plan["sourceBasis"]["sourceArchives"]
            .as_array()
            .into_iter()
            .flatten()
        {
            view=view.child(format!("Valgt arkivhenvisning: import {} · valuta {}. Henvisningen dokumenterer kilden, ikke en beregnet åbningssaldo.",source["importId"],source["declaredCurrency"].as_str().unwrap_or("ikke bekræftet")));
        }
        if review.plan["sourceBasis"]["sourceArchives"]
            .as_array()
            .is_some_and(Vec::is_empty)
        {
            view = view.child("Ingen arkivhenvisninger valgt.");
        }
        for blocker in review.plan["blockers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            view = view.child(format!("Kan ikke bogføres: {blocker}"));
        }
        view
    }
}
impl Render for OpeningBalance {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.entry_focus_requested && !self.busy {
            self.entry_focus_requested = false;
            let focus = self.entry_focus.clone();
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        }
        let mut view = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_xl().child("Åbningsbalance"))
            .child(format!("{} · {}", self.company, self.origin))
            .child(self.status.clone())
            .child(
                Button::new("opening-reload")
                    .track_focus(&self.entry_focus)
                    .label("Genlæs åbningsbalance og kontoplan")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.load(window, cx))),
            );
        let Some(data) = self.data.clone() else {
            return view;
        };
        view = view.child(format!(
            "Virksomhed: {} · Valuta: {}",
            data.profile["name"].as_str().unwrap_or("ikke bekræftet"),
            data.profile["currency"]
                .as_str()
                .unwrap_or("ikke bekræftet")
        ));
        if !data.opening.is_null() {
            view=view.child(div().text_lg().child("Registreret åbningsbalance"))
                .child(format!("Overtagelsesdato: {} · Postering: {}",data.opening["cutOverDate"].as_str().unwrap_or("ikke bekræftet"),data.opening["journalEntryNo"].as_str().unwrap_or("ikke bekræftet")))
                .child(format!("Registreret: {} · Bogført af: {}",data.opening["createdAt"].as_str().unwrap_or("ikke bekræftet"),data.opening["actor"].as_str().unwrap_or("ikke bekræftet")))
                .child("Åbningsbalancen er en særskilt overtagelsespostering. Arkivet bevarer sine egne kilder; beløb før overtagelsen medregnes ikke automatisk i den aktive bogføring.");
        }
        if let Some(attempt) = self.pending.clone() {
            view=view.child(div().text_lg().child("Uafklaret tidligere forsøg"))
                .child("Den oprindelige anmodning er bevaret privat. Der sendes ikke automatisk igen, og en kanonisk læsning erstatter ikke kvitteringen.");
            if !attempt.matches(
                &self.origin,
                &self.company,
                &data.principal,
                &data.ledger,
                self.local.as_deref(),
            ) {
                view = view.child(format!(
                    "Oprindelig server: {} · virksomhed: {}",
                    attempt.origin, attempt.company
                ));
                if attempt.can_rebind(
                    &self.company,
                    &data.principal,
                    &data.ledger,
                    self.local.as_deref(),
                ) {
                    view=view.child(format!("Aktuel lokal server: {}. Kun adressen ændres; det oprindelige forsøg bevares.",self.origin))
                        .child(Checkbox::new("opening-rebind-review").label("Jeg har kontrolleret, at dette er samme lokale virksomhed og bruger").checked(self.rebind_confirmed).on_click(cx.listener(|this,checked,_,cx|{this.rebind_confirmed=*checked;cx.notify();})))
                        .child(Button::new("opening-rebind").label("Bekræft lokal serveradresse").disabled(self.busy||!self.rebind_confirmed).on_click(cx.listener(|this,_,window,cx|this.rebind(window,cx))));
                } else {
                    view=view.child("Genåbn den oprindelige bruger, server og virksomhed for at afklare forsøget.");
                }
                return view;
            }
            view = view.child(
                Button::new("opening-read-first")
                    .label("Genlæs udfaldet uden at sende igen")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.reconcile(window, cx))),
            );
        }
        if let Some(review) = self.review.clone() {
            view = view.child(self.review_details(&review));
            let pending = self.pending.is_some();
            let allowed = if pending {
                self.reconciled
            } else {
                review.ready()
            };
            if pending {
                view=view.child("En bekræftet gentagelse bruger præcis samme anmodning og nøgle. Den kan hente den oprindelige kvittering eller bogføre nu, hvis første forsøg ikke blev gennemført.");
            }
            view=view.child(Checkbox::new("opening-reviewed").label(if pending{"Jeg har genlæst udfaldet og gennemgået det oprindelige forsøg igen"}else{"Jeg har gennemgået dato, konti, beløb og kildehenvisninger for denne virksomhed"}).checked(self.reviewed).on_click(cx.listener(|this,checked,_,cx|{this.reviewed=*checked;cx.notify();})))
                .child(Button::new("opening-apply").label(if pending{"Bekræft præcis samme forsøg"}else{"Bekræft og bogfør åbningsbalance"}).disabled(self.busy||!self.reviewed||!allowed).on_click(cx.listener(|this,_,window,cx|this.send(window,cx))));
            if !pending {
                view = view.child(
                    Button::new("opening-edit")
                        .label("Tilbage til linjerne")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.review = None;
                            this.reviewed = false;
                            cx.notify();
                        })),
                );
            }
            return view;
        }
        if self.pending.is_some() {
            return view;
        }
        if !data.opening.is_null() {
            let journal = serde_json::json!({"ok":true,"journal":data.journal});
            if state::journal_matches(&journal, &data.opening, &self.company, &data.ledger) {
                view = view.child("Den registrerede åbningspostering er fundet i journalen.");
            } else {
                view=view.child("Journalhenvisningen kunne ikke bekræftes. Genlæs, før du anvender oplysningerne.");
            }
            if self.receipt.is_some() {
                view = view.child("Serverens kvittering og den kanoniske bogføring er bekræftet.");
            }
            return view.child("Luk åbningsbalancen og vælg Rapporter for at se regnskabet. Kontroller rapportens dækning fra overtagelsesdatoen.");
        }
        view=view.child("Ingen åbningsbalance er registreret. Vælg selv overtagelsesdato og konti. Beløb angives som fx 100.00; lad den modsatte side være tom. Serveren afgør, om balancen kan bogføres.")
            .child("Overtagelsesdato").child(Input::new(&self.date).disabled(self.busy)).child("Note om grundlaget (valgfri)").child(Input::new(&self.note).disabled(self.busy));
        for (index, row) in self.rows.iter().enumerate() {
            let id = row.id;
            view = view
                .child(div().text_lg().child(format!("Åbningslinje {}", index + 1)))
                .child(
                    Select::new(&row.account)
                        .accessibility_label(SharedString::from(format!("Konto {}", index + 1)))
                        .placeholder("Vælg konto fra kontoplanen")
                        .disabled(self.busy),
                )
                .child("Debet")
                .child(Input::new(&row.debit).disabled(self.busy))
                .child("Kredit")
                .child(Input::new(&row.credit).disabled(self.busy))
                .child("Linjetekst (valgfri)")
                .child(Input::new(&row.text).disabled(self.busy));
            if self.rows.len() > 2 {
                view = view.child(
                    Button::new(("opening-remove", id))
                        .label("Fjern denne linje")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.busy && this.rows.len() > 2 {
                                this.rows.retain(|row| row.id != id);
                                cx.notify();
                            }
                        })),
                );
            }
        }
        view = view.child(
            Button::new("opening-add")
                .label("Tilføj åbningslinje")
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| this.add_row(window, cx))),
        );
        let mut imports = std::collections::BTreeMap::<i64, Vec<String>>::new();
        for archive in &self.archives {
            if let (Some(id), Some(year)) =
                (archive["importId"].as_i64(), archive["fiscalYear"].as_u64())
            {
                imports.entry(id).or_default().push(year.to_string());
            }
        }
        if !imports.is_empty() {
            view=view.child("Arkivhenvisninger (valgfrie). Vælg kun de kilder, der dokumenterer dine egne åbningsbeløb; ingen beløb beregnes eller kopieres her.");
        }
        for (id, years) in imports {
            view = view.child(
                Checkbox::new(("opening-archive", id as usize))
                    .label(SharedString::from(format!(
                        "Arkivimport {id} · regnskabsår {}",
                        years.join(", ")
                    )))
                    .checked(self.citations.contains(&id))
                    .on_click(cx.listener(move |this, checked, _, cx| {
                        if !this.busy {
                            if *checked {
                                this.citations.insert(id);
                            } else {
                                this.citations.remove(&id);
                            }
                            cx.notify();
                        }
                    })),
            );
        }
        view.child(
            Button::new("opening-plan")
                .label("Hent serverens plan til gennemgang")
                .disabled(self.busy || self.accounts.is_empty())
                .on_click(cx.listener(|this, _, _, cx| this.prepare(cx))),
        )
    }
}
