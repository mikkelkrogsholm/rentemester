import { createHash } from "node:crypto";
import { mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { Database } from "bun:sqlite";
import type { InvoicePayload } from "./invoice";
import { companyPaths } from "./paths";
import { insertAuditLog } from "./actor";
import { promoteTempFile, removeIfExists, writeTempFileFor } from "./atomic-file";
import { formatAmount, formatDkk } from "./money";

const RULE_ID = "DK-INVOICE-ISSUE-001";
const PDF_DOCUMENT_TYPE = "issued_invoice_pdf";

export type RenderIssuedInvoicePdfInput = {
  invoiceDocumentId: number;
};

export type RenderIssuedInvoicePdfResult = {
  ok: boolean;
  renderDocumentId?: number;
  invoiceNumber?: string;
  storedPath?: string;
  sha256?: string;
  appliedRules: string[];
  errors: string[];
};

function sha256(buffer: Uint8Array) {
  return createHash("sha256").update(buffer).digest("hex");
}

function escapePdfText(value: string) {
  return value.replace(/\\/g, "\\\\").replace(/\(/g, "\\(").replace(/\)/g, "\\)").replace(/\r?\n/g, " ");
}

function amountLabel(value: unknown, currency = "DKK") {
  if (value == null || value === "") return null;
  const amount = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(amount)) return null;
  return formatDkk(amount, currency);
}

function compact(value: string | null | undefined) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

function invoiceTextLines(payload: InvoicePayload & { invoiceNumber?: string; issuedAt?: string; status?: string }) {
  const currency = (payload.currency ?? "DKK").trim().toUpperCase();
  const lines: string[] = [];
  lines.push("Faktura");
  lines.push(`Fakturanummer: ${payload.invoiceNumber ?? ""}`);
  lines.push(`Fakturadato: ${payload.issueDate ?? ""}`);
  if (payload.dueDate) lines.push(`Forfaldsdato: ${payload.dueDate}`);
  lines.push(`Valuta: ${currency}`);
  lines.push("");
  lines.push("Saelger:");
  if (payload.seller?.name) lines.push(payload.seller.name);
  if (payload.seller?.address) lines.push(payload.seller.address);
  if (payload.seller?.vatOrCvr) lines.push(`CVR/VAT: ${payload.seller.vatOrCvr}`);
  lines.push("");
  lines.push("Koeber:");
  if (payload.buyer?.name) lines.push(payload.buyer.name);
  if (payload.buyer?.address) lines.push(payload.buyer.address);
  if (payload.buyer?.vatOrCvr) lines.push(`CVR/VAT: ${payload.buyer.vatOrCvr}`);
  lines.push("");
  lines.push("Linjer:");
  for (const line of payload.lines ?? []) {
    const qty = line.quantity == null ? "" : ` x ${line.quantity}`;
    const unit = line.unitPriceExVat == null ? "" : ` @ ${formatAmount(line.unitPriceExVat) ?? ""}`;
    const total = line.lineTotalExVat == null ? "" : ` = ${formatDkk(line.lineTotalExVat, currency) ?? ""}`;
    lines.push(`- ${line.description ?? ""}${qty}${unit}${total}`.trim());
  }
  lines.push("");
  const net = amountLabel(payload.totals?.netAmount, currency);
  const vat = amountLabel(payload.totals?.vatAmount, currency);
  const gross = amountLabel(payload.totals?.grossAmount, currency);
  if (net) lines.push(`Netto: ${net}`);
  if (vat) lines.push(`Moms: ${vat}`);
  if (gross) lines.push(`Total: ${gross}`);
  const fx = payload.totals?.fxRateToDkk == null ? null : Number(payload.totals.fxRateToDkk).toFixed(6);
  const grossDkk = amountLabel(payload.totals?.grossAmountDkk, "DKK");
  if (currency !== "DKK" && fx) lines.push(`Valutakurs til DKK: ${fx}`);
  if (currency !== "DKK" && grossDkk) lines.push(`Total DKK: ${grossDkk}`);
  if (payload.reverseChargeNote) lines.push(`Note: ${payload.reverseChargeNote}`);
  return lines.map((line) => compact(line) ?? "");
}

