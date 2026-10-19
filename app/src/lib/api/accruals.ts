import type {
	AccrualRegisterInput,
	AccrualSchedule,
	CompanyAccrualsResponse,
} from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export const accrualsApi = {
	registerAccrual: (slug: string, input: AccrualRegisterInput) =>
		request<{ ok: true; accrual: { accrualId: number } }>(
			`/api/companies/${encodeURIComponent(slug)}/accruals`,
			{ method: "POST", body: JSON.stringify({ ...input, confirm: true }) },
		).then((r) => r.accrual),
	accrualSchedule: (slug: string, id: number, options?: ReadRequestOptions) =>
		request<{ ok: true; accrualSchedule: AccrualSchedule }>(
			`/api/companies/${encodeURIComponent(slug)}/accruals/${id}/schedule`,
			options,
		).then((r) => r.accrualSchedule),
	recognizeAccrual: (
		slug: string,
		id: number,
		input: {
			periodIndex: number;
			transactionDate: string;
			settlementAccountNo?: string;
		},
	) =>
		request<{
			ok: true;
			recognition: { entryId: number; periodAmount: number };
		}>(`/api/companies/${encodeURIComponent(slug)}/accruals/${id}/recognize`, {
			method: "POST",
			body: JSON.stringify({ ...input, confirm: true }),
		}).then((r) => r.recognition),
	/**
	 * #337 — Periodiseringsregister (read).
	 */
	accruals: (slug: string, options?: ReadRequestOptions) =>
		request<CompanyAccrualsResponse>(
			`/api/companies/${encodeURIComponent(slug)}/accruals`,
			options,
		).then((r) => r.accruals),
};
