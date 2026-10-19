//! Native advisory company knowledge; server controls state, dates, identity and provenance.
use crate::{Session, company_knowledge_state as state, readable};
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
use state::{Action, Form, Review, Snapshot};
use std::sync::Arc;
fn read(session: &Session, company: &str, as_of: &str) -> Result<Snapshot, String> {
    let knowledge = session
        .runtime
        .block_on(session.client.company_knowledge(
            company,
            (!as_of.is_empty()).then_some(as_of),
            true,
        ))
        .map_err(readable)?;
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let journal = session
        .runtime
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    Snapshot::parse(company, knowledge, me, journal)
}
pub struct CompanyKnowledge {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    as_of: Entity<InputState>,
    loaded_as_of: String,
    value: Entity<InputState>,
    reference: Entity<InputState>,
    from: Entity<InputState>,
    to: Entity<InputState>,
    reason: Entity<InputState>,
    predicate: String,
    source: String,
    certainty: String,
    selected: Option<String>,
    data: Option<Snapshot>,
    review: Option<Review>,
    confirmed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
}
impl CompanyKnowledge {
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
            as_of: input("Valgfri ISO-dato/tid; tomt bruger serverens aktuelle tidspunkt"),
            loaded_as_of: String::new(),
            value: input("Værdi som JSON: \"tekst\", [\"liste\"] eller {\"felt\":true}"),
            reference: input("Eksplicit kildehenvisning / faktisk kilde-id"),
            from: input("Gyldig fra: ISO-dato eller tid med tidszone"),
            to: input("Gyldig til, eksklusiv: valgfri ISO-dato/tid"),
            reason: input("Valgfri konkret begrundelse for godkendelse/afvisning"),
            predicate: String::new(),
            source: String::new(),
            certainty: String::new(),
            selected: None,
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
        self.selected = None;
        self.predicate.clear();
        self.source.clear();
        self.certainty.clear();
        for input in [
            &self.value,
            &self.reference,
            &self.from,
            &self.to,
            &self.reason,
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
        self.loaded_as_of = self.as_of.read(cx).value().to_string();
        self.status = "Henter serverkontekst, fuld historik og aktuelle identiteter…".into();
        let (session, company, as_of) = (
            self.session.clone(),
            self.company.clone(),
            self.loaded_as_of.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            let pending = state::load()?;
            Ok::<_, String>((read(&session, &company, &as_of)?, pending))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok((data,pending))=>{this.data=Some(data);this.pending=pending;this.status="Kontekst og historik er serverens aktuelle projektion. Viden er rådgivende og ændrer ikke bogføringsregler. Nye/erstattende forslag kræver særskilt godkendelse.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn prepare(&mut self, action: Action, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        let (value, reference, from, to, reason) = (
            self.value.read(cx).value().to_string(),
            self.reference.read(cx).value().to_string(),
            self.from.read(cx).value().to_string(),
            self.to.read(cx).value().to_string(),
            self.reason.read(cx).value().to_string(),
        );
        let form = Form {
            predicate: &self.predicate,
            value: &value,
            source: &self.source,
            reference: &reference,
            from: &from,
            to: &to,
            certainty: &self.certainty,
            reason: &reason,
        };
        match Review::prepare(data, action, self.selected.as_deref(), form) {
            Ok(review) => {
                self.review = Some(review);
                self.confirmed = false;
                self.status =
                    "Gennemgå den præcise handling og kilde før særskilt bekræftelse. Intet sendt."
                        .into();
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
            "Genlæser historik/identitet og sender kun den gennemgåede handling én gang…".into();
        let (session, company, as_of, origin, local) = (
            self.session.clone(),
            self.company.clone(),
            self.loaded_as_of.clone(),
            self.origin.clone(),
            self.local.clone(),
        );
        let task=cx.background_executor().spawn(async move{
   let fresh=read(&session,&company,&as_of)?;if !review.current(&fresh){return Err("Bruger, ledger eller historik ændrede sig. Intet sendt; genlæs og gennemgå igen.".into());}
   let record=review.marker(&origin,&company,local.as_deref());state::begin(&record)?;
   let result=session.runtime.block_on(session.client.mutate_company_knowledge(&company,review.action.sdk(),review.body.clone(),true));
   let(message,receipt)=match result{
    Ok(value)=>if let Some(receipt)=review.receipt(&value){state::finish(&record,false)?;("Serveren bekræftede handlingen eller returnerede en eksisterende kanonisk dublet. Erstatning foretager ingen lokal godkendelse; kontrollér replacementens kanoniske reviewtilstand.".into(),Some(receipt))}else{("Serverkvitteringen er ufuldstændig. Handlingen kan være registreret; ingen genafsendelse. Genlæs og afklar manuelt.".into(),None)},
    Err(Error::Refused{status})=>{state::finish(&record,false)?;(format!("Serveren afviste handlingen (HTTP{status}). Kontrollér aktuel adgang, kildehenvisning, gyldighedsdatoer og assertionens reviewtilstand. Ingen automatisk gentagelse."),None)},
    Err(Error::InvalidInput(_))=>{state::finish(&record,false)?;("Anmodningen blev afvist før netværkskald. Genlæs og gennemgå igen.".into(),None)},
    _=>("Udfaldet er uafklaret. Der foretages ingen genafsendelse eller automatisk adoption. Afklar manuelt i den kanoniske historik før en ny selvstændig handling.".into(),None)
   };
   let next=read(&session,&company,&as_of);let message=if let(Some(receipt),Ok(data))=(&receipt,&next){format!("{message} {}",if state::readback(receipt,data){"Den præcise kvittering findes i serverens historik; kontekst og provenance er genlæst."}else{"Historikgenlæsningen bekræftede ikke hele kvitteringen; kontrollér aktuelle hændelser før videre arbejde."})}else{message};
   Ok::<_,String>((message,next,state::load()?))
  });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((message, data, pending)) => {
                        this.status = message;
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
impl Render for CompanyKnowledge {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut view=div().flex().flex_col().gap_3().child(div().text_xl().child("Virksomhedsviden · kontekst, kilder og review"))
   .child(format!("{} · {}",self.company,self.origin)).child(self.status.clone())
   .child("Kontekst pr. eksplicit ISO-dato/tid (valgfri). Serveren afgør effektiv gyldighed, konflikter og rettigheder; hele hændelseshistorikken vises uanset datofilter.")
   .child(Input::new(&self.as_of).disabled(self.busy||self.review.is_some()))
   .child(Button::new("knowledge-reload").label("Genlæs kontekst og fuld historik; ryd formular/review").disabled(self.busy||self.review.is_some()).on_click(cx.listener(|this,_,window,cx|this.load(window,cx))));
        let Some(data) = self.data.clone() else {
            return view;
        };
        view = view.child(
            div()
                .text_lg()
                .child("Serverens effektive kontekst og konflikter"),
        );
        for line in state::lines(&data.context, "kontekst") {
            view = view.child(line);
        }
        view = view.child(
            div()
                .text_lg()
                .child("Fuld kanonisk historik · værdi, kilde, gyldighed og reviewhændelser"),
        );
        for row in &data.history {
            for line in state::lines(row, "assertion") {
                view = view.child(line);
            }
            if let Some(id) = row["assertionId"].as_str() {
                let id = id.to_owned();
                view = view.child(
                    Button::new(SharedString::from(format!("knowledge-select-{id}")))
                        .label(format!("Vælg assertion {id} til review/erstatning"))
                        .disabled(self.busy || self.review.is_some() || self.pending.is_some())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected = Some(id.clone());
                            this.review = None;
                            this.confirmed = false;
                            cx.notify();
                        })),
                );
            }
        }
        if let Some(record) = self.pending.clone() {
            view=view.child(div().text_lg().child("Uafklaret handling · ingen genafsendelse"))
    .child(format!("Virksomhed {} · handling {} · assertion {} · oprindeligt endpoint {}",record["company"],record["operation"],record["assertionId"],record["origin"]))
    .child("Genlæs historikken og afklar manuelt. Ved forslag kan id være ukendt, og en lignende projektion beviser ikke alene forsøgets udfald. Formular, værdi, kildehenvisning og begrundelse gemmes ikke i den private status.");
            if state::scope(
                &record,
                &self.origin,
                &self.company,
                &data,
                self.local.as_deref(),
            ) {
                view=view.child("Profil, ledger og bruger matcher. Ved lokal genstart kan endpoint være ændret; ingen ny handling sendes ved arkivering.")
     .child(Checkbox::new("knowledge-resolved").label("Jeg har manuelt afklaret dette præcise forsøg i den viste virksomhed").checked(self.resolved).disabled(self.busy).on_click(cx.listener(|this,checked,_,cx|{this.resolved=*checked;cx.notify();})))
     .child(Button::new("knowledge-archive-pending").label("Arkivér afklaring uden genafsendelse").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{
      let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};if this.busy||!this.resolved||!state::scope(record,&this.origin,&this.company,data,this.local.as_deref()){return;}
      match state::finish(record,true){Ok(())=>{this.pending=None;this.resolved=false;this.status="Manuel afklaring arkiveret. Ingen handling blev genafsendt.".into();},Err(e)=>this.status=e}cx.notify();
     })));
            } else {
                view=view.child("Vælg den oprindelige bruger, virksomhed og lokale profil/eksterne endpoint for manuel afklaring.");
            }
            return view;
        }
        if let Some(review) = &self.review {
            view = view.child(div().text_lg().child(format!(
                "Gennemgå særskilt handling: {}",
                review.action.name()
            )));
            if let Some(old) = review
                .target
                .as_deref()
                .and_then(|id| review.source.find(id))
            {
                for line in state::lines(old, "oprindeligAssertion") {
                    view = view.child(line);
                }
            }
            let body: Value = serde_json::from_slice(&review.body).unwrap_or(Value::Null);
            for line in state::lines(&body, "præcisAnmodning") {
                view = view.child(line);
            }
            return view.child("Serveren validerer igen ved bekræftelse. Erstatning bruger serverens forslag eller eksisterende dublet; godkendelse er en særskilt handling.")
    .child(Checkbox::new("knowledge-confirmed").label("Jeg har gennemgået værdi, kilde/provenance, datoer og præcis handling").checked(self.confirmed).disabled(self.busy).on_click(cx.listener(|this,checked,_,cx|{this.confirmed=*checked;cx.notify();})))
    .child(Button::new("knowledge-send").label("Bekræft og send denne handling én gang").disabled(self.busy||!self.confirmed).on_click(cx.listener(|this,_,window,cx|this.send(window,cx))))
    .child(Button::new("knowledge-cancel").label("Annullér før afsendelse").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.confirmed=false;cx.notify();})));
        }
        view = view
            .child(
                div()
                    .text_lg()
                    .child("Nyt forslag eller eksplicit erstatning"),
            )
            .child("Vælg emne eksplicit:");
        for value in state::PREDICATES {
            let value = *value;
            view = view.child(
                Button::new(SharedString::from(format!("knowledge-predicate-{value}")))
                    .label(format!(
                        "{} {value}",
                        if self.predicate == value { "✓" } else { "" }
                    ))
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.predicate = value.into();
                        cx.notify();
                    })),
            );
        }
        view=view.child("Ny værdi som JSON · tekst i dobbelte anførselstegn, liste, objekt, tal, boolesk værdi eller null. Kun syntaks kontrolleres lokalt; serveren validerer viden og kilder.").child(Input::new(&self.value).disabled(self.busy)).child("Vælg kildeart eksplicit; henvisning er ikke dokumentation for adgang eller eksistens før serverens kontrol:");
        for value in state::SOURCES {
            let value = *value;
            view = view.child(
                Button::new(SharedString::from(format!("knowledge-source-{value}")))
                    .label(format!(
                        "{} {value}",
                        if self.source == value { "✓" } else { "" }
                    ))
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.source = value.into();
                        cx.notify();
                    })),
            );
        }
        view = view
            .child("Kildehenvisning / faktisk kilde-id")
            .child(Input::new(&self.reference).disabled(self.busy))
            .child("Gyldig fra (ISO-dato eller tid med eksplicit tidszone)")
            .child(Input::new(&self.from).disabled(self.busy))
            .child("Gyldig til, eksklusiv (valgfri)")
            .child(Input::new(&self.to).disabled(self.busy))
            .child("Vælg sikkerhed eksplicit:");
        for value in state::CERTAINTIES {
            let value = *value;
            view = view.child(
                Button::new(SharedString::from(format!("knowledge-certainty-{value}")))
                    .label(format!(
                        "{} {value}",
                        if self.certainty == value { "✓" } else { "" }
                    ))
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.certainty = value.into();
                        cx.notify();
                    })),
            );
        }
        view = view.child(
            Button::new("knowledge-propose")
                .label("Gennemgå nyt selvstændigt forslag")
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Propose, cx))),
        );
        if let Some(old) = self.selected.as_deref().and_then(|id| data.find(id)) {
            view = view.child(format!(
                "Valgt assertion: {} · kanonisk reviewtilstand {}",
                old["assertionId"], old["reviewState"]
            ));
            if old["reviewState"] == "proposed" {
                view = view
                    .child("Valgfri begrundelse for review")
                    .child(Input::new(&self.reason).disabled(self.busy))
                    .child(
                        Button::new("knowledge-approve")
                            .label("Gennemgå godkendelse af det valgte forslag")
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.prepare(Action::Approve, cx)),
                            ),
                    )
                    .child(
                        Button::new("knowledge-reject")
                            .label("Gennemgå afvisning af det valgte forslag")
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.prepare(Action::Reject, cx)),
                            ),
                    );
            } else if old["reviewState"] == "approved" {
                view = view.child(
                    Button::new("knowledge-supersede")
                        .label("Gennemgå erstatning med det nye forslag ovenfor")
                        .disabled(self.busy)
                        .on_click(
                            cx.listener(|this, _, _, cx| this.prepare(Action::Supersede, cx)),
                        ),
                );
            } else {
                view=view.child("Denne kanoniske tilstand tilbyder ikke review eller erstatning. Historikken bevares.");
            }
        }
        view
    }
}
