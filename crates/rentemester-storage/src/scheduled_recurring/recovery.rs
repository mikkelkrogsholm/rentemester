//! Typed restore provenance over unchanged physical extension1. No credentials or authority import.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RestoreConsent {
    pub required: bool,
    pub boundary_hash: String,
    pub historical_owner_user_id: Option<String>,
    pub historical_queued: Vec<Occurrence>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Marker {
    version: u8,
    archive_hash: String,
    ledger_uuid: String,
    imported_event_high_water_mark: i64,
    imported_event_head_hash: String,
}
#[derive(Clone)]
pub(super) struct Boundary {
    marker: Marker,
    hash: String,
}
impl Boundary {
    pub fn imported_high(&self) -> i64 {
        self.marker.imported_event_high_water_mark
    }
}
fn ledger(db: &Connection) -> Result<String, Error> {
    let id: String = db
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if id.len() != 32
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(refused());
    }
    Ok(id)
}
/// Audit ordering binds each fixed native event audit to the exact imported event prefix.
pub(super) fn boundaries(db: &Connection, all: &[Event]) -> Result<Vec<Boundary>, Error> {
    let mut q=db.prepare("SELECT id,entity_type,entity_id,message,actor FROM audit_log WHERE event_type='scheduled_recurring' ORDER BY id").map_err(invalid)?;
    let audits = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if audits.len() != all.len()
        || audits.iter().zip(all).any(|(a, e)| {
            a.0 <= 0
                || a.1 != "recurring_invoice_template"
                || a.2 != e.payload.template_id.to_string()
                || a.3 != e.payload.kind
                || a.4
                    != format!(
                        "{}:{}",
                        if e.payload.version == 3 && e.payload.kind != "enabled" {
                            "service_account"
                        } else {
                            "user"
                        },
                        e.payload.actor
                    )
        })
    {
        return Err(refused());
    }
    let mut q=db.prepare("SELECT id,entity_type,entity_id,message,actor FROM audit_log WHERE event_type='native_restore_scheduled_boundary' ORDER BY id").map_err(invalid)?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if !rows.is_empty() && !schema::installed(db)? {
        return Err(refused());
    }
    let uuid = ledger(db)?;
    let mut previous_high = 0;
    let mut out = vec![];
    for (id, entity, high, canonical, actor) in rows {
        let marker: Marker = serde_json::from_str(&canonical).map_err(invalid)?;
        let n = marker.imported_event_high_water_mark;
        let expected = if n == 0 {
            "0".repeat(64)
        } else {
            all.get(usize::try_from(n - 1).map_err(|_| refused())?)
                .ok_or_else(refused)?
                .hash
                .clone()
        };
        if id <= 0
            || entity != "native_scheduled_recurring_events"
            || actor != "system:portable-restore"
            || marker.version != 1
            || !engine::hash(&marker.archive_hash)
            || marker.ledger_uuid != uuid
            || n < previous_high
            || n < 0
            || n as usize > all.len()
            || high != n.to_string()
            || marker.imported_event_head_hash != expected
            || canonical != serde_json::to_string(&marker).map_err(invalid)?
            || audits.iter().filter(|a| a.0 < id).count() != n as usize
        {
            return Err(refused());
        }
        let hash = sha(json!({"auditId":id,"marker":marker}).to_string().as_bytes());
        previous_high = n;
        out.push(Boundary { marker, hash });
    }
    Ok(out)
}
pub(super) fn overlay(s: &mut State, b: &Boundary) {
    let mut queued: BTreeMap<u32, Occurrence> = s
        .restore
        .as_ref()
        .map(|r| {
            r.historical_queued
                .iter()
                .map(|o| (o.period_index, o.clone()))
                .collect()
        })
        .unwrap_or_default();
    queued.extend(s.queued.clone());
    queued.retain(|i, _| !s.resolved.contains(i));
    let historical = s.owner.clone().or_else(|| {
        s.restore
            .as_ref()
            .and_then(|r| r.historical_owner_user_id.clone())
    });
    let required = matches!(s.state, "enabled" | "paused");
    if required {
        s.state = "paused";
    }
    if let Some(h) = &mut s.hosted {
        h.active = false;
    }
    s.owner = None;
    s.queued.clear();
    s.hash = sha(
        json!({"version":1,"sourceStateHash":s.hash,"boundaryHash":b.hash})
            .to_string()
            .as_bytes(),
    );
    s.restore = Some(RestoreConsent {
        required,
        boundary_hash: b.hash.clone(),
        historical_owner_user_id: historical,
        historical_queued: queued.into_values().collect(),
    });
}
/// Read-only full native scheduler validation. Physical original bytes are independently checked by the backup inventory owner.
pub fn validate_backup(db: &Connection) -> Result<(), Error> {
    crate::journal_preview::require_current(db)?;
    let all = events(db)?;
    boundaries(db, &all)?;
    let audit_count: i64 = db
        .query_row(
            "SELECT count(*) FROM audit_log WHERE event_type='scheduled_recurring'",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if audit_count != all.len() as i64 {
        return Err(refused());
    }
    let ids: BTreeSet<i64> = all.iter().map(|e| e.payload.template_id).collect();
    for id in ids {
        let hash = spec(db, id)?.0;
        state(db, id, &hash, &all)?;
        validate_generations(db, id, &all)?;
    }
    Ok(())
}
/// Called ONLY by the protected fresh-target restore operator after verified bytes are admitted.
pub fn append_restore_boundary(tx: &Transaction<'_>, archive_hash: &str) -> Result<(), Error> {
    validate_backup(tx)?;
    if !engine::hash(archive_hash) {
        return Err(refused());
    }
    if !schema::installed(tx)? {
        return Ok(());
    }
    let all = events(tx)?;
    let marker = Marker {
        version: 1,
        archive_hash: archive_hash.into(),
        ledger_uuid: ledger(tx)?,
        imported_event_high_water_mark: all.len() as i64,
        imported_event_head_hash: all
            .last()
            .map(|e| e.hash.clone())
            .unwrap_or_else(|| "0".repeat(64)),
    };
    let max: i64 = tx
        .query_row("SELECT COALESCE(MAX(id),0) FROM audit_log", [], |r| {
            r.get(0)
        })
        .map_err(invalid)?;
    let id = max
        .checked_add(1)
        .filter(|id| *id > 0)
        .ok_or_else(refused)?;
    tx.execute("INSERT INTO audit_log(id,event_type,entity_type,entity_id,message,actor) VALUES(?1,'native_restore_scheduled_boundary','native_scheduled_recurring_events',?2,?3,'system:portable-restore')",params![id,marker.imported_event_high_water_mark.to_string(),serde_json::to_string(&marker).map_err(invalid)?]).map_err(invalid)?;
    validate_backup(tx)
}
fn validate_generations(db: &Connection, id: i64, all: &[Event]) -> Result<(), Error> {
    let mut q=db.prepare("SELECT period_index,document_id,invoice_number,issue_date,delivery_period_start,delivery_period_end FROM recurring_invoice_generations WHERE template_id=?1 ORDER BY period_index").map_err(invalid)?;
    let rows = q
        .query_map([id], |r| {
            Ok((
                r.get::<_, u32>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    for (index, document, number, date, start, end) in rows {
        if recurring::scheduled_date(db, id, index)? != date {
            return Err(refused());
        }
        let payload:String=db.query_row("SELECT payload_json FROM documents WHERE id=?1 AND document_type='issued_invoice' AND status='issued' AND invoice_no=?2 AND invoice_date=?3",params![document,number,date],|r|r.get(0)).map_err(invalid)?;
        let p: Value = serde_json::from_str(&payload).map_err(invalid)?;
        if p["recurringTemplateId"] != id
            || p["recurringPeriodIndex"] != index
            || p["invoiceNumber"] != number
            || p["issueDate"] != date
            || p["deliveryPeriodStart"] != json!(start)
            || p["deliveryPeriodEnd"] != json!(end)
        {
            return Err(refused());
        }
        for event in all.iter().filter(|e| {
            e.payload.template_id == id
                && e.payload
                    .data
                    .occurrences
                    .iter()
                    .any(|o| o.period_index == index)
        }) {
            if event.payload.kind == "succeeded" {
                let receipt = event.payload.data.generation.as_ref().ok_or_else(refused)?;
                for (key, value) in [
                    ("documentId", json!(document)),
                    ("invoiceNumber", json!(number)),
                    ("issueDate", json!(date)),
                    ("dueDate", p["dueDate"].clone()),
                    ("deliveryPeriodStart", json!(start)),
                    ("deliveryPeriodEnd", json!(end)),
                ] {
                    if receipt[key] != value {
                        return Err(refused());
                    }
                }
            }
        }
        let claimed:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM recurring_invoice_generation_claims WHERE template_id=?1 AND period_index=?2)",params![id,index],|r|r.get(0)).map_err(invalid)?;
        if !claimed {
            return Err(refused());
        }
        let mut q=db.prepare("SELECT id,document_type,mime_type,stored_path,sha256_hash,invoice_date,currency,amount_inc_vat,vat_amount FROM documents WHERE invoice_no=?1 AND payload_json=?2 AND document_type IN ('issued_invoice','issued_invoice_pdf') AND status='issued' ORDER BY document_type").map_err(invalid)?;
        let docs = q
            .query_map(params![number, payload], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, f64>(7)?,
                    r.get::<_, f64>(8)?,
                ))
            })
            .map_err(invalid)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(invalid)?;
        if docs.len() != 2 {
            return Err(refused());
        }
        for (doc, kind, mime, path, hash, issued, currency, gross, vat) in docs {
            if (kind == "issued_invoice"
                && (doc != document
                    || mime != "application/json"
                    || hash != sha(payload.as_bytes())))
                || (kind == "issued_invoice_pdf" && mime != "application/pdf")
                || path.is_empty()
                || !engine::hash(&hash)
                || issued != date
                || p["currency"] != currency
                || p["totals"]["grossAmount"] != gross
                || p["totals"]["vatAmount"] != vat
            {
                return Err(refused());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const TODAY: &str = "2026-01-01";
    const AT: &str = "2026-01-01T00:00:00Z";
    fn fixture() -> (Connection, i64) {
        let mut control = Connection::open_in_memory().unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic restored scheduler")
            .unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("UPDATE companies SET address='Synthetic1',cvr='87654321',vat_period_type='quarter' WHERE id=1").unwrap();
        let customer = crate::sales::create_customer(
            &tx,
            &serde_json::from_value(
                json!({"name":"Synthetic buyer","address":"Synthetic2","vatOrCvr":"DK12345678"}),
            )
            .unwrap(),
            "old",
        )
        .unwrap()["id"]
            .as_i64()
            .unwrap();
        let input=serde_json::from_value(json!({"name":"Synthetic recovery","interval":"weekly","intervalCount":1,"firstIssueDate":TODAY,"paymentTermsDays":14,"vatRatePercent":25,"currency":"DKK","customerId":customer,"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100}],"confirm":true})).unwrap();
        let id = recurring::create(&tx, &input, "old", AT).unwrap()["templateId"]
            .as_i64()
            .unwrap();
        let p = plan(&tx, id, TODAY).unwrap();
        enable(
            &tx,
            &serde_json::from_value(
                json!({"templateId":id,"templateHash":p["templateHash"],"confirm":true}),
            )
            .unwrap(),
            "old",
            TODAY,
            AT,
        )
        .unwrap();
        tx.commit().unwrap();
        (db, id)
    }
    #[test]
    fn unresolved_attempt_is_preserved_without_replay_or_reconsent() {
        let (mut db, id) = fixture();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let p = plan(&tx, id, TODAY).unwrap();
        begin(
            &tx,
            &serde_json::from_value(
                json!({"templateId":id,"planHash":p["planHash"],"periodIndex":0,"confirm":true}),
            )
            .unwrap(),
            "old",
            TODAY,
            AT,
        )
        .unwrap();
        tx.commit().unwrap();
        validate_backup(&db).unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        append_restore_boundary(&tx, &"c".repeat(64)).unwrap();
        let p = plan(&tx, id, TODAY).unwrap();
        assert_eq!(p["restoreConsent"]["required"], true);
        let row = current(&tx, id, TODAY).unwrap();
        assert_eq!(row["error"], "unresolved_attempt");
        assert_eq!(row["lastRun"]["status"], "unresolved");
        assert!(reconsent(&tx,&serde_json::from_value(json!({"templateId":id,"planHash":p["planHash"],"reason":"Explicit restored review","confirm":true})).unwrap(),"new",TODAY,AT).is_err());
        tx.commit().unwrap();
        validate_backup(&db).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM documents", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn malformed_boundary_and_unpaired_native_audit_refuse() {
        let (mut db, _) = fixture();
        {
            let tx = db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            append_restore_boundary(&tx, &"d".repeat(64)).unwrap();
            tx.commit().unwrap();
        }
        validate_backup(&db).unwrap();
        db.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('native_restore_scheduled_boundary','native_scheduled_recurring_events','1','{}','system:portable-restore')",[]).unwrap();
        assert!(validate_backup(&db).is_err());
        let (mut other, _) = fixture();
        other.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('scheduled_recurring','recurring_invoice_template','1','enabled','user:old')",[]).unwrap();
        assert!(validate_backup(&other).is_err());
        assert!(
            append_restore_boundary(
                &other
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                    .unwrap(),
                &"e".repeat(64)
            )
            .is_err()
        );
    }
}
