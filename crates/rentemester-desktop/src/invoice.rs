//! Native domestic invoice workflow. Server owns totals, numbering, PDF and ledger effects.
#[path = "invoice_lifecycle.rs"]
mod lifecycle;
use crate::{Session, invoice_state, readable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    component::{
        Disableable,
        button::{Button, ButtonVariants},
        checkbox::Checkbox,
        input::{Input, InputState},
        select::{SearchableVec, Select, SelectState},
    },
    *,
};
use rentemester_client::{Confirmation, DomainMutation};
use serde_json::{Number, Value, json};
use std::{path::PathBuf, str::FromStr, sync::Arc};
type Choices = SelectState<SearchableVec<SharedString>>;
#[derive(Clone)]
struct Data {
    profile: Value,
    customers: Vec<Value>,
    invoices: Vec<Value>,
    revenue: Vec<Value>,
    documents: Vec<i64>,
    principal: String,
    ledger: String,
    start: String,
    end: String,
}
fn text(v: &Value, key: &str) -> Result<String, String> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Server response missing {key}"))
}
fn array(v: &Value) -> Result<Vec<Value>, String> {
    v.as_array()
        .cloned()
        .ok_or_else(|| "Server response missing invoice data".into())
}
fn read(session: &Session, company: &str) -> Result<Data, String> {
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let profile = session
        .runtime
        .block_on(session.client.company_profile(company))
        .map_err(readable)?;
    let contacts = session
        .runtime
        .block_on(session.client.contacts(company))
        .map_err(readable)?;
    let invoices = session
        .runtime
        .block_on(session.client.invoices(company, None))
        .map_err(readable)?;
    let documents = session
        .runtime
        .block_on(session.client.documents(company))
        .map_err(readable)?;
    if [
        me["ok"].clone(),
        profile["ok"].clone(),
        contacts["ok"].clone(),
        invoices["ok"].clone(),
        documents["ok"].clone(),
    ]
    .iter()
    .any(|v| v != true)
    {
        return Err("Serveren bekræftede ikke fakturagrundlaget".into());
    }
    let view = &invoices["invoices"];
    Ok(Data {
        profile: profile["company"].clone(),
        customers: array(&contacts["contacts"]["customers"])?,
        invoices: array(&view["invoices"])?,
        revenue: array(&view["nativeSales"]["revenueAccounts"])?,
        documents: array(&documents["documents"]["documents"])?
            .iter()
            .filter_map(|v| v["id"].as_i64())
            .collect(),
        principal: text(&me["user"], "id")?,
        ledger: text(view, "ledgerUuid")?,
        start: text(view, "periodStart")?,
        end: text(view, "periodEnd")?,
    })
}
fn party(v: &Value, seller: bool) -> Result<Value, String> {
    let keys = ["name", "address", if seller { "cvr" } else { "vatOrCvr" }];
    if keys
        .iter()
        .any(|key| v[key].as_str().is_none_or(|value| value.trim().is_empty()))
    {
        return Err(format!(
            "{} navn, adresse og CVR skal udfyldes i serverens stamdata før PDF-visningen.",
            if seller { "Sælgerens" } else { "Kundens" }
        ));
    }
    Ok(json!({"name":v["name"],"address":v["address"],"vatOrCvr":v[keys[2]]}))
}
fn source_matches(expected_profile: Option<&Value>, current: &Data, input: &Value) -> bool {
    expected_profile == Some(&current.profile)
        && current
            .customers
            .iter()
            .find(|customer| customer["id"] == input["customerId"])
            .and_then(|customer| party(customer, false).ok())
            .is_some_and(|buyer| buyer == input["buyer"])
}

