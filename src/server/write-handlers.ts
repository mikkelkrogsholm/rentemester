// Cockpit write route handlers (#213, slice 1).
//
// Each handler here is a thin adapter: it parses route params + body, runs the
// shared `withCompanyMutation` pipeline (which owns the backup lock, the
// confirm gate, actor resolution and the localhost hard-gate), and calls the
// existing `src/core/` bookkeeping function. The Cockpit NEVER reimplements
// bookkeeping — it is a third caller of core, alongside the CLI and MCP.
//
// Slice 1 ships the resolve-exception action. Slices 2-3 add bank CSV import
// and document (bilag) intake — both file-upload routes: the frontend reads
// the file in the browser and POSTs its content inline (CSV as text, the
// binary document as base64), and the handler writes it to a `mkdtemp` file
// before calling core. Slice 4 (invoicing) follows the same shape.

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { extname, join } from "node:path";
import {
  resolveException,
  syncUnmatchedBankTransactionExceptions,
} from "../core/exceptions";
import { importBankCsv } from "../core/bank";
import { importDineroContacts } from "../core/import/dinero-contacts";
import {
  createCustomer,
  createVendor,
  updateCustomer,
  updateVendor,
  type CreateCustomerInput,
  type CreateVendorInput,
  type UpdateCustomerInput,
  type UpdateVendorInput,
} from "../core/master-data";
import { lookupCvrCompany } from "../core/cvr";
import { detectImportSource } from "../core/import/source-detect";
import { exportAuthorityPackage } from "../core/authority-export";
import { createTar, dirToTarEntries } from "../core/tar";
import { generateRecurringInvoice } from "../core/recurring-invoices";
import { ingestDocument, type DocumentMetadata } from "../core/documents";
import { resolveDocumentMasterData, resolveInvoiceMasterData } from "../core/master-data";
import {
  bookExpenseFromBank,
  type ExpenseVatTreatment,
} from "../core/expense-booking";
import {
  computeInvoiceAmounts,
  type InvoiceLineInput,
  type InvoicePayload,
} from "../core/invoice";
import { issueInvoice } from "../core/issued-invoices";
import { postIssuedInvoiceToLedger } from "../core/invoice-booking";
import { settleInvoiceFromBank } from "../core/invoice-settlement";
import { issueCreditNote } from "../core/credit-notes";
import {
  submitPublicEInvoicePeppol,
  type PeppolAccessPointConfig,
} from "../core/public-einvoice";
import { readFileSync } from "node:fs";
import {
  getCompanySettings,
  resolveCompanyPaymentDetails,
  setCompanyProfile,
  type CompanyPaymentInput,
} from "../core/company";
import {
  closeAccountingPeriod,
  reopenAccountingPeriod,
  setCompanyVatPeriodType,
  normalizeVatPeriodType,
} from "../core/periods";
import type { ServerConfig } from "./config";
import { ApiError } from "./errors";
import { withCockpitActor } from "./actor";
import { withCompanyMutation } from "./mutations";

/**
 * Max request-body size for the file-upload routes (#213, slices 2-3). A bank
 * CSV or a base64-encoded document is far larger than slice 1's tiny JSON
 * body, but still bounded — 12 MiB comfortably covers a multi-year CSV export
 * or a scanned multi-page PDF (base64 inflates bytes by ~33%) while refusing a
 * body that would exhaust memory. The guard runs in `withCompanyMutation`
 * before the body is read.
 */
const MAX_UPLOAD_BODY_BYTES = 12 * 1024 * 1024;

const JSON_HEADERS = { "content-type": "application/json; charset=utf-8" };

function okResponse(body: Record<string, unknown>, status = 200): Response {
  return new Response(JSON.stringify({ ok: true, ...body }), {
    status,
    headers: JSON_HEADERS,
  });
}

/** Parses a positive-integer path segment, mapping a bad value to a 400. */
function parseIdParam(raw: string, label: string): number {
  const value = Number(raw);
  if (!Number.isInteger(value) || value <= 0) {
    throw ApiError.badRequest(`'${label}' must be a positive integer`);
  }
  return value;
}

/** Reads an optional string body field, trimming and collapsing empty to undefined. */
function optionalBodyString(
  body: Record<string, unknown>,
  key: string,
): string | undefined {
  const value = body[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string") {
    throw ApiError.badRequest(`'${key}' must be a string when present`);
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

/**
 * POST /api/companies/:slug/exceptions/:id/resolve — clears an open exception.
 *
 * Body: `{ note?: string }`. Non-destructive (the exception stays in the
 * ledger, only its status flips to `resolved`), so no `confirm` is required —
 * the Cockpit modal is the human's consent.
 *
 * Goes through `withCompanyMutation`, so the backup lock, the localhost gate
 * and actor attribution all apply. The resolved actor is recorded as the
 * exception's `resolvedBy`, so the audit trail shows the Cockpit cleared it.
 */
export async function handleResolveException(
  config: ServerConfig,
  request: Request,
  slug: string,
  idRaw: string,
): Promise<Response> {
  const id = parseIdParam(idRaw, "id");

  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const note = optionalBodyString(body, "note");
      // The actor flows through as `resolvedBy` (an explicit payload param —
      // never an env var), so a Cockpit-cleared exception is attributable.
      const payload = withCockpitActor(
        { id, note: note ?? null, resolvedBy: ctx.actor.createdBy },
        ctx.actor,
      );
      return resolveException(ctx.db, payload);
    },
  );

  return okResponse({
    exception: { id, resolved: result.resolved },
  });
}

/** Reads a required, non-empty string body field, mapping a bad value to a 400. */
function requireBodyString(
  body: Record<string, unknown>,
  key: string,
): string {
  const value = body[key];
  if (typeof value !== "string" || value.trim().length === 0) {
    throw ApiError.badRequest(`'${key}' is required and must be a non-empty string`);
  }
  return value;
}

/** Reads an optional positive-integer body field, mapping a bad value to a 400. */
function optionalBodyPositiveInt(
  body: Record<string, unknown>,
  key: string,
): number | undefined {
  const value = body[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "number" || !Number.isInteger(value) || value <= 0) {
    throw ApiError.badRequest(`'${key}' must be a positive integer when present`);
  }
  return value;
}

/**
 * POST /api/companies/:slug/bank/import — imports a bank-statement CSV.
 *
 * Body: `{ csvContent: string, account?: string, profile?: string,
 * confirm: true }`. The frontend reads the chosen CSV file in the browser and
 * POSTs its text as `csvContent`; the handler writes it to a `mkdtemp` file
 * and calls the SAME `importBankCsv` core function the CLI/MCP use, then runs
 * `syncUnmatchedBankTransactionExceptions` exactly as `bank import` does.
 *
 * Destructive (it appends ledger rows) so `requireConfirm` is set — the body
 * must carry `confirm: true`. A `maxBodyBytes` cap hardens the upload route.
 * Goes through `withCompanyMutation`, so the backup lock, the localhost gate
 * and actor attribution all apply.
 */
