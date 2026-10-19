//! Native reviewed source profiles and read-only consolidated reports; server owns every figure.
use crate::{Session, consolidation_state as state, readable};
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
    if !state::date(&selection.from) || !state::date(&selection.as_of) {
        return Err("Angiv eksplicit fra-dato og pr-dato som YYYY-MM-DD. Serveren validerer faktiske datoer og interval.".into());
    }
    let current = session
        .runtime
        .block_on(session.client.consolidation(company, &selection.as_of))
        .map_err(readable)?;
    let available = session
        .runtime
        .block_on(session.client.group_report_profiles(&selection.as_of))
        .map_err(readable)?;
    let selected = selection.id.as_ref().and_then(|id| {
        current["profiles"]
            .as_array()
            .and_then(|a| a.iter().find(|s| s["profile"]["id"] == *id))
    });
    let (report, report_error) = if let (Some(id), Some(selected)) = (&selection.id, selected) {
        if selected["status"] == "approved" {
            match session
                .runtime
                .block_on(session.client.group_consolidated_report(
                    id,
                    &selection.from,
                    &selection.as_of,
                )) {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(readable(e))),
            }
        } else {
            (None, None)
        }
    } else {
        (None, None)
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
        company,
        selection,
        current,
        available,
        report,
        report_error,
        me,
        journal,
    )
}
pub struct Consolidation {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    selection: Selection,
    from: Entity<InputState>,
    as_of: Entity<InputState>,
    profile: Entity<InputState>,
    data: Option<Snapshot>,
    review: Option<Review>,
    confirmed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
}
impl Consolidation {
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
        Self{session,origin,company,local,selection:Selection::default(),from:input("Eksplicit fra-dato YYYY-MM-DD"),as_of:input("Eksplicit pr-dato YYYY-MM-DD"),profile:input("Komplet typed V1-profil JSON uden confirm/actor/nøgle"),data:None,review:None,confirmed:false,pending:None,resolved:false,busy:false,status:"Angiv begge rapportdatoer og hent aktuelle profiler. Der vælges ingen dato, rapportprofil, konto eller regnskabsfigur automatisk.".into()}
    }
    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.review = None;
        self.confirmed = false;
        self.resolved = false;
        self.profile.update(cx, |i, cx| i.set_value("", window, cx));
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.data = None;
        self.clear(window, cx);
        self.status="Henter kanoniske profiler, aktiv metadata, valgt rapport og aktuelle rettighedskilder…".into();
        let (session, company, selection) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((read(&session, &company, &selection)?, state::load()?))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok((data,pending))=>{this.data=Some(data);this.pending=pending;this.status="Viser serverens aktuelle profiler og synlige kilder. Forslag, uafhængig godkendelse og tilbagekaldelse er adskilte bekræftede handlinger; rapportlæsning bogfører intet.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn prepare(&mut self, action: Action, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        match Review::prepare(data, action, &self.profile.read(cx).value()) {
            Ok(review) => {
                self.review = Some(review);
                self.confirmed = false;
                self.status="Gennemgå præcis frossen profil/anmodning, hashes, datoer og kildebeviser. Intet sendt.".into();
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
        self.status="Genlæser præcis bruger, rettigheder, ledger, profiler og rapportkilder før én afsendelse…".into();
        let (session, company, selection, origin, local) = (
            self.session.clone(),
            self.company.clone(),
            self.selection.clone(),
            self.origin.clone(),
            self.local.clone(),
        );
        let task=cx.background_executor().spawn(async move{
   let fresh=read(&session,&company,&selection)?;if !review.current(&fresh){return Err("Bruger, rettighedskilder, ledger, profiler, rapportdatoer eller kildebeviser ændrede sig. Intet sendt; genlæs og gennemgå igen.".into());}
   let marker=review.marker(&origin,local.as_deref());state::begin(&marker)?;
   let response=session.runtime.block_on(session.client.mutate_consolidation(&company,review.action.sdk(),review.body.as_ref().clone(),true));
   let(message,receipt)=match response{
    Ok(value)=>if review.receipt(&value){state::finish(&marker,false)?;(match review.action{Action::Propose=>"Serveren bekræftede profilforslaget og dets kanoniske hash. En ANDEN autentificeret reviewer med aktuelle alle-virksomhedsrettigheder skal godkende samme profil/hash separat.".to_string(),Action::Approve=>"Serveren bekræftede godkendelsen af præcis profilHash. Rapportens faktiske ready/blocked-status, decimalstrenge og kildebeviser genlæses; godkendelse lover ikke en klar rapport.".into(),Action::Revoke=>"Serveren bekræftede tilbagekaldelsen. Profilen er ikke længere en godkendt rapportkilde; ingen postering eller tidligere kildesnapshot ændres.".into()},Some(value))}else{("Kvitteringen er ufuldstændig. Udfaldet er uafklaret; genlæs profilernes aktuelle kanoniske tilstand manuelt. Ingen genafsendelse.".into(),None)},
    Err(Error::Refused{status})=>{state::finish(&marker,false)?;(format!("Serveren afviste handlingen (HTTP{status}). Kontrollér aktuelle WorkspaceOwner/CompanyAdmin-rettigheder til ALLE konfigurerede virksomheder, en ANDEN reviewer, præcis profileHash, kildegruppe, datoer, valuta, rapportlinjer og kontodækning. SDK leverer kun afvisningsstatus; genlæs serverens tilgængelige profiler og rapportblockers. Intet gentages."),None)},
    Err(Error::InvalidInput(_))=>{state::finish(&marker,false)?;("Typed native JSON/bekræftelse blev afvist før netværkskald. Kontrollér felter og nativegrænser: forslag1MiB, godkendelse/tilbagekaldelse32KiB. Ingen actor/nøgle/defaults indsættes.".into(),None)},
    _=>("Udfaldet er uafklaret. Genlæs profilens aktuelle tilstand manuelt; en lignende projektion beviser ikke dette forsøg. Ingen genafsendelse eller automatisk adoption.".into(),None)
   };
   let mut selection=selection;if let Some(receipt)=&receipt{selection.id=receipt["profileId"].as_str().map(str::to_owned);}
   let next=read(&session,&company,&selection);let mut message=message;if let(Some(receipt),Ok(data))=(&receipt,&next){message.push_str(if review.readback(receipt,data){" Præcis kanonisk status/hash er genlæst; rapporten og metadata vises kun som serveren returnerer dem."}else{" Kanonisk genlæsning matcher ikke kvitteringens status/hash. Handlingen er bekræftet; inspicér kilderne før videre arbejde."});}
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
impl Render for Consolidation {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locked = self.busy || self.review.is_some();
        let mut view=div().flex().flex_col().gap_3().child(div().text_xl().child("Konsolidering · gennemgåede rapportprofiler og kanoniske kildetal"))
   .child(format!("Ankervirksomhed {} · {}",self.company,self.origin)).child(self.status.clone())
   .child("Rapportlaget læser særskilte selskabsledgers med eksplicitte rapportlinjer og serverkontrollerede elimineringer. Her beregnes ingen beløb, ejerandele, valuta eller deadlines, og der bogføres intet.")
   .child(Input::new(&self.from).disabled(locked)).child(Input::new(&self.as_of).disabled(locked))
   .child(Button::new("consolidation-load-dates").label("Hent profiler for eksplicitte datoer").disabled(locked).on_click(cx.listener(|this,_,window,cx|{this.selection=Selection{from:this.from.read(cx).value().to_string(),as_of:this.as_of.read(cx).value().to_string(),id:None};this.load(window,cx);})))
   .child(Button::new("consolidation-reload").label("Genlæs aktiv profil/datoer; ryd formular og review").disabled(locked||self.selection.as_of.is_empty()).on_click(cx.listener(|this,_,window,cx|this.load(window,cx))))
   .child(Button::new("consolidation-clear-selection").label("Ryd profilvalg og genlæs aktive datoer").disabled(locked||self.selection.as_of.is_empty()).on_click(cx.listener(|this,_,window,cx|{this.selection.id=None;this.load(window,cx);})));
        let Some(data) = self.data.clone() else {
            return view;
        };
        view=view.child(format!("Aktiv periode {} til {} · bruger {} · workspacerolle {}",data.from,data.as_of,data.principal,data.workspace_role))
   .child("Datofelter anvendes først med Hent-knappen. Aktiv periode, profilhash, kanoniske kilder og adgangskontekst fryses ved review; serveren validerer faktisk interval og alle konfigurerede virksomheders rettigheder.")
   .child(div().text_lg().child("Aktuelle profiler · immutable kilde, actor, status, profileHash og aktiv metadata"));
        for line in state::lines(&data.current, "aktuelProfil") {
            view = view.child(line);
        }
        for line in state::lines(&data.available, "tilgængeligeRapportprofiler") {
            view = view.child(line);
        }
        for selected in data.current["profiles"].as_array().into_iter().flatten() {
            if let Some(id) = selected["profile"]["id"].as_str() {
                let id = id.to_owned();
                view = view.child(
                    Button::new(SharedString::from(format!("consolidation-select-{id}")))
                        .label(format!(
                            "Vælg {} · {} · hash {} · actor {}",
                            id,
                            selected["status"].as_str().unwrap_or("?"),
                            selected["profileHash"].as_str().unwrap_or("?"),
                            selected["actor"].as_str().unwrap_or("?")
                        ))
                        .disabled(locked)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.selection.id = Some(id.clone());
                            this.load(window, cx);
                        })),
                );
            }
        }
        if let Some(selected) = &data.selected {
            view=view.child(div().text_lg().child("Valgt kanonisk profil · fulde linjer, current-result-rolle, konti, gyldighed og kildeevidens"));
            for line in state::lines(selected, "valgtProfil") {
                view = view.child(line);
            }
        }
        view=view.child(div().text_lg().child("Serverens rapport · rå selskabssummer, elimineringer, current result og endelige figurer"));
        if let Some(report) = &data.report {
            view = view.child(format!(
                "Faktisk serverstatus: {} · periode {} → {}",
                report["status"], report["period"]["from"], report["period"]["to"]
            ));
            for line in state::lines(report, "rapport") {
                view = view.child(line);
            }
            view=view.child("Alle rå summer, justeringer og endelige figurer er serverens DECIMALSTRENGE. SourceSnapshots, ledgerHeadHash, entryCount, sourceHashes, blockers/redaktion og v2-elimineringskilder vises uændret. Null/blokering er ikke nul eller ready; ingen lokale totaler/valutaomregning tilføjes.");
        } else if let Some(e) = &data.report_error {
            view=view.child(format!("Rapporten kunne ikke læses: {e}. Der vises ingen erstatningssummer, nulværdier eller ready-status. Genlæs aktuelle rettigheder, kilder og profil."));
        } else {
            view=view.child("Vælg en aktuelt godkendt synlig profil for at læse rapporten på de aktive datoer. Foreslåede/tilbagekaldte eller ukendte profiler er ikke godkendte rapportkilder.");
        }
        if data.self_approval()
            && data
                .selected
                .as_ref()
                .is_some_and(|s| s["status"] == "proposed")
        {
            view=view.child("Du er forslagets autentificerede actor. En ANDEN reviewer skal godkende samme profileHash; egen godkendelse tilbydes ikke, og session skiftes ikke automatisk.");
        }
        if let Some(record) = self.pending.clone() {
            view=view.child(div().text_lg().child("Uafklaret forsøg · manuel genlæsning, ingen genafsendelse"))
    .child(format!("Handling {} · profil {} · kendt profileHash {} · oprindeligt endpoint {}",record["operation"],record["profileId"],record["profileHash"],record["origin"]))
    .child("Genlæs den faktiske profil/kildestatus. Forslagets nye hash kan være ukendt; en matchende profil alene beviser ikke dette forsøgs udfald. Privat markør gemmer kun scope/ID/hash, ingen profil, kontolister, beløb, datoer eller kildeevidens.");
            if let Some(id) = record["profileId"].as_str() {
                let id = id.to_owned();
                view = view.child(
                    Button::new("consolidation-inspect-pending")
                        .label("Genlæs uafklaret profils kanoniske tilstand")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.selection.id = Some(id.clone());
                            this.load(window, cx);
                        })),
                );
            }
            if state::scope(&record, &self.origin, &data, self.local.as_deref()) {
                view=view.child("Bruger, ankervirksomhed, ledger og privat profil matcher. Lokalt endpoint må skifte efter genstart; arkivering sender intet.")
     .child(Checkbox::new("consolidation-resolved").label("Jeg har manuelt afklaret dette præcise forsøg i den viste profil/virksomhed").checked(self.resolved).disabled(self.busy).on_click(cx.listener(|this,value,_,cx|{this.resolved=*value;cx.notify();})))
     .child(Button::new("consolidation-archive-resolution").label("Arkivér manuel afklaring uden genafsendelse").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};if this.busy||!this.resolved||!state::scope(record,&this.origin,data,this.local.as_deref()){return;}match state::finish(record,true){Ok(())=>{this.pending=None;this.resolved=false;this.status="Manuel afklaring arkiveret. Intet blev gentaget.".into();},Err(e)=>this.status=e}cx.notify();})));
            } else {
                view=view.child("Vælg oprindelig bruger/virksomhed og privat lokal profil eller nøjagtigt eksternt endpoint for manuel afklaring.");
            }
            return view;
        }
        if self.selection.id.is_some() && data.selected.is_none() {
            return view.child("Profilen er ukendt eller ikke aktuelt synlig. Ingen handling tilbydes; skjult årsag udledes ikke. Ryd valget og genlæs.");
        }
        if let Some(review) = &self.review {
            view = view.child(div().text_lg().child(format!(
                "Gennemgå {} · periode {} til {} · anker {}",
                review.action.name(),
                review.snapshot.from,
                review.snapshot.as_of,
                review.snapshot.company
            )));
            if let Some(selected) = &review.snapshot.selected {
                for line in state::lines(selected, "frossenKildeprofil") {
                    view = view.child(line);
                }
            }
            for line in state::lines(&review.display, "præcisAnmodning") {
                view = view.child(line);
            }
            return view.child("Præcis profil/hash/anmodning og aktuelle kilde-/rettighedsprojektioner fastholdes. Serveren kontrollerer hele V1-profilen, kilder og alle rettigheder; godkendelse kræver anden autentificeret reviewer. Rapportberegning og v2-elimineringer er udelukkende serverens læselag.")
    .child(Checkbox::new("consolidation-confirmed").label("Jeg har gennemgået præcis profil/anmodning, sourceHash, rapportdatoer og denne ene handling").checked(self.confirmed).disabled(self.busy).on_click(cx.listener(|this,value,_,cx|{this.confirmed=*value;cx.notify();})))
    .child(Button::new("consolidation-send").label("Bekræft og send gennemgået handling én gang").disabled(self.busy||!self.confirmed).on_click(cx.listener(|this,_,window,cx|this.send(window,cx))))
    .child(Button::new("consolidation-cancel").label("Annullér før afsendelse").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.confirmed=false;cx.notify();})));
        }
        if let Some(selected) = &data.selected {
            if selected["status"] == "proposed" {
                view = view.child(
                    Button::new("consolidation-review-approve")
                        .label("Gennemgå separat uafhængig godkendelse af præcis profileHash")
                        .disabled(self.busy || data.self_approval())
                        .on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Approve, cx))),
                );
            }
            if selected["status"] == "approved" {
                view = view.child(
                    Button::new("consolidation-review-revoke")
                        .label("Gennemgå separat tilbagekaldelse af valgt godkendt profil")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Revoke, cx))),
                );
            }
        }
        view.child(div().text_lg().child("Foreslå komplet typed rapportprofil · eksplicitte kilder"))
   .child("Angiv selve V1-profilens JSON: version:1,id,groupId,currency,validFrom,validToExclusive?,reportingLines [{id,label,section,displayOrder,role?}],accountMappings [{id,companySlug,accountNo,reportingLineId,validFrom,validToExclusive?}],evidenceRefs. Sections: asset/liability/equity/income/expense. Rollen current-result er en eksplicit equity-linje. Ingen kilder eller defaults udledes. Serveren kræver fuld kontodækning og maksimalt524288bytes kanonisk profil; nativeanmodningen er højst1MiB. Udelad confirm/actor/nøgle; reviewet indpakker kun den viste profil med confirm:true.")
   .child(Input::new(&self.profile).disabled(self.busy))
   .child(Button::new("consolidation-review-propose").label("Gennemgå komplet profil før separat bekræftet forslag").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.prepare(Action::Propose,cx))))
    }
}
