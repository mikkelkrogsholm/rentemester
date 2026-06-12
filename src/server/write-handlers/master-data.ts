// Customer/vendor master-data + CVR lookup handlers (#390, #430).

import {
  createCustomer,
  createVendor,
  deleteCustomer,
  deleteVendor,
  updateCustomer,
  updateVendor,
  type CreateCustomerInput,
  type CreateVendorInput,
  type UpdateCustomerInput,
  type UpdateVendorInput,
} from "../../core/master-data";
import { lookupCvrCompany } from "../../core/cvr";
import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { withCompanyMutation } from "../mutations";
import {
  JSON_HEADERS,
  okResponse,
  parseIdParam,
  requireBodyString,
} from "./_shared";

// ---------------------------------------------------------------------------
// Contacts (Kontakter) — create/update from the Cockpit (#390).
//
// Until now the Kontakter page only offered Import + Administrér; the only
// path to a new customer/vendor was the CLI or a CSV import. These handlers
// give the Cockpit a first-class create/update path, reusing the same
// `createCustomer/createVendor/updateCustomer/updateVendor` core the CLI and
// MCP use. CVR lookup is a separate, read-only endpoint so the modal can
// prefill name/address/contact details before the human commits.
// ---------------------------------------------------------------------------

/** Reads an optional non-empty trimmed string field — `null` clears, missing keeps. */
function readNullableString(
  body: Record<string, unknown>,
  key: string,
): string | undefined {
  const value = body[key];
  if (value === undefined) return undefined;
  if (value === null) return undefined; // explicit null treated as missing (= no change)
  if (typeof value !== "string") {
    throw ApiError.badRequest(`'${key}' must be a string when present`);
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

function parseCreateCustomerBody(body: Record<string, unknown>): CreateCustomerInput {
  const name = requireBodyString(body, "name").trim();
  const input: CreateCustomerInput = { name };
  const address = readNullableString(body, "address");
  if (address !== undefined) input.address = address;
  const vatOrCvr = readNullableString(body, "vatOrCvr");
  if (vatOrCvr !== undefined) input.vatOrCvr = vatOrCvr;
  const email = readNullableString(body, "email");
  if (email !== undefined) input.email = email;
  const phone = readNullableString(body, "phone");
  if (phone !== undefined) input.phone = phone;
  const website = readNullableString(body, "website");
  if (website !== undefined) input.website = website;
  const eanNumber = readNullableString(body, "eanNumber");
  if (eanNumber !== undefined) input.eanNumber = eanNumber;
  const notes = readNullableString(body, "notes");
  if (notes !== undefined) input.notes = notes;
  const defaultCurrency = readNullableString(body, "defaultCurrency");
  if (defaultCurrency !== undefined) input.defaultCurrency = defaultCurrency;
  if (body.paymentTermsDays !== undefined && body.paymentTermsDays !== null) {
    const value = Number(body.paymentTermsDays);
    if (!Number.isInteger(value) || value <= 0) {
      throw ApiError.badRequest("'paymentTermsDays' must be a positive integer");
    }
    input.paymentTermsDays = value;
  }
  return input;
}

function parseCreateVendorBody(body: Record<string, unknown>): CreateVendorInput {
  const name = requireBodyString(body, "name").trim();
  const input: CreateVendorInput = { name };
  const address = readNullableString(body, "address");
  if (address !== undefined) input.address = address;
  const vatOrCvr = readNullableString(body, "vatOrCvr");
  if (vatOrCvr !== undefined) input.vatOrCvr = vatOrCvr;
  const email = readNullableString(body, "email");
  if (email !== undefined) input.email = email;
  const phone = readNullableString(body, "phone");
  if (phone !== undefined) input.phone = phone;
  const website = readNullableString(body, "website");
  if (website !== undefined) input.website = website;
  const defaultExpenseAccount = readNullableString(body, "defaultExpenseAccount");
  if (defaultExpenseAccount !== undefined) input.defaultExpenseAccount = defaultExpenseAccount;
  const defaultVatTreatment = readNullableString(body, "defaultVatTreatment");
  if (defaultVatTreatment !== undefined) input.defaultVatTreatment = defaultVatTreatment;
  const notes = readNullableString(body, "notes");
  if (notes !== undefined) input.notes = notes;
  return input;
}

/** POST /api/companies/:slug/customers — create a customer (#390). */
export async function handleCreateCustomer(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const input = parseCreateCustomerBody(body);
      const created = createCustomer(ctx.db, input);
      return {
        ok: created.ok,
        errors: created.errors,
        customerId: (created as { customerId?: number }).customerId ?? null,
      };
    },
  );
  return okResponse({ customer: { id: result.customerId } });
}

/** POST /api/companies/:slug/vendors — create a vendor (#390). */
export async function handleCreateVendor(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const input = parseCreateVendorBody(body);
      const created = createVendor(ctx.db, input);
      return {
        ok: created.ok,
        errors: created.errors,
        vendorId: (created as { vendorId?: number }).vendorId ?? null,
      };
    },
  );
  return okResponse({ vendor: { id: result.vendorId } });
}

