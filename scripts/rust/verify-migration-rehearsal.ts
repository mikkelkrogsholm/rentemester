/** Synthetic suffix fixtures; no archived historical backups or production paths. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
	cpSync,
	mkdirSync,
	mkdtempSync,
	readdirSync,
	readFileSync,
	rmSync,
	symlinkSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Database } from "bun:sqlite";
import { migrate } from "../../src/core/db";
import { postJournalEntry } from "../../src/core/ledger";

const root = resolve(import.meta.dir, "../..");
const cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const build = Bun.spawnSync(
	[cargo, "build", "--locked", "-p", "rentemester-cli"],
	{ cwd: root, stdout: "pipe", stderr: "pipe" },
);
assert.equal(build.exitCode, 0, build.stderr.toString());
const binary = join(root, "target/debug/rentemester-rust");
const temp = mkdtempSync(join(tmpdir(), "rentemester-migration-rehearsal-"));
const copies = join(temp, "private-copies");
mkdirSync(copies);
const passed: string[] = [];
let sequence = 0;
const artifacts = readdirSync(join(root, "src/core/migrations"))
	.sort()
	.filter((f) => /^003[3-6]-/.test(f))
	.map(
		(f) =>
			JSON.parse(
				readFileSync(join(root, "src/core/migrations", f), "utf8"),
			) as { sql: string },
	);
assert.equal(artifacts.length, 4);
const q = (s: string) => `"${s.replaceAll('"', '""')}"`;
type ObjectRow = { kind: string; name: string; table: string; sql: string };
function schema(db: Database): ObjectRow[] {
	return db
		.query(
			"SELECT type AS kind,name,tbl_name AS 'table',sql FROM sqlite_schema WHERE substr(name,1,7) <> 'sqlite_' ORDER BY name",
		)
		.all() as ObjectRow[];
}
function reference(version: number) {
	const db = new Database(":memory:");
	try {
		for (const a of artifacts.slice(0, version - 32)) db.exec(a.sql);
		return schema(db);
	} finally {
		db.close();
	}
}
const all = reference(36),
	names = new Set(all.map((o) => o.name)),
	tables = new Set(all.filter((o) => o.kind === "table").map((o) => o.name));
const affected = (db: Database) =>
	schema(db).filter((o) => names.has(o.name) || tables.has(o.table));
function inventory(dir: string, prefix = ""): Record<string, string> {
	const result: Record<string, string> = {};
	for (const e of readdirSync(dir, { withFileTypes: true }).sort((a, b) =>
		a.name.localeCompare(b.name),
	)) {
		const name = prefix + e.name,
			path = join(dir, e.name);
		if (e.isDirectory()) Object.assign(result, inventory(path, `${name}/`));
		else if (e.isSymbolicLink()) result[name] = "symlink";
		else
			result[name] = createHash("sha256")
				.update(readFileSync(path))
				.digest("hex");
	}
	return result;
}
function invoke(company: string, extra: string[] = []) {
	const p = Bun.spawnSync(
		[binary, "rehearse-company-migration", "--company", company, ...extra],
		{
			stdout: "pipe",
			stderr: "pipe",
			timeout: 15000,
			env: { ...process.env, TMPDIR: copies },
		},
	);
	assert.deepEqual(
		readdirSync(copies),
		[],
		"private copies must be explicitly removed on success and refusal",
	);
	assert.equal(p.stderr.length, 0, p.stderr.toString());
	assert.equal(p.stdout.toString().trim().split("\n").length, 1);
	return { exit: p.exitCode, body: JSON.parse(p.stdout.toString()) };
}
function guards(db: Database, table: string) {
	for (const o of schema(db).filter(
		(o) => o.kind === "trigger" && o.table === table,
	))
		db.exec(`DROP TRIGGER ${q(o.name)}`);
}
function fixture(version: number) {
	const company = join(temp, `${sequence++}-v${version}`);
	mkdirSync(join(company, "data"), { recursive: true });
	const db = new Database(join(company, "data/ledger.sqlite"));
	migrate(db);
	db.exec(
		"PRAGMA foreign_keys=ON; INSERT INTO accounts(account_no,name,type,normal_balance) VALUES ('A','Synthetic asset','asset','debit'),('L','Synthetic liability','liability','credit')",
	);
	const posted = postJournalEntry(db, {
		transactionDate: "2026-01-10",
		text: "Synthetic suffix preservation ø",
		createdBy: "agent:synthetic",
		lines: [
			{ accountNo: "A", debitAmount: 123.45 },
			{ accountNo: "L", creditAmount: 123.45 },
		],
	});
	assert.equal(posted.ok, true, JSON.stringify(posted));
	if (version < 36) {
		// Constructed synthetic suffix predecessors, not archived historical ledgers.
		for (const kind of ["trigger", "index", "view", "table"])
			for (const o of all.filter((o) => o.kind === kind))
				db.exec(`DROP ${kind.toUpperCase()} IF EXISTS ${q(o.name)}`);
		const baseline = JSON.parse(
			readFileSync(
				join(
					root,
					"src/core/migrations/0006-bank-journal-reconciliation-links.json",
				),
				"utf8",
			),
		).sql as string;
		const view = baseline.match(
			/CREATE VIEW IF NOT EXISTS bank_journal_reconciliations[\s\S]*?;/,
		)?.[0];
		assert(view);
		db.exec(view);
		for (const a of artifacts.slice(0, version - 32)) db.exec(a.sql);
		guards(db, "schema_migrations");
		db.run("DELETE FROM schema_migrations WHERE id>?", [version]);
	}
	db.close();
	return company;
}
function reject(
	label: string,
	edit: (db: Database) => void,
	code = "unsupported_schema",
	version = 35,
) {
	const company = fixture(version),
		db = new Database(join(company, "data/ledger.sqlite"));
	guards(db, "schema_migrations");
	edit(db);
	db.close();
	const before = inventory(company),
		result = invoke(company);
	assert.equal(result.exit, 1, JSON.stringify(result));
	assert.equal(result.body.code, code, JSON.stringify(result));
	assert.deepEqual(inventory(company), before);
	passed.push(label);
}
try {
	for (const version of [32, 33, 34, 35, 36]) {
		const company = fixture(version);
		const before = inventory(company);
		const result = invoke(company);
		assert.equal(result.exit, 0, JSON.stringify(result));
		assert.equal(
			result.body.format,
			"rentemester-company-migration-rehearsal/1",
		);
		const data = result.body.data;
		assert.equal(data.from_version, version);
		assert.equal(data.to_version, 36);
		assert.deepEqual(
			data.applied.map((m: any) => m.id),
			Array.from({ length: 36 - version }, (_, i) => version + i + 1),
		);
		assert.equal(data.writer_authority, false);
		assert.equal(data.private_copy_discarded, true);
		assert.match(data.business_data_digest_before, /^[0-9a-f]{64}$/);
		assert.equal(
			data.business_data_digest_before,
			data.business_data_digest_after,
		);
		assert.deepEqual(inventory(company), before);
		const ts = join(temp, `${sequence++}-ts-reference`);
		cpSync(company, ts, { recursive: true });
		const db = new Database(join(ts, "data/ledger.sqlite"));
		const prior = db.query("SELECT * FROM schema_migrations ORDER BY id").all();
		const journals = db
				.query("SELECT * FROM journal_entries ORDER BY id")
				.all(),
			lines = db.query("SELECT * FROM journal_lines ORDER BY id").all();
		migrate(db);
		assert.deepEqual(data.affected_schema, affected(db));
		const expected = db
			.query(
				"SELECT id,name,checksum FROM schema_migrations WHERE id>? ORDER BY id",
			)
			.all(version);
		assert.deepEqual(data.applied, expected);
		assert.deepEqual(
			db
				.query("SELECT * FROM schema_migrations WHERE id<=? ORDER BY id")
				.all(version),
			prior,
		);
		assert.deepEqual(
			db.query("SELECT * FROM journal_entries ORDER BY id").all(),
			journals,
		);
		assert.deepEqual(
			db.query("SELECT * FROM journal_lines ORDER BY id").all(),
			lines,
		);
		db.close();
		assert.deepEqual(
			invoke(company).body.data,
			data,
			"repeat must be deterministic, no exported timestamps/paths",
		);
		assert.deepEqual(inventory(company), before);
		passed.push(
			`synthetic v${version} -> v36 exact TS affected-schema/history parity, source/journal preservation and repeat`,
		);
	}
	reject(
		"older history refuses",
		(db) => db.run("DELETE FROM schema_migrations WHERE id=32"),
		"unsupported_schema",
		32,
	);
	reject("gapped history refuses", (db) =>
		db.run("DELETE FROM schema_migrations WHERE id=17"),
	);
	reject(
		"newer history refuses",
		(db) =>
			db.run(
				"INSERT INTO schema_migrations(id,name,checksum,applied_by_version) VALUES (37,'newer','newer','synthetic')",
			),
		"unsupported_schema",
		36,
	);
	reject("altered checksum refuses", (db) =>
		db.run("UPDATE schema_migrations SET checksum='changed' WHERE id=12"),
	);
	reject("altered name refuses", (db) =>
		db.run("UPDATE schema_migrations SET name='changed' WHERE id=12"),
	);
	reject(
		"empty provenance refuses",
		(db) =>
			db.run("UPDATE schema_migrations SET applied_by_version='' WHERE id=12"),
		"invalid_ledger",
	);
	reject("missing history refuses", (db) =>
		db.exec("DROP TABLE schema_migrations"),
	);
	reject("missing affected trigger refuses", (db) =>
		db.exec("DROP TRIGGER bank_reconciliation_correction_events_guard_insert"),
	);
	reject("altered affected trigger refuses", (db) =>
		db.exec(
			"DROP TRIGGER bank_reconciliation_correction_events_guard_insert; CREATE TRIGGER bank_reconciliation_correction_events_guard_insert BEFORE INSERT ON bank_reconciliation_correction_events BEGIN SELECT 1; END",
		),
	);
	reject("unexpected extra affected trigger refuses", (db) =>
		db.exec(
			"CREATE TRIGGER synthetic_extra BEFORE INSERT ON bank_reconciliation_correction_events BEGIN SELECT 1; END",
		),
	);
	reject("partial preexisting next-step object refuses", (db) =>
		db.exec(artifacts[3].sql),
	);
	reject(
		"foreign-key corruption refuses",
		(db) => {
			guards(db, "journal_lines");
			db.exec(
				"PRAGMA foreign_keys=OFF;INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES (999999,999999,1,0)",
			);
		},
		"invalid_ledger",
	);
	reject("empty checksum refuses", (db) =>
		db.run("UPDATE schema_migrations SET checksum='' WHERE id=12"),
	);
	reject(
		"altered affected table refuses",
		(db) =>
			db.exec("ALTER TABLE imported_receivable_headers ADD COLUMN extra TEXT"),
		"unsupported_schema",
		36,
	);
	reject(
		"private SQL/history failure refuses and removes copy",
		(db) =>
			db.exec(
				"CREATE TRIGGER synthetic_history_failure BEFORE INSERT ON schema_migrations WHEN NEW.id=36 BEGIN SELECT RAISE(ABORT,'synthetic refusal'); END",
			),
		"migration_failed",
	);
	const active = fixture(35),
		db = new Database(join(active, "data/ledger.sqlite"));
	db.exec(
		"PRAGMA journal_mode=WAL;PRAGMA wal_autocheckpoint=0;INSERT INTO accounts(account_no,name,type,normal_balance) VALUES ('W','WAL only','asset','debit')",
	);
	const wal = inventory(active);
	assert(Object.keys(wal).some((f) => f.endsWith("-wal")));
	assert.equal(invoke(active).exit, 0);
	assert.deepEqual(inventory(active), wal);
	db.close();
	passed.push(
		"active legacy WAL snapshot recovered privately; source DB/WAL/SHM unchanged",
	);
	const source = fixture(35),
		link = join(temp, "company-link");
	symlinkSync(source, link, "dir");
	for (const p of [link, `${link}/`, `${link}/.`, join(temp, "missing")]) {
		assert.equal(invoke(p).body.code, "source_unavailable");
	}
	passed.push("missing and final-symlink aliases refuse");
	for (const args of [
		[],
		["--company"],
		["--company", source, "--target", "36"],
		["--company", ""],
		["--company", source, "--company", source],
	]) {
		const p = Bun.spawnSync([binary, "rehearse-company-migration", ...args], {
			stdout: "pipe",
			stderr: "pipe",
		});
		assert.equal(p.exitCode, 2);
		assert.equal(JSON.parse(p.stdout.toString()).code, "invalid_input");
	}
	passed.push("strict CLI usage refuses actor/mutation/target ambiguities");
	const storageBefore = inventory(source);
	const storageFailure = Bun.spawnSync(
		[binary, "rehearse-company-migration", "--company", source],
		{
			stdout: "pipe",
			stderr: "pipe",
			timeout: 15000,
			env: { ...process.env, TMPDIR: join(source, "data/ledger.sqlite") },
		},
	);
	assert.equal(storageFailure.exitCode, 1);
	assert.equal(
		JSON.parse(storageFailure.stdout.toString()).code,
		"storage_failure",
	);
	assert.deepEqual(inventory(source), storageBefore);
	assert.deepEqual(readdirSync(copies), []);
	passed.push("unusable temporary storage refuses before any source effect");
	const old = Bun.spawnSync(
		[
			binary,
			"trial-balance",
			"--company",
			source,
			"--from",
			"2026-01-01",
			"--to",
			"2026-12-31",
		],
		{ stdout: "pipe", stderr: "pipe" },
	);
	assert.equal(old.exitCode, 1);
	assert.equal(JSON.parse(old.stdout.toString()).code, "unsupported_schema");
	passed.push("existing report retains exact-current policy");
	console.log(
		JSON.stringify(
			{
				ok: true,
				scope: "private-copy company suffix32–36 rehearsal only",
				checks: passed.length,
				passed,
				platform: process.platform,
				architecture: process.arch,
				limits: [
					"Constructed synthetic predecessor fixtures, not historical archive qualification",
					"No source migration, writer authority, workspace-control migration, baseline adoption or full R12 acceptance",
				],
			},
			null,
			2,
		),
	);
} finally {
	rmSync(temp, { recursive: true, force: true });
}
