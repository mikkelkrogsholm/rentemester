/**
 * Settlement-side `invoice <subcommand>` CLI handlers:
 *   settle-bank, settle-claim-bank, write-off-bad-debt, apply-payment,
 *   refund-bank.
 *
 * Split out of `../invoice.ts`. Registration order preserved.
 */

import { readJsonCliInput, openCommandDb } from "../../cli-dispatch";
import { migrate } from "../../core/db";
import { applyInvoicePayment } from "../../core/invoice-payments";
import { settleInvoiceFromBank } from "../../core/invoice-settlement";
import { settleInvoiceClaimsFromBank } from "../../core/invoice-claim-settlement";
import { refundInvoiceToBank } from "../../core/invoice-refunds";
import { writeOffInvoiceBadDebt } from "../../core/invoice-bad-debt";
import type { CommandDispatch } from "../../cli-dispatch";
import { withResolvedInvoicePayload } from "./_shared";

export function registerSettlementCommands(dispatch: CommandDispatch): void {
  dispatch.on("invoice", "settle-bank", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      readJsonCliInput(ctx, input, "--input"),
      "invoiceDocumentId",
      "invoiceNumber",
    );
    const result = settleInvoiceFromBank(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "settle-claim-bank", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      readJsonCliInput(ctx, input, "--input"),
      "invoiceDocumentId",
      "invoiceNumber",
    );
    const result = settleInvoiceClaimsFromBank(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "write-off-bad-debt", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      readJsonCliInput(ctx, input, "--input"),
      "invoiceDocumentId",
      "invoiceNumber",
    );
    const result = writeOffInvoiceBadDebt(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "apply-payment", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      readJsonCliInput(ctx, input, "--input"),
      "invoiceDocumentId",
      "invoiceNumber",
    );
    const result = applyInvoicePayment(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "refund-bank", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      readJsonCliInput(ctx, input, "--input"),
      "invoiceDocumentId",
      "invoiceNumber",
    );
    const result = refundInvoiceToBank(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });
}
