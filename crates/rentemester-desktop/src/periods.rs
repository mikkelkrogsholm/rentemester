//! Native VAT and reviewed period lifecycle. No filing, deadline arithmetic or force path.
use crate::{Session, periods_state as state, readable};
use gpui_kit::{
    component::{
        Disableable,
        button::Button,
        checkbox::Checkbox,
        input::{Input, InputState},
    },
    *,
};
use rentemester_client::{Error, PeriodMutation, Report};
use serde_json::{Value, json};
use std::sync::Arc;
#[derive(Clone, PartialEq, Eq)]
struct Bounds {
    start: String,
    end: String,
}
impl Bounds {
    fn from(value: &Value) -> Result<Self, String> {
        Ok(Self {
            start: text(value, "periodStart")?,
            end: text(value, "periodEnd")?,
        })
    }
}
#[derive(Clone)]
struct Data {
    vat: Value,
    periods: Value,
    profile: Value,
    packet: Option<Value>,
    target: Option<Bounds>,
    principal: String,
    ledger: String,
}
fn text(v: &Value, key: &str) -> Result<String, String> {
    v[key]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Serveren mangler {key}"))
}
fn array(v: &Value, key: &str) -> Result<Vec<Value>, String> {
    v[key]
        .as_array()
        .cloned()
        .ok_or_else(|| format!("Serveren mangler {key}"))
}
fn digest(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn packet(value: &Value, bounds: &Bounds) -> Result<(), String> {
    if Bounds::from(value)? != *bounds
        || !digest(&value["hash"])
        || !value["blockers"].is_u64()
        || !value["warnings"].is_u64()
    {
        return Err(
            "Serverens kontrolpakke mangler periode, hash eller aktuelle kontroltal".into(),
        );
    }
    array(value, "controlsRun")?;
    let items = array(value, "items")?;
    if items.is_empty() {
        return Err(
            "Ingen kontroller blev returneret; der tilbydes ikke review eller lukning".into(),
        );
    }
    for item in items {
        if item["code"].as_str().is_none()
            || !matches!(
                item["status"].as_str(),
                Some("passed" | "warning" | "blocked" | "unavailable")
            )
            || !item["count"].is_u64()
            || !item["amount"].is_number()
            || !item["waivable"].is_boolean()
            || !digest(&item["sourceHash"])
            || item["evidence"].as_array().is_none()
        {
            return Err(
                "Kontrolpakken mangler fuldstændig status/evidens; ingen lukning tilbydes".into(),
            );
        }
    }
    Ok(())
}
fn read(
    session: &Session,
    company: &str,
    year: &str,
    target: Option<Bounds>,
) -> Result<Data, String> {
    let vat = session
        .runtime
        .block_on(
            session
                .client
                .vat(company, (!year.is_empty()).then_some(year)),
        )
        .map_err(readable)?;
    let periods = session
        .runtime
        .block_on(session.client.periods(company))
        .map_err(readable)?;
    let profile = session
        .runtime
        .block_on(session.client.company_profile(company))
        .map_err(readable)?;
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let journal = session
        .runtime
        .block_on(session.client.report(
            company,
            Report::Journal,
            (!year.is_empty()).then_some(year),
            None,
        ))
        .map_err(readable)?;
    if [&vat, &periods, &profile, &me, &journal]
        .iter()
        .any(|r| r["ok"] != true)
        || vat["vat"]["slug"] != company
        || periods["periods"]["slug"] != company
        || journal["journal"]["slug"] != company
    {
        return Err("Serveren bekræftede ikke virksomhed, bruger, moms og perioder".into());
    }
    let vat = vat["vat"].clone();
    if !vat["vatRegistered"].is_boolean() {
        return Err("Momsregistreringen mangler i serverens svar".into());
    }
    if vat["vatRegistered"] == true {
        Bounds::from(&vat)?;
        text(&vat, "deadline")?;
        text(&profile["company"], "vatPeriodType")?;
        for key in ["outputVat", "outputVatAdjustment", "inputVat", "payable"] {
            if !vat[key].is_number() {
                return Err(format!(
                    "Serveren mangler momsbeløbet {key}; ingen lokale beløb indsættes"
                ));
            }
        }
        if vat["rubrikker"].as_object().is_none() || !vat["momsangivelseReady"].is_boolean() {
            return Err("Serveren mangler rubrikker eller angivelsesstatus".into());
        }
        array(&vat, "vatReportErrors")?;
        array(&vat, "vatReportWarnings")?;
    }
    array(&periods["periods"], "periods")?;
    let target = target.or_else(|| {
        (vat["vatRegistered"] == true)
            .then(|| Bounds::from(&vat).ok())
            .flatten()
    });
    let current_packet = if let Some(bounds) = &target {
        match session
            .runtime
            .block_on(
                session
                    .client
                    .period_close_readiness(company, &bounds.start, &bounds.end),
            ) {
            Ok(root) => {
                if root["ok"] != true {
                    return Err("Serveren bekræftede ikke kontrolpakken".into());
                }
                packet(&root["packet"], bounds)?;
                Some(root["packet"].clone())
            }
            Err(Error::Refused { status: 403 }) => None,
            Err(error) => return Err(readable(error)),
        }
    } else {
        None
    };
    Ok(Data {
        vat,
        periods: periods["periods"].clone(),
        profile: profile["company"].clone(),
        packet: current_packet,
        target,
        principal: text(&me["user"], "id")?,
        ledger: text(&journal["journal"], "ledgerUuid")?,
    })
}
fn matching_rows(data: &Data) -> Vec<&Value> {
    let Some(target) = data.target.as_ref() else {
        return vec![];
    };
    data.periods["periods"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| {
            r["periodStart"] == target.start
                && r["periodEnd"] == target.end
                && matches!(r["kind"].as_str(), Some("vat_period" | "vat_quarter"))
        })
        .collect()
}
fn row(data: &Data) -> Option<&Value> {
    let rows = matching_rows(data);
    (rows.len() == 1).then(|| rows[0])
}
fn state_of(data: &Data) -> Result<&str, String> {
    if matching_rows(data).len() > 1 {
        return Err(
            "Flere momsperioder har samme datoer. Afklar perioderne før en handling.".into(),
        );
    }
    if let Some(row) = row(data) {
        match row["effectiveStatus"].as_str() {
            Some(s @ ("open" | "closed" | "reported")) => Ok(s),
            _ => Err("Periodens effektive status mangler".into()),
        }
    } else if same_vat_window(data) {
        text(&data.vat, "periodStatus")?;
        Ok(data.vat["periodStatus"].as_str().unwrap())
    } else {
        Err("Vælg en faktisk VAT-periode eller den servervalgte momsperiode".into())
    }
}
fn same_vat_window(data: &Data) -> bool {
    data.vat["vatRegistered"] == true
        && data
            .target
            .as_ref()
            .is_some_and(|b| Bounds::from(&data.vat).ok().as_ref() == Some(b))
}
#[derive(Clone)]
struct Proof {
    id: i64,
    packet: Value,
    vat: Value,
    profile: Value,
    principal: String,
    ledger: String,
}
fn valid_proof(proof: &Proof, data: &Data) -> bool {
    same_vat_window(data)
        && data.principal == proof.principal
        && data.ledger == proof.ledger
        && data.profile == proof.profile
        && data.vat == proof.vat
        && data.packet.as_ref() == Some(&proof.packet)
        && state_of(data).ok() == Some("open")
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Review,
    Close,
    Reopen,
    Report,
}
impl Action {
    fn name(self) -> &'static str {
        match self {
            Self::Review => "review",
            Self::Close => "close",
            Self::Reopen => "reopen",
            Self::Report => "report",
        }
    }
    fn sdk(self) -> PeriodMutation {
        match self {
            Self::Review => PeriodMutation::Review,
            Self::Close => PeriodMutation::Close,
            Self::Reopen => PeriodMutation::Reopen,
            Self::Report => PeriodMutation::Report,
        }
    }
}
#[derive(Clone)]
struct Review {
    action: Action,
    data: Data,
    proof: Option<Proof>,
    body: Vec<u8>,
    detail: Option<String>,
}
fn request(
    action: Action,
    data: &Data,
    proof: Option<&Proof>,
    detail: &str,
    external: bool,
) -> Result<Review, String> {
    let bounds = data.target.as_ref().ok_or("Ingen periode er valgt")?;
    let mut body = json!({"periodStart":bounds.start,"periodEnd":bounds.end,"confirm":true});
    match action {
        Action::Review => {
            if !same_vat_window(data) || state_of(data)? != "open" {
                return Err("Review kræver den åbne momsperiode, hvis præcise tal serveren viser. Vælg korrekt år/serverperiode.".into());
            }
            let p = data.packet.as_ref().ok_or("Kontrolpakken mangler")?;
            packet(p, bounds)?;
        }
        Action::Close => {
            let proof =
                proof.ok_or("Registrér først et særskilt review af præcis denne kontrolpakke")?;
            if !valid_proof(proof, data) || proof.packet["blockers"] != 0 {
                return Err("Review er ændret eller kontroller blokerer. Løs årsagerne, genlæs og gennemgå et nyt review. Tvangslukning tilbydes ikke.".into());
            }
            body["kind"] = json!("vat_period");
            body["packetHash"] = proof.packet["hash"].clone();
            body["reviewId"] = json!(proof.id);
        }
        Action::Reopen => {
            if state_of(data)? != "closed" {
                return Err("Kun en lukket, ikke indberettet periode kan genåbnes".into());
            }
            if detail.trim().is_empty()
                || detail.len() > 4096
                || detail.chars().any(char::is_control)
            {
                return Err("Skriv en konkret begrundelse (højst4096 tegn)").map_err(str::to_owned);
            }
            body["kind"] = json!("vat_period");
            body["reason"] = json!(detail.trim());
        }
        Action::Report => {
            if !external
                || state_of(data)? != "closed"
                || detail.trim().is_empty()
                || detail.len() > 4096
                || detail.chars().any(char::is_control)
            {
                return Err("Bekræft en allerede gennemført ekstern indberetning og angiv dens faktiske kvitteringsreference. Kun lukket, ikke indberettet periode kan ændres.".into());
            }
            let original = &row(data).ok_or("Lukket periode mangler")?["closePacketHash"];
            if !digest(original) {
                return Err("Periodens oprindelige lukkebevis mangler. En ny kontrolpakke kan ikke erstatte det.".into());
            }
            body["kind"] = json!("vat_period");
            body["packetHash"] = original.clone();
            body["reference"] = json!(detail.trim());
        }
    }
    Ok(Review {
        action,
        data: data.clone(),
        proof: proof.cloned(),
        body: serde_json::to_vec(&body).map_err(|_| "Handlingen kunne ikke fastholdes")?,
        detail: matches!(action, Action::Reopen | Action::Report).then(|| detail.trim().into()),
    })
}
fn current(review: &Review, data: &Data) -> bool {
    let original = &review.data;
    if data.principal != original.principal
        || data.ledger != original.ledger
        || data.profile != original.profile
        || data.target != original.target
    {
        return false;
    }
    match review.action {
        Action::Review | Action::Close => {
            data.vat == original.vat
                && data.packet == original.packet
                && request(review.action, data, review.proof.as_ref(), "", false).is_ok()
        }
        Action::Reopen | Action::Report => {
            row(data) == row(original)
                && request(
                    review.action,
                    data,
                    None,
                    review.detail.as_deref().unwrap_or(""),
                    review.action == Action::Report,
                )
                .is_ok()
        }
    }
}
fn marker(review: &Review, origin: &str, company: &str, local: Option<&str>) -> Value {
    let d = &review.data;
    let b = d.target.as_ref().unwrap();
    json!({"format":"rentemester-desktop-periods/1","origin":origin,"company":company,"principal":d.principal,"ledger":d.ledger,"localIdentity":local,"operation":review.action.name(),"periodStart":b.start,"periodEnd":b.end,"periodId":row(d).map(|r|r["id"].clone()),"packetHash":if review.action==Action::Report{row(d).map(|r|r["closePacketHash"].clone())}else{d.packet.as_ref().map(|p|p["hash"].clone())},"reviewId":review.proof.as_ref().map(|p|p.id)})
}
fn scope(record: &Value, origin: &str, company: &str, data: &Data, local: Option<&str>) -> bool {
    record["company"] == company
        && record["principal"] == data.principal
        && record["ledger"] == data.ledger
        && record["localIdentity"].as_str() == local
        && (record["origin"] == origin || local.is_some())
        && data
            .target
            .as_ref()
            .is_some_and(|b| record["periodStart"] == b.start && record["periodEnd"] == b.end)
}
struct Receipt {
    proof: Option<Proof>,
    period: Option<i64>,
    message: String,
}
fn receipt(value: &Value, review: &Review) -> Option<Receipt> {
    if value["ok"] != true {
        return None;
    }
    let bounds = review.data.target.as_ref()?;
    if review.action == Action::Review {
        let r = &value["review"];
        let p = &r["packet"];
        let id = r["id"].as_i64().filter(|id| *id > 0)?;
        packet(p, bounds).ok()?;
        if r["reviewerPrincipal"]["subjectId"] != review.data.principal
            || r["reviewerPrincipal"]["kind"] != "user"
        {
            return None;
        }
        let proof = if review.data.packet.as_ref() == Some(p) {
            Some(Proof {
                id,
                packet: p.clone(),
                vat: review.data.vat.clone(),
                profile: review.data.profile.clone(),
                principal: review.data.principal.clone(),
                ledger: review.data.ledger.clone(),
            })
        } else {
            None
        };
        let message = if proof.is_some() {
            format!(
                "Review{id} er registreret for præcis kontrolpakke {}. Perioden er stadig åben; lukning kræver separat bekræftelse.",
                p["hash"].as_str()?
            )
        } else {
            "Review blev registreret, men serverpakken ændrede sig under handlingen. Den nye pakke skal genlæses og gennemgås; ingen lukkegodkendelse er overtaget.".into()
        };
        Some(Receipt {
            proof,
            period: None,
            message,
        })
    } else {
        let p = &value["period"];
        if Bounds::from(p).ok().as_ref() != Some(bounds)
            || !matches!(p["kind"].as_str(), Some("vat_period" | "vat_quarter"))
        {
            return None;
        }
        let state = if review.action == Action::Reopen {
            p["effectiveStatus"].as_str()?
        } else {
            p["status"].as_str()?
        };
        if state
            != match review.action {
                Action::Close => "closed",
                Action::Reopen => "open",
                Action::Report => "reported",
                _ => return None,
            }
        {
            return None;
        }
        if review.action == Action::Report && p["reference"].as_str() != review.detail.as_deref() {
            return None;
        }
        if review.action == Action::Reopen && p["reason"].as_str() != review.detail.as_deref() {
            return None;
        }
        let id = p["id"].as_i64().filter(|id| *id > 0)?;
        Some(Receipt {
            proof: None,
            period: Some(id),
            message: format!(
                "{} er bekræftet for periode{id}: {} til {} · status {state}. {}",
                review.action.name(),
                bounds.start,
                bounds.end,
                if review.action == Action::Report {
                    "Kun brugerens eksterne kvittering er registreret. Ingen indberetning blev sendt."
                } else {
                    "Genlæs den kanoniske periode og momstal."
                }
            ),
        })
    }
}
fn readback(receipt: &Receipt, review: &Review, data: &Data) -> bool {
    if data.principal != review.data.principal
        || data.ledger != review.data.ledger
        || data.target != review.data.target
    {
        return false;
    }
    if review.action == Action::Review {
        return receipt.proof.as_ref().is_some_and(|p| valid_proof(p, data));
    }
    let Some(r) = row(data) else {
        return false;
    };
    if r["id"].as_i64() != receipt.period {
        return false;
    }
    match review.action {
        Action::Close => {
            r["effectiveStatus"] == "closed"
                && r["closePacketHash"]
                    == review
                        .proof
                        .as_ref()
                        .map(|p| p.packet["hash"].clone())
                        .unwrap_or(Value::Null)
        }
        Action::Reopen => r["effectiveStatus"] == "open",
        Action::Report => {
            r["effectiveStatus"] == "reported"
                && r["reference"].as_str() == review.detail.as_deref()
                && r["closePacketHash"]
                    == row(&review.data)
                        .map(|r| r["closePacketHash"].clone())
                        .unwrap_or(Value::Null)
        }
        _ => false,
    }
}
/// Read-only leaf projection keeps every returned control/evidence field visible, without inferred values.
fn leaves(value: &Value, prefix: &str) -> Vec<String> {
    match value {
        Value::Object(fields) => fields
            .iter()
            .flat_map(|(k, v)| {
                leaves(
                    v,
                    &if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    },
                )
            })
            .collect(),
        Value::Array(items) => {
            if items.is_empty() {
                vec![format!("{prefix}: []")]
            } else {
                items
                    .iter()
                    .enumerate()
                    .flat_map(|(i, v)| leaves(v, &format!("{prefix}[{i}]")))
                    .collect()
            }
        }
        Value::String(s) => vec![format!("{prefix}: {s}")],
        _ => vec![format!("{prefix}: {value}")],
    }
}

