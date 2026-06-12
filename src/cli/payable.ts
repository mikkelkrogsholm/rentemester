import { migrate } from "../core/db";
import {
  registerPayable,
  payPayableFromBank,
  buildPayablesList,
  type PayablesListRow,
} from "../core/payables";
import { openCommandDb } from "../cli-dispatch";
import { formatKroner } from "../cli-format";
import type { CommandDispatch } from "../cli-dispatch";

function renderPayablesListHuman(result: {
  count: number;
  asOfDate: string;
  totalOpenBalance: number;
  overdueOpenBalance: number;
  notYetDueOpenBalance: number;
  rows: PayablesListRow[];
}): void {
  console.log(`Kreditorliste (${result.count}) — pr. ${result.asOfDate}`);
  console.log(
    `  Åben saldo i alt: ${formatKroner(result.totalOpenBalance)} ` +
      `(forfaldne: ${formatKroner(result.overdueOpenBalance)}, ` +
      `ikke-forfaldne: ${formatKroner(result.notYetDueOpenBalance)})`,
  );
  if (result.rows.length === 0) {
    console.log("Ingen kreditorposter for det valgte filter.");
    return;
  }
  for (const row of result.rows) {
    console.log("");
    console.log(`#${row.payableId} — ${row.supplierName ?? "Ukendt leverandør"} | bilag ${row.billNo ?? row.documentId}`);
    console.log(`  Bilagsdato: ${row.billDate} | Forfald: ${row.dueDate}`);
    console.log(`  Brutto: ${formatKroner(row.grossAmount)} | Betalt: ${formatKroner(row.paidAmount)} | Åben: ${formatKroner(row.openBalance)}`);
    const aging = row.isOverdue
      ? `Forfalden ${row.overdueDays} dage (interval ${row.agingBucket})`
      : row.status === "paid"
        ? "Betalt"
        : "Ikke forfalden";
    console.log(`  Status: ${aging}`);
  }
}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("payable", "register", (ctx) => {
    const documentId = Number(ctx.arg("--document-id"));
    const billDate = ctx.arg("--bill-date");
    const dueDate = ctx.arg("--due-date");
    const expenseAccountNo = ctx.arg("--expense-account");
    if (
      !Number.isInteger(documentId) ||
      documentId <= 0 ||
      !billDate ||
      !dueDate ||
      !expenseAccountNo
    ) {
      console.error(
        "Missing required --document-id <n>, --bill-date <YYYY-MM-DD>, --due-date <YYYY-MM-DD>, or --expense-account <konto>",
      );
      process.exit(2);
    }
    const vendorIdRaw = ctx.arg("--vendor-id");
    const vendorId = vendorIdRaw === undefined ? undefined : Number(vendorIdRaw);
    if (vendorIdRaw !== undefined && (!Number.isInteger(vendorId) || (vendorId as number) <= 0)) {
      console.error("--vendor-id must be a positive integer when present");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const result = registerPayable(db, {
      documentId,
      billDate,
      dueDate,
      expenseAccountNo,
      vatTreatment: ctx.arg("--vat-treatment") as "standard" | "exempt" | undefined,
      vendorId,
      note: ctx.arg("--note") ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });

  dispatch.on("payable", "pay", (ctx) => {
    const payableId = Number(ctx.arg("--payable-id"));
    const bankTransactionId = Number(ctx.arg("--bank-transaction-id"));
    if (
      !Number.isInteger(payableId) ||
      payableId <= 0 ||
      !Number.isInteger(bankTransactionId) ||
      bankTransactionId <= 0
    ) {
      console.error("Missing required --payable-id <n> or --bank-transaction-id <n>");
      process.exit(2);
    }
    const amount = ctx.parseOptionalNumber("--amount");
    if (!amount.ok) ctx.fatal(amount.error);
    const db = openCommandDb(ctx);
    migrate(db);
    const result = payPayableFromBank(db, {
      payableId,
      bankTransactionId,
      paymentDate: ctx.arg("--date") ?? undefined,
      amount: amount.value === undefined ? undefined : Number(amount.value),
      paymentAccountNo: ctx.arg("--payment-account") ?? undefined,
      note: ctx.arg("--note") ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });

  dispatch.on("payable", "list", (ctx) => {
    const minDays = ctx.parseOptionalNumber("--min-days");
    const vendorId = ctx.parseOptionalNumber("--vendor-id");
    if (!minDays.ok) ctx.fatal(minDays.error);
    if (!vendorId.ok) ctx.fatal(vendorId.error);
    const db = openCommandDb(ctx);
    migrate(db);
    const result = buildPayablesList(db, {
      status: ctx.arg("--status") as "open" | "paid" | "overdue" | "all" | undefined,
      asOfDate: ctx.arg("--as-of") ?? undefined,
      supplier: ctx.arg("--supplier") ?? undefined,
      from: ctx.arg("--from") ?? undefined,
      to: ctx.arg("--to") ?? undefined,
      vendorId: vendorId.value === undefined ? undefined : Number(vendorId.value),
      minDays: minDays.value === undefined ? undefined : Number(minDays.value),
    });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else if (result.ok) {
      renderPayablesListHuman(result);
    } else {
      console.error(result.errors.join("\n"));
    }
    db.close();
    if (!result.ok) process.exit(1);
  });
}
