import type { JournalEntryInput } from "../../core/ledger";
import {
  approveAndPostAccountingDraft,
  createAccountingDraft,
  rejectAccountingDraft,
  reviseAccountingDraft,
  submitAccountingDraft,
} from "../../core/accounting-drafts";
import type { ServerConfig } from "../config";
import { openWorkspaceControlDb } from "../../core/workspace-control";
import { ApiError } from "../errors";
import { withCompanyMutation, type MutationContext } from "../mutations";
import { okResponse, requireBodyString } from "./_shared";

function journalPayload(body: Record<string, unknown>): JournalEntryInput {
  const payload = body.payload;
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
    throw ApiError.badRequest("'payload' is required and must be a journal-entry object");
  }
  return payload as JournalEntryInput;
}

type DraftCoreResult = { ok: boolean; errors?: string[]; accountingDraft?: unknown };

function coreResult(operation: () => unknown): DraftCoreResult {
  try {
    return { ok: true, accountingDraft: operation() };
  } catch (error) {
    return { ok: false, errors: [error instanceof Error ? error.message : String(error)] };
  }
}

function principalId(ctx: MutationContext): string {
  const id = ctx.principal.serviceAccountId ?? ctx.principal.userId;
  if (typeof id !== "string" || !id.trim()) throw ApiError.unauthorized("missing or invalid credentials");
  return id;
}

async function mutate(
  config: ServerConfig,
  request: Request,
  slug: string,
  operation: (ctx: MutationContext, body: Record<string, unknown>) => DraftCoreResult,
  requireConfirm = false,
): Promise<Response> {
  const result = await withCompanyMutation(request, config, slug, operation, { requireConfirm });
  return okResponse({ accountingDraft: result.accountingDraft });
}

export function handleCreateAccountingDraft(config: ServerConfig, request: Request, slug: string): Promise<Response> {
  return mutate(config, request, slug, (ctx, body) => coreResult(() =>
    createAccountingDraft(ctx.db, requireBodyString(body, "draftId"), journalPayload(body), ctx.actor, { principalId: principalId(ctx) })),
  );
}

export function handleReviseAccountingDraft(config: ServerConfig, request: Request, slug: string, draftId: string): Promise<Response> {
  return mutate(config, request, slug, (ctx, body) => coreResult(() =>
    reviseAccountingDraft(ctx.db, draftId, requireBodyString(body, "expectedEventHash"), journalPayload(body), ctx.actor, { principalId: principalId(ctx) })),
  );
}

export function handleSubmitAccountingDraft(config: ServerConfig, request: Request, slug: string, draftId: string): Promise<Response> {
  return mutate(config, request, slug, (ctx, body) => coreResult(() =>
    submitAccountingDraft(ctx.db, draftId, requireBodyString(body, "expectedEventHash"), ctx.actor, { principalId: principalId(ctx) })),
  );
}

export function handleRejectAccountingDraft(config: ServerConfig, request: Request, slug: string, draftId: string): Promise<Response> {
  return mutate(config, request, slug, (ctx, body) => coreResult(() => {
    const control = openWorkspaceControlDb(config.workspaceRoot);
    try {
      return rejectAccountingDraft(ctx.db, draftId, requireBodyString(body, "expectedEventHash"), requireBodyString(body, "reason"), ctx.actor, {
        controlDb: control, workspaceRoot: config.workspaceRoot, companySlug: slug, principalId: principalId(ctx),
        expectedPolicyEventHash: typeof body.expectedPolicyEventHash === "string" ? body.expectedPolicyEventHash : null,
      });
    } finally { control.close(); }
  }),
  );
}

export function handleApproveAndPostAccountingDraft(config: ServerConfig, request: Request, slug: string, draftId: string): Promise<Response> {
  return mutate(config, request, slug, (ctx, body) => coreResult(() => {
    const control = openWorkspaceControlDb(config.workspaceRoot);
    try {
      return approveAndPostAccountingDraft(ctx.db, draftId, requireBodyString(body, "expectedEventHash"), ctx.actor, {
        controlDb: control, workspaceRoot: config.workspaceRoot, companySlug: slug, principalId: principalId(ctx),
        expectedPolicyEventHash: typeof body.expectedPolicyEventHash === "string" ? body.expectedPolicyEventHash : null,
      });
    } finally { control.close(); }
  }), true,
  );
}
