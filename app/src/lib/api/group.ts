import type { ConsolidatedReport, ConsolidationEliminations, ConsolidationReportProfiles, GroupOverview, IntercompanyReconciliation } from "../types";
import type { IntercompanyDispositionAction, IntercompanyDispositionActionResponse, IntercompanyDispositionStatus } from "../types/intercompany-dispositions";
import { request, type ReadRequestOptions } from "./_shared";

function assertStructureOnly(value: GroupOverview): GroupOverview {
  if (
    value.scope !== "structure-status-only" ||
    value.consolidationStatus !== "not-available" ||
    value.consolidatedFigures !== null ||
    value.rawCompanySums !== null
  ) {
    throw new Error("Koncernstrukturen kunne ikke bekræftes som ikke-konsolideret.");
  }
  return value;
}

export const groupApi = {
  groupOverview: (asOf: string, options?: ReadRequestOptions) =>
    request<GroupOverview>(`/api/group-overview?asOf=${encodeURIComponent(asOf)}`, options).then(assertStructureOnly),
  groupReconciliation: (asOf: string, options?: ReadRequestOptions) =>
    request<IntercompanyReconciliation>(`/api/group-reconciliation?asOf=${encodeURIComponent(asOf)}`, options).then((value) => {
      if (value.scope !== "intercompany-reconciliation") throw new Error("Mellemregningsafstemningen kunne ikke bekræftes.");
      return value;
    }),
  groupEliminations: (asOf: string, options?: ReadRequestOptions) =>
    request<ConsolidationEliminations>(`/api/group-eliminations?asOf=${encodeURIComponent(asOf)}`, options).then((value) => {
      if (value.scope !== "consolidation-eliminations") throw new Error("Eliminationsevidensen kunne ikke bekræftes.");
      return value;
    }),
  groupConsolidatedReport: (profileId: string, from: string, asOf: string, options?: ReadRequestOptions) =>
    request<ConsolidatedReport>(`/api/group-consolidated-report?profileId=${encodeURIComponent(profileId)}&from=${encodeURIComponent(from)}&asOf=${encodeURIComponent(asOf)}`, options).then((value) => {
      if (value.scope !== "consolidated-report" || (value.status === "ready" && value.consolidatedFigures === null)) throw new Error("Koncernrapporten kunne ikke bekræftes.");
      return value;
    }),
  groupReportProfiles: (asOf: string, options?: ReadRequestOptions) =>
    request<ConsolidationReportProfiles>(`/api/group-report-profiles?asOf=${encodeURIComponent(asOf)}`, options).then((value) => {
      if (value.scope !== "consolidation-report-profiles" || value.asOf !== asOf || !Array.isArray(value.profiles)) {
        throw new Error("Koncernrapportprofilerne kunne ikke bekræftes.");
      }
      return value;
    }),
  intercompanyDispositionStatus: (id: string, asOf?: string, options?: ReadRequestOptions) =>
    request<IntercompanyDispositionStatus>(`/api/group-dispositions/${encodeURIComponent(id)}${asOf ? `?asOf=${encodeURIComponent(asOf)}` : ""}`, options),
  intercompanyDispositionAction: <Action extends IntercompanyDispositionAction>(company: string, action: Action, payload: Record<string, unknown>) =>
    request<IntercompanyDispositionActionResponse<Action>>(`/api/companies/${encodeURIComponent(company)}/group-dispositions/${action}`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload) }),
};
