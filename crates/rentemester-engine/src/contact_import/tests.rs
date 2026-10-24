use super::*;
use serde_json::json;
#[test]
fn one_strict_csv_parser_detects_quotes_bom_multiline_and_all_source_normalizations() {
    let raw = "\u{feff}\"Kontaktnavn\";CVR-nummer;Kontakttype;Landekode;Adresse;Postnummer;By;Total salg;Total køb;Ukendt\r\n\"Firma; A\";dk 12345678;Company;DK;\"Gade 1, \";8000;Aarhus;1;2;\"ignored\nsecond line\"\r\nIngen;;Other;;;;;0;0;x\n";
    let p = parse(raw).unwrap();
    assert_eq!(p.contacts.len(), 2);
    let c = &p.contacts[0];
    assert_eq!(c.name, "Firma; A");
    assert_eq!(c.vat_or_cvr.as_deref(), Some("DK12345678"));
    assert_eq!(c.danish_cvr.as_deref(), Some("12345678"));
    assert_eq!(c.address.as_deref(), Some("Gade 1, 8000 Aarhus"));
    assert_eq!(c.roles(), vec![Role::Customer, Role::Vendor]);
    assert_eq!(p.contacts[1].roles(), vec![Role::Vendor]);
    for raw in [
        "Kontaktnavn;CVR-nummer;Kontakttype;Kontaktnavn\na;;x;b",
        "Kontaktnavn;CVR-nummer;Kontakttype\n\"unclosed;x;y",
        "Kontaktnavn;CVR-nummer;Kontakttype\n\"a\"oops;x;y",
        "Other;Format\na;b",
    ] {
        assert!(parse(raw).is_err(), "{raw}");
    }
    let p =
        parse("Kontaktnavn;CVR-nummer;Kontakttype;Total salg\nInvalid;;Other;\"1,25\"\n").unwrap();
    assert!(p.contacts.is_empty());
    assert_eq!(p.errors.len(), 1);
}
#[test]
fn cvr_mapping_preserves_temporal_source_publicity_full_business_metadata_and_identity() {
    let e = json!({"cvrNummer":12345678,"navne":[{"navn":"Old","periode":{"gyldigTil":"2020-01-01"}},{"navn":"Current","periode":{"gyldigTil":null}}],"beliggenhedsadresse":[{"vejnavn":"Gade","husnummerFra":1,"bogstavFra":"A","postnummer":8000,"postdistrikt":"By","kommune":{"kommuneKode":751},"periode":{"gyldigTil":null}}],"virksomhedsform":[{"virksomhedsformkode":80,"kortBeskrivelse":"ApS","langBeskrivelse":"Anpartsselskab"}],"hovedbranche":[{"branchekode":"123456","branchetekst":"Synthetic"}],"elektroniskPost":[{"kontaktoplysning":"hidden@example.invalid","hemmelig":true,"periode":{"gyldigTil":null}},{"kontaktoplysning":"public@example.invalid","periode":{"gyldigTil":null}}],"attributter":[{"type":"REGNSKABSÅR_START","vaerdier":[{"vaerdi":"--07-01","periode":{"gyldigTil":null}}]},{"type":"REVISION_FRAVALGT","vaerdier":[{"vaerdi":"true"}]},{"type":"KAPITAL","vaerdier":[{"vaerdi":"40000"}]}],"virksomhedMetadata":{"stiftelsesDato":"2020-01-01","nyesteAarsbeskaeftigelse":{"antalAnsatte":3}},"reklamebeskyttet":true,"deltagerRelation":[{"deltager":{"navne":[{"navn":"Synthetic Manager"}]},"organisationer":[{"hovedtype":"LEDELSESORGAN","medlemsData":[{"attributter":[{"type":"FUNKTION","vaerdier":[{"vaerdi":"Direktion","periode":{"gyldigTil":null}}]}]}]}]}]});
    let c = CvrCompany::map(&e, "12345678").unwrap();
    assert_eq!(c.name, "Current");
    assert_eq!(c.full_address().as_deref(), Some("Gade 1A, 8000 By"));
    assert_eq!(c.email.as_deref(), Some("public@example.invalid"));
    assert_eq!(c.fiscal_year_start_month, Some(7));
    assert_eq!(c.audit_waived, Some(true));
    assert_eq!(c.management[0].role, "Direktion");
    assert_eq!(c.share_capital.unwrap().to_string(), "40000");
    assert!(CvrCompany::map(&e, "87654321").is_err());
}

#[test]
fn original_headers_are_explicit_and_history_overrides_both_fallbacks() {
    let raw = "\u{feff} KoNtAkTnAvN ;Total salg;Total køb\n\"No; history\";;\nSales;-2;0\nPurchases;0;3\nBoth;1;1\n";
    assert!(parse(raw).is_err());
    let p = parse_original(raw).unwrap();
    assert!(p.errors.is_empty());
    for fallback in [Role::Customer, Role::Vendor] {
        let roles: Vec<_> = p
            .contacts
            .iter()
            .map(|c| c.roles_with_default(fallback))
            .collect();
        assert_eq!(
            roles,
            vec![
                vec![fallback],
                vec![Role::Customer],
                vec![Role::Vendor],
                vec![Role::Customer, Role::Vendor]
            ]
        );
    }
    assert!(parse_original("Kontaktnavn\nMinimal\n").is_ok());
    for raw in [
        "CVR-nummer\n12345678",
        "Kontaktnavn;KONTAKTNAVN\na;b",
        "Kontaktnavn\n\"bad",
    ] {
        assert!(parse_original(raw).is_err());
    }
    let p = parse_original("Kontaktnavn;Total salg;Betalingsfrist i dage\n;1;0\nBad;NaN;0\nUnsafe;0;9007199254740992\nGood;0;730\n").unwrap();
    assert_eq!(p.contacts.len(), 1);
    assert_eq!(p.errors.len(), 3);
}

#[test]
fn default_role_wire_is_exact_optional_and_appended_after_old_sequence() {
    let old = json!({"fileName":"contacts.csv","content":"Kontaktnavn\nA","confirm":true});
    assert_eq!(
        serde_json::from_value::<Input>(old.clone())
            .unwrap()
            .default_role,
        None
    );
    let seq: Input =
        serde_json::from_value(json!(["contacts.csv", "Kontaktnavn\nA", false, true])).unwrap();
    assert!(seq.confirm);
    assert!(!seq.enrich_cvr);
    assert_eq!(seq.default_role, None);
    for (value, expected) in [
        (json!("customer"), Role::Customer),
        (json!("vendor"), Role::Vendor),
    ] {
        let mut v = old.clone();
        v["defaultRole"] = value;
        assert_eq!(
            serde_json::from_value::<Input>(v).unwrap().default_role,
            Some(expected)
        );
    }
    for value in [
        json!("Customer"),
        json!("both"),
        json!(""),
        json!(false),
        json!(1),
        json!({}),
        json!([]),
    ] {
        let mut v = old.clone();
        v["defaultRole"] = value;
        assert!(serde_json::from_value::<Input>(v).is_err());
    }
    let mut v = old;
    v["actor"] = json!("user:untrusted");
    assert!(serde_json::from_value::<Input>(v).is_err());
    let seq: Input = serde_json::from_value(json!([
        "contacts.csv",
        "Kontaktnavn\nA",
        false,
        true,
        "customer"
    ]))
    .unwrap();
    assert_eq!(seq.default_role, Some(Role::Customer));
}
