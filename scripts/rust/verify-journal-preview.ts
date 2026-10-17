/** Actual source-backed native CLI, fixed engine expectations and unchanged TS validator. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
	copyFileSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	readdirSync,
	rmSync,
	symlinkSync,
	writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import type { Database } from "bun:sqlite";
import { migrate, openDb } from "../../src/core/db";
import { ensureCompanyDirs } from "../../src/core/paths";
import { validateJournalEntry } from "../../src/core/ledger";
import { currentRuleBundleVersion } from "../../src/core/rules-metadata";
const root = resolve(import.meta.dir, "../.."),
	dir = join(
		root,
		"docs/architecture/rust-target/implementation/journal-preview",
	),
	cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const hash = (s: string | Buffer) =>
	createHash("sha256").update(s).digest("hex");
const manifest = JSON.parse(
	readFileSync(join(dir, "source-cases-manifest.json"), "utf8"),
);
const manualText = readFileSync(
	join(dir, "../manual-journal/vectors.jsonl"),
	"utf8",
);
assert.equal(hash(manualText), manifest.manual_vectors_sha256);
assert.equal(
	hash(readFileSync(join(root, "scripts/rust/journal-preview-cases.py"))),
	manifest.generator_sha256,
);
assert.equal(
	hash(readFileSync(join(dir, "source-cases.json"))),
	manifest.cases_sha256,
);
const fixed = manualText
	.trimEnd()
	.split("\n")
	.map((s) => JSON.parse(s))
	.filter((v) => v.op === "manual" && v.legacy !== "native_only");
const priorObservations = new Map(
	readFileSync(join(dir, "../manual-journal/legacy-observations.jsonl"), "utf8")
		.trimEnd()
		.split("\n")
		.map((s) => {
			const v = JSON.parse(s);
			return [v.id, v.legacy];
		}),
);
const extra = JSON.parse(readFileSync(join(dir, "source-cases.json"), "utf8"));
assert.equal(extra.length, manifest.cases);
const build = Bun.spawnSync(
	[cargo, "build", "--locked", "-p", "rentemester-cli"],
	{ cwd: root, stdout: "pipe", stderr: "pipe" },
);
assert.equal(build.exitCode, 0, build.stderr.toString());
const binary = join(root, "target/debug/rentemester-rust"),
	temp = mkdtempSync(join(tmpdir(), "rm-preview-")),
	nativeTemp = join(temp, "private");
mkdirSync(nativeTemp, { mode: 0o700 });
const seedRoot = join(temp, "base"),
	seedPaths = ensureCompanyDirs(seedRoot),
	seedDb = openDb(seedPaths.db, { journalMode: "DELETE" });
migrate(seedDb);
seedDb.close();
const quote = (s: string) => `"${s.replaceAll('"', '""')}"`;
const schema = (db: Database) =>
	JSON.stringify(
		db
			.query(
				"SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY name",
			)
			.all(),
	);
function seed(db: Database, f: any, sql: any[]) {
	const triggers = db
		.query(
			"SELECT name,sql FROM sqlite_schema WHERE type='trigger' ORDER BY name",
		)
		.all() as Array<{ name: string; sql: string }>;
	const ddl = schema(db);
	for (const t of triggers) db.exec(`DROP TRIGGER ${quote(t.name)}`);
	for (const a of f.accounts) {
		db.query(
			"INSERT INTO accounts(account_no,name,type,normal_balance,active) VALUES(?,?,?,?,?)",
		).run(
			a.accountNo,
			"Synthetic",
			a.kind,
			a.kind === "asset" || a.kind === "expense" ? "debit" : "credit",
			a.active ? 1 : 0,
		);
		if (a.vatSettlement)
			db.query(
				"INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('vat_settlement',?,1,'user:synthetic')",
			).run(a.accountNo);
	}
	const entries = new Set<string>();
	function entry(id: string, name: string, documentId: string | null = null) {
		if (entries.has(id)) return;
		entries.add(id);
		db.query(
			"INSERT INTO journal_entries(id,entry_no,transaction_date,text,document_id,rule_version,entry_hash) VALUES(?,?,'2026-01-01','Synthetic',?,?,?)",
		).run(
			BigInt(id),
			name,
			documentId === null ? null : BigInt(documentId),
			currentRuleBundleVersion(),
			"0".repeat(64),
		);
	}
	for (const d of f.documents) {
		db.query(
			"INSERT INTO documents(id,source,sha256_hash,document_type) VALUES(?,'synthetic',?,?)",
		).run(
			BigInt(d.id),
			hash(d.id),
			d.existingCreditNoteEntry === null ? "purchase_sale" : "credit_note",
		);
		if (d.existingCreditNoteEntry !== null)
			entry("100", d.existingCreditNoteEntry, d.id);
	}
	for (const b of f.banks) {
		db.query(
			"INSERT INTO bank_transactions(id,transaction_date,text,amount) VALUES(?,'2026-01-01','Synthetic',10)",
		).run(BigInt(b.id));
		if (b.linkedJournal !== null) {
			entry(b.linkedJournal, "linked-" + b.linkedJournal);
			db.query(
				"INSERT INTO bank_journal_reconciliation_links(bank_transaction_id,journal_entry_id,match_method,created_by) VALUES(?,?,'manual-review','user:synthetic')",
			).run(BigInt(b.id), BigInt(b.linkedJournal));
		}
	}
	for (const p of f.periods) {
		db.query(
			"INSERT INTO accounting_periods(id,period_start,period_end,kind,status,reference) VALUES(?,?,?,?,?,?)",
		).run(
			BigInt(p.id),
			p.start,
			p.end,
			p.kind,
			p.effectiveState === "reported" ? "reported" : "closed",
			p.reference,
		);
		if (p.effectiveState === "open" || p.effectiveState === "reported")
			db.query(
				"INSERT INTO audit_log(event_type,entity_type,entity_id,message) VALUES('period_reopen','accounting_period',?,'Synthetic lifecycle')",
			).run(p.id);
	}
	for (const s of sql)
		db.query(s.sql).run(
			...s.params.map((p: any) =>
				p && typeof p === "object" && "i64" in p ? BigInt(p.i64) : p,
			),
		);
	for (const t of triggers) db.exec(t.sql);
	if (!sql.length)
		assert.equal(schema(db), ddl, "synthetic guards/schema restored");
}
function inventory(root: string) {
	return Object.fromEntries(
		readdirSync(root, { recursive: true, withFileTypes: true })
			.filter((e) => e.isFile())
			.map((e) => {
				const p = join(e.parentPath, e.name);
				return [p.slice(root.length + 1), hash(readFileSync(p))];
			})
			.sort(([a], [b]) => a.localeCompare(b)),
	);
}
function invoke(
	company: string,
	request: unknown,
	args?: string[],
	input?: Buffer,
) {
	const run = Bun.spawnSync(
		[binary, ...(args ?? ["validate-journal", "--company", company])],
		{
			cwd: temp,
			stdin: input ?? Buffer.from(JSON.stringify(request)),
			env: {
				...process.env,
				TMPDIR: nativeTemp,
				RENTEMESTER_TODAY: "invalid-clock-override",
				RENTEMESTER_MAX_FUTURE_DAYS: "invalid-future-days",
				TZ: "Pacific/Honolulu",
			},
			stdout: "pipe",
			stderr: "pipe",
			timeout: 30000,
		},
	);
	assert.equal(run.stderr.toString(), "");
	assert.equal(run.stdout.toString().trim().split("\n").length, 1);
	assert.deepEqual(
		readdirSync(nativeTemp),
		[],
		"private preview directories must be removed before response",
	);
	return { exit: run.exitCode, body: JSON.parse(run.stdout.toString()) };
}
const envelope = (request: any, horizon = "2026-03-18") => ({
	format: "rentemester-manual-journal-request/1",
	expectedRuleVersion: currentRuleBundleVersion(),
	latestAllowedDate: horizon,
	request,
});
function project(v: any) {
	const copy = structuredClone(v);
	for (const e of copy.errors) delete e.message;
	return copy;
}
const observations: any[] = [];
let checks = 0;
let sourceCases = 0;
const priorToday = process.env.RENTEMESTER_TODAY,
	priorFuture = process.env.RENTEMESTER_MAX_FUTURE_DAYS;
try {
	for (const v of [...fixed, ...extra]) {
		const company = join(temp, `case-${sourceCases}`),
			paths = ensureCompanyDirs(company);
		copyFileSync(seedPaths.db, paths.db);
		const db = openDb(paths.db);
		try {
			db.exec("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE");
			seed(db, v.facts, v.sql ?? []);
			db.exec("COMMIT; PRAGMA foreign_keys=ON");
			const image = db.serialize(),
				changes = db.query("SELECT total_changes() AS n").get();
			let legacy: any = null;
			if (!v.expected.error) {
				process.env.RENTEMESTER_TODAY = v.facts.latestAllowedDate;
				process.env.RENTEMESTER_MAX_FUTURE_DAYS = "0";
				const payload = structuredClone(v.request);
				for (const k of [
					"documentId",
					"sourceBankTransactionId",
					"amountForeign",
					"amountDkk",
					"fxRateToDkk",
				])
					if (payload[k] !== undefined && payload[k] !== null)
						payload[k] = Number(payload[k]);
				for (const l of payload.lines)
					for (const k of ["debitAmount", "creditAmount"])
						if (l[k] !== undefined) l[k] = Number(l[k]);
				legacy = validateJournalEntry(db, payload);
				if (v.sql === undefined)
					assert.deepEqual(
						legacy,
						priorObservations.get(v.id),
						`${v.id}: actual TS source drifted`,
					);
				else {
					const codes = legacy.errors.map((m: string) =>
						m.startsWith("manual journal entries")
							? "vat_base_required"
							: m.includes("inactive account")
								? "account_inactive"
								: m.includes("falls in")
									? "period_locked"
									: m.startsWith("credit note document")
										? "credit_note_already_posted"
										: m.includes("already reconciled")
											? "bank_reconciled"
											: m,
					);
					assert.deepEqual(
						codes,
						v.legacyCodes,
						`${v.id}: actual legacy source classification`,
					);
					assert.equal(legacy.ok, v.legacyCodes.length === 0);
				}
			}
			assert.deepEqual(
				db.serialize(),
				image,
				`${v.id}: reference changed logical source`,
			);
			assert.deepEqual(db.query("SELECT total_changes() AS n").get(), changes);
			// Source SQLite observers may update SHM read marks; native must preserve every byte after observer setup.
			const before = inventory(company);
			const request = envelope(v.request, v.facts.latestAllowedDate),
				first = invoke(company, request),
				second = invoke(company, request);
			assert.deepEqual(first, second, `${v.id}: deterministic repeat`);
			if (v.expected.error) {
				assert.equal(first.exit, 1);
				assert.equal(first.body.code, v.expected.error, v.id);
			} else {
				assert.equal(first.exit, v.expected.ok ? 0 : 1, v.id);
				assert.equal(first.body.ok, v.expected.ok);
				assert.equal(first.body.format, "rentemester-manual-journal-preview/1");
				assert.equal(first.body.data.advisory, true);
				assert.equal(first.body.data.clockPolicy, "explicit-preview-horizon");
				assert.equal(
					first.body.data.latestAllowedDate,
					v.facts.latestAllowedDate,
				);
				assert.equal(
					first.body.data.ruleArtifactSha256,
					hash(
						readFileSync(
							join(
								root,
								"crates/rentemester-storage/src/manual-rule-bundles.json",
							),
						),
					),
				);
				assert.deepEqual(project(first.body.data.validation), v.expected, v.id);
				for (const message of v.expectedMessages ?? [])
					assert.ok(
						first.body.data.validation.errors.some(
							(e: any) => e.message === message,
						),
						`${v.id}: selected blocker/earliest entry`,
					);
			}
			assert.deepEqual(
				inventory(company),
				before,
				`${v.id}: source files changed`,
			);
			assert.deepEqual(db.serialize(), image);
			assert.deepEqual(db.query("SELECT total_changes() AS n").get(), changes);
			observations.push({ id: v.id, legacy, native: first });
			checks++;
			sourceCases++;
		} finally {
			db.close();
			rmSync(company, { recursive: true, force: true });
		}
	}
	const company = join(temp, "transport"),
		paths = ensureCompanyDirs(company);
	copyFileSync(seedPaths.db, paths.db);
	const db = openDb(paths.db);
	db.exec("BEGIN IMMEDIATE");
	seed(db, fixed[0].facts, []);
	db.exec("COMMIT");
	db.close();
	const before = inventory(company),
		request = envelope(fixed[0].request);
	const failure = (
		id: string,
		bytes: Buffer | undefined,
		expectedExit: number,
		code: string,
		args?: string[],
		body: unknown = request,
	) => {
		const r = invoke(company, body, args, bytes);
		assert.equal(r.exit, expectedExit, id);
		assert.equal(r.body.ok, false, id);
		assert.equal(r.body.code, code, id);
		assert.deepEqual(inventory(company), before, id);
		observations.push({ id, native: r });
		checks++;
	};
	failure("empty-input", Buffer.alloc(0), 2, "invalid_input");
	failure("invalid-utf8", Buffer.from([255]), 2, "invalid_input");
	failure(
		"trailing-json",
		Buffer.from(JSON.stringify(request) + "{}"),
		2,
		"invalid_input",
	);
	failure("over-budget", Buffer.alloc(1048577, 32), 2, "invalid_input");
	for (const [id, body] of [
		["wrong-format", { ...request, format: "wrong" }],
		["client-facts", { ...request, facts: {} }],
		["client-rules", { ...request, bundles: [] }],
		[
			"numeric-money",
			{
				...request,
				request: {
					...request.request,
					lines: [{ accountNo: "cash", debitAmount: 10 }],
				},
			},
		],
		[
			"numeric-id",
			{ ...request, request: { ...request.request, documentId: 1 } },
		],
		["bad-date", { ...request, latestAllowedDate: " 2026-03-18" }],
		[
			"unknown-request-field",
			{ ...request, request: { ...request.request, historicalImport: true } },
		],
	] as const)
		failure(id, undefined, 2, "invalid_input", undefined, body);
	failure(
		"stale-rule-version",
		undefined,
		1,
		"rule_version_mismatch",
		undefined,
		{ ...request, expectedRuleVersion: "old" },
	);
	failure("bad-args", undefined, 2, "invalid_input", [
		"validate-journal",
		"--company",
		company,
		"--facts",
		"ignored",
	]);
	const exact = structuredClone(request);
	exact.request.text = "x".repeat(
		1048576 -
			Buffer.byteLength(JSON.stringify(exact)) +
			Buffer.byteLength(exact.request.text),
	);
	const exactBytes = Buffer.from(JSON.stringify(exact));
	assert.equal(exactBytes.length, 1048576);
	const admitted = invoke(company, exact, undefined, exactBytes);
	assert.equal(admitted.exit, 0);
	assert.deepEqual(inventory(company), before);
	observations.push({ id: "exact-byte-budget", native: admitted });
	checks++;
	const help = invoke("", {}, ["validate-journal", "--help"]);
	assert.equal(help.exit, 0);
	assert.equal(help.body.activeRuleVersion, currentRuleBundleVersion());
	assert.equal(help.body.maxRequestBytes, 1048576);
	assert.equal(help.body.requestFormat, request.format);
	observations.push({ id: "help", native: help });
	checks++;
	const missing = join(temp, "missing");
	const miss = invoke(missing, request);
	assert.equal(miss.exit, 1);
	assert.equal(miss.body.code, "source_unavailable");
	assert.equal(existsSync(missing), false);
	checks++;
	const alias = join(temp, "alias");
	symlinkSync(company, alias);
	const linked = invoke(alias, request);
	assert.equal(linked.exit, 1);
	assert.equal(linked.body.code, "source_unavailable");
	assert.deepEqual(inventory(company), before);
	checks++;
	const sidecarPaths = ensureCompanyDirs(join(temp, "sidecar-source"));
	copyFileSync(seedPaths.db, sidecarPaths.db);
	const sidecar = join(sidecarPaths.data, "ledger.sqlite-wal"),
		target = join(temp, "protected");
	writeFileSync(target, "protected");
	symlinkSync(target, sidecar);
	const sideBefore = hash(readFileSync(sidecarPaths.db));
	const side = invoke(sidecarPaths.root, request);
	assert.equal(side.exit, 1);
	assert.equal(side.body.code, "source_unavailable");
	assert.equal(readFileSync(target, "utf8"), "protected");
	assert.equal(hash(readFileSync(sidecarPaths.db)), sideBefore);
	checks++;
	const frozen = observations.map((v) => JSON.stringify(v)).join("\n") + "\n",
		path = join(dir, "observations.jsonl");
	if (process.argv.includes("--capture")) {
		assert.ok(!existsSync(path), "preview observations already frozen");
		writeFileSync(path, frozen);
	} else
		assert.equal(
			readFileSync(path, "utf8"),
			frozen,
			"source-backed preview observations changed;investigate explicitly",
		);
	console.log(
		JSON.stringify(
			{
				scope:
					"source-backed advisory journal preview:real native CLI and actual legacy validator",
				checks,
				source_cases: sourceCases,
				fixed_manual_cases: fixed.length,
				new_source_cases: extra.length,
				corpus_sha256: manifest.cases_sha256,
				observations_sha256: hash(frozen),
				passed: [
					"fixed independent exact expectations on actual current-schema source captures",
					"actual unchanged TS validation and exact source/native observations",
					"exact IDs,VAT roles/source chart,lifecycle/credit ordering and canonical fact refusals",
					"strict real stdin/size/formats/exits/help;source and sidecar refusal",
					"source files/database image/total_changes preserved;repeat stable;explicit private cleanup",
				],
				platform: process.platform,
				architecture: process.arch,
				limits: [
					"Advisory explicit horizon only;no clock/configured posting policy or writer authority",
					"Current full canonical schemas only;no migration/adoption or historical archive qualification",
					"Compiled manual policy metadata/source hashes,not generic YAML/legal-rule execution",
					"Lifecycle replay is not audit-chain/source-evidence integrity certification",
					"No authenticated API/MCP/UI,actual posting/dry-run insertion,full R09/R12/R14/R17 acceptance,distribution or cutover",
				],
			},
			null,
			2,
		),
	);
} finally {
	if (priorToday === undefined) delete process.env.RENTEMESTER_TODAY;
	else process.env.RENTEMESTER_TODAY = priorToday;
	if (priorFuture === undefined) delete process.env.RENTEMESTER_MAX_FUTURE_DAYS;
	else process.env.RENTEMESTER_MAX_FUTURE_DAYS = priorFuture;
	rmSync(temp, { recursive: true, force: true });
}
