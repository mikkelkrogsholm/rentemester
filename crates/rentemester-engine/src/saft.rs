//! Existing bounded Rentemester v3 XML profile, not an official SAF-T schema or submission validator.
//! Input projections come from the admitted company transaction. No SQL/files/authority here.
use crate::{Error, money::Money};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub const PROFILE: &str = "rentemester-dk-saft-v3-ledger-sales-purchases-masterfiles";
pub const RULE: &str = "DK-BOOKKEEPING-SAFT-EXPORT-001";
pub const OUT_OF_SCOPE: [&str; 5] = [
    "bank_statement_transport",
    "payments_collections",
    "fixed_assets",
    "stock_movements",
    "official_schema_submission",
];
pub struct Source {
    pub company: Value,
    pub accounts: Vec<Value>,
    pub customers: Vec<Value>,
    pub suppliers: Vec<Value>,
    pub journal: Vec<Value>,
    pub sales: Vec<Value>,
    pub purchases: Vec<Value>,
}
pub fn refused(message: impl Into<String>) -> Error {
    Error::new("saft_export_refused", message)
}
pub fn amount(v: &Value) -> Result<Money, Error> {
    if v.is_null() {
        return Ok(Money::default());
    }
    Money::parse(
        v.as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| v.to_string())
            .as_str(),
    )
}
pub fn timestamp(ms: i64) -> Result<String, Error> {
    let d = time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
        .map_err(|_| refused("invalid trusted export clock"))?;
    if !(0..=9999).contains(&d.year()) {
        return Err(refused("export clock outside supported range"));
    }
    Ok(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        d.year(),
        u8::from(d.month()),
        d.day(),
        d.hour(),
        d.minute(),
        d.second(),
        d.millisecond()
    ))
}
fn escape(v: &str) -> String {
    v.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
        .replace('\'', "&apos;")
}
struct Xml {
    lines: Vec<String>,
    level: usize,
}
impl Xml {
    fn open(&mut self, name: &str) {
        self.lines
            .push(format!("{}<{name}>", "  ".repeat(self.level)));
        self.level += 1;
    }
    fn close(&mut self, name: &str) {
        self.level -= 1;
        self.lines
            .push(format!("{}</{name}>", "  ".repeat(self.level)));
    }
    fn tag(&mut self, name: &str, v: &Value) {
        if v.is_null() || v.as_str() == Some("") {
            return;
        }
        let s = v
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| v.to_string());
        self.lines.push(format!(
            "{}<{name}>{}</{name}>",
            "  ".repeat(self.level),
            escape(&s)
        ));
    }
    fn fields(&mut self, row: &Value, fields: &[(&str, &str)]) {
        for (tag, key) in fields {
            self.tag(tag, &row[*key]);
        }
    }
    fn money(&mut self, name: &str, v: &Value) -> Result<(), Error> {
        self.tag(name, &json!(amount(v)?.decimal()));
        Ok(())
    }
}
pub struct Rendered {
    pub xml: String,
    pub vat_codes: usize,
    pub document_references: usize,
}
pub fn render(
    source: &Source,
    slug: &str,
    start: &str,
    end: &str,
    generated: &str,
) -> Result<Rendered, Error> {
    crate::trial_balance::Period::new(start, end)?;
    if source.company["cvr"].as_str().is_none_or(str::is_empty) {
        return Err(refused("company cvr is required for SAF-T export"));
    }
    let mut x = Xml {
        lines: vec![
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>".into(),
            format!(
                "<AuditFile xmlns=\"urn:rentemester:dk:saft:v1\" sourceCompanyRoot=\"{}\">",
                escape(slug)
            ),
        ],
        level: 1,
    };
    x.open("Header");
    x.tag("ProfileID", &json!(PROFILE));
    x.tag("AuditFileVersion", &json!("3.0"));
    x.fields(
        &source.company,
        &[
            ("CompanyName", "name"),
            ("RegistrationNumber", "cvr"),
            ("TaxRegistrationNumber", "cvr"),
            ("Country", "country"),
            ("DefaultCurrencyCode", "currency"),
            ("FiscalYearStartMonth", "fiscalYearStartMonth"),
            ("FiscalYearLabelStrategy", "fiscalYearLabelStrategy"),
        ],
    );
    for (k, v) in [
        ("SelectionStartDate", start),
        ("SelectionEndDate", end),
        ("DateCreated", generated),
    ] {
        x.tag(k, &json!(v));
    }
    x.close("Header");
    x.open("MasterFiles");
    x.open("GeneralLedgerAccounts");
    for row in &source.accounts {
        x.open("Account");
        x.fields(
            row,
            &[
                ("AccountID", "accountNo"),
                ("AccountDescription", "name"),
                ("AccountType", "type"),
                ("NormalBalance", "normalBalance"),
                ("TaxCode", "defaultVatCode"),
            ],
        );
        x.close("Account");
    }
    x.close("GeneralLedgerAccounts");
    x.open("Customers");
    for row in &source.customers {
        x.open("Customer");
        x.fields(
            row,
            &[
                ("CustomerID", "customerId"),
                ("CustomerName", "name"),
                ("CustomerTaxID", "vatOrCvr"),
                ("Address", "address"),
                ("Email", "email"),
                ("Telephone", "phone"),
                ("DefaultCurrencyCode", "defaultCurrency"),
                ("PaymentTermsDays", "paymentTermsDays"),
            ],
        );
        x.close("Customer");
    }
    x.close("Customers");
    x.open("Suppliers");
    for row in &source.suppliers {
        x.open("Supplier");
        x.fields(
            row,
            &[
                ("SupplierID", "supplierId"),
                ("SupplierName", "name"),
                ("SupplierTaxID", "vatOrCvr"),
                ("SupplierCountryCode", "countryCode"),
                ("SupplierIdentifierKind", "identifierKind"),
                ("SupplierIdentityStatus", "identityStatus"),
                ("Address", "address"),
                ("Email", "email"),
                ("Telephone", "phone"),
                ("DefaultExpenseAccount", "defaultExpenseAccount"),
                ("DefaultVatTreatment", "defaultVatTreatment"),
            ],
        );
        x.close("Supplier");
    }
    x.close("Suppliers");
    x.close("MasterFiles");
    x.open("GeneralLedgerEntries");
    x.open("Journal");
    x.tag("JournalID", &json!("GENERAL"));
    x.tag("Description", &json!("Rentemester general journal"));
    let mut vat: BTreeMap<String, (Money, Money, usize)> = BTreeMap::new();
    let mut references = std::collections::BTreeSet::new();
    for row in &source.journal {
        x.open("Transaction");
        x.fields(
            row,
            &[
                ("TransactionID", "entryNo"),
                ("TransactionDate", "transactionDate"),
                ("Description", "text"),
            ],
        );
        if !row["documentId"].is_null() {
            let number = row["documentNo"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| refused("journal source document missing"))?;
            references.insert(row["documentId"].to_string());
            x.open("DocumentReference");
            x.tag("SourceDocumentID", &json!(number));
            x.close("DocumentReference");
        }
        for line in row["lines"]
            .as_array()
            .ok_or_else(|| refused("journal lines missing"))?
        {
            x.open("Line");
            x.fields(
                line,
                &[
                    ("AccountID", "accountNo"),
                    ("AccountDescription", "accountName"),
                ],
            );
            x.money("DebitAmount", &line["debitAmount"])?;
            x.money("CreditAmount", &line["creditAmount"])?;
            x.fields(line, &[("TaxCode", "vatCode"), ("LineDescription", "text")]);
            x.close("Line");
            if let Some(code) = line["vatCode"].as_str().filter(|s| !s.is_empty()) {
                let v = vat.entry(code.into()).or_default();
                v.0.add(&amount(&line["debitAmount"])?);
                v.1.add(&amount(&line["creditAmount"])?);
                v.2 += 1;
            }
        }
        x.close("Transaction");
    }
    x.close("Journal");
    x.close("GeneralLedgerEntries");
    x.open("SourceDocuments");
    for (name, rows, sales) in [
        ("SalesInvoices", &source.sales, true),
        ("PurchaseInvoices", &source.purchases, false),
    ] {
        let mut total = Money::default();
        for row in rows {
            total.add(&amount(&row["grossAmount"])?);
        }
        x.open(name);
        x.tag("NumberOfEntries", &json!(rows.len()));
        x.tag(
            "TotalDebit",
            &json!(if sales {
                "0.00".into()
            } else {
                total.decimal()
            }),
        );
        x.tag(
            "TotalCredit",
            &json!(if sales {
                total.decimal()
            } else {
                "0.00".into()
            }),
        );
        for row in rows {
            x.open("Invoice");
            x.fields(
                row,
                &[("InvoiceNo", "invoiceNo"), ("DocumentNo", "documentNo")],
            );
            if sales {
                x.fields(
                    row,
                    &[
                        ("IssueDate", "issueDate"),
                        ("DueDate", "dueDate"),
                        ("CustomerName", "buyerName"),
                        ("CustomerTaxID", "buyerVatOrCvr"),
                        ("SellerName", "sellerName"),
                        ("SellerTaxID", "sellerVatOrCvr"),
                    ],
                );
            } else {
                x.fields(
                    row,
                    &[
                        ("InvoiceDate", "invoiceDate"),
                        ("SupplierName", "supplierName"),
                        ("SupplierTaxID", "supplierVatOrCvr"),
                        ("SupplierCountryCode", "supplierCountryCode"),
                        ("SupplierIdentifierKind", "supplierIdentifierKind"),
                        ("SupplierIdentityStatus", "supplierIdentityStatus"),
                    ],
                );
            }
            x.tag("DocumentCurrencyCode", &row["currency"]);
            x.money("NetTotal", &row["netAmount"])?;
            x.money("TaxPayable", &row["vatAmount"])?;
            x.money("GrossTotal", &row["grossAmount"])?;
            if sales {
                if let Some(lines) = row["lines"].as_array() {
                    for (i, line) in lines.iter().enumerate() {
                        x.open("Line");
                        x.tag("LineNumber", &json!(i + 1));
                        x.tag("Description", &line["description"]);
                        if line["quantity"].is_number() {
                            x.tag("Quantity", &line["quantity"]);
                        }
                        x.money("UnitPrice", &line["unitPriceExVat"])?;
                        x.money("CreditAmount", &line["lineTotalExVat"])?;
                        x.tag("TaxClassification", &line["taxClassification"]);
                        if line["vatRate"].is_number() {
                            x.tag("VatRate", &line["vatRate"]);
                        }
                        x.close("Line");
                    }
                }
            } else if let Some(lines) = row["purchaseVatLines"].as_array() {
                for line in lines {
                    x.open("TaxLine");
                    x.tag("TaxClassification", &line["classification"]);
                    x.money("TaxBase", &line["netAmount"])?;
                    x.money("TaxAmount", &line["vatAmount"])?;
                    x.close("TaxLine");
                }
            }
            x.close("Invoice");
        }
        x.close(name);
    }
    x.close("SourceDocuments");
    x.open("TaxSummary");
    x.tag("NumberOfTaxCodes", &json!(vat.len()));
    for (code, (debit, credit, count)) in &vat {
        x.open("TaxCodeDetail");
        x.tag("TaxCode", &json!(code));
        x.tag("DebitAmount", &json!(debit.decimal()));
        x.tag("CreditAmount", &json!(credit.decimal()));
        x.tag("NumberOfLines", &json!(count));
        x.close("TaxCodeDetail");
    }
    x.close("TaxSummary");
    x.close("AuditFile");
    Ok(Rendered {
        xml: x.lines.join("\n"),
        vat_codes: vat.len(),
        document_references: references.len(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Source {
        Source {
            company: json!({"name":"Synthetic <&> company","cvr":"12345678","country":"DK","currency":"DKK","fiscalYearStartMonth":7,"fiscalYearLabelStrategy":"end_year"}),
            accounts: vec![
                json!({"accountNo":"A","name":"Archived synthetic asset","type":"asset","normalBalance":"debit","defaultVatCode":"INPUT"}),
            ],
            customers: vec![
                json!({"customerId":1,"name":"Synthetic customer","vatOrCvr":"DK87654321","paymentTermsDays":7}),
            ],
            suppliers: vec![
                json!({"supplierId":2,"name":"Synthetic supplier","countryCode":"US","identifierKind":"non_eu","identityStatus":"resolved"}),
            ],
            journal: vec![
                json!({"id":1,"entryNo":"SYN-1","transactionDate":"2026-07-01","text":"Synthetic","documentId":3,"documentNo":"DOC-3","lines":[{"accountNo":"A","accountName":"Asset","debitAmount":"9007199254740993.01","creditAmount":"0","vatCode":"INPUT"},{"accountNo":"B","debitAmount":"0","creditAmount":"9007199254740993.01"}]}),
            ],
            sales: vec![
                json!({"invoiceNo":"SYN-S","documentNo":"DOC-S","issueDate":"2026-07-01","dueDate":"2026-07-08","currency":"DKK","netAmount":"1.01","vatAmount":"0.25","grossAmount":"1.26","lines":[{"quantity":1.005,"vatRate":25,"unitPriceExVat":"1.005","lineTotalExVat":"1.01"}]}),
            ],
            purchases: vec![
                json!({"documentNo":"DOC-P","invoiceNo":"SYN-P","invoiceDate":"2026-07-01","supplierCountryCode":"US","supplierIdentifierKind":"non_eu","supplierIdentityStatus":"resolved","currency":"USD","grossAmount":"125.01","netAmount":"100.01","vatAmount":"25.00","purchaseVatLines":[{"classification":"exempt","netAmount":"100.01","vatAmount":"0"}]}),
            ],
        }
    }
    #[test]
    fn v3_sections_escape_exact_money_fractional_units_and_source_references() {
        let s = source();
        let a = render(
            &s,
            "synthetic",
            "2026-07-01",
            "2027-06-30",
            "2027-07-01T00:00:00.000Z",
        )
        .unwrap();
        let b = render(
            &s,
            "synthetic",
            "2026-07-01",
            "2027-06-30",
            "2027-07-01T00:00:00.000Z",
        )
        .unwrap();
        assert_eq!(a.xml, b.xml);
        assert_eq!(a.document_references, 1);
        assert_eq!(a.vat_codes, 1);
        for expected in [
            PROFILE,
            "<AuditFileVersion>3.0</AuditFileVersion>",
            "Synthetic &lt;&amp;&gt; company",
            "<Quantity>1.005</Quantity>",
            "<UnitPrice>1.01</UnitPrice>",
            "<DebitAmount>9007199254740993.01</DebitAmount>",
            "<TaxSummary>",
            "<TotalDebit>125.01</TotalDebit>",
            "<SourceDocumentID>DOC-3</SourceDocumentID>",
            "<CustomerID>1</CustomerID>",
            "<SupplierID>2</SupplierID>",
            "<DocumentCurrencyCode>USD</DocumentCurrencyCode>",
        ] {
            assert!(a.xml.contains(expected), "missing {expected}");
        }
        assert!(!a.xml.ends_with('\n'));
    }
    #[test]
    fn missing_company_reference_bad_date_or_amount_cannot_render() {
        let mut s = source();
        s.company["cvr"] = Value::Null;
        assert!(render(&s, "s", "2026-01-01", "2026-12-31", "x").is_err());
        s = source();
        s.journal[0]["documentNo"] = Value::Null;
        assert!(render(&s, "s", "2026-01-01", "2026-12-31", "x").is_err());
        s = source();
        s.journal[0]["lines"][0]["debitAmount"] = json!("NaN");
        assert!(render(&s, "s", "2026-01-01", "2026-12-31", "x").is_err());
        assert!(render(&source(), "s", "2026-02-30", "2026-12-31", "x").is_err());
        assert_eq!(
            timestamp(1767225600123).unwrap(),
            "2026-01-01T00:00:00.123Z"
        );
        assert!(timestamp(i64::MAX).is_err());
    }
}