export async function handleBankImport(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const csvContent = requireBodyString(body, "csvContent");
      const account = optionalBodyString(body, "account");
      const profile = optionalBodyString(body, "profile");

      // Mirror the MCP `csvContent` pattern: persist the inline CSV to a
      // private temp file, then hand core a path — core reads from disk.
      const tmpDir = mkdtempSync(join(tmpdir(), "rentemester-cockpit-bank-"));
      const csvPath = join(tmpDir, "bank-import.csv");
      writeFileSync(csvPath, csvContent, "utf8");

      const imported = importBankCsv(ctx.db, ctx.companyRoot, csvPath, {
        account,
        profile,
      });
      // The CLI/MCP both sync unmatched-transaction exceptions after a
      // successful import — replicate that so the Cockpit behaves identically.
      const sync = imported.ok
        ? syncUnmatchedBankTransactionExceptions(ctx.db)
        : { ok: true, created: 0, errors: [] };
      return {
        ...(imported as Record<string, unknown>),
        ok: imported.ok,
        errors: imported.errors,
        exceptionsCreated: sync.created,
      };
    },
    { requireConfirm: true, maxBodyBytes: MAX_UPLOAD_BODY_BYTES },
  );

  // The core `BankImportResult` shape is echoed back so the UI can report the
  // batch id, the imported/skipped counts and any balance warnings.
  return okResponse({
    import: {
      importBatchId: result.importBatchId,
      imported: result.imported ?? 0,
      skippedDuplicates: result.skippedDuplicates ?? 0,
      skippedDuplicateRows: result.skippedDuplicateRows ?? [],
      bankAccountSlug: result.bankAccountSlug,
      profile: result.profile,
      balanceWarnings: result.balanceWarnings ?? [],
      exceptionsCreated: result.exceptionsCreated ?? 0,
    },
  });
}

/**
 * POST /api/companies/:slug/import — the cockpit's generic file-import.
 *
 * Body: `{ fileName: string, content: string, enrichCvr?: boolean,
 * confirm: true }`. The browser reads the chosen export file and POSTs its
 * text; the handler recognises WHICH system the file came from
 * (`detectImportSource`) and routes it to the matching core importer. Today
 * one source is recognised — a Dinero "Kontakter" CSV, landed in the
 * customer/vendor master data via the same `importDineroContacts` core the
 * CLI's `import contacts` uses.
 *
 * A write (it appends master-data rows) so `requireConfirm` is set; the upload
 * route is capped by `maxBodyBytes`. Goes through `withCompanyMutation`, so the
 * backup lock, the localhost gate and actor attribution all apply. A file that
 * matches no known format is a 400 with the supported-formats list.
 */
export async function handleDataImport(
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
      const content = requireBodyString(body, "content");
      const enrichCvr = body.enrichCvr === true;

      const detection = detectImportSource(fileName, content);
      if (!detection.ok) {
        throw ApiError.badRequest(detection.errors.join(" "));
      }
      const source = detection.module;

      if (source.dataType === "contacts") {
        const imported = await importDineroContacts(ctx.db, content, {
          enrichCvr,
        });
        return {
          ok: imported.ok,
          errors: imported.errors,
          detected: {
            id: source.id,
            label: source.label,
            system: source.system,
            dataType: source.dataType,
          },
          summary: imported.summary,
        };
      }

      // Unreachable today — every registered module's dataType is "contacts".
      throw ApiError.badRequest(
        `Datatypen '${source.dataType}' understøttes ikke endnu.`,
      );
    },
    { requireConfirm: true, maxBodyBytes: MAX_UPLOAD_BODY_BYTES },
  );

  return okResponse({
    import: {
      detected: result.detected,
      summary: result.summary,
      errors: result.errors,
    },
  });
}

/**
 * POST /api/companies/:slug/accountant-export — the "share with revisor"
 * action.
 *
 * Body: `{ periodStart: string, periodEnd: string, confirm: true }`. Generates
 * the same `accountant_handoff` package the CLI's `system export-accountant`
 * produces (a manifest plus the machine-readable + documents-readable
 * subtrees), packs the whole thing into one deterministic .tar, and returns
 * the archive as a single download. The temp output dir is removed on the way
 * out — the response is the only copy that leaves the workspace.
 *
 * Goes through `withCompanyMutation` (backup lock, localhost gate, actor
 * attribution); `requireConfirm` is set because the export writes an audit
 * event into the ledger.
 */
export async function handleAccountantExport(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const periodStart = requireBodyString(body, "periodStart");
      const periodEnd = requireBodyString(body, "periodEnd");

      const outputDir = mkdtempSync(
        join(tmpdir(), "rentemester-cockpit-accountant-"),
      );
      try {
        const exported = exportAuthorityPackage(ctx.db, ctx.companyRoot, {
          periodStart,
          periodEnd,
          outputDir,
          packageProfile: "accountant_handoff",
        });
        if (!exported.ok || !exported.exportDir) {
          throw ApiError.badRequest(
            (exported.errors ?? []).join("; ") ||
              "accountant export failed",
          );
        }
        // The flat directory the export wrote — packing this (not the parent
        // temp dir) keeps the tar coherent: untarring it yields a single
        // package folder, not a wrapper.
        const entries = dirToTarEntries(exported.exportDir);
        const tar = createTar(entries);
        return {
          ok: true,
          errors: [] as string[],
          tar,
          filename: `revisor-eksport-${slug}-${periodStart}-${periodEnd}.tar`,
          journalEntryCount: exported.journalEntryCount ?? 0,
          documentCount: exported.documentCount ?? 0,
          bankTransactionCount: exported.bankTransactionCount ?? 0,
        };
      } finally {
        try {
          rmSync(outputDir, { recursive: true, force: true });
        } catch {}
      }
    },
    { requireConfirm: true },
  );

  return new Response(result.tar, {
    headers: {
      "content-type": "application/x-tar",
      "content-disposition": `attachment; filename*=UTF-8''${encodeURIComponent(result.filename)}`,
      "x-content-type-options": "nosniff",
      "cache-control": "private, no-store",
      // Summary counters carried as headers so the UI can show a receipt
      // alongside the download without a second round-trip.
      "x-rentemester-journal-entries": String(result.journalEntryCount),
      "x-rentemester-documents": String(result.documentCount),
      "x-rentemester-bank-transactions": String(result.bankTransactionCount),
    },
  });
}

/**
 * POST /api/companies/:slug/recurring-invoices/:id/generate — materializes the
 * next invoice from a recurring template. Body: `{ asOfDate, confirm: true }`.
 *
 * Wraps the same `generateRecurringInvoice` core the CLI and MCP use.
 * Idempotent: a second call for the same period returns the existing
 * generation with `created: false`. Goes through `withCompanyMutation`, so the
 * backup lock + actor attribution apply; `requireConfirm` is set because the
 * action issues a real invoice document into the ledger.
 */
