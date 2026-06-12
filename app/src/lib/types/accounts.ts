// #344 — Kontoplan-view (read-only).

export type AccountRow = {
  accountNo: string;
  name: string;
  type: string;
  normalBalance: string;
  defaultVatCode: string | null;
  hasPostings: boolean;
};

export type CompanyAccounts = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  accounts: AccountRow[];
  byType: Record<string, number>;
};

export type AccountsResponse = {
  ok: true;
  accounts: CompanyAccounts;
};
