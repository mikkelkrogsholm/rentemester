//! Explicit native V1 manifest/mapping lifecycle and source-period readback; no local money math.
use crate::{Session, intercompany_state as state, readable};
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
    let current = session
        .runtime
        .block_on(session.client.intercompany(company))
        .map_err(readable)?;
    let report = session
        .runtime
        .block_on(session.client.group_reconciliation(&selection.as_of))
        .map_err(readable)?;
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let journal = session
        .runtime
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    Snapshot::parse(company, selection, current, report, me, journal)
}
pub struct Intercompany {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    selection: Selection,
    as_of: Entity<InputState>,
    manifest: Entity<InputState>,
    mapping: Entity<InputState>,
    data: Option<Snapshot>,
    review: Option<Review>,
    confirmed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
}
impl Intercompany {
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
        Self{session,origin,company,local,selection:Selection::default(),as_of:input("Eksplicit afstemning pr. dato YYYY-MM-DD"),manifest:input("Komplet eksplicit V1-manifest JSON uden confirm/actor/nøgle"),mapping:input("Komplet kildebelagt reciprocal mapping JSON uden confirm/actor/nøgle"),data:None,review:None,confirmed:false,pending:None,resolved:false,busy:false,status:"Angiv eksplicit pr-dato og hent aktuelt manifest, mappings og serverens kildeafstemning. Ingen dato, konti, ejerforhold eller beløb vælges automatisk.".into()}
    }
    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.review = None;
        self.confirmed = false;
        self.resolved = false;
        for input in [&self.manifest, &self.mapping] {
            input.update(cx, |i, cx| i.set_value("", window, cx));
        }
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.selection.as_of.trim().is_empty() {
            self.status = "Angiv en eksplicit afstemningsdato før læsning".into();
            cx.notify();
            return;
        }
        self.busy = true;
        self.data = None;
        self.clear(window, cx);
        self.status="Henter aktuelle synlige manifester/mappings, eksakte periodetal/kildebeviser og adgangskontekst…".into();
        let (session, company, selection) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((read(&session, &company, &selection)?, state::load()?))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok((data,pending))=>{this.data=Some(data);this.pending=pending;this.status="Viser kun serverens aktuelt synlige konfiguration og datoafgrænsede kildebeviser. Manifest, forslag, uafhængig godkendelse og tilbagekaldelse er separate bekræftede handlinger. Ingen selskabsledger ændres her.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn prepare(&mut self, action: Action, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        let raw = if action == Action::Manifest {
            self.manifest.read(cx).value().to_string()
        } else if action == Action::Propose {
            self.mapping.read(cx).value().to_string()
        } else {
            String::new()
        };
        match Review::prepare(data, action, &raw) {
            Ok(review) => {
                self.review = Some(review);
                self.confirmed = false;
                self.status="Gennemgå præcis frossen kildekonfiguration, manifest/mappinghash og aktuel serverafstemning. Intet sendt.".into();
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
        self.status="Genlæser bruger, roller, ledger, konfiguration og samme datoperiodes kildebeviser før én afsendelse…".into();
        let (session, company, selection, origin, local) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
            self.origin.clone(),
            self.local.clone(),
        );
        let task=cx.background_executor().spawn(async move{
            let fresh=read(&session,&company,&selection)?;if !review.current(&fresh){return Err("Bruger, rettighedskilder, ledger, manifest/mapping eller aktuelle kildebeviser ændrede sig. Intet sendt; genlæs og gennemgå igen.".into());}
            let marker=review.marker(&origin,local.as_deref());state::begin(&marker)?;
            let response=session.runtime.block_on(session.client.mutate_intercompany(&company,review.action.sdk(),review.body.as_ref().clone(),true));
            let(message,receipt)=match response{
                Ok(value)=>if review.receipt(&value){state::finish(&marker,false)?;(match review.action{Action::Manifest=>"Serveren bekræftede V1-manifestets præcise hash. Dette er eksplicit konfiguration; ingen ejerforhold udledes eller bogføres.".to_string(),Action::Propose=>"Serveren bekræftede mappingforslaget og dets kanoniske hash. En ANDEN autentificeret WorkspaceOwner med ejerret til begge endpoints skal godkende det separat.".into(),Action::Approve=>"Serveren bekræftede godkendelsen af præcis mapping/hash. Den aktive pr-dato-afstemning med eksakte kildebeløb genlæses; et godkendt forslag er ikke i sig selv en påstand om afstemte beløb.".into(),Action::Revoke=>"Serveren bekræftede tilbagekaldelsen af præcis mapping/hash. Afstemningsprojektionen genlæses; selskabsledger og tidligere kildebeviser er uændrede.".into()},Some(value))}else{("Kvitteringen er ufuldstændig. Udfaldet er uafklaret; genlæs konfiguration, mapping og datoperiodens kildebeviser manuelt. Ingen genafsendelse.".into(),None)},
                Err(Error::Refused{status})=>{state::finish(&marker,false)?;(format!("Serveren afviste handlingen (HTTP{status}). Aktuel WorkspaceOwner og CompanyOwner til alle relevante virksomheder kræves; egen mapping må ikke godkendes. Kontrollér eksplicit V1-kilde, medlems-/datointervaller, konti, positioner, evidens, eksisterende overlap og kanonisk mappingHash. Serveren kontrollerer alle regler; ingen nøgler eller gentagelse tilføjes."),None)},
                Err(Error::InvalidInput(_))=>{state::finish(&marker,false)?;("Typed kilde-JSON/bekræftelse blev afvist før netværkskald. Kontrollér de præcise SDK-felter og256KiB-grænsen; ukendte actor-/nøglefelter understøttes ikke.".into(),None)},
                _=>("Udfaldet er uafklaret. Genlæs aktuelt manifest/mappings og datoafstemning manuelt; en lignende projektion beviser ikke dette forsøgs udfald. Ingen genafsendelse eller automatisk adoption.".into(),None)
            };
            let mut selection=selection;if let Some(receipt)=&receipt{if review.action!=Action::Manifest{selection.id=receipt["mappingId"].as_str().map(str::to_owned);}}
            let next=read(&session,&company,&selection);let mut message=message;if let(Some(receipt),Ok(data))=(&receipt,&next){message.push_str(if review.readback(receipt,data){" Præcis kanonisk status/hash er genlæst sammen med den aktuelle synlige konfiguration og serverens datoperiode/kildebeviser."}else{" Genlæsningen matcher ikke kvitteringens status/hash. Handlingen er bekræftet; inspicér aktuelle hændelser før videre arbejde."});}
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
impl Render for Intercompany {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locked = self.busy || self.review.is_some();
        let mut view=div().flex().flex_col().gap_3().child(div().text_xl().child("Mellemregninger · eksplicit V1-konfiguration og eksakt kildeafstemning"))
            .child(format!("Aktuel ankervirksomhed {} · {}",self.company,self.origin)).child(self.status.clone())
            .child("V1-manifestet er workspace-globalt. Mappings refererer eksplicit til to selskabers faktiske konti og kildebeviser. Kun serverens godkendte aktive mappings afstemmes på den valgte dato; der beregnes ingen beløb, valutaomregning eller konsolidering lokalt.")
            .child(Input::new(&self.as_of).disabled(locked))
            .child(Button::new("intercompany-load-date").label("Hent konfiguration og afstemning for eksplicit pr-dato").disabled(locked).on_click(cx.listener(|this,_,window,cx|{this.selection=Selection{as_of:this.as_of.read(cx).value().to_string(),id:None};this.load(window,cx);})))
            .child(Button::new("intercompany-reload").label("Genlæs aktiv dato/mapping; ryd formular og review").disabled(locked||self.selection.as_of.is_empty()).on_click(cx.listener(|this,_,window,cx|this.load(window,cx))))
            .child(Button::new("intercompany-clear-selected").label("Ryd mappingvalg og genlæs aktiv dato").disabled(locked||self.selection.as_of.is_empty()).on_click(cx.listener(|this,_,window,cx|{this.selection.id=None;this.load(window,cx);})));
        let Some(data) = self.data.clone() else {
            return view;
        };
        view=view.child(format!("Aktiv server-pr-dato {} · bruger {} · workspacerolle {}",data.report["asOf"].as_str().unwrap_or("?"),data.principal,data.workspace_role))
            .child("Ændringer i datofeltet træder først i kraft med Hent-knappen. Den viste rapportdato og dens kildebeviser fryses i reviewet.")
            .child(div().text_lg().child("Aktuelt synligt V1-manifest, manifestHash og mappings"));
        for line in state::lines(&data.current, "konfiguration") {
            view = view.child(line);
        }
        if data.current["manifest"].is_null() {
            view=view.child("Manifest er enten ikke konfigureret eller ikke fuldt synligt for dig. Årsagen eller skjulte referencer udledes ikke. Ny konfiguration skal angives eksplicit, og serveren kræver adgang til både tidligere og nye referencer.");
        }
        for mapping in data.current["mappings"].as_array().into_iter().flatten() {
            if let Some(id) = mapping["mapping"]["id"].as_str() {
                let id = id.to_owned();
                view = view.child(
                    Button::new(SharedString::from(format!("intercompany-mapping-{id}")))
                        .label(format!(
                            "Gennemgå {} · {} · hash {} · actor {}",
                            id,
                            mapping["status"].as_str().unwrap_or("?"),
                            mapping["mappingHash"].as_str().unwrap_or("?"),
                            mapping["actor"].as_str().unwrap_or("?")
                        ))
                        .disabled(locked)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.selection.id = Some(id.clone());
                            this.load(window, cx);
                        })),
                );
            }
        }
        view=view.child(div().text_lg().child("Præcis serverafstemning · status, blockers, decimalstrenge og datoperiodens originale kildebeviser"));
        for line in state::lines(&data.report, "afstemning") {
            view = view.child(line);
        }
        view=view.child("balance, difference og sourceRefs.amount vises som serverens uændrede decimalstrenge. currency, selectionHash, ledgerHeadHash, entryCount og kildeposteringer er serverens beviser. Matched/mismatch/not-comparable og blockers ændres eller omregnes ikke lokalt. consolidationAvailable er false; ingen konsolideringsrapport fremstilles her.");
        if let Some(selected) = &data.selected {
            view=view.child(div().text_lg().child("Valgt kanonisk mapping · faktisk kilde, konti, intervaller, positioner og exact hash"));
            for line in state::lines(selected, "valgtMapping") {
                view = view.child(line);
            }
            if data.self_approval() && selected["status"] == "proposed" {
                view=view.child("Du er forslagets autentificerede actor. Egen godkendelse er afvist: en ANDEN autentificeret WorkspaceOwner med aktuel CompanyOwner-ret til begge endpoints skal gennemgå samme mapping/hash. Ingen session ændres automatisk.");
            }
        }
        if let Some(record) = self.pending.clone() {
            view=view.child(div().text_lg().child("Uafklaret forsøg · ingen genafsendelse"))
                .child(format!("Handling {} · mapping {} · mappingHash {} · oprindelig manifestHash {} · endpoint {}",record["operation"],record["mappingId"],record["mappingHash"],record["manifestHash"],record["origin"]))
                .child("Genlæs aktuelt manifest/mapping og kildeafstemning først. Et manifest-/forslags nye hash kan være ukendt; matchende projektion alene beviser ikke præcis dette forsøg. Den private status gemmer kun scope/identiteter/hash, aldrig kontolister, beløb, datointervaller eller evidenstekst.");
            if let Some(id) = record["mappingId"].as_str() {
                let id = id.to_owned();
                view = view.child(
                    Button::new("intercompany-inspect-pending")
                        .label("Genlæs uafklaret mappings aktuelle kilde/status")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.selection.id = Some(id.clone());
                            this.load(window, cx);
                        })),
                )
            }
            if state::scope(&record, &self.origin, &data, self.local.as_deref()) {
                view=view.child("Profil, bruger, anker og ledger matcher. Lokalt endpoint kan ændres efter genstart; arkivering sender intet.")
                    .child(Checkbox::new("intercompany-resolved").label("Jeg har manuelt afklaret dette præcise forsøg i den viste profil/virksomhed").checked(self.resolved).disabled(self.busy).on_click(cx.listener(|this,value,_,cx|{this.resolved=*value;cx.notify();})))
                    .child(Button::new("intercompany-archive-resolution").label("Arkivér manuel afklaring uden genafsendelse").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};if this.busy||!this.resolved||!state::scope(record,&this.origin,data,this.local.as_deref()){return;}match state::finish(record,true){Ok(())=>{this.pending=None;this.resolved=false;this.status="Manuel afklaring arkiveret. Intet blev gentaget.".into();},Err(e)=>this.status=e}cx.notify();})));
            } else {
                view=view.child("Vælg oprindelig bruger, virksomhed og privat lokal profil/eksternt endpoint for manuel afklaring.");
            }
            return view;
        }
        if self.selection.id.is_some() && data.selected.is_none() {
            return view.child("Valgt mapping er ukendt eller ikke aktuelt synlig. Der tilbydes ingen handling for denne kilde. Ryd mappingvalget for at fortsætte; skjult årsag/identitet udledes ikke.");
        }
        if let Some(review) = &self.review {
            view = view.child(div().text_lg().child(format!(
                "Gennemgå {} · anker {} · server-pr-dato {}",
                review.action.name(),
                review.snapshot.company,
                review.snapshot.report["asOf"].as_str().unwrap_or("?")
            )));
            if let Some(selected) = &review.snapshot.selected {
                for line in state::lines(selected, "frossenMappingkilde") {
                    view = view.child(line);
                }
            }
            for line in state::lines(&review.display, "præcisAnmodning") {
                view = view.child(line);
            }
            return view.child("Manifest/mapping/hash, synlige referencer, aktuel adgang og den viste datoperiodes kildebeviser er frosset. Serveren validerer V1-/konto-/interval-/overlap-regler og genkontrollerer ALLE rettigheder. Forslagets periodebeløb bliver først synlige i rapporten efter faktisk godkendelse, når mappingen er aktiv på datoen.")
                .child(Checkbox::new("intercompany-confirmed").label("Jeg har gennemgået præcis konfiguration, endpoints, kildebeviser, hash, aktuel pr-dato og denne ene handling").checked(self.confirmed).disabled(self.busy).on_click(cx.listener(|this,value,_,cx|{this.confirmed=*value;cx.notify();})))
                .child(Button::new("intercompany-send").label("Bekræft og send gennemgået handling én gang").disabled(self.busy||!self.confirmed).on_click(cx.listener(|this,_,window,cx|this.send(window,cx))))
                .child(Button::new("intercompany-cancel").label("Annullér før afsendelse").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.confirmed=false;cx.notify();})));
        }
        if let Some(selected) = &data.selected {
            if selected["status"] == "proposed" {
                view = view.child(
                    Button::new("intercompany-review-approve")
                        .label("Gennemgå separat uafhængig godkendelse af præcis mappingHash")
                        .disabled(self.busy || data.self_approval())
                        .on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Approve, cx))),
                )
            }
            if selected["status"] == "approved" {
                view = view.child(
                    Button::new("intercompany-review-revoke")
                        .label("Gennemgå separat tilbagekaldelse af præcis mappingHash")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Revoke, cx))),
                )
            }
        }
        view=view.child(div().text_lg().child("Eksplicit workspace-globalt V1-manifest"))
            .child("Komplet JSON: version:1 og groups med id,name,memberships [{id,companySlug,validFrom,validToExclusive?}] og ownership [{id,parentCompanySlug,childCompanySlug,basisPoints,evidenceRefs,validFrom,validToExclusive?}]. Angiv kun kildebelagte eksisterende virksomheder/forhold; tidligere og nye referencer kræver aktuelle ejerrettigheder. Ingen felter eller ejerforhold udledes. Udelad confirm/actor/nøgler; særskilt review bekræfter den præcise anmodning.")
            .child(Input::new(&self.manifest).disabled(self.busy))
            .child(Button::new("intercompany-review-manifest").label("Gennemgå eksplicit V1-manifest før bekræftet ændring").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.prepare(Action::Manifest,cx))))
            .child(div().text_lg().child("Foreslå konkret reciprocal mapping · ingen postering"))
            .child("Komplet JSON: id,groupId,leftCompanySlug,rightCompanySlug,leftAccountNos,rightAccountNos,leftPosition,rightPosition,evidenceRefs,validFrom og valgfri validToExclusive. Positioner er receivable/payable efter faktisk kilde. Brug aktuelle konti i de to faktiske ledgers; ingen konto eller beløb vælges/beregnes. Udelad confirm/actor/nøgler. Godkendelse kræver en anden autentificeret reviewer og giver ingen automatisk bogføring eller konsolidering.")
            .child(Input::new(&self.mapping).disabled(self.busy));
        view.child(
            Button::new("intercompany-review-propose")
                .label("Gennemgå konkret mappingforslag før separat bekræftelse")
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Propose, cx))),
        )
    }
}
