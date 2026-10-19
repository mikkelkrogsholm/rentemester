use super::*;
#[test]
fn signed_micro_values_quantize_once_and_preserve_exact_source() {
    for (raw, six, money) in [
        ("0,004999", "0.004999", "0.00"),
        ("0,005000", "0.005000", "0.01"),
        ("-0,004999", "-0.004999", "0.00"),
        ("-0,005000", "-0.005000", "-0.01"),
        ("1,234567", "1.234567", "1.23"),
        ("-1,235000", "-1.235000", "-1.24"),
        ("-0,000000", "0.000000", "0.00"),
    ] {
        let v = ArchiveDecimal::parse_csv(raw).unwrap();
        assert_eq!(v.decimal(), six);
        assert_eq!(v.money().decimal(), money);
        assert_eq!(serde_json::to_string(&v).unwrap(), format!("\"{six}\""));
    }
    for invalid in ["1.0000001", "1,2,3", "1e2", "+1", "NaN", "9007199254740992"] {
        assert!(ArchiveDecimal::parse_csv(invalid).is_err(), "{invalid}");
    }
    assert!(ArchiveDecimal::parse_csv("100000000000").is_ok());
    let mut total = Money::default();
    for _ in 0..2 {
        total.add(&ArchiveDecimal::parse_csv("0.005").unwrap().money());
    }
    assert_eq!(total.decimal(), "0.02");
}
#[test]
fn strict_source_parser_preserves_quotes_headers_exclusion_and_optional_balances() {
    let files=vec![CsvFile{path:"2024/Posteringer.csv".into(),bytes:b"\xef\xbb\xbfKonto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Moms;Bel\xc3\xb8b;Saldo\r\n6200;Synthetic;2024-06-01;1;Test;\"quoted; \"\"text\"\"\nsecond\";;-0,005000;\r\n".to_vec()},CsvFile{path:"2025/Posteringer.csv".into(),bytes:"Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Momstype;Beløb;Saldo\n6200;Marker;2025-01-01;0;;Primobeholdning;;0;0\n".as_bytes().to_vec()}];
    let p = parse(&files, "DKK").unwrap();
    assert_eq!(p.excluded_latest_year, 2025);
    assert_eq!(p.years.len(), 1);
    assert!(!p.years[0].has_balances);
    assert_eq!(p.years[0].postings[0].text, "quoted; \"text\"\nsecond");
    assert_eq!(p.years[0].postings[0].amount.money().decimal(), "-0.01");
    for path in [
        "../2024/Posteringer.csv",
        "2024/posteringer.csv",
        "2024\\Posteringer.csv",
        "0000/Posteringer.csv",
    ] {
        assert!(canonical_path(path).is_err());
    }
    let mut bad = files.clone();
    bad.push(files[0].clone());
    assert!(parse(&bad, "DKK").is_err());
    let mut bad = files.clone();
    bad[0].bytes = String::from_utf8(bad[0].bytes.clone())
        .unwrap()
        .replace("2024-06-01", "2024-02-30")
        .into_bytes();
    assert!(parse(&bad, "DKK").is_err());
    assert!(parse(&files[..1], "DKK").is_err());
}
#[test]
fn actual_synthetic_source_fixture_parses_all_closed_rows_without_live_year() {
    let files = vec![
        CsvFile {
            path: "2024/Posteringer.csv".into(),
            bytes: include_bytes!("../../../../examples/import-dinero/2024/Posteringer.csv")
                .to_vec(),
        },
        CsvFile {
            path: "2024/SaldoBalance.csv".into(),
            bytes: include_bytes!("../../../../examples/import-dinero/2024/SaldoBalance.csv")
                .to_vec(),
        },
        CsvFile {
            path: "2025/Posteringer.csv".into(),
            bytes: include_bytes!("../../../../examples/import-dinero/2025/Posteringer.csv")
                .to_vec(),
        },
    ];
    let p = parse(&files, "DKK").unwrap();
    assert_eq!(p.years.len(), 1);
    assert_eq!(p.years[0].fiscal_year, 2024);
    assert!(p.years[0].has_balances);
    assert!(!p.years[0].postings.is_empty());
    assert!(!p.years[0].balances.is_empty());
    assert_eq!(p, serde_json::from_str(&p.canonical().unwrap()).unwrap());
}
