import type { Page } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { isDeepStrictEqual } from "node:util";
const saved = "/tmp/rentemester-rust-reports/native-eliminations.json";
type Input = {
	id: string;
	mappingId: string;
	asOf: string;
	evidenceRefs: string[];
};
type Evidence = {
	anchor: string;
	other: string;
	asOf: string;
	current: unknown;
	applied: unknown;
	journals: unknown[];
	bothOrientations: true;
	restartVerified?: boolean;
};
/** Launcher supplies two normal independently verified/MFA workspace owners and two fresh, explicitly granted companies. */
export async function checkNativeEliminations(
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
		![anchor, other].every((s) => /^synthetic-eliminations-[a-z0-9-]+$/.test(s))
	)
		throw Error("Dedicated synthetic elimination fixture required");
	const headers = { origin: base, "content-type": "application/json" },
		root = `/api/companies/${anchor}/eliminations`,
		intercompany = `/api/companies/${anchor}/intercompany`;
	async function get(page: Page, path: string) {
		const r = await page.request.get(base + path);
		if (!r.ok()) throw Error(`Elimination read ${path}: HTTP${r.status()}`);
		return r.json();
	}
	async function post(page: Page, path: string, data: unknown, status = 200) {
		const r = await page.request.post(base + path, { headers, data });
		if (r.status() !== status)
			throw Error(`Elimination ${path}: expected${status}, got${r.status()}`);
		return r.json();
	}
	const maker = await get(author, "/api/me"),
		checker = await get(reviewer, "/api/me");
	if (!maker.user?.id || !checker.user?.id || maker.user.id === checker.user.id)
		throw Error("Independent current ordinary MFA principals required");
	async function open(page: Page) {
		await page.goto(`${base}/koncernstruktur`);
		await page
			.getByLabel("Intercompany virksomhed", { exact: true })
			.selectOption(anchor);
		return page.getByRole("region", {
			name: "Balanceelimineringer",
			exact: true,
		});
	}
	async function planUI(i: Input) {
		const p = await open(author);
		await author
			.getByLabel("Afstemning pr. dato", { exact: true })
			.fill(i.asOf);
		await p.getByLabel("Eliminerings-id", { exact: true }).fill(i.id);
		await p
			.getByLabel("Eliminering mapping", { exact: true })
			.selectOption(i.mappingId);
		await p
			.getByLabel("Eliminering kildereferencer", { exact: true })
			.fill(i.evidenceRefs.join("\n"));
		const response = author.waitForResponse(
			(r) =>
				r.url() === base + root + "/plan" && r.request().method() === "POST",
		);
		await p
			.getByRole("button", { name: "Vis elimineringsplan", exact: true })
			.click();
		const r = await response;
		if (!r.ok()) throw Error("Readonly reviewed plan refused");
		return { panel: p, plan: (await r.json()).plan };
	}
	async function proposeUI(i: Input) {
		const { panel, plan } = await planUI(i);
		const button = panel.getByRole("button", {
			name: "Foreslå eliminering",
			exact: true,
		});
		if (!(await button.isDisabled()))
			throw Error("Proposal available without explicit review");
		await panel
			.getByLabel("Bekræft elimineringsforslag", { exact: true })
			.check();
		const done = author.waitForResponse(
			(r) =>
				r.url() === base + root + "/propose" && r.request().method() === "POST",
		);
		await button.click();
		if (!(await done).ok()) throw Error("Reviewed proposal refused");
		return plan;
	}
	async function actUI(
		page: Page,
		id: string,
		action: "approve" | "apply" | "reverse" | "reject",
	) {
		const p = await open(page);
		await p.getByLabel("Vælg eliminering", { exact: true }).selectOption(id);
		await p
			.getByLabel("Bekræft elimineringsbeslutning", { exact: true })
			.check();
		const names = {
			approve: "Godkend eliminering",
			apply: "Anvend eliminering",
			reverse: "Tilbagefør eliminering",
			reject: "Afvis eliminering",
		};
		const done = page.waitForResponse(
			(r) =>
				r.url() === base + root + "/" + action &&
				r.request().method() === "POST",
		);
		await p.getByRole("button", { name: names[action], exact: true }).click();
		if (!(await done).ok())
			throw Error(`Reviewed elimination ${action} refused`);
	}
	async function journal(
		slug: string,
		lines: unknown,
		key: string,
		asOf: string,
	) {
		const j = (await get(author, `/api/companies/${slug}/journal`)).journal;
		await post(author, `/api/companies/${slug}/manual-journal`, {
			confirm: true,
			input: {
				format: "rentemester-manual-journal-post/1",
				expectedRuleVersion: j.nativePosting.ruleVersion,
				idempotencyKey: key,
				request: {
					transactionDate: asOf,
					text: "Synthetic explicit elimination source fixture",
					currency: "DKK",
					lines,
				},
			},
		});
	}
	let evidence: Evidence;
	if (!restarted) {
		const asOf = new Date().toLocaleDateString("sv-SE", {
			timeZone: "Europe/Copenhagen",
		});
		for (const slug of [anchor, other]) {
			const accounts =
				slug === anchor
					? [
							["1861", "asset", "debit"],
							["2862", "liability", "credit"],
							["1869", "asset", "debit"],
							["8861", "equity", "credit"],
						]
					: [
							["2861", "liability", "credit"],
							["1862", "asset", "debit"],
							["8861", "equity", "credit"],
						];
			for (const [accountNo, type, normalBalance] of accounts)
				await post(author, `/api/companies/${slug}/accounts`, {
					accountNo,
					name: `Synthetic elimination ${accountNo}`,
					type,
					normalBalance,
				});
		}
		await journal(
			anchor,
			[
				{ accountNo: "1861", debitAmount: "12.34" },
				{ accountNo: "8861", creditAmount: "12.34" },
			],
			"synthetic-elimination-forward-left",
			asOf,
		);
		await journal(
			other,
			[
				{ accountNo: "8861", debitAmount: "12.34" },
				{ accountNo: "2861", creditAmount: "12.34" },
			],
			"synthetic-elimination-forward-right",
			asOf,
		);
		await journal(
			anchor,
			[
				{ accountNo: "8861", debitAmount: "56.78" },
				{ accountNo: "2862", creditAmount: "56.78" },
			],
			"synthetic-elimination-reverse-left",
			asOf,
		);
		await journal(
			other,
			[
				{ accountNo: "1862", debitAmount: "56.78" },
				{ accountNo: "8861", creditAmount: "56.78" },
			],
			"synthetic-elimination-reverse-right",
			asOf,
		);
		if ((await get(author, intercompany)).manifest !== null)
			throw Error("Own fixture requires initially unconfigured manifest");
		await post(author, intercompany + "/manifest", {
			version: 1,
			groups: [
				{
					id: "synthetic-eliminations",
					name: "Synthetic elimination group",
					memberships: [
						{ id: "left-member", companySlug: anchor, validFrom: "2000-01-01" },
						{ id: "right-member", companySlug: other, validFrom: "2000-01-01" },
					],
					ownership: [
						{
							id: "synthetic-explicit-ownership",
							parentCompanySlug: anchor,
							childCompanySlug: other,
							basisPoints: 10000,
							evidenceRefs: ["synthetic-explicit-reviewed-ownership"],
							validFrom: "2000-01-01",
						},
					],
				},
			],
			confirm: true,
		});
		for (const reverse of [false, true]) {
			const m = {
				id: reverse
					? "synthetic-elimination-reverse"
					: "synthetic-elimination-forward",
				groupId: "synthetic-eliminations",
				leftCompanySlug: anchor,
				rightCompanySlug: other,
				leftAccountNos: [reverse ? "2862" : "1861"],
				rightAccountNos: [reverse ? "1862" : "2861"],
				leftPosition: reverse ? "payable" : "receivable",
				rightPosition: reverse ? "receivable" : "payable",
				evidenceRefs: ["synthetic-explicit-reviewed-source"],
				validFrom: "2000-01-01",
			};
			const p = await post(author, intercompany + "/propose", {
				...m,
				confirm: true,
			});
			await post(reviewer, intercompany + "/approve", {
				mappingId: m.id,
				mappingHash: p.mappingHash,
				confirm: true,
			});
		}
		const stale: Input = {
			id: "synthetic-stale-plan",
			mappingId: "synthetic-elimination-forward",
			asOf,
			evidenceRefs: ["synthetic-source"],
		};
		const before = await get(author, root),
			old = (await post(author, root + "/plan", stale)).plan;
		if (!isDeepStrictEqual(before, await get(author, root)))
			throw Error("Readonly plan changed lifecycle");
		await journal(
			anchor,
			[
				{ accountNo: "1869", debitAmount: "1.00" },
				{ accountNo: "8861", creditAmount: "1.00" },
			],
			"synthetic-elimination-later-source-head",
			asOf,
		);
		await post(
			author,
			root + "/propose",
			{ ...stale, expectedPayloadHash: old.payloadHash, confirm: true },
			422,
		);
		if (!isDeepStrictEqual(before, await get(author, root)))
			throw Error("Stale plan created evidence");
		const journals = await Promise.all(
			[anchor, other].map((s) => get(author, `/api/companies/${s}/journal`)),
		);
		for (const reverse of [false, true]) {
			const i: Input = {
				id: reverse ? "synthetic-applied-reverse" : "synthetic-applied-forward",
				mappingId: reverse
					? "synthetic-elimination-reverse"
					: "synthetic-elimination-forward",
				asOf,
				evidenceRefs: ["synthetic-source"],
			};
			const plan = await proposeUI(i);
			if (
				plan.payload.version !== 2 ||
				plan.payload.left.companySlug !== anchor ||
				plan.payload.right.companySlug !== other ||
				plan.payload.left.debitOre !== (reverse ? "5678" : "0") ||
				plan.payload.left.creditOre !== (reverse ? "0" : "1234") ||
				plan.payload.right.debitOre !== (reverse ? "0" : "1234") ||
				plan.payload.right.creditOre !== (reverse ? "5678" : "0")
			)
				throw Error("V2 polarity or actual company identity wrong");
			const exact = {
				eliminationId: i.id,
				payloadHash: plan.payloadHash,
				confirm: true,
			};
			await post(author, root + "/approve", exact, 422);
			await post(reviewer, root + "/apply", exact, 422);
			await post(
				reviewer,
				root + "/approve",
				{ ...exact, actor: "user:forged" },
				400,
			);
			await actUI(reviewer, i.id, "approve");
			await post(author, root + "/apply", exact, 422);
			await actUI(reviewer, i.id, "apply");
			if (!reverse) {
				await actUI(author, i.id, "reverse");
				const applied = await get(
					author,
					`/api/group-eliminations?asOf=${asOf}`,
				);
				if (applied.rows.some((r: any) => r.eliminationId === i.id))
					throw Error("Reversed evidence still applied");
				await proposeUI(i);
				await actUI(reviewer, i.id, "approve");
				await actUI(reviewer, i.id, "apply");
			}
			const duplicate = {
				...i,
				id: reverse
					? "synthetic-duplicate-reverse"
					: "synthetic-duplicate-forward",
			};
			const dup = (await post(author, root + "/plan", duplicate)).plan;
			await post(author, root + "/propose", {
				...duplicate,
				expectedPayloadHash: dup.payloadHash,
				confirm: true,
			});
			await post(reviewer, root + "/approve", {
				eliminationId: duplicate.id,
				payloadHash: dup.payloadHash,
				confirm: true,
			});
			await post(
				reviewer,
				root + "/apply",
				{
					eliminationId: duplicate.id,
					payloadHash: dup.payloadHash,
					confirm: true,
				},
				422,
			);
		}
		const current = await get(author, root);
		const applied = await get(author, `/api/group-eliminations?asOf=${asOf}`);
		if (
			applied.rows.length !== 2 ||
			applied.rows.some((r: any) => r.payload.version !== 2)
		)
			throw Error("Both exact applied orientations missing or duplicated");
		const wrongDate = asOf === "2026-01-01" ? "2026-01-02" : "2000-01-01";
		if (
			(await get(author, `/api/group-eliminations?asOf=${wrongDate}`)).rows
				.length
		)
			throw Error("Applied evidence incorrectly carried forward");
		await post(author, "/api/workspace/members/company", {
			userId: checker.user.id,
			companySlug: other,
			action: "revoke",
		});
		if (
			(await get(reviewer, root)).rows.length ||
			(await get(reviewer, `/api/group-eliminations?asOf=${asOf}`)).rows.length
		)
			throw Error("Hidden company elimination evidence leaked");
		const own = applied.rows[0];
		await post(
			reviewer,
			root + "/reverse",
			{
				eliminationId: own.eliminationId,
				payloadHash: own.payloadHash,
				confirm: true,
			},
			403,
		);
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
			current,
			applied,
			journals,
			bothOrientations: true,
		};
	} else evidence = JSON.parse(readFileSync(saved, "utf8"));
	if (
		evidence.anchor !== anchor ||
		evidence.other !== other ||
		!isDeepStrictEqual(await get(author, root), evidence.current) ||
		!isDeepStrictEqual(
			await get(author, `/api/group-eliminations?asOf=${evidence.asOf}`),
			evidence.applied,
		)
	)
		throw Error("Canonical elimination state/source/hash changed");
	for (const [index, slug] of [anchor, other].entries())
		if (
			!isDeepStrictEqual(
				await get(author, `/api/companies/${slug}/journal`),
				evidence.journals[index],
			)
		)
			throw Error("Elimination lifecycle changed company accounting");
	const panel = await open(author);
	await panel
		.getByLabel("Vælg eliminering", { exact: true })
		.selectOption("synthetic-applied-reverse");
	await panel.getByText("Status: Anvendt", { exact: true }).waitFor();
	await panel
		.getByText("Uændret beslutningsgrundlag og beviser", { exact: true })
		.click();
	await panel.getByText("5678", { exact: false }).first().waitFor();
	if (restarted) evidence.restartVerified = true;
	mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
	writeFileSync(saved, JSON.stringify(evidence), { mode: 0o600 });
	console.log(
		`PASS: native reviewed elimination V2 both actual mapping orientations, source-stale/four-eyes/duplicate/privacy refusal, explicit reversal and exact applied date without journal effects${restarted ? " survive normal-MFA restart" : ""}`,
	);
}
