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
import {
  registerAsset,
  postDepreciationPeriod,
  postImmediateWriteOff,
} from "../core/assets";
import { addBankAccount, importBankCsv } from "../core/bank";
import { eraseGdprSubject } from "../core/gdpr";
import {
  deleteBilagsmailImapConfig,
  saveBilagsmailImapConfig,
  setCompanyMailAlias,
  type BilagsmailImapConfig,
} from "../core/bilagsmail";
import { importDineroContacts } from "../core/import/dinero-contacts";
import {
  createCustomer,
  createVendor,
  deleteCustomer,
  deleteVendor,
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
import {
  createRecurringInvoiceTemplate,
  generateRecurringInvoice,
  retireRecurringInvoiceTemplate,
  type RecurringInterval,
  type DeliveryPeriodMode,
} from "../core/recurring-invoices";
import { ingestDocument, type DocumentMetadata } from "../core/documents";
import { resolveDocumentMasterData, resolveInvoiceMasterData } from "../core/master-data";
import {
  bookExpenseFromBank,
  type ExpenseVatTreatment,
} from "../core/expense-booking";
import {
  registerPayable as corePayableRegister,
  payPayableFromBank as corePayablePayFromBank,
} from "../core/payables";
import {
  computeInvoiceAmounts,
  type InvoiceLineInput,
  type InvoicePayload,
} from "../core/invoice";
import { issueInvoice, previewIssuedInvoicePdf } from "../core/issued-invoices";
import { postIssuedInvoiceToLedger } from "../core/invoice-booking";
import { settleInvoiceFromBank } from "../core/invoice-settlement";
import {
  postInvoiceReminderToLedger,
  registerInvoiceReminder,
} from "../core/invoice-reminders";
import { issueCreditNote } from "../core/credit-notes";
import { setBudget } from "../core/budget";
import {
  submitPublicEInvoicePeppol,
  type PeppolAccessPointConfig,
} from "../core/public-einvoice";
import {
  createSmtpTransport,
  sendInvoiceEmail,
  type EmailKind,
  type SmtpConfig,
} from "../core/email";
import { companyPaths } from "../core/paths";
import { existsSync, readFileSync } from "node:fs";
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
import {
  createMileageEntry,
  type CreateMileageEntryInput,
} from "../core/mileage";
import { migrate, openDb } from "../core/db";
import {
  companyRootForSlug,
  findWorkspaceCompany,
} from "../core/workspace";
import { authMiddleware } from "./auth";

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

/**
 * Verifies that the target exception is an open `AGENT_*` row, mapping a
 * missing/closed/wrong-type row to a friendly Danish 409 instead of a generic
 * "exception ... does not exist". The agent-suggestions view only ever shows
 * open AGENT_* rows, so a non-match means the row was closed in another tab
 * (a race) — never a server bug worth a 500.
 */
function assertOpenAgentException(
  db: ReturnType<typeof openDb>,
  id: number,
): { type: string } {
  const row = db
    .query(
      `SELECT type, status FROM exceptions WHERE id = ? LIMIT 1`,
    )
    .get(id) as { type: string; status: string } | null;
  if (!row) {
    throw ApiError.conflict(
      `agent-forslag #${id} findes ikke længere`,
    );
  }
  if (row.status !== "open") {
    throw ApiError.conflict(
      `agent-forslag #${id} er allerede afgjort`,
    );
  }
  if (!row.type.startsWith("AGENT_")) {
    throw ApiError.badRequest(
      `undtagelse #${id} er ikke et agent-forslag (type ${row.type})`,
    );
  }
  return { type: row.type };
}

/**
 * POST /api/companies/:slug/agent-suggestions/:id/approve — owner accepts the
 * agent's suggestion (#346). The cockpit's modal IS the human's consent; the
 * backend records the decision by resolving the underlying exception with a
 * decision-flavored Danish note ("Godkendt af ejer …"). Approval here does NOT
 * post the suggested ledger entry: a separate, action-specific write route
 * (e.g. "Beregn afskrivning" on the Anlæg view, "payable pay" on the
 * Leverandørfaktura view) is the one that touches the ledger. The audit chain
 * is: the suggestion was raised by an agent actor and resolved by the cockpit
 * actor with a "Godkendt"-note, then the action is logged separately when the
 * owner does it. This split keeps the "approve" idempotent: clicking it twice
 * never double-posts; the second click is a no-op 409.
 *
 * Goes through `withCompanyMutation` so the backup lock, the localhost gate
 * and actor attribution all apply.
 */
export async function handleApproveAgentSuggestion(
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
      assertOpenAgentException(ctx.db, id);
      const note = optionalBodyString(body, "note");
      // The decision-flavored note preserves the owner's free-text reason, if
      // any, while making the audit trail self-explaining at a glance.
      const decisionNote = note
        ? `Godkendt af ejer i cockpit: ${note}`
        : "Godkendt af ejer i cockpit";
      const payload = withCockpitActor(
        { id, note: decisionNote, resolvedBy: ctx.actor.createdBy },
        ctx.actor,
      );
      return resolveException(ctx.db, payload);
    },
  );

  return okResponse({
    suggestion: {
      id,
      decision: "approved" as const,
      resolved: result.resolved,
    },
  });
}

/**
 * POST /api/companies/:slug/agent-suggestions/:id/reject — owner rejects the
 * agent's suggestion (#346). Resolves the underlying exception with a
 * "Afvist"-note carrying the owner's free-text reason. A rejection NEVER posts
 * anything; it just clears the suggestion from the queue. The owner's reason
 * is recommended (it travels into `resolution_note`, the audit-traceable
 * column the agent loop reads to learn from rejections) but technically
 * optional, mirroring the resolve-exception route.
 *
 * Goes through `withCompanyMutation` so the backup lock, the localhost gate
 * and actor attribution all apply.
 */
