use super::*;
fn dashboard() -> Value {
    json!({"slug":"a","asOf":"2026-01-01","company":{"name":"Synthetic A","cvr":"12345678"},"vat":{"registered":true,"available":true,"periodStart":"2025-10-01","periodEnd":"2025-12-31","daysRemaining":60,"deadline":"2026-03-02","netVatPayable":12.34,"errors":[],"basis":"native_history"},"invoices":{"count":2,"openTotal":125.5,"available":true,"basis":"dated_native_invoice"},"overdueInvoices":{"count":1},"backup":{"backupDue":true,"available":true,"backupsFound":0,"daysSinceLatestBackup":null,"hasActivitySinceBackup":true,"basis":"current_state","scope":"local-retained-snapshots","errors":[]},"audit":{"ok":true,"entryCount":7,"firstError":null,"scope":"current_journal_hash_chain"},"exceptions":{"count":3,"rows":[{"severity":"high"},{"severity":"low"},{"severity":"high"}]},"basis":{"principalAsOf":"2026-01-01","currentStateDate":"2026-02-01","claims":"current_state"}})
}
#[test]
fn projects_original_fields_without_financial_recalculation() {
    let d = dashboard();
    let r = row(&d).unwrap();
    assert_eq!(r["openReceivables"]["totalOpenBalance"], 125.5);
    assert_eq!(r["openReceivables"]["overdueCount"], 1);
    assert_eq!(r["vat"]["daysUntilDue"], 60);
    assert_eq!(r["vat"]["deadline"], "2026-03-02");
    assert_eq!(r["vat"]["canCompute"], true);
    assert_eq!(r["openExceptions"]["highSeverityCount"], 2);
    assert_eq!(r["audit"]["entries"], 7);
    assert_eq!(r["backup"]["backupsFound"], 0);
    assert_eq!(r["basis"], d["basis"]);
    assert!(r.get("total").is_none());
}
#[test]
fn unavailable_values_and_errors_remain_truthful() {
    let mut d = dashboard();
    d["invoices"] = json!({"available":false,"count":null,"openTotal":null,"basis":"native_history_unavailable","reason":"imported_history"});
    d["overdueInvoices"]["count"] = Value::Null;
    d["vat"]["errors"] = json!(["unreported source unavailable"]);
    d["backup"] = json!({"available":false,"backupDue":null,"backupsFound":null,"daysSinceLatestBackup":null,"hasActivitySinceBackup":null,"errors":[],"requiredPermission":"company.admin"});
    let r = row(&d).unwrap();
    assert!(r["openReceivables"]["totalOpenBalance"].is_null());
    assert_eq!(r["vat"]["canCompute"], false);
    assert!(r["vat"]["netVatPayable"].is_null());
    d["vat"]["registered"] = json!(false);
    d["vat"]["errors"] = json!([]);
    let r = row(&d).unwrap();
    assert_eq!(r["vat"]["canCompute"], false);
    for field in ["periodStart", "periodEnd", "daysUntilDue", "netVatPayable"] {
        assert!(r["vat"][field].is_null());
    }
    assert!(r["backup"]["backupDue"].is_null());
    assert_eq!(r["backup"]["available"], false);
    d["exceptions"]["rows"] = Value::Null;
    assert!(row(&d).is_err());
}