fn decimal(raw: &str, places: usize) -> Result<Number, String> {
    let normalized = raw.trim().replace(',', ".");
    let mut parts = normalized.split('.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next().unwrap_or("");
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > places
        || parts.next().is_some()
    {
        return Err("Angiv et positivt decimaltal uden tusindtalsseparator. Prisen må have højst to decimaler.".into());
    }
    Number::from_str(&normalized).map_err(|_| "Decimaltallet kunne ikke læses".into())
}
fn body(
    data: &Data,
    customer: &Value,
    date: &str,
    due: &str,
    description: &str,
    quantity: &str,
    price: &str,
) -> Result<Vec<u8>, String> {
    if data.profile["currency"] != "DKK" || data.profile["vatPeriodType"].is_null() {
        return Err("Denne fakturaformular kræver en momsregistreret DKK-virksomhed. Tilpas stamdata i browseren.".into());
    }
    if date.len() != 10
        || date < data.start.as_str()
        || date > data.end.as_str()
        || due.len() != 10
        || description.trim().is_empty()
    {
        return Err("Angiv dato i det viste regnskabsår, forfaldsdato og linjetekst.".into());
    }
    let quantity = decimal(quantity, 6)?;
    let price = decimal(price, 2)?;
    serde_json::to_vec(&json!({"issueDate":date,"dueDate":due,"currency":"DKK","customerId":customer["id"],"seller":party(&data.profile,true)?,"buyer":party(customer,false)?,"vatRatePercent":25,"lines":[{"description":description,"quantity":quantity,"unitPriceExVat":price}]})).map_err(|_|"Fakturakladde kunne ikke oprettes".into())
}
fn same_scope(
    record: &Value,
    origin: &str,
    company: &str,
    data: &Data,
    local: Option<&str>,
) -> bool {
    record["company"] == company
        && record["principal"] == data.principal
        && record["ledger"] == data.ledger
        && (record["origin"] == origin
            || local.is_some_and(|identity| record["localIdentity"].as_str() == Some(identity)))
}
fn candidates(record: &Value, data: &Data) -> Vec<Value> {
    let input: Value = record["body"]
        .as_str()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null);
    let before = record["before"].as_array();
    data.invoices
        .iter()
        .filter(|v| {
            if record["operation"] == "post" {
                return v["documentId"] == input["invoiceDocumentId"];
            }
            v["customerId"] == input["customerId"]
                && v["invoiceDate"] == input["issueDate"]
                && !before.is_some_and(|ids| ids.contains(&v["documentId"]))
        })
        .cloned()
        .collect()
}
fn party_summary(value: &Value) -> String {
    format!(
        "{} · {} · CVR {}",
        value["name"].as_str().unwrap_or("?"),
        value["address"].as_str().unwrap_or("?"),
        value["vatOrCvr"].as_str().unwrap_or("?")
    )
}
fn request_summary(record: &Value) -> String {
    let input: Value = record["body"]
        .as_str()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null);
    if record["operation"] == "post" {
        return format!(
            "Særskilt bogføring af fakturabilag {} på indtægtskonto {}",
            input["invoiceDocumentId"],
            input["revenueAccountNo"].as_str().unwrap_or("?")
        );
    }
    format!(
        "Sælger: {}. Kunde: {}. Fakturadato: {}. Forfald: {}. Linje: {}. Antal: {}. Enhedspris ekskl. moms: {} DKK. 25% moms; se totalsummer i PDF.",
        party_summary(&input["seller"]),
        party_summary(&input["buyer"]),
        input["issueDate"].as_str().unwrap_or("?"),
        input["dueDate"].as_str().unwrap_or("?"),
        input["lines"][0]["description"].as_str().unwrap_or("?"),
        input["lines"][0]["quantity"],
        input["lines"][0]["unitPriceExVat"]
    )
}
struct Preview {
    body: Vec<u8>,
    profile: Value,
    pdf: PathBuf,
    opened: bool,
    reviewed: bool,
}
pub struct Invoice {
    entry_focus: FocusHandle,
    entry_focus_requested: bool,
    session: Arc<Session>,
    origin: String,
    company: String,
    local_identity: Option<String>,
    data: Option<Data>,
    customer: Entity<Choices>,
    revenue: Entity<Choices>,
    date: Entity<InputState>,
    due: Entity<InputState>,
    description: Entity<InputState>,
    quantity: Entity<InputState>,
    price: Entity<InputState>,
    preview: Option<Preview>,
    pending: Option<Value>,
    selected: Option<i64>,
    original: Option<PathBuf>,
    original_opened: bool,
    original_reviewed: bool,
    post_review: Option<Vec<u8>>,
    lifecycle: Option<Entity<lifecycle::Lifecycle>>,
    busy: bool,
    status: String,
}
impl Invoice {
    pub fn focus_entry(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.entry_focus_requested = true;
        cx.notify();
    }

