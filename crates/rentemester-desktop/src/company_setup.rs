//! Current-company master data. Review exact SDK bytes, send once, then read the canonical result.
use crate::{Session, company_setup_state as state, readable};
use gpui_kit::{
    component::{
        Disableable,
        button::Button,
        checkbox::Checkbox,
        input::{Input, InputState},
    },
    *,
};
use rentemester_client::{CompanySetupMutation, Error, Report};
use serde_json::Value;
use state::{Operation, Review, Snapshot};
use std::sync::Arc;
fn read(session: &Session, company: &str) -> Result<Snapshot, String> {
    let profile = session
        .runtime
        .block_on(session.client.company_profile(company))
        .map_err(readable)?;
    let chart = session
        .runtime
        .block_on(session.client.accounts(company))
        .map_err(readable)?;
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let journal = session
        .runtime
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    Snapshot::parse(company, profile, chart, me, journal)
}
struct Field {
    key: &'static str,
    input: Entity<InputState>,
}
pub struct CompanySetup {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    data: Option<Snapshot>,
    operation: Operation,
    fields: Vec<Field>,
    review: Option<Review>,
    reviewed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
}
impl CompanySetup {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        company: String,
        local: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            session,
            origin,
            company,
            local,
            data: None,
            operation: Operation::Profile,
            fields: vec![],
            review: None,
            reviewed: false,
            pending: None,
            resolved: false,
            busy: false,
            status: String::new(),
        };
        this.load(window, cx);
        this
    }
    fn form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.review = None;
        self.reviewed = false;
        self.fields = state::fields(self.operation)
            .iter()
            .map(|(key, label)| {
                let value = if self.operation == Operation::Profile {
                    self.data
                        .as_ref()
                        .map(|data| match *key {
                            "vatPeriodType" => {
                                data.profile[*key].as_str().unwrap_or("none").to_owned()
                            }
                            "paymentTermsDays" => data.profile[*key]
                                .as_i64()
                                .map(|n| n.to_string())
                                .unwrap_or_default(),
                            _ => data.profile[*key].as_str().unwrap_or("").to_owned(),
                        })
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                Field {
                    key,
                    input: cx.new(|cx| {
                        let mut input = InputState::new(window, cx).placeholder(*label);
                        input.set_value(value, window, cx);
                        input
                    }),
                }
            })
            .collect();
    }
    fn load(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.data = None;
        self.review = None;
        self.reviewed = false;
        self.resolved = false;
        self.status = "Henter virksomhedens kanoniske profil, kontoplan og roller…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let task = cx
            .background_executor()
            .spawn(async move { Ok::<_, String>((read(&session, &company)?, state::load()?)) });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{
            this.busy=false;match result {Ok((data,pending))=>{this.data=Some(data);this.pending=pending;this.fields.clear();this.status="Vælg den handling, du vil gennemgå. Serveren afgør aktuelle rettigheder, momsperiode og kontokompatibilitet.".into();},Err(e)=>this.status=e}cx.notify();
        });}).detach();
        cx.notify();
    }
    fn prepare(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = self.data.clone() else {
            return;
        };
        let values = self
            .fields
            .iter()
            .map(|field| {
                (
                    field.key.to_owned(),
                    field.input.read(cx).value().to_string(),
                )
            })
            .collect::<Vec<_>>();
        match state::body(self.operation, &values, &data) {
            Ok(bytes) => {
                self.review = Some(Review {
                    operation: self.operation,
                    bytes,
                    before: data,
                });
                self.reviewed = false;
                self.status =
                    "Gennemgå de præcise felter nedenfor. Kun disse bytes sendes ved bekræftelse."
                        .into();
            }
            Err(e) => self.status = e,
        }
        cx.notify();
    }
    fn send(&mut self, cx: &mut Context<Self>) {
        if self.busy || !self.reviewed || self.pending.is_some() {
            return;
        }
        let Some(review) = self.review.take() else {
            return;
        };
        self.reviewed = false;
        self.busy = true;
        self.data = None;
        self.fields.clear();
        self.status = "Sender den gennemgåede opsætning én gang…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let origin = self.origin.clone();
        let local = self.local.clone();
        let task=cx.background_executor().spawn(async move{
            let fresh=read(&session,&company)?;
            if !review.current(&fresh){return Err("Profil, kontoplan, bruger eller ledger er ændret siden gennemgangen. Intet sendt; genindlæs og gennemgå igen.".into());}
            let marker=fresh.marker(review.operation,&origin,&company,local.as_deref());state::begin(&marker)?;
            let op=match review.operation {Operation::Profile=>CompanySetupMutation::UpdateProfile,Operation::Account=>CompanySetupMutation::CreateAccount,Operation::Role=>CompanySetupMutation::ConfirmAccountRole};
            let response=session.runtime.block_on(session.client.company_setup_mutation(&company,op,review.bytes.clone(),true));
            let status=match response {
                Ok(value) if review.receipt(&value)=>{state::finish(&marker,false)?;"Serveren har bekræftet ændringen. Kanonisk profil og kontoplan genlæses.".to_owned()},
                Err(Error::Refused{status})=>{state::finish(&marker,false)?;format!("Serveren afviste ændringen (HTTP{status}). Kontrollér rettigheder, momsperiode, valgte felter og kontokompatibilitet. Der er ikke sendt igen.")},
                Err(Error::InvalidInput(_))=>{state::finish(&marker,false)?;"Anmodningen blev afvist før afsendelse. Genindlæs og gennemgå felterne igen.".into()},
                _=>"UDFALD UAFKLARET. Opsætningen kan være ændret. Der sendes ikke igen; genlæs den korrekte virksomhed og afklar manuelt før en ny ændring.".into(),
            };
            Ok::<_,String>((status,read(&session,&company),state::load()?))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((status, data, pending)) => {
                        this.status = status;
                        this.pending = pending;
                        match data {
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
impl Render for CompanySetup {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut view = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_xl().child("Virksomhedsopsætning"))
            .child(format!("{} · {}", self.company, self.origin))
            .child(self.status.clone())
            .child(
                Button::new("setup-reload")
                    .label("Genindlæs profil og kontoplan")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.load(window, cx))),
            );
        let Some(data) = self.data.clone() else {
            return view;
        };
        view = view
            .child(format!(
                "Kanonisk virksomhed: {} · CVR {} · valuta {}",
                data.profile["name"].as_str().unwrap_or("?"),
                data.profile["cvr"].as_str().unwrap_or("ikke angivet"),
                data.profile["currency"].as_str().unwrap_or("?")
            ))
            .child(format!(
                "Momsperiode: {} · betalingsfrist: {} dage",
                data.profile["vatPeriodType"]
                    .as_str()
                    .unwrap_or("ingen registrering"),
                data.profile["paymentTermsDays"]
            ))
            .child(format!(
                "Adresse: {} · {} {}",
                data.profile["address"].as_str().unwrap_or(""),
                data.profile["postalCode"].as_str().unwrap_or(""),
                data.profile["city"].as_str().unwrap_or("")
            ))
            .child("Kontoroller · kun serverens aktuelle status")
            .child(format!(
                "Status: {} · manglende: {} · tvetydige: {}",
                data.chart["accountRoles"]["status"],
                data.chart["accountRoles"]["missing"],
                data.chart["accountRoles"]["ambiguous"]
            ));
        for resolution in data.chart["accountRoles"]["resolutions"]
            .as_array()
            .into_iter()
            .flatten()
        {
            view = view.child(format!("{}", resolution));
        }
        for reason in data.chart["accountRoles"]["reasons"]
            .as_array()
            .into_iter()
            .flatten()
        {
            view = view.child(format!("{}", reason));
        }
        view = view.child("Kontoplan · eksplicit oprettede konti bevares historisk");
        for row in data.chart["accounts"].as_array().into_iter().flatten() {
            view = view.child(format!(
                "{} · {} · {} / {} · momskode {}{}",
                row["accountNo"].as_str().unwrap_or("?"),
                row["name"].as_str().unwrap_or("?"),
                row["type"].as_str().unwrap_or("?"),
                row["normalBalance"].as_str().unwrap_or("?"),
                row["defaultVatCode"].as_str().unwrap_or("ingen"),
                if row["active"] == true {
                    ""
                } else {
                    " · inaktiv"
                }
            ));
        }
        if let Some(marker) = self.pending.clone() {
            view=view.child("Uafklaret opsætningsændring. Ingen payload er gemt, og genafsendelse tilbydes ikke. Kanonisk læsning er ikke i sig selv en kvittering.")
                .child(format!("Oprindelig server {} · virksomhed {} · handling {}",marker["origin"],marker["company"],marker["operation"]));
            if data.matches_marker(&marker, &self.origin, &self.company, self.local.as_deref()) {
                let url = format!("{}/companies/{}/manage", self.origin, self.company);
                view=view.child(Button::new("setup-browser").label("Åbn virksomhed til manuel afklaring").on_click(move|_,_,cx|cx.open_url(&url)))
                    .child(Checkbox::new("setup-resolved").label("Jeg har kontrolleret udfaldet og afklaret den oprindelige ændring. Ingen genafsendelse.").checked(self.resolved).on_click(cx.listener(|this,checked,_,cx|{this.resolved=*checked;cx.notify();})))
                    .child(Button::new("setup-archive").label("Registrér manuel afklaring").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{
                        if this.busy||!this.resolved{return;}let(Some(data),Some(marker))=(&this.data,&this.pending)else{return;};
                        if !data.matches_marker(marker,&this.origin,&this.company,this.local.as_deref()){return;}
                        match state::finish(marker,true){Ok(())=>{this.pending=None;this.resolved=false;this.status="Manuel afklaring registreret. Dette er ikke en serverkvittering; vælg en ny handling eksplicit.".into();},Err(e)=>this.status=e}cx.notify();
                    })));
            } else {
                view = view.child(
                    "Genåbn den oprindelige bruger, server og virksomhed for at afklare ændringen.",
                );
            }
            return view;
        }
        if let Some(review) = self.review.clone() {
            let body = serde_json::from_slice::<Value>(&review.bytes).unwrap_or(Value::Null);
            view=view.child("Gennemgå præcis anmodning").child(serde_json::to_string_pretty(&body).unwrap_or_default())
                .child("Serveren genkontrollerer aktuelle stamdatarettigheder. Momsregistrering og kontoroller udledes ikke automatisk.")
                .child(Checkbox::new("setup-confirm-check").label("Jeg har gennemgået disse felter for den valgte virksomhed").checked(self.reviewed).on_click(cx.listener(|this,checked,_,cx|{this.reviewed=*checked;cx.notify();})))
                .child(Button::new("setup-confirm").label("Bekræft og send én gang").disabled(self.busy||!self.reviewed).on_click(cx.listener(|this,_,_,cx|this.send(cx))))
                .child(Button::new("setup-cancel").label("Tilbage til felterne").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.reviewed=false;cx.notify();})));
            return view;
        }
        for (id, label, operation) in [
            (
                "setup-profile",
                "Redigér virksomhedsprofil",
                Operation::Profile,
            ),
            ("setup-account", "Opret konto", Operation::Account),
            ("setup-role", "Bekræft kontorolle", Operation::Role),
        ] {
            view = view.child(Button::new(id).label(label).disabled(self.busy).on_click(
                cx.listener(move |this, _, window, cx| {
                    this.operation = operation;
                    this.form(window, cx);
                    cx.notify();
                }),
            ));
        }
        if !self.fields.is_empty() {
            view=view.child("Kontotype, normal saldo og eventuel momskode vælges af dig. Kompatibilitet og gældende regler valideres af serveren.");
            for field in &self.fields {
                let label = state::fields(self.operation)
                    .iter()
                    .find(|(key, _)| *key == field.key)
                    .map(|(_, label)| *label)
                    .unwrap_or(field.key);
                view = view
                    .child(label)
                    .child(Input::new(&field.input).disabled(self.busy));
            }
            view = view.child(
                Button::new("setup-review")
                    .label("Gennemgå valgte felter")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.prepare(cx))),
            );
        }
        view
    }
}