pub struct Periods {
    session: Arc<Session>,
    origin: String,
    company: String,
    year: String,
    local: Option<String>,
    data: Option<Data>,
    proof: Option<Proof>,
    review: Option<Review>,
    reason: Entity<InputState>,
    reference: Entity<InputState>,
    controls_checked: bool,
    reviewed: bool,
    external_done: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
}
impl Periods {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        company: String,
        year: String,
        local: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            session,
            origin,
            company,
            year,
            local,
            data: None,
            proof: None,
            review: None,
            reason: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Konkret begrundelse for genåbning")
            }),
            reference: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Faktisk ekstern kvitteringsreference")
            }),
            controls_checked: false,
            reviewed: false,
            external_done: false,
            pending: None,
            resolved: false,
            busy: false,
            status: String::new(),
        };
        this.load(None, window, cx);
        this
    }
    fn reset(&mut self) {
        self.proof = None;
        self.review = None;
        self.controls_checked = false;
        self.reviewed = false;
        self.external_done = false;
        self.resolved = false;
    }
    fn load(&mut self, target: Option<Bounds>, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.data = None;
        self.reset();
        self.status = "Henter kanoniske momstal, perioder og fuld kontrolpakke…".into();
        self.reason.update(cx, |i, cx| i.set_value("", window, cx));
        self.reference
            .update(cx, |i, cx| i.set_value("", window, cx));
        let session = self.session.clone();
        let company = self.company.clone();
        let year = self.year.clone();
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((read(&session, &company, &year, target)?, state::load()?))
        });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,_,cx|{
            this.busy=false;match result{Ok((data,pending))=>{this.data=Some(data);this.pending=pending;
            this.status="Beløb, frister, kontroller og periodestatus kommer fra serveren. Review lukker ikke perioden. Ekstern indberetning foretages ikke her.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn prepare(&mut self, action: Action, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        if matches!(action, Action::Review | Action::Close) && !self.controls_checked {
            self.status =
                "Gennemgå alle viste momstal, blokeringer, advarsler og evidens før review/lukning"
                    .into();
            cx.notify();
            return;
        }
        let detail = match action {
            Action::Reopen => self.reason.read(cx).value().to_string(),
            Action::Report => self.reference.read(cx).value().to_string(),
            _ => String::new(),
        };
        match request(
            action,
            data,
            self.proof.as_ref(),
            &detail,
            self.external_done,
        ) {
            Ok(review) => {
                self.review = Some(review);
                self.reviewed = false;
                self.status = "Gennemgå den særskilte handling nedenfor. Intet er sendt.".into();
            }
            Err(e) => self.status = e,
        }
        cx.notify();
    }
    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.reviewed || self.pending.is_some() {
            return;
        }
        let Some(review) = self.review.clone() else {
            return;
        };
        self.busy = true;
        self.data = None;
        self.reset();
        self.status = "Kontrollerer aktuelle kilder før én bekræftet handling…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let year = self.year.clone();
        let origin = self.origin.clone();
        let local = self.local.clone();
        let task=cx.background_executor().spawn(async move {
            let target=review.data.target.clone();let fresh=read(&session,&company,&year,target.clone())?;
            if !current(&review,&fresh){return Err("Bruger, ledger, moms, periode eller kontrolbevis ændrede sig. Intet sendt; genlæs og gennemgå igen.".into());}
            let record=marker(&review,&origin,&company,local.as_deref());state::begin(&record)?;
            let result=session.runtime.block_on(session.client.period_mutation(&company,review.action.sdk(),review.body.clone(),true));
            let(message,confirmed)=match result {
                Ok(value)=>if let Some(r)=receipt(&value,&review){(r.message.clone(),Some(r))}else{("Serverkvitteringen er ufuldstændig. Handlingen kan være registreret; ingen genafsendelse. Genlæs og afklar manuelt.".into(),None)},
                Err(Error::Refused{status})=>{state::finish(&record,false)?;(format!("Serveren afviste handlingen (HTTP{status}). Kontrollér adgang, CVR/momskadence, dato, blokeringer, review og periodestatus. Ingen tvang eller automatisk gentagelse."),None)},
                Err(Error::InvalidInput(_))=>{state::finish(&record,false)?;("Felter blev afvist før netværkskald. Genlæs og gennemgå igen.".into(),None)},
                _=>("Udfaldet er uafklaret. Genlæs perioder og moms og afklar den præcise handling manuelt. Ingen genafsendelse eller automatisk overtagelse af review/kvittering.".into(),None)
            };
            let next=read(&session,&company,&year,target);let mut proof=None;
            let message=if let(Some(receipt),Ok(data))=(&confirmed,&next){if readback(receipt,&review,data){state::finish(&record,false)?;proof=receipt.proof.clone();format!("{message} Kanonisk periodestatus og aktuelle servermomstal er læst tilbage.")}else{format!("{message} Genlæsning bekræftede ikke hele forventningen; gennemgå perioder. Intet nyt lukkebevis overtages.")}}else{message};
            Ok::<_,String>((message,proof,next,state::load()?))
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, _, cx| {
                this.busy = false;
                match result {
                    Ok((message, proof, data, pending)) => {
                        this.status = message;
                        this.proof = proof;
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
impl Render for Periods {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut view = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_xl()
                    .child("Moms og perioder · review, lukning og genåbning"),
            )
            .child(format!("{} · {}", self.company, self.origin))
            .child(self.status.clone())
            .child(
                Button::new("periods-refresh")
                    .label("Genlæs valgt periode og momstal (nyt review kræves)")
                    .disabled(self.busy || self.review.is_some())
                    .on_click(cx.listener(|this, _, window, cx| {
                        let target = this.data.as_ref().and_then(|d| d.target.clone());
                        this.load(target, window, cx);
                    })),
            )
            .child(
                Button::new("periods-current-vat")
                    .label("Vælg serverens aktuelle momsperiode for det valgte år")
                    .disabled(self.busy || self.review.is_some())
                    .on_click(cx.listener(|this, _, window, cx| this.load(None, window, cx))),
            );
        let Some(data) = self.data.clone() else {
            return view;
        };
        view = view.child(format!(
            "CVR: {} · momskadence: {} · bruger {}",
            data.profile["cvr"].as_str().unwrap_or("ikke oplyst"),
            data.profile["vatPeriodType"]
                .as_str()
                .unwrap_or("ikke oplyst"),
            data.principal
        ));
        if data.vat["vatRegistered"] == true {
            view = view.child(
                div()
                    .text_lg()
                    .child("Serverens momsopgørelse · kun den angivne periode"),
            );
            for key in [
                "periodLabel",
                "periodStart",
                "periodEnd",
                "deadline",
                "daysRemaining",
                "periodStatus",
                "momsangivelseReady",
                "periodActionsAvailable",
                "outputVat",
                "outputVatAdjustment",
                "inputVat",
                "payable",
                "rubrikker",
                "vatReportErrors",
                "vatReportWarnings",
            ] {
                for line in leaves(&data.vat[key], key) {
                    view = view.child(line);
                }
            }
            if !same_vat_window(&data) {
                view=view.child("Momstallene ovenfor tilhører serverens momsperiode, ikke den særskilt valgte historiske periode. Vælg korrekt år/serverperiode for nyt review og lukning.");
            }
        } else {
            view=view.child("Virksomheden er ikke momsregistreret ifølge serveren. Intet momsreview eller lukning tilbydes.");
        }
        view = view.child(
            div()
                .text_lg()
                .child("Kanoniske perioder · effektiv status"),
        );
        for p in data.periods["periods"].as_array().into_iter().flatten() {
            for line in leaves(p, "periode") {
                view = view.child(line);
            }
            if matches!(p["kind"].as_str(), Some("vat_period" | "vat_quarter")) {
                if let Ok(bounds) = Bounds::from(p) {
                    let id = p["id"].to_string();
                    view = view.child(
                        Button::new(SharedString::from(format!("periods-select-{id}")))
                            .label(format!(
                                "Vælg periode{id} · {} til {}",
                                bounds.start, bounds.end
                            ))
                            .disabled(self.busy || self.review.is_some())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.load(Some(bounds.clone()), window, cx)
                            })),
                    );
                }
            }
        }
        if let Some(record) = self.pending.clone() {
            view = view
                .child(
                    div()
                        .text_lg()
                        .child("Uafklaret handling · ingen genafsendelse"),
                )
                .child(format!(
                    "{} · {} · {} til {} · oprindeligt endpoint {}",
                    record["company"],
                    record["operation"],
                    record["periodStart"],
                    record["periodEnd"],
                    record["origin"]
                ));
            if let Ok(bounds) = Bounds::from(&record) {
                view = view.child(
                    Button::new("periods-pending-select")
                        .label("Genlæs den uafklarede periodes aktuelle status og evidens")
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.load(Some(bounds.clone()), window, cx)
                        })),
                );
            }
            if scope(
                &record,
                &self.origin,
                &self.company,
                &data,
                self.local.as_deref(),
            ) {
                view=view.child("Afklar den præcise handling manuelt. En matchende projektion beviser ikke alene forsøgets udfald. Lokalt endpoint kan være ændret; profil, ledger, bruger og periode er kontrolleret. Arkivering sender intet.")
                    .child(Checkbox::new("periods-resolved-check").label("Jeg har manuelt afklaret handlingen i den viste virksomhed og periode").checked(self.resolved).on_click(cx.listener(|this,checked,_,cx|{this.resolved=*checked;cx.notify();})))
                    .child(Button::new("periods-resolve").label("Arkivér manuelt afklaret status uden genafsendelse").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{
                        let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};
                        if this.busy||!this.resolved||!scope(record,&this.origin,&this.company,data,this.local.as_deref()){return;}
                        match state::finish(record,true){Ok(())=>{this.pending=None;this.reset();this.status="Afklaring arkiveret. Ingen handling sendt. Genlæs før et nyt selvstændigt review.".into();},Err(e)=>this.status=e}cx.notify();
                    })));
            } else {
                view=view.child("Skift til oprindelig bruger, virksomhed og profil/endpoint og vælg den præcise periode for manuel afklaring.");
            }
            return view;
        }
        if let Some(bounds) = &data.target {
            view = view.child(div().text_lg().child(format!(
                "Valgt periode {} til {} · {}",
                bounds.start,
                bounds.end,
                state_of(&data).unwrap_or("ukendt")
            )));
        }
        if let Some(packet) = &data.packet {
            view = view.child(
                div()
                    .text_lg()
                    .child("Fuld serverkontrolpakke · alle blokeringer, advarsler og evidens"),
            );
            for line in leaves(packet, "kontrolpakke") {
                view = view.child(line);
            }
        }
        if let Some(review) = &self.review {
            view = view.child(div().text_lg().child(format!(
                "Bekræft særskilt handling: {}",
                review.action.name()
            )));
            for line in leaves(
                &serde_json::from_slice::<Value>(&review.body).unwrap_or(Value::Null),
                "anmodning",
            ) {
                view = view.child(line);
            }
            return view.child("Anmodningen ovenfor er fastholdt. Serveren validerer igen ved bekræftelse. Ingen ekstern indberetning udføres.")
                .child(Checkbox::new("periods-confirm-check").label("Jeg bekræfter præcis denne særskilte handling og viste periode/bevis").checked(self.reviewed).disabled(self.busy).on_click(cx.listener(|this,checked,_,cx|{this.reviewed=*checked;cx.notify();})))
                .child(Button::new("periods-confirm").label("Bekræft og send den gennemgåede handling én gang").disabled(self.busy||!self.reviewed).on_click(cx.listener(|this,_,window,cx|this.confirm(window,cx))))
                .child(Button::new("periods-cancel").label("Annullér før afsendelse").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.reviewed=false;cx.notify();})));
        }
        if data.packet.is_none() {
            return view.child("Kontrolpakken er ikke tilgængelig med denne adgang eller uden en momsperiode. Momstal og perioder vises som læsning; handlinger kræver aktuelle kontroller.");
        }
        match state_of(&data) {
            Ok("open") if same_vat_window(&data) => {
                view = view
                    .child(
                        Checkbox::new("periods-controls-reviewed")
                            .label("Jeg har gennemgået momsopgørelsen og hele kontrolpakken")
                            .checked(self.controls_checked)
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, checked, _, cx| {
                                this.controls_checked = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("periods-review")
                            .label("Gennemgå og registrér review")
                            .disabled(self.busy || !self.controls_checked)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.prepare(Action::Review, cx)),
                            ),
                    );
                if let Some(proof) = self
                    .proof
                    .as_ref()
                    .filter(|proof| valid_proof(proof, &data))
                {
                    view = view
                        .child(format!(
                            "Aktuelt review {} · {}",
                            proof.id, proof.packet["hash"]
                        ))
                        .child(
                            Button::new("periods-close")
                                .label("Gennemgå særskilt lukning af perioden")
                                .disabled(
                                    self.busy
                                        || !self.controls_checked
                                        || proof.packet["blockers"] != 0,
                                )
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.prepare(Action::Close, cx)),
                                ),
                        );
                }
            }
            Ok("closed") => {
                view = view.child(div().text_lg().child("Genåbn med begrundelse"))
                    .child(Input::new(&self.reason).disabled(self.busy))
                    .child(Button::new("periods-reopen").label("Gennemgå genåbning")
                        .disabled(self.busy).on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Reopen, cx))))
                    .child(div().text_lg().child("Registrér en faktisk ekstern indberetningskvittering"))
                    .child("Indberet først uden for Rentemester. Her registrerer du kun kvitteringen; der sendes intet til myndighederne.")
                    .child(Input::new(&self.reference).disabled(self.busy))
                    .child(Checkbox::new("periods-external-done").label("Indberetningen er allerede gennemført, og dette er dens faktiske kvitteringsreference")
                        .checked(self.external_done).disabled(self.busy)
                        .on_click(cx.listener(|this, checked, _, cx| { this.external_done = *checked; cx.notify(); })))
                    .child(Button::new("periods-report").label("Gennemgå registrering af ekstern kvittering")
                        .disabled(self.busy || !self.external_done)
                        .on_click(cx.listener(|this, _, _, cx| this.prepare(Action::Report, cx))));
            }
            Ok("reported") => {
                view = view.child("Den eksterne kvittering er registreret. Perioden kan ikke genåbnes eller få kvitteringen ændret her.");
            }
            Err(error) => {
                view = view.child(error);
            }
            _ => {}
        }
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn data(status: &str) -> Data {
        let bounds = Bounds {
            start: "2026-01-01".into(),
            end: "2026-03-31".into(),
        };
        Data {
            vat: json!({"vatRegistered":true,"periodStart":bounds.start,"periodEnd":bounds.end,"periodStatus":status,"payable":12.34}),
            periods: json!({"periods":if status=="open" {json!([])} else {json!([{"id":7,"kind":"vat_period","periodStart":bounds.start,"periodEnd":bounds.end,"effectiveStatus":status,"closePacketHash":"b".repeat(64),"reference":if status=="reported" {"synthetic-filing-receipt"} else {""}}])}}),
            profile: json!({"cvr":"12345678","vatPeriodType":"quarter"}),
            packet: Some(
                json!({"periodStart":bounds.start,"periodEnd":bounds.end,"hash":"a".repeat(64),"blockers":0,"warnings":0,"controlsRun":["synthetic"],"items":[{"code":"synthetic","status":"passed","count":0,"amount":0,"waivable":false,"sourceHash":"c".repeat(64),"evidence":[]}]}),
            ),
            target: Some(bounds),
            principal: "synthetic-owner".into(),
            ledger: "synthetic-ledger".into(),
        }
    }
    fn proof(data: &Data) -> Proof {
        Proof {
            id: 11,
            packet: data.packet.clone().unwrap(),
            vat: data.vat.clone(),
            profile: data.profile.clone(),
            principal: data.principal.clone(),
            ledger: data.ledger.clone(),
        }
    }
    #[::core::prelude::v1::test]
    fn close_requires_separate_current_review_and_rejects_changed_sources_or_blockers() {
        let mut d = data("open");
        let p = proof(&d);
        assert!(request(Action::Close, &d, None, "", false).is_err());
        let close = request(Action::Close, &d, Some(&p), "", false).unwrap();
        let body: Value = serde_json::from_slice(&close.body).unwrap();
        assert_eq!(body["reviewId"], 11);
        assert_eq!(body["packetHash"], "a".repeat(64));
        d.vat["payable"] = json!(56.78);
        assert!(!current(&close, &d));
        d = data("open");
        d.packet.as_mut().unwrap()["blockers"] = json!(1);
        assert!(request(Action::Close, &d, Some(&proof(&d)), "", false).is_err());
    }
    #[::core::prelude::v1::test]
    fn filing_uses_original_close_hash_and_requires_actual_external_receipt() {
        let d = data("closed");
        assert!(request(Action::Report, &d, None, "synthetic-filing-receipt", false).is_err());
        assert!(request(Action::Report, &d, None, "", true).is_err());
        let review = request(Action::Report, &d, None, "synthetic-filing-receipt", true).unwrap();
        let body: Value = serde_json::from_slice(&review.body).unwrap();
        assert_eq!(body["packetHash"], "b".repeat(64));
        assert_ne!(body["packetHash"], d.packet.as_ref().unwrap()["hash"]);
        assert!(
            request(
                Action::Reopen,
                &data("reported"),
                None,
                "synthetic reason",
                false
            )
            .is_err()
        );
    }
    #[::core::prelude::v1::test]
    fn receipt_readback_cannot_cross_principal_ledger_or_original_close_evidence() {
        let closed = data("closed");
        let review = request(
            Action::Report,
            &closed,
            None,
            "synthetic-filing-receipt",
            true,
        )
        .unwrap();
        let mut canonical = data("reported");
        let value = json!({"ok":true,"period":{"id":7,"kind":"vat_period","periodStart":"2026-01-01","periodEnd":"2026-03-31","status":"reported","reference":"synthetic-filing-receipt"}});
        let receipt = receipt(&value, &review).unwrap();
        assert!(readback(&receipt, &review, &canonical));
        canonical.principal = "another-owner".into();
        assert!(!readback(&receipt, &review, &canonical));
        canonical = data("reported");
        canonical.ledger = "another-ledger".into();
        assert!(!readback(&receipt, &review, &canonical));
        canonical = data("reported");
        canonical.periods["periods"][0]["closePacketHash"] = json!("f".repeat(64));
        assert!(!readback(&receipt, &review, &canonical));
    }
    #[::core::prelude::v1::test]
    fn local_restart_allows_new_port_only_for_same_profile_subject_ledger_and_period() {
        let d = data("open");
        let review = request(Action::Review, &d, None, "", false).unwrap();
        let record = marker(
            &review,
            "http://127.0.0.1:1001",
            "synthetic",
            Some("same-profile"),
        );
        assert!(scope(
            &record,
            "http://127.0.0.1:1002",
            "synthetic",
            &d,
            Some("same-profile")
        ));
        assert!(!scope(
            &record,
            "http://127.0.0.1:1002",
            "synthetic",
            &d,
            Some("other-profile")
        ));
        let remote = marker(&review, "https://first.example.invalid", "synthetic", None);
        assert!(!scope(
            &remote,
            "https://second.example.invalid",
            "synthetic",
            &d,
            None
        ));
    }
    #[::core::prelude::v1::test]
    fn legacy_alias_alone_works_but_duplicate_period_selection_is_ambiguous() {
        let mut d = data("closed");
        d.periods["periods"][0]["kind"] = json!("vat_quarter");
        assert!(request(Action::Reopen, &d, None, "synthetic reason", false).is_ok());
        let mut other = d.periods["periods"][0].clone();
        other["id"] = json!(8);
        other["kind"] = json!("vat_period");
        d.periods["periods"].as_array_mut().unwrap().push(other);
        assert!(row(&d).is_none());
        assert!(state_of(&d).is_err());
        assert!(request(Action::Reopen, &d, None, "synthetic reason", false).is_err());
        assert!(request(Action::Report, &d, None, "synthetic external receipt", true).is_err());
    }
}
