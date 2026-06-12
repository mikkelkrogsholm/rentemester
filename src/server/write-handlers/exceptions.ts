// Exceptions + agent-suggestion write handlers (#213 slice 1, #346).

import {
  resolveException,
} from "../../core/exceptions";
import { openDb } from "../../core/db";
import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { withCockpitActor } from "../actor";
import { withCompanyMutation } from "../mutations";
import {
  okResponse,
  optionalBodyString,
  parseIdParam,
} from "./_shared";

/**
 * POST /api/companies/:slug/exceptions/:id/resolve — clears an open exception.
 *
 * Body: `{ note?: string }`. Non-destructive (the exception stays in the
 * ledger, only its status flips to `resolved`), so no `confirm` is required —
 * the Cockpit modal is the human's consent.
 *
 * Goes through `withCompanyMutation`, so the backup lock, the localhost gate
 * and actor attribution all apply. The resolved actor is recorded as the
 * exception's `resolvedBy`, so the audit trail shows the Cockpit cleared it.
 */
export async function handleResolveException(
  config: ServerConfig,
  request: Request,
  slug: string,
  idRaw: string,
): Promise<Response> {
  const id = parseIdParam(idRaw, "id");

  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const note = optionalBodyString(body, "note");
      // The actor flows through as `resolvedBy` (an explicit payload param —
      // never an env var), so a Cockpit-cleared exception is attributable.
      const payload = withCockpitActor(
        { id, note: note ?? null, resolvedBy: ctx.actor.createdBy },
        ctx.actor,
      );
      return resolveException(ctx.db, payload);
    },
  );

  return okResponse({
    exception: { id, resolved: result.resolved },
  });
}

/**
 * Verifies that the target exception is an open `AGENT_*` row, mapping a
 * missing/closed/wrong-type row to a friendly Danish 409 instead of a generic
 * "exception ... does not exist". The agent-suggestions view only ever shows
 * open AGENT_* rows, so a non-match means the row was closed in another tab
 * (a race) — never a server bug worth a 500.
 */
function assertOpenAgentException(
  db: ReturnType<typeof openDb>,
  id: number,
): { type: string } {
  const row = db
    .query(
      `SELECT type, status FROM exceptions WHERE id = ? LIMIT 1`,
    )
    .get(id) as { type: string; status: string } | null;
  if (!row) {
    throw ApiError.conflict(
      `agent-forslag #${id} findes ikke længere`,
    );
  }
  if (row.status !== "open") {
    throw ApiError.conflict(
      `agent-forslag #${id} er allerede afgjort`,
    );
  }
  if (!row.type.startsWith("AGENT_")) {
    throw ApiError.badRequest(
      `undtagelse #${id} er ikke et agent-forslag (type ${row.type})`,
    );
  }
  return { type: row.type };
}

/**
 * POST /api/companies/:slug/agent-suggestions/:id/approve — owner accepts the
 * agent's suggestion (#346). The cockpit's modal IS the human's consent; the
 * backend records the decision by resolving the underlying exception with a
 * decision-flavored Danish note ("Godkendt af ejer …"). Approval here does NOT
 * post the suggested ledger entry: a separate, action-specific write route
 * (e.g. "Beregn afskrivning" on the Anlæg view, "payable pay" on the
 * Leverandørfaktura view) is the one that touches the ledger. The audit chain
 * is: the suggestion was raised by an agent actor and resolved by the cockpit
 * actor with a "Godkendt"-note, then the action is logged separately when the
 * owner does it. This split keeps the "approve" idempotent: clicking it twice
 * never double-posts; the second click is a no-op 409.
 *
 * Goes through `withCompanyMutation` so the backup lock, the localhost gate
 * and actor attribution all apply.
 */
export async function handleApproveAgentSuggestion(
  config: ServerConfig,
  request: Request,
  slug: string,
  idRaw: string,
): Promise<Response> {
  const id = parseIdParam(idRaw, "id");

  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      assertOpenAgentException(ctx.db, id);
      const note = optionalBodyString(body, "note");
      // The decision-flavored note preserves the owner's free-text reason, if
      // any, while making the audit trail self-explaining at a glance.
      const decisionNote = note
        ? `Godkendt af ejer i cockpit: ${note}`
        : "Godkendt af ejer i cockpit";
      const payload = withCockpitActor(
        { id, note: decisionNote, resolvedBy: ctx.actor.createdBy },
        ctx.actor,
      );
      return resolveException(ctx.db, payload);
    },
  );

  return okResponse({
    suggestion: {
      id,
      decision: "approved" as const,
      resolved: result.resolved,
    },
  });
}

/**
 * POST /api/companies/:slug/agent-suggestions/:id/reject — owner rejects the
 * agent's suggestion (#346). Resolves the underlying exception with a
 * "Afvist"-note carrying the owner's free-text reason. A rejection NEVER posts
 * anything; it just clears the suggestion from the queue. The owner's reason
 * is recommended (it travels into `resolution_note`, the audit-traceable
 * column the agent loop reads to learn from rejections) but technically
 * optional, mirroring the resolve-exception route.
 *
 * Goes through `withCompanyMutation` so the backup lock, the localhost gate
 * and actor attribution all apply.
 */
export async function handleRejectAgentSuggestion(
  config: ServerConfig,
  request: Request,
  slug: string,
  idRaw: string,
): Promise<Response> {
  const id = parseIdParam(idRaw, "id");

  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      assertOpenAgentException(ctx.db, id);
      const note = optionalBodyString(body, "note");
      const decisionNote = note
        ? `Afvist af ejer i cockpit: ${note}`
        : "Afvist af ejer i cockpit";
      const payload = withCockpitActor(
        { id, note: decisionNote, resolvedBy: ctx.actor.createdBy },
        ctx.actor,
      );
      return resolveException(ctx.db, payload);
    },
  );

  return okResponse({
    suggestion: {
      id,
      decision: "rejected" as const,
      resolved: result.resolved,
    },
  });
}
