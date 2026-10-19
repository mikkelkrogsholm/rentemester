import type { Page } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { isDeepStrictEqual } from "node:util";
const saved = "/tmp/rentemester-rust-reports/native-intercompany.json";
type Evidence = {
	anchor: string;
	other: string;
	current: unknown;
	report: unknown;
	before: unknown[];
	asOf: string;
	restartVerified?: boolean;
};
/** Two real independently verified/MFA owners; launcher provisions/grants these dedicated companies through native APIs. */
export async function checkNativeIntercompany(
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
		![anchor, other].every((s) => /^synthetic-intercompany-[a-z0-9-]+$/.test(s))
	)
		throw Error("Dedicated owned two-company synthetic fixture required");
	const headers = { origin: base, "content-type": "application/json" },
		root = `/api/companies/${anchor}/intercompany`;
	async function get(page: Page, path: string) {
		const r = await page.request.get(base + path);
		if (!r.ok()) throw Error(`Intercompany read ${path}: HTTP${r.status()}`);
		return r.json();
	}
	async function post(page: Page, path: string, data: unknown, status = 200) {
		const r = await page.request.post(base + path, { headers, data });
		if (r.status() !== status)
			throw Error(`Intercompany ${path}: expected${status} got${r.status()}`);
		return r.json();
	}
	const maker = await get(author, "/api/me"),
		checker = await get(reviewer, "/api/me");
	if (!maker.user?.id || !checker.user?.id || maker.user.id === checker.user.id)
		throw Error("Independent current ordinary MFA users required");
	let evidence: Evidence;
	async function ui(page: Page, action: string, body: unknown) {
		await page.goto(`${base}/koncernstruktur`);
		await page
			.getByLabel("Intercompany virksomhed", { exact: true })
			.selectOption(anchor);
		await page
			.getByLabel("Intercompany handling", { exact: true })
			.selectOption(action);
		await page
			.getByLabel("Intercompany JSON", { exact: true })
			.fill(JSON.stringify(body));
		const button = page.getByRole("button", {
			name: "Registrér intercompany-handling",
			exact: true,
		});
		if (!(await button.isDisabled()))
			throw Error("Mutation available without explicit review");
		await page.getByLabel("Bekræft intercompany", { exact: true }).check();
		const response = page.waitForResponse(
			(r) =>
				r.url() === base + root + "/" + action &&
				r.request().method() === "POST",
		);
		await button.click();
		if (!(await response).ok())
			throw Error(`Intercompany UI ${action} refused`);
	}
	if (!restarted) {
		const asOf = new Date().toLocaleDateString("sv-SE", {
			timeZone: "Europe/Copenhagen",
		});
		for (const slug of [anchor, other]) {
			for (const [accountNo, type, normalBalance] of [
				[
					slug === anchor ? "1851" : "2851",
					slug === anchor ? "asset" : "liability",
					slug === anchor ? "debit" : "credit",
				],
				["8851", "equity", "credit"],
			] as const)
				await post(author, `/api/companies/${slug}/accounts`, {
					accountNo,
					name: `Synthetic intercompany ${accountNo}`,
					type,
					normalBalance,
				});
			const j = (await get(author, `/api/companies/${slug}/journal`)).journal;
			const no = slug === anchor ? "1851" : "2851";
			await post(author, `/api/companies/${slug}/manual-journal`, {
				confirm: true,
				input: {
					format: "rentemester-manual-journal-post/1",
					expectedRuleVersion: j.nativePosting.ruleVersion,
					idempotencyKey: `synthetic-intercompany-${slug}`,
					request: {
						transactionDate: asOf,
						text: "Synthetic exact reciprocal source",
						currency: "DKK",
						lines:
							slug === anchor
								? [
										{
											accountNo: no,
											debitAmount: "12.34",
											creditAmount: "0.00",
										},
										{
											accountNo: "8851",
											debitAmount: "0.00",
											creditAmount: "12.34",
										},
									]
								: [
										{
											accountNo: no,
											debitAmount: "0.00",
											creditAmount: "12.34",
										},
										{
											accountNo: "8851",
											debitAmount: "12.34",
											creditAmount: "0.00",
										},
									],
					},
				},
			});
		}
		const before = await Promise.all(
			[anchor, other].map((s) => get(author, `/api/companies/${s}/journal`)),
		);
		const membership = (companySlug: string, id: string) => ({
			id,
			companySlug,
			validFrom: "2000-01-01",
		});
		const manifest = {
			version: 1,
			groups: [
				{
					id: "synthetic-intercompany",
					name: "Synthetic reciprocal group",
					memberships: [
						membership(anchor, "left-member"),
						membership(other, "right-member"),
					],
					ownership: [],
				},
			],
		};
		// Isolated fixture only: never replaces a pre-existing unrelated manifest.
		const prior = await get(author, root);
		if (prior.manifest !== null)
			throw Error(
				"Intercompany helper requires own initially unconfigured groupmanifest",
			);
		await post(
			author,
			root + "/manifest",
			{ ...manifest, confirm: false },
			400,
		);
		await ui(author, "manifest", manifest);
		const mapping = {
			id: "synthetic-browser-reciprocal",
			groupId: "synthetic-intercompany",
			leftCompanySlug: anchor,
			rightCompanySlug: other,
			leftAccountNos: ["1851"],
			rightAccountNos: ["2851"],
			leftPosition: "receivable",
			rightPosition: "payable",
			evidenceRefs: ["synthetic-reviewed-intercompany-source"],
			validFrom: "2000-01-01",
		};
		await ui(author, "propose", mapping);
		let state = await get(author, root);
		const exact = {
			mappingId: mapping.id,
			mappingHash: state.mappings.find((v: any) => v.mapping.id === mapping.id)
				.mappingHash,
		};
		await post(author, root + "/approve", { ...exact, confirm: true }, 422);
		await post(
			reviewer,
			root + "/approve",
			{ ...exact, mappingHash: "0".repeat(64), confirm: true },
			422,
		);
		await post(
			reviewer,
			root + "/approve",
			{ ...exact, actor: "user:forged", confirm: true },
			400,
		);
		await ui(reviewer, "approve", exact);
		const matched = await get(author, `/api/group-reconciliation?asOf=${asOf}`);
		const row = matched.rows.find((v: any) => v.mappingId === mapping.id);
		if (
			row?.status !== "matched" ||
			row.difference !== "0.00" ||
			row.left.balance !== "12.34" ||
			row.right.balance !== "12.34" ||
			row.left.sourceRefs.length !== 1 ||
			row.right.sourceRefs.length !== 1
		)
			throw Error("Exact reciprocal posted source missing");
		const overlap = { ...mapping, id: "synthetic-browser-overlap" };
		await post(author, root + "/propose", { ...overlap, confirm: true });
		state = await get(author, root);
		await post(
			reviewer,
			root + "/approve",
			{
				mappingId: overlap.id,
				mappingHash: state.mappings.find(
					(v: any) => v.mapping.id === overlap.id,
				).mappingHash,
				confirm: true,
			},
			422,
		);
		await ui(author, "revoke", exact);
		if (
			(await get(author, `/api/group-reconciliation?asOf=${asOf}`)).rows.length
		)
			throw Error("Revoked mapping still reconciled");
		// Explicit new approved lifecycle restores the final visible row without editing prior evidence.
		await ui(author, "propose", mapping);
		await ui(reviewer, "approve", exact);
		await post(author, "/api/workspace/members/company", {
			userId: checker.user.id,
			companySlug: other,
			action: "revoke",
		});
		const hidden = await get(reviewer, root);
		if (
			hidden.manifest !== null ||
			hidden.mappings.length ||
			(await get(reviewer, `/api/group-reconciliation?asOf=${asOf}`)).rows
				.length
		)
			throw Error("Hidden endpoint identity or totals disclosed");
		await post(reviewer, root + "/revoke", { ...exact, confirm: true }, 403);
		await post(author, "/api/workspace/members/company", {
			userId: checker.user.id,
			companySlug: other,
			action: "grant",
			role: "owner",
		});
		evidence = {
			anchor,
			other,
			asOf,
			before,
			current: await get(author, root),
			report: await get(author, `/api/group-reconciliation?asOf=${asOf}`),
		};
	} else evidence = JSON.parse(readFileSync(saved, "utf8"));
	if (evidence.anchor !== anchor || evidence.other !== other)
		throw Error("Wrong intercompany fixture evidence");
	if (
		!isDeepStrictEqual(await get(author, root), evidence.current) ||
		!isDeepStrictEqual(
			await get(author, `/api/group-reconciliation?asOf=${evidence.asOf}`),
			evidence.report,
		)
	)
		throw Error("Intercompany canonical structure/hash/source changed");
	for (const [i, slug] of [anchor, other].entries())
		if (
			!isDeepStrictEqual(
				await get(author, `/api/companies/${slug}/journal`),
				evidence.before[i],
			)
		)
			throw Error("Intercompany action changed accounting");
	await author.goto(`${base}/koncernstruktur`);
	await author
		.getByLabel("Afstemning pr. dato", { exact: true })
		.fill(evidence.asOf);
	const card = author
		.getByRole("article")
		.filter({ hasText: "synthetic-browser-reciprocal" });
	await card.getByText("Eksakt difference: 0.00", { exact: true }).waitFor();
	await card.getByText("Posteringer og kildebeviser", { exact: true }).click();
	await card.getByText("12.34", { exact: false }).first().waitFor();
	if (restarted) evidence.restartVerified = true;
	mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
	writeFileSync(saved, JSON.stringify(evidence), { mode: 0o600 });
	console.log(
		`PASS: native manifest + exact reciprocal mapping four-eyes/hash/overlap/privacy/revoke and no accounting effects${restarted ? " survive normal-MFA restart" : ""}`,
	);
}
