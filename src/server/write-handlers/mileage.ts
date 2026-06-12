// Mileage register handler (#335).

import {
  createMileageEntry,
  type CreateMileageEntryInput,
} from "../../core/mileage";
import type { ServerConfig } from "../config";
import { withCompanyMutation } from "../mutations";
import {
  okResponse,
  optionalBodyString,
  requireBodyNumber,
  requireBodyString,
} from "./_shared";

/**
 * POST /api/companies/:slug/mileage — registers one mileage entry (#335).
 *
 * Body: `{ tripDate, purpose, fromLocation, toLocation, kilometers, vehicle,
 * driver, ratePerKm, rateBasis, rateSource?, notes?, confirm: true }`. Calls
 * the SAME `createMileageEntry` core function the CLI's `mileage add` command
 * and the MCP tool use — the cockpit is a third caller, never a reimplementer.
 *
 * The mileage register is append-only audit data (the schema's `BEFORE
 * UPDATE` / `BEFORE DELETE` triggers refuse anything else), so the body
 * carries `confirm: true`. Goes through `withCompanyMutation` for the backup
 * lock, the localhost gate and actor attribution.
 */
export async function handleMileageCreate(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const tripDate = requireBodyString(body, "tripDate");
      const purpose = requireBodyString(body, "purpose");
      const fromLocation = requireBodyString(body, "fromLocation");
      const toLocation = requireBodyString(body, "toLocation");
      const kilometers = requireBodyNumber(body, "kilometers");
      const vehicle = requireBodyString(body, "vehicle");
      const driver = requireBodyString(body, "driver");
      const ratePerKm = requireBodyNumber(body, "ratePerKm");
      const rateBasis = requireBodyString(body, "rateBasis");
      const rateSource = optionalBodyString(body, "rateSource");
      const notes = optionalBodyString(body, "notes");

      const input: CreateMileageEntryInput = {
        tripDate,
        purpose,
        fromLocation,
        toLocation,
        kilometers,
        vehicle,
        driver,
        ratePerKm,
        rateBasis,
        ...(rateSource ? { rateSource } : {}),
        ...(notes ? { notes } : {}),
      };
      const created = createMileageEntry(ctx.db, input);
      return {
        ok: created.ok,
        errors: created.errors,
        mileageEntryId: created.mileageEntryId,
        entryNo: created.entryNo,
        amountBasis: created.amountBasis,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    mileage: {
      mileageEntryId: result.mileageEntryId ?? null,
      entryNo: result.entryNo ?? null,
      amountBasis: result.amountBasis ?? null,
    },
  });
}
