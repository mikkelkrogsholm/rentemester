//! Separate reviewed DKK receipt/credit/refund operations; no local accounting or resend.
use super::{Data as InvoiceData, decimal, read as read_invoice};
use crate::{Session, invoice_state, readable};
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
use rentemester_client::{Error, ReceivablesMutation, Report};
use serde_json::{Value, json};
use std::sync::Arc;
type Choices = SelectState<SearchableVec<SharedString>>;
#[derive(Clone, Copy, PartialEq)]
enum Action {
    Receipt,
    Credit,
    Refund,
}
impl Action {
    fn name(self) -> &'static str {
        match self {
            Self::Receipt => "Indbetaling",
            Self::Credit => "Kreditnota",
            Self::Refund => "Tilbagebetaling",
        }
    }
    fn operation(self) -> &'static str {
        match self {
            Self::Receipt => "receipt",
            Self::Credit => "credit",
            Self::Refund => "refund",
        }
    }
    fn mutation(self) -> ReceivablesMutation {
        match self {
            Self::Receipt => ReceivablesMutation::Settle,
            Self::Credit => ReceivablesMutation::Credit,
            Self::Refund => ReceivablesMutation::Refund,
        }
    }
}
#[derive(Clone)]
struct Data {
    invoice: Value,
    core: InvoiceData,
    bank: Value,
    journal: Value,
}
fn text(value: &Value, key: &str) -> Result<String, String> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Serveren mangler {key}"))
}
fn read(session: &Session, company: &str, id: i64) -> Result<Data, String> {
    let core = read_invoice(session, company)?;
    let invoice = core
        .invoices
        .iter()
        .find(|invoice| invoice["documentId"] == id)
        .cloned()
        .ok_or(
            "Fakturaen mangler i det aktuelle regnskabsår; genindlæs den oprindelige virksomhed",
        )?;
    if invoice["posted"] != true || invoice["currency"] != "DKK" {
        return Err("Denne formular kræver en bekræftet bogført DKK-faktura".into());
    }
    text(&invoice, "invoiceNo")?;
    for key in ["grossAmount", "openBalance", "creditableBalance"] {
        if !invoice[key].is_number() {
            return Err(format!("Serveren mangler fakturabeløbet {key}"));
        }
    }
    let bank = session
        .runtime
        .block_on(session.client.bank(company, None))
        .map_err(readable)?;
    let journal = session
        .runtime
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    if bank["ok"] != true
        || journal["ok"] != true
        || bank["bank"]["slug"] != company
        || journal["journal"]["slug"] != company
        || journal["journal"]["ledgerUuid"] != core.ledger
    {
        return Err("Serveren bekræftede ikke fakturaens bank-/posteringsgrundlag".into());
    }
    if bank["bank"]["transactions"].as_array().is_none()
        || journal["journal"]["entries"].as_array().is_none()
    {
        return Err("Serveren mangler bankrækker eller posteringer".into());
    }
    Ok(Data {
        invoice,
        core,
        bank: bank["bank"].clone(),
        journal: journal["journal"].clone(),
    })
}
fn bank_label(row: &Value) -> String {
    format!(
        "id{} · {} · {} · Beløb {}",
        row["id"],
        row["date"].as_str().unwrap_or("?"),
        row["text"].as_str().unwrap_or("?"),
        row["amount"]
    )
}
#[derive(Clone)]
struct Review {
    action: Action,
    body: Vec<u8>,
    invoice: Value,
    bank: Option<Value>,
}
fn request(
    action: Action,
    invoice: &Value,
    bank: Option<&Value>,
    date: &str,
    reason: &str,
    gross: &str,
) -> Result<Review, String> {
    let id = invoice["documentId"]
        .as_i64()
        .filter(|id| *id > 0)
        .ok_or("Vælg en kanonisk faktura")?;
    if invoice["posted"] != true || invoice["currency"] != "DKK" {
        return Err("Bogført DKK-faktura kræves".into());
    }
    let mut body = json!({"invoiceDocumentId":id,"confirm":true});
    if action == Action::Credit {
        if date.len() != 10 || reason.trim().is_empty() {
            return Err("Angiv kreditnotadato (YYYY-MM-DD) og begrundelse".into());
        }
        body["issueDate"] = json!(date);
        body["reason"] = json!(reason.trim());
        if !gross.trim().is_empty() {
            body["grossAmount"] = json!(
                decimal(gross, 2)
                    .map_err(|_| "Angiv præcist delkreditbeløb med højst to decimaler")?
            );
        } else {
            if !invoice["creditableBalance"].is_number() {
                return Err("Serveren mangler resterende kreditbeløb".into());
            }
            body["grossAmount"] = invoice["creditableBalance"].clone();
        }
    } else {
        let row = bank.ok_or("Vælg en faktisk importeret bankrække")?;
        let id = row["id"]
            .as_i64()
            .filter(|id| *id > 0)
            .ok_or("Bankrækkens id mangler")?;
        if row["reconciliationStatus"] != "unmatched" {
            return Err("Bankrækken er allerede afstemt; genindlæs og vælg en anden".into());
        }
        body["bankTransactionId"] = json!(id);
    }
    Ok(Review {
        action,
        body: serde_json::to_vec(&body).map_err(|_| "Anmodningen kunne ikke fastholdes")?,
        invoice: invoice.clone(),
        bank: bank.cloned(),
    })
}
fn current(review: &Review, data: &Data) -> bool {
    review.invoice == data.invoice
        && review.bank.as_ref().is_none_or(|old| {
            data.bank["transactions"].as_array().is_some_and(|rows| {
                rows.contains(old) && old["reconciliationStatus"] == "unmatched"
            })
        })
}
fn scope(record: &Value, origin: &str, company: &str, data: &Data, local: Option<&str>) -> bool {
    record["company"] == company
        && record["principal"] == data.core.principal
        && record["ledger"] == data.core.ledger
        && record["localIdentity"].as_str() == local
        && (record["origin"] == origin
            || local.is_some_and(|identity| record["localIdentity"].as_str() == Some(identity)))
}
fn marker(review: &Review, origin: &str, company: &str, data: &Data, local: Option<&str>) -> Value {
    let body: Value = serde_json::from_slice(&review.body).unwrap_or(Value::Null);
    json!({"format":"rentemester-desktop-invoice/1","operation":review.action.operation(),"invoiceDocumentId":body["invoiceDocumentId"],"bankTransactionId":body["bankTransactionId"],"grossAmount":body["grossAmount"],"issueDate":body["issueDate"],"origin":origin,"company":company,"principal":data.core.principal,"ledger":data.core.ledger,"localIdentity":local})
}
struct Receipt {
    entry: i64,
    document: Option<i64>,
    message: String,
}
fn receipt(value: &Value, review: &Review) -> Option<Receipt> {
    if value["ok"] != true {
        return None;
    }
    let result = &value[match review.action {
        Action::Receipt => "settlement",
        Action::Credit => "creditNote",
        Action::Refund => "refund",
    }];
    if review.action == Action::Credit {
        if result["originalInvoiceNumber"] != review.invoice["invoiceNo"] {
            return None;
        }
        let entry = result["journalEntryId"].as_i64().filter(|id| *id > 0)?;
        let document = result["documentId"].as_i64().filter(|id| *id > 0)?;
        let number = result["creditNoteNumber"]
            .as_str()
            .filter(|s| !s.is_empty())?;
        Some(Receipt {
            entry,
            document: Some(document),
            message: format!("Kreditnota {number} bekræftet · bilag{document} · postering{entry}."),
        })
    } else {
        if result["invoiceNumber"] != review.invoice["invoiceNo"] {
            return None;
        }
        let entry = result["entryId"].as_i64().filter(|id| *id > 0)?;
        let key = if review.action == Action::Refund {
            "refundId"
        } else {
            "paymentId"
        };
        let id = result[key].as_i64().filter(|id| *id > 0)?;
        let balance = if review.action == Action::Refund {
            "remainingCreditBalance"
        } else {
            "openBalance"
        };
        if !result[balance].is_number() {
            return None;
        }
        Some(Receipt {
            entry,
            document: None,
            message: format!(
                "{} bekræftet · id{id} · postering{entry} · {} {} (serverberegnet).",
                review.action.name(),
                if review.action == Action::Refund {
                    "Resterende tilgodehavende"
                } else {
                    "Åbent beløb"
                },
                result[balance]
            ),
        })
    }
}
pub struct Lifecycle {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    id: i64,
    data: Option<Data>,
    action: Action,
    bank: Entity<Choices>,
    date: Entity<InputState>,
    reason: Entity<InputState>,
    gross: Entity<InputState>,
    review: Option<Review>,
    reviewed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
    last_entry: Option<i64>,
    original_reviewed: bool,
}
impl Lifecycle {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        company: String,
        local: Option<String>,
        id: i64,
        original_reviewed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let bank = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(Vec::<SharedString>::new()),
                None,
                window,
                cx,
            )
            .searchable(true)
        });
        let input = |label: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(label))
        };
        let mut this = Self {
            session,
            origin,
            company,
            local,
            id,
            data: None,
            action: Action::Receipt,
            bank,
            date: input("Kreditnotadato YYYY-MM-DD", window, cx),
            reason: input("Begrundelse for kreditnota", window, cx),
            gross: input(
                "Delkredit inkl. moms; tomt = hele resterende kreditbeløb",
                window,
                cx,
            ),
            review: None,
            reviewed: false,
            pending: None,
            resolved: false,
            busy: false,
            status: String::new(),
            last_entry: None,
            original_reviewed,
        };
        this.load(window, cx);
        this
    }
    fn install(&mut self, data: Data, window: &mut Window, cx: &mut Context<Self>) {
        let rows = data.bank["transactions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| row["reconciliationStatus"] == "unmatched")
            .map(|row| SharedString::from(bank_label(row)))
            .collect::<Vec<_>>();
        self.bank.update(cx, |state, cx| {
            state.set_items(SearchableVec::new(rows), window, cx)
        });
        self.data = Some(data);
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
        self.status = "Genlæser faktura, bank og kanoniske posteringer…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let id = self.id;
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((read(&session, &company, id)?, invoice_state::load()?))
        });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,window,cx|{this.busy=false;match result{Ok((data,pending))=>{this.install(data,window,cx);this.pending=pending;this.status="Beløb, saldo, konti, bankretning og perioder kontrolleres af serveren. Hver handling kræver særskilt gennemgang.".into();},Err(error)=>this.status=error}cx.notify();});}).detach();
        cx.notify();
    }
    fn review(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() || !self.original_reviewed {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        let selected = self.bank.read(cx).selected_value();
        let bank = data.bank["transactions"].as_array().and_then(|rows| {
            rows.iter()
                .find(|row| selected.is_some_and(|label| label.as_ref() == bank_label(row)))
        });
        match request(
            self.action,
            &data.invoice,
            bank,
            &self.date.read(cx).value(),
            &self.reason.read(cx).value(),
            &self.gross.read(cx).value(),
        ) {
            Ok(review) => {
                self.review = Some(review);
                self.reviewed = false;
                self.status =
                    "Gennemgå handling, faktura og præcise kildefelter før bekræftelse.".into();
            }
            Err(error) => self.status = error,
        }
        cx.notify();
    }
    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.reviewed || self.pending.is_some() || !self.original_reviewed {
            return;
        }
        let (Some(review), Some(data)) = (self.review.clone(), self.data.clone()) else {
            return;
        };
        self.busy = true;
        self.review = None;
        self.reviewed = false;
        self.data = None;
        self.status = "Sender præcis den gennemgåede handling én gang…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let origin = self.origin.clone();
        let local = self.local.clone();
        let id = self.id;
        let task=cx.background_executor().spawn(async move{
            let fresh=read(&session,&company,id)?;
            if data.core.principal!=fresh.core.principal||data.core.ledger!=fresh.core.ledger||!current(&review,&fresh){return Err("Faktura, bankrække eller bruger ændrede sig; intet sendt. Genindlæs og gennemgå på ny.".into());}
            let record=marker(&review,&origin,&company,&fresh,local.as_deref());invoice_state::begin(&record)?;
            let result=session.runtime.block_on(session.client.receivables_mutation(&company,review.action.mutation(),review.body.clone(),true));
            let (message,confirmed)=match result{
                Ok(value)=>if let Some(receipt)=receipt(&value,&review){invoice_state::finish(&record)?;(receipt.message.clone(),Some(receipt))}else{("Kvitteringen er uafklaret. Handlingen kan være gennemført; den gentages ikke.".into(),None)},
                Err(Error::Refused{status})=>{invoice_state::finish(&record)?;(format!("{} afvist (HTTP{status}). Kontrollér bankretning og afstemning, beløb, resterende saldo, originalens konti, periode og adgang. Ingen ny handling er sendt.",review.action.name()),None)},
                Err(Error::InvalidInput(_))=>{invoice_state::finish(&record)?;("Felterne blev afvist lokalt før netværkskald. Ret dato, begrundelse eller præcist beløb og gennemgå igen.".into(),None)},
                _=>("Udfaldet er uafklaret. Ingen genafsendelse eller ny kreditnota foretages. Genlæs og afklar faktura, bank og originaler manuelt i den oprindelige virksomhed.".into(),None),
            };
            Ok::<_,String>((message,confirmed,read(&session,&company,id),invoice_state::load()?))
        });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,window,cx|{this.busy=false;match result{
            Ok((message,receipt,data,pending))=>{this.status=message;this.pending=pending;if let Some(receipt)=receipt{this.last_entry=Some(receipt.entry);if let Ok(data)=&data{let journal=data.journal["entries"].as_array().is_some_and(|rows|rows.iter().any(|row|row["id"]==receipt.entry));let document=receipt.document.is_none_or(|id|data.core.documents.contains(&id));this.status.push_str(if journal&&document{" Kanonisk postering og eventuelt kreditnotabilag er læst tilbage."}else{" Kvitteringen er bekræftet; genindlæs for at se postering/original i den aktuelle periode."});}}
                match data{Ok(data)=>this.install(data,window,cx),Err(error)=>this.status.push_str(&format!(" Genlæsning fejlede: {error}"))}
            },Err(error)=>{this.status=error;this.pending=invoice_state::load().ok().flatten();}
        }cx.notify();});}).detach();
        cx.notify();
    }
}
impl Render for Lifecycle {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut view = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_xl()
                    .child("Indbetaling, kredit og tilbagebetaling"),
            )
            .child(format!(
                "{} · {} · fakturabilag{}",
                self.company, self.origin, self.id
            ))
            .child(self.status.clone())
            .child(
                Button::new("lifecycle-refresh")
                    .label("Genlæs faktura, bank og posteringer")
                    .disabled(self.busy || self.review.is_some())
                    .on_click(cx.listener(|this, _, window, cx| this.load(window, cx))),
            );
        let Some(data) = &self.data else {
            return view;
        };
        view=view.child(format!("Faktura {} · Brutto {} DKK · Åbent {} DKK · Resterende krediterbart {} DKK · Status {}",data.invoice["invoiceNo"].as_str().unwrap_or("?"),data.invoice["grossAmount"],data.invoice["openBalance"],data.invoice["creditableBalance"],data.invoice["status"].as_str().unwrap_or("?")))
            .child("Saldi og konti kommer fra serveren. Kreditering bruger oprindelig debitor, indtægt og moms. Bankkonto følger den importerede rækkes tilknytning.");
        if let Some(record) = self.pending.clone() {
            view=view.child(format!("UAFKLARET {} af fakturabilag{} · bankrække{} · kreditbeløb{} · kreditnotadato{}. Ingen genafsendelse eller erstatningskreditnota.",record["operation"].as_str().unwrap_or("handling"),record["invoiceDocumentId"],record["bankTransactionId"],record["grossAmount"],record["issueDate"]))
                .child("Genlæs faktura, bankafstemning, kreditnotabilag og posteringer. Projekterede saldi er ikke i sig selv kvittering for præcis dette forsøg.");
            if let Some(origin) = record["origin"].as_str() {
                let url = format!("{origin}/");
                view = view.child(
                    Button::new("lifecycle-browser")
                        .label("Åbn oprindelig server til manuel afklaring")
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                );
            }
            if invoice_state::is_lifecycle(&record)
                && record["invoiceDocumentId"] == self.id
                && scope(
                    &record,
                    &self.origin,
                    &self.company,
                    data,
                    self.local.as_deref(),
                )
            {
                view=view.child(Checkbox::new("lifecycle-manually-resolved").label("Jeg har manuelt afklaret præcis denne handling i den korrekte virksomhed. Arkivér status uden genafsendelse.").checked(self.resolved).on_click(cx.listener(|this,checked,_,cx|{this.resolved=*checked;cx.notify();})))
                    .child(Button::new("lifecycle-resolve").label("Bekræft manuel afklaring").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{
                        if !this.resolved||this.busy{return;}let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};if !scope(record,&this.origin,&this.company,data,this.local.as_deref())||record["invoiceDocumentId"]!=this.id{return;}
                        match invoice_state::resolve_lifecycle(record){Ok(())=>{this.pending=None;this.resolved=false;this.status="Manuel afklaring registreret uden genafsendelse. Genlæs før en ny selvstændig handling.".into();},Err(error)=>this.status=error}cx.notify();
                    })));
            } else {
                view=view.child("Åbn den oprindelige faktura, bruger og virksomhed for at afklare. Alle nye mutationer er blokeret.");
            }
        } else if !self.original_reviewed {
            view=view.child("Luk denne visning og gennemgå fakturaens udstedte original PDF før en ny selvstændig handling.");
        } else if let Some(review) = self.review.clone() {
            let body: Value = serde_json::from_slice(&review.body).unwrap_or(Value::Null);
            view = view.child(div().text_lg().child(format!(
                "Gennemgå {} · faktura{}",
                review.action.name(),
                self.id
            )));
            if review.action == Action::Credit {
                view=view.child(format!("Kreditnotadato {} · Bruttokredit {} DKK · Begrundelse {}. Serveren beregner netto/moms og bogfører kreditnotaen på originalens konti.",body["issueDate"].as_str().unwrap_or("?"),body["grossAmount"],body["reason"].as_str().unwrap_or("?")));
            } else if let Some(bank) = &review.bank {
                view=view.child(bank_label(bank)).child("Hele den importerede bankrækkes beløb og dato bruges af serveren. Indbetaling kræver positiv DKK-række; tilbagebetaling kræver udgående DKK-række og et tilgodehavende. Ingen rest eller fortegn beregnes lokalt.");
            }
            view = view
                .child(
                    Checkbox::new("lifecycle-reviewed")
                        .label("Jeg har gennemgået præcis faktura, handling og kilde/beløb.")
                        .checked(self.reviewed)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            this.reviewed = *checked;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("lifecycle-confirm")
                        .label(format!("Bekræft {} én gang", review.action.name()))
                        .disabled(self.busy || !self.reviewed)
                        .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
                )
                .child(
                    Button::new("lifecycle-cancel")
                        .label("Annullér gennemgang")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.review = None;
                            this.reviewed = false;
                            cx.notify();
                        })),
                );
        } else {
            let mut actions = div().flex().gap_2().flex_wrap();
            for (id, action) in [
                ("lifecycle-receipt", Action::Receipt),
                ("lifecycle-credit", Action::Credit),
                ("lifecycle-refund", Action::Refund),
            ] {
                actions = actions.child(
                    Button::new(id)
                        .label(action.name())
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.action = action;
                            this.review = None;
                            this.reviewed = false;
                            cx.notify();
                        })),
                );
            }
            view = view
                .child(actions)
                .child(format!("Valgt handling: {}", self.action.name()));
            if self.action == Action::Credit {
                view=view.child("Kreditnotadato").child(Input::new(&self.date).disabled(self.busy)).child("Begrundelse").child(Input::new(&self.reason).disabled(self.busy))
                    .child("Bruttobeløb for delkredit; tomt = hele viste resterende kreditbeløb, fastholdt til gennemgang").child(Input::new(&self.gross).disabled(self.busy));
            } else {
                view = view
                    .child(format!(
                        "Importerede, uafstemte bankrækker · {} til {} · alle bankkonti",
                        data.bank["periodStart"].as_str().unwrap_or("?"),
                        data.bank["periodEnd"].as_str().unwrap_or("?")
                    ))
                    .child(
                        Select::new(&self.bank)
                            .placeholder("Vælg faktisk bankrække")
                            .accessibility_label("Fakturaens bankrække")
                            .disabled(self.busy),
                    );
            }
            view = view.child(
                Button::new("lifecycle-review")
                    .label(format!("Gennemgå {}", self.action.name()))
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.review(cx))),
            );
        }
        view = view.child(
            div()
                .text_lg()
                .child("Kanoniske posteringer · alle virksomhedens posteringer i aktuel periode"),
        );
        for entry in data.journal["entries"].as_array().into_iter().flatten() {
            view = view.child(format!(
                "{} · {} · {} · bilag{}{}",
                entry["entryNo"].as_str().unwrap_or("?"),
                entry["date"].as_str().unwrap_or("?"),
                entry["text"].as_str().unwrap_or("?"),
                entry["documentId"],
                if self.last_entry == entry["id"].as_i64() {
                    " · seneste bekræftede kvittering"
                } else {
                    ""
                }
            ));
            for line in entry["lines"].as_array().into_iter().flatten() {
                view = view.child(format!(
                    "{} · {} · Debet {} · Kredit {}",
                    line["accountNo"].as_str().unwrap_or("?"),
                    line["accountName"].as_str().unwrap_or("?"),
                    line["debit"],
                    line["credit"]
                ));
            }
        }
        view
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn invoice() -> Value {
        json!({"documentId":7,"invoiceNo":"SYN-7","posted":true,"currency":"DKK","grossAmount":250,"openBalance":250,"creditableBalance":250})
    }
    #[::core::prelude::v1::test]
    fn receipt_and_refund_select_actual_bank_id_without_local_amount_or_date_math() {
        for action in [Action::Receipt, Action::Refund] {
            let bank = json!({"id":9,"date":"2026-10-19","amount":-125,"reconciliationStatus":"unmatched"});
            let review = request(action, &invoice(), Some(&bank), "", "", "").unwrap();
            let body: Value = serde_json::from_slice(&review.body).unwrap();
            assert_eq!(
                body,
                json!({"invoiceDocumentId":7,"bankTransactionId":9,"confirm":true})
            );
        }
        assert!(
            request(
                Action::Receipt,
                &invoice(),
                Some(&json!({"id":9,"reconciliationStatus":"matched"})),
                "",
                "",
                ""
            )
            .is_err()
        );
    }
    #[::core::prelude::v1::test]
    fn credit_freezes_exact_partial_or_server_remaining_amount_without_totals_calculation() {
        let partial = request(
            Action::Credit,
            &invoice(),
            None,
            "2026-10-19",
            "Synthetic correction",
            "12,34",
        )
        .unwrap();
        let body: Value = serde_json::from_slice(&partial.body).unwrap();
        assert_eq!(body["grossAmount"].to_string(), "12.34");
        assert!(body.get("vatAmount").is_none());
        let full = request(
            Action::Credit,
            &invoice(),
            None,
            "2026-10-19",
            "Synthetic remaining",
            "",
        )
        .unwrap();
        let body: Value = serde_json::from_slice(&full.body).unwrap();
        assert_eq!(body["grossAmount"], 250);
        assert!(request(Action::Credit, &invoice(), None, "2026-10-19", "", "12.345").is_err());
    }
    #[::core::prelude::v1::test]
    fn receipts_require_original_invoice_identity_and_positive_entry_evidence() {
        let review = request(
            Action::Credit,
            &invoice(),
            None,
            "2026-10-19",
            "Synthetic",
            "",
        )
        .unwrap();
        assert!(receipt(&json!({"ok":true,"creditNote":{"originalInvoiceNumber":"other","journalEntryId":8,"documentId":9,"creditNoteNumber":"CN-SYN"}}),&review).is_none());
        assert!(receipt(&json!({"ok":true,"creditNote":{"originalInvoiceNumber":"SYN-7","journalEntryId":8,"documentId":9,"creditNoteNumber":"CN-SYN"}}),&review).is_some());
        assert!(receipt(&json!({"ok":true,"creditNote":{"originalInvoiceNumber":"SYN-7","journalEntryId":0,"documentId":9,"creditNoteNumber":"CN-SYN"}}),&review).is_none());
    }
    fn data() -> Data {
        let invoice = invoice();
        let bank =
            json!({"id":9,"date":"2026-10-19","amount":125,"reconciliationStatus":"unmatched"});
        Data {
            invoice: invoice.clone(),
            bank: json!({"transactions":[bank]}),
            journal: json!({"entries":[]}),
            core: InvoiceData {
                profile: json!({}),
                customers: vec![],
                invoices: vec![invoice],
                revenue: vec![],
                documents: vec![7],
                principal: "owner".into(),
                ledger: "immutable".into(),
                start: "2026-01-01".into(),
                end: "2026-12-31".into(),
            },
        }
    }
    #[::core::prelude::v1::test]
    fn stale_invoice_balance_or_consumed_bank_row_blocks_reviewed_request() {
        let mut data = data();
        let bank = data.bank["transactions"][0].clone();
        let review = request(Action::Receipt, &data.invoice, Some(&bank), "", "", "").unwrap();
        assert!(current(&review, &data));
        data.invoice["openBalance"] = json!(100);
        assert!(!current(&review, &data));
        data.invoice = invoice();
        data.bank["transactions"][0]["reconciliationStatus"] = json!("matched");
        assert!(!current(&review, &data));
    }
    #[::core::prelude::v1::test]
    fn uncertainty_scope_and_marker_never_retain_credit_reason_or_customer_identity() {
        let data = data();
        let review = request(
            Action::Credit,
            &data.invoice,
            None,
            "2026-10-19",
            "Synthetic private reason",
            "12.34",
        )
        .unwrap();
        let record = marker(
            &review,
            "http://127.0.0.1:1234",
            "synthetic",
            &data,
            Some("owned-profile"),
        );
        assert_eq!(record["grossAmount"].to_string(), "12.34");
        assert!(!record.to_string().contains("Synthetic private reason"));
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
        assert!(!scope(
            &record,
            "http://127.0.0.1:1234",
            "other",
            &data,
            Some("owned-profile")
        ));
    }
}
