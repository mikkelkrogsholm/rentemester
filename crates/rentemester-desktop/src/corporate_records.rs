//! Explicit native governance evidence lifecycle through the shared authenticated HTTP client.
use crate::{Session, backup_state, corporate_records_state as state, documents, readable};
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
use std::{path::PathBuf, sync::Arc};
fn read(session: &Session, company: &str, selection: &Selection) -> Result<Snapshot, String> {
    let page = session
        .runtime
        .block_on(
            session
                .client
                .corporate_records(company, selection.cursor, 25),
        )
        .map_err(readable)?;
    let record = selection
        .id
        .as_ref()
        .map(|id| {
            session
                .runtime
                .block_on(session.client.corporate_record(company, id))
                .map_err(readable)
        })
        .transpose()?;
    let replacement = selection
        .replacement
        .as_ref()
        .map(|id| {
            session
                .runtime
                .block_on(session.client.corporate_record(company, id))
                .map_err(readable)
        })
        .transpose()?;
    let parties = session
        .runtime
        .block_on(
            session
                .client
                .workspace_parties(company, None, selection.party_cursor, 25),
        )
        .map_err(readable)?;
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let journal = session
        .runtime
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    Snapshot::parse(
        company,
        selection,
        page,
        record,
        replacement,
        parties,
        me,
        journal,
    )
}
pub struct CorporateRecords {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    selection: Selection,
    previous: Vec<usize>,
    previous_parties: Vec<usize>,
    data: Option<Snapshot>,
    file: Option<documents::Source>,
    path: Entity<InputState>,
    source: Entity<InputState>,
    received: Entity<InputState>,
    uploader: Entity<InputState>,
    date: Entity<InputState>,
    retention: Entity<InputState>,
    assertion: Entity<InputState>,
    reason: Entity<InputState>,
    reference: Entity<InputState>,
    destination: Entity<InputState>,
    kind: String,
    sensitivity: String,
    link_kind: String,
    link_id: String,
    links: Vec<Value>,
    review: Option<Review>,
    confirmed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
}
impl CorporateRecords {
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
            previous: vec![],
            previous_parties: vec![],
            data: None,
            file: None,
            path: input("Absolut sti til original (PDF/PNG/JPEG/TXT/JSON/XML, højst16MiB)"),
            source: input("Faktisk kildehenvisning"),
            received: input("Faktisk modtagelsestid: ISO-dato/tid med tidszone"),
            uploader: input("Uploader fra faktisk kildekontekst"),
            date: input("Recorddato fra originalen (valgfri ISO-dato/tid)"),
            retention: input("Faktisk retentionUntil (valgfri ISO-dato/tid; ingen frist udledes)"),
            assertion: input("Kildebelagt assertion; ændrer ikke originalmetadata"),
            reason: input("Faktisk begrundelse for supersession"),
            reference: input("Kildebelagt ownership/disposition/legacy reference-id"),
            destination: input("Ny absolut privat destinationsfil til originalen"),
            kind: String::new(),
            sensitivity: String::new(),
            link_kind: String::new(),
            link_id: String::new(),
            links: vec![],
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
        self.file = None;
        self.review = None;
        self.confirmed = false;
        self.resolved = false;
        self.kind.clear();
        self.sensitivity.clear();
        self.link_kind.clear();
        self.link_id.clear();
        self.links.clear();
        for input in [
            &self.path,
            &self.source,
            &self.received,
            &self.uploader,
            &self.date,
            &self.retention,
            &self.assertion,
            &self.reason,
            &self.reference,
            &self.destination,
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
            "Henter synlige records, valgt originalhistorik, aktuelle virksomheder og partside…"
                .into();
        let (session, company, selection) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((read(&session, &company, &selection)?, state::load()?))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok((data,pending))=>{this.data=Some(data);this.pending=pending;this.status="Viser kun serverens aktuelle synlige records og kilder. Detaljer/originaler kan kræve skriveret i alle scope-virksomheder ved fortroligt/begrænset materiale. Link/enrichment/supersession bevarer originalidentitet og historik.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn load_file(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() || self.review.is_some() {
            return;
        }
        let path = PathBuf::from(self.path.read(cx).value().as_ref());
        self.busy = true;
        self.file = None;
        self.status = "Indlæser afgrænset original og beregner eksakt SHA-256…".into();
        let task = cx.background_executor().spawn(async move {
            if !path.is_absolute() {
                return Err("Vælg en absolut lokal originalsti".into());
            }
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|_| "Originalfilen kunne ikke undersøges")?;
            if metadata.len() == 0 || metadata.len() > state::MAX_BYTES as u64 {
                return Err("Governance-original må ikke være tom og skal være højst16MiB".into());
            }
            let file = documents::file(&path)?;
            if file.bytes.len() > state::MAX_BYTES {
                return Err("Originalen ændrede størrelse over16MiB-grænsen".into());
            }
            Ok::<_, String>(file)
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(file) => {
                        this.status = format!(
                            "Original fastholdt: {} · {} bytes · SHA-256 {}",
                            file.filename,
                            file.bytes.len(),
                            file.hash
                        );
                        this.file = Some(file);
                    }
                    Err(e) => this.status = e,
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn add_link(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        let id = if matches!(self.link_kind.as_str(), "company" | "party") {
            self.link_id.clone()
        } else {
            self.reference.read(cx).value().to_string()
        };
        match data.link(&self.link_kind, &id) {
            Ok(link) => {
                if !self.links.contains(&link) {
                    self.links.push(link);
                }
                self.status="Eksplicit link tilføjet formularen; intet sendt. Link-handlingen sender præcis ét link, upload kan omfatte listen.".into();
            }
            Err(e) => self.status = e,
        }
        cx.notify();
    }
    fn prepare(&mut self, action: Action, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        let (source, received, uploader, date, retention, assertion, reason) = (
            self.source.read(cx).value().to_string(),
            self.received.read(cx).value().to_string(),
            self.uploader.read(cx).value().to_string(),
            self.date.read(cx).value().to_string(),
            self.retention.read(cx).value().to_string(),
            self.assertion.read(cx).value().to_string(),
            self.reason.read(cx).value().to_string(),
        );
        let form = Form {
            kind: &self.kind,
            source: &source,
            received: &received,
            uploader: &uploader,
            sensitivity: &self.sensitivity,
            date: &date,
            retention: &retention,
            assertion: &assertion,
            reason: &reason,
            links: &self.links,
        };
        match Review::prepare(data, action, form, self.file.as_ref()) {
            Ok(review) => {
                self.review = Some(review);
                self.confirmed = false;
                self.status =
                    "Gennemgå frosne kilder, scope, følsomhed og præcis handling. Intet sendt."
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
        self.status = "Genlæser scope/kilder og sender præcis én bekræftet handling…".into();
        let (session, company, selection, origin, local) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
            self.origin.clone(),
            self.local.clone(),
        );
        let task=cx.background_executor().spawn(async move{
            let fresh=read(&session,&company,&selection)?;if !review.current(&fresh){return Err("Bruger, ledger, synlige records, linkmål eller erstatningskilde ændrede sig. Intet sendt; genlæs og gennemgå igen.".into());}
            let action=review.sdk()?;let marker=review.marker(&origin,local.as_deref());state::begin(&marker)?;
            let response=session.runtime.block_on(session.client.mutate_corporate_record(&company,action,review.body.as_ref().clone(),true));
            let(message,receipt)=match response{
                Ok(value)=>if review.receipt(&value){state::finish(&marker,false)?;("Serveren bekræftede handlingen med en kanonisk record/hændelseshash.".to_string(),Some(value))}else{("Ufuldstændig kvittering: udfaldet er uafklaret. Ingen ny upload/handling; genlæs original, links og historik manuelt.".into(),None)},
                Err(Error::Refused{status})=>{state::finish(&marker,false)?;(format!("Serveren afviste handlingen (HTTP{status}). Kontrollér aktuell adgang i ALLE scope-/målvirksomheder, følsomhed, kildefakta, eksisterende SHA-256/record-id og link-/erstatningsidentitet. Identiske bytes kan kræve eksplicit link i stedet for ny upload. Intet genafsendes automatisk."),None)},
                Err(Error::InvalidInput(_))=>{state::finish(&marker,false)?;("Anmodningen blev afvist før netværkskald; gennemgå præcise felter igen.".into(),None)},
                _=>("Udfaldet er uafklaret. Genlæs synlige records, immutable original og historik; afklar manuelt. Ingen automatisk adoption, genafsendelse eller nye nøgler.".into(),None)
            };
            let mut selection=selection;if let Some(receipt)=&receipt{if review.action==Action::Ingest{selection.id=receipt["record"]["recordId"].as_str().map(str::to_owned);}}
            let next=read(&session,&company,&selection);
            let mut message=message;if let(Some(receipt),Ok(data))=(&receipt,&next){message.push_str(if review.readback(receipt,data){" Kvitteringen er kanonisk genlæst med bevaret originalidentitet og aktuelle links/historikhash."}else{" Kvitteringen matcher ikke hele genlæsningen. Handlingen er bekræftet; kontrollér kanonisk historik før videre arbejde."});}
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
    fn download(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.review.is_some() {
            return;
        }
        let Some(data) = self.data.clone() else {
            return;
        };
        let Some(record) = data.record.clone() else {
            return;
        };
        let path = PathBuf::from(self.destination.read(cx).value().as_ref());
        self.busy = true;
        self.status="Genlæser record og henter verificeret original til din eksplicit valgte nye private fil…".into();
        let (session, company, selection) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
        );
        let task=cx.background_executor().spawn(async move{
            let fresh=read(&session,&company,&selection)?;if fresh!=data{return Err("Record, bruger eller scope ændrede sig. Genlæs før originaldownload.".into());}
            let value=session.runtime.block_on(session.client.corporate_record_original(&company,record["recordId"].as_str().ok_or("Record-id mangler")?)).map_err(readable)?;
            let bytes=state::original(value,&record)?;backup_state::save_bytes(&bytes,&path)?;Ok::<_,String>(format!("Eksakt original gemt privat i {} · {} bytes · SHA-256 {}. Ingen fil overskrevet; intet åbnes automatisk.",path.display(),bytes.len(),record["sha256"].as_str().unwrap_or("?")))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                this.status = result.unwrap_or_else(|e| e);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
impl Render for CorporateRecords {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locked = self.busy || self.review.is_some();
        let mut view=div().flex().flex_col().gap_3().child(div().text_xl().child("Governance-records · immutable originaler og kildebelagt historik"))
            .child(format!("Aktuel virksomhed {} · {}",self.company,self.origin)).child(self.status.clone())
            .child("Kun aktuelt synlige records vises. Normal/fortrolig/begrænset følsomhed og samtlige virksomhedslinks styrer serverens adgang. Historikken viser kanoniske hændelsestyper/hash; assertions- og supersessionstekst rekonstrueres ikke fra hash.")
            .child(Button::new("corporate-reload").label("Genlæs aktuelle kilder; ryd formular/review").disabled(locked).on_click(cx.listener(|this,_,window,cx|this.load(window,cx))))
            .child(Button::new("corporate-clear-selection").label("Ryd record/erstatningsvalg og hent synlig liste").disabled(locked).on_click(cx.listener(|this,_,window,cx|{this.selection.id=None;this.selection.replacement=None;this.load(window,cx);})));
        let Some(data) = self.data.clone() else {
            return view;
        };
        view = view
            .child(format!(
                "Serverens synlige records: {} · cursor {} · op til25 pr. side",
                data.page["count"], self.selection.cursor
            ))
            .child(
                Button::new("corporate-previous")
                    .label("Forrige recordside")
                    .disabled(locked || self.previous.is_empty())
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(cursor) = this.previous.pop() {
                            this.selection.cursor = cursor;
                            this.load(window, cx);
                        }
                    })),
            );
        if let Some(cursor) = data.page["nextCursor"]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
        {
            view = view.child(
                Button::new("corporate-next")
                    .label("Næste recordside fra serveren")
                    .disabled(locked)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.previous.push(this.selection.cursor);
                        this.selection.cursor = cursor;
                        this.load(window, cx);
                    })),
            );
        }
        for record in data.page["rows"].as_array().into_iter().flatten() {
            if let Some(id) = record["recordId"].as_str() {
                let id = id.to_owned();
                let replacement = id.clone();
                view = view
                    .child(
                        Button::new(SharedString::from(format!("corporate-inspect-{id}")))
                            .label(format!(
                                "Inspicér {} · {} · {} · {} · SHA-256 {}",
                                id,
                                record["filename"].as_str().unwrap_or("?"),
                                record["type"].as_str().unwrap_or("?"),
                                record["sensitivity"].as_str().unwrap_or("?"),
                                record["sha256"].as_str().unwrap_or("?")
                            ))
                            .disabled(locked)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.selection.id = Some(id.clone());
                                this.load(window, cx);
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "corporate-replacement-{replacement}"
                        )))
                        .label(format!(
                            "Vælg og inspicér {replacement} som mulig erstatningskilde"
                        ))
                        .disabled(locked)
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.selection.replacement = Some(replacement.clone());
                                this.load(window, cx);
                            },
                        )),
                    );
            }
        }
        if let Some(record) = &data.record {
            view=view.child(div().text_lg().child("Valgt kanonisk record · originalidentitet, følsomhed, links og fuld serverhistorik"));
            for line in state::lines(record, "record") {
                view = view.child(line);
            }
            view=view.child("Gem originalens eksakte bytes i en selvvalgt ny absolut fil. Serveren leverer ingen MIME/fremviser-URL; intet indhold åbnes eller eksekveres automatisk.")
                .child(Input::new(&self.destination).disabled(locked))
                .child(Button::new("corporate-original").label("Hent original, verificér SHA-256 og gem ny privat fil").disabled(locked).on_click(cx.listener(|this,_,_,cx|this.download(cx))));
        }
        if let Some(replacement) = &data.replacement {
            view = view.child(
                div()
                    .text_lg()
                    .child("Valgt faktisk erstatningskilde · gennemgå separat før supersession"),
            );
            for line in state::lines(replacement, "erstatningsRecord") {
                view = view.child(line);
            }
        }
        if let Some(record) = self.pending.clone() {
            view=view.child(div().text_lg().child("Uafklaret governanceforsøg · ingen genafsendelse"))
                .child(format!("Handling {} · record {} · erstatning {} · originalhash {} · endpoint {}",record["operation"],record["recordId"],record["replacementId"],record["sha256"],record["origin"]))
                .child("Genlæs de aktuelle records, links, originaler og historik. Ny uploads record-id kan være ukendt; lignende bytes/hash alene beviser ikke dette forsøgs udfald. Den private status indeholder kun scope/identiteter/hash, aldrig originalbytes, filnavne, assertioner eller begrundelser.");
            if let Some(id) = record["recordId"].as_str() {
                let id = id.to_owned();
                view = view.child(
                    Button::new("corporate-inspect-pending")
                        .label("Genlæs uafklaret records historik")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.selection.id = Some(id.clone());
                            this.load(window, cx);
                        })),
                )
            }
            if state::scope(&record, &self.origin, &data, self.local.as_deref()) {
                view=view.child("Profil, bruger, virksomhed og ledger matcher. Et lokalt endpoint kan ændres ved genstart; arkivering sender intet.")
                    .child(Checkbox::new("corporate-resolved").label("Jeg har manuelt afklaret dette præcise forsøg i den viste profil og virksomhed").checked(self.resolved).disabled(self.busy).on_click(cx.listener(|this,value,_,cx|{this.resolved=*value;cx.notify();})))
                    .child(Button::new("corporate-archive-resolution").label("Arkivér manuel afklaring uden genafsendelse").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};if this.busy||!this.resolved||!state::scope(record,&this.origin,data,this.local.as_deref()){return;}match state::finish(record,true){Ok(())=>{this.pending=None;this.resolved=false;this.status="Manuel afklaring arkiveret. Ingen handling blev gentaget.".into();},Err(e)=>this.status=e}cx.notify();})));
            } else {
                view=view.child("Vælg oprindelig bruger, virksomhed og privat lokal profil/eksternt endpoint for manuel afklaring.");
            }
            return view;
        }
        if let Some(review) = &self.review {
            view = view.child(div().text_lg().child(format!(
                "Gennemgå {} · virksomhed {}",
                review.action.name(),
                review.snapshot.company
            )));
            if review.action != Action::Ingest {
                if let Some(record) = &review.snapshot.record {
                    for line in state::lines(record, "oprindeligRecord") {
                        view = view.child(line);
                    }
                }
            }
            if review.action == Action::Supersede {
                if let Some(record) = &review.snapshot.replacement {
                    for line in state::lines(record, "faktiskErstatning") {
                        view = view.child(line);
                    }
                }
            }
            for line in state::lines(&review.display, "præcisHandling") {
                view = view.child(line);
            }
            return view.child(format!("Upload knyttes af serveren til den aktuelle virksomhed {} samt de eksplicit valgte ekstra links. Originalbytes/identitet og kildefakta bevares. Serveren kontrollerer aktuelle rettigheder i alle scopes. Ingen ejerskabs-/juridiske facts udledes af filen.",self.company))
                .child(Checkbox::new("corporate-confirmed").label("Jeg har gennemgået præcis original, kildefakta, følsomhed, links, erstatning og handling").checked(self.confirmed).disabled(self.busy).on_click(cx.listener(|this,value,_,cx|{this.confirmed=*value;cx.notify();})))
                .child(Button::new("corporate-send").label("Bekræft og send denne ene gennemgåede handling").disabled(self.busy||!self.confirmed).on_click(cx.listener(|this,_,window,cx|this.send(window,cx))))
                .child(Button::new("corporate-cancel").label("Annullér før afsendelse").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.confirmed=false;cx.notify();})));
        }
        view = view.child(
            div()
                .text_lg()
                .child("Kildebelagte links · vælg type og konkret reference"),
        );
        for kind in state::LINKS {
            let kind = *kind;
            view = view.child(
                Button::new(SharedString::from(format!("corporate-link-kind-{kind}")))
                    .label(format!(
                        "{} {kind}",
                        if self.link_kind == kind { "✓" } else { "" }
                    ))
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.link_kind = kind.into();
                        this.link_id.clear();
                        cx.notify();
                    })),
            );
        }
        if self.link_kind == "company" {
            view=view.child("Aktuelt synlige virksomheder. Valg er ikke en forhåndsgodkendelse af skriveret; serveren kontrollerer alle scope-virksomheder.");
            for company in &data.companies {
                if let Some(slug) = company["slug"].as_str() {
                    let slug = slug.to_owned();
                    view = view.child(
                        Button::new(SharedString::from(format!("corporate-company-{slug}")))
                            .label(format!(
                                "{} {} · {slug}",
                                if self.link_id == slug { "✓" } else { "" },
                                company["name"].as_str().unwrap_or("?")
                            ))
                            .disabled(self.busy || company["archived"] == true)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.link_id = slug.clone();
                                cx.notify();
                            })),
                    );
                }
            }
        } else if self.link_kind == "party" {
            view=view.child(format!("Aktuelt synlig partside · count {} · cursor {}. Sideskift rydder formularens links; ingen skjult partidentitet udledes.",data.parties["count"],self.selection.party_cursor))
                .child(Button::new("corporate-parties-previous").label("Forrige partside").disabled(self.busy||self.previous_parties.is_empty()).on_click(cx.listener(|this,_,window,cx|{if let Some(cursor)=this.previous_parties.pop(){this.selection.party_cursor=cursor;this.load(window,cx);}})));
            if let Some(cursor) = data.parties["nextCursor"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
            {
                view = view.child(
                    Button::new("corporate-parties-next")
                        .label("Næste partside fra serveren")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.previous_parties.push(this.selection.party_cursor);
                            this.selection.party_cursor = cursor;
                            this.load(window, cx);
                        })),
                )
            }
            for party in data.parties["rows"].as_array().into_iter().flatten() {
                if let Some(id) = party["partyId"].as_str() {
                    let id = id.to_owned();
                    view = view.child(
                        Button::new(SharedString::from(format!("corporate-party-{id}")))
                            .label(format!(
                                "{} {} · {id}",
                                if self.link_id == id { "✓" } else { "" },
                                party["name"].as_str().unwrap_or("?")
                            ))
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.link_id = id.clone();
                                cx.notify();
                            })),
                    );
                }
            }
        } else if matches!(
            self.link_kind.as_str(),
            "ownership" | "disposition" | "legacy_evidence_ref"
        ) {
            view=view.child("Angiv den eksisterende kildebelagte reference-id. Dette opretter ingen ejer-/dispositionsfact eller juridisk konklusion.").child(Input::new(&self.reference).disabled(self.busy));
        }
        view = view
            .child(
                Button::new("corporate-add-link")
                    .label("Tilføj eksplicit link til gennemgangslisten")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.add_link(cx))),
            )
            .child(
                Button::new("corporate-clear-links")
                    .label("Ryd formularens linkliste")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.links.clear();
                        cx.notify();
                    })),
            );
        for line in state::lines(&serde_json::json!(self.links), "valgteLinks") {
            view = view.child(line);
        }
        if data.record.is_some() {
            view=view.child(Button::new("corporate-review-link").label("Gennemgå tilføjelse af ét link til valgt record").disabled(self.busy||self.links.len()!=1).on_click(cx.listener(|this,_,_,cx|this.prepare(Action::Link,cx))))
                .child("Konkret kildebelagt assertion (append-only; originalmetadata ændres ikke)").child(Input::new(&self.assertion).disabled(self.busy))
                .child(Button::new("corporate-review-enrich").label("Gennemgå enrichment af valgt record").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.prepare(Action::Enrich,cx))))
                .child("Supersession kræver en separat faktisk erstatnings-record valgt og inspiceret ovenfor samt din begrundelse. Original og historik bevares.").child(Input::new(&self.reason).disabled(self.busy))
                .child(Button::new("corporate-review-supersede").label("Gennemgå reasoned supersession").disabled(self.busy||data.replacement.is_none()).on_click(cx.listener(|this,_,_,cx|this.prepare(Action::Supersede,cx))));
        }
        view=view.child(div().text_lg().child("Indlæs ny immutable original · eksplicit type og faktiske metadata"))
            .child("Native filvalg understøtter PDF/PNG/JPEG/TXT/JSON/XML, højst16MiB. Der udledes ingen titel, juridisk effekt, ejerandel, retentionfrist eller identitet. Filnavnet bevares fra den valgte original.")
            .child(Input::new(&self.path).disabled(self.busy))
            .child(Button::new("corporate-load-file").label("Indlæs og fasthold originalbytes/SHA-256").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.load_file(cx))));
        if let Some(file) = &self.file {
            view = view.child(format!(
                "Fastholdt original {} · {} bytes · SHA-256 {}",
                file.filename,
                file.bytes.len(),
                file.hash
            ));
        }
        for kind in state::TYPES {
            let kind = *kind;
            view = view.child(
                Button::new(SharedString::from(format!("corporate-type-{kind}")))
                    .label(format!(
                        "{} {kind}",
                        if self.kind == kind { "✓" } else { "" }
                    ))
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.kind = kind.into();
                        cx.notify();
                    })),
            );
        }
        view = view.child("Vælg følsomhed eksplicit");
        for sensitivity in ["normal", "confidential", "restricted"] {
            view = view.child(
                Button::new(SharedString::from(format!(
                    "corporate-sensitivity-{sensitivity}"
                )))
                .label(format!(
                    "{} {sensitivity}",
                    if self.sensitivity == sensitivity {
                        "✓"
                    } else {
                        ""
                    }
                ))
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.sensitivity = sensitivity.into();
                    cx.notify();
                })),
            );
        }
        for (label, input) in [
            ("Faktisk kildehenvisning", &self.source),
            (
                "Faktisk modtaget (ISO-dato/tid med tidszone)",
                &self.received,
            ),
            ("Uploader fra faktisk kildekontekst", &self.uploader),
            ("Dato på record (valgfri; udledes ikke)", &self.date),
            (
                "Faktisk retentionUntil (valgfri; ingen frist beregnes)",
                &self.retention,
            ),
        ] {
            view = view
                .child(label)
                .child(Input::new(input).disabled(self.busy));
        }
        view.child(
            Button::new("corporate-review-ingest")
                .label("Gennemgå originalupload og eksplicitte metadata/links")
                .disabled(self.busy || self.file.is_none())
                .on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Ingest, cx))),
        )
    }
}
