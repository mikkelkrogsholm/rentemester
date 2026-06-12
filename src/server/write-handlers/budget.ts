// Budget line handler — append-only owner planning input (#339).
//
// Records (appends) the owner's planned amount for one account in one
// calendar month — the input behind the Budget vs. faktisk view. Each call
// inserts a NEW revision so the history is fully auditable; the latest
// revision is the effective budget. Re-setting the same (account, period)
// pair is therefore always safe — no special "edit existing" handler is
// needed and the audit log carries the full change-of-mind trail.
//
// Goes through `withCompanyMutation` so the backup lock, the localhost gate
// and actor attribution apply. Append-only and reversible by appending a new
// revision, so no `confirm` gate is required — the modal is the consent,
// mirroring the contact-master-data writes.

import { setBudget } from "../../core/budget";
import type { ServerConfig } from "../config";
import { withCockpitActor } from "../actor";
import { withCompanyMutation } from "../mutations";
import {
  okResponse,
  optionalBodyString,
  requireBodyNumber,
  requireBodyString,
} from "./_shared";

/**
 * POST /api/companies/:slug/budget — sets (appends a revision for) one
 * budget line.
 *
 * Body: `{ accountNo: string, period: 'YYYY-MM', amount: number, notes?: string }`.
 * Append-only (every call inserts a new revision, latest wins), so no
 * `confirm` gate. A core rejection (unknown account, malformed period,
 * negative amount) is mapped to a 400 by `withCompanyMutation`.
 */
export async function handleSetBudget(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const accountNo = requireBodyString(body, "accountNo");
      const period = requireBodyString(body, "period");
      const amount = requireBodyNumber(body, "amount");
      const notes = optionalBodyString(body, "notes");
      const payload = withCockpitActor(
        {
          accountNo,
          period,
          amount,
          ...(notes ? { notes } : {}),
        },
        ctx.actor,
      );
      const set = setBudget(ctx.db, payload);
      return {
        ok: set.ok,
        errors: set.errors,
        budgetLineId: set.budgetLineId ?? null,
        accountNo: set.accountNo ?? null,
        period: set.period ?? null,
        amount: set.amount ?? null,
      };
    },
  );
  return okResponse({
    budget: {
      id: result.budgetLineId,
      accountNo: result.accountNo,
      period: result.period,
      amount: result.amount,
    },
  });
}
