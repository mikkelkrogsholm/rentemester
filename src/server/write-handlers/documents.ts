// Document ingest + expense-from-bank booking handlers (#213 slice 3, #407).

import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { extname, join } from "node:path";
import { applyPurchaseVatPreflight } from "../../cli/purchase-vat-preflight";
import { parseRegisteredPdfBatch, parseRegisteredPdfDocument, planCurrentPdfParses } from "../../core/document-pdf-parser";
import { type DocumentMetadata, ingestDocumentAsync } from "../../core/documents";
import {
  bookExpenseFromBank,
  type ExpenseVatTreatment,
} from "../../core/expense-booking";
import { removePathWithRetry } from "../../core/fs-cleanup";
import { resolveDocumentMasterData } from "../../core/master-data";
import type { SupplierIdentifierKind } from "../../core/supplier-identity";
import { withCockpitActor } from "../actor";
import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { extractDocumentInvoice, invoiceExtractionSurface } from "../invoice-extraction-surface";
import { withCompanyMutation } from "../mutations";
import {
  MAX_UPLOAD_BODY_BYTES,
  okResponse,
  optionalBodyPositiveInt,
  optionalBodyString,
  requireBodyPositiveInt,
  requireBodyString,
} from "./_shared";

/** The write boundary returns operational facts, never parser pages/layout. */
function parseSummary(run: any, documentId?: number) {
  return {
    documentId, status: run?.status, errorCode: run?.errorCode ?? null,
    cached: Boolean(run?.cached), pageCount: Array.isArray(run?.pages) ? run.pages.length : 0,
    itemCount: Array.isArray(run?.pages) ? run.pages.reduce((n: number, p: any) => n + (p.layout?.length ?? 0), 0) : 0,
    textLength: Array.isArray(run?.pages) ? run.pages.reduce((n: number, p: any) => n + (p.text?.length ?? 0), 0) : 0,
    resultHash: run?.resultHash,
  };
}

/** Parse routes are explicit opt-in: ingest never invokes this service. */
export async function handleDocumentPdfParse(config: ServerConfig, request: Request, slug: string, idRaw: string): Promise<Response> {
  const documentId = Number(idRaw); if (!Number.isInteger(documentId) || documentId <= 0) throw ApiError.badRequest("document id must be a positive integer");
  const result = await withCompanyMutation(request, config, slug, async ({ db, actor, companyRoot }) => {
    try { return { ok: true, parse: parseSummary(await parseRegisteredPdfDocument(db, companyRoot, { documentId, createdBy: actor.createdBy, createdByProgram: actor.createdByProgram }), documentId) }; }
    catch { return { ok: false, errors: ["PDF_PARSE_FAILED"] }; }
  }, { requireConfirm: true });
  return okResponse(result.ok ? { parse: result.parse } : { errors: ["PDF_PARSE_FAILED"] });
}

export async function handleDocumentPdfParsePending(config: ServerConfig, request: Request, slug: string): Promise<Response> {
  const result = await withCompanyMutation(request, config, slug, async ({ db, actor, companyRoot }, body) => {
    const limit = body.limit === undefined ? 100 : Number(body.limit); if (!Number.isInteger(limit) || limit < 1 || limit > 100) throw ApiError.badRequest("limit must be an integer between 1 and 100");
    const cursor = body.cursor === undefined ? 0 : Number(body.cursor); if (!Number.isInteger(cursor) || cursor < 0) throw ApiError.badRequest("cursor must be a non-negative integer");
    const plan=planCurrentPdfParses(db,{limit,cursor});
    const parses = await parseRegisteredPdfBatch(db, companyRoot, plan.documentIds, { createdBy: actor.createdBy, createdByProgram: actor.createdByProgram });
    const failed = parses.filter((entry: any) => !entry.ok);
    return { ok: true, batch: { requested: plan.documentIds.length, parsed: parses.length - failed.length, failed: failed.length, cursor:plan.cursor, nextCursor:plan.nextCursor, resume: failed.length ? { documentIds: failed.map((entry: any) => entry.documentId) } : null } };
  }, { requireConfirm: true });
  return okResponse({ batch: result.batch });
}

