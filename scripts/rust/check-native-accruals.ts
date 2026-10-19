import type { Page } from "@playwright/test";
import { readFileSync, writeFileSync } from "node:fs";
const saved = "/tmp/rentemester-rust-reports/native-accruals.json";
export async function checkNativeAccruals(
	page: Page,
	base: string,
	restarted = false,
) {
	const url = `${base}/api/companies/synthetic`,
		headers = { origin: base, "content-type": "application/json" };
	let evidence: { accrualId: number; documentId: number; entryId: number };
	if (!restarted) {
		await page.goto(`${base}/companies/synthetic/kontoplan`);
		for (const [no, name, type, balance] of [
			["1310", "Synthetic prepaid services", "asset", "debit"],
			["3910", "Synthetic recognized service expense", "expense", "debit"],
		]) {
			const form = page.getByRole("form", { name: "Opret konto", exact: true });
			await form.getByLabel("Kontonummer", { exact: true }).fill(no);
			await form.getByLabel("Kontonavn", { exact: true }).fill(name);
			await form.getByLabel(/^Kontotype/).selectOption(type);
			await form.getByLabel(/^Normal saldo/).selectOption(balance);
			await form
				.getByRole("button", { name: "Opret konto", exact: true })
				.click();
			await page.getByRole("row").filter({ hasText: name }).waitFor();
		}
		await page.goto(`${base}/companies/synthetic/bilag`);
		await page
			.getByRole("button", { name: "Indlæs bilag", exact: true })
			.click();
		const upload = page.getByRole("dialog");
		const today = new Date().toLocaleDateString("sv-SE");
		await upload.getByLabel("Bilagsfil", { exact: true }).setInputFiles({
			name: "synthetic-accrual.txt",
			mimeType: "text/plain",
			buffer: Buffer.from(
				"Synthetic service accrual evidence; DKK1000, no deductible VAT\n",
			),
		});
		await upload
			.getByLabel("Afsender", { exact: true })
			.fill("Synthetic Service Supplier");
		await upload
			.getByLabel("Afsender CVR/moms", { exact: true })
			.fill("DK12345678");
		await upload
			.getByLabel("Afsenderadresse", { exact: true })
			.fill("Synthetic Supplier 2");
		await upload.getByLabel("Leverandørland (ISO)", { exact: true }).fill("DK");
		await upload.getByLabel(/^Identitetstype/).selectOption("dk_cvr");
		for (const [label, value] of Object.entries({
			Bilagsdato: today,
			"Fakturanr. (valgfri)": "SYN-ACCRUAL-1",
			"Beløb inkl. moms": "1000,00",
			Momsbeløb: "0,00",
			"Beskrivelse af leverance": "Synthetic service",
			Modtager: "Synthetic Company",
			"Modtager CVR/moms": "DK87654321",
			Modtageradresse: "Synthetic Street 1",
		}))
			await upload.getByLabel(label, { exact: true }).fill(value);
		await upload
			.getByRole("button", { name: "Indlæs bilag", exact: true })
			.click();
		await upload.getByText(/Bilaget er indlæst som DOC-/).waitFor();
		await upload.getByRole("button", { name: "Luk", exact: true }).click();
		const docs = await (await page.request.get(`${url}/documents`)).json();
		const document = docs.documents.documents.find(
			(d: { filename: string }) => d.filename === "synthetic-accrual.txt",
		);
		if (!document) throw new Error("Asset purchase original missing");

		await page.goto(`${base}/companies/synthetic/periodisering`);
		await page
			.getByRole("button", { name: "Ny periodisering", exact: true })
			.click();
		const dialog = page.getByRole("dialog");
		await dialog
			.getByLabel("Beskrivelse", { exact: true })
			.fill("Synthetic prepaid service");
		await dialog
			.getByLabel("Samlet beløb (kr.)", { exact: true })
			.fill("1000,00");
		await dialog.getByLabel("Antal perioder", { exact: true }).fill("3");
		// Historical anchored month end makes every synthetic period due regardless of run date.
		await dialog
			.getByLabel("Registreringsdato", { exact: true })
			.fill("2026-01-01");
		await dialog
			.getByLabel("Første periodedato", { exact: true })
			.fill("2026-01-31");
		await dialog
			.getByLabel("Balancekonto", { exact: true })
			.selectOption("1310");
		await dialog
			.getByLabel("Resultatkonto", { exact: true })
			.selectOption("3910");
		await dialog
			.getByLabel("Betalingskonto", { exact: true })
			.selectOption("1800");
		await dialog
			.getByLabel("Originalt bilag", { exact: true })
			.selectOption(String(document.id));
		await dialog
			.getByRole("button", { name: "Gennemse periodisering", exact: true })
			.click();
		await dialog.getByText("2026-02-28", { exact: true }).waitFor();
		await dialog.getByText(/333,34/).waitFor();
		const [registered] = await Promise.all([
			page.waitForResponse(
				(r) => r.url().endsWith("/accruals") && r.request().method() === "POST",
			),
			dialog
				.getByRole("button", { name: "Registrér og bogfør", exact: true })
				.click(),
		]);
		if (!registered.ok())
			throw new Error(
				`Accrual registration failed: ${await registered.text()}`,
			);
		const created = (await registered.json()).accrual;
		await dialog.waitFor({ state: "hidden" });
		evidence = {
			accrualId: created.accrualId,
			entryId: created.entryId,
			documentId: document.id,
		};
		const row = page
			.getByRole("row")
			.filter({ hasText: "Synthetic prepaid service" });
		for (let period = 1; period <= 3; period++) {
			await row
				.getByRole("button", {
					name: "Bogfør periode for Synthetic prepaid service",
					exact: true,
				})
				.click();
			const confirmation = page.getByRole("dialog");
			await confirmation
				.getByLabel("Periode", { exact: true })
				.selectOption(String(period));
			await confirmation
				.getByRole("button", { name: "Gennemse bogføring", exact: true })
				.click();
			const [posted] = await Promise.all([
				page.waitForResponse(
					(r) =>
						r.url().endsWith("/recognize") && r.request().method() === "POST",
				),
				confirmation
					.getByRole("button", {
						name: "Bekræft og bogfør periode",
						exact: true,
					})
					.click(),
			]);
			if (!posted.ok())
				throw new Error(`Accrual recognition failed: ${await posted.text()}`);
			if (
				(await posted.json()).recognition.periodAmount !==
				(period === 3 ? 333.34 : 333.33)
			)
				throw new Error("Recognition rounding mismatch");
			await confirmation.waitFor({ state: "hidden" });
			await row.getByText(`${period}/3 ×`, { exact: false }).waitFor();
		}
		const replay = await page.request.post(
			`${url}/accruals/${evidence.accrualId}/recognize`,
			{
				headers,
				data: { confirm: true, periodIndex: 1, transactionDate: today },
			},
		);
		if (replay.status() !== 422)
			throw new Error("Duplicate accrual period accepted");
		writeFileSync(saved, JSON.stringify(evidence));
	} else evidence = JSON.parse(readFileSync(saved, "utf8"));
	const listed = await (await page.request.get(`${url}/accruals`)).json();
	const accrual = listed.accruals.report.accruals.find(
		(a: { accrualId: number }) => a.accrualId === evidence.accrualId,
	);
	if (
		!accrual ||
		accrual.totalAmount !== 1000 ||
		accrual.recognizedAmount !== 1000 ||
		accrual.remainingAmount !== 0 ||
		!accrual.fullyRecognized
	)
		throw new Error("Accrual register does not reconcile");
	const plan = await (
		await page.request.get(`${url}/accruals/${evidence.accrualId}/schedule`)
	).json();
	if (
		plan.accrualSchedule.schedule
			.map((p: { amount: number }) => p.amount)
			.join(",") !== "333.33,333.33,333.34" ||
		plan.accrualSchedule.schedule.some(
			(p: { recognized: boolean }) => !p.recognized,
		)
	)
		throw new Error("Recognition schedule not persisted exactly");
	const trial = await (await page.request.get(`${url}/trial-balance`)).json();
	if (!trial.trialBalance.balanced)
		throw new Error("Accrual unbalanced ledger");
	const income = await (
		await page.request.get(`${url}/income-statement?year=2026`)
	).json();
	const expense = income.incomeStatement.expense.find(
		(line: { accountNo: string }) => line.accountNo === "3910",
	);
	if (!expense || expense.amount !== 1000)
		throw new Error(
			"Accrual recognition did not reach expense account in financial statements",
		);

	const journals = await (await page.request.get(`${url}/journal`)).json();
	const entries = journals.journal.entries.filter((j: { text: string }) =>
		j.text.includes("Synthetic prepaid service"),
	);
	if (
		entries.length !== 4 ||
		entries.some(
			(j: { documentId: number }) => j.documentId !== evidence.documentId,
		)
	)
		throw new Error(
			"Accrual journal lost original link or duplicated a period",
		);
	const original = await page.request.get(
		`${url}/documents/${evidence.documentId}/file`,
	);
	if (
		!original.ok() ||
		!(await original.text()).includes("Synthetic service accrual evidence")
	)
		throw new Error("Original evidence missing");
	await page.goto(`${base}/companies/synthetic/periodisering`);
	await page
		.getByRole("row")
		.filter({ hasText: "Synthetic prepaid service" })
		.getByText("Fuldt realiseret", { exact: true })
		.waitFor();
	await page.screenshot({
		path: "/tmp/rentemester-rust-accruals.png",
		fullPage: true,
	});
	console.log(
		restarted
			? "PASS: accrual register, exact schedule, journal evidence and original persist after restart"
			: "PASS: original → explicit accounts and month plan → confirmation →3 recognized periods (333.33/333.33/333.34) → zero remaining; duplicate period refused",
	);
}
