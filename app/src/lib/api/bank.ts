import type { BankAccountsResponse, BankResponse } from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export const bankApi = {
  /**
   * #345 — Bankkonti + CSV-mapping-profiler (read).
   */
  bankAccounts: (slug: string, options?: ReadRequestOptions) =>
    request<BankAccountsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/bank-accounts`,
      options,
    ).then((r) => r.bankAccounts),

  /**
   * #345 — Opretter en bankkonto.
   */
  createBankAccount: (
    slug: string,
    body: {
      name: string;
      slug?: string;
      bankName?: string;
      registrationNo?: string;
      accountNo?: string;
      iban?: string;
      currency?: string;
      ledgerAccountNo?: string;
    },
  ) =>
    request<{ ok: true; bankAccount: { id: number; slug: string } }>(
      `/api/companies/${encodeURIComponent(slug)}/bank-accounts`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      },
    ).then((r) => r.bankAccount),

  bank: (slug: string, year?: string, options?: ReadRequestOptions) =>
    request<BankResponse>(
      `/api/companies/${encodeURIComponent(slug)}/bank${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
      options,
    ).then((r) => r.bank),

  correctionPlan: (slug:string, bankTransactionId:number, replacementJournalEntryId:number, options?: ReadRequestOptions) =>
    request<{ok:true;plan:unknown}>(`/api/companies/${encodeURIComponent(slug)}/bank/reconciliation-correction-plan?bankTransactionId=${bankTransactionId}&replacementJournalEntryId=${replacementJournalEntryId}`, options).then(r=>r.plan),

  applyCorrection: (slug:string, input:{bankTransactionId:number;replacementJournalEntryId:number;expectedReconciliationId:string;planHash:string;reason:string;idempotencyKey:string}) =>
    request<{ok:true;correction:unknown}>(`/api/companies/${encodeURIComponent(slug)}/bank/reconciliation-correction`,{method:"POST",body:JSON.stringify({...input,confirm:true})}).then(r=>r.correction),

  /**
   * Imports a bank-statement CSV (#213, slice 2). The browser reads the file
   * and passes its text as `csvContent`; the server writes it to a temp file
   * and calls the same core import the CLI/MCP use. Destructive, so the body
   * carries `confirm: true`.
   */
  importBank: (slug: string, input: BankImportInput) =>
    request<{ ok: true; import: BankImportSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/bank/import`,
      {
        method: "POST",
        body: JSON.stringify({
          csvContent: input.csvContent,
          ...(input.account ? { account: input.account } : {}),
          ...(input.profile ? { profile: input.profile } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.import),
};

/** Input for `api.importBank` — the CSV text plus optional account/profile. */
export type BankImportInput = {
  csvContent: string;
  account?: string;
  profile?: string;
};

/** The bank-import result the server echoes back. */
export type BankImportSummary = {
  importBatchId?: string;
  imported: number;
  skippedDuplicates: number;
  skippedDuplicateRows: string[];
  bankAccountSlug?: string;
  profile?: string;
  balanceWarnings: string[];
  exceptionsCreated: number;
};
