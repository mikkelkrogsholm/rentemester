import { describe, expect, test } from "bun:test";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { renderAt } from "../test/render";
import { mockFetch } from "../test/fixtures";
import {
	RegisterAccrualModal,
	RecognizeAccrualModal,
	registrationPreview,
} from "./AccrualModal";
import type { AccrualRegisterInput } from "../lib/types";
const input: AccrualRegisterInput = {
	accrualType: "prepaid_expense",
	description: "Synthetic service",
	totalAmount: 1000,
	recognitionPeriods: 3,
	periodStepMonths: 1,
	firstRecognitionDate: "2024-01-31",
	registrationDate: "2024-01-01",
	balanceAccountNo: "P",
	resultAccountNo: "E",
	settlementAccountNo: "B",
	documentId: 1,
};
describe("accrual confirmation", () => {
	test("preview anchors month ends and exact final remainder across leap and low years", () => {
		expect(registrationPreview(input)).toEqual([
			{ periodIndex: 1, recognitionDate: "2024-01-31", amount: 333.33 },
			{ periodIndex: 2, recognitionDate: "2024-02-29", amount: 333.33 },
			{ periodIndex: 3, recognitionDate: "2024-03-31", amount: 333.34 },
		]);
		expect(
			registrationPreview({ ...input, firstRecognitionDate: "0004-01-31" })[1]
				?.recognitionDate,
		).toBe("0004-02-29");
		expect(() =>
			registrationPreview({ ...input, totalAmount: 0.01 }),
		).toThrow();
		expect(() =>
			registrationPreview({ ...input, firstRecognitionDate: "9999-12-31" }),
		).toThrow();
	});
	test("explicit accounts and original evidence are reviewed before any write", async () => {
		mockFetch({
			"GET /api/companies/synthetic/accounts": {
				accounts: {
					accounts: [
						{
							accountNo: "P",
							name: "Prepaid",
							type: "asset",
							normalBalance: "debit",
						},
						{
							accountNo: "B",
							name: "Bank",
							type: "asset",
							normalBalance: "debit",
						},
						{
							accountNo: "E",
							name: "Service",
							type: "expense",
							normalBalance: "debit",
						},
					],
				},
			},
			"GET /api/companies/synthetic/documents": {
				documents: {
					documents: [
						{ id: 1, documentNo: "DOC-1", filename: "synthetic.txt" },
					],
				},
			},
			"POST /api/companies/synthetic/accruals": { accrual: { accrualId: 1 } },
		});
		let saved = 0;
		renderAt(
			<RegisterAccrualModal
				slug="synthetic"
				onClose={() => {}}
				onSaved={() => saved++}
			/>,
			{
				route: "/companies/synthetic/periodisering",
				path: "/companies/:slug/periodisering",
			},
		);
		await screen.findByLabelText("Beskrivelse");
		for (const [label, value] of Object.entries({
			Beskrivelse: "Synthetic service",
			"Samlet beløb (kr.)": "1000,00",
			"Antal perioder": "3",
			Registreringsdato: "2024-01-01",
			"Første periodedato": "2024-01-31",
			Balancekonto: "P",
			Resultatkonto: "E",
			Betalingskonto: "B",
			"Originalt bilag": "1",
		}))
			fireEvent.change(screen.getByLabelText(label), { target: { value } });
		fireEvent.click(
			screen.getByRole("button", { name: "Gennemse periodisering" }),
		);
		await screen.findByText("2024-02-29");
		expect(saved).toBe(0);
		expect(
			(fetch as any).mock.calls.filter((c: any[]) => c[1]?.method === "POST"),
		).toHaveLength(0);
		fireEvent.click(
			screen.getByRole("button", { name: "Registrér og bogfør" }),
		);
		await waitFor(() => expect(saved).toBe(1));
		const call = (fetch as any).mock.calls.find(
			(c: any[]) => c[1]?.method === "POST",
		);
		expect(JSON.parse(call[1].body)).toMatchObject({ ...input, confirm: true });
	});
});

test("recognition reviews exact server period and date before confirmation", async () => {
	mockFetch({
		"GET /api/companies/synthetic/accruals/7/schedule": {
			accrualSchedule: {
				accrualId: 7,
				documentId: 4,
				accrualType: "prepaid_expense",
				schedule: [
					{
						periodIndex: 1,
						recognitionDate: "2024-01-31",
						amount: 333.33,
						recognized: false,
					},
				],
			},
		},
		"POST /api/companies/synthetic/accruals/7/recognize": {
			recognition: { entryId: 12, periodAmount: 333.33 },
		},
	});
	let saved = 0;
	renderAt(
		<RecognizeAccrualModal
			slug="synthetic"
			row={
				{
					accrualId: 7,
					description: "Synthetic service",
					accrualType: "prepaid_expense",
				} as any
			}
			onClose={() => {}}
			onSaved={() => saved++}
		/>,
		{
			route: "/companies/synthetic/periodisering",
			path: "/companies/:slug/periodisering",
		},
	);
	await screen.findByLabelText("Periode");
	fireEvent.change(screen.getByLabelText("Periode"), {
		target: { value: "1" },
	});
	expect(
		(screen.getByLabelText("Bogføringsdato") as HTMLInputElement).value,
	).toBe("2024-01-31");
	fireEvent.click(screen.getByRole("button", { name: "Gennemse bogføring" }));
	await screen.findByText(/originalt bilag 4/);
	expect(saved).toBe(0);
	expect(
		(fetch as any).mock.calls.filter((c: any[]) => c[1]?.method === "POST"),
	).toHaveLength(0);
	fireEvent.click(
		screen.getByRole("button", { name: "Bekræft og bogfør periode" }),
	);
	await waitFor(() => expect(saved).toBe(1));
	const call = (fetch as any).mock.calls.find(
		(c: any[]) => c[1]?.method === "POST",
	);
	expect(JSON.parse(call[1].body)).toEqual({
		periodIndex: 1,
		transactionDate: "2024-01-31",
		confirm: true,
	});
});
