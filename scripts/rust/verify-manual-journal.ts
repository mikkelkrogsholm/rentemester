/** Fixed independent expectations plus actual legacy manual validation on disposable canonical SQLite. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
	existsSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Database } from "bun:sqlite";
import { migrate } from "../../src/core/db";
import { validateJournalEntry } from "../../src/core/ledger";
import {
	currentRuleBundleVersion,
	readRuleBundleMetadata,
} from "../../src/core/rules-metadata";
import { loadVatAccountSemantics } from "../../src/core/vat-account-semantics";
const root = resolve(import.meta.dir, "../.."),
	dir = join(
		root,
		"docs/architecture/rust-target/implementation/manual-journal",
	),
	cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const hash = (s: string | Buffer) =>
	createHash("sha256").update(s).digest("hex");
const input = join(dir, "vectors.jsonl"),
	bundlePath = join(dir, "rule-bundles.json"),
	text = readFileSync(input, "utf8"),
	manifest = JSON.parse(
		readFileSync(join(dir, "vectors-manifest.json"), "utf8"),
	);
assert.equal(hash(text), manifest.vectors_sha256);
assert.equal(
	hash(readFileSync(join(root, "scripts/rust/manual-journal-vectors.py"))),
	manifest.generator_sha256,
);
assert.equal(hash(readFileSync(bundlePath)), manifest.rule_bundles_sha256);
assert.deepEqual(
	JSON.parse(readFileSync(bundlePath, "utf8")),
	readRuleBundleMetadata().map(({ name, version, ruleIds }) => ({
		name,
		version,
		ruleIds,
	})),
);
const cases = text
	.trimEnd()
	.split("\n")
	.map((s) => JSON.parse(s));
assert.equal(cases.length, manifest.checks);
const build = Bun.spawnSync(
	[
		cargo,
		"test",
		"--locked",
		"-p",
		"rentemester-cli",
		"--test",
		"manual_journal_vectors",
		"--no-run",
		"--message-format=json",
	],
	{ cwd: root, stdout: "pipe", stderr: "pipe" },
);
assert.equal(build.exitCode, 0, build.stderr.toString());
const helper = build.stdout
	.toString()
	.split("\n")
	.filter(Boolean)
	.map((s) => JSON.parse(s))
	.find(
		(x) =>
			x.reason === "compiler-artifact" &&
			x.target.name === "manual_journal_vectors" &&
			x.profile.test &&
			x.executable,
	)?.executable;
assert.ok(helper);
const temp = mkdtempSync(join(tmpdir(), "rm-manual-"));
const db = new Database(":memory:");
migrate(db);
const quote = (s: string) => `"${s.replaceAll('"', '""')}"`;
const triggers = db
	.query(
		"SELECT name,sql FROM sqlite_schema WHERE type='trigger' ORDER BY name",
	)
	.all() as Array<{ name: string; sql: string }>;
const ddl = JSON.stringify(
	db
		.query(
			"SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY name",
		)
		.all(),
);
function seed(f: any) {
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
	for (const t of triggers) db.exec(t.sql);
	assert.equal(
		JSON.stringify(
			db
				.query(
					"SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY name",
				)
				.all(),
		),
		ddl,
		"synthetic seeding restored canonical schema/guards",
	);
	const semantics = loadVatAccountSemantics(db);
	for (const a of f.accounts) {
		assert.equal(
			semantics.amountSideByAccountNo.has(a.accountNo),
			a.vatControl,
		);
		assert.equal(
			semantics.settlementAccountNos.has(a.accountNo),
			a.vatSettlement,
		);
	}
}
function violation(message: string) {
	const line = /^lines\[(\d+)\]/.exec(message)?.[0];
	const tests: Array<[boolean, string, string]> = [
		[message === "text is required", "text_required", "text"],
		[
			message.includes("cannot be later than"),
			"future_date",
			"transactionDate",
		],
		[message.includes("falls in"), "period_locked", "transactionDate"],
		[message.startsWith("at least two"), "lines_required", "lines"],
		[
			message.startsWith("createdByProgram"),
			"reserved_program",
			"createdByProgram",
		],
		[
			!!line && message.includes("existing account"),
			"account_missing",
			`${line}.accountNo`,
		],
		[
			!!line && message.includes("not a supported VAT code"),
			"vat_code_unsupported",
			`${line}.vatCode`,
		],
		[
			!!line && message.includes("inactive account"),
			"account_inactive",
			`${line}.accountNo`,
		],
		[
			!!line && message.includes("must not be negative"),
			"negative_amount",
			line ?? "",
		],
		[!!line && message.includes("must have either"), "line_side", line ?? ""],
		[message.startsWith("journal entry must balance"), "unbalanced", "lines"],
		[
			message.startsWith("manual journal entries"),
			"vat_base_required",
			"lines",
		],
		[
			message.startsWith("amountForeign must be positive"),
			"foreign_amount_required",
			"amountForeign",
		],
		[
			message.startsWith("amountDkk must be positive"),
			"dkk_amount_required",
			"amountDkk",
		],
		[
			message.startsWith("fxRateToDkk must be positive"),
			"fx_rate_required",
			"fxRateToDkk",
		],
		[
			message.startsWith("amountDkk must equal"),
			"fx_amount_mismatch",
			"amountDkk",
		],
		[
			message.startsWith("documentId is required"),
			"document_required",
			"documentId",
		],
		[message.startsWith("documentId "), "document_missing", "documentId"],
		[
			message.startsWith("credit note document"),
			"credit_note_already_posted",
			"documentId",
		],
		[
			message.startsWith("sourceBankTransactionId ") &&
				message.includes("does not exist"),
			"bank_missing",
			"sourceBankTransactionId",
		],
		[
			message.startsWith("sourceBankTransactionId ") &&
				message.includes("already reconciled"),
			"bank_reconciled",
			"sourceBankTransactionId",
		],
	];
	const match = tests.find(([yes]) => yes);
	assert.ok(match, `unclassified actual source error: ${message}`);
	return { code: match[1], field: match[2] };
}
const priorToday = process.env.RENTEMESTER_TODAY,
	priorFuture = process.env.RENTEMESTER_MAX_FUTURE_DAYS;
try {
	let native: any[] = [];
	for (const zone of ["UTC", "Pacific/Honolulu"]) {
		const output = join(temp, zone.replaceAll("/", "-") + ".json");
		const run = Bun.spawnSync(
			[helper, "--exact", "vectors", "--ignored", "--nocapture"],
			{
				cwd: root,
				env: {
					...process.env,
					RM_MANUAL_INPUT: input,
					RM_MANUAL_BUNDLES: bundlePath,
					RM_MANUAL_OUTPUT: output,
					TZ: zone,
					RENTEMESTER_TODAY: "invalid-clock-override",
				},
				stdout: "pipe",
				stderr: "pipe",
				timeout: 30000,
			},
		);
		assert.equal(
			run.exitCode,
			0,
			run.stderr.toString() + run.stdout.toString(),
		);
		const values = JSON.parse(readFileSync(output, "utf8"));
		if (native.length) assert.deepEqual(values, native);
		native = values;
	}
	const observations: any[] = [];
	let sourceChecks = 0;
	for (const [index, v] of cases.entries()) {
		if (v.legacy === "native_only") {
			observations.push({
				id: v.id,
				classification: "native_only",
				native: native[index],
			});
			continue;
		}
		db.exec("SAVEPOINT synthetic_case");
		try {
			seed(v.facts);
			process.env.RENTEMESTER_TODAY = v.facts.latestAllowedDate;
			process.env.RENTEMESTER_MAX_FUTURE_DAYS = "0";
			const payload = structuredClone(v.request);
			const bridge: Record<string, string> = {};
			for (const key of [
				"documentId",
				"sourceBankTransactionId",
				"amountForeign",
				"amountDkk",
				"fxRateToDkk",
			]) {
				if (payload[key] !== undefined && payload[key] !== null) {
					payload[key] = Number(payload[key]);
					bridge[key] = String(payload[key]);
				}
			}
			for (const [i, l] of payload.lines.entries())
				for (const key of ["debitAmount", "creditAmount"]) {
					if (l[key] !== undefined) {
						l[key] = Number(l[key]);
						bridge[`lines[${i}].${key}`] = String(l[key]);
					}
				}
			const before = db.serialize(),
				changes = db.query("SELECT total_changes() AS n").get();
			const actual = validateJournalEntry(db, payload);
			assert.deepEqual(
				db.serialize(),
				before,
				`${v.id}: validation changed canonical image`,
			);
			assert.deepEqual(db.query("SELECT total_changes() AS n").get(), changes);
			for (const message of v.expectedMessages ?? [])
				assert.ok(
					actual.errors.includes(message),
					`${v.id}: expected blocker identity/reference`,
				);
			const projected = {
				ok: actual.ok,
				requiredRules: actual.appliedRules,
				errors: actual.errors.map(violation),
			};
			const expected = {
				ok: v.expected.ok,
				requiredRules: v.expected.requiredRules,
				errors: v.expected.errors,
			};
			if (v.legacy === "match") assert.deepEqual(projected, expected, v.id);
			else assert.notDeepEqual(projected, expected, v.id);
			observations.push({
				id: v.id,
				classification: v.legacy,
				bridge,
				legacy: actual,
				native: native[index],
			});
			sourceChecks++;
		} finally {
			db.exec("ROLLBACK TO synthetic_case; RELEASE synthetic_case");
		}
	}
	const frozen = observations.map((v) => JSON.stringify(v)).join("\n") + "\n",
		path = join(dir, "legacy-observations.jsonl");
	if (process.argv.includes("--capture-legacy")) {
		assert.ok(!existsSync(path), "source observations already frozen");
		writeFileSync(path, frozen);
	} else
		assert.equal(
			readFileSync(path, "utf8"),
			frozen,
			"actual source/native observations changed; investigate explicitly",
		);
	console.log(
		JSON.stringify(
			{
				scope:
					"pure manual journal exact invariants and compiled rule-version binding",
				checks: cases.length + 1,
				vectors: cases.length,
				actual_legacy_cases: sourceChecks,
				legacy_policies: manifest.legacy_policies,
				corpus_sha256: manifest.vectors_sha256,
				legacy_observations_sha256: hash(frozen),
				passed: [
					"all native code/field/totals/rule projections match independently fixed expectations",
					"actual unchanged manual validator on canonical migrated synthetic SQLite; exact source/native diagnostics frozen",
					"canonical guards restored before validation; exact image and total_changes preserved",
					"alternate environment native outputs identical; all exact ID/bundle boundaries checked",
				],
				platform: process.platform,
				architecture: process.arch,
				limits: [
					"Trusted synthetic fact snapshots only; no production fact collector, audit replay port or admitted writer transaction",
					"Legacy parity compares outcomes, rule IDs and ordered violation code/field; all actual message text and numeric bridges are retained exactly, not claimed byte-compatible",
					"Native-only strict serde/context/version/identifier cases have no equivalent legacy policy operation",
					"Known manual bookkeeping/VAT versions only; version metadata is not a source content checksum or legal-source verification",
					"No mutation endpoint, historical/reversal privilege, invoice evidence, full R09/R12 acceptance, clients/distribution or cutover",
				],
			},
			null,
			2,
		),
	);
} finally {
	db.close();
	rmSync(temp, { recursive: true, force: true });
	if (priorToday === undefined) delete process.env.RENTEMESTER_TODAY;
	else process.env.RENTEMESTER_TODAY = priorToday;
	if (priorFuture === undefined) delete process.env.RENTEMESTER_MAX_FUTURE_DAYS;
	else process.env.RENTEMESTER_MAX_FUTURE_DAYS = priorFuture;
}
