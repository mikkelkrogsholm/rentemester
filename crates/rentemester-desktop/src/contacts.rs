//! Canonical customer/vendor forms. Never restore erased names or retry uncertain writes.
use crate::{Session, bank_state, readable};
use gpui_kit::{
    component::{
        Disableable,
        button::Button,
        checkbox::Checkbox,
        input::{Input, InputState},
    },
    *,
};
use rentemester_client::{Confirmation, DomainMutation, Error, Report};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::PathBuf,
    sync::Arc,
};
const COMMON: &[(&str, &str)] = &[
    ("name", "Navn (påkrævet)"),
    ("address", "Adresse"),
    ("vatOrCvr", "CVR / momsnummer"),
    ("email", "E-mail"),
    ("phone", "Telefon"),
    ("website", "Hjemmeside"),
    ("notes", "Noter"),
];
const CUSTOMER: &[(&str, &str)] = &[
    ("paymentTermsDays", "Betalingsfrist i dage (1–365)"),
    ("defaultCurrency", "Standardvaluta (ISO, fx DKK)"),
];
const VENDOR: &[(&str, &str)] = &[
    ("countryCode", "Landekode (fx DK)"),
    (
        "identifierKind",
        "Identitetstype (dk_cvr, eu_vat eller non_eu)",
    ),
    (
        "defaultExpenseAccount",
        "Standard omkostningskonto (kontonummer)",
    ),
    (
        "defaultVatTreatment",
        "Standard momsbehandling (standard, exempt, non_deductible)",
    ),
];
fn fields(vendor: bool) -> impl Iterator<Item = (&'static str, &'static str)> {
    COMMON
        .iter()
        .chain(if vendor { VENDOR } else { CUSTOMER })
        .copied()
}
fn text(v: &Value, key: &str) -> Result<String, String> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Serveren mangler {key}"))
}
fn editable(v: &Value) -> bool {
    v["gdprErased"] == false && v["archived"] != true && v["id"].as_i64().is_some_and(|id| id > 0)
}
#[derive(Clone)]
struct Data {
    contacts: Value,
    principal: String,
    ledger: String,
}
impl Data {
    fn rows(&self, vendor: bool) -> &[Value] {
        self.contacts[if vendor { "vendors" } else { "customers" }]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
    fn selected(&self, vendor: bool, id: i64) -> Option<&Value> {
        self.rows(vendor).iter().find(|v| v["id"] == id)
    }
}
fn read(session: &Session, company: &str) -> Result<Data, String> {
    let root = session
        .runtime
        .block_on(session.client.contacts(company))
        .map_err(readable)?;
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let journal = session
        .runtime
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    if [&root, &me, &journal].iter().any(|v| v["ok"] != true) {
        return Err("Serveren bekræftede ikke kontaktgrundlaget".into());
    }
    let contacts = root["contacts"].clone();
    if text(&contacts, "slug")? != company || text(&journal["journal"], "slug")? != company {
        return Err("Kontaktgrundlaget tilhører en anden virksomhed".into());
    }
    if contacts["customers"].as_array().is_none() || contacts["vendors"].as_array().is_none() {
        return Err("Serveren mangler kunders eller leverandørers kanoniske liste".into());
    }
    Ok(Data {
        contacts,
        principal: text(&me["user"], "id")?,
        ledger: text(&journal["journal"], "ledgerUuid")?,
    })
}
fn body(vendor: bool, values: &[(String, String)], updating: bool) -> Result<Vec<u8>, String> {
    let mut result = serde_json::Map::new();
    for (key, _) in fields(vendor) {
        let value = values
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.trim())
            .unwrap_or("");
        if key == "name" && value.is_empty() {
            return Err("Skriv kontaktens navn".into());
        }
        if key == "defaultCurrency" && value.is_empty() {
            return Err(if updating {
                "Standardvaluta kan ikke tømmes"
            } else {
                "Skriv kundens standardvaluta som ISO-kode, fx DKK"
            }
            .into());
        }
        let value = if key == "paymentTermsDays" && !value.is_empty() {
            json!(
                value
                    .parse::<i64>()
                    .ok()
                    .filter(|n| (1..=365).contains(n))
                    .ok_or("Betalingsfrist skal være et heltal mellem1 og365")?
            )
        } else if value.is_empty() {
            Value::Null
        } else {
            json!(value)
        };
        result.insert(key.into(), value);
    }
    serde_json::to_vec(&Value::Object(result))
        .map_err(|_| "Kontaktanmodningen kunne ikke fastholdes".into())
}
#[derive(Clone)]
struct Review {
    vendor: bool,
    delete: bool,
    source: Option<Value>,
    body: Vec<u8>,
}
impl Review {
    fn operation(&self) -> Result<DomainMutation, String> {
        let id = self.source.as_ref().and_then(|v| v["id"].as_i64());
        Ok(match (self.vendor, self.delete, id) {
            (false, false, None) => DomainMutation::CreateCustomer,
            (true, false, None) => DomainMutation::CreateVendor,
            (false, false, Some(id)) => DomainMutation::UpdateCustomer(id),
            (true, false, Some(id)) => DomainMutation::UpdateVendor(id),
            (false, true, Some(id)) => DomainMutation::DeleteCustomer(id),
            (true, true, Some(id)) => DomainMutation::DeleteVendor(id),
            _ => return Err("Vælg kontakten før sletning".into()),
        })
    }
    fn action(&self) -> &'static str {
        if self.delete {
            "delete"
        } else if self.source.is_some() {
            "update"
        } else {
            "create"
        }
    }
}
fn source_current(review: &Review, data: &Data) -> bool {
    review.source.as_ref().is_none_or(|source| {
        source["id"]
            .as_i64()
            .is_some_and(|id| editable(source) && data.selected(review.vendor, id) == Some(source))
    })
}
fn marker(review: &Review, origin: &str, company: &str, data: &Data, local: Option<&str>) -> Value {
    // No cached identity/name/body: manual recovery never re-sends this request.
    json!({"format":"rentemester-desktop-contact/1","operation":review.action(),"vendor":review.vendor,"contactId":review.source.as_ref().map(|v|v["id"].clone()),"origin":origin,"company":company,"principal":data.principal,"ledger":data.ledger,"localIdentity":local})
}
fn same_scope(
    record: &Value,
    origin: &str,
    company: &str,
    data: &Data,
    local: Option<&str>,
) -> bool {
    record["localIdentity"].as_str() == local
        && bank_state::same_scope(
            record,
            origin,
            company,
            &data.principal,
            &data.ledger,
            local,
        )
}
fn state_path() -> Result<PathBuf, String> {
    let dir = crate::journal_attempt::directory();
    if !dir.is_absolute() {
        return Err("Kontaktstatus kræver en absolut privat mappe".into());
    }
    if !dir.exists() {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .map_err(|_| "Privat kontaktstatus kunne ikke oprettes")?;
    }
    let metadata = fs::symlink_metadata(&dir).map_err(|_| "Kontaktstatus kunne ikke undersøges")?;
    let user = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .map_err(|_| "Brugeridentitet kunne ikke læses")?;
    let uid = String::from_utf8_lossy(&user.stdout)
        .trim()
        .parse::<u32>()
        .map_err(|_| "Brugeridentitet kunne ikke læses")?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.mode() & 0o077 != 0
        || metadata.uid() != uid
        || dir
            .canonicalize()
            .map_err(|_| "Kontaktstatus kunne ikke åbnes")?
            != dir
    {
        return Err("Kontaktstatus kræver en kanonisk ejet mappe0700".into());
    }
    let path = dir.join("pending-contact.json");
    for item in [&dir, &path] {
        if !item
            .try_exists()
            .map_err(|_| "Kontaktstatus kunne ikke undersøges")?
        {
            continue;
        }
        let metadata =
            fs::symlink_metadata(item).map_err(|_| "Kontaktstatus kunne ikke undersøges")?;
        let acl = std::process::Command::new("/bin/ls")
            .arg("-lde")
            .arg(item)
            .output()
            .map_err(|_| "Kontaktstatus adgang kunne ikke læses")?;
        if metadata.file_type().is_symlink()
            || metadata.uid() != uid
            || metadata.mode() & 0o077 != 0
            || !acl.status.success()
            || String::from_utf8_lossy(&acl.stdout)
                .lines()
                .skip(1)
                .any(|line| !line.trim().is_empty())
            || (item == &path && !metadata.is_file())
        {
            return Err(
                "Kontaktstatus kræver egne private filer uden udvidet adgang eller symbolske links"
                    .into(),
            );
        }
    }
    Ok(path)
}
fn pending() -> Result<Option<Value>, String> {
    let path = state_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(16385).read_to_end(&mut bytes))
        .map_err(|_| "Kontaktstatus kunne ikke læses")?;
    if bytes.len() > 16384 {
        return Err("Kontaktstatus overskrider grænsen; den bevares".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Kontaktstatus er ugyldig; den bevares")?;
    if value["format"] != "rentemester-desktop-contact/1"
        || !matches!(
            value["operation"].as_str(),
            Some("create" | "update" | "delete")
        )
    {
        return Err("Ukendt kontaktstatus bevares".into());
    }
    Ok(Some(value))
}
fn begin(record: &Value) -> Result<(), String> {
    let path = state_path()?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|_| "Et kontaktforsøg kræver afklaring før nye ændringer")?;
    file.write_all(record.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Kontaktstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(path.parent().ok_or("Statusmappen mangler")?)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Kontaktstatus kunne ikke synkroniseres; intet sendt".into())
}
fn finish(record: &Value, manual: bool) -> Result<(), String> {
    if pending()?.as_ref() != Some(record) {
        return Err("Kontaktstatus ændrede sig og bevares".into());
    }
    let path = state_path()?;
    if manual {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "Systemtid kunne ikke læses")?
            .as_nanos();
        let archive = path.with_file_name(format!("contact-manually-reconciled-{suffix}.json"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(archive)
            .map_err(|_| "Kontaktens afklaring kunne ikke arkiveres")?;
        file.write_all(record.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Kontaktens afklaring kunne ikke gemmes")?;
    }
    fs::remove_file(&path).map_err(|_| "Kontaktstatus kunne ikke afsluttes")?;
    fs::File::open(path.parent().ok_or("Statusmappen mangler")?)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Kontaktstatus kunne ikke synkroniseres".into())
}
fn receipt(value: &Value, review: &Review) -> Option<i64> {
    if value["ok"] != true {
        return None;
    }
    let row = &value[if review.vendor { "vendor" } else { "customer" }];
    let id = row["id"].as_i64().filter(|id| *id > 0)?;
    if review
        .source
        .as_ref()
        .is_some_and(|source| source["id"] != id)
        || review.delete && row["deleted"] != true
    {
        return None;
    }
    Some(id)
}
struct Field {
    key: &'static str,
    input: Entity<InputState>,
}
pub struct Contacts {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    data: Option<Data>,
    vendor: bool,
    selected: Option<Value>,
    fields: Vec<Field>,
    review: Option<Review>,
    reviewed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
}
impl Contacts {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        company: String,
        local: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let fields = COMMON
            .iter()
            .chain(CUSTOMER)
            .chain(VENDOR)
            .map(|(key, label)| Field {
                key,
                input: cx.new(|cx| InputState::new(window, cx).placeholder(*label)),
            })
            .collect();
        let mut this = Self {
            session,
            origin,
            company,
            local,
            data: None,
            vendor: false,
            selected: None,
            fields,
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
    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = None;
        self.review = None;
        self.reviewed = false;
        for field in &self.fields {
            field
                .input
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.clear(window, cx);
        self.data = None;
        self.busy = true;
        self.resolved = false;
        self.status = "Henter kanoniske kunder og leverandører…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let task = cx
            .background_executor()
            .spawn(async move { Ok::<_, String>((read(&session, &company)?, pending()?)) });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok((data,pending))=>{this.data=Some(data);this.pending=pending;this.status="Kontakter læses fra serveren. Stamdataændringer kræver dine aktuelle rettigheder.".into();},Err(error)=>this.status=error}cx.notify();});}).detach();
        cx.notify();
    }
    fn select(&mut self, value: Value, window: &mut Window, cx: &mut Context<Self>) {
        self.clear(window, cx);
        if !editable(&value) {
            self.status="Kontakten er GDPR-redigeret eller arkiveret og vises uden redigering. Beskyttet identitet genindføres ikke.".into();
            self.selected = Some(value);
            cx.notify();
            return;
        }
        for (key, _) in fields(self.vendor) {
            if let Some(field) = self.fields.iter().find(|field| field.key == key) {
                let value = if key == "paymentTermsDays" {
                    value[key]
                        .as_i64()
                        .map(|n| n.to_string())
                        .unwrap_or_default()
                } else {
                    value[key].as_str().unwrap_or("").to_owned()
                };
                field
                    .input
                    .update(cx, |input, cx| input.set_value(value, window, cx));
            }
        }
        self.selected = Some(value);
        cx.notify();
    }
    fn review(&mut self, delete: bool, cx: &mut Context<Self>) {
        if self.busy
            || self.pending.is_some()
            || self.selected.as_ref().is_some_and(|v| !editable(v))
        {
            return;
        }
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
        let bytes = if delete {
            if self.selected.is_none() {
                return;
            }
            Ok(br#"{"confirm":true}"#.to_vec())
        } else {
            body(self.vendor, &values, self.selected.is_some())
        };
        match bytes {
            Ok(body) => {
                self.review = Some(Review {
                    vendor: self.vendor,
                    delete,
                    source: self.selected.clone(),
                    body,
                });
                self.reviewed = false;
                self.status = "Gennemgå de præcise kontaktfelter før bekræftelse.".into();
            }
            Err(error) => self.status = error,
        }
        cx.notify();
    }
    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.reviewed || self.pending.is_some() {
            return;
        }
        let Some(review) = self.review.clone() else {
            return;
        };
        let Some(data) = self.data.clone() else {
            return;
        };
        let session = self.session.clone();
        let origin = self.origin.clone();
        let company = self.company.clone();
        let local = self.local.clone();
        self.clear(window, cx);
        self.data = None;
        self.busy = true;
        self.status = "Sender den gennemgåede kontaktændring én gang…".into();
        let task=cx.background_executor().spawn(async move{
            let fresh=read(&session,&company)?;
            if data.principal!=fresh.principal||data.ledger!=fresh.ledger{return Err("Bruger eller virksomhed ændrede sig; intet sendt".into());}
            if !source_current(&review,&fresh){return Err("Kontaktens kanoniske grundlag ændrede sig eller blev GDPR-redigeret. Intet sendt; genindlæs før ny gennemgang.".into());}
            let record=marker(&review,&origin,&company,&data,local.as_deref());begin(&record)?;
            let response=session.runtime.block_on(session.client.domain_mutation(&company,review.operation()?,review.body.clone(),Confirmation{confirm:true,invoice_preview_reviewed:false}));
            let message=match response{
                Ok(value)=>if let Some(id)=receipt(&value,&review){finish(&record,false)?;format!("Serveren bekræftede {} af {} id{id}. Listen genlæses kanonisk.",if review.delete{"sletning"}else if review.source.is_some(){"ændring"}else{"oprettelse"},if review.vendor{"leverandør"}else{"kunde"})}else{"Kontaktændringens udfald er uafklaret; kvitteringen mangler. Der sendes ikke igen.".into()},
                Err(Error::Refused{status})=>{finish(&record,false)?;if review.delete{format!("Sletning afvist (HTTP{status}). Kontakten er bevaret. Åbne fakturaer, leverandørgæld eller historiske relationer kan forhindre sletning; afklar forpligtelserne og kontrollér dine rettigheder på serveren.")}else{format!("Kontaktændring afvist (HTTP{status}). Kontrollér aktuelle stamdatarettigheder, identitetsfelter og eventuelle dubletter. Ingen gentagelse er sendt.")}},
                Err(Error::InvalidInput(_))=>{finish(&record,false)?;"Kontaktfelterne blev afvist lokalt før netværkskald. Genindlæs og ret felterne.".into()},
                _=>"Udfaldet er uafklaret. Kontakten kan være ændret; der sendes ikke igen. Genindlæs og afklar den korrekte virksomhed i browseren.".into(),
            };
            Ok::<_,String>((message,read(&session,&company),pending()?))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((message, data, pending)) => {
                        this.pending = pending;
                        this.status = message;
                        match data {
                            Ok(data) => this.data = Some(data),
                            Err(error) => this
                                .status
                                .push_str(&format!(" Kanonisk genlæsning fejlede: {error}")),
                        }
                    }
                    Err(error) => {
                        this.status = error;
                        this.pending = pending().ok().flatten();
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
impl Render for Contacts {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let blocked = self.busy || self.pending.is_some();
        let mut view = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_xl().child("Kontakter"))
            .child(format!("{} · {}", self.company, self.origin))
            .child(self.status.clone())
            .child(
                Button::new("contacts-refresh")
                    .label("Genindlæs kunder og leverandører")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.load(window, cx))),
            );
        let Some(data) = &self.data else {
            return view;
        };
        if let Some(record) = self.pending.clone() {
            view=view.child(format!("UAFKLARET {} af {} id{}. Der sendes ikke igen, og der dannes ingen erstatningskontakt. Kanonisk genlæsning er ikke i sig selv en kvittering.",record["operation"].as_str().unwrap_or("kontaktændring"),if record["vendor"]==true{"leverandør"}else{"kunde"},record["contactId"]))
                .child(format!("Oprindelig server {} · virksomhed {}",record["origin"].as_str().unwrap_or("?"),record["company"].as_str().unwrap_or("?")));
            if let Some(origin) = record["origin"].as_str() {
                let url = format!("{origin}/");
                view = view.child(
                    Button::new("contacts-browser")
                        .label("Åbn oprindelig server til manuel afklaring")
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                );
            }
            if same_scope(
                &record,
                &self.origin,
                &self.company,
                data,
                self.local.as_deref(),
            ) {
                view=view.child(Checkbox::new("contacts-manually-resolved").label("Jeg har afklaret udfaldet i den korrekte virksomhed. Arkivér status uden genafsendelse.").checked(self.resolved).on_click(cx.listener(|this,checked,_,cx|{this.resolved=*checked;cx.notify();})))
                    .child(Button::new("contacts-resolve").label("Bekræft manuel afklaring").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,window,cx|{
                        if !this.resolved||this.busy{return;}let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};
                        if !same_scope(record,&this.origin,&this.company,data,this.local.as_deref()){return;}
                        match finish(record,true){Ok(())=>{this.pending=None;this.resolved=false;this.clear(window,cx);this.status="Du har registreret manuel afklaring. Dette er ikke en serverkvittering.".into();},Err(error)=>this.status=error}cx.notify();
                    })));
            } else {
                view =
                    view.child("Åbn den oprindelige bruger og virksomhed for at afklare status.");
            }
        }
        let mut kinds = div().flex().gap_2();
        for (id, label, vendor) in [
            ("contacts-customers", "Kunder", false),
            ("contacts-vendors", "Leverandører", true),
        ] {
            kinds = kinds.child(
                Button::new(id)
                    .label(label)
                    .disabled(self.busy || self.review.is_some())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.vendor = vendor;
                        this.clear(window, cx);
                        cx.notify();
                    })),
            );
        }
        view = view.child(kinds).child(if self.vendor {
            "Leverandører · kanonisk liste"
        } else {
            "Kunder · kanonisk liste"
        });
        for contact in data.rows(self.vendor) {
            let id = contact["id"].as_i64().unwrap_or(0);
            let value = contact.clone();
            let name = contact["name"]
                .as_str()
                .unwrap_or("Navn mangler i kanonisk svar");
            view = view.child(
                Button::new(SharedString::from(format!("contact-{id}")))
                    .label(format!(
                        "{name} · id{id}{}",
                        if editable(contact) {
                            ""
                        } else {
                            " · beskyttet identitet"
                        }
                    ))
                    .disabled(self.busy || self.review.is_some())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select(value.clone(), window, cx)
                    })),
            );
        }
        if self.pending.is_some() {
            return view;
        }
        if let Some(review) = self.review.clone() {
            let values: Value = serde_json::from_slice(&review.body).unwrap_or(Value::Null);
            view = view.child(div().text_lg().child(if review.delete {
                "Bekræft sletning"
            } else {
                "Gennemgå kontaktfelter"
            }));
            if let Some(source) = &review.source {
                view = view.child(format!(
                    "{} · id{}",
                    source["name"].as_str().unwrap_or("?"),
                    source["id"]
                ));
            }
            if review.delete {
                view=view.child("Serveren afviser sletning ved åbne fakturaer, leverandørgæld eller beskyttede historiske relationer. Originalbilag og bogføring bevares.");
            } else {
                for (key, label) in fields(review.vendor) {
                    view = view.child(format!(
                        "{label}: {}",
                        values[key].as_str().map(str::to_owned).unwrap_or_else(|| {
                            if values[key].is_null() {
                                "Tomt".into()
                            } else {
                                values[key].to_string()
                            }
                        })
                    ));
                }
            }
            return view
                .child(
                    Checkbox::new("contacts-reviewed")
                        .label(if review.delete {
                            "Jeg bekræfter sletning af præcis denne kontakt."
                        } else {
                            "Jeg har gennemgået præcis disse kontaktfelter."
                        })
                        .checked(self.reviewed)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            this.reviewed = *checked;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("contacts-confirm")
                        .label(if review.delete {
                            "Bekræft sletning én gang"
                        } else {
                            "Bekræft kontaktændring én gang"
                        })
                        .disabled(blocked || !self.reviewed)
                        .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
                )
                .child(
                    Button::new("contacts-review-cancel")
                        .label("Annullér gennemgang")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.review = None;
                            this.reviewed = false;
                            cx.notify();
                        })),
                );
        }
        view = view.child(
            Button::new("contacts-new")
                .label(if self.vendor {
                    "Ny leverandør"
                } else {
                    "Ny kunde"
                })
                .disabled(blocked)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.clear(window, cx);
                    cx.notify();
                })),
        );
        if let Some(selected) = &self.selected {
            view = view.child(format!(
                "Valgt kanonisk kontakt: {} · id{}",
                selected["name"].as_str().unwrap_or("?"),
                selected["id"]
            ));
            if !self.vendor {
                view = view.child(format!(
                    "Åbne fakturaer: {} · Åbent beløb: {} (serverberegnet)",
                    selected["openInvoiceCount"], selected["openBalance"]
                ));
            }
            if !editable(selected) {
                return view.child("GDPR-redigeret eller arkiveret identitet kan ikke redigeres her. Ingen tidligere navne eller felter hentes fra lokal historik.");
            }
        }
        for (key, label) in fields(self.vendor) {
            if let Some(field) = self.fields.iter().find(|field| field.key == key) {
                view = view
                    .child(label)
                    .child(Input::new(&field.input).disabled(blocked));
            }
        }
        view.child(
            Button::new("contacts-review-save")
                .label(if self.selected.is_some() {
                    "Gennemgå ændringer"
                } else {
                    "Gennemgå oprettelse"
                })
                .disabled(blocked)
                .on_click(cx.listener(|this, _, _, cx| this.review(false, cx))),
        )
        .child(
            Button::new("contacts-review-delete")
                .label("Gennemgå sletning af valgt kontakt")
                .disabled(blocked || self.selected.is_none())
                .on_click(cx.listener(|this, _, _, cx| this.review(true, cx))),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn customer_form_preserves_explicit_fields_and_types_without_vendor_leakage() {
        let values = [
            ("name", "Synthetic buyer"),
            ("vatOrCvr", "DK12345678"),
            ("paymentTermsDays", "30"),
            ("defaultCurrency", "DKK"),
            ("countryCode", "not-customer-field"),
        ]
        .map(|(key, value)| (key.to_owned(), value.to_owned()));
        let wire: Value = serde_json::from_slice(&body(false, &values, true).unwrap()).unwrap();
        assert_eq!(wire["name"], "Synthetic buyer");
        assert_eq!(wire["paymentTermsDays"], 30);
        assert_eq!(wire["defaultCurrency"], "DKK");
        assert!(wire.get("countryCode").is_none());
        assert!(wire.get("confirm").is_none());
        assert!(
            body(
                false,
                &[
                    ("name".into(), "Synthetic".into()),
                    ("defaultCurrency".into(), "".into())
                ],
                true
            )
            .is_err()
        );
    }
    #[::core::prelude::v1::test]
    fn gdpr_archived_identity_and_stale_source_are_not_editable() {
        assert!(!editable(
            &json!({"id":1,"name":"[redigeret — GDPR]","gdprErased":true})
        ));
        assert!(!editable(
            &json!({"id":1,"name":"Synthetic archived","archived":true})
        ));
        assert!(editable(
            &json!({"id":1,"name":"Synthetic active","gdprErased":false})
        ));
    }
    #[::core::prelude::v1::test]
    fn delete_uses_confirmation_and_requires_exact_receipt_id() {
        let review = Review {
            vendor: true,
            delete: true,
            source: Some(json!({"id":7,"name":"Synthetic supplier"})),
            body: br#"{"confirm":true}"#.to_vec(),
        };
        assert!(matches!(
            review.operation().unwrap(),
            DomainMutation::DeleteVendor(7)
        ));
        assert_eq!(
            receipt(
                &json!({"ok":true,"vendor":{"id":7,"deleted":true}}),
                &review
            ),
            Some(7)
        );
        assert_eq!(
            receipt(
                &json!({"ok":true,"vendor":{"id":8,"deleted":true}}),
                &review
            ),
            None
        );
        assert_eq!(
            receipt(&json!({"ok":true,"vendor":{"id":7}}), &review),
            None
        );
    }
    #[::core::prelude::v1::test]
    fn uncertainty_marker_keeps_scope_without_retaining_names_or_retry_payload() {
        let review = Review {
            vendor: false,
            delete: false,
            source: Some(json!({"id":7,"name":"Synthetic private name"})),
            body: br#"{"name":"Synthetic private name"}"#.to_vec(),
        };
        let data = Data {
            contacts: json!({}),
            principal: "owner".into(),
            ledger: "immutable".into(),
        };
        let record = marker(
            &review,
            "https://synthetic.example",
            "synthetic",
            &data,
            None,
        );
        assert_eq!(record["contactId"], 7);
        assert_eq!(record["operation"], "update");
        assert!(bank_state::same_scope(
            &record,
            "https://synthetic.example",
            "synthetic",
            "owner",
            "immutable",
            None
        ));
        assert!(!record.to_string().contains("Synthetic private name"));
        assert!(record.get("body").is_none());
    }
    #[::core::prelude::v1::test]
    fn canonical_change_or_erasure_blocks_stale_edit_and_delete_before_sending() {
        let source = json!({"id":7,"name":"Synthetic current","gdprErased":false});
        let review = Review {
            vendor: false,
            delete: false,
            source: Some(source.clone()),
            body: br#"{"name":"Synthetic change"}"#.to_vec(),
        };
        let mut data = Data {
            contacts: json!({"customers":[source],"vendors":[]}),
            principal: "owner".into(),
            ledger: "immutable".into(),
        };
        assert!(source_current(&review, &data));
        data.contacts["customers"][0] =
            json!({"id":7,"name":"[redigeret — GDPR]","gdprErased":true});
        assert!(!source_current(&review, &data));
        data.contacts["customers"] = json!([]);
        assert!(!source_current(&review, &data));
        assert!(!editable(&json!({"id":7,"name":"Privacy status missing"})));
    }
    #[::core::prelude::v1::test]
    fn manual_resolution_scope_cannot_cross_profile_even_at_identical_origin() {
        let data = Data {
            contacts: json!({}),
            principal: "owner".into(),
            ledger: "immutable".into(),
        };
        let review = Review {
            vendor: false,
            delete: false,
            source: None,
            body: vec![],
        };
        let record = marker(
            &review,
            "http://127.0.0.1:1234",
            "synthetic",
            &data,
            Some("owned-profile"),
        );
        assert!(same_scope(
            &record,
            "http://127.0.0.1:5678",
            "synthetic",
            &data,
            Some("owned-profile")
        ));
        assert!(!same_scope(
            &record,
            "http://127.0.0.1:1234",
            "synthetic",
            &data,
            Some("different-profile")
        ));
        assert!(!same_scope(
            &record,
            "http://127.0.0.1:1234",
            "synthetic",
            &data,
            None
        ));
    }
}
