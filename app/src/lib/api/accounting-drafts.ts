import type { AccountingDraft, AccountingDraftPayload } from "../types";
import { request } from "./_shared";

function path(slug: string, suffix = ""): string {
  return `/api/companies/${encodeURIComponent(slug)}/accounting-drafts${suffix}`;
}

export const accountingDraftsApi = {
  accountingDrafts: (slug: string) =>
    request<{ ok: true; accountingDrafts: AccountingDraft[] }>(path(slug))
      .then((response) => response.accountingDrafts),

  createAccountingDraft: (slug: string, draftId: string, payload: AccountingDraftPayload) =>
    request<{ ok: true; accountingDraft: AccountingDraft }>(path(slug), {
      method: "POST",
      body: JSON.stringify({ draftId, payload }),
    }).then((response) => response.accountingDraft),

  reviseAccountingDraft: (slug: string, draft: AccountingDraft, payload: AccountingDraftPayload) =>
    request<{ ok: true; accountingDraft: AccountingDraft }>(path(slug, `/${encodeURIComponent(draft.id)}/revise`), {
      method: "POST",
      body: JSON.stringify({ expectedEventHash: draft.eventHash, payload }),
    }).then((response) => response.accountingDraft),

  submitAccountingDraft: (slug: string, draft: AccountingDraft) =>
    request<{ ok: true; accountingDraft: AccountingDraft }>(path(slug, `/${encodeURIComponent(draft.id)}/submit`), {
      method: "POST",
      body: JSON.stringify({ expectedEventHash: draft.eventHash }),
    }).then((response) => response.accountingDraft),

  rejectAccountingDraft: (slug: string, draft: AccountingDraft, reason: string, expectedPolicyEventHash?: string) =>
    request<{ ok: true; accountingDraft: AccountingDraft }>(path(slug, `/${encodeURIComponent(draft.id)}/reject`), {
      method: "POST",
      body: JSON.stringify({ expectedEventHash: draft.eventHash, reason, ...(expectedPolicyEventHash ? { expectedPolicyEventHash } : {}) }),
    }).then((response) => response.accountingDraft),

  approveAndPostAccountingDraft: (slug: string, draft: AccountingDraft, expectedPolicyEventHash?: string) =>
    request<{ ok: true; accountingDraft: AccountingDraft }>(path(slug, `/${encodeURIComponent(draft.id)}/approve-and-post`), {
      method: "POST",
      body: JSON.stringify({ expectedEventHash: draft.eventHash, confirm: true, ...(expectedPolicyEventHash ? { expectedPolicyEventHash } : {}) }),
    }).then((response) => response.accountingDraft),
};
