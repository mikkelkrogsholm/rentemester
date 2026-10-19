//! Native mileage documentation. Root navigation declares `mod mileage; mod mileage_state;`
//! and stores `Entity<Mileage>` built with `Mileage::new(session, origin, company, year,
//! local_identity, window, cx)`. Recreate on session/company/local-profile changes.
//! Requires an explicit fiscal label; blank input offers canonical labels via journal discovery.
use crate::{Session, mileage_state as state, readable};
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
use std::sync::Arc;
const FIELDS: &[(&str, &str)] = &[
    ("tripDate", "Dato · ÅÅÅÅ-MM-DD i valgt regnskabsår"),
    ("purpose", "Formål"),
    ("fromLocation", "Fra-adresse"),
    ("toLocation", "Til-adresse"),
    ("kilometers", "Kilometer · decimalpunktum"),
    ("vehicle", "Køretøj"),
    ("driver", "Chauffør"),
    ("ratePerKm", "Din valgte takst · kr/km med decimalpunktum"),
    ("rateBasis", "Dit bekræftede satsgrundlag"),
    ("rateSource", "Takstkilde · valgfri"),
    ("notes", "Noter · valgfrie"),
];
fn read(session: &Session, company: &str, year: &str) -> Result<state::MileageReport, String> {
    if year.is_empty() {
        let journal = session
            .runtime
            .block_on(session.client.report(company, Report::Journal, None, None))
            .map_err(readable)?;
        let labels = journal["journal"]["fiscalYears"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|y| y["label"].as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Vælg eksplicit regnskabsår og genlæs. Serverens år: {labels}"
        ));
    }
    let report = session
        .runtime
        .block_on(session.client.mileage(company, year))
        .map_err(readable)?;
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let journal = session
        .runtime
        .block_on(
            session
                .client
                .report(company, Report::Journal, Some(year), None),
        )
        .map_err(readable)?;
    state::MileageReport::parse(report, me, journal, company, year)
}
#[derive(Clone)]
struct Review {
    request: state::RegisterMileage,
    scope: Value,
}
pub struct Mileage {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    year: Entity<InputState>,
    fields: Vec<(String, Entity<InputState>)>,
    data: Option<state::MileageReport>,
    review: Option<Review>,
    reviewed: bool,
    pending: Option<Value>,
    resolved: bool,
    receipt: Option<Value>,
    receipt_year: String,
    busy: bool,
    status: String,
}
impl Mileage {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        company: String,
        year: String,
        local: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let year_input = cx.new(|cx| {
            let mut input =
                InputState::new(window, cx).placeholder("Eksplicit regnskabsår fra serveren");
            input.set_value(year, window, cx);
            input
        });
        let fields = FIELDS
            .iter()
            .map(|(key, label)| {
                (
                    (*key).to_owned(),
                    cx.new(|cx| InputState::new(window, cx).placeholder(*label)),
                )
            })
            .collect();
        let mut this = Self {
            session,
            origin,
            company,
            local,
            year: year_input,
            fields,
            data: None,
            review: None,
            reviewed: false,
            pending: None,
            resolved: false,
            receipt: None,
            receipt_year: String::new(),
            busy: false,
            status: String::new(),
        };
        this.load(window, cx);
        this
    }
    fn values(&self, cx: &Context<Self>) -> Vec<(String, String)> {
        self.fields
            .iter()
            .map(|(k, v)| (k.clone(), v.read(cx).value().to_string()))
            .collect()
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.data = None;
        self.review = None;
        self.reviewed = false;
        self.resolved = false;
        self.status = "Henter serverens kørsler og aktuelle bruger/ledgeridentitet…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let year = self.year.read(cx).value().trim().to_owned();
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((read(&session, &company, &year)?, state::load()?))
        });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,_,cx|{this.busy=false;match result{Ok((report,pending))=>{this.data=Some(report);this.pending=pending;this.status="Kørsel dokumenterer dit indtastede grundlag. Serveren beregner beløb og eventuelle advarsler; der oprettes ingen journalpost.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn review(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        if self.year.read(cx).value().trim() != data.data()["selectedYear"].as_str().unwrap_or("") {
            self.status = "Genlæs det valgte regnskabsår før gennemgang".into();
            cx.notify();
            return;
        }
        match state::RegisterMileage::review(&self.values(cx), data) {
            Ok(request) => {
                self.review = Some(Review {
                    request,
                    scope: data.marker(&self.origin, &self.company, self.local.as_deref()),
                });
                self.reviewed = false;
                self.receipt = None;
                self.status="Gennemgå dato, rute, kilometer og din egen sats/kilde. Beløb og serverrådgivning returneres først efter registrering; der findes ingen native forhåndsberegning.".into();
            }
            Err(e) => self.status = e,
        }
        cx.notify();
    }
    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.reviewed || self.pending.is_some() {
            return;
        }
        let (Some(review), Some(data)) = (self.review.clone(), self.data.as_ref()) else {
            return;
        };
        if state::RegisterMileage::review(&self.values(cx), data)
            .ok()
            .is_none_or(|now| now.body != review.request.body)
            || self.year.read(cx).value().trim()
                != data.data()["selectedYear"].as_str().unwrap_or("")
        {
            self.status =
                "Felter eller regnskabsår ændrede sig. Gennemgå igen; intet sendt.".into();
            self.review = None;
            self.reviewed = false;
            cx.notify();
            return;
        }
        self.busy = true;
        self.review = None;
        self.reviewed = false;
        self.receipt = None;
        self.resolved = false;
        self.status = "Kontrollerer scope og sender den gennemgåede registrering én gang…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let origin = self.origin.clone();
        let local = self.local.clone();
        let year = self.year.read(cx).value().trim().to_owned();
        // Never retain an automatically resubmittable form after an attempt.
        for (_, field) in &self.fields {
            field.update(cx, |input, cx| input.set_value("", window, cx));
        }
        let task=cx.background_executor().spawn(async move{
 let fresh=read(&session,&company,&year)?;if !fresh.same_scope(&review.scope,&origin,&company,local.as_deref()){return Err("Server, bruger, ledger eller regnskabsår ændrede sig. Intet sendt; gennemgå igen.".into());}
 // Recheck the exact chosen date against the current canonical fiscal bounds, without date math.
 let chosen:Value=serde_json::from_slice(&review.request.body).map_err(|_|"Gennemgået anmodning mangler")?;let date=chosen["tripDate"].as_str().ok_or("Gennemgået dato mangler")?;if date<fresh.data()["periodStart"].as_str().unwrap()||date>fresh.data()["periodEnd"].as_str().unwrap(){return Err("Regnskabsperioden ændrede sig. Intet sendt.".into());}
 let marker=fresh.marker(&origin,&company,local.as_deref());state::begin(&marker)?;
 let response=session.runtime.block_on(session.client.register_mileage(&company,review.request.body,true));
 let(mut message,receipt)=match response{
 Ok(value)=>match state::receipt(&value){Ok(receipt)=>("Serveren har kvitteret for registreringen.".to_owned(),Some(receipt)),Err(e)=>(format!("{e}. Ingen automatisk gentagelse; genlæs og afklar."),None)},
 Err(Error::Refused{status})=>{state::finish(&marker,false)?;(format!("Serveren afviste registreringen (HTTP{status}). Genlæs og kontrollér felter, satsgrundlag og aktuel adgang. Ingen automatisk gentagelse."),None)},
 Err(Error::InvalidInput(_))=>{state::finish(&marker,false)?;("Anmodningen blev afvist lokalt før netværkskald. Gennemgå igen.".into(),None)},
 _=>("Udfaldet er uafklaret. Registreringen kan være gennemført. Genlæs serverens kørsler og afklar manuelt; der sendes aldrig automatisk igen.".into(),None)
 };
 let next=read(&session,&company,&year);if let(Some(receipt),Ok(report))=(&receipt,&next){if report.same_scope(&marker,&origin,&company,local.as_deref())&&state::readback(report,receipt).is_some(){state::finish(&marker,false)?;message.push_str(" Den præcise kvitterede registrering er læst tilbage fra serverens kanoniske rapport.");}else{message.push_str(" Kanonisk registrering kunne ikke bekræftes; afklar manuelt før ny registrering.");}}else if receipt.is_some(){message.push_str(" Genlæsning fejlede; kvitteringen vises, men afklaring kræves før ny registrering.");}
 Ok::<_,String>((message,receipt,next,state::load()?))
 });
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, _, cx| {
                this.busy = false;
                match result {
                    Ok((message, receipt, next, pending)) => {
                        this.status = message;
                        this.receipt_year = if receipt.is_some() {
                            this.year.read(cx).value().trim().to_owned()
                        } else {
                            String::new()
                        };
                        this.receipt = receipt;
                        this.pending = pending;
                        match next {
                            Ok(data) => this.data = Some(data),
                            Err(e) => {
                                this.data = None;
                                this.status.push_str(&format!(" Genlæsning: {e}"));
                            }
                        }
                    }
                    Err(e) => {
                        this.status = e;
                        this.data = None;
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn resolve(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.resolved {
            return;
        }
        let Some(marker) = self.pending.clone() else {
            return;
        };
        let Some(data) = &self.data else {
            return;
        };
        if !data.same_scope(&marker, &self.origin, &self.company, self.local.as_deref()) {
            return;
        }
        self.busy = true;
        let session = self.session.clone();
        let company = self.company.clone();
        let origin = self.origin.clone();
        let local = self.local.clone();
        let year = marker["year"].as_str().unwrap().to_owned();
        let task = cx.background_executor().spawn(async move {
            let report = read(&session, &company, &year)?;
            if !report.same_scope(&marker, &origin, &company, local.as_deref()) {
                return Err("Den oprindelige bruger, virksomhed eller ledger er ikke aktiv".into());
            }
            state::finish(&marker, true)?;
            Ok::<_, String>(report)
        });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,_,cx|{this.busy=false;this.resolved=false;match result{Ok(report)=>{this.data=Some(report);this.pending=None;this.status="Den manuelle afklaring er arkiveret uden genafsendelse. En ny registrering kræver nye felter og ny gennemgang.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
}
impl Render for Mileage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let blocked =
            self.busy || self.pending.is_some() || self.review.is_some() || self.data.is_none();
        let mut view=div().flex().flex_col().gap_3().p_4().child(div().text_xl().child("Kørsel · dokumentation uden bogføring")).child(format!("{} · {}",self.company,self.origin)).child(self.status.clone()).child("Sats og satsgrundlag vælges af dig. Ingen sats indsættes automatisk, og registreringen fastslår ikke skattefrihed.").child(Input::new(&self.year).disabled(self.busy||self.review.is_some())).child(Button::new("mileage-refresh").label("Genlæs eksplicit regnskabsår og serverens kørsler").disabled(self.busy||self.review.is_some()).on_click(cx.listener(|this,_,window,cx|this.load(window,cx))));
        if let Some(receipt) = &self.receipt {
            view = view.child(div().text_lg().child(format!(
                "Serverkvittering {} · år {} · Beløbsgrundlag {}",
                receipt["entryNo"].as_str().unwrap_or("?"),
                self.receipt_year,
                receipt["amountBasis"]
            )));
            for warning in receipt["warnings"].as_array().into_iter().flatten() {
                view = view.child(format!(
                    "Serverens advarsel: {}",
                    warning.as_str().unwrap_or("?")
                ));
            }
            view = view.child(format!(
                "Anvendte serverregler: {}",
                receipt["appliedRules"]
            ));
        }
        let Some(data) = &self.data else {
            return view;
        };
        let report = data.data();
        view = view.child(format!(
            "{} · {} — {} · {} ture · {} km · beløbsgrundlag {} (serverens totaler)",
            report["selectedYear"].as_str().unwrap_or("?"),
            report["periodStart"].as_str().unwrap_or("?"),
            report["periodEnd"].as_str().unwrap_or("?"),
            report["tripCount"],
            report["totalKilometers"],
            report["totalAmountBasis"]
        ));
        view = view.child(format!(
            "Serverens regnskabsår: {}",
            report["fiscalYears"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|y| y["label"].as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        if let Some(marker) = self.pending.clone() {
            view=view.child(format!("UAFKLARET registrering · oprindelig server {} · virksomhed {} · år {}. Seneste kendte id før forsøget: {}. Privat status gemmer kun scope og seneste kendte id, aldrig tur, chauffør, sats eller genafsendelig anmodning.",marker["origin"].as_str().unwrap_or("?"),marker["company"].as_str().unwrap_or("?"),marker["year"].as_str().unwrap_or("?"),marker["baselineMaxId"])).child("Rapportlæsning er ikke i sig selv en kvittering. Andre brugere kan også have oprettet ture; kontrollér udfaldet manuelt.");
            if data.same_scope(&marker, &self.origin, &self.company, self.local.as_deref()) {
                view=view.child(Checkbox::new("mileage-resolved").label("Jeg har genlæst og manuelt afklaret den oprindelige registrering; arkivér status uden genafsendelse.").checked(self.resolved).disabled(self.busy).on_click(cx.listener(|this,checked,_,cx|{this.resolved=*checked;cx.notify();}))).child(Button::new("mileage-resolve").label("Genlæs og arkivér manuel afklaring").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,window,cx|this.resolve(window,cx))));
            }
        }
        view = view.child(div().text_lg().child("Registrér en ny tur"));
        for (key, label) in FIELDS {
            if let Some((_, input)) = self.fields.iter().find(|(k, _)| k == key) {
                view = view
                    .child(*label)
                    .child(Input::new(input).disabled(blocked));
            }
        }
        if let Some(review) = &self.review {
            let value: Value = serde_json::from_slice(&review.request.body).unwrap_or(Value::Null);
            view = view.child(div().text_lg().child("Gennemgå præcis registrering"));
            for (key, label) in FIELDS {
                if let Some(v) = value.get(*key) {
                    view = view.child(format!(
                        "{label}: {}",
                        v.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v.to_string())
                    ));
                }
            }
            view=view.child("Serveren beregner beløbet og returnerer sine advarsler ved registrering. Der oprettes ingen journalpost.").child(Checkbox::new("mileage-reviewed").label("Jeg bekræfter ruten, datoen, kilometer og mit udtrykkelige satsgrundlag.").checked(self.reviewed).disabled(self.busy).on_click(cx.listener(|this,checked,_,cx|{this.reviewed=*checked;cx.notify();}))).child(Button::new("mileage-confirm").label("Bekræft og registrér tur én gang").disabled(self.busy||!self.reviewed).on_click(cx.listener(|this,_,window,cx|this.send(window,cx)))).child(Button::new("mileage-cancel").label("Tilbage til felter").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.reviewed=false;cx.notify();})));
        } else {
            view = view.child(
                Button::new("mileage-review")
                    .label("Gennemgå registrering")
                    .disabled(blocked)
                    .on_click(cx.listener(|this, _, _, cx| this.review(cx))),
            );
        }
        view = view.child(div().text_lg().child("Kanoniske registreringer"));
        for entry in report["entries"].as_array().into_iter().flatten() {
            let recent = self
                .receipt
                .as_ref()
                .is_some_and(|r| state::readback(data, r).is_some_and(|e| e["id"] == entry["id"]));
            view = view.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(format!(
                        "{}{} · id{} · {} · {}",
                        if recent { "LÆST TILBAGE · " } else { "" },
                        entry["entryNo"].as_str().unwrap_or("?"),
                        entry["id"],
                        entry["tripDate"].as_str().unwrap_or("?"),
                        entry["purpose"].as_str().unwrap_or("?")
                    ))
                    .child(format!(
                        "{} → {} · {} · chauffør {}",
                        entry["fromLocation"].as_str().unwrap_or("?"),
                        entry["toLocation"].as_str().unwrap_or("?"),
                        entry["vehicle"].as_str().unwrap_or("?"),
                        entry["driver"].as_str().unwrap_or("?")
                    ))
                    .child(format!(
                        "{} km · valgt sats {} kr/km · beløbsgrundlag {} · {}",
                        entry["kilometers"],
                        entry["ratePerKm"],
                        entry["amountBasis"],
                        entry["rateBasis"].as_str().unwrap_or("?")
                    ))
                    .child(format!(
                        "Takstkilde: {} · Noter: {} · Oprettet {}",
                        entry["rateSource"].as_str().unwrap_or("Ikke angivet"),
                        entry["notes"].as_str().unwrap_or("Ikke angivet"),
                        entry["createdAt"].as_str().unwrap_or("?")
                    )),
            );
        }
        view = view.child(div().text_lg().child("Serverens månedsopgørelse"));
        for month in report["months"].as_array().into_iter().flatten() {
            view = view.child(format!(
                "{} · {} ture · {} km · beløbsgrundlag {}",
                month["label"].as_str().unwrap_or("?"),
                month["tripCount"],
                month["kilometers"],
                month["amountBasis"]
            ));
        }
        view
    }
}
