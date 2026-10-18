/** Source/native durable-receipt interoperability on disposable synthetic ledgers. */

import { Database } from "bun:sqlite";
import assert from "node:assert/strict";
import {
	copyFileSync,
	existsSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { migrate } from "../../src/core/db";
import {
	canonicalPayloadHash,
	executeLocalIdempotentMutation,
	pruneExpiredIdempotencyOutcomes,
	validateIdempotencyKey,
} from "../../src/core/idempotency";
import { nextSequenceValue } from "../../src/core/sequences";

const NOW = Date.parse("2026-01-01T00:00:00.000Z"),
	RETENTION = 30 * 86400000;
type Request = {
	mode?: string;
	key?: string;
	payload?: Record<string, unknown>;
	operation?: string;
	subject?: string;
	kind?: "user" | "service-account";
	actor?: string;
	program?: string;
	now?: number;
	reject?: boolean;
	fail?: boolean;
	large?: boolean;
	commitTimeoutMs?: number;
};
type Reply = {
	ok: boolean;
	result?: unknown;
	receipt?: { replayed: boolean; receiptId: number };
	code?: string;
	pruned?: number;
	rejection?: unknown;
	hash?: string;
};
function source(path: string, v: Request): Reply {
	const db = new Database(path);
	db.exec(
		"PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA busy_timeout=10000",
	);
	try {
		if (v.mode === "prune")
			return {
				ok: true,
				pruned: pruneExpiredIdempotencyOutcomes(db, new Date(v.now!)),
			};
		const reply = executeLocalIdempotentMutation(db, {
			key: validateIdempotencyKey(v.key),
			operation: v.operation ?? "journal_post",
			principal: {
				kind: v.kind ?? "service-account",
				subjectId: v.subject ?? "synthetic-service",
			},
			payload: v.payload ?? {},
			actor: {
				createdBy: v.actor ?? "agent:synthetic",
				createdByProgram: v.program ?? "test",
			},
			now: new Date(v.now ?? NOW),
			execute: () => {
				const n = nextSequenceValue(db, "journal", "synthetic");
				db.query("INSERT INTO receipt_test_effects VALUES(?)").run(n);
				if (v.reject)
					return { ok: false, errors: ["synthetic business rejection"] };
				if (v.fail) throw new Error("synthetic callback failure");
				if (v.large) return { ok: true, large: "x".repeat(65536) };
				return { ok: true, effect: n };
			},
		});
		return { ok: true, ...reply };
	} catch (e) {
		if (!(e as { code?: string }).code?.startsWith("IDEMPOTENCY_") && !v.fail)
			throw e;
		return {
			ok: false,
			code: (e as { code?: string }).code ?? "callback_failure",
		};
	} finally {
		db.close();
	}
}
if (process.argv[2] === "--source-worker") {
	const v = JSON.parse(process.env.RM_RECEIPT_REQUEST!) as Request;
	writeFileSync(process.env.RM_RECEIPT_MARKER!, "starting source transaction");
	const reply = source(process.env.RM_RECEIPT_DB!, v);
	writeFileSync(process.env.RM_RECEIPT_REPORT!, JSON.stringify(reply));
	process.exit(0);
}
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
const temp = mkdtempSync(join(tmpdir(), "rm-receipts-"));
let serial = 0;
const observations: { id: string }[] = [];
function checked(id: string, f: () => void) {
	f();
	observations.push({ id });
}
const args = [
	helper,
	"--ignored",
	"--exact",
	"idempotency::tests::child",
	"--nocapture",
];
function env(path: string, v: Request, report: string, marker?: string) {
	return {
		...process.env,
		RM_RECEIPT_DB: path,
		RM_RECEIPT_REQUEST: JSON.stringify(v),
		RM_RECEIPT_REPORT: report,
		...(marker ? { RM_RECEIPT_MARKER: marker } : {}),
	};
}
function native(path: string, v: Request): Reply {
	const report = join(temp, `report-${serial++}.json`);
	const p = Bun.spawnSync(args, {
		cwd: root,
		env: env(path, v, report),
		stdout: "pipe",
		stderr: "pipe",
	});
	assert.equal(p.exitCode, 0, p.stderr.toString() + p.stdout.toString());
	return JSON.parse(readFileSync(report, "utf8"));
}
function open(path: string) {
	const db = new Database(path, { safeIntegers: true });
	db.exec("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=10000");
	return db;
}
function fresh(mode = "wal") {
	const path = join(temp, `ledger-${serial++}.sqlite`),
		db = open(path);
	migrate(db);
	db.exec(
		`PRAGMA journal_mode=${mode}; CREATE TABLE receipt_test_effects(n INTEGER PRIMARY KEY)`,
	);
	assert.equal(
		Number(
			(db.query("PRAGMA wal_checkpoint(TRUNCATE)").get() as { busy: number })
				.busy,
		),
		0,
	);
	db.close();
	return path;
}
function encoded(v: unknown) {
	return JSON.stringify(v, (_, v) =>
		typeof v === "bigint" ? { $integer: v.toString() } : v,
	);
}
function snapshot(path: string) {
	const db = open(path);
	try {
		return encoded(
			(
				db
					.query(
						"SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name",
					)
					.all() as { name: string }[]
			).map(({ name }) => [
				name,
				db.query(`SELECT * FROM "${name.replaceAll('"', '""')}"`).all(),
			]),
		);
	} finally {
		db.close();
	}
}
function projection(path: string) {
	const db = open(path);
	try {
		return [
			"ledger_identity",
			"sequences",
			"receipt_test_effects",
			"mutation_idempotency_tombstones",
			"mutation_idempotency_outcomes",
			"audit_log",
		].map((table) => [
			table,
			db
				.query(`SELECT * FROM ${table} ORDER BY 1`)
				.all()
				.map((raw) => {
					const row = { ...(raw as Record<string, unknown>) };
					if (table === "audit_log") delete row.created_at;
					if (table === "mutation_idempotency_outcomes")
						row.outcome_json = JSON.parse(row.outcome_json as string);
					return row;
				}),
		]);
	} finally {
		db.close();
	}
}
function same(sourcePath: string, nativePath: string) {
	assert.deepEqual(projection(nativePath), projection(sourcePath));
}
function receipt(reply: Reply, replayed = false) {
	assert.equal(reply.ok, true, encoded(reply));
	assert.ok(reply.receipt);
	assert.equal(reply.receipt.replayed, replayed);
	return reply.receipt.receiptId;
}
async function waitFor(path: string) {
	const until = Date.now() + 10000;
	while (!existsSync(path)) {
		assert.ok(Date.now() < until, "probe checkpoint missing");
		await Bun.sleep(10);
	}
}
try {
	for (const raw of [
		"333333333.33333329",
		"0.9999999999999999",
		"1.0000000000000002",
		"-0",
		"4.50",
		"1e-27",
		"1e-300",
		"1e-7",
		"1e-6",
		"0.000000000000000000123456789",
		"-333333333.33333329",
		"9007199254740991",
	]) {
		checked(`raw-number/${observations.length}`, () => {
			const payload = `{"n":${raw}}`,
				report = join(temp, `raw-${serial++}.json`);
			const p = Bun.spawnSync(args, {
				cwd: root,
				env: {
					...env("", {}, report),
					RM_RECEIPT_REQUEST: `{"mode":"hash","payload":${payload}}`,
				},
				stdout: "pipe",
				stderr: "pipe",
			});
			assert.equal(p.exitCode, 0, p.stderr.toString());
			assert.equal(
				JSON.parse(readFileSync(report, "utf8")).hash,
				canonicalPayloadHash(JSON.parse(payload)),
			);
		});
	}
	for (const payload of [
		{},
		{ b: 2, a: 1 },
		{ n: -0 },
		{ n: 1.0 },
		{ n: 1e-7 },
		{ n: 1e-6 },
		{ n: Number.MAX_SAFE_INTEGER },
		{ n: -Number.MAX_SAFE_INTEGER },
		{ "\ue000": 1, "😀": 2, a: 3 },
		{ a: [null, true, false, "\u0000\n\tø😀"] },
		{ a: { b: 1, a: 2 }, b: [2, 1] },
		...[
			"333333333.33333329",
			"0.9999999999999999",
			"1.0000000000000002",
			"1e-27",
			"1e-300",
		].map((n) => JSON.parse(`{"n":${n}}`)),
		{ "2": 2, "10": 10, "01": 1 },
		{ s: '\u2028\u2029\ufeff\\"' },
		...Array.from({ length: 30 }, (_, i) => ({
			n: (i - 15) / 7,
			a: [i, { z: i + 0.25, a: "ø" }],
		})),
	])
		checked(`hash/${observations.length}`, () =>
			assert.equal(
				native("", { mode: "hash", payload }).hash,
				canonicalPayloadHash(payload),
			),
		);
	for (const key of [
		"\u0085",
		"\ufeffx",
		" x ",
		"😀".repeat(64),
		"x".repeat(128),
	])
		checked(`key/accepted-${observations.length}`, () => {
			const path = fresh();
			receipt(native(path, { key, payload: {} }));
			assert.equal(validateIdempotencyKey(key), key);
		});
	for (const key of ["", " \n", "\ufeff", "😀".repeat(65), "x".repeat(129)])
		checked(`key/refused-${observations.length}`, () => {
			assert.throws(() => validateIdempotencyKey(key));
			const path = fresh(),
				before = snapshot(path);
			assert.equal(native(path, { key, payload: {} }).ok, false);
			assert.equal(snapshot(path), before);
		});
	for (const payload of [
		{ n: Number.MAX_SAFE_INTEGER + 1 },
		{ s: "x".repeat(65536) },
	])
		checked(`hash/native-refusal-${observations.length}`, () =>
			assert.equal(native("", { mode: "hash", payload }).ok, false),
		);
	for (const mode of ["wal", "delete"]) {
		const original = fresh(mode),
			legacy = join(temp, `source-${serial++}.sqlite`);
		copyFileSync(original, legacy);
		const request = { key: "shared-key", payload: { b: 2, a: [1, "ø"] } };
		checked(`${mode}/new-interoperable`, () => {
			const l = source(legacy, request),
				n = native(original, request);
			receipt(l);
			receipt(n);
			assert.deepEqual(n.result, l.result);
			same(legacy, original);
		});
		checked(`${mode}/replay-actor-independent`, () => {
			const r = {
				...request,
				actor: " \ufeffagent:rotated ",
				program: "\tother-path\n",
				now: NOW + RETENTION + 1,
			};
			receipt(source(legacy, r), true);
			receipt(native(original, r), true);
			same(legacy, original);
		});
		checked(`${mode}/conflict-audit`, () => {
			const r = { ...request, payload: { a: 99 } };
			assert.equal(source(legacy, r).code, "IDEMPOTENCY_CONFLICT");
			assert.equal(native(original, r).code, "IDEMPOTENCY_CONFLICT");
			same(legacy, original);
		});
		checked(`${mode}/source-receipt-native-replay`, () => {
			const r = { key: "source-first", payload: { amount: "2.00" } };
			receipt(source(original, r));
			receipt(native(original, r), true);
			const db = open(original);
			assert.equal(
				(
					db.query("SELECT count(*) n FROM receipt_test_effects").get() as {
						n: bigint;
					}
				).n,
				2n,
			);
			db.close();
		});
		checked(`${mode}/native-receipt-source-replay`, () => {
			const r = { key: "native-first", payload: { amount: "3.00" } };
			receipt(native(legacy, r));
			receipt(source(legacy, r), true);
		});
		for (const [id, options] of Object.entries({
			principal: { subject: "other-service" },
			kind: { kind: "user" as const },
			operation: { operation: "journal_reverse" },
			raw_key: { key: " shared-key " },
		}))
			checked(`${mode}/scope-${id}`, () => {
				const r = { ...request, ...options };
				receipt(source(legacy, r));
				receipt(native(original, r));
			});
		// Previous two cross-read cases deliberately used different keys. Synchronize them before exact parity.
		receipt(
			source(legacy, { key: "source-first", payload: { amount: "2.00" } }),
		);
		receipt(
			native(original, { key: "native-first", payload: { amount: "3.00" } }),
		);
		checked(`${mode}/prune-at-equality`, () => {
			const n = native(original, { mode: "prune", now: NOW + RETENTION }),
				l = source(legacy, { mode: "prune", now: NOW + RETENTION });
			assert.equal(n.pruned, l.pruned);
		});
		checked(`${mode}/expired-tombstone-replay-refusal`, () =>
			assert.equal(
				native(original, request).code,
				"IDEMPOTENCY_OUTCOME_EXPIRED",
			),
		);
		checked(`${mode}/expired-conflict-precedence`, () =>
			assert.equal(
				native(original, { ...request, payload: { different: true } }).code,
				"IDEMPOTENCY_CONFLICT",
			),
		);
		for (const [name, flags] of Object.entries({
			business: { reject: true },
			callback: { fail: true },
			size: { large: true },
		}))
			checked(`${mode}/rollback-${name}`, () => {
				const r = { key: `failed-${name}`, payload: {}, ...flags },
					before = snapshot(original);
				const n = native(original, r);
				assert.equal(n.ok, false);
				if (name === "business")
					assert.deepEqual(n.rejection, {
						ok: false,
						errors: ["synthetic business rejection"],
					});
				assert.equal(snapshot(original), before);
				receipt(native(original, { key: r.key, payload: {} }));
			});
		for (const [name, flags] of Object.entries({
			business: { reject: true },
			callback: { fail: true },
			size: { large: true },
		}))
			checked(`${mode}/source-rollback-${name}`, () => {
				const r = { key: `source-failed-${name}`, payload: {}, ...flags },
					before = snapshot(legacy);
				const reply = source(legacy, r);
				if (name === "business")
					assert.deepEqual(reply.result, {
						ok: false,
						errors: ["synthetic business rejection"],
					});
				else assert.equal(reply.ok, false);
				assert.equal(snapshot(legacy), before);
				receipt(source(legacy, { key: r.key, payload: {} }));
			});
		checked(`${mode}/late-audit-rollback`, () => {
			const db = open(original);
			db.exec(
				"CREATE TRIGGER synthetic_failure BEFORE INSERT ON audit_log WHEN NEW.event_type='idempotency_original' BEGIN SELECT RAISE(ABORT,'late'); END",
			);
			db.close();
			const before = snapshot(original);
			assert.equal(native(original, { key: "late", payload: {} }).ok, false);
			assert.equal(snapshot(original), before);
			const clean = open(original);
			clean.exec("DROP TRIGGER synthetic_failure");
			clean.close();
		});
		checked(`${mode}/missing-outcome-before-expiry`, () => {
			const r = { key: "missing", payload: {} };
			receipt(native(original, r));
			const db = open(original);
			db.exec("DELETE FROM mutation_idempotency_outcomes");
			db.close();
			assert.equal(native(original, r).code, "IDEMPOTENCY_OUTCOME_EXPIRED");
		});
		const r = { key: "racing", payload: { amount: "7.00" } },
			before = open(original);
		const count = Number(
			(
				before.query("SELECT count(*) n FROM receipt_test_effects").get() as {
					n: bigint;
				}
			).n,
		);
		before.close();
		const marker = join(temp, `held-${serial++}`),
			report1 = marker + ".json",
			report2 = marker + "-source.json",
			ready = marker + "-ready";
		const a = Bun.spawn(args, {
			cwd: root,
			env: env(original, r, report1, marker),
			stdin: "pipe",
			stdout: "pipe",
			stderr: "pipe",
		});
		await waitFor(marker);
		const b = Bun.spawn(
			[process.execPath, import.meta.path, "--source-worker"],
			{
				cwd: root,
				env: env(original, r, report2, ready),
				stdout: "pipe",
				stderr: "pipe",
			},
		);
		await waitFor(ready);
		await Bun.sleep(30);
		assert.equal(b.exitCode, null);
		a.stdin.write("continue\n");
		a.stdin.end();
		assert.equal(await a.exited, 0, await new Response(a.stderr).text());
		assert.equal(await b.exited, 0, await new Response(b.stderr).text());
		checked(`${mode}/concurrent-native-source-one-effect`, () => {
			const x = JSON.parse(readFileSync(report1, "utf8")),
				y = JSON.parse(readFileSync(report2, "utf8"));
			receipt(x);
			receipt(y, true);
			assert.deepEqual(x.result, y.result);
			const db = open(original);
			assert.equal(
				Number(
					(
						db.query("SELECT count(*) n FROM receipt_test_effects").get() as {
							n: bigint;
						}
					).n,
				),
				count + 1,
			);
			db.close();
		});
		const crash = { key: "killed", payload: {} },
			mark = join(temp, `kill-${serial++}`),
			out = mark + ".json",
			snapshotBefore = snapshot(original);
		const killed = Bun.spawn(args, {
			cwd: root,
			env: env(original, crash, out, mark),
			stdin: "pipe",
			stdout: "pipe",
			stderr: "pipe",
		});
		await waitFor(mark);
		killed.kill("SIGKILL");
		await killed.exited;
		checked(`${mode}/death-before-commit`, () => {
			assert.equal(existsSync(out), false);
			assert.equal(snapshot(original), snapshotBefore);
			receipt(native(original, crash));
		});
	}
	for (const operation of [
		"expense_book",
		"payable_register",
		"payable_pay",
		"bank_reconciliation_correction_apply",
		"direct_bank_purchase_payable_correction_apply",
	])
		checked(`operation/${operation}`, () => {
			const path = fresh(),
				r = { key: "operation", operation, payload: {} };
			receipt(native(path, r));
			receipt(source(path, r), true);
		});
	for (const [name, sql] of Object.entries({
		ledger:
			"DROP TRIGGER ledger_identity_no_update; UPDATE ledger_identity SET ledger_uuid='bad'",
		payload_hash:
			"DROP TRIGGER mutation_idempotency_tombstones_no_update; UPDATE mutation_idempotency_tombstones SET payload_hash='bad'",
		expiry:
			"DROP TRIGGER mutation_idempotency_tombstones_no_update; UPDATE mutation_idempotency_tombstones SET expires_at='2026-01-30T00:00:00.000Z'",
		outcome_time:
			"UPDATE mutation_idempotency_outcomes SET created_at='2026-01-02T00:00:00.000Z'",
		rejection: `UPDATE mutation_idempotency_outcomes SET outcome_json='{"ok":false}'`,
		invalid_json: "UPDATE mutation_idempotency_outcomes SET outcome_json='bad'",
		oversized:
			"UPDATE mutation_idempotency_outcomes SET outcome_json=json_quote(printf('%070000d',0))",
	})) {
		checked(`metadata/${name}`, () => {
			const path = fresh(),
				r = { key: "metadata", payload: {} };
			receipt(native(path, r));
			const db = open(path);
			db.exec("PRAGMA foreign_keys=OFF; PRAGMA ignore_check_constraints=ON");
			db.exec(sql);
			db.close();
			const before = snapshot(path);
			assert.equal(native(path, r).ok, false);
			assert.equal(snapshot(path), before);
		});
	}
	checked("ledger-identity-isolates-key", () => {
		const a = fresh(),
			b = fresh();
		const r = { key: "same", payload: {} };
		receipt(native(a, r));
		receipt(native(b, r));
		const da = open(a),
			db = open(b);
		assert.notEqual(
			(
				da.query("SELECT ledger_uuid FROM ledger_identity").get() as {
					ledger_uuid: string;
				}
			).ledger_uuid,
			(
				db.query("SELECT ledger_uuid FROM ledger_identity").get() as {
					ledger_uuid: string;
				}
			).ledger_uuid,
		);
		da.close();
		db.close();
	});
	checked("actual-delete-commit-unknown-withholds-receipt", () => {
		const path = fresh("delete"),
			reader = open(path);
		reader.exec("BEGIN");
		reader.query("SELECT * FROM ledger_identity").all();
		const r = native(path, {
			key: "unknown",
			payload: {},
			commitTimeoutMs: 100,
		});
		assert.deepEqual(r, { ok: false, code: "commit_outcome_unknown" });
		reader.exec("ROLLBACK");
		reader.close();
	});
	for (const table of ["audit_log", "mutation_idempotency_tombstones"])
		checked(`ids/${table}-exhausted`, () => {
			const path = fresh();
			const db = open(path);
			if (table === "audit_log")
				db.exec(
					"INSERT INTO audit_log(id,event_type,entity_type,message) VALUES(9223372036854775807,'test','test','test')",
				);
			else {
				receipt(native(path, { key: "seed", payload: {} }));
				db.exec(
					"INSERT INTO mutation_idempotency_tombstones SELECT 9223372036854775807,printf('%064d',0),ledger_uuid,operation,principal_kind,principal_subject_id,payload_hash,created_at,expires_at FROM mutation_idempotency_tombstones LIMIT 1",
				);
			}
			db.close();
			const before = snapshot(path);
			assert.equal(native(path, { key: "exhausted", payload: {} }).ok, false);
			assert.equal(snapshot(path), before);
		});
	console.log(
		JSON.stringify(
			{
				ok: true,
				scope:
					"native durable keyed receipt storage/source interoperability; no complete posting or deployed writer authority",
				checks: observations.length,
				observations,
			},
			null,
			2,
		),
	);
} finally {
	rmSync(temp, { recursive: true, force: true });
}
