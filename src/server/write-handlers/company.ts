// Company profile + period close/reopen handlers (#284, #287, #300, #301).

import {
  getCompanySettings,
  resolveCompanyPaymentDetails,
  setCompanyProfile,
  type CompanyPaymentInput,
} from "../../core/company";
import {
  closeAccountingPeriod,
  reopenAccountingPeriod,
  setCompanyVatPeriodType,
  normalizeVatPeriodType,
} from "../../core/periods";
import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { withCompanyMutation } from "../mutations";
import {
  okResponse,
  optionalBodyString,
  requireBodyString,
} from "./_shared";

// --------------------------------------------------------------------------
// Company profile + bank details (#284).
//
// Without this route a Cockpit owner can only rename / CVR-sync / archive a
// company — there is no way to record the company's own postal address,
// payment terms or bank account. An invoice then goes out with no payment
// instructions. This route is the third caller of the SAME `setCompanyProfile`
// core function the CLI's `company profile` command uses; the primary bank
// account it creates is the one every issued-invoice payment block reads from.
// --------------------------------------------------------------------------

/**
 * Parses the optional `payment` body field into a core `CompanyPaymentInput`.
 * Every sub-field is optional — `setCompanyProfile` only creates the primary
 * bank account when at least one carries real information.
 */
function parsePaymentInput(raw: unknown): CompanyPaymentInput | undefined {
  if (raw === undefined || raw === null) return undefined;
  if (typeof raw !== "object" || Array.isArray(raw)) {
    throw ApiError.badRequest("'payment' must be an object when present");
  }
  const p = raw as Record<string, unknown>;
  const payment: CompanyPaymentInput = {};
  for (const field of ["bankName", "registrationNo", "accountNo", "iban"] as const) {
    const v = p[field];
    if (v === undefined || v === null) continue;
    if (typeof v !== "string") {
      throw ApiError.badRequest(`'payment.${field}' must be a string when present`);
    }
    const trimmed = v.trim();
    if (trimmed.length > 0) payment[field] = trimmed;
  }
  return payment;
}

/**
 * PATCH /api/companies/:slug/company — updates the editable company profile:
 * the company's own address, CVR, default payment terms and bank/payment
 * details. Body: `{ name?, cvr?, address?, postalCode?, city?,
 * paymentTermsDays?, payment?: {bankName, registrationNo, accountNo, iban} }`.
 *
 * Calls the SAME `setCompanyProfile` core function the CLI uses; the primary
 * bank account it creates feeds every issued-invoice payment block. At least
 * one recognised field must be present, else it is a 400. Goes through
 * `withCompanyMutation` — backup lock, localhost gate, actor attribution. The
 * profile edit is non-destructive (it never touches a posted journal entry),
 * so no `confirm` is required.
 */
