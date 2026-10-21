//! Voucher registration and display on caller-owned, authorized transactions.
use rentemester_engine::{
    Error,
    document::Metadata,
    fiscal_year::{self, Date, LabelStrategy, StartMonth},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};
fn fail(message: impl Into<String>) -> Error {
    Error::new("document_refused", message)
}
fn sql(_: rusqlite::Error) -> Error {
    fail("document storage unavailable; abort transaction")
}
pub struct Original<'a> {
    pub filename: &'a str,
    pub hash: &'a str,
    pub path: &'a str,
    pub mime: &'a str,
}
pub fn register(
    tx: &Transaction<'_>,
    original: Original<'_>,
    metadata: &Metadata,
    actor: &str,
    today: &str,
) -> Result<Value, Error> {
    register_scanned(tx, original, metadata, actor, today, None)
}
pub fn register_scanned(
    tx: &Transaction<'_>,
    original: Original<'_>,
    metadata: &Metadata,
    actor: &str,
    today: &str,
    scan: Option<&rentemester_engine::document_scan::ScanEvidence>,
) -> Result<Value, Error> {
    register_scanned_with_principal(
        tx,
        original,
        metadata,
        actor,
        today,
        scan,
        crate::idempotency::PrincipalKind::User,
    )
}
pub fn register_scanned_with_principal(
    tx: &Transaction<'_>,
    original: Original<'_>,
    metadata: &Metadata,
    actor: &str,
    today: &str,
    scan: Option<&rentemester_engine::document_scan::ScanEvidence>,
    principal: crate::idempotency::PrincipalKind,
) -> Result<Value, Error> {
    if let Some(scan) = scan {
        scan.validate()?;
        if scan.sha256 != original.hash {
            return Err(fail(
                "clean scanner evidence must bind the exact document original",
            ));
        }
    }
    crate::validate(tx)?;
    let identity = metadata.validate()?;
    let exists: Option<String> = tx
        .query_row(
            "SELECT document_no FROM documents WHERE sha256_hash=?1",
            [original.hash],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?;
    if let Some(number) = exists {
        return Err(fail(format!(
            "duplicate document content already ingested as {number}"
        )));
    }
    let identifier = identity
        .as_ref()
        .and_then(|i| i.identifier.as_deref())
        .or_else(|| {
            metadata
                .sender
                .as_ref()
                .and_then(|p| p.vat_or_cvr.as_deref())
        });
    if metadata.kind() == "purchase_sale"
        && let (Some(identifier), Some(invoice), Some(date)) = (
            identifier,
            metadata.invoice_no.as_deref(),
            metadata.issue_date.as_deref(),
        )
    {
        let exists:Option<String>=tx.query_row("SELECT document_no FROM documents WHERE document_type='purchase_sale' AND sender_vat_cvr=?1 AND invoice_no=?2 AND invoice_date=?3",[identifier,invoice,date],|r|r.get(0)).optional().map_err(sql)?;
        if exists.is_some() {
            return Err(fail(
                "supplier invoice identity already registered; human review required",
            ));
        }
    }
    if metadata.kind() == "internal_voucher" {
        let bank_id = metadata
            .source_bank_transaction_id
            .ok_or_else(|| fail("bank evidence required"))?;
        type BankEvidence = (
            String,
            f64,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let row:Option<BankEvidence>=tx.query_row("SELECT transaction_date,amount,currency,transaction_hash,source_file_hash,import_batch_id FROM bank_transactions WHERE id=?1",[bank_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional().map_err(sql)?;
        let (date, amount, currency, hash, file, batch) =
            row.ok_or_else(|| fail("bank evidence does not exist"))?;
        let gross = metadata
            .amount_inc_vat
            .as_ref()
            .map(rentemester_engine::document::money)
            .transpose()?
            .ok_or_else(|| fail("voucher gross amount required"))?;
        if amount >= 0.0
            || date != metadata.issue_date.as_deref().unwrap_or("")
            || currency != metadata.currency()
            || gross != crate::legacy_money(rusqlite::types::ValueRef::Real(-amount))?
            || hash.is_none() && (file.is_none() || batch.is_none())
        {
            return Err(fail(
                "internal voucher must match an outgoing bank row with stable import evidence",
            ));
        }
    }
    let (cvr,month,strategy):(Option<String>,i32,String)=tx.query_row("SELECT cvr,fiscal_year_start_month,fiscal_year_label_strategy FROM companies WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(sql)?;
    let basis = metadata.issue_date.as_deref().unwrap_or(today);
    let label = fiscal_year::for_date(
        Date::parse(basis)?,
        StartMonth::new(month)?,
        LabelStrategy::try_from(strategy.as_str())?,
    )?
    .identifier_label;
    let prefix = format!("DOC-{label}-");
    let pattern = format!("{prefix}[0-9][0-9][0-9][0-9][0-9][0-9]");
    let floor:i64=tx.query_row("SELECT COALESCE(MAX(CAST(substr(document_no,-6) AS INTEGER)),0) FROM documents WHERE document_no GLOB ?1",[pattern],|r|r.get(0)).map_err(sql)?;
    let scope = crate::sequences::company_scope(cvr.as_deref(), 1, &label);
    let value = crate::sequences::next(tx, "document", &scope, floor)?;
    let number = format!("{prefix}{value:06}");
    let sender = metadata.sender.as_ref();
    let recipient = metadata.recipient.as_ref();
    let retention = rentemester_engine::calendar::retention_deadline(basis, month as u32)?;
    let payload = serde_json::to_string(metadata).map_err(|_| fail("metadata encoding failed"))?;
    let id:i64=tx.query_row("INSERT INTO documents(document_no,source,original_filename,stored_path,mime_type,sha256_hash,supplier_name,invoice_no,invoice_date,amount_inc_vat,currency,document_type,delivery_description,sender_name,sender_address,sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status,recipient_name,recipient_address,recipient_vat_cvr,vat_amount,payment_details,exemption_code,payload_json,retain_until) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?7,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26) RETURNING id",params![number,metadata.source,original.filename,original.path,original.mime,original.hash,sender.and_then(|p|p.name.as_deref()),metadata.invoice_no,metadata.issue_date,metadata.amount_inc_vat.as_ref().and_then(|n|n.as_f64()),metadata.currency(),metadata.kind(),metadata.delivery_description,sender.and_then(|p|p.address.as_deref()),identifier,identity.as_ref().map(|i|i.country.as_str()),identity.as_ref().map(|i|i.kind.as_str()),identity.as_ref().map(|_|"resolved"),recipient.and_then(|p|p.name.as_deref()),recipient.and_then(|p|p.address.as_deref()),recipient.and_then(|p|p.vat_or_cvr.as_deref()),metadata.vat_amount.as_ref().and_then(|n|n.as_f64()),metadata.payment_details,metadata.exemption_code,payload,retention],|r|r.get(0)).map_err(sql)?;
    if let Some(scan) = scan {
        tx.execute("INSERT INTO document_scan_evidence(document_id,sha256_hash,scanner_id,scanner_version,result,evidence_ref) VALUES(?1,?2,?3,?4,'clean',?5)",params![id,scan.sha256,scan.scanner_id,scan.scanner_version,scan.evidence_ref]).map_err(sql)?;
    }
    if metadata.kind() == "internal_voucher" {
        tx.execute("INSERT INTO internal_voucher_evidence(document_id,bank_transaction_id,accounting_rationale,prepared_by,prepared_by_program) VALUES(?1,?2,?3,?4,'rentemester')",params![id,metadata.source_bank_transaction_id,metadata.accounting_rationale,actor]).map_err(sql)?;
    }
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('document_ingest','document',?1,?2,?3)",params![id.to_string(),format!("Ingested {number} ({})",original.hash),format!("{}:{actor}", match principal { crate::idempotency::PrincipalKind::User => "user", crate::idempotency::PrincipalKind::ServiceAccount => "service_account" })]).map_err(sql)?;
    Ok(json!({"ok":true,"document":{"id":id,"documentNo":number}}))
}
pub struct Evidence {
    pub path: String,
    pub hash: String,
    pub filename: String,
    pub mime: String,
}
pub fn evidence(tx: &Transaction<'_>, id: i64) -> Result<Evidence, Error> {
    tx.query_row(
        "SELECT stored_path,sha256_hash,original_filename,mime_type FROM documents WHERE id=?1",
        [id],
        |r| {
            Ok(Evidence {
                path: r.get(0)?,
                hash: r.get(1)?,
                filename: r.get(2)?,
                mime: r.get(3)?,
            })
        },
    )
    .map_err(sql)
}
pub fn list(tx: &Transaction<'_>, slug: &str) -> Result<Value, Error> {
    crate::validate(tx)?;
    let company=tx.query_row("SELECT name,cvr,country,currency,fiscal_year_start_month,fiscal_year_label_strategy FROM companies WHERE id=1",[],|r|Ok(json!({"name":r.get::<_,String>(0)?,"cvr":r.get::<_,Option<String>>(1)?,"country":r.get::<_,String>(2)?,"currency":r.get::<_,String>(3)?,"fiscalYearStartMonth":r.get::<_,i32>(4)?,"fiscalYearLabelStrategy":r.get::<_,String>(5)?}))).map_err(sql)?;
    // Imported links are additive evidence; prefer their voucher reference when
    // a direct association names the same pair, without duplicating that row.
    let mut stmt=tx.prepare("WITH associations AS (
        SELECT je.document_id AS document_id,je.id AS journal_entry_id,NULL AS voucher_ref
        FROM journal_entries je
        WHERE je.document_id IS NOT NULL AND NOT EXISTS(
            SELECT 1 FROM import_document_links imported
            WHERE imported.document_id=je.document_id AND imported.journal_entry_id=je.id
        )
        UNION ALL
        SELECT document_id,journal_entry_id,voucher_ref FROM import_document_links
    ) SELECT d.id,d.document_no,d.source,d.original_filename,d.document_type,d.supplier_name,d.sender_vat_cvr,d.supplier_country_code,d.supplier_identifier_kind,d.supplier_identity_status,d.invoice_no,d.invoice_date,d.amount_inc_vat,d.currency,d.status,d.stored_path,d.payload_json,je.id,je.entry_no,je.text,(SELECT sum(debit_amount) FROM journal_lines WHERE journal_entry_id=je.id),ive.bank_transaction_id,ive.accounting_rationale,ive.prepared_by,ive.prepared_by_program,d.sha256_hash,d.mime_type,d.retain_until,association.voucher_ref FROM documents d LEFT JOIN associations association ON association.document_id=d.id LEFT JOIN journal_entries je ON je.id=association.journal_entry_id LEFT JOIN internal_voucher_evidence ive ON ive.document_id=d.id ORDER BY d.id DESC,je.id").map_err(sql)?;
    let mut rows = stmt.query([]).map_err(sql)?;
    let mut documents = Vec::new();
    let mut linked = std::collections::HashSet::new();
    let mut unlinked = std::collections::HashSet::new();
    while let Some(r) = rows.next().map_err(sql)? {
        let id: i64 = r.get(0).map_err(sql)?;
        let journal: Option<i64> = r.get(17).map_err(sql)?;
        if journal.is_some() {
            linked.insert(id);
        } else {
            unlinked.insert(id);
        }
        let payload = r
            .get::<_, Option<String>>(16)
            .map_err(sql)?
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .unwrap_or(Value::Null);
        documents.push(json!({"id":id,"documentNo":r.get::<_,Option<String>>(1).map_err(sql)?,"source":r.get::<_,String>(2).map_err(sql)?,"filename":r.get::<_,Option<String>>(3).map_err(sql)?,"documentType":r.get::<_,String>(4).map_err(sql)?,"supplierName":r.get::<_,Option<String>>(5).map_err(sql)?,"supplierVatOrCvr":r.get::<_,Option<String>>(6).map_err(sql)?,"supplierCountryCode":r.get::<_,Option<String>>(7).map_err(sql)?,"supplierIdentifierKind":r.get::<_,Option<String>>(8).map_err(sql)?,"supplierIdentityStatus":r.get::<_,Option<String>>(9).map_err(sql)?,"invoiceNo":r.get::<_,Option<String>>(10).map_err(sql)?,"invoiceDate":r.get::<_,Option<String>>(11).map_err(sql)?,"amountIncVat":r.get::<_,Option<f64>>(12).map_err(sql)?,"currency":r.get::<_,String>(13).map_err(sql)?,"status":r.get::<_,String>(14).map_err(sql)?,"hasFile":r.get::<_,Option<String>>(15).map_err(sql)?.is_some(),"journalEntryId":journal,"journalEntryNo":r.get::<_,Option<String>>(18).map_err(sql)?,"journalEntryText":r.get::<_,Option<String>>(19).map_err(sql)?,"journalEntryTotal":r.get::<_,Option<f64>>(20).map_err(sql)?,"sourceBankTransactionId":r.get::<_,Option<i64>>(21).map_err(sql)?,"accountingRationale":r.get::<_,Option<String>>(22).map_err(sql)?,"preparedBy":r.get::<_,Option<String>>(23).map_err(sql)?,"preparedByProgram":r.get::<_,Option<String>>(24).map_err(sql)?,"sha256Hash":r.get::<_,String>(25).map_err(sql)?,"mimeType":r.get::<_,Option<String>>(26).map_err(sql)?,"retainUntil":r.get::<_,Option<String>>(27).map_err(sql)?,"voucherRef":r.get::<_,Option<String>>(28).map_err(sql)?,"purchaseVatLines":payload["purchaseVatLines"]}));
    }
    Ok(
        json!({"slug":slug,"company":company,"documents":documents,"linkedCount":linked.len(),"unlinkedCount":unlinked.len()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_registration_retains_evidence_and_refuses_duplicate_without_consuming_number() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut company = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic Documents")
            .unwrap();
        let metadata: Metadata = serde_json::from_value(json!({"source":"synthetic","documentType":"cash_register_receipt","issueDate":"2026-10-18","amountIncVat":250,"vatAmount":50})).unwrap();
        let hash = "a".repeat(64);
        let path = format!("documents/originals/{hash}.txt");
        let original = || Original {
            filename: "synthetic.txt",
            hash: &hash,
            path: &path,
            mime: "text/plain",
        };
        let tx = company
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let registered =
            register(&tx, original(), &metadata, "synthetic-owner", "2026-10-18").unwrap();
        assert_eq!(registered["document"]["documentNo"], "DOC-2026-000001");
        assert!(
            register(&tx, original(), &metadata, "synthetic-owner", "2026-10-18")
                .unwrap_err()
                .message
                .contains("duplicate document content")
        );
        tx.commit().unwrap();
        let tx = company
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let listed = list(&tx, "synthetic").unwrap();
        assert_eq!(listed["documents"].as_array().unwrap().len(), 1);
        assert_eq!(listed["unlinkedCount"], 1);
        assert_eq!(listed["documents"][0]["hasFile"], true);
        assert_eq!(listed["documents"][0]["sha256Hash"], hash);
        assert_eq!(listed["documents"][0]["mimeType"], "text/plain");
        assert_eq!(listed["documents"][0]["retainUntil"], "2031-12-31");
        assert_eq!(evidence(&tx, 1).unwrap().path, path);
        let (retain, actor): (String, String) = tx.query_row("SELECT retain_until,(SELECT actor FROM audit_log WHERE event_type='document_ingest') FROM documents WHERE id=1", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(retain, "2031-12-31");
        assert_eq!(actor, "user:synthetic-owner");
    }
    fn association_snapshot(db: &rusqlite::Connection) -> Value {
        let journals:String=db.query_row("SELECT json_group_array(json_object('id',id,'documentId',document_id,'entryHash',entry_hash,'previousHash',previous_hash)) FROM (SELECT * FROM journal_entries ORDER BY id)",[],|r|r.get(0)).unwrap();
        let documents:String=db.query_row("SELECT json_group_array(json_object('id',id,'documentNo',document_no,'hash',sha256_hash,'path',stored_path,'type',document_type)) FROM (SELECT * FROM documents ORDER BY id)",[],|r|r.get(0)).unwrap();
        let links:String=db.query_row("SELECT json_group_array(json_object('documentId',document_id,'journalId',journal_entry_id,'voucherRef',voucher_ref,'source',source_system)) FROM (SELECT * FROM import_document_links ORDER BY id)",[],|r|r.get(0)).unwrap();
        json!({"journals":journals,"documents":documents,"links":links})
    }
    #[test]
    fn additive_import_associations_preserve_direct_links_exact_refs_and_read_only_counts() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut company = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(
            &mut control,
            &mut company,
            "Synthetic linked documents",
        )
        .unwrap();
        let tx = company
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let mut docs = vec![];
        for i in 1..=4 {
            let hash = format!("{i:064x}");
            let path = format!("documents/originals/{hash}.pdf");
            let metadata=serde_json::from_value(json!({"source":"dinero-import-bilag","documentType":"cash_register_receipt","issueDate":"2026-10-18"})).unwrap();
            let value = register(
                &tx,
                Original {
                    filename: "synthetic.pdf",
                    hash: &hash,
                    path: &path,
                    mime: "application/pdf",
                },
                &metadata,
                "synthetic",
                "2026-10-18",
            )
            .unwrap();
            docs.push(value["document"]["id"].as_i64().unwrap());
        }
        for (no, kind, balance) in [("2100", "asset", "debit"), ("5100", "equity", "credit")] {
            crate::company_setup::create_account(
                &tx,
                &crate::company_setup::Account {
                    account_no: no.into(),
                    name: format!("Synthetic {no}"),
                    r#type: kind.into(),
                    normal_balance: balance.into(),
                    default_vat_code: None,
                },
                "synthetic",
            )
            .unwrap();
        }
        let mut journals = vec![];
        for document in [Some(docs[0]), None, Some(docs[1]), None] {
            let request=serde_json::from_value(json!({"transactionDate":"2026-10-18","text":"Synthetic association journal","currency":"DKK","documentId":document,"lines":[{"accountNo":"2100","debitAmount":"10.00"},{"accountNo":"5100","creditAmount":"10.00"}]})).unwrap();
            let result = crate::journal_post::post(
                &tx,
                &request,
                &crate::journal_preview::manual_journal_rule_version().unwrap(),
                Date::parse("2026-10-21").unwrap(),
                crate::journal_post::Actor {
                    subject: "synthetic",
                    program: "rentemester-rust",
                },
            )
            .unwrap();
            assert_eq!(result["ok"], true, "{result}");
            journals.push(result["entryId"].as_i64().unwrap());
        }
        // Link rows are the importer additive boundary. Inserting one never
        // changes the immutable journal's document_id or entry hash.
        let before_links = association_snapshot(&tx);
        for (document, journal, voucher) in [
            (docs[0], journals[0], "0007"),
            (docs[0], journals[1], "8-β"),
            (docs[2], journals[1], "1"),
            (docs[2], journals[3], "0010"),
        ] {
            tx.execute("INSERT INTO import_document_links(source_system,voucher_ref,document_id,journal_entry_id)VALUES('dinero',?1,?2,?3)",params![voucher,document,journal]).unwrap();
        }
        let after_links = association_snapshot(&tx);
        assert_eq!(after_links["journals"], before_links["journals"]);
        assert_eq!(after_links["documents"], before_links["documents"]);
        assert!(crate::journal_hash::verify(&tx).unwrap().ok);
        tx.commit().unwrap();
        company.pragma_update(None, "query_only", true).unwrap();
        let tx = company
            .transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)
            .unwrap();
        let before = association_snapshot(&tx);
        let changes = tx.total_changes();
        let listed = list(&tx, "synthetic").unwrap();
        assert_eq!(listed["linkedCount"], 3);
        assert_eq!(listed["unlinkedCount"], 1);
        let rows = listed["documents"].as_array().unwrap();
        assert_eq!(rows.len(), 6);
        let related = |id: i64| rows.iter().filter(|r| r["id"] == id).collect::<Vec<_>>();
        let mixed = related(docs[0]);
        assert_eq!(mixed.len(), 2);
        assert!(mixed.iter().all(|r| r["journalEntryId"].is_i64()));
        let same = mixed
            .iter()
            .filter(|r| r["journalEntryId"] == journals[0])
            .collect::<Vec<_>>();
        assert_eq!(same.len(), 1);
        assert_eq!(same[0]["voucherRef"], "0007");
        assert_eq!(
            mixed
                .iter()
                .find(|r| r["journalEntryId"] == journals[1])
                .unwrap()["voucherRef"],
            "8-β"
        );
        let direct = related(docs[1]);
        assert_eq!(direct.len(), 1);
        assert_eq!(direct[0]["journalEntryId"], journals[2]);
        assert_eq!(direct[0]["voucherRef"], Value::Null);
        let imported = related(docs[2]);
        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0]["voucherRef"], "1");
        assert_eq!(imported[1]["voucherRef"], "0010");
        assert!(imported.iter().all(|r| r["journalEntryNo"].is_string()
            && r["journalEntryText"] == "Synthetic association journal"
            && r["journalEntryTotal"].as_f64() == Some(10.0)));
        let unlinked = related(docs[3]);
        assert_eq!(unlinked.len(), 1);
        assert_eq!(unlinked[0]["journalEntryId"], Value::Null);
        assert_eq!(unlinked[0]["voucherRef"], Value::Null);
        assert_eq!(association_snapshot(&tx), before);
        assert_eq!(tx.total_changes(), changes);
        assert!(crate::journal_hash::verify(&tx).unwrap().ok);
        tx.rollback().unwrap();
    }
}
