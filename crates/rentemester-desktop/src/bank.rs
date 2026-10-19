//! Native bank statement import. Server owns parsing, deduplication and ledger rules.
use crate::{Session, bank_state, readable};
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
use rentemester_client::{BankMutation, Error, Report};
use serde_json::{Value, json};
use std::{io::Read, path::Path, sync::Arc};
type Choices = SelectState<SearchableVec<SharedString>>;
#[derive(Clone)]
struct Data {
    bank: Value,
    accounts: Vec<Value>,
    principal: String,
    ledger: String,
}
fn text(v: &Value, key: &str) -> Result<String, String> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Serveren mangler {key}"))
}
fn rows(v: &Value, key: &str) -> Result<Vec<Value>, String> {
    v[key]
        .as_array()
        .cloned()
        .ok_or_else(|| format!("Serveren mangler {key}"))
}
fn read(session: &Session, company: &str, year: &str) -> Result<Data, String> {
    let bank = session
        .runtime
        .block_on(
            session
                .client
                .bank(company, if year.is_empty() { None } else { Some(year) }),
        )
        .map_err(readable)?;
    let accounts = session
        .runtime
        .block_on(session.client.accounts(company))
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
            if year.is_empty() { None } else { Some(year) },
            None,
        ))
        .map_err(readable)?;
    if [&bank, &accounts, &me, &journal]
        .iter()
        .any(|v| v["ok"] != true)
    {
        return Err("Serveren bekræftede ikke bankgrundlaget".into());
    }
    let bank = bank["bank"].clone();
    if text(&bank, "slug")? != company || text(&journal["journal"], "slug")? != company {
        return Err("Bankgrundlaget tilhører en anden virksomhed".into());
    }
    rows(&bank, "accounts")?;
    rows(&bank, "transactions")?;
    Ok(Data {
        bank,
        accounts: rows(&accounts["accounts"], "accounts")?,
        principal: text(&me["user"], "id")?,
        ledger: text(&journal["journal"], "ledgerUuid")?,
    })
}
fn eligible(v: &Value) -> bool {
    v["active"] == true && v["ledgerAccountNo"].as_str().is_some_and(|s| !s.is_empty())
}
fn label(v: &Value) -> String {
    format!(
        "{} · {} → {}",
        v["slug"].as_str().unwrap_or("?"),
        v["name"].as_str().unwrap_or("?"),
        v["ledgerAccountNo"].as_str().unwrap_or("Ikke tilknyttet")
    )
}
fn import_body(content: &str, account: &Value) -> Result<Vec<u8>, String> {
    if content.is_empty() || content.len() > 240000 {
        return Err("Vælg en UTF-8 CSV-fil på højst240000 bytes.".into());
    }
    if !eligible(account) {
        return Err("Vælg en aktiv bankkonto med eksplicit kontotilknytning.".into());
    }
    serde_json::to_vec(
        &json!({"csvContent":content,"account":text(account,"slug")?,"confirm":true}),
    )
    .map_err(|_| "CSV-anmodningen kunne ikke fastholdes".into())
}
fn file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|_| "CSV-filen kunne ikke åbnes")?;
    if !file
        .metadata()
        .map_err(|_| "CSV-filen kunne ikke undersøges")?
        .is_file()
    {
        return Err("Vælg én almindelig CSV-fil".into());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(240001)
        .read_to_end(&mut bytes)
        .map_err(|_| "CSV-filen kunne ikke læses")?;
    if bytes.is_empty() || bytes.len() > 240000 {
        return Err("CSV-filen skal indeholde1–240000 bytes".into());
    }
    String::from_utf8(bytes)
        .map_err(|_| "CSV skal være UTF-8. Konvertér bankeksporten før import.".into())
}
#[derive(Clone)]
struct Review {
    operation: &'static str,
    body: Vec<u8>,
    target: Option<Value>,
}
fn receipt(value: &Value, operation: &str, body: &[u8]) -> Option<String> {
    if value["ok"] != true {
        return None;
    }
    if operation == "import" {
        let requested: Value = serde_json::from_slice(body).ok()?;
        let result = &value["import"];
        if result["bankAccountSlug"].as_str()? != requested["account"].as_str()? {
            return None;
        }
        let imported = result["imported"].as_u64()?;
        let skipped = result["skippedDuplicates"].as_u64()?;
        let batch = result["importBatchId"].as_str().filter(|s| !s.is_empty())?;
        Some(format!(
            "Serverkvittering {batch}: {imported} importerede · {skipped} dubletter sprunget over."
        ))
    } else {
        let result = &value["bankAccount"];
        let id = result["id"].as_i64().filter(|id| *id > 0)?;
        let slug = result["slug"].as_str().filter(|s| !s.is_empty())?;
        Some(format!("Bankkonto oprettet: {slug} · id{id}."))
    }
}
pub struct Bank {
    session: Arc<Session>,
    origin: String,
    company: String,
    local: Option<String>,
    data: Option<Data>,
    year: Entity<InputState>,
    name: Entity<InputState>,
    bank: Entity<Choices>,
    ledger: Entity<Choices>,
    source: Option<String>,
    source_name: String,
    review: Option<Review>,
    reviewed: bool,
    pending: Option<Value>,
    resolved: bool,
    busy: bool,
    status: String,
}
impl Bank {
    pub fn new(
        session: Arc<Session>,
        origin: String,
        company: String,
        year: String,
        local: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let year = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Regnskabsår (tomt = aktuelt)");
            input.set_value(year, window, cx);
            input
        });
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Navn på ny bankkonto"));
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
            data: None,
            year,
            name,
            bank: select(window, cx),
            ledger: select(window, cx),
            source: None,
            source_name: String::new(),
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
    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.data = None;
        self.review = None;
        self.reviewed = false;
        let session = self.session.clone();
        let company = self.company.clone();
        let year = self.year.read(cx).value().trim().to_owned();
        self.status = "Henter bankkonti og bankposteringer…".into();
        let task = cx.background_executor().spawn(async move {
            Ok::<_, String>((read(&session, &company, &year)?, bank_state::load()?))
        });
        cx.spawn_in(window,async move|view,cx|{let result=task.await;let _=view.update_in(cx,|this,window,cx|{
            this.busy=false;match result{
                Ok((data,pending))=>{
                    let banks=rows(&data.bank,"accounts").unwrap_or_default().iter().filter(|v|eligible(v)).map(|v|SharedString::from(label(v))).collect::<Vec<_>>();
                    this.bank.update(cx,|state,cx|state.set_items(SearchableVec::new(banks),window,cx));
                    let assets=data.accounts.iter().filter(|v|v["active"]==true && v["type"]=="asset").map(|v|SharedString::from(format!("{} · {}",v["accountNo"].as_str().unwrap_or("?"),v["name"].as_str().unwrap_or("?")))).collect::<Vec<_>>();
                    this.ledger.update(cx,|state,cx|state.set_items(SearchableVec::new(assets),window,cx));
                    this.resolved=false;this.pending=pending;this.data=Some(data);this.status="CSV-import opretter bankrækker og kildebilag. Den bogfører ikke betalinger. Genimport af samme kilde og konto deduplikeres af serveren.".into();
                },Err(error)=>this.status=error,
            }cx.notify();
        });}).detach();
        cx.notify();
    }
    fn choose_file(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        self.review = None;
        self.reviewed = false;
        let choice = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Vælg generisk bank-CSV (UTF-8)".into()),
        });
        self.busy = true;
        cx.spawn(async move|view,cx|{
            let selected=choice.await;
            let result=match selected{Ok(Ok(Some(paths)))=>paths.first().cloned(),Ok(Ok(None))=>None,_=>{let _=view.update(cx,|this,cx|{this.busy=false;this.status="Filvælgeren kunne ikke åbnes".into();cx.notify();});return;}};
            let loaded=if let Some(path)=result{let name=path.file_name().unwrap_or_default().to_string_lossy().into_owned();Some(cx.background_executor().spawn(async move{file(&path).map(|csv|(name,csv))}).await)}else{None};
            let _=view.update(cx,|this,cx|{this.busy=false;match loaded{Some(Ok((name,csv)))=>{this.source=Some(csv);this.source_name=name;this.status="CSV-filen er læst lokalt. Vælg modtagerkonto og gennemgå den præcise import.".into();},Some(Err(error))=>this.status=error,None=>{}}cx.notify();});
        }).detach();
        cx.notify();
    }
    fn review(&mut self, create: bool, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() {
            return;
        }
        let Some(data) = &self.data else {
            return;
        };
        let result = if create {
            let selected = self.ledger.read(cx).selected_value().cloned();
            let ledger = data.accounts.iter().find(|v| {
                selected.as_deref()
                    == Some(
                        format!(
                            "{} · {}",
                            v["accountNo"].as_str().unwrap_or("?"),
                            v["name"].as_str().unwrap_or("?")
                        )
                        .as_str(),
                    )
            });
            let name = self.name.read(cx).value().trim().to_owned();
            if name.is_empty() {
                Err("Skriv bankkontoens navn".into())
            } else if let Some(ledger) = ledger {
                serde_json::to_vec(&json!({"name":name,"currency":data.bank["company"]["currency"],"ledgerAccountNo":ledger["accountNo"]})).map(|body|Review{operation:"create",body,target:None}).map_err(|_|"Bankkonto kunne ikke fastholdes".into())
            } else {
                Err("Vælg en aktiv aktivkonto fra kontoplanen".into())
            }
        } else {
            let selected = self.bank.read(cx).selected_value().cloned();
            let account = data.bank["accounts"].as_array().and_then(|accounts| {
                accounts
                    .iter()
                    .find(|v| selected.as_deref() == Some(label(v).as_str()))
            });
            match (self.source.as_deref(), account) {
                (Some(csv), Some(account)) => import_body(csv, account).map(|body| Review {
                    operation: "import",
                    body,
                    target: Some(account.clone()),
                }),
                _ => Err("Vælg CSV-fil og en tilknyttet bankkonto".into()),
            }
        };
        match result {
            Ok(review) => {
                self.review = Some(review);
                self.reviewed = false;
                self.status="Gennemgå præcis konto/kontotilknytning og kilde nedenfor. Serveren validerer CSV-rækker, beløb og dubletter ved bekræftelse.".into();
            }
            Err(error) => self.status = error,
        }
        cx.notify();
    }
    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.pending.is_some() || !self.reviewed {
            return;
        }
        let Some(review) = self.review.clone() else {
            return;
        };
        let Some(data) = self.data.clone() else {
            return;
        };
        self.busy = true;
        self.status = "Sender den gennemgåede anmodning én gang…".into();
        let session = self.session.clone();
        let company = self.company.clone();
        let origin = self.origin.clone();
        let local = self.local.clone();
        let year = self.year.read(cx).value().trim().to_owned();
        let task=cx.background_executor().spawn(async move{
            let fresh=read(&session,&company,&year)?;
            if fresh.principal!=data.principal||fresh.ledger!=data.ledger{return Err("Bruger eller virksomhed ændrede sig. Intet blev sendt.".into());}
            if let Some(target)=&review.target{
                if !fresh.bank["accounts"].as_array().is_some_and(|accounts|accounts.contains(target)&&eligible(target)){return Err("Bankkontoens tilknytning ændrede sig. Genindlæs og gennemgå igen.".into());}
            }
            let record=json!({"format":"rentemester-desktop-bank/1","operation":review.operation,"body":String::from_utf8(review.body.clone()).map_err(|_|"Ugyldig importanmodning")?,"origin":origin,"company":company,"principal":data.principal,"ledger":data.ledger,"localIdentity":local,"year":year,"beforeIds":fresh.bank["transactions"].as_array().ok_or("Bankrækker mangler")?.iter().map(|v|v["id"].clone()).collect::<Vec<_>>()});
            bank_state::begin(&record)?;
            let mutation=if review.operation=="import"{BankMutation::ImportStatement}else{BankMutation::CreateAccount};
            let response=session.runtime.block_on(session.client.bank_mutation(&company,mutation,review.body.clone(),true));
            let message=match response{
                Ok(value)if value["ok"]==true=>{
                    let summary=receipt(&value,review.operation,&review.body);
                    if let Some(summary)=summary{bank_state::finish(&record)?;summary}else{"Udfaldet er uafklaret: serverkvitteringen var ufuldstændig. Anmodningen bevares uden gentagelse.".into()}
                }
                Err(Error::Refused{status})=>{bank_state::finish(&record)?;format!("Serveren afviste anmodningen (HTTP{status}). Intet bekræftet; kontrollér adgang, CSV og konto.")}
                _=>"Udfaldet er uafklaret. Importen kan være gennemført. Anmodningen bevares; der sendes ikke igen. Afklar med bankrækker og serverens browservisning.".into(),
            };
            Ok::<_,String>((message,read(&session,&company,&year),bank_state::load()?))
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, window, cx| {
                this.busy = false;
                this.review = None;
                this.reviewed = false;
                match result {
                    Ok((message, data, pending)) => {
                        this.pending = pending;
                        this.status = message;
                        match data {
                            Ok(data) => {
                                let banks = rows(&data.bank, "accounts")
                                    .unwrap_or_default()
                                    .iter()
                                    .filter(|v| eligible(v))
                                    .map(|v| SharedString::from(label(v)))
                                    .collect::<Vec<_>>();
                                this.bank.update(cx, |state, cx| {
                                    state.set_items(SearchableVec::new(banks), window, cx)
                                });
                                this.data = Some(data);
                            }
                            Err(error) => {
                                this.data = None;
                                this.status
                                    .push_str(&format!(" Genindlæsning fejlede: {error}"));
                            }
                        }
                    }
                    Err(error) => {
                        this.status = error;
                        this.pending = bank_state::load().ok().flatten();
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
impl Render for Bank {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let blocked = self.busy || self.pending.is_some();
        let mut view = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_xl().child("Bank"))
            .child(format!("{} · {}", self.company, self.origin))
            .child(self.status.clone())
            .child(Input::new(&self.year).disabled(self.busy || self.review.is_some()))
            .child(
                Button::new("bank-refresh")
                    .label("Genindlæs bank og valgt regnskabsår")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.load(window, cx))),
            );
        let Some(data) = &self.data else {
            return view;
        };
        view = view.child(format!(
            "Regnskabsår {} · {} til {} · {}",
            data.bank["selectedYear"].as_str().unwrap_or("?"),
            data.bank["periodStart"].as_str().unwrap_or("?"),
            data.bank["periodEnd"].as_str().unwrap_or("?"),
            data.bank["company"]["currency"].as_str().unwrap_or("?")
        ));
        view = view.child("Bankkonti og kontotilknytning");
        for account in data.bank["accounts"].as_array().into_iter().flatten() {
            view = view.child(format!(
                "{} · {}",
                label(account),
                if account["active"] == true {
                    "Aktiv"
                } else {
                    "Inaktiv"
                }
            ));
        }
        if let Some(record) = self.pending.clone() {
            let same = bank_state::same_scope(
                &record,
                &self.origin,
                &self.company,
                &data.principal,
                &data.ledger,
                self.local.as_deref(),
            );
            view=view.child("UAFKLARET: En tidligere anmodning kan være gennemført. Bankrækker alene beviser ikke denne imports identitet. Ingen automatisk gentagelse.")
                .child(format!("Oprindelig server: {} · Virksomhed: {} · Valgt år: {}",record["origin"].as_str().unwrap_or("?"),record["company"].as_str().unwrap_or("?"),record["year"].as_str().unwrap_or("?")))
                .child(record["body"].as_str().unwrap_or("Anmodningen kunne ikke vises").to_owned());
            if let Some(origin) = record["origin"].as_str() {
                let url = format!("{origin}/");
                view = view.child(
                    Button::new("bank-reconcile-browser")
                        .label("Åbn oprindelig server til manuel afklaring")
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                );
            }
            if same {
                let before = record["beforeIds"].as_array();
                for transaction in data.bank["transactions"].as_array().into_iter().flatten() {
                    if !before.is_some_and(|ids| ids.contains(&transaction["id"])) {
                        view = view.child(format!(
                            "Ny synlig bankrække siden forsøget (ikke importbevis): id{} · {} · {}",
                            transaction["id"],
                            transaction["date"].as_str().unwrap_or("?"),
                            transaction["text"].as_str().unwrap_or("?")
                        ));
                    }
                }
                view=view.child(Checkbox::new("bank-manually-resolved").label("Jeg har afklaret udfaldet manuelt i den korrekte virksomhed. Arkivér forsøget uden genafsendelse.").checked(self.resolved).on_click(cx.listener(|this,checked,_,cx|{this.resolved=*checked;cx.notify();})))
                    .child(Button::new("bank-resolve").label("Bekræft manuel afklaring og bevar anmodningen i privat arkiv").disabled(self.busy||!self.resolved).on_click(cx.listener(|this,_,_,cx|{
                        if !this.resolved||this.busy{return;}let Some(record)=&this.pending else{return;};let Some(data)=&this.data else{return;};
                        if !bank_state::same_scope(record,&this.origin,&this.company,&data.principal,&data.ledger,this.local.as_deref()){return;}
                        match bank_state::resolve(record){Ok(())=>{this.pending=None;this.resolved=false;this.status="Afklaring registreret af dig; originalanmodningen er bevaret privat. Dette er ikke en serverkvittering.".into();},Err(error)=>this.status=error}cx.notify();
                    })));
            } else {
                view=view.child("Åbn den oprindelige bruger og virksomhed for at afklare forsøget. Der sendes ingen ny anmodning.");
            }
        } else if let Some(review) = self.review.clone() {
            view = view.child(if review.operation == "import" {
                "Gennemgå importen — hele kilden og modtagerkonto er fastholdt"
            } else {
                "Gennemgå ny bankkonto og dens kontotilknytning"
            });
            if let Some(target) = &review.target {
                view = view.child(label(target));
            }
            let body: Value = serde_json::from_slice(&review.body).unwrap_or(Value::Null);
            view = view
                .child(if review.operation == "import" {
                    body["csvContent"]
                        .as_str()
                        .unwrap_or("Ugyldig CSV")
                        .to_owned()
                } else {
                    serde_json::to_string_pretty(&body).unwrap_or_default()
                })
                .child(
                    Checkbox::new("bank-reviewed")
                        .label(
                            "Jeg har gennemgået præcis konto, kontotilknytning og kilde/anmodning.",
                        )
                        .checked(self.reviewed)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            this.reviewed = *checked;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("bank-confirm")
                        .label(if review.operation == "import" {
                            "Bekræft import én gang"
                        } else {
                            "Bekræft oprettelse af bankkonto"
                        })
                        .disabled(self.busy || !self.reviewed)
                        .on_click(cx.listener(|this, _, window, cx| this.send(window, cx))),
                )
                .child(
                    Button::new("bank-cancel-review")
                        .label("Annullér gennemgang")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.review = None;
                            this.reviewed = false;
                            cx.notify();
                        })),
                );
        } else {
            view=view.child("Import til aktiv bankkonto med eksplicit kontotilknytning")
                .child(Select::new(&self.bank).disabled(blocked))
                .child("Generisk UTF-8 CSV: date;text;amount. Valgfrit currency,reference,booking_date,balance_after,amount_dkk,fx_rate_to_dkk. Navngivne bankprofiler er ikke tilgængelige. Serveren validerer kilden.")
                .child(Button::new("bank-csv").label("Vælg CSV-fil…").disabled(blocked).on_click(cx.listener(|this,_,_,cx|this.choose_file(cx))))
                .child(self.source_name.clone())
                .child(Button::new("bank-review-import").label("Gennemgå CSV og importkonto").disabled(blocked||self.source.is_none()).on_click(cx.listener(|this,_,_,cx|this.review(false,cx))))
                .child("Opret bankkonto med eksplicit tilknytning til en aktiv aktivkonto")
                .child(Input::new(&self.name).disabled(blocked)).child(Select::new(&self.ledger).disabled(blocked))
                .child(Button::new("bank-review-create").label("Gennemgå ny bankkonto").disabled(blocked).on_click(cx.listener(|this,_,_,cx|this.review(true,cx))));
        }
        view = view.child(
            div()
                .text_lg()
                .child("Bankrækker · alle bankkonti i valgt regnskabsår"),
        );
        for transaction in data.bank["transactions"].as_array().into_iter().flatten() {
            view = view.child(format!(
                "id{} · {} · {} · Beløb {} · {}",
                transaction["id"],
                transaction["date"].as_str().unwrap_or("?"),
                transaction["text"].as_str().unwrap_or("?"),
                transaction["amount"],
                transaction["reconciliationStatus"].as_str().unwrap_or("?")
            ));
        }
        view
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn import_freezes_exact_csv_and_explicit_mapped_bank_without_fake_idempotency() {
        let source = "date;text;amount;currency\n2026-10-19;Synthetic payment;-12.34;DKK\n";
        let bank = json!({"slug":"synthetic","active":true,"ledgerAccountNo":"1800"});
        let body: Value = serde_json::from_slice(&import_body(source, &bank).unwrap()).unwrap();
        assert_eq!(
            body,
            json!({"csvContent":source,"account":"synthetic","confirm":true})
        );
        assert!(import_body(source, &json!({"slug":"other","active":true})).is_err());
        assert!(
            import_body(
                source,
                &json!({"slug":"other","active":false,"ledgerAccountNo":"1800"})
            )
            .is_err()
        );
        assert!(import_body(&"x".repeat(240001), &bank).is_err());
    }
    #[::core::prelude::v1::test]
    fn csv_file_requires_bounded_utf8_regular_file() {
        let path = std::env::temp_dir().join(format!("rm-bank-source-{}", std::process::id()));
        std::fs::write(&path, b"date;text;amount\n2026-10-19;Synthetic;1\n").unwrap();
        assert!(file(&path).unwrap().contains("Synthetic"));
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(file(&path).is_err());
        std::fs::write(&path, vec![b'x'; 240001]).unwrap();
        assert!(file(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
    #[::core::prelude::v1::test]
    fn receipt_requires_matching_import_account_and_preserves_server_dedup_counts() {
        let body = br#"{"csvContent":"source","account":"synthetic","confirm":true}"#;
        let mut response = json!({"ok":true,"import":{"bankAccountSlug":"synthetic","imported":0,"skippedDuplicates":2,"importBatchId":"BANK-synthetic"}});
        assert_eq!(
            receipt(&response, "import", body).unwrap(),
            "Serverkvittering BANK-synthetic: 0 importerede · 2 dubletter sprunget over."
        );
        response["import"]["bankAccountSlug"] = json!("different");
        assert!(receipt(&response, "import", body).is_none());
        response["import"]["bankAccountSlug"] = json!("synthetic");
        response["import"]
            .as_object_mut()
            .unwrap()
            .remove("imported");
        assert!(receipt(&response, "import", body).is_none());
    }
}