export async function handleCompanyProfile(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const name = optionalBodyString(body, "name");
      const cvr = optionalBodyString(body, "cvr");
      const address = optionalBodyString(body, "address");
      const postalCode = optionalBodyString(body, "postalCode");
      const city = optionalBodyString(body, "city");
      const payment = parsePaymentInput(body.payment);

      let paymentTermsDays: number | undefined;
      if (body.paymentTermsDays !== undefined && body.paymentTermsDays !== null) {
        if (
          typeof body.paymentTermsDays !== "number" ||
          !Number.isInteger(body.paymentTermsDays)
        ) {
          throw ApiError.badRequest(
            "'paymentTermsDays' must be an integer when present",
          );
        }
        paymentTermsDays = body.paymentTermsDays;
      }

      // #300: the VAT settlement cadence is editable from the cockpit. An
      // unknown value is a 400 — the column has a CHECK constraint, so a bad
      // string would otherwise fail opaquely.
      const vatPeriodTypeRaw = optionalBodyString(body, "vatPeriodType");
      let vatPeriodType: ReturnType<typeof normalizeVatPeriodType> | undefined;
      if (vatPeriodTypeRaw !== undefined) {
        vatPeriodType = normalizeVatPeriodType(vatPeriodTypeRaw);
        if (vatPeriodType === null) {
          throw ApiError.badRequest(
            "'vatPeriodType' must be 'month', 'quarter' or 'half-year' when present",
          );
        }
      }

      const hasPayment =
        payment !== undefined && Object.keys(payment).length > 0;
      if (
        name === undefined &&
        cvr === undefined &&
        address === undefined &&
        postalCode === undefined &&
        city === undefined &&
        paymentTermsDays === undefined &&
        vatPeriodType === undefined &&
        !hasPayment
      ) {
        throw ApiError.badRequest(
          "angiv mindst et profilfelt for at opdatere " +
            "(name, cvr, address, postalCode, city, paymentTermsDays, vatPeriodType, payment)",
        );
      }

      // #300: the VAT cadence lives on the company row but `setCompanyProfile`
      // does not own it — write it first via the periods-core helper so the
      // settings the response carries reflect the new cadence.
      if (vatPeriodType !== undefined && vatPeriodType !== null) {
        const vatResult = setCompanyVatPeriodType(ctx.db, vatPeriodType);
        if (!vatResult.ok) {
          throw ApiError.badRequest(vatResult.errors[0] ?? "could not set VAT period type");
        }
      }

      const hasProfileField =
        name !== undefined ||
        cvr !== undefined ||
        address !== undefined ||
        postalCode !== undefined ||
        city !== undefined ||
        paymentTermsDays !== undefined ||
        hasPayment;
      const updated = hasProfileField
        ? setCompanyProfile(ctx.db, {
            ...(name !== undefined ? { name } : {}),
            ...(cvr !== undefined ? { cvr } : {}),
            ...(address !== undefined ? { address } : {}),
            ...(postalCode !== undefined ? { postalCode } : {}),
            ...(city !== undefined ? { city } : {}),
            ...(paymentTermsDays !== undefined ? { paymentTermsDays } : {}),
            ...(hasPayment ? { payment } : {}),
          })
        : // Only the VAT cadence changed — re-read the settings so the response
          // shape stays identical to a full profile edit.
          {
            ok: true as const,
            settings: getCompanySettings(ctx.db),
            updatedFields: ["vatPeriodType"],
            errors: [] as string[],
          };
      if (updated.ok && vatPeriodType !== undefined && hasProfileField) {
        updated.updatedFields = [
          ...(updated.updatedFields ?? []),
          "vatPeriodType",
        ];
      }
      // Re-resolve the payment block so the response carries the same
      // `{ ...settings, payment }` shape `GET .../company` returns — the
      // Cockpit form mirrors the persisted state without a re-fetch.
      const paymentDetails = updated.ok
        ? resolveCompanyPaymentDetails(ctx.db, updated.settings?.currency) ?? null
        : null;
      return {
        ok: updated.ok,
        errors: updated.errors,
        settings: updated.settings,
        payment: paymentDetails,
        updatedFields: updated.updatedFields ?? [],
      };
    },
  );

  return okResponse({
    company: {
      ...(result.settings ?? {}),
      payment: result.payment ?? null,
      updatedFields: result.updatedFields ?? [],
    },
  });
}

// --------------------------------------------------------------------------
// Close an accounting period (#287).
//
// A momsangivelse (VAT return) requires a CLOSED `vat_quarter` period — so
// without this route the key recurring legal duty cannot be completed from the
// Cockpit at all. This route is the third caller of the SAME
// `closeAccountingPeriod` core function the CLI's `period close` command uses.
// --------------------------------------------------------------------------

/**
 * POST /api/companies/:slug/periods/close — closes an accounting period.
 *
 * Body: `{ periodStart: string, periodEnd: string, kind?: 'vat_quarter' |
 * 'fiscal_year' | 'custom', reference?: string, confirm: true }`. Calls the
 * SAME `closeAccountingPeriod` core function the CLI uses.
 *
 * Closing a period locks bookkeeping inside it — it changes ledger state — so
 * `requireConfirm` is set. Closing an already-closed period is refused by core
 * ("overlaps existing period"), which `withCompanyMutation` maps to a 409.
 */
