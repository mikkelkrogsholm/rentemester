//! Explicit readonly source planning and reviewed reporting eliminations; no legal-entity posting or local math.
use crate::{Session, eliminations_state as state, readable};
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
use state::{Action, Plan, Review, Selection, Snapshot};
use std::sync::Arc;
fn read(session: &Session, company: &str, s: &Selection) -> Result<Snapshot, String> {
    if !state::date(&s.as_of) {
        return Err(
            "Angiv en eksplicit pr-dato YYYY-MM-DD; serveren validerer faktisk dato".into(),
        );
    }
    let current = session
        .runtime
        .block_on(session.client.eliminations(company))
        .map_err(readable)?;
    let mappings = session
        .runtime
        .block_on(session.client.intercompany(company))
        .map_err(readable)?;
    let overview = session
        .runtime
        .block_on(session.client.group_eliminations(&s.as_of))
        .map_err(readable)?;
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let journal = session
        .runtime
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    Snapshot::parse(company, s, current, mappings, overview, me, journal)
}
pub struct Eliminations {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    selection: Selection,
    as_of: Entity<InputState>,
    intent_id: Entity<InputState>,
    evidence: Entity<InputState>,
    data: Option<Snapshot>,
    plan: Option<Plan>,
    review: Option<Review>,
    confirmed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
}
impl Eliminations {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        company: String,
        local: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut input =
            |label: &str| cx.new(|cx| InputState::new(window, cx).placeholder(label.to_owned()));
        Self{session,origin,company,local,selection:Selection::default(),as_of:input("Eksplicit pr-dato YYYY-MM-DD"),intent_id:input("Eksplicit nyt eliminerings-ID fra din gennemgåede kilde"),evidence:input("Faktiske evidenceRefs som typed JSON-array af strenge"),data:None,plan:None,review:None,confirmed:false,pending:None,resolved:false,busy:false,status:"Angiv pr-dato, vælg faktisk godkendt mapping og indtast eksplicit ID/kildeevidens før readonly plan. Ingen beløb, fortegn, datoer eller nøgler udledes.".into()}
    }
    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.plan = None;
        self.review = None;
        self.confirmed = false;
        self.resolved = false;
        for input in [&self.intent_id, &self.evidence] {
            input.update(cx, |i, cx| i.set_value("", window, cx));
        }
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.data = None;
        self.clear(window, cx);
        self.status="Genlæser synlige kanoniske elimineringer, aktuelle mappingkilder, datooversigt og adgang…".into();
        let (session, company, selection) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((read(&session, &company, &selection)?, state::load()?))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok((data,pending))=>{this.data=Some(data);this.pending=pending;this.status="Viser kun kanonisk aktuelt synligt kildemateriale og anvendte elimineringer på valgt dato. Ingen selskabsledger ændres.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn fetch_plan(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() || self.review.is_some() {
            return;
        }
        let Some(data) = self.data.clone() else {
            return;
        };
        let input = match state::input(
            &data,
            &self.intent_id.read(cx).value(),
            &self.evidence.read(cx).value(),
        ) {
            Ok(input) => input,
            Err(e) => {
                self.status = e;
                cx.notify();
                return;
            }
        };
        self.busy = true;
        self.plan = None;
        self.status =
            "Genlæser aktuelle kilder og henter readonly serverplan uden at oprette forslag…"
                .into();
        let (session, company, selection) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            let fresh = read(&session, &company, &selection)?;
            if fresh != data {
                return Err(
                    "Kilde/adgang ændrede sig; genlæs og angiv plan igen. Intet foreslået.".into(),
                );
            }
            let proof = session
                .runtime
                .block_on(session.client.elimination_plan(
                    &company,
                    serde_json::to_vec(&input).map_err(|_| "Planinput kunne ikke fastholdes")?,
                ))
                .map_err(readable)?;
            Plan::parse(&fresh, input, proof)
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok(plan)=>{this.plan=Some(plan);this.status="Readonly plan modtaget. Gennemgå hele V1/V2-payload, source/selection/ledger/mapping-hashes, positioner og debitOre/creditOre før separat forslag. Intet skrevet.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn prepare(&mut self, action: Action, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        match Review::prepare(data, action, self.plan.as_ref()) {
            Ok(review) => {
                self.review = Some(review);
                self.confirmed = false;
                self.status="Gennemgå præcis frossen anmodning og hele serverpayload/hash før én separat handling. Intet sendt.".into();
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
        self.clear(window, cx);
        self.status="Genlæser aktuelle rettigheder, kanonisk tilstand og nødvendige readonly kildebeviser før én afsendelse…".into();
        let (session, company, selection, origin, local) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
            self.origin.clone(),
            self.local.clone(),
        );
        let task=cx.background_executor().spawn(async move{
   let fresh=read(&session,&company,&selection)?;if !review.current(&fresh){return Err("Bruger, rettigheder, ledger, mapping, dato eller kanonisk elimineringskilde ændrede sig. Intet sendt; genlæs og gennemgå igen.".into());}
   if review.action.needs_plan(){let input=state::selected_input(&review.payload);let proof=session.runtime.block_on(session.client.elimination_plan(&company,serde_json::to_vec(&input).map_err(|_|"Kildeinput kunne ikke fastholdes")?)).map_err(readable)?;if !review.fresh_plan_matches(&proof){return Err("Readonly kildeplan/hash er nu stale eller ændret. Intet sendt; genlæs og gennemgå den faktiske nye plan.".into());}}
   let marker=review.marker(&origin,local.as_deref());state::begin(&marker)?;let response=session.runtime.block_on(session.client.mutate_elimination(&company,review.action.sdk(),review.body.as_ref().clone(),true));
   let(message,receipt)=match response{
    Ok(v)=>if review.receipt(&v){state::finish(&marker,false)?;(format!("Serveren bekræftede {} med præcis eliminationId/payloadHash/eventHash. Kanonisk tilstand og datooversigt genlæses. Effekten er kun rapportlag; ingen selskabspostering udføres her.",review.action.name()),Some(v))}else{("Kvitteringen mangler præcis ID/hash/event/status. Udfaldet er uafklaret; genlæs manuelt. Ingen genafsendelse.".into(),None)},
    Err(Error::Refused{status})=>{state::finish(&marker,false)?;(format!("Serveren afviste handlingen (HTTP{status}). Kontrollér aktuelle WorkspaceOwner/CompanyAdmin-kilder, præcis payloadHash, mappingens godkendelse, source/selection/ledger hashes, valuta/dato og tidligere status. Godkendelse OG anvendelse kræver anden bruger end oprindelig forslagsstiller; seneste actor afslører ikke nødvendigvis denne. SDK leverer kun HTTP-status; ingen gentagelse eller fortegns-/beløbsgæt."),None)},
    Err(Error::InvalidInput(_))=>{state::finish(&marker,false)?;("Native kilde-JSON/bekræftelse blev afvist før netværkskald. Kontrollér præcise felter og256KiB-grænse. Ingen nøgler/defaults indsættes.".into(),None)},
    _=>("Udfaldet er uafklaret. Genlæs præcis kilde/ID/hash/status manuelt; lignende projektion beviser ikke dette forsøg. Ingen genafsendelse eller automatisk adoption.".into(),None)
   };
   let mut selection=selection;if let Some(v)=&receipt{selection.id=v["eliminationId"].as_str().map(str::to_owned);}let next=read(&session,&company,&selection);let mut message=message;if let(Some(v),Ok(data))=(&receipt,&next){message.push_str(if review.readback(v,data){" Præcis payload/status/hash og eventHash er kanonisk genlæst."}else{" Kanonisk genlæsning matcher ikke kvitteringen. Handlingen er bekræftet; inspicér aktuelle kilder før videre arbejde."});}Ok::<_,String>((message,next,selection,state::load()?))
  });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((message, next, selection, pending)) => {
                        this.status = message;
                        this.selection = selection;
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
impl Render for Eliminations {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locked = self.busy || self.review.is_some();
        let mut view=div().flex().flex_col().gap_3().child(div().text_xl().child("Elimineringer · readonly kildeplan og særskilt gennemgået rapporteffekt"))
   .child(format!("Ankervirksomhed {} · {}",self.company,self.origin)).child(self.status.clone())
   .child("Hver juridisk enhed beholder sin ledger. Plan og rapportlag bruger serverens kanoniske V1/V2-kilder, positioner og præcise ørestrenge. Her gættes ingen fortegn, modpost, valuta, ID, dato eller nøgle.")
   .child(Input::new(&self.as_of).disabled(locked))
   .child(Button::new("eliminations-load-date").label("Hent aktuelle kilder for eksplicit pr-dato").disabled(locked).on_click(cx.listener(|this,_,window,cx|{this.selection=Selection{as_of:this.as_of.read(cx).value().to_string(),id:None,mapping_id:None};this.load(window,cx);})))
   .child(Button::new("eliminations-reload").label("Genlæs aktiv kilde/dato; ryd readonly plan og review").disabled(locked||self.selection.as_of.is_empty()).on_click(cx.listener(|this,_,window,cx|this.load(window,cx))))
   .child(Button::new("eliminations-clear").label("Ryd kildevalg og genlæs aktiv dato").disabled(locked||self.selection.as_of.is_empty()).on_click(cx.listener(|this,_,window,cx|{this.selection.id=None;this.selection.mapping_id=None;this.load(window,cx);})));
        let Some(data) = self.data.clone() else {
            return view;
        };
        view=view.child(format!("Aktiv server-pr-dato {} · bruger {} · workspacerolle {}",data.as_of,data.principal,data.workspace_role)).child("Datofelt ændrer først aktiv dato med Hent. Readonly plan fryser den viste kilde; før skrivning genlæses aktuelt scope og nødvendig plan/hash uden nogen auto-genafsendelse.")
   .child(div().text_lg().child("Aktuelle synlige elimineringer · payload, status, seneste actor og hændelseshash"));
        for line in state::lines(&data.current, "kanoniskEliminering") {
            view = view.child(line);
        }
        for row in data.current["rows"].as_array().into_iter().flatten() {
            if let Some(id) = row["payload"]["id"].as_str() {
                let id = id.to_owned();
                view = view.child(
                    Button::new(SharedString::from(format!("eliminations-select-{id}")))
                        .label(format!(
                            "Vælg {} · {} · payloadHash {}",
                            id,
                            row["status"].as_str().unwrap_or("?"),
                            row["payloadHash"].as_str().unwrap_or("?")
                        ))
                        .disabled(locked)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.selection.id = Some(id.clone());
                            this.load(window, cx);
                        })),
                );
            }
        }
        view = view.child(div().text_lg().child(
            "Aktuelt anvendte rapportelimineringer for valgt pr-dato · fulde serverkilder/blockers",
        ));
        for line in state::lines(&data.overview, "datooversigt") {
            view = view.child(line);
        }
        if let Some(selected) = &data.selected {
            view = view.child(
                div()
                    .text_lg()
                    .child("Valgt kanonisk kilde · fuld V1/V2-payload og alle proof-hashes"),
            );
            for line in state::lines(selected, "valgtEliminering") {
                view = view.child(line);
            }
        }
        if let Some(record) = self.pending.clone() {
            view=view.child(div().text_lg().child("Uafklaret forsøg · ingen genafsendelse"))
    .child(format!("Handling {} · ID {} · payloadHash {} · oprindeligt endpoint {}",record["operation"],record["eliminationId"],record["payloadHash"],record["origin"]))
    .child("Genlæs præcis ID/hash/status. Privat markør gemmer kun scope/identitet/hash, ingen beløb, konti, evidens, dato eller finansiel payload. Matchende projektion alene beviser ikke dette forsøgs udfald.");
            if let Some(id) = record["eliminationId"].as_str() {
                let id = id.to_owned();
                view = view.child(
                    Button::new("eliminations-inspect-pending")
                        .label("Genlæs uafklaret eliminerings kanoniske kilde")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.selection.id = Some(id.clone());
                            this.load(window, cx);
                        })),
                );
            }
            if state::scope(&record, &self.origin, &data, self.local.as_deref()) {
                view=view.child("Oprindelig bruger, anker, ledger og privat profil matcher. Lokalt endpoint kan skifte efter genstart; arkivering sender intet.")
     .child(Checkbox::new("eliminations-resolved").label("Jeg har manuelt afklaret dette præcise forsøg i den viste profil/virksomhed").checked(self.resolved).disabled(self.busy).on_click(cx.listener(|this,v,_,cx|{this.resolved=*v;cx.notify();})))
     .child(Button::new("eliminations-archive").label("Arkivér manuel afklaring uden genafsendelse").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};if this.busy||!this.resolved||!state::scope(record,&this.origin,data,this.local.as_deref()){return;}match state::finish(record,true){Ok(())=>{this.pending=None;this.resolved=false;this.status="Manuel afklaring arkiveret. Intet gentaget.".into();},Err(e)=>this.status=e}cx.notify();})));
            } else {
                view=view.child("Vælg oprindeligt scope/bruger/ledger og privat lokal profil eller nøjagtigt eksternt endpoint for manuel afklaring.");
            }
            return view;
        }
        if self.selection.id.is_some() && data.selected.is_none() {
            return view.child("Elimineringen er ukendt eller ikke aktuelt synlig. Ingen handling tilbydes; skjult årsag udledes ikke. Ryd valget.");
        }
        if let Some(review) = &self.review {
            view = view.child(div().text_lg().child(format!(
                "Gennemgå separat {} · præcis payloadHash {}",
                review.action.name(),
                review.payload_hash
            )));
            for line in state::lines(&review.payload, "frossenKildepayload") {
                view = view.child(line);
            }
            for line in state::lines(&review.display, "præcisAnmodning") {
                view = view.child(line);
            }
            return view.child("V1/V2-positioner, debitOre/creditOre, amountOre, mapping-/selection-/ledgerhashes og evidens er serverens kilder, ikke lokalt beregnede posteringer. Godkendelse og anvendelse kræver anden autentificeret bruger end oprindelig forslagsstiller. Serveren afgør rettigheder, stale/overlap/duplikat og faktisk tilstand; ingen auto-gentagelse.")
    .child(Checkbox::new("eliminations-confirmed").label("Jeg har gennemgået præcis payload/anmodning/hash, kildebeviser og denne ene handling").checked(self.confirmed).disabled(self.busy).on_click(cx.listener(|this,v,_,cx|{this.confirmed=*v;cx.notify();})))
    .child(Button::new("eliminations-send").label("Bekræft og send gennemgået handling én gang").disabled(self.busy||!self.confirmed).on_click(cx.listener(|this,_,window,cx|this.send(window,cx))))
    .child(Button::new("eliminations-cancel").label("Annullér før afsendelse").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.confirmed=false;cx.notify();})));
        }
        if let Some(selected) = &data.selected {
            view=view.child("Seneste actor er ikke nødvendigvis oprindelig forslagsstiller. Serverens fireøjne-regel gælder både godkendelse og anvendelse. Reject/reverse følger hver sin aktuelle servertilstand; der udføres ingen skjult sessionændring.");
            for (action, status, label) in [
                (
                    Action::Approve,
                    "proposed",
                    "Gennemgå uafhængig godkendelse af præcis payloadHash",
                ),
                (
                    Action::Reject,
                    "proposed",
                    "Gennemgå separat afvisning af valgt forslag",
                ),
                (
                    Action::Apply,
                    "approved",
                    "Gennemgå særskilt anvendelse af godkendt præcis kildepayload",
                ),
                (
                    Action::Reverse,
                    "applied",
                    "Gennemgå særskilt tilbageførsel af anvendt rapporteliminering",
                ),
            ] {
                if selected["status"] == status {
                    view = view.child(
                        Button::new(SharedString::from(format!(
                            "eliminations-review-{}",
                            action.name()
                        )))
                        .label(label)
                        .disabled(self.busy || (action == Action::Approve && data.self_approval()))
                        .on_click(cx.listener(move |this, _, _, cx| this.prepare(action, cx))),
                    );
                }
            }
        }
        view = view.child(
            div()
                .text_lg()
                .child("Readonly kildeplan · vælg faktisk godkendt mapping"),
        );
        for line in state::lines(&data.mappings, "mappingkilder") {
            view = view.child(line);
        }
        for m in data.mappings["mappings"].as_array().into_iter().flatten() {
            if m["status"] == "approved" {
                if let Some(id) = m["mapping"]["id"].as_str() {
                    let id = id.to_owned();
                    view = view.child(
                        Button::new(SharedString::from(format!("eliminations-map-{id}")))
                            .label(format!("Vælg godkendt mapping {id} · {}", m["mappingHash"]))
                            .disabled(locked)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.selection.mapping_id = Some(id.clone());
                                this.load(window, cx);
                            })),
                    );
                }
            }
        }
        if let Some(m) = &data.mapping {
            for line in state::lines(m, "valgtMapping") {
                view = view.child(line);
            }
        } else {
            view = view.child(
                "Vælg eksplicit en synlig godkendt mapping; ingen første række vælges automatisk.",
            );
        }
        view = view
            .child(Input::new(&self.intent_id).disabled(locked || self.plan.is_some()))
            .child(Input::new(&self.evidence).disabled(locked || self.plan.is_some()))
            .child(
                Button::new("eliminations-plan")
                    .label("Hent readonly plan for præcis mapping/ID/dato/evidens")
                    .disabled(locked || data.mapping.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.fetch_plan(cx))),
            );
        if self.plan.is_some() {
            view = view.child(
                Button::new("eliminations-clear-plan")
                    .label("Ryd readonly plan og redigér eksplicit input")
                    .disabled(locked)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.plan = None;
                        cx.notify();
                    })),
            );
        }
        if let Some(plan) = &self.plan {
            view = view.child(
                div()
                    .text_lg()
                    .child("Readonly plan modtaget · gennemgå fuld kilde før forslag"),
            );
            for line in state::lines(&plan.input, "eksplicitPlaninput") {
                view = view.child(line);
            }
            for line in state::lines(&plan.proof, "serverplan") {
                view = view.child(line);
            }
            view = view.child(
                Button::new("eliminations-review-propose")
                    .label("Gennemgå separat forslag med præcis expectedPayloadHash")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Propose, cx))),
            );
        }
        view
    }
}