export async function handleRejectAgentSuggestion(
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
      assertOpenAgentException(ctx.db, id);
      const note = optionalBodyString(body, "note");
      const decisionNote = note
        ? `Afvist af ejer i cockpit: ${note}`
        : "Afvist af ejer i cockpit";
      const payload = withCockpitActor(
        { id, note: decisionNote, resolvedBy: ctx.actor.createdBy },
        ctx.actor,
      );
      return resolveException(ctx.db, payload);
    },
  );

  return okResponse({
    suggestion: {
      id,
      decision: "rejected" as const,
      resolved: result.resolved,
    },
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
 * POST /api/companies/:slug/recurring-invoices/:id/retire — deactivates a
 * recurring-invoice template (#435).
 *
 * Templates are append-only by schema: identity columns and the embedded
 * payload are guarded by a trigger, and a retired (active = 0) template
 * cannot be unretired. This handler is the cockpit's deactivation surface so
 * an owner does not have to drop to the CLI to stop a now-irrelevant template
 * from suggesting itself in the cockpit.
 *
 * Generation refuses an inactive template (the core `generateRecurringInvoice`
 * already returns an error in that case), so a retired template can never
 * issue another invoice — past generations stay untouched on `documents`.
 *
 * Goes through `withCompanyMutation` (backup lock, localhost gate, actor
 * attribution). `requireConfirm: true` matches the surrounding write-routes.
 */
export async function handleRetireRecurringInvoiceTemplate(
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
      const reason = optionalBodyString(body, "reason");
      const retired = retireRecurringInvoiceTemplate(ctx.db, {
        templateId,
        reason,
        createdBy: ctx.actor.createdBy,
        createdByProgram: ctx.actor.createdByProgram,
      });
      return {
        ok: retired.ok,
        errors: retired.errors,
        template: {
          id: retired.templateId ?? templateId,
          retired: retired.ok,
        },
      };
    },
    { requireConfirm: true },
  );

  return okResponse({ template: result.template });
}

const VALID_INTERVALS: readonly RecurringInterval[] = [
  "monthly",
  "quarterly",
  "yearly",
] as const;

const VALID_DELIVERY_MODES: readonly DeliveryPeriodMode[] = [
  "issue_month",
  "interval_window",
  "none",
] as const;

/**
 * POST /api/companies/:slug/recurring-invoices — creates a recurring-invoice
 * template from the cockpit (#386).
 *
 * Before #386 the only way to create a template was the CLI — the cockpit's
 * empty-state literally hard-coded the snippet
 *   `rentemester recurring-invoice create --company <path> --input template.json`
 * and gave the SMB-owner no UI affordance to actually start using the feature.
 * This handler closes that gap by accepting the same minimal shape the
 * `InvoiceIssueModal` already uses (line items + a single vatRatePercent),
 * running `computeInvoiceAmounts` server-side, then handing the full
 * `InvoicePayload` to the SAME `createRecurringInvoiceTemplate` core the CLI
 * calls. The human never does invoice arithmetic.
 *
 * Body: `{ name, interval, firstIssueDate, paymentTermsDays,
 * deliveryPeriodMode, notes, vatRatePercent, currency, customerId, buyer,
 * lines: [{description, quantity, unitPriceExVat}], confirm: true }`.
 *
 * Goes through `withCompanyMutation` (backup lock, localhost gate, actor
 * attribution). `requireConfirm: true` matches the surrounding write-routes —
 * a template is append-only by schema, so creating one is irreversible (can
 * only be retired, not deleted).
 */
export async function handleCreateRecurringInvoiceTemplate(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const name = requireBodyString(body, "name");
      const intervalRaw = requireBodyString(body, "interval");
      if (!(VALID_INTERVALS as readonly string[]).includes(intervalRaw)) {
        throw ApiError.badRequest(
          "'interval' must be one of monthly, quarterly, yearly",
        );
      }
      const interval = intervalRaw as RecurringInterval;
      const firstIssueDate = requireBodyString(body, "firstIssueDate");
      const paymentTermsDays =
        optionalBodyNumber(body, "paymentTermsDays") ?? 30;
      const deliveryModeRaw =
        optionalBodyString(body, "deliveryPeriodMode") ?? "issue_month";
      if (!(VALID_DELIVERY_MODES as readonly string[]).includes(deliveryModeRaw)) {
        throw ApiError.badRequest(
          "'deliveryPeriodMode' must be one of issue_month, interval_window, none",
        );
      }
      const deliveryPeriodMode = deliveryModeRaw as DeliveryPeriodMode;
      const notes = optionalBodyString(body, "notes");
      const vatRatePercent = optionalBodyNumber(body, "vatRatePercent") ?? 25;
      const currency = optionalBodyString(body, "currency") ?? "DKK";
      const customerId = optionalBodyPositiveInt(body, "customerId");
      const lines = parseInvoiceLines(body.lines);
      const buyer = parseInvoiceParty(body.buyer, "buyer");
      const seller = parseInvoiceParty(body.seller, "seller");

      // Server computes every derived amount — same compute path as
      // `handleInvoiceIssue`. A compute rejection (blank line, bad quantity)
      // returns ok:false from core and surfaces as a 400.
      const computed = computeInvoiceAmounts(lines, vatRatePercent);
      if (!computed.ok) {
        return { ok: false, errors: computed.errors };
      }

      const invoicePayload: InvoicePayload = {
        invoiceType: "full",
        vatTreatment: "standard",
        seller,
        buyer,
        lines: computed.lines,
        totals: {
          netAmount: computed.totals.netAmount,
          vatRate: computed.totals.vatRate,
          vatAmount: computed.totals.vatAmount,
          grossAmount: computed.totals.grossAmount,
        },
        currency,
      };

      // Master-data resolution mirrors the issue-invoice path: a given
      // `customerId` back-fills buyer name/address/VAT from registered
      // customer master-data.
      const resolved = resolveInvoiceMasterData(ctx.db, invoicePayload, {
        customerId,
      });
      if (!resolved.ok) {
        return {
          ok: false,
          errors: resolved.errors ?? ["master-data resolution failed"],
        };
      }

      const created = createRecurringInvoiceTemplate(ctx.db, {
        name,
        interval,
        firstIssueDate,
        paymentTermsDays,
        deliveryPeriodMode,
        notes,
        invoice: resolved.payload,
        createdBy: ctx.actor.createdBy,
        createdByProgram: ctx.actor.createdByProgram,
      });

      return {
        ok: created.ok,
        errors: created.errors,
        template: {
          templateId: created.templateId ?? null,
          name,
          interval,
          firstIssueDate,
        },
      };
    },
    { requireConfirm: true },
  );

  return okResponse({ template: result.template });
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

