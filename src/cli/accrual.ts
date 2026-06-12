import { migrate } from "../core/db";
import {
  registerAccrual,
  recognizeAccrualPeriod,
  buildAccrualRegisterReport,
  type AccrualType,
} from "../core/accruals";
import { openCommandDb } from "../cli-dispatch";
import type { CommandDispatch } from "../cli-dispatch";

const ACCRUAL_TYPES: ReadonlySet<AccrualType> = new Set<AccrualType>([
  "prepaid_expense",
  "accrued_expense",
  "deferred_revenue",
]);

export function register(dispatch: CommandDispatch): void {
  dispatch.on("accrual", "register", (ctx) => {
    const accrualType = ctx.arg("--type");
    const description = ctx.arg("--description");
    const totalAmount = Number(ctx.arg("--amount"));
    const recognitionPeriods = Number(ctx.arg("--periods"));
    const firstRecognitionDate = ctx.arg("--first-date");
    const resultAccountNo = ctx.arg("--result-account");
    if (
      !accrualType ||
      !ACCRUAL_TYPES.has(accrualType as AccrualType) ||
      !description ||
      !Number.isFinite(totalAmount) ||
      totalAmount <= 0 ||
      !Number.isInteger(recognitionPeriods) ||
      recognitionPeriods <= 0 ||
      !firstRecognitionDate ||
      !resultAccountNo
    ) {
      console.error(
        "Missing required --type prepaid_expense|accrued_expense|deferred_revenue --description <text> --amount <n> --periods <n> --first-date <YYYY-MM-DD> --result-account <konto>",
      );
      process.exit(2);
    }
    const periodStepRaw = ctx.arg("--period-step-months");
    const periodStepMonths = periodStepRaw === undefined ? undefined : Number(periodStepRaw);
    if (periodStepMonths !== undefined && (!Number.isInteger(periodStepMonths) || periodStepMonths <= 0)) {
      console.error("--period-step-months must be a positive integer when present");
      process.exit(2);
    }
    const documentIdRaw = ctx.arg("--document-id");
    const documentId = documentIdRaw === undefined ? undefined : Number(documentIdRaw);
    if (documentId !== undefined && (!Number.isInteger(documentId) || documentId <= 0)) {
      console.error("--document-id must be a positive integer when present");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const result = registerAccrual(db, {
      accrualType: accrualType as AccrualType,
      description,
      totalAmount,
      recognitionPeriods,
      firstRecognitionDate,
      registrationDate: ctx.arg("--registration-date") ?? undefined,
      periodStepMonths,
      resultAccountNo,
      balanceAccountNo: ctx.arg("--balance-account") ?? undefined,
      settlementAccountNo: ctx.arg("--settlement-account") ?? undefined,
      documentId,
      note: ctx.arg("--note") ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });

  dispatch.on("accrual", "recognize", (ctx) => {
    const accrualId = Number(ctx.arg("--accrual-id"));
    const periodIndex = Number(ctx.arg("--period"));
    if (
      !Number.isInteger(accrualId) ||
      accrualId <= 0 ||
      !Number.isInteger(periodIndex) ||
      periodIndex <= 0
    ) {
      console.error("Missing required --accrual-id <n> --period <n>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const result = recognizeAccrualPeriod(db, {
      accrualId,
      periodIndex,
      transactionDate: ctx.arg("--date") ?? undefined,
      settlementAccountNo: ctx.arg("--settlement-account") ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });

  dispatch.on("accrual", "register-report", (ctx) => {
    const db = openCommandDb(ctx);
    migrate(db);
    const result = buildAccrualRegisterReport(db);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });
}
