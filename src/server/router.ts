// HTTP request handler for the cockpit backend (#170).
//
// `handleRequest` is the pure heart of the server: a `(Request, config) =>
// Promise<Response>` with no `Bun.serve` dependency, so tests drive it
// directly. `src/cli/serve.ts` wires it into `Bun.serve`.
//
// Request flow, in order, for EVERY request:
//   1. authMiddleware  — the single auth seam (throws ApiError to reject)
//   2. route dispatch  — match method + path
//   3. handler         — a read, a workspace-management op, or a write
//   4. error edge      — any throw is mapped to a safe JSON error here
//
// No handler does its own auth and no handler shapes its own errors: both
// concerns live exactly once, here. Bookkeeping WRITE routes (#213) go through
// the `withCompanyMutation` pipeline in `mutations.ts`, which adds the backup
// lock, the confirm gate, actor attribution and the localhost hard-gate that
// the agent CLI / MCP stacks enforce — the server does not inherit them.

import { describeWorkflow, searchCapabilities } from "../agent-discovery-catalog";
import { MUTATING_COMMANDS } from "../cli-actor";
import { COMMAND_SPECS, SIDE_EFFECTING_COMMANDS } from "../cli-meta";
import type { RoutePermission } from "../core/access-permissions";
import type { ServerConfig } from "./config";

export type { RoutePermission } from "../core/access-permissions";

import { findRoutableWorkspaceCompany, isValidSlug } from "../core/workspace";
import { authorizeWorkspaceRoute } from "../core/workspace-access";
import { insertWorkspaceAuthorizationAudit, openWorkspaceControlDb, openWorkspaceControlReadOnlyDb } from "../core/workspace-control";
import { authMiddleware, type Principal } from "./auth";
import { recordHostedDocumentAccess } from "./document-access-audit";
import { ApiError, toErrorResponse } from "./errors";
import { assertHostedMutationOriginAllowed } from "./mutations";
import { jsonResponse } from "./router/_shared";
import { dispatchAgentDiscoveryRoute } from "./router/agent-discovery-dispatch";
import { dispatchGroupWorkspaceRoute } from "./router/group-workspace-dispatch";
import { dispatchSystemRoute } from "./router/system-dispatch";
import { handleCompanyAccountingDraft, handleCompanyAccountingDrafts } from "./router/accounting-drafts";
import {
  handleAssetNextDepreciation,
  handleCompanyAssets,
} from "./router/assets";
import {
  handleCompanyBank,
  handleCompanyBankAccounts,
  handleBankReconciliationCorrectionPlan, handleLegacyBankBindingApply, handleLegacyBankBindingPlan,
} from "./router/bank";
import { handleBookkeepingBatchApply, handleBookkeepingBatchApprove, handleBookkeepingBatchDryRun, handleBookkeepingBatchPersistDryRun, handleBookkeepingBatchStatus } from "./router/bookkeeping-batch";
import { handleAccountingApprovalPolicyGet, handleAccountingApprovalPolicySet } from "./router/accounting-approval-policy";
import { handleBookkeepingWorkbench } from "./router/bookkeeping-workbench";
import { handlePurchaseCaseCreate, handlePurchaseCaseGet, handlePurchaseCaseGroupReview, handlePurchaseCaseList, handlePurchaseCaseReassess, handlePurchaseCaseReview, handlePurchaseOverview } from "./router/purchase-cases";
import { handleDimensionAction, handleDimensionAssignments, handleDimensionBudgets, handleDimensionBudgetPlan, handleDimensionDefinitions, handleDimensionMembers, handleDimensionPlan } from "./router/dimensions";
import {
  handleCompanyAccounts,
  handleCompanyAccruals,
  handleCompanyAgentSuggestions,
  handleCompanyAnnualReport,
  handleCompanyArchiveYear,
  handleCompanyBilagsmail,
  handleCompanyBudget,
  handleCompanyBudgetDimensionActuals,
  handleCompanyBudgetVsActual,
  handleCompanyCashflow,
  handleCompanyExceptions,
  handleCompanyIntegrity,
  handleCompanyMileage,
  handleCompanyObligations,
  handleCompanyPayables,
  handleCompanyPeriods,
  handleCompanyRetention,
  handleCompanySettings,
  handleCompanySyncCvr,
} from "./router/company";
import { handleCompanyContacts } from "./router/contacts";
import {
  handleCompanyDashboard,
  handleCompanyFiscalYears,
  handleCompanyMultiYear,
  handleCompanyOverview,
} from "./router/dashboard";
import {
  handleCompanyDocumentBookingOptions,
  handleCompanyDocumentFile,
  handleCompanyDocumentInvoiceExtraction,
  handleCompanyDocumentParsedText,
  handleCompanyDocumentParseStatus,
  handleCompanyDocuments,
  handleCompanyDocumentVatPreflight,
  handleDocumentPartyLinks,
  handleDocumentPartyLinkInspect,
  handleDocumentPartyLinkPlan,
  handleDocumentPartyLinkAction,
  handleDocumentCompanyContext,
  handleDocumentPurchaseVatEvidenceReview,
  handleInternalNoExternalParty,
  handlePartyCoverage, handlePartyCoveragePlan, handlePartyCoverageApply,
} from "./router/documents";
import { handleGroupConsolidatedReport, handleGroupDispositionAction, handleGroupDispositionStatus, handleGroupEliminations, handleGroupOverview, handleGroupReconciliation, handleGroupReportProfiles } from "./router/group";
import {
  handleCompanyImportedReceivables,
  handleCompanyImportedReceivablesBackfillApply,
  handleCompanyImportedReceivablesBackfillPlan,
  handleCompanyImportedReceivableSettlementApply,
  handleCompanyImportedReceivableSettlementPlan,
  handleCompanyImportedReceivableSettlementStatus,
  handleCompanyInvoicePdf,
  handleCompanyInvoices,
  handleCompanyRecurringInvoices,
} from "./router/invoices";
import { handleMe } from "./router/me";
import {
  handleCompanyList,
  handlePortfolio,
} from "./router/portfolio";
import { handleCfoAnalytics } from "./router/cfo-analytics";
import { handleCompanyPostingRuleExplain, handleCompanyPostingRules } from "./router/posting-rules";
import { handleServicePrincipalCreate, handleServicePrincipalList, handleServicePrincipalRecover, handleServicePrincipalRevoke, handleServicePrincipalRotate } from "./router/service-principals";
import { handleCompanyKnowledge, handleCompanyKnowledgeAction, handleLegacyPartyMappingApply, handleLegacyPartyMappingPlan, handleLegacyPartyMappings, handleLegacyPartyMappingSupersede, handleVendorIdentityEnrichmentApply, handleVendorIdentityEnrichmentPlan, handleVendorIdentityEnrichments, handleOwnershipAction, handleOwnershipHistory, handleOwnershipQuery, handleRegistryParties, handleRegistryParty, handleRegistryPartyCreate, handleRegistryPartyMerge, handleRegistryPartyRole, handleRegistryRecord, handleRegistryRecordAction, handleRegistryRecordDownload, handleRegistryRecordIngest, handleRegistryRecords } from "./router/workspace-registry";
import { handleWorkspaceInboxApprove, handleWorkspaceInboxComplete, handleWorkspaceInboxIngest, handleWorkspaceInboxInspect, handleWorkspaceInboxList } from "./router/workspace-document-inbox";
import {
  handleCompanyBalance,
  handleCompanyIncomeStatement,
  handleCompanyJournal,
  handleCompanyJournalExport,
  handleCompanyStatementExport,
  handleCompanyTrialBalance,
  handleCompanyVatExport,
} from "./router/statements";
import {
  handleHealth,
  handleReadiness,
  handleRules,
  handleSystemCvrStatus,
} from "./router/system";
import { handleCompanyVat } from "./router/vat";
import { handleSupplierCommitmentApply, handleSupplierCommitmentChange, handleSupplierCommitmentMatch, handleSupplierCommitmentMatches, handleSupplierCommitmentPlan, handleSupplierCommitments } from "./router/supplier-commitments";
import {
  handleWorkspaceInvitationCancel,
  handleWorkspaceInvitationClaim,
  handleWorkspaceInvitationCreate,
  handleWorkspaceInvitationList,
} from "./router/workspace-invitations";
import {
  handleWorkspaceMemberAccessUpdate,
  handleWorkspaceMemberCompanyUpdate,
  handleWorkspaceMemberList,
} from "./router/workspace-members";
import {
  handleCompanyCreate,
  handleCompanyUpdate,
} from "./router/workspace-writes";
import { AUTH_SESSION_FRESH_AGE_SECONDS } from "./security-policy";
import { serveStatic } from "./static";
import {
  handleAccountantExport,
  handleApproveAgentSuggestion,
  handleApproveAndPostAccountingDraft,
  handleAssetDepreciate,
  handleAssetRegister,
  handleAssetWriteOff,
  handleBankImport,
  handleBankReconciliationCorrectionApply,
  handleClosePeriod,
  handleCompanyProfile,
  handleCreateAccountingDraft,
  handleCreateBankAccount,
  handleCreateCustomer,
  handleCreateRecurringInvoiceTemplate,
  handleCreateVendor,
  handleCvrLookup,
  handleDataImport,
  handleDeleteBilagsmailImapConfig,
  handleDeleteCustomer,
  handleDeleteVendor,
  handleDocumentBookExpense,
  handleDocumentIngest,
  handleDocumentPdfParse,
  handleDocumentPdfParsePending,
  handleDocumentVatPreflightApply,
  handleGdprErase,
  handleGdprExport,
  handleGenerateRecurringInvoice,
  handleInvoiceCreditNote,
  handleInvoiceIssue,
  handleInvoicePost,
  handleInvoicePreview,
  handleInvoiceSendEmail,
  handleInvoiceSendPublic,
  handleInvoiceSendPublicStatus,
  handleInvoiceSendReminder,
  handleInvoiceSettle,
  handleMileageCreate,
  handlePayablePay,
  handlePayableRegister,
  handleDirectBankPurchasePayablePlan,
  handleDirectBankPurchasePayableApply,
  handleLegacyPayableBackfillPlan,
  handleLegacyPayableBackfillApply,
  handlePeriodCloseReadiness,
  handlePeriodCloseReview,
  handlePeriodCloseStatus,
  handleRejectAccountingDraft,
  handleRejectAgentSuggestion,
  handleReopenPeriod,
  handleResolveException,
  handleRetireRecurringInvoiceTemplate,
  handleReviseAccountingDraft,
  handleSaveBilagsmailImapConfig,
  handleSetBilagsmailAlias,
  handleSetBudget,
  handleSubmitAccountingDraft,
  handleUpdateBankAccount,
  handleUpdateCustomer,
  handleUpdateVendor,
} from "./write-handlers";
import { handlePostingRuleMutation } from "./write-handlers/posting-rules";

// --------------------------------------------------------------------------
// Route handlers — reads + workspace management only.
// --------------------------------------------------------------------------

/**
 * The HTTP route catalog (#376) — a machine-readable list of every route
 * `handleRequest` dispatches, used by `GET /api` and `GET /api/health` so an
 * agent can enumerate the HTTP surface without reading source. Each entry
 * carries the `method`, the path `pattern` (with `:param`-placeholders) and a
 * short Danish `summary`. Order is the dispatch order in `handleRequest` to
 * make drift obvious in code review.
 *
 * The list is exported so `tests/unit/surface-diff-discoverable.test.ts` can
 * assert that it stays the single source of truth for the catalog.
 */
export type RouteScope = "public" | "workspace" | "company";
export type RouteEffect = "read" | "write" | "external";
export type RouteCatalogEntry = {
  method: string;
  pattern: string;
  summary: string;
  scope: RouteScope;
  effect: RouteEffect;
  permission: RoutePermission;
};

type RouteCatalogInput = RouteCatalogEntry;
/** Fails closed if a future route supplies contradictory capability metadata. */
export function validateRouteCatalog(entries: readonly RouteCatalogEntry[]): void {
  for (const entry of entries) {
    if (!entry.scope || !entry.effect || !entry.permission) {
      throw new Error(`route metadata missing for ${entry.method} ${entry.pattern}`);
    }
    if (entry.scope === "public" &&
      entry.permission !== "public.read" &&
      entry.permission !== "public.invitation.claim") {
      throw new Error(`public route has non-public permission: ${entry.pattern}`);
    }
    if (entry.scope === "public" && entry.permission === "public.read" && entry.effect !== "read") {
      throw new Error(`public route has non-read effect: ${entry.pattern}`);
    }
    if (entry.permission === "public.invitation.claim" &&
      (entry.pattern !== "/api/invitations/claim" || entry.method !== "POST" || entry.effect !== "write")) {
      throw new Error("invitation claim permission is restricted to its one token-bearing route");
    }
    // Credential inventory is read-only but deliberately owner-only, because
    // its service-account identities are sensitive workspace metadata.
    const ownerOnlyCredentialInventory = entry.pattern === "/api/workspace/service-principals" && entry.permission === "workspace.manage";
    if (entry.effect === "read" && /(?:\.write|\.manage|\.external)$/.test(entry.permission) && !ownerOnlyCredentialInventory) {
      throw new Error(`read route has mutating permission: ${entry.pattern}`);
    }
    if (entry.effect === "external" &&
      entry.permission !== "company.external-lookup" &&
      entry.permission !== "company.external-send") {
      throw new Error(`external route has non-external permission: ${entry.pattern}`);
    }
    if (entry.scope === "company" && !entry.permission.startsWith("company.")) {
      throw new Error(`company route has wrong permission scope: ${entry.pattern}`);
    }
    if (entry.scope === "workspace" && !entry.permission.startsWith("workspace.")) {
      throw new Error(`workspace route has wrong permission scope: ${entry.pattern}`);
    }
  }
}

