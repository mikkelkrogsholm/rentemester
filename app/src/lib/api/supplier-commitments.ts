import type { SupplierCommitmentsResponse } from "../types";
import { request, type ReadRequestOptions } from "./_shared";
const path=(slug:string)=>`/api/companies/${encodeURIComponent(slug)}/supplier-commitments`;
export const supplierCommitmentsApi={
  supplierCommitments:(slug:string,startDate:string, options?: ReadRequestOptions)=>request<SupplierCommitmentsResponse>(`${path(slug)}?startDate=${encodeURIComponent(startDate)}`, options).then(r=>r.supplierCommitments),
  supplierCommitmentChange:(slug:string,input:{commitmentId:string;action:"paused"|"ended"|"superseded";reason:string})=>request(`${path(slug)}/change`,{method:"POST",body:JSON.stringify({...input,confirm:true})}),
  supplierCommitmentMatch:(slug:string,input:{commitmentId:string;occurrenceDate:string;evidence:{kind:"canonical_document"|"payable"|"bank_transaction";id:string}})=>request(`${path(slug)}/match`,{method:"POST",body:JSON.stringify({...input,confirm:true})}),
};
