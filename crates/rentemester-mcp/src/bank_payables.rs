//! Discoverable tools over the native bank/payable routes; no matching heuristics.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, BankMutation, Error, bank_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "bank"
            | "bank_accounts"
            | "payables"
            | "expense_booking_options"
            | "create_bank_account"
            | "import_bank"
            | "register_payable"
            | "pay_payable"
            | "match_bank_expense"
    )
}
pub fn tools() -> Vec<Value> {
    let company = company_schema();
    let mut tools = vec![];
    for (name, description) in [
        (
            "bank",
            "Read imported statement rows, transaction ids/date/amount, matched status, journal references and ledger/statement reconciliation; optional year",
        ),
        (
            "bank_accounts",
            "Discover active bank account ids/slugs, currencies and ledger mappings; named import profiles currently unavailable",
        ),
        (
            "payables",
            "Read supplier bills, original document ids, paid/open balances, candidate unregistered originals, expense accounts and vendor data; defaults status=open and current server date",
        ),
        (
            "expense_booking_options",
            "Read one original document and allowed expense/VAT/bank transaction options before explicit direct purchase matching",
        ),
    ] {
        let mut schema = json!({"type":"object","required":["company"],"properties":{"company":company},"additionalProperties":false});
        match name {
            "bank" => {
                schema["properties"]["year"] = json!({"type":"string","maxLength":128,"description":"Server fiscal year label"})
            }
            "payables" => {
                schema["properties"]["status"] =
                    json!({"type":"string","enum":["open","paid","overdue","all"]});
                schema["properties"]["asOf"] = json!({"type":"string","maxLength":32,"pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$","description":"Explicit as-of date; earlier dates omit later bills/payments"});
            }
            "expense_booking_options" => {
                schema["required"].as_array_mut().unwrap().push(json!("id"));
                schema["properties"]["id"] = json!({"type":"integer","minimum":1,"description":"Original document id from documents/payables"});
            }
            _ => {}
        }
        tools.push(tool(name, description, schema, true));
    }
    for (name, operation, description) in [
        (
            "create_bank_account",
            BankMutation::CreateAccount,
            "Company-admin operation: create bank master data with explicit compatible ledger account mapping; does not invent a balance",
        ),
        (
            "import_bank",
            BankMutation::ImportStatement,
            "Ledger-post operation: import generic UTF-8 CSV into an existing bank account; preserve source and row identity, report known duplicates; no named profiles",
        ),
        (
            "register_payable",
            BankMutation::RegisterPayable,
            "Ledger-post operation: recognize an unpaid DKK supplier original once, using matching bill date, explicit expense account and current confirmed creditor/VAT roles; amounts derive from original, no payment yet",
        ),
        (
            "pay_payable",
            BankMutation::PayPayable(1),
            "Ledger-post operation: settle one supplier bill against full outgoing DKK bank row; optional amount/date must exactly match row, explicit account must match bank mapping; no overpayment or reused row",
        ),
        (
            "match_bank_expense",
            BankMutation::MatchExpense,
            "Ledger-post operation: book a direct purchase from immutable document and selected bank row via documents/book-expense. Read expense_booking_options first. Posts balanced expense/VAT/payment lines and reconciliation together. Not an arbitrary existing-journal match",
        ),
    ] {
        let mut schema = json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company,"body":native_body(bank_body_schema(operation)),"confirm":{"const":true}},"additionalProperties":false});
        if matches!(operation, BankMutation::MatchExpense) {
            schema["properties"]["body"]["maxLength"] = json!(16384);
        }
        if matches!(operation, BankMutation::PayPayable(_)) {
            schema["required"].as_array_mut().unwrap().push(json!("id"));
            schema["properties"]["id"] = json!({"type":"integer","minimum":1,"description":"Supplier bill payableId from payables"});
        }
        tools.push(tool(name,&format!("{description}. Exact native body bytes; confirm:true is also required inside body except bank-account creation. No generic idempotency key, automatic retry or local fallback. Reconcile outcome_unknown using bank/payables/journal/documents before further action. Client body limit262144 bytes (direct expense match16384)."),schema,false));
    }
    tools
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Company {
    company: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BankRead {
    company: String,
    year: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PayablesRead {
    company: String,
    status: Option<String>,
    #[serde(rename = "asOf")]
    as_of: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OptionsRead {
    company: String,
    id: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    company: String,
    body: String,
    confirm: bool,
    id: Option<i64>,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    for key in ["year", "status", "asOf"] {
        if args.get(key).is_some_and(|v| !v.is_string()) {
            return Err("bank/payable read filters must be strings");
        }
    }
    Ok(match name {
        "bank_accounts" => {
            let a: Company =
                serde_json::from_value(args).map_err(|_| "bank_accounts requires only company")?;
            api.bank_accounts(&a.company).await
        }
        "bank" => {
            let a: BankRead =
                serde_json::from_value(args).map_err(|_| "invalid bank read arguments")?;
            api.bank(&a.company, a.year.as_deref()).await
        }
        "payables" => {
            let a: PayablesRead =
                serde_json::from_value(args).map_err(|_| "invalid payables read arguments")?;
            api.payables(&a.company, a.status.as_deref(), a.as_of.as_deref())
                .await
        }
        "expense_booking_options" => {
            let a: OptionsRead = serde_json::from_value(args)
                .map_err(|_| "booking options require company and positive document id")?;
            if a.id <= 0 {
                return Err("document id must be positive");
            }
            api.expense_booking_options(&a.company, a.id).await
        }
        _ => {
            if args.get("id").is_some() != (name == "pay_payable")
                || args
                    .get("id")
                    .is_some_and(|v| v.as_i64().is_none_or(|n| n <= 0))
            {
                return Err("only pay_payable requires a positive payable id");
            }
            let a: Write = serde_json::from_value(args)
                .map_err(|_| "bank/payable write requires company, native body and confirm:true")?;
            if !a.confirm {
                return Err("explicit confirmation required");
            }
            let operation = match name {
                "create_bank_account" => BankMutation::CreateAccount,
                "import_bank" => BankMutation::ImportStatement,
                "register_payable" => BankMutation::RegisterPayable,
                "pay_payable" => BankMutation::PayPayable(a.id.unwrap()),
                "match_bank_expense" => BankMutation::MatchExpense,
                _ => return Err("unknown bank/payable tool"),
            };
            api.bank_mutation(&a.company, operation, a.body.into_bytes(), a.confirm)
                .await
        }
    })
}
