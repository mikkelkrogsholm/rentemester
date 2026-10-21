import { request, requestBlob, type ReadRequestOptions } from "./_shared";
export type DineroSource={fileName:string;contentBase64:string};
export type DineroApply=DineroSource&{planHash:string;idempotencyKey:string;confirm:true};
export type DineroCounts={accounts:number;openingLines:number;historicalVouchers:number;importedInvoices:number;importedEvents:number;archiveYears:number};
export type DineroPlan={status:"ready"|"blocked";planHash:string;source:{sha256:string;sizeBytes:number;entries:Array<{path:string;sha256:string;sizeBytes:number}>};latestYear:number;cutOverDate:string|null;controlDate:string|null;counts:DineroCounts;controls:Array<{accountNo:string;kind:string;sourceAmount:string;projectedAmount:string;scheduleAmount:string|null;reconciles:boolean}>;blockers:string[];warnings:string[];existingAttemptId:number|null};
export type DineroAttempt={attemptId:number;sourceSha256:string;sourceSizeBytes:number;parserContract:string;cutOverDate:string|null;controlDate:string|null;acceptedAt:string;counts:DineroCounts;sourceAssurance:"retained-native"|"legacy-unavailable"};
export type ImportedReceivables={ok:boolean;asOfDate:string;boundary:string;count:number;totalOpen:number;rows:Array<{source:"imported";externalInvoiceId:string;customerExternalId?:string|null;customerName?:string|null;invoiceDate:string;dueDate?:string|null;grossAmount:number;paidAmount:number;openBalance:number;controlAccountNo:string;sourceRecognitionRef:string;sourceDocumentHash:string;scheduleHash:string;archiveBoundary:string}>;errors:string[]};
const base=(slug:string)=>`/api/companies/${encodeURIComponent(slug)}`;
export const dineroImportApi={
 plan:(slug:string,body:DineroSource)=>request<{ok:true;dineroImport:DineroPlan}>(`${base(slug)}/imports/dinero/plan`,{method:"POST",readOnly:true,body:JSON.stringify(body)}).then(r=>r.dineroImport),
 apply:(slug:string,body:DineroApply)=>request<{ok:true;dineroImport:DineroAttempt}>(`${base(slug)}/imports/dinero/apply`,{method:"POST",body:JSON.stringify(body)}).then(r=>r.dineroImport),
 list:(slug:string,options?:ReadRequestOptions)=>request<{ok:true;dineroImport:{attempts:DineroAttempt[];count:number}}>(`${base(slug)}/imports/dinero`,options).then(r=>r.dineroImport),
 original:(slug:string,id:number,options?:ReadRequestOptions)=>requestBlob(`${base(slug)}/imports/dinero/${id}/original`,options),
 receivables:(slug:string,asOf:string,options?:ReadRequestOptions)=>request<{ok:true;importedReceivables:ImportedReceivables}>(`${base(slug)}/invoices/imported-receivables?${new URLSearchParams({asOf})}`,options).then(r=>r.importedReceivables),
};
