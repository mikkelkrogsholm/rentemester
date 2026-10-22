//! Atomic legacy invoice repair. Errors require rollback by the admitted writer.
use crate::{idempotency::PrincipalKind, invalid};
use rentemester_engine::{Error, fiscal_year::Date, identifiers::JournalEntryId};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};
fn refused(message: impl Into<String>) -> Error {
    Error::new("invoice_repair_refused", message)
}
fn require(ok: bool, message: &str) -> Result<(), Error> {
    if ok { Ok(()) } else { Err(refused(message)) }
}
/// Unforgeable outside this module; each writer rechecks its exact transition.
pub(crate) struct Permit {
    invoice: i64,
    legacy: i64,
    replacement: Option<i64>,
    invoice_date: Date,
    legacy_date: Date,
}
impl Permit {
    pub(crate) fn before_post(&self, tx: &Transaction<'_>, invoice: i64) -> Result<(), Error> {
        require(
            invoice == self.invoice && self.replacement.is_none(),
            "repair posting capability differs",
        )?;
        self.validate(tx)
    }
    pub(crate) fn before_reverse(
        &self,
        tx: &Transaction<'_>,
        journal: i64,
        date: Date,
    ) -> Result<(), Error> {
        require(
            journal == self.legacy && date == self.legacy_date && self.replacement.is_some(),
            "repair reversal capability differs",
        )?;
        self.validate(tx)
    }
    fn validate(&self, tx: &Transaction<'_>) -> Result<(), Error> {
        let invoice:Option<(String,String,String)>=tx.query_row("SELECT invoice_date,status,payload_json FROM documents WHERE id=?1 AND document_type='issued_invoice' AND invoice_no IS NOT NULL AND invoice_no<>''",[self.invoice],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(invalid)?;
        let (date, status, payload) =
            invoice.ok_or_else(|| refused("issued invoice with immutable payload required"))?;
        require(
            matches!(status.as_str(), "issued" | "open")
                && Date::parse(&date)? == self.invoice_date,
            "invoice must remain issued/open on its original date",
        )?;
        let payload: Value = serde_json::from_str(&payload).map_err(invalid)?;
        rentemester_engine::sales_invoice::project(&payload)?;
        require(payload["issueDate"] == date, "invoice payload date differs")?;
        let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_entries j WHERE j.id=?1 AND j.document_id=?2 AND j.transaction_date=?3 AND j.locked=1 AND j.status='posted' AND j.reversal_of_entry_id IS NULL AND j.source_bank_transaction_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id))",params![self.legacy,self.invoice,self.legacy_date.to_string()],|r|r.get(0)).map_err(invalid)?;
        require(
            valid,
            "legacy journal must be the locked active original unbanked invoice entry",
        )?;
        let canonical: Option<i64> = tx
            .query_row(
                "SELECT journal_entry_id FROM issued_invoice_postings WHERE invoice_document_id=?1",
                [self.invoice],
                |r| r.get(0),
            )
            .optional()
            .map_err(invalid)?;
        require(
            canonical == self.replacement,
            "invoice canonical posting differs from repair transition",
        )?;
        if let Some(replacement) = self.replacement {
            let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_entries j WHERE j.id=?1 AND j.document_id=?2 AND j.transaction_date=?3 AND j.status='posted' AND j.reversal_of_entry_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id))",params![replacement,self.invoice,self.invoice_date.to_string()],|r|r.get(0)).map_err(invalid)?;
            require(
                valid && replacement != self.legacy,
                "replacement must be the active canonical invoice entry",
            )?;
        }
        for table in [
            "invoice_payments",
            "invoice_refunds",
            "invoice_claim_payments",
            "invoice_reminders",
            "invoice_compensation_claims",
            "invoice_interest_claims",
            "invoice_interest_corrections",
            "invoice_bad_debt_writeoffs",
        ] {
            let exists: bool = tx
                .query_row(
                    &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE invoice_document_id=?1)"),
                    [self.invoice],
                    |r| r.get(0),
                )
                .map_err(invalid)?;
            require(
                !exists,
                "invoice has downstream evidence; repair requires an unused invoice",
            )?;
        }
        let credits:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM credit_note_postings WHERE original_invoice_document_id=?1) OR EXISTS(SELECT 1 FROM documents c JOIN documents i ON i.id=?1 WHERE c.document_type='credit_note' AND c.payment_details=i.invoice_no)",[self.invoice],|r|r.get(0)).map_err(invalid)?;
        require(!credits, "invoice has downstream credit evidence")?;
        // Inspect the actual native FK contract, including extensions. Journal lines are the
        // entry's own evidence; journal self references are checked explicitly above.
        let tables: Vec<String> = tx
            .prepare(
                "SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            )
            .map_err(invalid)?
            .query_map([], |r| r.get(0))
            .map_err(invalid)?
            .collect::<Result<_, _>>()
            .map_err(invalid)?;
        for table in tables {
            if table == "journal_lines" || table == "journal_entries" {
                continue;
            }
            let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
            let columns:Vec<String>=tx.prepare("SELECT \"from\" FROM pragma_foreign_key_list(?1) WHERE \"table\"='journal_entries'").map_err(invalid)?.query_map([&table],|r|r.get(0)).map_err(invalid)?.collect::<Result<_,_>>().map_err(invalid)?;
            for column in columns {
                let used: bool = tx
                    .query_row(
                        &format!(
                            "SELECT EXISTS(SELECT 1 FROM {} WHERE {}=?1)",
                            quote(&table),
                            quote(&column)
                        ),
                        [self.legacy],
                        |r| r.get(0),
                    )
                    .map_err(invalid)?;
                require(!used, "legacy journal has inbound classified evidence")?;
            }
        }
        let active:Vec<i64>=tx.prepare("SELECT j.id FROM journal_entries j WHERE j.document_id=?1 AND j.status='posted' AND j.reversal_of_entry_id IS NULL AND NOT EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) ORDER BY j.id").map_err(invalid)?.query_map([self.invoice],|r|r.get(0)).map_err(invalid)?.collect::<Result<_,_>>().map_err(invalid)?;
        let mut expected = vec![self.legacy];
        if let Some(id) = self.replacement {
            expected.push(id);
            expected.sort_unstable();
        }
        require(active == expected, "invoice has other active journals")?;
        for date in [self.invoice_date, self.legacy_date] {
            let mut q=tx.prepare("SELECT id,status FROM accounting_periods WHERE period_start<=?1 AND period_end>=?1 AND status IN ('closed','reported') ORDER BY period_end,id").map_err(invalid)?;
            for p in q
                .query_map([date.to_string()], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(invalid)?
            {
                let (id, status) = p.map_err(invalid)?;
                require(
                    crate::periods::effective(tx, id, status)? == "open",
                    "invoice or legacy journal date is in a closed/reported period",
                )?;
            }
        }
        Ok(())
    }
}
/// No independent transaction or identity: called only inside the native lifecycle owner.
pub fn repair(
    tx: &Transaction<'_>,
    invoice: i64,
    body: &Value,
    subject: &str,
    principal: PrincipalKind,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    require(
        body["confirm"] == true,
        "explicit repair confirmation required",
    )?;
    let legacy = body["legacyJournalEntryId"]
        .as_i64()
        .ok_or_else(|| refused("legacy journal required"))?;
    let reason = body["reason"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| refused("repair reason required"))?;
    let revenue = body["revenueAccountNo"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| refused("explicit revenue account required"))?;
    let invoice_date: String = tx
        .query_row(
            "SELECT invoice_date FROM documents WHERE id=?1",
            [invoice],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let legacy_date: String = tx
        .query_row(
            "SELECT transaction_date FROM journal_entries WHERE id=?1",
            [legacy],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let mut permit = Permit {
        invoice,
        legacy,
        replacement: None,
        invoice_date: Date::parse(&invoice_date)?,
        legacy_date: Date::parse(&legacy_date)?,
    };
    permit.validate(tx)?;
    let replacement = crate::sales::post_for_repair(
        tx,
        &crate::sales::Post {
            invoice_document_id: invoice,
            revenue_account_no: revenue.trim().into(),
            confirm: true,
        },
        subject,
        latest,
        principal,
        &permit,
    )?;
    require(
        replacement["ok"] == true,
        "native replacement posting refused",
    )?;
    let id = replacement["entryId"]
        .as_i64()
        .ok_or_else(|| refused("replacement receipt unavailable"))?;
    permit.replacement = Some(id);
    permit.validate(tx)?;
    let reversal = crate::journal_reversal::reverse_repaired(
        tx,
        &crate::journal_reversal::Input {
            entry_id: JournalEntryId::from_sqlite(legacy),
            transaction_date: permit.legacy_date,
            reason: reason.into(),
        },
        &crate::journal_preview::manual_journal_rule_version()?,
        latest,
        crate::journal_post::Actor {
            subject: &format!("user:{subject}"),
            program: match principal {
                PrincipalKind::User => "rentemester-rust",
                PrincipalKind::ServiceAccount => "service-principal",
            },
        },
        &permit,
    )?;
    require(reversal["ok"] == true, "native legacy mirror refused")?;
    crate::audit_invoice::payment_evidence(tx, invoice)?;
    // Includes exact principal/carrying validation after the old journal and mirror cancel.
    let status = crate::invoice_original::read(
        tx,
        crate::invoice_original::Kind::Status,
        &crate::invoice_original::Query {
            document_id: Some(invoice),
            ..Default::default()
        },
        &latest.to_string(),
    )?;
    require(
        status["ok"] == true,
        "repaired invoice principal evidence unavailable",
    )?;
    let reversal_id = reversal["entryId"]
        .as_i64()
        .ok_or_else(|| refused("reversal receipt unavailable"))?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('invoice_booking_repair','document',?1,?2,?3)",params![invoice.to_string(),json!({"reason":reason,"invoiceDocumentId":invoice,"legacyJournalEntryId":legacy,"replacementJournalEntryId":id,"reversalJournalEntryId":reversal_id}).to_string(),format!("{}:{subject}",match principal{PrincipalKind::User=>"user",PrincipalKind::ServiceAccount=>"service_account"})]).map_err(invalid)?;
    let mut rules = vec![];
    for value in [&replacement, &reversal] {
        if let Some(items) = value["appliedRules"].as_array() {
            for item in items {
                if !rules.contains(item) {
                    rules.push(item.clone());
                }
            }
        }
    }
    for rule in [
        "DK-INVOICE-BOOKKEEPING-001",
        "DK-INVOICE-BOOKKEEPING-REVERSE-002",
    ] {
        let value = json!(rule);
        if !rules.contains(&value) {
            rules.push(value);
        }
    }
    Ok(
        json!({"invoiceDocumentId":invoice,"legacyJournalEntryId":legacy,"replacementJournalEntryId":id,"replacementJournalEntryNo":replacement["entryNo"],"reversalJournalEntryId":reversal_id,"reversalJournalEntryNo":reversal["entryNo"],"entryId":id,"entryNo":replacement["entryNo"],"entryHash":replacement["entryHash"],"appliedRules":rules}),
    )
}
#[cfg(test)]
mod tests;
