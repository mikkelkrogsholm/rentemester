//! Source-bound ownership review through the shared HTTP client; no local graph arithmetic or legal inference.
use crate::{Session, ownership_state as state, readable};
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
use state::{Action, Review, Selection, Snapshot};
use std::sync::Arc;
fn read(session: &Session, company: &str, selection: &Selection) -> Result<Snapshot, String> {
    let graph = session
        .runtime
        .block_on(session.client.ownership(company, &selection.as_of))
        .map_err(readable)?;
    let history = session
        .runtime
        .block_on(
            session
                .client
                .ownership_history(company, None, Some(&selection.as_of)),
        )
        .map_err(readable)?;
    let selected = selection
        .id
        .as_ref()
        .map(|id| {
            session
                .runtime
                .block_on(session.client.ownership_history(
                    company,
                    Some(id),
                    Some(&selection.as_of),
                ))
                .map_err(readable)
        })
        .transpose()?;
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let journal = session
        .runtime
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    Snapshot::parse(company, selection, graph, history, selected, me, journal)
}
pub struct Ownership {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    selection: Selection,
    as_of: Entity<InputState>,
    source: Entity<InputState>,
    observed: Entity<InputState>,
    facts: Entity<InputState>,
    decision: String,
    data: Option<Snapshot>,
    review: Option<Review>,
    confirmed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
}
impl Ownership {
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
        Self{session,origin,company,local,selection:Selection::default(),as_of:input("Eksplicit pr-dato YYYY-MM-DD"),source:input("Faktisk kildehenvisning til observationen"),observed:input("Faktisk observedAt: ISO-dato/tid med eksplicit tidszone"),facts:input("Komplet typed JSON-array med kildebelagte ejer-/kontrolfakta"),decision:String::new(),data:None,review:None,confirmed:false,pending:None,resolved:false,busy:false,status:"Angiv en eksplicit pr-dato og hent serverens aktuelle graf og historik. Der vælges ingen dato eller ejerfakta automatisk.".into()}
    }
    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.review = None;
        self.confirmed = false;
        self.resolved = false;
        self.decision.clear();
        for input in [&self.source, &self.observed, &self.facts] {
            input.update(cx, |i, cx| i.set_value("", window, cx));
        }
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.selection.as_of.trim().is_empty() {
            self.status = "Angiv eksplicit pr-dato før læsning".into();
            cx.notify();
            return;
        }
        self.busy = true;
        self.data = None;
        self.clear(window, cx);
        self.status="Henter serverens pr-dato-graf, fulde synlige snapshot-historik og aktuelle kildeidentiteter…".into();
        let (session, company, selection) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((read(&session, &company, &selection)?, state::load()?))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok((data,pending))=>{this.data=Some(data);this.pending=pending;this.status="Kun serverens synlige aktuelle facts/provenance vises. Forslag, uafhængigt review og apply er separate bekræftede handlinger. Intet udledes lokalt af ejerandele, udeladte facts eller juridisk kontrol.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
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
            self.source.read(cx).value().as_ref(),
            self.observed.read(cx).value().as_ref(),
            self.facts.read(cx).value().as_ref(),
            &self.decision,
        ) {
            Ok(review) => {
                self.review = Some(review);
                self.confirmed = false;
                self.status="Gennemgå præcis frossen anmodning og serverkontekst. Intet sendt; ingen diff eller hash beregnes lokalt.".into();
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
        self.status =
            "Genlæser bruger, ledger, graf, historik og præcist snapshot før én afsendelse…".into();
        let (session, company, selection, origin, local) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
            self.origin.clone(),
            self.local.clone(),
        );
        let task=cx.background_executor().spawn(async move{
            let fresh=read(&session,&company,&selection)?;if !review.current(&fresh){return Err("Bruger, adgangskilder, ledger, pr-dato-graf, historik eller snapshot/diff ændrede sig. Intet sendt; genlæs og gennemgå igen.".into());}
            let marker=review.marker(&origin,local.as_deref());state::begin(&marker)?;
            let response=session.runtime.block_on(session.client.mutate_ownership(&company,review.action.sdk(),review.body.as_ref().clone(),true));
            let(message,receipt)=match response{
                Ok(value)=>if let Some(snapshot)=review.receipt(&value){state::finish(&marker,false)?;(match review.action{Action::Propose=>"Forslaget er bekræftet og serverens kanoniske diff/hashes er gemt. Ingen facts er anvendt; en ANDEN autentificeret reviewer skal gennemgå og godkende eller afvise det.".to_string(),Action::Review=>"Serveren bekræftede reviewbeslutningen. Ingen facts er anvendt; apply er fortsat en separat handling med de samme godkendte hashes.".into(),Action::Apply=>format!("Serveren bekræftede apply-status {}. Aktuelle serverfacts og samme snapshot genlæses; ingen konsolidering eller ledgerpostering udføres her.",value["status"].as_str().unwrap_or("?"))},Some(snapshot))}else{("Kvitteringen er ufuldstændig. Udfaldet er uafklaret; genlæs snapshot, historik og graf manuelt. Ingen genafsendelse.".into(),None)},
                Err(Error::Refused{status})=>{state::finish(&marker,false)?;(format!("Serveren afviste handlingen (HTTP{status}). Forslagsstilleren må ikke reviewe eget forslag. Review/apply kræver aktuelle administratorrettigheder til anker og ALLE endpoints; evidenskilders synlighed/følsomhed samt snapshot-/diffhash og live ending-plan genkontrolleres. Genlæs og gennemgå igen; ingen rettigheder eller sessioner ændres automatisk."),None)},
                Err(Error::InvalidInput(_))=>{state::finish(&marker,false)?;("Anmodningens typed source-DTO blev afvist før afsendelse. Kontrollér komplette factfelter, økonomisk eksaktværdi ELLER interval, evidens og præcis bekræftelse. Ingen facts beregnes lokalt.".into(),None)},
                _=>("Udfaldet er uafklaret. Genlæs synlig historik, præcist snapshot/diff og aktuelle pr-dato-facts; afklar manuelt. Ingen genafsendelse, nye nøgler eller automatisk adoption.".into(),None)
            };
            let mut selection=selection;if let Some(snapshot)=&receipt{selection.id=snapshot["snapshotId"].as_str().map(str::to_owned);}
            let next=read(&session,&company,&selection);let mut message=message;
            if let(Some(receipt),Ok(data))=(&receipt,&next){message.push_str(if state::readback(receipt,data){" Præcis kanonisk snapshot, server-diff/hashes og historik er genlæst sammen med den aktuelle graf."}else{" Genlæsningen matcher ikke hele kvitteringen. Handlingen er bekræftet; gennemgå aktuelle hændelser før videre arbejde."});}
            Ok::<_,String>((message,next,selection,state::load()?))
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
impl Render for Ownership {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locked = self.busy || self.review.is_some();
        let mut view=div().flex().flex_col().gap_3().child(div().text_xl().child("Ejer- og kontrolforhold · kilde → uafhængigt review → præcis apply"))
            .child(format!("Aktuel virksomhed {} · {}",self.company,self.origin)).child(self.status.clone())
            .child("Angiv eksplicit pr-dato. Kun serverens facts, delvise dækning, provenance og kildehashes vises. Skjulte endpoints/antal afsløres ikke. Denne skærm udleder ingen ejerandele, juridiske ejere eller konsolideringsresultater.")
            .child(Input::new(&self.as_of).disabled(locked))
            .child(Button::new("ownership-load-date").label("Hent servergraf og historik for denne eksplicitte dato").disabled(locked).on_click(cx.listener(|this,_,window,cx|{this.selection=Selection{as_of:this.as_of.read(cx).value().to_string(),id:None};this.load(window,cx);})))
            .child(Button::new("ownership-reload").label("Genlæs aktiv pr-dato og valgt snapshot; ryd formular/review").disabled(locked||self.selection.as_of.is_empty()).on_click(cx.listener(|this,_,window,cx|this.load(window,cx))))
            .child(Button::new("ownership-clear-selected").label("Ryd snapshotvalg og genlæs aktiv pr-dato").disabled(locked||self.selection.as_of.is_empty()).on_click(cx.listener(|this,_,window,cx|{this.selection.id=None;this.load(window,cx);})));
        let Some(data) = self.data.clone() else {
            return view;
        };
        view=view.child(format!("Aktiv server-pr-dato {} · aktuel autentificeret bruger {}",data.graph["asOf"].as_str().unwrap_or("?"),data.principal))
            .child("En ændring i datofeltet anvendes først med Hent-knappen. Den aktive serverdato er altid vist ovenfor.")
            .child(div().text_lg().child("Aktuel servergraf · fakta, delvis dækning og præcis konsolideringsstatus"));
        for line in state::lines(&data.graph, "aktuelGraf") {
            view = view.child(line);
        }
        if data.graph["partial"] == true {
            view=view.child("Serveren markerer delvis dækning. Der beregnes eller vises ingen skjulte endpoints/antal eller komplette ejer-/kontrolkonklusioner lokalt.");
        }
        view =
            view.child(div().text_lg().child(
                "Fuld aktuelt synlig snapshot-historik · vælg konkret kilde til review/apply",
            ));
        for snapshot in data.history["history"].as_array().into_iter().flatten() {
            if let Some(id) = snapshot["snapshotId"].as_str() {
                let id = id.to_owned();
                view=view.child(Button::new(SharedString::from(format!("ownership-snapshot-{id}"))).label(format!("Inspicér {} · {} · kilde {} · observeret {} · snapshotHash {} · diffHash {}",id,snapshot["state"].as_str().unwrap_or("?"),snapshot["source"].as_str().unwrap_or("?"),snapshot["observedAt"].as_str().unwrap_or("?"),snapshot["snapshotHash"].as_str().unwrap_or("?"),snapshot["diffHash"].as_str().unwrap_or("?"))).disabled(locked).on_click(cx.listener(move|this,_,window,cx|{this.selection.id=Some(id.clone());this.load(window,cx);})));
            }
        }
        if let Some(snapshot) = &data.selected {
            view=view.child(div().text_lg().child("Valgt kanonisk snapshot · source/provenance, hashes, fuld readonly server-diff og reviewhistorik"));
            for line in state::lines(snapshot, "valgtSnapshot") {
                view = view.child(line);
            }
            view=view.child("Diffens add/unchanged/conflicts/endings er serverens kilde-hashbundne plan. Et senere eksplicit dateret reviewed fact kan afslutte et tidligere fact; udeladelse ophæver aldrig tidligere facts. Ingen revoke-/supersede-operation opfindes her.");
            if data.self_review() {
                view=view.child("Du er forslagets autentificerede forfatter. Eget review er afvist: en ANDEN autentificeret reviewer med aktuelle endpointrettigheder skal gennemgå denne præcise kilde og diff. Der skiftes ikke session automatisk. Et uafhængigt godkendt snapshot kan bagefter anvendes separat af en aktuelt autoriseret bruger.");
            }
        }
        if let Some(record) = self.pending.clone() {
            view=view.child(div().text_lg().child("Uafklaret ejerforholdsforsøg · ingen genafsendelse"))
                .child(format!("Handling {} · snapshot {} · snapshotHash {} · diffHash {} · oprindeligt endpoint {}",record["operation"],record["snapshotId"],record["snapshotHash"],record["diffHash"],record["origin"]))
                .child("Genlæs præcist snapshot, historik og relevante server-pr-dato-facts. Et nyt forslags id kan være ukendt; lignende kilde/fakta/hash alene beviser ikke forsøgets udfald. Den private status indeholder kun scope/identiteter/hash, aldrig faktapayload, ejerandele eller kildebeskrivelse.");
            if let Some(id) = record["snapshotId"].as_str() {
                let id = id.to_owned();
                view = view.child(
                    Button::new("ownership-inspect-pending")
                        .label("Genlæs uafklaret snapshot og aktuel graf")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.selection.id = Some(id.clone());
                            this.load(window, cx);
                        })),
                );
            }
            if state::scope(&record, &self.origin, &data, self.local.as_deref()) {
                view=view.child("Profil, bruger, virksomhed og ledger matcher. Et lokalt endpoint kan ændres ved genstart; arkivering sender intet.")
                    .child(Checkbox::new("ownership-resolved").label("Jeg har manuelt afklaret dette præcise forsøg i den viste profil og virksomhed").checked(self.resolved).disabled(self.busy).on_click(cx.listener(|this,value,_,cx|{this.resolved=*value;cx.notify();})))
                    .child(Button::new("ownership-archive-resolution").label("Arkivér manuel afklaring uden genafsendelse").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};if this.busy||!this.resolved||!state::scope(record,&this.origin,data,this.local.as_deref()){return;}match state::finish(record,true){Ok(())=>{this.pending=None;this.resolved=false;this.status="Manuel afklaring arkiveret. Ingen handling blev gentaget.".into();},Err(e)=>this.status=e}cx.notify();})));
            } else {
                view=view.child("Vælg oprindelig bruger, virksomhed og privat lokal profil/eksternt endpoint for manuel afklaring.");
            }
            return view;
        }
        if self.selection.id.is_some() && data.selected.is_none() {
            return view.child("Valgt snapshot er ikke tilgængeligt med aktuel adgang eller findes ikke. Serveren afslører ikke hvilken årsag. Ingen review/apply/andre handlinger tilbydes for denne ukendte eller skjulte kilde; ryd snapshotvalget for at arbejde videre.");
        }
        if let Some(review) = &self.review {
            view = view.child(div().text_lg().child(format!(
                "Gennemgå {} · virksomhed {} · aktiv pr-dato {}",
                review.action.name(),
                review.snapshot.company,
                review.snapshot.graph["asOf"].as_str().unwrap_or("?")
            )));
            if review.action != Action::Propose {
                if let Some(selected) = &review.snapshot.selected {
                    for line in state::lines(selected, "frossenReviewkilde") {
                        view = view.child(line);
                    }
                }
            }
            for line in state::lines(&review.display, "præcisAnmodning") {
                view = view.child(line);
            }
            return view.child(if review.action==Action::Propose{"Propose er første skrivning: den gemmer kildeobservationen og returnerer den autoritative server-diff/hashes. Ingen preview-diff er beregnet lokalt; facts ændres først efter uafhængigt review og separat apply."}else{"Snapshotidentitet, sourcehash og readonly server-diff er frosset. Apply sender kun de præcise kanoniske godkendte hashes. Serveren genkontrollerer uafhængig review, aktuelle endpoint-/evidensrettigheder og live ending-plan."})
                .child(Checkbox::new("ownership-confirmed").label("Jeg har gennemgået præcis kilde, komplette fakta/server-diff, hashes, provenance og denne ene handling").checked(self.confirmed).disabled(self.busy).on_click(cx.listener(|this,value,_,cx|{this.confirmed=*value;cx.notify();})))
                .child(Button::new("ownership-send").label("Bekræft og send den gennemgåede handling én gang").disabled(self.busy||!self.confirmed).on_click(cx.listener(|this,_,window,cx|this.send(window,cx))))
                .child(Button::new("ownership-cancel").label("Annullér før afsendelse").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.confirmed=false;cx.notify();})));
        }
        if let Some(selected) = &data.selected {
            if selected["state"] == "proposed" {
                view = view.child(div().text_lg().child(
                    "Uafhængig reviewbeslutning · gennemgå hele den viste server-diff først",
                ));
                for (decision, label) in [
                    ("approved", "Godkend dette præcise snapshot"),
                    ("rejected", "Afvis dette præcise snapshot"),
                ] {
                    view = view.child(
                        Button::new(SharedString::from(format!("ownership-decision-{decision}")))
                            .label(format!(
                                "{} {label}",
                                if self.decision == decision { "✓" } else { "" }
                            ))
                            .disabled(self.busy || data.self_review())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.decision = decision.into();
                                cx.notify();
                            })),
                    );
                }
                view = view.child(
                    Button::new("ownership-review-review")
                        .label("Gennemgå separat reviewbekræftelse")
                        .disabled(self.busy || self.decision.is_empty() || data.self_review())
                        .on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Review, cx))),
                );
            } else if selected["state"] == "approved" {
                view = view.child(
                    Button::new("ownership-review-apply")
                        .label("Gennemgå separat apply af præcis godkendt snapshotHash + diffHash")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Apply, cx))),
                );
            } else {
                view=view.child("Det valgte snapshot er allerede afvist/anvendt. Aktuel graf, originale hashes og fuld historik kan fortsat inspiceres; ingen gentagelse tilbydes automatisk.");
            }
        }
        view=view.child(div().text_lg().child("Foreslå ny fuld kildeobservation · typed fakta fra faktisk evidens"))
            .child("Faktapayload er et eksplicit JSON-array med komplette objekter. Felter: owner {kind: company, companySlug} ELLER {kind: party, partyId}, ownedCompanySlug, validFrom, valgfri validToExclusive, economicBasisPoints ELLER economicIntervalBasisPoints {min,max}, valgfri votingBasisPoints/shareClass, controlType (equity/voting/board/agreement/other), jurisdiction og evidenceRefs. Brug faktiske eksisterende endpoints/evidenskilder. Ingen kilde-/dato-/land-/ejerandel-defaults indsættes.")
            .child("Faktisk kildehenvisning").child(Input::new(&self.source).disabled(self.busy))
            .child("Faktisk observedAt (ISO-dato/tid med eksplicit tidszone)").child(Input::new(&self.observed).disabled(self.busy))
            .child("Komplette typed facts som JSON-array; syntaks kontrolleres lokalt, alle domæne-/rettighedsregler på serveren").child(Input::new(&self.facts).disabled(self.busy))
            .child("Udeladte facts bliver ikke ophævet. En senere kildebelagt ændring kræver et nyt konkret dateret fact og nyt uafhængigt review; ingen implicit revoke eller lokal beregning.");
        view.child(
            Button::new("ownership-review-propose")
                .label("Gennemgå kildeobservation før separat Propose-bekræftelse")
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Propose, cx))),
        )
    }
}
