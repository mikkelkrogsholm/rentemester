import { ApiError, request, requestBlob, type ReadRequestOptions } from "./_shared";
import type { ArchiveApply, ArchiveImportPlan, ArchiveImportReceipt, ArchiveSource, ClosedArchiveYear } from "../types/archive-import";
function checkedYears(value:{ok:true;years:ClosedArchiveYear[];errors:string[]}){if(value?.ok!==true||!Array.isArray(value.years)||!Array.isArray(value.errors)||value.errors.length)throw new ApiError("internal","Arkivets verificerede kilder kunne ikke læses.",200);return value;}
const base=(slug:string)=>`/api/companies/${encodeURIComponent(slug)}`;
export const archiveImportApi={
  plan:(slug:string,source:ArchiveSource,options?:ReadRequestOptions)=>request<{ok:true;plan:ArchiveImportPlan}>(`${base(slug)}/archive-import/plan`,{method:"POST",readOnly:true,signal:options?.signal,body:JSON.stringify(source)}).then(r=>r.plan),
  apply:(slug:string,body:ArchiveApply)=>request<{ok:true;archiveImport:ArchiveImportReceipt}>(`${base(slug)}/archive-import/apply`,{method:"POST",body:JSON.stringify(body)}).then(r=>r.archiveImport),
  list:(slug:string,options?:ReadRequestOptions)=>request<{ok:true;years:ClosedArchiveYear[];errors:string[]}>(`${base(slug)}/archives`,options).then(checkedYears),
  year:(slug:string,year:number,options?:ReadRequestOptions)=>request<{ok:true;years:ClosedArchiveYear[];errors:string[]}>(`${base(slug)}/archives/${year}`,options).then(checkedYears),
  original:(slug:string,importId:number,options?:ReadRequestOptions)=>requestBlob(`${base(slug)}/archive-import/${importId}/original`,options),
};