/** PATCH /api/companies/:slug/customers/:id — update a customer (#390). */
export async function handleUpdateCustomer(
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
      // For updates we accept the same fields but pass them as a partial — any
      // field absent in the body is left untouched in the row.
      const input: UpdateCustomerInput = {};
      const create = parseCreateCustomerBody({ name: "x", ...body });
      // parseCreateCustomerBody requires `name`; for updates name is optional.
      // We re-read it explicitly so a missing `name` does not blank the row.
      if (body.name !== undefined) input.name = create.name;
      for (const k of [
        "address",
        "vatOrCvr",
        "email",
        "phone",
        "website",
        "eanNumber",
        "notes",
        "defaultCurrency",
      ] as const) {
        if (body[k] !== undefined) {
          // null = clear, "" = clear, otherwise the trimmed string.
          if (body[k] === null) (input as Record<string, unknown>)[k] = null;
          else (input as Record<string, unknown>)[k] = (create as Record<string, unknown>)[k] ?? null;
        }
      }
      if (body.paymentTermsDays !== undefined) {
        input.paymentTermsDays = create.paymentTermsDays;
      }
      const updated = updateCustomer(ctx.db, id, input);
      return { ok: updated.ok, errors: updated.errors };
    },
  );
  return okResponse({ customer: { id, ok: result.ok } });
}

/** PATCH /api/companies/:slug/vendors/:id — update a vendor (#390). */
export async function handleUpdateVendor(
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
      const input: UpdateVendorInput = {};
      const create = parseCreateVendorBody({ name: "x", ...body });
      if (body.name !== undefined) input.name = create.name;
      for (const k of [
        "address",
        "vatOrCvr",
        "email",
        "phone",
        "website",
        "defaultExpenseAccount",
        "defaultVatTreatment",
        "notes",
      ] as const) {
        if (body[k] !== undefined) {
          if (body[k] === null) (input as Record<string, unknown>)[k] = null;
          else (input as Record<string, unknown>)[k] = (create as Record<string, unknown>)[k] ?? null;
        }
      }
      const updated = updateVendor(ctx.db, id, input);
      return { ok: updated.ok, errors: updated.errors };
    },
  );
  return okResponse({ vendor: { id, ok: result.ok } });
}

/**
 * DELETE /api/companies/:slug/customers/:id — sletter en kunde fra master data
 * (#430). En fejl-importeret eller dubleret kunde skal kunne fjernes fra
 * cockpittet — ikke kun fra CLI'en. Tredje caller af samme `deleteCustomer`
 * core funktion som CLI'en og MCP'en bruger (når disse senere wires).
 *
 * Sletningen blokeres af core hvis kunden er i brug på en åben (ikke-betalt)
 * udstedt faktura — core returnerer `{ok:false, errors:[...]}` med et klart
 * dansk-sproget budskab + fakturanummeret, og `withCompanyMutation` mapper det
 * til en 400 så cockpittet kan vise beskeden verbatim. Bogførte fakturaer
 * påvirkes ikke (buyer-snapshottet på fakturaen er ikke en FK).
 *
 * Sletningen audit-logges i `audit_log` (event_type `customer_delete`).
 * `requireConfirm` er sat fordi sletningen er en irreversibel mutation af
 * master-data.
 */
export async function handleDeleteCustomer(
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
    (ctx) => {
      const deleted = deleteCustomer(ctx.db, id);
      return {
        ok: deleted.ok,
        errors: deleted.errors,
      };
    },
    { requireConfirm: true },
  );
  return okResponse({ customer: { id, deleted: result.ok } });
}

/**
 * DELETE /api/companies/:slug/vendors/:id — sletter en leverandør (#430).
 * Blokeres hvis leverandøren er i brug på en åben gæld (`payables` med
 * `vendor_id` FK). Audit-logges som `vendor_delete`.
 */
export async function handleDeleteVendor(
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
    (ctx) => {
      const deleted = deleteVendor(ctx.db, id);
      return {
        ok: deleted.ok,
        errors: deleted.errors,
      };
    },
    { requireConfirm: true },
  );
  return okResponse({ vendor: { id, deleted: result.ok } });
}

/**
 * GET /api/companies/:slug/cvr-lookup?cvr=12345678 — looks an 8-digit Danish CVR
 * number up in the CVR register (cached server-side, credentials never reach
 * the browser). Used by the Kontakter create/edit modal to prefill name +
 * address. A missing-credentials response degrades cleanly: `{ ok:false,
 * errors:[…] }` returned inside a 200 envelope so the UI can show a hint
 * without surfacing the call as an error.
 */
export async function handleCvrLookup(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  // Read-only — reuse the read-side resolution rather than withCompanyMutation,
  // since this is just an enrichment query (no audit row, no backup lock).
  const url = new URL(request.url);
  const cvr = url.searchParams.get("cvr");
  if (!cvr || cvr.trim().length === 0) {
    throw ApiError.badRequest("'cvr' query parameter is required");
  }
  // Resolve the company db so the CVR-cache table is scoped to this company.
  const { findWorkspaceCompany, companyRootForSlug } = await import(
    "../../core/workspace"
  );
  if (!findWorkspaceCompany(config.workspaceRoot, slug)) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(config.workspaceRoot, slug);
  const { companyPaths } = await import("../../core/paths");
  const dbPath = companyPaths(companyRoot).db;
  const { existsSync } = await import("node:fs");
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }
  const { openDb, migrate } = await import("../../core/db");
  const db = openDb(dbPath);
  try {
    migrate(db);
    const result = await lookupCvrCompany(db, cvr);
    return new Response(
      JSON.stringify({
        ok: true,
        cvr: {
          ok: result.ok,
          cached: result.cached,
          company: result.company ?? null,
          errors: result.errors ?? [],
        },
      }),
      { status: 200, headers: JSON_HEADERS },
    );
  } finally {
    db.close();
  }
}