/** Reads an optional boolean body field, mapping a bad value to a 400 (#434). */
function optionalBodyBoolean(
  body: Record<string, unknown>,
  key: string,
): boolean | undefined {
  const value = body[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "boolean") {
    throw ApiError.badRequest(`'${key}' must be a boolean when present`);
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
 * POST /api/companies/:slug/invoices/preview — render an invoice PDF without
 * writing anything to the ledger (#440).
 *
 * Body: identical shape to `handleInvoiceIssue` (issueDate, lines, vatRatePercent,
 * customerId, buyer, seller, ...). Runs the SAME compute + validate + enrich
 * + buyer-master-data resolution as `issueInvoice`, then calls
 * `previewIssuedInvoicePdf` instead — no document row, no audit_log, no
 * sequence draw. Returns the raw PDF bytes with Content-Type application/pdf,
 * exactly like `GET .../invoices/:id/pdf`.
 *
 * Does NOT go through `withCompanyMutation`: this is a pure read+render. The
 * backup-lock gate and the confirm gate are irrelevant — nothing is mutated —
 * and the audit-log actor attribution is similarly irrelevant. The localhost
 * hard-gate IS still enforced (Phase-1 trust); the company-resolution +
 * db-open pattern is the same shape as the mutation pipeline, minus those
 * gates.
 */
export async function handleInvoicePreview(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  // Phase-1 localhost trust + the auth seam — kept identical to the write
  // pipeline so the preview cannot be probed by a non-loopback client.
  authMiddleware(request, config);
  if (!config.authRequired) {
    const hostHeader = (request.headers.get("host") ?? "").trim().toLowerCase();
    const host = hostHeader.startsWith("[")
      ? hostHeader.slice(
          1,
          hostHeader.indexOf("]") === -1 ? undefined : hostHeader.indexOf("]"),
        )
      : (hostHeader.split(":")[0] ?? "");
    const isLoopback =
      host === "127.0.0.1" ||
      host === "localhost" ||
      host === "::1" ||
      host === "0:0:0:0:0:0:0:1";
    if (!isLoopback) {
      throw ApiError.unauthorized(
        "Forhåndsvisning fra Cockpit er kun tilladt fra localhost, " +
          "medmindre godkendelse er slået til.",
      );
    }
  }

  if (!findWorkspaceCompany(config.workspaceRoot, slug)) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(config.workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }

  const raw = await request.text();
  let body: Record<string, unknown>;
  if (raw.trim().length === 0) {
    body = {};
  } else {
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      throw ApiError.badRequest("request body must be valid JSON");
    }
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      throw ApiError.badRequest("request body must be a JSON object");
    }
    body = parsed as Record<string, unknown>;
  }

  const issueDate = requireBodyString(body, "issueDate");
  const lines = parseInvoiceLines(body.lines);
  const vatRatePercent = optionalBodyNumber(body, "vatRatePercent") ?? 25;
  const customerId = optionalBodyPositiveInt(body, "customerId");
  const invoiceNumber = optionalBodyString(body, "invoiceNumber");
  const dueDate = optionalBodyString(body, "dueDate");
  const currency = optionalBodyString(body, "currency");
  const buyer = parseInvoiceParty(body.buyer, "buyer");
  const seller = parseInvoiceParty(body.seller, "seller");

  const computed = computeInvoiceAmounts(lines, vatRatePercent);
  if (!computed.ok) {
    throw ApiError.badRequest(computed.errors.join("; "));
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

  const db = openDb(dbPath);
  try {
    migrate(db);
    const resolved = resolveInvoiceMasterData(db, payload, { customerId });
    if (!resolved.ok) {
      const message = (resolved.errors ?? ["master-data resolution failed"]).join("; ");
      if (/does not exist|findes ikke|not found|allerede|already/i.test(message)) {
        throw ApiError.conflict(message);
      }
      throw ApiError.badRequest(message);
    }
    const preview = previewIssuedInvoicePdf(db, resolved.payload);
    if (!preview.ok) {
      throw ApiError.badRequest(preview.errors.join("; "));
    }
    // PDF bytes inline — same Content-Type / cache headers as the real
    // `GET .../invoices/:id/pdf` route so the cockpit can open the response
    // in a new tab via window.open()/URL.createObjectURL.
    return new Response(preview.pdfBytes, {
      status: 200,
      headers: {
        "content-type": "application/pdf",
        "content-disposition": `inline; filename*=UTF-8''${encodeURIComponent(
          `${preview.invoiceNumber}.pdf`,
        )}`,
        "x-content-type-options": "nosniff",
        "cache-control": "private, no-store",
      },
    });
  } finally {
    db.close();
  }
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
// Send invoice by e-mail (#429).
//
// Before this route the cockpit could only render "Hent PDF" — to actually
// get the invoice to the customer the owner had to download the PDF, open
// Outlook/Gmail/Apple Mail, look up the customer's e-mail and attach the
// file by hand. The fall-back was the CLI command `invoice send`, which
// most owners never discover. This handler is the cockpit's third caller of
// the SAME `sendInvoiceEmail` core function the CLI's `invoice send` and the
// MCP tool `invoice_send_email` use — so the cockpit and the terminal
// produce a byte-identical MIME message and the same `email_send_log` row.
//
// SMTP CONFIG (non-secret host/port/fromAddress + optional credentials) is
// read from a file at `config/smtp.json` inside the company directory,
// mirroring how the CLI command loads its config. Credentials never enter
// the request body nor the server config object. When the file is missing
// the handler returns a 400 with a clear next-step message — never a 500.
// --------------------------------------------------------------------------

/**
 * Loads `config/smtp.json` from the company directory. The file is non-secret
 * config (host/port/fromAddress) plus optional credentials the default SMTP
 * transport reads but never persists. Returns a `400` with a clear next-step
 * when the file is missing or malformed — the cockpit shows that message
 * verbatim so the owner knows what to configure.
 */
function loadConfiguredSmtp(companyRoot: string): SmtpConfig {
  const path = join(companyPaths(companyRoot).config, "smtp.json");
  if (!existsSync(path)) {
    throw ApiError.badRequest(
      "SMTP er ikke konfigureret for denne virksomhed. Opret config/smtp.json " +
        "med {host, port, fromAddress} (og evt. username/password) inde i " +
        "virksomhedsmappen for at sende fakturaer på mail fra cockpittet.",
    );
  }
  let raw: string;
  try {
    raw = readFileSync(path, "utf8");
  } catch (error) {
    throw ApiError.badRequest(
      `SMTP-config kunne ikke læses fra ${path}: ${(error as Error).message}`,
    );
  }
  let parsed: Partial<SmtpConfig>;
  try {
    parsed = JSON.parse(raw) as Partial<SmtpConfig>;
  } catch (error) {
    throw ApiError.badRequest(
      `SMTP-config er ikke gyldig JSON: ${(error as Error).message}`,
    );
  }
  return {
    host: typeof parsed.host === "string" ? parsed.host : "",
    port: typeof parsed.port === "number" ? parsed.port : Number(parsed.port ?? 0),
    fromAddress: typeof parsed.fromAddress === "string" ? parsed.fromAddress : "",
    fromName: typeof parsed.fromName === "string" ? parsed.fromName : undefined,
    username: typeof parsed.username === "string" ? parsed.username : undefined,
    password: typeof parsed.password === "string" ? parsed.password : undefined,
    dryRun: typeof parsed.dryRun === "boolean" ? parsed.dryRun : undefined,
  };
}

/**
 * POST /api/companies/:slug/invoices/send-email — sends an issued invoice
 * (or a reminder for it) to the customer's e-mail via SMTP, with the PDF
 * attached.
 *
 * Body: `{ invoiceDocumentId: number, to: string, kind?: 'invoice' |
 * 'reminder', confirm: true }`. Calls the SAME `sendInvoiceEmail` core
 * function the CLI's `invoice send` command uses, so the cockpit and the
 * terminal produce a byte-identical MIME message and `email_send_log` row.
 * Idempotent: an identical send collapses onto the existing row instead of
 * re-transmitting (the core derives the message-id from the inputs).
 *
 * Write-irreversible (it appends an `email_send_log` row AND an `audit_log`
 * entry — both write-once tables) so `requireConfirm` is set. Goes through
 * `withCompanyMutation`, so the backup lock, the localhost gate and actor
 * attribution all apply.
 *
 * SMTP config (non-secret host/port/fromAddress + optional credentials) is
 * loaded from `config/smtp.json` inside the company directory; credentials
 * are NEVER passed in the request body. A missing/invalid config is a 400.
 */
export async function handleInvoiceSendEmail(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      void ctx.actor;
      const invoiceDocumentId = requireBodyPositiveInt(body, "invoiceDocumentId");
      const to = requireBodyString(body, "to");
      const kindRaw = body.kind;
      if (
        kindRaw !== undefined &&
        kindRaw !== "invoice" &&
        kindRaw !== "reminder"
      ) {
        throw ApiError.badRequest(
          "'kind' must be 'invoice' or 'reminder' when present",
        );
      }
      const kind = (kindRaw ?? "invoice") as EmailKind;
      const smtp = loadConfiguredSmtp(ctx.companyRoot);
      const sent = sendInvoiceEmail(ctx.db, ctx.companyRoot, {
        invoiceDocumentId,
        kind,
        to,
        smtp,
        transport: createSmtpTransport(smtp),
      });
      return {
        ok: sent.ok,
        errors: sent.errors,
        invoiceNumber: sent.invoiceNumber,
        recipient: sent.recipient,
        subject: sent.subject,
        messageId: sent.messageId,
        kind: sent.kind,
        duplicate: sent.duplicate,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    delivery: {
      invoiceNumber: result.invoiceNumber ?? null,
      recipient: result.recipient ?? null,
      subject: result.subject ?? null,
      messageId: result.messageId ?? null,
      duplicate: Boolean(result.duplicate),
    },
  });
}

// --------------------------------------------------------------------------
// Send rykker (betalingspaamindelse) (#434).
//
// Before this route the cockpit could show a row as "Forfalden · 47 dage"
// and surface the customer's e-mail, but there was no way to actually send
// the rykker — the owner had to download the PDF, open the mail client and
// type the reminder by hand, and the cockpit had no record that the rykker
// went out. This handler combines THREE existing core calls in a single
// mutation so the cockpit's "Send rykker" button is a one-click write:
//
//   1. `registerInvoiceReminder` — registers the reminder + statutory fee
//      (max 100 kr/reminder, max 3 reminders per rentel. § 9b),
//   2. (when `bookFee` is true) `postInvoiceReminderToLedger` — journals
//      the fee against the customer receivable so it shows up on the
//      claim balance,
//   3. `sendInvoiceEmail` with `kind: 'reminder'` — same SMTP transport as
//      "Send på mail" (#429), so the message is byte-identical to a CLI
//      send.
//
// Write-irreversible (insert into `invoice_reminders` + optional journal
// entry + always-on `email_send_log` and `audit_log` rows), so the route
// requires `confirm: true`. The SMTP config (non-secret host/port/from +
// optional credentials) is read from `config/smtp.json` inside the company
// directory — credentials never enter the request body.
// --------------------------------------------------------------------------

/**
 * POST /api/companies/:slug/invoices/send-reminder — registers a payment
 * reminder for an overdue issued invoice and sends it on e-mail.
 *
 * Body: `{ invoiceDocumentId: number, to: string, bookFee: boolean,
 * feeAmount?: number, confirm: true }`. `bookFee=true` also journals the
 * statutory reminder fee (defaults to 100 kr). The core registration step
 * enforces the rentel. § 9b limits — a 4th reminder, a fee above 100 kr or
 * a reminder less than 10 days after the previous one is rejected by the
 * core with a clear message that the handler echoes back as a 400.
 *
 * The handler is intentionally compound (3 core calls) but stays a thin
 * shell — every business rule remains in the core functions the CLI's
 * `invoice remind` / `invoice post-reminder` / `invoice send --kind
 * reminder` commands already exercise. The cockpit becomes a third caller
 * of the SAME functions, never a parallel implementation.
 */
export async function handleInvoiceSendReminder(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      void ctx.actor;
      const invoiceDocumentId = requireBodyPositiveInt(body, "invoiceDocumentId");
      const to = requireBodyString(body, "to");
      const bookFee = optionalBodyBoolean(body, "bookFee") ?? true;
      const feeAmount = optionalBodyNumber(body, "feeAmount");

      // Reminder date is "today" in ISO form (UTC slice) — the core enforces
      // the "10 days since previous reminder" rule and refuses anything that
      // would breach the statutory series.
      const reminderDate = new Date().toISOString().slice(0, 10);

      // (1) Register the reminder + fee. The core enforces:
      //     - the invoice must exist and be an issued_invoice,
      //     - currency must be DKK,
      //     - the invoice must be overdue with a positive open balance,
      //     - fee <= 100 kr,
      //     - <= 3 reminders per invoice,
      //     - >= 10 days since the previous reminder.
      const registered = registerInvoiceReminder(ctx.db, {
        invoiceDocumentId,
        reminderDate,
        feeAmount,
      });
      if (!registered.ok) {
        throw ApiError.badRequest(
          registered.errors[0] ?? "Reminder could not be registered.",
        );
      }

      // (2) Optionally journal the fee — the standard owner intent. The
      //     core uses the same receivable/income accounts as the CLI's
      //     `invoice post-reminder` command, so the journal entry is
      //     byte-identical no matter which surface the owner uses.
      let journalEntryNo: string | null = null;
      let feeBooked = false;
      if (bookFee) {
        const posted = postInvoiceReminderToLedger(ctx.db, {
          invoiceDocumentId,
          reminderId: registered.reminderId,
        });
        if (!posted.ok) {
          throw ApiError.badRequest(
            posted.errors?.[0] ?? "Reminder fee could not be booked.",
          );
        }
        journalEntryNo = posted.entryNo ?? null;
        feeBooked = true;
      }

      // (3) Send the reminder e-mail. SAME core function as "Send på mail"
      //     (#429), with `kind: 'reminder'` so the subject + body match
      //     what `invoice send --kind reminder` produces from the CLI.
      const smtp = loadConfiguredSmtp(ctx.companyRoot);
      const sent = sendInvoiceEmail(ctx.db, ctx.companyRoot, {
        invoiceDocumentId,
        kind: "reminder",
        to,
        smtp,
        transport: createSmtpTransport(smtp),
      });
      if (!sent.ok) {
        throw ApiError.badRequest(
          sent.errors?.[0] ?? "Reminder e-mail could not be sent.",
        );
      }

      return {
        ok: true as const,
        invoiceNumber: registered.invoiceNumber ?? sent.invoiceNumber ?? null,
        recipient: sent.recipient ?? to,
        reminderSequence: registered.reminderSequence ?? null,
        feeAmount: registered.feeAmount ?? null,
        feeBooked,
        journalEntryNo,
        messageId: sent.messageId ?? null,
        duplicate: Boolean(sent.duplicate),
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    reminder: {
      invoiceNumber: result.invoiceNumber ?? null,
      recipient: result.recipient ?? null,
      reminderSequence: result.reminderSequence ?? null,
      feeAmount: result.feeAmount ?? null,
      feeBooked: Boolean(result.feeBooked),
      journalEntryNo: result.journalEntryNo ?? null,
      messageId: result.messageId ?? null,
      duplicate: Boolean(result.duplicate),
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
 * DELETE /api/companies/:slug/customers/:id — sletter en kunde fra master data
 * (#430). En fejl-importeret eller dubleret kunde skal kunne fjernes fra
 * cockpittet — ikke kun fra CLI'en. Tredje caller af samme `deleteCustomer`
 * core funktion som CLI'en og MCP'en bruger (når disse senere wires).
 *
 * Sletningen blokeres af core hvis kunden er i brug på en åben (ikke-betalt)
 * udstedt faktura — core returnerer `{ok:false, errors:[...]}` med et klart
 * dansk-sproget budskab + fakturanummeret, og `withCompanyMutation` mapper det
 * til en 400 så cockpittet kan vise beskeden verbatim. Bogførte fakturaer
 * påvirkes ikke (buyer-snapshottet på fakturaen er ikke en FK).
 *
 * Sletningen audit-logges i `audit_log` (event_type `customer_delete`).
 * `requireConfirm` er sat fordi sletningen er en irreversibel mutation af
 * master-data.
 */
export async function handleDeleteCustomer(
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
    (ctx) => {
      const deleted = deleteCustomer(ctx.db, id);
      return {
        ok: deleted.ok,
        errors: deleted.errors,
      };
    },
    { requireConfirm: true },
  );
  return okResponse({ customer: { id, deleted: result.ok } });
}

/**
 * DELETE /api/companies/:slug/vendors/:id — sletter en leverandør (#430).
 * Blokeres hvis leverandøren er i brug på en åben gæld (`payables` med
 * `vendor_id` FK). Audit-logges som `vendor_delete`.
 */
export async function handleDeleteVendor(
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
    (ctx) => {
      const deleted = deleteVendor(ctx.db, id);
      return {
        ok: deleted.ok,
        errors: deleted.errors,
      };
    },
    { requireConfirm: true },
  );
  return okResponse({ vendor: { id, deleted: result.ok } });
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
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(config.workspaceRoot, slug);
  const { companyPaths } = await import("../core/paths");
  const dbPath = companyPaths(companyRoot).db;
  const { existsSync } = await import("node:fs");
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
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

/**
 * POST /api/companies/:slug/mileage — registers one mileage entry (#335).
 *
 * Body: `{ tripDate, purpose, fromLocation, toLocation, kilometers, vehicle,
 * driver, ratePerKm, rateBasis, rateSource?, notes?, confirm: true }`. Calls
 * the SAME `createMileageEntry` core function the CLI's `mileage add` command
 * and the MCP tool use — the cockpit is a third caller, never a reimplementer.
 *
 * The mileage register is append-only audit data (the schema's `BEFORE
 * UPDATE` / `BEFORE DELETE` triggers refuse anything else), so the body
 * carries `confirm: true`. Goes through `withCompanyMutation` for the backup
 * lock, the localhost gate and actor attribution.
 */
export async function handleMileageCreate(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const tripDate = requireBodyString(body, "tripDate");
      const purpose = requireBodyString(body, "purpose");
      const fromLocation = requireBodyString(body, "fromLocation");
      const toLocation = requireBodyString(body, "toLocation");
      const kilometers = requireBodyNumber(body, "kilometers");
      const vehicle = requireBodyString(body, "vehicle");
      const driver = requireBodyString(body, "driver");
      const ratePerKm = requireBodyNumber(body, "ratePerKm");
      const rateBasis = requireBodyString(body, "rateBasis");
      const rateSource = optionalBodyString(body, "rateSource");
      const notes = optionalBodyString(body, "notes");

      const input: CreateMileageEntryInput = {
        tripDate,
        purpose,
        fromLocation,
        toLocation,
        kilometers,
        vehicle,
        driver,
        ratePerKm,
        rateBasis,
        ...(rateSource ? { rateSource } : {}),
        ...(notes ? { notes } : {}),
      };
      const created = createMileageEntry(ctx.db, input);
      return {
        ok: created.ok,
        errors: created.errors,
        mileageEntryId: created.mileageEntryId,
        entryNo: created.entryNo,
        amountBasis: created.amountBasis,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    mileage: {
      mileageEntryId: result.mileageEntryId ?? null,
      entryNo: result.entryNo ?? null,
      amountBasis: result.amountBasis ?? null,
    },
  });
}

/** Reads a required, finite-number body field, mapping a bad value to a 400. */
function requireBodyNumber(
  body: Record<string, unknown>,
  key: string,
): number {
  const value = body[key];
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw ApiError.badRequest(
      `'${key}' is required and must be a finite number`,
    );
  }
  return value;
}

// --------------------------------------------------------------------------
// Anlæg (fixed assets) — #336
//
// All three actions are write-irreversible and go through the SAME core
// (`registerAsset`, `postDepreciationPeriod`, `postImmediateWriteOff`) the
// CLI `asset` sub-commands and the MCP tools use — the cockpit becomes a
// third caller, never re-implementing the depreciation arithmetic. Every
// action goes through `withCompanyMutation`, so the backup-lock, the
// localhost gate, actor attribution and the confirm gate all apply.
// --------------------------------------------------------------------------

/** Reads a required positive-integer body field, mapping a bad value to a 400. */
function requireBodyPositiveInt(
  body: Record<string, unknown>,
  key: string,
): number {
  const value = body[key];
  if (typeof value !== "number" || !Number.isInteger(value) || value <= 0) {
    throw ApiError.badRequest(
      `'${key}' is required and must be a positive integer`,
    );
  }
  return value;
}

/** Reads a required positive-number body field, mapping a bad value to a 400. */
function requireBodyPositiveNumber(
  body: Record<string, unknown>,
  key: string,
): number {
  const value = body[key];
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) {
    throw ApiError.badRequest(
      `'${key}' is required and must be a positive number`,
    );
  }
  return value;
}

/**
 * POST /api/companies/:slug/assets — registers a capitalised asset and writes
 * its deterministic linear depreciation plan. Third caller of `registerAsset`
 * (CLI: `asset register`, MCP: `asset_register`). The asset row is
 * append-only; the schedule is recomputed deterministically per the core
 * (never persisted by the cockpit). Write-irreversible, so the body carries
 * `confirm: true`.
 */
export async function handleAssetRegister(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const name = requireBodyString(body, "name");
      const category = requireBodyString(body, "category");
      const acquisitionDate = requireBodyString(body, "acquisitionDate");
      const cost = requireBodyPositiveNumber(body, "cost");
      const usefulLifeMonths = requireBodyPositiveInt(body, "usefulLifeMonths");
      const purchaseDocumentId = requireBodyPositiveInt(
        body,
        "purchaseDocumentId",
      );
      const assetAccountNo = optionalBodyString(body, "assetAccountNo");
      const depreciationExpenseAccountNo = optionalBodyString(
        body,
        "depreciationExpenseAccountNo",
      );
      const accumulatedDepreciationAccountNo = optionalBodyString(
        body,
        "accumulatedDepreciationAccountNo",
      );
      const note = optionalBodyString(body, "note");
      return registerAsset(
        ctx.db,
        withCockpitActor(
          {
            name,
            category,
            acquisitionDate,
            cost,
            usefulLifeMonths,
            purchaseDocumentId,
            ...(assetAccountNo ? { assetAccountNo } : {}),
            ...(depreciationExpenseAccountNo
              ? { depreciationExpenseAccountNo }
              : {}),
            ...(accumulatedDepreciationAccountNo
              ? { accumulatedDepreciationAccountNo }
              : {}),
            ...(note ? { note } : {}),
          },
          ctx.actor,
        ),
      );
    },
    { requireConfirm: true },
  );

  return okResponse({
    asset: {
      assetId: result.assetId ?? null,
      totalPeriods: result.totalPeriods ?? null,
      periodAmount: result.periodAmount ?? null,
    },
  });
}

