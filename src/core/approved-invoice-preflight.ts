import type { Database } from "bun:sqlite";
import { createHash, randomUUID } from "node:crypto";
import { realpathSync } from "node:fs";
import { dirname, isAbsolute, join, relative, sep } from "node:path";
import { insertAuditLog, resolveActor, type ResolveActorInput } from "./actor";
import { parsePdfBytes } from "./document-pdf-parser";
import { normalizeEanNumber } from "./ean";
import type { DigisenseClient } from "./efaktura/digisense-client";
import { checkDigisenseInvoiceReadiness } from "./efaktura/digisense-preflight";
import { prepareInvoiceApprovalPayload } from "./issued-invoices";
import type { InvoicePayload } from "./invoice";
import { buildPublicEInvoiceOioUblXml, validateOioUblPayload } from "./public-einvoice";
import { snapshotDocumentSource } from "./document-storage";
import { getCompanySettings } from "./company";
import { formatAmount } from "./money";
import { requireCachedViesValidation } from "./vies";

export type InvoiceReviewTransport = "digisense" | "email";

const digest = (bytes: Uint8Array | string) => createHash("sha256").update(bytes).digest("hex");
const compact = (text: string) => text.normalize("NFKC").toLocaleLowerCase("da-DK").replace(/\s+/gu, "");

/** The MCP caller must not use the provider as an arbitrary host-file reader. */
export function snapshotApprovedInvoiceSource(companyRoot: string, path: string) {
  const draftsPath = join(realpathSync(companyRoot), "invoices", "drafts");
  const drafts = realpathSync(draftsPath);
  if (drafts !== draftsPath) throw new Error("Approved PDF draft store must not be redirected");
  const actual = realpathSync(path);
  const inside = relative(drafts, actual);
  if (!inside || inside === ".." || inside.startsWith(`..${sep}`) || isAbsolute(inside) || dirname(inside) !== "." || !actual.toLowerCase().endsWith(".pdf")) throw new Error("Approved PDF must be directly in invoices/drafts");
  return snapshotDocumentSource(path, 7_000_000);
}

