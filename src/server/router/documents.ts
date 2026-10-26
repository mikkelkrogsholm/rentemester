// Document list, file serve, and booking-options read handlers.

import { purchaseVatPreflightSnapshot } from "../../cli/purchase-vat-preflight";
import { PDF_EVIDENCE_TAMPERED, PdfParseError, readVerifiedPdfParse } from "../../core/document-pdf-parser";
import { inspectOpenLedger, openLedgerReadOnly } from "../../core/ledger-inspection";
import { companyPaths } from "../../core/paths";
import { companyRootForSlug } from "../../core/workspace";
import type { ServerConfig } from "../config";
import {
  buildCompanyDocuments,
  buildDocumentBookingOptions,
  resolveCompanyDocumentFile,
} from "../data";
import { recordHostedDocumentAccess } from "../document-access-audit";
import { ApiError } from "../errors";
import { invoiceExtractionSurface } from "../invoice-extraction-surface";
import { responseBodyFromBytes } from "../response-body";
import { okResponse } from "./_shared";
import { readJsonBody, requireString } from "./_shared";
import { withCompanyMutation } from "../mutations";
import { applyDocumentPartyLink, decideInternalNoExternalParty, inspectDocumentPartyLinks, listDocumentPartyLinks, planDocumentPartyLink, supersedeDocumentPartyLink, supersedeInternalNoExternalParty } from "../../core/document-party-links";
import { openWorkspaceControlDb, openWorkspaceControlReadOnlyDb } from "../../core/workspace-control";
import { authorizeWorkspaceRoute } from "../../core/workspace-access";
import { setDocumentCompanyContext } from "../../core/document-company-context";
import { reviewIncompleteStandardPurchaseVatEvidence } from "../../core/document-purchase-vat-evidence-review";
import { applyPartyCoverage, planPartyCoverage, projectPartyCoverage } from "../../core/party-coverage";

