import type { AccountsResponse } from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export const accountsApi = {
  /**
   * #344 — kontoplan (read-only). Bygger på den eksisterende `accounts`-tabel
   * som seedAccounts + reconcileChartOfAccounts populerer.
   */
  accounts: (slug: string, options?: ReadRequestOptions) =>
    request<AccountsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/accounts`, options,
    ).then((r) => r.accounts),
};
