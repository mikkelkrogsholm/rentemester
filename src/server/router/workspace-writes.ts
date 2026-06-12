// Workspace-level write handlers: POST /api/companies (create) and
// PATCH /api/companies/:slug (rename/archive). These do NOT use
// withCompanyMutation because they operate on the workspace registry, not
// on a company's ledger — there is no ledger to back up here.

import { createCompany } from "../../core/company";
import {
  findWorkspaceCompany,
  renameWorkspaceCompany,
  setWorkspaceCompanyArchived,
} from "../../core/workspace";
import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { okResponse, optionalString, readJsonBody, requireString } from "./_shared";

/**
 * Parses the optional `payment` body field on the create-company form (#284)
 * into a core `CompanyPaymentInput`. Every sub-field is optional — `createCompany`
 * only creates the primary bank account when at least one carries information.
 */
function parseCreatePayment(
  body: Record<string, unknown>,
): { bankName?: string; registrationNo?: string; accountNo?: string; iban?: string } | undefined {
  const raw = body.payment;
  if (raw === undefined || raw === null) return undefined;
  if (typeof raw !== "object" || Array.isArray(raw)) {
    throw ApiError.badRequest("'payment' must be an object when present");
  }
  const p = raw as Record<string, unknown>;
  const payment: {
    bankName?: string;
    registrationNo?: string;
    accountNo?: string;
    iban?: string;
  } = {};
  for (const field of ["bankName", "registrationNo", "accountNo", "iban"] as const) {
    const value = optionalString(p, field);
    if (value !== undefined) payment[field] = value;
  }
  return Object.keys(payment).length > 0 ? payment : undefined;
}

export async function handleCompanyCreate(
  config: ServerConfig,
  request: Request,
): Promise<Response> {
  const body = await readJsonBody(request);
  const name = requireString(body, "name");
  const payment = parseCreatePayment(body);
  let result;
  try {
    result = createCompany(config.workspaceRoot, {
      name,
      slug: optionalString(body, "slug"),
      cvr: optionalString(body, "cvr") ?? null,
      fiscalYearStartMonth: optionalString(body, "fiscalYearStartMonth"),
      fiscalYearLabelStrategy: optionalString(body, "fiscalYearLabelStrategy"),
      // #300: the VAT settlement cadence. `initialiseCompanyVolume` validates
      // it and throws on an unknown value — re-mapped to a 400 below.
      vatPeriodType: optionalString(body, "vatPeriodType"),
      ...(payment ? { payment } : {}),
    });
  } catch (err) {
    // createCompany throws plain Errors for invalid slug / duplicate. Re-map
    // them to a safe code; the messages it produces are curated (no paths)
    // — except `companyRoot`, which createCompany only embeds for the
    // "already exists" case, so collapse that to a generic conflict.
    const message = err instanceof Error ? err.message : String(err);
    if (/already exists|already registered/i.test(message)) {
      throw ApiError.conflict("der findes allerede en virksomhed med den slug");
    }
    throw ApiError.badRequest(message);
  }
  return okResponse(
    {
      company: { slug: result.slug, name: result.name },
    },
    201,
  );
}

/**
 * Updates a registered company's mutable workspace metadata: the display
 * `name` and/or the `archived` flag. This never touches the slug or the
 * ledger — there is deliberately NO destructive delete of ledger data.
 */
export async function handleCompanyUpdate(
  config: ServerConfig,
  slug: string,
  request: Request,
): Promise<Response> {
  if (!findWorkspaceCompany(config.workspaceRoot, slug)) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const body = await readJsonBody(request);
  const name = optionalString(body, "name");
  const archivedRaw = body.archived;
  if (archivedRaw !== undefined && typeof archivedRaw !== "boolean") {
    throw ApiError.badRequest("'archived' must be a boolean when present");
  }
  if (name === undefined && archivedRaw === undefined) {
    throw ApiError.badRequest("angiv 'name' og/eller 'archived' for at opdatere");
  }
  try {
    let entry = findWorkspaceCompany(config.workspaceRoot, slug)!;
    if (name !== undefined) {
      entry = renameWorkspaceCompany(config.workspaceRoot, slug, name);
    }
    if (typeof archivedRaw === "boolean") {
      entry = setWorkspaceCompanyArchived(config.workspaceRoot, slug, archivedRaw);
    }
    return okResponse({
      company: {
        slug: entry.slug,
        name: entry.name,
        createdAt: entry.createdAt,
        archived: entry.archived,
      },
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    throw ApiError.badRequest(message);
  }
}