function partyLinkPrincipal(config: ServerConfig, slug: string) {
  const principal=config.requestPrincipal;
  if (!principal) throw ApiError.unauthorized("missing or invalid credentials");
  // In hosted mode the membership, never a caller supplied actor, is access.
  if (config.betterAuthProvider) {
    if (!principal.userId) throw ApiError.unauthorized("missing or invalid credentials");
    const control=openWorkspaceControlReadOnlyDb(config.workspaceRoot);
    try { if (!authorizeWorkspaceRoute(control,config.workspaceRoot,{userId:principal.userId,companySlug:slug,permission:"company.master-data"}).allowed) throw ApiError.notFound("document not found"); }
    finally { control.close(); }
  }
  return principal.serviceAccountId ? `service-account:${principal.serviceAccountId}` : principal.id;
}
function partyInput(slug:string, body:Record<string,unknown>) { return { documentId:Number(body.documentId),companySlug:slug,role:body.role as any,partyId:typeof body.partyId==="string"?body.partyId:undefined,jurisdiction:typeof body.jurisdiction==="string"?body.jurisdiction:undefined,identifierKind:typeof body.identifierKind==="string"?body.identifierKind:undefined,identifier:typeof body.identifier==="string"?body.identifier:undefined,legacyKind:typeof body.legacyKind==="string"?body.legacyKind as any:undefined,legacyId:typeof body.legacyId==="string"?body.legacyId:undefined,reviewedLegacyReference:typeof body.reviewedLegacyReference==="string"?body.reviewedLegacyReference:undefined,sourceReview:body.sourceReview&&typeof body.sourceReview==="object"?body.sourceReview as any:undefined}; }
export function handleDocumentPartyLinks(config:ServerConfig,slug:string,request:Request):Response { const u=new URL(request.url),db=openVerifiedRead(config,slug);try{return okResponse({links:listDocumentPartyLinks(db,{status:(u.searchParams.get("status")??undefined) as any})});}finally{db.close();} }
export function handleDocumentPartyLinkInspect(config:ServerConfig,slug:string,idRaw:string):Response {const id=Number(idRaw);if(!Number.isInteger(id)||id<=0)throw ApiError.notFound("document not found");const db=openVerifiedRead(config,slug);try{return okResponse({links:inspectDocumentPartyLinks(db,id)});}finally{db.close();}}
export async function handleDocumentPartyLinkPlan(config:ServerConfig,slug:string,request:Request):Promise<Response>{const body=await readJsonBody(request);const db=openVerifiedRead(config,slug);const control=openWorkspaceControlReadOnlyDb(config.workspaceRoot);try{return okResponse(planDocumentPartyLink(db,control,partyInput(slug,body),companyRootForSlug(config.workspaceRoot,slug)));}finally{control.close();db.close();}}
export async function handleDocumentPartyLinkAction(config:ServerConfig,slug:string,request:Request,action:"apply"|"supersede"):Promise<Response>{const result=await withCompanyMutation(request,config,slug,(ctx,body)=>{const principal=partyLinkPrincipal(config,slug);if(action==="supersede")return supersedeDocumentPartyLink(ctx.db,{documentId:Number(body.documentId),role:body.role as any,planHash:requireString(body,"planHash"),reason:requireString(body,"reason"),confirm:true,actor:ctx.actor.createdBy,principal});const control=openWorkspaceControlDb(config.workspaceRoot);try{return applyDocumentPartyLink(ctx.db,control,{...partyInput(slug,body),planHash:requireString(body,"planHash"),confirm:true,actor:ctx.actor.createdBy,principal,idempotencyKey:typeof body.idempotencyKey==="string"?body.idempotencyKey:undefined},companyRootForSlug(config.workspaceRoot,slug));}finally{control.close();}},{requireConfirm:true});return okResponse(result);}
export async function handleInternalNoExternalParty(config:ServerConfig,slug:string,request:Request,supersede=false):Promise<Response>{const result=await withCompanyMutation(request,config,slug,(ctx,body)=>{const base={documentId:Number(body.documentId),reason:requireString(body,"reason"),confirm:true,actor:ctx.actor.createdBy,principal:partyLinkPrincipal(config,slug)};return supersede?supersedeInternalNoExternalParty(ctx.db,{...base,decisionHash:requireString(body,"decisionHash")}):decideInternalNoExternalParty(ctx.db,{...base,idempotencyKey:typeof body.idempotencyKey==="string"?body.idempotencyKey:undefined});},{requireConfirm:true});return okResponse(result);}
function coverageInput(slug:string,body:Record<string,unknown>){return {companySlug:slug,asOf:typeof body.asOf==="string"?body.asOf:undefined,bankAccountId:Number.isSafeInteger(body.bankAccountId)?Number(body.bankAccountId):undefined,decisions:Array.isArray(body.decisions)?body.decisions as any:undefined};}
export function handlePartyCoverage(config:ServerConfig,slug:string,request:Request):Response{const url=new URL(request.url),db=openVerifiedRead(config,slug),control=openWorkspaceControlReadOnlyDb(config.workspaceRoot);try{return okResponse(projectPartyCoverage(db,control,{companySlug:slug,asOf:url.searchParams.get("asOf")??undefined,bankAccountId:url.searchParams.get("bankAccountId")?Number(url.searchParams.get("bankAccountId")):undefined}));}finally{control.close();db.close();}}
export async function handlePartyCoveragePlan(config:ServerConfig,slug:string,request:Request):Promise<Response>{const body=await readJsonBody(request),db=openVerifiedRead(config,slug),control=openWorkspaceControlReadOnlyDb(config.workspaceRoot);try{return okResponse(planPartyCoverage(db,control,{...coverageInput(slug,body),companyRoot:companyRootForSlug(config.workspaceRoot,slug)}));}finally{control.close();db.close();}}
export async function handlePartyCoverageApply(config:ServerConfig,slug:string,request:Request):Promise<Response>{const result=await withCompanyMutation(request,config,slug,(ctx,body)=>{const control=openWorkspaceControlDb(config.workspaceRoot);try{return applyPartyCoverage(ctx.db,control,companyRootForSlug(config.workspaceRoot,slug),{...coverageInput(slug,body),planHash:requireString(body,"planHash"),idempotencyKey:requireString(body,"idempotencyKey"),confirm:true,actor:ctx.actor.createdBy,principal:partyLinkPrincipal(config,slug)});}finally{control.close();}},{requireConfirm:true});return okResponse(result);}
/** Records reviewed attribution; it cannot alter invoice facts or grant VAT eligibility. */
export async function handleDocumentCompanyContext(config:ServerConfig,slug:string,request:Request):Promise<Response>{const result=await withCompanyMutation(request,config,slug,(ctx,body)=>setDocumentCompanyContext(ctx.db,{documentId:Number(body.documentId),sourceReference:requireString(body,"sourceReference"),businessUseReason:requireString(body,"businessUseReason"),confirm:true,createdBy:ctx.actor.createdBy,createdByProgram:ctx.actor.createdByProgram}),{requireConfirm:true});return okResponse(result);}
export async function handleDocumentPurchaseVatEvidenceReview(config:ServerConfig,slug:string,request:Request):Promise<Response>{const result=await withCompanyMutation(request,config,slug,(ctx,body)=>reviewIncompleteStandardPurchaseVatEvidence(ctx.db,{documentId:Number(body.documentId),bankTransactionId:Number(body.bankTransactionId),businessEvidenceReference:requireString(body,"businessEvidenceReference"),businessEvidenceSha256:requireString(body,"businessEvidenceSha256"),rationale:requireString(body,"rationale"),supersedesReviewSha256:typeof body.supersedesReviewSha256==="string"?body.supersedesReviewSha256:undefined,confirm:true,createdBy:ctx.actor.createdBy,createdByProgram:ctx.actor.createdByProgram,principal:partyLinkPrincipal(config,slug)}),{requireConfirm:true});return okResponse(result);}

