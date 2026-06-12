/**
 * Interest-side `invoice <subcommand>` CLI handlers:
 *   interest, claim-interest, post-interest.
 *
 * Split out of `../invoice.ts`. Registration order preserved.
 */

import { openCommandDb } from "../../cli-dispatch";
import { migrate } from "../../core/db";
import {
  calculateInvoiceLateInterest,
  postInvoiceLateInterestToLedger,
  registerInvoiceLateInterest,
  proposeInterestCorrection,
  postInterestCorrection,
} from "../../core/invoice-interest";
import type { CommandDispatch } from "../../cli-dispatch";
import { emitHumanReport } from "../../cli-format";
import { resolveInvoiceDocumentId } from "./_shared";

export function registerInterestCommands(dispatch: CommandDispatch): void {
  dispatch.on("invoice", "interest", (ctx) => {
    const asOfDate = ctx.arg("--as-of");
    const referenceRatePercent = Number(ctx.arg("--reference-rate"));
    if (!asOfDate || Number.isNaN(referenceRatePercent)) {
      console.error("Missing required --as-of <YYYY-MM-DD> or --reference-rate <pct>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = calculateInvoiceLateInterest(db, {
      invoiceDocumentId: documentId,
      asOfDate,
      referenceRatePercent,
    });
    // #250: render the late-interest figures (reference rate, statutory annual
    // rate, overdue window, computed amount) in Danish; `--format json` is
    // unchanged.
    emitHumanReport("invoice-interest", result as Record<string, unknown>, ctx.outputFormat);
    db.close();
  });

  dispatch.on("invoice", "claim-interest", (ctx) => {
    const asOfDate = ctx.arg("--as-of");
    const rateArg = ctx.arg("--reference-rate");
    const note = ctx.arg("--note");
    const referenceRatePercent = rateArg === undefined ? NaN : Number(rateArg);
    if (!asOfDate || Number.isNaN(referenceRatePercent)) {
      console.error("Missing required --as-of <YYYY-MM-DD> or --reference-rate <pct>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = registerInvoiceLateInterest(db, {
      invoiceDocumentId: documentId,
      asOfDate,
      referenceRatePercent,
      note: note ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "interest-correction", (ctx) => {
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = proposeInterestCorrection(db, { invoiceDocumentId: documentId });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "post-interest-correction", (ctx) => {
    const transactionDate = ctx.arg("--date");
    const reason = ctx.arg("--reason");
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = postInterestCorrection(db, {
      invoiceDocumentId: documentId,
      transactionDate: transactionDate ?? undefined,
      reason: reason ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "post-interest", (ctx) => {
    const claimIdArg = ctx.arg("--claim-id");
    const claimId = claimIdArg === undefined ? undefined : Number(claimIdArg);
    const transactionDate = ctx.arg("--date");
    if (
      claimIdArg !== undefined &&
      (!Number.isInteger(claimId) || (claimId as number) <= 0)
    ) {
      console.error("Optional --claim-id <n> must be a positive integer when present");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = postInvoiceLateInterestToLedger(db, {
      invoiceDocumentId: documentId,
      claimId,
      transactionDate: transactionDate ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });
}