export function buildIssuedInvoicePdf(payload: InvoicePayload & { invoiceNumber?: string; issuedAt?: string; status?: string }) {
  const lines = invoiceTextLines(payload);
  const streamLines = ["BT", "/F1 12 Tf"];
  let y = 805;
  for (const line of lines) {
    const text = escapePdfText(line);
    streamLines.push(`1 0 0 1 48 ${y} Tm (${text}) Tj`);
    y -= line === "" ? 10 : 16;
    if (y < 48) break;
  }
  streamLines.push("ET");
  const content = `${streamLines.join("\n")}\n`;
  const issueDate = compact(payload.issueDate) ?? "1970-01-01";
  const pdfDate = `D:${issueDate.replace(/-/g, "")}000000Z`;
  const producer = escapePdfText("Rentemester deterministic invoice renderer");
  const title = escapePdfText(`Invoice ${payload.invoiceNumber ?? ""}`);
  const objects = [
    "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
    "2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] >>\nendobj\n",
    "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n",
    `4 0 obj\n<< /Length ${Buffer.byteLength(content, "utf8")} >>\nstream\n${content}endstream\nendobj\n`,
    "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
    `6 0 obj\n<< /Producer (${producer}) /Title (${title}) /CreationDate (${pdfDate}) /ModDate (${pdfDate}) >>\nendobj\n`,
  ];

  let pdf = "%PDF-1.4\n%\xE2\xE3\xCF\xD3\n";
  const offsets: number[] = [0];
  for (const object of objects) {
    offsets.push(Buffer.byteLength(pdf, "binary"));
    pdf += object;
  }
  const xrefOffset = Buffer.byteLength(pdf, "binary");
  pdf += `xref\n0 ${objects.length + 1}\n`;
  pdf += "0000000000 65535 f \n";
  for (let i = 1; i < offsets.length; i += 1) {
    pdf += `${String(offsets[i]).padStart(10, "0")} 00000 n \n`;
  }
  pdf += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R /Info 6 0 R >>\nstartxref\n${xrefOffset}\n%%EOF\n`;
  return Buffer.from(pdf, "binary");
}

export function renderIssuedInvoicePdf(db: Database, companyRoot: string, input: RenderIssuedInvoicePdfInput): RenderIssuedInvoicePdfResult {
  const invoice = db.query(
    `SELECT id, invoice_no, invoice_date, payload_json, status, retain_until
     FROM documents WHERE id = ? AND document_type = 'issued_invoice'`
  ).get(input.invoiceDocumentId) as {
    id: number;
    invoice_no: string | null;
    invoice_date: string | null;
    payload_json: string | null;
    status: string | null;
    retain_until: string | null;
  } | null;

  if (!invoice) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice document ${input.invoiceDocumentId} does not exist or is not an issued invoice`] };
  if (!invoice.payload_json) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice ${invoice.invoice_no ?? input.invoiceDocumentId} is missing payload_json`] };

  const payload = JSON.parse(invoice.payload_json) as InvoicePayload & { invoiceNumber?: string; issuedAt?: string; status?: string };
  const invoiceNumber = compact(invoice.invoice_no ?? payload.invoiceNumber);
  if (!invoiceNumber) return { ok: false, appliedRules: [RULE_ID], errors: ["issued invoice is missing invoice number"] };

  const paths = companyPaths(companyRoot);
  mkdirSync(paths.invoicesIssued, { recursive: true });
  const storedPath = join(paths.invoicesIssued, `${invoiceNumber}.pdf`);
  const bytes = buildIssuedInvoicePdf({ ...payload, invoiceNumber, status: payload.status ?? invoice.status ?? "issued" });
  const hash = sha256(bytes);
  let tempPath: string | undefined;

  try {
    const result = db.transaction(() => {
      const existing = db.query(
        `SELECT id, sha256_hash, stored_path FROM documents
         WHERE document_type = ? AND invoice_no = ?
         ORDER BY id DESC LIMIT 1`
      ).get(PDF_DOCUMENT_TYPE, invoiceNumber) as { id: number; sha256_hash: string; stored_path: string | null } | null;

      tempPath = writeTempFileFor(storedPath, bytes);

      if (existing && existing.sha256_hash === hash) {
        return { renderDocumentId: existing.id };
      }

      const inserted = db.query(
        `INSERT INTO documents (
          document_no, source, original_filename, stored_path, mime_type, sha256_hash,
          supplier_name, invoice_no, invoice_date, amount_inc_vat, currency, status,
          document_type, sender_name, sender_address, sender_vat_cvr,
          recipient_name, recipient_address, recipient_vat_cvr, vat_amount, payload_json, retain_until
        ) VALUES (?, 'rentemester', ?, ?, 'application/pdf', ?, ?, ?, ?, ?, ?, 'issued', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING id`
      ).get(
        `${invoiceNumber}-pdf`,
        `${invoiceNumber}.pdf`,
        storedPath,
        hash,
        payload.seller?.name ?? null,
        invoiceNumber,
        invoice.invoice_date ?? payload.issueDate ?? null,
        payload.totals?.grossAmount ?? null,
        payload.currency ?? "DKK",
        PDF_DOCUMENT_TYPE,
        payload.seller?.name ?? null,
        payload.seller?.address ?? null,
        payload.seller?.vatOrCvr ?? null,
        payload.buyer?.name ?? null,
        payload.buyer?.address ?? null,
        payload.buyer?.vatOrCvr ?? null,
        payload.totals?.vatAmount ?? null,
        invoice.payload_json,
        invoice.retain_until ?? null,
      ) as { id: number };

      insertAuditLog(db, {
        eventType: "invoice_render_pdf",
        entityType: "document",
        entityId: inserted.id,
        message: `Rendered invoice PDF ${invoiceNumber}`,
      });

      return { renderDocumentId: inserted.id };
    }, { immediate: true })();

    promoteTempFile(tempPath!, storedPath);
    return { ok: true, renderDocumentId: result.renderDocumentId, invoiceNumber, storedPath, sha256: hash, appliedRules: [RULE_ID], errors: [] };
  } catch (error) {
    if (tempPath) removeIfExists(tempPath);
    return { ok: false, appliedRules: [RULE_ID], errors: [String(error)] };
  }
}

export function readIssuedInvoicePdfText(path: string) {
  return readFileSync(path, "latin1");
}
