/**
 * MCP-tools for fakturaer (issued invoices).
 *
 * Read:
 *   - invoice_status, invoice_list, invoice_find, invoice_overdue
 *   - invoice_interest_calc, invoice_compensation_calc
 *   - invoice_validate
 *
 * Write-irreversible:
 *   - invoice_issue, invoice_post, invoice_render
 *   - invoice_credit_note, invoice_settle_bank, invoice_settle_claim_bank
 *   - invoice_write_off_bad_debt, invoice_apply_payment, invoice_refund_bank
 *   - invoice_remind, invoice_post_reminder
 *   - invoice_claim_interest, invoice_post_interest
 *   - invoice_claim_compensation, invoice_post_compensation
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { validateInvoice, type InvoicePayload } from "../../core/invoice";
import { issueInvoice } from "../../core/issued-invoices";
import { resolveInvoiceMasterData } from "../../core/master-data";
import { postIssuedInvoiceToLedger } from "../../core/invoice-booking";
import { renderIssuedInvoicePdf } from "../../core/invoice-pdf";
import { issueCreditNote, type IssueCreditNoteInput } from "../../core/credit-notes";
import { settleInvoiceFromBank, type SettleInvoiceFromBankInput } from "../../core/invoice-settlement";
import {
  settleInvoiceClaimsFromBank,
  type SettleInvoiceClaimsFromBankInput,
} from "../../core/invoice-claim-settlement";
import { writeOffInvoiceBadDebt, type WriteOffInvoiceBadDebtInput } from "../../core/invoice-bad-debt";
import {
  applyInvoicePayment,
  getInvoiceStatus,
  type ApplyInvoicePaymentInput,
} from "../../core/invoice-payments";
import { refundInvoiceToBank, type RefundInvoiceToBankInput } from "../../core/invoice-refunds";
import {
  registerInvoiceReminder,
  postInvoiceReminderToLedger,
} from "../../core/invoice-reminders";
import {
  calculateInvoiceLateInterest,
  registerInvoiceLateInterest,
  postInvoiceLateInterestToLedger,
} from "../../core/invoice-interest";
import {
  calculateInvoiceLateCompensation,
  registerInvoiceLateCompensation,
  postInvoiceLateCompensationToLedger,
} from "../../core/invoice-compensation";
import {
  buildInvoiceList,
  findInvoices,
  buildOverdueInvoiceList,
  type InvoiceQueryStatus,
} from "../../core/invoice-list";
import { wrapCoreResult, errorEnvelope } from "../envelope";
import {
  withCompanyDb,
  withCompanyDbConfirmed,
  resolveIssuedInvoiceDocumentId,
} from "../helpers";

const invoicePayloadSchema = z
  .object({})
  .catchall(z.unknown())
  .describe("InvoicePayload — se examples/full-invoice.dk.json for schema");

const statusEnum = z
  .enum(["open", "paid", "credited", "refunded", "overpaid", "written_off", "overdue", "all"])
  .optional();

const docIdOrNumberSchema = {
  company: z.string().min(1),
  documentId: z.number().int().positive().optional(),
  invoiceNumber: z.string().optional(),
};

function notFoundEnvelope(args: { documentId?: number | null; invoiceNumber?: string | null }) {
  return errorEnvelope(
    `Could not resolve invoice: provide documentId or invoiceNumber (got documentId=${args.documentId ?? "-"}, invoiceNumber='${args.invoiceNumber ?? ""}')`,
  );
}

export function registerInvoiceTools(server: McpServer): void {
  // --------------------------------------------------------------- read tools

  server.registerTool(
    "invoice_validate",
    {
      title: "Validate invoice payload",
      description: "Validerer faktura-payload uden at gemme. Read-only.",
      inputSchema: { payload: invoicePayloadSchema },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    async ({ payload }) => {
      const result = validateInvoice(payload as InvoicePayload);
      return {
        content: [{ type: "text" as const, text: JSON.stringify(wrapCoreResult(result)) }],
        isError: !result.ok,
        structuredContent: wrapCoreResult(result),
      };
    },
  );

  server.registerTool(
    "invoice_status",
    {
      title: "Invoice status",
      description: "Viser åben saldo og status på en faktura. Read-only.",
      inputSchema: { ...docIdOrNumberSchema, asOf: z.string().optional() },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; documentId?: number; invoiceNumber?: string; asOf?: string }>(
      server,
      ({ db, args }) => {
        const id = resolveIssuedInvoiceDocumentId(db, args);
        if (!id) return notFoundEnvelope(args);
        const result = getInvoiceStatus(db, id, args.asOf);
        return wrapCoreResult(result);
      },
    ),
  );

  server.registerTool(
    "invoice_list",
    {
      title: "List invoices",
      description: "Lister udstedte fakturaer med filtre. Read-only.",
      inputSchema: {
        company: z.string().min(1),
        status: statusEnum,
        from: z.string().optional(),
        to: z.string().optional(),
        customerCvr: z.string().optional(),
        customer: z.string().optional(),
        invoiceNumber: z.string().optional(),
        minAmount: z.number().optional(),
        maxAmount: z.number().optional(),
        asOf: z.string().optional(),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{
      company: string;
      status?: InvoiceQueryStatus;
      from?: string;
      to?: string;
      customerCvr?: string;
      customer?: string;
      invoiceNumber?: string;
      minAmount?: number;
      maxAmount?: number;
      asOf?: string;
    }>(server, ({ db, args }) => {
      const result = buildInvoiceList(db, {
        status: args.status ?? "all",
        from: args.from,
        to: args.to,
        customerCvr: args.customerCvr,
        customer: args.customer,
        invoiceNumber: args.invoiceNumber,
        minAmount: args.minAmount,
        maxAmount: args.maxAmount,
        asOfDate: args.asOf,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_find",
    {
      title: "Find invoices",
      description: "Søger fakturaer på nummer, kunde eller beløb. Read-only.",
      inputSchema: {
        company: z.string().min(1),
        query: z.string().optional(),
        customer: z.string().optional(),
        invoiceNumber: z.string().optional(),
        amount: z.number().optional(),
        asOf: z.string().optional(),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{
      company: string;
      query?: string;
      customer?: string;
      invoiceNumber?: string;
      amount?: number;
      asOf?: string;
    }>(server, ({ db, args }) => {
      const result = findInvoices(db, {
        query: args.query,
        customer: args.customer,
        invoiceNumber: args.invoiceNumber,
        minAmount: args.amount,
        maxAmount: args.amount,
        asOfDate: args.asOf,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_overdue",
    {
      title: "List overdue invoices",
      description: "Lister forfaldne udstedte fakturaer som ikke er fuldt afregnet. Read-only.",
      inputSchema: {
        company: z.string().min(1),
        asOf: z.string().optional(),
        minDays: z.number().int().nonnegative().optional(),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; asOf?: string; minDays?: number }>(server, ({ db, args }) => {
      const result = buildOverdueInvoiceList(db, { asOfDate: args.asOf, minDays: args.minDays });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_interest_calc",
    {
      title: "Calculate invoice late interest",
      description: "Beregner morarente uden at registrere. Read-only.",
      inputSchema: {
        ...docIdOrNumberSchema,
        asOf: z.string().min(1),
        referenceRate: z.number(),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      asOf: string;
      referenceRate: number;
    }>(server, ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const result = calculateInvoiceLateInterest(db, {
        invoiceDocumentId: id,
        asOfDate: args.asOf,
        referenceRatePercent: args.referenceRate,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_compensation_calc",
    {
      title: "Calculate invoice late compensation",
      description: "Beregner kompensationskrav for sen betaling uden at registrere. Read-only.",
      inputSchema: {
        ...docIdOrNumberSchema,
        asOf: z.string().min(1),
        amountDkk: z.number().optional(),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      asOf: string;
      amountDkk?: number;
    }>(server, ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const result = calculateInvoiceLateCompensation(db, {
        invoiceDocumentId: id,
        asOfDate: args.asOf,
        compensationAmountDkk: args.amountDkk,
      });
      return wrapCoreResult(result);
    }),
  );

  // ------------------------------------------------------- write-irreversible

  server.registerTool(
    "invoice_issue",
    {
      title: "Issue invoice",
      description: "Udsteder kundefaktura + immutable snapshot. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        payload: invoicePayloadSchema,
        customerId: z.number().int().positive().optional(),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: InvoicePayload;
      customerId?: number;
      confirm: boolean;
    }>(server, "invoice_issue", ({ db, args }) => {
      const resolved = resolveInvoiceMasterData(db, args.payload, { customerId: args.customerId });
      if (!resolved.ok) return wrapCoreResult(resolved);
      const result = issueInvoice(db, args.company, resolved.payload);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_render",
    {
      title: "Render invoice PDF",
      description:
        "Renderer (eller genskaber) deterministisk PDF for udstedt faktura. write-irreversible.",
      inputSchema: { ...docIdOrNumberSchema, confirm: z.boolean() },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      confirm: boolean;
    }>(server, "invoice_render", ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const result = renderIssuedInvoicePdf(db, args.company, { invoiceDocumentId: id });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_credit_note",
    {
      title: "Issue credit note",
      description: "Udsteder kreditnota mod en eksisterende faktura. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        payload: z.object({}).catchall(z.unknown()),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; payload: IssueCreditNoteInput; confirm: boolean }>(
      server,
      "invoice_credit_note",
      ({ db, args }) => {
        const resolved = resolveOriginalInvoice(db, args.payload as Record<string, unknown>);
        const result = issueCreditNote(db, args.company, resolved as IssueCreditNoteInput);
        return wrapCoreResult(result);
      },
    ),
  );

  server.registerTool(
    "invoice_post",
    {
      title: "Post invoice to ledger",
      description: "Bogfører en udstedt faktura i finansen. write-irreversible.",
      inputSchema: { ...docIdOrNumberSchema, confirm: z.boolean() },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      confirm: boolean;
    }>(server, "invoice_post", ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const result = postIssuedInvoiceToLedger(db, { invoiceDocumentId: id });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_settle_bank",
    {
      title: "Settle invoice from bank",
      description: "Matcher en bankbetaling mod en faktura. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        payload: z.object({}).catchall(z.unknown()),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: SettleInvoiceFromBankInput & { invoiceNumber?: string };
      confirm: boolean;
    }>(server, "invoice_settle_bank", ({ db, args }) => {
      const resolved = resolveInvoiceInPayload(db, args.payload as Record<string, unknown>);
      const result = settleInvoiceFromBank(db, resolved as SettleInvoiceFromBankInput);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_settle_claim_bank",
    {
      title: "Settle invoice claims from bank",
      description: "Matcher en bankbetaling mod fakturakrav. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        payload: z.object({}).catchall(z.unknown()),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: SettleInvoiceClaimsFromBankInput & { invoiceNumber?: string };
      confirm: boolean;
    }>(server, "invoice_settle_claim_bank", ({ db, args }) => {
      const resolved = resolveInvoiceInPayload(db, args.payload as Record<string, unknown>);
      const result = settleInvoiceClaimsFromBank(db, resolved as SettleInvoiceClaimsFromBankInput);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_write_off_bad_debt",
    {
      title: "Write off bad debt",
      description: "Bogfører tab på debitor. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        payload: z.object({}).catchall(z.unknown()),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: WriteOffInvoiceBadDebtInput & { invoiceNumber?: string };
      confirm: boolean;
    }>(server, "invoice_write_off_bad_debt", ({ db, args }) => {
      const resolved = resolveInvoiceInPayload(db, args.payload as Record<string, unknown>);
      const result = writeOffInvoiceBadDebt(db, resolved as WriteOffInvoiceBadDebtInput);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_apply_payment",
    {
      title: "Apply invoice payment",
      description: "Registrerer fakturabetaling fra payload. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        payload: z.object({}).catchall(z.unknown()),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: ApplyInvoicePaymentInput & { invoiceNumber?: string };
      confirm: boolean;
    }>(server, "invoice_apply_payment", ({ db, args }) => {
      const resolved = resolveInvoiceInPayload(db, args.payload as Record<string, unknown>);
      const result = applyInvoicePayment(db, resolved as ApplyInvoicePaymentInput);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_refund_bank",
    {
      title: "Refund invoice to bank",
      description: "Bogfører refundering til kunde fra banken. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        payload: z.object({}).catchall(z.unknown()),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: RefundInvoiceToBankInput & { invoiceNumber?: string };
      confirm: boolean;
    }>(server, "invoice_refund_bank", ({ db, args }) => {
      const resolved = resolveInvoiceInPayload(db, args.payload as Record<string, unknown>);
      const result = refundInvoiceToBank(db, resolved as RefundInvoiceToBankInput);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_remind",
    {
      title: "Register invoice reminder",
      description: "Registrerer rykker på forfalden faktura. write-irreversible.",
      inputSchema: {
        ...docIdOrNumberSchema,
        date: z.string().min(1),
        fee: z.number().optional(),
        note: z.string().optional(),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      date: string;
      fee?: number;
      note?: string;
      confirm: boolean;
    }>(server, "invoice_remind", ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const result = registerInvoiceReminder(db, {
        invoiceDocumentId: id,
        reminderDate: args.date,
        feeAmount: args.fee,
        note: args.note,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_post_reminder",
    {
      title: "Post invoice reminder to ledger",
      description: "Bogfører en registreret rykker. write-irreversible.",
      inputSchema: {
        ...docIdOrNumberSchema,
        reminderId: z.number().int().positive().optional(),
        date: z.string().optional(),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      reminderId?: number;
      date?: string;
      confirm: boolean;
    }>(server, "invoice_post_reminder", ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const result = postInvoiceReminderToLedger(db, {
        invoiceDocumentId: id,
        reminderId: args.reminderId,
        transactionDate: args.date,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_claim_interest",
    {
      title: "Register late-interest claim",
      description: "Registrerer morarentekrav. write-irreversible.",
      inputSchema: {
        ...docIdOrNumberSchema,
        asOf: z.string().min(1),
        referenceRate: z.number(),
        note: z.string().optional(),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      asOf: string;
      referenceRate: number;
      note?: string;
      confirm: boolean;
    }>(server, "invoice_claim_interest", ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const result = registerInvoiceLateInterest(db, {
        invoiceDocumentId: id,
        asOfDate: args.asOf,
        referenceRatePercent: args.referenceRate,
        note: args.note,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_post_interest",
    {
      title: "Post late-interest claim to ledger",
      description: "Bogfører registreret morarentekrav. write-irreversible.",
      inputSchema: {
        ...docIdOrNumberSchema,
        claimId: z.number().int().positive().optional(),
        date: z.string().optional(),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      claimId?: number;
      date?: string;
      confirm: boolean;
    }>(server, "invoice_post_interest", ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const result = postInvoiceLateInterestToLedger(db, {
        invoiceDocumentId: id,
        claimId: args.claimId,
        transactionDate: args.date,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_claim_compensation",
    {
      title: "Register late-compensation claim",
      description: "Registrerer kompensationskrav (uden at bogføre). write-irreversible.",
      inputSchema: {
        ...docIdOrNumberSchema,
        asOf: z.string().min(1),
        amountDkk: z.number().optional(),
        note: z.string().optional(),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      asOf: string;
      amountDkk?: number;
      note?: string;
      confirm: boolean;
    }>(server, "invoice_claim_compensation", ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const result = registerInvoiceLateCompensation(db, {
        invoiceDocumentId: id,
        asOfDate: args.asOf,
        compensationAmountDkk: args.amountDkk,
        note: args.note,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_post_compensation",
    {
      title: "Post compensation claim to ledger",
      description: "Bogfører registreret kompensationskrav. write-irreversible.",
      inputSchema: {
        ...docIdOrNumberSchema,
        date: z.string().optional(),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      date?: string;
      confirm: boolean;
    }>(server, "invoice_post_compensation", ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const result = postInvoiceLateCompensationToLedger(db, {
        invoiceDocumentId: id,
        transactionDate: args.date,
      });
      return wrapCoreResult(result);
    }),
  );
}

// Resolve invoice document id inside a payload that may pass invoiceNumber instead.
function resolveInvoiceInPayload(
  db: import("bun:sqlite").Database,
  payload: Record<string, unknown>,
): Record<string, unknown> {
  if (
    payload.invoiceDocumentId === undefined ||
    payload.invoiceDocumentId === null ||
    payload.invoiceDocumentId === 0
  ) {
    const value = typeof payload.invoiceNumber === "string" ? payload.invoiceNumber.trim() : "";
    if (value) {
      const row = db
        .query(`SELECT id FROM documents WHERE document_type = 'issued_invoice' AND invoice_no = ? LIMIT 1`)
        .get(value) as { id: number } | null;
      if (row) return { ...payload, invoiceDocumentId: row.id };
    }
  }
  return payload;
}

function resolveOriginalInvoice(
  db: import("bun:sqlite").Database,
  payload: Record<string, unknown>,
): Record<string, unknown> {
  if (
    payload.originalInvoiceDocumentId === undefined ||
    payload.originalInvoiceDocumentId === null ||
    payload.originalInvoiceDocumentId === 0
  ) {
    const value =
      typeof payload.originalInvoiceNumber === "string" ? payload.originalInvoiceNumber.trim() : "";
    if (value) {
      const row = db
        .query(`SELECT id FROM documents WHERE document_type = 'issued_invoice' AND invoice_no = ? LIMIT 1`)
        .get(value) as { id: number } | null;
      if (row) return { ...payload, originalInvoiceDocumentId: row.id };
    }
  }
  return payload;
}
