import { createHash } from "node:crypto";
import { writeFileSync } from "node:fs";
import type { Database } from "bun:sqlite";
import { insertAuditLog } from "./actor";
import { normalizeEanNumber } from "./ean";
import type { InvoicePayload } from "./invoice";
import { formatAmount } from "./money";

const RULE_ID = "DK-INVOICE-PUBLIC-EXPORT-001";
const OIOUBL_RULE_ID = "DK-INVOICE-PUBLIC-OIOUBL-001";

// The public-recipient handoff document is a Peppol BIS Billing 3.0 invoice
// (UBL 2.1). Denmark's national OIOUBL 3.0 format was cancelled in January
// 2026; Peppol BIS Billing 3.0 is accepted by every Danish public authority
// and is the format NemHandel itself is migrating onto. The surrounding
// "OioUbl" function/CLI names are kept for interface stability.
const OIOUBL_UBL_VERSION = "2.1";
const PEPPOL_BIS_CUSTOMIZATION_ID =
  "urn:cen.eu:en16931:2017#compliant#urn:fdc:peppol.eu:2017:poacc:billing:3.0";
const PEPPOL_BIS_PROFILE_ID = "urn:fdc:peppol.eu:2017:poacc:billing:01:1.0";
// Peppol participant identifier schemes (ISO 6523): 0088 = GLN/EAN for the
// buying public authority, 0184 = Danish CVR for the selling company.
const BUYER_ENDPOINT_SCHEME_ID = "0088";
const SELLER_ENDPOINT_SCHEME_ID = "0184";
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

