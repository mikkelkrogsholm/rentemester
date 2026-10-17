/** Independent corrected-TS/native typed-cell parity on synthetic private copies. */
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
import { normalizeCompanyBaseline } from "../../src/core/db";
import { supportedSchemaMigrations } from "../../src/core/schema-version";
import { seedNativeAccountRoles } from "../../src/core/account-roles";
import {
	backfillRetentionDeadlines,
	retainUntilForDate,
} from "../../src/core/retention";
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
const build = run([
	cargo,
	"test",
	"--locked",
	"-p",
	"rentemester-storage",
	"--no-run",
	"--message-format=json",
]);
assert.equal(build.exitCode, 0, build.stderr.toString());
const helper = build.stdout
	.toString()
	.split("\n")
	.filter(Boolean)
	.map((s) => JSON.parse(s))
	.find(
		(x) => x.reason === "compiler-artifact" && x.profile.test && x.executable,
	)?.executable as string;
assert.ok(helper);
const cliBuild = run([cargo, "build", "--locked", "-p", "rentemester-cli"]);
assert.equal(cliBuild.exitCode, 0, cliBuild.stderr.toString());
const temp = mkdtempSync(join(tmpdir(), "rm-normalization-")),
	copies = join(temp, "copies");
mkdirSync(copies);
const passed: string[] = [];
let seq = 0;
const q = (s: string) => `"${s.replaceAll('"', '""')}"`;
const dbPath = (s: string) => join(s, "data/ledger.sqlite");
type Table = { columns: string[]; rows: string[][][] };
type Tables = Record<string, Table>;
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
function fixture(start = 1, label = "end-year") {
	const company = join(temp, `${seq++}-company`);
	mkdirSync(join(company, "data"), { recursive: true });
	const db = new Database(dbPath(company));
	db.exec("PRAGMA foreign_keys=ON");
	normalizeCompanyBaseline(db);
	const a = JSON.parse(
		readFileSync(
			join(root, "src/core/migrations/0002-peppol-submission-events.json"),
			"utf8",
		),
	);
	db.exec(a.sql);
	const m = supportedSchemaMigrations()[1]!;
	db.run(
		"INSERT INTO schema_migrations(id,name,checksum,applied_by_version) VALUES(?,?,?,'synthetic-prior')",
		[m.id, m.name, m.checksum],
	);
	db.run(
		"INSERT INTO companies(id,name,cvr,fiscal_year_start_month,fiscal_year_label_strategy) VALUES(1,'Synthetic','DK12345678',?,?)",
		[start, label],
	);
	db.exec(
		"INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('2000','Synthetic bank','asset','debit'),('1100','Synthetic debtor','asset','debit'),('7000','Synthetic creditor','liability','credit'),('1200','Synthetic output VAT','vat','credit'),('4000','Synthetic input VAT','vat','debit'),('4500','Synthetic settlement','liability','credit'),('3000','Synthetic expense','expense','debit')",
	);
	const vectors = [
		["2024-02-29", null, null],
		["2027-02-28", null, null],
		["2023-03-01", null, null],
		[" 2026-01-01 ", null, null],
		["0001-07-01", null, null],
		["0000-02-29", null, null],
		[null, "2026-07-01 08:10:00", null],
		["invalid", "2026-01-01", null],
		["", null, null],
		["2023-02-29", null, null],
		["2024-02-29", null, "existing-invalid"],
		["2024-02-29", null, ""],
		["\uFEFF2026-01-01\uFEFF", null, null],
		["\u00852026-01-01\u0085", null, null],
	];
	for (const [i, [basis, upload, deadline]] of vectors.entries())
		db.run(
			"INSERT INTO documents(id,source,sha256_hash,invoice_date,upload_datetime,retain_until) VALUES(?,'synthetic',?,?,COALESCE(?,'2026-01-01 00:00:00'),?)",
			[i + 1, `synthetic-${i}`, basis, upload, deadline],
		);
	db.exec(
		"INSERT INTO bank_transactions(id,transaction_date,booking_date,text,amount,raw_json) VALUES(1,'2026-07-01',NULL,'Synthetic',1.25,'opaque'),(2,'2026-07-01','invalid','Synthetic',2.5,'opaque')",
	);
	db.exec(
		"INSERT INTO audit_log(id,event_type,entity_type,message,actor,created_at) VALUES(40,'synthetic','synthetic','Keep blob/type/ID evidence','agent:synthetic','2020-01-01 01:02:03')",
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
function publicRun(company: string) {
	const before = inventory(company);
	const r = run(
		[
			join(root, "target/debug/rentemester-rust"),
			"rehearse-company-migration",
			"--company",
			company,
		],
		{
			TMPDIR: copies,
			RENTEMESTER_TODAY: "1900-01-01",
			RENTEMESTER_ACTOR: "agent:hostile",
		},
	);
	assert.deepEqual(inventory(company), before);
	assert.deepEqual(readdirSync(copies), []);
	return { exit: r.exitCode, body: JSON.parse(r.stdout.toString()) };
}
function parity(
	label: string,
	start = 1,
	strategy = "end-year",
	edit?: (db: Database) => void,
) {
	const { company, db } = fixture(start, strategy);
	edit?.(db);
	const old = snapshot(db);
	db.close();
	const before = inventory(company);
	const oracle = join(temp, `${seq++}-oracle`),
		native = join(temp, `${seq++}-native`);
	cpSync(company, oracle, { recursive: true });
	cpSync(company, native, { recursive: true });
	const ts = new Database(dbPath(oracle));
	ts.exec("PRAGMA foreign_keys=ON");
	ts.transaction(() => {
		seedNativeAccountRoles(ts);
		backfillRetentionDeadlines(ts);
	}).immediate();
	const expected = snapshot(ts);
	ts.close();
	const output = join(temp, `${seq++}.json`);
	const r = run(
		[
			helper,
			"--exact",
			"company_baseline::tests::normalization_child",
			"--ignored",
			"--nocapture",
		],
		{ RM_NORMALIZATION_DB: dbPath(native), RM_NORMALIZATION_OUTPUT: output },
	);
	assert.equal(r.exitCode, 0, r.stdout.toString() + r.stderr.toString());
	const result = JSON.parse(readFileSync(output, "utf8"));
	assert.deepEqual(
		normalizeTimes(result.tables, old),
		normalizeTimes(expected, old),
	);
	const product = publicRun(company);
	assert.equal(product.exit, 0, JSON.stringify(product));
	assert.deepEqual(product.body.data.normalization, result.effects);
	assert.equal(
		product.body.data.business_data_digest_before,
		product.body.data.business_data_digest_after,
	);
	assert.deepEqual(inventory(company), before);
	// Rerun on the SAME normalized copy: exact data/timestamps/IDs remain unchanged.
	const stable = readFileSync(dbPath(native));
	const again = run(
		[
			helper,
			"--exact",
			"company_baseline::tests::normalization_child",
			"--ignored",
			"--nocapture",
		],
		{ RM_NORMALIZATION_DB: dbPath(native), RM_NORMALIZATION_OUTPUT: output },
	);
	assert.equal(again.exitCode, 0);
	const after = JSON.parse(readFileSync(output, "utf8"));
	assert.equal(after.effects.role_mappings_inserted, 0);
	assert.equal(after.effects.document_deadlines_filled, 0);
	assert.equal(after.effects.bank_deadlines_filled, 0);
	assert.deepEqual(readFileSync(dbPath(native)), stable);
	passed.push(label);
}
try {
	for (let start = 1; start <= 12; start++)
		for (const strategy of ["end-year", "start-year", "span"])
			parity(
				`fiscal start${start}/${strategy}: typed TS parity, corrected dates, source preservation, idempotence`,
				start,
				strategy,
			);
	parity(
		"all mapping/proposal statuses prevent native reseeding",
		7,
		"span",
		(db) =>
			db.exec(
				"INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by,confirmation_source,confirmed_at) VALUES('bank','2000','inactive',4,'agent:synthetic','explicit','2020-01-01 00:00:00'),('creditors','7000','superseded',7,'system','native_seed','2020-01-01 00:00:00'),('operational_default','3000','confirmed',2,'agent:synthetic','explicit','2020-01-01 00:00:00');INSERT INTO account_role_proposals(role,account_no,source,status,proposed_at) VALUES('debtors','1100','synthetic','rejected','2020-01-01 00:00:00'),('output_vat','1200','synthetic','accepted','2020-01-01 00:00:00'),('input_vat','4000','synthetic','proposed','2020-01-01 00:00:00')",
			),
	);
	parity(
		"missing/inactive/wrong-type/wrong-balance accounts never seed",
		3,
		"span",
		(db) =>
			db.exec(
				"DELETE FROM accounts WHERE account_no='2000';UPDATE accounts SET active=0 WHERE account_no='1100';UPDATE accounts SET type='asset' WHERE account_no='7000';UPDATE accounts SET normal_balance='debit' WHERE account_no='1200'",
			),
	);
	parity("no company uses January default", 1, "end-year", (db) =>
		db.exec("DELETE FROM companies"),
	);
	parity("lowest company ID supplies fiscal month", 3, "end-year", (db) =>
		db.exec(
			"INSERT INTO companies(id,name,fiscal_year_start_month) VALUES(-2,'Synthetic lowest',7)",
		),
	);
	parity("fractional fiscal month falls back to January", 3, "end-year", (db) =>
		db.exec("UPDATE companies SET fiscal_year_start_month=1.5"),
	);

	parity(
		"exact unsafe-in-JS document and bank IDs retain separate deadlines",
		1,
		"end-year",
		(db) =>
			db.exec(
				"INSERT INTO documents(id,source,sha256_hash,invoice_date) VALUES(9007199254740992,'synthetic','large-a','2026-01-01'),(9007199254740993,'synthetic','large-b','2027-01-01');INSERT INTO documents(id,source,sha256_hash,invoice_date,retain_until) VALUES(9007199254740994,'synthetic','large-preserve','2026-01-01','existing-preserved'),(9007199254740995,'synthetic','large-neighbor','2027-01-01',NULL);INSERT INTO bank_transactions(id,transaction_date,text,amount) VALUES(9007199254740992,'2026-01-01','Synthetic large A',1),(9007199254740993,'2027-01-01','Synthetic large B',2)",
			),
	);

	// Invalid CVR is only inspected when a valid missing deadline needs settings.
	parity(
		"invalid CVR remains opaque when no backfill is needed",
		1,
		"end-year",
		(db) =>
			db.exec(
				"UPDATE companies SET cvr='invalid';UPDATE documents SET retain_until='preserve';UPDATE bank_transactions SET retain_until='preserve'",
			),
	);
	for (const [label, edit] of [
		[
			"overflow refuses without source/cleanup effects",
			(db: Database) =>
				db.exec("UPDATE documents SET invoice_date='9999-01-01' WHERE id=1"),
		],
		[
			"eligible backfill with invalid CVR refuses",
			(db: Database) => db.exec("UPDATE companies SET cvr='invalid'"),
		],
		[
			"referenced bank guard refuses, preserving original source",
			(db: Database) =>
				db.exec(
					"INSERT INTO journal_entries(id,entry_no,transaction_date,text,source_bank_transaction_id,entry_hash,rule_version) VALUES(1,'SYN-1','2026-01-01','Synthetic',1,'synthetic','synthetic')",
				),
		],
	] as const) {
		const { company, db } = fixture();
		edit(db);
		assert.throws(() =>
			db
				.transaction(() => {
					seedNativeAccountRoles(db);
					backfillRetentionDeadlines(db);
				})
				.immediate(),
		);
		db.close();
		const r = publicRun(company);
		assert.equal(r.exit, 1, JSON.stringify(r));
		passed.push(label);
	}
	// Explicit independently fixed calendar expectations supplement differential parity.
	const { db } = fixture(3);
	for (const [basis, expected] of [
		["2024-02-29", "2029-02-28"],
		["2027-02-28", "2032-02-29"],
		[" 0001-07-01 ", "0007-02-28"],
	])
		assert.equal(retainUntilForDate(db, basis!), expected);
	db.close();
	passed.push("fixed corrected calendar/leap/low-year expectations");
	console.log(
		JSON.stringify(
			{
				scope:
					"native company baseline data effects and explicit corrected retention calendar",
				checks: passed.length,
				passed,
				platform: process.platform,
				architecture: process.arch,
				limits: [
					"Canonical early schema admission only; historical physical normalization/replay pending",
					"Private-copy atomic normalization; existing stored deadlines preserved",
					"Generated NEW seed/audit timestamps validated then normalized only for independent comparisons",
					"No live writer authority or full R12/platform completion",
				],
			},
			null,
			2,
		),
	);
} finally {
	rmSync(temp, { recursive: true, force: true });
}
