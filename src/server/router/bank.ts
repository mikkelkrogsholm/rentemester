// Bank-transaction list + registered bank-accounts read handlers.

import type { ServerConfig } from "../config";
import { buildCompanyBankAccounts } from "../data/bank-accounts-view";
import { buildCompanyBank, resolveYearParam } from "../data";
import { okResponse } from "./_shared";
import { companyRootForSlug } from "../../core/workspace";
import { companyPaths } from "../../core/paths";
import { openLedgerReadOnly, inspectOpenLedger } from "../../core/ledger-inspection";
import { planBankReconciliationCorrection } from "../../core/bank-journal-reconciliation";
import { ApiError } from "../errors";
import { readJsonBody } from "./_shared";
import { withCompanyMutation } from "../mutations";
import { applyLegacyBankBinding, planLegacyBankBinding } from "../../core/legacy-bank-payable-backfill";

export function handleCompanyBank(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyBank(config.workspaceRoot, slug, year);
  return okResponse({ bank: data });
}

/**
 * GET /api/companies/:slug/bank-accounts — Bankkonti + CSV-mapping-profiler
 * (#345). Read-only. POST på samme path opretter en konto.
 */
export function handleCompanyBankAccounts(
  config: ServerConfig,
  slug: string,
): Response {
  const data = buildCompanyBankAccounts(config.workspaceRoot, slug);
  return okResponse({ bankAccounts: data });
}

/** GET is a pure inspection endpoint: it opens no transaction and writes no plan. */
export function handleBankReconciliationCorrectionPlan(config: ServerConfig, slug: string, url: URL): Response {
  const bankTransactionId=Number(url.searchParams.get("bankTransactionId")), replacementJournalEntryId=Number(url.searchParams.get("replacementJournalEntryId"));
  if(!Number.isInteger(bankTransactionId)||bankTransactionId<=0||!Number.isInteger(replacementJournalEntryId)||replacementJournalEntryId<=0) throw ApiError.badRequest("bankTransactionId and replacementJournalEntryId must be positive integers");
  const db=openLedgerReadOnly(companyPaths(companyRootForSlug(config.workspaceRoot,slug)).db); try {
    if(inspectOpenLedger(db).status!=="current") throw ApiError.conflict("ledger migration required before planning a bank reconciliation correction");
    const result=planBankReconciliationCorrection(db,{bankTransactionId,replacementJournalEntryId});
    if(!result.ok) throw ApiError.conflict(result.errors[0] ?? "bank reconciliation correction plan was rejected");
    return okResponse({plan:result});
  } finally { db.close(); }
}

export async function handleLegacyBankBindingPlan(config:ServerConfig,slug:string,request:Request):Promise<Response>{const body=await readJsonBody(request);const db=openLedgerReadOnly(companyPaths(companyRootForSlug(config.workspaceRoot,slug)).db);try{const result=planLegacyBankBinding(db,body as any);if(!result.ok)throw ApiError.conflict(result.errors.join("; "));return okResponse({plan:result.plan});}finally{db.close();}}
export async function handleLegacyBankBindingApply(config:ServerConfig,slug:string,request:Request):Promise<Response>{const result=await withCompanyMutation(request,config,slug,(ctx,body)=>{const p=ctx.principal;return applyLegacyBankBinding(ctx.db,{...(body as any),actor:ctx.actor.createdBy,principal:p.serviceAccountId?{kind:"service-account" as const,subjectId:p.serviceAccountId}:{kind:"user" as const,subjectId:p.userId??p.id},confirm:true});},{requireConfirm:true,keyIdempotent:"bank_legacy_binding_apply",requireIdempotencyKey:true});return okResponse({binding:result,...("idempotency" in result?{idempotency:result.idempotency}:{})});}
