//! Supplier planning and cash forecast; all money, dates, evidence and forecasts are server-owned.
use crate::{Session, commitments_state as state, readable};
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
use serde_json::{Value, json};
use state::{Action, Form, Review, Snapshot};
use std::sync::Arc;
fn read(session: &Session, company: &str, start: &str) -> Result<Snapshot, String> {
    let value = session
        .runtime
        .block_on(
            session
                .client
                .supplier_commitments(company, (!start.is_empty()).then_some(start)),
        )
        .map_err(readable)?;
    let matches = session
        .runtime
        .block_on(session.client.supplier_commitment_matches(company))
        .map_err(readable)?;
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let journal = session
        .runtime
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    Snapshot::parse(company, value, matches, me, journal)
}
pub struct Commitments {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    forecast_start: Entity<InputState>,
    loaded_start: String,
    reference: Entity<InputState>,
    kind: Entity<InputState>,
    description: Entity<InputState>,
    purpose: Entity<InputState>,
    amount: Entity<InputState>,
    currency: Entity<InputState>,
    next: Entity<InputState>,
    start: Entity<InputState>,
    end: Entity<InputState>,
    renewal: Entity<InputState>,
    notice: Entity<InputState>,
    vat: Entity<InputState>,
    reason: Entity<InputState>,
    frequency: String,
    vendor: Option<i64>,
    documents: Vec<i64>,
    selected: Option<String>,
    occurrence: Option<Value>,
    evidence: Option<Value>,
    data: Option<Snapshot>,
    review: Option<Review>,
    confirmed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
}
impl Commitments {
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
            forecast_start: input("Valgfri prognosestart ÅÅÅÅ-MM-DD; tomt bruger serverens dato"),
            loaded_start: String::new(),
            reference: input("Eksplicit forpligtelsesreference"),
            kind: input("Type"),
            description: input("Beskrivelse"),
            purpose: input("Forretningsformål"),
            amount: input("Beløb pr. betaling, punktum og ingen tusindtalsseparator"),
            currency: input("Eksplicit valuta, fx DKK; ingen automatisk FX"),
            next: input("Næste forventede betaling ÅÅÅÅ-MM-DD"),
            start: input("Valgfri startdato"),
            end: input("Valgfri slutdato"),
            renewal: input("Valgfri fornyelsesdato"),
            notice: input("Valgfri opsigelsesfrist"),
            vat: input("Valgfrit momsFORSLAG; ingen momsbehandling bogføres"),
            reason: input("Konkret begrundelse for statusændringen"),
            frequency: String::new(),
            vendor: None,
            documents: Vec::new(),
            selected: None,
            occurrence: None,
            evidence: None,
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
        self.frequency.clear();
        self.vendor = None;
        self.documents.clear();
        self.selected = None;
        self.occurrence = None;
        self.evidence = None;
        for input in [
            &self.reference,
            &self.kind,
            &self.description,
            &self.purpose,
            &self.amount,
            &self.currency,
            &self.next,
            &self.start,
            &self.end,
            &self.renewal,
            &self.notice,
            &self.vat,
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
        self.loaded_start = self.forecast_start.read(cx).value().to_string();
        self.status =
            "Henter alle forpligtelser, historik, kanoniske match og serverens13-ugers prognose…"
                .into();
        let (session, company, start) = (
            self.session.clone(),
            self.company.clone(),
            self.loaded_start.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            let pending = state::load()?;
            Ok::<_, String>((read(&session, &company, &start)?, pending))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok((data,pending))=>{this.loaded_start=data.values["startDate"].as_str().unwrap_or("").into();this.data=Some(data);this.pending=pending;this.status="Planlægning og matching opretter ingen faktura, kreditor, betaling eller journal. Prognosens dækning, antagelser, mangler og eventuelle FX-undtagelser kommer fra serveren.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn plan(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = self.data.clone() else {
            return;
        };
        let (
            reference,
            kind,
            description,
            purpose,
            amount,
            currency,
            next,
            start,
            end,
            renewal,
            notice,
            vat,
        ) = (
            self.reference.read(cx).value().to_string(),
            self.kind.read(cx).value().to_string(),
            self.description.read(cx).value().to_string(),
            self.purpose.read(cx).value().to_string(),
            self.amount.read(cx).value().to_string(),
            self.currency.read(cx).value().to_string(),
            self.next.read(cx).value().to_string(),
            self.start.read(cx).value().to_string(),
            self.end.read(cx).value().to_string(),
            self.renewal.read(cx).value().to_string(),
            self.notice.read(cx).value().to_string(),
            self.vat.read(cx).value().to_string(),
        );
        let input = match state::input(
            &data,
            Form {
                id: &reference,
                vendor: self.vendor,
                documents: &self.documents,
                kind: &kind,
                description: &description,
                purpose: &purpose,
                amount: &amount,
                currency: &currency,
                frequency: &self.frequency,
                next: &next,
                start: &start,
                end: &end,
                renewal: &renewal,
                notice: &notice,
                vat: &vat,
            },
        ) {
            Ok(i) => i,
            Err(e) => {
                self.status = e;
                cx.notify();
                return;
            }
        };
        self.busy = true;
        self.review = None;
        self.confirmed = false;
        self.status = "Henter en ikke-muterende serverplan med verificerede originalkilder…".into();
        let (session, company, start) = (
            self.session.clone(),
            self.company.clone(),
            self.loaded_start.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            let fresh = read(&session, &company, &start)?;
            if !data.current(&fresh) {
                return Err(
                    "Kilder, forpligtelser eller identitet ændrede sig; genlæs og vælg igen".into(),
                );
            }
            let bytes = serde_json::to_vec(&json!({"commitment":input}))
                .map_err(|_| "Input kunne ikke fastholdes")?;
            let plan = session
                .runtime
                .block_on(session.client.plan_supplier_commitment(&company, bytes))
                .map_err(readable)?;
            Review::planned(&fresh, input, plan)
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok(review)=>{this.review=Some(review);this.status="Serverplanen er beregnet uden godkendelse. Gennemgå alle kilder, beløb, datoer, advarsler og schedule før separat bekræftelse.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn change(&mut self, status: &str, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        let Some(row) = data
            .rows("commitments")
            .into_iter()
            .find(|r| r["commitmentId"].as_str() == self.selected.as_deref())
        else {
            self.status = "Vælg en faktisk forpligtelse".into();
            cx.notify();
            return;
        };
        match Review::change(data, &row, status, self.reason.read(cx).value().as_ref()) {
            Ok(r) => {
                self.review = Some(r);
                self.confirmed = false;
            }
            Err(e) => self.status = e,
        }
        cx.notify();
    }
    fn matching(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let (Some(data), Some(occurrence), Some(evidence)) =
            (&self.data, &self.occurrence, &self.evidence)
        else {
            self.status = "Vælg en faktisk uafstemt forekomst og kanonisk evidens".into();
            cx.notify();
            return;
        };
        match Review::matching(data, occurrence, evidence) {
            Ok(r) => {
                self.review = Some(r);
                self.confirmed = false;
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
            "Genkontrollerer kilder og gennemgået bevis; sender én selvstændig handling…".into();
        let (session, company, start, origin, local) = (
            self.session.clone(),
            self.company.clone(),
            self.loaded_start.clone(),
            self.origin.clone(),
            self.local.clone(),
        );
        let task=cx.background_executor().spawn(async move{
   let fresh=read(&session,&company,&start)?;if !review.source.current(&fresh){return Err("Kilder, forecast, historik eller identitet ændrede sig. Intet sendt; genlæs og gennemgå igen.".into());}
   if let Action::Apply{input,plan}=&review.action{let root=session.runtime.block_on(session.client.plan_supplier_commitment(&company,serde_json::to_vec(&json!({"commitment":input})).map_err(|_|"Plan kunne ikke fastholdes")?)).map_err(readable)?;if root["ok"]!=true||root["plan"]!=*plan{return Err("Serverplanen eller originalkilden ændrede sig; intet godkendt. Gennemgå en ny plan; intet bevis overtages.".into());}}
   let record=review.marker(&origin,local.as_deref());state::begin(&record)?;
   let result=session.runtime.block_on(session.client.mutate_supplier_commitment(&company,review.sdk(),review.body.clone(),true));
   let(message,receipt)=match result{
    Ok(value)=>if let Some(receipt)=review.receipt(&value){state::finish(&record,false)?;("Serveren bekræftede plan-/matchhandlingen; dette er ikke bogføring eller en betaling.".into(),Some(receipt))}else{("Kvitteringen er ufuldstændig. Handlingen kan være registreret; genlæs og afklar manuelt uden genafsendelse.".into(),None)},
    Err(Error::Refused{status})=>{state::finish(&record,false)?;(format!("Serveren afviste handlingen (HTTP{status}). Kontrollér adgang, aktuelle originaler/leverandør, planhash/status, forekomst, kanonisk evidens og tidligere matching. Ingen automatisk gentagelse."),None)},
    Err(Error::InvalidInput(_))=>{state::finish(&record,false)?;("Anmodningen blev afvist før netværkskald. Genlæs og gennemgå igen.".into(),None)},
    _=>("Udfaldet er uafklaret. Genlæs forpligtelseshistorik, approvalhash og kanoniske match og afklar præcis dette forsøg manuelt. Ingen genafsendelse, nye replaynøgler eller automatisk adoption.".into(),None)
   };
   let next=read(&session,&company,&start);let message=if let(Some(receipt),Ok(data))=(&receipt,&next){let mut message=format!("{message} {}",if review.readback(receipt,data){"Præcis kvittering findes i den kanoniske historik/matching; hele forecast er genlæst."}else{"Genlæsningen bekræftede ikke hele kvitteringen; gennemgå aktuel historik før videre arbejde."});for line in state::lines(receipt,"kvittering"){message.push_str(&format!("\n{line}"));}message}else{message};Ok::<_,String>((message,next,state::load()?))
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
impl Render for Commitments {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut view=div().flex().flex_col().gap_3().child(div().text_xl().child("Leverandørforpligtelser og13-ugers likviditetsprognose"))
   .child(format!("{} · {}",self.company,self.origin)).child(self.status.clone())
   .child("Planer er rådgivende. Denne visning udfører ingen bogføring, betaling, automatisk fakturering eller ekstern handling. Forecastets null/mangler er ukendte værdier, ikke nul.")
   .child(Input::new(&self.forecast_start).disabled(self.busy||self.review.is_some()))
   .child(Button::new("commitments-reload").label("Genlæs serverens forecast, alle planer, historik og match; ryd formular/review").disabled(self.busy||self.review.is_some()).on_click(cx.listener(|this,_,window,cx|this.load(window,cx))));
        let Some(data) = self.data.clone() else {
            return view;
        };
        view = view.child(div().text_lg().child(
            "Serverens fulde13-ugers forecast · dækning, kilder, antagelser og udeladelser",
        ));
        for line in state::lines(&data.values["forecast"], "forecast") {
            view = view.child(line);
        }
        for key in ["alerts", "history", "matches"] {
            view = view.child(div().text_lg().child(format!("Kanonisk {key}")));
            for line in state::lines(&data.values[key], key) {
                view = view.child(line);
            }
        }
        view = view.child(
            div()
                .text_lg()
                .child("Aktuelle forpligtelser · oprindeligt forslag og seneste status/hash"),
        );
        for row in data.rows("commitments") {
            for line in state::lines(&row, "forpligtelse") {
                view = view.child(line);
            }
            if let Some(id) = row["commitmentId"].as_str() {
                let id = id.to_owned();
                view = view.child(
                    Button::new(SharedString::from(format!("commitments-select-{id}")))
                        .label(format!("Vælg {id} til særskilt statusændring"))
                        .disabled(
                            self.busy
                                || self.review.is_some()
                                || self.pending.is_some()
                                || row["status"] == "ended",
                        )
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
            view=view.child(div().text_lg().child("Uafklaret handling · ingen genafsendelse")).child(format!("{} · {} · reference {} · forekomst {} · hash {} · oprindeligt endpoint {}",record["company"],record["operation"],record["commitmentId"],record["occurrenceDate"],record["payloadHash"],record["origin"]))
    .child("Afklar præcis dette forsøg manuelt i kanonisk approval-/statushistorik og match. En lignende projektion beviser ikke alene udfaldet. Privat status gemmer kun scope, handling, reference, forekomstdato og hash; ingen beløb, kilder eller anmodning kan afspilles.");
            if state::scope(&record, &self.origin, &data, self.local.as_deref()) {
                view=view.child("Bruger, virksomhed, ledger og profil matcher. Ved lokal genstart kan endpoint være ændret; ingen handling genafsendes ved arkivering.")
     .child(Checkbox::new("commitments-resolved").label("Jeg har manuelt afklaret dette præcise forsøg og bevis i korrekt virksomhed").checked(self.resolved).disabled(self.busy).on_click(cx.listener(|this,checked,_,cx|{this.resolved=*checked;cx.notify();})))
     .child(Button::new("commitments-archive-pending").label("Arkivér manuel afklaring uden genafsendelse").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};if this.busy||!this.resolved||!state::scope(record,&this.origin,data,this.local.as_deref()){return;}match state::finish(record,true){Ok(())=>{this.pending=None;this.resolved=false;this.status="Manuel afklaring arkiveret. Intet blev genafsendt; en ny handling kræver nyt kildevalg og review.".into();},Err(e)=>this.status=e}cx.notify();})));
            } else {
                view=view.child("Vælg oprindelig bruger, virksomhed og lokal profil/eksternt endpoint for manuel afklaring.");
            }
            return view;
        }
        if let Some(review) = &self.review {
            view = view.child(
                div()
                    .text_lg()
                    .child(format!("Gennemgå særskilt handling: {}", review.name())),
            );
            match &review.action {
                Action::Apply { plan, .. } => {
                    for line in state::lines(plan, "præcisServerplan") {
                        view = view.child(line);
                    }
                }
                Action::Change { row, .. } => {
                    for line in state::lines(row, "oprindeligForpligtelse") {
                        view = view.child(line);
                    }
                }
                Action::Match {
                    occurrence,
                    evidence,
                } => {
                    for (k, value) in [("forekomst", occurrence), ("kanoniskEvidens", evidence)] {
                        for line in state::lines(value, k) {
                            view = view.child(line);
                        }
                    }
                }
            }
            for line in state::lines(
                &serde_json::from_slice::<Value>(&review.body).unwrap_or(Value::Null),
                "præcisAnmodning",
            ) {
                view = view.child(line);
            }
            return view.child("Godkendelse bruger præcis serverplan/hash og kontrolleres igen før afsendelse. Ingen ny replaynøgle, automatisk gentagelse eller lokal beregning foretages.")
    .child(Checkbox::new("commitments-confirmed").label("Jeg har gennemgået faktiske kilder, leverandør, beløb/datoer, planadvarsler og denne præcise handling").checked(self.confirmed).disabled(self.busy).on_click(cx.listener(|this,checked,_,cx|{this.confirmed=*checked;cx.notify();})))
    .child(Button::new("commitments-send").label("Bekræft og send den gennemgåede handling én gang").disabled(self.busy||!self.confirmed).on_click(cx.listener(|this,_,window,cx|this.send(window,cx))))
    .child(Button::new("commitments-cancel").label("Annullér før afsendelse").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.confirmed=false;cx.notify();})));
        }
        if let Some(row) = data
            .rows("commitments")
            .into_iter()
            .find(|r| r["commitmentId"].as_str() == self.selected.as_deref())
        {
            view = view
                .child(div().text_lg().child(format!(
                    "Statusændring for {} · aktuelt {}",
                    row["commitmentId"], row["status"]
                )))
                .child(Input::new(&self.reason).disabled(self.busy));
            if row["status"] != "ended" {
                for (status, label) in [
                    ("paused", "Gennemgå pause med begrundelse"),
                    ("ended", "Gennemgå afslutning med begrundelse"),
                    (
                        "superseded",
                        "Gennemgå markering som erstattet med begrundelse",
                    ),
                ] {
                    view = view.child(
                        Button::new(SharedString::from(format!("commitments-change-{status}")))
                            .label(label)
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, _, cx| this.change(status, cx))),
                    );
                }
            }
        }
        view = view.child(
            div()
                .text_lg()
                .child("Match en faktisk uafstemt forekomst med kanonisk evidens"),
        );
        for (index, occurrence) in data.rows("occurrences").into_iter().enumerate() {
            let chosen = self.occurrence.as_ref() == Some(&occurrence);
            view = view.child(
                Button::new(SharedString::from(format!(
                    "commitments-occurrence-{index}"
                )))
                .label(format!(
                    "{} {} · {} · {} {}",
                    if chosen { "✓" } else { "" },
                    occurrence["commitmentId"],
                    occurrence["date"],
                    occurrence["amount"],
                    occurrence["currency"]
                ))
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.occurrence = Some(occurrence.clone());
                    cx.notify();
                })),
            );
        }
        for (index, evidence) in data.rows("evidenceOptions").into_iter().enumerate() {
            let chosen = self.evidence.as_ref() == Some(&evidence);
            view = view.child(
                Button::new(SharedString::from(format!("commitments-evidence-{index}")))
                    .label(format!(
                        "{} {}:{} · {} · {} {} · {}",
                        if chosen { "✓" } else { "" },
                        evidence["kind"],
                        evidence["id"],
                        evidence["date"],
                        evidence["amount"],
                        evidence["currency"],
                        evidence["label"]
                    ))
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.evidence = Some(evidence.clone());
                        cx.notify();
                    })),
            );
        }
        view=view.child("Beløb, bankretning, dato, dokumentation, partsidentitet og variance afgøres af serveren. Matching er planlægningsafstemning, ikke betaling/bogføring.")
   .child(Button::new("commitments-match").label("Gennemgå match af valgte aktuelle forekomst og evidens").disabled(self.busy||self.occurrence.is_none()||self.evidence.is_none()).on_click(cx.listener(|this,_,_,cx|this.matching(cx))))
   .child(div().text_lg().child("Planlæg ny eller eksplicit revideret forpligtelse · opretter intet før godkendelse"));
        for (label, input) in [
            (
                "Reference (eksplicit; afsluttet reference kan ikke genaktiveres)",
                &self.reference,
            ),
            ("Type", &self.kind),
            ("Beskrivelse", &self.description),
            ("Forretningsformål", &self.purpose),
            (
                "Beløb pr. betaling (punktum, ingen tusindtalsseparator)",
                &self.amount,
            ),
            ("Eksplicit valuta (ingen lokal omregning)", &self.currency),
            ("Næste betalingsdato ÅÅÅÅ-MM-DD", &self.next),
            ("Startdato (valgfri)", &self.start),
            ("Slutdato (valgfri)", &self.end),
            ("Fornyelsesdato (valgfri)", &self.renewal),
            ("Opsigelsesfrist (valgfri)", &self.notice),
            (
                "Momsforslag (valgfrit; kræver separat bilagskontrol)",
                &self.vat,
            ),
        ] {
            view = view
                .child(label)
                .child(Input::new(input).disabled(self.busy));
        }
        view = view.child("Vælg frekvens eksplicit; schedule beregnes kun af serveren:");
        for frequency in state::FREQUENCIES {
            let frequency = *frequency;
            view = view.child(
                Button::new(SharedString::from(format!(
                    "commitments-frequency-{frequency}"
                )))
                .label(format!(
                    "{} {frequency}",
                    if self.frequency == frequency {
                        "✓"
                    } else {
                        ""
                    }
                ))
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.frequency = frequency.into();
                    cx.notify();
                })),
            );
        }
        view = view.child(
            "Vælg faktisk ledgerlokal leverandør (ingen automatisk ækvivalens med workspacepart):",
        );
        for vendor in data.rows("vendors") {
            if let Some(id) = vendor["id"].as_i64() {
                view = view.child(
                    Button::new(SharedString::from(format!("commitments-vendor-{id}")))
                        .label(format!(
                            "{} vendor:{id} · {}",
                            if self.vendor == Some(id) { "✓" } else { "" },
                            vendor["name"].as_str().unwrap_or("Intet aktuelt navn")
                        ))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.vendor = Some(id);
                            cx.notify();
                        })),
                );
            }
        }
        view=view.child("Vælg originale kildebilag fra serverregistret. Serveren validerer originalernes bytes/hash ved plan og godkendelse:");
        for doc in data.rows("documents") {
            if let Some(id) = doc["id"].as_i64() {
                view = view.child(
                    Button::new(SharedString::from(format!("commitments-document-{id}")))
                        .label(format!(
                            "{} document:{id} · {} · hash {}",
                            if self.documents.contains(&id) {
                                "✓"
                            } else {
                                ""
                            },
                            doc["documentNo"]
                                .as_str()
                                .or(doc["filename"].as_str())
                                .unwrap_or("Original"),
                            doc["sha256Hash"]
                        ))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(index) = this.documents.iter().position(|d| *d == id) {
                                this.documents.remove(index);
                            } else {
                                this.documents.push(id);
                            }
                            cx.notify();
                        })),
                );
            }
        }
        view.child(
            Button::new("commitments-plan")
                .label("Hent ikke-muterende serverplan til gennemgang")
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, _, cx| this.plan(cx))),
        )
    }
}
