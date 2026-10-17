/** Exact normalized-prefix evidence; deliberately no production archive/adoption claim. */
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
import {
	normalizeCompanyBaseline,
	finalizeCompanySchema,
} from "../../src/core/db";
import {
	applySchemaMigrations,
	supportedSchemaMigrations,
} from "../../src/core/schema-version";
const root = resolve(import.meta.dir, "../.."),
	cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const build = Bun.spawnSync(
	[cargo, "build", "--locked", "-p", "rentemester-cli"],
	{ cwd: root, stdout: "pipe", stderr: "pipe" },
);
assert.equal(build.exitCode, 0, build.stderr.toString());
const binary = join(root, "target/debug/rentemester-rust"),
	temp = mkdtempSync(join(tmpdir(), "rentemester-company-baseline-")),
	copies = join(temp, "copies");
mkdirSync(copies);
const expected = supportedSchemaMigrations(),
	artifacts = readdirSync(join(root, "src/core/migrations"))
		.filter((f) => f.endsWith(".json"))
		.sort()
		.slice(1)
		.map(
			(f) =>
				JSON.parse(
					readFileSync(join(root, "src/core/migrations", f), "utf8"),
				) as { sql: string },
		);
const passed: string[] = [];
let seq = 0;
const dbPath = (company: string) => join(company, "data/ledger.sqlite"),
	q = (name: string) => `"${name.replaceAll('"', '""')}"`;
type Schema = { kind: string; name: string; table: string; sql: string };
const schema = (db: Database) =>
	db
		.query(
			"SELECT type AS kind,name,tbl_name AS 'table',sql FROM sqlite_schema WHERE substr(name,1,7)<>'sqlite_' ORDER BY name",
		)
		.all() as Schema[];
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
function invoke(company: string) {
	const p = Bun.spawnSync(
		[binary, "rehearse-company-migration", "--company", company],
		{
			stdout: "pipe",
			stderr: "pipe",
			timeout: 20000,
			env: { ...process.env, TMPDIR: copies },
		},
	);
	assert.deepEqual(readdirSync(copies), []);
	assert.equal(p.stderr.length, 0, p.stderr.toString());
	assert.equal(p.stdout.toString().trim().split("\n").length, 1);
	return { exit: p.exitCode, body: JSON.parse(p.stdout.toString()) };
}
function seed(db: Database, version: number) {
	if (!version) return;
	db.exec(
		"INSERT INTO companies(id,name) VALUES(1,'Synthetic company');INSERT INTO documents(id,source,sha256_hash,retain_until) VALUES(1,'synthetic','synthetic-document','2031-12-31');INSERT INTO exceptions(type,message) VALUES('synthetic','Keep this original value');INSERT INTO recurring_invoice_templates(id,name,interval,first_issue_date,next_issue_date,payload_json) VALUES(1,'Synthetic recurring','monthly','2026-01-01','2026-01-01','{}');INSERT INTO recurring_invoice_generations(id,template_id,period_index,document_id,invoice_number,issue_date) VALUES(1,1,0,1,'SYNTHETIC-1','2026-01-01')",
	);
	if (version >= 3)
		db.exec(
			"INSERT INTO recurring_invoice_generation_claims(template_id,period_index) VALUES(1,0)",
		);
	if (version >= 13) {
		db.run(
			"INSERT INTO bookkeeping_batch_runs(id,run_key,company_id,accounting_from,accounting_to,bank_from,bank_to,plan_hash,plan_json,created_at) VALUES(1,'synthetic-run',1,'2026-01-01','2026-12-31','2026-01-01','2026-12-31',?,'{}','2026-01-01')",
			["a".repeat(64)],
		);
		db.exec(
			"INSERT INTO bookkeeping_batch_final_checks(id,run_id,check_name,ok,detail_json,created_at) VALUES(1,1,'trial_balance',1,'{}','2026-01-01')",
		);
	}
	if (version >= 26)
		db.run(
			"INSERT INTO document_party_link_events(id,document_id,party_id,party_role,event_type,evidence_kind,evidence_json,document_sha256,document_payload_sha256,party_snapshot_json,plan_hash,actor,principal,created_at) VALUES(1,1,'synthetic-party','customer','linked','exact_identifier','{}',?,?,'{}',?,'agent:synthetic','agent:synthetic','2026-01-01')",
			Array(3).fill("a".repeat(64)),
		);
	if (version >= 14)
		db.exec(
			"INSERT INTO invoice_extraction_documents(id,document_id,company_id,sha256_hash,pdf_bytes,byte_length,created_at) VALUES(1,1,1,'synthetic-extraction',X'00FF',2,'2026-01-01');INSERT INTO invoice_extraction_attempts(id,extraction_document_id,extractor_id,extractor_version,status,created_at) VALUES(1,1,'synthetic','1','completed','2026-01-01');INSERT INTO invoice_extraction_results(id,attempt_id,result_json,created_at) VALUES(1,1,'{}','2026-01-01')",
		);
	if (version >= 21)
		db.exec(
			"UPDATE bookkeeping_batch_approval_policy SET self_approval_allowed=1 WHERE id=1",
		);
	if (version >= 28)
		db.exec(
			"INSERT INTO accounting_dimension_definition_events(dimension_id,kind,name,event_type,actor,principal,created_at) VALUES('synthetic','project','Synthetic','defined','agent:synthetic','agent:synthetic','2026-01-01');INSERT INTO accounting_dimension_member_events(dimension_id,member_id,name,status,event_type,actor,principal,created_at) VALUES('synthetic','member','Synthetic','inactive','defined','agent:synthetic','agent:synthetic','2026-01-01')",
		);
}
function fixture(version: number) {
	const company = join(temp, `${seq++}-v${version}`);
	mkdirSync(join(company, "data"), { recursive: true });
	const db = new Database(dbPath(company));
	db.exec("PRAGMA foreign_keys=ON");
	if (version) {
		normalizeCompanyBaseline(db);
		for (let i = 2; i <= version; i++) {
			db.exec(artifacts[i - 2].sql);
			const m = expected[i - 1]!;
			db.run(
				"INSERT INTO schema_migrations(id,name,checksum,applied_by_version) VALUES(?,?,?,'synthetic-prior')",
				[m.id, m.name, m.checksum],
			);
		}
	}
	seed(db, version);
	db.close();
	return company;
}
function projectedRows(db: Database, table: string, columns: string[]) {
	return db
		.query(`SELECT ${columns.map(q).join(",")} FROM ${q(table)}`)
		.all()
		.map((r) => JSON.stringify(r))
		.sort();
}