/**
 * Parses + validates the `metadata` body field into a core `DocumentMetadata`.
 * The shape mirrors the MCP `documents_ingest` input — amounts are kroner.
 * Anything malformed is a 400; core's `validateDocumentMetadata` performs the
 * deeper bookkeeping validation.
 */
function parseDocumentMetadata(raw: unknown): DocumentMetadata {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    throw ApiError.badRequest("'metadata' is required and must be an object");
  }
  const m = raw as Record<string, unknown>;

  function str(key: string): string | undefined {
    const v = m[key];
    if (v === undefined || v === null) return undefined;
    if (typeof v !== "string") {
      throw ApiError.badRequest(`metadata.${key} must be a string when present`);
    }
    const t = v.trim();
    return t.length > 0 ? t : undefined;
  }
  function num(key: string): number | undefined {
    const v = m[key];
    if (v === undefined || v === null) return undefined;
    if (typeof v !== "number" || !Number.isFinite(v)) {
      throw ApiError.badRequest(`metadata.${key} must be a number when present`);
    }
    return v;
  }
  function bool(key: string): boolean | undefined {
    const v = m[key];
    if (v === undefined || v === null) return undefined;
    if (typeof v !== "boolean") {
      throw ApiError.badRequest(`metadata.${key} must be a boolean when present`);
    }
    return v;
  }
  function party(key: string): DocumentMetadata["sender"] {
    const v = m[key];
    if (v === undefined || v === null) return undefined;
    if (typeof v !== "object" || Array.isArray(v)) {
      throw ApiError.badRequest(`metadata.${key} must be an object when present`);
    }
    const p = v as Record<string, unknown>;
    for (const f of ["name", "address", "vatOrCvr", "countryCode", "identifierKind"]) {
      if (p[f] !== undefined && p[f] !== null && typeof p[f] !== "string") {
        throw ApiError.badRequest(`metadata.${key}.${f} must be a string when present`);
      }
    }
    const trim = (x: unknown) =>
      typeof x === "string" && x.trim().length > 0 ? x.trim() : undefined;
    const identifierKind = trim(p.identifierKind);
    if (identifierKind !== undefined && !["dk_cvr", "eu_vat", "non_eu"].includes(identifierKind)) {
      throw ApiError.badRequest(`metadata.${key}.identifierKind must be dk_cvr, eu_vat or non_eu when present`);
    }
    return {
      name: trim(p.name),
      address: trim(p.address),
      vatOrCvr: trim(p.vatOrCvr),
      countryCode: trim(p.countryCode),
      identifierKind: identifierKind as SupplierIdentifierKind | undefined,
    };
  }

  const source = str("source");
  if (!source) {
    throw ApiError.badRequest("metadata.source is required");
  }
  const documentType = m.documentType;
  if (
    documentType !== undefined &&
    documentType !== "purchase_sale" &&
    documentType !== "cash_register_receipt" &&
    documentType !== "internal_voucher"
  ) {
    throw ApiError.badRequest(
      "metadata.documentType must be 'purchase_sale', 'cash_register_receipt' or 'internal_voucher'",
    );
  }
  const exemptionCode = m.exemptionCode;
  if (
    exemptionCode !== undefined &&
    exemptionCode !== null &&
    exemptionCode !== "FOREIGN_PHYSICAL_ONLY"
  ) {
    throw ApiError.badRequest(
      "metadata.exemptionCode must be 'FOREIGN_PHYSICAL_ONLY' or null when present",
    );
  }
  const rawLines = m.purchaseVatLines;
  let purchaseVatLines: DocumentMetadata["purchaseVatLines"];
  if (rawLines !== undefined) {
    if (!Array.isArray(rawLines) || rawLines.length === 0) throw ApiError.badRequest("metadata.purchaseVatLines must be a non-empty array when present");
    purchaseVatLines = rawLines.map((line, index) => {
      if (!line || typeof line !== "object" || Array.isArray(line)) throw ApiError.badRequest(`metadata.purchaseVatLines[${index}] must be an object`);
      const item = line as Record<string, unknown>;
      if (!['dk_purchase_25', 'exempt'].includes(String(item.classification))) throw ApiError.badRequest(`metadata.purchaseVatLines[${index}].classification is invalid`);
      if (typeof item.netAmount !== "number" || !Number.isFinite(item.netAmount)) throw ApiError.badRequest(`metadata.purchaseVatLines[${index}].netAmount must be a number`);
      if (item.vatAmount !== undefined && (typeof item.vatAmount !== "number" || !Number.isFinite(item.vatAmount))) throw ApiError.badRequest(`metadata.purchaseVatLines[${index}].vatAmount must be a number`);
      return { classification: item.classification as any, netAmount: item.netAmount, ...(item.vatAmount === undefined ? {} : { vatAmount: item.vatAmount as number }) };
    });
  }
  const rawWordingEvidence = m.reverseChargeWordingEvidence;
  let reverseChargeWordingEvidence: DocumentMetadata["reverseChargeWordingEvidence"];
  if (rawWordingEvidence !== undefined) {
    if (!rawWordingEvidence || typeof rawWordingEvidence !== "object" || Array.isArray(rawWordingEvidence)) throw ApiError.badRequest("metadata.reverseChargeWordingEvidence must be an object");
    const evidence = rawWordingEvidence as Record<string, unknown>;
    if (typeof evidence.excerpt !== "string" || typeof evidence.location !== "string") throw ApiError.badRequest("metadata.reverseChargeWordingEvidence requires excerpt and location strings");
    reverseChargeWordingEvidence = { excerpt: evidence.excerpt, location: evidence.location };
  }
  const rawExternalEvidence = m.externalAccountingEvidence;
  let externalAccountingEvidence: DocumentMetadata["externalAccountingEvidence"];
  if (rawExternalEvidence !== undefined) {
    if (!rawExternalEvidence || typeof rawExternalEvidence !== "object" || Array.isArray(rawExternalEvidence)) throw ApiError.badRequest("metadata.externalAccountingEvidence must be an object");
    const evidence = rawExternalEvidence as Record<string, unknown>;
    const totals = evidence.totals;
    if (evidence.category !== "payroll" || typeof evidence.accountingPeriod !== "string" || typeof evidence.externalReference !== "string" || !totals || typeof totals !== "object" || Array.isArray(totals) || typeof (totals as Record<string, unknown>).debitAmount !== "number" || typeof (totals as Record<string, unknown>).creditAmount !== "number") throw ApiError.badRequest("metadata.externalAccountingEvidence requires payroll category, period, reference and numeric totals");
    externalAccountingEvidence = { category: "payroll", accountingPeriod: evidence.accountingPeriod, externalReference: evidence.externalReference, totals: { debitAmount: (totals as Record<string, number>).debitAmount, creditAmount: (totals as Record<string, number>).creditAmount } };
  }

  return {
    source,
    documentType: documentType as DocumentMetadata["documentType"],
    issueDate: str("issueDate"),
    invoiceNo: str("invoiceNo"),
    deliveryDescription: str("deliveryDescription"),
    amountIncVat: num("amountIncVat"),
    currency: str("currency"),
    sender: party("sender"),
    recipient: party("recipient"),
    vatAmount: num("vatAmount"),
    purchaseVatLines,
    reverseChargeWordingConfirmed: bool("reverseChargeWordingConfirmed"),
    reverseChargeWordingEvidence,
    danishSimplifiedPurchaseInvoice: bool("danishSimplifiedPurchaseInvoice"),
    incompleteStandardPurchaseInvoice: bool("incompleteStandardPurchaseInvoice"),
    paymentDetails: str("paymentDetails"),
    exemptionCode: (exemptionCode ?? undefined) as DocumentMetadata["exemptionCode"],
    sourceBankTransactionId: num("sourceBankTransactionId"),
    internalVoucherKind: str("internalVoucherKind") as DocumentMetadata["internalVoucherKind"],
    legacyOpeningJournalEntryId: num("legacyOpeningJournalEntryId"),
    legacyOpeningJournalLineId: num("legacyOpeningJournalLineId"),
    accountingRationale: str("accountingRationale"),
    externalAccountingEvidence,
  };
}

