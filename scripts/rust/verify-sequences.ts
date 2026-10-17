/** Independent actual legacy sequence vectors on synthetic company databases. */
import assert from "node:assert/strict";
import { copyFileSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Database } from "bun:sqlite";
import { migrate } from "../../src/core/db";
import {
	companySequenceScope,
	currentSequenceValue,
	nextSequenceValue,
	reserveSequenceValue,
} from "../../src/core/sequences";
const root = resolve(import.meta.dir, "../.."),
	cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const build = Bun.spawnSync(
	[
		cargo,
		"test",
		"--locked",
		"-p",
		"rentemester-storage",
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
		(x) => x.reason === "compiler-artifact" && x.profile.test && x.executable,
	)?.executable;
assert.ok(helper);
type Operation =
	| {
			op: "next" | "current" | "reserve";
			kind: string;
			scope: string;
			floor: number;
			requested?: number;
			rollback?: boolean;
	  }
	| {
			op: "scope";
			cvr: string | null;
			rawCvr: string | null;
			id: number;
			scope: string;
	  };
const ops: Operation[] = [];
for (const [rawCvr, cvr] of [
	[null, null],
	["", null],
	["11112222", "DK11112222"],
	[" dk33334444 ", "DK33334444"],
] as const)
	ops.push({ op: "scope", cvr, rawCvr, id: 7, scope: "2026/27" });
const add = (
	op: "next" | "current" | "reserve",
	kind = "journal",
	scope = "company-7:2026",
	floor = 0,
	requested?: number,
	rollback = false,
) => ops.push({ op, kind, scope, floor, requested, rollback });
add("current");
add("current", "journal", "company-7:2026", 40);
add("next");
add("reserve", "journal", "company-7:2026", 0, 3);
add("reserve", "journal", "company-7:2026", 0, 2);
add("next", "journal", "company-7:2026", 30);
add("next", "journal", "company-7:2026", 4);
add("next", "journal", "company-7:2026", 100, undefined, true);
add("current");
for (const kind of ["invoice", "", "ø\0kind", "';DROP TABLE sequences;--"]) {
	for (const scope of ["2026", "2027", "", "ø\0scope"]) {
		add("current", kind, scope);
		add("next", kind, scope, 8);
		add("reserve", kind, scope, 0, 11);
		add("reserve", kind, scope, 0, 10);
		add("reserve", kind, scope, 19, 20, true);
		add("next", kind, scope);
		add("current", kind, scope, 25);
	}
}
add("next", "safe-limit", "", Number.MAX_SAFE_INTEGER - 1);
add("current", "safe-limit", "");
const temp = mkdtempSync(join(tmpdir(), "rm-sequences-"));
try {
	const path = join(temp, "legacy.sqlite"),
		native = join(temp, "native.sqlite"),
		output = join(temp, "native.json");
	const db = new Database(path);
	db.exec("PRAGMA foreign_keys=ON");
	migrate(db);
	db.exec(
		"INSERT INTO companies(id,name,country,currency) VALUES(7,'Synthetic','DK','DKK')",
	);
	db.close();
	copyFileSync(path, native);
	const ts = new Database(path);
	const results: unknown[] = [];
	for (const o of ops) {
		if (o.op === "scope") {
			ts.query("UPDATE companies SET cvr=? WHERE id=?").run(o.rawCvr, o.id);
			results.push(companySequenceScope(ts, o.scope));
			continue;
		}
		if (o.op === "current") {
			results.push(currentSequenceValue(ts, o.kind, o.scope, o.floor));
			continue;
		}
		ts.exec("BEGIN IMMEDIATE");
		try {
			if (o.op === "next")
				results.push(nextSequenceValue(ts, o.kind, o.scope, o.floor));
			else {
				const v = reserveSequenceValue(
					ts,
					o.kind,
					o.scope,
					o.requested!,
					o.floor,
				);
				results.push({
					ok: v.ok,
					current_value: v.currentValue,
					expected_value: v.expectedValue,
				});
			}
			ts.exec(o.rollback ? "ROLLBACK" : "COMMIT");
		} catch (e) {
			ts.exec("ROLLBACK");
			throw e;
		}
	}
	const rows = ts
		.query(
			"SELECT kind,scope,CAST(value AS TEXT) AS value,typeof(value) AS type FROM sequences ORDER BY kind,scope",
		)
		.all() as { kind: string; scope: string; value: string; type: string }[];
	ts.close();
	const run = Bun.spawnSync(
		[
			helper,
			"--exact",
			"sequences::tests::process_child",
			"--ignored",
			"--nocapture",
		],
		{
			cwd: root,
			env: {
				...process.env,
				RM_SEQUENCE_DB: native,
				RM_SEQUENCE_MODE: "parity",
				RM_SEQUENCE_OPS: JSON.stringify(ops),
				RM_SEQUENCE_OUTPUT: output,
			},
			stdout: "pipe",
			stderr: "pipe",
			timeout: 30000,
		},
	);
	assert.equal(run.exitCode, 0, run.stderr.toString() + run.stdout.toString());
	const actual = JSON.parse(readFileSync(output, "utf8"));
	assert.deepEqual(actual.results, results);
	const hex = (s: string) => Buffer.from(s).toString("hex");
	const expectedRows = rows.map((r) => {
		assert.equal(r.type, "integer");
		return [
			["text", hex(r.kind)],
			["text", hex(r.scope)],
			["integer", r.value],
		];
	});
	const sorted = (rs: unknown[]) => rs.map((r) => JSON.stringify(r)).sort();
	assert.deepEqual(actual.sequences.columns, ["kind", "scope", "value"]);
	assert.deepEqual(sorted(actual.sequences.rows), sorted(expectedRows));
	console.log(
		JSON.stringify(
			{
				scope:
					"transaction-owned native sequence values against actual TypeScript functions",
				checks: ops.length + 1,
				operation_vectors: ops.length,
				passed: [
					"all actual TS current/next/reserve/scope results",
					"exact final SQLite INTEGER and UTF8 cells",
					"outer rollback and independent kinds/scopes",
					"safe-integer maximum without rounding",
				],
				platform: process.platform,
				architecture: process.arch,
				limits: [
					"Only disposable synthetic databases; no product writer endpoint or original source writes",
					"Legacy parity only for valid safe-integer inputs; native exact signed64-bit/overflow tested separately",
					"No invoice/journal floor discovery, date/number formatting, idempotency receipts, authority/adoption or full R12 acceptance",
				],
			},
			null,
			2,
		),
	);
} finally {
	rmSync(temp, { recursive: true, force: true });
}