export async function handleGenerateRecurringInvoice(
  config: ServerConfig,
  request: Request,
  slug: string,
  templateIdRaw: string,
): Promise<Response> {
  const templateId = Number(templateIdRaw);
  if (!Number.isInteger(templateId) || templateId <= 0) {
    throw ApiError.badRequest("template id must be a positive integer");
  }
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const asOfDate = requireBodyString(body, "asOfDate");
      const gen = generateRecurringInvoice(ctx.db, ctx.companyRoot, {
        templateId,
        asOfDate,
        createdBy: ctx.actor.createdBy,
        createdByProgram: ctx.actor.createdByProgram,
      });
      return {
        ok: gen.ok,
        errors: gen.errors,
        generation: {
          created: gen.created ?? false,
          templateId: gen.templateId ?? null,
          periodIndex: gen.periodIndex ?? null,
          documentId: gen.documentId ?? null,
          invoiceNumber: gen.invoiceNumber ?? null,
          issueDate: gen.issueDate ?? null,
          dueDate: gen.dueDate ?? null,
          deliveryPeriodStart: gen.deliveryPeriodStart ?? null,
          deliveryPeriodEnd: gen.deliveryPeriodEnd ?? null,
        },
      };
    },
    { requireConfirm: true },
  );

  return okResponse({ generation: result.generation });
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
      const tmpDir = mkdtempSync(join(tmpdir(), "rentemester-cockpit-doc-"));
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

// --------------------------------------------------------------------------
// Slice 4 — invoicing.
//
// Three human-mode invoice actions, each routed through `withCompanyMutation`
// and each a third caller of the SAME `src/core/` functions the CLI
// (`src/cli/invoice.ts`) and MCP (`src/mcp/tools/invoice.ts`) use:
//
//   - issue   — the human enters customer + line items; Rentemester COMPUTES
//               every total via `computeInvoiceAmounts`, exactly as the CLI's
//               guided `invoice create` command does. The human never does
//               invoice arithmetic. Issuing is non-destructive at the ledger
//               level (no journal entry yet — a kladde), so no `requireConfirm`.
//   - post    — `postIssuedInvoiceToLedger`. Write-irreversible (it appends a
//               journal entry), so `requireConfirm: true`.
//   - settle  — `settleInvoiceFromBank`. Write-irreversible (it links a bank
//               receipt + appends a journal entry), so `requireConfirm: true`.
// --------------------------------------------------------------------------

/**
 * Reads a required positive-integer body field, mapping a bad value to a 400.
 * Used for the invoice document id on the post/settle routes.
 */
function requireBodyPositiveInt(
  body: Record<string, unknown>,
  key: string,
): number {
  const value = body[key];
  if (typeof value !== "number" || !Number.isInteger(value) || value <= 0) {
    throw ApiError.badRequest(`'${key}' is required and must be a positive integer`);
  }
  return value;
}

/** Reads an optional finite-number body field, mapping a bad value to a 400. */
function optionalBodyNumber(
  body: Record<string, unknown>,
  key: string,
): number | undefined {
  const value = body[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw ApiError.badRequest(`'${key}' must be a number when present`);
  }
  return value;
}

/**
 * Parses the `lines` body field into core `InvoiceLineInput[]`. Each line is
 * the three essentials a human supplies — description, quantity, unit price
 * ex-VAT. Anything malformed is a 400; `computeInvoiceAmounts` performs the
 * deeper numeric validation (quantity > 0, etc.) and is the single source of
 * truth for every derived amount.
 */
function parseInvoiceLines(raw: unknown): InvoiceLineInput[] {
  if (!Array.isArray(raw) || raw.length === 0) {
    throw ApiError.badRequest("'lines' is required and must be a non-empty array");
  }
  return raw.map((entry, index) => {
    if (!entry || typeof entry !== "object" || Array.isArray(entry)) {
      throw ApiError.badRequest(`lines[${index}] must be an object`);
    }
    const line = entry as Record<string, unknown>;
    if (typeof line.description !== "string" || line.description.trim().length === 0) {
      throw ApiError.badRequest(`lines[${index}].description is required and must be a non-empty string`);
    }
    if (typeof line.quantity !== "number" || !Number.isFinite(line.quantity)) {
      throw ApiError.badRequest(`lines[${index}].quantity is required and must be a number`);
    }
    if (typeof line.unitPriceExVat !== "number" || !Number.isFinite(line.unitPriceExVat)) {
      throw ApiError.badRequest(`lines[${index}].unitPriceExVat is required and must be a number`);
    }
    return {
      description: line.description.trim(),
      quantity: line.quantity,
      unitPriceExVat: line.unitPriceExVat,
    };
  });
}

/**
 * POST /api/companies/:slug/invoices/issue — issues a sales invoice.
 *
 * Body: `{ issueDate: string, lines: [{description, quantity, unitPriceExVat}],
 * vatRatePercent?: number, customerId?: number, buyer?: {name,address,vatOrCvr},
 * seller?: {name,address,vatOrCvr}, invoiceNumber?: string, dueDate?: string,
 * currency?: string }`.
 *
 * Rentemester COMPUTES every line total, the net amount, the VAT amount and
 * the gross amount from the human's minimal input — the exact compute path of
 * the CLI's guided `invoice create` command (`computeInvoiceAmounts` →
 * `issueInvoice`). The human never hand-writes an amount. A stored
 * `customerId` back-fills the buyer from master data, exactly as the CLI does.
 *
 * Issuing produces a kladde (no journal entry yet), so it is NOT marked
 * `requireConfirm` — the multi-line modal IS the human's consent. The route
 * still goes through `withCompanyMutation` for the backup lock, the localhost
 * gate and actor attribution.
 */
