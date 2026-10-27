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
export function approvedInvoiceTextErrors(text: string, payload: InvoicePayload, invoiceNumber: string): string[] {
  const haystack = compact(text);
  const errors: string[] = [];
  const requireOne = (label: string, values: Array<string | undefined>) => {
    if (!values.some((value) => value && haystack.includes(compact(value)))) errors.push(`PDF is missing ${label}`);
  };
  const date = (value: string | undefined) => value ? [value, `${value.slice(8, 10)}.${value.slice(5, 7)}.${value.slice(0, 4)}`, `${value.slice(8, 10)}-${value.slice(5, 7)}-${value.slice(0, 4)}`] : [];
  const amount = (value: number | undefined) => value === undefined ? [] : [formatAmount(value) ?? undefined, new Intl.NumberFormat("da-DK", { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(value)];
  requireOne("invoice number", [invoiceNumber]);
  requireOne("issue date", date(payload.issueDate));
  requireOne("due date", date(payload.dueDate));
  requireOne("seller name", [payload.seller?.name]);
  requireOne("seller address", [payload.seller?.address]);
  requireOne("seller identifier", [payload.seller?.vatOrCvr, payload.seller?.vatOrCvr?.replace(/^DK/i, "")]);
  requireOne("buyer name", [payload.buyer?.name]);
  requireOne("buyer address", [payload.buyer?.address]);
  if (payload.buyer?.vatOrCvr) requireOne("buyer identifier", [payload.buyer.vatOrCvr, payload.buyer.vatOrCvr.replace(/^DK/i, "")]);
  requireOne("buyer EAN", [normalizeEanNumber(payload.buyer?.eanNumber) ?? undefined]);
  requireOne("buyer reference", [payload.buyer?.buyerReference ?? payload.buyer?.orderReference]);
  requireOne("currency", [payload.currency ?? "DKK"]);
  requireOne("net amount", amount(payload.totals?.netAmount));
  requireOne("VAT amount", amount(payload.totals?.vatAmount ?? 0));
  requireOne("gross amount", amount(payload.totals?.grossAmount));
  if (typeof payload.totals?.vatRate === "number" && payload.totals.vatRate > 0) {
    const percentage = payload.totals.vatRate * 100;
    requireOne("VAT rate", [`${percentage}%`, `${String(percentage).replace(".", ",")}%`]);
  }
  for (const line of payload.lines ?? []) requireOne("line description", [line.description]);
  if (payload.vatTreatment === "domestic_reverse_charge" || payload.vatTreatment === "foreign_reverse_charge") {
    requireOne("reverse-charge classification", ["omvendt betalingspligt", "reverse charge"]);
  } else if (payload.totals?.vatRate === 0) {
    requireOne("VAT exemption classification", ["momsfritaget", "exempt from VAT"]);
  }
  return errors;
}

/** Prepare evidence only; no invoice number is reserved and no document is sent. */
export async function preflightApprovedInvoice(
  db: Database,
  input: { payload: InvoicePayload; pdfBytes: Uint8Array; bilateralAttachmentAgreement: boolean; providerBinding: string; environment?: "test" | "production" } & ResolveActorInput,
  client: Pick<DigisenseClient, "validateDocument" | "lookupParticipant"> & Partial<Pick<DigisenseClient, "validateAuth" | "listParticipantsForCompany">>,
) {
  if (!input.bilateralAttachmentAgreement) return { ok: false as const, errors: ["Recipient agreement for an embedded attachment is required"] };
  if (!/^[a-f0-9]{64}$/.test(input.providerBinding)) return { ok: false as const, errors: ["A resolved DigiSense provider binding is required"] };
  if (input.pdfBytes.byteLength === 0 || input.pdfBytes.byteLength > 7_000_000) return { ok: false as const, errors: ["PDF is empty or exceeds the preflight size limit"] };
  const pdfBytes = Buffer.from(input.pdfBytes);
  const prepared = prepareInvoiceApprovalPayload(db, input.payload);
  if (!prepared.ok) return prepared;
  const companyCvr = getCompanySettings(db).cvr?.replace(/^DK/i, "").replace(/\s/g, "");
  const sellerCvr = prepared.payload.seller?.vatOrCvr?.replace(/^DK/i, "").replace(/\s/g, "");
  if (!companyCvr || companyCvr !== sellerCvr) return { ok: false as const, errors: ["Seller CVR must match the company bound to DigiSense"] };
  const ean = normalizeEanNumber(prepared.payload.buyer?.eanNumber);
  const errors = validateOioUblPayload(prepared.invoiceNumber, prepared.payload, ean);
  if (prepared.payload.vatTreatment === "foreign_reverse_charge") {
    const vies = requireCachedViesValidation(db, prepared.payload.buyer?.vatOrCvr, "buyer.vatOrCvr");
    if (!vies.ok) errors.push(...vies.errors);
  }
  if (prepared.payload.lines?.some((line) => line.taxClassification)) errors.push("PDF review for line-level tax classifications is not supported");
  if (!prepared.payload.buyer?.buyerReference && !prepared.payload.buyer?.orderReference) errors.push("An explicit buyer reference is required");
  if (errors.length) return { ok: false as const, errors };
  let parsed: Awaited<ReturnType<typeof parsePdfBytes>>;
  try { parsed = await parsePdfBytes(pdfBytes); }
  catch { return { ok: false as const, errors: ["PDF could not be safely parsed"] }; }
  if (parsed.status !== "ok") return { ok: false as const, errors: ["PDF must contain a readable text layer"] };
  const pdfErrors = approvedInvoiceTextErrors(parsed.pages.map((page) => page.text).join("\n"), prepared.payload, prepared.invoiceNumber);
  if (pdfErrors.length) return { ok: false as const, errors: pdfErrors };
  const xml = buildPublicEInvoiceOioUblXml(prepared.invoiceNumber, prepared.payload, pdfBytes);
  const readiness = await checkDigisenseInvoiceReadiness(client, xml, ean!, input.environment ?? "production");
  if (!readiness.ok) return { ok: false as const, errors: [readiness.error] };
  const id = randomUUID(), pdfSha256 = digest(pdfBytes), xmlSha256 = digest(xml);
  const actor = resolveActor(input);
  return db.transaction(() => {
    // Company details, payment data or number may change while waiting for the provider.
    const current = prepareInvoiceApprovalPayload(db, input.payload);
    if (!current.ok || current.payloadSha256 !== prepared.payloadSha256) return { ok: false as const, errors: ["Invoice changed during preflight; repeat it"] };
    db.query("INSERT INTO approved_invoice_preflights(id,invoice_no,payload_sha256,pdf_sha256,xml_sha256,buyer_ean,provider_binding,actor) VALUES(?,?,?,?,?,?,?,?)")
      .run(id, prepared.invoiceNumber, prepared.payloadSha256, pdfSha256, xmlSha256, ean!, input.providerBinding, actor.auditActor);
    insertAuditLog(db, { eventType: "approved_invoice_preflight", entityType: "invoice_preflight", entityId: id, message: `Invoice preflight ${prepared.invoiceNumber}: PDF ${pdfSha256}, XML ${xmlSha256}`, ...input });
    return { ok: true as const, preflightId: id, invoiceNumber: prepared.invoiceNumber, payloadSha256: prepared.payloadSha256, pdfSha256, xmlSha256, payload: prepared.payload, requiresVisualReview: true as const };
  }).immediate();
}
