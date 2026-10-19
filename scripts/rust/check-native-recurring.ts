import type { Page } from "@playwright/test";
import { readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
const saved = "/tmp/rentemester-rust-reports/native-recurring.json";
const pdfPath = "/tmp/rentemester-rust-reports/native-recurring.pdf";
export async function checkNativeRecurring(
	page: Page,
	base: string,
	restarted = false,
) {
	const url = `${base}/api/companies/synthetic`,
		headers = { origin: base, "content-type": "application/json" };
	let evidence: {
		templateId: number;
		documentId: number;
		invoiceNumber: string;
		issueDate: string;
	};
	if (!restarted) {
		const today = new Date().toLocaleDateString("sv-SE", {
			timeZone: "Europe/Copenhagen",
		});
		const profile = await (await page.request.get(`${url}/company`)).json();
		if (
			!profile.company.cvr ||
			!profile.company.address ||
			!profile.company.vatPeriodType
		) {
			await page.goto(`${base}/companies/synthetic/manage`);
			const form = page.getByRole("form", { name: "Rediger stamdata" });
			if (!profile.company.cvr)
				await form.getByLabel("CVR-nummer", { exact: true }).fill("87654321");
			await form
				.getByLabel("Adresse", { exact: true })
				.fill("Synthetic Street 1");
			await form.getByLabel("Postnummer", { exact: true }).fill("1000");
			await form.getByLabel("By", { exact: true }).fill("Synthetic City");
			await form.getByLabel(/^Momsperiode/).selectOption("quarter");
			await form
				.getByRole("button", { name: "Gem stamdata", exact: true })
				.click();
			await page.getByText("Stamdata opdateret.", { exact: true }).waitFor();
		}
		await page.goto(`${base}/companies/synthetic/kontakter`);
		await page
			.getByRole("button", { name: "Tilføj kunde", exact: true })
			.first()
			.click();
		const contact = page.getByRole("dialog");
		await contact
			.getByLabel("Navn", { exact: true })
			.fill("Synthetic Recurring Buyer");
		await contact
			.getByLabel("Adresse", { exact: true })
			.fill("Synthetic Recurring Street 3");
		await contact.getByLabel(/^CVR \/ moms-nr\./).fill("DK12345678");
		await contact.getByRole("button", { name: "Opret", exact: true }).click();
		await contact.waitFor({ state: "hidden" });
		const contacts = await (await page.request.get(`${url}/contacts`)).json();
		const customer = contacts.contacts.customers.find(
			(c: { name: string }) => c.name === "Synthetic Recurring Buyer",
		);
		if (!customer) throw new Error("Recurring customer missing");
		await page.goto(`${base}/companies/synthetic/faktura-skabeloner`);
		await page
			.getByRole("button", { name: "Opret skabelon", exact: true })
			.first()
			.click();
		const dialog = page.getByRole("dialog");
		await dialog
			.getByLabel("Skabelonens navn", { exact: true })
			.fill("Synthetic monthly subscription");
		await dialog
			.getByLabel("Interval", { exact: true })
			.selectOption("monthly");
		await dialog
			.getByLabel("Første udstedelsesdato", { exact: true })
			.fill(today);
		await dialog
			.getByLabel("Betalingsfrist i dage", { exact: true })
			.fill("14");
		await dialog
			.getByLabel("Vælg kunde", { exact: true })
			.selectOption(String(customer.id));
		await dialog
			.getByLabel("Linje 1 beskrivelse", { exact: true })
			.fill("Synthetic recurring service");
		await dialog.getByLabel("Linje 1 antal", { exact: true }).fill("3");
		await dialog
			.getByLabel("Linje 1 enhedspris", { exact: true })
			.fill("19,99");
		const [created] = await Promise.all([
			page.waitForResponse(
				(r) =>
					r.url().endsWith("/recurring-invoices") &&
					r.request().method() === "POST",
			),
			dialog
				.getByRole("button", { name: "Opret skabelon", exact: true })
				.click(),
		]);
		if (!created.ok())
			throw new Error(`Template failed: ${await created.text()}`);
		const template = (await created.json()).template;
		await dialog.waitFor({ state: "hidden" });
		const card = page.locator(".card").filter({
			has: page.getByRole("heading", {
				name: "Synthetic monthly subscription",
				exact: true,
			}),
		});
		await card.getByRole("button", { name: "Generér", exact: true }).click();
		const confirmation = page.getByRole("dialog");
		await confirmation.getByText(/Der udstedes én faktura/).waitFor();
		const [generated] = await Promise.all([
			page.waitForResponse(
				(r) => r.url().endsWith("/generate") && r.request().method() === "POST",
			),
			confirmation
				.getByRole("button", { name: "Udsted periodens faktura", exact: true })
				.click(),
		]);
		if (!generated.ok())
			throw new Error(`Generation failed: ${await generated.text()}`);
		const generation = (await generated.json()).generation;
		if (!generation.created || generation.periodIndex !== 0)
			throw new Error("Wrong recurring generation period");
		await confirmation.waitFor({ state: "hidden" });
		await card
			.getByRole("link", { name: generation.invoiceNumber, exact: true })
			.waitFor();
		evidence = {
			templateId: template.templateId,
			documentId: generation.documentId,
			invoiceNumber: generation.invoiceNumber,
			issueDate: today,
		};
		const replay = await page.request.post(
			`${url}/recurring-invoices/${evidence.templateId}/generate`,
			{ headers, data: { asOfDate: today, confirm: true } },
		);
		const replayed = await replay.json();
		if (
			!replay.ok() ||
			replayed.generation.created ||
			replayed.generation.documentId !== evidence.documentId
		)
			throw new Error("Recurring period issued twice");
		writeFileSync(saved, JSON.stringify(evidence));
	} else evidence = JSON.parse(readFileSync(saved, "utf8"));
	const listed = await (
		await page.request.get(`${url}/recurring-invoices`)
	).json();
	const template = listed.recurringInvoices.templates.find(
		(t: { id: number }) => t.id === evidence.templateId,
	);
	if (
		!template ||
		template.generations.length !== 1 ||
		template.generations[0].documentId !== evidence.documentId ||
		template.nextIssueDate <= evidence.issueDate
	)
		throw new Error("Recurring linkage or next date lost");
	const invoices = await (await page.request.get(`${url}/invoices`)).json();
	const invoice = invoices.invoices.invoices.find(
		(i: { documentId: number }) => i.documentId === evidence.documentId,
	);
	if (
		!invoice ||
		invoice.grossAmount !== 74.96 ||
		invoice.posted ||
		invoice.customerName !== "Synthetic Recurring Buyer"
	)
		throw new Error("Generated invoice facts wrong");
	const original = await page.request.get(
		`${url}/documents/${evidence.documentId}/file`,
	);
	if (!original.ok()) throw new Error("Generated original unavailable");
	const payload = await original.json();
	if (
		payload.recurringTemplateId !== evidence.templateId ||
		payload.recurringPeriodIndex !== 0 ||
		payload.totals.netAmount !== 59.97 ||
		payload.totals.vatAmount !== 14.99 ||
		payload.totals.grossAmount !== 74.96 ||
		payload.buyer.name !== "Synthetic Recurring Buyer"
	)
		throw new Error("Immutable generated snapshot mismatch");
	const pdf = await page.request.get(
		`${url}/invoices/${evidence.documentId}/pdf`,
	);
	if (!pdf.ok()) throw new Error("Generated PDF unavailable");
	const bytes = await pdf.body();
	if (restarted && !readFileSync(pdfPath).equals(bytes))
		throw new Error("Generated PDF changed after restart");
	writeFileSync(pdfPath, bytes);
	const text = execFileSync("pdftotext", ["-layout", pdfPath, "-"], {
		encoding: "utf8",
	});
	for (const term of [
		evidence.invoiceNumber,
		"Synthetic Recurring Buyer",
		"Synthetic recurring service",
		"59,97",
		"14,99",
		"74,96",
	])
		if (!text.includes(term)) throw new Error(`Generated PDF missing ${term}`);
	await page.goto(`${base}/companies/synthetic/faktura-skabeloner`);
	await page
		.getByRole("link", { name: evidence.invoiceNumber, exact: true })
		.waitFor();
	await page
		.getByRole("link", { name: evidence.invoiceNumber, exact: true })
		.click();
	await page
		.getByRole("row")
		.filter({ hasText: evidence.invoiceNumber })
		.getByText("Udstedt – afventer bogføring", { exact: true })
		.waitFor();
	await page.screenshot({
		path: "/tmp/rentemester-rust-recurring.png",
		fullPage: true,
	});
	console.log(
		restarted
			? "PASS: recurring template, single generation, immutable original and identical PDF survive restart"
			: "PASS: customer → manual recurring template → confirmed due invoice → original and PDF with59.97/14.99/74.96; repeated period returns same invoice",
	);
}