export async function handleInvoiceIssue(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const issueDate = requireBodyString(body, "issueDate");
      const lines = parseInvoiceLines(body.lines);
      const vatRatePercent = optionalBodyNumber(body, "vatRatePercent") ?? 25;
      const customerId = optionalBodyPositiveInt(body, "customerId");
      const invoiceNumber = optionalBodyString(body, "invoiceNumber");
      const dueDate = optionalBodyString(body, "dueDate");
      const currency = optionalBodyString(body, "currency");

      // The human types either a stored --customer-id or the buyer details
      // directly — same as `invoice create`. `parseInvoiceParty` keeps a
      // partially-filled party object so master-data / validation can fill or
      // reject it.
      const buyer = parseInvoiceParty(body.buyer, "buyer");
      const seller = parseInvoiceParty(body.seller, "seller");

      // Rentemester computes every derived amount — the human never does
      // invoice arithmetic. A compute rejection (blank line, bad quantity) is
      // a core `{ok:false}` and surfaces as a 400 via `withCompanyMutation`.
      const computed = computeInvoiceAmounts(lines, vatRatePercent);
      if (!computed.ok) {
        return { ok: false, errors: computed.errors };
      }

      const payload: InvoicePayload = {
        invoiceType: "full",
        vatTreatment: "standard",
        issueDate,
        ...(invoiceNumber ? { invoiceNumber } : {}),
        seller,
        buyer,
        lines: computed.lines,
        totals: {
          netAmount: computed.totals.netAmount,
          vatRate: computed.totals.vatRate,
          vatAmount: computed.totals.vatAmount,
          grossAmount: computed.totals.grossAmount,
        },
        currency: currency ?? "DKK",
        ...(dueDate ? { dueDate } : {}),
      };

      // Master-data resolution mirrors the CLI: a given `customerId` back-fills
      // the buyer name/address/VAT from the registered customer.
      const resolved = resolveInvoiceMasterData(ctx.db, payload, { customerId });
      if (!resolved.ok) {
        return { ok: false, errors: resolved.errors ?? ["master-data resolution failed"] };
      }
      const issued = issueInvoice(ctx.db, ctx.companyRoot, resolved.payload);
      return {
        ok: issued.ok,
        errors: issued.errors,
        documentId: issued.documentId,
        invoiceNumber: issued.invoiceNumber,
        computed,
      };
    },
  );

  // Echo the computed amounts so the modal can show the human exactly what
  // Rentemester worked out from their input.
  return okResponse({
    invoice: {
      documentId: result.documentId ?? null,
      invoiceNumber: result.invoiceNumber ?? null,
      netAmount: result.computed?.totals?.netAmount ?? 0,
      vatRate: result.computed?.totals?.vatRate ?? 0,
      vatAmount: result.computed?.totals?.vatAmount ?? 0,
      grossAmount: result.computed?.totals?.grossAmount ?? 0,
      lines: result.computed?.lines ?? [],
    },
  });
}

/**
 * Parses an optional invoice party (`buyer` / `seller`) body field into the
 * partial-object shape `InvoicePayload` expects. Each field is optional — the
 * core validator / master-data resolution is the authority on completeness.
 */
function parseInvoiceParty(
  raw: unknown,
  label: string,
): { name?: string; address?: string; vatOrCvr?: string } {
  if (raw === undefined || raw === null) return {};
  if (typeof raw !== "object" || Array.isArray(raw)) {
    throw ApiError.badRequest(`'${label}' must be an object when present`);
  }
  const p = raw as Record<string, unknown>;
  const party: { name?: string; address?: string; vatOrCvr?: string } = {};
  for (const field of ["name", "address", "vatOrCvr"] as const) {
    const v = p[field];
    if (v === undefined || v === null) continue;
    if (typeof v !== "string") {
      throw ApiError.badRequest(`'${label}.${field}' must be a string when present`);
    }
    const trimmed = v.trim();
    if (trimmed.length > 0) party[field] = trimmed;
  }
  return party;
}

/**
 * POST /api/companies/:slug/invoices/post — posts an issued invoice to the
 * ledger.
 *
 * Body: `{ invoiceDocumentId: number, transactionDate?: string,
 * confirm: true }`. Calls the SAME `postIssuedInvoiceToLedger` core function
 * the CLI's `invoice post` command uses.
 *
 * Write-irreversible — it appends a journal entry — so `requireConfirm` is
 * set. A double-post is refused by core (`invoice X already has journal entry
 * Y`), which `withCompanyMutation` maps to a 409 conflict.
 */
export async function handleInvoicePost(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const invoiceDocumentId = requireBodyPositiveInt(body, "invoiceDocumentId");
      const transactionDate = optionalBodyString(body, "transactionDate");
      const posted = postIssuedInvoiceToLedger(
        ctx.db,
        withCockpitActor(
          { invoiceDocumentId, transactionDate },
          ctx.actor,
        ),
      );
      return {
        ok: posted.ok,
        errors: posted.errors,
        entryId: posted.entryId,
        entryNo: posted.entryNo,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    posting: {
      entryId: result.entryId ?? null,
      entryNo: result.entryNo ?? null,
    },
  });
}

/**
 * POST /api/companies/:slug/invoices/settle — settles an issued invoice
 * against a bank payment.
 *
 * Body: `{ invoiceDocumentId: number, bankTransactionId?: number,
 * bankTransactionReference?: string, paymentDate?: string, amount?: number,
 * confirm: true }`. Calls the SAME `settleInvoiceFromBank` core function the
 * CLI's `invoice settle-bank` command uses.
 *
 * Write-irreversible — it links a bank receipt and appends a journal entry —
 * so `requireConfirm` is set. A double-settle is refused by core (`bank
 * transaction N is already linked …`), mapped to a 409 conflict.
 */
export async function handleInvoiceSettle(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const invoiceDocumentId = requireBodyPositiveInt(body, "invoiceDocumentId");
      const bankTransactionId = optionalBodyPositiveInt(body, "bankTransactionId");
      const bankTransactionReference = optionalBodyString(
        body,
        "bankTransactionReference",
      );
      const paymentDate = optionalBodyString(body, "paymentDate");
      const amount = optionalBodyNumber(body, "amount");
      if (bankTransactionId === undefined && bankTransactionReference === undefined) {
        throw ApiError.badRequest(
          "'bankTransactionId' or 'bankTransactionReference' is required",
        );
      }
      const settled = settleInvoiceFromBank(
        ctx.db,
        withCockpitActor(
          {
            invoiceDocumentId,
            bankTransactionId,
            bankTransactionReference,
            paymentDate,
            amount,
          },
          ctx.actor,
        ),
      );
      return {
        ok: settled.ok,
        errors: settled.errors,
        entryId: settled.entryId,
        paymentId: settled.paymentId,
        principalAmount: settled.principalAmount,
        claimAmount: settled.claimAmount,
        invoiceNumber: settled.invoiceNumber,
        openBalance: settled.openBalance,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    settlement: {
      entryId: result.entryId ?? null,
      paymentId: result.paymentId ?? null,
      principalAmount: result.principalAmount ?? 0,
      claimAmount: result.claimAmount ?? 0,
      invoiceNumber: result.invoiceNumber ?? null,
      openBalance: result.openBalance ?? null,
    },
  });
}

/**
 * POST /api/companies/:slug/invoices/credit-note — issues a credit note for an
 * already-issued sales invoice (#412).
 *
 * Body: `{ invoiceDocumentId: number, issueDate: string, reason: string,
 * grossAmount?: number, creditNoteNumber?: string, confirm: true }`. Calls the
 * SAME `issueCreditNote` core function the CLI's `invoice credit-note` command
 * uses, so the Cockpit and the terminal produce byte-identical credit notes
 * and identical journal reversals.
 *
 * Write-irreversible — it inserts a credit-note document AND appends a
 * reversal journal entry — so `requireConfirm` is set. Re-crediting an already
 * fully credited invoice is refused by core (`invoice X is already fully
 * credited`); the `already` heuristic in `withCompanyMutation` maps that to a
 * 409, and a missing source invoice (`invoice document N does not exist`) to a
 * 409 as well. A blank reason or non-positive amount is rejected before core
 * is reached and surfaces as a 400.
 */
