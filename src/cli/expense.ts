import { migrate } from "../core/db";
import { inspectLedger, openCurrentLedgerSimulation, openLedgerReadOnly } from "../core/ledger-inspection";
import { companyPaths } from "../core/paths";
import { bookExpenseFromBank, previewBookExpenseFromBank, type ExpenseVatTreatment } from "../core/expense-booking";
import { openCommandDb, type CommandContext } from "../cli-dispatch";
import type { CommandDispatch } from "../cli-dispatch";
import { applyPurchaseVatPreflight, purchaseVatPreflightSnapshot } from "./purchase-vat-preflight";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("expense", "vat-preflight", async (ctx) => {
    const documentId = Number(ctx.arg("--document-id"));
    if (!Number.isInteger(documentId) || documentId <= 0) {
      console.error("Missing required --document-id <n>");
      process.exit(2);
    }
    const apply = ctx.arg("--apply");
    if (apply !== undefined && apply !== "yes") ctx.fatal("--apply must be exactly yes");
    // A dry-run is observational.  It must never bootstrap or upgrade a
    // ledger merely to render a decision.
    if (apply === undefined) {
      const inspection = inspectLedger(companyPaths(ctx.companyRoot()).db);
      if (inspection.status !== "current") {
        ctx.emitResult({ ok: false, errors: [`migration-required: schema_${inspection.status}: current=${inspection.currentVersion} required=${inspection.requiredVersion}`], schema: inspection });
        process.exit(1);
      }
      const db = openLedgerReadOnly(companyPaths(ctx.companyRoot()).db);
      const result = purchaseVatPreflightSnapshot(db, documentId);
      ctx.emitResult(result as Record<string, unknown>);
      db.close();
      if (!result.ok) process.exit(1);
      return;
    }
    const actor = (
      ctx.cliActor ??
      process.env.RENTEMESTER_ACTOR ??
      ctx.inferredMutationActor()
    ) ?? ctx.fatal("actor required for mutations");
    const db = openCommandDb(ctx);
    if (apply === "yes") {
      migrate(db);
    }
    const result = await applyPurchaseVatPreflight(db, documentId, actor);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });

  dispatch.on("expense", "book", bookOrPreview);
  dispatch.on("expense", "dry-run", bookOrPreview);
}

function bookOrPreview(ctx: CommandContext) {
  const documentId = Number(ctx.arg("--document-id"));
  const bankTransactionId = Number(ctx.arg("--bank-transaction-id"));
  const expenseAccountNo = ctx.arg("--expense-account");
  const vatTreatment = ctx.arg("--vat-treatment") as ExpenseVatTreatment | undefined;
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
  const dryRun = ctx.sub === "dry-run";
  const db = dryRun ? openCurrentLedgerSimulation(companyPaths(ctx.companyRoot()).db) : openCommandDb(ctx);
  try {
    if (!dryRun) migrate(db);
    const book = dryRun ? previewBookExpenseFromBank : bookExpenseFromBank;
    const result = book(db, {
      documentId,
      bankTransactionId,
      expenseAccountNo,
      vatTreatment,
      paymentAccountNo: ctx.arg("--payment-account") ?? undefined,
      transactionDate: ctx.arg("--date") ?? undefined,
      text: ctx.arg("--text") ?? undefined,
      createdBy:
        ctx.cliActor ??
        process.env.RENTEMESTER_ACTOR ??
        ctx.inferredMutationActor() ??
        undefined,
      createdByProgram:
        ctx.cliActorVia ??
        process.env.RENTEMESTER_ACTOR_VIA ??
        "rentemester-cli",
    });
    ctx.emitResult(result as Record<string, unknown>);
    if (!result.ok) process.exitCode = 1;
  } finally {
    db.close();
  }
}
