use super::*;
fn company_row() -> Value {
    json!({"companyType":{"type":"DK:CVR","id":"DK12345678"},"companyName":"Synthetic Seller","createdAt":"2026-10-20T10:00:00Z","companyKey":"synthetic_key"})
}
fn companies() -> Value {
    json!({"apiLicenseKey":"synthetic-license","companyKeyConstraint":null,"companies":[company_row()]})
}
fn parse(v: &Value) -> Result<Option<CompanyRegistration>, Error> {
    company(
        &serde_json::to_vec(v).unwrap(),
        "DK12345678",
        "Synthetic Seller",
        "private-key",
    )
}
fn participants() -> Value {
    let mut c = company_row();
    c.as_object_mut().unwrap().remove("companyKey");
    json!({"apiLicenseKey":"synthetic-license","companyKey":"synthetic_key","company":c,"participants":[{"networkType":"nemhandel","companyKey":"synthetic_key","direction":"outbound","participantType":"DK:CVR","participantId":"DK12345678","webhookUrl":null,"documentProfiles":"default-nemhandel","createdAt":"2026-10-20T10:00:00Z"}]})
}
fn parse_out(v: &Value) -> Result<Option<ParticipantRegistration>, Error> {
    outbound(
        &serde_json::to_vec(v).unwrap(),
        "synthetic_key",
        "DK12345678",
        "Synthetic Seller",
        "private-key",
    )
}
#[test]
fn exact_company_identity_and_constrained_license_are_required() {
    let mut v = companies();
    assert_eq!(parse(&v).unwrap().unwrap().company_key, "synthetic_key");
    v["companyKeyConstraint"] = json!("synthetic_key");
    assert!(parse(&v).unwrap().is_some());
    v["companyKeyConstraint"] = json!("other_key");
    assert_eq!(parse(&v), Err(Error::Protocol));
    v = companies();
    v["companies"][0]["companyName"] = json!("Different legal name");
    assert_eq!(parse(&v), Err(Error::Protocol));
    v = companies();
    v["companies"][0]["companyType"]["id"] = json!("DK87654321");
    assert!(parse(&v).unwrap().is_none());
}
#[test]
fn complete_empty_list_is_absence_but_partial_ambiguous_or_malformed_is_not() {
    let mut v = companies();
    v["companies"] = json!([]);
    assert!(parse(&v).unwrap().is_none());
    v["hasMore"] = json!(true);
    assert_eq!(parse(&v), Err(Error::Protocol));
    v = companies();
    v["companies"] = json!([company_row(), company_row()]);
    assert_eq!(parse(&v), Err(Error::Protocol));
    v = companies();
    v.as_object_mut().unwrap().remove("companyKeyConstraint");
    assert_eq!(parse(&v), Err(Error::Protocol));
    v = companies();
    v["companies"][0]["createdAt"] = json!("not a date");
    assert_eq!(parse(&v), Err(Error::Protocol));
    v = companies();
    v["companies"][0]["companyKey"] = json!("private-key");
    assert_eq!(parse(&v), Err(Error::Protocol));
}
#[test]
fn outbound_recovery_proves_legal_identity_key_profile_and_null_webhook() {
    let mut v = participants();
    assert_eq!(
        parse_out(&v).unwrap(),
        Some(ParticipantRegistration {
            registered_on_network: true,
            webhook_registered: false
        })
    );
    v["participants"] = json!([]);
    assert!(parse_out(&v).unwrap().is_none());
    for (key, value) in [
        ("companyKey", json!("other_key")),
        ("participantId", json!("DK87654321")),
        ("participantType", json!("GLN")),
        ("documentProfiles", json!(null)),
        ("webhookUrl", json!("https://example.invalid/hook")),
    ] {
        let mut v = participants();
        v["participants"][0][key] = value;
        assert_eq!(parse_out(&v), Err(Error::Protocol), "{key}");
    }
    let mut v = participants();
    v["company"]["companyName"] = json!("Another company");
    assert_eq!(parse_out(&v), Err(Error::Protocol));
}
#[test]
fn duplicate_participants_missing_fields_and_pagination_never_authorize_post() {
    let mut v = participants();
    let duplicate = v["participants"][0].clone();
    v["participants"].as_array_mut().unwrap().push(duplicate);
    assert_eq!(parse_out(&v), Err(Error::Protocol));
    v = participants();
    v["participants"][0]
        .as_object_mut()
        .unwrap()
        .remove("webhookUrl");
    assert_eq!(parse_out(&v), Err(Error::Protocol));
    v = participants();
    v["participants"][0]
        .as_object_mut()
        .unwrap()
        .remove("documentProfiles");
    assert_eq!(parse_out(&v), Err(Error::Protocol));
    v = participants();
    v["nextCursor"] = json!("partial");
    assert_eq!(parse_out(&v), Err(Error::Protocol));
}