/**
 * POST /api/companies/:slug/assets/:id/depreciate — posts one period of an
 * asset's linear depreciation schedule. The body may carry `transactionDate`
 * and `periodIndex`; when `periodIndex` is omitted the next unposted period
 * is derived from `asset_depreciation_entries` so the cockpit's one-click
 * "Beregn afskrivning" action just works. Third caller of
 * `postDepreciationPeriod`.
 */
export async function handleAssetDepreciate(
  config: ServerConfig,
  request: Request,
  slug: string,
  idRaw: string,
): Promise<Response> {
  const assetId = parseIdParam(idRaw, "id");

  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const transactionDate =
        optionalBodyString(body, "transactionDate") ??
        new Date().toISOString().slice(0, 10);

      let periodIndex: number;
      const explicit = body.periodIndex;
      if (explicit === undefined || explicit === null) {
        const posted = ctx.db.query(
          "SELECT COUNT(*) AS posted FROM asset_depreciation_entries WHERE asset_id = ?",
        ).get(assetId) as { posted: number };
        periodIndex = Number(posted.posted ?? 0) + 1;
      } else {
        if (
          typeof explicit !== "number" ||
          !Number.isInteger(explicit) ||
          explicit <= 0
        ) {
          throw ApiError.badRequest(
            "'periodIndex' must be a positive integer when present",
          );
        }
        periodIndex = explicit;
      }

      return postDepreciationPeriod(
        ctx.db,
        withCockpitActor(
          { assetId, periodIndex, transactionDate },
          ctx.actor,
        ),
      );
    },
    { requireConfirm: true },
  );

  return okResponse({
    depreciation: {
      entryId: result.entryId ?? null,
      assetId: result.assetId ?? null,
      periodIndex: result.periodIndex ?? null,
      periodAmount: result.periodAmount ?? null,
    },
  });
}

