//! One paid domestic receipt. Native server owns allocation, roles, posting and reconciliation.
#[path = "expense_state.rs"]
mod state;
use super::{Data as DocumentData, read as read_documents};
use crate::{Session, readable};
use gpui_kit::{
    component::{
        Disableable,
        button::Button,
        checkbox::Checkbox,
        select::{SearchableVec, Select, SelectState},
    },
    *,
};
use rentemester_client::{BankMutation, Error, Report, bank_body_schema};
use serde_json::{Value, json};
use std::sync::Arc;
type Choices = SelectState<SearchableVec<SharedString>>;
pub fn pending() -> Result<Option<Value>, String> {
    state::load()
}
#[derive(Clone)]
struct Data {
    core: DocumentData,
    document: Value,
    accounts: Vec<Value>,
    outgoing: Vec<Value>,
    bank: Value,
    journal: Value,
    trial: Value,
    income: Value,
}
fn rows(v: &Value, key: &str) -> Result<Vec<Value>, String> {
    v[key]
        .as_array()
        .cloned()
        .ok_or_else(|| format!("Serveren mangler {key}"))
}
pub(super) fn unposted(document: &Value) -> bool {
    document["currency"] == "DKK"
        && matches!(
            document["documentType"].as_str(),
            Some("purchase_sale" | "cash_register_receipt")
        )
        && document.get("journalEntryId") == Some(&Value::Null)
}
fn read(session: &Session, company: &str, id: i64) -> Result<Data, String> {
    let core = read_documents(session, company)?;
    let document = core
        .documents
        .iter()
        .find(|v| v["id"] == id)
        .ok_or("Bilaget mangler i kanonisk liste")?;
    let root = session
        .runtime
        .block_on(session.client.expense_booking_options(company, id))
        .map_err(readable)?;
    if root["ok"] != true
        || root["options"]["document"]["id"] != id
        || root["options"]["document"]["sha256Hash"] != document["sha256Hash"]
    {
        return Err("Serveren bekræftede ikke bilagets bogføringsgrundlag".into());
    }
    let document = root["options"]["document"].clone();
    if document["currency"] != "DKK" || core.profile["currency"] != "DKK" {
        return Err("Denne native visning understøtter kun betalte DKK-bilag i DKK-virksomheder. Valutaomregning tilbydes ikke her.".into());
    }
    let accounts = rows(&root["options"], "expenseAccounts")?;
    let outgoing = rows(&root["options"], "unmatchedOutgoingBank")?
        .into_iter()
        .filter(|row| row["currency"] == "DKK")
        .collect();
    let bank = session
        .runtime
        .block_on(session.client.bank(company, None))
        .map_err(readable)?;
    let journal = session
        .runtime
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    let trial = session
        .runtime
        .block_on(
            session
                .client
                .report(company, Report::TrialBalance, None, None),
        )
        .map_err(readable)?;
    let income = session
        .runtime
        .block_on(
            session
                .client
                .report(company, Report::IncomeStatement, None, None),
        )
        .map_err(readable)?;
    for (root, key) in [
        (&bank, "bank"),
        (&journal, "journal"),
        (&trial, "trialBalance"),
        (&income, "incomeStatement"),
    ] {
        if root["ok"] != true || root[key]["slug"] != company {
            return Err("Serveren bekræftede ikke bank-/rapportgrundlaget".into());
        }
    }
    for (root, key) in [
        (&journal, "journal"),
        (&trial, "trialBalance"),
        (&income, "incomeStatement"),
    ] {
        if root[key]["ledgerUuid"] != core.ledger {
            return Err("Rapporten tilhører en anden ledger".into());
        }
    }
    if !trial["trialBalance"]["totalDebit"].is_number()
        || !trial["trialBalance"]["totalCredit"].is_number()
        || !trial["trialBalance"]["balanced"].is_boolean()
        || !income["incomeStatement"]["totalExpense"].is_number()
        || !income["incomeStatement"]["result"].is_number()
    {
        return Err("Serverrapporterne mangler beløb eller balancekontrol".into());
    }
    rows(&income["incomeStatement"], "expense")?;
    rows(&journal["journal"], "entries")?;
    rows(&bank["bank"], "transactions")?;
    Ok(Data {
        core,
        document,
        accounts,
        outgoing,
        bank: bank["bank"].clone(),
        journal: journal["journal"].clone(),
        trial: trial["trialBalance"].clone(),
        income: income["incomeStatement"].clone(),
    })
}
fn bank_label(row: &Value) -> String {
    format!(
        "id{} · {} · {} · {} DKK",
        row["id"],
        row["date"].as_str().unwrap_or("?"),
        row["text"].as_str().unwrap_or("?"),
        row["amount"]
    )
}
fn account_label(row: &Value) -> String {
    format!(
        "{} · {}",
        row["accountNo"].as_str().unwrap_or("?"),
        row["name"].as_str().unwrap_or("?")
    )
}
fn treatments() -> Vec<String> {
    bank_body_schema(BankMutation::MatchExpense)["properties"]["vatTreatment"]["enum"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect()
}
fn treatment_label(value: &str) -> String {
    format!(
        "{} · {}",
        value,
        match value {
            "standard" => "Almindelig købsmoms",
            "exempt" => "Momsfrit",
            "non_deductible" => "Ingen momsfradrag",
            _ => "Servervalideret valg",
        }
    )
}
#[derive(Clone)]
struct Review {
    document: Value,
    bank: Value,
    account: Value,
    treatment: String,
    body: Vec<u8>,
}
fn request(data: &Data, bank: &Value, account: &Value, treatment: &str) -> Result<Review, String> {
    if !unposted(&data.document) {
        return Err("Vælg et endnu ikke bogført købsbilag eller en kassekvittering i DKK".into());
    }
    if !data.outgoing.contains(bank)
        || bank["currency"] != "DKK"
        || !data.accounts.contains(account)
        || !treatments().contains(&treatment.to_owned())
    {
        return Err("Vælg bankrække og omkostningskonto fra aktuelle servermuligheder samt eksplicit momsbehandling".into());
    }
    let document_id = data.document["id"]
        .as_i64()
        .filter(|id| *id > 0)
        .ok_or("Bilags-id mangler")?;
    let bank_id = bank["id"]
        .as_i64()
        .filter(|id| *id > 0)
        .ok_or("Bank-id mangler")?;
    let account_no = account["accountNo"]
        .as_str()
        .filter(|no| !no.is_empty())
        .ok_or("Kontonummer mangler")?;
    let body=serde_json::to_vec(&json!({"documentId":document_id,"bankTransactionId":bank_id,"expenseAccountNo":account_no,"vatTreatment":treatment,"confirm":true})).map_err(|_|"Bogføringsanmodningen kunne ikke fastholdes")?;
    Ok(Review {
        document: data.document.clone(),
        bank: bank.clone(),
        account: account.clone(),
        treatment: treatment.into(),
        body,
    })
}
fn current(review: &Review, data: &Data) -> bool {
    unposted(&data.document)
        && data.document == review.document
        && data.outgoing.contains(&review.bank)
        && data.accounts.contains(&review.account)
        && treatments().contains(&review.treatment)
}
fn scope(record: &Value, origin: &str, company: &str, data: &Data, local: Option<&str>) -> bool {
    record["company"] == company
        && record["principal"] == data.core.principal
        && record["ledger"] == data.core.ledger
        && record["documentId"] == data.document["id"]
        && record["documentHash"] == data.document["sha256Hash"]
        && record["localIdentity"].as_str() == local
        && (record["origin"] == origin
            || local.is_some_and(|identity| record["localIdentity"].as_str() == Some(identity)))
}
fn marker(review: &Review, origin: &str, company: &str, data: &Data, local: Option<&str>) -> Value {
    json!({"format":"rentemester-desktop-expense/1","origin":origin,"company":company,"principal":data.core.principal,"ledger":data.core.ledger,"localIdentity":local,"documentId":review.document["id"],"documentHash":review.document["sha256Hash"],"bankTransactionId":review.bank["id"],"expenseAccountNo":review.account["accountNo"],"vatTreatment":review.treatment})
}
struct Receipt {
    entry: i64,
    number: String,
    message: String,
}
fn receipt(value: &Value, review: &Review) -> Option<Receipt> {
    let result = &value["booking"];
    if value["ok"] != true
        || result["ok"] != true
        || result["documentId"] != review.document["id"]
        || result["bankTransactionId"] != review.bank["id"]
        || result["vatTreatment"] != review.treatment
    {
        return None;
    }
    let entry = result["entryId"].as_i64().filter(|id| *id > 0)?;
    let number = result["entryNo"]
        .as_str()
        .filter(|no| !no.is_empty())?
        .to_owned();
    for key in ["netAmountDkk", "vatAmountDkk", "grossAmountDkk"] {
        if !result[key].is_number() {
            return None;
        }
    }
    Some(Receipt {
        entry,
        number: number.clone(),
        message: format!(
            "Bogføring bekræftet: {number} · id{entry} · Netto {} DKK · Fradragsmoms {} DKK · Brutto {} DKK (serverberegnet).",
            result["netAmountDkk"], result["vatAmountDkk"], result["grossAmountDkk"]
        ),
    })
}
pub struct Expense {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    id: i64,
    reviewed_hash: Option<String>,
    data: Option<Data>,
    bank: Entity<Choices>,
    account: Entity<Choices>,
    vat: Entity<Choices>,
    review: Option<Review>,
    reviewed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
    last_entry: Option<i64>,
}
impl Expense {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        company: String,
        local: Option<String>,
        id: i64,
        reviewed_hash: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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
            session,
            origin,
            company,
            local,
            id,
            reviewed_hash,
            data: None,
            bank: select(window, cx),
            account: select(window, cx),
            vat: select(window, cx),
            review: None,
            reviewed: false,
            pending: None,
            resolved: false,
            busy: false,
            status: String::new(),
            last_entry: None,
        };
        this.load(window, cx);
        this
    }
    fn install(&mut self, data: Data, window: &mut Window, cx: &mut Context<Self>) {
        self.bank.update(cx, |state, cx| {
            state.set_items(
                SearchableVec::new(
                    data.outgoing
                        .iter()
                        .map(|v| SharedString::from(bank_label(v)))
                        .collect::<Vec<_>>(),
                ),
                window,
                cx,
            )
        });
        self.account.update(cx, |state, cx| {
            state.set_items(
                SearchableVec::new(
                    data.accounts
                        .iter()
                        .map(|v| SharedString::from(account_label(v)))
                        .collect::<Vec<_>>(),
                ),
                window,
                cx,
            )
        });
        self.vat.update(cx, |state, cx| {
            state.set_items(
                SearchableVec::new(
                    treatments()
                        .iter()
                        .map(|v| SharedString::from(treatment_label(v)))
                        .collect::<Vec<_>>(),
                ),
                window,
                cx,
            )
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
        self.status = "Henter aktuelle bogføringsmuligheder, bank og rapporter…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let id = self.id;
        let task = cx
            .background_executor()
            .spawn(async move { Ok::<_, String>((read(&session, &company, id)?, pending()?)) });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,window,cx|{this.busy=false;match result{Ok((data,pending))=>{this.install(data,window,cx);this.pending=pending;this.status="Vælg en faktisk udgående bankrække og en omkostningskonto fra serveren. Momsvalgene er protokolmuligheder; serveren validerer behandling, kilder og aktuelle roller ved bekræftelse.".into();},Err(error)=>this.status=error}cx.notify();});}).detach();
        cx.notify();
    }
    fn review(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        if self.reviewed_hash.as_deref() != data.document["sha256Hash"].as_str() {
            self.status = "Gennemgå den præcise kanoniske original før bogføring".into();
            cx.notify();
            return;
        }
        let bank = self.bank.read(cx).selected_value();
        let account = self.account.read(cx).selected_value();
        let vat = self.vat.read(cx).selected_value();
        let bank = data
            .outgoing
            .iter()
            .find(|row| bank.is_some_and(|selected| selected.as_ref() == bank_label(row)));
        let account = data
            .accounts
            .iter()
            .find(|row| account.is_some_and(|selected| selected.as_ref() == account_label(row)));
        let treatment = treatments()
            .into_iter()
            .find(|value| vat.is_some_and(|selected| selected.as_ref() == treatment_label(value)));
        let result = match (bank, account, treatment) {
            (Some(bank), Some(account), Some(treatment)) => {
                request(data, bank, account, &treatment)
            }
            _ => Err("Vælg bankrække, omkostningskonto og momsbehandling".into()),
        };
        match result {
            Ok(review) => {
                self.review = Some(review);
                self.reviewed = false;
                self.status="Gennemgå præcist bilag, bankrække, konto og momsvalg. Der beregnes ikke moms lokalt.".into();
            }
            Err(error) => self.status = error,
        }
        cx.notify();
    }
    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.reviewed || self.pending.is_some() {
            return;
        }
        let (Some(review), Some(data)) = (self.review.clone(), self.data.clone()) else {
            return;
        };
        if self.reviewed_hash.as_deref() != data.document["sha256Hash"].as_str() {
            return;
        }
        self.busy = true;
        self.data = None;
        self.review = None;
        self.reviewed = false;
        self.status = "Genlæser kilder og sender den gennemgåede bogføring én gang…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let origin = self.origin.clone();
        let local = self.local.clone();
        let id = self.id;
        let bank_id = review.bank["id"].clone();
        let task=cx.background_executor().spawn(async move{
            let fresh=read(&session,&company,id)?;if fresh.core.principal!=data.core.principal||fresh.core.ledger!=data.core.ledger||!current(&review,&fresh){return Err("Bilag, bankrække, kontovalg eller bruger ændrede sig; intet sendt. Genlæs og gennemgå igen.".into());}
            let record=marker(&review,&origin,&company,&fresh,local.as_deref());state::begin(&record)?;
            let response=session.runtime.block_on(session.client.bank_mutation(&company,BankMutation::MatchExpense,review.body.clone(),true));
            let(message,receipt)=match response{
                Ok(value)=>if let Some(receipt)=receipt(&value,&review){state::finish(&record,false)?;(receipt.message.clone(),Some(receipt))}else{("Kvitteringen er uafklaret; bilaget kan være bogført. Der sendes ikke igen.".into(),None)},
                Err(Error::Refused{status})=>{state::finish(&record,false)?;(format!("Bogføring afvist (HTTP{status}). Kontrollér original, bankbeløb/afstemning, momsvalg, aktuelle kontoroller, periode og adgang. Et bilag registreret som leverandørgæld skal betales gennem gældsflowet. Ingen automatisk gentagelse."),None)},
                Err(Error::InvalidInput(_))=>{state::finish(&record,false)?;("Valgene blev afvist lokalt før netværkskald. Genlæs og gennemgå igen.".into(),None)},
                _=>("Udfaldet er uafklaret. Bogføring kan være sket; ingen genafsendelse eller automatisk adoption foretages. Afklar original, bankafstemning og journal manuelt.".into(),None),
            };Ok::<_,String>((message,receipt,read(&session,&company,id),pending()?))
        });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,window,cx|{this.busy=false;match result{Ok((message,receipt,data,pending))=>{this.status=message;this.pending=pending;if let Some(receipt)=receipt{this.last_entry=Some(receipt.entry);if let Ok(data)=&data{let doc=data.document["journalEntryId"]==receipt.entry;let bank=data.bank["transactions"].as_array().is_some_and(|rows|rows.iter().any(|row|row["id"]==bank_id&&row["reconciliationStatus"]=="matched"&&row["journalEntryNo"]==receipt.number));let journal=data.journal["entries"].as_array().is_some_and(|rows|rows.iter().any(|row|row["id"]==receipt.entry&&row["documentId"]==this.id));this.status.push_str(if doc&&bank&&journal{" Bilag, bankafstemning og journal er læst kanonisk tilbage."}else{" Kvittering bekræftet; genlæs bilag/bank/journal i den relevante periode for fuldt læsebevis."});}}match data{Ok(data)=>this.install(data,window,cx),Err(error)=>this.status.push_str(&format!(" Genlæsning fejlede: {error}"))}},Err(error)=>{this.status=error;this.pending=pending().ok().flatten();}}cx.notify();});}).detach();
        cx.notify();
    }
}
impl Render for Expense {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut view = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_xl().child("Bogfør betalt DKK-bilag"))
            .child(format!(
                "{} · {} · bilag{}",
                self.company, self.origin, self.id
            ))
            .child(self.status.clone())
            .child(
                Button::new("expense-refresh")
                    .label("Genlæs bilag, bogføringsmuligheder, bank og rapporter")
                    .disabled(self.busy || self.review.is_some())
                    .on_click(cx.listener(|this, _, window, cx| this.load(window, cx))),
            );
        let Some(data) = &self.data else {
            return view;
        };
        view=view.child(format!("Bilag {} · Dato {} · Brutto {} DKK · Oprindelig moms {} DKK · SHA-256 {}",data.document["documentNo"].as_str().unwrap_or("?"),data.document["invoiceDate"].as_str().unwrap_or("?"),data.document["amountIncVat"],data.document["vatAmount"],data.document["sha256Hash"].as_str().unwrap_or("?")))
            .child("Serveren vælger betalingskonto fra banktilknytning/aktuel rolle og beregner udgift og fradragsmoms fra originalen. Bogføringsdato følger den valgte bankrække; ingen lokal momsberegning.");
        if let Some(record) = self.pending.clone() {
            view=view.child(format!("UAFKLARET bogføring af bilag{} · bankrække{} · konto{} · momsvalg {}. Ingen genafsendelse eller automatisk adoption.",record["documentId"],record["bankTransactionId"],record["expenseAccountNo"].as_str().unwrap_or("?"),record["vatTreatment"].as_str().unwrap_or("?")))
                .child("Genlæs original/hash, bankafstemning, journal og rapport. Projekterede rækker er ikke i sig selv kvittering for dette forsøg.");
            if let Some(origin) = record["origin"].as_str() {
                let url = format!("{origin}/");
                view = view.child(
                    Button::new("expense-browser")
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
                view=view.child(Checkbox::new("expense-manually-resolved").label("Jeg har afklaret præcis dette udfald i den korrekte virksomhed. Arkivér status uden genafsendelse.").checked(self.resolved).on_click(cx.listener(|this,checked,_,cx|{this.resolved=*checked;cx.notify();})))
                    .child(Button::new("expense-resolve").label("Bekræft manuel afklaring").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{if !this.resolved||this.busy{return;}let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};if !scope(record,&this.origin,&this.company,data,this.local.as_deref()){return;}match state::finish(record,true){Ok(())=>{this.pending=None;this.resolved=false;this.reviewed_hash=None;this.status="Manuel afklaring registreret uden genafsendelse. Luk og gennemgå originalen på ny før en anden selvstændig bogføring.".into();},Err(error)=>this.status=error}cx.notify();})));
            } else {
                view=view.child("Åbn den oprindelige bruger, virksomhed, profil og original for at afklare. Nye bogføringer er blokeret.");
            }
        } else if !unposted(&data.document) {
            view=view.child("Bilaget er allerede knyttet til bogføring eller er uden for dette betalte DKK-flow. Der tilbydes ingen ny bogføring.");
        } else if self.reviewed_hash.as_deref() != data.document["sha256Hash"].as_str() {
            view=view.child("Luk og gennemgå den præcise hashkontrollerede original i bilagsvisningen før bogføring.");
        } else if let Some(review) = self.review.clone() {
            view=view.child(div().text_lg().child("Gennemgå præcis bogføring"))
                .child(format!("Bilag{} · SHA-256 {}",review.document["id"],review.document["sha256Hash"].as_str().unwrap_or("?")))
                .child(bank_label(&review.bank)).child(account_label(&review.account)).child(treatment_label(&review.treatment))
                .child(format!("Oprindelig moms {} DKK · Oprindeligt bruttobeløb {} DKK · Bogføringsdato {}. Serveren kontrollerer beløbsmatch, momsregistrering, roller, periode og adgang ved bekræftelse.",review.document["vatAmount"],review.document["amountIncVat"],review.bank["date"].as_str().unwrap_or("?")))
                .child(Checkbox::new("expense-reviewed").label("Jeg har gennemgået originalen og præcis bankrække, konto og momsvalg.").checked(self.reviewed).on_click(cx.listener(|this,checked,_,cx|{this.reviewed=*checked;cx.notify();})))
                .child(Button::new("expense-confirm").label("Bekræft betalt bilagsbogføring én gang").disabled(self.busy||!self.reviewed).on_click(cx.listener(|this,_,window,cx|this.send(window,cx))))
                .child(Button::new("expense-cancel").label("Annullér gennemgang").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.reviewed=false;cx.notify();})));
        } else {
            view=view.child("Faktisk importeret, uafstemt udgående DKK-bankrække fra serveren")
                .child(Select::new(&self.bank).placeholder("Vælg betalt bankrække").accessibility_label("Udgiftens bankrække").disabled(self.busy))
                .child("Aktiv omkostningskonto fra serverens bogføringsmuligheder")
                .child(Select::new(&self.account).placeholder("Vælg omkostningskonto").accessibility_label("Udgiftens omkostningskonto").disabled(self.busy))
                .child("Eksplicit momsvalg · serveren validerer, om behandling og aktuelle roller er tilladt")
                .child(Select::new(&self.vat).placeholder("Vælg momsbehandling").accessibility_label("Udgiftens momsbehandling").disabled(self.busy))
                .child(Button::new("expense-review").label("Gennemgå betalt bilagsbogføring").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|this.review(cx))));
        }
        view = view.child(
            div()
                .text_lg()
                .child("Kanonisk bankafstemning · valgt års bankrækker"),
        );
        for row in data.bank["transactions"].as_array().into_iter().flatten() {
            view = view.child(format!(
                "id{} · {} · {} · {} · postering {}",
                row["id"],
                row["date"].as_str().unwrap_or("?"),
                row["amount"],
                row["reconciliationStatus"].as_str().unwrap_or("?"),
                row["journalEntryNo"].as_str().unwrap_or("Ikke afstemt")
            ));
        }
        view = view.child(
            div()
                .text_lg()
                .child("Kanoniske posteringer knyttet til bilaget"),
        );
        for entry in data.journal["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| entry["documentId"] == self.id)
        {
            view = view.child(format!(
                "{} · {} · {}{}",
                entry["entryNo"].as_str().unwrap_or("?"),
                entry["date"].as_str().unwrap_or("?"),
                entry["text"].as_str().unwrap_or("?"),
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
        view = view
            .child(
                div()
                    .text_lg()
                    .child("Serverrapporter · hele virksomhedens aktuelle regnskabsår"),
            )
            .child(format!(
                "{} til {} · Saldobalance debet {} / kredit {} · Balanceret {}",
                data.trial["periodStart"].as_str().unwrap_or("?"),
                data.trial["periodEnd"].as_str().unwrap_or("?"),
                data.trial["totalDebit"],
                data.trial["totalCredit"],
                data.trial["balanced"]
            ))
            .child(format!(
                "Resultatopgørelse · Omkostninger i alt {} · Resultat {}",
                data.income["totalExpense"], data.income["result"]
            ));
        for row in data.income["expense"].as_array().into_iter().flatten() {
            view = view.child(format!(
                "{} · {} · {}",
                row["accountNo"].as_str().unwrap_or("?"),
                row["name"].as_str().unwrap_or("?"),
                row["amount"]
            ));
        }
        view
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn data() -> Data {
        let document = json!({"id":7,"documentType":"purchase_sale","currency":"DKK","journalEntryId":null,"sha256Hash":"a".repeat(64),"amountIncVat":125,"vatAmount":25});
        Data {
            core: DocumentData {
                documents: vec![document.clone()],
                profile: json!({"currency":"DKK"}),
                principal: "owner".into(),
                ledger: "immutable".into(),
            },
            document,
            accounts: vec![
                json!({"accountNo":"X","name":"Synthetic expense","defaultVatCode":"DK_PURCHASE_25"}),
            ],
            outgoing: vec![
                json!({"id":9,"date":"2026-10-19","amount":-125,"currency":"DKK","text":"Synthetic outgoing"}),
            ],
            bank: json!({"transactions":[]}),
            journal: json!({"entries":[]}),
            trial: json!({}),
            income: json!({}),
        }
    }
    #[::core::prelude::v1::test]
    fn exact_request_uses_server_sources_and_explicit_treatment_without_amount_math() {
        let data = data();
        let review = request(&data, &data.outgoing[0], &data.accounts[0], "standard").unwrap();
        let body: Value = serde_json::from_slice(&review.body).unwrap();
        assert_eq!(
            body,
            json!({"documentId":7,"bankTransactionId":9,"expenseAccountNo":"X","vatTreatment":"standard","confirm":true})
        );
        assert!(body.get("vatAmount").is_none());
        assert!(body.get("paymentAccountNo").is_none());
        assert!(request(&data, &json!({"id":99}), &data.accounts[0], "standard").is_err());
        assert!(request(&data, &data.outgoing[0], &data.accounts[0], "invented").is_err());
    }
    #[::core::prelude::v1::test]
    fn consumed_bank_changed_account_or_posted_document_blocks_stale_review() {
        let mut data = data();
        let review = request(&data, &data.outgoing[0], &data.accounts[0], "standard").unwrap();
        assert!(current(&review, &data));
        data.outgoing.clear();
        assert!(!current(&review, &data));
        data.outgoing.push(review.bank.clone());
        data.accounts[0]["defaultVatCode"] = json!("changed");
        assert!(!current(&review, &data));
        data.accounts[0] = review.account.clone();
        data.document["journalEntryId"] = json!(8);
        assert!(!current(&review, &data));
    }
    #[::core::prelude::v1::test]
    fn receipt_requires_exact_document_bank_treatment_and_authoritative_amounts() {
        let data = data();
        let review = request(&data, &data.outgoing[0], &data.accounts[0], "standard").unwrap();
        let mut value = json!({"ok":true,"booking":{"ok":true,"documentId":7,"bankTransactionId":9,"vatTreatment":"standard","entryId":8,"entryNo":"SYN-8","netAmountDkk":100,"vatAmountDkk":25,"grossAmountDkk":125}});
        assert!(receipt(&value, &review).is_some());
        value["booking"]["bankTransactionId"] = json!(99);
        assert!(receipt(&value, &review).is_none());
        value["booking"]["bankTransactionId"] = json!(9);
        value["booking"]
            .as_object_mut()
            .unwrap()
            .remove("vatAmountDkk");
        assert!(receipt(&value, &review).is_none());
    }
    #[::core::prelude::v1::test]
    fn unknown_marker_is_scope_bound_without_retaining_supplier_or_body() {
        let data = data();
        let review = request(&data, &data.outgoing[0], &data.accounts[0], "standard").unwrap();
        let record = marker(
            &review,
            "http://127.0.0.1:1234",
            "synthetic",
            &data,
            Some("owned-profile"),
        );
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
        assert!(!record.to_string().contains("Synthetic expense"));
        assert!(!record.to_string().contains("Synthetic outgoing"));
        assert!(record.get("body").is_none());
    }
}
