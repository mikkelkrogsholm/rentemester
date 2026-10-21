use super::*;
fn file(path: &str, s: &str) -> SourceFile {
    SourceFile {
        path: path.into(),
        bytes: s.as_bytes().to_vec(),
    }
}
fn fixture() -> Vec<SourceFile> {
    vec![
        file(
            "2025/Kontoplan.csv",
            "Nummer;Navn;Type;Momstype;Total\n5520;Tilgodehavender fra salg;Aktiv;;\n5510;Bankkonto;Aktiv;;\n60010;Overført resultat;Passiv;;\n1000;Omsætning;Indtægt;;\n",
        ),
        file(
            "2025/Posteringer.csv",
            "Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Momstype;Beløb;Saldo\n5520;Debitor;2025-01-01;0;;Primobeholdning;;30000,000000;\n60010;Egenkapital;2025-01-01;0;;Primobeholdning;;-30000,000000;\n5510;Bank;2025-03-05;1;Betaling;Kundebetaling;;30000,000000;\n5520;Debitor;2025-03-05;1;Betaling;Kundebetaling;;-30000,000000;\n5520;Debitor;2025-03-20;2;Faktura;Ekstern faktura;;25000,000000;\n1000;Omsætning;2025-03-20;2;Faktura;Ekstern faktura;;-25000,000000;\n1000;Omsætning;2025-03-21;3;Kreditnota;Ekstern kreditnota;;500,000000;\n5520;Debitor;2025-03-21;3;Kreditnota;Ekstern kreditnota;;-500,000000;\n",
        ),
        file(
            "2025/SaldoBalance.csv",
            "Konto;Kontonavn;Beløb\n5520;Debitor;24500,000000\n",
        ),
    ]
}
#[test]
fn latest_year_opening_vouchers_and_authoritative_controls() {
    let p = parse(&fixture(), "DKK").unwrap();
    assert_eq!(p.latest_year, 2025);
    assert_eq!(p.cut_over_date.unwrap().to_string(), "2025-01-01");
    assert_eq!(p.control_date.unwrap().to_string(), "2025-03-21");
    assert_eq!(p.opening_lines.len(), 2);
    assert_eq!(p.vouchers.len(), 3);
    assert_eq!(p.controls[0].amount.decimal(), "24500.00");
    assert!(
        p.role_proposals
            .iter()
            .any(|r| r.role == "debtors" && r.account_no == "5520")
    );
    assert_eq!(p.chart[2].kind, "equity");
    assert!(p.notices.is_empty());
}
#[test]
fn no_primo_preserves_chart_only_dispatch_and_master_is_optional() {
    let mut f = fixture();
    f[1].bytes = String::from_utf8(f[1].bytes.clone())
        .unwrap()
        .lines()
        .filter(|l| !l.contains("Primobeholdning"))
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    let p = parse(&f, "DKK").unwrap();
    assert!(p.vouchers.is_empty());
    assert!(p.cut_over_date.is_none());
    assert!(p.notices[0].contains("chart-only"));
    assert_eq!(p.company_master, json!({}));
}
#[test]
fn bad_dates_unbalanced_vouchers_paths_currency_and_unknown_accounts_fail() {
    for n in 0..6 {
        let mut f = fixture();
        match n {
            0 => {
                f[1].bytes = String::from_utf8(f[1].bytes.clone())
                    .unwrap()
                    .replace("2025-03-21", "2025-02-30")
                    .into_bytes()
            }
            1 => {
                f[1].bytes = String::from_utf8(f[1].bytes.clone())
                    .unwrap()
                    .replace("-500,000000", "-500,010000")
                    .into_bytes()
            }
            2 => f[0].path = "../2025/Kontoplan.csv".into(),
            3 => f.push(file("2025/Kontoplan.csv", "duplicate")),
            4 => {
                f[1].bytes = String::from_utf8(f[1].bytes.clone())
                    .unwrap()
                    .replace("1000;Omsætning", "9999;Unknown")
                    .into_bytes()
            }
            _ => f[1].path = "2024/Posteringer.csv".into(),
        };
        assert!(parse(&f, "DKK").is_err(), "{n}");
    }
    assert!(parse(&fixture(), "EUR").is_err());
}
#[test]
fn source_vat_mapping_and_canonical_control_roles() {
    let mut f = fixture();
    f[0].bytes.extend_from_slice("64000;Salgsmoms;Passiv;;\n64060;Købsmoms;Passiv;;\n2000;Kontor;Udgift;I25 - Dansk købsmoms;\n2001;Varer;Udgift;IEUV - Varekøb EU;\n".as_bytes());
    let p = parse(&f, "DKK").unwrap();
    assert_eq!(
        p.chart
            .iter()
            .find(|a| a.account_no == "2000")
            .unwrap()
            .default_vat_code
            .as_deref(),
        Some("DK_PURCHASE_25")
    );
    assert_eq!(
        p.chart
            .iter()
            .find(|a| a.account_no == "64060")
            .unwrap()
            .normal_balance,
        "debit"
    );
    assert_eq!(p.unmapped_vat_codes, ["IEUV - Varekøb EU"]);
}
#[test]
fn six_decimal_rounding_is_integer_only() {
    assert_eq!(decimal("1,005000").unwrap().decimal(), "1.01");
    assert_eq!(decimal("-1,005000").unwrap().decimal(), "-1.01");
    assert!(decimal("1,0000001").is_err());
}
#[test]
fn reverse_charge_source_pattern_is_inferred_or_refused_without_guessing() {
    let mut f = fixture();
    f[0].bytes.extend_from_slice(
        "64040;Moms udenlandsk;Passiv;;\n64060;Købsmoms;Passiv;;\n2000;Kontor;Udgift;I25;\n"
            .as_bytes(),
    );
    f[1].bytes.extend_from_slice("2000;Kontor;2025-04-01;4;Køb;Service;;100,000000;\n5510;Bank;2025-04-01;4;Køb;Service;;-100,000000;\n64040;Moms;2025-04-01;4;Køb;Service;;-25,000000;\n64060;Moms;2025-04-01;4;Køb;Service;;25,000000;\n".as_bytes());
    let p = parse(&f, "DKK").unwrap();
    assert_eq!(
        p.vouchers[3].lines[0].vat_code.as_deref(),
        Some("EU_SERVICE_REVERSE_CHARGE")
    );
    f[1].bytes = String::from_utf8(f[1].bytes.clone())
        .unwrap()
        .replace("Service;;100,000000", "Service;I25;100,000000")
        .into_bytes();
    assert!(
        parse(&f, "DKK")
            .unwrap_err()
            .message
            .contains("conflicting")
    );
}

#[test]
fn source_display_labels_and_codes_share_exact_vat_identity() {
    assert_eq!(
        vat("Ydelseskøb EU (rubrik A - ydelser)"),
        Some("EU_SERVICE_REVERSE_CHARGE")
    );
    assert_eq!(vat("U25-Dansk salgsmoms"), Some("DK_SALE_25"));
    assert_eq!(vat("unknown"), None);
}
