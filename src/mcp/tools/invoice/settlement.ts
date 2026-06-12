/**
 * Settlement-side invoice_* MCP tools: invoice_settle_bank,
 * invoice_settle_claim_bank, invoice_write_off_bad_debt,
 * invoice_apply_payment, invoice_refund_bank.
 *
 * Split out of `../invoice.ts` (Batch G). Registration order preserved.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { settleInvoiceFromBank, type SettleInvoiceFromBankInput } from "../../../core/invoice-settlement";
import {
  settleInvoiceClaimsFromBank,
  type SettleInvoiceClaimsFromBankInput,
} from "../../../core/invoice-claim-settlement";
import { writeOffInvoiceBadDebt, type WriteOffInvoiceBadDebtInput } from "../../../core/invoice-bad-debt";
import {
  applyInvoicePayment,
  type ApplyInvoicePaymentInput,
} from "../../../core/invoice-payments";
import { refundInvoiceToBank, type RefundInvoiceToBankInput } from "../../../core/invoice-refunds";
import { withActor } from "../../actor";
import { envelopeShape, wrapCoreResult } from "../../envelope";
import {
  withCompanyDbConfirmed,
  confirmField,
} from "../../tool-runtime";
import { requireInvoicePostedEnvelope } from "./_shared";

// --- bank-settlement family --------------------------------------------------
// invoice_settle_bank, invoice_settle_claim_bank and invoice_refund_bank share
// the same shape: they match an existing bank transaction against an invoice.
const bankSettlementPayloadSchema = z
  .object({
    invoiceDocumentId: z
      .number()
      .int()
      .positive()
      .optional()
      .describe("Document ID of the invoice. Provide this OR invoiceNumber."),
    invoiceNumber: z
      .string()
      .optional()
      .describe("Invoice number. Provide this OR invoiceDocumentId."),
    bankTransactionId: z
      .number()
      .int()
      .positive()
      .optional()
      .describe("ID of the bank transaction to match. Provide this OR bankTransactionReference. See bank_list."),
    bankTransactionReference: z
      .string()
      .optional()
      .describe("Reference of the bank transaction to match. Provide this OR bankTransactionId."),
    paymentDate: z
      .string()
      .optional()
      .describe("Payment/settlement date in YYYY-MM-DD format. Defaults to the bank transaction's date."),
    amount: z
      .number()
      .optional()
      .describe(
        "Amount to settle, in kroner (decimal DKK, 2 decimals — NOT øre). Defaults to the " +
          "full bank-transaction amount.",
      ),
    bankAccountNo: z
      .string()
      .optional()
      .describe("Optional bank account number from the chart of accounts to post against."),
    receivableAccountNo: z
      .string()
      .optional()
      .describe("Optional accounts-receivable account number from the chart of accounts."),
  })
  .describe("Bank-settlement payload. amount is in kroner (decimal DKK, 2 decimals — NOT øre).");

const refundBankPayloadSchema = bankSettlementPayloadSchema
  .extend({
    refundDate: z
      .string()
      .optional()
      .describe("Refund date in YYYY-MM-DD format. Defaults to the bank transaction's date."),
  })
  .describe("Refund-to-bank payload. amount is in kroner (decimal DKK, 2 decimals — NOT øre).");

// --- bad-debt write-off ------------------------------------------------------
const badDebtPayloadSchema = z
  .object({
    invoiceDocumentId: z
      .number()
      .int()
      .positive()
      .optional()
      .describe("Document ID of the invoice. Provide this OR invoiceNumber."),
    invoiceNumber: z
      .string()
      .optional()
      .describe("Invoice number. Provide this OR invoiceDocumentId."),
    writeOffDate: z.string().describe("Write-off date in YYYY-MM-DD format."),
    grossAmount: z
      .number()
      .optional()
      .describe(
        "Amount to write off including VAT, in kroner (decimal DKK, 2 decimals — NOT øre). " +
          "Defaults to the full open principal balance; may not exceed it.",
      ),
    expenseAccountNo: z
      .string()
      .optional()
      .describe("Optional bad-debt expense account number from the chart of accounts."),
    receivableAccountNo: z
      .string()
      .optional()
      .describe("Optional accounts-receivable account number from the chart of accounts."),
    vatAccountNo: z
      .string()
      .optional()
      .describe("Optional VAT account number from the chart of accounts for the VAT relief."),
    note: z.string().optional().describe("Optional free-text note."),
  })
  .describe("Bad-debt write-off payload. grossAmount is in kroner (decimal DKK, 2 decimals — NOT øre).");

// --- apply payment -----------------------------------------------------------
const applyPaymentPayloadSchema = z
  .object({
    invoiceDocumentId: z
      .number()
      .int()
      .positive()
      .optional()
      .describe("Document ID of the invoice. Provide this OR invoiceNumber."),
    invoiceNumber: z
      .string()
      .optional()
      .describe("Invoice number. Provide this OR invoiceDocumentId."),
    paymentDate: z.string().describe("Payment date in YYYY-MM-DD format."),
    amount: z
      .number()
      .describe("Payment amount, in kroner (decimal DKK, 2 decimals — NOT øre)."),
    bankTransactionId: z
      .number()
      .int()
      .positive()
      .optional()
      .describe("Optional ID of the linked bank transaction. See bank_list."),
    journalEntryId: z
      .number()
      .int()
      .positive()
      .optional()
      .describe("Optional ID of an existing journal entry to link the payment to."),
    bankAccountNo: z
      .string()
      .optional()
      .describe("Optional bank account number from the chart of accounts to post against."),
    receivableAccountNo: z
      .string()
      .optional()
      .describe("Optional accounts-receivable account number from the chart of accounts."),
    note: z.string().optional().describe("Optional free-text note."),
  })
  .describe("Apply-payment payload. amount is in kroner (decimal DKK, 2 decimals — NOT øre).");

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

export function registerInvoiceSettlementTools(server: McpServer): void {
  server.registerTool(
    "invoice_settle_bank",
    {
      title: "Settle invoice from bank",
      description:
        "Matcher en bankbetaling mod en faktura (debit 2000 Bank, credit 1100 Debitorer). " +
        "Forudsætning: fakturaen skal være bogført med invoice_post — ellers er der intet åbent tilgodehavende at modregne. " +
        "payload.amount er i kroner (decimal DKK, ikke øre). write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        payload: bankSettlementPayloadSchema,
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: SettleInvoiceFromBankInput & { invoiceNumber?: string };
      confirm?: boolean;
    }>(server, "invoice_settle_bank", ({ db, actor, args }) => {
      const resolved = resolveInvoiceInPayload(db, args.payload as Record<string, unknown>);
      const docId = Number((resolved as { invoiceDocumentId?: unknown }).invoiceDocumentId);
      if (Number.isInteger(docId) && docId > 0) {
        const blocked = requireInvoicePostedEnvelope(db, docId, "invoice_settle_bank");
        if (blocked) return blocked;
      }
      // Actor-invariant (#63/#76): attribute the settlement entry to the
      // booking agent in the hash chain + audit_log, not the OS user.
      const result = settleInvoiceFromBank(db, withActor(resolved as SettleInvoiceFromBankInput, actor));
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_settle_claim_bank",
    {
      title: "Settle invoice claims from bank",
      description:
        "Matcher en bankbetaling mod fakturakrav (rykkergebyr, morarente, kompensation). " +
        "Forudsætning: fakturaen skal være bogført med invoice_post, og de krav der modregnes skal være registreret og bogført (invoice_post_reminder / invoice_post_interest / invoice_post_compensation). " +
        "payload.amount er i kroner (decimal DKK, ikke øre). write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        payload: bankSettlementPayloadSchema,
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: SettleInvoiceClaimsFromBankInput & { invoiceNumber?: string };
      confirm?: boolean;
    }>(server, "invoice_settle_claim_bank", ({ db, actor, args }) => {
      const resolved = resolveInvoiceInPayload(db, args.payload as Record<string, unknown>);
      const docId = Number((resolved as { invoiceDocumentId?: unknown }).invoiceDocumentId);
      if (Number.isInteger(docId) && docId > 0) {
        const blocked = requireInvoicePostedEnvelope(db, docId, "invoice_settle_claim_bank");
        if (blocked) return blocked;
      }
      // Actor-invariant (#63/#76): attribute the claim-settlement entry to the
      // booking agent in the hash chain + audit_log, not the OS user.
      const result = settleInvoiceClaimsFromBank(
        db,
        withActor(resolved as SettleInvoiceClaimsFromBankInput, actor),
      );
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_write_off_bad_debt",
    {
      title: "Write off bad debt",
      description:
        "Bogfører tab på debitor. " +
        "Forudsætning: fakturaen skal være bogført med invoice_post — kun et bogført tilgodehavende kan afskrives som tab. " +
        "payload.grossAmount er i kroner (decimal DKK, ikke øre). write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        payload: badDebtPayloadSchema,
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: WriteOffInvoiceBadDebtInput & { invoiceNumber?: string };
      confirm?: boolean;
    }>(server, "invoice_write_off_bad_debt", ({ db, actor, args }) => {
      const resolved = resolveInvoiceInPayload(db, args.payload as Record<string, unknown>);
      const docId = Number((resolved as { invoiceDocumentId?: unknown }).invoiceDocumentId);
      if (Number.isInteger(docId) && docId > 0) {
        const blocked = requireInvoicePostedEnvelope(db, docId, "invoice_write_off_bad_debt");
        if (blocked) return blocked;
      }
      // Actor-invariant (#63/#76): attribute the bad-debt write-off entry to the
      // booking agent in the hash chain + audit_log, not the OS user.
      const result = writeOffInvoiceBadDebt(db, withActor(resolved as WriteOffInvoiceBadDebtInput, actor));
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_apply_payment",
    {
      title: "Apply invoice payment",
      description:
        "Registrerer fakturabetaling fra payload (uden bank-match — typisk til at lukke betalinger der allerede er konteret separat). " +
        "Forudsætning: fakturaen skal være bogført med invoice_post — der skal være et åbent tilgodehavende at lukke. " +
        "payload.amount er i kroner (decimal DKK, ikke øre). write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        payload: applyPaymentPayloadSchema,
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: ApplyInvoicePaymentInput & { invoiceNumber?: string };
      confirm?: boolean;
    }>(server, "invoice_apply_payment", ({ db, actor, args }) => {
      const resolved = resolveInvoiceInPayload(db, args.payload as Record<string, unknown>);
      const docId = Number((resolved as { invoiceDocumentId?: unknown }).invoiceDocumentId);
      if (Number.isInteger(docId) && docId > 0) {
        const blocked = requireInvoicePostedEnvelope(db, docId, "invoice_apply_payment");
        if (blocked) return blocked;
      }
      // Actor-invariant (#63/#76): attribute the payment entry to the booking
      // agent in the hash chain + audit_log, not the OS user.
      const result = applyInvoicePayment(db, withActor(resolved as ApplyInvoicePaymentInput, actor));
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_refund_bank",
    {
      title: "Refund invoice to bank",
      description:
        "Bogfører refundering til kunde fra banken (credit 2000 Bank, debit 1100 Debitorer). " +
        "Forudsætning: fakturaen skal være bogført med invoice_post (typisk efter en kreditnota via invoice_credit_note som har skabt overskydende betaling). " +
        "payload.amount er i kroner (decimal DKK, ikke øre). write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        payload: refundBankPayloadSchema,
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: RefundInvoiceToBankInput & { invoiceNumber?: string };
      confirm?: boolean;
    }>(server, "invoice_refund_bank", ({ db, actor, args }) => {
      const resolved = resolveInvoiceInPayload(db, args.payload as Record<string, unknown>);
      const docId = Number((resolved as { invoiceDocumentId?: unknown }).invoiceDocumentId);
      if (Number.isInteger(docId) && docId > 0) {
        const blocked = requireInvoicePostedEnvelope(db, docId, "invoice_refund_bank");
        if (blocked) return blocked;
      }
      // Actor-invariant (#63/#76): attribute the refund entry to the booking
      // agent in the hash chain + audit_log, not the OS user.
      const result = refundInvoiceToBank(db, withActor(resolved as RefundInvoiceToBankInput, actor));
      return wrapCoreResult(result);
    }),
  );
}
