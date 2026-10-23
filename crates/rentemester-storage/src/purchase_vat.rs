//! Bounded provider-free canonical purchase proof. Physical originals and current grants
//! remain the caller's held Owner fence; this module never opens files or commits.
use crate::expense;
use rentemester_engine::{Error, document, fiscal_year::Date, manual_journal};
use rusqlite::{Transaction, params, types::ValueRef};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn fail(message: impl Into<String>) -> Error {
    Error::new("purchase_vat_refused", message)
}
fn sql(_: rusqlite::Error) -> Error {
    Error::new(
        "storage_failure",
        "purchase VAT source evidence unavailable; abort transaction",
    )
}
// Preserve the full selected source rows, including nullable/decimal metadata.
// Numeric SQLite bytes are represented using the same JS-compatible formatter
// used by the existing native bank reader, without introducing financial math.
fn rows(tx: &Transaction<'_>, query: &str, ids: &[&dyn rusqlite::ToSql]) -> Result<Value, Error> {
    let mut statement = tx.prepare(query).map_err(sql)?;
    let width = statement.column_count();
    let result = statement
        .query_map(ids, |row| {
            (0..width)
                .map(|i| {
                    Ok(match row.get_ref(i)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(n) => json!(n.to_string()),
                        ValueRef::Real(n) if n.is_finite() => {
                            json!(ryu_js::Buffer::new().format_finite(n))
                        }
                        ValueRef::Text(s) => json!(
                            std::str::from_utf8(s).map_err(|_| rusqlite::Error::InvalidQuery)?
                        ),
                        _ => return Err(rusqlite::Error::InvalidQuery),
                    })
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array)
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    Ok(Value::Array(result))
}
fn canonical_input(
    document_id: i64,
    bank_transaction_id: i64,
    account: &str,
    treatment: &str,
) -> expense::Input {
    expense::Input {
        document_id,
        bank_transaction_id,
        expense_account_no: account.to_owned(),
        vat_treatment: Some(treatment.to_owned()),
        payment_account_no: None,
        transaction_date: None,
        text: None,
    }
}
fn ready(
    tx: &Transaction<'_>,
    input: &expense::Input,
    latest: Date,
    now: &str,
) -> Result<Value, Error> {
    if ![
        "standard",
        "exempt",
        "non_deductible",
        "representation",
        "reverse_charge",
        "eu_goods_acquisition",
    ]
    .contains(&input.vat_treatment.as_deref().unwrap_or(""))
    {
        return Err(fail(
            "unsupported native batch VAT treatment; human review required",
        ));
    }
    let meta = expense::metadata(tx, input.document_id)?;
    if !["purchase_sale", "cash_register_receipt"].contains(&meta.kind()) {
        return Err(fail(
            "native batch purchase proof requires purchase invoices or cash receipts",
        ));
    }
    let supplier = document::supplier(
        meta.sender
            .as_ref()
            .ok_or_else(|| fail("documented supplier required"))?,
    )?;
    let classification = match supplier.kind.as_str() {
        "dk_cvr" => "DK",
        "eu_vat" => "EU",
        "non_eu" => "NON_EU",
        _ => return Err(fail("resolved supplier required")),
    };
    let cache = if classification == "EU" {
        Some(crate::vies::require(
            tx,
            supplier
                .identifier
                .as_deref()
                .ok_or_else(|| fail("EU supplier identifier required"))?,
            now,
        )?)
    } else {
        None
    };
    let identity: (Option<String>,Option<String>,Option<String>,Option<String>) = tx.query_row(
        "SELECT sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status FROM documents WHERE id=?1",[input.document_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(sql)?;
    let persisted = document::supplier(&document::Party {
        vat_or_cvr: identity.0,
        country_code: identity.1,
        identifier_kind: identity.2,
        ..Default::default()
    })?;
    if identity.3.as_deref() != Some("resolved")
        || persisted.kind != supplier.kind
        || persisted.country != supplier.country
        || persisted.identifier != supplier.identifier
    {
        return Err(fail(
            "persisted supplier identity conflicts with immutable document metadata",
        ));
    }
    crate::invoice_extraction::require_completed_or_absent(tx, input.document_id)?;
    let existing: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM purchase_posting_applications WHERE document_id=?1)",
            [input.document_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if existing {
        return Err(fail(
            "purchase already has a posting application; reconcile its original receipt",
        ));
    }
    let prepared = expense::prepare_at(tx, input, now)?;
    let version = crate::journal_preview::manual_journal_rule_version()?;
    let facts = crate::journal_preview::collect_facts(tx, &prepared.request, latest)?;
    let validation = manual_journal::validate(
        &prepared.request,
        &facts,
        &crate::journal_preview::compiled_rules()?,
        &version,
    )?;
    if !validation.errors.is_empty() {
        return Err(fail(format!(
            "purchase journal preview refused: {}",
            serde_json::to_string(&validation.errors)
                .map_err(|_| fail("invalid journal validation"))?
        )));
    }
    let proof = json!({
        "document":rows(tx,"SELECT * FROM documents WHERE id=?1", &[&input.document_id])?,
        "bank":rows(tx,"SELECT * FROM bank_transactions WHERE id=?1", &[&input.bank_transaction_id])?,
        "bankAccount":rows(tx,"SELECT * FROM bank_accounts WHERE id=(SELECT bank_account_id FROM bank_transactions WHERE id=?1)", &[&input.bank_transaction_id])?,
        "accounts":rows(tx,"SELECT * FROM accounts WHERE account_no IN (SELECT value FROM json_each(?1)) ORDER BY account_no", &[&serde_json::to_string(&prepared.request.lines.iter().map(|l|l.account_no.as_str().to_owned()).collect::<Vec<_>>()).map_err(|_|fail("invalid account proof"))?])?,
        "roles":rows(tx,"SELECT * FROM account_role_mappings WHERE role IN ('input_vat','bank') ORDER BY role", &[])?,
        "company":rows(tx,"SELECT * FROM companies ORDER BY id", &[])?,
        "periods":rows(tx,"SELECT * FROM accounting_periods ORDER BY id", &[])?,
        "documentJournals":rows(tx,"SELECT * FROM journal_entries WHERE document_id=?1 ORDER BY id", &[&input.document_id])?,
        "bankLinks":rows(tx,"SELECT * FROM bank_journal_reconciliations WHERE bank_transaction_id=?1 ORDER BY reconciliation_id", &[&input.bank_transaction_id])?
    });
    let foreign = meta.currency() != "DKK" || classification != "DK";
    let mut proof = proof;
    if foreign {
        proof["vies"] = json!(cache);
        proof["extraction"] = json!(crate::invoice_extraction::inspect(tx, input.document_id)?);
        proof["roles"] = rows(
            tx,
            "SELECT * FROM account_role_mappings WHERE role IN ('input_vat','bank','reverse_charge_vat') ORDER BY role",
            &[],
        )?;
        proof["partyLinks"] = rows(
            tx,
            "SELECT * FROM current_document_party_links WHERE document_id=?1 ORDER BY id",
            &[&input.document_id],
        )?;
        proof["partyResolution"] = rows(
            tx,
            "SELECT * FROM current_document_party_resolution_events WHERE document_id=?1 ORDER BY id",
            &[&input.document_id],
        )?;
    }
    let mut result = json!({"format":if foreign {"rentemester-native-purchase-vat/2"} else {"rentemester-native-purchase-vat/1"},"ok":true,"ready":true,
        "documentId":input.document_id,"bankTransactionId":input.bank_transaction_id,
        "account":input.expense_account_no,"vatTreatment":input.vat_treatment,
        "classification":classification,"wouldCallProvider":false,"errors":[],"latestAllowedDate":latest,
        "sourceProof":proof,"plan":{"request":prepared.request,"ruleVersion":version,
        "grossAmount":prepared.gross.decimal(),"netAmount":prepared.net.decimal(),
        "vatAmount":prepared.deductible.decimal(),"grossAmountDkk":prepared.basis.gross_dkk.decimal()}});
    if foreign {
        result["country"] = json!(supplier.country);
        result["providerStatus"] = json!(if cache.is_some() {
            "valid"
        } else {
            "not_required"
        });
        if let Some(cache) = &cache {
            result["validatedAt"] = cache["validatedAt"].clone();
            result["evidenceExpiresAt"] = cache["expiresAt"].clone();
            let raw = cache["rawResponse"]
                .as_str()
                .and_then(|s| serde_json::from_str::<Value>(s).ok());
            result["providerReference"] = raw
                .as_ref()
                .and_then(|r| r.get("reference").or_else(|| r.get("requestIdentifier")))
                .cloned()
                .unwrap_or(Value::Null);
        }
        if let Some(source) = prepared.basis.rate_source {
            result["plan"]["rateSource"] = json!(source);
        }
    }
    result["proofHash"] = json!(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&result).map_err(|_| fail("invalid proof"))?)
    ));
    Ok(result)
}
/// Pure selected-source inspection. Unsupported business evidence is explicit, not
/// an empty success. Malformed/unavailable storage remains a hard error.
pub fn inspect_at(
    tx: &Transaction<'_>,
    document_id: i64,
    bank_transaction_id: i64,
    account: &str,
    vat_treatment: &str,
    latest: Date,
    now: &str,
) -> Result<Value, Error> {
    match ready(
        tx,
        &canonical_input(document_id, bank_transaction_id, account, vat_treatment),
        latest,
        now,
    ) {
        Ok(proof) => Ok(proof),
        Err(error) if error.code != "storage_failure" => Ok(
            json!({"format":"rentemester-native-purchase-vat/1","ok":false,"ready":false,"documentId":document_id,"bankTransactionId":bank_transaction_id,"account":account,"vatTreatment":vat_treatment,"wouldCallProvider":false,"errors":[{"code":error.code,"message":error.message}]}),
        ),
        Err(error) => Err(error),
    }
}
/// All effects belong to the caller-owned outer transaction. Returning Err MUST
/// roll back that whole transaction, including successful earlier batch items.
pub fn apply_at(
    tx: &Transaction<'_>,
    input: &expense::Input,
    expected: &Value,
    application_key: &str,
    actor: &str,
    latest: Date,
    now: &str,
) -> Result<Value, Error> {
    if application_key.trim() != application_key
        || application_key.is_empty()
        || application_key.len() > 512
        || application_key.chars().any(char::is_control)
    {
        return Err(fail("explicit bounded application key required"));
    }
    let subject = actor
        .strip_prefix("user:")
        .filter(|s| !s.trim().is_empty() && !s.chars().any(char::is_control))
        .ok_or_else(|| fail("authenticated human actor label required"))?;
    let treatment = input
        .vat_treatment
        .as_deref()
        .ok_or_else(|| fail("explicit VAT treatment required"))?;
    let current = ready(
        tx,
        &canonical_input(
            input.document_id,
            input.bank_transaction_id,
            &input.expense_account_no,
            treatment,
        ),
        latest,
        now,
    )?;
    if &current != expected {
        return Err(fail(
            "purchase VAT source proof is stale or not ready; re-plan and review",
        ));
    }
    let actual = expense::prepare_at(tx, input, now)?;
    if serde_json::to_value(&actual.request).map_err(|_| fail("invalid purchase request"))?
        != current["plan"]["request"]
    {
        return Err(fail(
            "purchase input differs from reviewed exact monetary plan",
        ));
    }
    let mut result = expense::post_at(tx, input, subject, latest, now)?;
    let document_id = input.document_id;
    let sender = expense::metadata(tx, document_id)?
        .sender
        .ok_or_else(|| fail("supplier disappeared"))?;
    let supplier = document::supplier(&sender)?;
    tx.execute("INSERT INTO vat_validation_events(document_id,event_type,supplier_country_code,supplier_identifier,classification,provider_status,actor,detail_json,evidence_expires_at,created_at) VALUES(?1,'preflight_passed',?5,?2,?6,?7,?3,?4,?8,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![document_id,supplier.identifier,actor,serde_json::to_string(&current).map_err(|_|fail("invalid stored VAT proof"))?,supplier.country,current["classification"].as_str(),current["providerStatus"].as_str().unwrap_or("local"),current["evidenceExpiresAt"].as_str()]).map_err(sql)?;
    let vat_id = tx.last_insert_rowid();
    tx.execute("INSERT INTO purchase_posting_applications(document_id,application_key,journal_entry_id,bank_transaction_id,status) VALUES(?1,?2,?3,?4,'posted')",params![document_id,application_key,result["entryId"].as_i64().ok_or_else(||fail("journal result missing entry ID"))?,input.bank_transaction_id]).map_err(sql)?;
    result["vatPreflightId"] = json!(vat_id);
    result["purchaseApplicationId"] = json!(tx.last_insert_rowid());
    Ok(result)
}

fn wall_clock(tx: &Transaction<'_>) -> Result<String, Error> {
    tx.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
        r.get(0)
    })
    .map_err(sql)
}
pub fn inspect(
    tx: &Transaction<'_>,
    document: i64,
    bank: i64,
    account: &str,
    treatment: &str,
    latest: Date,
) -> Result<Value, Error> {
    inspect_at(
        tx,
        document,
        bank,
        account,
        treatment,
        latest,
        &wall_clock(tx)?,
    )
}
pub fn apply(
    tx: &Transaction<'_>,
    input: &expense::Input,
    expected: &Value,
    key: &str,
    actor: &str,
    latest: Date,
) -> Result<Value, Error> {
    apply_at(tx, input, expected, key, actor, latest, &wall_clock(tx)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, TransactionBehavior};
    fn horizon() -> Date {
        Date::parse("2026-12-01").unwrap()
    }
    fn fixture() -> Connection {
        let mut control = Connection::open_in_memory().unwrap();
        let mut company = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(
            &mut control,
            &mut company,
            "Synthetic batch purchases",
        )
        .unwrap();
        let tx = company
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("UPDATE companies SET vat_period_type='quarter' WHERE id=1;
            INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic bank','asset','debit'),('X','Synthetic expense','expense','debit'),('V','Synthetic VAT','vat','debit');
            INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by) VALUES('input_vat','V','confirmed',1,'synthetic');
            INSERT INTO bank_accounts(slug,name,ledger_account_no) VALUES('synthetic','Synthetic bank','A');").unwrap();
        for (number, hash, amount, vat) in [(1, "a", 125, 25), (2, "b", 250, 50)] {
            let meta:document::Metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-10-18","deliveryDescription":"Synthetic supplies","invoiceNo":format!("SYN-{number}"),"amountIncVat":amount,"vatAmount":vat,"sender":{"name":"Synthetic supplier","address":"Synthetic street 1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic buyer","address":"Synthetic street 2","vatOrCvr":"DK87654321"}})).unwrap();
            crate::documents::register(
                &tx,
                crate::documents::Original {
                    filename: "synthetic.txt",
                    hash: &hash.repeat(64),
                    path: "synthetic-original",
                    mime: "text/plain",
                },
                &meta,
                "synthetic",
                "2026-10-18",
            )
            .unwrap();
        }
        let csv = "date;text;amount\n2026-10-18;Synthetic purchase one;-125\n2026-10-18;Synthetic purchase two;-250\n";
        crate::bank::import(
            &tx,
            csv,
            &rentemester_engine::bank::parse(csv).unwrap(),
            Some("synthetic"),
            "synthetic",
            "2026-10-18T12:00:00Z",
            "synthetic-bank-original",
        )
        .unwrap();
        tx.commit().unwrap();
        company
    }
    fn count(tx: &Transaction<'_>, table: &str) -> i64 {
        tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }
    fn proof(tx: &Transaction<'_>, id: i64) -> Value {
        let result = inspect(tx, id, id, "X", "standard", horizon()).unwrap();
        assert_eq!(result["ready"], true, "{result}");
        result
    }
    #[test]
    fn two_reviewed_purchases_keep_second_proof_stable_and_return_real_references() {
        let mut db = fixture();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let before = tx.total_changes();
        let first = proof(&tx, 1);
        let second = proof(&tx, 2);
        assert_eq!(tx.total_changes(), before, "inspect must not mutate");
        assert_eq!(first["plan"]["netAmount"], "100.00");
        assert_eq!(first["plan"]["vatAmount"], "25.00");
        let a = apply(
            &tx,
            &canonical_input(1, 1, "X", "standard"),
            &first,
            "synthetic-batch:one",
            "user:synthetic",
            horizon(),
        )
        .unwrap();
        assert_eq!(
            proof(&tx, 2),
            second,
            "earlier item must not stale independent proof"
        );
        let b = apply(
            &tx,
            &canonical_input(2, 2, "X", "standard"),
            &second,
            "synthetic-batch:two",
            "user:synthetic",
            horizon(),
        )
        .unwrap();
        for result in [&a, &b] {
            let reference:(i64,i64,String)=tx.query_row("SELECT journal_entry_id,bank_transaction_id,status FROM purchase_posting_applications WHERE id=?1",[result["purchaseApplicationId"].as_i64().unwrap()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
            assert_eq!(reference.0, result["entryId"].as_i64().unwrap());
            assert_eq!(reference.2, "posted");
            assert_eq!(
                tx.query_row::<String, _, _>(
                    "SELECT event_type FROM vat_validation_events WHERE id=?1",
                    [result["vatPreflightId"].as_i64().unwrap()],
                    |r| r.get(0)
                )
                .unwrap(),
                "preflight_passed"
            );
        }
        assert_eq!(count(&tx, "journal_entries"), 2);
        assert_eq!(count(&tx, "bank_journal_reconciliations"), 2);
        assert_eq!(count(&tx, "purchase_posting_applications"), 2);
        let vat = crate::vat_report::read(&tx, "synthetic", "2026-10-18", None).unwrap();
        assert_eq!(vat["inputVat"].as_f64(), Some(75.0));
        assert_eq!(vat["vatReportErrors"], json!([]));
        assert!(
            apply(
                &tx,
                &canonical_input(1, 1, "X", "standard"),
                &first,
                "synthetic-batch:one",
                "user:synthetic",
                horizon()
            )
            .is_err()
        );
        assert_eq!(count(&tx, "journal_entries"), 2);
        tx.commit().unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(count(&tx, "vat_validation_events"), 2);
        assert_eq!(count(&tx, "purchase_posting_applications"), 2);
    }
    #[test]
    fn stale_bank_and_different_request_refuse_before_financial_effects() {
        let mut db = fixture();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let expected = proof(&tx, 1);
        let mut changed = canonical_input(1, 1, "X", "standard");
        changed.text = Some("Unreviewed different journal text".into());
        assert!(
            apply(
                &tx,
                &changed,
                &expected,
                "synthetic:changed",
                "user:synthetic",
                horizon()
            )
            .unwrap_err()
            .message
            .contains("differs")
        );
        tx.execute(
            "UPDATE bank_transactions SET text='Changed source evidence' WHERE id=1",
            [],
        )
        .unwrap();
        assert!(
            apply(
                &tx,
                &canonical_input(1, 1, "X", "standard"),
                &expected,
                "synthetic:stale",
                "user:synthetic",
                horizon()
            )
            .unwrap_err()
            .message
            .contains("stale")
        );
        assert_eq!(count(&tx, "journal_entries"), 0);
        assert_eq!(count(&tx, "vat_validation_events"), 0);
        assert_eq!(count(&tx, "purchase_posting_applications"), 0);
    }
    #[test]
    fn failed_second_application_rolls_back_entire_caller_transaction() {
        let mut db = fixture();
        {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let first = proof(&tx, 1);
            let second = proof(&tx, 2);
            apply(
                &tx,
                &canonical_input(1, 1, "X", "standard"),
                &first,
                "synthetic:conflicting-key",
                "user:synthetic",
                horizon(),
            )
            .unwrap();
            assert!(
                apply(
                    &tx,
                    &canonical_input(2, 2, "X", "standard"),
                    &second,
                    "synthetic:conflicting-key",
                    "user:synthetic",
                    horizon()
                )
                .is_err()
            );
            // The caller aborts; B never commits an independently successful prefix.
            tx.rollback().unwrap();
        }
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        for table in [
            "journal_entries",
            "journal_lines",
            "bank_journal_reconciliations",
            "vat_validation_events",
            "purchase_posting_applications",
        ] {
            assert_eq!(count(&tx, table), 0, "{table}");
        }
        assert!(proof(&tx, 1)["ready"].as_bool().unwrap());
    }
    #[test]
    fn supplier_conflict_and_unsupported_families_are_explicit_readonly_blockers() {
        let mut db = fixture();
        let tx = db.transaction().unwrap();
        let before = tx.total_changes();
        proof(&tx, 1);
        assert_eq!(
            before,
            tx.total_changes(),
            "deferred readonly inspection must not write"
        );
        for treatment in ["reverse_charge", "eu_goods_acquisition"] {
            let before = tx.total_changes();
            let blocked = inspect(&tx, 1, 1, "X", treatment, horizon()).unwrap();
            assert_eq!(blocked["ready"], false);
            assert!(!blocked["errors"].as_array().unwrap().is_empty());
            assert_eq!(before, tx.total_changes());
        }
        let representation = inspect(&tx, 1, 1, "X", "representation", horizon()).unwrap();
        assert_eq!(
            representation["ready"], true,
            "canonical representation supported: {representation}"
        );
        tx.execute(
            "UPDATE documents SET supplier_identity_status='human_resolution_required' WHERE id=1",
            [],
        )
        .unwrap();
        assert_eq!(
            inspect(&tx, 1, 1, "X", "standard", horizon()).unwrap()["ready"],
            false
        );
        assert_eq!(count(&tx, "journal_entries"), 0);
        assert_eq!(count(&tx, "vat_validation_events"), 0);
    }
}
