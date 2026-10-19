//! Visible canonical workspace parties, scoped to an explicit anchor company through HTTP only.
use crate::{Session, parties_state as state, readable};
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
use state::{Action, Form, Review, Selection, Snapshot};
use std::sync::Arc;
fn read(session: &Session, company: &str, selection: &Selection) -> Result<Snapshot, String> {
    let page = session
        .runtime
        .block_on(session.client.workspace_parties(
            company,
            (!selection.query.is_empty()).then_some(selection.query.as_str()),
            selection.cursor,
            25,
        ))
        .map_err(readable)?;
    let party = selection
        .id
        .as_ref()
        .map(|id| {
            session
                .runtime
                .block_on(session.client.workspace_party(company, id))
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
    Snapshot::parse(company, selection, page, party, me, journal)
}
pub struct WorkspaceParties {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    selection: Selection,
    previous: Vec<usize>,
    query: Entity<InputState>,
    name: Entity<InputState>,
    source: Entity<InputState>,
    observed: Entity<InputState>,
    declaration: Entity<InputState>,
    defaults: Entity<InputState>,
    country: Entity<InputState>,
    identifier: Entity<InputState>,
    alias: Entity<InputState>,
    field: Entity<InputState>,
    value: Entity<InputState>,
    kind: String,
    role: String,
    identifier_kind: String,
    review_state: String,
    identifiers: Vec<Value>,
    data: Option<Snapshot>,
    review: Option<Review>,
    confirmed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
}
impl WorkspaceParties {
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
        let mut this = Self {
            session,
            origin,
            company,
            local,
            selection: Selection::default(),
            previous: Vec::new(),
            query: input("Søg i synligt navn/alias"),
            name: input("Kanonisk navn til ny part"),
            source: input("Faktisk kildehenvisning"),
            observed: input("Observation: ISO-dato eller tid med eksplicit tidszone"),
            declaration: input("Din konkrete review-erklæring om oprettelsens kilde/identitet"),
            defaults: input("Eksplicit JSON-objekt: {} for ingen standarder"),
            country: input("Land til identifikator; udledes ikke"),
            identifier: input("Faktisk CVR/momsnummer/anden identifikator"),
            alias: input("Kildebelagt alias"),
            field: input("Assertionens feltbetegnelse, fx name eller email"),
            value: input("Assertionens tekstværdi"),
            kind: String::new(),
            role: String::new(),
            identifier_kind: String::new(),
            review_state: String::new(),
            identifiers: Vec::new(),
            data: None,
            review: None,
            confirmed: false,
            pending: None,
            resolved: false,
            busy: false,
            status: String::new(),
        };
        this.load(window, cx);
        this
    }
    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.review = None;
        self.confirmed = false;
        self.resolved = false;
        self.kind.clear();
        self.role.clear();
        self.identifier_kind.clear();
        self.review_state.clear();
        self.identifiers.clear();
        for input in [
            &self.name,
            &self.source,
            &self.observed,
            &self.declaration,
            &self.defaults,
            &self.country,
            &self.identifier,
            &self.alias,
            &self.field,
            &self.value,
        ] {
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
        self.status =
            "Henter synlig partside, valgt identitet og aktuelle rettighedskilder…".into();
        let (session, company, selection) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((read(&session, &company, &selection)?, state::load()?))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok((data,pending))=>{this.data=Some(data);this.pending=pending;this.status="Kun serverens aktuelt synlige parter vises. Navn, art og identifikatorer er immutable; alias/feltassertioner tilføjer evidens. Roller og standarder gælder den viste ankervirksomhed.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn prepare(&mut self, action: Action, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        let (name, source, observed, declaration, defaults, alias, field, value) = (
            self.name.read(cx).value().to_string(),
            self.source.read(cx).value().to_string(),
            self.observed.read(cx).value().to_string(),
            self.declaration.read(cx).value().to_string(),
            self.defaults.read(cx).value().to_string(),
            self.alias.read(cx).value().to_string(),
            self.field.read(cx).value().to_string(),
            self.value.read(cx).value().to_string(),
        );
        let form = Form {
            kind: &self.kind,
            name: &name,
            identifiers: &self.identifiers,
            role: &self.role,
            defaults: &defaults,
            source: &source,
            observed: &observed,
            review_assertion: &declaration,
            review_state: &self.review_state,
            alias: &alias,
            field: &field,
            value: &value,
        };
        match Review::prepare(data, action, form) {
            Ok(review) => {
                self.review = Some(review);
                self.confirmed = false;
                self.status =
                    "Gennemgå den frosne handling og ankervirksomhed nedenfor. Intet sendt.".into();
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
        self.status = "Genlæser den gennemgåede part/side og sender præcis én handling…".into();
        let (session, company, selection, origin, local) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
            self.origin.clone(),
            self.local.clone(),
        );
        let task=cx.background_executor().spawn(async move{
   let fresh=read(&session,&company,&selection)?;if !review.current(&fresh){return Err("Bruger, ledger, synlig side eller part ændrede sig. Intet sendt; genlæs og gennemgå igen.".into());}
   let action=review.sdk()?;let record=review.marker(&origin,local.as_deref());state::begin(&record)?;
   let response=session.runtime.block_on(session.client.mutate_workspace_party(&company,action,review.body.clone(),true));
   let(message,receipt)=match response{
    Ok(value)=>if let Some(party)=review.receipt(&value){state::finish(&record,false)?;("Serveren bekræftede handlingen og returnerede den kanoniske part. Ingen identitet er lokalt overskrevet eller sammenlagt.".into(),Some(party))}else{("Kvitteringen er ufuldstændig. Handlingen kan være registreret; ingen genafsendelse. Genlæs og afklar manuelt.".into(),None)},
    Err(Error::Refused{status})=>{state::finish(&record,false)?;(format!("Serveren afviste handlingen (HTTP{status}). Kontrollér aktuel adgang, typed identitet, kilde/dato, rolle og standarder. Alias/feltassertioner kræver skriveret til ALLE partens tilknyttede virksomheder; navne/identifikatorer og eksisterende standarder kan ikke overskrives her. Ingen automatisk gentagelse."),None)},
    Err(Error::InvalidInput(_))=>{state::finish(&record,false)?;("Anmodningen blev afvist før netværkskald. Genlæs og gennemgå felter igen.".into(),None)},
    _=>("Udfaldet er uafklaret. Genlæs synlig liste og kanonisk part/historik; afklar præcis dette forsøg manuelt. Ingen genafsendelse eller automatisk adoption.".into(),None)
   };
   let mut next_selection=selection;if let Some(p)=&receipt{next_selection.id=p["partyId"].as_str().map(str::to_owned);}
   let next=read(&session,&company,&next_selection);let message=if let(Some(receipt),Ok(data))=(&receipt,&next){format!("{message} {}",if state::readback(receipt,data){"Præcis partskvittering, roller, identifikatorer og provenance er kanonisk genlæst."}else{"Genlæsningen matcher ikke hele kvitteringen. Kontrollér aktuelle hændelser før videre arbejde."})}else{message};
   Ok::<_,String>((message,next,next_selection,state::load()?))
  });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((message, data, selection, pending)) => {
                        this.status = message;
                        this.selection = selection;
                        this.pending = pending;
                        match data {
                            Ok(d) => this.data = Some(d),
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
impl Render for WorkspaceParties {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut view=div().flex().flex_col().gap_3().child(div().text_xl().child("Workspaceparter · kanonisk identitet og kildebelagt evidens"))
   .child(format!("Ankervirksomhed {} · {}",self.company,self.origin)).child(self.status.clone())
   .child("Søg i serverens aktuelt synlige navne/aliaser. Rollen/standarder knyttes kun til ankervirksomheden. Andre tilknyttede virksomheder kan begrænse fælles ændringer.")
   .child(Input::new(&self.query).disabled(self.busy||self.review.is_some()))
   .child(Button::new("parties-search").label("Søg / første side").disabled(self.busy||self.review.is_some()).on_click(cx.listener(|this,_,window,cx|{this.selection=Selection{query:this.query.read(cx).value().to_string(),..Selection::default()};this.previous.clear();this.load(window,cx);})))
   .child(Button::new("parties-reload").label("Genlæs side og valgt part; ryd formular/review").disabled(self.busy||self.review.is_some()).on_click(cx.listener(|this,_,window,cx|this.load(window,cx))));
        let Some(data) = self.data.clone() else {
            return view;
        };
        view = view
            .child(format!(
                "Serverens synlige søgeresultat: {} parter · cursor {} · op til25 pr. side",
                data.page["count"], self.selection.cursor
            ))
            .child(
                Button::new("parties-previous")
                    .label("Forrige viste side")
                    .disabled(self.busy || self.review.is_some() || self.previous.is_empty())
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(cursor) = this.previous.pop() {
                            this.selection.cursor = cursor;
                            this.selection.id = None;
                            this.load(window, cx);
                        }
                    })),
            );
        if let Some(cursor) = data.page["nextCursor"]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
        {
            view = view.child(
                Button::new("parties-next")
                    .label("Næste side fra serveren")
                    .disabled(self.busy || self.review.is_some())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.previous.push(this.selection.cursor);
                        this.selection.cursor = cursor;
                        this.selection.id = None;
                        this.load(window, cx);
                    })),
            );
        }
        for p in data.page["rows"].as_array().into_iter().flatten() {
            if let Some(id) = p["partyId"].as_str() {
                let id = id.to_owned();
                view = view.child(
                    Button::new(SharedString::from(format!("parties-inspect-{id}")))
                        .label(format!(
                            "Inspicér {} · {} · {}",
                            id,
                            p["name"].as_str().unwrap_or("?"),
                            p["kind"].as_str().unwrap_or("?")
                        ))
                        .disabled(self.busy || self.review.is_some())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.selection.id = Some(id.clone());
                            this.load(window, cx);
                        })),
                );
            }
        }
        if let Some(party) = &data.party {
            view=view.child(div().text_lg().child("Valgt kanonisk part · immutable identitet, roller/standarder og fuld provenance/historik"));
            for line in state::lines(party, "part") {
                view = view.child(line);
            }
        }
        if let Some(record) = self.pending.clone() {
            view=view.child(div().text_lg().child("Uafklaret forsøg · ingen genafsendelse")).child(format!("Anker {} · handling {} · part {} · oprindeligt endpoint {}",record["company"],record["operation"],record["partyId"],record["origin"]))
    .child("Oprettelsens id kan være ukendt. En lignende part/projektion beviser ikke alene dette forsøgs udfald. Søg og inspicér kanonisk historik og afklar manuelt; formularens navne, identifikatorer, standarder og kilder gemmes ikke i privat status.");
            if let Some(id) = record["partyId"].as_str() {
                let id = id.to_owned();
                view = view.child(
                    Button::new("parties-inspect-pending")
                        .label("Genlæs den uafklarede parts aktuelle kanoniske historik")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.selection.id = Some(id.clone());
                            this.load(window, cx);
                        })),
                );
            }
            if state::scope(&record, &self.origin, &data, self.local.as_deref()) {
                view=view.child("Profil, bruger, anker og ledger matcher. Lokalt endpoint kan være ændret efter genstart; arkivering sender intet.")
     .child(Checkbox::new("parties-resolved").label("Jeg har manuelt afklaret dette præcise forsøg i den viste profil/virksomhed").checked(self.resolved).disabled(self.busy).on_click(cx.listener(|this,value,_,cx|{this.resolved=*value;cx.notify();})))
     .child(Button::new("parties-archive-pending").label("Arkivér manuel afklaring uden genafsendelse").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};if this.busy||!this.resolved||!state::scope(record,&this.origin,data,this.local.as_deref()){return;}match state::finish(record,true){Ok(())=>{this.pending=None;this.resolved=false;this.status="Manuel afklaring arkiveret. Ingen handling blev genafsendt.".into();},Err(e)=>this.status=e}cx.notify();})));
            } else {
                view=view.child("Vælg oprindelig bruger, ankervirksomhed og lokal profil/eksternt endpoint for manuel afklaring.");
            }
            return view;
        }
        if let Some(review) = &self.review {
            view = view.child(div().text_lg().child(format!(
                "Gennemgå {} · anker {}",
                review.action.name(),
                review.source.company
            )));
            if review.action != Action::Create {
                if let Some(p) = &review.source.party {
                    for line in state::lines(p, "oprindeligPart") {
                        view = view.child(line);
                    }
                }
            }
            for line in state::lines(
                &serde_json::from_slice::<Value>(&review.body).unwrap_or(Value::Null),
                "præcisAnmodning",
            ) {
                view = view.child(line);
            }
            return view.child("Ingen identitet overskrives eller sammenlægges. Serveren kontrollerer typed identifikatorer, aktuelle rettigheder og tilknyttede virksomheder ved bekræftelse.")
    .child(Checkbox::new("parties-confirmed").label("Jeg har gennemgået præcis part, kilde, tid, rolle, standarder og handling").checked(self.confirmed).disabled(self.busy).on_click(cx.listener(|this,value,_,cx|{this.confirmed=*value;cx.notify();})))
    .child(Button::new("parties-send").label("Bekræft og send den gennemgåede handling én gang").disabled(self.busy||!self.confirmed).on_click(cx.listener(|this,_,window,cx|this.send(window,cx))))
    .child(Button::new("parties-cancel").label("Annullér før afsendelse").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.confirmed=false;cx.notify();})));
        }
        view = view.child(
            div()
                .text_lg()
                .child("Opret ny selvstændig part · vælg art eksplicit"),
        );
        for value in state::KINDS {
            let value = *value;
            view = view.child(
                Button::new(SharedString::from(format!("parties-kind-{value}")))
                    .label(format!(
                        "{} {value}",
                        if self.kind == value { "✓" } else { "" }
                    ))
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.kind = value.into();
                        cx.notify();
                    })),
            );
        }
        view=view.child("Kanonisk navn (ny part; eksisterende identitet ændres ikke)").child(Input::new(&self.name).disabled(self.busy))
   .child("Typed identifikatorer (valgfri): angiv land, vælg art og angiv værdi; serveren normaliserer og validerer. Ingen land-/momsart udledes.").child(Input::new(&self.country).disabled(self.busy));
        for value in state::IDENTIFIERS {
            let value = *value;
            view = view.child(
                Button::new(SharedString::from(format!("parties-identifier-{value}")))
                    .label(format!(
                        "{} {value}",
                        if self.identifier_kind == value {
                            "✓"
                        } else {
                            ""
                        }
                    ))
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.identifier_kind = value.into();
                        cx.notify();
                    })),
            );
        }
        view = view
            .child(Input::new(&self.identifier).disabled(self.busy))
            .child(
                Button::new("parties-add-identifier")
                    .label("Tilføj denne typed identifikator til oprettelsen")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        match state::identifier(
                            this.country.read(cx).value().as_ref(),
                            &this.identifier_kind,
                            this.identifier.read(cx).value().as_ref(),
                        ) {
                            Ok(value) => this.identifiers.push(value),
                            Err(e) => this.status = e,
                        }
                        cx.notify();
                    })),
            )
            .child(
                Button::new("parties-clear-identifiers")
                    .label("Ryd oprettelsens identifikatorliste")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.identifiers.clear();
                        cx.notify();
                    })),
            );
        for line in state::lines(
            &serde_json::json!(self.identifiers),
            "oprettelsensIdentifikatorer",
        ) {
            view = view.child(line);
        }
        view = view
            .child("Faktisk kildehenvisning")
            .child(Input::new(&self.source).disabled(self.busy))
            .child("Observeret på (ISO-dato eller tid med eksplicit tidszone)")
            .child(Input::new(&self.observed).disabled(self.busy))
            .child("Din konkrete review-erklæring om den nye parts kilde og identitet")
            .child(Input::new(&self.declaration).disabled(self.busy))
            .child(format!(
                "Vælg rolle eksplicit i ankervirksomheden {}",
                self.company
            ));
        for value in state::ROLES {
            let value = *value;
            view = view.child(
                Button::new(SharedString::from(format!("parties-role-{value}")))
                    .label(format!(
                        "{} {value}",
                        if self.role == value { "✓" } else { "" }
                    ))
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.role = value.into();
                        cx.notify();
                    })),
            );
        }
        view=view.child("Eksplicitte standarder som JSON-objekt: {} betyder ingen. Understøttede felter: account, vat, currency, paymentTermsDays. Ingen regnskabsstandard udledes; eksisterende modstridende standarder kan afvises.")
   .child(Input::new(&self.defaults).disabled(self.busy))
   .child(Button::new("parties-create").label("Gennemgå ny selvstændig part med valgt rolle/standarder").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.prepare(Action::Create,cx))));
        if data.party.is_some() {
            view=view.child(Button::new("parties-add-role").label("Gennemgå tilføjelse af valgt rolle/standarder til den inspicerede part").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.prepare(Action::Role,cx))))
    .child(div().text_lg().child("Kildebelagt alias eller feltassertion · ændrer ikke kanonisk identitet"))
    .child("Bruger kilde og observationstid ovenfor. Vælg reviewtilstand eksplicit. Fælles assertion/alias kræver skriveret i ALLE tilknyttede virksomheder.");
            for value in state::REVIEW_STATES {
                let value = *value;
                view = view.child(
                    Button::new(SharedString::from(format!("parties-review-{value}")))
                        .label(format!(
                            "{} {value}",
                            if self.review_state == value {
                                "✓"
                            } else {
                                ""
                            }
                        ))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.review_state = value.into();
                            cx.notify();
                        })),
                );
            }
            view=view.child("Alias med kilde").child(Input::new(&self.alias).disabled(self.busy))
    .child(Button::new("parties-alias").label("Gennemgå kildebelagt alias for den inspicerede part").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.prepare(Action::Alias,cx))))
    .child("Feltbetegnelse og tekstværdi til assertion (fx name eller email); tilføjer evidens, overskriver ikke original identitet").child(Input::new(&self.field).disabled(self.busy)).child(Input::new(&self.value).disabled(self.busy))
    .child(Button::new("parties-assertion").label("Gennemgå feltassertion for den inspicerede part").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.prepare(Action::Assertion,cx))));
        }
        view
    }
}
