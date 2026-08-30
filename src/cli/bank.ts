import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { importBankCsv, resolveBankAccount } from "../core/bank";
import { suggestBankMatches } from "../core/bank-suggest-matches";
import { buildBankReconciliationReport, listBankTransactions } from "../core/reconciliation";
import { syncUnmatchedBankTransactionExceptions } from "../core/exceptions";
import {
  openCommandDb,
  optionalNumberOrFatal,
  requiredNumberOrFatal,
} from "../cli-dispatch";
import { renderHumanReport, formatKroner } from "../cli-format";
import { ledgerStatusDa } from "../core/messages";
import type { Database } from "bun:sqlite";
import type { CommandContext, CommandDispatch } from "../cli-dispatch";
import { linkBankTransactionToJournal, planBankReconciliationCorrection, applyBankReconciliationCorrection, type BankJournalMatchMethod } from "../core/bank-journal-reconciliation";
import { executeLocalIdempotentMutation, IdempotencyError, validateIdempotencyKey, type StablePrincipal } from "../core/idempotency";
import { inspectOpenLedger, openLedgerReadOnly } from "../core/ledger-inspection";
import { planDirectBankPurchasePayableCorrection, applyDirectBankPurchasePayableCorrection } from "../core/direct-bank-purchase-payable-correction";

function correctionPrincipal(ctx: CommandContext): StablePrincipal | undefined {
  const raw = ctx.trimToNull(ctx.arg("--principal"));
  const match = raw?.match(/^(user|service-account):(.+)$/);
  return match?.[2].trim() ? { kind: match[1] as StablePrincipal["kind"], subjectId: match[2].trim() } : undefined;
}

// ===== BANK CLUSTER (#187) =====
// Resolves an optional `--account <id|slug>` filter to a numeric bank-account
// id. A given-but-unknown account is a fatal CLI error.
function resolveAccountFilter(ctx: CommandContext, db: Database): number | undefined {
  const raw = ctx.trimToNull(ctx.arg("--account"));
  if (!raw) return undefined;
  const account = resolveBankAccount(db, raw);
  if (!account) {
    console.error(`--account '${raw}' does not match any registered bank account`);
    process.exit(2);
  }
  return account.id;
}
// ===== END BANK CLUSTER (#187) =====

function renderBankTransactionsHuman(rows: any[]): void {
  console.log(`Banktransaktioner (${rows.length})`);
  if (rows.length === 0) {
    console.log("Ingen banktransaktioner for det valgte filter.");
    return;
  }
  for (const row of rows) {
    const status = row.ledgerStatus != null ? ledgerStatusDa(String(row.ledgerStatus)) : "—";
    console.log("");
    console.log(`#${row.id} — ${row.transactionDate} | ${formatKroner(row.amount)}`);
    console.log(`  Tekst: ${row.text ?? "—"}`);
    const ref = row.reference ? ` | Reference: ${row.reference}` : "";
    console.log(`  Status: ${status}${ref}`);
    if (row.journalEntryNo) {
      console.log(`  Bogført som postering ${row.journalEntryNo}`);
    }
  }
}

