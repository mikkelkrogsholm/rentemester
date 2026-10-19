//! Confirmed contact deletion preserves immutable documents and refuses live obligations.
use crate::{invalid, legacy_money};
use rentemester_engine::{Error, money::Money, sales_invoice::refused};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};

pub fn delete(
    tx: &Transaction<'_>,
    id: i64,
    vendor: bool,
    confirm: bool,
    actor: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !confirm || id <= 0 {
        return Err(refused("contact id and explicit confirmation required"));
    }
    let (table, kind) = if vendor {
        ("vendors", "vendor")
    } else {
        ("customers", "customer")
    };
    let contact: Option<(String, Option<String>)> = tx
        .query_row(
            &format!("SELECT name,vat_or_cvr FROM {table} WHERE id=?1 AND archived=0"),
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(invalid)?;
    let (name, cvr) = contact.ok_or_else(|| refused("contact not found"))?;
    if vendor {
        let mut q = tx
            .prepare("SELECT id,bill_no,gross_amount FROM payables WHERE vendor_id=?1")
            .map_err(invalid)?;
        let mut rows = q.query([id]).map_err(invalid)?;
        while let Some(row) = rows.next().map_err(invalid)? {
            let payable: i64 = row.get(0).map_err(invalid)?;
            let gross = legacy_money(row.get_ref(2).map_err(invalid)?)?;
            let paid = sum(
                tx,
                "SELECT amount FROM payable_payments WHERE payable_id=?1",
                payable,
            )?;
            if gross.subtract(&paid) != Money::default() {
                let bill: Option<String> = row.get(1).map_err(invalid)?;
                return Err(refused(format!(
                    "Leverandøren er i brug på gæld: {}",
                    bill.unwrap_or_else(|| payable.to_string())
                )));
            }
        }
    } else {
        let mut q=tx.prepare("SELECT id,invoice_no,payload_json,amount_inc_vat FROM documents WHERE document_type='issued_invoice'").map_err(invalid)?;
        let mut rows = q.query([]).map_err(invalid)?;
        while let Some(row) = rows.next().map_err(invalid)? {
            let document: i64 = row.get(0).map_err(invalid)?;
            let payload: String = row.get(2).map_err(invalid)?;
            let payload: Value = serde_json::from_str(&payload).map_err(invalid)?;
            let wanted = cvr.as_deref().unwrap_or("").trim().to_uppercase();
            let buyer = payload["buyer"]["vatOrCvr"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_uppercase();
            let same = payload["customerId"].as_i64() == Some(id)
                || if !wanted.is_empty() {
                    wanted == buyer
                } else {
                    buyer.is_empty()
                        && payload["buyer"]["name"]
                            .as_str()
                            .unwrap_or("")
                            .trim()
                            .to_lowercase()
                            == name.trim().to_lowercase()
                };
            if !same {
                continue;
            }
            let gross = legacy_money(row.get_ref(3).map_err(invalid)?)?;
            let paid = sum(
                tx,
                "SELECT amount FROM invoice_payments WHERE invoice_document_id=?1",
                document,
            )?;
            let written_off = sum(
                tx,
                "SELECT gross_amount FROM invoice_bad_debt_writeoffs WHERE invoice_document_id=?1",
                document,
            )?;
            let credited = crate::credit_note::credited(tx, document)?;
            let refunded = crate::invoice_settlement::refunded(tx, document)?;
            let mut open = gross
                .subtract(&paid)
                .subtract(&written_off)
                .subtract(&credited);
            open.add(&refunded);
            if open != Money::default() {
                let invoice: Option<String> = row.get(1).map_err(invalid)?;
                return Err(refused(format!(
                    "Kunden er i brug på åben faktura: {}",
                    invoice.unwrap_or_else(|| document.to_string())
                )));
            }
        }
    }
    // An erased source-row identity must not be recycled by SQLite after physical deletion.
    // Retiring that row preserves its tombstone identity without changing protected material.
    let erased = !crate::gdpr::contact_fields(tx, table, id)?.is_empty();
    if erased && vendor {
        let related:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM payables WHERE vendor_id=?1) OR EXISTS(SELECT 1 FROM document_vendor_identity_links WHERE vendor_id=?1)",[id],|r|r.get(0)).map_err(invalid)?;
        if related {
            return Err(refused(
                "Kontakten har historiske relationer og kan ikke slettes.",
            ));
        }
    }
    let changed = tx
        .execute(
            &if erased {
                format!("UPDATE {table} SET archived=1 WHERE id=?1 AND archived=0")
            } else {
                format!("DELETE FROM {table} WHERE id=?1 AND archived=0")
            },
            [id],
        )
        .map_err(|_| refused("Kontakten har historiske relationer og kan ikke slettes."))?;
    if changed != 1 {
        return Err(refused("contact not found"));
    }
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,?2,?3,?4,?5)",params![format!("{kind}_delete"),kind,crate::gdpr::contact_audit_reference(table,id),format!("Deleted {kind} {}",crate::gdpr::contact_audit_reference(table,id)),format!("user:{actor}")]).map_err(invalid)?;
    Ok(json!({"id":id,"deleted":true}))
}
fn sum(tx: &Transaction<'_>, sql: &str, id: i64) -> Result<Money, Error> {
    let mut q = tx.prepare(sql).map_err(invalid)?;
    let mut rows = q.query([id]).map_err(invalid)?;
    let mut result = Money::default();
    while let Some(row) = rows.next().map_err(invalid)? {
        result.add(&legacy_money(row.get_ref(0).map_err(invalid)?)?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confirmed_unused_contacts_delete_once_with_audit_and_no_journal_effect() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Contacts").unwrap();
        let tx = db.transaction().unwrap();
        tx.execute_batch("INSERT INTO customers(id,name) VALUES(1,'Synthetic unused buyer'); INSERT INTO vendors(id,name) VALUES(1,'Synthetic unused supplier');").unwrap();
        assert!(delete(&tx, 1, false, false, "synthetic").is_err());
        assert!(delete(&tx, 0, false, true, "synthetic").is_err());
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM audit_log", [], |r| r.get(0))
                .unwrap(),
            0
        );
        for vendor in [false, true] {
            assert_eq!(
                delete(&tx, 1, vendor, true, "synthetic").unwrap(),
                json!({"id":1,"deleted":true})
            );
            assert!(delete(&tx, 1, vendor, true, "synthetic").is_err());
        }
        assert_eq!(tx.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE actor='user:synthetic' AND event_type IN ('customer_delete','vendor_delete')",[],|r|r.get(0)).unwrap(),2);
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn erased_contact_is_retired_without_reusing_identity_or_logging_raw_name() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Contacts").unwrap();
        let tx = db.transaction().unwrap();
        tx.execute_batch("INSERT INTO customers(id,name) VALUES(1,'Synthetic protected identity');INSERT INTO gdpr_erasures(subject_key,source,source_row_id,redacted_fields,rule_id,reason)VALUES('synthetic','customers',1,'[\"name\"]','synthetic','synthetic');").unwrap();
        delete(&tx, 1, false, true, "synthetic").unwrap();
        assert_eq!(
            tx.query_row("SELECT archived FROM customers WHERE id=1", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        tx.execute(
            "INSERT INTO customers(name)VALUES('Synthetic unrelated')",
            [],
        )
        .unwrap();
        assert!(tx.last_insert_rowid() > 1);
        let message: String = tx
            .query_row(
                "SELECT message FROM audit_log WHERE event_type='customer_delete'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!message.contains("Synthetic protected identity"));
        assert!(message.contains("sha256:"));
        assert!(delete(&tx, 1, false, true, "synthetic").is_err());
    }
    #[test]
    fn open_invoice_snapshot_blocks_even_after_contact_rename() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Seller").unwrap();
        let tx = db.transaction().unwrap();
        tx.execute_batch("UPDATE companies SET address='Synthetic Road 1',cvr='87654321',vat_period_type='quarter'; INSERT INTO customers(id,name,address,vat_or_cvr) VALUES(1,'Synthetic Buyer','Synthetic Road 2','DK12345678');").unwrap();
        let input=serde_json::from_value(json!({"issueDate":"2024-01-15","customerId":1,"lines":[{"description":"Synthetic old service","quantity":1,"unitPriceExVat":100}]})).unwrap();
        let payload = crate::sales::prepare(&tx, &input, "2024-01-15T12:00:00Z", true).unwrap();
        let file = |mime| crate::sales::Artifact {
            filename: "synthetic",
            path: "synthetic",
            hash: if mime == "application/json" {
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            } else {
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            },
            mime,
        };
        crate::sales::register(
            &tx,
            &payload,
            file("application/json"),
            file("application/pdf"),
            "synthetic",
        )
        .unwrap();
        let original: String = tx
            .query_row(
                "SELECT payload_json FROM documents WHERE document_type='issued_invoice'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        tx.execute(
            "UPDATE customers SET name='Synthetic renamed buyer',vat_or_cvr=NULL WHERE id=1",
            [],
        )
        .unwrap();
        assert!(
            delete(&tx, 1, false, true, "synthetic")
                .unwrap_err()
                .message
                .contains("2024-0001")
        );
        assert_eq!(
            tx.query_row::<String, _, _>(
                "SELECT payload_json FROM documents WHERE document_type='issued_invoice'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            original
        );
        assert_eq!(
            tx.query_row::<i64, _, _>("SELECT count(*) FROM customers", [], |r| r.get(0))
                .unwrap(),
            1
        );
        assert_eq!(
            tx.query_row::<i64, _, _>(
                "SELECT count(*) FROM audit_log WHERE event_type='customer_delete'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            0
        );
    }
}
