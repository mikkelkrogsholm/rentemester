// GDPR erasure handler (#334).

import { eraseGdprSubject } from "../../core/gdpr";
import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { withCompanyMutation } from "../mutations";
import { okResponse, optionalBodyString } from "./_shared";

/**
 * POST /api/companies/:slug/gdpr/erase — GDPR-anonymisering (#334).
 *
 * Body: `{ cvr?, name?, asOf? }`. Wrapper omkring `eraseGdprSubject` fra
 * kernen — den skriver append-only tombstones, men afviser rækker der
 * stadig er under bogføringspligt (5-års retention).
 */
export async function handleGdprErase(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const cvr = optionalBodyString(body, "cvr");
      const name = optionalBodyString(body, "name");
      const asOf = optionalBodyString(body, "asOf");
      if (!cvr && !name) {
        throw ApiError.badRequest(
          "cvr eller name skal sættes — én af dem identificerer subject'et.",
        );
      }
      const erasure = eraseGdprSubject(ctx.db, {
        cvr: cvr ?? null,
        name: name ?? null,
        asOf: asOf ?? null,
      });
      void ctx.actor;
      return erasure;
    },
  );
  return okResponse({ gdprErasure: result });
}