export function handleCompanyDocuments(config: ServerConfig, slug: string): Response {
  const data = buildCompanyDocuments(config.workspaceRoot, slug);
  return okResponse({ documents: data });
}

/**
 * GET /api/companies/:slug/documents/:id/booking-options — the read-side data
 * the Bogfør-bilag modal needs (#407): the document fields to prefill, the
 * bookable expense accounts, and the unmatched outgoing bank transactions the
 * owner can pair the bilag with. A read route, so it bypasses the mutation
 * pipeline; an unknown company / ledger / document is a 404.
 */
export function handleCompanyDocumentBookingOptions(
  config: ServerConfig,
  slug: string,
  idRaw: string,
): Response {
  const id = Number(idRaw);
  if (!Number.isInteger(id) || id <= 0) {
    throw ApiError.badRequest("document id must be a positive integer");
  }
  const data = buildDocumentBookingOptions(config.workspaceRoot, slug, id);
  return okResponse({ options: data });
}

/** Read-only VAT preflight. It deliberately performs no provider I/O. */
export function handleCompanyDocumentVatPreflight(config: ServerConfig, slug: string, idRaw: string): Response {
  const id = Number(idRaw);
  if (!Number.isInteger(id) || id <= 0) throw ApiError.badRequest("document id must be a positive integer");
  const db = openLedgerReadOnly(companyPaths(companyRootForSlug(config.workspaceRoot, slug)).db);
  try {
    if (inspectOpenLedger(db).status !== "current") throw ApiError.notFound("company ledger is not ready");
    return okResponse({ preflight: purchaseVatPreflightSnapshot(db, id) });
  } finally {
    db.close();
  }
}

/** Read-only extraction evidence. Never includes a stored path or provider configuration. */
export function handleCompanyDocumentInvoiceExtraction(config: ServerConfig, slug: string, idRaw: string): Response {
  const id = Number(idRaw);
  if (!Number.isInteger(id) || id <= 0) throw ApiError.badRequest("document id must be a positive integer");
  const db = openLedgerReadOnly(companyPaths(companyRootForSlug(config.workspaceRoot, slug)).db);
  try { if (inspectOpenLedger(db).status !== "current") throw ApiError.notFound("company ledger is not ready"); return okResponse({ extraction: invoiceExtractionSurface(db, id) }); } finally { db.close(); }
}

/**
 * Public, verified parser DTOs shared by HTTP, CLI and MCP.  Deliberately do
 * not expose parser result ids, stored paths, child diagnostics, or raw layout
 * coordinates.  Page text is evidence; layout is represented by its persisted
 * SHA-256 integrity hash.
 */
