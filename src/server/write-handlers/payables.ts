// Leverandørfaktura — payables handlers (#340).
//
// Two write actions: register an ingested purchase document (bilag) as a
// kreditorpost (debit expense + købsmoms, credit 7000 Leverandørgæld) AND
// match an outgoing bank payment against an open payable (debit 7000, credit
// bank). Both go through the SAME `core/payables.ts` functions the CLI's
// `payable register` and `payable pay` commands use, so the cockpit never
// reimplements bookkeeping. Both append journal entries and are therefore
// `requireConfirm: true`.

import {
  registerPayable as corePayableRegister,
  payPayableFromBank as corePayablePayFromBank,
} from "../../core/payables";
import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { withCockpitActor } from "../actor";
import { withCompanyMutation } from "../mutations";
import {
  okResponse,
  optionalBodyNumber,
  optionalBodyPositiveInt,
  optionalBodyString,
  parseIdParam,
  requireBodyPositiveInt,
  requireBodyString,
} from "./_shared";

/**
 * POST /api/companies/:slug/payables — registers an existing purchase
 * document (bilag) as a leverandørfaktura. Body:
 *   { documentId: number, billDate: string, dueDate: string,
 *     expenseAccountNo: string, vatTreatment?: "standard"|"exempt",
 *     vendorId?: number, note?: string, confirm: true }
 *
 * Write-irreversible (it appends a kreditorpost journal entry), so
 * `requireConfirm` is set. A duplicate registration is refused by core and is
 * mapped to a 409 conflict by the shared `withCompanyMutation` heuristic.
 */
export async function handlePayableRegister(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const documentId = requireBodyPositiveInt(body, "documentId");
      const billDate = requireBodyString(body, "billDate");
      const dueDate = requireBodyString(body, "dueDate");
      const expenseAccountNo = requireBodyString(body, "expenseAccountNo");
      const vatTreatmentRaw = optionalBodyString(body, "vatTreatment");
      if (
        vatTreatmentRaw !== undefined &&
        vatTreatmentRaw !== "standard" &&
        vatTreatmentRaw !== "exempt"
      ) {
        throw ApiError.badRequest(
          "'vatTreatment' must be one of: standard, exempt",
        );
      }
      const vendorId = optionalBodyPositiveInt(body, "vendorId");
      const note = optionalBodyString(body, "note");
      const registered = corePayableRegister(
        ctx.db,
        withCockpitActor(
          {
            documentId,
            billDate,
            dueDate,
            expenseAccountNo,
            ...(vatTreatmentRaw
              ? { vatTreatment: vatTreatmentRaw as "standard" | "exempt" }
              : {}),
            ...(vendorId !== undefined ? { vendorId } : {}),
            ...(note ? { note } : {}),
          },
          ctx.actor,
        ),
      );
      return {
        ok: registered.ok,
        errors: registered.errors,
        payableId: registered.payableId,
        documentId: registered.documentId,
        supplierName: registered.supplierName,
        billNo: registered.billNo,
        grossAmount: registered.grossAmount,
        netAmount: registered.netAmount,
        vatAmount: registered.vatAmount,
        dueDate: registered.dueDate,
        entryId: registered.entryId,
        entryNo: registered.entryNo,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    payable: {
      payableId: result.payableId ?? null,
      documentId: result.documentId ?? null,
      supplierName: result.supplierName ?? null,
      billNo: result.billNo ?? null,
      grossAmount: result.grossAmount ?? 0,
      netAmount: result.netAmount ?? 0,
      vatAmount: result.vatAmount ?? 0,
      dueDate: result.dueDate ?? null,
      entryId: result.entryId ?? null,
      entryNo: result.entryNo ?? null,
    },
  });
}

/**
 * POST /api/companies/:slug/payables/:id/pay — applies an outgoing bank
 * payment to an open payable. Body:
 *   { bankTransactionId: number, paymentDate?: string, amount?: number,
 *     paymentAccountNo?: string, note?: string, confirm: true }
 *
 * Write-irreversible (it appends a settlement journal entry + a
 * `payable_payments` row), so `requireConfirm` is set. A double-pay against
 * the same bank line is refused by core (`bank transaction N is already
 * linked …`) and the shared `already` heuristic maps it to a 409.
 */
export async function handlePayablePay(
  config: ServerConfig,
  request: Request,
  slug: string,
  idRaw: string,
): Promise<Response> {
  const payableId = parseIdParam(idRaw, "id");
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const bankTransactionId = requireBodyPositiveInt(
        body,
        "bankTransactionId",
      );
      const paymentDate = optionalBodyString(body, "paymentDate");
      const amount = optionalBodyNumber(body, "amount");
      const paymentAccountNo = optionalBodyString(body, "paymentAccountNo");
      const note = optionalBodyString(body, "note");
      const paid = corePayablePayFromBank(
        ctx.db,
        withCockpitActor(
          {
            payableId,
            bankTransactionId,
            ...(paymentDate ? { paymentDate } : {}),
            ...(amount !== undefined ? { amount } : {}),
            ...(paymentAccountNo ? { paymentAccountNo } : {}),
            ...(note ? { note } : {}),
          },
          ctx.actor,
        ),
      );
      return {
        ok: paid.ok,
        errors: paid.errors,
        paymentId: paid.paymentId,
        journalEntryId: paid.journalEntryId,
        payableId: paid.payableId,
        openBalance: paid.openBalance,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    payment: {
      paymentId: result.paymentId ?? null,
      journalEntryId: result.journalEntryId ?? null,
      payableId: result.payableId ?? payableId,
      openBalance: result.openBalance ?? null,
    },
  });
}
