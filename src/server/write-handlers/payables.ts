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
  registerPayableInCurrentTransaction as corePayableRegister,
  payPayableFromBankInCurrentTransaction as corePayablePayFromBank,
  payablePayOperationPayload,
} from "../../core/payables";
import type { ServerConfig } from "../config";
import { planDirectBankPurchasePayableCorrection, applyDirectBankPurchasePayableCorrection } from "../../core/direct-bank-purchase-payable-correction";
import { applyLegacyPayablePaymentBackfill, planLegacyPayablePaymentBackfill } from "../../core/legacy-bank-payable-backfill";
import { ApiError } from "../errors";
import { withCockpitActor } from "../actor";
import { withCompanyMutation } from "../mutations";
import { openLedgerReadOnly } from "../../core/ledger-inspection";
import { companyPaths } from "../../core/paths";
import { companyRootForSlug } from "../../core/workspace";
import { readJsonBody } from "../router/_shared";
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
 *     expenseAccountNo: string, vatTreatment?: "standard"|"exempt"|"non_deductible",
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
        vatTreatmentRaw !== "exempt" &&
        vatTreatmentRaw !== "non_deductible"
      ) {
        throw ApiError.badRequest(
          "'vatTreatment' must be one of: standard, exempt, non_deductible",
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
              ? { vatTreatment: vatTreatmentRaw as "standard" | "exempt" | "non_deductible" }
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
    { requireConfirm: true, keyIdempotent: "payable_register" },
  );

  return okResponse({
    // The shared mutation layer owns the durable receipt. Keep it on the
    // public response so an HTTP caller can distinguish original from replay.
    ...("idempotency" in result ? { idempotency: result.idempotency } : {}),
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
    { requireConfirm: true, keyIdempotent: "payable_pay", idempotencyPayload: (body) => payablePayOperationPayload({
      payableId,
      bankTransactionId: Number(body.bankTransactionId),
      amount: typeof body.amount === "number" ? body.amount : undefined,
      paymentDate: typeof body.paymentDate === "string" ? body.paymentDate : undefined,
      paymentAccountNo: typeof body.paymentAccountNo === "string" ? body.paymentAccountNo : undefined,
      note: typeof body.note === "string" ? body.note : undefined,
    }) },
  );

  return okResponse({
    ...("idempotency" in result ? { idempotency: result.idempotency } : {}),
    payment: {
      paymentId: result.paymentId ?? null,
      journalEntryId: result.journalEntryId ?? null,
      payableId: result.payableId ?? payableId,
      openBalance: result.openBalance ?? null,
    },
  });
}

function correctionInput(body: Record<string, unknown>) {
  const vatTreatmentRaw = optionalBodyString(body, "vatTreatment");
  if (vatTreatmentRaw !== undefined && !["standard", "exempt", "non_deductible"].includes(vatTreatmentRaw)) {
    throw ApiError.badRequest("'vatTreatment' must be one of: standard, exempt, non_deductible");
  }
  const vatTreatment = vatTreatmentRaw as "standard" | "exempt" | "non_deductible" | undefined;
  return {
    documentId: requireBodyPositiveInt(body, "documentId"),
    bankTransactionId: requireBodyPositiveInt(body, "bankTransactionId"),
    billDate: requireBodyString(body, "billDate"), dueDate: requireBodyString(body, "dueDate"),
    expenseAccountNo: requireBodyString(body, "expenseAccountNo"), vatTreatment,
    vendorId: optionalBodyPositiveInt(body, "vendorId"), note: optionalBodyString(body, "note"),
  };
}

export async function handleDirectBankPurchasePayablePlan(config: ServerConfig, request: Request, slug: string): Promise<Response> {
  const body = await readJsonBody(request);
  const db = openLedgerReadOnly(companyPaths(companyRootForSlug(config.workspaceRoot, slug)).db);
  try {
    const result = planDirectBankPurchasePayableCorrection(db, correctionInput(body));
    if (!result.ok) throw ApiError.conflict(result.errors.join("; "));
    return okResponse({ plan: result.plan });
  } finally {
    db.close();
  }
}

export async function handleDirectBankPurchasePayableApply(config: ServerConfig, request: Request, slug: string): Promise<Response> {
  const result = await withCompanyMutation(request, config, slug, (ctx, body) => {
    const stable = ctx.principal.via === "service-principal"
      ? (ctx.principal.serviceAccountId ? { kind: "service-account" as const, subjectId: ctx.principal.serviceAccountId } : undefined)
      : (ctx.principal.userId ? { kind: "user" as const, subjectId: ctx.principal.userId } : undefined);
    return applyDirectBankPurchasePayableCorrection(ctx.db, { ...correctionInput(body), planHash: requireBodyString(body,"planHash"), reason:requireBodyString(body,"reason"), actor:ctx.actor.createdBy, principal:stable, confirm:true });
  }, { requireConfirm:true, keyIdempotent:"direct_bank_purchase_payable_correction_apply", requireIdempotencyKey:true });
  return okResponse({ correction: result, ...("idempotency" in result ? { idempotency: result.idempotency } : {}) });
}

function legacyBackfillInput(body:Record<string,unknown>){return {purchaseJournalEntryId:requireBodyPositiveInt(body,"purchaseJournalEntryId"),paymentJournalEntryId:requireBodyPositiveInt(body,"paymentJournalEntryId"),documentId:requireBodyPositiveInt(body,"documentId"),bankTransactionId:requireBodyPositiveInt(body,"bankTransactionId")};}
export async function handleLegacyPayableBackfillPlan(config:ServerConfig,request:Request,slug:string):Promise<Response>{const body=await readJsonBody(request);const db=openLedgerReadOnly(companyPaths(companyRootForSlug(config.workspaceRoot,slug)).db);try{const result=planLegacyPayablePaymentBackfill(db,legacyBackfillInput(body));if(!result.ok)throw ApiError.conflict(result.errors.join("; "));return okResponse({plan:result.plan});}finally{db.close();}}
export async function handleLegacyPayableBackfillApply(config:ServerConfig,request:Request,slug:string):Promise<Response>{const result=await withCompanyMutation(request,config,slug,(ctx,body)=>{const p=ctx.principal;return applyLegacyPayablePaymentBackfill(ctx.db,{...legacyBackfillInput(body),planHash:requireBodyString(body,"planHash"),idempotencyKey:requireBodyString(body,"idempotencyKey"),actor:ctx.actor.createdBy,principal:p.serviceAccountId?{kind:"service-account" as const,subjectId:p.serviceAccountId}:{kind:"user" as const,subjectId:p.userId??p.id},confirm:true});},{requireConfirm:true,keyIdempotent:"payable_legacy_backfill_apply",requireIdempotencyKey:true});return okResponse({backfill:result,...("idempotency" in result?{idempotency:result.idempotency}:{})});}
