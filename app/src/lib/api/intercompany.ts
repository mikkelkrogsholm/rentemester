import { request, type ReadRequestOptions } from "./_shared";
export type MappingState = {
	mapping: {
		id: string;
		leftCompanySlug: string;
		rightCompanySlug: string;
		[key: string]: unknown;
	};
	mappingHash: string;
	status: "proposed" | "approved" | "revoked";
	actor: string;
};
export type IntercompanyCurrent = {
	manifest: unknown | null;
	manifestHash: string | null;
	mappings: MappingState[];
	consolidationAvailable: false;
};
type Side = {
	companySlug: string;
	currency: string;
	balance: string;
	sourceRefs: unknown[];
	sourceSnapshot: unknown;
};
export type NativeReconciliation = {
	scope: "intercompany-reconciliation";
	asOf: string;
	rows: Array<{
		mappingId: string;
		mappingHash?: string;
		status: string;
		reason: string;
		difference?: string;
		left?: Side;
		right?: Side;
		blockers: string[];
	}>;
};
export const intercompanyApi = {
	current: (slug: string, options?: ReadRequestOptions) =>
		request<IntercompanyCurrent>(
			`/api/companies/${encodeURIComponent(slug)}/intercompany`,
			options,
		),
	reconciliation: (asOf: string, options?: ReadRequestOptions) =>
		request<NativeReconciliation>(
			`/api/group-reconciliation?asOf=${encodeURIComponent(asOf)}`,
			options,
		),
	mutate: (
		slug: string,
		action: "manifest" | "propose" | "approve" | "revoke",
		body: Record<string, unknown>,
	) =>
		request<unknown>(
			`/api/companies/${encodeURIComponent(slug)}/intercompany/${action}`,
			{ method: "POST", body: JSON.stringify({ ...body, confirm: true }) },
		),
};
