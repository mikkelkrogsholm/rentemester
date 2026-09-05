import type { CommandDispatch } from "../cli-dispatch";
import { openLedgerReadOnly } from "../core/ledger-inspection";
import { companyPaths } from "../core/paths";
import { getPurchaseCase, listPurchaseCases } from "../core/purchase-cases";
import { createPurchaseCase, reviewPurchaseCase, reviewPurchaseCaseGroup, type DocumentationOutcome, type PurchaseCaseGroupMember, type PurchaseCaseSource } from "../core/purchase-cases";
import { buildPurchaseOverview } from "../core/purchase-overview";
import { openCommandDb } from "../cli-dispatch";
import { migrate } from "../core/db";
import { executeLocalIdempotentMutation, validateIdempotencyKey } from "../core/idempotency";
import { authorizeMcpTool, createMcpSecurityContextFromEnv } from "../mcp/security";
import { companyRootForSlug, listWorkspaceCompanies, resolveConfiguredWorkspaceRoot } from "../core/workspace";
import { openWorkspaceControlDb } from "../core/workspace-control";
import { realpathSync } from "node:fs";

const outcome = (value: unknown): DocumentationOutcome | null => value === "unresolved" || value === "ordinary_evidence_sufficient" || value === "alternative_evidence_assessed" ? value : null;
async function authenticated(ctx: any, tool: "purchase_case_create" | "purchase_case_review" | "purchase_case_group_review") {
  const security = createMcpSecurityContextFromEnv();
  if (!security) ctx.fatal(`${tool} requires RENTEMESTER_SERVICE_PRINCIPAL_TOKEN and RENTEMESTER_WORKSPACE`);
  const allowed = await authorizeMcpTool(security!, tool, { company: ctx.companyRoot() });
  if (!allowed) ctx.fatal(`${tool} requires an active authenticated service principal with current company membership`);
  return { kind: allowed!.principal.kind, subjectId: allowed!.principal.subjectId } as const;
}
const audit = (ctx: any) => { const createdBy = ctx.cliActor ?? ctx.inferredMutationActor() ?? ctx.fatal("--actor is required"); const createdByProgram = ctx.cliActorVia ?? "rentemester-cli"; return { createdBy, createdByProgram, auditActor: `${createdBy} via ${createdByProgram}` }; };
function approvalContext(ctx:any, principal:{subjectId:string}, expectedPolicyEventHash?:string){const workspaceRoot=resolveConfiguredWorkspaceRoot()??ctx.fatal("purchase-case review requires RENTEMESTER_WORKSPACE");const companyRoot=realpathSync(ctx.companyRoot());const companySlug=listWorkspaceCompanies(workspaceRoot).find(company=>realpathSync(companyRootForSlug(workspaceRoot,company.slug))===companyRoot)?.slug;if(!companySlug)ctx.fatal("purchase-case company is not registered in RENTEMESTER_WORKSPACE");return {controlDb:openWorkspaceControlDb(workspaceRoot),workspaceRoot,companySlug:companySlug!,principalId:principal.subjectId,expectedPolicyEventHash:expectedPolicyEventHash??null};}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("purchase-case", "list", (ctx) => {
    const db = openLedgerReadOnly(companyPaths(ctx.companyRoot()).db);
    try { ctx.emitResult({ ok: true, purchaseCases: listPurchaseCases(db) }); }
    finally { db.close(); }
  });
  dispatch.on("purchase-case", "show", (ctx) => {
    const id = ctx.arg("--case-id") ?? ctx.fatal("--case-id is required");
    const db = openLedgerReadOnly(companyPaths(ctx.companyRoot()).db);
    try { ctx.emitResult({ ok: true, purchaseCase: getPurchaseCase(db, id) }); }
    finally { db.close(); }
  });
  dispatch.on("purchase-case", "overview", (ctx) => {
    const from=ctx.arg("--from") ?? ctx.fatal("--from is required"); const to=ctx.arg("--to") ?? ctx.fatal("--to is required"); const include=ctx.arg("--include-provisional"); if(include!==undefined&&include!=="yes"&&include!=="no")ctx.fatal("--include-provisional must be yes or no");
    const db=openLedgerReadOnly(companyPaths(ctx.companyRoot()).db); try { ctx.emitResult({ok:true,overview:buildPurchaseOverview(db,{from,to,includeProvisional:include===undefined?undefined:include==="yes"})}); } finally { db.close(); }
  });
  dispatch.on("purchase-case", "create", async (ctx) => {
    if (ctx.arg("--confirm") !== "yes") ctx.fatal("purchase-case create requires --confirm yes");
    const kind = ctx.arg("--source-kind"); const id = Number(ctx.arg("--source-id"));
    const source = (kind === "document" || kind === "bank_transaction" || kind === "payable") && Number.isInteger(id) && id > 0 ? { kind, id } as PurchaseCaseSource : ctx.fatal("valid --source-kind and --source-id are required");
    const db = openCommandDb(ctx); try { migrate(db); const actor = audit(ctx); const principal=await authenticated(ctx, "purchase_case_create"); const payload = { caseId: ctx.arg("--case-id") ?? null, source, documentationOutcome: ctx.arg("--documentation-outcome") ?? "unresolved", note: ctx.arg("--note") ?? "" }; const run = executeLocalIdempotentMutation(db, { key: validateIdempotencyKey(ctx.arg("--idempotency-key") ?? ctx.fatal("--idempotency-key is required")), operation: "purchase_case_create", principal, payload, actor, execute: () => createPurchaseCase(db, { caseId: payload.caseId ?? undefined, source, documentationOutcome: outcome(payload.documentationOutcome) ?? "unresolved", note: payload.note, actor,principalId:principal.subjectId }) }); ctx.emitResult(run.receipt ? { ...run.result, idempotency: run.receipt } : run.result); } finally { db.close(); }
  });
  dispatch.on("purchase-case", "review", async (ctx) => {
    if (ctx.arg("--confirm") !== "yes") ctx.fatal("purchase-case review requires --confirm yes");
    const caseId = ctx.arg("--case-id") ?? ctx.fatal("--case-id is required");
    const expectedVersion = Number(ctx.arg("--expected-version"));
    const expectedSourceFingerprint = ctx.arg("--expected-source-fingerprint");
    const documentationOutcome = outcome(ctx.arg("--documentation-outcome"));
    if (!Number.isInteger(expectedVersion) || expectedVersion <= 0 || !expectedSourceFingerprint || !/^[a-f0-9]{64}$/.test(expectedSourceFingerprint) || !documentationOutcome) ctx.fatal("valid --expected-version, --expected-source-fingerprint and --documentation-outcome are required");
    const db = openCommandDb(ctx); try { migrate(db); const actor = audit(ctx); const principal=await authenticated(ctx, "purchase_case_review"); const payload = { caseId, expectedVersion, expectedSourceFingerprint: expectedSourceFingerprint!, expectedPolicyEventHash:ctx.arg("--expected-policy-event-hash")??null, documentationOutcome: documentationOutcome!, note: ctx.arg("--note") ?? "" }; const context=approvalContext(ctx,principal,payload.expectedPolicyEventHash??undefined); try { const run = executeLocalIdempotentMutation(db, { key: validateIdempotencyKey(ctx.arg("--idempotency-key") ?? ctx.fatal("--idempotency-key is required")), operation: "purchase_case_review", principal, payload, actor, execute: () => reviewPurchaseCase(db, { ...payload, actor,approval:context }) }); ctx.emitResult(run.receipt ? { ...run.result, idempotency: run.receipt } : run.result); } finally { context.controlDb.close(); } } finally { db.close(); }
  });
  dispatch.on("purchase-case", "group-review", async (ctx) => {
    if (ctx.arg("--confirm") !== "yes") ctx.fatal("purchase-case group-review requires --confirm yes");
    const raw=ctx.arg("--members-json") ?? ctx.fatal("--members-json is required"); let members:PurchaseCaseGroupMember[]; try { const parsed=JSON.parse(raw); if(!Array.isArray(parsed)||parsed.length<1||parsed.length>100)throw new Error(); const ids=new Set<string>(); members=parsed.map(item=>{if(!item||typeof item!=="object"||typeof item.caseId!=="string"||!Number.isInteger(item.expectedVersion)||typeof item.expectedSourceFingerprint!=="string"||!/^[a-f0-9]{64}$/.test(item.expectedSourceFingerprint)||ids.has(item.caseId))throw new Error();ids.add(item.caseId);return {caseId:item.caseId,expectedVersion:item.expectedVersion,expectedSourceFingerprint:item.expectedSourceFingerprint};}); } catch { ctx.fatal("--members-json must be a JSON array of unique exact case IDs, versions and source fingerprints"); }
    const parsedOutcome=outcome(ctx.arg("--documentation-outcome")); if(!parsedOutcome)ctx.fatal("--documentation-outcome is required"); const documentationOutcome=parsedOutcome!;
    const db=openCommandDb(ctx);try{migrate(db);const actor=audit(ctx);const payload={groupId:ctx.arg("--group-id")??null,members:members!,documentationOutcome,note:ctx.arg("--note")??""};const run=executeLocalIdempotentMutation(db,{key:validateIdempotencyKey(ctx.arg("--idempotency-key")??ctx.fatal("--idempotency-key is required")),operation:"purchase_case_group_review",principal:await authenticated(ctx,"purchase_case_group_review"),payload,actor,execute:()=>reviewPurchaseCaseGroup(db,{...payload,groupId:payload.groupId??undefined,actor})});ctx.emitResult(run.receipt?{...run.result,idempotency:run.receipt}:run.result);}finally{db.close();}
  });
}
