use super::*;
use crate::{dinero_import::Voucher, fiscal_year::Date};
use serde_json::json;
fn parsed() -> ParsedImport {
    ParsedImport {
        parser_contract: super::super::CONTRACT.into(),
        declared_currency: "DKK".into(),
        latest_year: 2025,
        cut_over_date: None,
        control_date: None,
        chart: vec![],
        company_master: json!({}),
        role_proposals: vec![],
        opening_lines: vec![],
        vouchers: vec![Voucher {
            voucher_ref: "1".into(),
            transaction_date: Date::parse("2025-02-01").unwrap(),
            text: "Synthetic historical voucher".into(),
            entry_type: "Finansbilag".into(),
            lines: vec![],
        }],
        controls: vec![],
        schedule: None,
        unmapped_vat_codes: vec![],
        notices: vec![],
    }
}
fn file(path: &str) -> SourceFile {
    SourceFile {
        path: path.into(),
        bytes: b"exact raw original bytes".to_vec(),
    }
}
#[test]
fn latest_root_year_nested_case_and_loose_receipts_preserve_source_identity() {
    let p = parsed();
    let unchanged = serde_json::to_value(&p).unwrap();
    let files = vec![
        file("2024/Bilag/older-Bilag-999.exe"),
        file("2025/bILaG/nested/arbitrary-FaKtUrA-1.PdF"),
        file("2025/FAKTURA/oldname-Bilag-1.JpEg"),
        file("IKKE-BOGFØRTE-BILAG/nested/loose.PNG"),
        file("Ikke-bogførte-bilag/readme.txt"),
    ];
    let r = plan(&files, &p).unwrap();
    assert_eq!(r.year, Some(2025));
    assert_eq!(r.receipts.len(), 3);
    assert_eq!(
        r.receipts[0].source_path,
        "2025/FAKTURA/oldname-Bilag-1.JpEg"
    );
    assert_eq!(r.receipts[0].document_type, "issued_invoice_pdf");
    assert_eq!(r.receipts[0].mime_type, "image/jpeg");
    assert_eq!(r.receipts[1].document_type, "cash_register_receipt");
    assert_eq!(r.receipts[1].voucher_ref.as_deref(), Some("1"));
    assert_eq!(r.receipts[1].original_filename, "arbitrary-FaKtUrA-1.PdF");
    assert_eq!(r.receipts[2].voucher_ref, None);
    assert!(r.receipts[2].unbooked);
    assert_eq!(serde_json::to_value(&p).unwrap(), unchanged);
}
#[test]
fn missing_unknown_and_noncanonical_voucher_references_refuse() {
    for path in [
        "2025/Bilag/receipt.pdf",
        "2025/Bilag/Bilag-1.pdf",
        "2025/Bilag/receipt-Bilag-2.pdf",
        "2025/Bilag/receipt-Bilag-0001.pdf",
        "2025/Bilag/receipt-Bilag-1a.pdf",
        "2025/Bilag/receipt-Bilag-1.pdf.exe",
    ] {
        assert!(plan(&[file(path)], &parsed()).is_err(), "{path}");
    }
    let mut p = parsed();
    p.vouchers[0].voucher_ref = "0001".into();
    assert_eq!(
        plan(&[file("2025/Bilag/receipt-Bilag-0001.pdf")], &p)
            .unwrap()
            .receipts[0]
            .voucher_ref
            .as_deref(),
        Some("0001")
    );
}
#[test]
fn unsupported_booked_extensions_refuse_but_loose_inventory_is_untouched() {
    let p = parsed();
    for ext in ["txt", "json", "xml", "exe", ""] {
        assert!(plan(&[file(&format!("2025/Bilag/receipt-Bilag-1.{ext}"))], &p).is_err());
    }
    let r = plan(
        &[
            file("Ikke-bogførte-bilag/opaque.bin"),
            file("2025/other/file.exe"),
        ],
        &p,
    )
    .unwrap();
    assert!(r.receipts.is_empty());
    assert_eq!(r.year, Some(2025));
}
#[test]
fn duplicate_content_keeps_every_path_and_does_not_validate_new_byte_magic() {
    let files = vec![
        file("2025/Bilag/a-Bilag-1.pdf"),
        file("2025/Faktura/b-Faktura-1.pdf"),
        file("Ikke-bogførte-bilag/a.pdf"),
    ];
    let r = plan(&files, &parsed()).unwrap();
    assert_eq!(r.receipts.len(), 3);
    assert!(r.receipts.iter().all(|r| r.sha256 == r.receipts[0].sha256));
    assert_eq!(r.receipts[0].size_bytes, 24);
    assert_ne!(r.receipts[0].source_path, r.receipts[1].source_path);
    assert_eq!(r.receipts[2].voucher_ref, None);
}
#[test]
fn other_year_voucher_cannot_bind_same_live_reference() {
    let f = vec![
        file("2025/Posteringer.csv"),
        file("2026/Bilag/anything-Bilag-1.pdf"),
    ];
    let e = plan(&f, &parsed()).unwrap_err();
    assert!(e.message.contains("receipt year differs"));
}
#[test]
fn no_root_year_still_selects_unbooked_and_ignores_embedded_years() {
    let r = plan(
        &[
            file("nested/2025/Bilag/x-Bilag-1.pdf"),
            file("Ikke-bogførte-bilag/x.jpg"),
        ],
        &parsed(),
    )
    .unwrap();
    assert_eq!(r.year, None);
    assert_eq!(r.receipts.len(), 1);
    assert!(r.receipts[0].unbooked);
}
