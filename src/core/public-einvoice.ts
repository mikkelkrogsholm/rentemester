import { runSql } from "./sqlite";
import { createHash } from "node:crypto";
import { writeFileSync } from "node:fs";
import type { Database } from "bun:sqlite";
import { insertAuditLog } from "./actor";
import { normalizeEanNumber } from "./ean";
import type { InvoicePayload } from "./invoice";
import { formatAmount, roundDkk, sumDkk } from "./money";
import { projectVatLines } from "./vat-lines";

const RULE_ID = "DK-INVOICE-PUBLIC-EXPORT-001";
const OIOUBL_RULE_ID = "DK-INVOICE-PUBLIC-OIOUBL-001";

// DigiSense routes the TEST document to NemHandel from these OIOUBL markers.
// Keep this exporter genuinely OIOUBL 2.02; a Peppol BIS3 customization would
// instead select DigiSense's separate Peppol participant registry.
const OIOUBL_UBL_VERSION = "2.0";
const OIOUBL_CUSTOMIZATION_ID = "OIOUBL-2.02";
const OIOUBL_PROFILE_ID = "Procurement-BilSim-1.0";
const OIOUBL_PROFILE_SCHEME_ID = "urn:oioubl:id:profileid-1.2";
const OIOUBL_AGENCY_ID = "320";
const BUYER_ENDPOINT_SCHEME_ID = "GLN";
const SELLER_ENDPOINT_SCHEME_ID = "DK:CVR";
const PEPPOL_SUBMIT_RULE_ID = "DK-PEPPOL-SUBMIT-001";
const PEPPOL_ENVELOPE_VERSION = "rentemester:dk:peppol-submission:v1";

type ExportedInvoiceRow = {
  id: number;
  invoice_no: string | null;
  invoice_date: string | null;
  document_type: string;
  payload_json: string | null;
};

export type ExportPublicEInvoiceInput = {
  invoiceDocumentId: number;
  outPath?: string;
};

export type ExportPublicEInvoiceResult = {
  ok: boolean;
  invoiceNumber?: string;
  outPath?: string;
  sha256?: string;
  xml?: string;
  appliedRules: string[];
  errors: string[];
};

function escapeXml(value: string | number | null | undefined) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
}

function xmlTag(name: string, value: string | number | null | undefined, indent = "") {
  if (value === null || value === undefined || value === "") return "";
  return `${indent}<${name}>${escapeXml(value)}</${name}>`;
}

function xmlTagWithAttrs(
  name: string,
  attrs: Record<string, string | number | null | undefined>,
  value: string | number | null | undefined,
  indent = "",
) {
  if (value === null || value === undefined || value === "") return "";
  const renderedAttrs = Object.entries(attrs)
    .filter(([, attrValue]) => attrValue !== null && attrValue !== undefined && attrValue !== "")
    .map(([key, attrValue]) => ` ${key}="${escapeXml(attrValue)}"`)
    .join("");
  return `${indent}<${name}${renderedAttrs}>${escapeXml(value)}</${name}>`;
}

function hasText(value: string | null | undefined): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function formatVatPercent(value: number | null | undefined) {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) return null;
  // `totals.vatRate` is the canonical 0..1 fraction everywhere else (invoice.ts,
  // invoice-pdf.ts unconditionally multiplies by 100), so do the same here. The
  // old `value <= 1 ? value*100 : value` heuristic rendered a 1.0 (100%) rate as
  // "1" and passed a stray percent-form value straight through, making the
  // transmitted cbc:Percent disagree with the PDF/TaxAmount on the same invoice.
  const pct = value * 100;
  // An integer rate renders plainly ("25"); a fractional rate keeps up to two
  // decimals, rounded via integer math so a float artifact (e.g. 20.0000000004)
  // never leaks into the transmitted cbc:Percent.
  return Number.isInteger(pct) ? String(pct) : String(Math.round(pct * 100) / 100);
}

// Peppol/UBL tax-category code (BT-151 / cbc:ID under cac:[Classified]TaxCategory),
// derived from the invoice's VAT treatment rather than hardcoded "S":
//   S  = standard rated; E = exempt; AE = VAT reverse charge.
// A standard line with a 0% rate is treated as exempt (E) so the category and
// the percent never disagree (an "S" with 0% is rejected by EN16931 BR-S-*).
//
// EN16931 BR-AE-10 (reverse charge) and BR-E-10 (exempt) require the document
// VAT breakdown (BG-23) to carry a VAT exemption reason code (BT-121) OR a VAT
// exemption reason text (BT-120); without one a receiving Peppol access point
// rejects the invoice in schematron validation. We therefore attach an
// exemption reason to E and AE (never to standard "S", which BR-S-09/10 forbid
// from carrying one).
type UblTaxCategory = {
  id: "S" | "E" | "AE";
  percent: string | null;
  // BT-121 — only populated for AE (the single AE-valid VATEX code).
  exemptionReasonCode?: string;
  // BT-120 — free-text reason populated for AE and E.
  exemptionReason?: string;
};

// VATEX-EU-AE is the only VAT-exemption-reason code valid for category AE in the
// EN16931/Peppol "VAT exemption reasons" (VATEX) code list. Source (Peppol PINT
// EU aligned code list):
//   https://docs.peppol.eu/poac/eu/pint-eu/trn-invoice/codelist/Aligned-TaxExemptionCodes/
// For category E we deliberately emit a free-text reason (BT-120) only: BR-E-10
// is satisfied by the text alone, and a generic 0%/exempt Danish line has no
// single defensible VATEX code (the E-specific codes are margin-scheme cases),
// so a free-text reason avoids transmitting an over-specific, wrong code.
const VATEX_REVERSE_CHARGE_CODE = "VATEX-EU-AE";
const REVERSE_CHARGE_REASON_TEXT = "Reverse charge / Omvendt betalingspligt";
const EXEMPT_REASON_TEXT = "Exempt from VAT / Momsfritaget";

function deriveUblTaxCategory(
  payload: InvoicePayload,
  vatPercent: string | null,
): UblTaxCategory {
  const treatment = payload.vatTreatment ?? "standard";
  if (treatment === "domestic_reverse_charge" || treatment === "foreign_reverse_charge") {
    // Reverse charge: the buyer accounts for VAT; the category percent is 0.
    // BR-AE-10 requires an exemption reason; carry the documented reverse-charge
    // basis from the payload as supplementary free text when present.
    const basis = hasText(payload.reverseChargeBasis) ? payload.reverseChargeBasis.trim() : null;
    return {
      id: "AE",
      percent: "0",
      exemptionReasonCode: VATEX_REVERSE_CHARGE_CODE,
      exemptionReason: basis
        ? `${REVERSE_CHARGE_REASON_TEXT} (${basis})`
        : REVERSE_CHARGE_REASON_TEXT,
    };
  }
  if (vatPercent === null || vatPercent === "0") {
    // Exempt / 0%: BR-E-10 requires an exemption reason. A free-text reason
    // (BT-120) alone satisfies the rule.
    return { id: "E", percent: "0", exemptionReason: EXEMPT_REASON_TEXT };
  }
  return { id: "S", percent: vatPercent };
}

/**
 * Projects the tax breakdown that will be transmitted in OIOUBL.
 *
 * Old issued-invoice payloads did not carry a per-line tax classification.
 * A legacy, otherwise internally consistent 0% standard payload consequently
 * has to mean exempt here: treating its unclassified line as taxable would
 * invent a taxable 0% line and reject the very E-category export that the
 * document-level totals describe. The compatibility branch is deliberately
 * narrow: any explicit classification remains authoritative, and a non-zero
 * VAT amount or rate still follows normal taxable validation.
 */
function projectOioUblVatLines(payload: InvoicePayload) {
  const isLegacyExempt =
    (payload.vatTreatment ?? "standard") === "standard" &&
    payload.totals?.vatRate === 0 &&
    payload.totals?.vatAmount === 0 &&
    (payload.lines ?? []).every((line) => !line.taxClassification);
  const lines = isLegacyExempt
    ? (payload.lines ?? []).map((line) => ({ ...line, taxClassification: "exempt" as const }))
    : payload.lines;
  return projectVatLines(lines, payload.vatTreatment ?? "standard", payload.totals?.vatRate);
}

// Normalise a Danish seller participant id to eight digits before rendering
// OIOUBL's DK:CVR / DK:SE values with the required DK prefix. Returns null when
// the result is not exactly 8 digits, so a
// malformed CVR surfaces as a validation error rather than a bad EndpointID.
function normalizeDanishCvrEndpoint(value: string | null | undefined): string | null {
  if (!hasText(value)) return null;
  const digits = value.trim().replace(/^DK/i, "").replace(/\D/g, "");
  return /^\d{8}$/.test(digits) ? digits : null;
}

// OIOUBL 2.02 uses the legacy UN/ECE list where piece is "EA".
const DEFAULT_UNIT_CODE = "EA";

