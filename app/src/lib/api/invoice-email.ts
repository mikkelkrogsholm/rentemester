import { ApiError, request, type ReadRequestOptions } from "./_shared";
export type InvoiceEmailRow = {
  invoiceDocumentId: number; invoiceNumber: string; kind: "invoice" | "reminder";
  recipient: string; subject: string; messageId: string; bodySha256: string; pdfSha256: string | null;
  state: "accepted" | "refused" | "unknown"; stateHash: string; attemptId: string | null;
  history: unknown[]; duplicate?: boolean; legacy?: boolean; humanResolution?: unknown;
};
export type InvoiceEmailStatus = { configured: boolean; rows: InvoiceEmailRow[] };
export type InvoiceEmailSend = { invoiceDocumentId: number; to: string; kind: "invoice" | "reminder"; confirm: true };
export type InvoiceEmailRetry = { messageId: string; expectedStateHash: string; reason: string; confirm: true };
export type InvoiceEmailReconcile = InvoiceEmailRetry & { resolution: "accepted" | "not_accepted"; evidence: string };
export function isInvoiceEmailRow(value: unknown): value is InvoiceEmailRow {
  if (!value || typeof value !== "object") return false;
  const v = value as Record<string, unknown>;
  return Number.isSafeInteger(v.invoiceDocumentId) && Number(v.invoiceDocumentId) > 0
    && ["invoiceNumber", "recipient", "subject", "messageId", "bodySha256", "stateHash"].every(k => typeof v[k] === "string")
    && ["bodySha256", "stateHash"].every(k => /^[a-f0-9]{64}$/.test(String(v[k])))
    && /^<[a-f0-9]{32}@rentemester\.local>$/.test(String(v.messageId))
    && ((typeof v.pdfSha256 === "string" && /^[a-f0-9]{64}$/.test(v.pdfSha256)) || (v.pdfSha256 === null && v.state === "accepted"))
    && ["invoice", "reminder"].includes(String(v.kind)) && ["accepted", "refused", "unknown"].includes(String(v.state)) && Array.isArray(v.history);
}
function checked(row: unknown): InvoiceEmailRow {
  if (!isInvoiceEmailRow(row)) throw new ApiError("internal", "Serverens leveringskvittering er ugyldig. Kontrollér status; gentag ikke afsendelsen.", 200);
  return row;
}
const base = (slug: string) => `/api/companies/${encodeURIComponent(slug)}/invoices`;
export const invoiceEmailApi = {
  nativeInvoiceEmailStatus: (slug: string, options?: ReadRequestOptions) =>
    request<{ ok: true; invoiceEmail: InvoiceEmailStatus }>(`${base(slug)}/email-deliveries`, options).then(r => {
      if (r.ok !== true || typeof r.invoiceEmail?.configured !== "boolean" || !Array.isArray(r.invoiceEmail.rows) || !r.invoiceEmail.rows.every(isInvoiceEmailRow)) throw new ApiError("internal", "Leveringsstatus kunne ikke bekræftes.", 200);
      return r.invoiceEmail;
    }),
  nativeInvoiceEmailSend: (slug: string, body: InvoiceEmailSend) =>
    request<{ ok: true; delivery: InvoiceEmailRow }>(`${base(slug)}/send-email`, { method: "POST", body: JSON.stringify(body) }).then(r => checked(r.ok === true ? r.delivery : undefined)),
  nativeInvoiceEmailRetry: (slug: string, body: InvoiceEmailRetry) =>
    request<{ ok: true; delivery: InvoiceEmailRow }>(`${base(slug)}/email-deliveries/retry`, { method: "POST", body: JSON.stringify(body) }).then(r => checked(r.ok === true ? r.delivery : undefined)),
  nativeInvoiceEmailReconcile: (slug: string, body: InvoiceEmailReconcile) =>
    request<{ ok: true; delivery: InvoiceEmailRow }>(`${base(slug)}/email-deliveries/reconcile`, { method: "POST", body: JSON.stringify(body) }).then(r => checked(r.ok === true ? r.delivery : undefined)),
};
