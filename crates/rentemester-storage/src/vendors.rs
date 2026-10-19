//! Supplier master data. Purchase documents retain their own immutable party snapshot.
use crate::invalid;
use rentemester_engine::{
    Error,
    document::{Party, supplier},
    sales_invoice::{refused, text},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Vendor {
    pub name: String,
    pub address: Option<String>,
    pub vat_or_cvr: Option<String>,
    pub country_code: Option<String>,
    pub identifier_kind: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub default_expense_account: Option<String>,
    pub default_vat_treatment: Option<String>,
    pub notes: Option<String>,
}
fn normalize(tx: &Transaction<'_>, input: &mut Vendor) -> Result<bool, Error> {
    input.name = text(&input.name, 256)?;
    for field in [
        &mut input.address,
        &mut input.vat_or_cvr,
        &mut input.country_code,
        &mut input.identifier_kind,
        &mut input.email,
        &mut input.phone,
        &mut input.website,
        &mut input.default_expense_account,
        &mut input.default_vat_treatment,
        &mut input.notes,
    ] {
        if let Some(value) = field.take() {
            if value.len() > 4096 || value.chars().any(char::is_control) {
                return Err(refused("invalid vendor field"));
            }
            let value = value.trim();
            if !value.is_empty() {
                *field = Some(value.to_owned());
            }
        }
    }
    let explicit = input.country_code.is_some() || input.identifier_kind.is_some();
    if explicit {
        let identity = supplier(&Party {
            vat_or_cvr: input.vat_or_cvr.clone(),
            country_code: input.country_code.clone(),
            identifier_kind: input.identifier_kind.clone(),
            ..Default::default()
        })?;
        input.country_code = Some(identity.country);
        input.identifier_kind = Some(identity.kind);
        input.vat_or_cvr = identity.identifier;
    }
    if let Some(account) = &input.default_expense_account {
        let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND active=1 AND type='expense' AND normal_balance='debit')",[account],|r|r.get(0)).map_err(invalid)?;
        if !valid {
            return Err(refused(
                "vendor default account must be an active expense account",
            ));
        }
    }
    if input.default_vat_treatment.as_deref().is_some_and(|s| {
        ![
            "standard",
            "domestic_reverse_charge",
            "foreign_reverse_charge",
            "exempt",
            "non_deductible",
        ]
        .contains(&s)
    }) {
        return Err(refused("unsupported vendor VAT treatment"));
    }
    Ok(explicit)
}
pub fn create(tx: &Transaction<'_>, input: &Vendor, actor: &str) -> Result<Value, Error> {
    save(tx, None, input.clone(), actor)
}
pub fn update(tx: &Transaction<'_>, id: i64, patch: &Value, actor: &str) -> Result<Value, Error> {
    let fields = patch
        .as_object()
        .filter(|p| !p.is_empty())
        .ok_or_else(|| refused("vendor patch must be a nonempty object"))?;
    let mut value:Value=tx.query_row("SELECT name,address,vat_or_cvr,country_code,identifier_kind,email,phone,website,default_expense_account,default_vat_treatment,notes FROM vendors WHERE id=?1 AND archived=0",[id],|r|Ok(json!({"name":r.get::<_,String>(0)?,"address":r.get::<_,Option<String>>(1)?,"vatOrCvr":r.get::<_,Option<String>>(2)?,"countryCode":r.get::<_,Option<String>>(3)?,"identifierKind":r.get::<_,Option<String>>(4)?,"email":r.get::<_,Option<String>>(5)?,"phone":r.get::<_,Option<String>>(6)?,"website":r.get::<_,Option<String>>(7)?,"defaultExpenseAccount":r.get::<_,Option<String>>(8)?,"defaultVatTreatment":r.get::<_,Option<String>>(9)?,"notes":r.get::<_,Option<String>>(10)?}))).optional().map_err(invalid)?.ok_or_else(||refused("vendor not found"))?;
    for (key, item) in fields {
        value
            .as_object_mut()
            .unwrap()
            .insert(key.clone(), item.clone());
    }
    save(
        tx,
        Some(id),
        serde_json::from_value(value).map_err(|_| refused("invalid vendor field"))?,
        actor,
    )
}
fn save(
    tx: &Transaction<'_>,
    id: Option<i64>,
    mut input: Vendor,
    actor: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let event = if id.is_some() {
        "vendor_update"
    } else {
        "vendor_create"
    };
    let resolved = normalize(tx, &mut input)?;
    if let Some(id)=id {
        let previous:(String,Option<String>)=tx.query_row("SELECT name,vat_or_cvr FROM vendors WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(invalid)?;
        crate::gdpr::require_contact_identity(tx,"vendors",Some(id),&previous.0,previous.1.as_deref())?;
    }
    crate::gdpr::require_contact_identity(tx,"vendors",id,&input.name,input.vat_or_cvr.as_deref())?;
    let status = if resolved {
        "resolved"
    } else {
        "human_resolution_required"
    };
    let values = params![
        input.name,
        input.address,
        input.vat_or_cvr,
        input.country_code,
        input.identifier_kind,
        status,
        input.email,
        input.phone,
        input.website,
        input.default_expense_account,
        input.default_vat_treatment,
        input.notes,
        id
    ];
    let id = if let Some(id) = id {
        let changed=tx.execute("UPDATE vendors SET name=?1,address=?2,vat_or_cvr=?3,country_code=?4,identifier_kind=?5,identity_status=?6,email=?7,phone=?8,website=?9,default_expense_account=?10,default_vat_treatment=?11,notes=?12 WHERE id=?13 AND archived=0",values).map_err(|_|refused("vendor already exists or could not be stored"))?;
        if changed != 1 {
            return Err(refused("vendor not found"));
        }
        id
    } else {
        tx.query_row("INSERT INTO vendors(name,address,vat_or_cvr,country_code,identifier_kind,identity_status,email,phone,website,default_expense_account,default_vat_treatment,notes) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) RETURNING id",&values[..12],|r|r.get(0)).map_err(|e|if e.sqlite_error().is_some_and(|v|v.extended_code==rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE){refused("vendor already exists")}else{Error::new("contact_storage_unavailable","contact could not be stored")})?
    };
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'vendor',?2,?3,?4)",params![event,crate::gdpr::contact_audit_reference("vendors",id),format!("{event} {}",crate::gdpr::contact_audit_reference("vendors",id)),format!("user:{actor}")]).map_err(invalid)?;
    Ok(json!({"id":id,"ok":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_identity_partial_updates_and_invalid_defaults_preserve_source_evidence() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Company").unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('E','Synthetic expense','expense','debit'),('B','Synthetic bank','asset','debit');").unwrap();
        let vendor = |value| serde_json::from_value::<Vendor>(value).unwrap();
        let id=create(&tx,&vendor(json!({"name":"Synthetic Supplier","address":"Synthetic Street 1","countryCode":"dk","identifierKind":"dk_cvr","vatOrCvr":"12 34 56 78","phone":"1111","defaultExpenseAccount":"E","defaultVatTreatment":"standard"})),"synthetic").unwrap()["id"].as_i64().unwrap();
        let read = || {
            crate::sales::contacts(&tx, "synthetic", "2026-10-18").unwrap()["vendors"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == id)
                .unwrap()
                .clone()
        };
        assert_eq!(read()["vatOrCvr"], "DK12345678");
        assert_eq!(read()["identityStatus"], "resolved");
        let metadata = rentemester_engine::document::Metadata {
            source: "synthetic".into(),
            document_type: Some("purchase_sale".into()),
            issue_date: Some("2026-10-18".into()),
            invoice_no: Some("SYN-1".into()),
            delivery_description: Some("Synthetic supplies".into()),
            amount_inc_vat: Some(125.into()),
            currency: Some("DKK".into()),
            sender: Some(Party {
                name: Some("Synthetic Supplier".into()),
                address: Some("Synthetic Street 1".into()),
                vat_or_cvr: Some("DK12345678".into()),
                country_code: Some("DK".into()),
                identifier_kind: Some("dk_cvr".into()),
            }),
            recipient: Some(Party {
                name: Some("Synthetic Buyer".into()),
                address: Some("Synthetic Street 2".into()),
                vat_or_cvr: Some("DK87654321".into()),
                ..Default::default()
            }),
            vat_amount: Some(25.into()),
            purchase_vat_lines: None,
            reverse_charge_wording_confirmed: None,
            danish_simplified_purchase_invoice: None,
            payment_details: None,
            exemption_code: None,
            source_bank_transaction_id: None,
            accounting_rationale: None,
        };
        let registered = crate::documents::register(
            &tx,
            crate::documents::Original {
                filename: "synthetic.txt",
                hash: &"a".repeat(64),
                path: "synthetic.txt",
                mime: "text/plain",
            },
            &metadata,
            "synthetic",
            "2026-10-18",
        )
        .unwrap();
        let document_id = registered["document"]["id"].as_i64().unwrap();
        let original: String = tx
            .query_row(
                "SELECT payload_json FROM documents WHERE id=?1",
                [document_id],
                |r| r.get(0),
            )
            .unwrap();
        update(&tx,id,&json!({"name":"Synthetic Renamed Supplier","address":"Synthetic New Street 3","phone":null}),"synthetic").unwrap();
        assert_eq!(read()["name"], "Synthetic Renamed Supplier");
        assert_eq!(read()["phone"], Value::Null);
        assert_eq!(read()["defaultExpenseAccount"], "E");
        assert_eq!(
            tx.query_row::<String, _, _>(
                "SELECT payload_json FROM documents WHERE id=?1",
                [document_id],
                |r| r.get(0)
            )
            .unwrap(),
            original
        );
        let listed = crate::documents::list(&tx, "synthetic").unwrap();
        assert_eq!(listed["documents"][0]["supplierName"], "Synthetic Supplier");
        for patch in [
            json!({"name":" "}),
            json!({"countryCode":"DE"}),
            json!({"defaultExpenseAccount":"B"}),
            json!({"defaultVatTreatment":"invented"}),
            json!({"unknownField":true}),
        ] {
            assert!(update(&tx, id, &patch, "synthetic").is_err());
        }
        assert_eq!(read()["name"], "Synthetic Renamed Supplier");
        assert!(update(&tx, 999, &json!({"name":"Missing"}), "synthetic").is_err());
        for value in [
            json!({"name":"Synthetic EU","countryCode":"DE","identifierKind":"eu_vat","vatOrCvr":"DE123456789"}),
            json!({"name":"Synthetic US","countryCode":"US","identifierKind":"non_eu"}),
            json!({"name":"Synthetic Unresolved","vatOrCvr":"untyped"}),
        ] {
            assert!(create(&tx, &vendor(value), "synthetic").is_ok());
        }
        assert_eq!(
            tx.query_row::<i64, _, _>(
                "SELECT count(*) FROM vendors WHERE identity_status='human_resolution_required'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(tx.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE event_type='vendor_update' AND entity_type='vendor'",[],|r|r.get(0)).unwrap(),1);
        tx.execute_batch("INSERT INTO gdpr_erasures(subject_key,source,source_row_id,redacted_fields,rule_id,reason) VALUES('synthetic','vendors',1,'[\"name\"]','synthetic','synthetic');").unwrap();
        assert!(
            update(
                &tx,
                id,
                &json!({"name":"Must not bypass erasure"}),
                "synthetic"
            )
            .is_err()
        );
    }
}
