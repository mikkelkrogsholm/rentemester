use super::*;
#[test]
fn source_empty_bytes() {
    let (json, csv) = artifacts(
        "2025-01-01",
        "2026-12-31",
        &[],
        &Money::default(),
        &Money::default(),
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(json).unwrap(),
        concat!(
            "{\n  \"appliedRules\": [\n    \"DK-MILEAGE-LOG-001\"\n  ],\n  \"from\": \"2025-01-01\",\n  \"to\": \"2026-12-31\",\n  \"entryCount\": 0,\n  \"totalKilometers\": 0,\n  \"totalAmountBasis\": 0,\n  \"note\": \"Mileage log only. Rentemester records the trips; tax treatment is the user/advisor's responsibility.\",\n  \"entries\": []\n}\n"
        )
    );
    assert_eq!(csv,b"entry_no,trip_date,purpose,from_location,to_location,kilometers,vehicle,driver,rate_per_km,amount_basis,rate_basis,rate_source\n");
}
#[test]
fn source_unicode_quotes_and_rate_bytes() {
    let row = Row {
        id: 7,
        entry_no: "MIL-2026-000001".into(),
        trip_date: "2026-01-02".into(),
        purpose: "Møde, \"Ø\"\nretur".into(),
        from_location: "Å".into(),
        to_location: "B\rC".into(),
        kilometers: 2.5,
        vehicle: "Bil".into(),
        driver: "Æ".into(),
        rate_per_km: 9.125,
        amount_basis: 22.81,
        rate_basis: "Egen".into(),
        rate_source: None,
        notes: Some("note".into()),
        created_at: "2026-01-03 01:02:03".into(),
    };
    let (json, csv) = artifacts(
        "2026-01-02",
        "2026-01-02",
        &[row],
        &Money::parse("2.5").unwrap(),
        &Money::parse("22.81").unwrap(),
    )
    .unwrap();
    assert_eq!(csv,"entry_no,trip_date,purpose,from_location,to_location,kilometers,vehicle,driver,rate_per_km,amount_basis,rate_basis,rate_source\nMIL-2026-000001,2026-01-02,\"Møde, \"\"Ø\"\"\nretur\",Å,\"B\rC\",2.5,Bil,Æ,9.125,22.81,Egen,\n".as_bytes());
    assert_eq!(
        String::from_utf8(json).unwrap(),
        r#"{
  "appliedRules": [
    "DK-MILEAGE-LOG-001"
  ],
  "from": "2026-01-02",
  "to": "2026-01-02",
  "entryCount": 1,
  "totalKilometers": 2.5,
  "totalAmountBasis": 22.81,
  "note": "Mileage log only. Rentemester records the trips; tax treatment is the user/advisor's responsibility.",
  "entries": [
    {
      "id": 7,
      "entryNo": "MIL-2026-000001",
      "tripDate": "2026-01-02",
      "purpose": "Møde, \"Ø\"\nretur",
      "fromLocation": "Å",
      "toLocation": "B\rC",
      "kilometers": 2.5,
      "vehicle": "Bil",
      "driver": "Æ",
      "ratePerKm": 9.125,
      "amountBasis": 22.81,
      "rateBasis": "Egen",
      "rateSource": null,
      "notes": "note",
      "createdAt": "2026-01-03 01:02:03"
    }
  ]
}
"#
    );
}
#[test]
fn source_validation_order_and_prepared_service_audit_no_financial_effects() {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Mileage").unwrap();
    let tx = db.transaction().unwrap();
    let mut input:Log=serde_json::from_value(json!({"tripDate":"","purpose":"","fromLocation":"","toLocation":"","vehicle":"","driver":"","rateBasis":""})).unwrap();
    assert_eq!(
        log(&tx, &input, "service", PrincipalKind::ServiceAccount).unwrap()["errors"],
        json!([
            "tripDate is required",
            "purpose is required",
            "fromLocation is required",
            "toLocation is required",
            "kilometers must be a positive number",
            "vehicle is required",
            "driver is required",
            "ratePerKm must be a positive number",
            "rateBasis is required (user-supplied / source-backed)"
        ])
    );
    assert_eq!(
        report(&tx, "bad", "bad").unwrap()["errors"],
        json!([
            "from must be a valid YYYY-MM-DD date",
            "to must be a valid YYYY-MM-DD date"
        ])
    );
    input.trip_date = "2026-01-02".into();
    input.purpose = "Trip".into();
    input.from_location = "A".into();
    input.to_location = "B".into();
    input.vehicle = "Car".into();
    input.driver = "Driver".into();
    input.rate_basis = "Explicit".into();
    input.kilometers = Some(Number::from_f64(2.5).unwrap());
    input.rate_per_km = Some(Number::from_f64(9.125).unwrap());
    assert_eq!(
        log(&tx, &input, "service", PrincipalKind::ServiceAccount).unwrap()["amountBasis"],
        22.81
    );
    assert!(matches!(
        export(
            &tx,
            "bad",
            "bad",
            "service",
            PrincipalKind::ServiceAccount,
            Date::parse("2026-01-03").unwrap()
        )
        .unwrap(),
        ExportOutcome::Refused(_)
    ));
    assert!(matches!(
        export(
            &tx,
            "2026-01-02",
            "2026-01-02",
            "service",
            PrincipalKind::ServiceAccount,
            Date::parse("2026-01-03").unwrap()
        )
        .unwrap(),
        ExportOutcome::Ready(_)
    ));
    assert_eq!(tx.query_row("SELECT count(*) FROM audit_log WHERE actor='service_account:service' AND (event_type='mileage_entry_create' OR (event_type='mileage_log_export' AND message LIKE 'Prepared%'))",[],|r|r.get::<_,i64>(0)).unwrap(),2);
    assert_eq!(
        tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(danish_rate(12345.1255).unwrap(), "12.345,126");
    tx.rollback().unwrap();
}
