import { realpathSync } from "node:fs";
import { relative, resolve, sep } from "node:path";
import { AsyncLocalStorage } from "node:async_hooks";
import { defaultKeyHasher } from "@better-auth/api-key";
import type { RoutePermission } from "../core/access-permissions";
import { authorizeWorkspaceRoute } from "../core/workspace-access";
import { openWorkspaceControlReadOnlyDb } from "../core/workspace-control";
import { isValidSlug, listWorkspaceCompanies } from "../core/workspace";
import { resolveWorkspaceCompany } from "../core/workspace-company-resolver";
import { WORKSPACE_SERVICE_PRINCIPAL_CONFIG_ID } from "../server/better-auth";

export const MCP_TOOL_PERMISSIONS: Readonly<Record<string, RoutePermission>> = Object.freeze(Object.fromEntries([
  ["agent_capability_search", "public.read"], ["agent_workflow_describe", "public.read"], ["cvr_lookup", "public.read"], ["invoice_validate", "public.read"], ["meta_about", "public.read"],
  ["portfolio_overview", "workspace.read"], ["company_add", "workspace.manage"],
  ["cfo_analytics_query", "workspace.read"],
  ..."accounts_list accounts_roles_status accrual_register_report asset_register_report audit_log_list audit_verify bank_account_list bank_list bank_suggest_matches bank_reconciliation_correction_plan bank_legacy_binding_plan direct_bank_purchase_payable_correction_plan bookkeeping_batch_plan bookkeeping_batch_status bookkeeping_workbench purchase_case_get purchase_case_list purchase_overview budget_forecast liquidity_forecast_13_week budget_list budget_vs_actual company_profile_get customer_list dimension_assignment_list dimension_assignment_plan dimension_budget_list dimension_budget_plan dimension_definition_list dimension_member_list efaktura_onboarding_status efaktura_status exceptions_list import_archive_list invoice_compensation_calc invoice_find invoice_imported_receivables invoice_imported_receivables_backfill_plan invoice_imported_receivable_settlement_plan invoice_imported_receivable_settlement_status invoice_interest_calc invoice_interest_correction_calc invoice_list invoice_overdue invoice_status journal_dry_run journal_list mileage_list mileage_report payable_legacy_backfill_plan payable_list supplier_commitment_plan supplier_commitment_list supplier_commitment_matches supplier_commitment_alerts period_close_readiness period_close_status period_list posting_rule_explain reconcile_bank recurring_invoice_list retention_status system_healthcheck tax_return_prepare vat_filing vat_oss_report vat_report vendor_list".split(" ").map((n) => [n, "company.read"]),
  ["accounting_approval_policy_get", "company.read"], ["accounting_approval_policy_set", "company.admin"],
  ..."workspace_party_search workspace_party_inspect corporate_record_list corporate_record_inspect corporate_record_download legacy_party_mapping_plan legacy_party_mapping_list vendor_identity_enrichment_plan vendor_identity_enrichment_list".split(" ").map((n) => [n, "company.read"]),
  ..."intercompany_disposition_plan intercompany_disposition_status".split(" ").map((n) => [n, "company.ownership.read"]),
  ..."company_knowledge_context".split(" ").map((n) => [n, "company.knowledge.read"]),
  ..."ownership_graph_query ownership_snapshot_history".split(" ").map((n) => [n, "company.ownership.read"]),
  ..."documents_invoice_extraction documents_list documents_parsed_text documents_parse_status".split(" ").map((n) => [n, "company.documents.read"]),
  ..."documents_party_link_list documents_party_link_inspect documents_party_link_plan party_coverage party_coverage_plan".split(" ").map((n) => [n, "company.documents.read"]),
  ..."workspace_inbox_list workspace_inbox_inspect workspace_inbox_status".split(" ").map((n) => [n, "company.documents.read"]),
  ..."documents_ingest documents_parse documents_parse_pending mail_intake_ingest imap_intake_poll".split(" ").map((n) => [n, "company.documents.upload"]),
  ..."workspace_inbox_ingest workspace_inbox_assign workspace_inbox_complete".split(" ").map((n) => [n, "company.documents.upload"]),
  ..."accounts_add accounts_role_confirm bank_account_update company_sync_cvr customer_create dimension_definition_create dimension_member_create dimension_definition_lifecycle dimension_member_lifecycle documents_enrich documents_extract_invoice documents_set_company_context documents_review_purchase_vat_evidence documents_review_non_eu_reverse_charge_evidence posting_rule_propose recurring_invoice_create vendor_create".split(" ").map((n) => [n, "company.master-data"]),
  ..."workspace_party_create workspace_party_link_role corporate_record_ingest corporate_record_link corporate_record_enrich corporate_record_supersede legacy_party_mapping_apply legacy_party_mapping_supersede vendor_identity_enrichment_apply".split(" ").map((n) => [n, "company.master-data"]),
  ..."documents_party_link_apply documents_party_link_supersede documents_internal_no_external_party documents_internal_no_external_party_supersede party_coverage_apply".split(" ").map((n) => [n, "company.master-data"]),
  ..."company_knowledge_propose company_knowledge_review company_knowledge_supersede".split(" ").map((n) => [n, "company.knowledge.manage"]),
  ..."ownership_snapshot_propose ownership_snapshot_review ownership_snapshot_apply".split(" ").map((n) => [n, "company.ownership.manage"]),
  ..."intercompany_disposition_propose intercompany_disposition_approve intercompany_disposition_link intercompany_disposition_settle intercompany_disposition_reopen intercompany_disposition_supersede".split(" ").map((n) => [n, "company.ownership.manage"]),
  ..."accrual_register asset_register bank_import bookkeeping_batch_dry_run bookkeeping_batch_persist purchase_case_create budget_set dimension_assignment_apply supplier_commitment_apply supplier_commitment_change supplier_commitment_match efaktura_konfigurer efaktura_modtag efaktura_modtag_workspace efaktura_onboard efaktura_registrer expense_book invoice_claim_compensation invoice_claim_interest invoice_credit_note invoice_issue invoice_render invoice_remind mileage_log payable_register recurring_invoice_generate recurring_invoice_run_workspace vat_filing_evidence_record".split(" ").map((n) => [n, "company.draft.write"]),
  ..."accrual_recognize asset_depreciate asset_write_off bank_reconciliation_correction_apply bank_legacy_binding_apply direct_bank_purchase_payable_correction_apply bookkeeping_batch_apply expense_vat_preflight_apply invoice_apply_payment invoice_imported_receivables_backfill_apply invoice_imported_receivable_settlement_apply invoice_post invoice_post_compensation invoice_post_interest invoice_post_interest_correction invoice_post_reminder invoice_refund_bank invoice_settle_bank invoice_settle_claim_bank invoice_write_off_bad_debt journal_post journal_reverse payable_legacy_backfill_apply payable_pay period_close vat_post_eu_service_purchase vat_post_representation_purchase".split(" ").map((n) => [n, "company.ledger.post"]),
  ..."bookkeeping_batch_approve purchase_case_review purchase_case_reassess purchase_case_group_review dimension_assignment_replace dimension_assignment_supersede dimension_budget_apply exception_resolve period_close period_close_review posting_rule_approve".split(" ").map((n) => [n, "company.review"]),
  ..."workspace_party_propose_merge workspace_party_approve_merge".split(" ").map((n) => [n, "company.review"]),
  ..."gdpr_audit_log gdpr_discover gdpr_export import_archive_year mileage_export".split(" ").map((n) => [n, "company.export"]),
  ..."customer_validate_vat expense_vat_preflight vat_eu_sales_list".split(" ").map((n) => [n, "company.external-lookup"]),
  ..."efaktura_send invoice_send_email peppol_submit_public_invoice".split(" ").map((n) => [n, "company.external-send"]),
  ..."system_backup system_backup_archive system_backup_confirm_placement system_backup_destination_add system_backup_destination_list system_backup_destination_remove system_backup_governance system_backup_lock system_backup_place system_backup_status system_backup_verify_remote_placement system_export_authority system_restore_backup".split(" ").map((n) => [n, "company.admin"]),
] as Array<[string, RoutePermission]>));

