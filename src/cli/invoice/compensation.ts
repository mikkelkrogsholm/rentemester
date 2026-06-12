/**
 * Compensation-side `invoice <subcommand>` CLI handlers:
 *   compensation, claim-compensation, post-compensation.
 *
 * Split out of `../invoice.ts`. Registration order preserved.
 */

import { openCommandDb } from "../../cli-dispatch";
import { migrate } from "../../core/db";
import {
  calculateInvoiceLateCompensation,
  postInvoiceLateCompensationToLedger,
  registerInvoiceLateCompensation,
} from "../../core/invoice-compensation";
import type { CommandDispatch } from "../../cli-dispatch";
import { emitHumanReport } from "../../cli-format";
import { resolveInvoiceDocumentId } from "./_shared";

export function registerCompensationCommands(dispatch: CommandDispatch): void {
  dispatch.on("invoice", "compensation", (ctx) => {
    const asOfDate = ctx.arg("--as-of");
    const amountArg = ctx.arg("--amount-dkk");
    const compensationAmountDkk = amountArg === undefined ? undefined : Number(amountArg);
    if (!asOfDate || (amountArg !== undefined && Number.isNaN(compensationAmountDkk))) {
      console.error(
        "Missing required --as-of <YYYY-MM-DD>; optional --amount-dkk <n> must be numeric when present",
      );
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = calculateInvoiceLateCompensation(db, {
      invoiceDocumentId: documentId,
      asOfDate,
      compensationAmountDkk,
    });
    // #250: render the compensation assessment — eligibility, amount and a
    // clear reason — in Danish; `--format json` is unchanged.
    emitHumanReport("invoice-compensation", result as Record<string, unknown>, ctx.outputFormat);
    db.close();
  });

  dispatch.on("invoice", "claim-compensation", (ctx) => {
    const asOfDate = ctx.arg("--as-of");
    const amountArg = ctx.arg("--amount-dkk");
    const note = ctx.arg("--note");
    const compensationAmountDkk = amountArg === undefined ? undefined : Number(amountArg);
    if (!asOfDate || (amountArg !== undefined && Number.isNaN(compensationAmountDkk))) {
      console.error(
        "Missing required --as-of <YYYY-MM-DD>; optional --amount-dkk must be numeric when present",
      );
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = registerInvoiceLateCompensation(db, {
      invoiceDocumentId: documentId,
      asOfDate,
      compensationAmountDkk,
      note: note ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "post-compensation", (ctx) => {
    const transactionDate = ctx.arg("--date");
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = postInvoiceLateCompensationToLedger(db, {
      invoiceDocumentId: documentId,
      transactionDate: transactionDate ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });
}