/** Presence checks complement, but cannot replace, a person's visual review. */
export function approvedInvoiceTextErrors(text: string, payload: InvoicePayload, invoiceNumber: string, warnings: string[] = [], transport: InvoiceReviewTransport = "digisense"): string[] {
  const haystack = compact(text);
  const errors: string[] = [];
  const requireOne = (label: string, values: Array<string | undefined>, allowSeparatedWords = false) => {
    if (!values.some((value) => {
      if (!value) return false;
      if (haystack.includes(compact(value))) return true;
      if (!allowSeparatedWords) return false;
      // PDF text extraction may interleave separate layout fields. Check each
      // meaningful word independently; visual confirmation remains mandatory.
      const words = value.normalize("NFKC").toLocaleLowerCase("da-DK").match(/[\p{L}\p{N}]+/gu) ?? [];
      const available = new Set(text.normalize("NFKC").toLocaleLowerCase("da-DK").match(/[\p{L}\p{N}]+/gu) ?? []);
      return words.length > 1 && words.every((word) => available.has(word));
    })) errors.push(`PDF is missing ${label}`);
  };
  const date = (value: string | undefined) => value ? [value, `${value.slice(8, 10)}.${value.slice(5, 7)}.${value.slice(0, 4)}`, `${value.slice(8, 10)}-${value.slice(5, 7)}-${value.slice(0, 4)}`] : [];
  const amount = (value: number | undefined) => value === undefined ? [] : [formatAmount(value) ?? undefined, new Intl.NumberFormat("da-DK", { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(value)];
  requireOne("invoice number", [invoiceNumber]);
  requireOne("issue date", date(payload.issueDate));
  requireOne("due date", date(payload.dueDate));
  requireOne("seller name", [payload.seller?.name], true);
  requireOne("seller address", [payload.seller?.address], true);
  requireOne("seller identifier", [payload.seller?.vatOrCvr, payload.seller?.vatOrCvr?.replace(/^DK/i, "")]);
  requireOne("buyer name", [payload.buyer?.name], true);
  requireOne("buyer address", [payload.buyer?.address], true);
  if (payload.buyer?.vatOrCvr) requireOne("buyer identifier", [payload.buyer.vatOrCvr, payload.buyer.vatOrCvr.replace(/^DK/i, "")]);
  if (transport === "digisense" || payload.buyer?.eanNumber) requireOne("buyer EAN", [normalizeEanNumber(payload.buyer?.eanNumber) ?? undefined]);
  if (transport === "digisense" || payload.buyer?.buyerReference || payload.buyer?.orderReference) requireOne("buyer reference", [payload.buyer?.buyerReference ?? payload.buyer?.orderReference], true);
  requireOne("currency", [payload.currency ?? "DKK"]);
  requireOne("net amount", amount(payload.totals?.netAmount));
  requireOne("VAT amount", amount(payload.totals?.vatAmount ?? 0));
  requireOne("gross amount", amount(payload.totals?.grossAmount));
  if (typeof payload.totals?.vatRate === "number" && payload.totals.vatRate > 0) {
    const percentage = payload.totals.vatRate * 100;
    requireOne("VAT rate", [`${percentage}%`, `${String(percentage).replace(".", ",")}%`]);
  }
  for (const line of payload.lines ?? []) {
    const before = errors.length;
    requireOne("line description", [line.description], true);
    if (errors.length === before || !line.description) continue;
    // Long descriptions can also contain labels for information displayed in
    // separate PDF fields. A near match is never proof of equivalence: expose
    // the unmatched words for the mandatory human review before issuance.
    const words = line.description.normalize("NFKC").toLocaleLowerCase("da-DK").match(/[\p{L}\p{N}]+/gu) ?? [];
    const available = new Set(text.normalize("NFKC").toLocaleLowerCase("da-DK").match(/[\p{L}\p{N}]+/gu) ?? []);
    const missing = words.filter((word) => !available.has(word));
    if (words.length >= 20 && missing.length > 0 && missing.length / words.length <= 0.1 && missing.every((word) => !/\d/u.test(word))) {
      errors.pop();
      warnings.push(`PDF line description requires visual review; words not found: ${missing.join(", ")}`);
    }
  }
  if (payload.vatTreatment === "domestic_reverse_charge" || payload.vatTreatment === "foreign_reverse_charge") {
    requireOne("reverse-charge classification", ["omvendt betalingspligt", "reverse charge"]);
  } else if (payload.totals?.vatRate === 0 || (payload.lines?.length && payload.lines.every((line) => line.taxClassification === "exempt"))) {
    requireOne("VAT exemption classification", ["momsfritaget", "exempt from VAT"]);
  }
  return errors;
}

/** Prepare evidence only; no invoice number is reserved and no document is sent. */
export async function preflightApprovedInvoice(
  db: Database,
  input: { payload: InvoicePayload; pdfBytes: Uint8Array; bilateralAttachmentAgreement: boolean; attachmentPolicy?: "embedded" | "none"; descriptionDifferencesReviewed?: boolean; transport?: InvoiceReviewTransport; providerBinding?: string; environment?: "test" | "production" } & ResolveActorInput,
  client?: Pick<DigisenseClient, "validateDocument" | "lookupParticipant"> & Partial<Pick<DigisenseClient, "validateAuth" | "listParticipantsForCompany">>,
) {
  const transport = input.transport ?? "digisense";
  if (transport !== "email" && transport !== "digisense") return { ok: false as const, errors: ["Invalid invoice review transport"] };
  const attachmentPolicy = input.attachmentPolicy ?? (transport === "email" ? "none" : "embedded");
  if (transport === "email" && attachmentPolicy !== "none") return { ok: false as const, errors: ["Email PDF review requires attachment policy none; embedded is a DigiSense policy"] };
  if (attachmentPolicy !== "embedded" && attachmentPolicy !== "none") return { ok: false as const, errors: ["Invalid PDF attachment policy"] };
  if (attachmentPolicy === "embedded" && !input.bilateralAttachmentAgreement) return { ok: false as const, errors: ["Recipient agreement for an embedded attachment is required"] };
  if (transport === "digisense" && (!client || !/^[a-f0-9]{64}$/.test(input.providerBinding ?? ""))) return { ok: false as const, errors: ["A resolved DigiSense provider binding is required"] };
  if (input.pdfBytes.byteLength === 0 || input.pdfBytes.byteLength > 7_000_000) return { ok: false as const, errors: ["PDF is empty or exceeds the preflight size limit"] };
  const pdfBytes = Buffer.from(input.pdfBytes);
  const prepared = prepareInvoiceApprovalPayload(db, input.payload);
  if (!prepared.ok) return prepared;
  const companyCvr = getCompanySettings(db).cvr?.replace(/^DK/i, "").replace(/\s/g, "");
  const sellerCvr = prepared.payload.seller?.vatOrCvr?.replace(/^DK/i, "").replace(/\s/g, "");
  if (!companyCvr || companyCvr !== sellerCvr) return { ok: false as const, errors: ["Seller CVR must match the company"] };
  const ean = normalizeEanNumber(prepared.payload.buyer?.eanNumber);
  const errors = transport === "digisense" ? validateOioUblPayload(prepared.invoiceNumber, prepared.payload, ean) : [];
  if (prepared.payload.vatTreatment === "foreign_reverse_charge") {
    const vies = requireCachedViesValidation(db, prepared.payload.buyer?.vatOrCvr, "buyer.vatOrCvr");
    if (!vies.ok) errors.push(...vies.errors);
  }
  if (prepared.payload.lines?.some((line) => line.taxClassification) &&
      !prepared.payload.lines.every((line) => line.taxClassification === "exempt") &&
      !(transport === "email" && (prepared.payload.vatTreatment ?? "standard") === "standard" && prepared.payload.lines.every((line) => (line.taxClassification ?? "taxable") === "taxable" && (line.vatRate ?? prepared.payload.totals?.vatRate) === prepared.payload.totals?.vatRate))) errors.push("PDF review for mixed or non-exempt line-level tax classifications is not supported");
  if (transport === "digisense" && !prepared.payload.buyer?.buyerReference && !prepared.payload.buyer?.orderReference) errors.push("An explicit buyer reference is required");
  if (errors.length) return { ok: false as const, errors };
  let parsed: Awaited<ReturnType<typeof parsePdfBytes>>;
  try { parsed = await parsePdfBytes(pdfBytes); }
  catch { return { ok: false as const, errors: ["PDF could not be safely parsed"] }; }
  if (parsed.status !== "ok") return { ok: false as const, errors: ["PDF must contain a readable text layer"] };
  const warnings: string[] = [];
  const pdfErrors = approvedInvoiceTextErrors(parsed.pages.map((page) => page.text).join("\n"), prepared.payload, prepared.invoiceNumber, warnings, transport);
  if (pdfErrors.length) return { ok: false as const, errors: pdfErrors };
  if (warnings.length && input.descriptionDifferencesReviewed !== true) return { ok: false as const, errors: ["PDF description differences require explicit visual review before preflight", ...warnings], warnings };
  if (transport === "email") {
    const id = randomUUID(), pdfSha256 = digest(pdfBytes);
    const actor = resolveActor(input);
    return db.transaction(() => {
      const current = prepareInvoiceApprovalPayload(db, input.payload);
      if (!current.ok || current.payloadSha256 !== prepared.payloadSha256) return { ok: false as const, errors: ["Invoice changed during preflight; repeat it"] };
      db.query("INSERT INTO email_invoice_pdf_reviews(id,invoice_no,payload_sha256,pdf_sha256,actor) VALUES(?,?,?,?,?)")
        .run(id, prepared.invoiceNumber, prepared.payloadSha256, pdfSha256, actor.auditActor);
      insertAuditLog(db, { eventType: "approved_invoice_preflight", entityType: "invoice_preflight", entityId: id, message: `Invoice preflight ${prepared.invoiceNumber}: transport email, PDF ${pdfSha256}; description differences reviewed: ${warnings.length > 0}`, ...input });
      return { ok: true as const, transport, preflightId: id, invoiceNumber: prepared.invoiceNumber, attachmentPolicy, payloadSha256: prepared.payloadSha256, pdfSha256, payload: prepared.payload, warnings, requiresVisualReview: true as const };
    }).immediate();
  }
  let version: "2.1" | "2.02" = "2.1";
  const transportPdf = attachmentPolicy === "embedded" ? pdfBytes : undefined;
  let xml = buildPublicEInvoiceOioUblXml(prepared.invoiceNumber, prepared.payload, transportPdf, version);
  let readiness = await checkDigisenseInvoiceReadiness(client!, xml, ean!, input.environment ?? "production", "2.1");
  // A 2.02-only recipient is still supported, but only after validating and
  // checking that exact older document; all other errors remain fail-closed.
  if (!readiness.ok && readiness.code === "recipient_not_found") {
    version = "2.02";
    xml = buildPublicEInvoiceOioUblXml(prepared.invoiceNumber, prepared.payload, transportPdf, version);
    readiness = await checkDigisenseInvoiceReadiness(client!, xml, ean!, input.environment ?? "production", version);
  }
  if (!readiness.ok) return { ok: false as const, errors: [readiness.error, ...warnings], warnings };
  const id = randomUUID(), pdfSha256 = digest(pdfBytes), xmlSha256 = digest(xml);
  const actor = resolveActor(input);
  return db.transaction(() => {
    // Company details, payment data or number may change while waiting for the provider.
    const current = prepareInvoiceApprovalPayload(db, input.payload);
    if (!current.ok || current.payloadSha256 !== prepared.payloadSha256) return { ok: false as const, errors: ["Invoice changed during preflight; repeat it"] };
    db.query("INSERT INTO approved_invoice_preflights(id,invoice_no,payload_sha256,pdf_sha256,xml_sha256,buyer_ean,provider_binding,actor) VALUES(?,?,?,?,?,?,?,?)")
      .run(id, prepared.invoiceNumber, prepared.payloadSha256, pdfSha256, xmlSha256, ean!, input.providerBinding!, actor.auditActor);
    insertAuditLog(db, { eventType: "approved_invoice_preflight", entityType: "invoice_preflight", entityId: id, message: `Invoice preflight ${prepared.invoiceNumber}: PDF ${pdfSha256}, XML ${xmlSha256}; attachment policy: ${attachmentPolicy}; description differences reviewed: ${warnings.length > 0}`, ...input });
    return { ok: true as const, transport, preflightId: id, invoiceNumber: prepared.invoiceNumber, oioublVersion: version, attachmentPolicy, payloadSha256: prepared.payloadSha256, pdfSha256, xmlSha256, payload: prepared.payload, warnings, requiresVisualReview: true as const };
  }).immediate();
}
