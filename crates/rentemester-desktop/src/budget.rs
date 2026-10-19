//! Budget UI consumes canonical fiscal periods, comparisons and immutable server revisions.
use crate::{Session, budget_state as state, readable};
use gpui_kit::{
    component::{
        Disableable,
        button::Button,
        checkbox::Checkbox,
        input::{Input, InputState},
    },
    *,
};
use rentemester_client::{Error, Report};
use serde_json::Value;
use state::{Review, Selection, Snapshot};
use std::sync::Arc;
fn read(
    session: &Session,
    company: &str,
    year: &str,
    selection: &Selection,
) -> Result<Snapshot, String> {
    let journal = session
        .runtime
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    // Journal is read only for the stable ledger identity/current-year fallback.
    // A budget-only historical year need not exist in the journal year catalog.
    let year = if year.is_empty() {
        journal["journal"]["selectedYear"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or("Serverens regnskabsår mangler")?
            .to_owned()
    } else {
        year.to_owned()
    };
    let year = year.as_str();
    let plan = session
        .runtime
        .block_on(session.client.budget(company, year))
        .map_err(readable)?;
    let compare = session
        .runtime
        .block_on(session.client.budget_vs_actual(company, year))
        .map_err(readable)?;
    let dimensions = if plan["budget"]["dimensionActualsAvailable"] == false {
        None
    } else {
        Some(
            session
                .runtime
                .block_on(session.client.budget_dimension_actuals(
                    company,
                    year,
                    selection.dimension.as_deref(),
                    selection.member.as_deref(),
                ))
                .map_err(readable)?,
        )
    };
    let chart = session
        .runtime
        .block_on(session.client.accounts(company))
        .map_err(readable)?;
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let snapshot = Snapshot::parse(
        company,
        year,
        plan,
        compare,
        dimensions,
        chart,
        (me, journal),
    )?;
    if snapshot.dimensions.is_some() && snapshot.selection() != *selection {
        return Err("Serverens dimensionsvalg matcher ikke det eksplicitte filter".into());
    }
    Ok(snapshot)
}
pub struct Budget {
    session: Arc<Session>,
    origin: String,
    company: String,
    year: String,
    local: Option<String>,
    amount: Entity<InputState>,
    notes: Entity<InputState>,
    account: String,
    period: String,
    selection: Selection,
    data: Option<Snapshot>,
    review: Option<Review>,
    reviewed: bool,
    pending: Option<Value>,
    resolved: bool,
    receipt: Option<Value>,
    busy: bool,
    status: String,
}
impl Budget {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        company: String,
        year: String,
        local: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let amount = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Dit planlagte beløb · komma eller punktum, ingen tusindtalsseparator")
        });
        let notes =
            cx.new(|cx| InputState::new(window, cx).placeholder("Valgfrie noter til revisionen"));
        let mut this = Self {
            session,
            origin,
            company,
            year,
            local,
            amount,
            notes,
            account: String::new(),
            period: String::new(),
            selection: Selection::default(),
            data: None,
            review: None,
            reviewed: false,
            pending: None,
            resolved: false,
            receipt: None,
            busy: false,
            status: String::new(),
        };
        this.load(window, cx);
        this
    }
    fn load(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.review = None;
        self.reviewed = false;
        self.resolved = false;
        self.data = None;
        self.status =
            "Henter serverens budgetrevisioner, faktiske beløb, regnskabsår og kontoplan…".into();
        let (session, company, year, selection) = (
            self.session.clone(),
            self.company.clone(),
            self.year.clone(),
            self.selection.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            let pending = state::load()?;
            Ok::<_, String>((read(&session, &company, &year, &selection)?, pending))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok((data,pending))=>{this.year=data.plan["selectedYear"].as_str().unwrap_or("").into();this.data=Some(data);this.pending=pending;this.status="Beløb, perioder, afvigelser og eventuelle ukendte dimensionsbudgetter kommer fra serveren. Budgetrevisioner bogfører ikke.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn prepare(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = self.data.as_ref() else {
            return;
        };
        match Review::prepare(
            data,
            &self.account,
            &self.period,
            self.amount.read(cx).value().as_ref(),
            self.notes.read(cx).value().as_ref(),
        ) {
            Ok(review) => {
                self.review = Some(review);
                self.reviewed = false;
                self.receipt = None;
                self.status="Gennemgå konto, måned, præcist beløb og noter. Bekræft separat for at tilføje én revision.".into();
            }
            Err(e) => self.status = e,
        }
        cx.notify();
    }
    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() || !self.reviewed {
            return;
        }
        let Some(review) = self.review.take() else {
            return;
        };
        self.busy = true;
        self.reviewed = false;
        self.resolved = false;
        self.data = None;
        self.receipt = None;
        self.amount.update(cx, |i, cx| i.set_value("", window, cx));
        self.notes.update(cx, |i, cx| i.set_value("", window, cx));
        self.status = "Genkontrollerer det gennemgåede budget og sender én revision…".into();
        let (session, company, year, origin, local, selection) = (
            self.session.clone(),
            self.company.clone(),
            self.year.clone(),
            self.origin.clone(),
            self.local.clone(),
            self.selection.clone(),
        );
        let task=cx.background_executor().spawn(async move{
   let fresh=read(&session,&company,&year,&selection)?;
   if !review.current(&fresh){return Err("Budget, kontoplan, år, bruger eller ledger ændrede sig. Intet sendt; genlæs og gennemgå igen.".into());}
   let marker=fresh.marker(&origin,&company,local.as_deref());state::begin(&marker)?;
   let response=session.runtime.block_on(session.client.set_budget(&company,review.bytes.clone(),true));
   let receipt=response.as_ref().ok().and_then(|v|review.receipt(v)).cloned();
   let mut message=match response{
    Ok(_) if receipt.is_some()=>"Serveren kvitterede for en budgetrevision.".to_owned(),
    Err(Error::Refused{status})=>{state::finish(&marker,false)?;format!("Serveren afviste (HTTP{status}). Kontrollér aktuelle administratorrettigheder og valgte felter. Intet gentages.")},
    Err(Error::InvalidInput(_))=>{state::finish(&marker,false)?;"SDK afviste før afsendelse. Gennemgå igen; intet gentages.".into()},
    _=>"UDFALD UAFKLARET. Revisionen kan være gemt. Ingen automatisk gentagelse; genlæs og afklar manuelt.".into(),
   };
   let next=read(&session,&company,&year,&selection);
   if let(Some(receipt),Ok(data))=(&receipt,&next){if data.matches_marker(&marker,&origin,&company,local.as_deref())&&review.readback(data,receipt){state::finish(&marker,false)?;message.push_str(" Den præcise kvitterede revision og noter er læst tilbage kanonisk.");}else{message.push_str(" Seneste kanoniske revision matcher ikke kvitteringen; manuel afklaring kræves.");}}
   else if receipt.is_some(){message.push_str(" Kanonisk genlæsning fejlede; manuel afklaring kræves.");}
   Ok::<_,String>((message,receipt,next,state::load()?))
  });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((message, receipt, next, pending)) => {
                        this.status = message;
                        this.receipt = receipt;
                        this.pending = pending;
                        match next {
                            Ok(data) => this.data = Some(data),
                            Err(e) => this.status.push_str(&format!(" Genlæsning: {e}")),
                        }
                    }
                    Err(e) => {
                        this.status = e;
                        this.pending = state::load().ok().flatten();
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn resolve(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.resolved {
            return;
        }
        let (Some(marker), Some(data)) = (self.pending.clone(), self.data.as_ref()) else {
            return;
        };
        if !data.matches_marker(&marker, &self.origin, &self.company, self.local.as_deref()) {
            return;
        }
        self.busy = true;
        self.resolved = false;
        let (session, company, origin, local, year, selection) = (
            self.session.clone(),
            self.company.clone(),
            self.origin.clone(),
            self.local.clone(),
            self.year.clone(),
            self.selection.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            let data = read(&session, &company, &year, &selection)?;
            if !data.matches_marker(&marker, &origin, &company, local.as_deref()) {
                return Err("Afklaring kræver oprindelig bruger, ledger og regnskabsår".into());
            }
            state::finish(&marker, true)?;
            Ok::<_, String>(data)
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok(data)=>{this.data=Some(data);this.pending=None;this.status="Manuel afklaring arkiveret uden genafsendelse. Dette er ikke en serverkvittering; en ny revision kræver nye felter og gennemgang.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
}
fn text(v: &Value) -> String {
    v.as_str().map(str::to_owned).unwrap_or_else(|| {
        if v.is_null() {
            "Ukendt".into()
        } else {
            v.to_string()
        }
    })
}
impl Render for Budget {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locked = self.busy || self.review.is_some();
        let mut view=div().flex().flex_col().gap_3().p_4().child(div().text_xl().child("Budget og faktisk")).child(format!("{} · {} · år {}",self.company,self.origin,self.year)).child(self.status.clone())
   .child(Button::new("budget-reload").label("Genlæs kanonisk budget, kontoplan og faktisk").disabled(locked).on_click(cx.listener(|this,_,w,cx|this.load(w,cx))))
   .child(Button::new("budget-current-year").label("Læs serverens aktuelle regnskabsår").disabled(locked).on_click(cx.listener(|this,_,w,cx|{this.year.clear();this.period.clear();this.selection=Selection::default();this.load(w,cx);})))
   .child(Button::new("budget-clear-dimension").label("Fjern dimensionsfilter og genlæs valgt år").disabled(locked).on_click(cx.listener(|this,_,w,cx|{this.selection=Selection::default();this.load(w,cx);})))
            .child("Budget ændrer ingen journalbeløb. Hvert bekræftet forsøg tilføjer en historisk revision; gentagelse er aldrig automatisk.");
        if let Some(r) = &self.receipt {
            view = view.child(format!(
                "Serverkvittering · revision {} · konto {} · måned {} · serverbeløb {}",
                r["id"],
                text(&r["accountNo"]),
                text(&r["period"]),
                r["amount"]
            ));
        }
        let Some(data) = self.data.clone() else {
            return view;
        };
        view = view.child(format!(
            "{} · {} · serverperioder {} — {}",
            text(&data.plan["company"]["name"]),
            text(&data.plan["company"]["currency"]),
            text(&data.plan["periodStart"]),
            text(&data.plan["periodEnd"])
        ));
        for (index, year) in data.plan["fiscalYears"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            if let Some(label) = year["label"].as_str() {
                let label = label.to_owned();
                view = view.child(
                    Button::new(SharedString::from(format!("budget-year-{index}")))
                        .label(format!(
                            "År {} · {} — {}",
                            label,
                            text(&year["start"]),
                            text(&year["end"])
                        ))
                        .disabled(locked)
                        .on_click(cx.listener(move |this, _, w, cx| {
                            this.year = label.clone();
                            this.period.clear();
                            this.selection = Selection::default();
                            this.load(w, cx);
                        })),
                );
            }
        }
        view=view.child(format!("Serverens totaler · budget {} · faktisk {} · afvigelse {}",data.compare["totalBudget"],data.compare["totalActual"],data.compare["totalVariance"]))
   .child("På kontolinjerne er positiv afvigelse under budget for udgifter og over mål for indtægter. Afvigelsesandel vises direkte (1 = 100%); nulgrundlag er ukendt. Ingen lokal beregning.");
        for row in data.compare["lines"].as_array().into_iter().flatten() {
            view = view.child(format!(
                "Konto {} {} · {} · budget {} · faktisk {} · afvigelse {} · andel {}",
                text(&row["accountNo"]),
                text(&row["accountName"]),
                text(&row["period"]),
                row["budget"],
                row["actual"],
                row["variance"],
                text(&row["variancePercent"])
            ));
        }
        view = view.child(div().text_lg().child("Seneste kanoniske budgetrevisioner"));
        for row in data.plan["lines"].as_array().into_iter().flatten() {
            view = view.child(format!(
                "Revision {} · konto {} · {} · beløb {} · noter {} · oprettet {}",
                row["id"],
                text(&row["accountNo"]),
                text(&row["period"]),
                row["amount"],
                text(&row["notes"]),
                text(&row["createdAt"])
            ));
        }
        view = view.child(div().text_lg().child("Dimensionsaktualer mod konto-budget"));
        if let Some(d) = &data.dimensions {
            view=view.child("Vælg en eksplicit dimension eller medlem. Konto-budget gælder hele kontoen; dimensionsbudget og variance er ukendte uden en aktuel reviewet fordeling.");
            for (index, option) in d["dimensionOptions"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                if let Ok(selection) = Selection::from_option(option) {
                    view = view.child(
                        Button::new(SharedString::from(format!("budget-dimension-{index}")))
                            .label(format!(
                                "{} · dimension {} · medlem {}",
                                text(&option["label"]),
                                text(&option["dimensionId"]),
                                text(&option["memberId"])
                            ))
                            .disabled(locked)
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.selection = selection.clone();
                                this.load(w, cx);
                            })),
                    );
                }
            }
            if data.selection().dimension.is_some() {
                view = view.child(format!("Serverens eksplicitte valg: {}", d["selection"]));
                for note in d["notes"].as_array().into_iter().flatten() {
                    view = view.child(text(note));
                }
                for row in d["comparison"].as_array().into_iter().flatten() {
                    view=view.child(format!("Konto {} · {} · dimensionsaktual {} · dimensionsbudget {} · variance {} · andre medlemmer {} · ufordelt {} · kontoaktual {} · konto-budget {} · journal-linjer {}",text(&row["accountNo"]),text(&row["period"]),row["actual"],text(&row["budget"]),text(&row["variance"]),row["otherMembersActual"],row["unallocatedActual"],row["accountActual"],row["accountBudget"],row["journalLineIds"]));
                }
            }
        } else {
            view = view.child("Dimensionssammenligning er ikke tilgængelig i serverens rapport.");
        }
        if let Some(marker) = self.pending.clone() {
            view=view.child(format!("UAFKLARET budgetrevision · server {} · virksomhed {} · år {}. Privat markør indeholder kun scope; intet beløb, noter eller genafsendelig anmodning.",text(&marker["origin"]),text(&marker["company"]),text(&marker["year"])))
    .child("Læs seneste revisioner og kontrollér det oprindelige udfald manuelt. En almindelig læsning beviser ikke, hvem der gemte revisionen.");
            if data.matches_marker(&marker, &self.origin, &self.company, self.local.as_deref()) {
                view=view.child(Checkbox::new("budget-resolved").label("Jeg har genlæst og manuelt afklaret den oprindelige budgetændring; ingen genafsendelse.").checked(self.resolved).disabled(self.busy).on_click(cx.listener(|this,checked,_,cx|{this.resolved=*checked;cx.notify();})))
     .child(Button::new("budget-resolve").label("Genlæs og arkivér manuel afklaring").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,w,cx|this.resolve(w,cx))));
            } else {
                view=view.child("Afklaring kræver den oprindelige bruger, server/forankrede lokale identitet, virksomhed, ledger og regnskabsår.");
            }
            return view;
        }
        if data.plan["archived"] != false {
            return view.child("Arkiverede regnskabsår er skrivebeskyttede.");
        }
        let blocked = locked || self.pending.is_some();
        if let Some(review) = &self.review {
            view=view.child(div().text_lg().child("Gennemgå præcis ny budgetrevision"))
    .child(String::from_utf8_lossy(&review.bytes).to_string())
    .child("Serveren validerer kontoen, aktuelle administratorrettigheder og beløbet. Eventuel afrunding vises i serverkvitteringen.")
    .child(Checkbox::new("budget-reviewed").label("Jeg bekræfter denne konto, måned, det indtastede beløb og noter.").checked(self.reviewed).disabled(self.busy).on_click(cx.listener(|this,checked,_,cx|{this.reviewed=*checked;cx.notify();})))
    .child(Button::new("budget-confirm").label("Bekræft og gem én budgetrevision").disabled(self.busy||!self.reviewed).on_click(cx.listener(|this,_,w,cx|this.send(w,cx))))
    .child(Button::new("budget-cancel").label("Tilbage til felter").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.reviewed=false;cx.notify();})));
        } else {
            view = view.child(div().text_lg().child("Vælg konto fra faktisk kontoplan"));
            for (index, account) in data.chart["accounts"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                if let Some(no) = account["accountNo"].as_str() {
                    let no = no.to_owned();
                    view = view.child(
                        Button::new(SharedString::from(format!("budget-account-{index}")))
                            .label(format!(
                                "{} · {} · {} / {}{}",
                                no,
                                text(&account["name"]),
                                text(&account["type"]),
                                text(&account["normalBalance"]),
                                if self.account == no { " · VALGT" } else { "" }
                            ))
                            .disabled(blocked)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.account = no.clone();
                                cx.notify();
                            })),
                    );
                }
            }
            view = view.child("Vælg måned fra serverens regnskabsår");
            for (index, month) in data.plan["periods"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                if let Some(p) = month.as_str() {
                    let p = p.to_owned();
                    view = view.child(
                        Button::new(SharedString::from(format!("budget-period-{index}")))
                            .label(format!(
                                "{}{}",
                                p,
                                if self.period == p { " · VALGT" } else { "" }
                            ))
                            .disabled(blocked)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.period = p.clone();
                                cx.notify();
                            })),
                    );
                }
            }
            view = view
                .child(format!(
                    "Valgt konto {} · måned {}",
                    self.account, self.period
                ))
                .child("Planlagt beløb i virksomhedens valuta")
                .child(Input::new(&self.amount).disabled(blocked))
                .child("Noter")
                .child(Input::new(&self.notes).disabled(blocked))
                .child(
                    Button::new("budget-review")
                        .label("Gennemgå ny revision")
                        .disabled(blocked)
                        .on_click(cx.listener(|this, _, _, cx| this.prepare(cx))),
                );
        }
        view
    }
}
