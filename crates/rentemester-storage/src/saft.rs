//! Bounded v3 SAF-T package from one admitted company transaction. Paths are evidence only.
use crate::invalid;
use rentemester_engine::{
    Error,
    document::{Metadata, Party},
    saft::{self, Source, amount, refused},
    trial_balance::Period,
};
use rusqlite::{ToSql, Transaction, params, types::ValueRef};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub period_start: String,
    pub period_end: String,
    pub confirm: bool,
}
pub struct Download {
    pub bytes: Vec<u8>,
    pub journal_entry_count: usize,
    pub document_count: usize,
}
pub struct Prepared {
    pub source: Source,
    pub originals: Vec<crate::documents::Evidence>,
    pub documents: Vec<Value>,
    pub audit: Vec<Value>,
    pub trial_balance: Value,
    pub chain: Value,
    pub slug: String,
    pub start: String,
    pub end: String,
    pub generated: String,
}
fn rows(tx: &Transaction<'_>, sql: &str, args: &[&dyn ToSql]) -> Result<Vec<Value>, Error> {
    let mut q = tx.prepare(sql).map_err(invalid)?;
    let names = q
        .column_names()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut rs = q.query(args).map_err(invalid)?;
    let mut output = vec![];
    while let Some(r) = rs.next().map_err(invalid)? {
        let mut object = serde_json::Map::new();
        for (i, name) in names.iter().enumerate() {
            let v = match r.get_ref(i).map_err(invalid)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Real(n) => serde_json::Number::from_f64(n)
                    .map(Value::Number)
                    .ok_or_else(|| refused("nonfinite persisted export value"))?,
                ValueRef::Text(s) => json!(std::str::from_utf8(s).map_err(invalid)?),
                ValueRef::Blob(_) => {
                    return Err(refused("unexpected binary field in export projection"));
                }
            };
            object.insert(name.clone(), v);
        }
        output.push(Value::Object(object));
    }
    Ok(output)
}