/**
 * POST /api/companies/:slug/assets/write-off — books a small purchase as a
 * straksafskrivning (immediate write-off). The endpoint is anchored at the
 * collection rather than `:id` because no `assets` row exists yet for a
 * straksafskrivning — the purchase document is expensed directly. Third
 * caller of `postImmediateWriteOff`. The core's threshold-rule-source guard
 * is propagated verbatim through the request body.
 */
export async function handleAssetWriteOff(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const name = requireBodyString(body, "name");
      const category = requireBodyString(body, "category");
      const acquisitionDate = requireBodyString(body, "acquisitionDate");
      const transactionDate = requireBodyString(body, "transactionDate");
      const cost = requireBodyPositiveNumber(body, "cost");
      const purchaseDocumentId = requireBodyPositiveInt(
        body,
        "purchaseDocumentId",
      );
      const expenseAccountNo = requireBodyString(body, "expenseAccountNo");
      const thresholdRuleSource = requireBodyString(
        body,
        "thresholdRuleSource",
      );
      const paymentAccountNo = optionalBodyString(body, "paymentAccountNo");
      const note = optionalBodyString(body, "note");

      return postImmediateWriteOff(
        ctx.db,
        withCockpitActor(
          {
            name,
            category,
            acquisitionDate,
            transactionDate,
            cost,
            purchaseDocumentId,
            expenseAccountNo,
            thresholdRuleSource,
            // The cockpit modal IS the human's deliberate confirmation —
            // mirroring the CLI's `--confirm yes`. The server's
            // `withCompanyMutation` already required `confirm: true` on the
            // request body, so this propagates the same intent into core.
            confirmImmediateWriteOff: true,
            ...(paymentAccountNo ? { paymentAccountNo } : {}),
            ...(note ? { note } : {}),
          },
          ctx.actor,
        ),
      );
    },
    { requireConfirm: true },
  );

  return okResponse({
    writeOff: {
      writeOffId: result.writeOffId ?? null,
      entryId: result.entryId ?? null,
      cost: result.cost ?? null,
      thresholdDkk: result.thresholdDkk ?? null,
    },
  });
}

