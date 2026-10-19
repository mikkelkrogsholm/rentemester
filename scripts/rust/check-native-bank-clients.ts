// Build host CLI/MCP binaries first (or RM_NATIVE_CLI_BIN/RM_NATIVE_MCP_BIN).
// The helper uses the real native server and leaves shared build caches alone.
import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
const saved = "/tmp/rentemester-rust-reports/native-bank-clients.json";
export async function checkNativeBankClients(
	page: Page,
	base: string,
	restarted = false,
) {
	const temp = mkdtempSync("/tmp/rm-native-bank-clients-"),
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
						clientInfo: { name: "synthetic-native-bank-check", version: "1" },
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
		payableId: number;
		billDocument: number;
		directDocument: number;
		bankId: number;
		bankSlug: string;
		paymentRow: number;
		directRow: number;
		paymentEntry: number;
		directEntry: number;
		date: string;
		year: string;
		originals: Record<string, string>;
	};
	try {
		if (!restarted) {
			const today = new Date().toLocaleDateString("sv-SE", {
				timeZone: "Europe/Copenhagen",
			});
			const chart = cliCall("accounts").value.data.accounts;
			if (chart.accountRoles.ambiguous.includes("creditors"))
				throw new Error(
					"Ambiguous creditor mapping requires reconciliation; fixture does not replace roles",
				);
			const creditor = chart.accountRoles.resolutions.find(
				(r: { role: string; ok: boolean }) => r.role === "creditors" && r.ok,
			)?.accountNo;
			await page.goto(`${base}/companies/synthetic/kontoplan`);
			for (const [no, name, type, normal] of [
				["1711", "Synthetic bank client asset", "asset", "debit"],
				["3711", "Synthetic bank client purchases", "expense", "debit"],
				...(!creditor
					? [["7011", "Synthetic bank client creditor", "liability", "credit"]]
					: []),
			]) {
				const form = page.getByRole("form", {
					name: "Opret konto",
					exact: true,
				});
				await form.getByLabel("Kontonummer", { exact: true }).fill(no);
				await form.getByLabel("Kontonavn", { exact: true }).fill(name);
				await form.getByLabel(/^Kontotype/).selectOption(type);
				await form.getByLabel(/^Normal saldo/).selectOption(normal);
				await form
					.getByRole("button", { name: "Opret konto", exact: true })
					.click();
				await page.getByRole("row").filter({ hasText: name }).waitFor();
			}
			if (!creditor) {
				const form = page.getByRole("form", {
					name: "Bekræft kontorolle",
					exact: true,
				});
				await form.getByLabel(/^Kontorolle/).selectOption("creditors");
				await form.getByLabel(/^Konto til rollen/).selectOption("7011");
				await form
					.getByRole("button", { name: "Bekræft kontorolle", exact: true })
					.click();
				await page
					.getByRole("row")
					.filter({ hasText: "Kreditorer" })
					.getByText(/Konto 7011/)
					.waitFor();
			}
			const bankAccount = cliCall(
				"create-bank-account",
				JSON.stringify({
					name: "Synthetic bank client account",
					slug: "synthetic-bank-clients",
					currency: "DKK",
					ledgerAccountNo: "1711",
				}),
				["--confirm"],
			).value.data.bankAccount;
			const vendor = rpc("create_vendor", {
				company: "synthetic",
				confirm: true,
				body: JSON.stringify({
					name: "Synthetic Bank Client Supplier",
					address: "Synthetic Supplier4",
					countryCode: "DK",
					identifierKind: "dk_cvr",
					vatOrCvr: "DK12345678",
					defaultExpenseAccount: "3711",
					defaultVatTreatment: "exempt",
				}),
			}).vendor;
			const profile = await (
				await page.request.get(`${base}/api/companies/synthetic/company`)
			).json();
			for (const [filename, invoice, amount, description] of [
				[
					"synthetic-bank-client-bill.txt",
					"SYN-BANK-CLIENT-BILL",
					"125,00",
					"Synthetic client supplier bill",
				],
				[
					"synthetic-bank-client-direct.txt",
					"SYN-BANK-CLIENT-DIRECT",
					"75,00",
					"Synthetic client direct purchase",
				],
			]) {
				await page.goto(`${base}/companies/synthetic/bilag`);
				await page
					.getByRole("button", { name: "Indlæs bilag", exact: true })
					.click();
				const dialog = page.getByRole("dialog");
				await dialog.getByLabel("Bilagsfil", { exact: true }).setInputFiles({
					name: filename,
					mimeType: "text/plain",
					buffer: Buffer.from(
						`${invoice}: ${description}; gross DKK${amount}; VAT0. Synthetic source evidence.\n`,
					),
				});
				await dialog
					.getByLabel("Afsender", { exact: true })
					.fill("Synthetic Bank Client Supplier");
				await dialog
					.getByLabel("Afsender CVR/moms", { exact: true })
					.fill("DK12345678");
				await dialog
					.getByLabel("Afsenderadresse", { exact: true })
					.fill("Synthetic Supplier4");
				await dialog
					.getByLabel("Leverandørland (ISO)", { exact: true })
					.fill("DK");
				await dialog.getByLabel(/^Identitetstype/).selectOption("dk_cvr");
				for (const [label, value] of Object.entries({
					Bilagsdato: today,
					"Fakturanr. (valgfri)": invoice,
					"Beløb inkl. moms": amount,
					Momsbeløb: "0,00",
					"Beskrivelse af leverance": description,
					Modtager: profile.company.name,
					"Modtager CVR/moms": profile.company.cvr ?? "DK87654321",
					Modtageradresse:
						profile.company.address ?? "Synthetic Client Street1",
				}))
					await dialog.getByLabel(label, { exact: true }).fill(value);
				await dialog
					.getByRole("button", { name: "Indlæs bilag", exact: true })
					.click();
				await dialog.getByText(/Bilaget er indlæst som DOC-/).waitFor();
				await dialog.getByRole("button", { name: "Luk", exact: true }).click();
			}
			const docs = cliCall("documents").value.data.documents.documents,
				bill = docs.find(
					(d: { filename: string }) =>
						d.filename === "synthetic-bank-client-bill.txt",
				),
				direct = docs.find(
					(d: { filename: string }) =>
						d.filename === "synthetic-bank-client-direct.txt",
				);
			if (!bill || !direct)
				throw new Error("Client original documents missing");
			const originals: Record<string, string> = {};
			for (const id of [bill.id, direct.id]) {
				const original = await page.request.get(
					`${base}/api/companies/synthetic/documents/${id}/file`,
				);
				if (!original.ok())
					throw new Error("Client original unavailable before booking");
				originals[String(id)] = (await original.body()).toString("base64");
			}
			const registerBody = JSON.stringify({
				documentId: bill.id,
				billDate: today,
				dueDate: today,
				expenseAccountNo: "3711",
				vatTreatment: "exempt",
				vendorId: vendor.id,
				confirm: true,
			});
			const registered = cliCall("register-payable", registerBody, [
				"--confirm",
			]).value.data.payable;
			const open = cliCall("payables", undefined, [
				"--status",
				"open",
				"--as-of",
				today,
			]).value.data.payables.rows.find(
				(r: { payableId: number }) => r.payableId === registered.payableId,
			);
			if (
				!open ||
				open.grossAmount !== 125 ||
				open.openBalance !== 125 ||
				open.paidAmount !== 0
			)
				throw new Error(
					"Registered supplier bill did not retain original amount",
				);
			const csv = `date;text;amount;currency;reference\n${today};Synthetic bank client supplier payment;-125;DKK;SYN-CLIENT-BILL-PAY\n${today};Synthetic bank client direct payment;-75;DKK;SYN-CLIENT-DIRECT-PAY\n`;
			const importBody = JSON.stringify({
				csvContent: csv,
				account: bankAccount.slug,
				confirm: true,
			});
			const imported = rpc("import_bank", {
				company: "synthetic",
				body: importBody,
				confirm: true,
			}).import;
			if (imported.imported !== 2)
				throw new Error("MCP CSV did not import exactly two client rows");
			const duplicateImport = cliCall("import-bank", importBody, ["--confirm"])
				.value.data.import;
			if (
				duplicateImport.imported !== 0 ||
				duplicateImport.skippedDuplicates !== 2
			)
				throw new Error("Known duplicate CSV inserted new bank rows");
			const bank = cliCall("bank").value.data.bank,
				paymentRow = bank.transactions.find(
					(r: { text: string }) =>
						r.text === "Synthetic bank client supplier payment",
				),
				directRow = bank.transactions.find(
					(r: { text: string }) =>
						r.text === "Synthetic bank client direct payment",
				);
			if (!paymentRow || !directRow)
				throw new Error("Imported client bank rows not discoverable");
			const options = rpc("expense_booking_options", {
				company: "synthetic",
				id: direct.id,
			}).options;
			if (
				options.document.id !== direct.id ||
				!options.unmatchedOutgoingBank.find(
					(r: { id: number }) => r.id === directRow.id,
				)
			)
				throw new Error("Original bank-match options missing");
			const paymentBody = JSON.stringify({
				bankTransactionId: paymentRow.id,
				paymentDate: today,
				amount: 125,
				paymentAccountNo: "1711",
				confirm: true,
			});
			const payment = rpc("pay_payable", {
				company: "synthetic",
				id: registered.payableId,
				body: paymentBody,
				confirm: true,
			}).payment;
			if (payment.openBalance !== 0)
				throw new Error("Supplier payment did not clear its bill");
			const replay = cliCall(
				"pay-payable",
				paymentBody,
				["--id", String(registered.payableId), "--confirm"],
				true,
			);
			if (replay.status !== 1 || replay.value.code !== "server_refused")
				throw new Error("Bank payment applied twice");
			const matchBody = JSON.stringify({
				documentId: direct.id,
				bankTransactionId: directRow.id,
				expenseAccountNo: "3711",
				vatTreatment: "exempt",
				paymentAccountNo: "1711",
				transactionDate: today,
				text: "Synthetic bank client direct purchase",
				confirm: true,
			});
			const booking = cliCall("match-bank-expense", matchBody, ["--confirm"])
				.value.data.booking;
			const duplicate = cliCall(
				"match-bank-expense",
				matchBody,
				["--confirm"],
				true,
			);
			if (duplicate.status !== 1 || duplicate.value.code !== "server_refused")
				throw new Error("Direct expense bank row matched twice");
			evidence = {
				payableId: registered.payableId,
				billDocument: bill.id,
				directDocument: direct.id,
				bankId: bankAccount.id,
				bankSlug: bankAccount.slug,
				paymentRow: paymentRow.id,
				directRow: directRow.id,
				paymentEntry: payment.journalEntryId,
				directEntry: booking.entryId,
				date: today,
				year: bank.selectedYear,
				originals,
			};
			writeFileSync(saved, JSON.stringify(evidence));
		} else evidence = JSON.parse(readFileSync(saved, "utf8"));
		const accounts = rpc("bank_accounts", { company: "synthetic" }).bankAccounts
			.accounts;
		if (
			!accounts.find(
				(a: { id: number; slug: string; ledgerAccountNo: string }) =>
					a.id === evidence.bankId &&
					a.slug === evidence.bankSlug &&
					a.ledgerAccountNo === "1711",
			)
		)
			throw new Error("Client bank account mapping not persisted");
		const paid = cliCall("payables", undefined, [
			"--status",
			"paid",
			"--as-of",
			evidence.date,
		]).value.data.payables.rows.find(
			(r: { payableId: number }) => r.payableId === evidence.payableId,
		);
		if (
			!paid ||
			paid.documentId !== evidence.billDocument ||
			paid.grossAmount !== 125 ||
			paid.paidAmount !== 125 ||
			paid.openBalance !== 0
		)
			throw new Error("Paid supplier bill not persisted correctly");
		const bank = rpc("bank", {
			company: "synthetic",
			year: evidence.year,
		}).bank;
		for (const id of [evidence.paymentRow, evidence.directRow]) {
			const row = bank.transactions.find((r: { id: number }) => r.id === id);
			if (!row || row.reconciliationStatus !== "matched" || !row.journalEntryNo)
				throw new Error("CLI/MCP bank match lost");
		}
		const journal = cliCall("journal", undefined, ["--year", evidence.year])
			.value.data.journal.entries;
		for (const [entry, document] of [
			[evidence.paymentEntry, evidence.billDocument],
			[evidence.directEntry, evidence.directDocument],
		])
			if (
				!journal.find(
					(j: { id: number; documentId: number }) =>
						j.id === entry && j.documentId === document,
				)
			)
				throw new Error(
					"Supplier payment or direct match lost original journal evidence",
				);
		const report = cliCall("income-statement", undefined, [
			"--year",
			evidence.year,
		]).value.data.incomeStatement;
		if (
			report.expense.find((r: { accountNo: string }) => r.accountNo === "3711")
				?.amount !== 200
		)
			throw new Error(
				"Payment repeated supplier expense or bank match missing from report",
			);
		const trial = cliCall("trial-balance", undefined, ["--year", evidence.year])
			.value.data.trialBalance;
		if (
			!trial.balanced ||
			trial.rows.find((r: { accountNo: string }) => r.accountNo === "1711")
				?.balance !== -200
		)
			throw new Error(
				"Client bank ledger not reconciled to its matched outflows",
			);
		for (const [id, expected] of Object.entries(evidence.originals)) {
			const original = await page.request.get(
				`${base}/api/companies/synthetic/documents/${id}/file`,
			);
			if (
				!original.ok() ||
				!(await original.body()).equals(Buffer.from(expected, "base64"))
			)
				throw new Error("Client payment changed original evidence");
		}
		await page.goto(
			`${base}/companies/synthetic/bank?year=${encodeURIComponent(evidence.year)}`,
		);
		await page
			.getByText("Synthetic bank client direct payment", { exact: true })
			.waitFor();
		await page.screenshot({
			path: "/tmp/rentemester-rust-bank-clients.png",
			fullPage: true,
		});
		console.log(
			restarted
				? "PASS: CLI/MCP supplier bill, payment, direct bank match, originals and reports survive restart"
				: "PASS: CLI bank account → MCP import/dedupe → original supplier bill registration → MCP payment → explicit direct expense match; duplicate row/payment refused, original preserved, expense200 and bank-200",
		);
	} finally {
		rmSync(temp, { recursive: true, force: true });
	}
}
