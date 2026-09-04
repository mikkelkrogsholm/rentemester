import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { withCompanyMutation } from "../mutations";
import { companyRootForSlug } from "../../core/workspace";
import { companyPaths } from "../../core/paths";
import { openLedgerReadOnly } from "../../core/ledger-inspection";
import { createPurchaseCase, getPurchaseCase, listPurchaseCases, reviewPurchaseCase, type DocumentationOutcome, type PurchaseCaseSource } from "../../core/purchase-cases";
import { okResponse } from "./_shared";

const outcomes=new Set<DocumentationOutcome>(["unresolved","ordinary_evidence_sufficient","alternative_evidence_assessed"]);
function source(body:Record<string,unknown>):PurchaseCaseSource { const raw=body.source; if(!raw||typeof raw!=="object"||Array.isArray(raw))throw ApiError.badRequest("source is required"); const value=raw as Record<string,unknown>; if((value.kind!=="document"&&value.kind!=="bank_transaction"&&value.kind!=="payable")||!Number.isInteger(value.id)||Number(value.id)<=0)throw ApiError.badRequest("source must be a typed positive document, bank_transaction, or payable reference"); return {kind:value.kind,id:Number(value.id)} as PurchaseCaseSource; }
function outcome(value:unknown):DocumentationOutcome { if(!outcomes.has(value as DocumentationOutcome))throw ApiError.badRequest("documentationOutcome is required"); return value as DocumentationOutcome; }
function optionalNote(value:unknown){if(value===undefined)return undefined;if(typeof value!=="string"||value.length>2000)throw ApiError.badRequest("note must be at most 2000 characters");return value;}
export async function handlePurchaseCaseCreate(config:ServerConfig,request:Request,slug:string){return okResponse(await withCompanyMutation(request,config,slug,({db,actor},body)=>createPurchaseCase(db,{caseId:typeof body.caseId==="string"?body.caseId:undefined,source:source(body),documentationOutcome:body.documentationOutcome===undefined?undefined:outcome(body.documentationOutcome),note:optionalNote(body.note),actor}),{requireConfirm:true,keyIdempotent:"purchase_case_create",requireIdempotencyKey:true}));}
export async function handlePurchaseCaseReview(config:ServerConfig,request:Request,slug:string,id:string){return okResponse(await withCompanyMutation(request,config,slug,({db,actor},body)=>{if(!Number.isInteger(body.expectedVersion)||typeof body.expectedSourceFingerprint!=="string"||!/^[a-f0-9]{64}$/.test(body.expectedSourceFingerprint))throw ApiError.badRequest("expectedVersion and expectedSourceFingerprint are required");return reviewPurchaseCase(db,{caseId:id,expectedVersion:Number(body.expectedVersion),expectedSourceFingerprint:body.expectedSourceFingerprint,documentationOutcome:outcome(body.documentationOutcome),note:optionalNote(body.note),actor});},{requireConfirm:true,keyIdempotent:"purchase_case_review",requireIdempotencyKey:true}));}
export function handlePurchaseCaseList(config:ServerConfig,slug:string){const db=openLedgerReadOnly(companyPaths(companyRootForSlug(config.workspaceRoot,slug)).db);try{return okResponse({purchaseCases:listPurchaseCases(db)});}finally{db.close();}}
export function handlePurchaseCaseGet(config:ServerConfig,slug:string,id:string){const db=openLedgerReadOnly(companyPaths(companyRootForSlug(config.workspaceRoot,slug)).db);try{return okResponse({purchaseCase:getPurchaseCase(db,id)});}finally{db.close();}}
