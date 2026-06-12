import type {
  PayableListStatusFilter,
  PayablePayInput,
  PayablePaySummary,
  PayableRegisterInput,
  PayableRegisterSummary,
  PayablesResponse,
} from "../types";
import { request } from "./_shared";

// --- Leverandørfaktura (payables) — #340 --------------------------------

export const payablesApi = {
  /**
   * #340 — Leverandørfaktura-arbejdsbordet. Returns the kreditorliste from
   * `core/payables.ts#buildPayablesList` plus the modal picker rows
   * (unregistered purchase documents, expense accounts, vendors).
   */
  payables: (slug: string, status?: PayableListStatusFilter, asOf?: string) => {
    const params = new URLSearchParams();
    if (status) params.set("status", status);
    if (asOf) params.set("asOf", asOf);
    const qs = params.toString();
    return request<PayablesResponse>(
      `/api/companies/${encodeURIComponent(slug)}/payables${qs ? `?${qs}` : ""}`,
    ).then((r) => r.payables);
  },

  /**
   * #340 — registers an existing ingested purchase document as a
   * leverandørfaktura. Write-irreversible (it appends a journal entry), so
   * the body carries `confirm: true`.
   */
  registerPayable: (slug: string, input: PayableRegisterInput) =>
    request<{ ok: true; payable: PayableRegisterSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/payables`,
      {
        method: "POST",
        body: JSON.stringify({
          documentId: input.documentId,
          billDate: input.billDate,
          dueDate: input.dueDate,
          expenseAccountNo: input.expenseAccountNo,
          ...(input.vatTreatment ? { vatTreatment: input.vatTreatment } : {}),
          ...(input.vendorId !== undefined ? { vendorId: input.vendorId } : {}),
          ...(input.note ? { note: input.note } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.payable),

  /**
   * #340 — applies an outgoing bank payment to an open leverandørfaktura.
   * Write-irreversible (it appends a settlement journal entry + a
   * `payable_payments` row), so the body carries `confirm: true`.
   */
  payPayable: (slug: string, input: PayablePayInput) =>
    request<{ ok: true; payment: PayablePaySummary }>(
      `/api/companies/${encodeURIComponent(slug)}/payables/${input.payableId}/pay`,
      {
        method: "POST",
        body: JSON.stringify({
          bankTransactionId: input.bankTransactionId,
          ...(input.paymentDate ? { paymentDate: input.paymentDate } : {}),
          ...(input.amount !== undefined ? { amount: input.amount } : {}),
          ...(input.paymentAccountNo
            ? { paymentAccountNo: input.paymentAccountNo }
            : {}),
          ...(input.note ? { note: input.note } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.payment),
};
