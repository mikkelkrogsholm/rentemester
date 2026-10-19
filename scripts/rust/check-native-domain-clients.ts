// Real host CLI/MCP binaries against the same authenticated native server as the browser.
// Build --locked -p rentemester-cli -p rentemester-mcp on the host first, or set
// RM_NATIVE_CLI_BIN/RM_NATIVE_MCP_BIN. No Docker build, mocked requests or local ledger access.
import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
const saved = "/tmp/rentemester-rust-reports/native-domain-clients.json";
export async function checkNativeDomainClients(
	page: Page,
	base: string,
	restarted = false,
) {
	const temp = mkdtempSync("/tmp/rm-native-domain-clients-"),
		cookieFile = join(temp, "session.cookie");
	const cookies = (await page.context().cookies(base))
		.map((c) => `${c.name}=${c.value}`)
		.join("; ");
	writeFileSync(cookieFile, cookies, { mode: 0o600, flag: "wx" });
	const common = [
			"--server",
			base,
			"--allow-loopback-http",
			"--cookie-file",
			cookieFile,
		],
		cli =
			process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"),
		mcp =
			process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp");
	function cliCall(
		command: string,
		body?: string,
		flags: string[] = [],
		allowFailure = false,
	) {
		const child = spawnSync(
			cli,
			["remote", command, ...common, "--company", "synthetic", ...flags],
			{ input: body, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 },
		);
		if (child.error)
			throw new Error(
				"Host Rust CLI unavailable; build rentemester-cli or configure RM_NATIVE_CLI_BIN",
			);
		if (!allowFailure && child.status !== 0)
			throw new Error(`CLI ${command} failed: ${child.stdout.slice(0, 1000)}`);
		const value = JSON.parse(child.stdout);
		return { status: child.status, value };
	}
	function rpc(name: string, args: Record<string, unknown>) {
		const input =
			[
				{
					jsonrpc: "2.0",
					id: 1,
					method: "initialize",
					params: {
						protocolVersion: "2025-11-25",
						capabilities: {},
						clientInfo: { name: "synthetic-native-domain-check", version: "1" },
					},
				},
				{ jsonrpc: "2.0", method: "notifications/initialized" },
				{
					jsonrpc: "2.0",
					id: 2,
					method: name === "tools/list" ? name : "tools/call",
					params: name === "tools/list" ? {} : { name, arguments: args },
				},
			]
				.map((v) => JSON.stringify(v))
				.join("\n") + "\n";
		const child = spawnSync(mcp, common, {
			input,
			encoding: "utf8",
			maxBuffer: 16 * 1024 * 1024,
		});
		if (child.error)
			throw new Error(
				"Host Rust MCP unavailable; build rentemester-mcp or configure RM_NATIVE_MCP_BIN",
			);
		if (child.status !== 0) throw new Error("MCP subprocess failed");
		const replies = child.stdout
			.trim()
			.split("\n")
			.map((line) => JSON.parse(line));
		const result = replies.find((r) => r.id === 2);
		if (result?.error || result?.result?.isError)
			throw new Error(
				`MCP ${name} failed: ${JSON.stringify(result).slice(0, 1000)}`,
			);
		return name === "tools/list"
			? result.result
			: result.result.structuredContent.data;
	}
	let evidence: {
		customerId: number;
		vendorId: number;
		invoiceId: number;
		invoiceNumber: string;
	};
	try {
		const tools = rpc("tools/list", {}).tools;
		for (const name of [
			"contacts",
			"documents",
			"invoices",
			"preview_invoice",
			"create_customer",
			"update_customer",
			"create_vendor",
			"update_vendor",
			"issue_invoice",
			"post_invoice",
		]) {
			if (!tools.find((t: { name: string }) => t.name === name))
				throw new Error(`Missing domain MCP tool ${name}`);
		}
		if (!restarted) {
			const url = `${base}/api/companies/synthetic`,
				profile = await (await page.request.get(`${url}/company`)).json();
			if (
				!profile.company.cvr ||
				!profile.company.address ||
				!profile.company.vatPeriodType
			) {
				await page.goto(`${base}/companies/synthetic/manage`);
				const form = page.getByRole("form", { name: "Rediger stamdata" });
				if (!profile.company.cvr)
					await form.getByLabel("CVR-nummer", { exact: true }).fill("87654321");
				if (!profile.company.address)
					await form
						.getByLabel("Adresse", { exact: true })
						.fill("Synthetic Domain Street1");
				await form.getByLabel(/^Momsperiode/).selectOption("quarter");
				await form
					.getByRole("button", { name: "Gem stamdata", exact: true })
					.click();
				await page.getByText("Stamdata opdateret.", { exact: true }).waitFor();
			}
			const customer = cliCall(
				"create-customer",
				'{ "name":"Synthetic Domain Client Buyer", "address":"Synthetic Buyer2", "vatOrCvr":"DK12345678" }',
				["--confirm"],
			).value.data.customer;
			const vendor = rpc("create_vendor", {
				company: "synthetic",
				confirm: true,
				body: '{ "name":"Synthetic Domain Client Vendor", "countryCode":"DK", "identifierKind":"dk_cvr", "vatOrCvr":"DK12345678", "defaultExpenseAccount":"3000" }',
			}).vendor;
			rpc("update_customer", {
				company: "synthetic",
				id: customer.id,
				confirm: true,
				body: '{ "email":"synthetic-domain@example.invalid" }',
			});
			cliCall("update-vendor", '{ "phone":"12345678" }', [
				"--id",
				String(vendor.id),
				"--confirm",
			]);
			const initialChart = cliCall("accounts").value.data.accounts;
			const confirmedRole = (role: string) =>
				initialChart.accountRoles.resolutions.find(
					(r: { role: string; ok: boolean; accountNo?: string }) =>
						r.role === role && r.ok,
				)?.accountNo as string | undefined;
			const existingDebtors = confirmedRole("debtors"),
				existingOutputVat = confirmedRole("output_vat");
			if (
				initialChart.accountRoles.ambiguous.some(
					(role: string) => role === "debtors" || role === "output_vat",
				)
			)
				throw new Error(
					"Ambiguous existing posting role requires reconciliation; fixture will not remap it",
				);
			const debtors = existingDebtors ?? "1111",
				outputVat = existingOutputVat ?? "4211";
			await page.goto(`${base}/companies/synthetic/kontoplan`);
			for (const [no, name, type, balance] of [
				...(!existingDebtors
					? [["1111", "Synthetic domain debtors", "asset", "debit"]]
					: []),
				["1011", "Synthetic domain revenue", "income", "credit"],
				...(!existingOutputVat
					? [["4211", "Synthetic domain output VAT", "vat", "credit"]]
					: []),
			]) {
				const form = page.getByRole("form", {
					name: "Opret konto",
					exact: true,
				});
				await form.getByLabel("Kontonummer", { exact: true }).fill(no);
				await form.getByLabel("Kontonavn", { exact: true }).fill(name);
				await form.getByLabel(/^Kontotype/).selectOption(type);
				await form.getByLabel(/^Normal saldo/).selectOption(balance);
				await form
					.getByRole("button", { name: "Opret konto", exact: true })
					.click();
				await page.getByRole("row").filter({ hasText: name }).waitFor();
			}
			for (const [role, no] of [
				...(!existingDebtors ? [["debtors", debtors]] : []),
				...(!existingOutputVat ? [["output_vat", outputVat]] : []),
			]) {
				const form = page.getByRole("form", {
					name: "Bekræft kontorolle",
					exact: true,
				});
				await form.getByLabel(/^Kontorolle/).selectOption(role);
				await form.getByLabel(/^Konto til rollen/).selectOption(no);
				await form
					.getByRole("button", { name: "Bekræft kontorolle", exact: true })
					.click();
				await page
					.getByRole("row")
					.filter({ hasText: no })
					.filter({ hasText: role === "debtors" ? "Debitorer" : "Salgsmoms" })
					.waitFor();
			}
			const chart = cliCall("accounts").value.data.accounts;
			for (const [role, no] of [
				["debtors", debtors],
				["output_vat", outputVat],
			])
				if (
					!chart.accountRoles.resolutions.find(
						(r: { role: string; accountNo: string; ok: boolean }) =>
							r.role === role && r.accountNo === no && r.ok,
					)
				)
					throw new Error("CLI chart did not expose confirmed posting role");

			const today = new Date().toLocaleDateString("sv-SE", {
				timeZone: "Europe/Copenhagen",
			});
			const invoiceBody = JSON.stringify({
				issueDate: today,
				customerId: customer.id,
				lines: [
					{
						description: "Synthetic domain client service",
						quantity: 1,
						unitPriceExVat: 200,
					},
				],
			});
			const before = rpc("invoices", {
				company: "synthetic",
			}).invoices.invoices.filter(
				(i: { customerId: number }) => i.customerId === customer.id,
			);
			if (before.length) throw new Error("New customer already has an invoice");
			const preview = cliCall("preview-invoice", invoiceBody).value.data
				.preview;
			if (
				preview.contentType !== "application/pdf" ||
				!Buffer.from(preview.bytes).subarray(0, 5).equals(Buffer.from("%PDF-"))
			)
				throw new Error("CLI preview did not preserve PDF");
			if (
				rpc("invoices", { company: "synthetic" }).invoices.invoices.some(
					(i: { customerId: number }) => i.customerId === customer.id,
				)
			)
				throw new Error("Preview issued an invoice");
			const invoice = rpc("issue_invoice", {
				company: "synthetic",
				body: invoiceBody,
				confirm: true,
				previewReviewed: true,
			}).invoice;
			if (
				invoice.netAmount !== 200 ||
				invoice.vatAmount !== 50 ||
				invoice.grossAmount !== 250
			)
				throw new Error("MCP invoice amounts differ from server computation");
			const postBody = JSON.stringify({
				invoiceDocumentId: invoice.documentId,
				revenueAccountNo: "1011",
				confirm: true,
			});
			cliCall("post-invoice", postBody, ["--confirm"]);
			const replay = cliCall("post-invoice", postBody, ["--confirm"], true);
			if (replay.status !== 1 || replay.value.code !== "server_refused")
				throw new Error("Duplicate invoice posting accepted");
			evidence = {
				customerId: customer.id,
				vendorId: vendor.id,
				invoiceId: invoice.documentId,
				invoiceNumber: invoice.invoiceNumber,
			};
			writeFileSync(saved, JSON.stringify(evidence));
		} else evidence = JSON.parse(readFileSync(saved, "utf8"));
		const contacts = cliCall("contacts").value.data.contacts;
		const customer = contacts.customers.find(
				(c: { id: number }) => c.id === evidence.customerId,
			),
			vendor = contacts.vendors.find(
				(v: { id: number }) => v.id === evidence.vendorId,
			);
		if (
			customer?.email !== "synthetic-domain@example.invalid" ||
			vendor?.phone !== "12345678"
		)
			throw new Error("CLI/MCP master data changes not persisted");
		const invoiceData = cliCall("invoices").value.data.invoices;
        const invoices = invoiceData.invoices;
		const invoice = invoices.find(
			(i: { documentId: number }) => i.documentId === evidence.invoiceId,
		);
		if (!invoice || invoice.grossAmount !== 250 || invoice.posted !== true)
			throw new Error("Native domain client invoice state not persisted");
		const documents = rpc("documents", { company: "synthetic" }).documents
			.documents;
		if (!documents.find((d: { id: number }) => d.id === evidence.invoiceId))
			throw new Error("Generated original absent from MCP document list");
		await page.goto(`${base}/companies/synthetic/fakturaer?year=${encodeURIComponent(invoiceData.selectedYear)}`);
        const labels: Record<string, string> = { open: "Bogført", overdue: "Forfalden", paid: "Betalt", credited: "Krediteret", refunded: "Refunderet", overpaid: "Overbetalt", written_off: "Afskrevet" };
        if (!labels[invoice.status]) throw new Error(`Unknown persisted invoice settlement status: ${invoice.status}`);
        const row = page.getByRole("row").filter({ hasText: evidence.invoiceNumber });
        try { await row.getByText(labels[invoice.status], { exact: true }).waitFor(); }
        catch { await page.screenshot({path:"/tmp/rentemester-rust-domain-client-failure.png",fullPage:true}); throw new Error(`Invoice UI does not show persisted ${invoiceData.selectedYear}/${invoice.status}: ${(await page.locator("body").innerText()).slice(-2500)}`); }
		console.log(
			restarted
				? "PASS: CLI/MCP customer, vendor, original and posted invoice survive restart"
				: "PASS: CLI customer → MCP vendor → cross-client updates → CLI nonmutating PDF preview → confirmed MCP issue → explicit CLI post; duplicate posting refused",
		);
	} finally {
		rmSync(temp, { recursive: true, force: true });
	}
}