export type DocumentPdfParseStatusDto = {
  documentId: number; sourceSha256: string; parserId: string; parserVersion: string;
  contractVersion: string; status: string; errorCode: string | null; pageCount: number;
  itemCount: number; textLength: number; resultHash: string;
};
export function documentPdfParseStatus(db: any, companyRoot: string, documentId: number): DocumentPdfParseStatusDto | null {
  return readVerifiedPdfParse(db, companyRoot, documentId)?.parse ?? null;
}
export function documentPdfParsedText(db: any, companyRoot: string, documentId: number, offset = 0, limit = 10) {
  const verified=readVerifiedPdfParse(db, companyRoot, documentId); const parse=verified?.parse ?? null;
  if (!verified) return { parse, pages: [], offset, limit, nextOffset: null };
  const pages=verified.pages.slice(offset,offset+limit);
  return { parse, pages, offset, limit, nextOffset: offset + pages.length < verified.pages.length ? offset + pages.length : null };
}
function openVerifiedRead(config: ServerConfig, slug: string) {
  const db = openLedgerReadOnly(companyPaths(companyRootForSlug(config.workspaceRoot, slug)).db);
  const inspection = inspectOpenLedger(db);
  if (inspection.status !== "current") { db.close(); throw ApiError.notFound("company ledger is not ready"); }
  return db;
}
/** Read-only PDF parse state; parser errors are persisted as codes, not stderr. */
export function handleCompanyDocumentParseStatus(config: ServerConfig, slug: string, idRaw: string): Response {
  const id = Number(idRaw); if (!Number.isInteger(id) || id <= 0) throw ApiError.badRequest("document id must be a positive integer");
  const db = openVerifiedRead(config, slug); try { return okResponse({ parse: documentPdfParseStatus(db, companyRootForSlug(config.workspaceRoot, slug), id) }); } catch (error) { if (error instanceof PdfParseError && error.code === "tampered_result") throw ApiError.conflict("PDF evidence integrity verification failed", { subcode: PDF_EVIDENCE_TAMPERED }); throw error; } finally { db.close(); }
}
/** Read-only parsed pages, capped at ten to bound agent and HTTP responses. */
export function handleCompanyDocumentParsedText(config: ServerConfig, slug: string, idRaw: string, url: URL): Response {
  const id = Number(idRaw), offset = Number(url.searchParams.get("offset") ?? "0"), limit = Number(url.searchParams.get("limit") ?? "10");
  if (!Number.isInteger(id) || id <= 0 || !Number.isInteger(offset) || offset < 0 || !Number.isInteger(limit) || limit < 1 || limit > 10) throw ApiError.badRequest("document id, offset and limit (1..10) are invalid");
  const db = openVerifiedRead(config, slug); try { return okResponse(documentPdfParsedText(db, companyRootForSlug(config.workspaceRoot, slug), id, offset, limit)); } catch (error) { if (error instanceof PdfParseError && error.code === "tampered_result") throw ApiError.conflict("PDF evidence integrity verification failed", { subcode: PDF_EVIDENCE_TAMPERED }); throw error; } finally { db.close(); }
}

/**
 * GET /api/companies/:slug/documents/:id/file — serves the stored bilag file
 * so a human can open it in the cockpit. A read route, so it does not run the
 * mutation pipeline; an unknown company or document is a 404.
 */
export function handleCompanyDocumentFile(
  config: ServerConfig,
  slug: string,
  idRaw: string,
): Response {
  const id = Number(idRaw);
  if (!Number.isInteger(id) || id <= 0) {
    throw ApiError.badRequest("document id must be a positive integer");
  }
  const file = resolveCompanyDocumentFile(config.workspaceRoot, slug, id);
  recordHostedDocumentAccess(config, {
    companySlug: slug,
    resourceType: "document_file",
    resourceId: id,
    outcome: "served",
    reasonCode: "authorized",
  });
  // Stored filenames never cross this boundary.  The resolver returns a
  // verified fd-backed byte snapshot and a generated safe name; all source
  // documents are attachments so untrusted document bytes cannot render in
  // the cockpit origin.
  return new Response(responseBodyFromBytes(file.bytes), {
    headers: {
      "content-type": file.mimeType,
      "content-disposition": `attachment; filename=\"${file.filename}\"; filename*=UTF-8''${encodeURIComponent(file.filename)}`,
      "x-content-type-options": "nosniff",
      "cache-control": "private, no-store",
    },
  });
}
