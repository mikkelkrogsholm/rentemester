import { afterEach, expect, test } from "bun:test";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockFetch } from "../test/fixtures";
import { restoreGlobals, stubGlobal } from "../test/globals";
import { renderAt } from "../test/render";
import { NativeIntercompanyView } from "./NativeIntercompanyView";
afterEach(restoreGlobals);
const report = {
	scope: "intercompany-reconciliation",
	asOf: "2026-01-01",
	rows: [
		{
			mappingId: "synthetic-map",
			status: "matched",
			reason: "exact-native-currency-difference",
			difference: "0.00",
			left: {
				companySlug: "a",
				balance: "90071992547409.93",
				currency: "DKK",
				sourceRefs: [],
			},
			right: {
				companySlug: "b",
				balance: "90071992547409.93",
				currency: "DKK",
				sourceRefs: [],
			},
			blockers: [],
		},
	],
};
test("renders exact decimal evidence without unsupported consolidation requests", async () => {
	const urls: string[] = [];
	stubGlobal("fetch", async (url: unknown) => {
		urls.push(String(url));
		return new Response(
			JSON.stringify(
				String(url) === "/api/companies" ? { companies: [] } : report,
			),
			{ headers: { "content-type": "application/json" } },
		);
	});
	renderAt(<NativeIntercompanyView />);
	expect(
		await screen.findByText("Eksakt difference: 0.00"),
	).toBeInTheDocument();
	expect(
		screen.getByText("a: 90071992547409.93 DKK · b: 90071992547409.93 DKK"),
	).toBeInTheDocument();
	expect(
		urls.some(
			(u) => u.includes("eliminations") || u.includes("report-profiles"),
		),
	).toBe(false);
});
test("confirmation clears when reviewed payload changes", async () => {
	mockFetch({
		"GET /api/companies": {
			companies: [{ slug: "a", name: "Synthetic A", archived: false }],
		},
		"GET /api/companies/a/intercompany": {
			manifest: null,
			mappings: [],
			consolidationAvailable: false,
		},
		"GET /api/group-reconciliation": { ...report, rows: [] },
	});
	renderAt(<NativeIntercompanyView />);
	await screen.findByRole("option", { name: "Synthetic A" });
	await userEvent.selectOptions(
		screen.getByLabelText("Intercompany virksomhed"),
		"a",
	);
	await waitFor(() =>
		expect(screen.getByText(/consolidationAvailable/)).toBeInTheDocument(),
	);
	await userEvent.click(screen.getByLabelText("Intercompany JSON"));
	await userEvent.paste('{"version":1,"groups":[]}');
	const submit = screen.getByRole("button", {
		name: "Registrér intercompany-handling",
	});
	expect(submit).toBeDisabled();
	await userEvent.click(screen.getByLabelText("Bekræft intercompany"));
	expect(submit).not.toBeDisabled();
	await userEvent.type(screen.getByLabelText("Intercompany JSON"), " ");
	expect(submit).toBeDisabled();
});
