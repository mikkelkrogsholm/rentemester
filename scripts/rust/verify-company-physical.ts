/** Exact typed-cell/full-schema parity for explicitly admitted unversioned baseline layouts. */
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
	temp = mkdtempSync(join(tmpdir(), "rm-company-physical-")),
	copies = join(temp, "copies");
mkdirSync(copies);
let seq = 0;
const passed: string[] = [];
const images = JSON.parse(
	readFileSync(
		join(root, "crates/rentemester-storage/src/legacy-company-schemas.json"),
		"utf8",
	),
) as { baseline: ObjectRow[] };
const contract = JSON.parse(
	readFileSync(
		join(root, "crates/rentemester-storage/src/legacy-company-physical.json"),
		"utf8",
	),
) as { rules: { table: string; source_sql: string }[] };
type ObjectRow = { kind: string; name: string; table: string; sql: string };
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
function normalizeGenerated(t: Tables, old: Tables) {
	for (const [table, col] of [
		["ledger_identity", "created_at"],
		["bookkeeping_batch_approval_policy", "configured_at"],
		["schema_migrations", "applied_at"],
	]) {
		const target = t[table]!;
		for (const row of target.rows) {
			const v = row[target.columns.indexOf(col)]!;
			assert.equal(v[0], "text");
			assert.match(
				Buffer.from(v[1]!, "hex").toString(),
				/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/,
			);
			row[target.columns.indexOf(col)] = ["text", "GENERATED_TIMESTAMP"];
		}
	}
	const history = t.schema_migrations!;
	assert.equal(history.rows.length, 36);
	for (const row of history.rows) {
		for (const col of ["applied_by_version", "applied_by_commit"]) {
			const v = row[history.columns.indexOf(col)]!;
			if (col === "applied_by_version") {
				assert.equal(v[0], "text");
				assert.ok(v[1]!.length > 0);
			}
			row[history.columns.indexOf(col)] = ["text", "BUILD_PROVENANCE"];
		}
	}
	const identity = t.ledger_identity!;
	assert.equal(identity.rows.length, 1);
	const uuid = identity.rows[0]![identity.columns.indexOf("ledger_uuid")]!;
	assert.match(Buffer.from(uuid[1]!, "hex").toString(), /^[0-9a-f]{32}$/);
	identity.rows[0]![identity.columns.indexOf("ledger_uuid")] = [
		"text",
		"GENERATED_UUID",
	];
	return normalizeTimes(t, old);
}
function fixture(
	mask: number,
	cadence: string | null = "quarter",
	kind = "vat_quarter",
) {
	const company = join(temp, `${seq++}-company`);
	mkdirSync(join(company, "data"), { recursive: true });
	const db = new Database(dbPath(company));
	db.exec("PRAGMA foreign_keys=ON");
	for (const o of images.baseline) {
		const variant = contract.rules.find(
			(r, i) => mask & (1 << i) && r.table === o.name,
		);
		db.exec(variant?.source_sql ?? o.sql);
	}
	db.run(
		"INSERT INTO companies(id,name,vat_period_type) VALUES(1,'Synthetic company',?)",
		[cadence],
	);
	const end =
		cadence === "month"
			? "2026-01-31"
			: cadence === "half-year"
				? "2026-06-30"
				: "2026-03-31";
	db.run(
		"INSERT INTO accounting_periods(id,period_start,period_end,kind,status,closed_at,closed_by,reference,created_at) VALUES(9007199254740993,'2026-01-01',? ,?,'closed','2026-07-01','agent:synthetic','Synthetic Ω reference','2020-01-01 00:00:00')",
		[end, kind],
	);
	db.exec(
		"INSERT INTO accounting_periods(id,period_start,period_end,kind,status,reference,created_at) VALUES(7,'2025-01-01','2025-12-31','fiscal_year','open','Preserve fiscal','2020-01-01 00:00:00');INSERT INTO customers(id,name,address,payment_terms_days,created_at) VALUES(9007199254740993,'Synthetic Ω customer','Synthetic'||char(0)||'address',30,'2020-01-01 00:00:00');INSERT INTO gdpr_erasures(id,subject_key,source,source_row_id,redacted_fields,rule_id,reason,retained_until_at_erasure,erased_at) VALUES(9007199254740993,'Synthetic'||char(0)||'raw-subject','customers',9007199254740993,'[\"name\"]','synthetic','Synthetic evidence','2031-12-31','2020-01-01 00:00:00');INSERT INTO audit_log(id,event_type,entity_type,message,actor,created_at) VALUES(40,'synthetic','synthetic','Keep old row','agent:synthetic','2020-01-01 00:00:00');INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('2000','Synthetic bank','asset','debit');INSERT INTO documents(id,source,sha256_hash,invoice_date,retain_until) VALUES(1,'synthetic','synthetic-document','2026-01-01','2031-12-31')",
	);
	return { company, db };
}
function inventory(company: string) {
	const data = join(company, "data");
	return readdirSync(data)
		.sort()
		.map((n) => [
			n,
			createHash("sha256")
				.update(readFileSync(join(data, n)))
				.digest("hex"),
		]);
}
function invoke(company: string, command = "rehearse-company-baseline") {
	const before = inventory(company);
	const r = run([binary, command, "--company", company], { TMPDIR: copies });
	assert.deepEqual(inventory(company), before);
	assert.deepEqual(readdirSync(copies), []);
	assert.equal(r.stderr.length, 0, r.stderr.toString());
	assert.equal(r.stdout.toString().trim().split("\n").length, 1);
	return { exit: r.exitCode, body: JSON.parse(r.stdout.toString()) };
}
function parity(
	label: string,
	mask: number,
	cadence: string | null = "quarter",
	edit?: (db: Database) => void,
	kind = "vat_quarter",
) {
	const { company, db } = fixture(mask, cadence, kind);
	edit?.(db);
	const old = snapshot(db);
	db.close();
	assert.equal(
		invoke(company, "rehearse-company-migration").exit,
		1,
		"ordinary command must not silently normalize unversioned populated source",
	);
	const tsRoot = join(temp, `${seq++}-ts`),
		native = join(temp, `${seq++}-native`);
	cpSync(company, tsRoot, { recursive: true });
	cpSync(company, native, { recursive: true });
	const ts = new Database(dbPath(tsRoot));
	ts.exec("PRAGMA foreign_keys=ON");
	migrate(ts);
	const expected = snapshot(ts),
		expectedSchema = schema(ts);
	ts.close();
	const output = join(temp, `${seq++}.json`);
	const child = run(
		[
			helper,
			"--exact",
			"company_physical::tests::crash_child",
			"--ignored",
			"--nocapture",
		],
		{ RM_PHYSICAL_DB: dbPath(native), RM_PHYSICAL_OUTPUT: output },
	);
	assert.equal(
		child.exitCode,
		0,
		child.stdout.toString() + child.stderr.toString(),
	);
	const result = JSON.parse(readFileSync(output, "utf8"));
	assert.deepEqual(
		normalizeGenerated(result.tables, old),
		normalizeGenerated(expected, old),
	);
	assert.deepEqual(result.report.migration.affected_schema, expectedSchema);
	assert.equal(
		result.report.unchanged_cell_digest_before,
		result.report.unchanged_cell_digest_after,
	);
	assert.equal(result.report.writer_authority, false);
	assert.equal(result.report.private_copy_discarded, true);
	assert.equal(result.report.baseline_history_stamped, true);
	assert.equal(result.report.migration.to_version, 36);
	assert.equal(result.report.migration.from_version, 1);
	assert.equal(result.report.migration.applied.length, 35);
	const product = invoke(company);
	assert.equal(product.exit, 0, JSON.stringify(product));
	assert.deepEqual(product.body.data.migration.affected_schema, expectedSchema);
	assert.deepEqual(
		product.body.data.data_normalization,
		result.report.data_normalization,
	);
	assert.equal(product.body.data.writer_authority, false);
	assert.deepEqual(
		Object.keys(product.body.data).sort(),
		[
			"source_layouts",
			"baseline_history_stamped",
			"vat_rows_relabeled",
			"unchanged_cell_digest_before",
			"unchanged_cell_digest_after",
			"data_normalization",
			"migration",
			"private_copy_discarded",
			"writer_authority",
		].sort(),
	);
	// Rehearsing the SAME source again has no source effects; generated UUIDs/times remain explicitly scoped.
	assert.equal(invoke(company).exit, 0);
	passed.push(label);
}
function reject(
	label: string,
	edit: (db: Database) => void,
	mask = 0,
	tsAlso = true,
	expectedCode = tsAlso ? "invalid_ledger" : "unsupported_schema",
) {
	const { company, db } = fixture(mask);
	edit(db);
	db.close();
	if (tsAlso) {
		const oracle = join(temp, `${seq++}-negative-oracle`);
		cpSync(company, oracle, { recursive: true });
		const ts = new Database(dbPath(oracle));
		ts.exec("PRAGMA foreign_keys=ON");
		assert.throws(() => migrate(ts));
		ts.close();
	}
	const r = invoke(company);
	assert.equal(r.exit, 1, JSON.stringify(r));
	assert.equal(r.body.code, expectedCode, JSON.stringify(r));
	passed.push(label);
}
try {
	for (let mask = 0; mask < 16; mask++)
		parity(
			`all four historical constraints combination${mask}: typed TS full migration and complete DDL`,
			mask,
		);
	for (const cadence of ["month", "quarter", "half-year"])
		parity(`registered ${cadence} VAT relabel`, 7, cadence);
	parity("deregistered globally consistent historical VAT cadence", 7, null);
	parity(
		"missing company row uses default quarterly VAT cadence",
		7,
		"quarter",
		(db) => db.exec("DELETE FROM companies"),
	);
	parity(
		"canonical VAT kind with old other constraints remains unchanged",
		14,
		"quarter",
		undefined,
		"vat_period",
	);
	reject("registered cadence mismatch refuses", (db) =>
		db.exec("UPDATE companies SET vat_period_type='month'"),
	);
	reject("deregistered mixed cadence refuses globally", (db) =>
		db.exec(
			"UPDATE companies SET vat_period_type=NULL;INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('2026-04-01','2026-04-30','vat_quarter','closed')",
		),
	);
	reject("duplicate legacy/canonical period refuses", (db) =>
		db.exec(
			"INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('2026-01-01','2026-03-31','vat_period','closed')",
		),
	);

	// Real overlapping reported pairs with unchanged canonical source objects.
	reject("reported exact-window different kinds refuse", (db) =>
		db.exec(
			"INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('2026-04-01','2026-06-30','vat_quarter','reported'),('2026-04-01','2026-06-30','vat_period','reported')",
		),
	);
	reject(
		"low-year historical VAT ambiguity refuses explicitly",
		(db) =>
			db.exec(
				"INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('0001-01-01','0001-03-31','vat_quarter','closed')",
			),
		0,
		true,
		"unsupported_schema",
	);
	for (const [label, sql] of [
		["unknown extra table refuses", "CREATE TABLE extra(a)"],
		[
			"unknown extra column refuses",
			"ALTER TABLE customers ADD COLUMN unexplained TEXT",
		],
		[
			"unknown company trigger refuses",
			"CREATE TRIGGER custom_company AFTER INSERT ON companies BEGIN SELECT 1;END",
		],
		[
			"unknown index refuses",
			"CREATE INDEX custom_customer ON customers(name)",
		],
		[
			"scratch rebuild evidence refuses",
			"CREATE TABLE customers_payment_terms_rebuild(id INTEGER)",
		],
		["altered guard refuses", "DROP TRIGGER gdpr_erasures_no_update"],
		[
			"nonempty history refuses explicit baseline adoption",
			"INSERT INTO schema_migrations(id,name,checksum,applied_by_version) VALUES(1,'unknown','unknown','synthetic')",
		],
	])
		reject(label!, (db) => db.exec(sql!), 0, false);
	const { company, db } = fixture(15);
	db.exec(
		"PRAGMA journal_mode=WAL;INSERT INTO documents(id,source,sha256_hash,invoice_date,retain_until) VALUES(2,'synthetic','wal','2026-01-01','2031-12-31')",
	);
	assert.equal(invoke(company).exit, 0);
	db.close();
	passed.push(
		"active WAL privately recovered; source inventory and cleanup preserved",
	);
	const linked = join(temp, "linked");
	symlinkSync(company, linked);
	assert.equal(invoke(linked).exit, 1);
	passed.push("symlink root refuses");
	for (const args of [
		[],
		["--company"],
		["--company", company, "--repair"],
		["--company", company, "--company", company],
	]) {
		const r = run([binary, "rehearse-company-baseline", ...args], {
			TMPDIR: copies,
		});
		assert.equal(r.exitCode, 2);
		assert.equal(JSON.parse(r.stdout.toString()).code, "invalid_input");
	}
	passed.push("usage errors exit2 and never mutate source");
	console.log(
		JSON.stringify(
			{
				scope:
					"known unversioned physical company baseline rebuilds and full migration parity",
				checks: passed.length,
				passed,
				platform: process.platform,
				architecture: process.arch,
				limits: [
					"Synthetic exact known208-object baseline layouts, not historical production archives or arbitrary missing-column predecessors",
					"Empty strict history only; no checksum adoption, late-prefix parent/child rebuild or current full-history VAT normalization",
					"New default timestamps/UUID/build provenance validated then masked only in independent comparison; originals exact",
					"Private-copy/source-read-only; no writer or original adoption authority, hardware/cross-platform or full R12 completion",
				],
			},
			null,
			2,
		),
	);
} finally {
	rmSync(temp, { recursive: true, force: true });
}
