//! Native source-document intake and explicitly reviewed paid-receipt action.
#[path = "expense.rs"]
mod expense;
use crate::{Session, document_state, invoice_state, readable};
use base64::{Engine, engine::general_purpose::STANDARD};
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
use serde_json::{Number, Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};
const MAX_BYTES: usize = 25 * 1024 * 1024;
pub(crate) const FIELDS: &[(&str, &str)] = &[
    ("source", "Kilde (fx manuel upload)"),
    ("issueDate", "Bilagsdato YYYY-MM-DD"),
    ("invoiceNo", "Leverandørens faktura-/kvitteringsnummer"),
    ("deliveryDescription", "Beskrivelse af leverancen"),
    ("amountIncVat", "Bruttobeløb inkl. moms"),
    ("vatAmount", "Momsbeløb fra originalen"),
    ("currency", "Valuta (ISO, fx DKK)"),
    ("senderName", "Afsenders navn"),
    ("senderAddress", "Afsenders adresse"),
    ("senderVatOrCvr", "Afsenders CVR / momsnummer"),
    ("senderCountryCode", "Afsenders landekode (fx DK)"),
    (
        "senderIdentifierKind",
        "Afsenders identitetstype (dk_cvr, eu_vat, non_eu)",
    ),
    ("recipientName", "Modtagers navn"),
    ("recipientAddress", "Modtagers adresse"),
    ("recipientVatOrCvr", "Modtagers CVR / momsnummer"),
];
fn text(v: &Value, key: &str) -> Result<String, String> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Serveren mangler {key}"))
}
fn valid_hash(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}
#[derive(Clone)]
struct Data {
    documents: Vec<Value>,
    profile: Value,
    principal: String,
    ledger: String,
}
fn parse_documents(root: Value, company: &str) -> Result<Vec<Value>, String> {
    if root["ok"] != true || root["documents"]["slug"] != company {
        return Err("Serveren bekræftede ikke bilagslisten for denne virksomhed".into());
    }
    let rows = root["documents"]["documents"]
        .as_array()
        .ok_or("Serveren mangler kanonisk bilagsliste")?;
    let mut documents = Vec::<Value>::new();
    for row in rows {
        let id = row["id"]
            .as_i64()
            .filter(|id| *id > 0)
            .ok_or("Bilags-id mangler")?;
        if !valid_hash(&row["sha256Hash"]) {
            return Err("Serveren mangler et kanonisk SHA-256-hash. Genindlæs fra en opdateret native server.".into());
        }
        if !row["mimeType"].is_null() && !row["mimeType"].is_string() {
            return Err("Originaltypen er ugyldig i kanonisk metadata".into());
        }
        if let Some(previous) = documents.iter().find(|v| v["id"] == id) {
            if previous["sha256Hash"] != row["sha256Hash"]
                || previous["mimeType"] != row["mimeType"]
            {
                return Err("Bilagslisten indeholder modstridende originalidentitet".into());
            }
        } else {
            documents.push(row.clone());
        }
    }
    Ok(documents)
}
fn read(session: &Session, company: &str) -> Result<Data, String> {
    let documents = session
        .runtime
        .block_on(session.client.documents(company))
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
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    if profile["ok"] != true
        || me["ok"] != true
        || journal["ok"] != true
        || journal["journal"]["slug"] != company
    {
        return Err("Serveren bekræftede ikke virksomhed og bruger".into());
    }
    Ok(Data {
        documents: parse_documents(documents, company)?,
        profile: profile["company"].clone(),
        principal: text(&me["user"], "id")?,
        ledger: text(&journal["journal"], "ledgerUuid")?,
    })
}
#[derive(Clone)]
pub(crate) struct Source {
    pub(crate) filename: String,
    pub(crate) mime: &'static str,
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) hash: String,
}
pub(crate) fn file(path: &Path) -> Result<Source, String> {
    let before =
        std::fs::symlink_metadata(path).map_err(|_| "Originalfilen kunne ikke undersøges")?;
    if !before.is_file()
        || before.file_type().is_symlink()
        || before.len() == 0
        || before.len() > MAX_BYTES as u64
    {
        return Err(
            "Vælg én almindelig originalfil på højst25MiB (ikke tom) uden symbolske links".into(),
        );
    }
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Filnavnet skal være UTF-8")?
        .to_owned();
    if filename.len() > 255
        || filename.contains(['/', '\\'])
        || filename.chars().any(char::is_control)
    {
        return Err("Originalfilens navn er ugyldigt".into());
    }
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime = match extension.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "txt" => "text/plain",
        "json" => "application/json",
        "xml" => "application/xml",
        _ => {
            return Err(
                "Vælg PDF, PNG, JPEG, TXT, JSON eller XML. Andre typer understøttes ikke.".into(),
            );
        }
    };
    let mut input = std::fs::File::open(path).map_err(|_| "Originalfilen kunne ikke åbnes")?;
    let opened = input
        .metadata()
        .map_err(|_| "Originalfilen kunne ikke undersøges")?;
    if !opened.is_file() || opened.dev() != before.dev() || opened.ino() != before.ino() {
        return Err("Originalfilen ændrede identitet under åbning".into());
    }
    let mut bytes = Vec::new();
    input
        .by_ref()
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Originalfilen kunne ikke læses")?;
    let after = input
        .metadata()
        .map_err(|_| "Originalfilen kunne ikke undersøges")?;
    if bytes.is_empty()
        || bytes.len() > MAX_BYTES
        || after.len() != bytes.len() as u64
        || opened.mtime() != after.mtime()
        || opened.mtime_nsec() != after.mtime_nsec()
    {
        return Err("Originalfilen ændrede sig under læsning eller overskrider25MiB".into());
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    Ok(Source {
        filename,
        mime,
        bytes: Arc::new(bytes),
        hash,
    })
}
fn decimal(value: &str) -> Result<Number, String> {
    let normalized = value.trim().replace(',', ".");
    let mut parts = normalized.split('.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next().unwrap_or("");
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 2
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || parts.next().is_some()
    {
        return Err(
            "Angiv et præcist ikke-negativt beløb med højst to decimaler, uden tusindtalsseparator"
                .into(),
        );
    }
    Number::from_str(&normalized).map_err(|_| "Beløbet kunne ikke læses".into())
}
pub(crate) fn metadata(kind: &str, values: &[(String, String)]) -> Result<Value, String> {
    let get = |key: &str| {
        values
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.trim())
            .unwrap_or("")
    };
    let mut result = json!({"documentType":kind});
    for key in ["source", "issueDate", "deliveryDescription", "currency"] {
        if get(key).is_empty() {
            return Err("Angiv kilde, dato, beskrivelse og valuta fra originalen".into());
        }
        result[key] = json!(get(key));
    }
    for key in ["amountIncVat", "vatAmount"] {
        result[key] = json!(decimal(get(key))?);
    }
    if !get("invoiceNo").is_empty() {
        result["invoiceNo"] = json!(get("invoiceNo"));
    }
    for (party, prefix) in [("sender", "sender"), ("recipient", "recipient")] {
        let mut fields = serde_json::Map::new();
        for key in [
            "Name",
            "Address",
            "VatOrCvr",
            "CountryCode",
            "IdentifierKind",
        ] {
            let field = format!("{prefix}{key}");
            if get(&field).is_empty() {
                continue;
            }
            let key = match key {
                "Name" => "name",
                "Address" => "address",
                "VatOrCvr" => "vatOrCvr",
                "CountryCode" => "countryCode",
                _ => "identifierKind",
            };
            fields.insert(key.into(), json!(get(&field)));
        }
        if !fields.is_empty() {
            result[party] = Value::Object(fields);
        }
    }
    Ok(result)
}
#[derive(Clone)]
struct Review {
    source: Source,
    metadata: Value,
    body: Arc<Vec<u8>>,
}
fn review(source: Source, metadata: Value) -> Result<Review, String> {
    let body=serde_json::to_vec(&json!({"fileName":source.filename,"fileBase64":STANDARD.encode(source.bytes.as_ref()),"metadata":metadata,"confirm":true})).map_err(|_|"Uploadanmodningen kunne ikke fastholdes")?;
    if body.len() > 35 * 1024 * 1024 {
        return Err("Uploadanmodningen overskrider35MiB".into());
    }
    Ok(Review {
        source,
        metadata,
        body: Arc::new(body),
    })
}
fn scope(record: &Value, origin: &str, company: &str, data: &Data, local: Option<&str>) -> bool {
    record["company"] == company
        && record["principal"] == data.principal
        && record["ledger"] == data.ledger
        && record["localIdentity"].as_str() == local
        && (record["origin"] == origin
            || local.is_some_and(|identity| record["localIdentity"].as_str() == Some(identity)))
}
fn marker(review: &Review, origin: &str, company: &str, data: &Data, local: Option<&str>) -> Value {
    json!({"format":"rentemester-desktop-document/1","origin":origin,"company":company,"principal":data.principal,"ledger":data.ledger,"localIdentity":local,"byteCount":review.source.bytes.len(),"mime":review.source.mime,"sha256Hash":review.source.hash})
}
struct Field {
    key: &'static str,
    input: Entity<InputState>,
}
pub struct Documents {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    data: Option<Data>,
    fields: Vec<Field>,
    kind: &'static str,
    source: Option<Source>,
    review: Option<Review>,
    reviewed: bool,
    pending: Option<Value>,
    resolved: bool,
    selected: Option<i64>,
    download: Option<(PathBuf, String, String)>,
    original_opened: bool,
    original_reviewed: bool,
    expense: Option<Entity<expense::Expense>>,
    expense_pending: Option<Value>,
    busy: bool,
    status: String,
}
impl Documents {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        company: String,
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
        let mut this = Self {
            session,
            origin,
            company,
            local,
            data: None,
            fields,
            kind: "purchase_sale",
            source: None,
            review: None,
            reviewed: false,
            pending: None,
            resolved: false,
            selected: None,
            download: None,
            original_opened: false,
            original_reviewed: false,
            expense: None,
            expense_pending: None,
            busy: false,
            status: String::new(),
        };
        this.load(window, cx);
        this
    }
    fn reset_metadata(&mut self, profile: &Value, window: &mut Window, cx: &mut Context<Self>) {
        for field in &self.fields {
            field
                .input
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        for (key, value) in [
            ("source", Some("native-desktop")),
            ("currency", profile["currency"].as_str()),
            ("recipientName", profile["name"].as_str()),
            ("recipientAddress", profile["address"].as_str()),
            ("recipientVatOrCvr", profile["cvr"].as_str()),
        ] {
            if let (Some(value), Some(field)) =
                (value, self.fields.iter().find(|field| field.key == key))
            {
                field.input.update(cx, |input, cx| {
                    input.set_value(value.to_owned(), window, cx)
                });
            }
        }
    }
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.data = None;
        self.review = None;
        self.reviewed = false;
        self.resolved = false;
        self.download = None;
        self.original_opened = false;
        self.original_reviewed = false;
        self.status = "Henter kanoniske bilag og originalidentiteter…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((
                read(&session, &company)?,
                document_state::load()?,
                expense::pending()?,
            ))
        });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,window,cx|{this.busy=false;match result{Ok((data,pending,expense_pending))=>{
            this.reset_metadata(&data.profile,window,cx);
            this.data=Some(data);this.pending=pending;this.expense_pending=expense_pending;this.status="Vælg en original og indtast metadata fra den. Serveren validerer filtype, indhold, dubletter og opbevaring; ingen OCR eller bogføring udføres her.".into();
        },Err(error)=>this.status=error}cx.notify();});}).detach();
        cx.notify();
    }
    fn choose(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        self.review = None;
        self.reviewed = false;
        let choice = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Vælg originalbilag (PDF, PNG, JPEG, TXT, JSON eller XML)".into()),
        });
        self.busy = true;
        cx.spawn(async move|view,cx|{let selected=match choice.await{Ok(Ok(Some(paths)))=>paths.first().cloned(),Ok(Ok(None))=>None,_=>{let _=view.update(cx,|this,cx|{this.busy=false;this.status="Filvælgeren kunne ikke åbnes".into();cx.notify();});return;}};
            let result=if let Some(path)=selected{Some(cx.background_executor().spawn(async move{file(&path)}).await)}else{None};
            let _=view.update(cx,|this,cx|{this.busy=false;match result{Some(Ok(source))=>{this.source=Some(source);this.status="Originalens præcise bytes er fastholdt i hukommelsen. Indtast og gennemgå felterne før upload.".into();},Some(Err(error))=>this.status=error,None=>{}}cx.notify();});}).detach();
        cx.notify();
    }
    fn review(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(source) = self.source.clone() else {
            self.status = "Vælg en originalfil".into();
            cx.notify();
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
        let metadata = match metadata(self.kind, &values) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.status = error;
                cx.notify();
                return;
            }
        };
        self.busy = true;
        self.status = "Fastholder præcise originalbytes og metadata til gennemgang…".into();
        let task = cx
            .background_executor()
            .spawn(async move { review(source, metadata) });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok(review)=>{this.review=Some(review);this.reviewed=false;this.status="Gennemgå originalfilens navn, størrelse, hash, type og alle metadata før bekræftelse.".into();},Err(error)=>this.status=error}cx.notify();});}).detach();
        cx.notify();
    }
    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.reviewed || self.pending.is_some() {
            return;
        }
        let (Some(review), Some(data)) = (self.review.clone(), self.data.clone()) else {
            return;
        };
        self.reset_metadata(&data.profile, window, cx);
        self.busy = true;
        self.review = None;
        self.reviewed = false;
        self.source = None;
        self.data = None;
        self.download = None;
        self.original_opened = false;
        self.original_reviewed = false;
        self.status = "Sender den gennemgåede original og metadata én gang…".into();
        let session = self.session.clone();
        let origin = self.origin.clone();
        let company = self.company.clone();
        let local = self.local.clone();
        let task=cx.background_executor().spawn(async move{
            let fresh=read(&session,&company)?;if fresh.principal!=data.principal||fresh.ledger!=data.ledger{return Err("Bruger eller virksomhed ændrede sig; intet sendt".into());}
            let record=marker(&review,&origin,&company,&fresh,local.as_deref());document_state::begin(&record)?;
            let response=session.runtime.block_on(session.client.ingest_document(&company,review.body.as_ref().clone(),true));
            let(message,id)=match response{
                Ok(value)if value["ok"]==true=>if let Some(id)=value["document"]["id"].as_i64().filter(|id|*id>0){if value["document"]["documentNo"].as_str().is_some_and(|s|!s.is_empty()){document_state::finish(&record,false)?;(format!("Indlæsning bekræftet: {} · id{id}.",value["document"]["documentNo"].as_str().unwrap()),Some(id))}else{("Ufuldstændig kvittering; udfaldet er uafklaret. Ingen ny upload sendes.".into(),None)}}else{("Bilags-id mangler i kvitteringen; udfaldet er uafklaret. Ingen gentagelse.".into(),None)},
                Err(Error::Refused{status})=>{document_state::finish(&record,false)?;(format!("Indlæsning afvist (HTTP{status}). Kontrollér metadata, adgang, filtype/indhold og eksisterende dubletter. Serveren er autoritet; der sendes ikke igen automatisk."),None)},
                Err(Error::InvalidInput(_))=>{document_state::finish(&record,false)?;("Uploadfelter eller størrelse afvist lokalt før netværkskald. Ret og gennemgå igen.".into(),None)},
                _=>("Udfaldet er uafklaret; originalen kan være indlæst. Ingen genafsendelse. Genlæs kanoniske bilag/hash og afklar i den oprindelige virksomhed.".into(),None),
            };Ok::<_,String>((message,id,review.source.hash.clone(),read(&session,&company),document_state::load()?))
        });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,_window,cx|{this.busy=false;match result{Ok((message,id,source_hash,data,pending))=>{this.status=message;this.pending=pending;this.selected=id;match data{Ok(data)=>{if let Some(id)=id{if let Some(document)=data.documents.iter().find(|v|v["id"]==id){if document["sha256Hash"]==source_hash{this.status.push_str(&format!(" Kanonisk SHA-256 {} svarer præcist til den gennemgåede kilde.",source_hash));}else{this.status.push_str(" Kanonisk hash afviger fra kilden; hent ikke denne original før manuel afklaring.");this.selected=None;}}else{this.status.push_str(" Kvittering bekræftet; genlæs for kanonisk identitet.");}}this.data=Some(data);},Err(error)=>this.status.push_str(&format!(" Genlæsning fejlede: {error}"))}},Err(error)=>{this.status=error;this.pending=document_state::load().ok().flatten();}}cx.notify();});}).detach();
        cx.notify();
    }
    fn open_expense(
        &mut self,
        id: i64,
        recovery: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.review.is_some() {
            return;
        }
        let hash = if recovery {
            None
        } else {
            if self.pending.is_some()
                || self.expense_pending.is_some()
                || !self.original_reviewed
                || !self.original_opened
            {
                return;
            }
            let Some(document) = self
                .data
                .as_ref()
                .and_then(|data| data.documents.iter().find(|row| row["id"] == id))
            else {
                return;
            };
            let Some((_, _, hash)) = &self.download else {
                return;
            };
            if !expense::unposted(document) || document["sha256Hash"] != *hash {
                return;
            }
            Some(hash.clone())
        };
        let session = self.session.clone();
        let origin = self.origin.clone();
        let company = self.company.clone();
        let local = self.local.clone();
        self.expense = Some(cx.new(|cx| {
            expense::Expense::new(session, origin, company, local, id, hash, window, cx)
        }));
        cx.notify();
    }
    fn original(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let (Some(id), Some(data)) = (self.selected, self.data.clone()) else {
            return;
        };
        let Some(document) = data.documents.iter().find(|v| v["id"] == id).cloned() else {
            return;
        };
        self.busy = true;
        self.download = None;
        self.original_opened = false;
        self.original_reviewed = false;
        self.status =
            "Henter præcis original via serverens autoriserede, hashkontrollerede læsning…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let task = cx.background_executor().spawn(async move {
            let fresh = read(&session, &company)?;
            let current = fresh
                .documents
                .iter()
                .find(|v| v["id"] == id)
                .ok_or("Bilaget findes ikke længere i kanonisk liste")?;
            if fresh.principal != data.principal
                || fresh.ledger != data.ledger
                || current["sha256Hash"] != document["sha256Hash"]
                || current["mimeType"] != document["mimeType"]
            {
                return Err(
                    "Originalidentitet eller bruger ændrede sig; genlæs før download".into(),
                );
            }
            let mime = text(current, "mimeType")?;
            let hash = text(current, "sha256Hash")?;
            let value = session
                .runtime
                .block_on(session.client.document_original(&company, id))
                .map_err(readable)?;
            let path = document_state::download(value, id, &mime, &hash)?;
            Ok::<_, String>((path, mime, hash))
        });
        cx.spawn(async move|view,cx|{let result=task.await;let _=view.update(cx,|this,cx|{this.busy=false;match result{Ok(download)=>{this.download=Some(download);this.status="Originalens præcise bytes er gemt i en privat fil0600 med kontrolleret filtype. Strukturerede JSON/XML-originaler downloades uden automatisk åbning.".into();},Err(error)=>this.status=error}cx.notify();});}).detach();
        cx.notify();
    }
}
impl Render for Documents {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(expense) = self.expense.clone() {
            return div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    Button::new("expense-close")
                        .label("Luk bogføring og genlæs bilag")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.expense = None;
                            this.load(window, cx);
                        })),
                )
                .child(expense);
        }
        let blocked = self.busy || self.pending.is_some();
        let mut view = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_xl().child("Bilag og originaler"))
            .child(format!("{} · {}", self.company, self.origin))
            .child(self.status.clone())
            .child(
                Button::new("documents-refresh")
                    .label("Genlæs kanonisk bilagsliste og hash")
                    .disabled(self.busy || self.review.is_some())
                    .on_click(cx.listener(|this, _, window, cx| this.load(window, cx))),
            );
        let Some(data) = &self.data else {
            return view;
        };
        if let Some(record) = self.pending.clone() {
            view=view.child(format!("UAFKLARET upload · {} bytes · {} · SHA-256 {}. Original/metadata genafsendes ikke, og markerfilen indeholder dem ikke.",record["byteCount"],record["mime"].as_str().unwrap_or("?"),record["sha256Hash"].as_str().unwrap_or("?")))
                .child("Genlæs bilag og originalhash; en mulig matchende række er ikke i sig selv kvittering for dette forsøg. Afklar manuelt i den oprindelige virksomhed.");
            if let Some(origin) = record["origin"].as_str() {
                let url = format!("{origin}/");
                view = view.child(
                    Button::new("documents-browser")
                        .label("Åbn oprindelig server til manuel afklaring")
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                );
            }
            if scope(
                &record,
                &self.origin,
                &self.company,
                data,
                self.local.as_deref(),
            ) {
                view=view.child(Checkbox::new("documents-manually-resolved").label("Jeg har manuelt afklaret uploadens udfald i den korrekte virksomhed. Arkivér status uden genafsendelse.").checked(self.resolved).on_click(cx.listener(|this,checked,_,cx|{this.resolved=*checked;cx.notify();})))
                    .child(Button::new("documents-resolve").label("Bekræft manuel afklaring").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{
                        if !this.resolved||this.busy{return;}let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};if !scope(record,&this.origin,&this.company,data,this.local.as_deref()){return;}
                        match document_state::finish(record,true){Ok(())=>{this.pending=None;this.resolved=false;this.status="Manuel afklaring registreret; original og metadata blev ikke genafsendt. Dette er ikke en serverkvittering.".into();},Err(error)=>this.status=error}cx.notify();
                    })));
            } else {
                view=view.child("Åbn den oprindelige bruger, virksomhed og profil for at afklare. Nye uploads er blokeret.");
            }
        }
        if let Some(record) = &self.expense_pending {
            let id = record["documentId"].as_i64().unwrap_or(0);
            view=view.child(format!("Uafklaret bilagsbogføring: bilag{id} · bankrække{}. Ingen ny bogføring sendes før manuel afklaring.",record["bankTransactionId"]))
            .child(Button::new("expense-recovery-open").label("Åbn manuel afklaring af bilagsbogføring").disabled(self.busy||id<=0).on_click(cx.listener(move|this,_,window,cx|this.open_expense(id,true,window,cx))));
        }
        view = view.child(div().text_lg().child("Kanoniske bilag"));
        for document in &data.documents {
            let id = document["id"].as_i64().unwrap_or(0);
            view = view.child(
                Button::new(SharedString::from(format!("document-{id}")))
                    .label(format!(
                        "{} · {} · {} · {} {} · {}",
                        document["documentNo"].as_str().unwrap_or("?"),
                        document["filename"]
                            .as_str()
                            .unwrap_or("Originalnavn ikke oplyst"),
                        document["invoiceDate"]
                            .as_str()
                            .unwrap_or("Dato ikke oplyst"),
                        document["amountIncVat"],
                        document["currency"].as_str().unwrap_or("?"),
                        document["status"].as_str().unwrap_or("?")
                    ))
                    .disabled(self.busy || self.review.is_some())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = Some(id);
                        this.download = None;
                        this.original_opened = false;
                        this.original_reviewed = false;
                        cx.notify();
                    })),
            );
        }
        if let Some(id) = self.selected {
            if let Some(document) = data.documents.iter().find(|document| document["id"] == id) {
                let supported = document["mimeType"]
                    .as_str()
                    .is_some_and(|mime| document_state::extension(mime).is_some());
                view = view
                    .child(format!(
                        "Bilag{id} · SHA-256 {} · Type {} · Opbevares til {}",
                        document["sha256Hash"].as_str().unwrap_or("?"),
                        document["mimeType"].as_str().unwrap_or("Ikke oplyst"),
                        document["retainUntil"]
                            .as_str()
                            .unwrap_or("Ikke oplyst i kanonisk metadata")
                    ))
                    .child(
                        Button::new("document-original")
                            .label("Hent præcis original og verificér kanonisk hash")
                            .disabled(self.busy || document["hasFile"] != true || !supported)
                            .on_click(cx.listener(|this, _, _, cx| this.original(cx))),
                    );
            }
            if let Some(document) = data.documents.iter().find(|document| document["id"] == id) {
                view = view.child(
                    Button::new("document-expense-open")
                        .label("Bogfør betalt DKK-bilag")
                        .disabled(
                            self.busy
                                || self.pending.is_some()
                                || self.expense_pending.is_some()
                                || !self.original_reviewed
                                || !expense::unposted(document),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_expense(id, false, window, cx)
                        })),
                );
            }
            if let Some((path, mime, hash)) = self.download.clone() {
                view = view.child(format!(
                    "Privat original: {} · SHA-256 {hash}",
                    path.display()
                ));
                if document_state::can_open(&mime) {
                    view = view.child(
                        Button::new("document-original-open")
                            .label("Åbn original i almindelig filviser")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.open_url(&invoice_state::file_url(&path));
                                this.original_opened = true;
                                cx.notify();
                            })),
                    );
                    view = view.child(
                        Checkbox::new("document-original-reviewed")
                            .label("Jeg har gennemgået originalbilaget i filviseren før bogføring.")
                            .checked(self.original_reviewed)
                            .disabled(self.busy || !self.original_opened)
                            .on_click(cx.listener(|this, checked, _, cx| {
                                if this.original_opened {
                                    this.original_reviewed = *checked;
                                    cx.notify();
                                }
                            })),
                    );
                } else {
                    view=view.child("JSON/XML er gemt privat som præcise originaler og åbnes ikke automatisk af appen.");
                }
            }
        }
        if self.pending.is_some() {
            return view;
        }
        if let Some(review) = self.review.clone() {
            view = view
                .child(
                    div()
                        .text_lg()
                        .child("Gennemgå præcis original og metadata"),
                )
                .child(format!(
                    "{} · {} bytes · {} · SHA-256 {}",
                    review.source.filename,
                    review.source.bytes.len(),
                    review.source.mime,
                    review.source.hash
                ))
                .child(format!(
                    "Bilagstype: {}",
                    if review.metadata["documentType"] == "cash_register_receipt" {
                        "Kassekvittering"
                    } else {
                        "Købs-/salgsbilag"
                    }
                ));
            for (key, label) in FIELDS {
                let value = match *key {
                    "senderName" => &review.metadata["sender"]["name"],
                    "senderAddress" => &review.metadata["sender"]["address"],
                    "senderVatOrCvr" => &review.metadata["sender"]["vatOrCvr"],
                    "senderCountryCode" => &review.metadata["sender"]["countryCode"],
                    "senderIdentifierKind" => &review.metadata["sender"]["identifierKind"],
                    "recipientName" => &review.metadata["recipient"]["name"],
                    "recipientAddress" => &review.metadata["recipient"]["address"],
                    "recipientVatOrCvr" => &review.metadata["recipient"]["vatOrCvr"],
                    _ => &review.metadata[key],
                };
                view = view.child(format!(
                    "{label}: {}",
                    value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| if value.is_null() {
                            "Ikke angivet".into()
                        } else {
                            value.to_string()
                        })
                ));
            }
            return view.child(Checkbox::new("document-reviewed").label("Jeg har gennemgået præcis original, bilagstype og indtastede metadata fra originalen.").checked(self.reviewed).on_click(cx.listener(|this,checked,_,cx|{this.reviewed=*checked;cx.notify();})))
                .child(Button::new("document-confirm").label("Bekræft indlæsning én gang").disabled(self.busy||!self.reviewed).on_click(cx.listener(|this,_,window,cx|this.send(window,cx))))
                .child(Button::new("document-back").label("Tilbage og ret; ny gennemgang kræves").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.reviewed=false;cx.notify();})));
        }
        view = view.child(div().text_lg().child("Indlæs originalbilag"));
        let mut kinds = div().flex().gap_2();
        for (id, label, kind) in [
            ("document-purchase", "Købs-/salgsbilag", "purchase_sale"),
            (
                "document-receipt",
                "Kassekvittering",
                "cash_register_receipt",
            ),
        ] {
            kinds = kinds.child(Button::new(id).label(label).disabled(blocked).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.kind = kind;
                    this.review = None;
                    this.reviewed = false;
                    cx.notify();
                }),
            ));
        }
        view = view
            .child(kinds)
            .child(format!(
                "Valgt type: {}",
                if self.kind == "purchase_sale" {
                    "Købs-/salgsbilag"
                } else {
                    "Kassekvittering"
                }
            ))
            .child(
                Button::new("document-file")
                    .label("Vælg lokal original…")
                    .disabled(blocked)
                    .on_click(cx.listener(|this, _, _, cx| this.choose(cx))),
            );
        if let Some(source) = &self.source {
            view = view.child(format!(
                "{} · {} bytes · {} · SHA-256 {}",
                source.filename,
                source.bytes.len(),
                source.mime,
                source.hash
            ));
        }
        for field in &self.fields {
            let label = FIELDS
                .iter()
                .find(|(key, _)| *key == field.key)
                .map(|(_, label)| *label)
                .unwrap_or(field.key);
            view = view
                .child(label)
                .child(Input::new(&field.input).disabled(blocked));
        }
        view.child(
            Button::new("document-review")
                .label("Gennemgå original og metadata")
                .disabled(blocked || self.source.is_none())
                .on_click(cx.listener(|this, _, _, cx| this.review(cx))),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn labelled_metadata_keeps_exact_amounts_and_party_identity_without_ocr_or_math() {
        let values = [
            ("source", "native-desktop"),
            ("issueDate", "2026-10-19"),
            ("deliveryDescription", "Synthetic receipt"),
            ("amountIncVat", "12,34"),
            ("vatAmount", "2.34"),
            ("currency", "DKK"),
            ("senderName", "Synthetic supplier"),
            ("senderCountryCode", "DK"),
            ("senderIdentifierKind", "dk_cvr"),
            ("senderVatOrCvr", "DK12345678"),
        ]
        .map(|(key, value)| (key.to_owned(), value.to_owned()));
        let result = metadata("purchase_sale", &values).unwrap();
        assert_eq!(result["amountIncVat"].to_string(), "12.34");
        assert_eq!(result["vatAmount"].to_string(), "2.34");
        assert_eq!(result["sender"]["identifierKind"], "dk_cvr");
        assert!(result.get("netAmount").is_none());
        assert!(result.get("force").is_none());
    }
    #[::core::prelude::v1::test]
    fn exact_review_payload_does_not_reopen_source_or_invent_override() {
        let bytes = b"Synthetic receipt\n".to_vec();
        let source = Source {
            filename: "synthetic.txt".into(),
            mime: "text/plain",
            hash: format!("{:x}", Sha256::digest(&bytes)),
            bytes: Arc::new(bytes.clone()),
        };
        let review = review(
            source,
            json!({"source":"native-desktop","documentType":"cash_register_receipt"}),
        )
        .unwrap();
        let wire: Value = serde_json::from_slice(&review.body).unwrap();
        assert_eq!(
            STANDARD
                .decode(wire["fileBase64"].as_str().unwrap())
                .unwrap(),
            bytes
        );
        assert_eq!(wire["fileName"], "synthetic.txt");
        assert_eq!(wire["confirm"], true);
        assert!(wire.get("force").is_none());
        assert!(wire.get("vendorId").is_none());
    }
    #[::core::prelude::v1::test]
    fn canonical_identity_and_scope_fail_closed_without_creating_replay_payload() {
        let hash = "a".repeat(64);
        let root = json!({"ok":true,"documents":{"slug":"synthetic","documents":[{"id":1,"sha256Hash":hash,"mimeType":"text/plain"}]}});
        assert_eq!(parse_documents(root.clone(), "synthetic").unwrap().len(), 1);
        assert!(parse_documents(root.clone(), "other").is_err());
        let mut invalid = root;
        invalid["documents"]["documents"][0]["sha256Hash"] = json!("bad");
        assert!(parse_documents(invalid, "synthetic").is_err());
        let data = Data {
            documents: vec![],
            profile: json!({}),
            principal: "owner".into(),
            ledger: "immutable".into(),
        };
        let source = Source {
            filename: "Private.txt".into(),
            mime: "text/plain",
            bytes: Arc::new(vec![1]),
            hash: "a".repeat(64),
        };
        let reviewed = review(source, json!({"source":"Private metadata"})).unwrap();
        let record = marker(
            &reviewed,
            "http://127.0.0.1:1234",
            "synthetic",
            &data,
            Some("owned-profile"),
        );
        assert!(!record.to_string().contains("Private"));
        assert!(record.get("body").is_none());
        assert!(scope(
            &record,
            "http://127.0.0.1:5678",
            "synthetic",
            &data,
            Some("owned-profile")
        ));
        assert!(!scope(
            &record,
            "http://127.0.0.1:1234",
            "synthetic",
            &data,
            Some("other-profile")
        ));
    }
    #[::core::prelude::v1::test]
    fn local_source_is_bounded_regular_whitelisted_and_exactly_hashed() {
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "rm-document-source-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("synthetic.txt");
        let bytes = b"Synthetic source\n";
        std::fs::write(&path, bytes).unwrap();
        let source = file(&path).unwrap();
        assert_eq!(source.bytes.as_slice(), bytes);
        assert_eq!(source.hash, format!("{:x}", Sha256::digest(bytes)));
        let link = dir.join("linked.txt");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(file(&link).is_err());
        assert!(file(&dir).is_err());
        let empty = dir.join("empty.txt");
        std::fs::write(&empty, []).unwrap();
        assert!(file(&empty).is_err());
        let unsupported = dir.join("synthetic.exe");
        std::fs::write(&unsupported, [1]).unwrap();
        assert!(file(&unsupported).is_err());
        let large = dir.join("large.txt");
        std::fs::File::create(&large)
            .unwrap()
            .set_len(MAX_BYTES as u64 + 1)
            .unwrap();
        assert!(file(&large).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