// --------------------------------------------------------------------------
// Leverandørfaktura — payables (#340).
//
// Two write actions: register an ingested purchase document (bilag) as a
// kreditorpost (debit expense + købsmoms, credit 7000 Leverandørgæld) AND
// match an outgoing bank payment against an open payable (debit 7000, credit
// bank). Both go through the SAME `core/payables.ts` functions the CLI's
// `payable register` and `payable pay` commands use, so the cockpit never
// reimplements bookkeeping. Both append journal entries and are therefore
// `requireConfirm: true`.
// --------------------------------------------------------------------------

/**
 * POST /api/companies/:slug/payables — registers an existing purchase
 * document (bilag) as a leverandørfaktura. Body:
 *   { documentId: number, billDate: string, dueDate: string,
 *     expenseAccountNo: string, vatTreatment?: "standard"|"exempt",
 *     vendorId?: number, note?: string, confirm: true }
 *
 * Write-irreversible (it appends a kreditorpost journal entry), so
 * `requireConfirm` is set. A duplicate registration is refused by core and is
 * mapped to a 409 conflict by the shared `withCompanyMutation` heuristic.
 */
export async function handlePayableRegister(
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
      const billDate = requireBodyString(body, "billDate");
      const dueDate = requireBodyString(body, "dueDate");
      const expenseAccountNo = requireBodyString(body, "expenseAccountNo");
      const vatTreatmentRaw = optionalBodyString(body, "vatTreatment");
      if (
        vatTreatmentRaw !== undefined &&
        vatTreatmentRaw !== "standard" &&
        vatTreatmentRaw !== "exempt"
      ) {
        throw ApiError.badRequest(
          "'vatTreatment' must be one of: standard, exempt",
        );
      }
      const vendorId = optionalBodyPositiveInt(body, "vendorId");
      const note = optionalBodyString(body, "note");
      const registered = corePayableRegister(
        ctx.db,
        withCockpitActor(
          {
            documentId,
            billDate,
            dueDate,
            expenseAccountNo,
            ...(vatTreatmentRaw
              ? { vatTreatment: vatTreatmentRaw as "standard" | "exempt" }
              : {}),
            ...(vendorId !== undefined ? { vendorId } : {}),
            ...(note ? { note } : {}),
          },
          ctx.actor,
        ),
      );
      return {
        ok: registered.ok,
        errors: registered.errors,
        payableId: registered.payableId,
        documentId: registered.documentId,
        supplierName: registered.supplierName,
        billNo: registered.billNo,
        grossAmount: registered.grossAmount,
        netAmount: registered.netAmount,
        vatAmount: registered.vatAmount,
        dueDate: registered.dueDate,
        entryId: registered.entryId,
        entryNo: registered.entryNo,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    payable: {
      payableId: result.payableId ?? null,
      documentId: result.documentId ?? null,
      supplierName: result.supplierName ?? null,
      billNo: result.billNo ?? null,
      grossAmount: result.grossAmount ?? 0,
      netAmount: result.netAmount ?? 0,
      vatAmount: result.vatAmount ?? 0,
      dueDate: result.dueDate ?? null,
      entryId: result.entryId ?? null,
      entryNo: result.entryNo ?? null,
    },
  });
}