export type McpSecurityContext = { workspaceRoot: string; verify(): Promise<{ serviceAccountId: string; credentialId: string } | null> };
export type McpAuthenticatedPrincipal = { kind: "service-account"; subjectId: string; credentialId: string };
const requestPrincipal = new AsyncLocalStorage<McpAuthenticatedPrincipal>();
const requestWorkspace = new AsyncLocalStorage<string>();
export function currentMcpAuthenticatedPrincipal(): McpAuthenticatedPrincipal | undefined { return requestPrincipal.getStore(); }

/** Re-check a permission against the live control database at the moment an
 * exceptional write is attempted.  The audit actor is deliberately absent:
 * only the authenticated service principal and its current membership count. */
export function mcpHasLiveCompanyPermission(companyRoot: string, permission: RoutePermission): boolean {
  const principal = currentMcpAuthenticatedPrincipal();
  const workspaceRoot = requestWorkspace.getStore();
  if (!principal || !workspaceRoot) return false;
  const company = resolveMcpWorkspaceCompany({ workspaceRoot, verify: async () => null }, companyRoot);
  if (!company) return false;
  const db = openWorkspaceControlReadOnlyDb(workspaceRoot);
  try { return authorizeWorkspaceRoute(db, workspaceRoot, { userId: principal.subjectId, companySlug: company.slug, permission }).allowed; }
  finally { db.close(); }
}

