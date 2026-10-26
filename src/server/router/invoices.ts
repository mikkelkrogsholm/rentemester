// Invoice list, recurring invoices, and issued-invoice PDF read handlers.

import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { recordHostedDocumentAccess } from "../document-access-audit";
import {
  buildCompanyInvoices,
  buildCompanyImportedReceivables,
  buildCompanyRecurringInvoices,
  resolveCompanyIssuedInvoicePdf,
  resolveYearParam,
} from "../data";
import { okResponse } from "./_shared";
import { responseBodyFromBytes } from "../response-body";
import { openLedgerReadOnly } from "../../core/ledger-inspection";
import { companyPaths } from "../../core/paths";
import { companyRootForSlug } from "../../core/workspace";
import { applyImportedReceivableBankSettlement, applyLegacyImportedReceivableBackfill, getImportedReceivableBankSettlement, planImportedReceivableBankSettlement, planLegacyImportedReceivableBackfill } from "../../core/imported-receivables";
import { withCompanyMutation } from "../mutations";
import { readJsonBody } from "./_shared";

export function handleCompanyRecurringInvoices(
  config: ServerConfig,
  slug: string,
): Response {
  const data = buildCompanyRecurringInvoices(config.workspaceRoot, slug);
  return okResponse({ recurringInvoices: data });
}

/**
 * GET /api/companies/:slug/invoices/:id/pdf — serves the issued-invoice PDF so
 * the owner can download or forward it without leaving the cockpit. It serves
 * only the existing issued evidence snapshot; GET never invokes a renderer.
 */
export function handleCompanyInvoicePdf(
  config: ServerConfig,
  slug: string,
  idRaw: string,
): Response {
  const id = Number(idRaw);
  if (!Number.isInteger(id) || id <= 0) {
    throw ApiError.badRequest("invoice id must be a positive integer");
  }
  const file = resolveCompanyIssuedInvoicePdf(config.workspaceRoot, slug, id);
  recordHostedDocumentAccess(config, {
    companySlug: slug,
    resourceType: "issued_invoice_pdf",
    resourceId: id,
    outcome: "served",
    reasonCode: "authorized",
  });
  return new Response(responseBodyFromBytes(file.bytes), {
    headers: {
      "content-type": file.mimeType,
      "content-disposition": `inline; filename=\"${file.filename}\"; filename*=UTF-8''${encodeURIComponent(file.filename)}`,
      "x-content-type-options": "nosniff",
      "cache-control": "private, no-store",
    },
  });
}

export function handleCompanyInvoices(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyInvoices(config.workspaceRoot, slug, year);
  return okResponse({ invoices: data });
}

/** The imported/archive receivable schedule has a separate endpoint from
 * issued invoices, mirroring CLI and MCP and preventing accidental summing. */
export function handleCompanyImportedReceivables(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const asOf = url.searchParams.get("asOf") ?? new Date().toISOString().slice(0, 10);
  return okResponse({ importedReceivables: buildCompanyImportedReceivables(config.workspaceRoot, slug, asOf) });
}

export async function handleCompanyImportedReceivablesBackfillPlan(config:ServerConfig,slug:string,request:Request):Promise<Response>{
  const body=await readJsonBody(request);const db=openLedgerReadOnly(companyPaths(companyRootForSlug(config.workspaceRoot,slug)).db);
  try{return okResponse({backfill:planLegacyImportedReceivableBackfill(db,body as any)});}finally{db.close();}
}

export async function handleCompanyImportedReceivablesBackfillApply(config:ServerConfig,slug:string,request:Request):Promise<Response>{
  const result=await withCompanyMutation(request,config,slug,(ctx,body)=>{
    const p=ctx.principal;
    const principal=p.serviceAccountId?{kind:"service-account" as const,subjectId:p.serviceAccountId}:{kind:"user" as const,subjectId:p.userId??p.id};
    return applyLegacyImportedReceivableBackfill(ctx.db,{...(body as any),actor:ctx.actor.createdBy,principal,confirm:true});
  },{requireConfirm:true});
  return okResponse({backfill:result});
}

export async function handleCompanyImportedReceivableSettlementPlan(config:ServerConfig,slug:string,request:Request):Promise<Response>{
  const body=await readJsonBody(request);const db=openLedgerReadOnly(companyPaths(companyRootForSlug(config.workspaceRoot,slug)).db);
  try{return okResponse({settlement:planImportedReceivableBankSettlement(db,body as any)});}finally{db.close();}
}

export async function handleCompanyImportedReceivableSettlementApply(config:ServerConfig,slug:string,request:Request):Promise<Response>{
  const result=await withCompanyMutation(request,config,slug,(ctx,body)=>{
    const p=ctx.principal;const principal=p.serviceAccountId?{kind:"service-account" as const,subjectId:p.serviceAccountId}:{kind:"user" as const,subjectId:p.userId??p.id};
    return applyImportedReceivableBankSettlement(ctx.db,{...(body as any),actor:ctx.actor.createdBy,principal,confirm:true});
  },{requireConfirm:true});
  return okResponse({settlement:result});
}

export function handleCompanyImportedReceivableSettlementStatus(config:ServerConfig,slug:string,url:URL):Response{
  const id=Number(url.searchParams.get("bankTransactionId"));const db=openLedgerReadOnly(companyPaths(companyRootForSlug(config.workspaceRoot,slug)).db);
  try{return okResponse({settlement:getImportedReceivableBankSettlement(db,id)});}finally{db.close();}
}