/**
 * POST /api/companies/:slug/documents/ingest — ingests a voucher/document.
 *
 * Body: `{ fileName: string, fileBase64: string, metadata: {...},
 * vendorId?: number, force?: boolean, confirm: true }`. The document file is
 * binary (a PDF/PNG/JPEG, or a plain-text receipt), so the frontend base64-
 * encodes it; the handler decodes it to a `mkdtemp` file — keeping the
 * original file extension, which `core/documents` keys its MIME detection on
 * — and calls the SAME `ingestDocument` (+ `resolveDocumentMasterData`) core
 * functions the CLI/MCP use. No multipart.
 *
 * Destructive (it hash-stores the bilag and may post) so `requireConfirm` is
 * set. A `maxBodyBytes` cap hardens the upload route. Goes through
 * `withCompanyMutation` — backup lock, localhost gate, actor attribution.
 */
export async function handleDocumentIngest(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    async (ctx, body) => {
      const fileName = requireBodyString(body, "fileName");
      const fileBase64 = requireBodyString(body, "fileBase64");
      const metadata = parseDocumentMetadata(body.metadata);
      const vendorId = optionalBodyPositiveInt(body, "vendorId");
      if (body.force !== undefined && typeof body.force !== "boolean") {
        throw ApiError.badRequest("'force' must be a boolean when present");
      }
      const force = body.force === true;

      // Decode the inline file to a private temp file. The extension is
      // preserved from the client filename because `core/documents`
      // resolves the MIME type from it (and cross-checks the magic bytes).
      let bytes: Buffer;
      try {
        bytes = Buffer.from(fileBase64, "base64");
      } catch {
        throw ApiError.badRequest("'fileBase64' must be valid base64");
      }
      if (bytes.length === 0) {
        throw ApiError.badRequest("'fileBase64' decodes to an empty file");
      }
      const ext = extname(fileName).toLowerCase();
      // The temp dir is a transient staging area: ingestDocument copies the
      // bytes into the company's permanent originals store, so the temp copy
      // must be removed on EVERY exit path (success, duplicate rejection or
      // throw). Without the finally each cockpit ingest leaked a temp dir
      // forever (matches the MCP/import-export #383 cleanup).
      const tmpDir = mkdtempSync(join(tmpdir(), "rentemester-cockpit-doc-"));
      try {
        const filePath = join(tmpDir, `document${ext}`);
        writeFileSync(filePath, bytes);

        // Master-data resolution mirrors the CLI/MCP: a given `vendorId`
        // back-fills the sender from the registered vendor.
        const resolved = resolveDocumentMasterData(ctx.db, metadata, { vendorId });
        if (!resolved.ok) {
          return { ok: false, errors: resolved.errors ?? ["master-data resolution failed"] };
        }
        const ingested = await ingestDocumentAsync(ctx.db, ctx.companyRoot, filePath, resolved.metadata, {
          forceDuplicateLogicalIdentity: force,
          createdBy: ctx.actor.createdBy,
          createdByProgram: ctx.actor.createdByProgram,
          // Hosted composition injects this only after validating the
          // deployment scanner contract. Local CLI/cockpit remains explicitly
          // scanner-off; a required policy with no runtime provider fails
          // closed in the core rather than accepting an unscanned upload.
          scannerPolicy: config.documentScannerPolicy ?? "off",
          scanner: config.documentScanner,
          scannerTimeoutMs: config.hostedDocumentScanning?.provider?.timeoutMs,
        });
        const extraction = ingested.ok && ingested.documentId && config.invoiceExtractor
          ? await extractDocumentInvoice(ctx.db, ctx.companyRoot, ingested.documentId, config.invoiceExtractor, ctx.actor.createdBy)
          : undefined;
        return {
          ok: ingested.ok,
          errors: ingested.errors,
          documentId: ingested.documentId,
          documentNo: ingested.documentNo,
          ...(extraction ? { extraction: invoiceExtractionSurface(ctx.db, ingested.documentId!) } : {}),
        };
      } finally {
        removePathWithRetry(tmpDir);
      }
    },
    { requireConfirm: true, maxBodyBytes: MAX_UPLOAD_BODY_BYTES },
  );

  return okResponse({
    document: {
      id: result.ok ? result.documentId ?? null : null,
      documentNo: result.ok ? result.documentNo ?? null : null,
    },
  });
}

