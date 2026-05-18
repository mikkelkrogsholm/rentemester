import { readFileSync } from "node:fs";
import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { validateInvoice } from "../core/invoice";
import { issueInvoice } from "../core/issued-invoices";
import { renderIssuedInvoicePdf } from "../core/invoice-pdf";
import { applyInvoicePayment, getInvoiceStatus } from "../core/invoice-payments";
import { buildInvoiceList, buildOverdueInvoiceList, findInvoices } from "../core/invoice-list";
import { postIssuedInvoiceToLedger } from "../core/invoice-booking";
import { settleInvoiceFromBank } from "../core/invoice-settlement";
import { settleInvoiceClaimsFromBank } from "../core/invoice-claim-settlement";
import { issueCreditNote } from "../core/credit-notes";
import { refundInvoiceToBank } from "../core/invoice-refunds";
import {
  calculateInvoiceLateInterest,
  postInvoiceLateInterestToLedger,
  registerInvoiceLateInterest,
} from "../core/invoice-interest";
import {
  calculateInvoiceLateCompensation,
  postInvoiceLateCompensationToLedger,
  registerInvoiceLateCompensation,
} from "../core/invoice-compensation";
import { postInvoiceReminderToLedger, registerInvoiceReminder } from "../core/invoice-reminders";
import { writeOffInvoiceBadDebt } from "../core/invoice-bad-debt";
import { resolveInvoiceMasterData } from "../core/master-data";
import type { CommandDispatch, CommandContext } from "../cli-dispatch";

type Db = ReturnType<typeof openDb>;

function resolveInvoiceDocumentId(db: Db, ctx: CommandContext): number | null {
  const documentId = Number(ctx.arg("--document-id"));
  if (Number.isInteger(documentId) && documentId > 0) return documentId;
  const invoiceNumber = ctx.arg("--invoice-number")?.trim();
  if (invoiceNumber) {
    const row = db
      .query(
        `SELECT id FROM documents WHERE document_type = 'issued_invoice' AND invoice_no = ? LIMIT 1`,
      )
      .get(invoiceNumber) as { id: number } | null;
    return row?.id ?? null;
  }
  return null;
}

function findInvoiceDocumentIdByNumber(db: Db, invoiceNumber?: string | null): number | null {
  const value = invoiceNumber?.trim();
  if (!value) return null;
  const row = db
    .query(
      `SELECT id FROM documents WHERE document_type = 'issued_invoice' AND invoice_no = ? LIMIT 1`,
    )
    .get(value) as { id: number } | null;
  return row?.id ?? null;
}

function withResolvedInvoicePayload<T extends Record<string, unknown>>(
  db: Db,
  payload: T,
  idKey: keyof T,
  numberKey: keyof T,
): T {
  if (Number.isInteger(payload[idKey] as number) && Number(payload[idKey]) > 0) return payload;
  const resolved = findInvoiceDocumentIdByNumber(db, payload[numberKey] as string | undefined);
  if (!resolved) return payload;
  return { ...payload, [idKey]: resolved };
}