/**
 * POST /api/companies/:slug/payables/:id/pay — applies an outgoing bank
 * payment to an open payable. Body:
 *   { bankTransactionId: number, paymentDate?: string, amount?: number,
 *     paymentAccountNo?: string, note?: string, confirm: true }
 *
 * Write-irreversible (it appends a settlement journal entry + a
 * `payable_payments` row), so `requireConfirm` is set. A double-pay against
 * the same bank line is refused by core (`bank transaction N is already
 * linked …`) and the shared `already` heuristic maps it to a 409.
 */
export async function handlePayablePay(
  config: ServerConfig,
  request: Request,
  slug: string,
  idRaw: string,
): Promise<Response> {
  const payableId = parseIdParam(idRaw, "id");
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const bankTransactionId = requireBodyPositiveInt(
        body,
        "bankTransactionId",
      );
      const paymentDate = optionalBodyString(body, "paymentDate");
      const amount = optionalBodyNumber(body, "amount");
      const paymentAccountNo = optionalBodyString(body, "paymentAccountNo");
      const note = optionalBodyString(body, "note");
      const paid = corePayablePayFromBank(
        ctx.db,
        withCockpitActor(
          {
            payableId,
            bankTransactionId,
            ...(paymentDate ? { paymentDate } : {}),
            ...(amount !== undefined ? { amount } : {}),
            ...(paymentAccountNo ? { paymentAccountNo } : {}),
            ...(note ? { note } : {}),
          },
          ctx.actor,
        ),
      );
      return {
        ok: paid.ok,
        errors: paid.errors,
        paymentId: paid.paymentId,
        journalEntryId: paid.journalEntryId,
        payableId: paid.payableId,
        openBalance: paid.openBalance,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    payment: {
      paymentId: result.paymentId ?? null,
      journalEntryId: result.journalEntryId ?? null,
      payableId: result.payableId ?? payableId,
      openBalance: result.openBalance ?? null,
    },
  });
}

// --------------------------------------------------------------------------
// Set budget line (#339).
//
// Records (appends) the owner's planned amount for one account in one
// calendar month — the input behind the Budget vs. faktisk view. Each call
// inserts a NEW revision so the history is fully auditable; the latest
// revision is the effective budget. Re-setting the same (account, period)
// pair is therefore always safe — no special "edit existing" handler is
// needed and the audit log carries the full change-of-mind trail.
//
// Goes through `withCompanyMutation` so the backup lock, the localhost gate
// and actor attribution apply. Append-only and reversible by appending a new
// revision, so no `confirm` gate is required — the modal is the consent,
// mirroring the contact-master-data writes.
// --------------------------------------------------------------------------