/** Captures the secret once then removes it from child-process environment. */
export function createMcpSecurityContextFromEnv(env: NodeJS.ProcessEnv = process.env): McpSecurityContext | null {
  const token = env.RENTEMESTER_SERVICE_PRINCIPAL_TOKEN?.trim();
  if (!token) return null;
  delete env.RENTEMESTER_SERVICE_PRINCIPAL_TOKEN;
  const configured = env.RENTEMESTER_WORKSPACE?.trim();
  if (!configured) throw new Error("MCP service credentials require RENTEMESTER_WORKSPACE");
  const workspaceRoot = realpathSync(configured);
  return {
    workspaceRoot,
    async verify() {
      const hash = await defaultKeyHasher(token);
      const db = openWorkspaceControlReadOnlyDb(workspaceRoot);
      try {
        const row = db.query(`SELECT "referenceId" AS user_id, "id" AS credential_id FROM "apikey" WHERE "key" = ? AND "configId" = ? AND COALESCE("enabled",1) = 1 AND ("expiresAt" IS NULL OR "expiresAt" > CURRENT_TIMESTAMP)`).get(hash, WORKSPACE_SERVICE_PRINCIPAL_CONFIG_ID) as { user_id?: string; credential_id?: string } | null;
        if (!row?.user_id) return null;
        const principal = db.query("SELECT 1 FROM rm_workspace_service_principals WHERE user_id = ?").get(row.user_id);
        return principal && row.credential_id ? { serviceAccountId: row.user_id, credentialId: row.credential_id } : null;
      } finally { db.close(); }
    },
  };
}

export function resolveMcpWorkspaceCompany(context: McpSecurityContext, raw: unknown): { slug: string; root: string } | null {
  if (typeof raw !== "string" || !raw.trim()) return null;
  const value = raw.trim();
  const bySlug = isValidSlug(value)
    ? resolveWorkspaceCompany(context.workspaceRoot, value, { selection: "registered", archived: "allow", ledger: "optional" })
    : null;
  if (bySlug?.ok) return { slug: bySlug.company.entry.slug, root: realpathSync(bySlug.company.companyRoot) };
  try {
    const candidate = realpathSync(resolve(value));
    const rel = relative(context.workspaceRoot, candidate);
    if (rel === "" || rel.startsWith(`..${sep}`) || rel === ".." || resolve(context.workspaceRoot, rel) !== candidate) return null;
    const byPath = resolveWorkspaceCompany(context.workspaceRoot, rel, { selection: "registered", archived: "allow", ledger: "optional" });
    return byPath.ok ? { slug: byPath.company.entry.slug, root: candidate } : null;
  } catch { return null; }
}

/** A workspace argument is an identity, not a spelling.  `/var` and
 * `/private/var` can name the same macOS workspace, so compare real paths
 * before rejecting it; a different root or an unresolvable/symlink escape
 * still fails closed. */