export async function handleInvoiceCreditNote(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const invoiceDocumentId = requireBodyPositiveInt(body, "invoiceDocumentId");
      const issueDate = requireBodyString(body, "issueDate");
      const reason = requireBodyString(body, "reason");
      const grossAmount = optionalBodyNumber(body, "grossAmount");
      const creditNoteNumber = optionalBodyString(body, "creditNoteNumber");
      const credited = issueCreditNote(
        ctx.db,
        ctx.companyRoot,
        withCockpitActor(
          {
            originalInvoiceDocumentId: invoiceDocumentId,
            issueDate,
            reason,
            ...(grossAmount !== undefined ? { grossAmount } : {}),
            ...(creditNoteNumber ? { creditNoteNumber } : {}),
          },
          ctx.actor,
        ),
      );
      return {
        ok: credited.ok,
        errors: credited.errors,
        documentId: credited.documentId,
        creditNoteNumber: credited.creditNoteNumber,
        originalInvoiceNumber: credited.originalInvoiceNumber,
        journalEntryId: credited.journalEntryId,
        journalEntryNo: credited.journalEntryNo,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    creditNote: {
      documentId: result.documentId ?? null,
      creditNoteNumber: result.creditNoteNumber ?? null,
      originalInvoiceNumber: result.originalInvoiceNumber ?? null,
      journalEntryId: result.journalEntryId ?? null,
      journalEntryNo: result.journalEntryNo ?? null,
    },
  });
}

// --------------------------------------------------------------------------
// Send som e-faktura (NemHandel / PEPPOL) — #428.
//
// A SMB owner that invoices a public buyer is required by law to deliver the
// invoice as an e-faktura. Until now the only way to do so from Rentemester
// was the CLI command `invoice submit-public-peppol`, which most owners never
// discover. This handler is the Cockpit's third caller of the SAME
// `submitPublicEInvoicePeppol` core function the CLI/MCP use — so the
// Cockpit and the terminal produce byte-identical PEPPOL envelopes and
// identical `peppol_submissions` rows.
//
// Access-point CONFIG (non-secret: accessPointId + endpointUrl + sender
// endpointId) is read from a file referenced by the `RENTEMESTER_PEPPOL_ACCESS_POINT`
// env var, mirroring how `bun run cli invoice submit-public-peppol` consumes
// its `--access-point <file.json>`. Credentials never enter the request body
// nor the server config object. When the env var is not configured, the
// handler returns a 400 with a clear next-step message — never a 500.
// --------------------------------------------------------------------------

/**
 * Loads the non-secret PEPPOL access-point config from a JSON file at the
 * path in `RENTEMESTER_PEPPOL_ACCESS_POINT`. Returns `null` (not throws) when
 * the env var is missing — that case is mapped to a 400 with a clear
 * next-step so the SMB owner knows what to configure. A malformed file is a
 * 400 with the parse error verbatim.
 */
function loadConfiguredPeppolAccessPoint(): PeppolAccessPointConfig {
  const path = (process.env.RENTEMESTER_PEPPOL_ACCESS_POINT ?? "").trim();
  if (!path) {
    throw ApiError.badRequest(
      "PEPPOL er ikke konfigureret i denne installation. " +
        "Sæt RENTEMESTER_PEPPOL_ACCESS_POINT til stien for en JSON-fil med " +
        "{accessPointId, endpointUrl, senderEndpointId} for at sende e-fakturaer.",
    );
  }
  let raw: string;
  try {
    raw = readFileSync(path, "utf8");
  } catch (error) {
    throw ApiError.badRequest(
      `PEPPOL access-point-config kunne ikke læses fra ${path}: ${(error as Error).message}`,
    );
  }
  let parsed: {
    accessPointId?: string;
    endpointUrl?: string;
    senderEndpointId?: string;
  };
  try {
    parsed = JSON.parse(raw) as typeof parsed;
  } catch (error) {
    throw ApiError.badRequest(
      `PEPPOL access-point-config er ikke gyldig JSON: ${(error as Error).message}`,
    );
  }
  return {
    accessPointId: (parsed.accessPointId ?? "").trim(),
    endpointUrl: (parsed.endpointUrl ?? "").trim(),
    senderEndpointId: (parsed.senderEndpointId ?? "").trim(),
  };
}

/**
 * POST /api/companies/:slug/invoices/send-public — sends an issued invoice
 * as a public e-faktura via NemHandel / PEPPOL.
 *
 * Body: `{ invoiceDocumentId: number, confirm: true }`. Calls the SAME
 * `submitPublicEInvoicePeppol` core function the CLI's
 * `invoice submit-public-peppol` command uses, so the Cockpit and the
 * terminal produce byte-identical PEPPOL submission envelopes. Idempotent:
 * a second submission for the same invoice/access-point pair collapses onto
 * the existing `peppol_submissions` row (the underlying core enforces this
 * via a derived idempotency key) and the handler echoes `duplicate: true`.
 *
 * Write-irreversible (it inserts a `peppol_submissions` row AND appends an
 * `audit_log` entry — both write-once tables) so `requireConfirm` is set.
 * Goes through `withCompanyMutation`, so the backup lock, the localhost gate
 * and actor attribution all apply.
 *
 * The access-point CONFIG (non-secret: accessPointId + endpointUrl + sender
 * endpointId) is loaded from `RENTEMESTER_PEPPOL_ACCESS_POINT`; credentials
 * are NEVER passed in the request body. A missing/invalid config is a 400.
 */
export async function handleInvoiceSendPublic(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const accessPoint = loadConfiguredPeppolAccessPoint();

  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      // Touch the resolved actor so the cockpit's submit is attributable in
      // the audit_log entry the core writes (the core itself records the
      // submission as the authenticated actor that opened the db).
      void ctx.actor;
      const invoiceDocumentId = requireBodyPositiveInt(body, "invoiceDocumentId");
      const submitted = submitPublicEInvoicePeppol(ctx.db, {
        invoiceDocumentId,
        accessPoint,
      });
      return {
        ok: submitted.ok,
        errors: submitted.errors,
        invoiceNumber: submitted.invoiceNumber,
        submissionReference: submitted.submissionReference,
        status: submitted.status,
        duplicate: submitted.duplicate,
        envelopeSha256: submitted.envelopeSha256,
        oioublSha256: submitted.oioublSha256,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    submission: {
      invoiceNumber: result.invoiceNumber ?? null,
      submissionReference: result.submissionReference ?? null,
      status: result.status ?? null,
      duplicate: Boolean(result.duplicate),
      envelopeSha256: result.envelopeSha256 ?? null,
      oioublSha256: result.oioublSha256 ?? null,
    },
  });
}