function renderInvoiceRowsHuman(title: string, rows: any[], emptyMessage: string): void {
  console.log(title);
  if (rows.length === 0) {
    console.log(emptyMessage);
    return;
  }
  console.table(
    rows.map((row) => ({
      invoiceNumber: row.invoiceNumber,
      customerName: row.customerName,
      invoiceDate: row.invoiceDate,
      effectiveDueDate: row.effectiveDueDate,
      grossAmount: row.grossAmount,
      openBalance: row.openBalance,
      status: row.status,
      overdueDays: row.overdueDays,
    })),
  );
}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("invoice", "validate", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const payload = JSON.parse(readFileSync(input, "utf8"));
    const result = validateInvoice(payload);
    ctx.emitResult(result as Record<string, unknown>);
  });

  dispatch.on("invoice", "issue", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const root = ctx.companyRoot();
    const db = openDb(companyPaths(root).db);
    migrate(db);
    const payload = JSON.parse(readFileSync(input, "utf8"));
    const customerIdRaw = ctx.arg("--customer-id");
    const customerId = customerIdRaw === undefined ? undefined : Number(customerIdRaw);
    const resolved = resolveInvoiceMasterData(db, payload, {
      customerId: Number.isInteger(customerId) && customerId > 0 ? customerId : undefined,
    });
    if (!resolved.ok) {
      ctx.emitResult(resolved as Record<string, unknown>);
      db.close();
      process.exit(1);
    }
    const result = issueInvoice(db, root, resolved.payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "render", (ctx) => {
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    if (!documentId) {
      console.error("Missing required --document-id <n> or --invoice-number <no>");
      process.exit(2);
    }
    const result = renderIssuedInvoicePdf(db, ctx.companyRoot(), { invoiceDocumentId: documentId });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "credit-note", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const root = ctx.companyRoot();
    const db = openDb(companyPaths(root).db);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      JSON.parse(readFileSync(input, "utf8")),
      "originalInvoiceDocumentId",
      "originalInvoiceNumber",
    );
    const result = issueCreditNote(db, root, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "post", (ctx) => {
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    if (!documentId) {
      console.error("Missing required --document-id <n> or --invoice-number <no>");
      process.exit(2);
    }
    const result = postIssuedInvoiceToLedger(db, { invoiceDocumentId: documentId });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "settle-bank", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      JSON.parse(readFileSync(input, "utf8")),
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
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      JSON.parse(readFileSync(input, "utf8")),
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
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      JSON.parse(readFileSync(input, "utf8")),
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
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      JSON.parse(readFileSync(input, "utf8")),
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
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      JSON.parse(readFileSync(input, "utf8")),
      "invoiceDocumentId",
      "invoiceNumber",
    );
    const result = refundInvoiceToBank(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "remind", (ctx) => {
    const reminderDate = ctx.arg("--date");
    const feeArg = ctx.arg("--fee");
    const feeAmount = feeArg === undefined ? undefined : Number(feeArg);
    const note = ctx.arg("--note");
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    if (!documentId || !reminderDate || (feeArg !== undefined && Number.isNaN(feeAmount))) {
      console.error(
        "Missing required --document-id <n> or --invoice-number <no> or --date <YYYY-MM-DD>; optional --fee <n> must be numeric when present",
      );
      process.exit(2);
    }
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
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    if (
      !documentId ||
      (reminderIdArg !== undefined && (!Number.isInteger(reminderId) || (reminderId as number) <= 0))
    ) {
      console.error(
        "Missing required --document-id <n> or --invoice-number <no>; optional --reminder-id <n> must be a positive integer when present",
      );
      process.exit(2);
    }
    const result = postInvoiceReminderToLedger(db, {
      invoiceDocumentId: documentId,
      reminderId,
      transactionDate: transactionDate ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "status", (ctx) => {
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    if (!documentId) {
      console.error("Missing required --document-id <n> or --invoice-number <no>");
      process.exit(2);
    }
    const result = getInvoiceStatus(db, documentId, ctx.arg("--as-of"));
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "list", (ctx) => {
    const minAmount = ctx.parseOptionalNumber("--min-amount");
    const maxAmount = ctx.parseOptionalNumber("--max-amount");
    if (!minAmount.ok) ctx.fatal(minAmount.error);
    if (!maxAmount.ok) ctx.fatal(maxAmount.error);
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = buildInvoiceList(db, {
      status: (ctx.arg("--status") as any) ?? "all",
      from: ctx.arg("--from") ?? undefined,
      to: ctx.arg("--to") ?? undefined,
      customerCvr: ctx.arg("--customer-cvr") ?? undefined,
      customer: ctx.arg("--customer") ?? undefined,
      invoiceNumber: ctx.arg("--invoice-number") ?? undefined,
      minAmount: minAmount.value,
      maxAmount: maxAmount.value,
      asOfDate: ctx.arg("--as-of") ?? undefined,
    });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else {
      renderInvoiceRowsHuman(`Invoices (${result.count})`, result.rows, "No invoices for current filter.");
    }
    db.close();
  });

  dispatch.on("invoice", "find", (ctx) => {
    const amount = ctx.parseOptionalNumber("--amount");
    if (!amount.ok) ctx.fatal(amount.error);
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = findInvoices(db, {
      query: ctx.parsedArgs.positionals.slice(2).join(" ") || undefined,
      customer: ctx.arg("--customer") ?? undefined,
      invoiceNumber: ctx.arg("--invoice-number") ?? undefined,
      minAmount: amount.value,
      maxAmount: amount.value,
      asOfDate: ctx.arg("--as-of") ?? undefined,
    });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else {
      renderInvoiceRowsHuman(`Invoice matches (${result.count})`, result.rows, "No invoices matched the query.");
    }
    db.close();
  });

  dispatch.on("invoice", "overdue", (ctx) => {
    const minDays = ctx.parseOptionalNumber("--min-days");
    if (!minDays.ok) ctx.fatal(minDays.error);
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = buildOverdueInvoiceList(db, {
      asOfDate: ctx.arg("--as-of") ?? undefined,
      minDays: minDays.value,
    });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else {
      renderInvoiceRowsHuman(
        `Overdue invoices as of ${result.asOfDate ?? "today"} (${result.count})`,
        result.rows,
        "No overdue invoices for current filter.",
      );
    }
    db.close();
  });

  dispatch.on("invoice", "interest", (ctx) => {
    const asOfDate = ctx.arg("--as-of");
    const referenceRatePercent = Number(ctx.arg("--reference-rate"));
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    if (!documentId || !asOfDate || Number.isNaN(referenceRatePercent)) {
      console.error(
        "Missing required --document-id <n> or --invoice-number <no>, --as-of <YYYY-MM-DD>, or --reference-rate <pct>",
      );
      process.exit(2);
    }
    const result = calculateInvoiceLateInterest(db, {
      invoiceDocumentId: documentId,
      asOfDate,
      referenceRatePercent,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "claim-interest", (ctx) => {
    const asOfDate = ctx.arg("--as-of");
    const rateArg = ctx.arg("--reference-rate");
    const note = ctx.arg("--note");
    const referenceRatePercent = rateArg === undefined ? NaN : Number(rateArg);
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    if (!documentId || !asOfDate || Number.isNaN(referenceRatePercent)) {
      console.error(
        "Missing required --document-id <n> or --invoice-number <no>, --as-of <YYYY-MM-DD>, or --reference-rate <pct>",
      );
      process.exit(2);
    }
    const result = registerInvoiceLateInterest(db, {
      invoiceDocumentId: documentId,
      asOfDate,
      referenceRatePercent,
      note: note ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "post-interest", (ctx) => {
    const claimIdArg = ctx.arg("--claim-id");
    const claimId = claimIdArg === undefined ? undefined : Number(claimIdArg);
    const transactionDate = ctx.arg("--date");
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    if (
      !documentId ||
      (claimIdArg !== undefined && (!Number.isInteger(claimId) || (claimId as number) <= 0))
    ) {
      console.error(
        "Missing required --document-id <n> or --invoice-number <no>; optional --claim-id <n> must be a positive integer when present",
      );
      process.exit(2);
    }
    const result = postInvoiceLateInterestToLedger(db, {
      invoiceDocumentId: documentId,
      claimId,
      transactionDate: transactionDate ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "compensation", (ctx) => {
    const asOfDate = ctx.arg("--as-of");
    const amountArg = ctx.arg("--amount-dkk");
    const compensationAmountDkk = amountArg === undefined ? undefined : Number(amountArg);
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    if (
      !documentId ||
      !asOfDate ||
      (amountArg !== undefined && Number.isNaN(compensationAmountDkk))
    ) {
      console.error(
        "Missing required --document-id <n> or --invoice-number <no> or --as-of <YYYY-MM-DD>; optional --amount-dkk <n> must be numeric when present",
      );
      process.exit(2);
    }
    const result = calculateInvoiceLateCompensation(db, {
      invoiceDocumentId: documentId,
      asOfDate,
      compensationAmountDkk,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "claim-compensation", (ctx) => {
    const asOfDate = ctx.arg("--as-of");
    const amountArg = ctx.arg("--amount-dkk");
    const note = ctx.arg("--note");
    const compensationAmountDkk = amountArg === undefined ? undefined : Number(amountArg);
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    if (
      !documentId ||
      !asOfDate ||
      (amountArg !== undefined && Number.isNaN(compensationAmountDkk))
    ) {
      console.error(
        "Missing required --document-id <n> or --invoice-number <no> or --as-of <YYYY-MM-DD>; optional --amount-dkk must be numeric when present",
      );
      process.exit(2);
    }
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
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    if (!documentId) {
      console.error("Missing required --document-id <n> or --invoice-number <no>");
      process.exit(2);
    }
    const result = postInvoiceLateCompensationToLedger(db, {
      invoiceDocumentId: documentId,
      transactionDate: transactionDate ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });
}
