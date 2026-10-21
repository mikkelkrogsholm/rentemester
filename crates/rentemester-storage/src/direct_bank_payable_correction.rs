//! One source-proven direct purchase reclassified through the shared payable writers.
//! No commit authority, new keys, accounting calculations, or public bypass flags.
use crate::{
    bank_reconciliation::correction as bank_correction,
    idempotency::{self, Operation, Outcome, PrincipalKind},
    payables,
};
use rentemester_engine::{
    Error,
    fiscal_year::Date,
    identifiers::{AccountNo, JournalEntryId},
    manual_journal,
    money::Money,
};
use rusqlite::{Connection, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[cfg(test)]
mod tests;
fn fail(code: &str) -> Error {
    Error::new("direct_bank_payable_correction_refused", code)
}
fn sql(_: rusqlite::Error) -> Error {
    fail("CORRECTION_STORAGE_UNAVAILABLE")
}
fn id(n: i64) -> Result<(), Error> {
    if !(1..=9_007_199_254_740_991).contains(&n) {
        Err(fail("POSITIVE_SAFE_ID_REQUIRED"))
    } else {
        Ok(())
    }
}
fn hash(v: &Value) -> Result<String, Error> {
    if crate::bookkeeping_batch::canonical(v).len() > 64 * 1024 {
        return Err(fail("CORRECTION_PROOF_EXCEEDS_64KIB"));
    }
    Ok(crate::bookkeeping_batch::hash(v))
}
fn rows(db: &Connection, q: &str, p: impl rusqlite::Params) -> Result<Vec<Value>, Error> {
    crate::bookkeeping_batch::rows(db, q, p)
}
fn get_id(v: &Value, k: &str) -> Result<i64, Error> {
    let n = v[k]
        .as_i64()
        .ok_or_else(|| fail("REQUIRED_RESULT_ID_MISSING"))?;
    id(n)?;
    Ok(n)
}
fn string<'a>(v: &'a Value, k: &str) -> Result<&'a str, Error> {
    v[k].as_str()
        .ok_or_else(|| fail("REQUIRED_SOURCE_FIELD_MISSING"))
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanInput {
    pub document_id: i64,
    pub bank_transaction_id: i64,
    pub bill_date: String,
    pub due_date: String,
    pub expense_account_no: String,
    pub vat_treatment: Option<String>,
    pub vendor_id: Option<i64>,
    pub note: Option<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyInput {
    pub document_id: i64,
    pub bank_transaction_id: i64,
    pub bill_date: String,
    pub due_date: String,
    pub expense_account_no: String,
    pub vat_treatment: Option<String>,
    pub vendor_id: Option<i64>,
    pub note: Option<String>,
    pub plan_hash: String,
    pub reason: String,
    pub idempotency_key: Option<String>,
    pub confirm: bool,
}
impl PlanInput {
    pub fn validate(&self) -> Result<(), Error> {
        id(self.document_id)?;
        id(self.bank_transaction_id)?;
        if let Some(v) = self.vendor_id {
            id(v)?
        }
        AccountNo::parse(&self.expense_account_no)?;
        if Date::parse(&self.due_date)? < Date::parse(&self.bill_date)? {
            return Err(fail("DUE_DATE_BEFORE_BILL_DATE"));
        }
        if self
            .vat_treatment
            .as_deref()
            .is_some_and(|v| !["standard", "exempt", "non_deductible"].contains(&v))
        {
            return Err(fail("VAT_TREATMENT_INVALID"));
        }
        Ok(())
    }
    fn bill(&self) -> payables::Register {
        payables::Register {
            document_id: self.document_id,
            bill_date: self.bill_date.clone(),
            due_date: self.due_date.clone(),
            expense_account_no: self.expense_account_no.clone(),
            vat_treatment: self.vat_treatment.clone(),
            vendor_id: self.vendor_id,
            note: self.note.clone(),
            confirm: true,
        }
    }
}
impl ApplyInput {
    pub fn source(&self) -> PlanInput {
        PlanInput {
            document_id: self.document_id,
            bank_transaction_id: self.bank_transaction_id,
            bill_date: self.bill_date.clone(),
            due_date: self.due_date.clone(),
            expense_account_no: self.expense_account_no.clone(),
            vat_treatment: self.vat_treatment.clone(),
            vendor_id: self.vendor_id,
            note: self.note.clone(),
        }
    }
    pub fn validate(&self) -> Result<(), Error> {
        self.source().validate()?;
        if !self.confirm
            || self.plan_hash.len() != 64
            || !self
                .plan_hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(fail("CONFIRMATION_AND_PLAN_HASH_REQUIRED"));
        }
        let reason = crate::journal_post::trim(&self.reason);
        if reason.is_empty() || reason.chars().count() > 1000 {
            return Err(fail("REASON_REQUIRED_OR_TOO_LONG"));
        }
        if let Some(key) = &self.idempotency_key {
            idempotency::validate_key(key)?
        }
        Ok(())
    }
}
/// Constructible ONLY after this module proves the exact current original and bank pair.
/// The optional reversal identifies a real mirror, never permission to skip arbitrary history.
pub(crate) struct PurchaseProof {
    document: i64,
    bank: i64,
    original: i64,
    original_hash: String,
    reconciliation: String,
    reversal: Option<i64>,
}
impl PurchaseProof {
    fn current(&self, tx: &Transaction<'_>) -> Result<(), Error> {
        let current = bank_correction::current(tx, self.bank)?;
        if current.0 != self.reconciliation || current.1 != self.original {
            return Err(fail("DIRECT_BANK_RECONCILIATION_CHANGED"));
        }
        let proof:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_entries WHERE id=?1 AND document_id=?2 AND source_bank_transaction_id=?3 AND entry_hash=?4 AND reversal_of_entry_id IS NULL AND status='posted')",params![self.original,self.document,self.bank,self.original_hash],|r|r.get(0)).map_err(sql)?;
        if !proof {
            return Err(fail("DIRECT_BANK_ORIGINAL_CHANGED"));
        }
        Ok(())
    }
    pub(crate) fn require_reversed(&self, tx: &Transaction<'_>) -> Result<(), Error> {
        self.current(tx)?;
        let actual = bank_correction::reversal(tx, self.original)?;
        if Some(actual.0) != self.reversal {
            return Err(fail("EXACT_MIRROR_REVERSAL_REQUIRED"));
        }
        Ok(())
    }
    pub(crate) fn document_history(
        &self,
        tx: &Transaction<'_>,
        document: i64,
    ) -> Result<(), Error> {
        self.current(tx)?;
        if document != self.document {
            return Err(fail("CORRECTION_DOCUMENT_MISMATCH"));
        }
        if self.reversal.is_some() {
            self.require_reversed(tx)?;
        }
        let extra:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM payables WHERE document_id=?1) OR EXISTS(SELECT 1 FROM journal_entries WHERE document_id=?1 AND id<>?2 AND (?3 IS NULL OR id<>?3))",params![document,self.original,self.reversal],|r|r.get(0)).map_err(sql)?;
        if extra {
            return Err(fail("UNRELATED_DOCUMENT_HISTORY_BLOCKS_CORRECTION"));
        }
        Ok(())
    }
    pub(crate) fn payment_history(
        &self,
        tx: &Transaction<'_>,
        document: i64,
        bank: i64,
        payable: i64,
    ) -> Result<(), Error> {
        self.require_reversed(tx)?;
        if document != self.document || bank != self.bank {
            return Err(fail("CORRECTION_PAYMENT_SOURCE_MISMATCH"));
        }
        let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM payables p JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.id=?1 AND p.document_id=?2 AND j.document_id=?2 AND j.source_bank_transaction_id IS NULL AND j.reversal_of_entry_id IS NULL AND j.status='posted') AND NOT EXISTS(SELECT 1 FROM payable_payments WHERE bank_transaction_id=?3) AND NOT EXISTS(SELECT 1 FROM journal_entries WHERE document_id=?2 AND id NOT IN(?4,?5,(SELECT journal_entry_id FROM payables WHERE id=?1)))",params![payable,document,bank,self.original,self.reversal],|r|r.get(0)).map_err(sql)?;
        if !valid {
            return Err(fail("UNRELATED_PAYMENT_OR_JOURNAL_HISTORY"));
        }
        Ok(())
    }
}
fn source(
    tx: &Transaction<'_>,
    i: &PlanInput,
) -> Result<(Value, Value, Value, PurchaseProof), Error> {
    i.validate()?;
    let document = rows(tx, "SELECT * FROM documents WHERE id=?1", [i.document_id])?
        .into_iter()
        .next()
        .ok_or_else(|| fail("DIRECT_BANK_PURCHASE_RECONCILIATION_REQUIRED"))?;
    let bank = rows(
        tx,
        "SELECT * FROM bank_transactions WHERE id=?1",
        [i.bank_transaction_id],
    )?
    .into_iter()
    .next()
    .ok_or_else(|| fail("DIRECT_BANK_PURCHASE_RECONCILIATION_REQUIRED"))?;
    let originals = rows(
        tx,
        "SELECT * FROM journal_entries WHERE document_id=?1 AND source_bank_transaction_id=?2 AND status='posted' AND reversal_of_entry_id IS NULL ORDER BY id",
        params![i.document_id, i.bank_transaction_id],
    )?;
    if originals.len() != 1 {
        return Err(fail("DIRECT_BANK_PURCHASE_RECONCILIATION_REQUIRED"));
    }
    let original = originals.into_iter().next().unwrap();
    let original_id = get_id(&original, "id")?;
    if crate::journal_hash::stored_hash(tx, original_id, string(&original, "previous_hash")?)?
        != string(&original, "entry_hash")?
    {
        return Err(fail("ORIGINAL_JOURNAL_HASH_MISMATCH"));
    }

    let current = bank_correction::current(tx, i.bank_transaction_id)?;
    if current.1 != original_id {
        return Err(fail("DIRECT_BANK_PURCHASE_RECONCILIATION_REQUIRED"));
    }
    let proof = PurchaseProof {
        document: i.document_id,
        bank: i.bank_transaction_id,
        original: original_id,
        original_hash: string(&original, "entry_hash")?.into(),
        reconciliation: current.0,
        reversal: None,
    };
    proof.document_history(tx, i.document_id)?;
    let reversed: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM journal_entries WHERE reversal_of_entry_id=?1)",
            [original_id],
            |r| r.get(0),
        )
        .map_err(sql)?;
    if reversed {
        return Err(fail("DIRECT_BANK_JOURNAL_ALREADY_REVERSED"));
    }
    if document["currency"] != "DKK" || bank["currency"] != "DKK" {
        return Err(fail("OUTGOING_DKK_BANK_EVIDENCE_REQUIRED"));
    }
    let signed: Money = tx
        .query_row(
            "SELECT amount FROM bank_transactions WHERE id=?1",
            [i.bank_transaction_id],
            |r| crate::bank::signed_money(r.get_ref(0)?).map_err(|_| rusqlite::Error::InvalidQuery),
        )
        .map_err(sql)?;
    let gross = Money::parse(&document["amount_inc_vat"].to_string())?;
    if !signed.is_negative() {
        return Err(fail("OUTGOING_DKK_BANK_EVIDENCE_REQUIRED"));
    }
    if gross != Money::default().subtract(&signed) {
        return Err(fail("DOCUMENT_AND_BANK_AMOUNT_MISMATCH"));
    }
    if document["invoice_date"] != i.bill_date {
        return Err(fail("BILL_DATE_MUST_EQUAL_DOCUMENT_INVOICE_DATE"));
    }
    if string(&bank, "transaction_date")? < i.bill_date.as_str() {
        return Err(fail("SETTLEMENT_BEFORE_BILL_DATE"));
    }
    Ok((document, bank, original, proof))
}
pub fn plan(tx: &Transaction<'_>, i: &PlanInput, latest: Date) -> Result<Value, Error> {
    let (document, bank, original, proof) = source(tx, i)?;
    let bill = payables::prepare_bill(tx, &i.bill(), Some(&proof))?;
    let version = crate::journal_preview::manual_journal_rule_version()?;
    let preview = manual_journal::validate(
        &bill.journal,
        &crate::journal_preview::collect_facts(tx, &bill.journal, latest)?,
        &crate::journal_preview::compiled_rules()?,
        &version,
    )?;
    if !preview.errors.is_empty() {
        return Err(fail(
            &serde_json::to_string(&preview.errors).map_err(|_| fail("INVALID_PREVIEW"))?,
        ));
    }
    let dates = [
        i.bill_date.as_str(),
        string(&bank, "transaction_date")?,
        string(&original, "transaction_date")?,
    ];
    let first = dates.iter().min().unwrap();
    let last = dates.iter().max().unwrap();
    let all_periods = rows(
        tx,
        "SELECT * FROM accounting_periods WHERE NOT(period_end<?1 OR period_start>?2) ORDER BY id",
        params![first, last],
    )?;
    let mut periods = vec![];
    for mut p in all_periods {
        p["effectiveStatus"] = json!(crate::periods::effective(
            tx,
            get_id(&p, "id")?,
            string(&p, "status")?.into()
        )?);
        if (p["effectiveStatus"] == "closed" || p["effectiveStatus"] == "reported")
            && dates.iter().any(|date| {
                *date >= p["period_start"].as_str().unwrap_or("")
                    && *date <= p["period_end"].as_str().unwrap_or("")
            })
        {
            return Err(fail("ACCOUNTING_PERIOD_CLOSED"));
        }
        periods.push(p)
    }
    let gross = Money::parse(&document["amount_inc_vat"].to_string())?;
    let amount = rentemester_engine::sales_invoice::wire(&gross)?;
    let roles = rows(
        tx,
        "SELECT m.role,m.account_no,m.version,a.type,a.active FROM account_role_mappings m JOIN accounts a ON a.account_no=m.account_no WHERE m.status='confirmed' AND m.role IN('bank','creditors','input_vat') ORDER BY m.role",
        [],
    )?;
    let accounts = rows(
        tx,
        "SELECT account_no,type,active,name FROM accounts WHERE account_no=?1",
        [&i.expense_account_no],
    )?;
    let company = rows(tx, "SELECT * FROM companies WHERE id=1", [])?
        .into_iter()
        .next()
        .ok_or_else(|| fail("COMPANY_MISSING"))?;
    let original_id = proof.original;
    let mut result = json!({"schemaVersion":"rentemester-direct-bank-purchase-payable-correction-v1","documentId":i.document_id,"documentHash":document["sha256_hash"],"documentPayloadHash":hash(&document["payload_json"])?,"documentVatAmount":document["vat_amount"],"bankTransactionId":i.bank_transaction_id,"bankDate":bank["transaction_date"],"bankAmount":amount,"originalJournalEntryId":original_id,"originalJournalEntryNo":original["entry_no"],"originalJournalHash":original["entry_hash"],"originalJournalDate":original["transaction_date"],"billDate":i.bill_date,"dueDate":i.due_date,"expenseAccountNo":i.expense_account_no,"vatTreatment":i.vat_treatment,"vendorId":i.vendor_id,"note":i.note,"vatPeriodType":company["vat_period_type"],"accountRoles":roles,"accounts":accounts,"accountingPeriods":periods,
 "sourceProof":{"bankSourceHash":crate::bank_reconciliation::source_hash(tx,i.bank_transaction_id)?,"bank":bank,"documentHash":document["sha256_hash"],"documentMetadataHash":hash(&document)?,"original":original,"originalLines":rows(tx,"SELECT * FROM journal_lines WHERE journal_entry_id=?1 ORDER BY id",[original_id])?,"reconciliation":proof.reconciliation,"reconciliationHistory":bank_correction::history(tx,i.bank_transaction_id)?,"company":company,"vendor":if let Some(v)=i.vendor_id{rows(tx,"SELECT * FROM vendors WHERE id=?1",[v])?}else{vec![]},"billRequest":bill.journal,"ruleVersion":version},
 "summary":{"document":{"id":i.document_id,"documentNo":document["document_no"],"invoiceNo":document["invoice_no"],"invoiceDate":document["invoice_date"],"supplierName":document["supplier_name"],"currency":document["currency"],"grossAmount":amount,"vatAmount":document["vat_amount"]},"bank":{"id":i.bank_transaction_id,"date":bank["transaction_date"],"text":bank["text"],"currency":bank["currency"],"signedAmount":bank["amount"],"amount":amount},"originalJournal":{"id":original_id,"entryNo":original["entry_no"],"date":original["transaction_date"],"hash":original["entry_hash"]},"expenseAccount":{"accountNo":i.expense_account_no,"name":accounts.first().map(|a|&a["name"])},"effectiveVatTreatment":bill.treatment}});
    result["planHash"] = json!(hash(&result)?);
    Ok(result)
}
fn row_result(db: &Connection, row: &Value, idempotent: bool) -> Result<Value, Error> {
    let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM documents d JOIN journal_entries j ON j.id=?1 JOIN journal_entries r ON r.id=?2 JOIN payables p ON p.id=?3 JOIN payable_payments pp ON pp.id=?4 JOIN journal_entries s ON s.id=?5 WHERE d.id=?6 AND d.sha256_hash=?7 AND j.document_id=d.id AND j.source_bank_transaction_id=?8 AND j.entry_hash=?9 AND r.reversal_of_entry_id=j.id AND p.document_id=d.id AND pp.payable_id=p.id AND pp.bank_transaction_id=?8 AND pp.journal_entry_id=s.id AND EXISTS(SELECT 1 FROM bank_reconciliation_correction_events e WHERE e.bank_transaction_id=?8 AND e.replacement_journal_entry_id=s.id AND e.superseded_journal_entry_id=j.id))",params![get_id(row,"original_journal_entry_id")?,get_id(row,"reversal_journal_entry_id")?,get_id(row,"payable_id")?,get_id(row,"payment_id")?,get_id(row,"settlement_journal_entry_id")?,get_id(row,"document_id")?,string(row,"document_hash")?,get_id(row,"bank_transaction_id")?,string(row,"original_journal_hash")?],|r|r.get(0)).map_err(sql)?;
    if !valid {
        return Err(fail("CORRECTION_HISTORY_PROOF_INVALID"));
    }
    Ok(
        json!({"ok":true,"idempotent":idempotent,"id":row["id"],"payableId":row["payable_id"],"paymentId":row["payment_id"],"settlementJournalEntryId":row["settlement_journal_entry_id"],"originalJournalEntryId":row["original_journal_entry_id"],"reversalJournalEntryId":row["reversal_journal_entry_id"],"sourcePrincipal":{"kind":row["principal_kind"],"subjectId":row["principal_subject_id"]},"actor":row["actor"],"errors":[]}),
    )
}
fn execute(
    tx: &Transaction<'_>,
    i: &ApplyInput,
    subject: &str,
    latest: Date,
) -> Result<Value, Error> {
    let old = rows(
        tx,
        "SELECT * FROM direct_bank_purchase_payable_corrections WHERE document_id=?1 AND bank_transaction_id=?2 ORDER BY id",
        params![i.document_id, i.bank_transaction_id],
    )?;
    if !old.is_empty() {
        if old.len() != 1 || old[0]["plan_hash"] != i.plan_hash {
            return Err(fail("IDEMPOTENCY_PAYLOAD_CONFLICT"));
        }
        return row_result(tx, &old[0], true);
    }
    let source = i.source();
    let current = plan(tx, &source, latest)?;
    if current["planHash"] != i.plan_hash {
        return Err(fail("PLAN_HASH_MISMATCH"));
    }
    let (_, bank, original, mut proof) = self::source(tx, &source)?;
    let by = format!("user:{subject}");
    let reversal = crate::journal_reversal::reverse(
        tx,
        &crate::journal_reversal::Input {
            entry_id: JournalEntryId::from_sqlite(proof.original),
            transaction_date: Date::parse(string(&original, "transaction_date")?)?,
            reason: i.reason.clone(),
        },
        &crate::journal_preview::manual_journal_rule_version()?,
        latest,
        crate::journal_post::Actor {
            subject: &by,
            program: "direct-bank-purchase-payable-correction",
        },
    )?;
    if reversal["ok"] != true {
        return Err(fail(&reversal["errors"].to_string()));
    }
    proof.reversal = Some(get_id(&reversal, "entryId")?);
    #[cfg(test)]
    tests::checkpoint(1)?;
    let payable = payables::register_corrected(tx, &source.bill(), subject, latest, &proof)?;
    #[cfg(test)]
    tests::checkpoint(2)?;
    let payment = payables::pay_corrected(
        tx,
        get_id(&payable, "payableId")?,
        &payables::Payment {
            bank_transaction_id: i.bank_transaction_id,
            payment_date: Some(string(&bank, "transaction_date")?.into()),
            amount: None,
            payment_account_no: None,
            note: i.note.clone(),
            confirm: true,
        },
        subject,
        latest,
        &proof,
    )?;
    #[cfg(test)]
    tests::checkpoint(3)?;
    let settlement = get_id(&payment, "journalEntryId")?;
    let bp = bank_correction::plan(tx, i.bank_transaction_id, settlement)?;
    bank_correction::apply_reviewed(
        tx,
        &bank_correction::ReviewedCorrection {
            bank_transaction_id: i.bank_transaction_id,
            replacement_journal_entry_id: settlement,
            expected_reconciliation_id: &proof.reconciliation,
            plan_hash: string(&bp["plan"], "planHash")?,
            reason: &i.reason,
        },
        subject,
    )?;
    #[cfg(test)]
    tests::checkpoint(4)?;
    tx.execute("INSERT INTO direct_bank_purchase_payable_corrections(document_id,bank_transaction_id,original_journal_entry_id,reversal_journal_entry_id,payable_id,payment_id,settlement_journal_entry_id,document_hash,original_journal_hash,plan_hash,reason,actor,principal_kind,principal_subject_id,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'user',?13,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![i.document_id,i.bank_transaction_id,proof.original,proof.reversal,get_id(&payable,"payableId")?,get_id(&payment,"paymentId")?,settlement,current["documentHash"].as_str(),proof.original_hash,i.plan_hash,crate::journal_post::trim(&i.reason),by,subject]).map_err(sql)?;
    let id = tx.last_insert_rowid();
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('direct_bank_purchase_payable_corrected','bank_transaction',?1,?2,?3)",params![i.bank_transaction_id.to_string(),format!("Corrected direct-bank purchase into payable {} and settled on {}",get_id(&payable,"payableId")?,string(&bank,"transaction_date")?),by]).map_err(sql)?;
    #[cfg(test)]
    tests::checkpoint(5)?;
    let row = rows(
        tx,
        "SELECT * FROM direct_bank_purchase_payable_corrections WHERE id=?1",
        [id],
    )?
    .into_iter()
    .next()
    .unwrap();
    row_result(tx, &row, false)
}
pub fn apply(
    tx: &Transaction<'_>,
    i: &ApplyInput,
    subject: &str,
    latest: Date,
    now_ms: i64,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    i.validate()?;
    let mut payload = serde_json::to_value(i).map_err(|_| fail("INVALID_REQUEST"))?;
    payload.as_object_mut().unwrap().remove("idempotencyKey");
    payload.as_object_mut().unwrap().remove("confirm");
    let Some(key) = &i.idempotency_key else {
        return Ok(json!({"ok":true,"correction":execute(tx,i,subject,latest)?}));
    };
    let by = format!("user:{subject}");
    match idempotency::execute(
        tx,
        idempotency::Input {
            key,
            operation: Operation::DirectBankPurchasePayableCorrectionApply,
            principal_kind: PrincipalKind::User,
            subject,
            payload: &payload,
            created_by: &by,
            created_by_program: "direct-bank-purchase-payable-correction",
            now_ms,
        },
        |tx| execute(tx, i, subject, latest),
    )
    .map_err(|f| f.error)?
    {
        Outcome::Accepted { result, receipt } => {
            Ok(json!({"ok":true,"correction":result,"idempotency":receipt}))
        }
        Outcome::Refused { code } => Err(Error::new(
            code,
            "Correction request key refused; read the retained original outcome",
        )),
    }
}
/// Complete current candidates or explicit finite refusal; ordinary bill picker is unchanged.
pub fn candidates(tx: &Transaction<'_>) -> Result<Value, Error> {
    let found = rows(
        tx,
        "SELECT d.id AS documentId,d.document_no AS documentNo,d.invoice_no AS invoiceNo,d.invoice_date AS invoiceDate,d.supplier_name AS supplierName,d.amount_inc_vat AS amountIncVat,d.vat_amount AS vatAmount,d.currency,b.id AS bankTransactionId,b.transaction_date AS bankDate,j.id AS originalJournalEntryId,j.entry_no AS originalJournalEntryNo,b.amount AS bankAmount FROM documents d JOIN journal_entries j ON j.document_id=d.id JOIN bank_transactions b ON b.id=j.source_bank_transaction_id JOIN bank_journal_reconciliations br ON br.bank_transaction_id=b.id AND br.journal_entry_id=j.id WHERE d.document_type IN('purchase_sale','cash_register_receipt') AND d.currency='DKK' AND b.currency='DKK' AND b.amount<0 AND j.status='posted' AND j.reversal_of_entry_id IS NULL AND d.invoice_date IS NOT NULL AND b.transaction_date>=d.invoice_date AND NOT EXISTS(SELECT 1 FROM payables p WHERE p.document_id=d.id) AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) ORDER BY b.transaction_date,d.id,b.id",
        [],
    )?;
    let mut out = vec![];
    for mut row in found {
        let gross = Money::parse(&row["amountIncVat"].to_string());
        let signed = Money::parse(&row["bankAmount"].to_string());
        if let (Ok(g), Ok(b)) = (gross, signed) {
            if g != Money::default().subtract(&b) {
                continue;
            }
        } else {
            continue;
        }
        let pairs:i64=tx.query_row("SELECT count(*) FROM journal_entries WHERE document_id=?1 AND source_bank_transaction_id=?2 AND status='posted' AND reversal_of_entry_id IS NULL",params![get_id(&row,"documentId")?,get_id(&row,"bankTransactionId")?],|r|r.get(0)).map_err(sql)?;
        if pairs != 1 {
            continue;
        }
        if out.len() == 200 {
            return Err(fail("CORRECTION_CANDIDATES_EXCEED_200"));
        }
        row.as_object_mut().unwrap().remove("bankAmount");
        out.push(row)
    }
    Ok(json!(out))
}
/// Audit a single retained correction with the same immutable-reference checks
/// as portable backup validation, without stopping at an earlier damaged row.
pub(crate) fn validate_audit_row(db: &Connection, row: &Value) -> Result<(), Error> {
    row_result(db, row, true).map(|_| ())
}
pub fn validate_backup(db: &Connection) -> Result<(), Error> {
    for row in rows(
        db,
        "SELECT * FROM direct_bank_purchase_payable_corrections ORDER BY id",
        [],
    )? {
        row_result(db, &row, true)?;
    }
    Ok(())
}