// --------------------------------------------------------------------------
// Company profile + bank details (#284).
//
// Without this route a Cockpit owner can only rename / CVR-sync / archive a
// company — there is no way to record the company's own postal address,
// payment terms or bank account. An invoice then goes out with no payment
// instructions. This route is the third caller of the SAME `setCompanyProfile`
// core function the CLI's `company profile` command uses; the primary bank
// account it creates is the one every issued-invoice payment block reads from.
// --------------------------------------------------------------------------

/**
 * Parses the optional `payment` body field into a core `CompanyPaymentInput`.
 * Every sub-field is optional — `setCompanyProfile` only creates the primary
 * bank account when at least one carries real information.
 */
function parsePaymentInput(raw: unknown): CompanyPaymentInput | undefined {
  if (raw === undefined || raw === null) return undefined;
  if (typeof raw !== "object" || Array.isArray(raw)) {
    throw ApiError.badRequest("'payment' must be an object when present");
  }
  const p = raw as Record<string, unknown>;
  const payment: CompanyPaymentInput = {};
  for (const field of ["bankName", "registrationNo", "accountNo", "iban"] as const) {
    const v = p[field];
    if (v === undefined || v === null) continue;
    if (typeof v !== "string") {
      throw ApiError.badRequest(`'payment.${field}' must be a string when present`);
    }
    const trimmed = v.trim();
    if (trimmed.length > 0) payment[field] = trimmed;
  }
  return payment;
}

/**
 * PATCH /api/companies/:slug/company — updates the editable company profile:
 * the company's own address, CVR, default payment terms and bank/payment
 * details. Body: `{ name?, cvr?, address?, postalCode?, city?,
 * paymentTermsDays?, payment?: {bankName, registrationNo, accountNo, iban} }`.
 *
 * Calls the SAME `setCompanyProfile` core function the CLI uses; the primary
 * bank account it creates feeds every issued-invoice payment block. At least
 * one recognised field must be present, else it is a 400. Goes through
 * `withCompanyMutation` — backup lock, localhost gate, actor attribution. The
 * profile edit is non-destructive (it never touches a posted journal entry),
 * so no `confirm` is required.
 */
export async function handleCompanyProfile(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const name = optionalBodyString(body, "name");
      const cvr = optionalBodyString(body, "cvr");
      const address = optionalBodyString(body, "address");
      const postalCode = optionalBodyString(body, "postalCode");
      const city = optionalBodyString(body, "city");
      const payment = parsePaymentInput(body.payment);

      let paymentTermsDays: number | undefined;
      if (body.paymentTermsDays !== undefined && body.paymentTermsDays !== null) {
        if (
          typeof body.paymentTermsDays !== "number" ||
          !Number.isInteger(body.paymentTermsDays)
        ) {
          throw ApiError.badRequest(
            "'paymentTermsDays' must be an integer when present",
          );
        }
        paymentTermsDays = body.paymentTermsDays;
      }

      // #300: the VAT settlement cadence is editable from the cockpit. An
      // unknown value is a 400 — the column has a CHECK constraint, so a bad
      // string would otherwise fail opaquely.
      const vatPeriodTypeRaw = optionalBodyString(body, "vatPeriodType");
      let vatPeriodType: ReturnType<typeof normalizeVatPeriodType> | undefined;
      if (vatPeriodTypeRaw !== undefined) {
        vatPeriodType = normalizeVatPeriodType(vatPeriodTypeRaw);
        if (vatPeriodType === null) {
          throw ApiError.badRequest(
            "'vatPeriodType' must be 'month', 'quarter' or 'half-year' when present",
          );
        }
      }

      const hasPayment =
        payment !== undefined && Object.keys(payment).length > 0;
      if (
        name === undefined &&
        cvr === undefined &&
        address === undefined &&
        postalCode === undefined &&
        city === undefined &&
        paymentTermsDays === undefined &&
        vatPeriodType === undefined &&
        !hasPayment
      ) {
        throw ApiError.badRequest(
          "provide at least one profile field to update " +
            "(name, cvr, address, postalCode, city, paymentTermsDays, vatPeriodType, payment)",
        );
      }

      // #300: the VAT cadence lives on the company row but `setCompanyProfile`
      // does not own it — write it first via the periods-core helper so the
      // settings the response carries reflect the new cadence.
      if (vatPeriodType !== undefined && vatPeriodType !== null) {
        const vatResult = setCompanyVatPeriodType(ctx.db, vatPeriodType);
        if (!vatResult.ok) {
          throw ApiError.badRequest(vatResult.errors[0] ?? "could not set VAT period type");
        }
      }

      const hasProfileField =
        name !== undefined ||
        cvr !== undefined ||
        address !== undefined ||
        postalCode !== undefined ||
        city !== undefined ||
        paymentTermsDays !== undefined ||
        hasPayment;
      const updated = hasProfileField
        ? setCompanyProfile(ctx.db, {
            ...(name !== undefined ? { name } : {}),
            ...(cvr !== undefined ? { cvr } : {}),
            ...(address !== undefined ? { address } : {}),
            ...(postalCode !== undefined ? { postalCode } : {}),
            ...(city !== undefined ? { city } : {}),
            ...(paymentTermsDays !== undefined ? { paymentTermsDays } : {}),
            ...(hasPayment ? { payment } : {}),
          })
        : // Only the VAT cadence changed — re-read the settings so the response
          // shape stays identical to a full profile edit.
          {
            ok: true as const,
            settings: getCompanySettings(ctx.db),
            updatedFields: ["vatPeriodType"],
            errors: [] as string[],
          };
      if (updated.ok && vatPeriodType !== undefined && hasProfileField) {
        updated.updatedFields = [
          ...(updated.updatedFields ?? []),
          "vatPeriodType",
        ];
      }
      // Re-resolve the payment block so the response carries the same
      // `{ ...settings, payment }` shape `GET .../company` returns — the
      // Cockpit form mirrors the persisted state without a re-fetch.
      const paymentDetails = updated.ok
        ? resolveCompanyPaymentDetails(ctx.db, updated.settings?.currency) ?? null
        : null;
      return {
        ok: updated.ok,
        errors: updated.errors,
        settings: updated.settings,
        payment: paymentDetails,
        updatedFields: updated.updatedFields ?? [],
      };
    },
  );

  return okResponse({
    company: {
      ...(result.settings ?? {}),
      payment: result.payment ?? null,
      updatedFields: result.updatedFields ?? [],
    },
  });
}