function resolveUnitCode(
  payload: InvoicePayload,
  line: { unitCode?: string },
): string {
  if (hasText(line.unitCode)) return line.unitCode.trim() === "H87" ? "EA" : line.unitCode.trim();
  if (hasText(payload.unitCode)) return payload.unitCode.trim() === "H87" ? "EA" : payload.unitCode.trim();
  return DEFAULT_UNIT_CODE;
}

function buildAddressXml(
  tagName: string,
  address: string | null | undefined,
  indent = "",
  _countryCode = "DK",
) {
  if (!hasText(address)) return "";
  return [
    `${indent}<${tagName}>`,
    xmlTagWithAttrs(
      "cbc:AddressFormatCode",
      { listID: "urn:oioubl:codelist:addressformatcode-1.1", listAgencyID: OIOUBL_AGENCY_ID },
      "Unstructured",
      `${indent}  `,
    ),
    `${indent}  <cac:AddressLine>`,
    xmlTag("cbc:Line", address.trim(), `${indent}    `),
    `${indent}  </cac:AddressLine>`,
    `${indent}</${tagName}>`,
  ].join("\n");
}

function buildPublicEInvoiceXml(invoiceNumber: string, payload: InvoicePayload) {
  const lines = payload.lines ?? [];
  const lineXml = lines
    .map((line, index) => [
      "      <Line>",
      xmlTag("LineNumber", index + 1, "        "),
      xmlTag("Description", line.description, "        "),
      xmlTag("Quantity", typeof line.quantity === "number" ? line.quantity : null, "        "),
      xmlTag("UnitPriceExVat", formatAmount(line.unitPriceExVat), "        "),
      xmlTag("LineTotalExVat", formatAmount(line.lineTotalExVat), "        "),
      xmlTag("TaxClassification", line.taxClassification, "        "),
      xmlTag("VatRate", typeof line.vatRate === "number" ? line.vatRate : null, "        "),
      "      </Line>",
    ].filter(Boolean).join("\n"))
    .join("\n");

  return [
    '<?xml version="1.0" encoding="UTF-8"?>',
    '<PublicEInvoicePreview xmlns="urn:rentemester:dk:public-einvoice-preview:v1">',
    xmlTag("InvoiceNumber", invoiceNumber, "  "),
    xmlTag("IssueDate", payload.issueDate, "  "),
    xmlTag("DueDate", payload.dueDate, "  "),
    xmlTag("Currency", payload.currency ?? "DKK", "  "),
    xmlTag("Profile", "public-recipient-preview-only", "  "),
    xmlTag("Transport", "out_of_scope_peppol_access_point_required", "  "),
    "  <Seller>",
    xmlTag("Name", payload.seller?.name, "    "),
    xmlTag("Address", payload.seller?.address, "    "),
    xmlTag("VatOrCvr", payload.seller?.vatOrCvr, "    "),
    "  </Seller>",
    "  <Buyer>",
    xmlTag("Name", payload.buyer?.name, "    "),
    xmlTag("Address", payload.buyer?.address, "    "),
    xmlTag("VatOrCvr", payload.buyer?.vatOrCvr, "    "),
    xmlTag("EanNumber", payload.buyer?.eanNumber, "    "),
    "  </Buyer>",
    "  <Delivery>",
    xmlTag("DeliveryDate", payload.deliveryDate, "    "),
    xmlTag("DeliveryPeriodStart", payload.deliveryPeriodStart, "    "),
    xmlTag("DeliveryPeriodEnd", payload.deliveryPeriodEnd, "    "),
    "  </Delivery>",
    "  <Totals>",
    xmlTag("NetAmount", formatAmount(payload.totals?.netAmount), "    "),
    xmlTag("VatRate", typeof payload.totals?.vatRate === "number" ? payload.totals.vatRate : null, "    "),
    xmlTag("VatAmount", formatAmount(payload.totals?.vatAmount), "    "),
    xmlTag("GrossAmount", formatAmount(payload.totals?.grossAmount), "    "),
    "  </Totals>",
    "  <Lines>",
    lineXml,
    "  </Lines>",
    "</PublicEInvoicePreview>",
    "",
  ].filter((line) => line !== "").join("\n");
}

function validateOioUblPayload(invoiceNumber: string, payload: InvoicePayload, eanNumber: string | null) {
  const errors: string[] = [];
  if (!hasText(payload.issueDate)) errors.push(`invoice ${invoiceNumber} is missing issueDate required for OIOUBL handoff`);
  if (!hasText(payload.dueDate)) errors.push(`invoice ${invoiceNumber} is missing dueDate required for OIOUBL handoff`);
  if (!hasText(payload.seller?.name)) errors.push(`invoice ${invoiceNumber} is missing seller.name required for OIOUBL handoff`);
  if (!hasText(payload.seller?.address)) errors.push(`invoice ${invoiceNumber} is missing seller.address required for OIOUBL handoff`);
  if (!hasText(payload.seller?.vatOrCvr)) {
    errors.push(`invoice ${invoiceNumber} is missing seller.vatOrCvr required for OIOUBL handoff`);
  } else if (!normalizeDanishCvrEndpoint(payload.seller?.vatOrCvr)) {
    errors.push(`invoice ${invoiceNumber} seller.vatOrCvr must be a Danish 8-digit CVR for EndpointID schemeID DK:CVR`);
  }
  // PEPPOL-EN16931-R003: a public-recipient invoice must carry a BuyerReference.
  // The export falls back to orderReference / invoice number, so this only fails
  // when none of the three is present (an effectively unreferenced invoice).
  if (
    !hasText(payload.buyer?.buyerReference) &&
    !hasText(payload.buyer?.orderReference) &&
    !hasText(invoiceNumber)
  ) {
    errors.push(`invoice ${invoiceNumber} is missing buyer.buyerReference required by Peppol PEPPOL-EN16931-R003 for public recipients`);
  }
  if (!hasText(payload.buyer?.name)) errors.push(`invoice ${invoiceNumber} is missing buyer.name required for OIOUBL handoff`);
  if (!hasText(payload.buyer?.address)) errors.push(`invoice ${invoiceNumber} is missing buyer.address required for OIOUBL handoff`);
  if (!eanNumber) errors.push(`invoice ${invoiceNumber} is missing buyer.eanNumber as 13 digits required for OIOUBL handoff`);
  if (!hasText(payload.currency)) errors.push(`invoice ${invoiceNumber} is missing currency required for OIOUBL handoff`);
  if (typeof payload.totals?.netAmount !== "number") errors.push(`invoice ${invoiceNumber} is missing totals.netAmount required for OIOUBL handoff`);
  if (typeof payload.totals?.grossAmount !== "number") errors.push(`invoice ${invoiceNumber} is missing totals.grossAmount required for OIOUBL handoff`);
  // Reverse-charge invoices deliberately carry no VAT amount/rate (the buyer
  // accounts for VAT), so only standard-rated invoices require them. The
  // tax-category code is derived from vatTreatment (AE for reverse charge).
  const isReverseCharge =
    payload.vatTreatment === "domestic_reverse_charge" ||
    payload.vatTreatment === "foreign_reverse_charge";
  if (!isReverseCharge) {
    if (typeof payload.totals?.vatAmount !== "number") errors.push(`invoice ${invoiceNumber} is missing totals.vatAmount required for OIOUBL handoff`);
    if (typeof payload.totals?.vatRate !== "number") errors.push(`invoice ${invoiceNumber} is missing totals.vatRate required for OIOUBL handoff`);
  }
  if (!Array.isArray(payload.lines) || payload.lines.length === 0) {
    errors.push(`invoice ${invoiceNumber} is missing invoice lines required for OIOUBL handoff`);
  } else {
    payload.lines.forEach((line, index) => {
      if (!hasText(line.description)) errors.push(`invoice ${invoiceNumber} line ${index + 1} is missing description required for OIOUBL handoff`);
      if (typeof line.quantity !== "number") errors.push(`invoice ${invoiceNumber} line ${index + 1} is missing quantity required for OIOUBL handoff`);
      if (typeof line.unitPriceExVat !== "number") errors.push(`invoice ${invoiceNumber} line ${index + 1} is missing unitPriceExVat required for OIOUBL handoff`);
      if (typeof line.lineTotalExVat !== "number") errors.push(`invoice ${invoiceNumber} line ${index + 1} is missing lineTotalExVat required for OIOUBL handoff`);
    });
  }

  // This is an export trust boundary: historical rows may predate the invoice
  // validator, so never render contradictory OIOUBL tax totals from a legacy
  // payload. Re-project the rounded line tax amounts independently here.
  if (Array.isArray(payload.lines) && payload.lines.every((line) => typeof line.lineTotalExVat === "number")) {
    const projection = projectOioUblVatLines(payload);
    errors.push(...projection.errors.map((error) => `invoice ${invoiceNumber} ${error}`));
    // Required-total errors are collected above. Use zero only as a safe
    // comparison sentinel here so a missing legacy field produces validation
    // errors instead of throwing while we check the remaining evidence.
    const netAmount = roundDkk(Number(payload.totals?.netAmount ?? 0));
    const vatAmount = roundDkk(Number(payload.totals?.vatAmount ?? 0));
    const grossAmount = roundDkk(Number(payload.totals?.grossAmount ?? 0));
    if (netAmount !== projection.netAmount) errors.push(`invoice ${invoiceNumber} totals.netAmount must equal rounded OIOUBL line bases (${projection.netAmount})`);
    if (vatAmount !== projection.vatAmount) errors.push(`invoice ${invoiceNumber} totals.vatAmount must equal rounded OIOUBL line VAT amounts (${projection.vatAmount})`);
    if (grossAmount !== projection.grossAmount) errors.push(`invoice ${invoiceNumber} totals.grossAmount must equal rounded OIOUBL line totals (${projection.grossAmount})`);
    const expectedGross = sumDkk([netAmount, vatAmount]);
    if (grossAmount !== expectedGross) errors.push(`invoice ${invoiceNumber} totals.grossAmount must equal totals.netAmount + totals.vatAmount (${expectedGross})`);
  }
  return errors;
}

