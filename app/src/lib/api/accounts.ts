import type { AccountsResponse } from "../types";
import { request } from "./_shared";

export const accountsApi = {
  /**
   * #344 — kontoplan (read-only). Bygger på den eksisterende `accounts`-tabel
   * som seedAccounts + reconcileChartOfAccounts populerer.
   */
  accounts: (slug: string) =>
    request<AccountsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/accounts`,
    ).then((r) => r.accounts),
};