// --------------------------------------------------------------------------
// Close an accounting period (#287).
//
// A momsangivelse (VAT return) requires a CLOSED `vat_quarter` period — so
// without this route the key recurring legal duty cannot be completed from the
// Cockpit at all. This route is the third caller of the SAME
// `closeAccountingPeriod` core function the CLI's `period close` command uses.
// --------------------------------------------------------------------------

/**
 * POST /api/companies/:slug/periods/close — closes an accounting period.
 *
 * Body: `{ periodStart: string, periodEnd: string, kind?: 'vat_quarter' |
 * 'fiscal_year' | 'custom', reference?: string, confirm: true }`. Calls the
 * SAME `closeAccountingPeriod` core function the CLI uses.
 *
 * Closing a period locks bookkeeping inside it — it changes ledger state — so
 * `requireConfirm` is set. Closing an already-closed period is refused by core
 * ("overlaps existing period"), which `withCompanyMutation` maps to a 409.
 */
export async function handleClosePeriod(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const periodStart = requireBodyString(body, "periodStart");
      const periodEnd = requireBodyString(body, "periodEnd");
      const reference = optionalBodyString(body, "reference");
      const kindRaw = body.kind;
      if (
        kindRaw !== undefined &&
        kindRaw !== "vat_quarter" &&
        kindRaw !== "fiscal_year" &&
        kindRaw !== "custom"
      ) {
        throw ApiError.badRequest(
          "'kind' must be 'vat_quarter', 'fiscal_year' or 'custom' when present",
        );
      }
      const closed = closeAccountingPeriod(ctx.db, {
        periodStart,
        periodEnd,
        ...(kindRaw ? { kind: kindRaw } : {}),
        ...(reference ? { reference } : {}),
        createdBy: ctx.actor.createdBy,
        createdByProgram: ctx.actor.createdByProgram,
      });
      return {
        ok: closed.ok,
        errors: closed.errors,
        periodId: closed.periodId,
        periodStart: closed.periodStart,
        periodEnd: closed.periodEnd,
        kind: closed.kind,
        status: closed.status,
        reference: closed.reference,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    period: {
      id: result.periodId ?? null,
      periodStart: result.periodStart ?? null,
      periodEnd: result.periodEnd ?? null,
      kind: result.kind ?? null,
      status: result.status ?? null,
      reference: result.reference ?? null,
    },
  });
}

// --------------------------------------------------------------------------
// Reopen an accounting period (#301).
//
// The cockpit could close a VAT period but had no way back — an owner who
// closed a period too early (e.g. before the period had even ended) was stuck
// unless they dropped to the CLI's `period reopen`. This route is the third
// caller of the SAME `reopenAccountingPeriod` core function the CLI uses: the
// reopen is a controlled, fully audit-logged, append-only fact — the immutable
// period row is never mutated.
// --------------------------------------------------------------------------

/**
 * POST /api/companies/:slug/periods/reopen — reopens a closed accounting
 * period.
 *
 * Body: `{ periodStart: string, periodEnd: string, kind?: 'vat_quarter' |
 * 'fiscal_year' | 'custom', reason: string, confirm: true }`. The mandatory
 * `reason` is recorded verbatim in the audit log. Calls the SAME
 * `reopenAccountingPeriod` core the CLI's `period reopen` uses, so a `reported`
 * period (already filed) is refused and an already-open period is a no-op —
 * both surface as a 409 via `withCompanyMutation`.
 */
export async function handleReopenPeriod(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const periodStart = requireBodyString(body, "periodStart");
      const periodEnd = requireBodyString(body, "periodEnd");
      const reason = requireBodyString(body, "reason");
      const kindRaw = body.kind;
      if (
        kindRaw !== undefined &&
        kindRaw !== "vat_quarter" &&
        kindRaw !== "fiscal_year" &&
        kindRaw !== "custom"
      ) {
        throw ApiError.badRequest(
          "'kind' must be 'vat_quarter', 'fiscal_year' or 'custom' when present",
        );
      }
      const reopened = reopenAccountingPeriod(ctx.db, {
        periodStart,
        periodEnd,
        ...(kindRaw ? { kind: kindRaw } : {}),
        reason,
        createdBy: ctx.actor.createdBy,
        createdByProgram: ctx.actor.createdByProgram,
      });
      return {
        ok: reopened.ok,
        errors: reopened.errors,
        periodId: reopened.periodId,
        periodStart: reopened.periodStart,
        periodEnd: reopened.periodEnd,
        kind: reopened.kind,
        effectiveStatus: reopened.effectiveStatus,
        reopenedBy: reopened.reopenedBy,
        reason: reopened.reason,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    period: {
      id: result.periodId ?? null,
      periodStart: result.periodStart ?? null,
      periodEnd: result.periodEnd ?? null,
      kind: result.kind ?? null,
      effectiveStatus: result.effectiveStatus ?? null,
      reopenedBy: result.reopenedBy ?? null,
      reason: result.reason ?? null,
    },
  });
}

// ---------------------------------------------------------------------------
// Contacts (Kontakter) — create/update from the Cockpit (#390).
//
// Until now the Kontakter page only offered Import + Administrér; the only
// path to a new customer/vendor was the CLI or a CSV import. These handlers
// give the Cockpit a first-class create/update path, reusing the same
// `createCustomer/createVendor/updateCustomer/updateVendor` core the CLI and
// MCP use. CVR lookup is a separate, read-only endpoint so the modal can
// prefill name/address/contact details before the human commits.
// ---------------------------------------------------------------------------

