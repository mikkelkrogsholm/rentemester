/**
 * Reminder-side `invoice <subcommand>` CLI handlers:
 *   remind, post-reminder.
 *
 * Split out of `../invoice.ts`. Registration order preserved.
 */

import { openCommandDb } from "../../cli-dispatch";
import { migrate } from "../../core/db";
import { postInvoiceReminderToLedger, registerInvoiceReminder } from "../../core/invoice-reminders";
import type { CommandDispatch } from "../../cli-dispatch";
import { resolveInvoiceDocumentId } from "./_shared";

export function registerReminderCommands(dispatch: CommandDispatch): void {
  dispatch.on("invoice", "remind", (ctx) => {
    const reminderDate = ctx.arg("--date");
    const feeArg = ctx.arg("--fee");
    const feeAmount = feeArg === undefined ? undefined : Number(feeArg);
    const note = ctx.arg("--note");
    if (!reminderDate || (feeArg !== undefined && Number.isNaN(feeAmount))) {
      console.error(
        "Missing required --date <YYYY-MM-DD>; optional --fee <n> must be numeric when present",
      );
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = registerInvoiceReminder(db, {
      invoiceDocumentId: documentId,
      reminderDate,
      feeAmount,
      note: note ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "post-reminder", (ctx) => {
    const reminderIdArg = ctx.arg("--reminder-id");
    const reminderId = reminderIdArg === undefined ? undefined : Number(reminderIdArg);
    const transactionDate = ctx.arg("--date");
    if (
      reminderIdArg !== undefined &&
      (!Number.isInteger(reminderId) || (reminderId as number) <= 0)
    ) {
      console.error("Optional --reminder-id <n> must be a positive integer when present");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = postInvoiceReminderToLedger(db, {
      invoiceDocumentId: documentId,
      reminderId,
      transactionDate: transactionDate ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });
}