function buildAddressXml(
  tagName: string,
  address: string | null | undefined,
  indent = "",
  countryCode = "DK",
) {
  if (!hasText(address)) return "";
  return [
    `${indent}<${tagName}>`,
    `${indent}  <cac:AddressLine>`,
    xmlTag("cbc:Line", address.trim(), `${indent}    `),
    `${indent}  </cac:AddressLine>`,
    `${indent}  <cac:Country>`,
    xmlTag("cbc:IdentificationCode", countryCode, `${indent}    `),
    `${indent}  </cac:Country>`,
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
  if (!hasText(payload.seller?.vatOrCvr)) errors.push(`invoice ${invoiceNumber} is missing seller.vatOrCvr required for OIOUBL handoff`);
  if (!hasText(payload.buyer?.name)) errors.push(`invoice ${invoiceNumber} is missing buyer.name required for OIOUBL handoff`);
  if (!hasText(payload.buyer?.address)) errors.push(`invoice ${invoiceNumber} is missing buyer.address required for OIOUBL handoff`);
  if (!eanNumber) errors.push(`invoice ${invoiceNumber} is missing buyer.eanNumber as 13 digits required for OIOUBL handoff`);
  if (!hasText(payload.currency)) errors.push(`invoice ${invoiceNumber} is missing currency required for OIOUBL handoff`);
  if (typeof payload.totals?.netAmount !== "number") errors.push(`invoice ${invoiceNumber} is missing totals.netAmount required for OIOUBL handoff`);
  if (typeof payload.totals?.grossAmount !== "number") errors.push(`invoice ${invoiceNumber} is missing totals.grossAmount required for OIOUBL handoff`);
  if (typeof payload.totals?.vatAmount !== "number") errors.push(`invoice ${invoiceNumber} is missing totals.vatAmount required for OIOUBL handoff`);
  if (typeof payload.totals?.vatRate !== "number") errors.push(`invoice ${invoiceNumber} is missing totals.vatRate required for OIOUBL handoff`);
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
  return errors;
}

function buildPublicEInvoiceOioUblXml(invoiceNumber: string, payload: InvoicePayload) {
  const currency = (payload.currency ?? "DKK").trim().toUpperCase();
  const vatPercent = formatVatPercent(payload.totals?.vatRate);
  const lines = payload.lines ?? [];
  const lineXml = lines
    .map((line, index) => [
      "  <cac:InvoiceLine>",
      xmlTag("cbc:ID", index + 1, "    "),
      xmlTagWithAttrs("cbc:InvoicedQuantity", { unitCode: "H87" }, line.quantity, "    "),
      xmlTagWithAttrs("cbc:LineExtensionAmount", { currencyID: currency }, formatAmount(line.lineTotalExVat), "    "),
      "    <cac:Item>",
      xmlTag("cbc:Name", line.description, "      "),
      "      <cac:ClassifiedTaxCategory>",
      xmlTag("cbc:ID", "S", "        "),
      xmlTag("cbc:Percent", vatPercent, "        "),
      "        <cac:TaxScheme>",
      xmlTag("cbc:ID", "VAT", "          "),
      "        </cac:TaxScheme>",
      "      </cac:ClassifiedTaxCategory>",
      "    </cac:Item>",
      "    <cac:Price>",
      xmlTagWithAttrs("cbc:PriceAmount", { currencyID: currency }, formatAmount(line.unitPriceExVat), "      "),
      "    </cac:Price>",
      "  </cac:InvoiceLine>",
    ].filter(Boolean).join("\n"))
    .join("\n");

  return [
    '<?xml version="1.0" encoding="UTF-8"?>',
    '<Invoice xmlns="urn:oasis:names:specification:ubl:schema:xsd:Invoice-2" xmlns:cac="urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2" xmlns:cbc="urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2">',
    xmlTag("cbc:UBLVersionID", OIOUBL_UBL_VERSION, "  "),
    xmlTag("cbc:CustomizationID", PEPPOL_BIS_CUSTOMIZATION_ID, "  "),
    xmlTag("cbc:ProfileID", PEPPOL_BIS_PROFILE_ID, "  "),
    xmlTag("cbc:ID", invoiceNumber, "  "),
    xmlTag("cbc:IssueDate", payload.issueDate, "  "),
    xmlTag("cbc:DueDate", payload.dueDate, "  "),
    xmlTag("cbc:InvoiceTypeCode", "380", "  "),
    xmlTag("cbc:DocumentCurrencyCode", currency, "  "),
    "  <cac:AccountingSupplierParty>",
    "    <cac:Party>",
    xmlTagWithAttrs("cbc:EndpointID", { schemeID: SELLER_ENDPOINT_SCHEME_ID }, payload.seller?.vatOrCvr, "      "),
    "      <cac:PartyName>",
    xmlTag("cbc:Name", payload.seller?.name, "        "),
    "      </cac:PartyName>",
    buildAddressXml("cac:PostalAddress", payload.seller?.address, "      "),
    "      <cac:PartyTaxScheme>",
    xmlTag("cbc:CompanyID", payload.seller?.vatOrCvr, "        "),
    "        <cac:TaxScheme>",
    xmlTag("cbc:ID", "VAT", "          "),
    "        </cac:TaxScheme>",
    "      </cac:PartyTaxScheme>",
    "      <cac:PartyLegalEntity>",
    xmlTag("cbc:RegistrationName", payload.seller?.name, "        "),
    xmlTag("cbc:CompanyID", payload.seller?.vatOrCvr, "        "),
    "      </cac:PartyLegalEntity>",
    "    </cac:Party>",
    "  </cac:AccountingSupplierParty>",
    "  <cac:AccountingCustomerParty>",
    "    <cac:Party>",
    xmlTagWithAttrs("cbc:EndpointID", { schemeID: BUYER_ENDPOINT_SCHEME_ID }, payload.buyer?.eanNumber, "      "),
    "      <cac:PartyName>",
    xmlTag("cbc:Name", payload.buyer?.name, "        "),
    "      </cac:PartyName>",
    buildAddressXml("cac:PostalAddress", payload.buyer?.address, "      "),
    "      <cac:PartyLegalEntity>",
    xmlTag("cbc:RegistrationName", payload.buyer?.name, "        "),
    "      </cac:PartyLegalEntity>",
    "    </cac:Party>",
    "  </cac:AccountingCustomerParty>",
    "  <cac:TaxTotal>",
    xmlTagWithAttrs("cbc:TaxAmount", { currencyID: currency }, formatAmount(payload.totals?.vatAmount), "    "),
    "    <cac:TaxSubtotal>",
    xmlTagWithAttrs("cbc:TaxableAmount", { currencyID: currency }, formatAmount(payload.totals?.netAmount), "      "),
    xmlTagWithAttrs("cbc:TaxAmount", { currencyID: currency }, formatAmount(payload.totals?.vatAmount), "      "),
    "      <cac:TaxCategory>",
    xmlTag("cbc:ID", "S", "        "),
    xmlTag("cbc:Percent", vatPercent, "        "),
    "        <cac:TaxScheme>",
    xmlTag("cbc:ID", "VAT", "          "),
    "        </cac:TaxScheme>",
    "      </cac:TaxCategory>",
    "    </cac:TaxSubtotal>",
    "  </cac:TaxTotal>",
    "  <cac:LegalMonetaryTotal>",
    xmlTagWithAttrs("cbc:LineExtensionAmount", { currencyID: currency }, formatAmount(payload.totals?.netAmount), "    "),
    xmlTagWithAttrs("cbc:TaxExclusiveAmount", { currencyID: currency }, formatAmount(payload.totals?.netAmount), "    "),
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
  /** 'prepared' or 'acknowledged'. */
  status?: "prepared" | "acknowledged";
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
    xmlTag("Format", "PEPPOL-BIS-3.0", "    "),
    xmlTag("Profile", PEPPOL_BIS_CUSTOMIZATION_ID, "    "),
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
  row: PeppolSubmissionRow,
  invoiceNumber: string,
  duplicate: boolean,
  outPath?: string,
): SubmitPublicEInvoicePeppolResult {
  if (outPath) writeFileSync(outPath, row.envelope_xml);
  return {
    ok: true,
    invoiceNumber,
    submissionReference: row.submission_reference,
    idempotencyKey: row.idempotency_key,
    oioublSha256: row.oioubl_sha256,
    envelopeSha256: row.envelope_sha256,
    envelope: row.envelope_xml,
    status: row.status,
    duplicate,
    outPath,
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
  const receiver = endpointMatch
    ? `${BUYER_ENDPOINT_SCHEME_ID}:${endpointMatch[1]}`
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
    return rowToSubmissionResult(existing, invoiceNumber, true, input.outPath);
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

  db.run(
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
    status,
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
// A successful transmission is recorded as an `acknowledged` peppol_submissions
// row (reusing submitPublicEInvoicePeppol's acknowledgement path). A failed
// attempt is recorded only in the append-only audit log — no submission row is
// written, so a later retry can still reach `acknowledged`.
// ============================================================================

/** Outcome of one transport attempt through an access point. */
export type PeppolTransmissionOutcome =
  | { ok: true; transmissionId: string; transmittedAt: string }
  | { ok: false; error: string };

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

  // Idempotent fast-path: this invoice was already transmitted successfully.
  const existing = db
    .query(
      `SELECT id, invoice_document_id, invoice_no, idempotency_key, submission_reference,
              access_point_id, receiver_endpoint_id, oioubl_sha256, envelope_sha256,
              envelope_xml, status, transmission_id, acknowledged_at
       FROM peppol_submissions WHERE idempotency_key = ? LIMIT 1`,
    )
    .get(idempotencyKey) as PeppolSubmissionRow | null;
  if (existing && existing.status === "acknowledged") {
    return {
      ...rowToSubmissionResult(existing, invoiceNumber, true),
      transmissionId: existing.transmission_id ?? undefined,
    };
  }

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
    outcome = { ok: false, error: error instanceof Error ? error.message : String(error) };
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
    return {
      ok: false,
      invoiceNumber,
      appliedRules: [PEPPOL_SUBMIT_RULE_ID],
      errors: [`PEPPOL transmission failed: ${outcome.error}`],
    };
  }

  // Success: record the submission as acknowledged with the transmission id.
  const submitted = submitPublicEInvoicePeppol(db, {
    invoiceDocumentId: input.invoiceDocumentId,
    accessPoint: input.accessPoint,
    acknowledgement: { transmissionId: outcome.transmissionId, acknowledgedAt: outcome.transmittedAt },
  });
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