    pub fn new(
        session: Arc<Session>,
        origin: String,
        company: String,
        local_identity: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = |label: &str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| {
                InputState::new(window, cx).placeholder(SharedString::from(label.to_owned()))
            })
        };
        let select = |window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| {
                SelectState::new(
                    SearchableVec::new(Vec::<SharedString>::new()),
                    None,
                    window,
                    cx,
                )
                .searchable(true)
            })
        };
        let mut this = Self {
            entry_focus: cx.focus_handle(),
            entry_focus_requested: false,
            session,
            origin,
            company,
            local_identity,
            data: None,
            customer: select(window, cx),
            revenue: select(window, cx),
            date: input("Fakturadato (YYYY-MM-DD)", window, cx),
            due: input("Forfaldsdato (YYYY-MM-DD)", window, cx),
            description: input("Fakturalinje", window, cx),
            quantity: input("Antal", window, cx),
            price: input("Enhedspris ekskl. moms i DKK", window, cx),
            preview: None,
            pending: None,
            selected: None,
            original: None,
            original_opened: false,
            original_reviewed: false,
            post_review: None,
            lifecycle: None,
            busy: false,
            status: String::new(),
        };
        this.refresh(window, cx);
        this
    }
    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status = "Henter kunder, stamdata, fakturaer og originalregister…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((read(&session, &company)?, invoice_state::load()?))
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this,window,cx| {
                this.busy = false;
                match result {
                    Ok((data,pending)) => {
                        if pending.as_ref().is_some_and(|record| !same_scope(record,&this.origin,&this.company,&data,this.local_identity.as_deref())) {
                            this.data = None;
                            this.status = "En anden server, virksomhed eller bruger har et uafklaret fakturaforsøg. Åbn den oprindelige forbindelse; der sendes ingen ny faktura.".into();
                            cx.notify();
                            return;
                        }
                        let customers = SearchableVec::new(data.customers.iter().map(|v| SharedString::from(format!("{} · {}",v["name"].as_str().unwrap_or("Kunde"),v["id"]))).collect::<Vec<_>>());
                        this.customer.update(cx, |state,cx| state.set_items(customers,window,cx));
                        let revenue = SearchableVec::new(data.revenue.iter().map(|v| SharedString::from(format!("{} · {}",v["accountNo"].as_str().unwrap_or("?"),v["name"].as_str().unwrap_or("?")))).collect::<Vec<_>>());
                        this.revenue.update(cx, |state,cx| state.set_items(revenue,window,cx));
                        this.pending = pending;
                        this.data = Some(data);
                        this.status = if this.pending.is_some() {
                            "Et tidligere udfald er uafklaret. Der er ingen automatisk gentagelse. Gennemgå den eksisterende original og bekræft resultatet; opret ikke en erstatningsfaktura."
                        } else {
                            "Opret en almindelig dansk B2B-faktura med én linje og 25% moms, eller vælg en eksisterende faktura."
                        }.into();
                    }
                    Err(error) => this.status = error,
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn preview(&mut self, cx: &mut Context<Self>) {
        let Some(data) = self.data.clone() else {
            return;
        };
        let selected = self.customer.read(cx).selected_value().cloned();
        let buyer = data.customers.iter().find(|v| {
            selected.as_deref()
                == Some(format!("{} · {}", v["name"].as_str().unwrap_or("Kunde"), v["id"]).as_str())
        });
        let result = buyer
            .ok_or_else(|| "Vælg en kunde fra serverens register".to_owned())
            .and_then(|buyer| {
                body(
                    &data,
                    buyer,
                    &self.date.read(cx).value(),
                    &self.due.read(cx).value(),
                    &self.description.read(cx).value(),
                    &self.quantity.read(cx).value(),
                    &self.price.read(cx).value(),
                )
            });
        let bytes = match result {
            Ok(v) => v,
            Err(e) => {
                self.status = e;
                cx.notify();
                return;
            }
        };
        self.busy = true;
        let session = self.session.clone();
        let company = self.company.clone();
        let task = cx.background_executor().spawn(async move {
            let pdf = session
                .runtime
                .block_on(session.client.invoice_preview(&company, bytes.clone()))
                .map_err(readable)?;
            Ok::<_, String>(Preview {
                body: bytes,
                profile: data.profile,
                pdf: invoice_state::pdf(&pdf, "preview")?,
                opened: false,
                reviewed: false,
            })
        });
        cx.spawn(async move |view,cx| {
            let result = task.await;
            let _ = view.update(cx, |this,cx| {
                this.busy = false;
                match result {
                    Ok(preview) => {
                        this.preview = Some(preview);
                        this.status = "PDF-forhåndsvisningen kommer fra serveren. Åbn den og gennemgå sælger, kunde, datoer, linje, moms og total. Fakturanummer reserveres først ved udstedelse.".into();
                    }
                    Err(error) => this.status = format!("{error}. Kontrollér datoer, linjetekst, kundens CVR/adresse og virksomhedens momsoplysninger."),
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn mutate(&mut self, posting: bool, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = self.data.clone() else {
            return;
        };
        let bytes = if posting {
            let Some(bytes) = self.post_review.clone() else {
                return;
            };
            bytes
        } else {
            let Some(preview) = &self.preview else {
                return;
            };
            if !preview.opened || !preview.reviewed {
                return;
            }
            preview.body.clone()
        };
        let origin = self.origin.clone();
        let company = self.company.clone();
        let local = self.local_identity.clone();
        let session = self.session.clone();
        let profile = self.preview.as_ref().map(|p| p.profile.clone());
        self.busy = true;
        self.status = if posting {
            "Sender særskilt bekræftet bogføring én gang…"
        } else {
            "Sender den gennemgåede faktura én gang…"
        }
        .into();
        let task=cx.background_executor().spawn(async move {
   let current=read(&session,&company)?;
   if current.ledger!=data.ledger||current.principal!=data.principal{return Err("Virksomhed eller bruger har ændret sig. Intet er sendt.".into());}
   let input:Value=serde_json::from_slice(&bytes).map_err(|_|"Ugyldig gennemgået anmodning")?;
   if !posting{
    if !source_matches(profile.as_ref(),&current,&input){return Err("Stamdata er ændret siden PDF-visningen. Hent grundlaget og gennemgå en ny PDF; intet er udstedt.".into());}
   }
   let record=json!({"format":"rentemester-desktop-invoice/1","operation":if posting{"post"}else{"issue"},"origin":origin,"company":company,"principal":current.principal,"ledger":current.ledger,"localIdentity":local,"body":String::from_utf8(bytes.clone()).map_err(|_|"Ugyldig fakturakladde")?,"before":current.invoices.iter().map(|v|v["documentId"].clone()).collect::<Vec<_>>()});
   invoice_state::begin(&record)?;
   let operation=if posting{DomainMutation::PostInvoice}else{DomainMutation::IssueInvoice};
   let result=match session.runtime.block_on(session.client.domain_mutation(&company,operation,bytes,Confirmation{confirm:true,invoice_preview_reviewed:!posting})){
    Ok(result)=>result,
    Err(error@rentemester_client::Error::Refused{status:400|401|403|404|409|422})=>{invoice_state::finish(&record)?;return Err(format!("{}. Serveren afviste forsøget. Gennemgå stamdata, kontoroller og din aktuelle adgang.",readable(error)));}
    Err(error)=>return Err(format!("{}. Udfaldet er uafklaret. Den præcise anmodning er gemt; der gentages ikke automatisk.",readable(error))),
   };
   if result["ok"]!=true{return Err("Serveren bekræftede ikke resultatet; forsøget bevares.".into());}
   let id=if posting{input["invoiceDocumentId"].as_i64()}else{result["invoice"]["documentId"].as_i64()}.filter(|id|*id>0).ok_or("Kvitteringen mangler faktura-id; forsøget bevares")?;
   if posting&&result["posting"]["ok"]!=true{return Err("Bogføringskvitteringen er uafklaret; forsøget bevares.".into());}
   invoice_state::finish(&record)?;
   Ok::<_,String>((id,read(&session,&company)))
  });
        cx.spawn(async move |view,cx| {
            let result = task.await;
            let _ = view.update(cx, |this,cx| {
                this.busy = false;
                match invoice_state::load() {
                    Ok(pending) => this.pending = pending,
                    Err(error) => {
                        this.status = format!("{error}. Det gemte forsøg bevares; genindlæs før næste skridt.");
                        this.data = None;
                        cx.notify();
                        return;
                    }
                }
                if this.pending.is_some() { this.preview = None; this.post_review = None; }
                match result {
                    Ok((id,readback)) => {
                        this.preview = None;
                        this.post_review = None;
                        this.selected = Some(id);
                        this.original = None;
                        this.original_opened = false;
                        this.original_reviewed = false;
                        match readback {
                            Ok(data) => {
                                let verified = data.invoices.iter().any(|v| v["documentId"]==id && (!posting || v["posted"]==true)) && data.documents.contains(&id);
                                this.data = Some(data);
                                this.status = format!("{} · Fakturabilag {id}. {}", if posting { "Bogføring bekræftet" } else { "Udstedelse bekræftet; indtægten er endnu ikke bogført" }, if verified { "Faktura og originalregister er læst tilbage fra serveren." } else { "Kvittering modtaget; genindlæs faktura og original før næste skridt." });
                            }
                            Err(error) => this.status = format!("Kvittering bekræftet for bilag {id}; genindlæsning fejlede: {error}"),
                        }
                    }
                    Err(error) => this.status = error,
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn original(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        if self.busy {
            return;
        }
        self.busy = true;
        let session = self.session.clone();
        let company = self.company.clone();
        let task = cx.background_executor().spawn(async move {
            let pdf = session
                .runtime
                .block_on(session.client.invoice_pdf(&company, id))
                .map_err(readable)?;
            invoice_state::pdf(&pdf, "pdf")
        });
        cx.spawn(async move |view,cx| {
            let result = task.await;
            let _ = view.update(cx, |this,cx| {
                this.busy = false;
                match result {
                    Ok(path) => {
                        this.original = Some(path);
                        this.original_opened = false;
                        this.original_reviewed = false;
                        this.status = "Den eksisterende original er hentet fra serveren. Åbn PDF og gennemgå den før bekræftelse.".into();
                    }
                    Err(error) => this.status = error,
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
    fn review_post(&mut self, cx: &mut Context<Self>) {
        let (Some(id), Some(data)) = (self.selected, self.data.as_ref()) else {
            return;
        };
        let value = self.revenue.read(cx).selected_value();
        let account = data.revenue.iter().find(|v| {
            value.is_some_and(|s| {
                s.as_ref()
                    == format!(
                        "{} · {}",
                        v["accountNo"].as_str().unwrap_or("?"),
                        v["name"].as_str().unwrap_or("?")
                    )
            })
        });
        match account {
            Some(account) => {
                self.post_review=serde_json::to_vec(&json!({"invoiceDocumentId":id,"revenueAccountNo":account["accountNo"],"confirm":true})).ok();
                self.status="Gennemgå faktura-id og indtægtskonto. Serveren bruger de bekræftede kontoroller for debitorer og salgsmoms og den udstedte originals beløb.".into();
            }
            None => self.status = "Vælg en indtægtskonto fra serverens kontoplan.".into(),
        }
        cx.notify();
    }
    fn open_lifecycle(
        &mut self,
        id: i64,
        recovery: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.preview.is_some() || self.post_review.is_some() {
            return;
        }
        if !recovery && (!self.original_reviewed || self.pending.is_some()) {
            return;
        }
        let session = self.session.clone();
        let origin = self.origin.clone();
        let company = self.company.clone();
        let local = self.local_identity.clone();
        let reviewed = self.original_reviewed && !recovery;
        self.lifecycle = Some(cx.new(|cx| {
            lifecycle::Lifecycle::new(session, origin, company, local, id, reviewed, window, cx)
        }));
        cx.notify();
    }
    fn adopt(&mut self, cx: &mut Context<Self>) {
        if !self.original_opened || !self.original_reviewed {
            return;
        }
        let (Some(record), Some(data), Some(id)) =
            (self.pending.as_ref(), self.data.as_ref(), self.selected)
        else {
            return;
        };
        if !candidates(record, data)
            .iter()
            .any(|v| v["documentId"] == id)
        {
            return;
        }
        if record["operation"] == "post"
            && !data
                .invoices
                .iter()
                .any(|v| v["documentId"] == id && v["posted"] == true)
        {
            self.status="Serveren bekræfter endnu ikke bogføring. Forsøget bevares; opret ikke en erstatning.".into();
            cx.notify();
            return;
        }
        match invoice_state::finish(record) {
            Ok(()) => {
                self.pending = None;
                self.preview = None;
                self.post_review = None;
                self.status="Den eksisterende original er valgt som det manuelt afklarede resultat. Ingen anmodning blev gentaget.".into();
            }
            Err(e) => self.status = e,
        }
        cx.notify();
    }
}
impl Render for Invoice {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.entry_focus_requested && !self.busy {
            self.entry_focus_requested = false;
            let focus = self.entry_focus.clone();
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        }
        if let Some(lifecycle) = self.lifecycle.clone() {
            return div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    Button::new("invoice-lifecycle-close")
                        .track_focus(&self.entry_focus)
                        .label("Luk betaling/kredit og genindlæs fakturaer")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.lifecycle = None;
                            this.refresh(window, cx);
                        })),
                )
                .child(lifecycle);
        }
        let mut view = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_xl().child("Fakturaer"))
            .child(format!("{} · {}", self.company, self.origin))
            .child(self.status.clone())
            .child(
                Button::new("invoice-refresh")
                    .when(
                        self.preview.is_none() && self.post_review.is_none(),
                        |button| button.track_focus(&self.entry_focus),
                    )
                    .label("Genindlæs fakturaer og originalregister")
                    .disabled(self.busy || self.preview.is_some() || self.post_review.is_some())
                    .on_click(cx.listener(|this, _, window, cx| this.refresh(window, cx))),
            );
        let Some(data) = self.data.clone() else {
            return view;
        };
        if let Some(record) = self
            .pending
            .as_ref()
            .filter(|record| invoice_state::is_lifecycle(record))
        {
            let id = record["invoiceDocumentId"].as_i64().unwrap_or(0);
            let origin = record["origin"].as_str().unwrap_or(&self.origin).to_owned();
            return view.child(format!("Uafklaret {} af fakturabilag{id}. Ingen ny faktura, kredit eller betaling sendes. Genlæs og afklar manuelt.",record["operation"].as_str().unwrap_or("handling")))
                .child(Button::new("invoice-lifecycle-recovery").label("Åbn fakturaens betaling/kredit til manuel afklaring").disabled(self.busy||id<=0).on_click(cx.listener(move|this,_,window,cx|this.open_lifecycle(id,true,window,cx))))
                .child(Button::new("invoice-lifecycle-recovery-browser").label("Åbn oprindelig server i browser").on_click(move|_,_,cx|cx.open_url(&format!("{origin}/"))));
        }
        let recovery_origin = self.origin.clone();
        if self.pending.is_some() {
            view = view.child(
                Button::new("invoice-browser-reconcile")
                    .label("Åbn serveren i browser til manuel afklaring")
                    .on_click(move |_, _, cx| cx.open_url(&format!("{recovery_origin}/"))),
            );
        }
        let items = if let Some(record) = &self.pending {
            view=view.child(format!("Uafklaret {} fra {}. Der udsendes ingen ny faktura. Åbn et eksisterende resultat og sammenhold PDF med den gemte anmodning.",record["operation"].as_str().unwrap_or("operation"),record["origin"].as_str().unwrap_or("?"))).child(request_summary(record));
            candidates(record, &data)
        } else {
            data.invoices.clone()
        };
        for invoice in items {
            let id = invoice["documentId"].as_i64().unwrap_or(0);
            view = view.child(
                Button::new(SharedString::from(format!("invoice-{id}")))
                    .label(format!(
                        "{} · {} · {} DKK · {}",
                        invoice["invoiceNo"].as_str().unwrap_or("?"),
                        invoice["customerName"].as_str().unwrap_or("?"),
                        invoice["grossAmount"],
                        if invoice["posted"] == true {
                            "Bogført"
                        } else {
                            "Ikke bogført"
                        }
                    ))
                    .disabled(self.busy || self.preview.is_some() || self.post_review.is_some())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = Some(id);
                        this.original = None;
                        this.original_opened = false;
                        this.original_reviewed = false;
                        cx.notify();
                    })),
            );
        }
        if self.pending.is_some() && self.selected.is_none() {
            return view.child("Ingen sikker automatisk identifikation findes. Genindlæs og gennemgå serverens fakturaer. Den uafklarede anmodning bevares; en ny udstedelse er blokeret.");
        }
        if let Some(id) = self.selected {
            let invoice = data.invoices.iter().find(|v| v["documentId"] == id);
            view = view
                .child(format!(
                    "Valgt fakturabilag {id}: {}",
                    invoice.map(|v| v.to_string()).unwrap_or_else(|| {
                        "Ikke fundet i det aktuelle regnskabsår; genindlæs før næste skridt".into()
                    })
                ))
                .child(
                    Button::new("invoice-original-fetch")
                        .label("Hent udstedt original PDF")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.original(cx))),
                );
            if let Some(path) = self.original.clone() {
                view = view
                    .child(
                        Button::new("invoice-original-open")
                            .label("Åbn udstedt PDF")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.open_url(&invoice_state::file_url(&path));
                                this.original_opened = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("invoice-original-reviewed")
                            .label("Jeg har gennemgået den udstedte original PDF")
                            .checked(self.original_reviewed)
                            .disabled(!self.original_opened || self.busy)
                            .on_change(cx.listener(|this, value, _, cx| {
                                this.original_reviewed = *value;
                                cx.notify();
                            })),
                    );
            }
            if self.pending.is_some() {
                return view.child(
                    Button::new("invoice-adopt")
                        .label("Bekræft dette eksisterende resultat efter PDF-kontrol")
                        .disabled(self.busy || !self.original_reviewed)
                        .on_click(cx.listener(|this, _, _, cx| this.adopt(cx))),
                );
            }
            if invoice.is_some_and(|v| v["posted"] != true) {
                if let Some(bytes) = &self.post_review {
                    let wire: Value = serde_json::from_slice(bytes).unwrap_or(Value::Null);
                    view=view.child(format!("Særskilt bogføring: fakturabilag {} · indtægtskonto {} · fakturaens originale beløb, moms og debitor fra serveren.",wire["invoiceDocumentId"],wire["revenueAccountNo"].as_str().unwrap_or("?"))).child(Button::new("invoice-post-confirm").track_focus(&self.entry_focus).primary().label("Bekræft særskilt indtægtsbogføring").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.mutate(true,cx)))).child(Button::new("invoice-post-back").label("Tilbage og vælg konto").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.post_review=None;cx.notify();})));
                } else {
                    view = view
                        .child("Indtægtskonto")
                        .child(
                            Select::new(&self.revenue)
                                .accessibility_label("Indtægtskonto")
                                .placeholder("Vælg indtægtskonto")
                                .disabled(self.busy),
                        )
                        .child(
                            Button::new("invoice-post-review")
                                .label("Gennemse særskilt bogføring")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| this.review_post(cx))),
                        );
                }
            }
            if invoice.is_some_and(|invoice| invoice["posted"] == true) {
                view = view.child(
                    Button::new("invoice-lifecycle-open")
                        .label("Indbetaling, kredit og tilbagebetaling")
                        .disabled(self.busy || !self.original_reviewed)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_lifecycle(id, false, window, cx)
                        })),
                );
            }
            return view.child(
                Button::new("invoice-new")
                    .label("Ny faktura")
                    .disabled(self.busy || self.post_review.is_some())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.selected = None;
                        this.original = None;
                        this.original_opened = false;
                        this.original_reviewed = false;
                        cx.notify();
                    })),
            );
        }
        if let Some(preview) = &self.preview {
            let request: Value = serde_json::from_slice(&preview.body).unwrap_or(Value::Null);
            view=view.child("Gennemgå præcis kilde og PDF før udstedelse").child(format!("Sælger: {} · Kunde: {} · Dato: {} · Forfald: {}",party_summary(&request["seller"]),party_summary(&request["buyer"]),request["issueDate"].as_str().unwrap_or("?"),request["dueDate"].as_str().unwrap_or("?"))).child(format!("Linje: {} · Antal: {} · Enhedspris ekskl. moms: {} DKK · 25% moms. Totalsummer beregnes af serveren og fremgår af PDF.",request["lines"][0]["description"].as_str().unwrap_or("?"),request["lines"][0]["quantity"],request["lines"][0]["unitPriceExVat"]));
            let path = preview.pdf.clone();
            return view
                .child(
                    Button::new("invoice-preview-open")
                        .track_focus(&self.entry_focus)
                        .label("Åbn serverens PDF-forhåndsvisning")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.open_url(&invoice_state::file_url(&path));
                            if let Some(p) = this.preview.as_mut() {
                                p.opened = true;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Checkbox::new("invoice-preview-reviewed")
                        .label("Jeg har gennemgået PDF med kunde, datoer, linje, moms og total")
                        .checked(preview.reviewed)
                        .disabled(!preview.opened || self.busy)
                        .on_change(cx.listener(|this, value, _, cx| {
                            if let Some(p) = this.preview.as_mut() {
                                p.reviewed = *value;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("invoice-issue")
                        .primary()
                        .label("Udsted den gennemgåede faktura")
                        .disabled(self.busy || !preview.reviewed)
                        .on_click(cx.listener(|this, _, _, cx| this.mutate(false, cx))),
                )
                .child(
                    Button::new("invoice-edit")
                        .label("Tilbage og ret; ny PDF kræves")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.preview = None;
                            cx.notify();
                        })),
                );
        }
        view.child(format!("Regnskabsår: {} – {}. Dansk B2B, DKK,25% moms. Sælger og kunde hentes fra kanoniske stamdata; totalsummer fremgår af PDF.",data.start,data.end)).child("Kunde").child(Select::new(&self.customer).accessibility_label("Fakturakunde").placeholder("Vælg kunde").disabled(self.busy)).child("Fakturadato").child(Input::new(&self.date).disabled(self.busy)).child("Forfaldsdato").child(Input::new(&self.due).disabled(self.busy)).child("Fakturalinje").child(Input::new(&self.description).disabled(self.busy)).child("Antal").child(Input::new(&self.quantity).disabled(self.busy)).child("Enhedspris ekskl. moms i DKK").child(Input::new(&self.price).disabled(self.busy)).child(Button::new("invoice-preview").label("Hent PDF-forhåndsvisning").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.preview(cx))))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn reviewed_request_freezes_canonical_source_and_contains_no_client_totals() {
        let buyer = json!({"id":7,"name":"Synthetic Buyer","address":"Synthetic Buyer Street2","vatOrCvr":"DK87654321"});
        let mut data = Data {
            profile: json!({"name":"Synthetic Seller","address":"Synthetic Seller Street1","cvr":"12345678","currency":"DKK","vatPeriodType":"quarter","payment":{"iban":"synthetic"}}),
            customers: vec![buyer.clone()],
            invoices: vec![],
            revenue: vec![],
            documents: vec![],
            principal: "owner".into(),
            ledger: "ledger".into(),
            start: "2026-01-01".into(),
            end: "2026-12-31".into(),
        };
        let bytes = body(
            &data,
            &buyer,
            "2026-10-19",
            "2026-11-02",
            "Synthetic service",
            "2",
            "100,00",
        )
        .unwrap();
        let input: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(input["buyer"]["vatOrCvr"], "DK87654321");
        assert_eq!(input["seller"]["name"], "Synthetic Seller");
        assert_eq!(input["lines"][0]["unitPriceExVat"].as_f64(), Some(100.0));
        assert!(input.get("totals").is_none());
        let original = data.profile.clone();
        assert!(source_matches(Some(&original), &data, &input));
        data.profile["payment"]["iban"] = json!("changed");
        assert!(!source_matches(Some(&original), &data, &input));
        data.profile = original.clone();
        data.customers[0]["address"] = json!("changed");
        assert!(!source_matches(Some(&original), &data, &input));
        assert!(
            body(
                &data,
                &buyer,
                "2025-10-19",
                "2025-11-02",
                "Synthetic",
                "1",
                "100"
            )
            .is_err()
        );
    }
    #[::core::prelude::v1::test]
    fn invoice_values_are_wire_numbers_without_local_totals() {
        assert_eq!(decimal("12,34", 2).unwrap().to_string(), "12.34");
        assert!(decimal("1.001", 2).is_err());
        assert!(decimal("1e3", 2).is_err());
    }
    #[::core::prelude::v1::test]
    fn reconciliation_never_adopts_preexisting_or_different_customer_invoice() {
        let data = Data {
            profile: Value::Null,
            customers: vec![],
            revenue: vec![],
            documents: vec![],
            principal: "owner".into(),
            ledger: "ledger".into(),
            start: "2026-01-01".into(),
            end: "2026-12-31".into(),
            invoices: vec![
                json!({"documentId":1,"customerId":7,"invoiceDate":"2026-10-19"}),
                json!({"documentId":2,"customerId":7,"invoiceDate":"2026-10-19"}),
                json!({"documentId":3,"customerId":8,"invoiceDate":"2026-10-19"}),
            ],
        };
        let record = json!({"operation":"issue","body":"{\"customerId\":7,\"issueDate\":\"2026-10-19\"}","before":[1],"company":"synthetic","principal":"owner","ledger":"ledger","origin":"https://old.invalid","localIdentity":"local-profile"});
        let result = candidates(&record, &data);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["documentId"], 2);
        assert!(!same_scope(
            &record,
            "https://other.invalid",
            "synthetic",
            &data,
            None
        ));
        assert!(same_scope(
            &record,
            "http://127.0.0.1:2",
            "synthetic",
            &data,
            Some("local-profile")
        ));
    }
}