export async function handleClosePeriod(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const periodStart = requireBodyString(body, "periodStart");
      const periodEnd = requireBodyString(body, "periodEnd");
      const reference = optionalBodyString(body, "reference");
      const kindRaw = body.kind;
      if (
        kindRaw !== undefined &&
        kindRaw !== "vat_quarter" &&
        kindRaw !== "fiscal_year" &&
        kindRaw !== "custom"
      ) {
        throw ApiError.badRequest(
          "'kind' must be 'vat_quarter', 'fiscal_year' or 'custom' when present",
        );
      }
      const force = body.force === true;
      const closed = closeAccountingPeriod(ctx.db, {
        periodStart,
        periodEnd,
        ...(kindRaw ? { kind: kindRaw } : {}),
        ...(reference ? { reference } : {}),
        force,
        createdBy: ctx.actor.createdBy,
        createdByProgram: ctx.actor.createdByProgram,
      });
      return {
        ok: closed.ok,
        errors: closed.errors,
        periodId: closed.periodId,
        periodStart: closed.periodStart,
        periodEnd: closed.periodEnd,
        kind: closed.kind,
        status: closed.status,
        reference: closed.reference,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    period: {
      id: result.periodId ?? null,
      periodStart: result.periodStart ?? null,
      periodEnd: result.periodEnd ?? null,
      kind: result.kind ?? null,
      status: result.status ?? null,
      reference: result.reference ?? null,
    },
  });
}

// --------------------------------------------------------------------------
// Reopen an accounting period (#301).
//
// The cockpit could close a VAT period but had no way back — an owner who
// closed a period too early (e.g. before the period had even ended) was stuck
// unless they dropped to the CLI's `period reopen`. This route is the third
// caller of the SAME `reopenAccountingPeriod` core function the CLI uses: the
// reopen is a controlled, fully audit-logged, append-only fact — the immutable
// period row is never mutated.
// --------------------------------------------------------------------------

/**
 * POST /api/companies/:slug/periods/reopen — reopens a closed accounting
 * period.
 *
 * Body: `{ periodStart: string, periodEnd: string, kind?: 'vat_quarter' |
 * 'fiscal_year' | 'custom', reason: string, confirm: true }`. The mandatory
 * `reason` is recorded verbatim in the audit log. Calls the SAME
 * `reopenAccountingPeriod` core the CLI's `period reopen` uses, so a `reported`
 * period (already filed) is refused and an already-open period is a no-op —
 * both surface as a 409 via `withCompanyMutation`.
 */
export async function handleReopenPeriod(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const periodStart = requireBodyString(body, "periodStart");
      const periodEnd = requireBodyString(body, "periodEnd");
      const reason = requireBodyString(body, "reason");
      const kindRaw = body.kind;
      if (
        kindRaw !== undefined &&
        kindRaw !== "vat_quarter" &&
        kindRaw !== "fiscal_year" &&
        kindRaw !== "custom"
      ) {
        throw ApiError.badRequest(
          "'kind' must be 'vat_quarter', 'fiscal_year' or 'custom' when present",
        );
      }
      const reopened = reopenAccountingPeriod(ctx.db, {
        periodStart,
        periodEnd,
        ...(kindRaw ? { kind: kindRaw } : {}),
        reason,
        createdBy: ctx.actor.createdBy,
        createdByProgram: ctx.actor.createdByProgram,
      });
      return {
        ok: reopened.ok,
        errors: reopened.errors,
        periodId: reopened.periodId,
        periodStart: reopened.periodStart,
        periodEnd: reopened.periodEnd,
        kind: reopened.kind,
        effectiveStatus: reopened.effectiveStatus,
        reopenedBy: reopened.reopenedBy,
        reason: reopened.reason,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    period: {
      id: result.periodId ?? null,
      periodStart: result.periodStart ?? null,
      periodEnd: result.periodEnd ?? null,
      kind: result.kind ?? null,
      effectiveStatus: result.effectiveStatus ?? null,
      reopenedBy: result.reopenedBy ?? null,
      reason: result.reason ?? null,
    },
  });
}
