import {request,type ReadRequestOptions} from "./_shared";
export type ViesValidation={countryCode:string;vatNumber:string;normalized:string;valid:boolean;name:string|null;address:string|null;validatedAt:string;expiresAt:string};
export type EuSalesReport={ok:true;periodStart:string;periodEnd:string;customers:Array<{vatNumber:string;countryCode:string;customerName:string;invoiceCount:number;creditNoteCount:number;totalValue:number;invoiceNumbers:string[];creditNoteNumbers:string[]}>;totalValue:number;invoiceCount:number;creditNoteCount:number;sources:Array<{documentId:number;documentType:string;documentNumber:string;issueDate:string;vatNumber:string;currency:string;netAmountDkk:number}>;warnings:string[];errors:string[]};
export const euSalesApi={
  viesValidate:(slug:string,vatOrCvr:string)=>request<{ok:true;validation:ViesValidation}>(`/api/companies/${encodeURIComponent(slug)}/vies/validate`,{method:"POST",body:JSON.stringify({vatOrCvr})}),
  euSales:(slug:string,from:string,to:string,options?:ReadRequestOptions)=>request<EuSalesReport>(`/api/companies/${encodeURIComponent(slug)}/eu-sales?${new URLSearchParams({from,to})}`,options),
};