function isMcpWorkspaceRoot(context: McpSecurityContext, raw: unknown): boolean {
  if (typeof raw !== "string" || !raw.trim()) return false;
  try { return realpathSync(raw) === context.workspaceRoot; } catch { return false; }
}

export async function authorizeMcpTool(context: McpSecurityContext, name: string, args: Record<string, unknown>): Promise<{ root?: string; principal: McpAuthenticatedPrincipal } | null> {
  const permission = MCP_TOOL_PERMISSIONS[name];
  if (!permission) return null;
  const principal = await context.verify();
  if (!principal) return null;
  const authenticated = { kind: "service-account" as const, subjectId: principal.serviceAccountId, credentialId: principal.credentialId };
  if (permission === "public.read") return { principal: authenticated };
  const db = openWorkspaceControlReadOnlyDb(context.workspaceRoot);
  try {
    if (permission.startsWith("workspace.")) {
      if (args.workspace !== undefined && !isMcpWorkspaceRoot(context, args.workspace)) return null;
      return authorizeWorkspaceRoute(db, context.workspaceRoot, { userId: principal.serviceAccountId, permission }).allowed ? { principal: authenticated } : null;
    }
    // Workspace fan-out tools are not a single-company operation.  Authorize
    // the complete active manifest before their handler opens the first
    // ledger.  This prevents a partially-authorized key from learning about
    // or mutating a later company through a best-effort loop.
    if (name === "efaktura_modtag_workspace" || name === "recurring_invoice_run_workspace") {
      if (!isMcpWorkspaceRoot(context, args.workspace)) return null;
      const active = listWorkspaceCompanies(context.workspaceRoot).filter((company) => !company.archived);
      return active.every((company) => authorizeWorkspaceRoute(db, context.workspaceRoot, {
        userId: principal.serviceAccountId, permission, companySlug: company.slug,
      }).allowed) ? { principal: authenticated } : null;
    }
    const company = resolveMcpWorkspaceCompany(context, args.company);
    if (!company) return null;
    if (!authorizeWorkspaceRoute(db, context.workspaceRoot, { userId: principal.serviceAccountId, permission, companySlug: company.slug }).allowed) return null;
    // Ownership snapshots can describe a legal relation across multiple legal
    // entities.  The anchor is not enough: a service credential must have the
    // narrow ownership permission for *every* company endpoint before it can
    // observe, review or apply that relation.  Actors remain audit-only.
    if (name.startsWith("ownership_")) {
      const endpointSlugs = ownershipEndpointSlugs(db, args);
      const endpointPermission = name === "ownership_snapshot_propose" ? permission : name === "ownership_snapshot_history" ? permission : "company.admin";
      if (!endpointSlugs || ![...endpointSlugs].every((slug) =>
        authorizeWorkspaceRoute(db, context.workspaceRoot, { userId: principal.serviceAccountId, permission: endpointPermission, companySlug: slug }).allowed,
      )) return null;
    }
    if (name.startsWith("intercompany_disposition_")) {
      let endpoints: string[] = [];
      const proposal = args.disposition as any;
      if (proposal?.left?.companySlug && proposal?.right?.companySlug) endpoints = [proposal.left.companySlug, proposal.right.companySlug];
      else if (typeof args.dispositionId === "string") {
        const row = db.query("SELECT canonical_payload FROM rm_intercompany_dispositions WHERE disposition_id=?").get(args.dispositionId) as { canonical_payload?: string } | null;
        try { const value=JSON.parse(row?.canonical_payload ?? "null"); endpoints=[value?.left?.companySlug,value?.right?.companySlug].filter((v):v is string=>typeof v==="string"); } catch { return null; }
      }
      if (endpoints.length !== 2 || ![...new Set(endpoints)].every((slug) => isValidSlug(slug) && authorizeWorkspaceRoute(db, context.workspaceRoot, { userId: principal.serviceAccountId, permission, companySlug: slug }).allowed)) return null;
    }
    if (name.startsWith("corporate_record_")) {
      const recordIds=[args.recordId,args.replacementRecordId].filter((value):value is string=>typeof value==="string");
      const scopes=new Set<string>(); let sensitivity="normal";
      for(const recordId of recordIds){const row=db.query("SELECT sensitivity FROM rm_corporate_record_bytes WHERE record_id=?").get(recordId)as {sensitivity?:string}|null;if(!row)return null;sensitivity=row.sensitivity??"normal";for(const scope of db.query("SELECT scope_id FROM rm_corporate_record_scope_assertions WHERE record_id=? AND scope_kind='company'").all(recordId)as Array<{scope_id:string}>)scopes.add(scope.scope_id);}
      if(Array.isArray(args.links))for(const link of args.links as Array<any>)if(link?.type==="company"&&typeof link.id==="string")scopes.add(link.id);
      if(args.type==="company"&&typeof args.id==="string")scopes.add(args.id);
      if(scopes.size>0){const needed=(sensitivity==="normal"&&name!=="corporate_record_ingest"&&name!=="corporate_record_link"&&name!=="corporate_record_supersede")?permission:"company.master-data";if(![...scopes].every(slug=>authorizeWorkspaceRoute(db,context.workspaceRoot,{userId:principal.serviceAccountId,permission:needed,companySlug:slug}).allowed))return null;}
    }
    // Inbox routing can name a second legal entity.  The anchor alone never
    // authorises that destination: validate it before any company database is
    // opened, and return the same opaque denial for hidden targets.
    if (name === "workspace_inbox_assign" || name === "workspace_inbox_complete") {
      if (typeof args.companySlug !== "string" || !isValidSlug(args.companySlug)) return null;
      if (!authorizeWorkspaceRoute(db, context.workspaceRoot, { userId: principal.serviceAccountId, permission, companySlug: args.companySlug }).allowed) return null;
    }
    return { root: company.root, principal: authenticated };
  } finally { db.close(); }
}