function buildPublicEInvoiceOioUblXml(invoiceNumber: string, payload: InvoicePayload) {
  const currency = (payload.currency ?? "DKK").trim().toUpperCase();
  const vatPercent = formatVatPercent(payload.totals?.vatRate);
  // The tax category is derived from the VAT treatment, not hardcoded "S", so
  // exempt / reverse-charge lines carry the correct EN16931 category code.
  const taxCategory = deriveUblTaxCategory(payload, vatPercent);
  const projectedTaxLines = projectOioUblVatLines(payload).lines;
  // Uniform historical payloads have no per-line classification. Retain their
  // established document-level E/AE meaning while explicit payloads use their
  // individual classifications.
  const taxLines = payload.lines?.some((line) => line.taxClassification)
    ? projectedTaxLines
    : projectedTaxLines.map((line) => ({ ...line, taxClassification: taxCategory.id === "S" ? "taxable" as const : taxCategory.id === "AE" ? "reverse_charge" as const : "exempt" as const }));
  // OIOUBL scheme DK:CVR carries the DK-prefixed 10-character identifier.
  const sellerEndpoint = normalizeDanishCvrEndpoint(payload.seller?.vatOrCvr);
  // Reverse-charge / exempt invoices carry no VAT amount; render it as 0.00 so
  // cac:TaxTotal stays well-formed (TaxAmount is mandatory in UBL).
  const vatAmountForXml =
    formatAmount(payload.totals?.vatAmount) ?? "0.00";
  // OIOUBL 2.02's Procurement-BilSim profile retains a historical semantic
  // deviation from generic UBL: LegalMonetaryTotal/TaxExclusiveAmount is the
  // document tax total, not the VAT-exclusive line base. DigiSense validates
  // this with F-INV127. Keep the ordinary net amount in LineExtensionAmount;
  // TaxInclusiveAmount remains net + tax.
  const oioUblTaxExclusiveAmount = vatAmountForXml;
  const lines = payload.lines ?? [];
  const taxSubtotalGroups = new Map<string, typeof taxLines>();
  for (const line of taxLines) {
    const key = `${line.taxClassification}:${line.vatRate}`;
    taxSubtotalGroups.set(key, [...(taxSubtotalGroups.get(key) ?? []), line]);
  }
  const taxSubtotalXml = [...taxSubtotalGroups.values()].map((selected) => {
    const classification = selected[0].taxClassification;
    const base = sumDkk(selected.map((line) => line.vatBase));
    const vat = sumDkk(selected.map((line) => line.vatAmount));
    const rate = selected[0].vatRate;
    const category = classification === "taxable" ? "StandardRated" : classification === "reverse_charge" ? "ReverseCharge" : "ZeroRated";
    return [
      "    <cac:TaxSubtotal>",
      xmlTagWithAttrs("cbc:TaxableAmount", { currencyID: currency }, formatAmount(base), "      "),
      xmlTagWithAttrs("cbc:TaxAmount", { currencyID: currency }, formatAmount(vat), "      "),
      "      <cac:TaxCategory>",
      xmlTagWithAttrs("cbc:ID", { schemeID: "urn:oioubl:id:taxcategoryid-1.1", schemeAgencyID: OIOUBL_AGENCY_ID }, category, "        "),
      xmlTag("cbc:Percent", String(rate * 100), "        "),
      ...(category === "ReverseCharge" ? [xmlTag("cbc:TaxExemptionReasonCode", VATEX_REVERSE_CHARGE_CODE, "        "), xmlTag("cbc:TaxExemptionReason", hasText(payload.reverseChargeBasis) ? `${REVERSE_CHARGE_REASON_TEXT} (${payload.reverseChargeBasis})` : REVERSE_CHARGE_REASON_TEXT, "        ")] : category === "ZeroRated" ? [xmlTag("cbc:TaxExemptionReason", EXEMPT_REASON_TEXT, "        ")] : []),
      "        <cac:TaxScheme>",
      xmlTagWithAttrs("cbc:ID", { schemeID: "urn:oioubl:id:taxschemeid-1.1", schemeAgencyID: OIOUBL_AGENCY_ID }, "63", "          "),
      xmlTag("cbc:Name", "Moms", "          "),
      "        </cac:TaxScheme>",
      "      </cac:TaxCategory>", "    </cac:TaxSubtotal>",
    ].filter(Boolean).join("\n");
  }).filter(Boolean).join("\n");
  const lineXml = lines
    .map((line, index) => {
      const projected = taxLines[index];
      const lineCategory = projected?.taxClassification === "taxable" ? "StandardRated" : projected?.taxClassification === "reverse_charge" ? "ReverseCharge" : "ZeroRated";
      const lineVatAmount = projected?.vatAmount ?? 0;
      const lineVatBase = projected?.vatBase ?? line.lineTotalExVat;
      const lineVatRate = (projected?.vatRate ?? 0) * 100;
      return [
      "  <cac:InvoiceLine>",
      xmlTag("cbc:ID", index + 1, "    "),
      xmlTagWithAttrs("cbc:InvoicedQuantity", { unitCode: resolveUnitCode(payload, line) }, line.quantity, "    "),
      xmlTagWithAttrs("cbc:LineExtensionAmount", { currencyID: currency }, formatAmount(line.lineTotalExVat), "    "),
      "    <cac:TaxTotal>",
      xmlTagWithAttrs("cbc:TaxAmount", { currencyID: currency }, formatAmount(lineVatAmount), "      "),
      "      <cac:TaxSubtotal>",
      xmlTagWithAttrs("cbc:TaxableAmount", { currencyID: currency }, formatAmount(lineVatBase), "        "),
      xmlTagWithAttrs("cbc:TaxAmount", { currencyID: currency }, formatAmount(lineVatAmount), "        "),
      "        <cac:TaxCategory>",
      xmlTagWithAttrs("cbc:ID", { schemeID: "urn:oioubl:id:taxcategoryid-1.1", schemeAgencyID: OIOUBL_AGENCY_ID }, lineCategory, "          "),
      xmlTag("cbc:Percent", String(lineVatRate), "          "),
      "          <cac:TaxScheme>",
      xmlTagWithAttrs("cbc:ID", { schemeID: "urn:oioubl:id:taxschemeid-1.1", schemeAgencyID: OIOUBL_AGENCY_ID }, "63", "            "),
      xmlTag("cbc:Name", "Moms", "            "),
      "          </cac:TaxScheme>",
      "        </cac:TaxCategory>",
      "      </cac:TaxSubtotal>",
      "    </cac:TaxTotal>",
      "    <cac:Item>",
      xmlTag("cbc:Name", line.description, "      "),
      "      <cac:ClassifiedTaxCategory>",
      xmlTagWithAttrs("cbc:ID", { schemeID: "urn:oioubl:id:taxcategoryid-1.1", schemeAgencyID: OIOUBL_AGENCY_ID }, lineCategory, "        "),
      xmlTag("cbc:Percent", String(lineVatRate), "        "),
      "        <cac:TaxScheme>",
      xmlTagWithAttrs("cbc:ID", { schemeID: "urn:oioubl:id:taxschemeid-1.1", schemeAgencyID: OIOUBL_AGENCY_ID }, "63", "          "),
      xmlTag("cbc:Name", "Moms", "          "),
      "        </cac:TaxScheme>",
      "      </cac:ClassifiedTaxCategory>",
      "    </cac:Item>",
      "    <cac:Price>",
      xmlTagWithAttrs("cbc:PriceAmount", { currencyID: currency }, formatAmount(line.unitPriceExVat), "      "),
      "    </cac:Price>",
      "  </cac:InvoiceLine>",
    ].filter(Boolean).join("\n");
    })
    .join("\n");

  return [
    '<?xml version="1.0" encoding="UTF-8"?>',
    '<Invoice xmlns="urn:oasis:names:specification:ubl:schema:xsd:Invoice-2" xmlns:cac="urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2" xmlns:cbc="urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2">',
    xmlTag("cbc:UBLVersionID", OIOUBL_UBL_VERSION, "  "),
    xmlTag("cbc:CustomizationID", OIOUBL_CUSTOMIZATION_ID, "  "),
    xmlTagWithAttrs("cbc:ProfileID", { schemeID: OIOUBL_PROFILE_SCHEME_ID, schemeAgencyID: OIOUBL_AGENCY_ID }, OIOUBL_PROFILE_ID, "  "),
    xmlTag("cbc:ID", invoiceNumber, "  "),
    xmlTag("cbc:IssueDate", payload.issueDate, "  "),
    xmlTag("cbc:DueDate", payload.dueDate, "  "),
    xmlTagWithAttrs("cbc:InvoiceTypeCode", { listID: "urn:oioubl:codelist:invoicetypecode-1.1", listAgencyID: OIOUBL_AGENCY_ID }, "380", "  "),
    xmlTag("cbc:DocumentCurrencyCode", currency, "  "),
    // cbc:BuyerReference (BT-10) — mandatory for a public-recipient invoice
    // (PEPPOL-EN16931-R003). Falls back to the order reference, then the
    // invoice number, so a public invoice always carries a buyer reference.
    xmlTag(
      "cbc:BuyerReference",
      payload.buyer?.buyerReference ?? payload.buyer?.orderReference ?? invoiceNumber,
      "  ",
    ),
    payload.buyer?.orderReference
      ? [
          "  <cac:OrderReference>",
          xmlTag("cbc:ID", payload.buyer.orderReference, "    "),
          "  </cac:OrderReference>",
        ].join("\n")
      : "",
    "  <cac:AccountingSupplierParty>",
    "    <cac:Party>",
    xmlTagWithAttrs("cbc:EndpointID", { schemeID: SELLER_ENDPOINT_SCHEME_ID }, sellerEndpoint ? `DK${sellerEndpoint}` : null, "      "),
    "      <cac:PartyName>",
    xmlTag("cbc:Name", payload.seller?.name, "        "),
    "      </cac:PartyName>",
    buildAddressXml("cac:PostalAddress", payload.seller?.address, "      "),
    "      <cac:PartyTaxScheme>",
    xmlTagWithAttrs("cbc:CompanyID", { schemeID: "DK:SE" }, sellerEndpoint ? `DK${sellerEndpoint}` : null, "        "),
    "        <cac:TaxScheme>",
    xmlTagWithAttrs("cbc:ID", { schemeID: "urn:oioubl:id:taxschemeid-1.1", schemeAgencyID: OIOUBL_AGENCY_ID }, "63", "          "),
    xmlTag("cbc:Name", "Moms", "          "),
    "        </cac:TaxScheme>",
    "      </cac:PartyTaxScheme>",
    "      <cac:PartyLegalEntity>",
    xmlTag("cbc:RegistrationName", payload.seller?.name, "        "),
    xmlTagWithAttrs(
      "cbc:CompanyID",
      { schemeID: SELLER_ENDPOINT_SCHEME_ID },
      sellerEndpoint ? `DK${sellerEndpoint}` : null,
      "        ",
    ),
    "      </cac:PartyLegalEntity>",
    "      <cac:Contact>",
    xmlTag("cbc:ID", "TEST", "        "),
    xmlTag("cbc:Name", payload.seller?.name, "        "),
    "      </cac:Contact>",
    "    </cac:Party>",
    "  </cac:AccountingSupplierParty>",
    "  <cac:AccountingCustomerParty>",
    "    <cac:Party>",
    xmlTagWithAttrs("cbc:EndpointID", { schemeID: BUYER_ENDPOINT_SCHEME_ID }, payload.buyer?.eanNumber, "      "),
    "      <cac:PartyName>",
    xmlTag("cbc:Name", payload.buyer?.name, "        "),
    "      </cac:PartyName>",
    buildAddressXml("cac:PostalAddress", payload.buyer?.address, "      "),
    "      <cac:Contact>",
    xmlTag("cbc:ID", "TEST", "        "),
    xmlTag("cbc:Name", payload.buyer?.name, "        "),
    "      </cac:Contact>",
    "    </cac:Party>",
    "  </cac:AccountingCustomerParty>",
    "  <cac:TaxTotal>",
    xmlTagWithAttrs("cbc:TaxAmount", { currencyID: currency }, vatAmountForXml, "    "),
    taxSubtotalXml,
    "  </cac:TaxTotal>",
    "  <cac:LegalMonetaryTotal>",
    xmlTagWithAttrs("cbc:LineExtensionAmount", { currencyID: currency }, formatAmount(payload.totals?.netAmount), "    "),
    xmlTagWithAttrs("cbc:TaxExclusiveAmount", { currencyID: currency }, oioUblTaxExclusiveAmount, "    "),
    xmlTagWithAttrs("cbc:TaxInclusiveAmount", { currencyID: currency }, formatAmount(payload.totals?.grossAmount), "    "),
    xmlTagWithAttrs("cbc:PayableAmount", { currencyID: currency }, formatAmount(payload.totals?.grossAmount), "    "),
    "  </cac:LegalMonetaryTotal>",
    lineXml,
    "</Invoice>",
    "",
  ].filter((line) => line !== "").join("\n");
}

