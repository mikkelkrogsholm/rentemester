//! Unpaid source recognition and a separately confirmed later payment. Server owns all amounts.
use crate::{Session, document_state, invoice_state, payables_state as state, readable};
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
use rentemester_client::{BankMutation, Error, Report, bank_body_schema};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
type Choices = SelectState<SearchableVec<SharedString>>;
#[derive(Clone)]
struct Data {
    payables: Value,
    documents: Vec<Value>,
    options: Option<Value>,
    bank: Value,
    trial: Value,
    income: Value,
    journal: Value,
    principal: String,
    ledger: String,
}
fn rows(v: &Value, key: &str) -> Result<Vec<Value>, String> {
    v[key]
        .as_array()
        .cloned()
        .ok_or_else(|| format!("Serveren mangler {key}"))
}
fn text(v: &Value, key: &str) -> Result<String, String> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Serveren mangler {key}"))
}
fn read(
    session: &Session,
    company: &str,
    year: &str,
    document: Option<i64>,
) -> Result<Data, String> {
    let get = |result: Result<Value, rentemester_client::Error>| result.map_err(readable);
    let payables =
        get(session
            .runtime
            .block_on(session.client.payables(company, Some("all"), None)))?;
    let docs = get(session.runtime.block_on(session.client.documents(company)))?;
    let profile = get(session
        .runtime
        .block_on(session.client.company_profile(company)))?;
    let me = get(session.runtime.block_on(session.client.me()))?;
    let year = (!year.is_empty()).then_some(year);
    let bank = get(session.runtime.block_on(session.client.bank(company, year)))?;
    let trial = get(session.runtime.block_on(session.client.report(
        company,
        Report::TrialBalance,
        year,
        None,
    )))?;
    let journal =
        get(session
            .runtime
            .block_on(session.client.report(company, Report::Journal, year, None)))?;
    let income = get(session.runtime.block_on(session.client.report(
        company,
        Report::IncomeStatement,
        year,
        None,
    )))?;
    for (root, key) in [
        (&payables, "payables"),
        (&docs, "documents"),
        (&bank, "bank"),
        (&trial, "trialBalance"),
        (&journal, "journal"),
        (&income, "incomeStatement"),
    ] {
        if root["ok"] != true || root[key]["slug"] != company {
            return Err(
                "Serveren bekræftede ikke virksomhedens gæld, bilag, bank og rapporter".into(),
            );
        }
    }
    if profile["ok"] != true || profile["company"]["currency"] != "DKK" || me["ok"] != true {
        return Err("Denne native visning kræver en DKK-virksomhed og aktuel bruger. Ubetalte valutabilag tilbydes ikke.".into());
    }
    let ledger = text(&journal["journal"], "ledgerUuid")?;
    if trial["trialBalance"]["ledgerUuid"] != ledger
        || income["incomeStatement"]["ledgerUuid"] != ledger
    {
        return Err("Rapporternes ledgeridentitet stemmer ikke".into());
    }
    for key in [
        "rows",
        "unregisteredDocuments",
        "expenseAccounts",
        "vendors",
    ] {
        rows(&payables["payables"], key)?;
    }
    if payables["payables"]["asOfDate"].as_str().is_none()
        || !["totalOpenBalance", "overdueOpenBalance"]
            .iter()
            .all(|k| payables["payables"][*k].is_number())
    {
        return Err("Serveren mangler aktuel opgørelsesdato eller kanoniske gældssaldi".into());
    }
    for row in rows(&payables["payables"], "rows")? {
        if !["grossAmount", "paidAmount", "openBalance"]
            .iter()
            .all(|k| row[*k].is_number())
            || !row["payableId"].as_i64().is_some_and(|n| n > 0)
            || !row["documentId"].as_i64().is_some_and(|n| n > 0)
        {
            return Err(
                "Serveren mangler forpligtelsens præcise identitet og aktuelle saldi".into(),
            );
        }
    }
    rows(&bank["bank"], "transactions")?;
    rows(&bank["bank"], "accounts")?;
    rows(&journal["journal"], "entries")?;
    if !trial["trialBalance"]["balanced"].is_boolean()
        || !["totalDebit", "totalCredit"]
            .iter()
            .all(|k| trial["trialBalance"][*k].is_number())
        || !income["incomeStatement"]["result"].is_number()
        || !income["incomeStatement"]["totalExpense"].is_number()
    {
        return Err("Serverrapporter mangler beløb eller balancekontrol".into());
    }
    let mut documents = Vec::<Value>::new();
    for row in rows(&docs["documents"], "documents")? {
        if !hash(&row["sha256Hash"]) || !row["id"].as_i64().is_some_and(|n| n > 0) {
            return Err("Bilagslisten mangler kanonisk originalidentitet".into());
        }
        if let Some(previous) = documents.iter().find(|d| d["id"] == row["id"]) {
            if previous["sha256Hash"] != row["sha256Hash"]
                || previous["mimeType"] != row["mimeType"]
            {
                return Err("Originalidentiteten er modstridende".into());
            }
        } else {
            documents.push(row);
        }
    }
    let options = if let Some(id) = document {
        let root = get(session
            .runtime
            .block_on(session.client.expense_booking_options(company, id)))?;
        if root["ok"] != true
            || root["options"]["document"]["id"] != id
            || !documents.iter().any(|d| {
                d["id"] == id && d["sha256Hash"] == root["options"]["document"]["sha256Hash"]
            })
        {
            return Err("Serveren bekræftede ikke det valgte bilags original".into());
        }
        rows(&root["options"], "unmatchedOutgoingBank")?;
        Some(root["options"].clone())
    } else {
        None
    };
    Ok(Data {
        payables: payables["payables"].clone(),
        documents,
        options,
        bank: bank["bank"].clone(),
        trial: trial["trialBalance"].clone(),
        income: income["incomeStatement"].clone(),
        journal: journal["journal"].clone(),
        principal: text(&me["user"], "id")?,
        ledger,
    })
}
fn hash(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn document(data: &Data) -> Option<&Value> {
    data.options.as_ref().map(|o| &o["document"])
}
fn accounts(data: &Data) -> Vec<Value> {
    rows(&data.payables, "expenseAccounts").unwrap_or_default()
}
fn vendors(data: &Data) -> Vec<Value> {
    rows(&data.payables, "vendors")
        .unwrap_or_default()
        .into_iter()
        .filter(|v| v["gdprErased"] == false && v["archived"] != true)
        .collect()
}
fn outgoing(data: &Data) -> Vec<Value> {
    data.options
        .as_ref()
        .and_then(|v| v["unmatchedOutgoingBank"].as_array())
        .into_iter()
        .flatten()
        .filter(|v| v["currency"] == "DKK")
        .cloned()
        .collect()
}
fn bank_accounts(data: &Data) -> Vec<Value> {
    rows(&data.bank, "accounts")
        .unwrap_or_default()
        .into_iter()
        .filter(|v| {
            v["active"] == true
                && v["currency"] == "DKK"
                && v["ledgerAccountNo"].as_str().is_some_and(|s| !s.is_empty())
        })
        .collect()
}
fn treatments() -> Vec<String> {
    bank_body_schema(BankMutation::RegisterPayable)["properties"]["vatTreatment"]["enum"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}
fn label(v: &Value, key: &str) -> String {
    format!("{} · {}", v[key], v["name"].as_str().unwrap_or("?"))
}
fn bank_label(v: &Value) -> String {
    format!(
        "id{} · {} · {} · {} {}",
        v["id"],
        v["date"].as_str().unwrap_or("?"),
        v["text"].as_str().unwrap_or("?"),
        v["amount"],
        v["currency"]
            .as_str()
            .unwrap_or("(serverrapportens beløb; originalvaluta vises ikke her)")
    )
}
fn treatment_label(v: &str) -> String {
    format!(
        "{v} · {}",
        match v {
            "standard" => "Almindelig købsmoms",
            "exempt" => "Momsfrit",
            "non_deductible" => "Ingen momsfradrag",
            _ => "Servervalidering kræves",
        }
    )
}
#[derive(Clone)]
enum Operation {
    Register {
        source: Value,
        vendor: Value,
        account: Value,
        treatment: String,
        due: String,
    },
    Pay {
        payable: Value,
        bank: Value,
        account: Value,
    },
}
#[derive(Clone)]
struct Review {
    document: Value,
    operation: Operation,
    body: Vec<u8>,
}
fn original_ready(
    original: Option<&(PathBuf, String, String)>,
    opened: bool,
    checked: bool,
    data: &Data,
) -> bool {
    opened
        && checked
        && original.is_some_and(|(_, mime, hash)| {
            document_state::can_open(mime)
                && document(data).is_some_and(|d| d["sha256Hash"] == *hash)
        })
}
fn register(
    data: &Data,
    vendor: &Value,
    account: &Value,
    vat: &str,
    due: &str,
) -> Result<Review, String> {
    let doc = document(data).ok_or("Vælg et kildebilag")?;
    let source = rows(&data.payables, "unregisteredDocuments")?
        .into_iter()
        .find(|v| v["id"] == doc["id"])
        .ok_or("Bilaget er ikke længere uregistreret")?;
    if doc["currency"] != "DKK"
        || !matches!(
            doc["documentType"].as_str(),
            Some("purchase_sale" | "cash_register_receipt")
        )
        || doc.get("journalEntryId") != Some(&Value::Null)
        || !vendor["id"].as_i64().is_some_and(|n| n > 0)
        || account["accountNo"].as_str().is_none_or(str::is_empty)
        || !vendors(data).contains(vendor)
        || !accounts(data).contains(account)
        || !treatments().contains(&vat.into())
    {
        return Err("Vælg faktisk aktiv leverandør, omkostningskonto og eksplicit momsvalg fra aktuelle muligheder".into());
    }
    let bill = text(&source, "invoiceDate")?;
    if due.len() != 10
        || !due.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
    {
        return Err("Angiv forfaldsdato som ÅÅÅÅ-MM-DD; serveren validerer datoen".into());
    }
    let body=serde_json::to_vec(&json!({"documentId":doc["id"],"billDate":bill,"dueDate":due,"expenseAccountNo":account["accountNo"],"vatTreatment":vat,"vendorId":vendor["id"],"confirm":true})).map_err(|_|"Registreringsanmodningen kunne ikke fastholdes")?;
    Ok(Review {
        document: doc.clone(),
        operation: Operation::Register {
            source,
            vendor: vendor.clone(),
            account: account.clone(),
            treatment: vat.into(),
            due: due.into(),
        },
        body,
    })
}
fn payment(data: &Data, payable: &Value, bank: &Value, account: &Value) -> Result<Review, String> {
    let doc = document(data).ok_or("Vælg gældens kildebilag")?;
    if !payable["payableId"].as_i64().is_some_and(|n| n > 0)
        || !bank["id"].as_i64().is_some_and(|n| n > 0)
        || !bank["amount"].is_number()
        || bank["date"].as_str().is_none_or(str::is_empty)
        || payable["currency"] != "DKK"
        || payable["status"] != "open"
        || payable["documentId"] != doc["id"]
        || !rows(&data.payables, "rows")?.contains(payable)
        || !outgoing(data).contains(bank)
        || !bank_accounts(data).contains(account)
    {
        return Err(
            "Vælg aktuel åben DKK-gæld, uafstemt udgående bankrække og aktiv bogført bankkonto"
                .into(),
        );
    }
    let body=serde_json::to_vec(&json!({"bankTransactionId":bank["id"],"paymentAccountNo":account["ledgerAccountNo"],"confirm":true})).map_err(|_|"Betalingsanmodningen kunne ikke fastholdes")?;
    Ok(Review {
        document: doc.clone(),
        operation: Operation::Pay {
            payable: payable.clone(),
            bank: bank.clone(),
            account: account.clone(),
        },
        body,
    })
}
fn current(review: &Review, data: &Data) -> bool {
    if document(data) != Some(&review.document) {
        return false;
    }
    match &review.operation {
        Operation::Register {
            source,
            vendor,
            account,
            treatment,
            due,
        } => {
            rows(&data.payables, "unregisteredDocuments")
                .unwrap_or_default()
                .contains(source)
                && register(data, vendor, account, treatment, due).is_ok()
        }
        Operation::Pay {
            payable,
            bank,
            account,
        } => payment(data, payable, bank, account).is_ok(),
    }
}
fn marker(review: &Review, origin: &str, company: &str, data: &Data, local: Option<&str>) -> Value {
    let (op, payable, bank) = match &review.operation {
        Operation::Register { .. } => ("register", Value::Null, Value::Null),
        Operation::Pay { payable, bank, .. } => {
            ("pay", payable["payableId"].clone(), bank["id"].clone())
        }
    };
    json!({"format":"rentemester-desktop-payables/1","origin":origin,"company":company,"principal":data.principal,"ledger":data.ledger,"localIdentity":local,"operation":op,"documentId":review.document["id"],"documentHash":review.document["sha256Hash"],"payableId":payable,"bankTransactionId":bank})
}
fn scope(record: &Value, origin: &str, company: &str, data: &Data, local: Option<&str>) -> bool {
    record["company"] == company
        && record["principal"] == data.principal
        && record["ledger"] == data.ledger
        && record["localIdentity"].as_str() == local
        && (record["origin"] == origin || local.is_some())
        && document(data).is_some_and(|d| {
            record["documentId"] == d["id"] && record["documentHash"] == d["sha256Hash"]
        })
}
struct Receipt {
    entry: i64,
    payable: i64,
    open: Option<Value>,
    message: String,
}
fn receipt(value: &Value, review: &Review) -> Option<Receipt> {
    if value["ok"] != true {
        return None;
    }
    match &review.operation {
        Operation::Register { due, .. } => {
            let r = &value["payable"];
            if r["documentId"] != review.document["id"]
                || r["dueDate"] != *due
                || !["grossAmount", "netAmount", "vatAmount"]
                    .iter()
                    .all(|k| r[*k].is_number())
            {
                return None;
            }
            Some(Receipt {
                entry: r["entryId"].as_i64().filter(|n| *n > 0)?,
                payable: r["payableId"].as_i64().filter(|n| *n > 0)?,
                open: None,
                message: format!(
                    "Registrering bekræftet: gæld{} · postering {} · Brutto {} DKK · Netto {} · Moms {}. Dette er registrering, ikke betaling.",
                    r["payableId"],
                    r["entryNo"].as_str().filter(|s| !s.is_empty())?,
                    r["grossAmount"],
                    r["netAmount"],
                    r["vatAmount"]
                ),
            })
        }
        Operation::Pay { payable, .. } => {
            let r = &value["payment"];
            if r["payableId"] != payable["payableId"]
                || !r["openBalance"].is_number()
                || !r["paymentId"].as_i64().is_some_and(|n| n > 0)
            {
                return None;
            }
            Some(Receipt {
                entry: r["journalEntryId"].as_i64().filter(|n| *n > 0)?,
                payable: r["payableId"].as_i64()?,
                open: Some(r["openBalance"].clone()),
                message: format!(
                    "Betaling bekræftet: betaling{} · gæld{} · journal{} · åben saldo {} DKK (serverens beløb).",
                    r["paymentId"], r["payableId"], r["journalEntryId"], r["openBalance"]
                ),
            })
        }
    }
}
fn readback(receipt: &Receipt, review: &Review, data: &Data) -> bool {
    let Some(row) = data.payables["rows"].as_array().and_then(|rows| {
        rows.iter()
            .find(|r| r["payableId"] == receipt.payable && r["documentId"] == review.document["id"])
    }) else {
        return false;
    };
    let Some(entry) = data.journal["entries"].as_array().and_then(|rows| {
        rows.iter()
            .find(|e| e["id"] == receipt.entry && e["documentId"] == review.document["id"])
    }) else {
        return false;
    };
    match &review.operation {
        Operation::Register { .. } => data
            .documents
            .iter()
            .any(|d| d["id"] == review.document["id"] && d["journalEntryId"] == receipt.entry),
        Operation::Pay { bank, .. } => {
            receipt.open.as_ref() == Some(&row["openBalance"])
                && data.bank["transactions"].as_array().is_some_and(|rows| {
                    rows.iter().any(|r| {
                        r["id"] == bank["id"]
                            && r["reconciliationStatus"] == "matched"
                            && r["journalEntryNo"] == entry["entryNo"]
                    })
                })
        }
    }
}
pub struct Payables {
    session: Arc<Session>,
    origin: String,
    company: String,
    year: String,
    local: Option<String>,
    data: Option<Data>,
    document: Option<i64>,
    payable: Option<i64>,
    vendor: Entity<Choices>,
    expense: Entity<Choices>,
    vat: Entity<Choices>,
    bank: Entity<Choices>,
    bank_account: Entity<Choices>,
    due: Entity<InputState>,
    original: Option<(PathBuf, String, String)>,
    original_opened: bool,
    original_checked: bool,
    review: Option<Review>,
    reviewed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
    last_entry: Option<i64>,
}
impl Payables {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        company: String,
        year: String,
        local: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let choice = |window: &mut Window, cx: &mut Context<Self>| {
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
            year,
            local,
            data: None,
            document: None,
            payable: None,
            vendor: choice(window, cx),
            expense: choice(window, cx),
            vat: choice(window, cx),
            bank: choice(window, cx),
            bank_account: choice(window, cx),
            due: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Forfaldsdato ÅÅÅÅ-MM-DD (vælg eksplicit)")
            }),
            original: None,
            original_opened: false,
            original_checked: false,
            review: None,
            reviewed: false,
            pending: None,
            resolved: false,
            busy: false,
            status: String::new(),
            last_entry: None,
        };
        this.load(None, None, window, cx);
        this
    }
    fn install(&mut self, data: Data, window: &mut Window, cx: &mut Context<Self>) {
        for (entity, items) in [
            (
                &self.vendor,
                vendors(&data)
                    .iter()
                    .map(|v| SharedString::from(label(v, "id")))
                    .collect::<Vec<_>>(),
            ),
            (
                &self.expense,
                accounts(&data)
                    .iter()
                    .map(|v| SharedString::from(label(v, "accountNo")))
                    .collect(),
            ),
            (
                &self.vat,
                treatments()
                    .iter()
                    .map(|v| SharedString::from(treatment_label(v)))
                    .collect(),
            ),
            (
                &self.bank,
                outgoing(&data)
                    .iter()
                    .map(|v| SharedString::from(bank_label(v)))
                    .collect(),
            ),
            (
                &self.bank_account,
                bank_accounts(&data)
                    .iter()
                    .map(|v| {
                        SharedString::from(format!(
                            "{} · bogført konto {}",
                            label(v, "id"),
                            v["ledgerAccountNo"].as_str().unwrap_or("?")
                        ))
                    })
                    .collect(),
            ),
        ] {
            entity.update(cx, |s, cx| {
                s.set_items(SearchableVec::new(items), window, cx)
            });
        }
        self.data = Some(data);
    }
    fn reset_review(&mut self) {
        self.review = None;
        self.reviewed = false;
        self.original = None;
        self.original_opened = false;
        self.original_checked = false;
        self.resolved = false;
    }
    fn load(
        &mut self,
        id: Option<i64>,
        payable: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.document = id;
        self.payable = payable;
        self.data = None;
        self.reset_review();
        self.due
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.status =
            "Henter aktuelle forpligtelser, bilag, konti, bank og serverrapporter…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let year = self.year.clone();
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((read(&session, &company, &year, id)?, state::load()?))
        });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,window,cx|{this.busy=false;match result{Ok((data,pending))=>{this.install(data,window,cx);this.pending=pending;this.status="Registrering og betaling er to selvstændige bogføringer. Vælg kilde og gennemgå originalen før hver bekræftelse. Serveren validerer adgang, roller, periode og beløb.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn download(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(doc) = self.data.as_ref().and_then(document).cloned() else {
            return;
        };
        let (Some(id), Some(mime), Some(hash)) = (
            doc["id"].as_i64(),
            doc["mimeType"].as_str(),
            doc["sha256Hash"].as_str(),
        ) else {
            self.status = "Originalens id, MIME eller hash mangler".into();
            cx.notify();
            return;
        };
        let mime = mime.to_owned();
        let hash = hash.to_owned();
        self.busy = true;
        self.original = None;
        self.original_opened = false;
        self.original_checked = false;
        self.review = None;
        self.reviewed = false;
        let session = self.session.clone();
        let company = self.company.clone();
        let task = cx.background_executor().spawn(async move {
            let value = session
                .runtime
                .block_on(session.client.document_original(&company, id))
                .map_err(readable)?;
            if value["ok"] != true {
                return Err("Serveren bekræftede ikke originalen".into());
            }
            let path = document_state::download(value, id, &mime, &hash)?;
            Ok::<_, String>((path, mime, hash))
        });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,_,cx|{this.busy=false;match result{Ok(original)=>{this.original=Some(original);this.status="Originalen er hentet privat og SHA-256/MIME kontrolleret mod kanonisk bilag. Åbn og gennemgå den.".into();},Err(e)=>this.status=e}cx.notify();});}).detach();
        cx.notify();
    }
    fn review(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        if !original_ready(
            self.original.as_ref(),
            self.original_opened,
            self.original_checked,
            data,
        ) {
            self.status = "Åbn og gennemgå den præcise hashkontrollerede original først".into();
            cx.notify();
            return;
        }
        let selected =
            |entity: &Entity<Choices>| entity.read(cx).selected_value().map(|s| s.to_string());
        let result = if let Some(id) = self.payable {
            let payable = data.payables["rows"]
                .as_array()
                .and_then(|rows| rows.iter().find(|p| p["payableId"] == id));
            let bank_value = selected(&self.bank);
            let account_value = selected(&self.bank_account);
            let bank = outgoing(data)
                .into_iter()
                .find(|v| bank_value.as_deref() == Some(bank_label(v).as_str()));
            let account = bank_accounts(data).into_iter().find(|v| {
                account_value.as_deref()
                    == Some(
                        format!(
                            "{} · bogført konto {}",
                            label(v, "id"),
                            v["ledgerAccountNo"].as_str().unwrap_or("?")
                        )
                        .as_str(),
                    )
            });
            match(payable,bank,account){(Some(p),Some(b),Some(a))=>payment(data,p,&b,&a),_=>Err("Vælg aktuel åben gæld, en udgående bankrække og faktisk bankkonto med bogføringsmapping".into())}
        } else {
            let vendor_value = selected(&self.vendor);
            let account_value = selected(&self.expense);
            let vat_value = selected(&self.vat);
            let vendor = vendors(data)
                .into_iter()
                .find(|v| vendor_value.as_deref() == Some(label(v, "id").as_str()));
            let account = accounts(data)
                .into_iter()
                .find(|v| account_value.as_deref() == Some(label(v, "accountNo").as_str()));
            let vat = treatments()
                .into_iter()
                .find(|v| vat_value.as_deref() == Some(treatment_label(v).as_str()));
            match (vendor, account, vat) {
                (Some(v), Some(a), Some(vat)) => {
                    register(data, &v, &a, &vat, &self.due.read(cx).value())
                }
                _ => Err(
                    "Vælg faktisk leverandør, omkostningskonto, momsbehandling og en forfaldsdato"
                        .into(),
                ),
            }
        };
        match result {
            Ok(review) => {
                self.review = Some(review);
                self.reviewed = false;
                self.status="Gennemgå den præcise selvstændige handling. Ingen beløb eller moms beregnes lokalt.".into();
            }
            Err(e) => self.status = e,
        }
        cx.notify();
    }
    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy
            || !self.reviewed
            || self.pending.is_some()
            || !self.original_checked
            || !self.original_opened
        {
            return;
        }
        let (Some(review), Some(data)) = (self.review.clone(), self.data.clone()) else {
            return;
        };
        if !original_ready(
            self.original.as_ref(),
            self.original_opened,
            self.original_checked,
            &data,
        ) {
            return;
        }
        self.busy = true;
        self.data = None;
        self.reset_review();
        self.status = "Genlæser kilder og sender kun den gennemgåede handling én gang…".into();
        let session = self.session.clone();
        let origin = self.origin.clone();
        let company = self.company.clone();
        let year = self.year.clone();
        let local = self.local.clone();
        let id = self.document;
        let task=cx.background_executor().spawn(async move{
   let fresh=read(&session,&company,&year,id)?;if fresh.principal!=data.principal||fresh.ledger!=data.ledger||!current(&review,&fresh){return Err("Kildebilag, leverandør, konto, gæld, bank eller bruger ændrede sig. Intet sendt; genlæs og gennemgå igen.".into());}
   let record=marker(&review,&origin,&company,&fresh,local.as_deref());state::begin(&record)?;
   let operation=match &review.operation{Operation::Register{..}=>BankMutation::RegisterPayable,Operation::Pay{payable,..}=>BankMutation::PayPayable(payable["payableId"].as_i64().ok_or("Gælds-id mangler")?)};
   let response=session.runtime.block_on(session.client.bank_mutation(&company,operation,review.body.clone(),true));
   let(message,confirmed)=match response{
    Ok(value)=>if let Some(receipt)=receipt(&value,&review){state::finish(&record,false)?;(receipt.message.clone(),Some(receipt))}else{("Kvitteringen er ufuldstændig. Handlingen kan være bogført; der sendes ikke igen.".into(),None)},
    Err(Error::Refused{status})=>{state::finish(&record,false)?;(format!("Handlingen er afvist (HTTP{status}). Kontrollér original/leverandøridentitet, omkostnings- og bankkonto, momsvalg, bekræftede kreditor-/momsroller, åben saldo, faktisk bankretning, periode og adgang. Serveren er autoritet; ingen automatisk gentagelse."),None)},
    Err(Error::InvalidInput(_))=>{state::finish(&record,false)?;("Felter blev afvist lokalt før netværkskald. Genlæs og gennemgå igen.".into(),None)},
    _=>("Udfaldet er uafklaret. Gæld eller betaling kan være bogført; ingen genafsendelse eller automatisk adoption. Genlæs kanoniske forpligtelser, bilag, bank og journal og afklar manuelt.".into(),None)
   };
   let next=read(&session,&company,&year,id);let message=if let(Some(receipt),Ok(next))=(&confirmed,&next){format!("{message} {}",if readback(receipt,&review,next){"Kanonisk gæld, original/journal og relevant bankafstemning er læst tilbage."}else{"Kvittering bekræftet; vælg relevant regnskabsår og genlæs for fuldt dokument-/bank-/journalbevis."})}else{message};
   Ok::<_,String>((message,confirmed.map(|r|r.entry),next,state::load()?))
  });
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok((message, entry, data, pending)) => {
                        this.status = message;
                        this.last_entry = entry;
                        this.pending = pending;
                        match data {
                            Ok(data) => this.install(data, window, cx),
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
impl Render for Payables {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut view = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_xl()
                    .child("Leverandørgæld · registrering og senere betaling"),
            )
            .child(format!("{} · {}", self.company, self.origin))
            .child(self.status.clone())
            .child(
                Button::new("payables-refresh")
                    .label("Genlæs forpligtelser, bilag, bank og rapporter")
                    .disabled(self.busy || self.review.is_some())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.load(this.document, this.payable, window, cx)
                    })),
            );
        let Some(data) = &self.data else {
            return view;
        };
        view=view.child(format!("Forpligtelser pr. {} · Åben gæld i alt {} DKK · Forfalden gæld {} (serverens aktuelle beløb).",data.payables["asOfDate"].as_str().unwrap_or("?"),data.payables["totalOpenBalance"],data.payables["overdueOpenBalance"]));
        for p in data.payables["rows"].as_array().into_iter().flatten() {
            view=view.child(format!("Gæld{} · {} · Faktura {} · Bilag{} · {} → {} · Brutto {} / betalt {} / åben {} {} · {}",p["payableId"],p["supplierName"].as_str().unwrap_or("Ingen navn i aktuel projektion"),p["billNo"].as_str().unwrap_or("?"),p["documentId"],p["billDate"].as_str().unwrap_or("?"),p["dueDate"].as_str().unwrap_or("?"),p["grossAmount"],p["paidAmount"],p["openBalance"],p["currency"].as_str().unwrap_or("?"),p["status"].as_str().unwrap_or("?")));
            if let (Some(id), Some(doc)) = (p["payableId"].as_i64(), p["documentId"].as_i64()) {
                view = view.child(
                    Button::new(SharedString::from(format!("payable-select-{id}")))
                        .label(if p["status"] == "open" {
                            format!("Gennemgå original og vælg senere betaling af gæld{id}")
                        } else {
                            format!("Læs afsluttet gæld{id} og original")
                        })
                        .disabled(self.busy || self.review.is_some() || self.pending.is_some())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.load(Some(doc), Some(id), window, cx)
                        })),
                );
            }
        }
        view = view.child(
            div()
                .text_lg()
                .child("Uregistrerede DKK-købsbilag · registrering opretter gæld, ikke betaling"),
        );
        for d in data.payables["unregisteredDocuments"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(id) = d["id"].as_i64() {
                view = view.child(
                    Button::new(SharedString::from(format!("payable-document-{id}")))
                        .label(format!(
                            "Vælg bilag{id} · {} · {} · brutto {} DKK · oprindelig moms {}",
                            d["invoiceNo"].as_str().unwrap_or("?"),
                            d["supplierName"]
                                .as_str()
                                .unwrap_or("Ingen navn i aktuel projektion"),
                            d["amountIncVat"],
                            d["vatAmount"]
                        ))
                        .disabled(self.busy || self.review.is_some() || self.pending.is_some())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.load(Some(id), None, window, cx)
                        })),
                );
            }
        }
        if let Some(record) = self.pending.clone() {
            view=view.child(format!("UAFKLARET {} · bilag{} · gæld{} · bank{}. Privat status indeholder kun kilde-id'er og scope, aldrig beløb eller genafsendelig anmodning.",record["operation"].as_str().unwrap_or("handling"),record["documentId"],record["payableId"],record["bankTransactionId"])).child("Genlæs original/hash, åben gæld, bankafstemning og journal manuelt. Projektioner alene beviser ikke udfaldet af præcis dette forsøg.");
            if let Some(id) = record["documentId"].as_i64() {
                let payable = record["payableId"].as_i64();
                view = view.child(
                    Button::new("payables-recover-source")
                        .label("Åbn den uafklarede handlings oprindelige kilde til læsning")
                        .disabled(self.busy || self.review.is_some())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.load(Some(id), payable, window, cx)
                        })),
                );
            }
            if scope(
                &record,
                &self.origin,
                &self.company,
                data,
                self.local.as_deref(),
            ) {
                view=view.child(format!("Oprindelig server {} · aktuel {}. Manuel afklaring kræver samme bruger, ledger, original og private lokale profil (eller nøjagtig ekstern origin).",record["origin"].as_str().unwrap_or("?"),self.origin))
     .child(Checkbox::new("payables-manually-resolved").label("Jeg har manuelt afklaret denne nøjagtige registrering/betaling i den korrekte virksomhed. Arkivér status uden genafsendelse.").checked(self.resolved).on_click(cx.listener(|this,checked,_,cx|{this.resolved=*checked;cx.notify();})))
     .child(Button::new("payables-resolve").label("Bekræft manuel afklaring uden genafsendelse").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{let(Some(record),Some(data))=(&this.pending,&this.data)else{return;};if this.busy||!this.resolved||!scope(record,&this.origin,&this.company,data,this.local.as_deref()){return;}match state::finish(record,true){Ok(())=>{this.pending=None;this.resolved=false;this.reset_review();this.status="Manuel afklaring arkiveret. Ingen handling blev genafsendt. Vælg og gennemgå originalen på ny før en anden selvstændig handling.".into();},Err(e)=>this.status=e}cx.notify();})));
            } else {
                view=view.child("Åbn oprindelig bruger, virksomhed og profil for at afklare. Nye bogføringer er blokeret.");
            }
        }
        if let Some(doc) = document(data) {
            view = view
                .child(div().text_lg().child("Valgt original"))
                .child(format!(
                    "Bilag{} · {} · dato {} · Brutto {} DKK · Oprindelig moms {} · SHA-256 {}",
                    doc["id"],
                    doc["documentNo"].as_str().unwrap_or("?"),
                    doc["invoiceDate"].as_str().unwrap_or("?"),
                    doc["amountIncVat"],
                    doc["vatAmount"],
                    doc["sha256Hash"].as_str().unwrap_or("?")
                ))
                .child(
                    Button::new("payables-original-download")
                        .label("Hent original og kontrollér SHA-256")
                        .disabled(self.busy || self.review.is_some())
                        .on_click(cx.listener(|this, _, window, cx| this.download(window, cx))),
                );
            if let Some((path, mime, hash)) = self.original.clone() {
                if document_state::can_open(&mime) {
                    let url = invoice_state::file_url(&path);
                    view = view.child(
                        Button::new("payables-original-open")
                            .label("Åbn præcis hashkontrolleret original i almindelig fremviser")
                            .disabled(self.busy || doc["sha256Hash"] != hash)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.open_url(&url);
                                this.original_opened = true;
                                cx.notify();
                            })),
                    );
                } else {
                    view=view.child("Originalen er gemt privat. Dette native flow understøtter almindelig fremvisning af PDF, PNG, JPEG og tekst; andre originaltyper kræver den eksisterende browserarbejdsgang.");
                }
                view=view.child(Checkbox::new("payables-original-reviewed").label("Jeg har åbnet og gennemgået denne præcise original før registrering/betaling.").checked(self.original_checked).disabled(self.busy||!self.original_opened).on_click(cx.listener(|this,checked,_,cx|{this.original_checked=*checked;cx.notify();})));
            }
            if self.pending.is_none() {
                if let Some(review) = self.review.clone() {
                    view = view.child(div().text_lg().child("Gennemgå præcis handling"));
                    match &review.operation {
                        Operation::Register {
                            source,
                            vendor,
                            account,
                            treatment,
                            due,
                        } => {
                            view=view.child("REGISTRERING · opretter leverandørgæld og bogfører udgift/moms. Ingen betaling sendes.").child(format!("{} · {} · dato {} · forfald {} · brutto {} / oprindelig moms {} DKK",label(vendor,"id"),label(account,"accountNo"),source["invoiceDate"].as_str().unwrap_or("?"),due,source["amountIncVat"],source["vatAmount"])).child(treatment_label(treatment));
                            view=view.child(format!("Aktuel leverandøridentitet · CVR/moms {} · land {} · identitetsstatus {}. Serveren kræver match til originalens leverandør.",vendor["vatOrCvr"].as_str().unwrap_or("Ikke registreret"),vendor["countryCode"].as_str().unwrap_or("Ikke registreret"),vendor["identityStatus"].as_str().unwrap_or("Ikke oplyst")));
                        }
                        Operation::Pay {
                            payable,
                            bank,
                            account,
                        } => {
                            view=view.child("SENERE BETALING · udligner eksisterende gæld mod den valgte bankrække. Udgift og moms beregnes ikke igen lokalt.").child(format!("Gæld{} · oprindelig brutto {} · allerede betalt {} · åben {} DKK",payable["payableId"],payable["grossAmount"],payable["paidAmount"],payable["openBalance"])).child(bank_label(bank)).child(format!("Bankkonto {} · betalingskonto {}. Serveren kræver match til bankrækkens faktiske mapping og tillader kun dens hele beløb.",label(account,"id"),account["ledgerAccountNo"].as_str().unwrap_or("?")));
                        }
                    }
                    view=view.child("Serveren kontrollerer original/leverandøridentitet, gælds-/momsroller, bankmapping, beløb, periode og aktuelle rettigheder ved bekræftelse.")
     .child(Checkbox::new("payables-reviewed").label("Jeg bekræfter den viste selvstændige handling med præcis kilde og valg.").checked(self.reviewed).on_click(cx.listener(|this,checked,_,cx|{this.reviewed=*checked;cx.notify();})))
     .child(Button::new("payables-confirm").label(if self.payable.is_some(){"Bekræft kun den senere betaling én gang"}else{"Bekræft kun gældsregistreringen én gang"}).disabled(self.busy||!self.reviewed||!self.original_checked||!self.original_opened).on_click(cx.listener(|this,_,window,cx|this.send(window,cx))))
     .child(Button::new("payables-cancel-review").label("Annullér gennemgang").disabled(self.busy).on_click(cx.listener(|this,_,_,cx|{this.review=None;this.reviewed=false;cx.notify();})));
                } else if self.payable.is_some()
                    && !data.payables["rows"].as_array().is_some_and(|rows| {
                        rows.iter().any(|p| {
                            p["payableId"].as_i64() == self.payable && p["status"] == "open"
                        })
                    })
                {
                    view=view.child("Denne gæld er ikke åben. Original, aktuelle saldi og posteringer kan læses; ingen ny betaling tilbydes.");
                } else if self.payable.is_some() {
                    view=view.child("Vælg faktisk uafstemt udgående DKK-bankrække og faktisk bankkonto med mapping. Ingen beløbs- eller datotilpasning tilbydes.")
     .child(Select::new(&self.bank).placeholder("Vælg faktisk udgående bankrække").accessibility_label("Gældens betalingsbankrække").disabled(self.busy))
     .child(Select::new(&self.bank_account).placeholder("Vælg faktisk mapped bankkonto").accessibility_label("Gældens betalingskonto").disabled(self.busy))
     .child(Button::new("payables-review-payment").label("Gennemgå selvstændig senere betaling").disabled(self.busy||!self.original_checked).on_click(cx.listener(|this,_,_,cx|this.review(cx))));
                } else if data.payables["unregisteredDocuments"]
                    .as_array()
                    .is_some_and(|rows| rows.iter().any(|d| d["id"] == doc["id"]))
                {
                    view=view.child("Leverandør og omkostningskonto vælges eksplicit fra serveren; der udledes ingen standardkonto, momsregime eller betalingsfrist.")
     .child(Select::new(&self.vendor).placeholder("Vælg faktisk leverandør").accessibility_label("Gældens leverandør").disabled(self.busy))
     .child(Select::new(&self.expense).placeholder("Vælg omkostningskonto").accessibility_label("Gældens omkostningskonto").disabled(self.busy))
     .child(Select::new(&self.vat).placeholder("Vælg eksplicit momsbehandling").accessibility_label("Gældens momsbehandling").disabled(self.busy))
     .child("Forfaldsdato (ÅÅÅÅ-MM-DD). Fakturadato fastholdes fra det uforanderlige kildebilag.").child(Input::new(&self.due).disabled(self.busy))
     .child(Button::new("payables-review-register").label("Gennemgå selvstændig gældsregistrering").disabled(self.busy||!self.original_checked).on_click(cx.listener(|this,_,_,cx|this.review(cx))));
                } else {
                    view=view.child("Bilaget er allerede registreret eller bogført. Vælg den kanoniske gæld ovenfor for en separat senere betaling eller læsning.");
                }
            }
            view = view.child(
                div()
                    .text_lg()
                    .child("Kanoniske posteringer knyttet til originalen · valgt regnskabsår"),
            );
            for e in data.journal["entries"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|e| e["documentId"] == doc["id"])
            {
                view = view.child(format!(
                    "{} · {} · {}{}",
                    e["entryNo"].as_str().unwrap_or("?"),
                    e["date"].as_str().unwrap_or("?"),
                    e["text"].as_str().unwrap_or("?"),
                    if e["id"].as_i64() == self.last_entry {
                        " · seneste bekræftede kvittering"
                    } else {
                        ""
                    }
                ));
                for l in e["lines"].as_array().into_iter().flatten() {
                    view = view.child(format!(
                        "{} · {} · Debet {} / Kredit {}",
                        l["accountNo"].as_str().unwrap_or("?"),
                        l["accountName"].as_str().unwrap_or("?"),
                        l["debit"],
                        l["credit"]
                    ));
                }
            }
        }
        view = view.child(
            div()
                .text_lg()
                .child("Kanonisk bankafstemning · valgt regnskabsår"),
        );
        for b in data.bank["transactions"].as_array().into_iter().flatten() {
            view = view.child(format!(
                "{} · {} · postering {}",
                bank_label(b),
                b["reconciliationStatus"].as_str().unwrap_or("?"),
                b["journalEntryNo"].as_str().unwrap_or("Ikke afstemt")
            ));
        }
        view.child(div().text_lg().child("Serverrapporter · hele virksomheden, valgt regnskabsår"))
   .child(format!("{} til {} · Saldobalance debet {} / kredit {} · Balanceret {}",data.trial["periodStart"].as_str().unwrap_or("?"),data.trial["periodEnd"].as_str().unwrap_or("?"),data.trial["totalDebit"],data.trial["totalCredit"],data.trial["balanced"]))
   .child(format!("Resultatopgørelse · omkostninger {} · resultat {}. Alle beløb og gældssaldi kommer fra serverens aktuelle projektioner.",data.income["totalExpense"],data.income["result"]))
    }
}
#[cfg(test)]
mod tests {
    use super::{
        Data, Review, accounts, bank_accounts, current, marker, original_ready, outgoing, payment,
        readback, receipt, register, scope, vendors,
    };
    use serde_json::{Value, json};
    use std::path::PathBuf;
    fn data() -> Data {
        let doc = json!({"id":7,"documentType":"purchase_sale","currency":"DKK","journalEntryId":null,"invoiceDate":"2026-10-19","amountIncVat":125,"vatAmount":25,"sha256Hash":"a".repeat(64),"mimeType":"text/plain"});
        let source = json!({"id":7,"invoiceDate":"2026-10-19","amountIncVat":125,"vatAmount":25});
        let vendor =
            json!({"id":3,"name":"Synthetic Supplier","gdprErased":false,"archived":false});
        let account =
            json!({"accountNo":"E","name":"Synthetic Expense","defaultVatCode":"DK_PURCHASE_25"});
        let bank = json!({"id":8,"date":"2026-10-20","text":"Synthetic later payment","amount":-125,"currency":"DKK"});
        let payable = json!({"payableId":9,"documentId":7,"status":"open","currency":"DKK","grossAmount":125,"paidAmount":0,"openBalance":125});
        Data {
            payables: json!({"unregisteredDocuments":[source],"rows":[payable],"vendors":[vendor],"expenseAccounts":[account]}),
            documents: vec![doc.clone()],
            options: Some(json!({"document":doc,"unmatchedOutgoingBank":[bank]})),
            bank: json!({"accounts":[{"id":2,"name":"Synthetic Bank","active":true,"currency":"DKK","ledgerAccountNo":"B"}],"transactions":[]}),
            trial: json!({}),
            income: json!({}),
            journal: json!({"entries":[]}),
            principal: "owner".into(),
            ledger: "immutable".into(),
        }
    }
    fn registration(d: &Data) -> Review {
        register(d, &vendors(d)[0], &accounts(d)[0], "standard", "2026-11-01").unwrap()
    }
    fn paid(d: &Data) -> Review {
        payment(
            d,
            &d.payables["rows"][0],
            &outgoing(d)[0],
            &bank_accounts(d)[0],
        )
        .unwrap()
    }
    #[test]
    fn original_review_requires_explicit_open_check_and_exact_hash_supported_type() {
        let d = data();
        let original = (
            PathBuf::from("/private/synthetic.txt"),
            "text/plain".into(),
            "a".repeat(64),
        );
        assert!(!original_ready(Some(&original), false, true, &d));
        assert!(!original_ready(Some(&original), true, false, &d));
        assert!(!original_ready(None, true, true, &d));
        assert!(original_ready(Some(&original), true, true, &d));
        let wrong = (original.0.clone(), original.1.clone(), "b".repeat(64));
        assert!(!original_ready(Some(&wrong), true, true, &d));
        let unsupported = (original.0, "application/json".into(), original.2);
        assert!(!original_ready(Some(&unsupported), true, true, &d));
    }
    #[test]
    fn registration_freezes_explicit_choices_without_local_amounts_regime_or_due_default() {
        let d = data();
        let r = registration(&d);
        let body: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(
            body,
            json!({"documentId":7,"billDate":"2026-10-19","dueDate":"2026-11-01","vendorId":3,"expenseAccountNo":"E","vatTreatment":"standard","confirm":true})
        );
        assert!(register(&d, &vendors(&d)[0], &accounts(&d)[0], "", "2026-11-01").is_err());
        assert!(register(&d, &vendors(&d)[0], &accounts(&d)[0], "standard", "").is_err());
        let mut changed = d.clone();
        changed.payables["vendors"][0]["gdprErased"] = json!(true);
        assert!(!current(&r, &changed));
        let mut changed = d.clone();
        changed.payables["expenseAccounts"][0]["defaultVatCode"] = json!("changed");
        assert!(!current(&r, &changed));
        let mut changed = d.clone();
        changed.options.as_mut().unwrap()["document"]["journalEntryId"] = json!(99);
        assert!(!current(&r, &changed));
        let mut fx = d.clone();
        fx.options.as_mut().unwrap()["document"]["currency"] = json!("EUR");
        assert!(
            register(
                &fx,
                &vendors(&fx)[0],
                &accounts(&fx)[0],
                "standard",
                "2026-11-01"
            )
            .is_err()
        );
    }
    #[test]
    fn later_payment_uses_exact_actual_row_and_explicit_mapping_not_amount_math() {
        let d = data();
        let r = paid(&d);
        assert_eq!(
            serde_json::from_slice::<Value>(&r.body).unwrap(),
            json!({"bankTransactionId":8,"paymentAccountNo":"B","confirm":true})
        );
        let mut changed = d.clone();
        changed.payables["rows"][0]["openBalance"] = json!(75);
        assert!(!current(&r, &changed));
        let mut changed = d.clone();
        changed.options.as_mut().unwrap()["unmatchedOutgoingBank"] = json!([]);
        assert!(!current(&r, &changed));
        let mut changed = d.clone();
        changed.bank["accounts"][0]["ledgerAccountNo"] = json!("OTHER");
        assert!(!current(&r, &changed));
        let mut changed = d.clone();
        changed.payables["rows"][0]["status"] = json!("paid");
        assert!(
            payment(
                &changed,
                &changed.payables["rows"][0],
                &outgoing(&changed)[0],
                &bank_accounts(&changed)[0]
            )
            .is_err()
        );
    }
    #[test]
    fn confirmed_receipts_are_distinct_and_readback_requires_canonical_links() {
        let mut d = data();
        let r = registration(&d);
        let receipt_value = json!({"ok":true,"payable":{"payableId":9,"documentId":7,"grossAmount":125,"netAmount":100,"vatAmount":25,"dueDate":"2026-11-01","entryId":11,"entryNo":"SYN-11"}});
        let c = receipt(&receipt_value, &r).unwrap();
        assert!(!readback(&c, &r, &d));
        d.documents[0]["journalEntryId"] = json!(11);
        d.journal["entries"] = json!([{"id":11,"documentId":7,"entryNo":"SYN-11"}]);
        assert!(readback(&c, &r, &d));
        let mut wrong = receipt_value.clone();
        wrong["payable"]["documentId"] = json!(70);
        assert!(receipt(&wrong, &r).is_none());
        let r = paid(&d);
        let value = json!({"ok":true,"payment":{"paymentId":12,"journalEntryId":13,"payableId":9,"openBalance":0}});
        let c = receipt(&value, &r).unwrap();
        d.payables["rows"][0]["openBalance"] = json!(0);
        d.journal["entries"] = json!([{"id":13,"documentId":7,"entryNo":"SYN-13"}]);
        assert!(!readback(&c, &r, &d));
        d.bank["transactions"] =
            json!([{"id":8,"reconciliationStatus":"matched","journalEntryNo":"SYN-13"}]);
        assert!(readback(&c, &r, &d));
        d.bank["transactions"][0]["journalEntryNo"] = json!("OTHER");
        assert!(!readback(&c, &r, &d));
        let mut wrong = value;
        wrong["payment"]["payableId"] = json!(90);
        assert!(receipt(&wrong, &r).is_none());
    }
    #[test]
    fn unknown_marker_binds_original_user_company_ledger_and_local_profile_without_replay() {
        let d = data();
        let r = paid(&d);
        let record = marker(&r, "https://synthetic.invalid", "synthetic", &d, None);
        assert!(scope(
            &record,
            "https://synthetic.invalid",
            "synthetic",
            &d,
            None
        ));
        assert!(!scope(
            &record,
            "https://elsewhere.invalid",
            "synthetic",
            &d,
            None
        ));
        assert!(!scope(
            &record,
            "https://synthetic.invalid",
            "other",
            &d,
            None
        ));
        let mut changed = d.clone();
        changed.principal = "other".into();
        assert!(!scope(
            &record,
            "https://synthetic.invalid",
            "synthetic",
            &changed,
            None
        ));
        let local = marker(
            &r,
            "http://127.0.0.1:1",
            "synthetic",
            &d,
            Some("owned-profile"),
        );
        assert!(scope(
            &local,
            "http://127.0.0.1:2",
            "synthetic",
            &d,
            Some("owned-profile")
        ));
        assert!(!scope(
            &local,
            "http://127.0.0.1:2",
            "synthetic",
            &d,
            Some("other-profile")
        ));
        let mut changed = d.clone();
        changed.ledger = "different".into();
        assert!(!scope(
            &local,
            "http://127.0.0.1:2",
            "synthetic",
            &changed,
            Some("owned-profile")
        ));
        let raw = record.to_string();
        for forbidden in [
            "Synthetic Supplier",
            "Synthetic later payment",
            "amount",
            "body",
            "vatTreatment",
            "dueDate",
        ] {
            assert!(!raw.contains(forbidden));
        }
    }
}
