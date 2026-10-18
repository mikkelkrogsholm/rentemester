import type { AccountRole, AccountRoleResolution, AccountRow, AccountsResponse } from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export const accountsApi = {
  createAccount: (slug: string, input: Pick<AccountRow, "accountNo" | "name" | "type" | "normalBalance" | "defaultVatCode">) =>
    request<{ ok: true; account: { accountNo: string } }>(`/api/companies/${encodeURIComponent(slug)}/accounts`, {
      method: "POST", body: JSON.stringify(input),
    }).then(r => r.account),
  confirmAccountRole: (slug: string, role: AccountRole, accountNo: string) =>
    request<{ ok: true; resolution: AccountRoleResolution }>(`/api/companies/${encodeURIComponent(slug)}/account-roles`, {
      method: "POST", body: JSON.stringify({ role, accountNo, confirm: true }),
    }).then(r => r.resolution),
  /**
   * #344 — kontoplan (read-only). Bygger på den eksisterende `accounts`-tabel
   * som seedAccounts + reconcileChartOfAccounts populerer.
   */
  accounts: (slug: string, options?: ReadRequestOptions) =>
    request<AccountsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/accounts`, options,
    ).then((r) => r.accounts),
};