function loadExportedInvoice(db: Database, input: ExportPublicEInvoiceInput) {
  return db.query(
    `SELECT id, invoice_no, invoice_date, document_type, payload_json
     FROM documents
     WHERE id = ? LIMIT 1`,
  ).get(input.invoiceDocumentId) as ExportedInvoiceRow | null;
}

export function exportPublicEInvoicePreview(
  db: Database,
  input: ExportPublicEInvoiceInput,
): ExportPublicEInvoiceResult {
  const row = loadExportedInvoice(db, input);

  if (!row) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`invoice ${input.invoiceDocumentId} was not found`] };
  }
  if (row.document_type !== "issued_invoice") {
    return { ok: false, appliedRules: [RULE_ID], errors: [`document ${input.invoiceDocumentId} is not an issued invoice`] };
  }
  if (!row.payload_json) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`invoice ${row.invoice_no ?? input.invoiceDocumentId} is missing payload_json`] };
  }

  const payload = JSON.parse(row.payload_json) as InvoicePayload & { invoiceNumber?: string };
  const invoiceNumber = payload.invoiceNumber ?? row.invoice_no ?? String(input.invoiceDocumentId);
  const eanNumber = normalizeEanNumber(payload.buyer?.eanNumber);

  if (payload.buyer?.publicRecipient !== true && !eanNumber) {
    return {
      ok: false,
      appliedRules: [RULE_ID],
      errors: [`invoice ${invoiceNumber} is not marked as a public-recipient e-invoice`],
    };
  }
  if (!eanNumber) {
    return {
      ok: false,
      appliedRules: [RULE_ID],
      errors: [`invoice ${invoiceNumber} is missing buyer.eanNumber as 13 digits`],
    };
  }

  const normalizedPayload: InvoicePayload = {
    ...payload,
    buyer: {
      ...payload.buyer,
      eanNumber,
      publicRecipient: true,
    },
  };

  const xml = buildPublicEInvoiceXml(invoiceNumber, normalizedPayload);
  const sha256 = createHash("sha256").update(xml).digest("hex");
  if (input.outPath) writeFileSync(input.outPath, xml);

  return {
    ok: true,
    invoiceNumber,
    outPath: input.outPath,
    sha256,
    xml,
    appliedRules: [RULE_ID],
    errors: [],
  };
}