function renderBankSuggestionsHuman(rows: any[]): void {
  if (rows.length === 0) {
    console.log("Ingen uafstemte banktransaktioner for det valgte filter.");
    return;
  }
  for (const row of rows) {
    console.log(
      `Banktransaktion ${row.bankTransactionId} | ${row.date} | ${row.amount} ${row.currency} | ${row.text}`,
    );
    if (row.suggestions.length === 0) {
      // EJER-7: when an exact-amount candidate exists, explain WHY it is not a
      // safe suggestion instead of a bare "Ingen sikre forslag" that
      // contradicts the exceptions queue (which names the same bilag).
      if (row.unsafeMatchReason) {
        console.log(`  Ingen sikre forslag. ${row.unsafeMatchReason}`);
      } else {
        console.log("  Ingen sikre forslag.");
      }
      continue;
    }
    console.table(
      row.suggestions.map((suggestion: any) => ({
        type: suggestion.kind,
        bilagsId: suggestion.documentId,
        fakturanr: suggestion.invoiceNo,
        leverandør: suggestion.supplierName ?? null,
        kunde: suggestion.customerName ?? null,
        sikkerhed: suggestion.confidence,
        begrundelser: suggestion.reasons.join("; "),
      })),
    );
  }
}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("bank", "import", (ctx) => {
    const file = ctx.arg("--file");
    if (!file) {
      console.error("Missing required --file <transactions.csv>");
      process.exit(2);
    }
    const root = ctx.companyRoot();
    const db = openDb(companyPaths(root).db);
    migrate(db);
    const result = importBankCsv(db, root, file, {
      account: ctx.trimToNull(ctx.arg("--account")) ?? undefined,
      profile: ctx.trimToNull(ctx.arg("--profile")) ?? undefined,
    });
    const sync = result.ok
      ? syncUnmatchedBankTransactionExceptions(db)
      : { ok: true, created: 0, errors: [] };
    ctx.emitResult({
      ...(result as Record<string, unknown>),
      exceptionsCreated: sync.created,
    });
    db.close();
  });

  dispatch.on("bank", "list", (ctx) => {
    const amountArg = ctx.arg("--amount");
    const amount = amountArg === undefined ? undefined : Number(amountArg);
    if (amountArg !== undefined && Number.isNaN(amount)) {
      console.error("--amount must be numeric when present");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const bankAccountId = resolveAccountFilter(ctx, db);
    const result = listBankTransactions(db, {
      status: ctx.arg("--status") as any,
      from: ctx.arg("--from") ?? undefined,
      to: ctx.arg("--to") ?? undefined,
      textMatch: ctx.arg("--text-match") ?? undefined,
      amount,
      bankAccountId,
    });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else if (result.ok) {
      renderBankTransactionsHuman(result.rows);
    } else {
      console.error(result.errors.join("\n"));
    }
    db.close();
    if (!result.ok) process.exit(1);
  });

  dispatch.on("bank", "suggest-matches", (ctx) => {
    const bankTransactionId = optionalNumberOrFatal(ctx, "--bank-transaction-id");
    const max = optionalNumberOrFatal(ctx, "--max");
    const db = openCommandDb(ctx);
    migrate(db);
    const result = suggestBankMatches(db, {
      bankTransactionId:
        bankTransactionId === undefined ? undefined : Number(bankTransactionId),
      max: max === undefined ? undefined : Number(max),
    });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else if (result.ok) {
      renderBankSuggestionsHuman(result.rows);
    } else {
      console.error(result.errors.join("\n"));
    }
    db.close();
    if (!result.ok) process.exit(1);
  });

  dispatch.on("bank", "link-journal", (ctx) => {
    if (ctx.arg("--confirm") !== "yes") {
      ctx.fatal("bank link-journal requires the exact confirmation --confirm yes");
    }
    const bankId = requiredNumberOrFatal(ctx, "--bank-transaction-id");
    const journalId = requiredNumberOrFatal(ctx, "--journal-entry-id");
    const matchMethod = ctx.trimToNull(ctx.arg("--match-method")) as BankJournalMatchMethod | null;
    if (!matchMethod) ctx.fatal("Missing required --match-method <method>");
    const db = openCommandDb(ctx);
    migrate(db);
    const result = linkBankTransactionToJournal(db, {
      bankTransactionId: bankId,
      journalEntryId: journalId,
      matchMethod: matchMethod as BankJournalMatchMethod,
      sourceReference: ctx.trimToNull(ctx.arg("--source-reference")) ?? undefined,
      note: ctx.trimToNull(ctx.arg("--note")) ?? undefined,
      createdBy: ctx.cliActor ?? ctx.inferredMutationActor() ?? undefined,
      createdByProgram: "rentemester-cli",
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("bank", "correction-plan", (ctx) => {
    const db = openLedgerReadOnly(companyPaths(ctx.companyRoot()).db);
    if (inspectOpenLedger(db).status !== "current") { db.close(); ctx.fatal("bank correction-plan requires a current ledger schema; run a write migration first"); }
    const result = planBankReconciliationCorrection(db, { bankTransactionId: requiredNumberOrFatal(ctx, "--bank-transaction-id"), replacementJournalEntryId: requiredNumberOrFatal(ctx, "--replacement-journal-entry-id") });
    ctx.emitResult(result as Record<string, unknown>); db.close();
  });

  dispatch.on("bank", "correction-apply", (ctx) => {
    if (ctx.arg("--confirm") !== "yes") ctx.fatal("bank correction-apply requires the exact confirmation --confirm yes");
    const db = openCommandDb(ctx); migrate(db);
    const principal = correctionPrincipal(ctx);
    const key = ctx.trimToNull(ctx.arg("--idempotency-key"));
    if (!key) ctx.fatal("bank correction-apply requires --idempotency-key <key>");
    if (!principal) ctx.fatal("bank correction-apply requires --principal user:<id>|service-account:<id>");
    const payload = { bankTransactionId: requiredNumberOrFatal(ctx, "--bank-transaction-id"), replacementJournalEntryId: requiredNumberOrFatal(ctx, "--replacement-journal-entry-id"), expectedReconciliationId: ctx.trimToNull(ctx.arg("--expected-reconciliation-id")) ?? "", planHash: ctx.trimToNull(ctx.arg("--plan-hash")) ?? "", reason: ctx.trimToNull(ctx.arg("--reason")) ?? "" };
    let result: Record<string, unknown>;
    try { const run = executeLocalIdempotentMutation(db, { key: validateIdempotencyKey(key), operation:"bank_reconciliation_correction_apply", principal, payload, actor:{createdBy:ctx.cliActor ?? ctx.inferredMutationActor() ?? "",createdByProgram:"rentemester-cli"}, execute:()=>applyBankReconciliationCorrection(db,{...payload,actor:ctx.cliActor ?? ctx.inferredMutationActor() ?? undefined,principal,confirm:true}) }); result = run.receipt ? {...run.result,idempotency:run.receipt} : run.result; }
    catch (error) { result={ok:false,errors:[error instanceof IdempotencyError ? error.code : String(error)]}; }
    ctx.emitResult(result as Record<string, unknown>); db.close();
  });

  dispatch.on("bank", "direct-payable-plan", (ctx) => {
    const db = openLedgerReadOnly(companyPaths(ctx.companyRoot()).db);
    if (inspectOpenLedger(db).status !== "current") { db.close(); ctx.fatal("bank direct-payable-plan requires a current ledger schema; run a write migration first"); }
    const result = planDirectBankPurchasePayableCorrection(db, {
      documentId: requiredNumberOrFatal(ctx, "--document-id"), bankTransactionId: requiredNumberOrFatal(ctx, "--bank-transaction-id"),
      billDate: ctx.trimToNull(ctx.arg("--bill-date")) ?? "", dueDate: ctx.trimToNull(ctx.arg("--due-date")) ?? "",
      expenseAccountNo: ctx.trimToNull(ctx.arg("--expense-account")) ?? "", vatTreatment: ctx.arg("--vat-treatment") as any,
      vendorId: optionalNumberOrFatal(ctx, "--vendor-id"), note: ctx.trimToNull(ctx.arg("--note")) ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>); db.close();
  });

  dispatch.on("bank", "direct-payable-apply", (ctx) => {
    if (ctx.arg("--confirm") !== "yes") ctx.fatal("bank direct-payable-apply requires the exact confirmation --confirm yes");
    const principal = correctionPrincipal(ctx); const key = ctx.trimToNull(ctx.arg("--idempotency-key"));
    if (!principal) ctx.fatal("bank direct-payable-apply requires --principal user:<id>|service-account:<id>");
    if (!key) ctx.fatal("bank direct-payable-apply requires --idempotency-key <key>");
    const payload = { documentId: requiredNumberOrFatal(ctx,"--document-id"), bankTransactionId: requiredNumberOrFatal(ctx,"--bank-transaction-id"), billDate:ctx.trimToNull(ctx.arg("--bill-date"))??"", dueDate:ctx.trimToNull(ctx.arg("--due-date"))??"", expenseAccountNo:ctx.trimToNull(ctx.arg("--expense-account"))??"", vatTreatment:ctx.arg("--vat-treatment") as any, vendorId:optionalNumberOrFatal(ctx,"--vendor-id"), note:ctx.trimToNull(ctx.arg("--note"))??undefined, planHash:ctx.trimToNull(ctx.arg("--plan-hash"))??"", reason:ctx.trimToNull(ctx.arg("--reason"))??"" };
    const db=openCommandDb(ctx); migrate(db); let result:Record<string,unknown>;
    try { const run=executeLocalIdempotentMutation(db,{key:validateIdempotencyKey(key),operation:"direct_bank_purchase_payable_correction_apply",principal,payload,actor:{createdBy:ctx.cliActor??ctx.inferredMutationActor()??"",createdByProgram:"rentemester-cli"},execute:()=>applyDirectBankPurchasePayableCorrection(db,{...payload,actor:ctx.cliActor??ctx.inferredMutationActor()??undefined,principal,confirm:true})}); result=run.receipt?{...run.result,idempotency:run.receipt}:run.result; }
    catch(error){result={ok:false,errors:[error instanceof IdempotencyError?error.code:String(error)]};}
    ctx.emitResult(result); db.close();
  });

  dispatch.on("reconcile", "bank", (ctx) => {
    const from = ctx.arg("--from");
    const to = ctx.arg("--to");
    const amountArg = ctx.arg("--amount");
    const amount = amountArg === undefined ? undefined : Number(amountArg);
    if (!from || !to) {
      console.error("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>");
      process.exit(2);
    }
    if (amountArg !== undefined && Number.isNaN(amount)) {
      console.error("--amount must be numeric when present");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const bankAccountId = resolveAccountFilter(ctx, db);
    const result = buildBankReconciliationReport(db, from, to, {
      status: ctx.arg("--status") as any,
      textMatch: ctx.arg("--text-match") ?? undefined,
      amount,
      bankAccountId,
    });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else if (result.ok) {
      const human = renderHumanReport("reconcile-bank", result as Record<string, unknown>);
      console.log(human ?? "");
    } else {
      console.error(result.errors.join("\n"));
    }
    db.close();
    if (!result.ok) process.exit(1);
  });
}
