//! Native closed-history intake; server review, originals and receipts remain authoritative.
use crate::{
    Session,
    archive_import_attempt::{self as attempts, Attempt},
    archive_import_state as state, readable,
};
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
use std::{io::Read, os::unix::fs::MetadataExt, path::Path, sync::Arc};

struct Row {
    id: usize,
    year: Entity<InputState>,
    kind: Option<state::FileKind>,
    file: Option<(String, Vec<u8>)>,
}
struct Readback {
    scope: state::Scope,
    archives: Value,
}
fn read(
    session: &Session,
    origin: &str,
    company: &str,
    local: Option<&str>,
) -> Result<Readback, String> {
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let journal = session
        .runtime
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    let scope = state::Scope::parse(origin, company, local, &me, &journal)?;
    let archives = session
        .runtime
        .block_on(session.client.archives(company, None))
        .map_err(readable)?;
    if archives["ok"] != true
        || !archives["years"].is_array()
        || archives["errors"].as_array().is_none_or(|e| !e.is_empty())
    {
        return Err("Arkivlisten kunne ikke verificeres. Intet importkald er sendt.".into());
    }
    Ok(Readback { scope, archives })
}
fn file(path: &Path) -> Result<(String, Vec<u8>), String> {
    let fail = || {
        "Vælg en almindelig CSV-fil på højst 16 MiB. Filen må ikke ændres under læsningen."
            .to_owned()
    };
    if !path
        .extension()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v.eq_ignore_ascii_case("csv"))
    {
        return Err("Vælg en original fil med filtypen .csv".into());
    }
    let before = std::fs::symlink_metadata(path).map_err(|_| fail())?;
    if !before.is_file() || before.file_type().is_symlink() || before.len() > state::MAX_FILE as u64
    {
        return Err(fail());
    }
    let source = std::fs::File::open(path).map_err(|_| fail())?;
    let opened = source.metadata().map_err(|_| fail())?;
    if (
        before.dev(),
        before.ino(),
        before.len(),
        before.modified().ok(),
    ) != (
        opened.dev(),
        opened.ino(),
        opened.len(),
        opened.modified().ok(),
    ) {
        return Err(fail());
    }
    let mut bytes = vec![];
    source
        .take(state::MAX_FILE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| fail())?;
    let after = std::fs::symlink_metadata(path).map_err(|_| fail())?;
    if after.file_type().is_symlink()
        || bytes.len() > state::MAX_FILE
        || (
            before.dev(),
            before.ino(),
            before.len(),
            before.modified().ok(),
        ) != (after.dev(), after.ino(), after.len(), after.modified().ok())
    {
        return Err(fail());
    }
    Ok((
        path.file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(fail)?
            .to_owned(),
        bytes,
    ))
}
fn key() -> Result<String, String> {
    let mut bytes = [0; 24];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|_| "Forsøgsnøglen kunne ikke oprettes; intet sendt".to_owned())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn message(v: &Value) -> String {
    v["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(" · ")
}
fn definite(error: &Error) -> bool {
    matches!(
        error,
        Error::InvalidInput(_)
            | Error::Refused {
                status: 400 | 401 | 403 | 404 | 422
            }
    )
}

pub struct ArchiveImport {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    entry_focus: FocusHandle,
    entry_focus_requested: bool,
    currency: Entity<InputState>,
    rows: Vec<Row>,
    next_row: usize,
    data: Option<Readback>,
    review: Option<state::Review>,
    pending: Option<Attempt>,
    blocked: bool,
    rebind_confirmed: bool,
    reviewed: bool,
    reconciled: bool,
    receipt: Option<Value>,
    busy: bool,
    status: String,
}
impl ArchiveImport {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        company: String,
        local: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let currency = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Historisk basisvaluta · f.eks. DKK")
        });
        let mut this = Self {
            session,
            origin,
            company,
            local,
            entry_focus: cx.focus_handle(),
            entry_focus_requested: false,
            currency,
            rows: vec![],
            next_row: 1,
            data: None,
            review: None,
            pending: None,
            blocked: false,
            rebind_confirmed: false,
            reviewed: false,
            reconciled: false,
            receipt: None,
            busy: false,
            status: String::new(),
        };
        this.add_row(window, cx);
        this.add_row(window, cx);
        this.load(cx);
        this
    }
    pub fn focus_entry(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.entry_focus_requested = true;
        cx.notify();
    }
    fn add_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.review.is_some() || self.rows.len() >= 32 {
            return;
        }
        let id = self.next_row;
        self.next_row += 1;
        self.rows.push(Row {
            id,
            year: cx.new(|cx| InputState::new(window, cx).placeholder("Regnskabsår · ÅÅÅÅ")),
            kind: None,
            file: None,
        });
        cx.notify();
    }
    fn load(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.reviewed = false;
        self.rebind_confirmed = false;
        self.reconciled = false;
        self.status = "Henter bruger, virksomhedsidentitet og bevarede arkivår…".into();
        let session = self.session.clone();
        let (origin, company, local) = (
            self.origin.clone(),
            self.company.clone(),
            self.local.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((
                read(&session, &origin, &company, local.as_deref())?,
                Attempt::load(&attempts::directory())?,
            ))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok((data,pending))=>{this.data=Some(data);this.pending=pending;this.blocked=false;this.status=if this.pending.is_some(){"Et privat arkivforsøg er uafklaret. Vælg præcis de oprindelige CSV-filer, år og valuta for at genskabe det; intet sendes ved indlæsning."}else{"Vælg originale Posteringer- og eventuelle SaldoBalance-filer. Medtag også højeste år som kontrol; det udelades af importen."}.into();},Err(e)=>{this.data=None;this.blocked=true;this.status=e}}cx.notify();});}).detach();
        cx.notify();
    }
    fn choose(&mut self, id: usize, cx: &mut Context<Self>) {
        if self.busy || self.review.is_some() {
            return;
        }
        let choice = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Vælg den originale CSV-fil; bytes bevares uændret".into()),
        });
        self.busy = true;
        cx.spawn(async move|view,cx|{let path=match choice.await{Ok(Ok(Some(paths)))=>paths.first().cloned(),Ok(Ok(None))=>None,_=>{let _=view.update(cx,|this,cx|{this.busy=false;this.status="Filvælgeren kunne ikke åbnes".into();cx.notify();});return;}};let result=if let Some(path)=path{Some(cx.background_executor().spawn(async move{file(&path)}).await)}else{None};let _=view.update(cx,|this,cx|{this.busy=false;match result{Some(Ok(source))=>{let total=this.rows.iter().filter(|r|r.id!=id).filter_map(|r|r.file.as_ref()).map(|(_,bytes)|bytes.len()).sum::<usize>();if total+source.1.len()>state::MAX_TOTAL {this.status="De valgte CSV-filer må samlet højst være 18 MiB. Den tidligere fil er bevaret.".into();}else{if let Some(row)=this.rows.iter_mut().find(|r|r.id==id){row.file=Some(source);}this.receipt=None;this.status="Originalens bytes er fastholdt i hukommelsen. Vælg filens år og type eksplicit.".into();}},Some(Err(e))=>this.status=e,None=>{}}cx.notify();});}).detach();
        cx.notify();
    }
    fn package(&self, cx: &Context<Self>) -> Result<state::Package, String> {
        let mut files = vec![];
        for row in &self.rows {
            let year_text = row.year.read(cx).value().to_string();
            if year_text.len() != 4 || !year_text.bytes().all(|b| b.is_ascii_digit()) {
                return Err("Angiv præcis fire cifre for hver fils regnskabsår".into());
            }
            let year = year_text
                .parse::<u16>()
                .map_err(|_| "Regnskabsåret kunne ikke læses".to_owned())?;
            let kind = row
                .kind
                .ok_or("Vælg Posteringer eller SaldoBalance for hver fil")?;
            let (_, bytes) = row
                .file
                .as_ref()
                .ok_or("Vælg en original CSV-fil for hver række")?;
            files.push(state::SourceFile {
                year,
                kind,
                bytes: bytes.clone(),
            });
        }
        state::Package::build(&self.currency.read(cx).value(), files)
    }
    fn plan(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.blocked || self.review.is_some() {
            return;
        }
        let package = match self.package(cx) {
            Ok(p) => p,
            Err(e) => {
                self.status = e;
                cx.notify();
                return;
            }
        };
        self.busy = true;
        self.reviewed = false;
        self.reconciled = false;
        self.receipt = None;
        self.status = "Kontrollerer kilder og henter en skrivebeskyttet plan…".into();
        let session = self.session.clone();
        let (origin, company, local, pending) = (
            self.origin.clone(),
            self.company.clone(),
            self.local.clone(),
            self.pending.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            let data = read(&session, &origin, &company, local.as_deref())?;
            let review = if let Some(pending) = pending {
                state::Review::recover(package, data.scope.clone(), &pending)?
            } else {
                let response = session
                    .runtime
                    .block_on(session.client.archive_import(
                        &company,
                        package.plan_body(),
                        false,
                        false,
                    ))
                    .map_err(readable)?;
                state::Review::prepare(package, data.scope.clone(), response, key()?)?
            };
            Ok::<_, String>((data, review))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok((data,review))=>{this.data=Some(data);this.review=Some(review);this.status=if this.pending.is_some(){"Den oprindelige anmodning er genskabt uden ny plan eller nøgle. Kontrollér først det bevarede arkiv og kilden."}else{"Gennemgå valuta, højeste udeladte år, årstal, antal og kildehash. Planen har ikke importeret noget."}.into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn reconcile(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_none() {
            return;
        }
        let Some(review) = self.review.clone() else {
            return;
        };
        self.busy = true;
        self.reviewed = false;
        self.reconciled = false;
        let session = self.session.clone();
        let (origin, company, local) = (
            self.origin.clone(),
            self.company.clone(),
            self.local.clone(),
        );
        let task = cx.background_executor().spawn(async move {
            let data = read(&session, &origin, &company, local.as_deref())?;
            if !review.current(&data.scope) {
                return Err(
                    "Aktuel bruger, virksomhedsidentitet eller ejerrettighed afviger fra forsøget"
                        .into(),
                );
            }
            let ids = state::reconciliation_original_ids(&review, &data.archives)?;
            let mut originals = vec![];
            for id in ids {
                originals.push((
                    id,
                    session
                        .runtime
                        .block_on(session.client.archive_import_original(&company, id))
                        .map_err(readable)?,
                ));
            }
            state::verify_reconciliation(&review, &data.archives, &originals)?;
            Ok::<_, String>(data)
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok(data)=>{this.data=Some(data);this.reconciled=true;this.status="Arkivlisten og eventuelle bevarede kildebytes er kontrolleret. Det afklarer ikke kvitteringen og sender intet. Bekræft særskilt, hvis du vil hente kvitteringen eller færdiggøre præcis samme import med den oprindelige nøgle.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn rebind(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.blocked || !self.rebind_confirmed {
            return;
        }
        let Some(pending) = self.pending.clone() else {
            return;
        };
        self.busy = true;
        self.rebind_confirmed = false;
        let session = self.session.clone();
        let (origin, company, local) = (
            self.origin.clone(),
            self.company.clone(),
            self.local.clone(),
        );
        let task=cx.background_executor().spawn(async move {
            let data=read(&session,&origin,&company,local.as_deref())?;
            if !data.scope.can_apply || !pending.can_rebind(&data.scope) {return Err("Forsøget tilhører ikke samme lokale bruger og virksomhed med aktuel ejerrettighed".into());}
            let rebound=pending.rebind(&attempts::directory(),&data.scope)?;
            Ok::<_,String>((data,rebound))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok((data,pending))=>{this.data=Some(data);this.pending=Some(pending);this.review=None;this.reviewed=false;this.reconciled=false;this.status="Kun forbindelsen til samme lokale profil er bekræftet. Vælg de oprindelige filer og gennemgå det oprindelige forsøg; intet importkald er sendt.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn apply(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.blocked || !self.reviewed || self.pending.is_some() && !self.reconciled
        {
            return;
        }
        let Some(review) = self.review.clone() else {
            return;
        };
        self.busy = true;
        self.reviewed = false;
        self.status =
            "Kontrollerer aktuel ejerrettighed og bevarer forsøget privat før ét importkald…"
                .into();
        let session = self.session.clone();
        let (origin, company, local, pending) = (
            self.origin.clone(),
            self.company.clone(),
            self.local.clone(),
            self.pending.clone(),
        );
        let task=cx.background_executor().spawn(async move{
            let data=read(&session,&origin,&company,local.as_deref())?;if !review.current(&data.scope){return Err("Bruger, virksomhed eller ejerrettighed ændrede sig; intet importkald sendt".into())}
            let was_pending=pending.is_some();let attempt=match pending{Some(a)=>a,None=>Attempt::new(&review)?};attempt.save(&attempts::directory())?;
            let result=session.runtime.block_on(session.client.archive_import(&company,review.body().to_vec(),true,true));
            let mut status="Importens udfald er uafklaret. Der gentages intet automatisk; kontrollér arkiv og originalkilde før et særskilt forsøg med samme nøgle.".to_owned();let mut receipt=None;
            match result{
                Ok(value) if value["ok"]==true=>{match state::receipt(&review,&value){Ok(id)=>{let verified=(||{let fresh=read(&session,&origin,&company,local.as_deref())?;let original=session.runtime.block_on(session.client.archive_import_original(&company,id)).map_err(readable)?;let confirmed=state::verify_receipt(&review,&value,&fresh.archives,&original)?;Ok::<_,String>((fresh,confirmed))})();match verified{Ok((fresh,confirmed))=>{attempt.finish(&attempts::directory())?;status="Arkivårene og de præcise originalbytes er bekræftet mod serverens kvittering.".into();receipt=Some((fresh,confirmed));},Err(e)=>status=format!("Kvitteringen er endnu ikke bekræftet: {e}. Forsøget bevares privat.")}},Err(e)=>status=e}},
                Ok(value) if state::known_refusal(&value)=>{if !was_pending{attempt.finish(&attempts::directory())?;}status=message(&value);if status.is_empty(){status="Serveren afviste importen uden gennemførelse. Gennemgå kilden og rettighederne igen.".into();}},
                Err(e) if definite(&e)=>{if !was_pending{attempt.finish(&attempts::directory())?;}status=readable(e);},
                Ok(value)=>{let detail=message(&value);if !detail.is_empty(){status.push_str(" · ");status.push_str(&detail);}},Err(e)=>status.push_str(&format!(" · {}",readable(e))),
            }
            Ok::<_,String>((status,receipt,Attempt::load(&attempts::directory())?))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                this.reconciled = false;
                match result {
                    Ok((status, receipt, pending)) => {
                        this.status = status;
                        this.pending = pending;
                        if let Some((data, receipt)) = receipt {
                            this.data = Some(data);
                            this.receipt = Some(receipt);
                            this.review = None;
                        }
                    }
                    Err(e) => {
                        this.status = e;
                        match Attempt::load(&attempts::directory()) {
                            Ok(p) => this.pending = p,
                            Err(e) => {
                                this.blocked = true;
                                this.status.push_str(&format!(" · {e}"));
                            }
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
impl Render for ArchiveImport {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.entry_focus_requested && !self.busy {
            self.entry_focus_requested = false;
            let focus = self.entry_focus.clone();
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        }
        let mut view=div().flex().flex_col().gap_3().child(div().text_xl().child("Importér afsluttede regnskabsår")).child(format!("{} · {}",self.company,self.origin)).child(self.status.clone()).child("Arkivet er bevaret historik. Der oprettes ingen aktive posteringer, kontoplan eller åbningsbalance, og ingen valuta omregnes.").child(Button::new("archive-reload").track_focus(&self.entry_focus).label("Genlæs virksomhed og arkivår").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.load(cx))));
        if let Some(data) = &self.data {
            if !data.scope.can_apply {
                view=view.child("Du kan læse og gennemgå arkivet, men import kræver aktuel ejerrettighed til virksomheden.");
            }
            if let Some(years) = data.archives["years"].as_array() {
                for year in years {
                    view = view.child(format!(
                        "Bevaret år {} · import {} · {} posteringer · {} saldi · kilde {}",
                        year["fiscalYear"],
                        year["importId"],
                        year["postingCount"],
                        year["balanceCount"],
                        year["sourceHash"].as_str().unwrap_or("Ukendt")
                    ));
                }
            }
        }
        if let Some(pending) = &self.pending {
            view = view.child(format!(
                "Uafklaret kildehash: {} · oprindelig plan-hash: {} · anmodningshash: {}",
                pending.source_sha256, pending.plan_hash, pending.body_sha256
            ));
            if let Some(data) = &self.data {
                if !pending.matches(&data.scope) {
                    if pending.can_rebind(&data.scope) {
                        view=view.child(format!("Oprindelig lokal forbindelse: {} · aktuel: {}",pending.scope.origin,data.scope.origin))
                            .child(Checkbox::new("archive-rebind-confirmed").label("Jeg har kontrolleret samme lokale profil, bruger og virksomhed på den nye forbindelse").checked(self.rebind_confirmed).disabled(self.busy).on_click(cx.listener(|this,checked,_,cx|{this.rebind_confirmed=*checked;cx.notify();})))
                            .child(Button::new("archive-rebind").label("Bekræft den nye lokale forbindelse uden import").disabled(self.busy||self.blocked||!self.rebind_confirmed||!data.scope.can_apply).on_click(cx.listener(|this,_,_,cx|this.rebind(cx))));
                    } else {
                        view=view.child("Forsøget tilhører en anden bruger, virksomhed eller profil. Det bevares privat; gå tilbage til den oprindelige forbindelse. Der sendes ingen ny import.");
                    }
                }
            }
        }
        if let Some(receipt) = &self.receipt {
            view = view
                .child(div().text_lg().child("Arkivimport gennemført"))
                .child(format!(
                    "Import {} · kilde {}",
                    receipt["importId"],
                    receipt["sourceSha256"].as_str().unwrap_or("")
                ));
            if let Some(years) = receipt["years"].as_array() {
                for y in years {
                    view = view.child(format!(
                        "År {} · {} posteringer · {} saldi",
                        y["fiscalYear"], y["postingCount"], y["balanceCount"]
                    ))
                }
            }
            view=view.child("Luk arkivimporten. Gå til Virksomhedsopsætning for at oprette de ønskede konti. Brug derefter Åbningsbalance med eksplicit skæringsdato, beløb og eventuelle arkivhenvisninger, og kontrollér Rapporter. Arkivets saldi kopieres ikke automatisk.");
        }
        if let Some(review) = &self.review {
            let plan = review.summary();
            view = view
                .child(div().text_lg().child("Gennemgå arkivimport"))
                .child(format!(
                    "Højeste år {} udelades",
                    plan["excludedLatestYear"]
                ))
                .child(format!(
                    "Historisk basisvaluta: {} · {} kildebytes",
                    plan["declaredCurrency"].as_str().unwrap_or(""),
                    plan["source"]["sizeBytes"]
                ))
                .child(format!(
                    "Kildehash: {}",
                    plan["source"]["sha256"].as_str().unwrap_or("")
                ))
                .child(format!(
                    "Plan-hash: {}",
                    plan["planHash"].as_str().unwrap_or("")
                ));
            for y in plan["years"].as_array().into_iter().flatten() {
                view = view.child(format!(
                    "År {} · {} posteringer · {} · {}",
                    y["fiscalYear"],
                    y["postingCount"],
                    if y["hasBalances"] == true {
                        format!("{} saldi", y["balanceCount"])
                    } else {
                        "Ingen SaldoBalance-fil".into()
                    },
                    if y["status"] == "retained" {
                        "Allerede bevaret"
                    } else {
                        "Nyt arkivår"
                    }
                ));
            }
            for text in plan["limitations"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                view = view.child(text.to_owned());
            }
            if self.pending.is_some() {
                view=view.child("Forsøgets oprindelige nøgle og hash er bevaret. En læsning gentager aldrig importen.").child(Button::new("archive-reconcile").label("Kontrollér bevaret arkiv og kilde").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.reconcile(cx))));
            }
            let allowed = self.data.as_ref().is_some_and(|d| review.current(&d.scope));
            view = view
                .child(
                    Checkbox::new("archive-reviewed")
                        .label("Jeg bekræfter valuta og de gennemgåede arkivår")
                        .checked(self.reviewed)
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            this.reviewed = *checked;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("archive-apply")
                        .label(if self.pending.is_some() {
                            "Hent kvittering / prøv præcis samme arkivimport"
                        } else {
                            "Importér afsluttede år"
                        })
                        .disabled(
                            self.busy
                                || self.blocked
                                || !self.reviewed
                                || !allowed
                                || self.pending.is_some() && !self.reconciled,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.apply(cx))),
                );
            if self.pending.is_none() {
                view = view.child(
                    Button::new("archive-edit")
                        .label("Tilbage til kildefiler uden import")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.review = None;
                            this.reviewed = false;
                            this.reconciled = false;
                            cx.notify();
                        })),
                );
            }
            return view;
        }
        view=view.child("Vælg mindst to forskellige år med Posteringer.csv, inklusive det højeste kontrolår. SaldoBalance.csv er valgfri for hvert år. Højst 32 filer; 16 MiB pr. fil, 18 MiB i alt.").child("Historisk basisvaluta (tre store bogstaver)").child(Input::new(&self.currency).disabled(self.busy));
        for row in &self.rows {
            let id = row.id;
            let mut line = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(format!("Kildefil {id}"))
                .child(Input::new(&row.year).disabled(self.busy));
            for (kind, label) in [
                (state::FileKind::Postings, "Posteringer.csv"),
                (state::FileKind::Balances, "SaldoBalance.csv"),
            ] {
                let selected = row.kind == Some(kind);
                line = line.child(
                    Button::new(SharedString::from(format!("archive-kind-{id}-{label}")))
                        .label(format!(
                            "{}{}",
                            if selected { "Valgt: " } else { "" },
                            label
                        ))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(row) = this.rows.iter_mut().find(|r| r.id == id) {
                                row.kind = Some(kind);
                            }
                            cx.notify();
                        })),
                );
            }
            line = line
                .child(
                    row.file
                        .as_ref()
                        .map(|(name, bytes)| format!("{name} · {} bytes", bytes.len()))
                        .unwrap_or("Ingen fil valgt".into()),
                )
                .child(
                    Button::new(("archive-file", id))
                        .label("Vælg original CSV-fil")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| this.choose(id, cx))),
                )
                .child(
                    Button::new(("archive-remove", id))
                        .label("Fjern filrække")
                        .disabled(self.busy || self.rows.len() <= 2)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.rows.retain(|r| r.id != id);
                            cx.notify();
                        })),
                );
            view = view.child(line);
        }
        view.child(
            Button::new("archive-add")
                .label("Tilføj CSV-fil")
                .disabled(self.busy || self.rows.len() >= 32)
                .on_click(cx.listener(|this, _, window, cx| this.add_row(window, cx))),
        )
        .child(
            Button::new("archive-plan")
                .label(if self.pending.is_some() {
                    "Genskab oprindelig gennemgang fra samme kildefiler"
                } else {
                    "Gennemgå arkivimport"
                })
                .disabled(self.busy || self.blocked || self.data.is_none())
                .on_click(cx.listener(|this, _, _, cx| this.plan(cx))),
        )
    }
}