export function exportPublicEInvoiceOioUbl(
  db: Database,
  input: ExportPublicEInvoiceInput,
): ExportPublicEInvoiceResult {
  const row = loadExportedInvoice(db, input);

  if (!row) {
    return { ok: false, appliedRules: [OIOUBL_RULE_ID], errors: [`invoice ${input.invoiceDocumentId} was not found`] };
  }
  if (row.document_type !== "issued_invoice") {
    return { ok: false, appliedRules: [OIOUBL_RULE_ID], errors: [`document ${input.invoiceDocumentId} is not an issued invoice`] };
  }
  if (!row.payload_json) {
    return { ok: false, appliedRules: [OIOUBL_RULE_ID], errors: [`invoice ${row.invoice_no ?? input.invoiceDocumentId} is missing payload_json`] };
  }

  const payload = JSON.parse(row.payload_json) as InvoicePayload & { invoiceNumber?: string };
  const invoiceNumber = payload.invoiceNumber ?? row.invoice_no ?? String(input.invoiceDocumentId);
  const eanNumber = normalizeEanNumber(payload.buyer?.eanNumber);

  if (payload.buyer?.publicRecipient !== true && !eanNumber) {
    return {
      ok: false,
      appliedRules: [OIOUBL_RULE_ID],
      errors: [`invoice ${invoiceNumber} is not marked as a public-recipient e-invoice`],
    };
  }

  const normalizedPayload: InvoicePayload = {
    ...payload,
    currency: (payload.currency ?? "DKK").trim().toUpperCase(),
    buyer: {
      ...payload.buyer,
      eanNumber: eanNumber ?? undefined,
      publicRecipient: true,
    },
  };

  const errors = validateOioUblPayload(invoiceNumber, normalizedPayload, eanNumber);
  if (errors.length > 0) {
    return {
      ok: false,
      appliedRules: [OIOUBL_RULE_ID],
      errors,
    };
  }

  const xml = buildPublicEInvoiceOioUblXml(invoiceNumber, normalizedPayload);
  const sha256 = createHash("sha256").update(xml).digest("hex");
  if (input.outPath) writeFileSync(input.outPath, xml);
  insertAuditLog(db, {
    eventType: "public_einvoice_oioubl_export",
    entityType: "document",
    entityId: row.id,
    message: `Generated public OIOUBL handoff artifact for invoice ${invoiceNumber} (sha256 ${sha256})`,
  });

  return {
    ok: true,
    invoiceNumber,
    outPath: input.outPath,
    sha256,
    xml,
    appliedRules: [OIOUBL_RULE_ID],
    errors: [],
  };
}

// ============================================================================
// PEPPOL submission (#128)
//
// The next step on top of the OIOUBL handoff artifact: a deterministic
// submission command that wraps an already-validated public-invoice OIOUBL
// export in a stable PEPPOL submission envelope, records the attempt and is
// idempotent on a derived idempotency key.
//
// Trust boundary: access-point CREDENTIALS never enter core bookkeeping
// state. The caller supplies the (non-secret) access-point configuration —
// id, endpoint URL, sender endpoint id — which is used only to derive the
// envelope. No real network call is performed; this slice produces the
// submission request artifact and records the attempt for the audit trail.
// The preview/OIOUBL handoff exports remain the lower-trust fallback.
// ============================================================================

/**
 * Non-secret access-point configuration. Credentials (certificates, API
 * tokens) deliberately have no field here — they stay outside core state.
 */
export type PeppolAccessPointConfig = {
  accessPointId: string;
  endpointUrl: string;
  senderEndpointId: string;
};

/**
 * Optional transport acknowledgement metadata, recorded verbatim when the
 * caller has confirmation that the access point accepted the transmission.
 */
export type PeppolTransportAcknowledgement = {
  transmissionId: string;
  acknowledgedAt: string;
};

export type SubmitPublicEInvoicePeppolInput = {
  invoiceDocumentId: number;
  accessPoint: PeppolAccessPointConfig;
  acknowledgement?: PeppolTransportAcknowledgement;
  /** Optional path to write the submission envelope artifact to. */
  outPath?: string;
};

export type SubmitPublicEInvoicePeppolResult = {
  ok: boolean;
  invoiceNumber?: string;
  /** Stable reference for this submission attempt. */
  submissionReference?: string;
  /** Derived idempotency key — duplicates collapse onto the same record. */
  idempotencyKey?: string;
  /** sha256 of the underlying OIOUBL handoff artifact. */
  oioublSha256?: string;
  /** sha256 of the generated submission envelope. */
  envelopeSha256?: string;
  /** The deterministic submission envelope XML. */
  envelope?: string;
  /** queued/prepared, terminally failed after provider acceptance, or delivered. */
  status?: "prepared" | "failed" | "uncertain" | "acknowledged";
  /** True when an existing submission record was reused (idempotent re-run). */
  duplicate?: boolean;
  outPath?: string;
  /** Transport transmission id, set once the invoice has actually been transmitted. */
  transmissionId?: string;
  appliedRules: string[];
  errors: string[];
};

type PeppolSubmissionRow = {
  id: number;
  invoice_document_id: number;
  invoice_no: string | null;
  idempotency_key: string;
  submission_reference: string;
  access_point_id: string;
  receiver_endpoint_id: string;
  oioubl_sha256: string;
  envelope_sha256: string;
  envelope_xml: string;
  status: "prepared" | "acknowledged";
  transmission_id: string | null;
  acknowledged_at: string | null;
};

type PeppolSubmissionEventRow = {
  event_type: "delivery_reserved" | "delivery_failed" | "queued" | "status_observed" | "delivered";
  document_id: string;
  observed_at: string;
  status: string | null;
};

function effectiveAcknowledgement(
  db: Database,
  row: PeppolSubmissionRow,
): { document_id: string; observed_at: string } | null {
  const event = db.query(
    `SELECT document_id, observed_at FROM peppol_submission_events
     WHERE submission_id = ? AND event_type = 'delivered' ORDER BY id DESC LIMIT 1`,
  ).get(row.id) as PeppolSubmissionEventRow | null;
  if (event) return event;

  // Databases created before the append-only event migration stored the
  // acknowledgement on the immutable submission row. Preserve that evidence
  // as a read-only fallback so an upgrade can never make a delivered invoice
  // appear unsent (and therefore tempt a duplicate delivery).
  if (row.status === "acknowledged" && row.transmission_id) {
    return {
      document_id: row.transmission_id,
      observed_at: row.acknowledged_at ?? "",
    };
  }
  return null;
}

function latestSubmissionEvent(db: Database, submissionId: number): PeppolSubmissionEventRow | null {
  return db.query(
    `SELECT event_type, document_id, observed_at, status
       FROM peppol_submission_events WHERE submission_id = ? ORDER BY id DESC LIMIT 1`,
  ).get(submissionId) as PeppolSubmissionEventRow | null;
}

function queuedSubmissionEvent(db: Database, submissionId: number): PeppolSubmissionEventRow | null {
  return db.query(
    `SELECT event_type, document_id, observed_at, status
       FROM peppol_submission_events
      WHERE submission_id = ? AND event_type = 'queued'
      ORDER BY id DESC LIMIT 1`,
  ).get(submissionId) as PeppolSubmissionEventRow | null;
}

const TERMINAL_ACCEPTED_FAILURE_STATUSES = new Set([
  "document-not-valid",
  "unable-to-deliver",
  "unknown-server-error",
]);

function isTerminalAcceptedFailure(status: string | null | undefined): boolean {
  return Boolean(status && TERMINAL_ACCEPTED_FAILURE_STATUSES.has(status));
}

function recordSubmissionEvent(
  db: Database,
  submissionId: number,
  eventType: PeppolSubmissionEventRow["event_type"],
  args: { documentId?: string; status?: string; observedAt?: string; message?: string; publicUrl?: string | null } = {},
): void {
  runSql(db,
    `INSERT INTO peppol_submission_events
       (submission_id, event_type, document_id, status, observed_at, message, public_url)
     VALUES (?, ?, ?, ?, ?, ?, ?)`,
    submissionId, eventType, args.documentId ?? null, args.status ?? null,
    args.observedAt ?? new Date().toISOString(), args.message ?? "", args.publicUrl ?? null,
  );
}

function validateAccessPointConfig(config: PeppolAccessPointConfig | undefined): string[] {
  const errors: string[] = [];
  if (!config) {
    errors.push("PEPPOL submission requires access-point config (accessPointId, endpointUrl, senderEndpointId)");
    return errors;
  }
  if (!hasText(config.accessPointId)) errors.push("PEPPOL submission requires a non-empty access-point id");
  if (!hasText(config.endpointUrl)) errors.push("PEPPOL submission requires a non-empty access-point endpointUrl");
  if (!hasText(config.senderEndpointId)) errors.push("PEPPOL submission requires a non-empty access-point senderEndpointId");
  return errors;
}

function buildPeppolSubmissionEnvelope(args: {
  submissionReference: string;
  idempotencyKey: string;
  invoiceNumber: string;
  accessPoint: PeppolAccessPointConfig;
  receiverEndpointId: string;
  oioublSha256: string;
  status: "prepared" | "acknowledged";
  acknowledgement?: PeppolTransportAcknowledgement;
}) {
  // The envelope is fully derived from deterministic inputs (no timestamps,
  // no random ids) so re-running on identical inputs yields an identical
  // artifact. It references the OIOUBL handoff by hash rather than embedding
  // (and thus risking mutation of) the original invoice payload.
  return [
    '<?xml version="1.0" encoding="UTF-8"?>',
    `<PeppolSubmission xmlns="urn:${PEPPOL_ENVELOPE_VERSION}">`,
    xmlTag("SubmissionReference", args.submissionReference, "  "),
    xmlTag("IdempotencyKey", args.idempotencyKey, "  "),
    xmlTag("Status", args.status, "  "),
    "  <Document>",
    xmlTag("InvoiceNumber", args.invoiceNumber, "    "),
    xmlTag("Format", OIOUBL_CUSTOMIZATION_ID, "    "),
    xmlTag("Profile", OIOUBL_PROFILE_ID, "    "),
    xmlTag("HandoffArtifactSha256", args.oioublSha256, "    "),
    "  </Document>",
    "  <AccessPoint>",
    xmlTag("AccessPointId", args.accessPoint.accessPointId, "    "),
    xmlTag("EndpointUrl", args.accessPoint.endpointUrl, "    "),
    xmlTag("SenderEndpointId", args.accessPoint.senderEndpointId, "    "),
    xmlTag("ReceiverEndpointId", args.receiverEndpointId, "    "),
    "  </AccessPoint>",
    args.acknowledgement
      ? [
          "  <Acknowledgement>",
          xmlTag("TransmissionId", args.acknowledgement.transmissionId, "    "),
          xmlTag("AcknowledgedAt", args.acknowledgement.acknowledgedAt, "    "),
          "  </Acknowledgement>",
        ].join("\n")
      : "",
    "</PeppolSubmission>",
    "",
  ].filter((line) => line !== "").join("\n");
}

