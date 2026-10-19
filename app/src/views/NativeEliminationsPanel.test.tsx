import { afterEach, expect, test } from "bun:test";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { restoreGlobals, stubGlobal } from "../test/globals";
import { renderAt } from "../test/render";
import { NativeEliminationsPanel } from "./NativeEliminationsPanel";
afterEach(restoreGlobals);
const mapping = {
	mapping: { id: "m", leftCompanySlug: "a", rightCompanySlug: "b" },
	mappingHash: "a".repeat(64),
	status: "approved" as const,
	actor: "user:reviewer",
};
const payload = {
	version: 2,
	id: "eliminate",
	kind: "intercompany-balance",
	groupId: "g",
	mappingId: "m",
	mappingHash: "a".repeat(64),
	asOf: "2026-01-01",
	currency: "DKK",
	amountOre: "5678",
	left: {
		companySlug: "a",
		accountNos: ["2400"],
		position: "payable",
		debitOre: "5678",
		creditOre: "0",
	},
	right: {
		companySlug: "b",
		accountNos: ["1400"],
		position: "receivable",
		debitOre: "0",
		creditOre: "5678",
	},
	evidenceRefs: ["synthetic"],
};
function fixture() {
	const writes: Array<{ url: string; body: Record<string, unknown> }> = [];
	stubGlobal("fetch", async (url: unknown, init?: RequestInit) => {
		if (init?.method === "POST") {
			const body = JSON.parse(String(init.body));
			writes.push({ url: String(url), body });
			return new Response(
				JSON.stringify(
					String(url).endsWith("/plan")
						? { plan: { payload, payloadHash: "b".repeat(64) } }
						: { ok: true, status: "proposed" },
				),
				{ headers: { "content-type": "application/json" } },
			);
		}
		return new Response(JSON.stringify({ rows: [] }), {
			headers: { "content-type": "application/json" },
		});
	});
	renderAt(
		<NativeEliminationsPanel
			anchor="a"
			asOf="2026-01-01"
			mappings={[mapping]}
		/>,
	);
	return writes;
}
async function preview() {
	await userEvent.type(screen.getByLabelText("Eliminerings-id"), "eliminate");
	await userEvent.selectOptions(
		screen.getByLabelText("Eliminering mapping"),
		"m",
	);
	await userEvent.type(
		screen.getByLabelText("Eliminering kildereferencer"),
		"synthetic",
	);
	await userEvent.click(
		screen.getByRole("button", { name: "Vis elimineringsplan" }),
	);
	await screen.findByRole("heading", { name: "Gennemgå planen" });
}
test("readonly plan shows actual reverse orientation and confirmed proposal sends exact source hash without caller amount", async () => {
	const writes = fixture();
	await preview();
	expect(writes[0].body).toEqual({
		id: "eliminate",
		mappingId: "m",
		asOf: "2026-01-01",
		evidenceRefs: ["synthetic"],
	});
	expect(
		screen.getByText("a · 2400 · Debet 56.78 / Kredit 0.00 DKK"),
	).toBeInTheDocument();
	const button = screen.getByRole("button", { name: "Foreslå eliminering" });
	expect(button).toBeDisabled();
	await userEvent.click(screen.getByLabelText("Bekræft elimineringsforslag"));
	await userEvent.click(button);
	await screen.findByText(
		"Forslaget er registreret. En anden person skal godkende og anvende det.",
	);
	expect(writes[1].body).toEqual({
		...writes[0].body,
		expectedPayloadHash: "b".repeat(64),
		confirm: true,
	});
	await waitFor(() =>
		expect(
			screen.queryByRole("button", { name: "Foreslå eliminering" }),
		).not.toBeInTheDocument(),
	);
});
test("editing reviewed evidence invalidates plan and prevents stale proposal", async () => {
	fixture();
	await preview();
	await userEvent.click(screen.getByLabelText("Bekræft elimineringsforslag"));
	await userEvent.type(
		screen.getByLabelText("Eliminering kildereferencer"),
		" changed",
	);
	expect(
		screen.queryByRole("button", { name: "Foreslå eliminering" }),
	).not.toBeInTheDocument();
});
