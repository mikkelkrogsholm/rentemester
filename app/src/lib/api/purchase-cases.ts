import type { PurchaseOverview } from "../types";
import { request } from "./_shared";

export const purchaseCasesApi = {
  purchaseOverview:(slug:string,input:{from:string;to:string;includeProvisional?:boolean}) => request<{ok:true;overview:PurchaseOverview}>(`/api/companies/${encodeURIComponent(slug)}/purchase-overview?${new URLSearchParams(Object.entries(input).filter(([,value])=>value!==undefined).map(([key,value])=>[key,String(value)]))}`).then(result=>result.overview),
  reviewPurchaseCaseGroup:(slug:string,input:{groupId?:string;members:Array<{caseId:string;expectedVersion:number;expectedSourceFingerprint:string}>;expectedPolicyEventHash?:string;documentationOutcome:"ordinary_evidence_sufficient"|"alternative_evidence_assessed"|"unresolved";note?:string;idempotencyKey:string}) => request<{ok:true;group:unknown}>(`/api/companies/${encodeURIComponent(slug)}/purchase-cases/group-review`,{method:"POST",headers:{"idempotency-key":input.idempotencyKey},body:JSON.stringify({...input,confirm:true})}),
};
