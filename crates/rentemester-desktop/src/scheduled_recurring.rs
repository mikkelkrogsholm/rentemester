//! Explicit queue/review controls. Status, constructor and reopen never dispatch; no timers or background service.
use crate::{Session, readable, scheduled_recurring_state as state};
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
use state::{Action, Review, Snapshot};
use std::sync::Arc;
fn read(session: &Session, company: &str, id: Option<i64>) -> Result<Snapshot, String> {
    let templates = session
        .runtime
        .block_on(session.client.recurring_invoices(company))
        .map_err(readable)?;
    let invoices = session
        .runtime
        .block_on(session.client.invoices(company, None))
        .map_err(readable)?;
    let status = session
        .runtime
        .block_on(session.client.scheduled_recurring(company))
        .map_err(readable)?;
    let (plan, plan_error) = match id {
        Some(id) => match session
            .runtime
            .block_on(session.client.scheduled_recurring_plan(company, id))
        {
            Ok(plan) => (Some(plan), None),
            Err(e) => (None, Some(readable(e))),
        },
        None => (None, None),
    };
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let journal = session
        .runtime
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    Snapshot::parse(
        company, id, templates, invoices, status, plan, plan_error, me, journal,
    )
}
pub struct ScheduledRecurring {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    selected: Option<i64>,
    reason: Entity<InputState>,
    data: Option<Snapshot>,
    review: Option<Review>,
    indexes: Vec<u64>,
    dispatch: Option<u64>,
    consent: bool,
    confirmed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
    receipt: Option<Value>,
}
impl ScheduledRecurring {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        company: String,
        local: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let reason = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Konkret begrundelse for pause/genoptag/annullér/skip")
        });
        let mut this = Self {
            session,
            origin,
            company,
            local,
            selected: None,
            reason,
            data: None,
            review: None,
            indexes: vec![],
            dispatch: None,
            consent: false,
            confirmed: false,
            pending: None,
            resolved: false,
            busy: false,
            status: String::new(),
            receipt: None,
        };
        this.load(window, cx);
        this
    }
    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.review = None;
        self.indexes.clear();
        self.dispatch = None;
        self.consent = false;
        self.confirmed = false;
        self.resolved = false;
        self.reason.update(cx, |i, cx| i.set_value("", window, cx));
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.data = None;
        self.receipt = None;
        self.clear(window, cx);
        self.status="Læser faktiske skabeloner, køstatus, readonly forekomstplan og kanoniske fakturaer. Genlæsning starter intet.".into();
        let (session, company, selected) =
            (self.session.clone(), self.company.clone(), self.selected);
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((read(&session, &company, selected)?, state::load()?))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok((data,pending))=>{this.data=Some(data);this.pending=pending;this.status="Viser serverens aktuelle kø-/kildestatus. Denne visning har eksplicitte review-/dispatchkontroller, ingen timer eller baggrundsservice; åbning og genlæsning udsteder intet.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn prepare(&mut self, action: Action, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        match Review::prepare(
            data,
            action,
            &self.reason.read(cx).value(),
            &self.indexes,
            self.dispatch,
            self.consent,
        ) {
            Ok(review) => {
                self.review = Some(review);
                self.confirmed = false;
                self.status="Gennemgå præcis frossen serverplan, kildehash, valgte forekomster og denne ene handling. Intet sendt.".into();
            }
            Err(e) => self.status = e,
        }
        cx.notify();
    }
    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.confirmed || self.pending.is_some() {
            return;
        }
        let Some(review) = self.review.take() else {
            return;
        };
        self.busy = true;
        self.data = None;
        self.receipt = None;
        self.clear(window, cx);
        self.status="Genlæser aktuel bruger, rettighed, skabelon, state/planHash og fakturakilder før én afsendelse…".into();
        let (session, company, selected, origin, local) = (
            self.session.clone(),
            self.company.clone(),
            self.selected,
            self.origin.clone(),
            self.local.clone(),
        );
        let task=cx.background_executor().spawn(async move{
   let fresh=read(&session,&company,selected)?;if !review.current(&fresh){return Err("Bruger, adgang, ledger, skabelon, dato, kø eller kanonisk plan ændrede sig. Intet sendt; genlæs og gennemgå igen.".into());}
   let marker=review.marker(&origin,local.as_deref());state::begin(&marker)?;let response=session.runtime.block_on(session.client.mutate_scheduled_recurring(&company,review.action.sdk(),review.body.as_ref().clone(),true));
   let(message,receipt)=match response{
    Ok(v)=>if review.receipt(&v){state::finish(&marker,false)?;(if review.action==Action::Dispatch{if v["dispatch"]["status"]=="failed"{"Serveren registrerede et kendt mislykket dispatch. Fejlen er terminal i denne leverance; ingen automatisk gentagelse. Kontrollér fuld kvittering, køhistorik og fakturaer.".into()}else{"Serveren bekræftede dispatch. Faktisk generation/udstedelse og dokument/PDF følger kvitteringen: UDSTEDT er ikke bogført og ikke sendt. Kanoniske fakturaer og køstatus genlæses.".into()}}else{format!("Serveren bekræftede {}. Catch-up sætter kun valgte forekomster i kø; skip udsteder intet. Aktivering er samtykke/køtilstand, ikke bevis på afsendelse eller baggrundskørsel.",review.action.name())},Some(v))}else{("Kvitteringen mangler kanonisk skabelon-/køstatus. Udfaldet er uafklaret; genlæs manuelt. Ingen genafsendelse.".into(),None)},
    Err(Error::Refused{status})=>{state::finish(&marker,false)?;(format!("Serveren afviste handlingen (HTTP{status}). Kontrollér aktuel CompanyDraftWrite, samme samtykkende bruger ved resume/dispatch, eksakt state/plan/templateHash, serverdato og faktiske kø-/missed-forekomster. Cancelled/known-failed gentages ikke automatisk. SDK leverer kun HTTP-status; ingen skjult retry."),None)},
    Err(Error::InvalidInput(_))=>{state::finish(&marker,false)?;("Native JSON/bekræftelse blev afvist før netværkskald. Kontrollér præcise16KiB-felter, begrundelse og1–100 eksplicitte forekomster. Ingen nøgler eller datoer indsættes.".into(),None)},
    _=>("Udfaldet er uafklaret. Genlæs køhistorik, valgte forekomster og kanoniske fakturaer manuelt før videre handling. Lignende projektion beviser ikke præcis dette forsøg; ingen genafsendelse.".into(),None)
   };
   let next=read(&session,&company,selected);let mut message=message;if let(Some(v),Ok(data))=(&receipt,&next){message.push_str(if review.readback(v,data){" Præcis kanonisk køtilstand er genlæst sammen med fakturalisten."}else{" Den aktuelle køprojektion matcher ikke kvitteringens tilstand; handlingen er bekræftet, inspicér historik før videre arbejde."});}Ok::<_,String>((message,next,receipt,state::load()?))
  });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((message, next, receipt, pending)) => {
                        this.status = message;
                        this.receipt = receipt;
                        this.pending = pending;
                        match next {
                            Ok(data) => this.data = Some(data),
                            Err(e) => this.status.push_str(&format!(" Genlæsning fejlede: {e}")),
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
}
impl Render for ScheduledRecurring {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locked = self.busy || self.review.is_some();
        let mut view=div().flex().flex_col().gap_3().child(div().text_xl().child("Planlagte fakturaer · kø og eksplicitte reviewkontroller"))
   .child(format!("Aktuel virksomhed {} · {}",self.company,self.origin)).child(self.status.clone())
   .child("Denne visning er køstyring og manuel planreview/dispatch, ikke færdig unattended automation. Åbning, genstart og genlæsning starter ingen udstedelse. Lokal lukning/søvn udfører ingen baggrundsservice her. Serverens tidszone/dato, missed-regler og forekomster vises uændret.")
   .child(Button::new("scheduled-recurring-reload").label("Genlæs kilder/kø/readonly plan; ryd review og forekomstvalg").disabled(locked).on_click(cx.listener(|this,_,window,cx|this.load(window,cx))))
   .child(Button::new("scheduled-recurring-clear").label("Ryd skabelonvalg og genlæs").disabled(locked).on_click(cx.listener(|this,_,window,cx|{this.selected=None;this.load(window,cx);})));
        let Some(data) = self.data.clone() else {
            return view;
        };
        view = view
            .child(format!(
                "Bruger {} · aktuel rolle {}",
                data.principal, data.role
            ))
            .child(
                div()
                    .text_lg()
                    .child("Faktiske serverlistede fakturaskabeloner og generationer"),
            );
        for line in state::lines(&data.templates, "skabelonkilder") {
            view = view.child(line);
        }
        for t in data.templates["recurringInvoices"]["templates"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(id) = t["id"].as_i64() {
                view = view.child(
                    Button::new(SharedString::from(format!(
                        "scheduled-recurring-select-{id}"
                    )))
                    .label(format!(
                        "Vælg skabelon {} · {} · aktiv {}",
                        id, t["name"], t["active"]
                    ))
                    .disabled(locked)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected = Some(id);
                        this.load(window, cx);
                    })),
                );
            }
        }
        view = view.child(
            div()
                .text_lg()
                .child("Kanonisk køstatus · seneste/næste, missed, paused, fejl og historik"),
        );
        for line in state::lines(&data.status, "køstatus") {
            view = view.child(line);
        }
        if let Some(plan) = &data.plan {
            view = view.child(div().text_lg().child(
                "Readonly serverplan · fuldt planHash/kilde/stateHash og actual forekomster",
            ));
            for line in state::lines(plan, "readonlyPlan") {
                view = view.child(line);
            }
            if plan["plan"]["truncated"] == true {
                view=view.child("Planen er trunkeret. Kun faktisk viste forekomster kan gennemgås; flere/manglende forekomster udledes ikke. Genlæs og gennemgå hver næste serverplan særskilt.");
            }
        }
        view=view.child(div().text_lg().child("Aktuelle kanoniske fakturaer · udstedelse, postering og afsendelse er forskellige trin"));
        for line in state::lines(&data.invoices, "fakturaliste") {
            view = view.child(line);
        }
        if let Some(error) = &data.plan_error {
            view=view.child(format!("Readonly plan er utilgængelig/blokeret: {error}. Aktuel køstatus bevares til kontrol/pause/annullering; ingen forekomst, dato eller dispatch udledes som erstatning."));
        }
        if let Some(receipt) = &self.receipt {
            view = view.child(
                div()
                    .text_lg()
                    .child("Faktisk serverkvittering · generation/dispatch og dokumentreferencer"),
            );
            for line in state::lines(receipt, "kvittering") {
                view = view.child(line);
            }
        }
        if let Some(record) = self.pending.clone() {
            view=view.child(div().text_lg().child("Uafklaret forsøg · ingen gentagelse"))
    .child(format!("Handling {} · skabelon {} · oprindelig stateHash {} · endpoint {}",record["operation"],record["templateId"],record["stateHash"],record["origin"]))
    .child("Genlæs præcis køhistorik og kanoniske fakturaer. Markøren gemmer kun scope/ID/hash, ingen fakturapayload, beløb, forekomstvalg, begrundelse eller credentials. Matchende projektion alene beviser ikke dette forsøg.");
            if let Some(id) = record["templateId"].as_i64() {
                view = view.child(
                    Button::new("scheduled-recurring-inspect-pending")
                        .label("Genlæs oprindelig skabelons status/plan uden dispatch")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.selected = Some(id);
                            this.load(window, cx);
                        })),
                );
            }
            if state::scope(&record, &self.origin, &data, self.local.as_deref()) {
                view=view.child("Bruger, virksomhed, ledger og privat profil matcher. Lokalt endpoint må ændres ved genstart; arkivering sender intet.")
     .child(Checkbox::new("scheduled-recurring-resolved").label("Jeg har manuelt afklaret dette præcise forsøg i den viste profil/virksomhed").checked(self.resolved).disabled(self.busy).on_click(cx.listener(|this,v,_,cx|{this.resolved=*v;cx.notify();})))
     .child(Button::new("scheduled-recurring-archive").label("Arkivér manuel afklaring uden genafsendelse").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};if this.busy||!this.resolved||!state::scope(record,&this.origin,data,this.local.as_deref()){return;}match state::finish(record,true){Ok(())=>{this.pending=None;this.resolved=false;this.status="Manuel afklaring arkiveret. Intet gentaget.".into();},Err(e)=>this.status=e}cx.notify();})));
            } else {
                view=view.child("Vælg oprindelig bruger, virksomhed/ledger og privat lokal profil eller nøjagtigt eksternt endpoint for afklaring.");
            }
            return view;
        }
        if self.selected.is_some() && (data.selected.is_none() || data.template.is_none()) {
            return view.child("Skabelon eller køkilde er ukendt/ikke synlig. Ingen mutation eller genskabelse tilbydes; ryd valget.");
        }
        let Some(selected) = &data.selected else {
            return view.child("Vælg eksplicit en faktisk fakturaskabelon. Ingen første skabelon vælges automatisk. Opret skabelon gennem den eksisterende server/web/CLI-skabelonfunktion, hvis listen er tom.");
        };
        if let Some(review) = &self.review {
            view = view.child(div().text_lg().child(format!(
                "Gennemgå {} · faktisk skabelon {}",
                review.action.name(),
                review.display["templateId"]
            )));
            for line in state::lines(&review.display, "præcisAnmodning") {
                view = view.child(line);
            }
            return view.child("Hele viste skabelon, serverplan, stateHash/planHash og adgangskontekst er frosset. Enable registrerer samtykke; catch-up sætter kun eksplicit valgte forekomster i kø; dispatch kan udstede en faktura med original/PDF, men hverken bogfører revenue eller sender mail. Ingen genereret nøgle eller automatisk retry.")
    .child(Checkbox::new("scheduled-recurring-confirmed").label("Jeg har gennemgået præcis kildeplan, forekomstvalg og denne ene handling").checked(self.confirmed).disabled(self.busy).on_click(cx.listener(|this,v,_,cx|{this.confirmed=*v;cx.notify();})))
    .child(Button::new("scheduled-recurring-send").label("Bekræft og send gennemgået handling én gang").disabled(self.busy||!self.confirmed).on_click(cx.listener(|this,_,window,cx|this.send(window,cx))))
    .child(Button::new("scheduled-recurring-cancel-review").label("Annullér review før afsendelse").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.confirmed=false;cx.notify();})));
        }
        view = view.child(Input::new(&self.reason).disabled(self.busy));
        let state = selected["state"].as_str().unwrap_or("?");
        if state == "disabled" {
            view=view.child(Checkbox::new("scheduled-recurring-consent").label("Jeg samtykker eksplicit til denne skabelons planlagte udstedelse under min aktuelle bruger; ingen automatisk postering/mail eller unattended lokal service loves").checked(self.consent).disabled(self.busy).on_click(cx.listener(|this,v,_,cx|{this.consent=*v;cx.notify();})))
   .child(Button::new("scheduled-recurring-review-enable").label("Gennemgå aktivering med præcis templateHash").disabled(self.busy||!self.consent).on_click(cx.listener(|this,_,_,cx|this.prepare(Action::Enable,cx))));
        }
        for (action, show, label) in [
            (
                Action::Pause,
                state == "enabled",
                "Gennemgå pause (kø bevares)",
            ),
            (
                Action::Resume,
                state == "paused",
                "Gennemgå genoptag under samme samtykkende bruger",
            ),
            (
                Action::Cancel,
                matches!(state, "enabled" | "paused"),
                "Gennemgå terminal annullering (historik bevares)",
            ),
        ] {
            if show {
                view = view.child(
                    Button::new(SharedString::from(format!(
                        "scheduled-recurring-review-{}",
                        action.name()
                    )))
                    .label(label)
                    .disabled(self.busy || (action == Action::Resume && !data.same_owner()))
                    .on_click(cx.listener(move |this, _, _, cx| this.prepare(action, cx))),
                );
            }
        }
        if state == "cancelled" {
            return view.child("Denne kø er cancelled og terminal. Der tilbydes ingen genstart/automatisk rekreation; kildehistorik og fakturaer bevares.");
        }
        if matches!(state, "enabled" | "paused") {
            view = view.child(div().text_lg().child(
                "Missed · vælg synlige forekomster til særskilt catch-up eller reasoned skip",
            ));
            for occurrence in data.visible_missed() {
                if let Some(index) = occurrence["periodIndex"].as_u64() {
                    view = view.child(
                        Checkbox::new(SharedString::from(format!(
                            "scheduled-recurring-missed-{index}"
                        )))
                        .label(format!(
                            "Forekomst {} · serverdato {}",
                            index, occurrence["issueDate"]
                        ))
                        .checked(self.indexes.contains(&index))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, v, _, cx| {
                            if *v {
                                if !this.indexes.contains(&index) {
                                    this.indexes.push(index);
                                }
                            } else {
                                this.indexes.retain(|n| *n != index);
                            }
                            cx.notify();
                        })),
                    );
                }
            }
            for (action, label) in [
                (
                    Action::CatchUp,
                    "Gennemgå catch-up: sæt kun valgte missed-forekomster i kø",
                ),
                (
                    Action::Skip,
                    "Gennemgå skip af valgte forekomster med begrundelse",
                ),
            ] {
                view = view.child(
                    Button::new(SharedString::from(format!(
                        "scheduled-recurring-review-{}",
                        action.name()
                    )))
                    .label(label)
                    .disabled(self.busy || self.indexes.is_empty())
                    .on_click(cx.listener(move |this, _, _, cx| this.prepare(action, cx))),
                );
            }
        }
        if state == "enabled" {
            view = view.child(div().text_lg().child(
                "Explicit dispatch · kun serverens normalDue eller faktisk køsat forekomst",
            ));
            for occurrence in data.dispatchable() {
                if let Some(index) = occurrence["periodIndex"].as_u64() {
                    view = view.child(
                        Button::new(SharedString::from(format!(
                            "scheduled-recurring-dispatch-choice-{index}"
                        )))
                        .label(format!(
                            "Vælg forekomst {} · faktisk serverdato {}",
                            index, occurrence["issueDate"]
                        ))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.dispatch = Some(index);
                            cx.notify();
                        })),
                    );
                }
            }
            view=view.child(format!("Eksplicit valgt dispatch: {:?}. Samme samtykkende bruger kræves; kørsel udføres kun efter separat planreview/bekræftelse.",self.dispatch))
    .child(Button::new("scheduled-recurring-review-dispatch").label("Gennemgå Kør planlagt forekomst · udstedelse er ikke bogføring/afsendelse").disabled(self.busy||self.dispatch.is_none()||!data.same_owner()).on_click(cx.listener(|this,_,_,cx|this.prepare(Action::Dispatch,cx))));
        }
        view
    }
}
