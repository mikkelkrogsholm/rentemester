// Recurring-invoice template handlers (#386, #435).

import {
  createRecurringInvoiceTemplate,
  generateRecurringInvoice,
  retireRecurringInvoiceTemplate,
  type RecurringInterval,
  type DeliveryPeriodMode,
} from "../../core/recurring-invoices";
import { resolveInvoiceMasterData } from "../../core/master-data";
import {
  computeInvoiceAmounts,
  type InvoicePayload,
} from "../../core/invoice";
import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { withCompanyMutation } from "../mutations";
import {
  okResponse,
  optionalBodyNumber,
  optionalBodyPositiveInt,
  optionalBodyString,
  parseInvoiceLines,
  parseInvoiceParty,
  requireBodyString,
} from "./_shared";

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