/** Reads an optional non-empty trimmed string field — `null` clears, missing keeps. */
function readNullableString(
  body: Record<string, unknown>,
  key: string,
): string | undefined {
  const value = body[key];
  if (value === undefined) return undefined;
  if (value === null) return undefined; // explicit null treated as missing (= no change)
  if (typeof value !== "string") {
    throw ApiError.badRequest(`'${key}' must be a string when present`);
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

function parseCreateCustomerBody(body: Record<string, unknown>): CreateCustomerInput {
  const name = requireBodyString(body, "name").trim();
  const input: CreateCustomerInput = { name };
  const address = readNullableString(body, "address");
  if (address !== undefined) input.address = address;
  const vatOrCvr = readNullableString(body, "vatOrCvr");
  if (vatOrCvr !== undefined) input.vatOrCvr = vatOrCvr;
  const email = readNullableString(body, "email");
  if (email !== undefined) input.email = email;
  const phone = readNullableString(body, "phone");
  if (phone !== undefined) input.phone = phone;
  const website = readNullableString(body, "website");
  if (website !== undefined) input.website = website;
  const eanNumber = readNullableString(body, "eanNumber");
  if (eanNumber !== undefined) input.eanNumber = eanNumber;
  const notes = readNullableString(body, "notes");
  if (notes !== undefined) input.notes = notes;
  const defaultCurrency = readNullableString(body, "defaultCurrency");
  if (defaultCurrency !== undefined) input.defaultCurrency = defaultCurrency;
  if (body.paymentTermsDays !== undefined && body.paymentTermsDays !== null) {
    const value = Number(body.paymentTermsDays);
    if (!Number.isInteger(value) || value <= 0) {
      throw ApiError.badRequest("'paymentTermsDays' must be a positive integer");
    }
    input.paymentTermsDays = value;
  }
  return input;
}

function parseCreateVendorBody(body: Record<string, unknown>): CreateVendorInput {
  const name = requireBodyString(body, "name").trim();
  const input: CreateVendorInput = { name };
  const address = readNullableString(body, "address");
  if (address !== undefined) input.address = address;
  const vatOrCvr = readNullableString(body, "vatOrCvr");
  if (vatOrCvr !== undefined) input.vatOrCvr = vatOrCvr;
  const email = readNullableString(body, "email");
  if (email !== undefined) input.email = email;
  const phone = readNullableString(body, "phone");
  if (phone !== undefined) input.phone = phone;
  const website = readNullableString(body, "website");
  if (website !== undefined) input.website = website;
  const defaultExpenseAccount = readNullableString(body, "defaultExpenseAccount");
  if (defaultExpenseAccount !== undefined) input.defaultExpenseAccount = defaultExpenseAccount;
  const defaultVatTreatment = readNullableString(body, "defaultVatTreatment");
  if (defaultVatTreatment !== undefined) input.defaultVatTreatment = defaultVatTreatment;
  const notes = readNullableString(body, "notes");
  if (notes !== undefined) input.notes = notes;
  return input;
}

/** POST /api/companies/:slug/customers — create a customer (#390). */
export async function handleCreateCustomer(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const input = parseCreateCustomerBody(body);
      const created = createCustomer(ctx.db, input);
      return {
        ok: created.ok,
        errors: created.errors,
        customerId: (created as { customerId?: number }).customerId ?? null,
      };
    },
  );
  return okResponse({ customer: { id: result.customerId } });
}

/** POST /api/companies/:slug/vendors — create a vendor (#390). */
export async function handleCreateVendor(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const input = parseCreateVendorBody(body);
      const created = createVendor(ctx.db, input);
      return {
        ok: created.ok,
        errors: created.errors,
        vendorId: (created as { vendorId?: number }).vendorId ?? null,
      };
    },
  );
  return okResponse({ vendor: { id: result.vendorId } });
}

/** PATCH /api/companies/:slug/customers/:id — update a customer (#390). */
export async function handleUpdateCustomer(
  config: ServerConfig,
  request: Request,
  slug: string,
  idRaw: string,
): Promise<Response> {
  const id = parseIdParam(idRaw, "id");
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      // For updates we accept the same fields but pass them as a partial — any
      // field absent in the body is left untouched in the row.
      const input: UpdateCustomerInput = {};
      const create = parseCreateCustomerBody({ name: "x", ...body });
      // parseCreateCustomerBody requires `name`; for updates name is optional.
      // We re-read it explicitly so a missing `name` does not blank the row.
      if (body.name !== undefined) input.name = create.name;
      for (const k of [
        "address",
        "vatOrCvr",
        "email",
        "phone",
        "website",
        "eanNumber",
        "notes",
        "defaultCurrency",
      ] as const) {
        if (body[k] !== undefined) {
          // null = clear, "" = clear, otherwise the trimmed string.
          if (body[k] === null) (input as Record<string, unknown>)[k] = null;
          else (input as Record<string, unknown>)[k] = (create as Record<string, unknown>)[k] ?? null;
        }
      }
      if (body.paymentTermsDays !== undefined) {
        input.paymentTermsDays = create.paymentTermsDays;
      }
      const updated = updateCustomer(ctx.db, id, input);
      return { ok: updated.ok, errors: updated.errors };
    },
  );
  return okResponse({ customer: { id, ok: result.ok } });
}

/** PATCH /api/companies/:slug/vendors/:id — update a vendor (#390). */
export async function handleUpdateVendor(
  config: ServerConfig,
  request: Request,
  slug: string,
  idRaw: string,
): Promise<Response> {
  const id = parseIdParam(idRaw, "id");
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const input: UpdateVendorInput = {};
      const create = parseCreateVendorBody({ name: "x", ...body });
      if (body.name !== undefined) input.name = create.name;
      for (const k of [
        "address",
        "vatOrCvr",
        "email",
        "phone",
        "website",
        "defaultExpenseAccount",
        "defaultVatTreatment",
        "notes",
      ] as const) {
        if (body[k] !== undefined) {
          if (body[k] === null) (input as Record<string, unknown>)[k] = null;
          else (input as Record<string, unknown>)[k] = (create as Record<string, unknown>)[k] ?? null;
        }
      }
      const updated = updateVendor(ctx.db, id, input);
      return { ok: updated.ok, errors: updated.errors };
    },
  );
  return okResponse({ vendor: { id, ok: result.ok } });
}

/**
 * GET /api/companies/:slug/cvr-lookup?cvr=12345678 — looks an 8-digit Danish CVR
 * number up in the CVR register (cached server-side, credentials never reach
 * the browser). Used by the Kontakter create/edit modal to prefill name +
 * address. A missing-credentials response degrades cleanly: `{ ok:false,
 * errors:[…] }` returned inside a 200 envelope so the UI can show a hint
 * without surfacing the call as an error.
 */
export async function handleCvrLookup(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  // Read-only — reuse the read-side resolution rather than withCompanyMutation,
  // since this is just an enrichment query (no audit row, no backup lock).
  const url = new URL(request.url);
  const cvr = url.searchParams.get("cvr");
  if (!cvr || cvr.trim().length === 0) {
    throw ApiError.badRequest("'cvr' query parameter is required");
  }
  // Resolve the company db so the CVR-cache table is scoped to this company.
  const { findWorkspaceCompany, companyRootForSlug } = await import(
    "../core/workspace"
  );
  if (!findWorkspaceCompany(config.workspaceRoot, slug)) {
    throw ApiError.notFound(`no company with slug '${slug}' in the workspace`);
  }
  const companyRoot = companyRootForSlug(config.workspaceRoot, slug);
  const { companyPaths } = await import("../core/paths");
  const dbPath = companyPaths(companyRoot).db;
  const { existsSync } = await import("node:fs");
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`company '${slug}' has no ledger`);
  }
  const { openDb, migrate } = await import("../core/db");
  const db = openDb(dbPath);
  try {
    migrate(db);
    const result = await lookupCvrCompany(db, cvr);
    return new Response(
      JSON.stringify({
        ok: true,
        cvr: {
          ok: result.ok,
          cached: result.cached,
          company: result.company ?? null,
          errors: result.errors ?? [],
        },
      }),
      { status: 200, headers: JSON_HEADERS },
    );
  } finally {
    db.close();
  }
}
