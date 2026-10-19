import { afterEach, expect, test } from "bun:test";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockFetch } from "../test/fixtures";
import { restoreGlobals } from "../test/globals";
import { renderAt } from "../test/render";
import { NativeConsolidationView } from "./NativeConsolidationView";
afterEach(restoreGlobals);
const setup = () =>
	mockFetch({
		"GET /api/companies": {
			companies: [{ slug: "a", name: "Synthetic A", archived: false }],
		},
		"GET /api/companies/a/consolidation": {
			profiles: [],
			availableProfiles: [{ id: "synthetic", currency: "DKK" }],
		},
		"GET /api/group-consolidated-report": {
			scope: "consolidated-report",
			status: "ready",
			period: { from: "2026-01-01", to: "2026-12-31" },
			currency: "DKK",
			blockers: [],
			consolidatedFigures: [
				{
					lineId: "cash",
					label: "Synthetic cash",
					section: "asset",
					rawCompanySum: "90071992547409.93",
					eliminationAdjustment: "-0.01",
					consolidatedAmount: "90071992547409.92",
				},
			],
			sourceSnapshots: [],
		},
	});
test("explicit selected dates/profile render exact server strings without local financial conversion", async () => {
	setup();
	renderAt(<NativeConsolidationView />);
	await screen.findByRole("option", { name: "Synthetic A" });
	await userEvent.selectOptions(
		screen.getByLabelText("Konsolidering virksomhed"),
		"a",
	);
	await userEvent.type(
		screen.getByLabelText("Konsolidering pr. dato"),
		"2026-12-31",
	);
	await screen.findByRole("option", { name: "synthetic · DKK" });
	await userEvent.selectOptions(
		screen.getByLabelText("Konsolidering rapportprofil"),
		"synthetic",
	);
	await userEvent.type(
		screen.getByLabelText("Konsolidering fra dato"),
		"2026-01-01",
	);
	await userEvent.click(
		screen.getByRole("button", { name: "Hent konsolideret rapport" }),
	);
	expect(
		await screen.findByRole("cell", { name: "90071992547409.92" }),
	).toBeInTheDocument();
	expect(
		screen.getByRole("cell", { name: "90071992547409.93" }),
	).toBeInTheDocument();
});
test("profile confirmation is specific to current explicit body and action", async () => {
	setup();
	renderAt(<NativeConsolidationView />);
	await screen.findByRole("option", { name: "Synthetic A" });
	await userEvent.selectOptions(
		screen.getByLabelText("Konsolidering virksomhed"),
		"a",
	);
	await userEvent.click(screen.getByLabelText("Konsolidering JSON"));
	await userEvent.paste('{"profile":{"id":"synthetic"}}');
	const button = screen.getByRole("button", {
		name: "Registrér rapportprofil-handling",
	});
	expect(button).toBeDisabled();
	await userEvent.click(screen.getByLabelText("Bekræft konsolideringsprofil"));
	expect(button).not.toBeDisabled();
	await userEvent.type(screen.getByLabelText("Konsolidering JSON"), " ");
	expect(button).toBeDisabled();
	await userEvent.click(screen.getByLabelText("Bekræft konsolideringsprofil"));
	await userEvent.selectOptions(
		screen.getByLabelText("Konsolidering handling"),
		"approve",
	);
	await waitFor(() => expect(button).toBeDisabled());
	expect(screen.getByLabelText("Konsolidering JSON")).toHaveValue("");
});
