//! Canonical, read-only finance decisions. Descriptors are advice, never posting authority.
//! Replan inside the current admitted transaction immediately before executing a candidate.
use rentemester_engine::{Error, agent_run::*, fiscal_year::Date, money::Money};
use rusqlite::Transaction;
use serde_json::{Value, json};

pub enum ExpenseStep {
    Route(Proposal),
    Book {
        candidate: crate::expense_original::Input,
        summary: BookedExpense,
    },
}
pub enum PayableStep {
    Route(Proposal),
    Pay {
        candidate: crate::payable_original::Pay,
        summary: PayableMatch,
    },
}
fn invalid() -> Error {
    Error::new(
        "agent_policy_invalid",
        "Trusted company supplier policy is invalid",
    )
}
pub fn validate_policy(policy: &Policy) -> Result<(), Error> {
    for r in &policy.supplier_rules {
        if [&r.token, &r.expense_account, &r.label]
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control))
            || !matches!(
                r.vat_treatment.as_str(),
                "standard"
                    | "exempt"
                    | "non_deductible"
                    | "representation"
                    | "reverse_charge"
                    | "eu_goods_acquisition"
            )
        {
            return Err(invalid());
        }
    }
    Ok(())
}
fn amount(v: &Value) -> Result<Money, Error> {
    Money::parse(&v.to_string())
}
fn number(v: &Value) -> Result<serde_json::Number, Error> {
    v.as_number().cloned().ok_or_else(invalid)
}
fn id(v: &Value) -> Result<i64, Error> {
    v.as_i64().filter(|n| *n > 0).ok_or_else(invalid)
}
fn string(v: &Value) -> String {
    v.as_str().unwrap_or("ukendt").to_owned()
}
fn proposal(kind: &str, bank: i64, document: i64, evidence: Value) -> Proposal {
    let supplier = evidence["supplierName"].as_str().unwrap_or("ukendt");
    let (message, action) = match kind {
        "AGENT_LOW_CONFIDENCE_MATCH" => (format!("Banktransaktion {bank} matcher bilag {document} med en sikkerhed på {:.2} — under grænsen på {AUTO_BOOK_CONFIDENCE} for automatisk bogføring, så agenten bogfører den ikke selv.", evidence["confidence"].as_f64().unwrap_or(0.0)), "Gennemgå det foreslåede match, og bogfør det derefter manuelt med 'expense book' — eller udlign det.".to_owned()),
        "AGENT_NO_ACCOUNT_RULE" => (format!("Banktransaktion {bank} matcher sikkert bilag {document} (leverandør '{supplier}'), men der findes ingen entydig kontoregel for leverandøren — agenten gætter ikke på en konto."), "Opret en kontoregel for leverandøren, eller bogfør udgiften manuelt med 'expense book'.".to_owned()),
        "AGENT_POSSIBLE_FIXED_ASSET" => {
            let threshold = &evidence["straksafskrivningThresholdDkk"];
            let action = if evidence["aboveSmallAssetThreshold"] == true {
                format!("Beløbet er over småaktiv-grænsen på {threshold} kr. og skal som udgangspunkt aktiveres og afskrives. Opret aktivet med 'asset register' og afskriv det med 'asset depreciate', eller bogfør udgiften manuelt hvis den ikke er et anlægsaktiv.")
            } else {
                format!("Beløbet er under småaktiv-grænsen på {threshold} kr. Vurder om det skal straksafskrives ('asset write-off'), aktiveres og afskrives ('asset register'), eller bogføres som en almindelig driftsudgift — og bogfør det derefter.")
            };
            (format!("Banktransaktion {bank} på {} kr. matcher bilag {document} (leverandør '{supplier}') i kategorien '{}'. Købet ligner et anlægsaktiv (driftsmiddel) og er ikke automatisk bogført som en driftsudgift — om det skal aktiveres og afskrives eller straksafskrives er en aktiv-/skattevurdering, som agenten ikke gætter på.", evidence["grossAmountDkk"], evidence["category"].as_str().unwrap_or("")), action)
        },
        "AGENT_PAYABLE_MATCH_UNCERTAIN" => (format!("Banktransaktion {bank} på {} kr. har samme beløb som kreditorpost #{} til {supplier}, men banklinjens tekst nævner hverken leverandøren eller bilagsnummeret — agenten afregner den ikke automatisk, da et beløbssammenfald alene ikke er sikkert nok.", evidence["openBalance"], evidence["payableId"]), format!("Kontroller om banktransaktionen rent faktisk er betalingen af kreditorpost #{}, og afregn den i så fald manuelt med 'payable pay'.", evidence["payableId"])),
        _ => (format!("Banktransaktion {bank}, bilag {document}: {kind}; ikke automatisk bogført."), "Gennemgå bilag, bankmatch og kontoregel før manuel bogføring.".to_owned()),
    };
    Proposal {
        kind: kind.into(),
        severity: if kind == "AGENT_POSSIBLE_FIXED_ASSET" {
            "high"
        } else {
            "medium"
        }
        .into(),
        message,
        required_action: Some(action),
        related_bank_transaction_id: Some(bank),
        related_document_id: Some(document),
        source_evidence: evidence,
        posting_preview: None,
    }
}
fn best_purchase(suggestions: &[Value]) -> Option<&Value> {
    suggestions
        .iter()
        .filter(|s| s["kind"] == "purchase_sale")
        .min_by(|a, b| {
            b["confidence"]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&a["confidence"].as_f64().unwrap_or(0.0))
                .then_with(|| a["documentId"].as_i64().cmp(&b["documentId"].as_i64()))
        })
}
/// Initial canonical matcher snapshot: five suggestions per bank row, bank ID ascending.
pub fn plan_expenses(
    tx: &Transaction<'_>,
    policy: &Policy,
    as_of: &str,
) -> Result<Vec<ExpenseStep>, Error> {
    validate_policy(policy)?;
    Date::parse(as_of)?;
    let result = crate::bank_matches::suggest(
        tx,
        &crate::bank_matches::Input {
            max: Some(5),
            bank_transaction_id: None,
        },
    )?;
    if result["ok"] != true {
        return Err(Error::new(
            "agent_match_failed",
            "Canonical bank suggestions unavailable",
        ));
    }
    let mut rows = result["rows"]
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .collect::<Vec<_>>();
    rows.sort_by_key(|r| r["bankTransactionId"].as_i64());
    let mut steps = vec![];
    for row in rows {
        let Some(best) = best_purchase(row["suggestions"].as_array().ok_or_else(invalid)?) else {
            continue;
        };
        let bank = id(&row["bankTransactionId"])?;
        let doc = id(&best["documentId"])?;
        let mut evidence = json!({"rule":RULE,"confidence":best["confidence"],"reasons":best["reasons"],"supplierName":best["supplierName"]});
        let rules = policy
            .supplier_rules
            .iter()
            .filter(|r| {
                best["supplierName"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&r.token.to_lowercase())
            })
            .collect::<Vec<_>>();
        let confidence = best["confidence"].as_f64().unwrap_or(0.0);
        let refusal = if confidence < AUTO_BOOK_CONFIDENCE {
            Some("AGENT_LOW_CONFIDENCE_MATCH")
        } else if rules.len() != 1 {
            Some("AGENT_NO_ACCOUNT_RULE")
        } else {
            None
        };
        if let Some(kind) = refusal {
            steps.push(ExpenseStep::Route(proposal(kind, bank, doc, evidence)));
            continue;
        }
        let rule = rules[0];
        let signed = amount(&row["amount"])?;
        let gross = if signed.is_negative() {
            Money::default().subtract(&signed)
        } else {
            signed
        };
        if rule.asset_like
            && !gross
                .subtract(&Money::parse(&FIXED_ASSET_REVIEW_DKK.to_string())?)
                .is_negative()
        {
            let year = row["date"]
                .as_str()
                .and_then(|s| s.get(..4))
                .and_then(|s| s.parse::<u32>().ok())
                .ok_or_else(invalid)?;
            let threshold = crate::asset_original::threshold(year);
            evidence["straksafskrivningThresholdDkk"] = json!(threshold);
            evidence["aboveSmallAssetThreshold"] = json!(
                !gross
                    .subtract(&Money::parse(&threshold.to_string())?)
                    .is_negative()
            );
            evidence["grossAmountDkk"] = json!(rentemester_engine::sales_invoice::wire(&gross)?);
            evidence["fixedAssetReviewThresholdDkk"] = json!(FIXED_ASSET_REVIEW_DKK);
            evidence["category"] = json!(rule.label);
            evidence["expenseAccount"] = json!(rule.expense_account);
            steps.push(ExpenseStep::Route(proposal(
                "AGENT_POSSIBLE_FIXED_ASSET",
                bank,
                doc,
                evidence,
            )));
            continue;
        }
        let candidate = crate::expense_original::Input {
            document_id: doc,
            bank_transaction_id: bank,
            expense_account: rule.expense_account.clone(),
            vat_treatment: Some(rule.vat_treatment.clone()),
            payment_account: None,
            date: None,
            text: None,
            idempotency_key: None,
        };
        let summary = BookedExpense {
            document_no: best["invoiceNo"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("DOC-{doc}")),
            document_id: doc,
            bank_transaction_id: bank,
            supplier: string(&best["supplierName"]),
            amount: number(&row["amount"])?,
            currency: string(&row["currency"]),
            expense_account: rule.expense_account.clone(),
            vat_treatment: rule.vat_treatment.clone(),
            label: rule.label.clone(),
            journal_entry_no: None,
        };
        steps.push(ExpenseStep::Book { candidate, summary });
    }
    Ok(steps)
}
const STOP: &[&str] = &[
    "APS", "A/S", "AS", "IVS", "P/S", "PS", "K/S", "KS", "I/S", "IS", "AMBA", "FMBA", "SMBA",
    "GMBH", "LTD", "INC", "PLC", "LLC", "AB", "OY", "DKK", "EUR", "USD", "SEK", "NOK", "GBP",
    "FOR", "OG", "THE", "AND", "VED", "MED", "TIL", "FRA", "DEN", "DET",
];
fn tokens(s: &str) -> Vec<String> {
    s.to_uppercase()
        .split(|c| !rentemester_engine::bank::match_token_character(c))
        .filter(|t| t.encode_utf16().count() >= 3 && !STOP.contains(t))
        .map(str::to_owned)
        .collect()
}
fn corroborates(bank: &str, supplier: &str, bill: &str) -> bool {
    (!bill.is_empty() && bank.to_uppercase().contains(&bill.to_uppercase()))
        || tokens(supplier).iter().any(|t| tokens(bank).contains(t))
}
/// Re-read after every successful payment; do not execute this entire advisory vector unchanged.
/// `as_of` filters bills; the writer independently enforces current open balance and bank use.
pub fn plan_payables(
    tx: &Transaction<'_>,
    policy: &Policy,
    as_of: &str,
) -> Result<Vec<PayableStep>, Error> {
    validate_policy(policy)?;
    Date::parse(as_of)?;
    let open = crate::payables::dated_rows(tx, as_of, "open")?;
    let mut q=tx.prepare("SELECT b.id,b.amount,COALESCE(b.text,'')||' '||COALESCE(b.counterparty_name,'')||' '||COALESCE(b.reference,'')||' '||COALESCE(b.message,'') FROM bank_transactions b WHERE b.amount<0 AND upper(trim(b.currency))='DKK' AND NOT EXISTS(SELECT 1 FROM bank_journal_reconciliations r WHERE r.bank_transaction_id=b.id) ORDER BY b.id").map_err(crate::invalid)?;
    let mut rows = q.query([]).map_err(crate::invalid)?;
    let mut steps = vec![];
    while let Some(row) = rows.next().map_err(crate::invalid)? {
        let bank: i64 = row.get(0).map_err(crate::invalid)?;
        let gross = Money::default().subtract(&crate::bank::signed_money(
            row.get_ref(1).map_err(crate::invalid)?,
        )?);
        let text: String = row.get(2).map_err(crate::invalid)?;
        let candidates = open["rows"]
            .as_array()
            .ok_or_else(invalid)?
            .iter()
            .filter(|p| amount(&p["openBalance"]).is_ok_and(|a| a == gross))
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            continue;
        }
        let p = candidates[0];
        let doc = id(&p["documentId"])?;
        let payable = id(&p["payableId"])?;
        if !corroborates(
            &text,
            p["supplierName"].as_str().unwrap_or(""),
            p["billNo"].as_str().unwrap_or(""),
        ) {
            steps.push(PayableStep::Route(proposal("AGENT_PAYABLE_MATCH_UNCERTAIN",bank,doc,json!({"rule":RULE,"bankTransactionId":bank,"payableId":payable,"openBalance":p["openBalance"],"supplierName":p["supplierName"],"reason":"amount-only match, no supplier/bill-no corroboration"}))));
            continue;
        }
        let candidate = crate::payable_original::Pay {
            payable_id: payable,
            bank_transaction_id: bank,
            confirm: true,
            amount: None,
            date: None,
            payment_account: None,
            note: None,
            idempotency_key: None,
        };
        let summary = PayableMatch {
            payable_id: payable,
            document_id: doc,
            bank_transaction_id: bank,
            supplier: string(&p["supplierName"]),
            amount: number(&p["openBalance"])?,
            journal_entry_no: None,
        };
        steps.push(PayableStep::Pay { candidate, summary });
    }
    Ok(steps)
}

#[cfg(test)]
mod tests;
