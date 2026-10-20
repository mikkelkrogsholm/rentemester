//! Workspace onboarding, independent of the currently selected company. Root declares this
//! module plus company_creation_state/attempt and reads created_slug only when closing.
use crate::{
    Session, company_creation_attempt::Attempt, company_creation_state as state, readable,
};
use gpui_kit::{
    component::{
        Disableable,
        button::Button,
        checkbox::Checkbox,
        input::{Input, InputState},
        select::{SearchableVec, Select, SelectState},
    },
    *,
};
use rentemester_client::Error;
use serde_json::Value;
use std::sync::Arc;
type Choices = SelectState<SearchableVec<SharedString>>;
const MONTHS: &[(&str, &str)] = &[
    ("1", "Januar"),
    ("2", "Februar"),
    ("3", "Marts"),
    ("4", "April"),
    ("5", "Maj"),
    ("6", "Juni"),
    ("7", "Juli"),
    ("8", "August"),
    ("9", "September"),
    ("10", "Oktober"),
    ("11", "November"),
    ("12", "December"),
];
const LABELS: &[(&str, &str)] = &[
    ("end-year", "Året hvor regnskabsåret slutter"),
    ("start-year", "Året hvor regnskabsåret starter"),
    ("span", "Begge år i regnskabsåret"),
];
const VAT: &[(&str, &str)] = &[
    ("month", "Måned"),
    ("quarter", "Kvartal"),
    ("half-year", "Halvår"),
    ("none", "Ikke momsregistreret"),
];
const FIELDS: &[(&str, &str)] = &[
    ("name", "Virksomhedsnavn"),
    ("slug", "Adresse i Rentemester (valgfri)"),
    ("cvr", "CVR (valgfrit)"),
    ("bankName", "Banknavn (valgfrit)"),
    ("registrationNo", "Registreringsnummer (valgfrit)"),
    ("accountNo", "Kontonummer (valgfrit)"),
    ("iban", "IBAN (valgfrit)"),
];
struct Field {
    key: &'static str,
    input: Entity<InputState>,
}
fn read(session: &Session) -> Result<state::Snapshot, String> {
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let catalog = session
        .runtime
        .block_on(session.client.companies())
        .map_err(readable)?;
    state::Snapshot::parse(me, catalog)
}
fn selected(select: &Entity<Choices>, choices: &[(&str, &str)], cx: &App) -> String {
    select
        .read(cx)
        .selected_value()
        .and_then(|label| {
            choices
                .iter()
                .find(|(_, name)| *name == label.as_ref())
                .map(|(value, _)| value.to_string())
        })
        .unwrap_or_default()
}
fn label<'a>(value: &str, choices: &'a [(&str, &str)]) -> &'a str {
    choices
        .iter()
        .find(|(key, _)| *key == value)
        .map(|(_, label)| *label)
        .unwrap_or("Ikke angivet")
}
fn body_value(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or(Value::Null)
}
fn candidates(snapshot: &state::Snapshot, attempt: &Attempt) -> Vec<Value> {
    let body = body_value(&attempt.body);
    snapshot.catalog["companies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|company| {
            if let Some(slug) = body["slug"].as_str().filter(|s| !s.is_empty()) {
                company["slug"] == slug
            } else {
                company["name"] == body["name"]
            }
        })
        .cloned()
        .collect()
}
struct Outcome {
    status: String,
    pending: Option<Attempt>,
    created: Option<String>,
    profile: Option<Value>,
    snapshot: Option<state::Snapshot>,
}
pub struct CompanyCreation {
    entry_focus: FocusHandle,
    entry_focus_requested: bool,
    session: Arc<Session>,
    origin: String,
    local: Option<String>,
    snapshot: Option<state::Snapshot>,
    fields: Vec<Field>,
    month: Entity<Choices>,
    strategy: Entity<Choices>,
    vat: Entity<Choices>,
    review: Option<state::Review>,
    reviewed: bool,
    pending: Option<Attempt>,
    checked_catalog: bool,
    candidates: Vec<Value>,
    candidate_profile: Option<Value>,
    resolution: Entity<InputState>,
    resolved: bool,
    local_confirmed: bool,
    created: Option<String>,
    profile: Option<Value>,
    busy: bool,
    status: String,
}
impl CompanyCreation {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        local: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let fields = FIELDS
            .iter()
            .map(|(key, label)| Field {
                key,
                input: cx.new(|cx| InputState::new(window, cx).placeholder(*label)),
            })
            .collect();
        let mut choose = |options: &[(&str, &str)], cx: &mut Context<Self>| {
            cx.new(|cx| {
                SelectState::new(
                    SearchableVec::new(
                        options
                            .iter()
                            .map(|(_, name)| SharedString::from(name.to_string()))
                            .collect::<Vec<_>>(),
                    ),
                    None,
                    window,
                    cx,
                )
            })
        };
        let month = choose(MONTHS, cx);
        let strategy = choose(LABELS, cx);
        let vat = choose(VAT, cx);
        let resolution = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Beskriv hvordan oprettelsen blev afklaret manuelt")
        });
        let mut this = Self {
            entry_focus: cx.focus_handle(),
            entry_focus_requested: false,
            session,
            origin,
            local,
            snapshot: None,
            fields,
            month,
            strategy,
            vat,
            review: None,
            reviewed: false,
            pending: None,
            checked_catalog: false,
            candidates: vec![],
            candidate_profile: None,
            resolution,
            resolved: false,
            local_confirmed: false,
            created: None,
            profile: None,
            busy: false,
            status: String::new(),
        };
        this.load(cx);
        this
    }
    pub fn focus_entry(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.entry_focus_requested = true;
        cx.notify();
    }
    /// Never set by a same-name candidate, manual acknowledgement, or an uncertain response.
    pub fn created_slug(&self) -> Option<String> {
        self.created.clone()
    }
    fn load(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.snapshot = None;
        self.review = None;
        self.reviewed = false;
        self.checked_catalog = false;
        self.resolved = false;
        self.local_confirmed = false;
        self.candidates.clear();
        self.candidate_profile = None;
        self.status = "Henter din aktuelle adgang og virksomhedsliste…".into();
        let session = self.session.clone();
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((
                read(&session)?,
                Attempt::load(&crate::company_creation_attempt::directory())?,
            ))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((snapshot, pending)) => {
                        this.snapshot = Some(snapshot);
                        this.pending = pending;
                        this.status = if this.pending.is_some() {
                            "Et tidligere oprettelsesforsøg skal afklares. Der sendes ikke igen."
                        } else {
                            "Udfyld virksomhedsoplysningerne. Ingen virksomhed er oprettet endnu."
                        }
                        .into();
                    }
                    Err(e) => this.status = e,
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn prepare(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() || self.created.is_some() {
            return;
        }
        let Some(snapshot) = self.snapshot.clone() else {
            return;
        };
        let mut values = self
            .fields
            .iter()
            .map(|field| {
                (
                    field.key.to_owned(),
                    field.input.read(cx).value().to_string(),
                )
            })
            .collect::<Vec<_>>();
        values.extend([
            (
                "fiscalYearStartMonth".into(),
                selected(&self.month, MONTHS, cx),
            ),
            (
                "fiscalYearLabelStrategy".into(),
                selected(&self.strategy, LABELS, cx),
            ),
            ("vatPeriodType".into(), selected(&self.vat, VAT, cx)),
        ]);
        match state::body(&values).and_then(|bytes| state::Review::new(snapshot, bytes)) {
            Ok(review) => {
                self.review = Some(review);
                self.reviewed = false;
                self.status =
                    "Gennemgå virksomheden og de valgte indstillinger før oprettelse.".into();
            }
            Err(e) => self.status = e,
        }
        cx.notify();
    }
    fn send(&mut self, cx: &mut Context<Self>) {
        if self.busy || !self.reviewed || self.pending.is_some() || self.created.is_some() {
            return;
        }
        let Some(review) = self.review.clone() else {
            return;
        };
        self.busy = true;
        self.reviewed = false;
        self.status = "Opretter den gennemgåede virksomhed én gang…".into();
        let session = self.session.clone();
        let origin = self.origin.clone();
        let local = self.local.clone();
        let task=cx.background_executor().spawn(async move{
            let fresh=read(&session)?;if !review.current(&fresh){return Err("Din adgang eller virksomhedsliste er ændret. Genlæs og gennemgå igen; intet sendt.".into());}
            let attempt=Attempt {origin,principal:fresh.principal.clone(),local_identity:local,body:review.bytes.clone(),receipt:None};let dir=crate::company_creation_attempt::directory();attempt.save(&dir)?;
            match session.runtime.block_on(session.client.create_company(review.bytes.clone(),true)) {
                Ok(receipt)=>{
                    let slug=match review.receipt(&receipt){Ok(slug)=>slug,Err(_)=>return Ok(Outcome{status:"Udfaldet er uafklaret. Forsøget er bevaret privat og må ikke sendes igen. Genlæs virksomhedsliste og afklar manuelt.".into(),pending:Some(attempt),created:None,profile:None,snapshot:None})};
                    let attempt=attempt.record_receipt(&dir,&receipt)?;
                    let verified=(||{let snapshot=read(&session)?;let profile=session.runtime.block_on(session.client.company_profile(&slug)).map_err(readable)?;let selected=review.canonical(&receipt,&snapshot.catalog,&profile)?;attempt.finish(&dir)?;Ok::<_,String>((selected,profile,snapshot))})();
                    match verified{Ok((slug,profile,snapshot))=>Ok(Outcome{status:"Virksomheden er oprettet. Kvittering, virksomhedsliste og profil er bekræftet fra serveren.".into(),pending:None,created:Some(slug),profile:Some(profile),snapshot:Some(snapshot)}),Err(e)=>Ok(Outcome{status:format!("Oprettelseskvitteringen er bevaret, men genlæsningen kunne ikke bekræftes: {e}. Hent oplysningerne igen; der oprettes ikke en ny virksomhed."),pending:Some(attempt),created:None,profile:None,snapshot:None})}
                },
                Err(Error::Refused{status}) if matches!(status,400|401|403|404|409|422)=>{attempt.definite_refusal(&dir,status)?;Ok(Outcome{status:readable(Error::Refused{status}),pending:None,created:None,profile:None,snapshot:None})},
                Err(_)=>Ok(Outcome{status:"Udfaldet er uafklaret. Forsøget er bevaret privat og må ikke sendes igen. Genlæs virksomhedsliste og afklar manuelt.".into(),pending:Some(attempt),created:None,profile:None,snapshot:None})
            }
        });
        self.complete(task, cx);
        cx.notify();
    }
    fn complete(&mut self, task: Task<Result<Outcome, String>>, cx: &mut Context<Self>) {
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(outcome) => {
                        this.status = outcome.status;
                        this.pending = outcome.pending;
                        this.created = outcome.created;
                        this.profile = outcome.profile;
                        if let Some(snapshot) = outcome.snapshot {
                            this.snapshot = Some(snapshot);
                        }
                        this.review = None;
                        this.checked_catalog = false;
                        this.resolved = false;
                    }
                    Err(e) => {
                        this.status = e;
                        match Attempt::load(&crate::company_creation_attempt::directory()) {
                            Ok(pending) => this.pending = pending,
                            Err(e) => {
                                this.snapshot = None;
                                this.status = e;
                            }
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn inspect_pending(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(attempt) = self.pending.clone() else {
            return;
        };
        self.busy = true;
        self.resolved = false;
        self.checked_catalog = false;
        let session = self.session.clone();
        let origin = self.origin.clone();
        let local = self.local.clone();
        let local_confirmed = self.local_confirmed;
        let task = cx.background_executor().spawn(async move {
            let snapshot = read(&session)?;
            if !attempt.matches(&origin, &snapshot.principal, local.as_deref())
                && !(local_confirmed
                    && attempt.can_resolve_at(&origin, &snapshot.principal, local.as_deref()))
            {
                return Err(
                    "Forsøget tilhører en anden server eller bruger. Originalen bevares.".into(),
                );
            }
            if let Some(receipt) = &attempt.receipt {
                let verified = (|| {
                    let review = state::Review::new(snapshot.clone(), attempt.body.clone())?;
                    let slug = review.receipt(receipt)?;
                    let profile = session
                        .runtime
                        .block_on(session.client.company_profile(&slug))
                        .map_err(readable)?;
                    let slug = review.canonical(receipt, &snapshot.catalog, &profile)?;
                    attempt.finish(&crate::company_creation_attempt::directory())?;
                    Ok::<_, String>((slug, profile))
                })();
                match verified {
                    Ok(known) => Ok::<_, String>((snapshot, Some(known), Vec::new(), None)),
                    Err(error) => {
                        let matches = candidates(&snapshot, &attempt);
                        Ok((snapshot, None, matches, Some(error)))
                    }
                }
            } else {
                let matches = candidates(&snapshot, &attempt);
                Ok((snapshot, None, matches, None))
            }
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok((snapshot,known,candidates,notice))=>{this.snapshot=Some(snapshot);this.candidates=candidates;this.checked_catalog=true;this.candidate_profile=None;if let Some((slug,profile))=known{this.created=Some(slug);this.profile=Some(profile);this.pending=None;this.status="Den oprindelige oprettelseskvittering er nu bekræftet mod virksomhedsliste og profil.".into();}else{this.status=notice.map(|e|format!("Virksomhedslisten er genlæst, men den gemte kvittering kunne ikke bekræftes: {e}. Originalen bevares til afklaring.")).unwrap_or_else(||"Virksomhedslisten er genlæst. Et lignende navn eller en adresse er ikke en oprettelseskvittering. Afklar resultatet manuelt før en ny handling.".into());}},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn inspect_company(&mut self, slug: String, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(attempt) = self.pending.clone() else {
            return;
        };
        self.busy = true;
        self.candidate_profile = None;
        let session = self.session.clone();
        let origin = self.origin.clone();
        let local = self.local.clone();
        let local_confirmed = self.local_confirmed;
        let task = cx.background_executor().spawn(async move {
            let snapshot = read(&session)?;
            if (!attempt.matches(&origin, &snapshot.principal, local.as_deref())
                && !(local_confirmed
                    && attempt.can_resolve_at(&origin, &snapshot.principal, local.as_deref())))
                || !candidates(&snapshot, &attempt)
                    .iter()
                    .any(|row| row["slug"] == slug)
            {
                return Err(
                    "Virksomheden kunne ikke bekræftes i den aktuelle liste. Originalen bevares."
                        .into(),
                );
            }
            session
                .runtime
                .block_on(session.client.company_profile(&slug))
                .map_err(readable)
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok(profile)=>{this.candidate_profile=Some(profile);this.status="Aktuelle virksomhedsoplysninger er hentet til din manuelle afklaring. Dette beviser ikke det oprindelige forsøg.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn acknowledge(&mut self, cx: &mut Context<Self>) {
        if self.busy || !self.resolved || !self.checked_catalog {
            return;
        }
        let Some(attempt) = self.pending.clone() else {
            return;
        };
        self.busy = true;
        let reason = self.resolution.read(cx).value().to_string();
        let session = self.session.clone();
        let origin = self.origin.clone();
        let local = self.local.clone();
        let local_confirmed = self.local_confirmed;
        let task = cx.background_executor().spawn(async move {
            let fresh = read(&session)?;
            if !attempt.matches(&origin, &fresh.principal, local.as_deref())
                && !(local_confirmed
                    && attempt.can_resolve_at(&origin, &fresh.principal, local.as_deref()))
            {
                return Err(
                    "Den oprindelige bruger og server skal bekræftes før manuel afklaring.".into(),
                );
            }
            attempt.acknowledge(
                &crate::company_creation_attempt::directory(),
                &reason,
                &fresh,
                &origin,
                local.as_deref(),
            )?;
            Ok::<_, String>(fresh)
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok(snapshot)=>{this.snapshot=Some(snapshot);this.pending=None;this.review=None;this.created=None;this.profile=None;this.checked_catalog=false;this.resolved=false;this.candidates.clear();this.candidate_profile=None;this.status="Din manuelle afklaring er registreret. Dette er ikke en oprettelseskvittering, og ingen virksomhed vælges automatisk.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
}
fn profile_details(profile: &Value) -> Div {
    let company = &profile["company"];
    let month = company["fiscalYearStartMonth"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            company["fiscalYearStartMonth"]
                .as_u64()
                .map(|n| n.to_string())
        })
        .unwrap_or_default();
    let vat = if company["vatPeriodType"].is_null() {
        "Ikke momsregistreret"
    } else {
        label(company["vatPeriodType"].as_str().unwrap_or(""), VAT)
    };
    let mut view = div()
        .flex()
        .flex_col()
        .gap_2()
        .child(format!(
            "Virksomhed: {}",
            company["name"].as_str().unwrap_or("Ikke bekræftet")
        ))
        .child(format!(
            "CVR: {} · Valuta: {}",
            company["cvr"].as_str().unwrap_or("Ikke angivet"),
            company["currency"].as_str().unwrap_or("Ikke bekræftet")
        ))
        .child(format!(
            "Regnskabsåret starter i {} · {}",
            label(&month, MONTHS),
            label(
                company["fiscalYearLabelStrategy"].as_str().unwrap_or(""),
                LABELS
            )
        ))
        .child(format!("Moms: {vat}"));
    for (key, name) in [
        ("bankName", "Bank"),
        ("registrationNo", "Registreringsnummer"),
        ("accountNo", "Kontonummer"),
        ("iban", "IBAN"),
    ] {
        if let Some(value) = company["payment"][key].as_str().filter(|v| !v.is_empty()) {
            view = view.child(format!("{name}: {value}"));
        }
    }
    view
}
fn review_details(review: &state::Review) -> Div {
    let body = body_value(&review.bytes);
    let mut view = div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_lg().child("Gennemgå den nye virksomhed"))
        .child(format!(
            "Navn: {}",
            body["name"].as_str().unwrap_or("Ikke angivet")
        ))
        .child(format!(
            "Adresse i Rentemester: {}",
            body["slug"]
                .as_str()
                .unwrap_or("Serveren danner adressen fra navnet")
        ))
        .child(format!(
            "CVR: {}",
            body["cvr"].as_str().unwrap_or("Ikke angivet")
        ))
        .child(format!(
            "Regnskabsåret starter i {}",
            label(body["fiscalYearStartMonth"].as_str().unwrap_or(""), MONTHS)
        ))
        .child(format!(
            "Regnskabsårets navn: {}",
            label(
                body["fiscalYearLabelStrategy"].as_str().unwrap_or(""),
                LABELS
            )
        ))
        .child(format!(
            "Moms: {}",
            if body["vatPeriodType"].is_null() {
                "Ikke momsregistreret"
            } else {
                label(body["vatPeriodType"].as_str().unwrap_or(""), VAT)
            }
        ));
    for (key, name) in [
        ("bankName", "Bank"),
        ("registrationNo", "Registreringsnummer"),
        ("accountNo", "Kontonummer"),
        ("iban", "IBAN"),
    ] {
        if let Some(value) = body["payment"][key].as_str() {
            view = view.child(format!("{name}: {value}"));
        }
    }
    view.child("Virksomheden oprettes med en tom kontoplan. Serveren fastsætter valuta og øvrige standardindstillinger; de vises i den oprettede profil. Der bogføres ingen åbningssaldo.")
}
impl Render for CompanyCreation {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.entry_focus_requested && !self.busy {
            self.entry_focus_requested = false;
            let focus = self.entry_focus.clone();
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        }
        let mut view = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_xl().child("Opret virksomhed"))
            .child(format!("Server: {}", self.origin))
            .child(self.status.clone())
            .child(
                Button::new("creation-reload")
                    .track_focus(&self.entry_focus)
                    .label("Genlæs adgang og virksomhedsliste")
                    .disabled(self.busy || self.created.is_some())
                    .on_click(cx.listener(|this, _, _, cx| this.load(cx))),
            );
        let Some(snapshot) = self.snapshot.clone() else {
            return view;
        };
        if let Some(slug) = &self.created {
            view = view.child(div().text_lg().child("Virksomheden er oprettet"));
            if let Some(profile) = &self.profile {
                view = view.child(profile_details(profile));
            }
            return view.child(format!("Virksomhedens adresse: {slug}"))
                .child("Luk denne side og vælg Virksomhedsopsætning. Opret og bekræft dine valgte konti dér. Fortsæt derefter med Importér historik, Åbningsbalance og Rapporter efter behov. Ingen konti eller saldi kopieres fra en anden virksomhed.");
        }
        if let Some(attempt) = self.pending.clone() {
            view=view.child(div().text_lg().child("Tidligere oprettelse kræver afklaring"))
                .child("Der sendes ikke igen. Virksomhedsnavn eller adresse i en liste beviser ikke, at det oprindelige forsøg lykkedes.");
            let exact = attempt.matches(&self.origin, &snapshot.principal, self.local.as_deref());
            if !exact {
                if !attempt.can_resolve_at(&self.origin, &snapshot.principal, self.local.as_deref())
                {
                    return view.child("Genåbn den oprindelige server, bruger og lokale profil for at afklare forsøget. De oprindelige oplysninger bevares.");
                }
                view=view.child(format!("Oprindelig lokal server: {} · Aktuel lokal server: {}",attempt.origin,self.origin))
                    .child(Checkbox::new("creation-local-context").label("Jeg har kontrolleret, at dette er samme lokale profil og bruger. Der må kun læses og afklares; intet sendes igen.").checked(self.local_confirmed).on_click(cx.listener(|this,checked,_,cx|{this.local_confirmed=*checked;this.checked_catalog=false;this.resolved=false;cx.notify();})));
            }
            let allowed = exact || self.local_confirmed;
            view = view.child(
                Button::new("creation-read-status")
                    .label(if attempt.receipt.is_some() {
                        "Bekræft den gemte kvittering mod serveren"
                    } else {
                        "Læs aktuelle virksomheder uden at sende igen"
                    })
                    .disabled(self.busy || !allowed)
                    .on_click(cx.listener(|this, _, _, cx| this.inspect_pending(cx))),
            );
            for company in &self.candidates {
                let Some(slug) = company["slug"].as_str() else {
                    continue;
                };
                let slug = slug.to_owned();
                view = view
                    .child(format!(
                        "Muligt match: {} · {}",
                        company["name"].as_str().unwrap_or("Navn ikke bekræftet"),
                        slug
                    ))
                    .child(
                        Button::new(SharedString::from(format!("creation-inspect-{slug}")))
                            .label("Læs virksomhedens aktuelle profil")
                            .disabled(self.busy || !allowed)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.inspect_company(slug.clone(), cx)
                            })),
                    );
            }
            if let Some(profile) = &self.candidate_profile {
                view=view.child(profile_details(profile)).child("Denne profil er kun læst til manuel afklaring og er ikke en erstatning for en oprettelseskvittering.");
            }
            if self.checked_catalog && self.candidates.is_empty() && attempt.receipt.is_none() {
                view=view.child("Ingen matchende virksomhed blev fundet i den aktuelle liste. Oprettelsen kan stadig være gennemført; der forsøges ikke igen.");
            }
            return view.child("Hvis resultatet er afklaret på anden vis, beskriv din undersøgelse. Den manuelle registrering er ikke en serverkvittering og vælger ikke en virksomhed.")
                .child(Input::new(&self.resolution).disabled(self.busy||!allowed))
                .child(Checkbox::new("creation-manual-resolved").label("Jeg har undersøgt udfaldet og vil registrere den manuelle afklaring uden genafsendelse").checked(self.resolved).on_click(cx.listener(|this,checked,_,cx|{this.resolved=*checked;cx.notify();})))
                .child(Button::new("creation-acknowledge").label("Registrér manuel afklaring").disabled(self.busy||!allowed||!self.checked_catalog||!self.resolved).on_click(cx.listener(|this,_,_,cx|this.acknowledge(cx))));
        }
        if let Some(review) = self.review.clone() {
            return view.child(review_details(&review))
                .child(Checkbox::new("creation-reviewed").label("Jeg har gennemgået navn, regnskabsår, moms og eventuelle betalingsoplysninger").checked(self.reviewed).on_click(cx.listener(|this,checked,_,cx|{this.reviewed=*checked;cx.notify();})))
                .child(Button::new("creation-confirm").label("Bekræft og opret virksomhed").disabled(self.busy||!self.reviewed).on_click(cx.listener(|this,_,_,cx|this.send(cx))))
                .child(Button::new("creation-edit").label("Tilbage til felterne").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.reviewed=false;cx.notify();})));
        }
        view=view.child("Opret en ny, selvstændig virksomhed. Du vælger regnskabsår og momsstatus. Kontoplan og åbningssaldi opsættes særskilt efter oprettelsen.");
        for field in &self.fields[..3] {
            let name = FIELDS
                .iter()
                .find(|(key, _)| *key == field.key)
                .map(|(_, name)| *name)
                .unwrap_or("");
            view = view
                .child(name)
                .child(Input::new(&field.input).disabled(self.busy));
        }
        view=view.child("Regnskabsårets startmåned").child(Select::new(&self.month).accessibility_label("Regnskabsårets startmåned").placeholder("Vælg startmåned").disabled(self.busy))
            .child("Navn på regnskabsåret").child(Select::new(&self.strategy).accessibility_label("Navn på regnskabsåret").placeholder("Vælg navngivning").disabled(self.busy))
            .child("Momsstatus og afregningsperiode").child(Select::new(&self.vat).accessibility_label("Momsstatus og afregningsperiode").placeholder("Vælg momsstatus").disabled(self.busy))
            .child("Betalingsoplysninger (valgfrie). Dette er oplysninger til virksomhedens profil, ikke valg af bogføringskonti.");
        for field in &self.fields[3..] {
            let name = FIELDS
                .iter()
                .find(|(key, _)| *key == field.key)
                .map(|(_, name)| *name)
                .unwrap_or("");
            view = view
                .child(name)
                .child(Input::new(&field.input).disabled(self.busy));
        }
        view.child(
            Button::new("creation-review")
                .label("Gennemgå virksomheden før oprettelse")
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, _, cx| this.prepare(cx))),
        )
    }
}