fn payload(row: &Value) -> Result<Value, Error> {
    match row["payloadJson"].as_str() {
        None => Ok(json!({})),
        Some(s) => {
            let v: Value = serde_json::from_str(s).map_err(invalid)?;
            if !v.is_object() {
                return Err(refused("document payload must be a metadata object"));
            }
            Ok(v)
        }
    }
}
pub fn prepare(
    tx: &Transaction<'_>,
    slug: &str,
    input: &Input,
    now_ms: i64,
) -> Result<Prepared, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm {
        return Err(refused("export confirmation required"));
    }
    let period = Period::new(&input.period_start, &input.period_end)?;
    if slug.is_empty()
        || !slug
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(refused("invalid company slug"));
    }
    let generated = saft::timestamp(now_ms)?;
    let chain = crate::journal_hash::verify(tx)?;
    if !chain.ok {
        return Err(refused(
            "journal hash chain verification failed; export refused",
        ));
    }
    let trial_balance = crate::trial_balance_owned(tx, Period::new(&period.start, &period.end)?)?;
    if !trial_balance.balanced {
        return Err(refused("selected trial balance is unbalanced"));
    }
    let trial_balance = serde_json::to_value(trial_balance).map_err(invalid)?;
    let chain = serde_json::to_value(chain).map_err(invalid)?;

    let company=rows(tx,"SELECT id,name,country,currency,cvr,fiscal_year_start_month AS fiscalYearStartMonth,fiscal_year_label_strategy AS fiscalYearLabelStrategy FROM companies ORDER BY id LIMIT 1",&[])?.into_iter().next().ok_or_else(||refused("company row required"))?;
    if company["cvr"].as_str().is_none_or(str::is_empty) {
        return Err(refused("company cvr is required for SAF-T export"));
    }
    let accounts = rows(
        tx,
        "SELECT account_no AS accountNo,name,type,normal_balance AS normalBalance,default_vat_code AS defaultVatCode FROM accounts ORDER BY account_no",
        &[],
    )?;
    let customers = rows(
        tx,
        "SELECT id AS customerId,name,vat_or_cvr AS vatOrCvr,address,email,phone,coalesce(default_currency,'DKK') AS defaultCurrency,coalesce(payment_terms_days,30) AS paymentTermsDays FROM customers ORDER BY id",
        &[],
    )?;
    let suppliers = rows(
        tx,
        "SELECT id AS supplierId,name,vat_or_cvr AS vatOrCvr,country_code AS countryCode,identifier_kind AS identifierKind,identity_status AS identityStatus,address,email,phone,default_expense_account AS defaultExpenseAccount,default_vat_treatment AS defaultVatTreatment FROM vendors ORDER BY id",
        &[],
    )?;
    let args: &[&dyn ToSql] = &[&period.start, &period.end];
    let mut journal = rows(
        tx,
        "SELECT je.id,je.entry_no AS entryNo,je.transaction_date AS transactionDate,je.registration_datetime AS registrationDatetime,je.text,je.currency,je.amount_dkk AS amountDkk,je.document_id AS documentId,d.document_no AS documentNo,je.entry_hash AS hash,je.previous_hash AS prevHash,je.created_by AS createdBy,je.created_by_program AS createdByProgram,je.rule_version AS ruleVersion,je.status FROM journal_entries je LEFT JOIN documents d ON d.id=je.document_id WHERE je.transaction_date BETWEEN ?1 AND ?2 ORDER BY je.transaction_date,je.id",
        args,
    )?;
    let mut ids = BTreeSet::new();
    for row in &mut journal {
        let id = row["id"]
            .as_i64()
            .ok_or_else(|| refused("invalid journal identity"))?;
        row["lines"] = json!(rows(
            tx,
            "SELECT a.account_no AS accountNo,a.name AS accountName,jl.debit_amount AS debitAmount,jl.credit_amount AS creditAmount,jl.vat_code AS vatCode,jl.text FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id WHERE jl.journal_entry_id=?1 ORDER BY jl.id",
            &[&id]
        )?);
        if let Some(id) = row["documentId"].as_i64() {
            ids.insert(id);
        }
    }
    let mut sales = rows(
        tx,
        "SELECT id,document_no AS documentNo,invoice_no AS invoiceNo,invoice_date AS issueDate,coalesce(currency,'DKK') AS currency,sender_name AS sellerName,sender_vat_cvr AS sellerVatOrCvr,recipient_name AS buyerName,recipient_vat_cvr AS buyerVatOrCvr,amount_inc_vat AS grossAmount,vat_amount AS vatAmount,payload_json AS payloadJson FROM documents WHERE document_type='issued_invoice' AND invoice_date BETWEEN ?1 AND ?2 ORDER BY invoice_date,id",
        args,
    )?;
    for row in &mut sales {
        let p = payload(row)?;
        row["dueDate"] = p["dueDate"].clone();
        row["netAmount"] = p["totals"]["netAmount"].clone();
        if !p["totals"]["grossAmount"].is_null() {
            row["grossAmount"] = p["totals"]["grossAmount"].clone();
        }
        if p.get("lines")
            .is_some_and(|v| !v.is_null() && !v.is_array())
        {
            return Err(refused("issued invoice payload lines must be an array"));
        }
        row["lines"] = p["lines"].clone();
        ids.insert(
            row["id"]
                .as_i64()
                .ok_or_else(|| refused("invalid invoice identity"))?,
        );
    }
    let mut purchases = rows(
        tx,
        "SELECT id,document_no AS documentNo,invoice_no AS invoiceNo,invoice_date AS invoiceDate,supplier_name AS supplierName,sender_vat_cvr AS supplierVatOrCvr,supplier_country_code AS supplierCountryCode,supplier_identifier_kind AS supplierIdentifierKind,supplier_identity_status AS supplierIdentityStatus,coalesce(currency,'DKK') AS currency,amount_inc_vat AS grossAmount,vat_amount AS vatAmount,payload_json AS payloadJson FROM documents WHERE document_type='purchase_sale' AND invoice_date BETWEEN ?1 AND ?2 ORDER BY invoice_date,document_no,id",
        args,
    )?;
    for row in &mut purchases {
        let p = payload(row)?;
        let status = row["supplierIdentityStatus"].as_str();
        if status.is_some_and(|s| s != "resolved")
            || (status.is_none()
                && (!row["supplierCountryCode"].is_null()
                    || !row["supplierIdentifierKind"].is_null()))
        {
            return Err(refused(
                "purchase document has explicitly unresolved supplier identity",
            ));
        }
        if status == Some("resolved")
            && (row["supplierCountryCode"].as_str().is_none()
                || row["supplierIdentifierKind"].as_str().is_none())
        {
            return Err(refused(
                "resolved purchase identity requires persisted country and kind",
            ));
        }
        let party = Party {
            vat_or_cvr: row["supplierVatOrCvr"].as_str().map(str::to_owned),
            country_code: row["supplierCountryCode"].as_str().map(str::to_owned),
            identifier_kind: row["supplierIdentifierKind"].as_str().map(str::to_owned),
            ..Default::default()
        };
        rentemester_engine::document::supplier(&party)?;
        if row["grossAmount"].is_null() || row["vatAmount"].is_null() {
            return Err(refused(
                "purchase document requires canonical gross and VAT amounts",
            ));
        }
        let gross = amount(&row["grossAmount"])?;
        let vat = amount(&row["vatAmount"])?;
        row["netAmount"] = json!(gross.subtract(&vat).decimal());
        if let Some(split) = p.get("purchaseVatLines") {
            if split
                .as_array()
                .is_none_or(|a| a.is_empty() || a.len() > 1000)
                || !p["amountIncVat"].is_number()
                || !p["vatAmount"].is_number()
            {
                return Err(refused(
                    "invalid persisted purchase VAT split or canonical payload totals",
                ));
            }
            if amount(&p["amountIncVat"])? != gross
                || amount(&p["vatAmount"])? != vat
                || p["amountIncVat"].is_null()
                || p["vatAmount"].is_null()
            {
                return Err(refused(
                    "purchase VAT split payload differs from canonical document amounts",
                ));
            }
            // Reuse native source split validation without imposing unrelated purchase invoice admission policy.
            let metadata:Metadata=serde_json::from_value(json!({"source":"saft persisted split validation","documentType":"cash_register_receipt","amountIncVat":row["grossAmount"],"vatAmount":row["vatAmount"],"purchaseVatLines":split})).map_err(invalid)?;
            metadata.validate()?;
            row["purchaseVatLines"] = split.clone();
        }
        ids.insert(
            row["id"]
                .as_i64()
                .ok_or_else(|| refused("invalid purchase identity"))?,
        );
    }
    let mut originals = vec![];
    let mut documents = vec![];
    for id in ids {
        originals.push(crate::documents::evidence(tx, id)?);
        documents.extend(rows(tx,"SELECT id,document_no AS documentNo,document_type AS documentType,invoice_date AS invoiceDate,sha256_hash AS sha256,stored_path AS storedPath,mime_type AS mimeType,source,retain_until AS retainUntil FROM documents WHERE id=?1",&[&id])?);
    }
    let audit = rows(tx, "SELECT * FROM audit_log ORDER BY id", &[])?;
    Ok(Prepared {
        trial_balance,
        chain,
        source: Source {
            company,
            accounts,
            customers,
            suppliers,
            journal,
            sales,
            purchases,
        },
        originals,
        documents,
        audit,
        slug: slug.into(),
        start: period.start,
        end: period.end,
        generated,
    })
}
pub fn finish(tx: &Transaction<'_>, prepared: Prepared, actor: &str) -> Result<Download, Error> {
    let p = prepared;
    let rendered = saft::render(&p.source, &p.slug, &p.start, &p.end, &p.generated)?;
    let mut files = BTreeMap::new();
    files.insert("saft.xml".to_owned(), rendered.xml.into_bytes());
    let readme = format!(
        "Rentemester SAF-T export (third deterministic slice)\n\nThis is a bounded, profile-scoped export. It is NOT a complete or generic SAF-T implementation.\nProfile: {}\nAuditFileVersion: 3.0\nPeriod: {}..{}\nGenerated at (trusted runtime clock): {}\n\nIncludes company, chart (including archived accounts), customer/supplier master files, selected ledger and sales/purchase invoices, VAT summary and source document references.\nManifest sourceContext contains registration/actor/rule/hash data, original-document evidence metadata and the audit log before this export. Original bytes are verified but are NOT packaged. No journal is generated.\n\nOut of scope: {}\nNative full release/rule corpus provenance unavailable (manifest provenance:null). This is not an officially validated submission or a backup.\n",
        saft::PROFILE,
        p.start,
        p.end,
        p.generated,
        saft::OUT_OF_SCOPE.join(", ")
    );
    files.insert("README.txt".into(), readme.into_bytes());
    let outputs:Vec<_>=files.iter().map(|(path,b)|json!({"path":path,"sha256":format!("{:x}",Sha256::digest(b)),"sizeBytes":b.len()})).collect();
    let manifest = json!({"manifestVersion":2,"provenance":null,"packageType":"saft_export","profileId":saft::PROFILE,"generatedAt":p.generated,"generatedAtExplicit":false,"generatedAtSource":"trusted_runtime_clock","periodStart":p.start,"periodEnd":p.end,"sourceCompanyRootName":p.slug,"appliedRules":[saft::RULE],"files":{"saftXml":"saft.xml","readme":"README.txt"},"counts":{"accounts":p.source.accounts.len(),"journalEntries":p.source.journal.len(),"salesInvoices":p.source.sales.len(),"purchaseInvoices":p.source.purchases.len(),"vatSummaryCodes":rendered.vat_codes,"documentReferences":rendered.document_references,"customers":p.source.customers.len(),"suppliers":p.source.suppliers.len()},"outOfScope":saft::OUT_OF_SCOPE,"outputs":outputs,"sourceContext":{"journalEntries":p.source.journal,"documents":p.documents,"auditLog":p.audit,"trialBalance":p.trial_balance,"journalHashChain":p.chain},"limitations":["native_release_provenance_unavailable","original_bytes_not_packaged","not_official_schema_submission"]});
    let mut bytes = serde_json::to_vec_pretty(&manifest).map_err(invalid)?;
    bytes.push(b'\n');
    files.insert("manifest.json".into(), bytes);
    let root = format!(
        "saft-export-{}_{}_{}",
        p.start,
        p.end,
        p.generated.replace(['-', ':'], "").replace(".000Z", "Z")
    );
    let bytes = crate::accountant::tar(
        files
            .into_iter()
            .map(|(k, v)| (format!("{root}/{k}"), v))
            .collect(),
    )?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('saft_export','company',?1,?2,?3)",params![p.source.company["id"].as_i64(),format!("Exported saft_export {} for {}..{}",saft::PROFILE,p.start,p.end),format!("user:{actor}")]).map_err(invalid)?;
    Ok(Download {
        bytes,
        journal_entry_count: p.source.journal.len(),
        document_count: p.originals.len(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (rusqlite::Connection, rusqlite::Connection) {
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        let mut l = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic SAF-T").unwrap();
        l.execute_batch("UPDATE companies SET cvr='12345678',fiscal_year_start_month=7; INSERT INTO accounts(id,account_no,name,type,normal_balance)VALUES(1,'2000','Synthetic asset','asset','debit'),(2,'5000','Synthetic equity','equity','credit');INSERT INTO customers(name,vat_or_cvr)VALUES('Archived synthetic customer','DK87654321');INSERT INTO vendors(name,country_code,identifier_kind,identity_status)VALUES('Synthetic overseas supplier','US','non_eu','resolved'); INSERT INTO documents(id,document_no,document_type,source,sha256_hash,stored_path,original_filename,mime_type,invoice_date)VALUES(1,'DOC-SYN','cash_register_receipt','synthetic','synthetic-hash','documents/originals/synthetic.txt','synthetic.txt','text/plain','2026-06-30');").unwrap();
        let tx = l
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let request=serde_json::from_value(json!({"transactionDate":"2026-07-01","text":"Synthetic capital","currency":"DKK","documentId":"1","lines":[{"accountNo":"2000","debitAmount":"100.01"},{"accountNo":"5000","creditAmount":"100.01"}]})).unwrap();
        crate::journal_post::post(
            &tx,
            &request,
            &crate::journal_preview::manual_journal_rule_version().unwrap(),
            rentemester_engine::fiscal_year::Date::parse("2026-07-02").unwrap(),
            crate::journal_post::Actor {
                subject: "user:synthetic",
                program: "synthetic",
            },
        )
        .unwrap();
        tx.commit().unwrap();
        (c, l)
    }
    fn input() -> Input {
        Input {
            period_start: "2026-07-01".into(),
            period_end: "2027-06-30".into(),
            confirm: true,
        }
    }
    fn files(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
        let mut map = BTreeMap::new();
        let mut pos = 0;
        while bytes[pos..pos + 512].iter().any(|b| *b != 0) {
            let h = &bytes[pos..pos + 512];
            let text = |b: &[u8]| {
                String::from_utf8(b.iter().copied().take_while(|b| *b != 0).collect()).unwrap()
            };
            let name = text(&h[..100]);
            let prefix = text(&h[345..500]);
            let name = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let size = usize::from_str_radix(text(&h[124..136]).trim(), 8).unwrap();
            map.insert(name, bytes[pos + 512..pos + 512 + size].to_vec());
            pos += 512 + size.div_ceil(512) * 512;
        }
        map
    }
    #[test]
    fn exact_package_reconciles_shared_trial_balance_and_preserves_sources_without_journal_effects()
    {
        let (_c, mut l) = fixture();
        let tx = l
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let p = prepare(&tx, "synthetic", &input(), 1767225600000).unwrap();
        assert_eq!(p.originals.len(), 1);
        assert_eq!(p.source.accounts.len(), 2);
        assert_eq!(p.trial_balance["totalDebit"], "100.01");
        let xml = saft::render(&p.source, "synthetic", &p.start, &p.end, &p.generated)
            .unwrap()
            .xml;
        let same = saft::render(&p.source, "synthetic", &p.start, &p.end, &p.generated)
            .unwrap()
            .xml;
        assert_eq!(xml, same);
        let a = finish(&tx, p, "synthetic").unwrap();
        let fs = files(&a.bytes);
        assert_eq!(fs.len(), 3);
        let (_, bytes) = fs
            .iter()
            .find(|(k, _)| k.ends_with("/manifest.json"))
            .unwrap();
        let m: Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(m["profileId"], saft::PROFILE);
        assert_eq!(m["counts"]["journalEntries"], 1);
        assert_eq!(m["counts"]["documentReferences"], 1);
        assert_eq!(
            m["sourceContext"]["documents"][0]["sha256"],
            "synthetic-hash"
        );
        assert_eq!(m["sourceContext"]["journalHashChain"]["ok"], true);
        assert_eq!(m["sourceContext"]["trialBalance"]["totalDebit"], "100.01");
        assert_eq!(m["generatedAt"], "2026-01-01T00:00:00.000Z");
        assert!(
            m["sourceContext"]["auditLog"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["event_type"] != "saft_export")
        );
        for o in m["outputs"].as_array().unwrap() {
            let suffix = format!("/{}", o["path"].as_str().unwrap());
            let b = &fs.iter().find(|(k, _)| k.ends_with(&suffix)).unwrap().1;
            assert_eq!(o["sha256"], format!("{:x}", Sha256::digest(b)));
            assert_eq!(o["sizeBytes"].as_u64().unwrap(), b.len() as u64);
        }
        assert_eq!(
            tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            tx.query_row(
                "SELECT actor FROM audit_log WHERE event_type='saft_export'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "user:synthetic"
        );
    }
    #[test]
    fn reopened_source_produces_identical_xml_with_same_trusted_clock() {
        let (_c, mut l) = fixture();
        let before = {
            let tx = l
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            let p = prepare(&tx, "synthetic", &input(), 1767225600000).unwrap();
            saft::render(&p.source, &p.slug, &p.start, &p.end, &p.generated)
                .unwrap()
                .xml
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("synthetic.sqlite");
        l.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
            .unwrap();
        drop(l);
        let mut reopened = rusqlite::Connection::open(path).unwrap();
        let tx = reopened
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let p = prepare(&tx, "synthetic", &input(), 1767225600000).unwrap();
        assert_eq!(
            before,
            saft::render(&p.source, &p.slug, &p.start, &p.end, &p.generated)
                .unwrap()
                .xml
        );
    }
    #[test]
    fn explicit_confirmation_cvr_period_and_corrupt_journal_refuse_before_audit() {
        let (_c, mut l) = fixture();
        {
            let tx = l
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            let mut i = input();
            i.confirm = false;
            assert!(prepare(&tx, "synthetic", &i, 0).is_err());
            i = input();
            i.period_end = "2026-02-30".into();
            assert!(prepare(&tx, "synthetic", &i, 0).is_err());
        }
        l.execute("UPDATE companies SET cvr=NULL", []).unwrap();
        {
            let tx = l
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            assert!(prepare(&tx, "synthetic", &input(), 0).is_err());
        }
        l.execute("UPDATE companies SET cvr='12345678'", [])
            .unwrap();
        let guard: String = l
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE name='journal_entries_no_update'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        l.execute_batch("DROP TRIGGER journal_entries_no_update;UPDATE journal_entries SET entry_hash='corrupt'").unwrap();
        l.execute_batch(&guard).unwrap();
        let tx = l
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert!(prepare(&tx, "synthetic", &input(), 0).is_err());
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM audit_log WHERE event_type='saft_export'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn persisted_purchase_identity_and_split_are_not_silently_dropped() {
        let (_c, mut l) = fixture();
        l.execute_batch("INSERT INTO documents(document_no,document_type,source,sha256_hash,stored_path,original_filename,mime_type,invoice_date,amount_inc_vat,vat_amount,sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status,payload_json)VALUES('DOC-P','purchase_sale','synthetic','h','documents/originals/p.txt','p.txt','text/plain','2026-07-01',125,25,'DK12345678','DK','dk_cvr','resolved','{\"amountIncVat\":125,\"vatAmount\":25,\"purchaseVatLines\":[{\"classification\":\"dk_purchase_25\",\"netAmount\":100,\"vatAmount\":25}]}')").unwrap();
        {
            let tx = l
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            let p = prepare(&tx, "synthetic", &input(), 0).unwrap();
            assert_eq!(p.source.purchases[0]["netAmount"], "100.00");
            assert_eq!(p.originals.len(), 2);
        }
        l.execute("UPDATE documents SET payload_json='{}',supplier_identity_status='human_resolution_required' WHERE document_no='DOC-P'",[]).unwrap();
        let tx = l
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert!(prepare(&tx, "synthetic", &input(), 0).is_err());
    }
}
