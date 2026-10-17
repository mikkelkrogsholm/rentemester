/** Full-prefix VAT normalization and populated-FK parity against actual corrected TS migrate. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
	cpSync,
	mkdirSync,
	mkdtempSync,
	readdirSync,
	readFileSync,
	rmSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Database } from "bun:sqlite";
import { normalizeCompanyBaseline, migrate } from "../../src/core/db";
import { supportedSchemaMigrations } from "../../src/core/schema-version";
const root = resolve(import.meta.dir, "../.."),
	cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
function run(args: string[], env: Record<string, string | undefined> = {}) {
	return Bun.spawnSync(args, {
		cwd: root,
		env: { ...process.env, ...env },
		stdout: "pipe",
		stderr: "pipe",
		timeout: 30000,
	});
}
const built = run([
	cargo,
	"test",
	"--locked",
	"-p",
	"rentemester-storage",
	"--no-run",
	"--message-format=json",
]);
assert.equal(built.exitCode, 0, built.stderr.toString());
const helper = built.stdout
	.toString()
	.split("\n")
	.filter(Boolean)
	.map((s) => JSON.parse(s))
	.find(
		(x) => x.reason === "compiler-artifact" && x.profile.test && x.executable,
	)?.executable as string;
assert.ok(helper);
const cli = run([cargo, "build", "--locked", "-p", "rentemester-cli"]);
assert.equal(cli.exitCode, 0, cli.stderr.toString());
const binary = join(root, "target/debug/rentemester-rust"),
	temp = mkdtempSync(join(tmpdir(), "rm-company-vat-")),
	copies = join(temp, "copies");
mkdirSync(copies);
let seq = 0;
const passed: string[] = [];
const identities = supportedSchemaMigrations();
const artifacts = readdirSync(join(root, "src/core/migrations"))
	.filter((f) => f.endsWith(".json"))
	.sort()
	.slice(1)
	.map(
		(f) =>
			JSON.parse(
				readFileSync(join(root, "src/core/migrations", f), "utf8"),
			) as { sql: string },
	);
type Table = { columns: string[]; rows: string[][][] };
type Tables = Record<string, Table>;
const q = (s: string) => `"${s.replaceAll('"', '""')}"`,
	dbPath = (s: string) => join(s, "data/ledger.sqlite");
const schema = (db: Database) =>
	db
		.query(
			"SELECT type AS kind,name,tbl_name AS 'table',sql FROM sqlite_schema WHERE substr(name,1,7)<>'sqlite_' ORDER BY name",
		)
		.all();
function snapshot(db: Database): Tables {
	const result: Tables = {};
	for (const { name } of db
		.query(
			"SELECT name FROM sqlite_schema WHERE type='table' AND substr(name,1,7)<>'sqlite_' ORDER BY name",
		)
		.all() as { name: string }[]) {
		const columns = (
			db.query(`PRAGMA table_info(${q(name)})`).all() as { name: string }[]
		).map((x) => x.name);

		const expressions = columns.flatMap((c) => [
			`typeof(${q(c)})`,
			`CASE WHEN typeof(${q(c)})='integer' THEN CAST(${q(c)} AS TEXT) ELSE ${q(c)} END`,
		]);
		const rows = db
			.query(`SELECT ${expressions.join(",")} FROM ${q(name)}`)
			.values()
			.map((row) =>
				columns.map((_, i) => {
					const type = row[i * 2],
						v = row[i * 2 + 1];
					if (type === "null") return ["null"];
					if (type === "integer") {
						assert.equal(typeof v, "string");
						return ["integer", String(v)];
					}
					if (type === "real") {
						assert.equal(typeof v, "number");
						const b = Buffer.alloc(8);
						b.writeDoubleBE(v as number);
						return ["real", b.toString("hex")];
					}
					if (type === "text") {
						assert.equal(typeof v, "string");
						return ["text", Buffer.from(v as string).toString("hex")];
					}
					assert.equal(type, "blob");
					assert.ok(v instanceof Uint8Array);
					return ["blob", Buffer.from(v).toString("hex")];
				}),
			);
		result[name] = { columns, rows };
	}
	return result;
}
function normalizeTimes(t: Tables, old: Tables) {
	for (const [table, col] of [
		["account_role_mappings", "confirmed_at"],
		["audit_log", "created_at"],
	]) {
		const prior = new Set(
			old[table]!.rows.map((r) => r[old[table]!.columns.indexOf("id")]![1]),
		);
		const target = t[table]!;
		for (const row of target.rows)
			if (!prior.has(row[target.columns.indexOf("id")]![1])) {
				const value = row[target.columns.indexOf(col)]!;
				assert.equal(value[0], "text");
				assert.match(
					Buffer.from(value[1]!, "hex").toString(),
					/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/,
				);
				row[target.columns.indexOf(col)] = ["text", "GENERATED_TIMESTAMP"];
			}
	}
	for (const table of Object.values(t))
		table.rows.sort((a, b) =>
			JSON.stringify(a).localeCompare(JSON.stringify(b)),
		);
	return t;
}
function normalizeGenerated(t: Tables, old: Tables, version: number) {
	const history = t.schema_migrations!;
	assert.equal(history.rows.length, 36);
	for (const row of history.rows) {
		const id = BigInt(row[history.columns.indexOf("id")]![1]!);
		if (id > BigInt(version)) {
			const stamp = row[history.columns.indexOf("applied_at")]!;
			assert.match(
				Buffer.from(stamp[1]!, "hex").toString(),
				/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/,
			);
			row[history.columns.indexOf("applied_at")] = ["text", "NEW_TIMESTAMP"];
			for (const col of ["applied_by_version", "applied_by_commit"]) {
				const v = row[history.columns.indexOf(col)]!;
				if (col === "applied_by_version") {
					assert.equal(v[0], "text");
					assert.ok(v[1]!.length > 0);
				}
				row[history.columns.indexOf(col)] = ["text", "NEW_BUILD_PROVENANCE"];
			}
		}
	}
	for (const [name, col] of [
		["ledger_identity", "created_at"],
		["bookkeeping_batch_approval_policy", "configured_at"],
	])
		if (!old[name!]) {
			const table = t[name!]!;
			for (const row of table.rows) {
				const v = row[table.columns.indexOf(col!)]!;
				assert.match(
					Buffer.from(v[1]!, "hex").toString(),
					/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/,
				);
				row[table.columns.indexOf(col!)] = ["text", "NEW_TIMESTAMP"];
			}
		}
	if (!old.ledger_identity) {
		const t1 = t.ledger_identity!;
		assert.equal(t1.rows.length, 1);
		const i = t1.columns.indexOf("ledger_uuid");
		assert.match(
			Buffer.from(t1.rows[0]![i]![1]!, "hex").toString(),
			/^[0-9a-f]{32}$/,
		);
		t1.rows[0]![i] = ["text", "NEW_UUID"];
	}
	return normalizeTimes(t, old);
}
function fixture(version: number, cadence: string | null = "quarter") {
	const company = join(temp, `${seq++}-v${version}`);
	mkdirSync(join(company, "data"), { recursive: true });
	const db = new Database(dbPath(company));
	db.exec("PRAGMA foreign_keys=ON");
	normalizeCompanyBaseline(db);
	for (let i = 2; i <= version; i++) {
		db.exec(artifacts[i - 2]!.sql);
		const m = identities[i - 1]!;
		db.run(
			"INSERT INTO schema_migrations(id,name,checksum,applied_by_version,applied_at) VALUES(?,?,?,'synthetic-prior','2020-01-01 00:00:00')",
			[m.id, m.name, m.checksum],
		);
	}
	if (version === 36) migrate(db);
	db.run(
		"INSERT INTO companies(id,name,vat_period_type) VALUES(1,'Synthetic',?)",
		[cadence],
	);
	const span = cadence === "month" ? 1 : cadence === "half-year" ? 6 : 3;
	const end = (m: number) =>
		new Date(Date.UTC(2027, m, 0)).toISOString().slice(0, 10);
	db.run(
		"INSERT INTO accounting_periods(id,period_start,period_end,kind,status,reference,created_at) VALUES(9007199254740993,'2027-01-01',?,'vat_quarter','closed','Synthetic legacy','2020-01-01'),(9007199254740994,?,?,'vat_period','closed','Synthetic canonical','2020-01-01'),(7,'2026-01-01','2026-12-31','fiscal_year','open','Preserve fiscal','2020-01-01')",
		[end(span), `2027-${String(span + 1).padStart(2, "0")}-01`, end(span * 2)],
	);
	db.exec(
		"INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('2000','Synthetic bank','asset','debit');INSERT INTO audit_log(id,event_type,entity_type,message,actor,created_at) VALUES(40,'synthetic','synthetic','Keep old audit','agent:synthetic','2020-01-01');INSERT INTO documents(id,source,sha256_hash,invoice_date) VALUES(1,'synthetic','synthetic','2026-01-01')",
	);
	if (version >= 22)
		db.exec(
			"INSERT INTO period_close_readiness_packets(id,packet_hash,period_start,period_end,cutoff,packet_json,created_at) VALUES(1,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','2027-01-01','2027-03-31','2027-04-01','{}','2020-01-01');INSERT INTO period_close_decisions(id,period_id,packet_hash,decision,actor,created_at) VALUES(1,9007199254740993,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','closed','agent:synthetic','2020-01-01');INSERT INTO period_close_open_items(id,decision_id,period_id,packet_hash,code,severity,count,amount,evidence_json,reason,actor,created_at) VALUES(1,1,9007199254740993,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','synthetic','warning',1,10,'{}','synthetic','agent:synthetic','2020-01-01')",
		);
	return { company, db };
}
function inventory(company: string) {
	return readdirSync(join(company, "data"))
		.sort()
		.map((n) => [
			n,
			createHash("sha256")
				.update(readFileSync(join(company, "data", n)))
				.digest("hex"),
		]);
}
function invoke(company: string) {
	const before = inventory(company);
	const r = run([binary, "rehearse-company-migration", "--company", company], {
		TMPDIR: copies,
	});
	assert.deepEqual(inventory(company), before);
	assert.deepEqual(readdirSync(copies), []);
	assert.equal(r.stderr.length, 0, r.stderr.toString());
	return { exit: r.exitCode, body: JSON.parse(r.stdout.toString()) };
}
function parity(
	label: string,
	version: number,
	cadence: string | null = "quarter",
	edit?: (db: Database) => void,
) {
	const { company, db } = fixture(version, cadence);
	edit?.(db);
	const old = snapshot(db);
	db.close();
	const tsRoot = join(temp, `${seq++}-ts`),
		native = join(temp, `${seq++}-native`);
	cpSync(company, tsRoot, { recursive: true });
	cpSync(company, native, { recursive: true });
	const ts = new Database(dbPath(tsRoot));
	ts.exec("PRAGMA foreign_keys=ON");
	migrate(ts);
	const expected = snapshot(ts),
		expectedSchema = schema(ts);
	assert.deepEqual(ts.query("PRAGMA foreign_key_check").all(), []);
	ts.close();
	const output = join(temp, `${seq++}.json`);
	const child = run(
		[
			helper,
			"--exact",
			"company_vat::tests::crash_child",
			"--ignored",
			"--nocapture",
		],
		{ RM_VAT_DB: dbPath(native), RM_VAT_OUTPUT: output },
	);
	assert.equal(
		child.exitCode,
		0,
		child.stdout.toString() + child.stderr.toString(),
	);
	const result = JSON.parse(readFileSync(output, "utf8"));
	assert.deepEqual(
		normalizeGenerated(result.tables, old, version),
		normalizeGenerated(expected, old, version),
	);
	assert.deepEqual(result.report.affected_schema, expectedSchema);
	assert.equal(result.report.normalization.vat_rows_relabeled, 1);
	assert.equal(
		result.report.business_data_digest_before,
		result.report.business_data_digest_after,
	);
	assert.equal(result.report.from_version, version);
	assert.equal(result.report.to_version, 36);
	assert.equal(result.report.applied.length, 36 - version);
	assert.equal(
		result.report.schema_scope,
		version < 32
			? "full-normalized-early-prefix"
			: "full-normalized-company-prefix",
	);
	const product = invoke(company);
	assert.equal(product.exit, 0, JSON.stringify(product));
	assert.deepEqual(
		product.body.data.normalization,
		result.report.normalization,
	);
	assert.deepEqual(product.body.data.affected_schema, expectedSchema);
	assert.equal(product.body.data.writer_authority, false);
	assert.equal(product.body.data.private_copy_discarded, true);
	passed.push(label);
}
function reject(
	label: string,
	edit: (db: Database) => void,
	code = "invalid_ledger",
	tsRefuses = true,
) {
	const { company, db } = fixture(36);
	edit(db);
	db.close();
	if (tsRefuses) {
		const copy = join(temp, `${seq++}-negative`);
		cpSync(company, copy, { recursive: true });
		const ts = new Database(dbPath(copy));
		ts.exec("PRAGMA foreign_keys=ON");
		assert.throws(() => migrate(ts));
		ts.close();
	}
	const r = invoke(company);
	assert.equal(r.exit, 1, JSON.stringify(r));
	assert.equal(r.body.code, code, JSON.stringify(r));
	passed.push(label);
}
try {
	for (let version = 1; version <= 36; version++)
		parity(
			`full prefix${version}: exact corrected TS migration, VAT/typed/history preservation and late-FK children`,
			version,
		);
	for (const cadence of ["month", "quarter", "half-year"])
		parity(
			`current registered ${cadence} cadence with FK children`,
			36,
			cadence,
		);
	parity(
		"current deregistered common historical cadence with FK children",
		36,
		null,
	);
	parity(
		"VAT uses ID1 default when only another company row exists",
		36,
		"quarter",
		(db) =>
			db.exec(
				"INSERT INTO companies(id,name,vat_period_type) VALUES(-2,'Synthetic lowest','half-year');DELETE FROM companies WHERE id=1",
			),
	);
	reject(
		"current registered cadence mismatch refuses without source effect",
		(db) => db.exec("UPDATE companies SET vat_period_type='month'"),
	);
	reject("current deregistered mixed cadence refuses globally", (db) =>
		db.exec(
			"UPDATE companies SET vat_period_type=NULL;INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('2027-07-01','2027-07-31','vat_quarter','closed')",
		),
	);
	reject("current duplicate legacy/canonical refuses", (db) =>
		db.exec(
			"INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('2027-01-01','2027-03-31','vat_period','closed')",
		),
	);
	reject(
		"current low-year century ambiguity explicitly refuses",
		(db) =>
			db.exec(
				"INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('0001-01-01','1901-03-31','vat_quarter','closed')",
			),
		"unsupported_schema",
	);
	reject(
		"unknown current object cannot gain VAT normalization access",
		(db) => db.exec("CREATE TABLE unexplained(a)"),
		"unsupported_schema",
		false,
	);
	reject(
		"unknown current period guard cannot gain VAT normalization access",
		(db) =>
			db.exec(
				"CREATE TRIGGER independent_guard BEFORE UPDATE ON accounting_periods BEGIN SELECT RAISE(ABORT,'independent');END",
			),
		"unsupported_schema",
		false,
	);
	reject(
		"tampered current history refuses",
		(db) => {
			db.exec("UPDATE schema_migrations SET checksum='changed' WHERE id=36");
		},
		"unsupported_schema",
		false,
	);
	const { company, db } = fixture(36);
	db.exec(
		"PRAGMA journal_mode=WAL;INSERT INTO documents(id,source,sha256_hash,retain_until) VALUES(2,'synthetic','wal','2031-12-31')",
	);
	assert.equal(invoke(company).exit, 0);
	db.close();
	passed.push(
		"current active WAL privately recovered; source inventory and cleanup intact",
	);
	console.log(
		JSON.stringify(
			{
				scope: "FK-safe VAT normalization and complete company prefixes1–36",
				checks: passed.length,
				passed,
				platform: process.platform,
				architecture: process.arch,
				limits: [
					"Canonical complete schemas with strict history; old physical/missing-column/checksumless/archive adoption remain separate",
					"Years below100 explicitly refused while normalization is required",
					"Only generated NEW timestamps/UUID/build provenance masked; old provenance and parent/child typed cells exact",
					"Private captures only; no writer authority, other late parent rebuilds, hardware/cross-platform or full R12 acceptance",
				],
			},
			null,
			2,
		),
	);
} finally {
	rmSync(temp, { recursive: true, force: true });
}
