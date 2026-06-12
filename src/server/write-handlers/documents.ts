// Document ingest + expense-from-bank booking handlers (#213 slice 3, #407).

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { extname, join } from "node:path";
import { ingestDocument, type DocumentMetadata } from "../../core/documents";
import { resolveDocumentMasterData } from "../../core/master-data";
import {
  bookExpenseFromBank,
  type ExpenseVatTreatment,
} from "../../core/expense-booking";
import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { withCockpitActor } from "../actor";
import { withCompanyMutation } from "../mutations";
import {
  MAX_UPLOAD_BODY_BYTES,
  okResponse,
  optionalBodyPositiveInt,
  optionalBodyString,
  requireBodyPositiveInt,
  requireBodyString,
} from "./_shared";

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
  function party(key: string): DocumentMetadata["sender"] {
    const v = m[key];
    if (v === undefined || v === null) return undefined;
    if (typeof v !== "object" || Array.isArray(v)) {
      throw ApiError.badRequest(`metadata.${key} must be an object when present`);
    }
    const p = v as Record<string, unknown>;
    for (const f of ["name", "address", "vatOrCvr"]) {
      if (p[f] !== undefined && p[f] !== null && typeof p[f] !== "string") {
        throw ApiError.badRequest(`metadata.${key}.${f} must be a string when present`);
      }
    }
    const trim = (x: unknown) =>
      typeof x === "string" && x.trim().length > 0 ? x.trim() : undefined;
    return { name: trim(p.name), address: trim(p.address), vatOrCvr: trim(p.vatOrCvr) };
  }

  const source = str("source");
  if (!source) {
    throw ApiError.badRequest("metadata.source is required");
  }
  const documentType = m.documentType;
  if (
    documentType !== undefined &&
    documentType !== "purchase_sale" &&
    documentType !== "cash_register_receipt"
  ) {
    throw ApiError.badRequest(
      "metadata.documentType must be 'purchase_sale' or 'cash_register_receipt'",
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
    paymentDetails: str("paymentDetails"),
    exemptionCode: (exemptionCode ?? undefined) as DocumentMetadata["exemptionCode"],
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
    (ctx, body) => {
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
        const ingested = ingestDocument(ctx.db, ctx.companyRoot, filePath, resolved.metadata, {
          forceDuplicateLogicalIdentity: force,
        });
        return {
          ok: ingested.ok,
          errors: ingested.errors,
          documentId: ingested.documentId,
          documentNo: ingested.documentNo,
        };
      } finally {
        rmSync(tmpDir, { recursive: true, force: true });
      }
    },
    { requireConfirm: true, maxBodyBytes: MAX_UPLOAD_BODY_BYTES },
  );

  return okResponse({
    document: {
      id: result.documentId ?? null,
      documentNo: result.documentNo ?? null,
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
 * 'representation'|'exempt', paymentAccountNo?: string, transactionDate?:
 * string, text?: string, confirm: true }`.
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
          !["standard", "reverse_charge", "representation", "exempt"].includes(
            vatTreatmentRaw,
          )
        ) {
          throw ApiError.badRequest(
            "'vatTreatment' must be one of standard, reverse_charge, representation, exempt",
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
    },
  });
}
