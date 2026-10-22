//! Deterministic OIOUBL 2.02 over an immutable issued snapshot, never live contacts.
//! Full issued snapshots with source tax classifications and original currency. Unlike the old
//! TS formatter, unit prices retain exact decimal precision: rounding 0.335 to
//! 0.34 would contradict a recorded quantity 3 / line base 1.01. No totals are repaired.
use crate::{Error, decimal, fiscal_year::Date, money::Money};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
const MAX_XML: usize = 10 * 1024 * 1024;
const MAX_TEXT: usize = 16 * 1024;
const AGENCY: &str = "320";
const TAX_ID: &str = "urn:oioubl:id:taxcategoryid-1.1";
const TAX_SCHEME: &str = "urn:oioubl:id:taxschemeid-1.1";
#[derive(Debug, PartialEq, Eq)]
pub struct Export {
    pub xml: String,
    pub sha256: String,
    pub receiver_endpoint_id: String,
}
fn refused(message: &str) -> Error {
    Error::new("public_einvoice_refused", message)
}
fn xml_char(c: char) -> bool {
    matches!(c,'\t'|'\n'|'\r'|'\u{20}'..='\u{d7ff}'|'\u{e000}'..='\u{fffd}'|'\u{10000}'..='\u{10ffff}')
}
fn text(value: &str) -> Result<&str, Error> {
    if value.len() > MAX_TEXT || value.chars().any(|c| !xml_char(c)) {
        return Err(refused("XML text is invalid or exceeds 16 KiB"));
    }
    Ok(value)
}
fn required(value: &Value) -> Result<&str, Error> {
    value
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| refused("Required invoice text is missing"))
        .and_then(text)
}
fn optional(value: &Value) -> Result<Option<&str>, Error> {
    if value.is_null() {
        return Ok(None);
    }
    let s = value
        .as_str()
        .ok_or_else(|| refused("Invoice text has wrong type"))?;
    text(s)?;
    Ok((!s.trim().is_empty()).then_some(s))
}
fn whitespace(c: char) -> bool {
    c != '\u{85}' && (c.is_whitespace() || c == '\u{feff}')
}
fn ean(value: &Value) -> Result<String, Error> {
    let raw = required(value)?;
    let digits = raw.chars().filter(|c| !whitespace(*c)).collect::<String>();
    if digits.len() != 13 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(refused("Buyer EAN must contain 13 digits"));
    }
    Ok(digits)
}
fn seller_endpoint(value: &Value) -> Result<String, Error> {
    let raw = required(value)?.trim();
    let raw = if raw.get(..2).is_some_and(|s| s.eq_ignore_ascii_case("DK")) {
        &raw[2..]
    } else {
        raw
    };
    if raw
        .chars()
        .any(|c| !c.is_ascii_digit() && !whitespace(c) && !matches!(c, '-' | '.'))
    {
        return Err(refused("Seller must have a Danish 8-digit CVR"));
    }
    let digits = raw.chars().filter(char::is_ascii_digit).collect::<String>();
    if digits.len() != 8 {
        return Err(refused("Seller must have a Danish 8-digit CVR"));
    }
    Ok(format!("DK{digits}"))
}
/// Canonical non-exponent decimal, preserving the exact Number value without float arithmetic.
fn number(value: &Value) -> Result<String, Error> {
    let n = value
        .as_number()
        .ok_or_else(|| refused("Invoice amount/quantity must be a JSON number"))?;
    let raw = n.to_string();
    let (negative, raw) = raw
        .strip_prefix('-')
        .map_or((false, raw.as_str()), |s| (true, s));
    let (mantissa, exponent) = raw
        .split_once(['e', 'E'])
        .map_or(Ok((raw, 0)), |(m, e)| e.parse::<i32>().map(|e| (m, e)))
        .map_err(|_| refused("Decimal exponent invalid"))?;
    if !(-400..=400).contains(&exponent) {
        return Err(refused("Decimal exponent exceeds bound"));
    }
    let mut digits = mantissa.replace('.', "");
    let point = mantissa.find('.').unwrap_or(mantissa.len()) as i32 + exponent;
    let mut rendered = if point <= 0 {
        format!("0.{}{}", "0".repeat((-point) as usize), digits)
    } else if point as usize >= digits.len() {
        let padding = "0".repeat(point as usize - digits.len());
        digits.push_str(&padding);
        digits
    } else {
        digits.insert(point as usize, '.');
        digits
    };
    if rendered.contains('.') {
        while rendered.ends_with('0') {
            rendered.pop();
        }
        if rendered.ends_with('.') {
            rendered.pop();
        }
    }
    if negative && rendered != "0" {
        rendered.insert(0, '-');
    }
    if rendered.len() > 2048 {
        return Err(refused("Decimal exceeds bound"));
    }
    Ok(rendered)
}
fn money(value: &Value) -> Result<Money, Error> {
    let raw = number(value)?;
    let m = Money::parse(&raw).map_err(|_| refused("Money decimal invalid"))?;
    if m.is_negative()
        || decimal::compare(&raw, &m.decimal()).map_err(|_| refused("Money decimal invalid"))?
            != Ordering::Equal
    {
        return Err(refused("Recorded amounts must be nonnegative exact cents"));
    }
    Ok(m)
}
fn fraction(value: &Value) -> Result<String, Error> {
    let raw = number(value)?;
    if !decimal::positive(&raw).map_err(|_| refused("VAT rate invalid"))? {
        return Err(refused("Taxable VAT rate must be positive"));
    }
    Ok(raw)
}
// Formatting only: rate is already a canonical nonnegative decimal. Move the
// decimal point without quantizing the rate used by the shared arithmetic.
fn percent(rate: &str) -> String {
    let (whole, fraction) = rate.split_once('.').unwrap_or((rate, ""));
    let padded = format!("{fraction}00");
    let integer = format!("{whole}{}", &padded[..2]);
    let integer = integer.trim_start_matches('0');
    let integer = if integer.is_empty() { "0" } else { integer };
    let tail = padded[2..].trim_end_matches('0');
    if tail.is_empty() {
        integer.into()
    } else {
        format!("{integer}.{tail}")
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Tax {
    category: &'static str,
    rate: String,
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
struct Xml {
    value: String,
    currency: String,
}
impl Xml {
    fn line(&mut self, line: &str) -> Result<(), Error> {
        if self
            .value
            .len()
            .saturating_add(line.len())
            .saturating_add(1)
            > MAX_XML
        {
            return Err(refused("OIOUBL exceeds 10 MiB"));
        }
        if !self.value.is_empty() {
            self.value.push('\n');
        }
        self.value.push_str(line);
        Ok(())
    }
    fn tag(
        &mut self,
        indent: &str,
        name: &str,
        value: &str,
        attrs: &[(&str, &str)],
    ) -> Result<(), Error> {
        text(value)?;
        let attrs = attrs
            .iter()
            .map(|(k, v)| format!(" {k}=\"{}\"", escape(v)))
            .collect::<String>();
        self.line(&format!(
            "{indent}<{name}{attrs}>{}</{name}>",
            escape(value)
        ))
    }
    fn amount(&mut self, indent: &str, name: &str, m: &Money) -> Result<(), Error> {
        self.tag(
            indent,
            name,
            &m.decimal(),
            &[("currencyID", &self.currency.clone())],
        )
    }
    fn tax_category(&mut self, indent: &str, tax: &Tax, reason: Option<&str>) -> Result<(), Error> {
        self.line(&format!("{indent}<cac:TaxCategory>"))?;
        self.tax_fields(&format!("{indent}  "), tax, reason)?;
        self.line(&format!("{indent}</cac:TaxCategory>"))
    }
    fn tax_fields(&mut self, indent: &str, tax: &Tax, reason: Option<&str>) -> Result<(), Error> {
        self.tag(
            indent,
            "cbc:ID",
            tax.category,
            &[("schemeID", TAX_ID), ("schemeAgencyID", AGENCY)],
        )?;
        self.tag(indent, "cbc:Percent", &percent(&tax.rate), &[])?;
        if let Some(reason) = reason {
            if tax.category == "ReverseCharge" {
                self.tag(indent, "cbc:TaxExemptionReasonCode", "VATEX-EU-AE", &[])?;
            }
            self.tag(indent, "cbc:TaxExemptionReason", reason, &[])?;
        }
        self.line(&format!("{indent}<cac:TaxScheme>"))?;
        self.tag(
            &format!("{indent}  "),
            "cbc:ID",
            "63",
            &[("schemeID", TAX_SCHEME), ("schemeAgencyID", AGENCY)],
        )?;
        self.tag(&format!("{indent}  "), "cbc:Name", "Moms", &[])?;
        self.line(&format!("{indent}</cac:TaxScheme>"))
    }
    fn subtotal(
        &mut self,
        indent: &str,
        base: &Money,
        vat: &Money,
        tax: &Tax,
        reason: Option<&str>,
    ) -> Result<(), Error> {
        self.line(&format!("{indent}<cac:TaxSubtotal>"))?;
        self.amount(&format!("{indent}  "), "cbc:TaxableAmount", base)?;
        self.amount(&format!("{indent}  "), "cbc:TaxAmount", vat)?;
        self.tax_category(&format!("{indent}  "), tax, reason)?;
        self.line(&format!("{indent}</cac:TaxSubtotal>"))
    }
    fn address(&mut self, address: &str) -> Result<(), Error> {
        self.line("      <cac:PostalAddress>")?;
        self.tag(
            "        ",
            "cbc:AddressFormatCode",
            "Unstructured",
            &[
                ("listID", "urn:oioubl:codelist:addressformatcode-1.1"),
                ("listAgencyID", AGENCY),
            ],
        )?;
        self.line("        <cac:AddressLine>")?;
        self.tag("          ", "cbc:Line", address.trim(), &[])?;
        self.line("        </cac:AddressLine>")?;
        self.line("      </cac:PostalAddress>")
    }
    fn party(
        &mut self,
        kind: &str,
        party: &Value,
        endpoint: &str,
        seller: bool,
    ) -> Result<(), Error> {
        let name = required(&party["name"])?;
        let address = required(&party["address"])?;
        self.line(&format!("  <cac:{kind}>"))?;
        self.line("    <cac:Party>")?;
        self.tag(
            "      ",
            "cbc:EndpointID",
            endpoint,
            &[("schemeID", if seller { "DK:CVR" } else { "GLN" })],
        )?;
        self.line("      <cac:PartyName>")?;
        self.tag("        ", "cbc:Name", name, &[])?;
        self.line("      </cac:PartyName>")?;
        self.address(address)?;
        if seller {
            self.line("      <cac:PartyTaxScheme>")?;
            self.tag(
                "        ",
                "cbc:CompanyID",
                endpoint,
                &[("schemeID", "DK:SE")],
            )?;
            self.line("        <cac:TaxScheme>")?;
            self.tag(
                "          ",
                "cbc:ID",
                "63",
                &[("schemeID", TAX_SCHEME), ("schemeAgencyID", AGENCY)],
            )?;
            self.tag("          ", "cbc:Name", "Moms", &[])?;
            self.line("        </cac:TaxScheme>")?;
            self.line("      </cac:PartyTaxScheme>")?;
            self.line("      <cac:PartyLegalEntity>")?;
            self.tag("        ", "cbc:RegistrationName", name, &[])?;
            self.tag(
                "        ",
                "cbc:CompanyID",
                endpoint,
                &[("schemeID", "DK:CVR")],
            )?;
            self.line("      </cac:PartyLegalEntity>")?;
        }
        self.line("      <cac:Contact>")?;
        self.tag("        ", "cbc:ID", "TEST", &[])?;
        self.tag("        ", "cbc:Name", name, &[])?;
        self.line("      </cac:Contact>")?;
        self.line("    </cac:Party>")?;
        self.line(&format!("  </cac:{kind}>"))
    }
}
struct Line<'a> {
    source: &'a Value,
    quantity: String,
    price: String,
    base: Money,
    vat: Money,
    unit: &'a str,
    tax: Tax,
}
/// Consumes only the immutable snapshot's fields; unrelated source fields remain untouched.
pub fn export(invoice_number: &str, payload: &Value) -> Result<Export, Error> {
    let invoice_number = required(&Value::String(invoice_number.to_owned()))?.to_owned();
    if payload
        .get("invoiceNumber")
        .is_some_and(|n| n.as_str() != Some(invoice_number.as_str()))
    {
        return Err(refused("Invoice snapshot number differs"));
    }
    if payload.get("invoiceType").is_some_and(|v| v != "full")
        || payload.get("status").is_some_and(|v| v != "issued")
    {
        return Err(refused("Only a full issued invoice snapshot is supported"));
    }
    let treatment = optional(&payload["vatTreatment"])?.unwrap_or("standard");
    let reverse = matches!(
        treatment,
        "domestic_reverse_charge" | "foreign_reverse_charge"
    );
    if treatment != "standard" && !reverse {
        return Err(refused("Unknown VAT treatment"));
    }
    let currency = required(&payload["currency"])?.trim().to_ascii_uppercase();
    if currency.len() != 3 || !currency.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(refused("Currency must be a three-letter code"));
    }
    let issue = required(&payload["issueDate"])?;
    let due = required(&payload["dueDate"])?;
    let issue_day = Date::parse(issue).map_err(|_| refused("Invoice issue date invalid"))?;
    let due_day = Date::parse(due).map_err(|_| refused("Invoice due date invalid"))?;
    if due_day < issue_day {
        return Err(refused("Due date precedes invoice issue"));
    }
    let receiver = ean(&payload["buyer"]["eanNumber"])?;
    // Source eligibility accepts a valid EAN even when the optional public flag is absent/false.
    if payload["buyer"]
        .get("publicRecipient")
        .is_some_and(|v| !v.is_boolean() && !v.is_null())
    {
        return Err(refused("Public-recipient flag has wrong type"));
    }
    let seller = seller_endpoint(&payload["seller"]["vatOrCvr"])?;
    let net = money(&payload["totals"]["netAmount"])?;
    let vat = if reverse && payload["totals"].get("vatAmount").is_none() {
        Money::default()
    } else {
        money(&payload["totals"]["vatAmount"])?
    };
    let gross = money(&payload["totals"]["grossAmount"])?;
    if !reverse {
        number(&payload["totals"]["vatRate"])?;
    }
    let items = payload["lines"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 200)
        .ok_or_else(|| refused("Invoice requires 1–200 source lines"))?;
    let default_unit = optional(&payload["unitCode"])?;
    let mut lines = Vec::new();
    let mut bases = Money::default();
    let mut taxes = Money::default();
    let explicit = items.iter().any(|l| {
        l["taxClassification"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    });
    let legacy_exempt = !reverse
        && !explicit
        && number(&payload["totals"]["vatRate"])? == "0"
        && vat == Money::default();
    // Issued snapshots retain delivery evidence, although the original OIOUBL
    // formatter does not serialize it. Reuse the shared calendar boundary;
    // issuance-only FX/legal checks must not constrain historical exports.
    for key in ["deliveryDate", "deliveryPeriodStart", "deliveryPeriodEnd"] {
        if let Some(value) = payload.get(key) {
            Date::parse(required(value)?).map_err(|_| refused("Delivery date invalid"))?;
        }
    }
    let start = payload.get("deliveryPeriodStart");
    let end = payload.get("deliveryPeriodEnd");
    if start.is_some() != end.is_some()
        || payload.get("deliveryDate").is_some() && start.is_some()
        || start.zip(end).is_some_and(|(s, e)| s.as_str() > e.as_str())
    {
        return Err(refused("Delivery date/period evidence is inconsistent"));
    }
    let mut groups: Vec<(Tax, Money, Money)> = Vec::new();
    for source in items {
        required(&source["description"])?;
        let classification = optional(&source["taxClassification"])?.unwrap_or(if reverse {
            "reverse_charge"
        } else if legacy_exempt {
            "exempt"
        } else {
            "taxable"
        });
        let tax = match classification {
            "taxable" => Tax {
                category: "StandardRated",
                rate: fraction(
                    source
                        .get("vatRate")
                        .filter(|v| !v.is_null())
                        .unwrap_or(&payload["totals"]["vatRate"]),
                )?,
            },
            "exempt" => Tax {
                category: "ZeroRated",
                rate: "0".into(),
            },
            "reverse_charge" => {
                if optional(&source["taxClassification"])?.is_some() {
                    required(&source["reverseChargeBasis"])?;
                }
                Tax {
                    category: "ReverseCharge",
                    rate: "0".into(),
                }
            }
            _ => return Err(refused("Unknown tax classification")),
        };
        let quantity = number(&source["quantity"])?;
        let raw_price = number(&source["unitPriceExVat"])?;
        let cent_price = Money::parse(&raw_price).map_err(|_| refused("Price invalid"))?;
        let price = if decimal::compare(&raw_price, &cent_price.decimal())
            .map_err(|_| refused("Price invalid"))?
            == Ordering::Equal
        {
            cent_price.decimal()
        } else {
            raw_price
        };
        if !decimal::positive(&quantity).map_err(|_| refused("Quantity invalid"))?
            || decimal::compare(&price, "0").map_err(|_| refused("Price invalid"))?
                == Ordering::Less
        {
            return Err(refused("Quantity must be positive and price nonnegative"));
        }
        let base = money(&source["lineTotalExVat"])?;
        if Money::multiply_decimal(&quantity, &price)
            .map_err(|_| refused("Line arithmetic invalid"))?
            != base
        {
            return Err(refused(
                "Recorded line total differs from exact price times quantity",
            ));
        }
        let line_vat = Money::multiply_decimal(&base.decimal(), &tax.rate)
            .map_err(|_| refused("Line VAT invalid"))?;
        if let Some(value) = source.get("vatAmount").filter(|v| !v.is_null())
            && money(value)? != line_vat
        {
            return Err(refused("Recorded line VAT differs"));
        }
        let unit = optional(&source["unitCode"])?
            .or(default_unit)
            .unwrap_or("EA")
            .trim();
        let unit = if unit == "H87" { "EA" } else { unit };
        bases.add(&base);
        taxes.add(&line_vat);
        if let Some((_, group_base, group_vat)) = groups.iter_mut().find(|(t, _, _)| *t == tax) {
            group_base.add(&base);
            group_vat.add(&line_vat);
        } else {
            groups.push((tax.clone(), base.clone(), line_vat.clone()));
        }
        lines.push(Line {
            source,
            quantity,
            price,
            base,
            vat: line_vat,
            unit,
            tax,
        });
    }
    let mut total = net.clone();
    total.add(&vat);
    if bases != net || taxes != vat || total != gross {
        return Err(refused(
            "Immutable totals differ from summed rounded line bases/VAT; exporter never rewrites invoice amounts",
        ));
    }
    let buyer_reference = optional(&payload["buyer"]["buyerReference"])?
        .or(optional(&payload["buyer"]["orderReference"])?)
        .unwrap_or(&invoice_number);
    let order = optional(&payload["buyer"]["orderReference"])?;
    let mut xml = Xml {
        value: String::new(),
        currency: currency.clone(),
    };
    xml.line("<?xml version=\"1.0\" encoding=\"UTF-8\"?>")?;
    xml.line("<Invoice xmlns=\"urn:oasis:names:specification:ubl:schema:xsd:Invoice-2\" xmlns:cac=\"urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2\" xmlns:cbc=\"urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2\">")?;
    xml.tag("  ", "cbc:UBLVersionID", "2.0", &[])?;
    xml.tag("  ", "cbc:CustomizationID", "OIOUBL-2.02", &[])?;
    xml.tag(
        "  ",
        "cbc:ProfileID",
        "Procurement-BilSim-1.0",
        &[
            ("schemeID", "urn:oioubl:id:profileid-1.2"),
            ("schemeAgencyID", AGENCY),
        ],
    )?;
    xml.tag("  ", "cbc:ID", &invoice_number, &[])?;
    xml.tag("  ", "cbc:IssueDate", issue, &[])?;
    xml.tag("  ", "cbc:DueDate", due, &[])?;
    xml.tag(
        "  ",
        "cbc:InvoiceTypeCode",
        "380",
        &[
            ("listID", "urn:oioubl:codelist:invoicetypecode-1.1"),
            ("listAgencyID", AGENCY),
        ],
    )?;
    xml.tag("  ", "cbc:DocumentCurrencyCode", &currency, &[])?;
    xml.tag("  ", "cbc:BuyerReference", buyer_reference, &[])?;
    if let Some(order) = order {
        xml.line("  <cac:OrderReference>")?;
        xml.tag("    ", "cbc:ID", order, &[])?;
        xml.line("  </cac:OrderReference>")?;
    }
    xml.party("AccountingSupplierParty", &payload["seller"], &seller, true)?;
    xml.party(
        "AccountingCustomerParty",
        &payload["buyer"],
        &receiver,
        false,
    )?;
    xml.line("  <cac:TaxTotal>")?;
    xml.amount("    ", "cbc:TaxAmount", &vat)?;
    let basis = optional(&payload["reverseChargeBasis"])?;
    let reverse_reason = basis
        .map(|b| format!("Reverse charge / Omvendt betalingspligt ({b})"))
        .unwrap_or_else(|| "Reverse charge / Omvendt betalingspligt".into());
    for (tax, base, vat) in &groups {
        let reason = match tax.category {
            "ReverseCharge" => Some(reverse_reason.as_str()),
            "ZeroRated" => Some("Exempt from VAT / Momsfritaget"),
            _ => None,
        };
        xml.subtotal("    ", base, vat, tax, reason)?;
    }
    xml.line("  </cac:TaxTotal>")?;
    xml.line("  <cac:LegalMonetaryTotal>")?;
    for (name, m) in [
        ("cbc:LineExtensionAmount", &net),
        ("cbc:TaxExclusiveAmount", &net),
        ("cbc:TaxInclusiveAmount", &gross),
        ("cbc:PayableAmount", &gross),
    ] {
        xml.amount("    ", name, m)?;
    }
    xml.line("  </cac:LegalMonetaryTotal>")?;
    for (index, line) in lines.iter().enumerate() {
        xml.line("  <cac:InvoiceLine>")?;
        xml.tag("    ", "cbc:ID", &(index + 1).to_string(), &[])?;
        xml.tag(
            "    ",
            "cbc:InvoicedQuantity",
            &line.quantity,
            &[("unitCode", line.unit)],
        )?;
        xml.amount("    ", "cbc:LineExtensionAmount", &line.base)?;
        xml.line("    <cac:TaxTotal>")?;
        xml.amount("      ", "cbc:TaxAmount", &line.vat)?;
        xml.subtotal("      ", &line.base, &line.vat, &line.tax, None)?;
        xml.line("    </cac:TaxTotal>")?;
        xml.line("    <cac:Item>")?;
        xml.tag(
            "      ",
            "cbc:Name",
            required(&line.source["description"])?,
            &[],
        )?;
        xml.line("      <cac:ClassifiedTaxCategory>")?;
        xml.tax_fields("        ", &line.tax, None)?;
        xml.line("      </cac:ClassifiedTaxCategory>")?;
        xml.line("    </cac:Item>")?;
        xml.line("    <cac:Price>")?;
        xml.tag(
            "      ",
            "cbc:PriceAmount",
            &line.price,
            &[("currencyID", &currency)],
        )?;
        xml.line("    </cac:Price>")?;
        xml.line("  </cac:InvoiceLine>")?;
    }
    xml.line("</Invoice>")?;
    let xml = xml.value;
    let sha256 = format!("{:x}", Sha256::digest(xml.as_bytes()));
    Ok(Export {
        xml,
        sha256,
        receiver_endpoint_id: receiver,
    })
}
#[cfg(test)]
mod tests;
