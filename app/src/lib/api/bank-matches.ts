import type {BankMatchSuggestions} from "../types/bank-matches";
import {type ReadRequestOptions, request} from "./_shared";

export const bankMatchesApi = {
  bankSuggestMatches: (slug: string, bankTransactionId?: number, max?: number, options?: ReadRequestOptions) => {
    const query = new URLSearchParams();
    if (bankTransactionId !== undefined) query.set("bankTransactionId", String(bankTransactionId));
    if (max !== undefined) query.set("max", String(max));
    return request<BankMatchSuggestions>(`/api/companies/${encodeURIComponent(slug)}/bank/match-suggestions${query.size ? `?${query}` : ""}`, options);
  },
};