/**
 * POST /api/companies/:slug/documents/book-expense — books an ingested
 * purchase document (bilag) against an unmatched outgoing bank transaction
 * (#407). The Cockpit becomes a third caller of `bookExpenseFromBank`,
 * alongside the CLI's `expense book` command and the MCP tool.
 *
 * Body: `{ documentId: number, bankTransactionId: number,
 * expenseAccountNo: string, vatTreatment?: 'standard'|'reverse_charge'|
 * 'representation'|'exempt'|'non_deductible', paymentAccountNo?: string,
 * transactionDate?: string, text?: string, confirm: true }`.
 *
 * Write-irreversible (it appends a journal entry that links both the
 * document and the bank transaction) so `requireConfirm` is set. The same
 * conflict heuristic in `withCompanyMutation` maps core's "already linked"
 * rejection (a double-book attempt) to a 409.
 */
export async function handleDocumentBookExpense(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const documentId = requireBodyPositiveInt(body, "documentId");
      const bankTransactionId = requireBodyPositiveInt(body, "bankTransactionId");
      const expenseAccountNo = requireBodyString(body, "expenseAccountNo");
      const vatTreatmentRaw = optionalBodyString(body, "vatTreatment");
      let vatTreatment: ExpenseVatTreatment | undefined;
      if (vatTreatmentRaw !== undefined) {
        if (
          !["standard", "reverse_charge", "representation", "exempt", "non_deductible"].includes(
            vatTreatmentRaw,
          )
        ) {
          throw ApiError.badRequest(
            "'vatTreatment' must be one of standard, reverse_charge, representation, exempt, non_deductible",
          );
        }
        vatTreatment = vatTreatmentRaw as ExpenseVatTreatment;
      }
      const paymentAccountNo = optionalBodyString(body, "paymentAccountNo");
      const transactionDate = optionalBodyString(body, "transactionDate");
      const text = optionalBodyString(body, "text");
      const booked = bookExpenseFromBank(
        ctx.db,
        withCockpitActor(
          {
            documentId,
            bankTransactionId,
            expenseAccountNo,
            ...(vatTreatment ? { vatTreatment } : {}),
            ...(paymentAccountNo ? { paymentAccountNo } : {}),
            ...(transactionDate ? { transactionDate } : {}),
            ...(text ? { text } : {}),
          },
          ctx.actor,
        ),
      );
      return {
        ok: booked.ok,
        errors: booked.errors,
        entryId: booked.entryId,
        documentId: booked.documentId,
        bankTransactionId: booked.bankTransactionId,
        grossAmount: booked.grossAmount,
        netAmount: booked.netAmount,
        vatAmount: booked.vatAmount,
        vatTreatment: booked.vatTreatment,
        grossAmountForeign: booked.grossAmountForeign,
        grossAmountDkk: booked.grossAmountDkk,
        netAmountDkk: booked.netAmountDkk,
        vatAmountDkk: booked.vatAmountDkk,
        fxRateToDkk: booked.fxRateToDkk,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    booking: {
      entryId: result.entryId ?? null,
      documentId: result.documentId ?? null,
      bankTransactionId: result.bankTransactionId ?? null,
      grossAmount: result.grossAmount ?? null,
      netAmount: result.netAmount ?? null,
      vatAmount: result.vatAmount ?? null,
      vatTreatment: result.vatTreatment ?? null,
      grossAmountForeign: result.grossAmountForeign ?? null,
      grossAmountDkk: result.grossAmountDkk ?? null,
      netAmountDkk: result.netAmountDkk ?? null,
      vatAmountDkk: result.vatAmountDkk ?? null,
      fxRateToDkk: result.fxRateToDkk ?? null,
    },
  });
}

/** POST /documents/:id/vat-preflight/apply — actor-attributed provider call. */
export async function handleDocumentVatPreflightApply(config: ServerConfig, request: Request, slug: string, idRaw: string): Promise<Response> {
  const documentId = Number(idRaw);
  if (!Number.isInteger(documentId) || documentId <= 0) throw ApiError.badRequest("document id must be a positive integer");
  const result = await withCompanyMutation(request, config, slug, async (ctx) => {
    const preflight = await applyPurchaseVatPreflight(ctx.db, documentId, ctx.actor.createdBy);
    return { ok: preflight.ok, errors: preflight.errors, preflight };
  }, { requireConfirm: true });
  return okResponse({ preflight: result.preflight });
}
