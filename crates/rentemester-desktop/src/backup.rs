//! Whole-workspace export. The server owns signing, coverage and current authorization.
use crate::{
    Session,
    backup_state::{Archive, Scope},
    readable,
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
use rentemester_client::Error;
use std::{path::PathBuf, sync::Arc};
fn scope(session: &Session) -> Result<Scope, String> {
    Scope::parse(
        session
            .runtime
            .block_on(session.client.me())
            .map_err(readable)?,
    )
}
pub struct Backup {
    session: Arc<Session>,
    origin: String,
    data: Option<Scope>,
    review: Option<Scope>,
    confirmed: bool,
    archive: Option<Arc<Archive>>,
    destination: Entity<InputState>,
    local_public_key: Option<(PathBuf, [u8; 32])>,
    key_destination: Entity<InputState>,
    busy: bool,
    attempted: bool,
    status: String,
}
impl Backup {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        local_public_key: Option<(PathBuf, [u8; 32])>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            session,
            origin,
            data: None,
            review: None,
            confirmed: false,
            archive: None,
            destination: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Absolut ny filsti i din egen mappe")
            }),
            local_public_key,
            key_destination: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Ny absolut filsti til separat offentlig nøgle (.key)")
            }),
            busy: false,
            attempted: false,
            status: String::new(),
        };
        this.load(cx);
        this
    }
    fn load(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.review = None;
        self.confirmed = false;
        self.status = "Henter den aktuelle bruger og workspaceoversigt…".into();
        let session = self.session.clone();
        let task = cx
            .background_executor()
            .spawn(async move { scope(&session) });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok(data)=>{this.data=Some(data);this.status="Eksport omfatter serverens hele workspace, ikke kun en valgt virksomhed. Ingen fil gemmes før du vælger en destination.".into();},Err(e)=>{this.data=None;this.status=e}}cx.notify();});}).detach();
        cx.notify();
    }
    fn export(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.attempted || !self.confirmed || self.archive.is_some() {
            return;
        }
        let Some(review) = self.review.take() else {
            return;
        };
        self.busy = true;
        self.attempted = true;
        self.confirmed = false;
        self.status = "Kontrollerer aktuel oversigt og anmoder om én workspaceeksport…".into();
        let session = self.session.clone();
        let task=cx.background_executor().spawn(async move{
   if scope(&session)?!=review{return Err("Bruger, workspacerolle eller synlig virksomhedsoversigt ændrede sig. Intet eksportkald sendt; gennemgå igen.".into());}
   let value=session.runtime.block_on(session.client.workspace_backup(true)).map_err(|error|match error{
    Error::OutcomeUnknown|Error::Transport=>"Eksportudfaldet er uafklaret, og ingen fuldstændige bytes blev modtaget. Serveren kan have registreret forsøget. Ingen automatisk gentagelse.".into(),
    Error::Refused{status:403}=>"Backup er afvist. Aktuel workspaceadministration og administratoradgang til alle registrerede virksomheder kræves.".into(),
    Error::Refused{status:422}=>"Backup er utilgængelig: serveren afviste nøgleopsætning, datadækning eller anden snapshotforudsætning. Ingen fil er modtaget. Nøgleopsætning udføres separat af operatøren.".into(),
    other=>readable(other),
   })?;
   Archive::from_response(value).map(Arc::new)
  });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok(archive)=>{this.archive=Some(archive);this.status="Hele transportfilen er modtaget i hukommelsen. Vælg en ny lokal filsti og gem. Signatur og gendannelse er ikke verificeret her.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn save_public_key(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some((source, bytes)) = self.local_public_key.clone() else {
            return;
        };
        let destination = PathBuf::from(self.key_destination.read(cx).value().as_ref());
        self.busy = true;
        self.status =
            "Gemmer kun den separate offentlige nøgle; intet backup-/netværkskald…".into();
        let task = cx.background_executor().spawn(async move {
            crate::backup_state::save_public_key(&source, &bytes, &destination)
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;this.status=match result{
            Ok(())=>"Den offentlige nøgles præcise 32 rå bytes er gemt privat (0600) og synkroniseret. Opbevar denne uafhængige kopi særskilt fra backupfilen. Ingen privat nøgle blev eksporteret, og ingen backup blev verificeret eller gendannet.".into(),
            Err(error)=>format!("{error}. Ingen eksisterende fil er overskrevet. Hvis en delvis ny fil findes, vælg et andet filnavn og gem eksplicit igen. Intet backupkald blev sendt.")};cx.notify();});}).detach();
        cx.notify();
    }
    fn save(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(archive) = self.archive.clone() else {
            return;
        };
        let path = PathBuf::from(self.destination.read(cx).value().as_ref());
        self.busy = true;
        self.status = "Gemmer de allerede modtagne bytes; intet nyt eksportkald…".into();
        let task = cx
            .background_executor()
            .spawn(async move { archive.save(&path) });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;this.status=match result{Ok(())=>"De præcise modtagne backupbytes er gemt privat (0600) og synkroniseret. Dette er ikke en signaturverifikation eller en gennemført gendannelse.".into(),Err(e)=>format!("{e}. De modtagne bytes er stadig i hukommelsen. En delvis fil kan være oprettet; vælg et nyt filnavn og gem manuelt igen uden ny eksport.")};cx.notify();});}).detach();
        cx.notify();
    }
}
impl Render for Backup {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut view=div().flex().flex_col().gap_3().p_4().child(div().text_xl().child("Eksportér hele workspace som native backup"))
   .child(format!("Server: {}",self.origin)).child(self.status.clone())
   .child("Serverens native format omfatter understøttede virksomhedsdata og businessaudit med stabile identiteter. Loginoplysninger, levende adgangsrettigheder og drifts-/signeringsnøgler er udeladt. Ikke-understøttede data kan medføre afvisning; der loves ikke fuld legacy-paritet.")
   .child("Signeringsnøglen skal være konfigureret på serveren. Nye administrerede lokale profiler har en separat nøgle; eksisterende usignerede profiler opgraderes ikke her. Uafhængig verifikation kræver en særskilt betroet offentlig nøgle. Denne visning ændrer ingen backup-politik og gendanner intet.");
        if let Some((source, _)) = &self.local_public_key {
            view=view.child(div().text_lg().child("Gem separat offentlig backupnøgle · RAW Ed25519, 32 bytes"))
                .child(format!("Betroet lokal profilnøgle: {}. Valideret mod profilens konfigurerede signer ved åbning af denne visning.",source.display()))
                .child("Vælg selv en ny absolut filsti. Bevar kopien uafhængigt af backupfilen til senere signaturkontrol. Nøgler fra en backupfil overtages aldrig som betroede. Privat signeringsnøgle udleveres ikke.")
                .child(Input::new(&self.key_destination).disabled(self.busy))
                .child(Button::new("backup-save-public-key").label("Gem kun offentlig nøgle i en ny privat fil").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.save_public_key(cx))));
        } else {
            view=view.child("Ingen valideret administreret lokal offentlig nøgle er tilgængelig. Ved ekstern server skal administratoren levere den betroede offentlige nøgle på 32 bytes særskilt. En eksisterende lokal profil uden signer kræver en fremtidig eksplicit nøgleopsætning; der oprettes eller overtages ingen nøgle her.");
        }
        if let Some(archive) = &self.archive {
            return view.child(format!("Modtaget {}bytes. Ingen automatisk lagring; lukning af visningen fjerner kun kopien i hukommelsen.",archive.bytes.len()))
    .child("Ny absolut lokal filsti · ingen symbolske links eller overskrivning").child(Input::new(&self.destination).disabled(self.busy))
    .child(Button::new("backup-save").label("Gem allerede modtagne bytes i en ny privat fil").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.save(cx))));
        }
        if let Some(data) = &self.data {
            view = view
                .child(format!(
                    "Aktuel bruger {} · workspacerolle {}",
                    data.user, data.workspace_role
                ))
                .child("Din aktuelle synlige virksomhedsoversigt (ingen eksportfiltrering):");
            for c in data.companies.as_array().into_iter().flatten() {
                view = view.child(format!(
                    "{} · {} · rolle {} · arkiveret {}",
                    c["slug"].as_str().unwrap_or("?"),
                    c["name"].as_str().unwrap_or("?"),
                    c["role"].as_str().unwrap_or("?"),
                    c["archived"]
                ));
            }
        }
        if self.attempted {
            return view.child("Der foretages ingen automatisk gentagelse. En eventuel ny eksport er en selvstændig handling med nyt review.")
   .child(Button::new("backup-new-review").label("Start en ny selvstændig gennemgang").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.attempted=false;this.load(cx);})));
        }
        if self.review.is_some() {
            view=view.child("Gennemgå: hele serverens workspace med alle registrerede virksomheder. Ingen virksomhed, år eller mappe vælges som eksportfilter.")
   .child(Checkbox::new("backup-confirmed").label("Jeg bekræfter eksport af hele dette workspace og forstår, at datafilen er følsom.").checked(self.confirmed).disabled(self.busy).on_click(cx.listener(|this,value,_,cx|{this.confirmed=*value;cx.notify();})))
   .child(Button::new("backup-export").label("Bekræft og hent én workspacebackup").disabled(self.busy||!self.confirmed).on_click(cx.listener(|this,_,_,cx|this.export(cx))))
   .child(Button::new("backup-cancel").label("Annullér før eksport").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.confirmed=false;cx.notify();})));
        } else {
            view = view
                .child(
                    Button::new("backup-reload")
                        .label("Genlæs aktuel workspaceoversigt")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.load(cx))),
                )
                .child(
                    Button::new("backup-review")
                        .label("Gennemgå eksport af hele workspace")
                        .disabled(self.busy || self.data.is_none())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.review = this.data.clone();
                            this.confirmed = false;
                            cx.notify();
                        })),
                );
        }
        view
    }
}
