// Bank import + bank-account write handlers (#213 slice 2, #345).

import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { syncUnmatchedBankTransactionExceptions } from "../../core/exceptions";
import { addBankAccount, importBankCsv, updateBankAccount } from "../../core/bank";
import { applyBankReconciliationCorrection } from "../../core/bank-journal-reconciliation";
import type { ServerConfig } from "../config";
import { withCompanyMutation } from "../mutations";
import { removePathWithRetry } from "../../core/fs-cleanup";
import {
  MAX_UPLOAD_BODY_BYTES,
  okResponse,
  optionalBodyString,
  requireBodyString,
} from "./_shared";
import { ApiError } from "../errors";

function correctionPrincipal(principal:{via:string;userId?:string;serviceAccountId?:string}) { if(principal.via==="service-principal"&&principal.serviceAccountId)return {kind:"service-account" as const,subjectId:principal.serviceAccountId}; return principal.userId ? {kind:"user" as const,subjectId:principal.userId} : undefined; }

export async function handleBankReconciliationCorrectionApply(config:ServerConfig,request:Request,slug:string):Promise<Response>{
  const result=await withCompanyMutation(request,config,slug,({db,actor,principal},body)=>{
    if (!correctionPrincipal(principal)) throw ApiError.unauthorized("bank reconciliation correction requires an authenticated user or service principal");
    const int=(key:string)=>{const value=body[key];if(!Number.isInteger(value)||Number(value)<=0)throw ApiError.badRequest(`${key} must be a positive integer`);return Number(value);};
    const text=(key:string)=>typeof body[key]==="string"&&body[key].trim()?body[key].trim():"";
    return applyBankReconciliationCorrection(db,{bankTransactionId:int("bankTransactionId"),replacementJournalEntryId:int("replacementJournalEntryId"),expectedReconciliationId:text("expectedReconciliationId"),planHash:text("planHash"),reason:text("reason"),actor:actor.createdBy,principal:correctionPrincipal(principal),confirm:true});
  },{requireConfirm:true,keyIdempotent:"bank_reconciliation_correction_apply",requireIdempotencyKey:true,idempotencyPayload:(body)=>({bankTransactionId:body.bankTransactionId,replacementJournalEntryId:body.replacementJournalEntryId,expectedReconciliationId:body.expectedReconciliationId,planHash:body.planHash,reason:body.reason})});
  if(!result.ok) throw ApiError.conflict(result.errors[0] ?? "bank reconciliation correction was rejected");
  return okResponse({correction:result});
}

/**
 * POST /api/companies/:slug/bank/import — imports a bank-statement CSV.
 *
 * Body: `{ csvContent: string, account?: string, profile?: string, statementOrder?: "ascending"|"descending",
 * confirm: true }`. The frontend reads the chosen CSV file in the browser and
 * POSTs its text as `csvContent`; the handler writes it to a `mkdtemp` file
 * and calls the SAME `importBankCsv` core function the CLI/MCP use, then runs
 * `syncUnmatchedBankTransactionExceptions` exactly as `bank import` does.
 *
 * Destructive (it appends ledger rows) so `requireConfirm` is set — the body
 * must carry `confirm: true`. A `maxBodyBytes` cap hardens the upload route.
 * Goes through `withCompanyMutation`, so the backup lock, the localhost gate
 * and actor attribution all apply.
 */
