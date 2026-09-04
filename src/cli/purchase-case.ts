import type { CommandDispatch } from "../cli-dispatch";
import { openLedgerReadOnly } from "../core/ledger-inspection";
import { companyPaths } from "../core/paths";
import { getPurchaseCase, listPurchaseCases } from "../core/purchase-cases";
import { createPurchaseCase, reviewPurchaseCase, type DocumentationOutcome, type PurchaseCaseSource } from "../core/purchase-cases";
import { openCommandDb } from "../cli-dispatch";
import { migrate } from "../core/db";
import { executeLocalIdempotentMutation, validateIdempotencyKey } from "../core/idempotency";
import { authorizeMcpTool, createMcpSecurityContextFromEnv } from "../mcp/security";

const outcome = (value: unknown): DocumentationOutcome | null => value === "unresolved" || value === "ordinary_evidence_sufficient" || value === "alternative_evidence_assessed" ? value : null;
async function authenticated(ctx: any, tool: "purchase_case_create" | "purchase_case_review") {
  const security = createMcpSecurityContextFromEnv();
  if (!security) ctx.fatal(`${tool} requires RENTEMESTER_SERVICE_PRINCIPAL_TOKEN and RENTEMESTER_WORKSPACE`);
  const allowed = await authorizeMcpTool(security!, tool, { company: ctx.companyRoot() });
  if (!allowed) ctx.fatal(`${tool} requires an active authenticated service principal with current company membership`);
  return { kind: allowed!.principal.kind, subjectId: allowed!.principal.subjectId } as const;
}
const audit = (ctx: any) => { const createdBy = ctx.cliActor ?? ctx.inferredMutationActor() ?? ctx.fatal("--actor is required"); const createdByProgram = ctx.cliActorVia ?? "rentemester-cli"; return { createdBy, createdByProgram, auditActor: `${createdBy} via ${createdByProgram}` }; };

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
  dispatch.on("purchase-case", "create", async (ctx) => {
    if (ctx.arg("--confirm") !== "yes") ctx.fatal("purchase-case create requires --confirm yes");
    const kind = ctx.arg("--source-kind"); const id = Number(ctx.arg("--source-id"));
    const source = (kind === "document" || kind === "bank_transaction" || kind === "payable") && Number.isInteger(id) && id > 0 ? { kind, id } as PurchaseCaseSource : ctx.fatal("valid --source-kind and --source-id are required");
    const db = openCommandDb(ctx); try { migrate(db); const actor = audit(ctx); const payload = { caseId: ctx.arg("--case-id") ?? null, source, documentationOutcome: ctx.arg("--documentation-outcome") ?? "unresolved", note: ctx.arg("--note") ?? "" }; const run = executeLocalIdempotentMutation(db, { key: validateIdempotencyKey(ctx.arg("--idempotency-key") ?? ctx.fatal("--idempotency-key is required")), operation: "purchase_case_create", principal: await authenticated(ctx, "purchase_case_create"), payload, actor, execute: () => createPurchaseCase(db, { caseId: payload.caseId ?? undefined, source, documentationOutcome: outcome(payload.documentationOutcome) ?? "unresolved", note: payload.note, actor }) }); ctx.emitResult(run.receipt ? { ...run.result, idempotency: run.receipt } : run.result); } finally { db.close(); }
  });
  dispatch.on("purchase-case", "review", async (ctx) => {
    if (ctx.arg("--confirm") !== "yes") ctx.fatal("purchase-case review requires --confirm yes");
    const caseId = ctx.arg("--case-id") ?? ctx.fatal("--case-id is required");
    const expectedVersion = Number(ctx.arg("--expected-version"));
    const expectedSourceFingerprint = ctx.arg("--expected-source-fingerprint");
    const documentationOutcome = outcome(ctx.arg("--documentation-outcome"));
    if (!Number.isInteger(expectedVersion) || expectedVersion <= 0 || !expectedSourceFingerprint || !/^[a-f0-9]{64}$/.test(expectedSourceFingerprint) || !documentationOutcome) ctx.fatal("valid --expected-version, --expected-source-fingerprint and --documentation-outcome are required");
    const db = openCommandDb(ctx); try { migrate(db); const actor = audit(ctx); const payload = { caseId, expectedVersion, expectedSourceFingerprint: expectedSourceFingerprint!, documentationOutcome: documentationOutcome!, note: ctx.arg("--note") ?? "" }; const run = executeLocalIdempotentMutation(db, { key: validateIdempotencyKey(ctx.arg("--idempotency-key") ?? ctx.fatal("--idempotency-key is required")), operation: "purchase_case_review", principal: await authenticated(ctx, "purchase_case_review"), payload, actor, execute: () => reviewPurchaseCase(db, { ...payload, actor }) }); ctx.emitResult(run.receipt ? { ...run.result, idempotency: run.receipt } : run.result); } finally { db.close(); }
  });
}
