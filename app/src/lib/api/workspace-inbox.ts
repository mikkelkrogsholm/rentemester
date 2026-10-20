import type { DocumentIngestMetadata } from "./documents";
import { request, type ReadRequestOptions } from "./_shared";

const base=(slug:string)=>`/api/companies/${encodeURIComponent(slug)}/workspace-inbox`;
export type InboxMetadataReview={companySlug:string;ledgerUuid:string;sourceHash:string;metadataHash:string;metadata:DocumentIngestMetadata;reviewedBy:string;reviewedAt:string};
export type InboxSource={sourceId:string;sha256:string;filename:string;transport:string;receivedAt:string;metadata?:Partial<DocumentIngestMetadata>;metadataReview?:InboxMetadataReview|null;mail?:{role:"attachment"|"eml"|"refusal";messageId?:string|null;emlSourceId?:string|null;emlSha256?:string|null}|null;handoffs?:Array<{companySlug:string;state:string;leaseExpiresAt:string}>;history?:Array<{event_type:string;actor:string;created_at:string}>;exception?:{code:string}|null;assignments:Array<{companySlug:string;state:string;documentNo?:string|null;documentId?:number|null}>};
export const workspaceInboxApi={
  list:(slug:string, options?: ReadRequestOptions, cursor=0)=>request<{rows:InboxSource[];count?:number;nextCursor?:number|null}>(`${base(slug)}${cursor ? `?cursor=${cursor}&limit=25` : ""}`, options),
  inspect:(slug:string,sourceId:string, options?: ReadRequestOptions)=>request<{source:InboxSource}>(`${base(slug)}/${encodeURIComponent(sourceId)}`, options),
  ingest:(slug:string,body:Record<string,unknown>)=>request<{source:InboxSource}>(base(slug),{method:"POST",body:JSON.stringify({...body,confirm:true})}),
  originalUrl:(slug:string,sourceId:string)=>`${base(slug)}/${encodeURIComponent(sourceId)}/original`,
  review:(slug:string,sourceId:string,companySlug:string,sourceHash:string,metadata:DocumentIngestMetadata)=>request<{source:InboxSource}>(`${base(slug)}/${encodeURIComponent(sourceId)}/review`,{method:"POST",body:JSON.stringify({companySlug,sourceHash,metadata,confirm:true})}),
  assign:(slug:string,sourceId:string,companySlug:string)=>request<{source:InboxSource}>(`${base(slug)}/${encodeURIComponent(sourceId)}/assign`,{method:"POST",body:JSON.stringify({companySlug,confirm:true})}),
  complete:(slug:string,sourceId:string,companySlug:string)=>request<{source:InboxSource}>(`${base(slug)}/${encodeURIComponent(sourceId)}/complete`,{method:"POST",body:JSON.stringify({companySlug,confirm:true})}),
};
