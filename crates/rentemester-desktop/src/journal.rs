//! Focused native manual transfer. Server owns accounting validation and effects.
use crate::{
    Session,
    journal_attempt::{self, Attempt},
    readable,
};
use gpui_kit::{
    component::{
        Disableable,
        button::{Button, ButtonVariants},
        input::{Input, InputState},
        select::{SearchableVec, Select, SelectState},
    },
    *,
};
use rentemester_client::{JournalMutation, Report};
use serde_json::{Value, json};
use std::{io::Read, sync::Arc};
type Choices = SelectState<SearchableVec<SharedString>>;
pub struct Journal {
    session: Arc<Session>,
    origin: String,
    company: String,
    principal: String,
    ledger_uuid: String,
    local_identity: Option<String>,
    date: Entity<InputState>,
    text: Entity<InputState>,
    amount: Entity<InputState>,
    debit: Entity<Choices>,
    credit: Entity<Choices>,
    document: Entity<Choices>,
    accounts: Vec<(String, String)>,
    documents: Vec<(String, String)>,
    version: String,
    review: Option<Attempt>,
    pending: bool,
    busy: bool,
    status: String,
    receipt: Option<(i64, String)>,
}
fn text(v: &Value, k: &str) -> Result<String, String> {
    v[k].as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("Server response missing {k}"))
}
fn exact_amount(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().replace(',', ".");
    let mut parts = normalized.split('.');
    let whole = parts.next().unwrap_or("");
    let cents = parts.next().unwrap_or("");
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || cents.len() > 2
        || !cents.bytes().all(|b| b.is_ascii_digit())
        || parts.next().is_some()
    {
        return Err("Indtast et positivt beløb med højst to decimaler.".into());
    }
    let whole = whole.trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    let result = format!("{whole}.{cents:0<2}");
    if result == "0.00" {
        return Err("Beløbet skal være større end nul.".into());
    }
    Ok(result)
}
fn definitive_first_rejection(error: &rentemester_client::Error, previously_pending: bool) -> bool {
    !previously_pending
        && matches!(
            error,
            rentemester_client::Error::Refused {
                status: 400 | 401 | 403 | 404 | 409 | 422
            }
        )
}
fn key() -> Result<String, String> {
    let mut bytes = [0; 24];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|_| "Posteringsnøgle kunne ikke oprettes")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn body(
    version: &str,
    date: &str,
    note: &str,
    debit: &str,
    credit: &str,
    amount: &str,
    document: Option<&str>,
    key: &str,
) -> Result<Vec<u8>, String> {
    if date.len() != 10
        || note.trim().is_empty()
        || note.len() > 500
        || debit.is_empty()
        || credit.is_empty()
        || debit == credit
    {
        return Err("Angiv dato, tekst og to forskellige konti.".into());
    }
    let mut request = json!({"transactionDate":date,"text":note,"currency":"DKK","lines":[{"accountNo":debit,"debitAmount":exact_amount(amount)?},{"accountNo":credit,"creditAmount":exact_amount(amount)?}]});
    if let Some(id) = document {
        request["documentId"] = json!(id);
    }
    serde_json::to_vec(&json!({"confirm":true,"input":{"format":"rentemester-manual-journal-post/1","expectedRuleVersion":version,"idempotencyKey":key,"request":request}})).map_err(|_|"Posteringen kunne ikke læses".into())
}
fn document_choices(docs: &Value) -> Result<Vec<(String, String)>, String> {
    docs["documents"]["documents"]
        .as_array()
        .ok_or("Server response missing documents")?
        .iter()
        .map(|d| {
            let id = d["id"]
                .as_i64()
                .ok_or("Server response missing document id")?
                .to_string();
            Ok((
                format!(
                    "{} · {}",
                    text(d, "documentNo")?,
                    d["supplierName"]
                        .as_str()
                        .or(d["filename"].as_str())
                        .unwrap_or("Bilag")
                ),
                id,
            ))
        })
        .collect::<Result<Vec<_>, String>>()
}
struct Loaded {
    principal: String,
    ledger_uuid: String,
    version: String,
    accounts: Vec<(String, String)>,
    documents: Vec<(String, String)>,
    pending: Option<Attempt>,
}
fn load(
    session: &Session,
    origin: &str,
    company: &str,
    local_identity: Option<&str>,
) -> Result<Loaded, String> {
    let me = session
        .runtime
        .block_on(session.client.me())
        .map_err(readable)?;
    let principal = text(&me["user"], "id")?;
    let report = session
        .runtime
        .block_on(session.client.report(company, Report::Journal, None, None))
        .map_err(readable)?;
    if report["ok"] != true
        || report
            .pointer("/journal/company/currency")
            .and_then(Value::as_str)
            != Some("DKK")
    {
        return Err("Den native formular kræver en virksomhed i DKK. Brug serverens browser til øvrige valutaer.".into());
    }
    let ledger_uuid = text(&report["journal"], "ledgerUuid")?;
    if ledger_uuid.len() != 32
        || !ledger_uuid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Server response missing canonical ledger identity".into());
    }
    let options = &report["journal"]["nativePosting"];
    let accounts = options["accounts"]
        .as_array()
        .ok_or("Server response missing posting accounts")?
        .iter()
        .filter(|a| a["active"] == true && a["allowDirectPosting"] == true)
        .map(|a| {
            let no = text(a, "accountNo")?;
            Ok((format!("{no} · {}", text(a, "name")?), no))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let docs = session
        .runtime
        .block_on(session.client.documents(company))
        .map_err(readable)?;
    if docs["ok"] != true {
        return Err("Serveren bekræftede ikke bilagslisten.".into());
    }
    let documents = document_choices(&docs)?;
    let pending = journal_attempt::Attempt::load(&journal_attempt::directory())?;
    if let Some(a) = &pending {
        if a.ledger_uuid != ledger_uuid
            || (!a.matches(origin, company, &principal)
                && !a.can_rebind(company, &principal, &ledger_uuid, local_identity))
        {
            return Err("Der findes en uafsluttet postering for en anden server, virksomhed eller bruger. Åbn den oprindelige forbindelse og afklar forsøget først.".into());
        }
    }
    Ok(Loaded {
        principal,
        ledger_uuid,
        version: text(options, "ruleVersion")?,
        accounts,
        documents,
        pending,
    })
}
impl Journal {
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
            session,
            origin,
            company,
            principal: String::new(),
            ledger_uuid: String::new(),
            local_identity,
            date: input("Dato (YYYY-MM-DD)", window, cx),
            text: input("Posteringstekst", window, cx),
            amount: input("Beløb i DKK", window, cx),
            debit: select(window, cx),
            credit: select(window, cx),
            document: select(window, cx),
            accounts: vec![],
            documents: vec![],
            version: String::new(),
            review: None,
            pending: false,
            busy: false,
            status: String::new(),
            receipt: None,
        };
        this.reload(window, cx);
        this
    }
    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status = "Henter konti og bilag fra serveren…".into();
        let session = self.session.clone();
        let origin = self.origin.clone();
        let company = self.company.clone();
        let local_identity = self.local_identity.clone();
        let task = cx
            .background_executor()
            .spawn(async move { load(&session, &origin, &company, local_identity.as_deref()) });
        cx.spawn_in(window, async move |view, cx| {
            let result = task.await;
            let _ = view.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(data) => {
                        this.principal = data.principal;
                        this.ledger_uuid = data.ledger_uuid;
                        this.version = data.version;
                        this.accounts = data.accounts;
                        this.documents = data.documents;
                        let labels = || SearchableVec::new(this.accounts.iter().map(|a| SharedString::from(a.0.clone())).collect::<Vec<_>>());
                        this.debit.update(cx, |s,cx| s.set_items(labels(),window,cx));
                        this.credit.update(cx, |s,cx| s.set_items(labels(),window,cx));
                        let docs = SearchableVec::new(this.documents.iter().map(|d| SharedString::from(d.0.clone())).collect::<Vec<_>>());
                        this.document.update(cx, |s,cx| s.set_items(docs,window,cx));
                        this.review = data.pending;
                        this.pending = this.review.is_some();
                        this.status = if this.pending {
                            "Et tidligere forsøg er uafsluttet. Gennemgå den gemte anmodning. Gentag kun med samme nøgle; forsøget kan blive bogført nu."
                        } else {
                            "Vælg konti og eventuelt bilag. Serveren kontrollerer regler og adgang ved bogføring."
                        }.into();
                    }
                    Err(error) => this.status = error,
                }
                cx.notify();
            });
        }).detach();
    }
    fn review(&mut self, cx: &mut Context<Self>) {
        let selected = |state: &Entity<Choices>, choices: &[(String, String)]| {
            state.read(cx).selected_value().and_then(|v| {
                choices
                    .iter()
                    .find(|a| a.0 == v.as_ref())
                    .map(|a| a.1.clone())
            })
        };
        let result = (|| {
            let debit = selected(&self.debit, &self.accounts).ok_or("Vælg debetkonto")?;
            let credit = selected(&self.credit, &self.accounts).ok_or("Vælg kreditkonto")?;
            let document = selected(&self.document, &self.documents);
            Ok::<_, String>(Attempt {
                origin: self.origin.clone(),
                company: self.company.clone(),
                principal: self.principal.clone(),
                ledger_uuid: self.ledger_uuid.clone(),
                local_identity: self.local_identity.clone(),
                body: body(
                    &self.version,
                    &self.date.read(cx).value(),
                    &self.text.read(cx).value(),
                    &debit,
                    &credit,
                    &self.amount.read(cx).value(),
                    document.as_deref(),
                    &key()?,
                )?,
            })
        })();
        match result {
            Ok(a) => {
                self.review = Some(a);
                self.status =
                    "Gennemgå de præcise linjer. Bogfør sender dem til serverens validering."
                        .into();
            }
            Err(e) => self.status = e,
        }
        cx.notify();
    }
    fn post(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(mut attempt) = self.review.clone() else {
            return;
        };
        if attempt.origin != self.origin {
            match attempt.rebind(
                &journal_attempt::directory(),
                &self.origin,
                &self.company,
                &self.principal,
                &self.ledger_uuid,
                self.local_identity.as_deref(),
            ) {
                Ok(rebound) => {
                    attempt = rebound;
                    self.review = Some(attempt.clone());
                }
                Err(error) => {
                    self.status = error;
                    cx.notify();
                    return;
                }
            }
        }
        let previously_pending = self.pending;
        if !self.pending {
            if let Err(e) = attempt.save(&journal_attempt::directory()) {
                self.status = e;
                cx.notify();
                return;
            }
            self.pending = true;
        }
        self.busy = true;
        self.status = "Sender én anmodning…".into();
        let session = self.session.clone();
        let task = cx.background_executor().spawn(async move {
            let result = match session.runtime.block_on(session.client.manual_journal(
                &attempt.company,
                JournalMutation::Post,
                attempt.body.clone(),
            )) {
                Ok(result) => result,
                Err(error) if definitive_first_rejection(&error, previously_pending) => {
                    attempt.finish(&journal_attempt::directory())?;
                    return Ok((0, String::new(), Ok(()), false));
                }
                Err(error) => return Err(readable(error)),
            };
            if result["ok"] != true {
                return Err(
                    "Serveren bekræftede ikke bogføring. Den gemte anmodning bevares.".into(),
                );
            }
            let id = result["entryId"]
                .as_i64()
                .filter(|n| *n > 0)
                .ok_or("Kvittering mangler entryId; forsøget bevares")?;
            let number = text(&result, "entryNo")?;
            let cleanup = attempt.finish(&journal_attempt::directory());
            let read = session
                .runtime
                .block_on(
                    session
                        .client
                        .report(&attempt.company, Report::Journal, None, None),
                )
                .map_err(readable);
            let observed = read
                .as_ref()
                .ok()
                .and_then(|v| v.pointer("/journal/entries"))
                .and_then(Value::as_array)
                .is_some_and(|entries| entries.iter().any(|e| e["id"] == id));
            Ok::<_, String>((id, number, cleanup, observed))
        });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |this,cx| {
                this.busy = false;
                match result {
                    Ok((0,_,_,_)) => {
                        this.pending = false;
                        this.review = None;
                        this.status = "Serveren afviste den første anmodning uden en bogføringskvittering. Kontrollér dato, bilag, konti, perioder og din adgang, og gennemgå rettelserne igen.".into();
                    }
                    Ok((id,no,cleanup,observed)) => {
                        this.receipt = Some((id,no.clone()));
                        this.pending = cleanup.is_err();
                        if !this.pending { this.review = None; }
                        this.status = format!("Bogført: {no} (id {id}). {}{}", if observed { "Posteringen er læst tilbage fra serveren." } else { "Kvittering modtaget; posteringen ses ikke i det aktuelle regnskabsår. Genindlæs serverens rapport." }, cleanup.err().map(|e|format!(" {e}")).unwrap_or_default());
                    }
                    Err(error) => this.status = format!("{error}. Udfaldet af forsøget skal afklares. Den præcise anmodning og nøgle er gemt. Gentag samme forsøg for at få eller fuldføre serverens kvittering."),
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }
}
impl Render for Journal {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let locked = self.busy || self.review.is_some();
        let mut content=div().flex().flex_col().gap_3().child(div().text_xl().child("Ny manuel postering")).child(format!("Virksomhed: {} · {}",self.company,self.origin)).child("Indtægter og udgifter kræver bilag. Denne formular bogfører et beløb i DKK på to konti; øvrige regler kontrolleres af serveren.").child(self.status.clone());
        if self.version.is_empty() {
            return content.child(
                Button::new("journal-reload")
                    .label("Hent posteringsgrundlag")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.reload(window, cx))),
            );
        }
        if let Some(attempt) = &self.review {
            let v: Value = serde_json::from_slice(&attempt.body).unwrap_or(Value::Null);
            let request = &v["input"]["request"];
            if attempt.origin != self.origin {
                content=content.child(format!("Den samme private lokale virksomhed er genåbnet. Tidligere endpoint: {}. Nyt endpoint: {}. Bekræft for at sende præcis samme nøgle og anmodning til den genåbnede ledger; forsøget kan blive bogført nu.",attempt.origin,self.origin));
            }
            content = content
                .child("Gennemse postering")
                .child(format!(
                    "Dato: {} · Tekst: {}",
                    request["transactionDate"].as_str().unwrap_or("?"),
                    request["text"].as_str().unwrap_or("?")
                ))
                .child(format!(
                    "Bilag: {}",
                    request["documentId"].as_str().unwrap_or("Intet bilag")
                ));
            for line in request["lines"].as_array().into_iter().flatten() {
                content = content.child(format!(
                    "Konto {} · Debet {} · Kredit {} DKK",
                    line["accountNo"].as_str().unwrap_or("?"),
                    line["debitAmount"].as_str().unwrap_or("0.00"),
                    line["creditAmount"].as_str().unwrap_or("0.00")
                ));
            }
            content = content.child(
                Button::new("journal-confirm")
                    .primary()
                    .label(if attempt.origin != self.origin {
                        "Bekræft lokalt endpoint og gentag samme forsøg"
                    } else if self.pending {
                        "Gentag præcis samme forsøg"
                    } else {
                        "Bogfør postering"
                    })
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.post(cx))),
            );
            if !self.pending {
                content = content.child(
                    Button::new("journal-edit")
                        .label("Tilbage og ret")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.review = None;
                            cx.notify();
                        })),
                );
            }
            return content;
        }
        if self.receipt.is_some() {
            return content.child("Kvitteringen er bekræftet. Luk formularen og genindlæs saldobalancen for at se resultatet.");
        }
        content
            .child("Dato")
            .child(Input::new(&self.date).disabled(locked))
            .child("Posteringstekst")
            .child(Input::new(&self.text).disabled(locked))
            .child("Bilag (valgfrit)")
            .child(
                Select::new(&self.document)
                    .accessibility_label("Bilag (valgfrit)")
                    .placeholder("Vælg bilag")
                    .cleanable(true)
                    .disabled(locked),
            )
            .child("Debetkonto")
            .child(
                Select::new(&self.debit)
                    .accessibility_label("Debetkonto")
                    .placeholder("Vælg debetkonto")
                    .disabled(locked),
            )
            .child("Kreditkonto")
            .child(
                Select::new(&self.credit)
                    .accessibility_label("Kreditkonto")
                    .placeholder("Vælg kreditkonto")
                    .disabled(locked),
            )
            .child("Beløb i DKK")
            .child(Input::new(&self.amount).disabled(locked))
            .child(
                Button::new("journal-review")
                    .label("Gennemse postering")
                    .disabled(locked)
                    .on_click(cx.listener(|this, _, _, cx| this.review(cx))),
            )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn prior_unknown_attempt_is_never_discarded_on_later_refusal() {
        let refusal = rentemester_client::Error::Refused { status: 422 };
        assert!(definitive_first_rejection(&refusal, false));
        assert!(!definitive_first_rejection(&refusal, true));
        assert!(!definitive_first_rejection(
            &rentemester_client::Error::OutcomeUnknown,
            false
        ));
    }
    #[::core::prelude::v1::test]
    fn document_selection_uses_actual_nested_native_envelope() {
        let docs = json!({"ok":true,"documents":{"documents":[{"id":7,"documentNo":"2026-D7","supplierName":"Synthetic supplier","filename":"receipt.txt"}]}});
        assert_eq!(
            document_choices(&docs).unwrap(),
            vec![("2026-D7 · Synthetic supplier".into(), "7".into())]
        );
        assert!(document_choices(&json!({"ok":true,"documents":[]})).is_err());
    }
    #[::core::prelude::v1::test]
    fn money_is_exact_text_not_float_math() {
        assert_eq!(exact_amount("000125,5").unwrap(), "125.50");
        for bad in ["0", "-2", "1.001", "1e2", "1.2.3"] {
            assert!(exact_amount(bad).is_err());
        }
    }
    #[::core::prelude::v1::test]
    fn exact_review_body_has_one_key_and_explicit_evidence() {
        let bytes = body(
            "version",
            "2026-10-19",
            "Synthetic",
            "1800",
            "8000",
            "12,34",
            Some("9"),
            "stable-key",
        )
        .unwrap();
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["input"]["idempotencyKey"], "stable-key");
        assert_eq!(v["input"]["request"]["documentId"], "9");
        assert_eq!(v["input"]["request"]["lines"][0]["debitAmount"], "12.34");
        assert!(body("v", "2026-10-19", "Synthetic", "A", "A", "1", None, "key").is_err());
    }
}