export async function handleBankImport(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const csvContent = requireBodyString(body, "csvContent");
      const account = optionalBodyString(body, "account");
      const profile = optionalBodyString(body, "profile");
      const statementOrder = optionalBodyString(body, "statementOrder");
      if (statementOrder !== undefined && statementOrder !== "ascending" && statementOrder !== "descending") {
        throw ApiError.badRequest("statementOrder must be 'ascending' or 'descending'");
      }

      // Mirror the MCP `csvContent` pattern: persist the inline CSV to a
      // private temp file, then hand core a path — core reads from disk and
      // copies the file into the company dir, so the temp dir is a transient
      // staging area that must be removed on EVERY exit path (success, import
      // rejection or throw). Without the finally each cockpit import — and each
      // retried/failing one — leaked a temp dir forever (matches the MCP bank
      // tool's #383 cleanup).
      const tmpDir = mkdtempSync(join(tmpdir(), "rentemester-cockpit-bank-"));
      try {
        const csvPath = join(tmpDir, "bank-import.csv");
        writeFileSync(csvPath, csvContent, "utf8");

        const imported = importBankCsv(ctx.db, ctx.companyRoot, csvPath, {
          account,
          profile,
          statementOrder: statementOrder as "ascending" | "descending" | undefined,
        });
        // The CLI/MCP both sync unmatched-transaction exceptions after a
        // successful import — replicate that so the Cockpit behaves identically.
        const sync = imported.ok
          ? syncUnmatchedBankTransactionExceptions(ctx.db)
          : { ok: true, created: 0, errors: [] };
        return {
          ...imported,
          exceptionsCreated: sync.created,
        };
      } finally {
        removePathWithRetry(tmpDir);
      }
    },
    { requireConfirm: true, maxBodyBytes: MAX_UPLOAD_BODY_BYTES },
  );

  // The core `BankImportResult` shape is echoed back so the UI can report the
  // batch id, the imported/skipped counts and any balance warnings.
  return okResponse({
    import: {
      importBatchId: result.importBatchId,
      imported: result.imported ?? 0,
      skippedDuplicates: result.skippedDuplicates ?? 0,
      skippedDuplicateRows: result.skippedDuplicateRows ?? [],
      bankAccountSlug: result.bankAccountSlug,
      profile: result.profile,
      balanceWarnings: result.balanceWarnings ?? [],
      exceptionsCreated: result.exceptionsCreated ?? 0,
    },
  });
}

/**
 * POST /api/companies/:slug/bank-accounts — opretter en bankkonto (#345).
 *
 * Body: `{ name, slug?, bankName?, registrationNo?, accountNo?, iban?,
 * currency?, ledgerAccountNo? }`. Wrapper omkring `addBankAccount` fra
 * kernen. Backup-lock + actor-attribution sker via `withCompanyMutation`.
 */
export async function handleCreateBankAccount(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const name = requireBodyString(body, "name");
      const accSlug = optionalBodyString(body, "slug");
      const bankName = optionalBodyString(body, "bankName");
      const registrationNo = optionalBodyString(body, "registrationNo");
      const accountNo = optionalBodyString(body, "accountNo");
      const iban = optionalBodyString(body, "iban");
      const bic = optionalBodyString(body, "bic");
      const accountOwner = optionalBodyString(body, "accountOwner");
      const customerNo = optionalBodyString(body, "customerNo");
      const currency = optionalBodyString(body, "currency");
      const ledgerAccountNo = optionalBodyString(body, "ledgerAccountNo");
      const created = addBankAccount(ctx.db, {
        name,
        ...(accSlug ? { slug: accSlug } : {}),
        ...(bankName ? { bankName } : {}),
        ...(registrationNo ? { registrationNo } : {}),
        ...(accountNo ? { accountNo } : {}),
        ...(iban ? { iban } : {}),
        ...(bic ? { bic } : {}),
        ...(accountOwner ? { accountOwner } : {}),
        ...(customerNo ? { customerNo } : {}),
        ...(currency ? { currency } : {}),
        ...(ledgerAccountNo ? { ledgerAccountNo } : {}),
      });
      // Marker actor på audit-log'en (write går gennem withCompanyMutation,
      // som sørger for at append-only audit-log fanger den).
      void ctx.actor;
      if (!created.ok) {
        return { ok: false, account: null, errors: created.errors };
      }
      return { ok: true, account: created.account, errors: [] as string[] };
    },
  );
  return okResponse({ bankAccount: result.account });
}

/** Audited payment-profile update; confirmation is required by the cockpit
 * mutation contract just like every other ledger write. */
export async function handleUpdateBankAccount(config: ServerConfig, request: Request, slug: string, account: string): Promise<Response> {
  const result = await withCompanyMutation(request, config, slug, (ctx, body) => {
    const optional = (key: string) => Object.prototype.hasOwnProperty.call(body, key) ? optionalBodyString(body, key) ?? "" : undefined;
    const updated = updateBankAccount(ctx.db, {
      idOrSlug: account, name: optional("name"), bankName: optional("bankName"), registrationNo: optional("registrationNo"),
      accountNo: optional("accountNo"), iban: optional("iban"), bic: optional("bic"), accountOwner: optional("accountOwner"),
      customerNo: optional("customerNo"), currency: optional("currency"), ledgerAccountNo: optional("ledgerAccountNo"),
      active: typeof body.active === "boolean" ? body.active : undefined,
      createdBy: ctx.actor.createdBy,
      createdByProgram: ctx.actor.createdByProgram,
    });
    return { ok: updated.ok, account: updated.account ?? null, errors: updated.errors };
  }, { requireConfirm: true });
  return okResponse({ bankAccount: result.account });
}
