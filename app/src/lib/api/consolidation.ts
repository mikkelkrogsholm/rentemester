import { request, type ReadRequestOptions } from "./_shared";
export type ProfileMetadata = {
	id: string;
	groupId: string;
	currency: string;
	validFrom: string;
	validToExclusive?: string;
};
export type ConsolidationCurrent = {
	scope: "native-consolidation";
	asOf: string;
	profiles: Array<{
		profile: Record<string, unknown> & { id: string };
		profileHash: string;
		status: string;
		actor: string;
	}>;
	availableProfiles: ProfileMetadata[];
};
export type ConsolidatedReport = {
	scope: "consolidated-report";
	profileId?: string;
	profileHash?: string;
	groupId?: string;
	currency?: string;
	period: { from: string; to: string };
	status: "ready" | "blocked";
	blockers: string[];
	rawCompanySums: unknown[];
	appliedEliminations: unknown[];
	sourceSnapshots: unknown[];
	consolidatedFigures: null | Array<{
		lineId: string;
		label: string;
		section: string;
		rawCompanySum: string;
		eliminationAdjustment: string;
		consolidatedAmount: string;
	}>;
};
export const consolidationApi = {
	current: (slug: string, asOf: string, options?: ReadRequestOptions) =>
		request<ConsolidationCurrent>(
			`/api/companies/${encodeURIComponent(slug)}/consolidation?asOf=${encodeURIComponent(asOf)}`,
			options,
		),
	report: (
		profileId: string,
		from: string,
		asOf: string,
		options?: ReadRequestOptions,
	) =>
		request<ConsolidatedReport>(
			`/api/group-consolidated-report?${new URLSearchParams({ profileId, from, asOf })}`,
			options,
		),
	mutate: (
		slug: string,
		action: "propose" | "approve" | "revoke",
		body: Record<string, unknown>,
	) =>
		request<{ profileId: string; profileHash?: string; status: string }>(
			`/api/companies/${encodeURIComponent(slug)}/consolidation/${action}`,
			{ method: "POST", body: JSON.stringify({ ...body, confirm: true }) },
		),
};
