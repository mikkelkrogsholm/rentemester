import { getAccountingApprovalPolicy, setAccountingApprovalPolicy, type AccountingApprovalRiskClass, type AccountingApprovalReviewMode } from "../../core/accounting-approval-policy";
import { openWorkspaceControlDb, openWorkspaceControlReadOnlyDb } from "../../core/workspace-control";
import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { withCompanyMutation } from "../mutations";
import { okResponse } from "./_shared";

function principalId(config: ServerConfig): string {
  const principal = config.requestPrincipal;
  const id = principal?.serviceAccountId ?? principal?.userId;
  if (typeof id !== "string" || !id.trim()) throw ApiError.unauthorized("missing or invalid credentials");
  return id;
}

function riskClass(value: unknown): AccountingApprovalRiskClass {
  if (value === undefined) return "normal";
  if (value === "normal" || value === "elevated") return value;
  throw ApiError.badRequest("riskClass must be normal or elevated");
}

function reviewMode(value: unknown): AccountingApprovalReviewMode {
  if (value === "sole_authorized_bookkeeper" || value === "independent_reviewer") return value;
  throw ApiError.badRequest("reviewMode is required");
}

export function handleAccountingApprovalPolicyGet(config: ServerConfig, slug: string, request: Request): Response {
  const risk = riskClass(new URL(request.url).searchParams.get("riskClass") ?? undefined);
  const db = openWorkspaceControlReadOnlyDb(config.workspaceRoot);
  try { return okResponse({ policy: getAccountingApprovalPolicy(db, slug, risk) }); }
  finally { db.close(); }
}

export async function handleAccountingApprovalPolicySet(config: ServerConfig, request: Request, slug: string): Promise<Response> {
  return okResponse(await withCompanyMutation(request, config, slug, (ctx, body) => {
    const db = openWorkspaceControlDb(config.workspaceRoot);
    try {
      const result = setAccountingApprovalPolicy(db, config.workspaceRoot, {
        scope: { kind: "company", companySlug: slug }, riskClass: riskClass(body.riskClass), reviewMode: reviewMode(body.reviewMode),
        expectedEventHash: typeof body.expectedEventHash === "string" ? body.expectedEventHash : null,
        principalId: principalId(config), actor: ctx.actor.createdBy, confirm: true,
      });
      return { ok: true, ...result };
    } finally { db.close(); }
  }, { requireConfirm: true }));
}