function rowToSubmissionResult(
  db: Database,
  row: PeppolSubmissionRow,
  invoiceNumber: string,
  duplicate: boolean,
  outPath?: string,
): SubmitPublicEInvoicePeppolResult {
  const acknowledgement = effectiveAcknowledgement(db, row);
  if (outPath) writeFileSync(outPath, row.envelope_xml);
  return {
    ok: true,
    invoiceNumber,
    submissionReference: row.submission_reference,
    idempotencyKey: row.idempotency_key,
    oioublSha256: row.oioubl_sha256,
    envelopeSha256: row.envelope_sha256,
    envelope: row.envelope_xml,
    status: acknowledgement ? "acknowledged" : "prepared",
    duplicate,
    outPath,
    transmissionId: acknowledgement?.document_id,
    appliedRules: [PEPPOL_SUBMIT_RULE_ID],
    errors: [],
  };
}

/**
 * Derive the stable identity of a public-invoice submission shared by the
 * submission and transmission paths: the invoice number, the receiver
 * participant id (read back from the OIOUBL EndpointID so it stays consistent
 * with the validated handoff) and the idempotency key.
 */
function deriveSubmissionIdentity(
  oioubl: ExportPublicEInvoiceResult,
  invoiceDocumentId: number,
  accessPoint: PeppolAccessPointConfig,
) {
  const invoiceNumber = oioubl.invoiceNumber ?? String(invoiceDocumentId);
  const endpointMatch = oioubl.xml?.match(
    new RegExp(`<cbc:EndpointID schemeID="${BUYER_ENDPOINT_SCHEME_ID}">([^<]+)</cbc:EndpointID>`),
  );
  const endpointValue = endpointMatch?.[1];
  const receiver = endpointValue
    ? `${BUYER_ENDPOINT_SCHEME_ID}:${endpointValue}`
    : `${BUYER_ENDPOINT_SCHEME_ID}:unknown`;
  const idempotencyKey = createHash("sha256")
    .update(
      [
        invoiceNumber,
        oioubl.sha256 ?? "",
        accessPoint.accessPointId.trim(),
        accessPoint.senderEndpointId.trim(),
        receiver,
      ].join("|"),
    )
    .digest("hex");
  return { invoiceNumber, receiver, idempotencyKey };
}

/**
 * Produces a deterministic PEPPOL submission envelope for an already-validated
 * public-recipient invoice, building on the existing OIOUBL handoff artifact.
 *
 * Idempotent: the idempotency key is derived from the invoice number, the
 * OIOUBL artifact hash and the access-point/receiver identifiers, so a
 * duplicate submission collapses onto the existing record without writing a
 * new row or audit event. Fails clearly when the OIOUBL handoff validation
 * fails (missing public-recipient metadata) or when access-point config is
 * missing. The original invoice payload is never mutated.
 */
