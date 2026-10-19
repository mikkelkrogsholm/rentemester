import type { Page } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { isDeepStrictEqual } from "node:util";
const saved = "/tmp/rentemester-rust-reports/native-consolidation-browser.json";
type Evidence = {
	anchor: string;
	other: string;
	asOf: string;
	profileId: string;
	profileHash: string;
	current: unknown;
	report: any;
	journals: unknown[];
	restartVerified?: boolean;
};
/** Dedicated API-created companies and TWO ordinarily verified current MFA workspace owners.
 * No authentication/grant fixture is created here. Company ledgers are never shared with other helpers. */
export async function checkNativeConsolidation(
	author: Page,
	reviewer: Page,
	base: string,
	anchor: string,
	other: string,
	restarted = false,
) {
	if (
		!/^http:\/\/127\.0\.0\.1:\d+$/.test(base) ||
		anchor === other ||
		![anchor, other].every((s) =>
			/^synthetic-consolidation-browser-[a-z0-9-]+$/.test(s),
		)
	)
		throw Error(
			"Dedicated synthetic consolidation companies/loopback required",
		);
	const headers = { origin: base, "content-type": "application/json" },
		root = `/api/companies/${anchor}/consolidation`;
	async function get(page: Page, path: string) {
		const r = await page.request.get(base + path);
		if (!r.ok()) throw Error(`Consolidation read${path}: HTTP${r.status()}`);
		return r.json();
	}
	async function post(page: Page, path: string, data: unknown, status = 200) {
		const r = await page.request.post(base + path, { headers, data });
		if (r.status() !== status)
			throw Error(`Consolidation ${path}: expected${status} got${r.status()}`);
		return r.json();
	}
	const a = await get(author, "/api/me"),
		b = await get(reviewer, "/api/me");
	if (!a.user?.id || !b.user?.id || a.user.id === b.user.id)
		throw Error("Independent normal current MFA principals required");
	async function panel(page: Page, asOf: string) {
		await page.goto(base + "/koncernstruktur");
		const region = page.getByRole("region", {
			name: "Konsolideret rapport",
			exact: true,
		});
		await region
			.getByLabel("Konsolidering virksomhed", { exact: true })
			.selectOption(anchor);
		await region
			.getByLabel("Konsolidering pr. dato", { exact: true })
			.fill(asOf);
		return region;
	}
	async function ui(page: Page, asOf: string, action: string, body: unknown) {
		const region = await panel(page, asOf);
		await region
			.getByLabel("Konsolidering handling", { exact: true })
			.selectOption(action);
		await region
			.getByLabel("Konsolidering JSON", { exact: true })
			.fill(JSON.stringify(body));
		const button = region.getByRole("button", {
			name: "Registrér rapportprofil-handling",
			exact: true,
		});
		if (!(await button.isDisabled()))
			throw Error("Profile write available without review confirmation");
		await region
			.getByLabel("Bekræft konsolideringsprofil", { exact: true })
			.check();
		const [response] = await Promise.all([
			page.waitForResponse(
				(r) =>
					r.url() === base + root + "/" + action &&
					r.request().method() === "POST",
			),
			button.click(),
		]);
		if (!response.ok())
			throw Error(`Consolidation UI${action} refused${response.status()}`);
		return response.json();
	}
	let evidence: Evidence;
	if (!restarted) {
		const asOf = new Date().toLocaleDateString("sv-SE", {
				timeZone: "Europe/Copenhagen",
			}),
			groupId = "synthetic-consolidation-browser",
			profileId = "synthetic-consolidation-browser-profile";
		for (const slug of [anchor, other]) {
			const journal = (await get(author, `/api/companies/${slug}/journal`))
				.journal;
			if (journal.entries.length)
				throw Error("Consolidation requires own initially empty ledger");
			for (const [accountNo, type, normalBalance] of [
				["1861", "asset", "debit"],
				["3861", "income", "credit"],
			] as const)
				await post(author, `/api/companies/${slug}/accounts`, {
					accountNo,
					name: `Synthetic consolidation ${accountNo}`,
					type,
					normalBalance,
				});
			const original = await post(
				author,
				`/api/companies/${slug}/documents/ingest`,
				{
					confirm: true,
					fileName: `synthetic-consolidation-${slug}.txt`,
					fileBase64: Buffer.from(
						`Synthetic consolidation retained receipt ${slug}: DKK12.34, no VAT.\n`,
					).toString("base64"),
					metadata: {
						source: "Synthetic consolidation fixture",
						documentType: "cash_register_receipt",
					},
				},
			);
			await post(author, `/api/companies/${slug}/manual-journal`, {
				confirm: true,
				input: {
					format: "rentemester-manual-journal-post/1",
					expectedRuleVersion: journal.nativePosting.ruleVersion,
					idempotencyKey: `synthetic-consolidation-${slug}`,
					request: {
						documentId: String(original.document.id),
						transactionDate: asOf,
						text: "Synthetic consolidation source",
						currency: "DKK",
						lines: [
							{ accountNo: "1861", debitAmount: "12.34", creditAmount: "0.00" },
							{ accountNo: "3861", debitAmount: "0.00", creditAmount: "12.34" },
						],
					},
				},
			});
		}
		const interRoot = `/api/companies/${anchor}/intercompany`,
			prior = await get(author, interRoot);
		if (prior.manifest === null && prior.manifestHash !== null)
			throw Error("Cannot replace hidden groupmanifest");
		const groups = prior.manifest?.groups ?? [];
		if (groups.some((g: any) => g.id === groupId))
			throw Error("Own groupfixture already exists");
		await post(author, interRoot + "/manifest", {
			version: 1,
			groups: [
				...groups,
				{
					id: groupId,
					name: "Synthetic consolidation browser group",
					memberships: [
						{
							id: "browser-parent",
							companySlug: anchor,
							validFrom: "2000-01-01",
						},
						{
							id: "browser-child",
							companySlug: other,
							validFrom: "2000-01-01",
						},
					],
					ownership: [
						{
							id: "browser-full-child",
							parentCompanySlug: anchor,
							childCompanySlug: other,
							basisPoints: 10000,
							evidenceRefs: ["synthetic-reviewed-group-source"],
							validFrom: "2000-01-01",
						},
					],
				},
			],
			confirm: true,
		});
		const reportingLines = [
			["assets", "Assets", "asset"],
			["debt", "Debt", "liability"],
			["equity", "Equity", "equity"],
			["result", "Current result", "equity"],
			["income", "Income", "income"],
			["expense", "Expense", "expense"],
		].map(([id, label, section], displayOrder) => ({
			id,
			label,
			section,
			displayOrder,
			...(id === "result" ? { role: "current-result" } : {}),
		}));
		const profile = {
			version: 1,
			id: profileId,
			groupId,
			currency: "DKK",
			reportingLines,
			accountMappings: [anchor, other].flatMap((companySlug, index) =>
				[
					["1861", "assets"],
					["3861", "income"],
				].map(([accountNo, reportingLineId]) => ({
					id: `browser-${index}-${accountNo}`,
					companySlug,
					accountNo,
					reportingLineId,
					validFrom: "2000-01-01",
				})),
			),
			evidenceRefs: ["synthetic-reviewed-reporting-chart"],
			validFrom: "2000-01-01",
		};
		await post(author, root + "/propose", { profile, confirm: false }, 400);
		const proposal = await ui(author, asOf, "propose", { profile }),
			exact = { profileId, profileHash: proposal.profileHash };
		await post(author, root + "/approve", { ...exact, confirm: true }, 422);
		await post(
			reviewer,
			root + "/approve",
			{ ...exact, profileHash: "0".repeat(64), confirm: true },
			422,
		);
		await post(
			reviewer,
			root + "/approve",
			{ ...exact, actor: "user:forged", confirm: true },
			400,
		);
		await ui(reviewer, asOf, "approve", exact);
		const read = () =>
				get(
					author,
					`/api/group-consolidated-report?${new URLSearchParams({ profileId, from: asOf, asOf })}`,
				),
			report = await read();
		if (
			report.status !== "ready" ||
			report.consolidatedFigures?.find((v: any) => v.lineId === "assets")
				?.consolidatedAmount !== "24.68" ||
			report.consolidatedFigures?.find((v: any) => v.lineId === "result")
				?.consolidatedAmount !== "24.68" ||
			report.appliedEliminations.length !== 0 ||
			report.sourceSnapshots.length !== 2
		)
			throw Error(
				"Nonempty exact balanced consolidation without fabricated elimination failed",
			);
		const current = await get(author, root + `?asOf=${asOf}`),
			own = current.profiles.find((s: any) => s.profile.id === profileId);
		if (
			own?.actor !== `user:${b.user.id}` ||
			own.profileHash !== proposal.profileHash ||
			own.status !== "approved"
		)
			throw Error("Exact independent approval identity/history missing");
		// Exercise revocation and immutable prior source without replacing another helper’s profile.
		const revocable = {
			...profile,
			id: "synthetic-consolidation-browser-revoked",
		};
		const pending = await ui(author, asOf, "propose", { profile: revocable });
		await ui(reviewer, asOf, "approve", {
			profileId: revocable.id,
			profileHash: pending.profileHash,
		});
		await ui(author, asOf, "revoke", { profileId: revocable.id });
		await post(
			author,
			root + "/revoke",
			{ profileId: revocable.id, confirm: true },
			422,
		);
		evidence = {
			anchor,
			other,
			asOf,
			profileId,
			profileHash: proposal.profileHash,
			current: (await get(author, root + `?asOf=${asOf}`)).profiles.filter(
				(s: any) => s.profile.id.startsWith("synthetic-consolidation-browser-"),
			),
			report,
			journals: await Promise.all(
				[anchor, other].map((s) => get(author, `/api/companies/${s}/journal`)),
			),
		};
	} else {
		evidence = JSON.parse(readFileSync(saved, "utf8"));
		if (evidence.anchor !== anchor || evidence.other !== other)
			throw Error("Consolidation fixture mismatch");
	}
	const query = new URLSearchParams({
		profileId: evidence.profileId,
		from: evidence.asOf,
		asOf: evidence.asOf,
	});
	const actual = await get(author, "/api/group-consolidated-report?" + query);
	if (!isDeepStrictEqual(actual, evidence.report))
		throw Error(
			"Full consolidated figures/provenance changed after canonical reload",
		);
	const current = await get(author, root + `?asOf=${evidence.asOf}`);
	if (
		!isDeepStrictEqual(
			current.profiles.filter((s: any) =>
				s.profile.id.startsWith("synthetic-consolidation-browser-"),
			),
			evidence.current,
		)
	)
		throw Error("Profile exact state/revocation evidence changed");
	const region = await panel(author, evidence.asOf);
	await region
		.getByLabel("Konsolidering rapportprofil", { exact: true })
		.selectOption(evidence.profileId);
	await region
		.getByLabel("Konsolidering fra dato", { exact: true })
		.fill(evidence.asOf);
	const [r] = await Promise.all([
		author.waitForResponse(
			(r) => r.url() === base + "/api/group-consolidated-report?" + query,
		),
		region
			.getByRole("button", { name: "Hent konsolideret rapport", exact: true })
			.click(),
	]);
	if (!r.ok()) throw Error("Actual consolidation UI report refused");
	const row = region.locator('tr[data-line-id="assets"]');
	await row
		.getByRole("cell")
		.nth(2)
		.filter({ hasText: /^24\.68$/ })
		.waitFor();
	if (
		!isDeepStrictEqual(
			await Promise.all(
				[anchor, other].map((s) => get(author, `/api/companies/${s}/journal`)),
			),
			evidence.journals,
		)
	)
		throw Error(
			"Read-only consolidation/profile commands changed legal-entity journals",
		);
	if (restarted) evidence.restartVerified = true;
	mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
	writeFileSync(saved, JSON.stringify(evidence), { mode: 0o600 });
	await region.screenshot({
		path: "/tmp/rentemester-rust-reports/native-consolidation-browser.png",
	});
	console.log(
		`PASS: actual nonempty balanced consolidated report, explicit profile→independent exact approval, revocation, exact serverMoney/provenance and unchanged journals${restarted ? " survive restart" : ""}`,
	);
}
