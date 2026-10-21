//! Complete persisted invoice relationship scan. Files are verified by the host.
//! Imported schedules retain hashes, not their original top-level JSON: this
//! module validates persisted relationships and never claims to reconstruct it.
use crate::invalid;
use rentemester_engine::{Error, fiscal_year::Date, money::Money};
use rusqlite::{Transaction, types::ValueRef};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
type Effects = BTreeMap<i64, (Money, Money)>;
fn refused(message: impl Into<String>) -> Error {
    Error::new("invoice_audit_evidence", message)
}
fn require(ok: bool, message: &str) -> Result<(), Error> {
    if ok { Ok(()) } else { Err(refused(message)) }
}
fn number(v: &Value) -> Result<i64, Error> {
    v.as_i64()
        .ok_or_else(|| refused("integer identity required"))
}
fn text(v: &Value) -> Result<&str, Error> {
    v.as_str().ok_or_else(|| refused("text evidence required"))
}
fn money(v: &Value) -> Result<Money, Error> {
    crate::invoice_fx::amount(v)
}
fn positive(v: &Value) -> Result<Money, Error> {
    let m = money(v)?;
    require(
        !m.is_negative() && m != Money::default(),
        "positive exact amount required",
    )?;
    Ok(m)
}
fn rows(
    tx: &Transaction<'_>,
    sql: &str,
    args: &[&dyn rusqlite::ToSql],
) -> Result<Vec<Value>, Error> {
    let mut q = tx.prepare(sql).map_err(invalid)?;
    let names = q
        .column_names()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut cursor = q.query(args).map_err(invalid)?;
    let mut out = vec![];
    while let Some(r) = cursor.next().map_err(invalid)? {
        let mut value = serde_json::Map::new();
        for (i, name) in names.iter().enumerate() {
            let v = match r.get_ref(i).map_err(invalid)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Real(n) => serde_json::Number::from_f64(n)
                    .map(Value::Number)
                    .ok_or_else(|| refused("nonfinite stored number"))?,
                ValueRef::Text(s) => json!(std::str::from_utf8(s).map_err(invalid)?),
                ValueRef::Blob(_) => return Err(refused("unexpected blob evidence")),
            };
            value.insert(name.clone(), v);
        }
        out.push(Value::Object(value));
    }
    Ok(out)
}
fn one(tx: &Transaction<'_>, sql: &str, id: i64) -> Result<Value, Error> {
    let mut r = rows(tx, sql, &[&id])?;
    require(r.len() == 1, "missing or ambiguous linked record")?;
    Ok(r.remove(0))
}
fn journal(
    tx: &Transaction<'_>,
    id: i64,
    document: i64,
    bank: Option<i64>,
    date: &str,
) -> Result<Value, Error> {
    Date::parse(date)?;
    let j = one(
        tx,
        "SELECT *,EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) reversed FROM journal_entries j WHERE id=?1",
        id,
    )?;
    require(
        j["status"] == "posted"
            && j["document_id"] == document
            && j["source_bank_transaction_id"].as_i64() == bank
            && j["transaction_date"] == date
            && j["reversal_of_entry_id"].is_null()
            && j["reversed"] == 0,
        "journal document/bank/date/status/reversal identity differs",
    )?;
    Ok(j)
}
fn lines(tx: &Transaction<'_>, id: i64) -> Result<Vec<Value>, Error> {
    rows(
        tx,
        "SELECT l.*,a.account_no,a.type kind,a.normal_balance FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1 ORDER BY l.id",
        &[&id],
    )
}
fn effects(lines: &[Value]) -> Result<Effects, Error> {
    let mut out = Effects::new();
    require(!lines.is_empty(), "journal lines missing")?;
    for line in lines {
        let debit = money(&line["debit_amount"])?;
        let credit = money(&line["credit_amount"])?;
        require(
            !debit.is_negative() && !credit.is_negative(),
            "negative journal amount",
        )?;
        let e = out.entry(number(&line["account_id"])?).or_default();
        e.0.add(&debit);
        e.1.add(&credit);
    }
    Ok(out)
}
fn effect(out: &mut Effects, id: i64, debit: Money, credit: Money) {
    let e = out.entry(id).or_default();
    e.0.add(&debit);
    e.1.add(&credit);
}
fn equivalent(mut left: Effects, mut right: Effects) -> bool {
    left.retain(|_, e| e.0 != Money::default() || e.1 != Money::default());
    right.retain(|_, e| e.0 != Money::default() || e.1 != Money::default());
    left == right
}
fn record(errors: &mut Vec<Value>, family: &str, id: i64, result: Result<(), Error>) {
    if let Err(e) = result {
        errors.push(
            json!({"code":"INVALID_INVOICE_EVIDENCE","family":family,"id":id,"error":e.message}),
        );
    }
}
fn account(tx: &Transaction<'_>, id: i64, kind: &str, normal: &str) -> Result<(), Error> {
    let a = one(tx, "SELECT * FROM accounts WHERE id=?1", id)?;
    require(
        a["type"] == kind && a["normal_balance"] == normal,
        "linked account kind/normal balance differs",
    )
}
fn posted_invoice(tx: &Transaction<'_>, d: &Value, p: &Value) -> Result<(), Error> {
    let id = number(&d["id"])?;
    let payload: Value = serde_json::from_str(text(&d["payload_json"])?).map_err(invalid)?;
    require(
        payload["invoiceNumber"] == d["invoice_no"]
            && payload["issueDate"] == d["invoice_date"]
            && payload["currency"] == d["currency"],
        "frozen invoice identity differs",
    )?;
    let projection = crate::invoice_fx::projection(&payload)?;
    let original = rentemester_engine::sales_invoice::project(&payload)?;
    require(
        original.gross == money(&d["amount_inc_vat"])?
            && projection.gross == money(&p["booked_gross_dkk"])?,
        "invoice original/frozen DKK/posting totals differ",
    )?;
    let jid = number(&p["journal_entry_id"])?;
    let j = journal(tx, jid, id, None, text(&d["invoice_date"])?)?;
    require(
        j["currency"] == d["currency"],
        "invoice journal currency differs",
    )?;
    let debtor = number(&p["receivable_account_id"])?;
    account(tx, debtor, "asset", "debit")?;
    let ls = lines(tx, jid)?;
    let mut debt = Money::default();
    let mut income = Money::default();
    let mut vat = Money::default();
    let mut bases = BTreeMap::<String, Money>::new();
    for line in &projection.lines {
        bases
            .entry(line.vat_code.to_owned())
            .or_default()
            .add(&line.net);
    }
    for l in &ls {
        let dr = money(&l["debit_amount"])?;
        let cr = money(&l["credit_amount"])?;
        if l["account_id"] == debtor {
            require(cr == Money::default(), "invoice receivable has credit")?;
            debt.add(&dr);
        } else if l["kind"] == "income" && l["normal_balance"] == "credit" {
            require(dr == Money::default(), "invoice income has debit")?;
            income.add(&cr);
            let code = l["vat_code"].as_str().unwrap_or("");
            let remaining = bases.entry(code.to_owned()).or_default();
            *remaining = remaining.subtract(&cr);
        } else if l["kind"] == "vat" && l["normal_balance"] == "credit" {
            require(dr == Money::default(), "output VAT has debit")?;
            vat.add(&cr);
        } else {
            return Err(refused("unexpected invoice posting counteraccount"));
        }
    }
    require(
        debt == projection.gross
            && income == projection.net
            && vat == projection.vat
            && bases.values().all(|v| *v == Money::default()),
        "exact invoice debtor/revenue/VAT classification differs",
    )?;
    if d["currency"] != "DKK" {
        require(
            money(&j["amount_foreign"])? == original.gross
                && money(&j["amount_dkk"])? == projection.gross
                && j["fx_rate_to_dkk"] == payload["totals"]["fxRateToDkk"],
            "frozen invoice FX metadata differs",
        )?;
    }
    Ok(())
}
fn claims(
    tx: &Transaction<'_>,
    id: i64,
    errors: &mut Vec<Value>,
) -> Result<BTreeMap<i64, Effects>, Error> {
    let mut events = BTreeMap::new();
    for (table, link, key, amount, date) in [
        (
            "invoice_reminders",
            "invoice_reminder_postings",
            "reminder_id",
            "fee_amount",
            "reminder_date",
        ),
        (
            "invoice_compensation_claims",
            "invoice_compensation_postings",
            "compensation_claim_id",
            "amount_dkk",
            "claim_date",
        ),
        (
            "invoice_interest_claims",
            "invoice_interest_postings",
            "interest_claim_id",
            "amount_dkk",
            "claim_date",
        ),
    ] {
        let sql = format!(
            "SELECT c.*,p.journal_entry_id FROM {table} c LEFT JOIN {link} p ON p.{key}=c.id WHERE c.invoice_document_id=?1 ORDER BY c.id"
        );
        for c in rows(tx, &sql, &[&id])? {
            let result = (|| {
                let amount = positive(&c[amount])?;
                Date::parse(text(&c[date])?)?;
                if table == "invoice_reminders" {
                    require(
                        c["currency"] == "DKK"
                            && !Money::parse("100")?.subtract(&amount).is_negative(),
                        "reminder currency/fee differs",
                    )?;
                }
                if table == "invoice_compensation_claims" {
                    require(
                        !Money::parse("310")?.subtract(&amount).is_negative(),
                        "compensation exceeds source statutory maximum",
                    )?;
                }
                if table == "invoice_interest_claims" {
                    positive(&c["principal_open_balance"])?;
                    require(
                        number(&c["overdue_days"])? > 0
                            && ["manual-override", "statutory-table"]
                                .contains(&text(&c["reference_rate_source"])?),
                        "interest registration window/rate source differs",
                    )?;
                    let mut annual = money(&c["reference_rate_percent"])?;
                    annual.add(&Money::parse("8")?);
                    require(
                        annual == money(&c["annual_interest_rate_percent"])?,
                        "interest annual rate differs from reference plus eight",
                    )?;
                }
                let Some(jid) = c["journal_entry_id"].as_i64() else {
                    return Ok(());
                };
                let header = one(
                    tx,
                    "SELECT transaction_date FROM journal_entries WHERE id=?1",
                    jid,
                )?;
                let posted = text(&header["transaction_date"])?;
                require(
                    posted >= text(&c[date])?,
                    "claim posting predates registration",
                )?;
                let j = journal(tx, jid, id, None, posted)?;
                require(
                    j["currency"] == "DKK"
                        && j["amount_foreign"].is_null()
                        && j["amount_dkk"].is_null()
                        && j["fx_rate_to_dkk"].is_null(),
                    "claim posting has foreign metadata",
                )?;
                let mut assets = Money::default();
                let mut incomes = Money::default();
                let mut debits = Effects::new();
                for l in lines(tx, jid)? {
                    require(
                        l["vat_code"].is_null() && l["currency"].as_str().unwrap_or("DKK") == "DKK",
                        "claim has VAT/foreign line",
                    )?;
                    let dr = money(&l["debit_amount"])?;
                    let cr = money(&l["credit_amount"])?;
                    if l["kind"] == "asset"
                        && l["normal_balance"] == "debit"
                        && cr == Money::default()
                    {
                        assets.add(&dr);
                        effect(&mut debits, number(&l["account_id"])?, dr, Money::default());
                    } else if l["kind"] == "income"
                        && l["normal_balance"] == "credit"
                        && dr == Money::default()
                    {
                        incomes.add(&cr);
                    } else {
                        return Err(refused("claim posting has unexpected account effects"));
                    }
                }
                require(
                    assets == amount && incomes == amount,
                    "claim posting amount differs from registered claim",
                )?;
                require(
                    events.insert(jid, debits).is_none(),
                    "claim journal reused across claim families",
                )?;
                Ok::<(), Error>(())
            })();
            record(errors, table, number(&c["id"])?, result);
        }
    }
    Ok(events)
}
fn bad_debt(tx: &Transaction<'_>, row: &Value) -> Result<(), Error> {
    let id = number(&row["id"])?;
    let gross = positive(&row["gross_amount"])?;
    let net = money(&row["net_amount"])?;
    let vat = money(&row["vat_amount"])?;
    let mut sum = net.clone();
    sum.add(&vat);
    require(
        sum == gross && !net.is_negative() && !vat.is_negative(),
        "bad debt net/VAT/gross differ",
    )?;
    let evidence = one(
        tx,
        "SELECT * FROM invoice_bad_debt_writeoff_journal_evidence WHERE writeoff_id=?1",
        id,
    )?;
    require(
        evidence["is_valid"] == 1,
        "bad debt source/date/VAT relief/receivable evidence differs",
    )?;
    let jid = number(&row["journal_entry_id"])?;
    journal(
        tx,
        jid,
        number(&row["invoice_document_id"])?,
        None,
        text(&row["writeoff_date"])?,
    )?;
    let mut expense = Money::default();
    let mut output = Money::default();
    let mut receivable = Money::default();
    for l in lines(tx, jid)? {
        let dr = money(&l["debit_amount"])?;
        let cr = money(&l["credit_amount"])?;
        match (l["kind"].as_str(), l["normal_balance"].as_str()) {
            (Some("expense"), Some("debit")) if cr == Money::default() => expense.add(&dr),
            (Some("vat"), Some("credit")) if cr == Money::default() => output.add(&dr),
            (Some("asset"), Some("debit")) if dr == Money::default() => receivable.add(&cr),
            _ => return Err(refused("bad debt unexpected journal account effects")),
        }
    }
    let cents = |name: &str| -> Result<Money, Error> {
        let n = number(&evidence[name])?;
        require(n >= 0, "negative bad debt frozen carrying split")?;
        Money::parse(&format!("{}.{:02}", n / 100, n % 100))
    };
    require(
        expense == cents("expected_net_ore")?
            && output == cents("expected_vat_ore")?
            && receivable == cents("expected_gross_ore")?,
        "bad debt exact frozen DKK journal amounts differ",
    )
}
fn imported(tx: &Transaction<'_>, h: &Value) -> Result<(), Error> {
    let id = number(&h["id"])?;
    let gross = positive(&h["gross_amount"])?;
    let date = text(&h["invoice_date"])?;
    Date::parse(date)?;
    if !h["due_date"].is_null() {
        Date::parse(text(&h["due_date"])?)?;
        require(
            text(&h["due_date"])? >= date,
            "imported due date predates invoice",
        )?;
    }
    let hash = |v: &Value| {
        v.as_str()
            .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    };
    require(
        hash(&h["source_document_hash"])
            && hash(&h["schedule_hash"])
            && !text(&h["external_invoice_id"])?.trim().is_empty()
            && !text(&h["source_recognition_ref"])?.trim().is_empty(),
        "imported source identity/hash evidence missing",
    )?;
    let attempt = number(&h["dinero_import_attempt_id"])?;
    require(
        tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM dinero_import_attempts WHERE id=?1)",
            [attempt],
            |r| r.get::<_, bool>(0),
        )
        .map_err(invalid)?,
        "import attempt missing",
    )?;
    require(
        tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1)",
            [text(&h["control_account_no"])?],
            |r| r.get::<_, bool>(0),
        )
        .map_err(invalid)?,
        "imported control account missing",
    )?;
    let mut paid = Money::default();
    let mut ids = BTreeSet::new();
    for e in rows(
        tx,
        "SELECT * FROM imported_receivable_events WHERE receivable_id=?1 ORDER BY id",
        &[&id],
    )? {
        Date::parse(text(&e["effective_date"])?)?;
        require(
            text(&e["effective_date"])? >= date
                && ["payment", "credit_note"].contains(&text(&e["event_kind"])?)
                && hash(&e["source_document_hash"])
                && e["schedule_hash"] == h["schedule_hash"]
                && !text(&e["source_event_ref"])?.trim().is_empty()
                && !text(&e["external_event_id"])?.trim().is_empty()
                && ids.insert(text(&e["external_event_id"])?.to_owned()),
            "imported event source/date/kind/schedule identity differs",
        )?;
        paid.add(&positive(&e["amount"])?);
    }
    require(
        !gross.subtract(&paid).is_negative(),
        "imported payments/credits exceed invoice gross",
    )
}
fn legacy_foreign_relief(
    tx: &Transaction<'_>,
    invoice: i64,
    debtor: i64,
    journal: i64,
    gross: &Money,
    amount: &Money,
) -> Result<Money, Error> {
    let mut open = gross.clone();
    for r in rows(
        tx,
        "SELECT amount FROM invoice_payments WHERE invoice_document_id=?1 AND journal_entry_id<?2 UNION ALL SELECT c.amount_inc_vat FROM credit_note_postings p JOIN documents c ON c.id=p.credit_note_document_id JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.original_invoice_document_id=?1 AND j.id<?2 AND j.status='posted' AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) UNION ALL SELECT gross_amount FROM invoice_bad_debt_writeoffs WHERE invoice_document_id=?1 AND journal_entry_id<?2",
        &[&invoice, &journal],
    )? {
        open = open.subtract(&positive(&r["amount"])?);
    }
    let mut carrying = Money::default();
    for l in rows(
        tx,
        "SELECT l.debit_amount,l.credit_amount FROM invoice_principal_receivable_effects e JOIN journal_entries j ON j.id=e.journal_entry_id JOIN journal_lines l ON l.journal_entry_id=j.id AND l.account_id=e.receivable_account_id WHERE e.invoice_document_id=?1 AND e.receivable_account_id=?2 AND j.id<?3 ORDER BY j.id,l.id",
        &[&invoice, &debtor, &journal],
    )? {
        carrying.add(&money(&l["debit_amount"])?.subtract(&money(&l["credit_amount"])?));
    }
    crate::invoice_fx::portion(&open, &carrying, &Money::default(), amount)
}
fn cash(
    tx: &Transaction<'_>,
    d: &Value,
    p: &Value,
    claim_events: &BTreeMap<i64, Effects>,
) -> Result<(), Error> {
    let id = number(&d["id"])?;
    let debtor = number(&p["receivable_account_id"])?;
    let currency = text(&d["currency"])?;
    let gross = positive(&d["amount_inc_vat"])?;
    let frozen = positive(&p["booked_gross_dkk"])?;
    let applications = rows(
        tx,
        "SELECT 'payment' kind,id,invoice_document_id,bank_transaction_id,journal_entry_id,payment_date date,amount,currency FROM invoice_payments WHERE invoice_document_id=?1 UNION ALL SELECT 'refund',id,invoice_document_id,bank_transaction_id,journal_entry_id,refund_date,amount,currency FROM invoice_refunds WHERE invoice_document_id=?1 UNION ALL SELECT 'claim',id,invoice_document_id,bank_transaction_id,journal_entry_id,payment_date,amount,currency FROM invoice_claim_payments WHERE invoice_document_id=?1 ORDER BY journal_entry_id,kind,id",
        &[&id],
    )?;
    let mut groups = BTreeMap::<i64, Vec<Value>>::new();
    for a in applications {
        groups
            .entry(number(&a["journal_entry_id"])?)
            .or_default()
            .push(a);
    }
    let mut paid = Money::default();
    let mut refunded = Money::default();
    let mut balances = BTreeMap::<i64, Money>::new();
    let mut applied_claims = BTreeSet::new();
    for (jid, group) in groups {
        for (claim_journal, effects) in claim_events.range(..jid) {
            if applied_claims.insert(*claim_journal) {
                for (account, (debit, credit)) in effects {
                    balances
                        .entry(*account)
                        .or_default()
                        .add(&debit.subtract(credit));
                }
            }
        }
        let first = group[0].clone();
        require(
            group.len() == 1
                || group.len() == 2
                    && group.iter().any(|a| a["kind"] == "claim")
                    && group.iter().any(|a| a["kind"] == "payment"),
            "unsupported/reused cash application journal group",
        )?;
        let bank = number(&first["bank_transaction_id"])?;
        let date = text(&first["date"])?;
        let refund = first["kind"] == "refund";
        let j = journal(tx, jid, id, Some(bank), date)?;
        let b = one(
            tx,
            "SELECT b.*,a.ledger_account_no,a.currency bank_currency FROM bank_transactions b LEFT JOIN bank_accounts a ON a.id=b.bank_account_id WHERE b.id=?1",
            bank,
        )?;
        let mut total = Money::default();
        let mut expected = Effects::new();
        let mut principal = Money::default();
        for a in group {
            require(
                a["bank_transaction_id"] == bank
                    && a["date"] == date
                    && a["currency"] == first["currency"],
                "grouped cash invoice/bank/date/currency differs",
            )?;
            let amount = positive(&a["amount"])?;
            total.add(&amount);
            if a["kind"] == "claim" {
                require(a["currency"] == "DKK", "claim receipt currency differs")?;
                let mut remaining = amount.clone();
                for (account, balance) in &mut balances {
                    if remaining == Money::default() {
                        break;
                    }
                    require(!balance.is_negative(), "negative claim receivable balance")?;
                    let part = if balance.subtract(&remaining).is_negative() {
                        balance.clone()
                    } else {
                        remaining.clone()
                    };
                    effect(&mut expected, *account, Money::default(), part.clone());
                    *balance = balance.subtract(&part);
                    remaining = remaining.subtract(&part);
                }
                require(
                    remaining == Money::default(),
                    "claim receipt exceeds previously posted account capacities",
                )?;
            } else {
                require(
                    a["currency"] == currency,
                    "principal movement currency differs from invoice",
                )?;
                let before = if refund { &refunded } else { &paid };
                let relief = if currency == "DKK" {
                    amount.clone()
                } else if j["created_by_program"] == "rentemester-rust"
                    || j["created_by_program"] == "service-principal"
                {
                    crate::invoice_fx::portion(&gross, &frozen, before, &amount)?
                } else {
                    require(
                        !refund,
                        "legacy foreign refunds lack a source-supported carrying contract",
                    )?;
                    legacy_foreign_relief(tx, id, debtor, jid, &gross, &amount)?
                };
                principal.add(&relief);
                effect(
                    &mut expected,
                    debtor,
                    if refund {
                        relief.clone()
                    } else {
                        Money::default()
                    },
                    if refund { Money::default() } else { relief },
                );
                if refund {
                    refunded.add(&amount)
                } else {
                    paid.add(&amount)
                }
            }
        }
        let signed = money(&b["amount"])?;
        let absolute = if signed.is_negative() {
            Money::default().subtract(&signed)
        } else {
            signed.clone()
        };
        require(
            signed.is_negative() == refund
                && absolute == total
                && b["currency"] == first["currency"]
                && j["currency"] == first["currency"]
                && (b["bank_currency"].is_null() || b["bank_currency"] == b["currency"]),
            "bank amount/direction/currency or account currency differs",
        )?;
        let bank_account = if let Some(no) = b["ledger_account_no"].as_str() {
            let mut accounts = rows(tx, "SELECT id FROM accounts WHERE account_no=?1", &[&no])?;
            require(accounts.len() == 1, "bank ledger mapping missing")?;
            number(&accounts.remove(0)["id"])?
        } else {
            require(
                b["bank_account_id"].is_null(),
                "concrete bank account has no ledger mapping",
            )?;
            let candidates = rows(
                tx,
                "SELECT a.id FROM account_role_mappings r JOIN accounts a ON a.account_no=r.account_no WHERE r.role='bank' AND r.confirmed_at<=?1 AND r.confirmed_at=(SELECT max(confirmed_at) FROM account_role_mappings WHERE role='bank' AND confirmed_at<=?1)",
                &[&text(&j["registration_datetime"])?],
            )?;
            let actual = effects(&lines(tx, jid)?)?;
            let signed_dkk = if first["currency"] == "DKK" {
                total.clone()
            } else {
                money(&b["amount_dkk"])?
            };
            let bank_basis = if signed_dkk.is_negative() {
                Money::default().subtract(&signed_dkk)
            } else {
                signed_dkk
            };
            let matching = candidates
                .iter()
                .filter_map(|c| c["id"].as_i64())
                .filter(|id| {
                    actual.get(id).is_some_and(|e| {
                        if refund {
                            e.1 == bank_basis
                        } else {
                            e.0 == bank_basis
                        }
                    })
                })
                .collect::<Vec<_>>();
            require(
                matching.len() == 1,
                "historical bank role mapping ambiguous or missing",
            )?;
            matching[0]
        };
        require(bank_account != debtor, "bank ledger is invoice receivable")?;
        let bank_dkk = if first["currency"] == "DKK" {
            total.clone()
        } else {
            let signed = money(&b["amount_dkk"])?;
            require(
                signed.is_negative() == refund,
                "foreign bank frozen amount direction differs",
            )?;
            let value = if refund {
                Money::default().subtract(&signed)
            } else {
                signed
            };
            let rate = rentemester_engine::fx::FxRate::parse(&b["fx_rate_to_dkk"].to_string())?;
            require(
                rate.apply(&total) == value
                    && money(&j["amount_foreign"])? == total
                    && money(&j["amount_dkk"])? == value
                    && j["fx_rate_to_dkk"] == b["fx_rate_to_dkk"],
                "foreign cash frozen FX basis differs",
            )?;
            value
        };
        effect(
            &mut expected,
            bank_account,
            if refund {
                Money::default()
            } else {
                bank_dkk.clone()
            },
            if refund {
                bank_dkk.clone()
            } else {
                Money::default()
            },
        );
        if first["currency"] != "DKK" {
            let difference = if refund {
                principal.subtract(&bank_dkk)
            } else {
                bank_dkk.subtract(&principal)
            };
            if difference != Money::default() {
                let loss = difference.is_negative();
                let role = if loss { "fx_loss" } else { "fx_gain" };
                let extension:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='native_fx_role_confirmations')",[],|r|r.get(0)).map_err(invalid)?;
                let candidate = if extension {
                    rows(
                        tx,
                        "SELECT a.id FROM native_fx_role_confirmations f JOIN accounts a ON a.id=f.account_id WHERE f.role=?1 AND strftime('%Y-%m-%dT%H:%M:%S',f.created_at)<=strftime('%Y-%m-%dT%H:%M:%S',?2) ORDER BY f.version DESC LIMIT 1",
                        &[&role, &text(&j["registration_datetime"])?],
                    )?
                } else {
                    vec![]
                };
                let fx = if let Some(row) = candidate.first() {
                    number(&row["id"])?
                } else {
                    let no = if loss { "3320" } else { "1020" };
                    let rows = rows(tx, "SELECT id FROM accounts WHERE account_no=?1", &[&no])?;
                    require(
                        rows.len() == 1 && j["created_by_program"] != "rentemester-rust",
                        "frozen historical FX account confirmation missing",
                    )?;
                    number(&rows[0]["id"])?
                };
                account(
                    tx,
                    fx,
                    if loss { "expense" } else { "income" },
                    if loss { "debit" } else { "credit" },
                )?;
                let absolute = if loss {
                    Money::default().subtract(&difference)
                } else {
                    difference
                };
                effect(
                    &mut expected,
                    fx,
                    if loss {
                        absolute.clone()
                    } else {
                        Money::default()
                    },
                    if loss { Money::default() } else { absolute },
                );
            }
        }
        require(
            equivalent(effects(&lines(tx, jid)?)?, expected),
            "cash bank/principal/claim/FX exact account effects differ",
        )?;
    }
    Ok(())
}
fn status(tx: &Transaction<'_>, d: &Value) -> Result<(), Error> {
    let id = number(&d["id"])?;
    let gross = money(&d["amount_inc_vat"])?;
    let mut open = gross.clone();
    let mut totals = BTreeMap::new();
    for (table, column, sign) in [
        ("invoice_payments", "amount", false),
        ("invoice_refunds", "amount", true),
        ("invoice_bad_debt_writeoffs", "gross_amount", false),
    ] {
        let mut total = Money::default();
        for r in rows(
            tx,
            &format!(
                "SELECT {column} amount FROM {table} WHERE invoice_document_id=?1 ORDER BY id"
            ),
            &[&id],
        )? {
            total.add(&positive(&r["amount"])?);
        }
        if sign {
            open.add(&total)
        } else {
            open = open.subtract(&total)
        }
        totals.insert(table, total);
    }
    let credit = crate::credit_note::credited(tx, id)?;
    open = open.subtract(&credit);
    let expected = if open.is_negative() {
        "overpaid"
    } else if open != Money::default() {
        "open"
    } else if totals["invoice_bad_debt_writeoffs"] != Money::default() {
        "written_off"
    } else if totals["invoice_refunds"] != Money::default() {
        "refunded"
    } else if credit == gross && totals["invoice_payments"] == Money::default() {
        "credited"
    } else {
        "paid"
    };
    let stored = d["status"].as_str().unwrap_or("");
    require(
        stored == "issued" || stored == expected || expected == "paid" && stored == "settled",
        "stored invoice status differs from exact principal counters",
    )
}
fn correction_capacity(
    tx: &Transaction<'_>,
    invoice: i64,
    journal: i64,
    account: i64,
    credit: &Money,
    debit: &Money,
) -> Result<(), Error> {
    let mut interest = Money::default();
    let mut other = Money::default();
    let mut paid = Money::default();
    let mut income = Money::default();
    let sql = "SELECT 'interest' kind,l.debit_amount,l.credit_amount FROM invoice_interest_postings p JOIN invoice_interest_claims c ON c.id=p.interest_claim_id JOIN journal_lines l ON l.journal_entry_id=p.journal_entry_id WHERE c.invoice_document_id=?1 AND p.journal_entry_id<?2 AND l.account_id=?3 UNION ALL SELECT 'other',l.debit_amount,l.credit_amount FROM invoice_compensation_postings p JOIN invoice_compensation_claims c ON c.id=p.compensation_claim_id JOIN journal_lines l ON l.journal_entry_id=p.journal_entry_id WHERE c.invoice_document_id=?1 AND p.journal_entry_id<?2 AND l.account_id=?3 UNION ALL SELECT 'other',l.debit_amount,l.credit_amount FROM invoice_reminder_postings p JOIN invoice_reminders c ON c.id=p.reminder_id JOIN journal_lines l ON l.journal_entry_id=p.journal_entry_id WHERE c.invoice_document_id=?1 AND p.journal_entry_id<?2 AND l.account_id=?3 UNION ALL SELECT 'correction',l.debit_amount,l.credit_amount FROM invoice_interest_corrections c JOIN journal_lines l ON l.journal_entry_id=c.journal_entry_id WHERE c.invoice_document_id=?1 AND c.journal_entry_id<?2 AND l.account_id=?3 UNION ALL SELECT 'payment',l.debit_amount,l.credit_amount FROM invoice_claim_payments c JOIN journal_lines l ON l.journal_entry_id=c.journal_entry_id WHERE c.invoice_document_id=?1 AND c.journal_entry_id<?2 AND l.account_id=?3";
    for row in rows(tx, sql, &[&invoice, &journal, &account])? {
        let dr = money(&row["debit_amount"])?;
        let cr = money(&row["credit_amount"])?;
        match row["kind"].as_str() {
            Some("interest") => {
                interest.add(&dr.subtract(&cr));
                income.add(&cr.subtract(&dr));
            }
            Some("other") => other.add(&dr.subtract(&cr)),
            Some("correction") => {
                interest.add(&dr.subtract(&cr));
                income.add(&cr.subtract(&dr));
            }
            Some("payment") => paid.add(&cr.subtract(&dr)),
            _ => return Err(refused("unknown correction capacity source")),
        }
    }
    if *credit != Money::default() {
        let mut remaining = interest.clone();
        remaining.add(&other);
        remaining = remaining.subtract(&paid).subtract(&other);
        require(
            !interest.subtract(credit).is_negative() && !remaining.subtract(credit).is_negative(),
            "interest correction exceeds certain uncollected interest receivable capacity",
        )?;
    }
    if *debit != Money::default() {
        require(
            !income.subtract(debit).is_negative(),
            "interest correction exceeds unreversed interest income capacity",
        )?;
    }
    Ok(())
}
fn correction(tx: &Transaction<'_>, c: &Value) -> Result<Effects, Error> {
    let effects = correction_structure(tx, c)?;
    crate::invoice_interest::history::verify_historical_plan(tx, c)?;
    Ok(effects)
}
fn correction_structure(tx: &Transaction<'_>, c: &Value) -> Result<Effects, Error> {
    let jid = number(&c["journal_entry_id"])?;
    let invoice = number(&c["invoice_document_id"])?;
    let amount = positive(&c["amount_dkk"])?;
    let j = journal(tx, jid, invoice, None, text(&c["correction_date"])?)?;
    require(
        j["currency"] == "DKK"
            && j["amount_foreign"].is_null()
            && j["amount_dkk"].is_null()
            && j["fx_rate_to_dkk"].is_null(),
        "interest correction foreign metadata differs",
    )?;
    let p = one(
        tx,
        "SELECT * FROM invoice_interest_correction_plans WHERE journal_entry_id=?1",
        jid,
    )?;
    require(
        p["invoice_document_id"] == invoice
            && p["correction_date"] == c["correction_date"]
            && money(&p["amount_dkk"])? == amount,
        "interest correction causal plan identity differs",
    )?;
    let mut sum = Money::default();
    let planned = rows(
        tx,
        "SELECT p.*,c.invoice_document_id,c.claim_date original_date FROM invoice_interest_correction_plan_claims p LEFT JOIN invoice_interest_claims c ON c.id=p.interest_claim_id WHERE p.journal_entry_id=?1 ORDER BY p.interest_claim_id",
        &[&jid],
    )?;
    require(
        !planned.is_empty(),
        "interest correction causal claims missing",
    )?;
    for r in planned {
        let portion = positive(&r["amount_dkk"])?;
        let ceiling = positive(&r["claim_ceiling_dkk"])?;
        require(
            r["invoice_document_id"] == invoice
                && r["claim_date"] == r["original_date"]
                && text(&c["correction_date"])? >= text(&r["claim_date"])?
                && !ceiling.subtract(&portion).is_negative(),
            "interest correction claim authority/ceiling differs",
        )?;
        let claim = number(&r["interest_claim_id"])?;
        let mut used = Money::default();
        for prior in rows(
            tx,
            "SELECT p.amount_dkk FROM invoice_interest_correction_plan_claims p JOIN invoice_interest_corrections c ON c.journal_entry_id=p.journal_entry_id WHERE p.interest_claim_id=?1 AND c.journal_entry_id<=?2",
            &[&claim, &jid],
        )? {
            used.add(&positive(&prior["amount_dkk"])?);
        }
        require(
            !ceiling.subtract(&used).is_negative(),
            "interest correction cumulative allocation exceeds claim ceiling",
        )?;
        sum.add(&portion);
    }
    require(
        sum == amount,
        "interest correction plan claim amounts do not reconcile",
    )?;
    let plan = rows(
        tx,
        "SELECT p.*,a.id account_id FROM invoice_interest_correction_plan_lines p LEFT JOIN accounts a ON a.account_no=p.account_no WHERE p.journal_entry_id=?1 ORDER BY p.account_no",
        &[&jid],
    )?;
    let expected = rows(
        tx,
        "SELECT p.*,a.id account_id FROM invoice_interest_correction_expected_plan_lines p LEFT JOIN accounts a ON a.account_no=p.account_no WHERE p.journal_entry_id=?1 ORDER BY p.account_no",
        &[&jid],
    )?;
    require(
        equivalent(effects(&plan)?, effects(&expected)?),
        "interest correction planned account allocation differs from causal claim effects",
    )?;
    let actual = lines(tx, jid)?;
    require(
        equivalent(effects(&actual)?, effects(&plan)?),
        "interest correction actual lines differ from exact plan",
    )?;
    let mut income = Money::default();
    let mut asset = Money::default();
    let mut claim_effect = Effects::new();
    for l in actual {
        require(
            l["vat_code"].is_null() && l["currency"] == "DKK",
            "interest correction VAT/line currency differs",
        )?;
        let dr = money(&l["debit_amount"])?;
        let cr = money(&l["credit_amount"])?;
        correction_capacity(tx, invoice, jid, number(&l["account_id"])?, &cr, &dr)?;
        if l["kind"] == "income" && l["normal_balance"] == "credit" && cr == Money::default() {
            income.add(&dr);
        } else if l["kind"] == "asset" && l["normal_balance"] == "debit" && dr == Money::default() {
            asset.add(&cr);
            effect(
                &mut claim_effect,
                number(&l["account_id"])?,
                Money::default(),
                cr,
            );
        } else {
            return Err(refused("interest correction counteraccount differs"));
        }
    }
    require(
        income == amount && asset == amount,
        "interest correction exact total differs",
    )?;
    Ok(claim_effect)
}
/// Primitive invoice/cash/claim structure only. Causal interest history is checked
/// separately, so loading principal events never recurses through claims/carrying.
pub(crate) fn interest_principal_evidence(tx: &Transaction<'_>, id: i64) -> Result<(), Error> {
    let d = one(
        tx,
        "SELECT * FROM documents WHERE id=?1 AND document_type='issued_invoice'",
        id,
    )?;
    let p = one(
        tx,
        "SELECT * FROM issued_invoice_postings WHERE invoice_document_id=?1",
        id,
    )?;
    posted_invoice(tx, &d, &p)?;
    let mut errors = vec![];
    let mut events = claims(tx, id, &mut errors)?;
    if !errors.is_empty() {
        return Err(refused(json!(errors).to_string()));
    }
    for c in rows(
        tx,
        "SELECT * FROM invoice_interest_corrections WHERE invoice_document_id=?1 ORDER BY journal_entry_id",
        &[&id],
    )? {
        require(
            events
                .insert(
                    number(&c["journal_entry_id"])?,
                    correction_structure(tx, &c)?,
                )
                .is_none(),
            "reused correction journal",
        )?;
    }
    cash(tx, &d, &p, &events)?;
    crate::credit_note::verify_existing(tx, id)?;
    for w in rows(
        tx,
        "SELECT * FROM invoice_bad_debt_writeoffs WHERE invoice_document_id=?1",
        &[&id],
    )? {
        bad_debt(tx, &w)?;
    }
    Ok(())
}
pub(crate) fn interest_correction_effects(
    tx: &Transaction<'_>,
    id: i64,
) -> Result<BTreeMap<String, Money>, Error> {
    let c = one(
        tx,
        "SELECT * FROM invoice_interest_corrections WHERE id=?1",
        id,
    )?;
    let effects = correction(tx, &c)?;
    let mut out = BTreeMap::new();
    for (account, (debit, credit)) in effects {
        let no: String = tx
            .query_row(
                "SELECT account_no FROM accounts WHERE id=?1",
                [account],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        out.insert(no, credit.subtract(&debit));
    }
    Ok(out)
}
/// Exact historical grouped principal/claim applications reuse audit119 cash proof.
pub(crate) fn interest_cash_evidence(tx: &Transaction<'_>, id: i64) -> Result<(), Error> {
    interest_principal_evidence(tx, id)
}
/// Every persisted family is scanned; a failed record does not suppress later findings.
pub fn verify(tx: &Transaction<'_>) -> Result<Value, Error> {
    let mut errors = vec![];
    let mut checked = BTreeMap::new();
    for table in [
        "issued_invoice_postings",
        "credit_note_postings",
        "invoice_payments",
        "invoice_refunds",
        "invoice_claim_payments",
        "invoice_reminders",
        "invoice_compensation_claims",
        "invoice_compensation_postings",
        "invoice_interest_claims",
        "invoice_interest_postings",
        "invoice_interest_corrections",
        "invoice_bad_debt_writeoffs",
        "imported_receivable_headers",
        "imported_receivable_events",
    ] {
        let count: i64 = tx
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .map_err(invalid)?;
        checked.insert(table, count);
    }
    let documents = rows(
        tx,
        "SELECT * FROM documents WHERE document_type='issued_invoice' ORDER BY id",
        &[],
    )?;
    checked.insert("invoices", documents.len() as i64);
    for d in documents {
        let id = number(&d["id"])?;
        let snapshot = (|| {
            let payload: Value =
                serde_json::from_str(text(&d["payload_json"])?).map_err(invalid)?;
            Date::parse(text(&d["invoice_date"])?)?;
            require(
                payload["invoiceNumber"] == d["invoice_no"]
                    && payload["issueDate"] == d["invoice_date"]
                    && payload["currency"] == d["currency"],
                "invoice snapshot identity differs",
            )?;
            require(
                rentemester_engine::sales_invoice::project(&payload)?.gross
                    == positive(&d["amount_inc_vat"])?,
                "invoice frozen original total differs",
            )?;
            crate::invoice_fx::projection(&payload)?;
            if let Some(due) = payload["dueDate"].as_str() {
                Date::parse(due)?;
                require(
                    due >= text(&d["invoice_date"])?,
                    "invoice due date predates issuance",
                )?;
            }
            if payload["vatTreatment"] == "foreign_reverse_charge" {
                rentemester_engine::vies::require_record(
                    &payload["viesValidation"],
                    payload["buyer"]["vatOrCvr"].as_str().unwrap_or(""),
                    payload["issuedAt"].as_str().unwrap_or(""),
                )?;
            }
            Ok(())
        })();
        record(&mut errors, "invoice_snapshot", id, snapshot);
        let postings = rows(
            tx,
            "SELECT * FROM issued_invoice_postings WHERE invoice_document_id=?1 ORDER BY journal_entry_id",
            &[&id],
        )?;
        if postings.is_empty() {
            let linked = rows(
                tx,
                "SELECT id FROM journal_entries WHERE document_id=?1 AND status='posted' AND reversal_of_entry_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=journal_entries.id)",
                &[&id],
            )?;
            record(
                &mut errors,
                "invoice_posting",
                id,
                require(
                    linked.is_empty(),
                    "issued invoice active journal lacks explicit posting link",
                ),
            );
        }
        for p in &postings {
            record(
                &mut errors,
                "invoice_posting",
                id,
                posted_invoice(tx, &d, p),
            );
        }
        let mut claim_events = match claims(tx, id, &mut errors) {
            Ok(e) => e,
            Err(e) => {
                record(&mut errors, "claims", id, Err(e));
                BTreeMap::new()
            }
        };
        for c in rows(
            tx,
            "SELECT * FROM invoice_interest_corrections WHERE invoice_document_id=?1 ORDER BY id",
            &[&id],
        )? {
            let cid = number(&c["id"])?;
            match correction(tx, &c) {
                Ok(e) => {
                    if claim_events
                        .insert(number(&c["journal_entry_id"])?, e)
                        .is_some()
                    {
                        record(
                            &mut errors,
                            "interest_correction",
                            cid,
                            Err(refused("correction journal reused")),
                        );
                    }
                }
                Err(e) => record(&mut errors, "interest_correction", cid, Err(e)),
            }
        }
        for p in &postings {
            record(
                &mut errors,
                "cash_applications",
                id,
                cash(tx, &d, p, &claim_events),
            );
        }
        record(&mut errors, "invoice_status", id, status(tx, &d));
        let credits:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM credit_note_postings WHERE original_invoice_document_id=?1)",[id],|r|r.get(0)).map_err(invalid)?;
        if credits || !postings.is_empty() {
            record(
                &mut errors,
                "credit_counters",
                id,
                crate::credit_note::verify_existing(tx, id),
            );
        }
        // Every active journal attached to an invoice must have a unique domain owner.
        for j in rows(
            tx,
            "SELECT j.id,(SELECT count(*) FROM issued_invoice_postings x WHERE x.journal_entry_id=j.id)+(SELECT count(*) FROM invoice_payments x WHERE x.journal_entry_id=j.id)+(SELECT count(*) FROM invoice_refunds x WHERE x.journal_entry_id=j.id)+(SELECT count(*) FROM invoice_claim_payments x WHERE x.journal_entry_id=j.id)+(SELECT count(*) FROM invoice_bad_debt_writeoffs x WHERE x.journal_entry_id=j.id)+(SELECT count(*) FROM invoice_interest_corrections x WHERE x.journal_entry_id=j.id)+(SELECT count(*) FROM invoice_reminder_postings x WHERE x.journal_entry_id=j.id)+(SELECT count(*) FROM invoice_compensation_postings x WHERE x.journal_entry_id=j.id)+(SELECT count(*) FROM invoice_interest_postings x WHERE x.journal_entry_id=j.id) owners FROM journal_entries j WHERE j.document_id=?1 AND j.status='posted' AND j.reversal_of_entry_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) ORDER BY j.id",
            &[&id],
        )? {
            let owners = number(&j["owners"])?;
            record(&mut errors,"journal_classification",number(&j["id"] )?,require(owners==1||owners==2&&tx.query_row("SELECT EXISTS(SELECT 1 FROM invoice_payments p JOIN invoice_claim_payments c ON c.journal_entry_id=p.journal_entry_id AND c.invoice_document_id=p.invoice_document_id WHERE p.journal_entry_id=?1)",[number(&j["id"])?],|r|r.get::<_,bool>(0)).map_err(invalid)?,"unclassified or multiply owned invoice journal"));
        }
    }
    for row in rows(
        tx,
        "SELECT * FROM invoice_bad_debt_writeoffs ORDER BY id",
        &[],
    )? {
        record(
            &mut errors,
            "bad_debt",
            number(&row["id"])?,
            bad_debt(tx, &row),
        );
    }
    for row in rows(
        tx,
        "SELECT * FROM imported_receivable_headers ORDER BY id",
        &[],
    )? {
        record(
            &mut errors,
            "imported_receivable",
            number(&row["id"])?,
            imported(tx, &row),
        );
    }
    // Global orphan/cross-invoice checks cannot rely on INNER JOIN fact reconstructors.
    for (family, sql) in [
        (
            "credit_link",
            "SELECT c.id FROM documents c LEFT JOIN credit_note_postings p ON p.credit_note_document_id=c.id LEFT JOIN documents d ON d.id=p.original_invoice_document_id WHERE c.document_type='credit_note' AND (p.credit_note_document_id IS NULL OR d.document_type<>'issued_invoice' OR d.invoice_no<>c.payment_details OR EXISTS(SELECT 1 FROM journal_entries j WHERE j.document_id=c.id AND j.id<>p.journal_entry_id)) ORDER BY c.id",
        ),
        (
            "interest_plan",
            "SELECT p.journal_entry_id id FROM invoice_interest_correction_plans p LEFT JOIN invoice_interest_corrections c ON c.journal_entry_id=p.journal_entry_id WHERE c.id IS NULL ORDER BY p.journal_entry_id",
        ),
        (
            "cash_journal_reuse",
            "SELECT journal_entry_id id FROM (SELECT invoice_document_id,journal_entry_id FROM invoice_payments UNION ALL SELECT invoice_document_id,journal_entry_id FROM invoice_refunds UNION ALL SELECT invoice_document_id,journal_entry_id FROM invoice_claim_payments) GROUP BY journal_entry_id HAVING count(DISTINCT invoice_document_id)>1 ORDER BY journal_entry_id",
        ),
        (
            "imported_event",
            "SELECT e.id FROM imported_receivable_events e LEFT JOIN imported_receivable_headers h ON h.id=e.receivable_id WHERE h.id IS NULL ORDER BY e.id",
        ),
    ] {
        for row in rows(tx, sql, &[])? {
            record(
                &mut errors,
                family,
                row["id"].as_i64().unwrap_or(0),
                Err(refused("orphan/noncanonical/cross-invoice relationship")),
            );
        }
    }
    for (table, column) in [
        ("issued_invoice_postings", "invoice_document_id"),
        ("invoice_payments", "invoice_document_id"),
        ("invoice_refunds", "invoice_document_id"),
        ("invoice_claim_payments", "invoice_document_id"),
        ("invoice_reminders", "invoice_document_id"),
        ("invoice_compensation_claims", "invoice_document_id"),
        ("invoice_interest_claims", "invoice_document_id"),
        ("invoice_interest_corrections", "invoice_document_id"),
        ("invoice_bad_debt_writeoffs", "invoice_document_id"),
    ] {
        let sql = format!(
            "SELECT x.rowid id FROM {table} x LEFT JOIN documents d ON d.id=x.{column} WHERE d.id IS NULL OR d.document_type<>'issued_invoice' ORDER BY x.rowid"
        );
        for row in rows(tx, &sql, &[])? {
            record(
                &mut errors,
                table,
                number(&row["id"])?,
                Err(refused("application does not identify an issued invoice")),
            );
        }
    }
    for row in rows(
        tx,
        "SELECT min(id) id FROM imported_receivable_headers GROUP BY dinero_import_attempt_id HAVING count(DISTINCT schedule_hash)>1 ORDER BY min(id)",
        &[],
    )? {
        record(
            &mut errors,
            "imported_schedule",
            number(&row["id"])?,
            Err(refused(
                "one accepted import attempt contains conflicting source schedule identities",
            )),
        );
    }
    for row in rows(
        tx,
        "SELECT x.invoice_document_id id FROM (SELECT invoice_document_id FROM invoice_payments UNION ALL SELECT invoice_document_id FROM invoice_refunds UNION ALL SELECT invoice_document_id FROM invoice_claim_payments UNION ALL SELECT original_invoice_document_id FROM credit_note_postings) x LEFT JOIN issued_invoice_postings p ON p.invoice_document_id=x.invoice_document_id WHERE p.invoice_document_id IS NULL ORDER BY x.invoice_document_id",
        &[],
    )? {
        record(
            &mut errors,
            "missing_original_posting",
            number(&row["id"])?,
            Err(refused(
                "invoice application lacks canonical original posting",
            )),
        );
    }
    for row in rows(
        tx,
        "SELECT p.credit_note_document_id id FROM credit_note_postings p LEFT JOIN documents c ON c.id=p.credit_note_document_id LEFT JOIN documents d ON d.id=p.original_invoice_document_id LEFT JOIN journal_entries j ON j.id=p.journal_entry_id WHERE c.document_type IS NOT 'credit_note' OR d.document_type IS NOT 'issued_invoice' OR j.id IS NULL OR j.document_id IS NOT c.id ORDER BY p.credit_note_document_id",
        &[],
    )? {
        record(
            &mut errors,
            "credit_posting",
            number(&row["id"])?,
            Err(refused(
                "credit posting source/journal/original identity missing",
            )),
        );
    }
    for (posting, claim, key) in [
        (
            "invoice_reminder_postings",
            "invoice_reminders",
            "reminder_id",
        ),
        (
            "invoice_compensation_postings",
            "invoice_compensation_claims",
            "compensation_claim_id",
        ),
        (
            "invoice_interest_postings",
            "invoice_interest_claims",
            "interest_claim_id",
        ),
    ] {
        let sql = format!(
            "SELECT p.id FROM {posting} p LEFT JOIN {claim} c ON c.id=p.{key} WHERE c.id IS NULL ORDER BY p.id"
        );
        for row in rows(tx, &sql, &[])? {
            record(
                &mut errors,
                posting,
                number(&row["id"])?,
                Err(refused("orphan claim posting")),
            );
        }
    }
    errors.sort_by_key(Value::to_string);
    Ok(json!({"available":true,"ok":errors.is_empty(),"checked":checked,"errors":errors}))
}
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, params};
    fn fixture() -> Connection {
        let db = crate::credit_note::tests::fixture();
        let names = db
            .prepare("SELECT name FROM sqlite_master WHERE type='trigger'")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for name in names {
            db.execute_batch(&format!("DROP TRIGGER \"{}\"", name.replace('"', "\"\"")))
                .unwrap();
        }
        db
    }
    fn check(db: &mut Connection) -> Value {
        let tx = db.transaction().unwrap();
        verify(&tx).unwrap()
    }
    fn append(db: &Connection, jid: i64, date: &str, amount: &str, asset: i64, income: i64) {
        db.execute("INSERT INTO journal_entries(id,entry_no,transaction_date,text,document_id,rule_version,entry_hash)VALUES(?1,?2,?3,'Synthetic claim',1,'synthetic',?4)",params![jid,format!("2026-{jid:05}"),date,"a".repeat(64)]).unwrap();
        db.execute("INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount)VALUES(?1,?2,?3,0),(?1,?4,0,?3)",params![jid,asset,amount,income]).unwrap();
    }
    #[test]
    fn actual_native_invoice_writer_is_positive_and_counter_account_tamper_is_found() {
        let mut db = fixture();
        let before = check(&mut db);
        assert_eq!(before["ok"], true, "{before}");
        db.execute("UPDATE journal_lines SET account_id=(SELECT id FROM accounts WHERE account_no='B') WHERE journal_entry_id=1 AND account_id=(SELECT id FROM accounts WHERE account_no='S')",[]).unwrap();
        let after = check(&mut db);
        assert_eq!(after["ok"], false);
        assert!(
            after["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["family"] == "invoice_posting")
        );
    }
    #[test]
    fn populated_compensation_interest_and_reminder_postings_are_validated_not_blocked() {
        let mut db = fixture();
        append(&db, 2, "2026-11-20", "20", 1, 2);
        append(&db, 3, "2026-11-21", "310", 1, 2);
        append(&db, 4, "2026-11-22", "1.88", 1, 2);
        db.execute_batch("INSERT INTO invoice_reminders(id,invoice_document_id,reminder_date,fee_amount)VALUES(1,1,'2026-11-20',20);INSERT INTO invoice_reminder_postings(reminder_id,journal_entry_id)VALUES(1,2);INSERT INTO invoice_compensation_claims(id,invoice_document_id,claim_date,amount_dkk)VALUES(1,1,'2026-11-21',310);INSERT INTO invoice_compensation_postings(compensation_claim_id,journal_entry_id)VALUES(1,3);INSERT INTO invoice_interest_claims(id,invoice_document_id,claim_date,reference_rate_percent,annual_interest_rate_percent,reference_rate_source,overdue_days,principal_open_balance,amount_dkk)VALUES(1,1,'2026-11-22',3,11,'manual-override',5,1250,1.88);INSERT INTO invoice_interest_postings(interest_claim_id,journal_entry_id)VALUES(1,4);").unwrap();
        let good = check(&mut db);
        assert_eq!(good["ok"], true, "{good}");
        for (table, column, id) in [
            ("invoice_reminders", "fee_amount", 1),
            ("invoice_compensation_claims", "amount_dkk", 1),
            ("invoice_interest_claims", "amount_dkk", 1),
        ] {
            let tx = db.transaction().unwrap();
            tx.execute(
                &format!("UPDATE {table} SET {column}={column}+1 WHERE id=?1"),
                [id],
            )
            .unwrap();
            let report = verify(&tx).unwrap();
            assert_eq!(report["ok"], false, "{table}: {report}");
            tx.rollback().unwrap();
        }
    }
    #[test]
    fn populated_standard_vat_bad_debt_has_exact_split_and_rejects_wrong_expense() {
        let mut db = fixture();
        db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('E','Synthetic loss','expense','debit');INSERT INTO journal_entries(id,entry_no,transaction_date,text,document_id,rule_version,entry_hash)VALUES(2,'2026-00002','2026-10-19','Synthetic established loss',1,'synthetic','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount,vat_code)VALUES(2,(SELECT id FROM accounts WHERE account_no='E'),1000,0,'DK_BAD_DEBT_25'),(2,3,250,0,NULL),(2,1,0,1250,NULL);INSERT INTO invoice_bad_debt_writeoffs(invoice_document_id,writeoff_date,gross_amount,net_amount,vat_amount,journal_entry_id)VALUES(1,'2026-10-19',1250,1000,250,2);").unwrap();
        let good = check(&mut db);
        assert_eq!(good["ok"], true, "{good}");
        db.execute(
            "UPDATE journal_lines SET account_id=2 WHERE journal_entry_id=2 AND debit_amount=1000",
            [],
        )
        .unwrap();
        let bad = check(&mut db);
        assert_eq!(bad["ok"], false);
        assert!(
            bad["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["family"] == "bad_debt")
        );
    }
    #[test]
    fn imported_source_schedule_supports_paid_and_partial_exact_events_and_rejects_mismatch() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE dinero_import_attempts(id INTEGER PRIMARY KEY);INSERT INTO dinero_import_attempts VALUES(1);CREATE TABLE accounts(account_no TEXT);INSERT INTO accounts VALUES('AR');CREATE TABLE imported_receivable_headers(id INTEGER,dinero_import_attempt_id INTEGER,external_invoice_id TEXT,source_document_hash TEXT,invoice_date TEXT,due_date TEXT,gross_amount NUMERIC,control_account_no TEXT,source_recognition_ref TEXT,schedule_hash TEXT);CREATE TABLE imported_receivable_events(id INTEGER,receivable_id INTEGER,external_event_id TEXT,event_kind TEXT,effective_date TEXT,amount NUMERIC,source_event_ref TEXT,source_document_hash TEXT,schedule_hash TEXT);").unwrap();
        let hash = "a".repeat(64);
        db.execute("INSERT INTO imported_receivable_headers VALUES(1,1,'source-1',?1,'2025-01-01','2025-02-01',100.03,'AR','recognition-1',?1)",[&hash]).unwrap();
        db.execute("INSERT INTO imported_receivable_events VALUES(1,1,'payment-1','payment','2025-01-02',40.01,'source-receipt-1',?1,?1),(2,1,'credit-1','credit_note','2025-01-03',60.02,'source-credit-1',?1,?1)",[&hash]).unwrap();
        let tx = db.transaction().unwrap();
        let h = one(
            &tx,
            "SELECT * FROM imported_receivable_headers WHERE id=?1",
            1,
        )
        .unwrap();
        assert!(imported(&tx, &h).is_ok());
        tx.execute(
            "UPDATE imported_receivable_events SET schedule_hash=?1 WHERE id=2",
            ["b".repeat(64)],
        )
        .unwrap();
        assert!(imported(&tx, &h).is_err());
        tx.execute(
            "UPDATE imported_receivable_events SET schedule_hash=?1,amount=60.03 WHERE id=2",
            [&hash],
        )
        .unwrap();
        assert!(imported(&tx, &h).is_err());
    }
    #[test]
    fn cash_movement_matches_actual_bank_and_original_frozen_debtor() {
        let mut db = fixture();
        append(&db, 2, "2026-10-19", "50", 4, 1);
        db.execute_batch("INSERT INTO bank_transactions(id,transaction_date,text,amount,currency,bank_account_id) VALUES(1,'2026-10-19','Synthetic receipt',50,'DKK',1);UPDATE journal_entries SET source_bank_transaction_id=1 WHERE id=2;INSERT INTO invoice_payments(invoice_document_id,bank_transaction_id,journal_entry_id,payment_date,amount,currency)VALUES(1,1,2,'2026-10-19',50,'DKK');").unwrap();
        let good = check(&mut db);
        assert_eq!(good["ok"], true, "{good}");
        db.execute("UPDATE invoice_payments SET amount=49 WHERE id=1", [])
            .unwrap();
        let bad = check(&mut db);
        assert_eq!(bad["ok"], false);
        assert!(
            bad["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["family"] == "cash_applications")
        );
    }
    #[test]
    fn native_credit_writer_with_actual_snapshot_hash_and_counter_tamper() {
        let mut db = crate::credit_note::tests::fixture();
        {
            let tx = db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            let input:crate::credit_note::Input=serde_json::from_value(json!({"invoiceDocumentId":1,"issueDate":"2026-10-18","reason":"Synthetic credit","grossAmount":125,"confirm":true})).unwrap();
            let prepared =
                crate::credit_note::prepare(&tx, &input, "2026-10-18T12:00:00Z").unwrap();
            let raw = serde_json::to_string_pretty(&prepared.payload).unwrap();
            let hash = crate::invoice_email::sha(raw.as_bytes());
            crate::credit_note::register_and_post(
                &tx,
                prepared,
                crate::sales::Artifact {
                    filename: "synthetic-credit.json",
                    path: "synthetic-credit.json",
                    hash: &hash,
                    mime: "application/json",
                },
                "synthetic",
                Date::parse("2026-10-18").unwrap(),
            )
            .unwrap();
            tx.commit().unwrap();
        }
        let good = check(&mut db);
        assert_eq!(good["ok"], true, "{good}");
        let names = db
            .prepare("SELECT name FROM sqlite_master WHERE type='trigger'")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for name in names {
            db.execute_batch(&format!("DROP TRIGGER \"{}\"", name.replace('"', "\"\"")))
                .unwrap();
        }
        db.execute(
            "UPDATE credit_note_postings SET booked_gross_dkk=124 WHERE credit_note_document_id>1",
            [],
        )
        .unwrap();
        let bad = check(&mut db);
        assert_eq!(bad["ok"], false);
        assert!(
            bad["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["family"] == "credit_counters")
        );
    }
    #[test]
    fn unavailable_schema_is_never_empty_green() {
        let mut db = Connection::open_in_memory().unwrap();
        let tx = db.transaction().unwrap();
        assert!(verify(&tx).is_err());
    }
}