export function submitPublicEInvoicePeppol(
  db: Database,
  input: SubmitPublicEInvoicePeppolInput,
): SubmitPublicEInvoicePeppolResult {
  const configErrors = validateAccessPointConfig(input.accessPoint);
  if (configErrors.length > 0) {
    return { ok: false, appliedRules: [PEPPOL_SUBMIT_RULE_ID], errors: configErrors };
  }
  if (input.acknowledgement) {
    const ackErrors: string[] = [];
    if (!hasText(input.acknowledgement.transmissionId)) {
      ackErrors.push("PEPPOL acknowledgement requires a non-empty transmissionId");
    }
    if (!hasText(input.acknowledgement.acknowledgedAt)) {
      ackErrors.push("PEPPOL acknowledgement requires a non-empty acknowledgedAt timestamp");
    }
    if (ackErrors.length > 0) {
      return { ok: false, appliedRules: [PEPPOL_SUBMIT_RULE_ID], errors: ackErrors };
    }
  }

  // Reuse the shipped OIOUBL handoff slice unchanged as the validated input
  // package. Its own validation surfaces missing public-recipient metadata.
  const oioubl = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: input.invoiceDocumentId });
  if (!oioubl.ok || !oioubl.sha256) {
    return {
      ok: false,
      invoiceNumber: oioubl.invoiceNumber,
      appliedRules: [PEPPOL_SUBMIT_RULE_ID, ...oioubl.appliedRules],
      errors: oioubl.errors.length > 0
        ? oioubl.errors
        : ["PEPPOL submission could not generate the required OIOUBL handoff artifact"],
    };
  }

  const { invoiceNumber, receiver, idempotencyKey } = deriveSubmissionIdentity(
    oioubl,
    input.invoiceDocumentId,
    input.accessPoint,
  );
  // Idempotent fast-path: an identical submission already exists.
  const existing = db
    .query(
      `SELECT id, invoice_document_id, invoice_no, idempotency_key, submission_reference,
              access_point_id, receiver_endpoint_id, oioubl_sha256, envelope_sha256,
              envelope_xml, status, transmission_id, acknowledged_at
       FROM peppol_submissions WHERE idempotency_key = ? LIMIT 1`,
    )
    .get(idempotencyKey) as PeppolSubmissionRow | null;
  if (existing) {
    return rowToSubmissionResult(db, existing, invoiceNumber, true, input.outPath);
  }

  const submissionReference = `PEPPOL-${invoiceNumber}-${idempotencyKey.slice(0, 12)}`;
  const status: "prepared" | "acknowledged" = input.acknowledgement ? "acknowledged" : "prepared";
  const envelope = buildPeppolSubmissionEnvelope({
    submissionReference,
    idempotencyKey,
    invoiceNumber,
    accessPoint: input.accessPoint,
    receiverEndpointId: receiver,
    oioublSha256: oioubl.sha256,
    status,
    acknowledgement: input.acknowledgement,
  });
  const envelopeSha256 = createHash("sha256").update(envelope).digest("hex");

  runSql(db,
    `INSERT INTO peppol_submissions
       (invoice_document_id, invoice_no, idempotency_key, submission_reference,
        access_point_id, receiver_endpoint_id, oioubl_sha256, envelope_sha256,
        envelope_xml, status, transmission_id, acknowledged_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    input.invoiceDocumentId,
    invoiceNumber,
    idempotencyKey,
    submissionReference,
    input.accessPoint.accessPointId.trim(),
    receiver,
    oioubl.sha256,
    envelopeSha256,
    envelope,
    status,
    input.acknowledgement?.transmissionId ?? null,
    input.acknowledgement?.acknowledgedAt ?? null,
  );
  const inserted = db.query(
    `SELECT id, invoice_document_id, invoice_no, idempotency_key, submission_reference,
            access_point_id, receiver_endpoint_id, oioubl_sha256, envelope_sha256,
            envelope_xml, status, transmission_id, acknowledged_at
     FROM peppol_submissions WHERE idempotency_key = ? LIMIT 1`,
  ).get(idempotencyKey) as PeppolSubmissionRow;
  if (input.acknowledgement) {
    recordSubmissionEvent(db, inserted.id, "delivered", {
      documentId: input.acknowledgement.transmissionId,
      status: "delivered",
      observedAt: input.acknowledgement.acknowledgedAt,
      message: "Delivery acknowledged by access point",
    });
  }

  insertAuditLog(db, {
    eventType: "public_einvoice_peppol_submission",
    entityType: "document",
    entityId: input.invoiceDocumentId,
    message:
      `Recorded PEPPOL submission ${submissionReference} for invoice ${invoiceNumber} ` +
      `via access point ${input.accessPoint.accessPointId.trim()} ` +
      `(oioubl ${oioubl.sha256}, envelope ${envelopeSha256}, status ${status})`,
  });

  if (input.outPath) writeFileSync(input.outPath, envelope);

  return {
    ok: true,
    invoiceNumber,
    submissionReference,
    idempotencyKey,
    oioublSha256: oioubl.sha256,
    envelopeSha256,
    envelope,
    status: input.acknowledgement ? "acknowledged" : "prepared",
    duplicate: false,
    outPath: input.outPath,
    appliedRules: [PEPPOL_SUBMIT_RULE_ID],
    errors: [],
  };
}

// ============================================================================
// PEPPOL transmission
//
// One step beyond the #128 submission envelope: hand the OIOUBL invoice to a
// transport that actually delivers it through an access point, and record the
// outcome.
//
// The transport itself is an INJECTED dependency (`PeppolTransmitter`) so this
// orchestration stays deterministic and unit-testable. The production
// transmitter drives the self-hosted NemHandel eDelivery access point
// (Oxalis); it is wired in separately once an access point and a MitID system
// certificate are available, and its credentials never enter core state.
//
// A successful transmission is recorded as effective append-only delivered
// evidence. A pre-acceptance failure remains retryable; once a provider assigns
// a document id, that accepted identity is persisted and can never be delivered
// again, regardless of whether its later terminal status is success or failure.
// ============================================================================

/**
 * Outcome of one transport attempt through an access point.
 *
 * `ok:false` may carry an `acceptedDocumentId`: the provider assigned a remote
 * identity, either while queued or before reporting a terminal rejection. That
 * identity is durable acceptance evidence, so a blind retry could duplicate the
 * invoice. The public-einvoice layer records it append-only and permanently
 * refuses a second delivery. `queuedDocumentId` remains as a compatibility alias.
 */
export type PeppolTransmissionOutcome =
  | { ok: true; transmissionId: string; transmittedAt: string }
  | {
      ok: false;
      error: string;
      /** A remote id proves provider acceptance and permanently forbids re-delivery. */
      acceptedDocumentId?: string;
      /** Provider status observed for the accepted document. */
      acceptedStatus?: string;
      /** Delivery POST completed ambiguously without a trustworthy remote id. */
      deliveryUncertain?: boolean;
      /** Explicit proof that failure happened before any delivery POST. */
      retryableBeforeDelivery?: boolean;
      /** @deprecated Compatibility alias for acceptedDocumentId. */
      queuedDocumentId?: string;
    };

/**
 * Performs the actual AS4 transport of an OIOUBL invoice through an access
 * point. Injected so the orchestration here stays deterministic and testable.
 */
export type PeppolTransmitter = (input: {
  oioublXml: string;
  oioublSha256: string;
  receiverEndpointId: string;
  accessPoint: PeppolAccessPointConfig;
}) => Promise<PeppolTransmissionOutcome> | PeppolTransmissionOutcome;

export type TransmitPublicEInvoicePeppolInput = {
  invoiceDocumentId: number;
  accessPoint: PeppolAccessPointConfig;
};

export type ResumePublicEInvoicePeppolInput = TransmitPublicEInvoicePeppolInput;

/** A status-only lookup: it must never deliver a document. */
export type PeppolSubmissionStatusChecker = (documentId: string) => Promise<{
  ok: boolean;
  status?: string;
  observedAt?: string;
  message?: string;
  publicUrl?: string;
  error?: string;
}> | {
  ok: boolean;
  status?: string;
  observedAt?: string;
  message?: string;
  publicUrl?: string;
  error?: string;
};

/**
 * Resume an accepted, queued submission by observing its existing document id.
 * No mutation of peppol_submissions occurs; every observation is separate,
 * append-only evidence. A delivered observation becomes the effective result
 * used by later transmit attempts, preventing a second document-delivery call.
 */
export async function resumePublicEInvoicePeppolSubmission(
  db: Database,
  input: ResumePublicEInvoicePeppolInput,
  checkStatus: PeppolSubmissionStatusChecker,
): Promise<SubmitPublicEInvoicePeppolResult> {
  const oioubl = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: input.invoiceDocumentId });
  if (!oioubl.ok || !oioubl.sha256 || !oioubl.xml) {
    return { ok: false, invoiceNumber: oioubl.invoiceNumber, appliedRules: [PEPPOL_SUBMIT_RULE_ID, ...oioubl.appliedRules], errors: oioubl.errors };
  }
  const { invoiceNumber, idempotencyKey } = deriveSubmissionIdentity(oioubl, input.invoiceDocumentId, input.accessPoint);
  const row = db.query(
    `SELECT id, invoice_document_id, invoice_no, idempotency_key, submission_reference,
            access_point_id, receiver_endpoint_id, oioubl_sha256, envelope_sha256,
            envelope_xml, status, transmission_id, acknowledged_at
     FROM peppol_submissions WHERE idempotency_key = ? LIMIT 1`,
  ).get(idempotencyKey) as PeppolSubmissionRow | null;
  // An already acknowledged event is terminal even if a historical queued
  // event was pruned or never recorded. Never require queued evidence before
  // returning that existing result, and never call the status checker for it.
  const effective = row ? effectiveAcknowledgement(db, row) : null;
  if (row && effective) {
    return {
      ...rowToSubmissionResult(db, row, invoiceNumber, true),
      status: "acknowledged",
      transmissionId: effective.document_id,
    };
  }
  const queued = row ? queuedSubmissionEvent(db, row.id) : null;
  const queuedDocumentId = queued && hasText(queued.document_id)
    ? queued.document_id
    : row?.transmission_id;
  if (!row || row.status !== "prepared" || !hasText(queuedDocumentId)) {
    return { ok: false, invoiceNumber, appliedRules: [PEPPOL_SUBMIT_RULE_ID], errors: ["No queued PEPPOL submission exists for this invoice and configured Digisense identity"] };
  }
  let observed: Awaited<ReturnType<PeppolSubmissionStatusChecker>>;
  try {
    observed = await checkStatus(queuedDocumentId);
  } catch (error) {
    return { ok: false, invoiceNumber, submissionReference: row.submission_reference, idempotencyKey, status: "prepared", transmissionId: queuedDocumentId, appliedRules: [PEPPOL_SUBMIT_RULE_ID], errors: [error instanceof Error ? error.message : String(error)] };
  }
  if (!observed.ok) return { ok: false, invoiceNumber, submissionReference: row.submission_reference, idempotencyKey, status: "prepared", transmissionId: queuedDocumentId, appliedRules: [PEPPOL_SUBMIT_RULE_ID], errors: [observed.error ?? "PEPPOL document-status failed"] };
  const observedAt = observed.observedAt ?? new Date().toISOString();
  runSql(db,
    `INSERT INTO peppol_submission_events
       (submission_id, event_type, document_id, status, observed_at, message, public_url)
     VALUES (?, 'status_observed', ?, ?, ?, ?, ?)`,
    row.id, queuedDocumentId, observed.status ?? "unknown", observedAt,
    observed.message ?? "", observed.publicUrl ?? null,
  );
  insertAuditLog(db, { eventType: "public_einvoice_peppol_status", entityType: "document", entityId: input.invoiceDocumentId, message: `Observed PEPPOL document ${row.transmission_id} status ${observed.status ?? "unknown"} for invoice ${invoiceNumber}` });
  if (observed.status === "delivered") {
    recordSubmissionEvent(db, row.id, "delivered", { documentId: queuedDocumentId, status: "delivered", observedAt, message: observed.message, publicUrl: observed.publicUrl });
    return { ...rowToSubmissionResult(db, row, invoiceNumber, true), status: "acknowledged", transmissionId: queuedDocumentId };
  }
  if (isTerminalAcceptedFailure(observed.status)) {
    return {
      ok: true,
      invoiceNumber,
      submissionReference: row.submission_reference,
      idempotencyKey,
      status: "failed",
      transmissionId: queuedDocumentId,
      appliedRules: [PEPPOL_SUBMIT_RULE_ID],
      errors: [],
    };
  }
  return { ok: false, invoiceNumber, submissionReference: row.submission_reference, idempotencyKey, status: "prepared", transmissionId: queuedDocumentId, appliedRules: [PEPPOL_SUBMIT_RULE_ID], errors: [`PEPPOL submission remains ${observed.status ?? "unknown"}: ${observed.message ?? "no status message"}`] };
}

/**
 * Transmits a public-recipient invoice through an access point and records the
 * outcome.
 *
 * Idempotent: when the invoice was already transmitted (an `acknowledged`
 * submission row exists for the derived idempotency key) the transmitter is
 * not invoked again. A failed attempt is recorded in the audit log only, so a
 * subsequent retry can still succeed and produce the `acknowledged` record.
 */
export async function transmitPublicEInvoicePeppol(
  db: Database,
  input: TransmitPublicEInvoicePeppolInput,
  transmitter: PeppolTransmitter,
): Promise<SubmitPublicEInvoicePeppolResult> {
  const configErrors = validateAccessPointConfig(input.accessPoint);
  if (configErrors.length > 0) {
    return { ok: false, appliedRules: [PEPPOL_SUBMIT_RULE_ID], errors: configErrors };
  }

  // Reuse the shipped OIOUBL handoff slice as the validated transport payload;
  // its own validation surfaces missing public-recipient metadata.
  const oioubl = exportPublicEInvoiceOioUbl(db, { invoiceDocumentId: input.invoiceDocumentId });
  if (!oioubl.ok || !oioubl.sha256 || !oioubl.xml) {
    return {
      ok: false,
      invoiceNumber: oioubl.invoiceNumber,
      appliedRules: [PEPPOL_SUBMIT_RULE_ID, ...oioubl.appliedRules],
      errors:
        oioubl.errors.length > 0
          ? oioubl.errors
          : ["PEPPOL transmission could not generate the required OIOUBL handoff artifact"],
    };
  }

  const { invoiceNumber, receiver, idempotencyKey } = deriveSubmissionIdentity(
    oioubl,
    input.invoiceDocumentId,
    input.accessPoint,
  );
  const oioublSha256 = oioubl.sha256;

  // Reserve the stable key in SQLite before yielding to any external transport.
  // The append-only event tells a concurrent caller whether it is in progress,
  // retryable after a pre-acceptance failure, or already queued at Digisense.
  const reservation = db.transaction(() => {
    const reference = `PEPPOL-${invoiceNumber}-${idempotencyKey.slice(0, 12)}`;
    const envelope = buildPeppolSubmissionEnvelope({ submissionReference: reference, idempotencyKey, invoiceNumber, accessPoint: input.accessPoint, receiverEndpointId: receiver, oioublSha256, status: "prepared" });
    runSql(db,
      `INSERT OR IGNORE INTO peppol_submissions
         (invoice_document_id, invoice_no, idempotency_key, submission_reference, access_point_id, receiver_endpoint_id, oioubl_sha256, envelope_sha256, envelope_xml, status, transmission_id, acknowledged_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'prepared', NULL, NULL)`,
      input.invoiceDocumentId, invoiceNumber, idempotencyKey, reference, input.accessPoint.accessPointId.trim(), receiver,
      oioublSha256, createHash("sha256").update(envelope).digest("hex"), envelope,
    );
    const row = db.query(
      `SELECT id, invoice_document_id, invoice_no, idempotency_key, submission_reference, access_point_id, receiver_endpoint_id, oioubl_sha256, envelope_sha256, envelope_xml, status, transmission_id, acknowledged_at FROM peppol_submissions WHERE idempotency_key = ?`,
    ).get(idempotencyKey) as PeppolSubmissionRow;
    const effective = effectiveAcknowledgement(db, row);
    if (row.status === "acknowledged" || effective) return { row, action: "acknowledged" as const, documentId: effective?.document_id ?? row.transmission_id };
    const latest = latestSubmissionEvent(db, row.id);
    const queued = queuedSubmissionEvent(db, row.id);
    if (queued && hasText(queued.document_id)) {
      const acceptedStatus = latest?.event_type === "status_observed" ? latest.status : queued.status;
      return { row, action: "accepted" as const, documentId: queued.document_id, acceptedStatus };
    }
    if (latest?.event_type === "delivery_failed" && latest.status !== "pre-acceptance-failed") {
      return { row, action: "uncertain" as const };
    }
    if (latest?.event_type === "delivery_reserved") return { row, action: "in_progress" as const };
    // A delivery_failed event proves the prior reservation never obtained a
    // remote id, so it is safe to create a new attempt rather than deadlock it.
    recordSubmissionEvent(db, row.id, "delivery_reserved", { message: "Reserved deterministic PEPPOL delivery attempt" });
    return { row, action: "deliver" as const };
  }).immediate();
  if (reservation.action === "acknowledged") return { ...rowToSubmissionResult(db, reservation.row, invoiceNumber, true), status: "acknowledged", transmissionId: reservation.documentId ?? undefined };
  if (reservation.action === "accepted") return { ok: true, invoiceNumber, submissionReference: reservation.row.submission_reference, idempotencyKey, status: isTerminalAcceptedFailure(reservation.acceptedStatus) ? "failed" : "prepared", duplicate: true, transmissionId: reservation.documentId, appliedRules: [PEPPOL_SUBMIT_RULE_ID], errors: [] };
  if (reservation.action === "uncertain") return { ok: true, invoiceNumber, submissionReference: reservation.row.submission_reference, idempotencyKey, status: "uncertain", duplicate: true, appliedRules: [PEPPOL_SUBMIT_RULE_ID], errors: [] };
  if (reservation.action === "in_progress") return { ok: false, invoiceNumber, submissionReference: reservation.row.submission_reference, idempotencyKey, status: "prepared", appliedRules: [PEPPOL_SUBMIT_RULE_ID], errors: [`PEPPOL transmission is already in progress for invoice ${invoiceNumber}; retry later or poll its queued status.`] };

  // Perform the transport. A thrown error is treated as a failed attempt.
  let outcome: PeppolTransmissionOutcome;
  try {
    outcome = await transmitter({
      oioublXml: oioubl.xml,
      oioublSha256: oioubl.sha256,
      receiverEndpointId: receiver,
      accessPoint: input.accessPoint,
    });
  } catch (error) {
    // A transmitter throw does not reveal whether its delivery POST reached
    // the provider. Fail closed: persist an uncertain result and forbid an
    // automatic second delivery.
    outcome = {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
      deliveryUncertain: true,
    };
  }

  if (!outcome.ok) {
    insertAuditLog(db, {
      eventType: "public_einvoice_peppol_transmission",
      entityType: "document",
      entityId: input.invoiceDocumentId,
      message:
        `PEPPOL transmission failed for invoice ${invoiceNumber} ` +
        `via access point ${input.accessPoint.accessPointId.trim()}: ${outcome.error}`,
    });
    const acceptedDocumentId = outcome.acceptedDocumentId ?? outcome.queuedDocumentId;
    if (acceptedDocumentId) {
      const acceptedStatus = outcome.acceptedStatus ?? "queued-for-delivery";
      recordSubmissionEvent(db, reservation.row.id, "queued", { documentId: acceptedDocumentId, status: acceptedStatus, message: outcome.error });
      insertAuditLog(db, {
        eventType: "public_einvoice_peppol_transmission",
        entityType: "document",
        entityId: input.invoiceDocumentId,
        message:
          `PEPPOL transmission for invoice ${invoiceNumber} was accepted by the access point ` +
          `(documentId ${acceptedDocumentId}, status ${acceptedStatus}); recorded accepted evidence to prevent a re-deliver. ` +
          `Observe only the existing documentId; never retry transmit.`,
      });
      return {
        // The provider assigned a remote identity. Persisted state is the
        // authority: callers reload into queued or terminal-failed mode.
        ok: true,
        invoiceNumber,
        submissionReference: reservation.row.submission_reference,
        idempotencyKey,
        status: isTerminalAcceptedFailure(acceptedStatus) ? "failed" : "prepared",
        transmissionId: acceptedDocumentId,
        appliedRules: [PEPPOL_SUBMIT_RULE_ID],
        errors: [],
      };
    }
    if (outcome.deliveryUncertain || !outcome.retryableBeforeDelivery) {
      recordSubmissionEvent(db, reservation.row.id, "delivery_failed", {
        status: "delivery-uncertain",
        message: outcome.error,
      });
      insertAuditLog(db, {
        eventType: "public_einvoice_peppol_transmission",
        entityType: "document",
        entityId: input.invoiceDocumentId,
        message: `PEPPOL delivery outcome is uncertain for invoice ${invoiceNumber}; automatic re-delivery is blocked pending manual provider reconciliation.`,
      });
      return {
        ok: true,
        invoiceNumber,
        submissionReference: reservation.row.submission_reference,
        idempotencyKey,
        status: "uncertain",
        appliedRules: [PEPPOL_SUBMIT_RULE_ID],
        errors: [],
      };
    }
    recordSubmissionEvent(db, reservation.row.id, "delivery_failed", {
      status: "pre-acceptance-failed",
      message: outcome.error,
    });
    return {
      ok: false,
      invoiceNumber,
      appliedRules: [PEPPOL_SUBMIT_RULE_ID],
      errors: [`PEPPOL transmission failed: ${outcome.error}`],
    };
  }

  // `peppol_submissions` is immutable: delivery is represented by an event.
  recordSubmissionEvent(db, reservation.row.id, "delivered", { documentId: outcome.transmissionId, status: "delivered", observedAt: outcome.transmittedAt, message: "Delivery acknowledged by access point" });
  const submitted = { ...rowToSubmissionResult(db, reservation.row, invoiceNumber, false), status: "acknowledged" as const };
  insertAuditLog(db, {
    eventType: "public_einvoice_peppol_transmission",
    entityType: "document",
    entityId: input.invoiceDocumentId,
    message:
      `Transmitted invoice ${invoiceNumber} via access point ${input.accessPoint.accessPointId.trim()} ` +
      `(transmission ${outcome.transmissionId}, status ${submitted.status})`,
  });
  return { ...submitted, transmissionId: outcome.transmissionId };
}