/**
 * POST /api/companies/:slug/budget — sets (appends a revision for) one
 * budget line.
 *
 * Body: `{ accountNo: string, period: 'YYYY-MM', amount: number, notes?: string }`.
 * Append-only (every call inserts a new revision, latest wins), so no
 * `confirm` gate. A core rejection (unknown account, malformed period,
 * negative amount) is mapped to a 400 by `withCompanyMutation`.
 */
export async function handleSetBudget(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const accountNo = requireBodyString(body, "accountNo");
      const period = requireBodyString(body, "period");
      const amount = requireBodyNumber(body, "amount");
      const notes = optionalBodyString(body, "notes");
      const payload = withCockpitActor(
        {
          accountNo,
          period,
          amount,
          ...(notes ? { notes } : {}),
        },
        ctx.actor,
      );
      const set = setBudget(ctx.db, payload);
      return {
        ok: set.ok,
        errors: set.errors,
        budgetLineId: set.budgetLineId ?? null,
        accountNo: set.accountNo ?? null,
        period: set.period ?? null,
        amount: set.amount ?? null,
      };
    },
  );
  return okResponse({
    budget: {
      id: result.budgetLineId,
      accountNo: result.accountNo,
      period: result.period,
      amount: result.amount,
    },
  });
}

/**
 * POST /api/companies/:slug/bank-accounts — opretter en bankkonto (#345).
 *
 * Body: `{ name, slug?, bankName?, registrationNo?, accountNo?, iban?,
 * currency?, ledgerAccountNo? }`. Wrapper omkring `addBankAccount` fra
 * kernen. Backup-lock + actor-attribution sker via `withCompanyMutation`.
 */
export async function handleCreateBankAccount(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const name = requireBodyString(body, "name");
      const accSlug = optionalBodyString(body, "slug");
      const bankName = optionalBodyString(body, "bankName");
      const registrationNo = optionalBodyString(body, "registrationNo");
      const accountNo = optionalBodyString(body, "accountNo");
      const iban = optionalBodyString(body, "iban");
      const currency = optionalBodyString(body, "currency");
      const ledgerAccountNo = optionalBodyString(body, "ledgerAccountNo");
      const created = addBankAccount(ctx.db, {
        name,
        ...(accSlug ? { slug: accSlug } : {}),
        ...(bankName ? { bankName } : {}),
        ...(registrationNo ? { registrationNo } : {}),
        ...(accountNo ? { accountNo } : {}),
        ...(iban ? { iban } : {}),
        ...(currency ? { currency } : {}),
        ...(ledgerAccountNo ? { ledgerAccountNo } : {}),
      });
      // Marker actor på audit-log'en (write går gennem withCompanyMutation,
      // som sørger for at append-only audit-log fanger den).
      void ctx.actor;
      if (!created.ok) {
        return { ok: false, account: null, errors: created.errors };
      }
      return { ok: true, account: created.account, errors: [] as string[] };
    },
  );
  return okResponse({ bankAccount: result.account });
}

/**
 * POST /api/companies/:slug/gdpr/erase — GDPR-anonymisering (#334).
 *
 * Body: `{ cvr?, name?, asOf? }`. Wrapper omkring `eraseGdprSubject` fra
 * kernen — den skriver append-only tombstones, men afviser rækker der
 * stadig er under bogføringspligt (5-års retention).
 */
export async function handleGdprErase(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const cvr = optionalBodyString(body, "cvr");
      const name = optionalBodyString(body, "name");
      const asOf = optionalBodyString(body, "asOf");
      if (!cvr && !name) {
        throw ApiError.badRequest(
          "cvr eller name skal sættes — én af dem identificerer subject'et.",
        );
      }
      const erasure = eraseGdprSubject(ctx.db, {
        cvr: cvr ?? null,
        name: name ?? null,
        asOf: asOf ?? null,
      });
      void ctx.actor;
      return erasure;
    },
  );
  return okResponse({ gdprErasure: result });
}

/**
 * POST /api/companies/:slug/bilagsmail/imap-config — Saves the IMAP config to
 * `config/imap.json` (0600). Body: `{ host, port, username, password, secure?,
 * mailbox? }`. Credentials never enter the ledger — they live as a per-company
 * config-file (#348).
 */
export async function handleSaveBilagsmailImapConfig(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  await withCompanyMutation(request, config, slug, (ctx, body) => {
    const host = requireBodyString(body, "host");
    const port = body["port"];
    if (typeof port !== "number" || !Number.isInteger(port) || port <= 0) {
      throw ApiError.badRequest("'port' must be a positive integer");
    }
    const username = requireBodyString(body, "username");
    const password = requireBodyString(body, "password");
    const secure = body["secure"] !== false; // default true
    const mailbox = optionalBodyString(body, "mailbox") ?? "INBOX";
    const imapConfig: BilagsmailImapConfig = {
      host,
      port,
      username,
      password,
      secure,
      mailbox,
    };
    saveBilagsmailImapConfig(ctx.companyRoot, imapConfig);
    void ctx.actor;
    return { ok: true, errors: [] as string[] };
  });
  return okResponse({ imapConfig: { ok: true } });
}

/**
 * DELETE /api/companies/:slug/bilagsmail/imap-config — Removes the stored
 * IMAP config from disk (#348).
 */
export async function handleDeleteBilagsmailImapConfig(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  await withCompanyMutation(request, config, slug, (ctx) => {
    deleteBilagsmailImapConfig(ctx.companyRoot);
    void ctx.actor;
    return { ok: true, errors: [] as string[] };
  });
  return okResponse({ imapConfig: { ok: true, removed: true } });
}

/**
 * PATCH /api/companies/:slug/bilagsmail/alias — Sets or clears the per-company
 * mail alias used as the localpart in the bilagsmail address (#350).
 * Body: `{ alias: string | null }`.
 */
export async function handleSetBilagsmailAlias(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  await withCompanyMutation(request, config, slug, (ctx, body) => {
    const raw = body["alias"];
    const alias =
      raw === null || raw === undefined
        ? null
        : typeof raw === "string"
          ? raw
          : null;
    setCompanyMailAlias(ctx.db, alias);
    void ctx.actor;
    return { ok: true, errors: [] as string[] };
  });
  return okResponse({ mailAlias: { ok: true } });
}
