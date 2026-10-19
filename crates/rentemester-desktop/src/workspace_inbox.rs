//! Anchor-scoped workspace ingress; all authorization, classification and handoff rules remain on the server.
use crate::{Session, document_state, documents, readable, workspace_inbox_state as state};
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
use std::{path::PathBuf, sync::Arc};

fn read(session: &Session, company: &str, selection: &Selection) -> Result<Snapshot, String> {
    let page = session
        .runtime
        .block_on(
            session
                .client
                .workspace_inbox(company, selection.cursor, 25),
        )
        .map_err(readable)?;
    let source = selection
        .id
        .as_ref()
        .map(|id| {
            session
                .runtime
                .block_on(session.client.workspace_inbox_source(company, id))
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
    Snapshot::parse(company, selection, page, source, me, journal)
}
fn original(session: &Session, source: &Value, target: &str) -> Result<(PathBuf, String), String> {
    let documents = session
        .runtime
        .block_on(session.client.documents(target))
        .map_err(readable)?;
    let (id, mime, hash) = state::completed_document(source, target, &documents)?;
    let value = session
        .runtime
        .block_on(session.client.document_original(target, id))
        .map_err(readable)?;
    if value["ok"] != true {
        return Err("Serveren bekræftede ikke målbilagets original".into());
    }
    Ok((document_state::download(value, id, &mime, &hash)?, mime))
}
pub struct WorkspaceInbox {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    selection: Selection,
    previous: Vec<usize>,
    data: Option<Snapshot>,
    path: Entity<InputState>,
    received: Entity<InputState>,
    transport: Entity<InputState>,
    fields: Vec<(String, &'static str, Entity<InputState>)>,
    kind: String,
    file: Option<documents::Source>,
    target: String,
    review: Option<Review>,
    frozen_attempt: Option<Review>,
    confirmed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
    downloaded: Option<(PathBuf, String)>,
}
impl WorkspaceInbox {
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
        let path = input("Absolut sti til lokal original (PDF, PNG, JPEG, TXT, JSON, XML)");
        let received = input("Faktisk modtaget: ISO-tid med eksplicit tidszone");
        let transport = input("Kilde-/transportidentitet (valgfri, fra faktisk kilde)");
        let fields = documents::FIELDS
            .iter()
            .map(|(key, label)| (key.to_string(), *label, input(label)))
            .collect();
        let mut this = Self {
            session,
            origin,
            company,
            local,
            selection: Selection::default(),
            previous: vec![],
            data: None,
            path,
            received,
            transport,
            fields,
            kind: String::new(),
            file: None,
            target: String::new(),
            review: None,
            frozen_attempt: None,
            confirmed: false,
            pending: None,
            resolved: false,
            busy: false,
            status: String::new(),
            downloaded: None,
        };
        this.load(window, cx);
        this
    }
    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.review = None;
        self.confirmed = false;
        self.resolved = false;
        self.file = None;
        self.kind.clear();
        self.target.clear();
        self.downloaded = None;
        for input in [&self.path, &self.received, &self.transport]
            .into_iter()
            .chain(self.fields.iter().map(|(_, _, input)| input))
        {
            input.update(cx, |input, cx| input.set_value("", window, cx));
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
            "Henter synlig indbakke, kildehistorik og aktuelle virksomhedsmuligheder…".into();
        let (session, company, selection) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((read(&session, &company, &selection)?, state::load()?))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result {Ok((data,pending))=>{this.data=Some(data);this.pending=pending;this.status="Indbakken er afgrænset til den viste ankervirksomhed. Upload, tildeling og fuldendelse er separate bekræftede handlinger. Serveren kontrollerer aktuelle rettigheder i både anker og valgt mål ved hvert trin.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn load_file(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() || self.review.is_some() {
            return;
        }
        let path = PathBuf::from(self.path.read(cx).value().as_ref());
        self.busy = true;
        self.file = None;
        self.status =
            "Indlæser en afgrænset regulær original og beregner dens eksakte hash…".into();
        let task = cx.background_executor().spawn(async move {
            if !path.is_absolute() {
                return Err("Vælg en absolut lokal originalsti".into());
            }
            documents::file(&path)
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok(file)=>{this.status=format!("Original fastholdt: {} · {} · {} bytes · SHA-256 {}. Serveren validerer den faktiske filtype ved upload.",file.filename,file.mime,file.bytes.len(),file.hash);this.file=Some(file);},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn prepare(&mut self, action: Action, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        let result = if action == Action::Ingest {
            let fields = self
                .fields
                .iter()
                .map(|(key, _, i)| (key.clone(), i.read(cx).value().to_string()))
                .collect::<Vec<_>>();
            self.file
                .as_ref()
                .ok_or_else(|| "Indlæs en faktisk lokal original først".to_string())
                .and_then(|file| {
                    Review::ingest(
                        data,
                        file,
                        &self.kind,
                        &fields,
                        self.received.read(cx).value().as_ref(),
                        self.transport.read(cx).value().as_ref(),
                    )
                })
        } else {
            Review::handoff(data, action, &self.target)
        };
        match result {
            Ok(review) => {
                self.review = Some(review);
                self.confirmed = false;
                self.status="Gennemgå den frosne kilde, klassifikation, provenance og præcise handling. Intet er sendt.".into();
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
        self.frozen_attempt = Some(review.clone());
        self.busy = true;
        self.data = None;
        self.clear(window, cx);
        self.status =
            "Genlæser bruger, ledger, side, kilde og synlige mål før én afsendelse…".into();
        let (session, company, selection, origin, local) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
            self.origin.clone(),
            self.local.clone(),
        );
        let task=cx.background_executor().spawn(async move {
            let fresh=read(&session,&company,&selection)?;
            if !review.current(&fresh){return Err("Bruger, anker, ledger, synlig side, mål eller kilde ændrede sig. Intet sendt; genlæs og gennemgå igen.".into());}
            let action=review.sdk()?;let record=review.marker(&origin,local.as_deref());state::begin(&record)?;
            let response=session.runtime.block_on(session.client.mutate_workspace_inbox(&company,action,review.body.as_ref().clone(),true));
            let(message,receipt)=match response {
                Ok(value)=>if let Some(receipt)=review.receipt(&value){state::finish(&record,false)?;("Serveren bekræftede handlingen med den kanoniske originalkilde.".to_string(),Some(receipt))}else{("Kvitteringen er ufuldstændig. Udfaldet er uafklaret; ingen genafsendelse. Genlæs kilden og dens historik manuelt.".into(),None)},
                Err(Error::Refused{status})=>{state::finish(&record,false)?;(format!("Serveren afviste handlingen (HTTP{status}). Kontrollér kildefakta, faktisk MIME, modtagelsestid, eksisterende destination, overdragelsesstatus og aktuelle rettigheder i anker/mål. Aktiv overdragelse skal inspiceres før ny handling; serveren styrer lease/recovery. Ingen automatisk gentagelse."),None)},
                Err(Error::InvalidInput(_))=>{state::finish(&record,false)?;("Anmodningen blev afvist før afsendelse; gennemgå felter igen.".into(),None)},
                _=>("Udfaldet er uafklaret. Genlæs liste/kilde og målbilag manuelt. En lignende række/hash beviser ikke alene dette forsøgs udfald. Ingen genafsendelse eller ny uploadintention.".into(),None)
            };
            let mut selection=selection;if let Some(source)=&receipt{selection.id=source["sourceId"].as_str().map(str::to_owned);}
            let next=read(&session,&company,&selection);
            let mut message=message;let mut downloaded=None;
            if let Some(receipt)=&receipt {
                if next.as_ref().ok().and_then(|s|s.source.as_ref())==Some(receipt) {
                    message.push_str(" Præcis kilde, tildeling, historie og overdragelsesstatus er kanonisk genlæst.");
                    if review.action==Action::Complete {
                        match original(&session,receipt,review.target.as_deref().ok_or("Mål mangler")?){Ok(file)=>{downloaded=Some(file);message.push_str(" Målbilagets nummer/hash/type og eksakte original er også genlæst og privat gemt.");},Err(e)=>message.push_str(&format!(" Fuldendelse er bekræftet, men målbilag/original kræver genlæsning: {e}"))}
                    }
                }else{message.push_str(" Genlæsningen matcher ikke hele kvitteringen; kontrollér aktuelle hændelser før videre arbejde.");}
            }
            Ok::<_,String>((message,next,selection,state::load()?,downloaded))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((message, next, selection, pending, downloaded)) => {
                        this.status = message;
                        this.selection = selection;
                        this.pending = pending;
                        this.downloaded = downloaded;
                        if this.pending.is_none() {
                            this.frozen_attempt = None;
                        }
                        match next {
                            Ok(data) => this.data = Some(data),
                            Err(e) => this.status.push_str(&format!(" Genlæsning fejlede: {e}")),
                        }
                    }
                    Err(e) => {
                        this.status = e;
                        this.pending = state::load().ok().flatten();
                        if this.pending.is_none() {
                            this.frozen_attempt = None;
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn read_original(&mut self, target: String, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(data) = self.data.clone() else {
            return;
        };
        let Some(source) = data.source.clone() else {
            return;
        };
        self.busy = true;
        self.downloaded = None;
        self.status = "Genlæser fuldført kildes aktuelle målbilag og eksakte original…".into();
        let (session, company, selection) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            let fresh = read(&session, &company, &selection)?;
            if fresh != data {
                return Err(
                    "Kilde, bruger eller synlige mål ændrede sig. Genlæs før originaldownload."
                        .into(),
                );
            }
            original(&session, &source, &target)
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok(file)=>{this.status="Fuldført målbilags nummer, hash, MIME og eksakte original er kanonisk verificeret. Originalen er privat gemt.".into();this.downloaded=Some(file);},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
}
impl Render for WorkspaceInbox {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locked = self.busy || self.review.is_some();
        let mut view=div().flex().flex_col().gap_3()
            .child(div().text_xl().child("Fælles indbakke · original → eksplicit mål → bilag"))
            .child(format!("Synlighedsanker {} · {}",self.company,self.origin)).child(self.status.clone())
            .child("Originalkilden og historikken bevares i workspace. Ankervirksomheden er adgangsafgrænsning, aldrig en automatisk destination. Overdragelse registrerer bilag, ikke bogføring.")
            .child(Button::new("inbox-reload").label("Genlæs side og kilde; ryd formular/review").disabled(locked).on_click(cx.listener(|this,_,window,cx|this.load(window,cx))));
        if let Some((path, mime)) = self.downloaded.clone() {
            view = view.child(format!(
                "Verificeret privat original: {} · {mime}",
                path.display()
            ));
            if document_state::can_open(&mime) {
                view = view.child(
                    Button::new("inbox-open-original")
                        .label("Åbn verificeret original i almindelig fremviser")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            match std::process::Command::new("/usr/bin/open")
                                .arg(&path)
                                .spawn()
                            {
                                Ok(_) => {
                                    this.status =
                                        "Originalen er åbnet fra den verificerede private fil."
                                            .into()
                                }
                                Err(_) => this.status =
                                    "Fremviseren kunne ikke åbnes. Brug den viste private filsti."
                                        .into(),
                            }
                            cx.notify();
                        })),
                );
            } else {
                view=view.child("JSON/XML kan hentes som original; åbnes ikke automatisk som eksekverbart eller aktivt indhold.");
            }
        }
        let Some(data) = self.data.clone() else {
            return view;
        };
        view = view
            .child(format!(
                "Serverens synlige kilder: {} · cursor {} · op til25 pr. side",
                data.page["count"], self.selection.cursor
            ))
            .child(
                Button::new("inbox-previous")
                    .label("Forrige viste side")
                    .disabled(locked || self.previous.is_empty())
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
                Button::new("inbox-next")
                    .label("Næste side fra serveren")
                    .disabled(locked)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.previous.push(this.selection.cursor);
                        this.selection.cursor = cursor;
                        this.selection.id = None;
                        this.load(window, cx);
                    })),
            );
        }
        for source in data.page["rows"].as_array().into_iter().flatten() {
            if let Some(id) = source["sourceId"].as_str() {
                let id = id.to_owned();
                view = view.child(
                    Button::new(SharedString::from(format!("inbox-source-{id}")))
                        .label(format!(
                            "Inspicér {} · {} · modtaget {}",
                            id,
                            source["filename"].as_str().unwrap_or("?"),
                            source["receivedAt"].as_str().unwrap_or("?")
                        ))
                        .disabled(locked)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.selection.id = Some(id.clone());
                            this.load(window, cx);
                        })),
                );
            }
        }
        if let Some(source) = &data.source {
            view=view.child(div().text_lg().child("Permanent originalkilde · metadata, synlige destinationer, undtagelser, claims og fuld historik"));
            for line in state::lines(source, "kilde") {
                view = view.child(line);
            }
            if source["handoffs"].as_array().is_some_and(|a| !a.is_empty()) {
                view=view.child("Overdragelsesstatus og leaseExpiresAt ovenfor er serverens værdier. Native skærmen beregner ikke udløb og genoptager intet automatisk. Ved uafklaret forsøg: genlæs kilden og målbilag først; manuel afklaring kræves. En aktiv claim kan give en ærlig serverafvisning; serveren styrer recovery efter udløb.");
            }
            for assignment in source["assignments"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|a| a["state"] == "completed")
            {
                if let Some(target) = assignment["companySlug"].as_str() {
                    let target = target.to_owned();
                    view = view.child(
                        Button::new(SharedString::from(format!("inbox-original-{target}")))
                            .label(format!(
                                "Genlæs målbilag {} i {target} og hent eksakt original",
                                assignment["documentNo"]
                            ))
                            .disabled(locked)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.read_original(target.clone(), cx)
                            })),
                    );
                }
            }
        }
        if let Some(record) = self.pending.clone() {
            view=view.child(div().text_lg().child("Uafklaret forsøg · ingen gentagelse"))
                .child(format!("Anker {} · handling {} · kilde {} · mål {} · originalhash {} · oprindeligt endpoint {}",record["company"],record["operation"],record["sourceId"],record["targetCompany"],record["sha256"],record["origin"]))
                .child("Genlæs listen, kildehistorik og eventuelle målbilag. Oprettelsens kilde-id kan være ukendt; en lignende række/hash beviser ikke alene udfaldet. Ingen bytes, filnavne, dokumentfakta eller uploadnøgle gemmes i den private status. En fastholdt upload i denne session genafsendes aldrig automatisk.");
            if let Some(id) = record["sourceId"].as_str() {
                let id = id.to_owned();
                view = view.child(
                    Button::new("inbox-inspect-pending")
                        .label("Genlæs uafklaret kildes aktuelle historik")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.selection.id = Some(id.clone());
                            this.load(window, cx);
                        })),
                );
            }
            if state::scope(&record, &self.origin, &data, self.local.as_deref()) {
                view=view.child("Profil, bruger, anker og ledger matcher. Et lokalt endpoint kan ændres ved genstart; arkivering skriver intet på serveren.")
                    .child(Checkbox::new("inbox-manually-resolved").label("Jeg har manuelt afklaret dette præcise forsøg i den viste profil og virksomhed").checked(self.resolved).disabled(self.busy).on_click(cx.listener(|this,value,_,cx|{this.resolved=*value;cx.notify();})))
                    .child(Button::new("inbox-archive-resolution").label("Arkivér manuel afklaring uden genafsendelse").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};if this.busy||!this.resolved||!state::scope(record,&this.origin,data,this.local.as_deref()){return;}match state::finish(record,true){Ok(())=>{this.pending=None;this.frozen_attempt=None;this.resolved=false;this.status="Manuel afklaring arkiveret. Ingen handling er gentaget.".into();},Err(e)=>this.status=e}cx.notify();})));
            } else {
                view=view.child("Vælg oprindelig bruger, ankervirksomhed og privat lokal profil/eksternt endpoint før manuel afklaring.");
            }
            return view;
        }
        if let Some(review) = &self.review {
            view = view.child(div().text_lg().child(format!(
                "Gennemgå {} · anker {} · mål {}",
                review.action.name(),
                review.snapshot.company,
                review.target.as_deref().unwrap_or("intet mål; kun upload")
            )));
            if review.action != Action::Ingest {
                if let Some(source) = &review.snapshot.source {
                    for line in state::lines(source, "gennemgåetKilde") {
                        view = view.child(line);
                    }
                }
            }
            for line in state::lines(&review.display, "præcisHandling") {
                view = view.child(line);
            }
            return view.child("Originalbytes, kildefakta og uploadintention er frosset. Tildeling og fuldendelse kræver skriveret i både anker og mål. Serveren validerer alle fakta og bevarer original/historik.")
                .child(Checkbox::new("inbox-confirmed").label("Jeg har gennemgået original, modtagelsestid, klassifikation, kildefakta, mål og denne ene handling").checked(self.confirmed).disabled(self.busy).on_click(cx.listener(|this,value,_,cx|{this.confirmed=*value;cx.notify();})))
                .child(Button::new("inbox-send").label("Bekræft og send gennemgået handling én gang").disabled(self.busy||!self.confirmed).on_click(cx.listener(|this,_,window,cx|this.send(window,cx))))
                .child(Button::new("inbox-cancel").label("Annullér før afsendelse").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.confirmed=false;cx.notify();})));
        }
        if data.source.is_some() {
            view=view.child(div().text_lg().child("Eksplicit destination for valgt kilde"))
                .child("Synlige virksomheder fra serveren. Visning er ikke en forhåndsgodkendelse af uploadret; serveren kontrollerer aktuelle rettigheder ved handlingen. Ingen virksomhed er valgt automatisk.");
            for company in &data.targets {
                if let Some(slug) = company["slug"].as_str() {
                    let slug = slug.to_owned();
                    view = view.child(
                        Button::new(SharedString::from(format!("inbox-target-{slug}")))
                            .label(format!(
                                "{} {} · {}{}",
                                if self.target == slug { "✓" } else { "" },
                                company["name"].as_str().unwrap_or("?"),
                                slug,
                                if company["archived"] == true {
                                    " (arkiveret)"
                                } else {
                                    ""
                                }
                            ))
                            .disabled(self.busy || company["archived"] == true)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.target = slug.clone();
                                cx.notify();
                            })),
                    );
                }
            }
            view = view
                .child(
                    Button::new("inbox-review-assign")
                        .label("Gennemgå tildeling til valgt mål")
                        .disabled(self.busy || self.target.is_empty())
                        .on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Assign, cx))),
                )
                .child(
                    Button::new("inbox-review-complete")
                        .label("Gennemgå separat fuldendelse af godkendt tildeling")
                        .disabled(self.busy || self.target.is_empty())
                        .on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Complete, cx))),
                );
        }
        view=view.child(div().text_lg().child("Upload ny original · ingen målvirksomhed tildeles"))
            .child("Kun lokal fil med transport upload. Angiv faktiske kildefakta fra originalen; ingen OCR eller gættede fakturaoplysninger. Interne bilag med bankevidens/rationale oprettes i virksomhedens bilagsflow.")
            .child(Input::new(&self.path).disabled(self.busy))
            .child(Button::new("inbox-load-file").label("Indlæs og fasthold originalbytes/hash").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.load_file(cx))));
        if let Some(file) = &self.file {
            view = view.child(format!(
                "Fastholdt original {} · {} · {} bytes · SHA-256 {}",
                file.filename,
                file.mime,
                file.bytes.len(),
                file.hash
            ));
        }
        for (kind, label) in [
            ("purchase_sale", "Købs-/salgsbilag"),
            ("cash_register_receipt", "Kassekvittering"),
        ] {
            view = view.child(
                Button::new(SharedString::from(format!("inbox-kind-{kind}")))
                    .label(format!(
                        "{} {label}",
                        if self.kind == kind { "✓" } else { "" }
                    ))
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.kind = kind.into();
                        cx.notify();
                    })),
            );
        }
        view = view
            .child("Faktisk modtaget (ISO-tid med eksplicit tidszone)")
            .child(Input::new(&self.received).disabled(self.busy))
            .child("Faktisk transport-/kildeidentitet (valgfri)")
            .child(Input::new(&self.transport).disabled(self.busy));
        for (_, label, input) in &self.fields {
            view = view
                .child(*label)
                .child(Input::new(input).disabled(self.busy));
        }
        view.child(
            Button::new("inbox-review-upload")
                .label("Gennemgå upload med én fastholdt uploadintention")
                .disabled(self.busy || self.file.is_none())
                .on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Ingest, cx))),
        )
    }
}