const ROUTE_CATALOG_INPUT: readonly RouteCatalogInput[] = [
  { scope: "public", effect: "read", permission: "public.read", method: "GET", pattern: "/api", summary: "Sundhedstjek + rute-katalog." },
  { scope: "public", effect: "read", permission: "public.read", method: "GET", pattern: "/api/health", summary: "Alias for GET /api." },
  { scope: "public", effect: "read", permission: "public.read", method: "GET", pattern: "/api/ready", summary: "Read-only readiness for workspace, control DB and registered ledgers." },
  { scope: "public", effect: "read", permission: "public.read", method: "GET", pattern: "/api/rules", summary: "Lovgrundlag — bundler, regler og SHA-256-citationer (#347)." },
  { scope: "public", effect: "read", permission: "public.read", method: "GET", pattern: "/api/agent-capabilities", summary: "Versioneret, pagineret agent-kapabilitetssøgning (#584)." },
  { scope: "public", effect: "read", permission: "public.read", method: "GET", pattern: "/api/agent-workflows/:id", summary: "Versioneret agent-workflow med live HTTP/CLI-opløsning (#584)." },
  { scope: "workspace", effect: "read", permission: "workspace.read", method: "GET", pattern: "/api/system/cvr-status", summary: "Er CVR-login konfigureret på serveren? (#402)" },
  { scope: "workspace", effect: "read", permission: "workspace.read", method: "GET", pattern: "/api/portfolio", summary: "Workspace-portfolio." },
  { scope: "workspace", effect: "read", permission: "workspace.read", method: "GET", pattern: "/api/cfo-analytics", summary: "Versioneret, kildehenvisende CFO-analyse uden ledger-mutation." },
  { scope: "workspace", effect: "read", permission: "workspace.read", method: "GET", pattern: "/api/me", summary: "Sikker hosted bruger- og medlemskabs-kontekst." },
  { scope: "workspace", effect: "read", permission: "workspace.members.read", method: "GET", pattern: "/api/workspace/invitations", summary: "Lister workspace-invitationer uden tokens." },
  { scope: "workspace", effect: "write", permission: "workspace.members.manage", method: "POST", pattern: "/api/workspace/invitations", summary: "Opretter og leverer en tidsbegrænset workspace-invitation." },
  { scope: "workspace", effect: "write", permission: "workspace.members.manage", method: "POST", pattern: "/api/workspace/invitations/cancel", summary: "Annullerer en ubrugt workspace-invitation." },
  { scope: "workspace", effect: "read", permission: "workspace.members.read", method: "GET", pattern: "/api/workspace/members", summary: "Lister aktive workspace-brugere og kun administrerbare selskabsmedlemskaber." },
  { scope: "workspace", effect: "write", permission: "workspace.members.manage", method: "POST", pattern: "/api/workspace/members/access", summary: "Ændrer workspace-rolle eller deaktiverer en bruger append-only." },
  { scope: "workspace", effect: "write", permission: "workspace.members.manage", method: "POST", pattern: "/api/workspace/members/company", summary: "Ændrer adgang til ét selskab append-only." },
  { scope: "workspace", effect: "read", permission: "workspace.manage", method: "GET", pattern: "/api/workspace/service-principals", summary: "Lister servicekonti uden credentials." },
  { scope: "workspace", effect: "write", permission: "workspace.manage", method: "POST", pattern: "/api/workspace/service-principals", summary: "Opretter servicekonto og viser credential én gang." },
  { scope: "workspace", effect: "write", permission: "workspace.manage", method: "POST", pattern: "/api/workspace/service-principals/rotate", summary: "Roterer servicecredential og viser den nye nøgle én gang." },
  { scope: "workspace", effect: "write", permission: "workspace.manage", method: "POST", pattern: "/api/workspace/service-principals/revoke", summary: "Tilbagekalder servicecredential." },
  { scope: "workspace", effect: "write", permission: "workspace.manage", method: "POST", pattern: "/api/workspace/service-principals/recover", summary: "Afslutter sikkert en afbrudt servicecredential-operation uden at vise en nøgle." },
  { scope: "public", effect: "write", permission: "public.invitation.claim", method: "POST", pattern: "/api/invitations/claim", summary: "Indløser en e-mailbundet invitation uden at oprette en session." },
  { scope: "workspace", effect: "read", permission: "workspace.group.read", method: "GET", pattern: "/api/group-overview", summary: "Koncernstruktur og status uden konsoliderede tal." },
  { scope: "workspace", effect: "read", permission: "workspace.group.read", method: "GET", pattern: "/api/group-reconciliation", summary: "Eksakt read-only mellemregningsafstemning med kildehenvisninger." },
  { scope: "workspace", effect: "read", permission: "workspace.group.read", method: "GET", pattern: "/api/group-eliminations", summary: "Anvendte, append-only balanceelimineringer uden selskabsledger-skrivning." },
  { scope: "workspace", effect: "read", permission: "workspace.group.read", method: "GET", pattern: "/api/group-consolidated-report", summary: "Godkendt, read-only konsolideret resultat og balance med kildeevidens." },
  { scope: "workspace", effect: "read", permission: "workspace.group.read", method: "GET", pattern: "/api/group-report-profiles", summary: "Lister kun aktive, godkendte og fuldt synlige konsolideringsprofiler." },
  { scope: "workspace", effect: "read", permission: "workspace.group.read", method: "GET", pattern: "/api/group-dispositions/:id", summary: "Read-only status for two-sided intercompany evidence with redacted endpoint boundary." },
  { scope: "workspace", effect: "write", permission: "workspace.manage", method: "POST", pattern: "/api/companies/:slug/group-dispositions/:action", summary: "Confirmed intercompany disposition lifecycle; core additionally requires live narrow access to both legal entities." },
  { scope: "workspace", effect: "read", permission: "workspace.read", method: "GET", pattern: "/api/companies", summary: "Lister virksomheder i workspacet." },
  { scope: "workspace", effect: "write", permission: "workspace.manage", method: "POST", pattern: "/api/companies", summary: "Opretter virksomhed i workspacet." },
  { scope: "company", effect: "write", permission: "company.admin", method: "PATCH", pattern: "/api/companies/:slug", summary: "Omdøber/arkiverer en virksomhed." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/workspace-parties", summary: "Synlige canonical workspace parties (#573)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/workspace-parties/:partyId", summary: "Synlig party-provenance (#573)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/workspace-parties", summary: "Opretter party + lokal rolle (#573)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/workspace-parties/:partyId/role", summary: "Knytter company-scoped party role (#573)." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/workspace-parties/merge/propose", summary: "Foreslår party merge (#573)." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/workspace-parties/merge/approve", summary: "Godkender party merge (#573)." },
  { scope: "company", effect: "read", permission: "company.read", method: "POST", pattern: "/api/companies/:slug/legacy-party-mappings/plan", summary: "Planlægger dokument-hashbundet legacy contact mapping (#638)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/legacy-party-mappings", summary: "Lister append-only legacy contact mapping history (#638)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/legacy-party-mappings/apply", summary: "Anvender eksakt reviewet legacy contact mapping (#638)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/legacy-party-mappings/supersede", summary: "Supersederer legacy contact mapping append-only (#638)." },
  { scope: "company", effect: "read", permission: "company.read", method: "POST", pattern: "/api/companies/:slug/vendor-identity-enrichments/plan", summary: "Planlægger byte-bundet leverandør-identitetsberigelse (#638)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/vendor-identity-enrichments", summary: "Lister append-only leverandør-identitetsberigelser (#638)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/vendor-identity-enrichments/apply", summary: "Anvender eksakt reviewet leverandør-identitetsberigelse (#638)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/corporate-records", summary: "Synlige immutable corporate records (#575)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/corporate-records/:recordId", summary: "Corporate record metadata (#575)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/corporate-records/:recordId/file", summary: "Verificerede corporate record bytes (#575)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/corporate-records", summary: "Indlæser immutable corporate record (#575)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/corporate-records/:recordId/link", summary: "Knytter corporate record scope (#575)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/corporate-records/:recordId/enrich", summary: "Beriger corporate record append-only (#575)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/corporate-records/:recordId/supersede", summary: "Supersederer corporate record append-only (#575)." },
  { scope: "company", effect: "read", permission: "company.knowledge.read", method: "GET", pattern: "/api/companies/:slug/knowledge", summary: "Kompakt, kildeunderbygget virksomheds-kontekst (#574)." },
  { scope: "company", effect: "write", permission: "company.knowledge.manage", method: "POST", pattern: "/api/companies/:slug/knowledge/propose", summary: "Foreslår en dateret knowledge assertion (#574)." },
  { scope: "company", effect: "write", permission: "company.knowledge.manage", method: "POST", pattern: "/api/companies/:slug/knowledge/review", summary: "Godkender eller afviser en præcis knowledge assertion (#574)." },
  { scope: "company", effect: "write", permission: "company.knowledge.manage", method: "POST", pattern: "/api/companies/:slug/knowledge/supersede", summary: "Supersederer godkendt knowledge append-only (#574)." },
  { scope: "company", effect: "read", permission: "company.ownership.read", method: "GET", pattern: "/api/companies/:slug/ownership", summary: "Synligt, party-aware ownership/control graph (#576)." },
  { scope: "company", effect: "read", permission: "company.ownership.read", method: "GET", pattern: "/api/companies/:slug/ownership/history", summary: "Append-only ownership snapshot history (#576)." },
  { scope: "company", effect: "write", permission: "company.ownership.manage", method: "POST", pattern: "/api/companies/:slug/ownership/propose", summary: "Foreslår kilde-hashet ownership snapshot (#576)." },
  { scope: "company", effect: "write", permission: "company.ownership.manage", method: "POST", pattern: "/api/companies/:slug/ownership/review", summary: "Reviewer ownership snapshot (#576)." },
  { scope: "company", effect: "write", permission: "company.ownership.manage", method: "POST", pattern: "/api/companies/:slug/ownership/apply", summary: "Anvender eksakt approved ownership diff (#576)." },
  { scope: "company", effect: "read", permission: "company.documents.read", method: "GET", pattern: "/api/companies/:slug/workspace-inbox", summary: "Workspace-indbakke uden ledger-effekt (#577)." },
  { scope: "company", effect: "write", permission: "company.documents.upload", method: "POST", pattern: "/api/companies/:slug/workspace-inbox", summary: "Indlæser immutable workspace-kilde (#577)." },
  { scope: "company", effect: "read", permission: "company.documents.read", method: "GET", pattern: "/api/companies/:slug/workspace-inbox/:sourceId", summary: "Inspectorerer en synlig workspace-kilde (#577)." },
  { scope: "company", effect: "write", permission: "company.documents.upload", method: "POST", pattern: "/api/companies/:slug/workspace-inbox/:sourceId/assign", summary: "Godkender en workspace-ruting (#577)." },
  { scope: "company", effect: "write", permission: "company.documents.upload", method: "POST", pattern: "/api/companies/:slug/workspace-inbox/:sourceId/complete", summary: "Overdrager én godkendt kilde til canonical company-ingest (#577)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/dashboard", summary: "Virksomhedens dashboard." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/fiscal-years", summary: "Kendte regnskabsår." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/overview", summary: "Nøgletalsoverblik." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/income-statement", summary: "Resultatopgørelse." },
  { scope: "company", effect: "read", permission: "company.export", method: "GET", pattern: "/api/companies/:slug/income-statement/export", summary: "Resultatopgørelse som CSV-download (#372)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/balance", summary: "Balance." },
  { scope: "company", effect: "read", permission: "company.export", method: "GET", pattern: "/api/companies/:slug/balance/export", summary: "Balance som CSV-download (#372)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/trial-balance", summary: "Saldobalance." },
  { scope: "company", effect: "read", permission: "company.export", method: "GET", pattern: "/api/companies/:slug/trial-balance/export", summary: "Saldobalance som CSV-download (#372)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/journal", summary: "Journalposter." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/accounting-drafts", summary: "Bogføringskladder og deres seneste reviewtilstand." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/accounting-approval-policy", summary: "Aktuel versioneret approval-policy; historisk elevated-status er eksplicit ikke-håndhævet." },
  { scope: "company", effect: "write", permission: "company.admin", method: "POST", pattern: "/api/companies/:slug/accounting-approval-policy", summary: "Ændrer selskabets append-only normal approval-policy med confirm; elevated aktivering afvises." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/posting-rules", summary: "Selskabslokale posteringsregler." },
  { scope: "company", effect: "read", permission: "company.read", method: "POST", pattern: "/api/companies/:slug/posting-rules/explain", summary: "Dry-run med præcise match- og afvigelsesårsager." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/posting-rules/propose", summary: "Opretter et regel-forslag med confirm." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/posting-rules/approve", summary: "Godkender en præcis regelversion med confirm." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/posting-rules/disable", summary: "Deaktiverer en regelversion med confirm." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/posting-rules/supersede", summary: "Erstatter en regelversion med confirm." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/accounting-drafts/:draftId", summary: "Én bogføringskladde med præcis event-hash." },
  { scope: "company", effect: "write", permission: "company.draft.write", method: "POST", pattern: "/api/companies/:slug/accounting-drafts", summary: "Opretter en append-only bogføringskladde." },
  { scope: "company", effect: "write", permission: "company.draft.write", method: "POST", pattern: "/api/companies/:slug/accounting-drafts/:draftId/revise", summary: "Opretter en ny version af en redigerbar bogføringskladde." },
  { scope: "company", effect: "write", permission: "company.draft.write", method: "POST", pattern: "/api/companies/:slug/accounting-drafts/:draftId/submit", summary: "Indsender den præcise kladde-version til review." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/accounting-drafts/:draftId/reject", summary: "Afviser en indsendt kladde med begrundelse." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/accounting-drafts/:draftId/approve-and-post", summary: "Godkender og bogfører atomisk den præcise indsendte kladde." },
  { scope: "company", effect: "read", permission: "company.export", method: "GET", pattern: "/api/companies/:slug/journal/export", summary: "Posteringer (kassekladde) som CSV-download (#465)." },
  { scope: "company", effect: "read", permission: "company.export", method: "GET", pattern: "/api/companies/:slug/vat/export", summary: "Moms-rapport som PDF-download m. SKAT-rubrikker + frist (#464)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/retention", summary: "5-års retention-status pr. data-domæne (#343)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/integrity", summary: "Audit chain + backup status panel (#333)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/accounts", summary: "Kontoplan — read-only liste (#344)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/bank", summary: "Bank-transaktioner." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/bank/reconciliation-correction-plan", summary: "Read-only plan for bankafstemningskorrektion." },
  { scope:"company",effect:"read",permission:"company.read",method:"POST",pattern:"/api/companies/:slug/bank/legacy-binding/plan",summary:"Read-only NULL-only plan for legacy bank binding (#601)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/vat", summary: "Momsoplysninger." },
  { scope: "company", effect: "read", permission: "company.documents.read", method: "GET", pattern: "/api/companies/:slug/documents", summary: "Bilagsliste." },
  { scope: "company", effect: "read", permission: "company.documents.read", method: "GET", pattern: "/api/companies/:slug/documents/:id/file", summary: "Henter et bilag." },
  { scope: "company", effect: "read", permission: "company.documents.read", method: "GET", pattern: "/api/companies/:slug/documents/:id/booking-options", summary: "Forslagsdata til bogføring af et bilag." },
  { scope: "company", effect: "read", permission: "company.documents.read", method: "GET", pattern: "/api/companies/:slug/documents/:id/vat-preflight", summary: "Købsmoms-preflight uden provider-kald." },
  { scope: "company", effect: "read", permission: "company.documents.read", method: "GET", pattern: "/api/companies/:slug/documents/:id/invoice-extraction", summary: "Citeret fakturaudtræk uden filsti eller secrets." },
  { scope: "company", effect: "read", permission: "company.documents.read", method: "GET", pattern: "/api/companies/:slug/documents/:id/parse-status", summary: "PDF-parserstatus uden child-stderr." },
  { scope: "company", effect: "read", permission: "company.documents.read", method: "GET", pattern: "/api/companies/:slug/documents/:id/parsed-text", summary: "Pagineret PDF-tekst, højst 10 sider." },
  { scope: "company", effect: "read", permission: "company.documents.read", method: "GET", pattern: "/api/companies/:slug/documents/party-links", summary: "Kanoniske partskoblinger pr. bilag (#588)." },
  { scope: "company", effect: "read", permission: "company.documents.read", method: "GET", pattern: "/api/companies/:slug/documents/:id/party-links", summary: "Append-only partskoblingshistorik (#588)." },
  { scope: "company", effect: "read", permission: "company.documents.read", method: "POST", pattern: "/api/companies/:slug/documents/party-links/plan", summary: "Read-only partskoblingsplan (#588)." },
  { scope:"company",effect:"read",permission:"company.documents.read",method:"GET",pattern:"/api/companies/:slug/documents/party-coverage",summary:"Deterministisk bank-til-part coverage med særskilt juridisk, observeret og uafklaret ekstern status (#644/#645)." },
  { scope:"company",effect:"read",permission:"company.documents.read",method:"POST",pattern:"/api/companies/:slug/documents/party-coverage/plan",summary:"Hash-bundet plan for sikre links og reviewede kilde-/opfølgningsbeslutninger (#644/#645)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/bookkeeping-batch", summary: "Read-only batchplan med plan-hash og partitioner." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/bookkeeping-workbench", summary: "Kanonisk, read-only bankarbejdskø med batch- og periodeluk-drilldown." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/purchase-cases", summary: "Kildebundne foreløbige købscases uden ledger-mutation." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/purchase-cases/:caseId", summary: "Aktuel afledt købscase med kilde- og evidensstatus." },
  { scope: "company", effect: "write", permission: "company.draft.write", method: "POST", pattern: "/api/companies/:slug/purchase-cases", summary: "Opretter append-only foreløbig købscase." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/purchase-cases/:caseId/review", summary: "Appender eksakt kildebundet købscase-review." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/purchase-cases/:caseId/reassess", summary: "Appender eksplicit genvurdering mod ændret kilde." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/purchase-overview", summary: "Read-only overblik over purchase cases og dokumenterede behov." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/purchase-cases/group-review", summary: "Atomisk review af en eksakt gruppe purchase cases." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/dimensions/:journalLineId", summary: "Append-only dimensionshistorik for en journal linje (#589)." },
  { scope: "company", effect: "read", permission: "company.read", method: "POST", pattern: "/api/companies/:slug/dimensions/plan", summary: "Read-only hash-bundet dimensionsplan (#589)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/dimensions/define", summary: "Opretter dimensionsdefinition (#589)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/dimensions/member", summary: "Opretter dimensionsmedlem (#589)." },
  { scope: "company", effect: "write", permission: "company.draft.write", method: "POST", pattern: "/api/companies/:slug/dimensions/apply", summary: "Anvender eksakt reviewet dimensionsplan (#589)." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/dimensions/replace", summary: "Erstatter atomisk en eksakt reviewet dimensionsklassifikation (#589)." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/dimensions/supersede", summary: "Superseder dimensionsklassifikation append-only (#589)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/dimensions", summary: "Append-only dimensionsdefinitioner (#589)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/dimensions/budgets", summary: "Reviewede dimensionsbudgetter (#589)." },
  { scope: "company", effect: "read", permission: "company.read", method: "POST", pattern: "/api/companies/:slug/dimensions/budget-plan", summary: "Read-only hash-bundet dimensionsbudget-plan (#589)." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/dimensions/budget-apply", summary: "Anvender en eksakt reviewet dimensionsbudget-fordeling (#589)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/dimensions/members", summary: "Append-only dimensionsmedlemmer (#589)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/dimensions/definition-lifecycle", summary: "Appender lifecycle for definition (#589)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/dimensions/member-lifecycle", summary: "Appender lifecycle for medlem (#589)." },
  { scope: "company", effect: "write", permission: "company.draft.write", method: "POST", pattern: "/api/companies/:slug/bookkeeping-batch/persist", summary: "Persisterer en reviewbar batchrevision." },
  { scope: "company", effect: "write", permission: "company.draft.write", method: "POST", pattern: "/api/companies/:slug/bookkeeping-batch/dry-run", summary: "Kompatibilitetsalias for persist." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/bookkeeping-batch/approve", summary: "Godkender eksakt batch-hash." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/bookkeeping-batch/runs/:runId", summary: "Append-only batchhistorik." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/bookkeeping-batch/apply", summary: "Anvender eller genoptager præcis hash-bundet batch med confirm." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/documents/:id/vat-preflight/apply", summary: "Henter nødvendig købsmoms-evidens før bogføring." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/recurring-invoices", summary: "Gentagende fakturaer." },
  { scope: "company", effect: "write", permission: "company.draft.write", method: "POST", pattern: "/api/companies/:slug/recurring-invoices", summary: "Opretter faktura-skabelon (#386)." },
  { scope: "company", effect: "write", permission: "company.draft.write", method: "POST", pattern: "/api/companies/:slug/recurring-invoices/:id/generate", summary: "Materialiserer en gentagende faktura." },
  { scope: "company", effect: "write", permission: "company.draft.write", method: "POST", pattern: "/api/companies/:slug/recurring-invoices/:id/retire", summary: "Deaktiverer en gentagende fakturaskabelon." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/archive/:year", summary: "Arkiveret regnskabsår." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/multi-year", summary: "Flerårsoversigt." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/invoices", summary: "Udstedte fakturaer." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/imported-receivables", summary: "Kildebeviste importerede tilgodehavender; holdes adskilt fra udstedte fakturaer." },
  { scope: "company", effect: "read", permission: "company.read", method: "POST", pattern: "/api/companies/:slug/imported-receivables/backfill/plan", summary: "Read-only hash-bundet plan for legacy Dinero-debitorbackfill uden import-replay." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/imported-receivables/backfill/apply", summary: "Appender eksakt reviewet legacy Dinero-debitorplan uden at ændre journaler eller arkiv." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/invoices/:id/pdf", summary: "Henter en faktura-PDF." },
  { scope: "company", effect: "read", permission: "company.master-data", method: "GET", pattern: "/api/companies/:slug/contacts", summary: "Kunder + leverandører." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/customers", summary: "Opretter kunde." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "PATCH", pattern: "/api/companies/:slug/customers/:id", summary: "Opdaterer kunde." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "DELETE", pattern: "/api/companies/:slug/customers/:id", summary: "Sletter kunde (#430). Blokeres ved åbne fakturaer." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/vendors", summary: "Opretter leverandør." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "PATCH", pattern: "/api/companies/:slug/vendors/:id", summary: "Opdaterer leverandør." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "DELETE", pattern: "/api/companies/:slug/vendors/:id", summary: "Sletter leverandør (#430). Blokeres ved åbne gælder." },
  { scope: "company", effect: "external", permission: "company.external-lookup", method: "GET", pattern: "/api/companies/:slug/cvr-lookup", summary: "Slår CVR op." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/company", summary: "Virksomhedens stamdata." },
  { scope: "company", effect: "write", permission: "company.admin", method: "PATCH", pattern: "/api/companies/:slug/company", summary: "Opdaterer stamdata + bank/betaling." },
  { scope: "company", effect: "external", permission: "company.external-lookup", method: "POST", pattern: "/api/companies/:slug/sync-cvr", summary: "Synkroniserer stamdata fra CVR." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/obligations", summary: "Frister og forpligtelser." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/cashflow", summary: "Likviditetsprognose." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/budget", summary: "Budget pr. konto pr. måned (#339)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/budget-vs-actual", summary: "Budget vs. faktisk for året (#339)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/budget-dimension-actuals", summary: "Godkendte dimensionsaktualer med konto-budgetkontekst (#589)." },
  { scope: "company", effect: "write", permission: "company.admin", method: "POST", pattern: "/api/companies/:slug/budget", summary: "Sætter (append-only revision) en budgetlinje (#339)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/exceptions", summary: "Exceptions queue — undtagelser, filtrerbar pr. status (#332)." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/exceptions/:id/resolve", summary: "Løser en exception." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/periods", summary: "Periodelås-liste med effective status (#342)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/bank-accounts", summary: "Registrerede bankkonti + CSV-mapping-profiler (#345)." },
  { scope: "company", effect: "write", permission: "company.admin", method: "POST", pattern: "/api/companies/:slug/bank-accounts", summary: "Opretter en bankkonto (#345)." },
  { scope: "company", effect: "write", permission: "company.admin", method: "PATCH", pattern: "/api/companies/:slug/bank-accounts/:account", summary: "Auditeret opdatering af bankkontoens betalingsprofil (#539)." },
  { scope: "company", effect: "write", permission: "company.export", method: "POST", pattern: "/api/companies/:slug/gdpr/export", summary: "GDPR-indsigt — actor-attribueret og confirm-gatet (#334)." },
  { scope: "company", effect: "write", permission: "company.admin", method: "POST", pattern: "/api/companies/:slug/gdpr/erase", summary: "GDPR-anonymisering — append-only tombstones (#334)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/bilagsmail", summary: "Bilagsmail-status: IMAP-config, alias, inbox (#348/#350/#351)." },
  { scope: "company", effect: "write", permission: "company.admin", method: "POST", pattern: "/api/companies/:slug/bilagsmail/imap-config", summary: "Gemmer IMAP-config til config/imap.json (#348)." },
  { scope: "company", effect: "write", permission: "company.admin", method: "DELETE", pattern: "/api/companies/:slug/bilagsmail/imap-config", summary: "Sletter den gemte IMAP-config (#348)." },
  { scope: "company", effect: "write", permission: "company.admin", method: "PATCH", pattern: "/api/companies/:slug/bilagsmail/alias", summary: "Sætter eller rydder mail-alias (#350)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/accruals", summary: "Periodiseringsregister (#337)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/annual-report", summary: "Årsrapport-builder (regnskabsklasse-B) (#338)." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/bank/import", summary: "Importerer bank-CSV." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/bank/reconciliation-correction", summary: "Anvender reviewet bankafstemningskorrektion." },
  { scope:"company",effect:"write",permission:"company.ledger.post",method:"POST",pattern:"/api/companies/:slug/bank/legacy-binding/apply",summary:"Applies exact reviewed legacy bank binding (#601)." },
  { scope:"company",effect:"read",permission:"company.read",method:"POST",pattern:"/api/companies/:slug/payables/legacy-backfill/plan",summary:"Read-only exact-ID payable/payment backfill plan (#601)." },
  { scope:"company",effect:"write",permission:"company.ledger.post",method:"POST",pattern:"/api/companies/:slug/payables/legacy-backfill/apply",summary:"Applies exact-ID payable/payment backfill (#601)." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/import", summary: "Generel data-import." },
  { scope: "company", effect: "write", permission: "company.export", method: "POST", pattern: "/api/companies/:slug/accountant-export", summary: "Revisor-eksport (.tar)." },
  { scope: "company", effect: "write", permission: "company.documents.upload", method: "POST", pattern: "/api/companies/:slug/documents/ingest", summary: "Modtager et bilag." },
  { scope: "company", effect: "write", permission: "company.documents.upload", method: "POST", pattern: "/api/companies/:slug/documents/:id/parse", summary: "Parser et gemt PDF-bilag med confirm." },
  { scope: "company", effect: "write", permission: "company.documents.upload", method: "POST", pattern: "/api/companies/:slug/documents/parse-pending", summary: "Parser ventende PDF-bilag med confirm." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/documents/party-links/apply", summary: "Anvender auditeret partskobling (#588)." },
  { scope:"company",effect:"write",permission:"company.master-data",method:"POST",pattern:"/api/companies/:slug/documents/party-coverage/apply",summary:"Anvender eksakt party coverage-plan append-only uden bogførings- eller momseffekt (#644/#645)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/documents/party-links/supersede", summary: "Supersederer auditeret partskobling (#588)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/documents/internal-no-external-party", summary: "Bekræfter hash-bundet intern bilagskontekst uden ekstern part (#588)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/documents/internal-no-external-party/supersede", summary: "Supersederer intern no-party-beslutning append-only (#588)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/documents/company-context", summary: "Gemmer hash-bundet attribution for forenklet eller ufuldstændigt købsbilag (#618)." },
  { scope: "company", effect: "write", permission: "company.master-data", method: "POST", pattern: "/api/companies/:slug/documents/purchase-vat-evidence-review", summary: "Gemmer hash-bundet vurdering af formel fakturamangel; ikke en moms-override (#622)." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/documents/book-expense", summary: "Bogfører et bilag som udgift mod en banktransaktion." },
  { scope: "company", effect: "write", permission: "company.draft.write", method: "POST", pattern: "/api/companies/:slug/invoices/issue", summary: "Udsteder en faktura." },
  { scope: "company", effect: "write", permission: "company.draft.write", method: "POST", pattern: "/api/companies/:slug/invoices/preview", summary: "Forhåndsviser en faktura-PDF uden at udstede." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/invoices/post", summary: "Bogfører en udstedt faktura." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/invoices/settle", summary: "Afregner faktura fra bank." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/invoices/credit-note", summary: "Udsteder kreditnota." },
  { scope: "company", effect: "external", permission: "company.external-send", method: "POST", pattern: "/api/companies/:slug/invoices/send-public", summary: "Sender faktura som e-faktura (NemHandel/PEPPOL)." },
  { scope: "company", effect: "external", permission: "company.external-send", method: "POST", pattern: "/api/companies/:slug/invoices/send-public/status", summary: "Kontrollerer kun status for en køsat DigiSense e-faktura." },
  { scope: "company", effect: "external", permission: "company.external-send", method: "POST", pattern: "/api/companies/:slug/invoices/send-email", summary: "Sender faktura til kundens e-mail med PDF vedhæftet." },
  { scope: "company", effect: "external", permission: "company.external-send", method: "POST", pattern: "/api/companies/:slug/invoices/send-reminder", summary: "Registrerer rykker (rentel. § 9b) og sender den på e-mail." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/periods/close", summary: "Lukker regnskabsperiode." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/periods/close-readiness", summary: "Genererer hash-bundet periodelukningspacket." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/periods/close-review", summary: "Gemmer eksplicit reviewet periodelukningspacket." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/periods/close-status", summary: "Læser et gemt periodelukningsreview uden rekalkulering." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/periods/reopen", summary: "Genåbner regnskabsperiode (#247-modstykke til CLI-only)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/mileage", summary: "Kørselsregister for valgt regnskabsår (#335)." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/mileage", summary: "Registrerer en kørsel (#335)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/assets", summary: "Anlægskartotek — kapitaliserede aktiver + straksafskrivninger (#336)." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/assets", summary: "Registrerer et anlæg + lineær afskrivningsplan (#336)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/assets/:id/next-depreciation", summary: "Næste afskrivningsperiode for et anlæg (#336)." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/assets/:id/depreciate", summary: "Bogfører næste afskrivningsperiode (#336)." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/assets/write-off", summary: "Straksafskriver et småanskaffelse (#336)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/payables", summary: "Leverandørfaktura-arbejdsbord — kreditorliste + modal-data (#340)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/supplier-commitments", summary: "Supplier commitments og 13-ugers likviditetsdrilldown (#590)." },
  { scope: "company", effect: "read", permission: "company.read", method: "POST", pattern: "/api/companies/:slug/supplier-commitments/plan", summary: "Read-only commitment-plan (#590)." },
  { scope: "company", effect: "write", permission: "company.draft.write", method: "POST", pattern: "/api/companies/:slug/supplier-commitments", summary: "Godkender hash-bundet supplier commitment (#590)." },
  { scope: "company", effect: "write", permission: "company.draft.write", method: "POST", pattern: "/api/companies/:slug/supplier-commitments/change", summary: "Pauser, afslutter eller supersederer commitment append-only (#590)." },
  { scope: "company", effect: "write", permission: "company.draft.write", method: "POST", pattern: "/api/companies/:slug/supplier-commitments/match", summary: "Matcher en occurrence til canonical evidence append-only (#590)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/supplier-commitments/matches", summary: "Læser canonical occurrence-matches, varians og alerts (#590)." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/payables", summary: "Registrerer et bilag som leverandørfaktura (#340)." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/payables/:id/pay", summary: "Markerer leverandørfaktura betalt fra bankpost (#340)." },
  { scope: "company", effect: "read", permission: "company.read", method: "POST", pattern: "/api/companies/:slug/payables/direct-bank-correction/plan", summary: "Planlægger hash-bundet direct-bank→payable-korrektion (#594)." },
  { scope: "company", effect: "write", permission: "company.ledger.post", method: "POST", pattern: "/api/companies/:slug/payables/direct-bank-correction/apply", summary: "Anvender reviewet direct-bank→payable-korrektion append-only (#594)." },
  { scope: "company", effect: "read", permission: "company.read", method: "GET", pattern: "/api/companies/:slug/agent-suggestions", summary: "Agent-forslag i kø — afventer ejerens godkendelse (#346)." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/agent-suggestions/:id/approve", summary: "Ejer godkender agent-forslag — løser undtagelsen med 'Godkendt'-note (#346)." },
  { scope: "company", effect: "write", permission: "company.review", method: "POST", pattern: "/api/companies/:slug/agent-suggestions/:id/reject", summary: "Ejer afviser agent-forslag — løser undtagelsen med 'Afvist'-note (#346)." },
];

export const ROUTE_CATALOG: readonly RouteCatalogEntry[] = ROUTE_CATALOG_INPUT;
validateRouteCatalog(ROUTE_CATALOG);

export type MatchedCatalogRoute = {
  entry: RouteCatalogEntry;
  /** Decoded only after it has remained a single valid slug segment. */
  companySlug?: string;
  /** Positive numeric resource id for an `:id` segment, if the route has one. */
  resourceId?: number;
};

/**
 * High-risk hosted operations require a freshly established Better Auth
 * session. This is deliberately one central, server-clock policy: handlers
 * never inspect client time or implement their own step-up checks.
 */
export const HIGH_RISK_SESSION_MAX_AGE_MS = AUTH_SESSION_FRESH_AGE_SECONDS * 1000;

const HIGH_RISK_WRITE_PERMISSIONS = new Set<RoutePermission>([
  "workspace.manage",
  "workspace.members.manage",
  "company.admin",
  "company.master-data",
  "company.draft.write",
  "company.ledger.post",
  "company.review",
  "company.ownership.manage",
  "company.export", // Includes GDPR/accountant exports, never read-only CSV downloads.
  "company.external-send",
]);

function isUnsafeMethod(method: string): boolean {
  return method !== "GET" && method !== "HEAD" && method !== "OPTIONS";
}

/** Exported for catalog tests: reads, including normal report downloads, stay available. */
export function routeRequiresFreshSession(route: MatchedCatalogRoute): boolean {
  return isUnsafeMethod(route.entry.method) && HIGH_RISK_WRITE_PERMISSIONS.has(route.entry.permission);
}

function assertFreshHostedSession(principal: Principal | undefined, route: MatchedCatalogRoute): void {
  if (principal?.via !== "better-auth" || !routeRequiresFreshSession(route)) return;
  const createdAt = principal.sessionCreatedAt?.getTime();
  const age = createdAt === undefined ? Number.NaN : Date.now() - createdAt;
  if (!Number.isFinite(age) || age < 0 || age > HIGH_RISK_SESSION_MAX_AGE_MS) {
    throw new ApiError("unauthorized", "reauthentication required", { subcode: "SESSION_REAUTH_REQUIRED" });
  }
}

/**
 * Custom route security which must happen before dispatch (and therefore
 * before any company ledger can be opened). Hosted mutations require a trusted
 * browser origin; hosted high-risk actions also require a recent provider
 * session. Local legacy requests retain their existing localhost/CLI contract.
 */
function assertCatalogRouteSecurity(
  request: Request,
  config: ServerConfig,
  principal: Principal | undefined,
  route: MatchedCatalogRoute,
): void {
  if (config.betterAuthProvider && isUnsafeMethod(route.entry.method)) {
    assertHostedMutationOriginAllowed(request, config);
  }
  assertFreshHostedSession(principal, route);
}

/**
 * Matches dispatch against the catalog before authorization.  This keeps the
 * security policy and the imperative handler chain in lockstep without
 * opening a company ledger just to determine permission.
 */
export function matchCatalogRoute(method: string, path: string): MatchedCatalogRoute | null {
  const requestedSegments = path.split("/").filter(Boolean);
  for (const entry of ROUTE_CATALOG) {
    if (entry.method !== method) continue;
    const patternSegments = entry.pattern.split("/").filter(Boolean);
    if (patternSegments.length !== requestedSegments.length) continue;
    let companySlug: string | undefined;
    let resourceId: number | undefined;
    let matched = true;
    for (let index = 0; index < patternSegments.length; index += 1) {
      const pattern = patternSegments[index]!;
      const segment = requestedSegments[index]!;
      if (!pattern.startsWith(":")) {
        if (pattern !== segment) matched = false;
        continue;
      }
      let decoded: string;
      try {
        decoded = decodeURIComponent(segment);
      } catch {
        matched = false;
        continue;
      }
      if (pattern === ":slug") {
        // `%2f` and encoded traversal are denied here, before a handler can
        // interpret the value as a filesystem/company selection.
        if (!isValidSlug(decoded) || decoded.includes("/")) matched = false;
        else companySlug = decoded;
      } else if (pattern === ":id" && /^\d+$/.test(decoded)) {
        const id = Number(decoded);
        if (!Number.isSafeInteger(id)) matched = false;
        else if (id > 0) resourceId = id;
      }
    }
    if (matched) return { entry, companySlug, resourceId };
  }
  return null;
}

function authorizeCatalogRoute(
  config: ServerConfig,
  principal: Principal | undefined,
  route: MatchedCatalogRoute,
): void {
  if (route.entry.permission === "public.read" || route.entry.permission === "public.invitation.claim") return;
  // The two legacy modes are deliberately whole-workspace escape hatches for
  // local single-owner use only. Hosted Better Auth principals are always
  // checked against append-only workspace/company membership events.
  if (!config.betterAuthProvider || (principal?.via !== "better-auth" && principal?.via !== "service-principal")) return;
  const userId = principal.userId ?? "";
  const db = openWorkspaceControlReadOnlyDb(config.workspaceRoot);
  let allowed = false;
  try {
    const decision = authorizeWorkspaceRoute(db, config.workspaceRoot, {
      userId,
      permission: route.entry.permission,
      companySlug: route.companySlug,
    });
    allowed = decision.allowed;
  } finally {
    db.close();
  }
  if (!allowed) {
    const auditDb = openWorkspaceControlDb(config.workspaceRoot);
    try { insertWorkspaceAuthorizationAudit(auditDb, { actor: principal.id, method: route.entry.method, routeTemplate: route.entry.pattern, permission: route.entry.permission, companySlug: route.companySlug, requestId: config.requestId ?? null }); }
    finally { auditDb.close(); }
  }
  if (allowed) return;
  const resourceType = route.entry.pattern === "/api/companies/:slug/documents/:id/file"
    ? "document_file"
    : route.entry.pattern === "/api/companies/:slug/invoices/:id/pdf"
    ? "issued_invoice_pdf"
    : null;
  // This event records an authenticated authorization denial only.  It never
  // opens a company ledger or changes the deliberately generic HTTP denial,
  // so it cannot become a cross-company existence oracle.
  if (resourceType && route.companySlug) {
    recordHostedDocumentAccess(config, {
      companySlug: route.companySlug,
      resourceType,
      resourceId: route.resourceId ?? null,
      outcome: "denied",
      reasonCode: "authorization_denied",
    });
  }
  throw ApiError.unauthorized("missing or invalid credentials");
}

function catalogContainsPath(path: string): boolean {
  return ROUTE_CATALOG.some((entry) => matchCatalogRoute(entry.method, path) !== null);
}

// --------------------------------------------------------------------------
// Dispatch
// --------------------------------------------------------------------------

/**
 * Handles one HTTP request end-to-end. Always resolves to a `Response` —
 * thrown `ApiError`s (and any other error) are mapped to safe JSON here.
 */
export async function handleRequest(
  request: Request,
  config: ServerConfig,
): Promise<Response> {
  try {
    // (1) Route metadata is resolved before authorization. The catalog never
    // opens a ledger and is therefore safe to inspect before identity.
    const url = new URL(request.url);
    const path = url.pathname.replace(/\/+$/, "") || "/";
    const method = request.method.toUpperCase();
    const route = matchCatalogRoute(method, path);

    // Better Auth owns only its documented `/api/auth/*` endpoints. Public
    // signup is explicitly blocked here even though the production runtime
    // also sets `disableSignUp: true`; the private bootstrap factory is never
    // reachable through HTTP.
    if (path === "/api/auth" || path.startsWith("/api/auth/")) {
      if (path === "/api/auth/sign-up" || path.startsWith("/api/auth/sign-up/")) {
        throw ApiError.notFound("ukendt endpoint");
      }
      if (!config.betterAuthProvider) throw ApiError.notFound("ukendt endpoint");
      return await config.betterAuthProvider.handle(request);
    }

    let principal: Principal | undefined;
    if (config.betterAuthProvider) {
      // Public health/rules/catalog calls are intentionally anonymous. A
      // protected route performs exactly one Better Auth session lookup.
      if (route?.entry.permission !== "public.read" && route?.entry.permission !== "public.invitation.claim") {
        principal = await (config.authenticateRequest ?? authMiddleware)(request, config);
      }
    } else {
      // Preserve the explicit local/shared-secret contract, including its
      // existing all-route authentication behavior when authRequired is set.
      principal = await (config.authenticateRequest ?? authMiddleware)(request, config);
    }
    if (principal) {
      // A request-local immutable copy carries the exact principal through all
      // existing handlers without re-authentication or global request state.
      config = {
        ...config,
        // Existing mutation/origin gates treat an authenticated Better Auth
        // deployment like the established shared-secret hosted mode.
        authRequired: config.authRequired || Boolean(config.betterAuthProvider),
        requestPrincipal: principal,
      };
    }
    if (route) {
      authorizeCatalogRoute(config, principal, route);
      assertCatalogRouteSecurity(request, config, principal, route);
      if (route.companySlug && !findRoutableWorkspaceCompany(config.workspaceRoot, route.companySlug)) {
        throw ApiError.notFound("virksomhed findes ikke i det aktive workspace");
      }
    } else if (path === "/api" || path.startsWith("/api/")) {
      // The catalog is an enforcement boundary, not documentation only. A
      // future imperative dispatch branch is unreachable until it declares
      // scope, effect and permission metadata. Known paths keep the existing
      // method-not-allowed contract; unknown API paths fail closed as 404.
      if (catalogContainsPath(path)) {
        throw ApiError.methodNotAllowed("metoden er ikke understøttet på denne rute");
      }
      throw ApiError.notFound("ukendt endpoint");
    }

    // (2) Imperative route dispatch. `route` above ensures every handler
    // reached by a catalogued request was authorized first.

    const systemResponse = dispatchSystemRoute(path, method, {
      health: () => handleHealth(config, ROUTE_CATALOG),
      readiness: () => handleReadiness(config),
      cvrStatus: () => handleSystemCvrStatus(),
    });
    if (systemResponse) return systemResponse;

    const groupWorkspaceResponse = await dispatchGroupWorkspaceRoute(path, method, url, request, {
      portfolio: () => handlePortfolio(config, url), cfoAnalytics: () => handleCfoAnalytics(config, url), me: () => handleMe(config),
      invitationList: () => handleWorkspaceInvitationList(config), invitationCreate: () => handleWorkspaceInvitationCreate(config, request), invitationCancel: () => handleWorkspaceInvitationCancel(config, request),
      servicePrincipalList: () => handleServicePrincipalList(config), servicePrincipalCreate: () => handleServicePrincipalCreate(config, request), servicePrincipalRotate: () => handleServicePrincipalRotate(config, request), servicePrincipalRevoke: () => handleServicePrincipalRevoke(config, request), servicePrincipalRecover: () => handleServicePrincipalRecover(config, request),
      workspaceMemberList: () => handleWorkspaceMemberList(config), workspaceMemberAccessUpdate: () => handleWorkspaceMemberAccessUpdate(config, request), workspaceMemberCompanyUpdate: () => handleWorkspaceMemberCompanyUpdate(config, request), invitationClaim: () => handleWorkspaceInvitationClaim(config, request),
      workspaceInboxComplete: (slug: string, documentId: string) => handleWorkspaceInboxComplete(config, request, slug, documentId), workspaceInboxApprove: (slug: string, documentId: string) => handleWorkspaceInboxApprove(config, request, slug, documentId), workspaceInboxInspect: (slug: string, documentId: string) => handleWorkspaceInboxInspect(config, slug, documentId), workspaceInboxList: (slug: string) => handleWorkspaceInboxList(config, slug, url), workspaceInboxIngest: (slug: string) => handleWorkspaceInboxIngest(config, request, slug),
      registryParties: (slug: string) => handleRegistryParties(config, slug, request), registryPartyCreate: (slug: string) => handleRegistryPartyCreate(config, slug, request), registryPartyRole: (slug: string, partyId: string) => handleRegistryPartyRole(config, slug, partyId, request), registryPartyMerge: (slug: string, approve: boolean) => handleRegistryPartyMerge(config, slug, request, approve), registryParty: (slug: string, partyId: string) => handleRegistryParty(config, slug, partyId),
      legacyPartyMappingPlan: (slug:string) => handleLegacyPartyMappingPlan(config,slug,request), legacyPartyMappings: (slug:string) => handleLegacyPartyMappings(config,slug,request), legacyPartyMappingApply: (slug:string) => handleLegacyPartyMappingApply(config,slug,request), legacyPartyMappingSupersede: (slug:string) => handleLegacyPartyMappingSupersede(config,slug,request), vendorIdentityEnrichmentPlan:(slug:string)=>handleVendorIdentityEnrichmentPlan(config,slug,request),vendorIdentityEnrichments:(slug:string)=>handleVendorIdentityEnrichments(config,slug,request),vendorIdentityEnrichmentApply:(slug:string)=>handleVendorIdentityEnrichmentApply(config,slug,request),
      registryRecords: (slug: string) => handleRegistryRecords(config, slug, request), registryRecordIngest: (slug: string) => handleRegistryRecordIngest(config, slug, request), registryRecordAction: (slug: string, recordId: string, action: "link" | "enrich" | "supersede") => handleRegistryRecordAction(config, slug, recordId, request, action), registryRecordDownload: (slug: string, recordId: string) => handleRegistryRecordDownload(config, slug, recordId), registryRecord: (slug: string, recordId: string) => handleRegistryRecord(config, slug, recordId),
      companyKnowledge: (slug: string) => handleCompanyKnowledge(config, slug, request), companyKnowledgeAction: (slug: string, action: "propose" | "review" | "supersede") => handleCompanyKnowledgeAction(config, slug, request, action), ownershipHistory: (slug: string) => handleOwnershipHistory(config, slug, request), ownershipAction: (slug: string, action: "propose" | "review" | "apply") => handleOwnershipAction(config, slug, request, action), ownershipQuery: (slug: string) => handleOwnershipQuery(config, slug, request),
      groupOverview: (asOf: string) => handleGroupOverview(config, asOf), groupReconciliation: (asOf: string) => handleGroupReconciliation(config, asOf), groupEliminations: (asOf: string) => handleGroupEliminations(config, asOf), groupConsolidatedReport: (profileId: string, from: string, asOf: string) => handleGroupConsolidatedReport(config, profileId, from, asOf), groupReportProfiles: (asOf: string) => handleGroupReportProfiles(config, asOf), groupDispositionStatus: (id: string, asOf?: string) => handleGroupDispositionStatus(config, id, asOf), groupDispositionAction: (slug: string, action: any) => handleGroupDispositionAction(config, slug, request, action),
    });
    if (groupWorkspaceResponse) return groupWorkspaceResponse;

    const agentDiscoveryResponse = dispatchAgentDiscoveryRoute(path, method, {
      rules: () => handleRules(),
      capabilities: () => {
        const cursor = Number(url.searchParams.get("cursor") ?? "0");
        const limit = Number(url.searchParams.get("limit") ?? "10");
        if (!Number.isInteger(cursor) || cursor < 0 || !Number.isInteger(limit) || limit < 1 || limit > 50) throw ApiError.badRequest("cursor must be >= 0 and limit must be between 1 and 50");
        return jsonResponse({ ok: true, ...searchCapabilities(url.searchParams.get("query") ?? undefined, cursor, limit, { commands: COMMAND_SPECS.map((command) => ({ key: command.key, allowedFlags: command.allowedFlags, mutating: MUTATING_COMMANDS.has(command.key), sideEffecting: SIDE_EFFECTING_COMMANDS.has(command.key) })), routes: ROUTE_CATALOG, unavailableSurfaces: ["mcp"] }) });
      },
      workflow: (id) => {
        const description = describeWorkflow(id, { commands: COMMAND_SPECS, routes: ROUTE_CATALOG, unavailableSurfaces: ["mcp"] });
        if (!description) throw ApiError.notFound("ukendt agent-workflow");
        return jsonResponse({ ok: true, ...description });
      },
    });
    if (agentDiscoveryResponse) return agentDiscoveryResponse;

    if (path === "/api/companies") {
      if (method === "GET") return handleCompanyList(config);
      if (method === "POST") return await handleCompanyCreate(config, request);
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }

    const dashboardMatch = /^\/api\/companies\/([^/]+)\/dashboard$/.exec(path);
    if (dashboardMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(dashboardMatch[1]!);
      return handleCompanyDashboard(config, slug, url);
    }
    const bookkeepingBatchApplyMatch = /^\/api\/companies\/([^/]+)\/bookkeeping-batch\/apply$/.exec(path);
    if (bookkeepingBatchApplyMatch) { if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute"); return await handleBookkeepingBatchApply(config, request, decodeURIComponent(bookkeepingBatchApplyMatch[1]!)); }
    const bookkeepingBatchPersistMatch = /^\/api\/companies\/([^/]+)\/bookkeeping-batch\/(?:persist|dry-run)$/.exec(path);
    if (bookkeepingBatchPersistMatch) { if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute"); return await handleBookkeepingBatchPersistDryRun(config, request, decodeURIComponent(bookkeepingBatchPersistMatch[1]!)); }
    const bookkeepingBatchApproveMatch = /^\/api\/companies\/([^/]+)\/bookkeeping-batch\/approve$/.exec(path);
    if (bookkeepingBatchApproveMatch) { if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute"); return await handleBookkeepingBatchApprove(config, request, decodeURIComponent(bookkeepingBatchApproveMatch[1]!)); }
    const bookkeepingBatchStatusMatch = /^\/api\/companies\/([^/]+)\/bookkeeping-batch\/runs\/(\d+)$/.exec(path);
    if (bookkeepingBatchStatusMatch) { if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute"); return handleBookkeepingBatchStatus(config, decodeURIComponent(bookkeepingBatchStatusMatch[1]!), Number(bookkeepingBatchStatusMatch[2])); }
    const bookkeepingBatchMatch = /^\/api\/companies\/([^/]+)\/bookkeeping-batch$/.exec(path);
    if (bookkeepingBatchMatch) { if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute"); return handleBookkeepingBatchDryRun(config, decodeURIComponent(bookkeepingBatchMatch[1]!), url); }
    const bookkeepingWorkbenchMatch = /^\/api\/companies\/([^/]+)\/bookkeeping-workbench$/.exec(path);
    if (bookkeepingWorkbenchMatch) { if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute"); return handleBookkeepingWorkbench(config, decodeURIComponent(bookkeepingWorkbenchMatch[1]!), url); }
    const accountingApprovalPolicyMatch = /^\/api\/companies\/([^/]+)\/accounting-approval-policy$/.exec(path);
    if (accountingApprovalPolicyMatch) {
      const slug = decodeURIComponent(accountingApprovalPolicyMatch[1]!);
      if (method === "GET") return handleAccountingApprovalPolicyGet(config, slug, request);
      if (method === "POST") return await handleAccountingApprovalPolicySet(config, request, slug);
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }
    const purchaseCaseReviewMatch=/^\/api\/companies\/([^/]+)\/purchase-cases\/([^/]+)\/review$/.exec(path);
    if(purchaseCaseReviewMatch){if(method!=="POST")throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");return await handlePurchaseCaseReview(config,request,decodeURIComponent(purchaseCaseReviewMatch[1]!),decodeURIComponent(purchaseCaseReviewMatch[2]!));}
    const purchaseCaseReassessMatch=/^\/api\/companies\/([^/]+)\/purchase-cases\/([^/]+)\/reassess$/.exec(path);
    if(purchaseCaseReassessMatch){if(method!=="POST")throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");return await handlePurchaseCaseReassess(config,request,decodeURIComponent(purchaseCaseReassessMatch[1]!),decodeURIComponent(purchaseCaseReassessMatch[2]!));}
    const purchaseCaseGroupReviewMatch=/^\/api\/companies\/([^/]+)\/purchase-cases\/group-review$/.exec(path);
    if(purchaseCaseGroupReviewMatch){if(method!=="POST")throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");return await handlePurchaseCaseGroupReview(config,request,decodeURIComponent(purchaseCaseGroupReviewMatch[1]!));}
    const purchaseOverviewMatch=/^\/api\/companies\/([^/]+)\/purchase-overview$/.exec(path);
    if(purchaseOverviewMatch){if(method!=="GET")throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");return handlePurchaseOverview(config,decodeURIComponent(purchaseOverviewMatch[1]!),url);}
    const purchaseCaseDetailMatch=/^\/api\/companies\/([^/]+)\/purchase-cases\/([^/]+)$/.exec(path);
    if(purchaseCaseDetailMatch){if(method!=="GET")throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");return handlePurchaseCaseGet(config,decodeURIComponent(purchaseCaseDetailMatch[1]!),decodeURIComponent(purchaseCaseDetailMatch[2]!));}
    const purchaseCasesMatch=/^\/api\/companies\/([^/]+)\/purchase-cases$/.exec(path);
    if(purchaseCasesMatch){if(method==="GET")return handlePurchaseCaseList(config,decodeURIComponent(purchaseCasesMatch[1]!));if(method==="POST")return await handlePurchaseCaseCreate(config,request,decodeURIComponent(purchaseCasesMatch[1]!));throw ApiError.methodNotAllowed("kun GET og POST er understøttet på denne rute");}
    const dimensionPlanMatch=/^\/api\/companies\/([^/]+)\/dimensions\/plan$/.exec(path);
    if(dimensionPlanMatch){if(method!=="POST")throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");return await handleDimensionPlan(config,decodeURIComponent(dimensionPlanMatch[1]!),request);}
    const dimensionBudgetPlanMatch=/^\/api\/companies\/([^/]+)\/dimensions\/budget-plan$/.exec(path);
    if(dimensionBudgetPlanMatch){if(method!=="POST")throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");return await handleDimensionBudgetPlan(config,decodeURIComponent(dimensionBudgetPlanMatch[1]!),request);}
    const dimensionBudgetsMatch=/^\/api\/companies\/([^/]+)\/dimensions\/budgets$/.exec(path);
    if(dimensionBudgetsMatch){if(method!=="GET")throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");return handleDimensionBudgets(config,decodeURIComponent(dimensionBudgetsMatch[1]!));}
    const dimensionActionMatch=/^\/api\/companies\/([^/]+)\/dimensions\/(define|member|apply|replace|supersede|definition-lifecycle|member-lifecycle|budget-apply)$/.exec(path);
    if(dimensionActionMatch){if(method!=="POST")throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");return await handleDimensionAction(config,decodeURIComponent(dimensionActionMatch[1]!),request,dimensionActionMatch[2]! as any);}
    const dimensionListMatch=/^\/api\/companies\/([^/]+)\/dimensions\/(\d+)$/.exec(path);
    if(dimensionListMatch){if(method!=="GET")throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");return handleDimensionAssignments(config,decodeURIComponent(dimensionListMatch[1]!),Number(dimensionListMatch[2]));}
    const dimensionMembersMatch=/^\/api\/companies\/([^/]+)\/dimensions\/members$/.exec(path);
    if(dimensionMembersMatch){if(method!=="GET")throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");return handleDimensionMembers(config,decodeURIComponent(dimensionMembersMatch[1]!),url.searchParams.get("dimensionId")??undefined);}
    const dimensionDefinitionsMatch=/^\/api\/companies\/([^/]+)\/dimensions$/.exec(path);
    if(dimensionDefinitionsMatch){if(method!=="GET")throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");return handleDimensionDefinitions(config,decodeURIComponent(dimensionDefinitionsMatch[1]!));}
    const fiscalYearsMatch = /^\/api\/companies\/([^/]+)\/fiscal-years$/.exec(path);
    if (fiscalYearsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(fiscalYearsMatch[1]!);
      return handleCompanyFiscalYears(config, slug);
    }

    const overviewMatch = /^\/api\/companies\/([^/]+)\/overview$/.exec(path);
    if (overviewMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(overviewMatch[1]!);
      return handleCompanyOverview(config, slug, url);
    }

    const retentionMatch = /^\/api\/companies\/([^/]+)\/retention$/.exec(path);
    if (retentionMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(retentionMatch[1]!);
      return handleCompanyRetention(config, slug);
    }

    const integrityMatch = /^\/api\/companies\/([^/]+)\/integrity$/.exec(path);
    if (integrityMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(integrityMatch[1]!);
      return handleCompanyIntegrity(config, slug);
    }

    const accountsMatch = /^\/api\/companies\/([^/]+)\/accounts$/.exec(path);
    if (accountsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(accountsMatch[1]!);
      return handleCompanyAccounts(config, slug);
    }

    const incomeStatementExportMatch =
      /^\/api\/companies\/([^/]+)\/income-statement\/export$/.exec(path);
    if (incomeStatementExportMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(incomeStatementExportMatch[1]!);
      return handleCompanyStatementExport(config, slug, url, "income-statement");
    }

    const incomeStatementMatch =
      /^\/api\/companies\/([^/]+)\/income-statement$/.exec(path);
    if (incomeStatementMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(incomeStatementMatch[1]!);
      return handleCompanyIncomeStatement(config, slug, url);
    }

    const balanceExportMatch = /^\/api\/companies\/([^/]+)\/balance\/export$/.exec(path);
    if (balanceExportMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(balanceExportMatch[1]!);
      return handleCompanyStatementExport(config, slug, url, "balance");
    }

    const balanceMatch = /^\/api\/companies\/([^/]+)\/balance$/.exec(path);
    if (balanceMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(balanceMatch[1]!);
      return handleCompanyBalance(config, slug, url);
    }

    const trialBalanceExportMatch =
      /^\/api\/companies\/([^/]+)\/trial-balance\/export$/.exec(path);
    if (trialBalanceExportMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(trialBalanceExportMatch[1]!);
      return handleCompanyStatementExport(config, slug, url, "trial-balance");
    }

    const trialBalanceMatch =
      /^\/api\/companies\/([^/]+)\/trial-balance$/.exec(path);
    if (trialBalanceMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(trialBalanceMatch[1]!);
      return handleCompanyTrialBalance(config, slug, url);
    }

    const journalExportMatch = /^\/api\/companies\/([^/]+)\/journal\/export$/.exec(path);
    if (journalExportMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(journalExportMatch[1]!);
      return handleCompanyJournalExport(config, slug, url);
    }

    const vatExportMatch = /^\/api\/companies\/([^/]+)\/vat\/export$/.exec(path);
    if (vatExportMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(vatExportMatch[1]!);
      return handleCompanyVatExport(config, slug, url);
    }

    const journalMatch = /^\/api\/companies\/([^/]+)\/journal$/.exec(path);
    if (journalMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(journalMatch[1]!);
      return handleCompanyJournal(config, slug, url);
    }

    const bankMatch = /^\/api\/companies\/([^/]+)\/bank$/.exec(path);
    if (bankMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(bankMatch[1]!);
      return handleCompanyBank(config, slug, url);
    }

    const vatMatch = /^\/api\/companies\/([^/]+)\/vat$/.exec(path);
    if (vatMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(vatMatch[1]!);
      return handleCompanyVat(config, slug, url);
    }

    const documentsMatch = /^\/api\/companies\/([^/]+)\/documents$/.exec(path);
    if (documentsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(documentsMatch[1]!);
      return handleCompanyDocuments(config, slug);
    }
    const documentPartyLinksMatch = /^\/api\/companies\/([^/]+)\/documents\/party-links$/.exec(path);
    if (documentPartyLinksMatch) { if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute"); return handleDocumentPartyLinks(config,decodeURIComponent(documentPartyLinksMatch[1]!),request); }
    const documentPartyLinkInspectMatch = /^\/api\/companies\/([^/]+)\/documents\/(\d+)\/party-links$/.exec(path);
    if (documentPartyLinkInspectMatch) { if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute"); return handleDocumentPartyLinkInspect(config,decodeURIComponent(documentPartyLinkInspectMatch[1]!),documentPartyLinkInspectMatch[2]!); }
    const partyCoverageMatch=/^\/api\/companies\/([^/]+)\/documents\/party-coverage$/.exec(path);
    if(partyCoverageMatch){if(method!=="GET")throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");return handlePartyCoverage(config,decodeURIComponent(partyCoverageMatch[1]!),request);}

    const documentFileMatch =
      /^\/api\/companies\/([^/]+)\/documents\/(\d+)\/file$/.exec(path);
    if (documentFileMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(documentFileMatch[1]!);
      return handleCompanyDocumentFile(config, slug, documentFileMatch[2]!);
    }

    // The Bogfør-bilag modal pulls its picker rows from this endpoint (#407).
    const documentBookingOptionsMatch =
      /^\/api\/companies\/([^/]+)\/documents\/(\d+)\/booking-options$/.exec(path);
    if (documentBookingOptionsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(documentBookingOptionsMatch[1]!);
      return handleCompanyDocumentBookingOptions(
        config,
        slug,
        documentBookingOptionsMatch[2]!,
      );
    }

    const documentVatPreflightMatch = /^\/api\/companies\/([^/]+)\/documents\/(\d+)\/vat-preflight$/.exec(path);
    if (documentVatPreflightMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      return handleCompanyDocumentVatPreflight(config, decodeURIComponent(documentVatPreflightMatch[1]!), documentVatPreflightMatch[2]!);
    }
    const documentExtractionMatch = /^\/api\/companies\/([^/]+)\/documents\/(\d+)\/invoice-extraction$/.exec(path);
    if (documentExtractionMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      return handleCompanyDocumentInvoiceExtraction(config, decodeURIComponent(documentExtractionMatch[1]!), documentExtractionMatch[2]!);
    }
    const documentParseStatusMatch = /^\/api\/companies\/([^/]+)\/documents\/(\d+)\/parse-status$/.exec(path);
    if (documentParseStatusMatch) { if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute"); return handleCompanyDocumentParseStatus(config, decodeURIComponent(documentParseStatusMatch[1]!), documentParseStatusMatch[2]!); }
    const documentParsedTextMatch = /^\/api\/companies\/([^/]+)\/documents\/(\d+)\/parsed-text$/.exec(path);
    if (documentParsedTextMatch) { if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute"); return handleCompanyDocumentParsedText(config, decodeURIComponent(documentParsedTextMatch[1]!), documentParsedTextMatch[2]!, url); }

    const recurringInvoicesMatch =
      /^\/api\/companies\/([^/]+)\/recurring-invoices$/.exec(path);

    const accountingDraftsMatch =
      /^\/api\/companies\/([^/]+)\/accounting-drafts$/.exec(path);
    if (accountingDraftsMatch) {
      const slug = decodeURIComponent(accountingDraftsMatch[1]!);
      if (method === "GET") return handleCompanyAccountingDrafts(config, slug);
      if (method === "POST") return await handleCreateAccountingDraft(config, request, slug);
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }

    const postingRulesExplainMatch = /^\/api\/companies\/([^/]+)\/posting-rules\/explain$/.exec(path);
    if (postingRulesExplainMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const body = await request.json() as Record<string, unknown>;
      return handleCompanyPostingRuleExplain(config, decodeURIComponent(postingRulesExplainMatch[1]!), (body.context ?? {}) as Record<string, unknown>, typeof body.at === "string" ? body.at : undefined);
    }
    const postingRulesMatch = /^\/api\/companies\/([^/]+)\/posting-rules$/.exec(path);
    if (postingRulesMatch) { if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute"); return handleCompanyPostingRules(config, decodeURIComponent(postingRulesMatch[1]!)); }
    const postingRuleActionMatch = /^\/api\/companies\/([^/]+)\/posting-rules\/(propose|approve|disable|supersede)$/.exec(path);
    if (postingRuleActionMatch) { if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute"); return await handlePostingRuleMutation(config, request, decodeURIComponent(postingRuleActionMatch[1]!), postingRuleActionMatch[2]! as "propose" | "approve" | "disable" | "supersede"); }

    const accountingDraftActionMatch =
      /^\/api\/companies\/([^/]+)\/accounting-drafts\/([^/]+)\/(revise|submit|reject|approve-and-post)$/.exec(path);
    if (accountingDraftActionMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(accountingDraftActionMatch[1]!);
      const draftId = decodeURIComponent(accountingDraftActionMatch[2]!);
      const action = accountingDraftActionMatch[3]!;
      if (action === "revise") return await handleReviseAccountingDraft(config, request, slug, draftId);
      if (action === "submit") return await handleSubmitAccountingDraft(config, request, slug, draftId);
      if (action === "reject") return await handleRejectAccountingDraft(config, request, slug, draftId);
      return await handleApproveAndPostAccountingDraft(config, request, slug, draftId);
    }

    const accountingDraftMatch =
      /^\/api\/companies\/([^/]+)\/accounting-drafts\/([^/]+)$/.exec(path);
    if (accountingDraftMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      return handleCompanyAccountingDraft(
        config,
        decodeURIComponent(accountingDraftMatch[1]!),
        decodeURIComponent(accountingDraftMatch[2]!),
      );
    }

    if (recurringInvoicesMatch) {
      const slug = decodeURIComponent(recurringInvoicesMatch[1]!);
      if (method === "GET") return handleCompanyRecurringInvoices(config, slug);
      // #386 — cockpit can create a recurring-invoice template instead of
      // having to drop to the CLI. POSTs through the same write pipeline as
      // the rest of the write-routes (backup lock, localhost gate, actor
      // attribution, requireConfirm) — see `handleCreateRecurringInvoiceTemplate`.
      if (method === "POST")
        return await handleCreateRecurringInvoiceTemplate(config, request, slug);
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }

    const recurringInvoiceGenerateMatch =
      /^\/api\/companies\/([^/]+)\/recurring-invoices\/(\d+)\/generate$/.exec(path);
    if (recurringInvoiceGenerateMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(recurringInvoiceGenerateMatch[1]!);
      return await handleGenerateRecurringInvoice(
        config,
        request,
        slug,
        recurringInvoiceGenerateMatch[2]!,
      );
    }

    // Cockpit write route (#435) — deactivate (retire) a recurring-invoice
    // template so it stops suggesting itself when the underlying contract has
    // ended. Templates are append-only by schema: the trigger forbids
    // unretiring, and identity/payload columns cannot be mutated. Owners who
    // need to change terms create a new template that supersedes the retired
    // one — historical generations on the old template stay intact.
    const recurringInvoiceRetireMatch =
      /^\/api\/companies\/([^/]+)\/recurring-invoices\/(\d+)\/retire$/.exec(path);
    if (recurringInvoiceRetireMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(recurringInvoiceRetireMatch[1]!);
      return await handleRetireRecurringInvoiceTemplate(
        config,
        request,
        slug,
        recurringInvoiceRetireMatch[2]!,
      );
    }

    const archiveMatch =
      /^\/api\/companies\/([^/]+)\/archive\/([^/]+)$/.exec(path);
    if (archiveMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(archiveMatch[1]!);
      const year = decodeURIComponent(archiveMatch[2]!);
      return handleCompanyArchiveYear(config, slug, year);
    }

    const multiYearMatch = /^\/api\/companies\/([^/]+)\/multi-year$/.exec(path);
    if (multiYearMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(multiYearMatch[1]!);
      return handleCompanyMultiYear(config, slug);
    }

    const importedReceivablesMatch = /^\/api\/companies\/([^/]+)\/imported-receivables$/.exec(path);
    if (importedReceivablesMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      return handleCompanyImportedReceivables(config, decodeURIComponent(importedReceivablesMatch[1]!), url);
    }

    const importedBackfillMatch=/^\/api\/companies\/([^/]+)\/imported-receivables\/backfill\/(plan|apply)$/.exec(path);
    if(importedBackfillMatch){if(method!=="POST")throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");const slug=decodeURIComponent(importedBackfillMatch[1]!);return importedBackfillMatch[2]==="plan"?handleCompanyImportedReceivablesBackfillPlan(config,slug,request):handleCompanyImportedReceivablesBackfillApply(config,slug,request);}

    const importedSettlementMatch=/^\/api\/companies\/([^/]+)\/imported-receivables\/settlement\/(plan|apply|status)$/.exec(path);
    if(importedSettlementMatch){const slug=decodeURIComponent(importedSettlementMatch[1]!);const action=importedSettlementMatch[2]!;if(action==="status"){if(method!=="GET")throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");return handleCompanyImportedReceivableSettlementStatus(config,slug,url);}if(method!=="POST")throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");return action==="plan"?handleCompanyImportedReceivableSettlementPlan(config,slug,request):handleCompanyImportedReceivableSettlementApply(config,slug,request);}

    const invoicesMatch = /^\/api\/companies\/([^/]+)\/invoices$/.exec(path);
    if (invoicesMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(invoicesMatch[1]!);
      return handleCompanyInvoices(config, slug, url);
    }

    // Cockpit read route (#378): serve the issued-invoice PDF so the owner
    // can download/forward it without leaving the browser. Re-uses the same
    // `renderIssuedInvoicePdf` core the CLI runs — no new rendering path.
    const invoicePdfMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/(\d+)\/pdf$/.exec(path);
    if (invoicePdfMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(invoicePdfMatch[1]!);
      return handleCompanyInvoicePdf(config, slug, invoicePdfMatch[2]!);
    }

    const contactsMatch = /^\/api\/companies\/([^/]+)\/contacts$/.exec(path);
    if (contactsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(contactsMatch[1]!);
      return handleCompanyContacts(config, slug);
    }

    // Cockpit write routes for contacts (#390) — create + edit kunder/leverandører
    // from the Kontakter page instead of the CLI.
    const createCustomerMatch =
      /^\/api\/companies\/([^/]+)\/customers$/.exec(path);
    if (createCustomerMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(createCustomerMatch[1]!);
      return await handleCreateCustomer(config, request, slug);
    }

    const customerByIdMatch =
      /^\/api\/companies\/([^/]+)\/customers\/(\d+)$/.exec(path);
    if (customerByIdMatch) {
      const slug = decodeURIComponent(customerByIdMatch[1]!);
      if (method === "PATCH") {
        return await handleUpdateCustomer(
          config,
          request,
          slug,
          customerByIdMatch[2]!,
        );
      }
      if (method === "DELETE") {
        return await handleDeleteCustomer(
          config,
          request,
          slug,
          customerByIdMatch[2]!,
        );
      }
      throw ApiError.methodNotAllowed("kun PATCH eller DELETE er understøttet på denne rute");
    }

    const createVendorMatch =
      /^\/api\/companies\/([^/]+)\/vendors$/.exec(path);
    if (createVendorMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(createVendorMatch[1]!);
      return await handleCreateVendor(config, request, slug);
    }

    const vendorByIdMatch =
      /^\/api\/companies\/([^/]+)\/vendors\/(\d+)$/.exec(path);
    if (vendorByIdMatch) {
      const slug = decodeURIComponent(vendorByIdMatch[1]!);
      if (method === "PATCH") {
        return await handleUpdateVendor(
          config,
          request,
          slug,
          vendorByIdMatch[2]!,
        );
      }
      if (method === "DELETE") {
        return await handleDeleteVendor(
          config,
          request,
          slug,
          vendorByIdMatch[2]!,
        );
      }
      throw ApiError.methodNotAllowed("kun PATCH eller DELETE er understøttet på denne rute");
    }

    // CVR lookup helper for the Kontakter modal (#390) — read-only enrichment.
    const cvrLookupMatch =
      /^\/api\/companies\/([^/]+)\/cvr-lookup$/.exec(path);
    if (cvrLookupMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(cvrLookupMatch[1]!);
      return await handleCvrLookup(config, request, slug);
    }

    const companySettingsMatch = /^\/api\/companies\/([^/]+)\/company$/.exec(path);
    if (companySettingsMatch) {
      const slug = decodeURIComponent(companySettingsMatch[1]!);
      if (method === "GET") return handleCompanySettings(config, slug);
      // PATCH edits the company profile + bank/payment details (#284).
      if (method === "PATCH") {
        return await handleCompanyProfile(config, request, slug);
      }
      throw ApiError.methodNotAllowed(
        "kun GET eller PATCH er understøttet på denne rute",
      );
    }

    const syncCvrMatch = /^\/api\/companies\/([^/]+)\/sync-cvr$/.exec(path);
    if (syncCvrMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(syncCvrMatch[1]!);
      return await handleCompanySyncCvr(request, config, slug);
    }

    const obligationsMatch =
      /^\/api\/companies\/([^/]+)\/obligations$/.exec(path);
    if (obligationsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(obligationsMatch[1]!);
      return handleCompanyObligations(config, slug, url);
    }

    const cashflowMatch = /^\/api\/companies\/([^/]+)\/cashflow$/.exec(path);
    if (cashflowMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(cashflowMatch[1]!);
      return handleCompanyCashflow(config, slug, url);
    }

    // Kørsel (#335). GET lists the register for the selected fiscal year; POST
    // registers one mileage entry through the SAME `createMileageEntry` core
    // function the CLI's `mileage add` and the MCP tool use.
    const mileageMatch = /^\/api\/companies\/([^/]+)\/mileage$/.exec(path);
    if (mileageMatch) {
      const slug = decodeURIComponent(mileageMatch[1]!);
      if (method === "GET") return handleCompanyMileage(config, slug, url);
      if (method === "POST") return await handleMileageCreate(config, request, slug);
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }

    // Budget endpoints (#339). The longer `/budget-vs-actual` route MUST come
    // before `/budget` so the shorter pattern does not shadow it.
    const budgetVsActualMatch =
      /^\/api\/companies\/([^/]+)\/budget-vs-actual$/.exec(path);
    if (budgetVsActualMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(budgetVsActualMatch[1]!);
      return handleCompanyBudgetVsActual(config, slug, url);
    }

    const budgetDimensionActualsMatch =
      /^\/api\/companies\/([^/]+)\/budget-dimension-actuals$/.exec(path);
    if (budgetDimensionActualsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(budgetDimensionActualsMatch[1]!);
      return handleCompanyBudgetDimensionActuals(config, slug, url);
    }

    const budgetMatch = /^\/api\/companies\/([^/]+)\/budget$/.exec(path);
    if (budgetMatch) {
      const slug = decodeURIComponent(budgetMatch[1]!);
      if (method === "GET") return handleCompanyBudget(config, slug, url);
      if (method === "POST") return await handleSetBudget(config, request, slug);
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }

    // Exceptions queue read endpoint (#332).
    const exceptionsListMatch =
      /^\/api\/companies\/([^/]+)\/exceptions$/.exec(path);
    if (exceptionsListMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(exceptionsListMatch[1]!);
      return handleCompanyExceptions(config, slug, url);
    }

    // Periods read endpoint (#342). Close/reopen er dækket separat længere nede.
    const periodsListMatch =
      /^\/api\/companies\/([^/]+)\/periods$/.exec(path);
    if (periodsListMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(periodsListMatch[1]!);
      return handleCompanyPeriods(config, slug);
    }

    const bankAccountUpdateMatch = /^\/api\/companies\/([^/]+)\/bank-accounts\/([^/]+)$/.exec(path);
    if (bankAccountUpdateMatch) {
      if (method !== "PATCH") throw ApiError.methodNotAllowed("kun PATCH er understøttet på denne rute");
      return await handleUpdateBankAccount(config, request, decodeURIComponent(bankAccountUpdateMatch[1]!), decodeURIComponent(bankAccountUpdateMatch[2]!));
    }
    // Bank-accounts list + create (#345).
    const bankAccountsMatch = /^\/api\/companies\/([^/]+)\/bank-accounts$/.exec(path);
    if (bankAccountsMatch) {
      const slug = decodeURIComponent(bankAccountsMatch[1]!);
      if (method === "GET") return handleCompanyBankAccounts(config, slug);
      if (method === "POST")
        return await handleCreateBankAccount(config, request, slug);
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }

    // GDPR export + erase are both actor-attributed writes (#334).
    const gdprExportMatch = /^\/api\/companies\/([^/]+)\/gdpr\/export$/.exec(path);
    if (gdprExportMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(gdprExportMatch[1]!);
      return await handleGdprExport(config, request, slug);
    }

    const gdprEraseMatch = /^\/api\/companies\/([^/]+)\/gdpr\/erase$/.exec(path);
    if (gdprEraseMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(gdprEraseMatch[1]!);
      return await handleGdprErase(config, request, slug);
    }

    // Accruals (periodiseringsregister) read endpoint (#337).
    const accrualsMatch = /^\/api\/companies\/([^/]+)\/accruals$/.exec(path);
    if (accrualsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(accrualsMatch[1]!);
      return handleCompanyAccruals(config, slug);
    }

    // Annual-report builder read endpoint (#338).
    const annualReportMatch = /^\/api\/companies\/([^/]+)\/annual-report$/.exec(path);
    if (annualReportMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(annualReportMatch[1]!);
      return handleCompanyAnnualReport(config, slug, url);
    }

    // Bilagsmail read endpoint (#348/#350/#351).
    const bilagsmailMatch = /^\/api\/companies\/([^/]+)\/bilagsmail$/.exec(path);
    if (bilagsmailMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(bilagsmailMatch[1]!);
      return handleCompanyBilagsmail(config, slug);
    }

    // Bilagsmail IMAP-config write (#348).
    const imapConfigMatch =
      /^\/api\/companies\/([^/]+)\/bilagsmail\/imap-config$/.exec(path);
    if (imapConfigMatch) {
      const slug = decodeURIComponent(imapConfigMatch[1]!);
      if (method === "POST")
        return await handleSaveBilagsmailImapConfig(config, request, slug);
      if (method === "DELETE")
        return await handleDeleteBilagsmailImapConfig(config, request, slug);
      throw ApiError.methodNotAllowed("kun POST eller DELETE er understøttet på denne rute");
    }

    // Bilagsmail alias write (#350).
    const bilagsmailAliasMatch =
      /^\/api\/companies\/([^/]+)\/bilagsmail\/alias$/.exec(path);
    if (bilagsmailAliasMatch) {
      if (method !== "PATCH") throw ApiError.methodNotAllowed("kun PATCH er understøttet på denne rute");
      const slug = decodeURIComponent(bilagsmailAliasMatch[1]!);
      return await handleSetBilagsmailAlias(config, request, slug);
    }

    // Bookkeeping write route (#213, slice 1): resolve an open exception.
    const resolveExceptionMatch =
      /^\/api\/companies\/([^/]+)\/exceptions\/([^/]+)\/resolve$/.exec(path);
    if (resolveExceptionMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(resolveExceptionMatch[1]!);
      const id = decodeURIComponent(resolveExceptionMatch[2]!);
      return await handleResolveException(config, request, slug, id);
    }

    // Bookkeeping write route (#213, slice 2): import a bank-statement CSV.
    const bankImportMatch =
      /^\/api\/companies\/([^/]+)\/bank\/import$/.exec(path);
    if (bankImportMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(bankImportMatch[1]!);
      return await handleBankImport(config, request, slug);
    }
    const bankCorrectionPlanMatch=/^\/api\/companies\/([^/]+)\/bank\/reconciliation-correction-plan$/.exec(path);
    if(bankCorrectionPlanMatch){if(method!=="GET")throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");return handleBankReconciliationCorrectionPlan(config,decodeURIComponent(bankCorrectionPlanMatch[1]!),url);}
    const bankCorrectionMatch=/^\/api\/companies\/([^/]+)\/bank\/reconciliation-correction$/.exec(path);
    if(bankCorrectionMatch){if(method!=="POST")throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");return await handleBankReconciliationCorrectionApply(config,request,decodeURIComponent(bankCorrectionMatch[1]!));}
    const legacyBindingMatch=/^\/api\/companies\/([^/]+)\/bank\/legacy-binding\/(plan|apply)$/.exec(path);
    if(legacyBindingMatch){if(method!=="POST")throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");const slug=decodeURIComponent(legacyBindingMatch[1]!);return legacyBindingMatch[2]==="plan"?await handleLegacyBankBindingPlan(config,slug,request):await handleLegacyBankBindingApply(config,slug,request);}

    // Cockpit write route: the generic file-import. Recognises which system
    // an export file came from and routes it to the matching core importer.
    const dataImportMatch = /^\/api\/companies\/([^/]+)\/import$/.exec(path);
    if (dataImportMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(dataImportMatch[1]!);
      return await handleDataImport(config, request, slug);
    }

    // Cockpit write route: the accountant-export download. Generates the
    // accountant-handoff package and streams it back as one .tar file.
    const accountantExportMatch =
      /^\/api\/companies\/([^/]+)\/accountant-export$/.exec(path);
    if (accountantExportMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(accountantExportMatch[1]!);
      return await handleAccountantExport(config, request, slug);
    }

    // Bookkeeping write route (#213, slice 3): ingest a document (bilag).
    const documentIngestMatch =
      /^\/api\/companies\/([^/]+)\/documents\/ingest$/.exec(path);
    if (documentIngestMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(documentIngestMatch[1]!);
      return await handleDocumentIngest(config, request, slug);
    }
    const documentPartyPlanMatch = /^\/api\/companies\/([^/]+)\/documents\/party-links\/plan$/.exec(path);
    if (documentPartyPlanMatch) { if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute"); return await handleDocumentPartyLinkPlan(config,decodeURIComponent(documentPartyPlanMatch[1]!),request); }
    const documentPartyApplyMatch = /^\/api\/companies\/([^/]+)\/documents\/party-links\/(apply|supersede)$/.exec(path);
    if (documentPartyApplyMatch) { if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute"); return await handleDocumentPartyLinkAction(config,decodeURIComponent(documentPartyApplyMatch[1]!),request,documentPartyApplyMatch[2]! as "apply"|"supersede"); }
    const partyCoverageActionMatch=/^\/api\/companies\/([^/]+)\/documents\/party-coverage\/(plan|apply)$/.exec(path);
    if(partyCoverageActionMatch){if(method!=="POST")throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");const slug=decodeURIComponent(partyCoverageActionMatch[1]!);return partyCoverageActionMatch[2]==="plan"?await handlePartyCoveragePlan(config,slug,request):await handlePartyCoverageApply(config,slug,request);}
    const internalNoExternalMatch = path.match(/^\/api\/companies\/([^/]+)\/documents\/internal-no-external-party(\/supersede)?$/);
    if (internalNoExternalMatch) { if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute"); return await handleInternalNoExternalParty(config,decodeURIComponent(internalNoExternalMatch[1]!),request,Boolean(internalNoExternalMatch[2])); }
    const documentCompanyContextMatch = /^\/api\/companies\/([^/]+)\/documents\/company-context$/.exec(path);
    if (documentCompanyContextMatch) { if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute"); return await handleDocumentCompanyContext(config,decodeURIComponent(documentCompanyContextMatch[1]!),request); }
    const purchaseVatEvidenceReviewMatch = /^\/api\/companies\/([^/]+)\/documents\/purchase-vat-evidence-review$/.exec(path);
    if (purchaseVatEvidenceReviewMatch) { if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute"); return await handleDocumentPurchaseVatEvidenceReview(config,decodeURIComponent(purchaseVatEvidenceReviewMatch[1]!),request); }
    const documentParsePendingMatch = /^\/api\/companies\/([^/]+)\/documents\/parse-pending$/.exec(path);
    if (documentParsePendingMatch) { if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute"); return await handleDocumentPdfParsePending(config, request, decodeURIComponent(documentParsePendingMatch[1]!)); }
    const documentParseMatch = /^\/api\/companies\/([^/]+)\/documents\/(\d+)\/parse$/.exec(path);
    if (documentParseMatch) { if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute"); return await handleDocumentPdfParse(config, request, decodeURIComponent(documentParseMatch[1]!), documentParseMatch[2]!); }

    // Bookkeeping write route (#407): book an ingested purchase document
    // (bilag) against an unmatched outgoing bank transaction. Third caller
    // of `bookExpenseFromBank` alongside the CLI's `expense book` and the
    // MCP tool.
    const documentBookExpenseMatch =
      /^\/api\/companies\/([^/]+)\/documents\/book-expense$/.exec(path);
    if (documentBookExpenseMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(documentBookExpenseMatch[1]!);
      return await handleDocumentBookExpense(config, request, slug);
    }

    const documentVatPreflightApplyMatch = /^\/api\/companies\/([^/]+)\/documents\/(\d+)\/vat-preflight\/apply$/.exec(path);
    if (documentVatPreflightApplyMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      return await handleDocumentVatPreflightApply(config, request, decodeURIComponent(documentVatPreflightApplyMatch[1]!), documentVatPreflightApplyMatch[2]!);
    }

    // Bookkeeping write route (#213, slice 4): issue a sales invoice.
    const invoiceIssueMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/issue$/.exec(path);
    if (invoiceIssueMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoiceIssueMatch[1]!);
      return await handleInvoiceIssue(config, request, slug);
    }

    // Cockpit read+render route (#440): forhåndsvis en faktura — render the
    // customer-facing PDF without writing anything to the ledger so the owner
    // can verify layout/amounts/customer-address BEFORE the irreversible
    // posting. Same body shape as `invoices/issue`; the response is the raw
    // PDF bytes (Content-Type application/pdf). NO sequence draw, NO documents
    // row, NO audit_log entry — the preview is read-only.
    const invoicePreviewMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/preview$/.exec(path);
    if (invoicePreviewMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoicePreviewMatch[1]!);
      return await handleInvoicePreview(config, request, slug);
    }

    // Bookkeeping write route (#213, slice 4): post an issued invoice.
    const invoicePostMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/post$/.exec(path);
    if (invoicePostMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoicePostMatch[1]!);
      return await handleInvoicePost(config, request, slug);
    }

    // Bookkeeping write route (#213, slice 4): settle an invoice from bank.
    const invoiceSettleMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/settle$/.exec(path);
    if (invoiceSettleMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoiceSettleMatch[1]!);
      return await handleInvoiceSettle(config, request, slug);
    }

    // Bookkeeping write route (#412): credit an issued invoice. The Cockpit
    // becomes a third caller of `issueCreditNote`, alongside the CLI's
    // `invoice credit-note` command and the MCP tool.
    const invoiceCreditMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/credit-note$/.exec(path);
    if (invoiceCreditMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoiceCreditMatch[1]!);
      return await handleInvoiceCreditNote(config, request, slug);
    }

    // Status-only route must precede the broader send-public matcher.
    const invoiceSendPublicStatusMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/send-public\/status$/.exec(path);
    if (invoiceSendPublicStatusMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoiceSendPublicStatusMatch[1]!);
      return await handleInvoiceSendPublicStatus(config, request, slug);
    }

    // Bookkeeping write route (#428): transmit an issued invoice through the
    // selected company's locally configured DigiSense identity.
    const invoiceSendPublicMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/send-public$/.exec(path);
    if (invoiceSendPublicMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoiceSendPublicMatch[1]!);
      return await handleInvoiceSendPublic(config, request, slug);
    }

    // Bookkeeping write route (#429): send an issued invoice to the
    // customer's e-mail with the PDF attached. Cockpit becomes a third
    // caller of `sendInvoiceEmail`, alongside the CLI's `invoice send`
    // command and the MCP tool `invoice_send_email`. SMTP config is read
    // from `config/smtp.json` inside the company directory so credentials
    // never enter core state or the request body.
    const invoiceSendEmailMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/send-email$/.exec(path);
    if (invoiceSendEmailMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoiceSendEmailMatch[1]!);
      return await handleInvoiceSendEmail(config, request, slug);
    }

    // Bookkeeping write route (#434): register + send a payment reminder
    // (rykker) for an overdue invoice. Combines three existing core calls
    // (`registerInvoiceReminder`, `postInvoiceReminderToLedger`,
    // `sendInvoiceEmail` with `kind: 'reminder'`) so the cockpit's
    // "Send rykker" button is a one-click write. Statutory rentel. § 9b
    // limits (max 100 kr/reminder, max 3 reminders, >= 10 days apart) are
    // enforced by the core; a violation is mapped to a 400.
    const invoiceSendReminderMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/send-reminder$/.exec(path);
    if (invoiceSendReminderMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoiceSendReminderMatch[1]!);
      return await handleInvoiceSendReminder(config, request, slug);
    }

    const supplierCommitmentPlanMatch = /^\/api\/companies\/([^/]+)\/supplier-commitments\/plan$/.exec(path);
    if (supplierCommitmentPlanMatch) { if(method!=="POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute"); return await handleSupplierCommitmentPlan(config,request,decodeURIComponent(supplierCommitmentPlanMatch[1]!)); }
    const supplierCommitmentChangeMatch = /^\/api\/companies\/([^/]+)\/supplier-commitments\/change$/.exec(path);
    if (supplierCommitmentChangeMatch) { if(method!=="POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute"); return await handleSupplierCommitmentChange(config,request,decodeURIComponent(supplierCommitmentChangeMatch[1]!)); }
    const supplierCommitmentMatchMatch = /^\/api\/companies\/([^/]+)\/supplier-commitments\/match$/.exec(path);
    if (supplierCommitmentMatchMatch) { if(method!=="POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute"); return await handleSupplierCommitmentMatch(config,request,decodeURIComponent(supplierCommitmentMatchMatch[1]!)); }
    const supplierCommitmentMatchesMatch = /^\/api\/companies\/([^/]+)\/supplier-commitments\/matches$/.exec(path);
    if (supplierCommitmentMatchesMatch) { if(method!=="GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute"); return handleSupplierCommitmentMatches(config,decodeURIComponent(supplierCommitmentMatchesMatch[1]!),url); }
    const supplierCommitmentsMatch = /^\/api\/companies\/([^/]+)\/supplier-commitments$/.exec(path);
    if (supplierCommitmentsMatch) { const slug=decodeURIComponent(supplierCommitmentsMatch[1]!); if(method==="GET")return handleSupplierCommitments(config,slug,url); if(method==="POST")return await handleSupplierCommitmentApply(config,request,slug); throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute"); }

    // Leverandørfaktura-arbejdsbordet (#340) — match the per-id /pay route
    // first because the bare /payables routes would otherwise consume it.
    const directPayableCorrectionMatch = /^\/api\/companies\/([^/]+)\/payables\/direct-bank-correction\/(plan|apply)$/.exec(path);
    if (directPayableCorrectionMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(directPayableCorrectionMatch[1]!);
      return directPayableCorrectionMatch[2] === "plan"
        ? await handleDirectBankPurchasePayablePlan(config, request, slug)
        : await handleDirectBankPurchasePayableApply(config, request, slug);
    }
    const legacyPayableBackfillMatch=/^\/api\/companies\/([^/]+)\/payables\/legacy-backfill\/(plan|apply)$/.exec(path);
    if(legacyPayableBackfillMatch){if(method!=="POST")throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");const slug=decodeURIComponent(legacyPayableBackfillMatch[1]!);return legacyPayableBackfillMatch[2]==="plan"?await handleLegacyPayableBackfillPlan(config,request,slug):await handleLegacyPayableBackfillApply(config,request,slug);}
    const payablePayMatch =
      /^\/api\/companies\/([^/]+)\/payables\/(\d+)\/pay$/.exec(path);
    if (payablePayMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(payablePayMatch[1]!);
      return await handlePayablePay(config, request, slug, payablePayMatch[2]!);
    }

    const payablesMatch = /^\/api\/companies\/([^/]+)\/payables$/.exec(path);
    if (payablesMatch) {
      const slug = decodeURIComponent(payablesMatch[1]!);
      if (method === "GET") return handleCompanyPayables(config, slug, url);
      if (method === "POST") {
        return await handlePayableRegister(config, request, slug);
      }
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }

    // Bookkeeping write route (#287): close an accounting period — the
    // prerequisite for a momsangivelse.
    const periodCloseMatch =
      /^\/api\/companies\/([^/]+)\/periods\/close$/.exec(path);
    if (periodCloseMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(periodCloseMatch[1]!);
      return await handleClosePeriod(config, request, slug);
    }

    const periodReadinessMatch = /^\/api\/companies\/([^/]+)\/periods\/close-readiness$/.exec(path);
    if (periodReadinessMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      return handlePeriodCloseReadiness(config, decodeURIComponent(periodReadinessMatch[1]!), request);
    }
    const periodReviewMatch = /^\/api\/companies\/([^/]+)\/periods\/close-review$/.exec(path);
    if (periodReviewMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      return await handlePeriodCloseReview(config, request, decodeURIComponent(periodReviewMatch[1]!));
    }
    const periodStatusMatch = /^\/api\/companies\/([^/]+)\/periods\/close-status$/.exec(path);
    if (periodStatusMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      return handlePeriodCloseStatus(config, decodeURIComponent(periodStatusMatch[1]!), request);
    }

    // Bookkeeping write route (#301): reopen a closed accounting period — the
    // controlled, audit-logged recovery path for a period closed too early.
    const periodReopenMatch =
      /^\/api\/companies\/([^/]+)\/periods\/reopen$/.exec(path);
    if (periodReopenMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(periodReopenMatch[1]!);
      return await handleReopenPeriod(config, request, slug);
    }

    // Anlægskartotek read + write routes (#336). The cockpit becomes a third
    // caller of `src/core/assets.ts` alongside the CLI's `asset` sub-commands
    // and the MCP `asset_*` tools — no depreciation arithmetic is reimplemented
    // here. Write routes go through `withCompanyMutation`, so the backup-lock,
    // the localhost gate, actor attribution and the confirm gate all apply.
    const assetsCollectionMatch =
      /^\/api\/companies\/([^/]+)\/assets$/.exec(path);
    if (assetsCollectionMatch) {
      const slug = decodeURIComponent(assetsCollectionMatch[1]!);
      if (method === "GET") return handleCompanyAssets(config, slug);
      if (method === "POST")
        return await handleAssetRegister(config, request, slug);
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }

    const assetWriteOffMatch =
      /^\/api\/companies\/([^/]+)\/assets\/write-off$/.exec(path);
    if (assetWriteOffMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(assetWriteOffMatch[1]!);
      return await handleAssetWriteOff(config, request, slug);
    }

    const assetNextDepreciationMatch =
      /^\/api\/companies\/([^/]+)\/assets\/(\d+)\/next-depreciation$/.exec(path);
    if (assetNextDepreciationMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(assetNextDepreciationMatch[1]!);
      return handleAssetNextDepreciation(
        config,
        slug,
        assetNextDepreciationMatch[2]!,
      );
    }

    const assetDepreciateMatch =
      /^\/api\/companies\/([^/]+)\/assets\/(\d+)\/depreciate$/.exec(path);
    if (assetDepreciateMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(assetDepreciateMatch[1]!);
      return await handleAssetDepreciate(
        config,
        request,
        slug,
        assetDepreciateMatch[2]!,
      );
    }

    // Agent-forslag → menneskelig godkendelse (#346). The agent loop and the
    // exception sync functions in `core/exceptions.ts` produce open `AGENT_*`
    // rows whenever a deterministic agent run needs a human decision; this
    // surface lists them, approves them, or rejects them. Write routes go
    // through `withCompanyMutation`, so the backup-lock, the localhost gate
    // and actor attribution all apply. Match the per-id /approve and /reject
    // routes BEFORE the bare /agent-suggestions route so the shorter pattern
    // does not consume them.
    const agentSuggestionApproveMatch =
      /^\/api\/companies\/([^/]+)\/agent-suggestions\/(\d+)\/approve$/.exec(
        path,
      );
    if (agentSuggestionApproveMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(agentSuggestionApproveMatch[1]!);
      return await handleApproveAgentSuggestion(
        config,
        request,
        slug,
        agentSuggestionApproveMatch[2]!,
      );
    }

    const agentSuggestionRejectMatch =
      /^\/api\/companies\/([^/]+)\/agent-suggestions\/(\d+)\/reject$/.exec(
        path,
      );
    if (agentSuggestionRejectMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(agentSuggestionRejectMatch[1]!);
      return await handleRejectAgentSuggestion(
        config,
        request,
        slug,
        agentSuggestionRejectMatch[2]!,
      );
    }

    const agentSuggestionsMatch =
      /^\/api\/companies\/([^/]+)\/agent-suggestions$/.exec(path);
    if (agentSuggestionsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(agentSuggestionsMatch[1]!);
      return handleCompanyAgentSuggestions(config, slug);
    }

    const companyMatch = /^\/api\/companies\/([^/]+)$/.exec(path);
    if (companyMatch) {
      const slug = decodeURIComponent(companyMatch[1]!);
      if (method === "PATCH") return await handleCompanyUpdate(config, slug, request);
      throw ApiError.methodNotAllowed("kun PATCH er understøttet på denne rute");
    }

    // Anything under /api that did not match a route is a JSON 404. Any other
    // path is a cockpit-SPA route: serve the built app (with the index.html
    // fallback) when it exists, else fall through to the JSON 404.
    if (!path.startsWith("/api")) {
      if (method === "GET" || method === "HEAD") {
        const asset = serveStatic(config.staticRoot, path);
        if (asset) return asset;
        // No SPA built — keep `/` a friendly health probe for API-only runs.
        if (path === "/") return handleHealth(config, ROUTE_CATALOG);
      }
    }

    throw ApiError.notFound("ukendt endpoint");
  } catch (err) {
    // (4) Single error edge. ApiError → its code; anything else → generic 500
    // with no leaked detail.
    const { status, body } = toErrorResponse(err);
    return jsonResponse(body, status);
  }
}
