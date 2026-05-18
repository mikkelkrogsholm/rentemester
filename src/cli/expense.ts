import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { bookExpenseFromBank } from "../core/expense-booking";
import type { CommandDispatch } from "../cli-dispatch";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("expense", "book", (ctx) => {
    const documentId = Number(ctx.arg("--document-id"));
    const bankTransactionId = Number(ctx.arg("--bank-transaction-id"));
    const expenseAccountNo = ctx.arg("--expense-account");
    const vatTreatment = ctx.arg("--vat-treatment") as
      | "standard"
      | "reverse_charge"
      | "representation"
      | "exempt"
      | undefined;
    if (
      !Number.isInteger(documentId) ||
      documentId <= 0 ||
      !Number.isInteger(bankTransactionId) ||
      bankTransactionId <= 0 ||
      !expenseAccountNo
    ) {
      console.error(
        "Missing required --document-id <n>, --bank-transaction-id <n>, or --expense-account <account>",
      );
      process.exit(2);
    }
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = bookExpenseFromBank(db, {
      documentId,
      bankTransactionId,
      expenseAccountNo,
      vatTreatment,
      paymentAccountNo: ctx.arg("--payment-account") ?? undefined,
      transactionDate: ctx.arg("--date") ?? undefined,
      text: ctx.arg("--text") ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });
}
