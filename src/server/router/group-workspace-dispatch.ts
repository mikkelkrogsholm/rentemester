import { ApiError } from "../errors";

type RouteHandler = (...args: any[]) => Response | Promise<Response>;

/**
 * Matches the workspace and group route family after router.ts has completed
 * authentication and catalogue enforcement. Handler closures keep request
 * configuration and the server's error edge owned by router.ts.
 */
export async function dispatchGroupWorkspaceRoute(
  path: string,
  method: string,
  url: URL,
  request: Request,
  handlers: Record<string, RouteHandler>,
): Promise<Response | null> {
  if (path === "/api/portfolio") {
    if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
    return handlers.portfolio();
  }
  if (path === "/api/cfo-analytics") {
    if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
    return handlers.cfoAnalytics();
  }
  if (path === "/api/me") {
    if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
    return handlers.me();
  }
  if (path === "/api/workspace/invitations") {
    if (method === "GET") return handlers.invitationList();
    if (method === "POST") return handlers.invitationCreate();
    throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
  }
  if (path === "/api/workspace/invitations/cancel") return postOnly(method, handlers.invitationCancel);
  if (path === "/api/workspace/service-principals") {
    if (method === "GET") return handlers.servicePrincipalList();
    if (method === "POST") return handlers.servicePrincipalCreate();
    throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
  }
  if (path === "/api/workspace/service-principals/rotate") return postOnly(method, handlers.servicePrincipalRotate);
  if (path === "/api/workspace/service-principals/revoke") return postOnly(method, handlers.servicePrincipalRevoke);
  if (path === "/api/workspace/service-principals/recover") return postOnly(method, handlers.servicePrincipalRecover);
  if (path === "/api/workspace/members") return getOnly(method, handlers.workspaceMemberList);
  if (path === "/api/workspace/members/access") return postOnly(method, handlers.workspaceMemberAccessUpdate);
  if (path === "/api/workspace/members/company") return postOnly(method, handlers.workspaceMemberCompanyUpdate);
  if (path === "/api/invitations/claim") return postOnly(method, handlers.invitationClaim);

  const inboxComplete = /^\/api\/companies\/([^/]+)\/workspace-inbox\/([^/]+)\/complete$/.exec(path);
  if (inboxComplete) return postOnly(method, () => handlers.workspaceInboxComplete(decodeURIComponent(inboxComplete[1]!), decodeURIComponent(inboxComplete[2]!)));
  const inboxAssign = /^\/api\/companies\/([^/]+)\/workspace-inbox\/([^/]+)\/assign$/.exec(path);
  if (inboxAssign) return postOnly(method, () => handlers.workspaceInboxApprove(decodeURIComponent(inboxAssign[1]!), decodeURIComponent(inboxAssign[2]!)));
  const inboxOne = /^\/api\/companies\/([^/]+)\/workspace-inbox\/([^/]+)$/.exec(path);
  if (inboxOne) return getOnly(method, () => handlers.workspaceInboxInspect(decodeURIComponent(inboxOne[1]!), decodeURIComponent(inboxOne[2]!)));
  const inbox = /^\/api\/companies\/([^/]+)\/workspace-inbox$/.exec(path);
  if (inbox) {
    const slug = decodeURIComponent(inbox[1]!);
    if (method === "GET") return handlers.workspaceInboxList(slug);
    if (method === "POST") return handlers.workspaceInboxIngest(slug);
    throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
  }

  const partyCollection = /^\/api\/companies\/([^/]+)\/workspace-parties$/.exec(path);
  if (partyCollection) return getOrPost(method, () => handlers.registryParties(decodeURIComponent(partyCollection[1]!)), () => handlers.registryPartyCreate(decodeURIComponent(partyCollection[1]!)));
  const partyRole = /^\/api\/companies\/([^/]+)\/workspace-parties\/([^/]+)\/role$/.exec(path);
  if (partyRole) return postOnly(method, () => handlers.registryPartyRole(decodeURIComponent(partyRole[1]!), decodeURIComponent(partyRole[2]!)));
  const partyMerge = /^\/api\/companies\/([^/]+)\/workspace-parties\/merge\/(propose|approve)$/.exec(path);
  if (partyMerge) return postOnly(method, () => handlers.registryPartyMerge(decodeURIComponent(partyMerge[1]!), partyMerge[2] === "approve"));
  const partyOne = /^\/api\/companies\/([^/]+)\/workspace-parties\/([^/]+)$/.exec(path);
  if (partyOne) return getOnly(method, () => handlers.registryParty(decodeURIComponent(partyOne[1]!), decodeURIComponent(partyOne[2]!)));
  const legacyMappingAction = /^\/api\/companies\/([^/]+)\/legacy-party-mappings\/(plan|apply|supersede)$/.exec(path);
  if (legacyMappingAction) {
    const slug=decodeURIComponent(legacyMappingAction[1]!);
    if(legacyMappingAction[2]==="plan")return postOnly(method,()=>handlers.legacyPartyMappingPlan(slug));
    if(legacyMappingAction[2]==="apply")return postOnly(method,()=>handlers.legacyPartyMappingApply(slug));
    return postOnly(method,()=>handlers.legacyPartyMappingSupersede(slug));
  }
  const legacyMappings = /^\/api\/companies\/([^/]+)\/legacy-party-mappings$/.exec(path);
  if (legacyMappings) return getOnly(method, () => handlers.legacyPartyMappings(decodeURIComponent(legacyMappings[1]!)));
  const recordCollection = /^\/api\/companies\/([^/]+)\/corporate-records$/.exec(path);
  if (recordCollection) return getOrPost(method, () => handlers.registryRecords(decodeURIComponent(recordCollection[1]!)), () => handlers.registryRecordIngest(decodeURIComponent(recordCollection[1]!)));
  const recordAction = /^\/api\/companies\/([^/]+)\/corporate-records\/([^/]+)\/(link|enrich|supersede)$/.exec(path);
  if (recordAction) return postOnly(method, () => handlers.registryRecordAction(decodeURIComponent(recordAction[1]!), decodeURIComponent(recordAction[2]!), recordAction[3]!));
  const recordFile = /^\/api\/companies\/([^/]+)\/corporate-records\/([^/]+)\/file$/.exec(path);
  if (recordFile) return getOnly(method, () => handlers.registryRecordDownload(decodeURIComponent(recordFile[1]!), decodeURIComponent(recordFile[2]!)));
  const recordOne = /^\/api\/companies\/([^/]+)\/corporate-records\/([^/]+)$/.exec(path);
  if (recordOne) return getOnly(method, () => handlers.registryRecord(decodeURIComponent(recordOne[1]!), decodeURIComponent(recordOne[2]!)));
  const knowledge = /^\/api\/companies\/([^/]+)\/knowledge$/.exec(path);
  if (knowledge) return getOnly(method, () => handlers.companyKnowledge(decodeURIComponent(knowledge[1]!)));
  const knowledgeAction = /^\/api\/companies\/([^/]+)\/knowledge\/(propose|review|supersede)$/.exec(path);
  if (knowledgeAction) return postOnly(method, () => handlers.companyKnowledgeAction(decodeURIComponent(knowledgeAction[1]!), knowledgeAction[2]!));
  const ownershipHistory = /^\/api\/companies\/([^/]+)\/ownership\/history$/.exec(path);
  if (ownershipHistory) return getOnly(method, () => handlers.ownershipHistory(decodeURIComponent(ownershipHistory[1]!)));
  const ownershipAction = /^\/api\/companies\/([^/]+)\/ownership\/(propose|review|apply)$/.exec(path);
  if (ownershipAction) return postOnly(method, () => handlers.ownershipAction(decodeURIComponent(ownershipAction[1]!), ownershipAction[2]!));
  const ownership = /^\/api\/companies\/([^/]+)\/ownership$/.exec(path);
  if (ownership) return getOnly(method, () => handlers.ownershipQuery(decodeURIComponent(ownership[1]!)));

  if (path === "/api/group-overview") return groupAsOf(method, url, handlers.groupOverview);
  if (path === "/api/group-reconciliation") return groupAsOf(method, url, handlers.groupReconciliation);
  if (path === "/api/group-eliminations") return groupAsOf(method, url, handlers.groupEliminations);
  if (path === "/api/group-consolidated-report") {
    if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
    const profileIds = url.searchParams.getAll("profileId");
    const fromValues = url.searchParams.getAll("from");
    const asOfValues = url.searchParams.getAll("asOf");
    if (profileIds.length !== 1 || fromValues.length !== 1 || asOfValues.length !== 1) throw ApiError.badRequest("exactly one profileId, from and asOf are required");
    return handlers.groupConsolidatedReport(profileIds[0]!, fromValues[0]!, asOfValues[0]!);
  }
  if (path === "/api/group-report-profiles") return groupAsOf(method, url, handlers.groupReportProfiles);
  const dispositionStatus = /^\/api\/group-dispositions\/([^/]+)$/.exec(path);
  if (dispositionStatus) return getOnly(method, () => handlers.groupDispositionStatus(decodeURIComponent(dispositionStatus[1]!), url.searchParams.get("asOf") ?? undefined));
  const dispositionAction = /^\/api\/companies\/([^/]+)\/group-dispositions\/(plan|propose|approve|link|settle|supersede|reopen)$/.exec(path);
  if (dispositionAction) return postOnly(method, () => handlers.groupDispositionAction(decodeURIComponent(dispositionAction[1]!), dispositionAction[2]!));
  return null;
}

function getOnly(method: string, handler: RouteHandler): Response | Promise<Response> {
  if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
  return handler();
}

function postOnly(method: string, handler: RouteHandler): Response | Promise<Response> {
  if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
  return handler();
}

function getOrPost(method: string, get: RouteHandler, post: RouteHandler): Response | Promise<Response> {
  if (method === "GET") return get();
  if (method === "POST") return post();
  throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
}

function groupAsOf(method: string, url: URL, handler: RouteHandler): Response | Promise<Response> {
  if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
  const asOfValues = url.searchParams.getAll("asOf");
  if (asOfValues.length !== 1) throw ApiError.badRequest("exactly one asOf is required as YYYY-MM-DD");
  return handler(asOfValues[0]!);
}
