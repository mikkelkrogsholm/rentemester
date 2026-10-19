import { request, type ReadRequestOptions } from "./_shared";
export type EliminationInput = {
	id: string;
	mappingId: string;
	asOf: string;
	evidenceRefs: string[];
};
export type EliminationSide = {
	companySlug: string;
	accountNos: string[];
	selectionHash: string;
	ledgerHeadHash: string | null;
	entryCount: number;
	creditOre?: string;
	debitOre?: string;
	position?: "receivable" | "payable";
};
export type BalanceEliminationPayload = {
	version: 1 | 2;
	id: string;
	kind: "intercompany-balance";
	groupId: string;
	mappingId: string;
	mappingHash: string;
	asOf: string;
	currency: string;
	amountOre: string;
	left: EliminationSide;
	right: EliminationSide;
	evidenceRefs: string[];
};
export type EliminationState = {
	payload: BalanceEliminationPayload;
	payloadHash: string;
	eventHash: string;
	status: "proposed" | "approved" | "rejected" | "applied" | "reversed";
	actor: string;
};
export const eliminationsApi = {
	current: (anchor: string, options?: ReadRequestOptions) =>
		request<{ rows: EliminationState[] }>(
			`/api/companies/${encodeURIComponent(anchor)}/eliminations`,
			options,
		),
	plan: (anchor: string, input: EliminationInput) =>
		request<{
			plan: { payload: BalanceEliminationPayload; payloadHash: string };
		}>(`/api/companies/${encodeURIComponent(anchor)}/eliminations/plan`, {
			method: "POST",
			body: JSON.stringify(input),
			readOnly: true,
		}),
	propose: (
		anchor: string,
		input: EliminationInput,
		expectedPayloadHash: string,
	) =>
		request<unknown>(
			`/api/companies/${encodeURIComponent(anchor)}/eliminations/propose`,
			{
				method: "POST",
				body: JSON.stringify({ ...input, expectedPayloadHash, confirm: true }),
			},
		),
	act: (
		anchor: string,
		action: "approve" | "reject" | "apply" | "reverse",
		state: EliminationState,
	) =>
		request<unknown>(
			`/api/companies/${encodeURIComponent(anchor)}/eliminations/${action}`,
			{
				method: "POST",
				body: JSON.stringify({
					eliminationId: state.payload.id,
					payloadHash: state.payloadHash,
					confirm: true,
				}),
			},
		),
};
