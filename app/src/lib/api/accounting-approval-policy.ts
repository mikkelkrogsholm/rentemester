import { request } from "./_shared";

export type AccountingApprovalPolicy = {
  riskClass: "normal" | "elevated";
  reviewMode: "sole_authorized_bookkeeper" | "independent_reviewer";
  version: number;
  eventHash: string;
};

const path = (slug: string) => `/api/companies/${encodeURIComponent(slug)}/accounting-approval-policy`;

export const accountingApprovalPolicyApi = {
  accountingApprovalPolicy: (slug: string) => request<{ ok: true; policy: AccountingApprovalPolicy | null }>(path(slug)).then((result) => result.policy),
  setAccountingApprovalPolicy: (slug: string, reviewMode: AccountingApprovalPolicy["reviewMode"], expectedEventHash: string | null) =>
    request<{ ok: true; policy: AccountingApprovalPolicy }>(path(slug), { method: "POST", body: JSON.stringify({ reviewMode, expectedEventHash, confirm: true }) }).then((result) => result.policy),
};