function reject(label: string, edit: (db: Database) => void, version = 2) {
	const company = fixture(version),
		db = new Database(dbPath(company));
	edit(db);
	db.close();
	const before = inventory(company),
		r = invoke(company);
	assert.equal(r.exit, 1, JSON.stringify(r));
	assert.equal(r.body.code, "unsupported_schema", JSON.stringify(r));
	assert.deepEqual(inventory(company), before);
	passed.push(label);
}
try {
	for (let version = 0; version < 32; version++) {
		const company = fixture(version),
			before = inventory(company),
			result = invoke(company);
		assert.equal(result.exit, 0, JSON.stringify(result));
		const data = result.body.data;
		assert.equal(data.schema_scope, "full-normalized-early-prefix");
		assert.equal(data.from_version, version);
		assert.equal(data.to_version, 36);
		assert.equal(data.private_copy_discarded, true);
		assert.equal(data.writer_authority, false);
		assert.equal(
			data.business_data_digest_before,
			data.business_data_digest_after,
		);
		assert.deepEqual(inventory(company), before);
		const ts = join(temp, `${seq++}-ts`);
		cpSync(company, ts, { recursive: true });
		const db = new Database(dbPath(ts));
		db.exec("PRAGMA foreign_keys=ON");
		const oldHistory = version
			? db.query("SELECT * FROM schema_migrations ORDER BY id").all()
			: [];
		const old = schema(db)
			.filter((o) => o.kind === "table" && o.name !== "schema_migrations")
			.map((o) => {
				const cols = (
					db.query(`PRAGMA table_info(${q(o.name)})`).all() as {
						name: string;
					}[]
				).map((c) => c.name);
				return { table: o.name, cols, rows: projectedRows(db, o.name, cols) };
			});
		if (!version) normalizeCompanyBaseline(db);
		applySchemaMigrations(db);
		finalizeCompanySchema(db);
		assert.deepEqual(data.affected_schema, schema(db));
		assert.deepEqual(
			data.applied,
			db
				.query(
					"SELECT id,name,checksum FROM schema_migrations WHERE id>? ORDER BY id",
				)
				.all(version),
		);
		assert.deepEqual(
			db
				.query("SELECT * FROM schema_migrations WHERE id<=? ORDER BY id")
				.all(version),
			oldHistory,
		);
		for (const table of old)
			assert.deepEqual(projectedRows(db, table.table, table.cols), table.rows);
		const identity = db
			.query("SELECT ledger_uuid FROM ledger_identity WHERE id=1")
			.get() as { ledger_uuid: string };
		assert.match(identity.ledger_uuid, /^[0-9a-f]{32}$/);
		if (version < 3) {
			assert.deepEqual(
				db
					.query(
						"SELECT interval_count,delivery_channel FROM recurring_invoice_templates",
					)
					.get(),
				version ? { interval_count: 1, delivery_channel: "manual" } : null,
			);
			assert.equal(
				(
					db
						.query(
							"SELECT COUNT(*) AS n FROM recurring_invoice_generation_claims",
						)
						.get() as { n: number }
				).n,
				version ? 1 : 0,
			);
		}
		db.close();
		assert.deepEqual(invoke(company).body.data, data);
		assert.deepEqual(inventory(company), before);
		passed.push(
			`normalized prefix${version} ->36 exact TS post-baseline DDL/history, original columns and defaults; repeat/source preserved`,
		);
	}
	reject(
		"populated unversioned state is not silently adopted",
		(db) => db.exec("CREATE TABLE sqliteShadow(value TEXT)"),
		0,
	);
	reject("early unknown object refuses", (db) =>
		db.exec("CREATE TABLE sqliteShadow(value TEXT)"),
	);
	reject("early altered canonical guard refuses", (db) =>
		db.exec("DROP TRIGGER audit_log_no_update"),
	);
	reject("early unsupported physical variant refuses", (db) =>
		db.exec("ALTER TABLE companies ADD COLUMN unknown TEXT"),
	);
	reject("early history checksum refuses", (db) =>
		db.exec("UPDATE schema_migrations SET checksum='changed' WHERE id=1"),
	);
	reject("early history gap refuses", (db) =>
		db.exec("DELETE FROM schema_migrations WHERE id=1"),
	);
	const active = fixture(2),
		db = new Database(dbPath(active));
	db.exec(
		"PRAGMA journal_mode=WAL;PRAGMA wal_autocheckpoint=0;INSERT INTO exceptions(type,message) VALUES('synthetic','WAL only')",
	);
	const wal = inventory(active);
	assert(Object.keys(wal).some((f) => f.endsWith("-wal")));
	assert.equal(invoke(active).exit, 0);
	assert.deepEqual(inventory(active), wal);
	db.close();
	passed.push(
		"early active WAL privately recovered and source inventories unchanged",
	);
	const source = fixture(2),
		link = join(temp, "source-link");
	symlinkSync(source, link, "dir");
	for (const p of [link, `${link}/`, `${link}/.`])
		assert.equal(invoke(p).body.code, "source_unavailable");
	passed.push("early symlink roots refuse");
	console.log(
		JSON.stringify(
			{
				ok: true,
				scope:
					"normalized company bootstrap0 and canonical early prefixes1–31; suffix32–36 verified separately",
				checks: passed.length,
				passed,
				platform: process.platform,
				architecture: process.arch,
				limits: [
					"Post-baseline canonical fixtures, not arbitrary historical business normalization/adoption",
					"Observed exact current DDL includes corrected strict purchase guard; full immutable domain qualification remains pending",
					"No source migrations, writer authority, hardware power-loss or full R12 completion",
				],
			},
			null,
			2,
		),
	);
} finally {
	rmSync(temp, { recursive: true, force: true });
}