/** Returns every company endpoint in an ownership operation, or null when a
 * stored snapshot cannot be resolved.  Never infer access from the actor. */
function ownershipEndpointSlugs(db: ReturnType<typeof openWorkspaceControlReadOnlyDb>, args: Record<string, unknown>): Set<string> | null {
  let facts: unknown[] | null = null;
  if (Array.isArray(args.facts)) facts = args.facts;
  else if (typeof args.snapshotId === "string") {
    const row = db.query("SELECT canonical_facts FROM rm_ownership_source_snapshots WHERE snapshot_id=?").get(args.snapshotId) as { canonical_facts?: string } | null;
    if (!row?.canonical_facts) return null;
    try { facts = JSON.parse(row.canonical_facts); } catch { return null; }
  }
  if (!facts) return new Set();
  const result = new Set<string>();
  for (const raw of facts) {
    if (!raw || typeof raw !== "object") return null;
    const fact = raw as { ownedCompanySlug?: unknown; owner?: { kind?: unknown; companySlug?: unknown } };
    if (typeof fact.ownedCompanySlug !== "string" || !isValidSlug(fact.ownedCompanySlug)) return null;
    result.add(fact.ownedCompanySlug);
    if (fact.owner?.kind === "company") {
      if (typeof fact.owner.companySlug !== "string" || !isValidSlug(fact.owner.companySlug)) return null;
      result.add(fact.owner.companySlug);
    }
  }
  return result;
}

/**
 * Bind authenticated identity (and, for live re-authorisation, its workspace)
 * to one tool invocation.  The two-argument form is retained for existing
 * direct harnesses; production registration always supplies the workspace.
 */
export async function runWithMcpAuthenticatedPrincipal<T>(principal: McpAuthenticatedPrincipal, run: () => Promise<T>): Promise<T>;
export async function runWithMcpAuthenticatedPrincipal<T>(principal: McpAuthenticatedPrincipal, workspaceRoot: string, run: () => Promise<T>): Promise<T>;
export async function runWithMcpAuthenticatedPrincipal<T>(principal: McpAuthenticatedPrincipal, workspaceOrRun: string | (() => Promise<T>), maybeRun?: () => Promise<T>): Promise<T> {
  const run = typeof workspaceOrRun === "function" ? workspaceOrRun : maybeRun!;
  return requestPrincipal.run(principal, () => typeof workspaceOrRun === "string" ? requestWorkspace.run(workspaceOrRun, run) : run());
}
