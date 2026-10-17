/** Genuine synthetic exact-artifact prefixes, no production workspace or credentials. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
	cpSync,
	lstatSync,
	mkdirSync,
	mkdtempSync,
	readdirSync,
	readFileSync,
	rmSync,
	symlinkSync,
	writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Database } from "bun:sqlite";
import {
	openWorkspaceControlDb,
	validateWorkspaceControlMigrationHistory,
} from "../../src/core/workspace-control";

const root = resolve(import.meta.dir, "../..");
const cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const build = Bun.spawnSync(
	[cargo, "build", "--locked", "-p", "rentemester-cli"],
	{ cwd: root, stdout: "pipe", stderr: "pipe" },
);
assert.equal(build.exitCode, 0, build.stderr.toString());
const binary = join(root, "target/debug/rentemester-rust");
const temp = mkdtempSync(join(tmpdir(), "rentemester-control-migrations-")),
	copies = join(temp, "private-copies");
mkdirSync(copies);
const passed: string[] = [];
let seq = 0;
const canaries = [
	"synthetic-CREDENTIAL-CANARY",
	"synthetic-enabled-user",
	"synthetic-disabled-user",
	"synthetic-session-one",
	"synthetic-session-two",
];
const artifacts = readdirSync(join(root, "src/core/workspace-migrations"))
	.filter((f) => f.endsWith(".json"))
	.sort()
	.map((file) => {
		const bytes = readFileSync(
			join(root, "src/core/workspace-migrations", file),
		);
		const a = JSON.parse(bytes.toString("utf8"));
		return { ...a, checksum: createHash("sha256").update(bytes).digest("hex") };
	});
assert.equal(artifacts.length, 30);
validateWorkspaceControlMigrationHistory(artifacts);
const path = (workspace: string) =>
	join(workspace, ".rentemester/workspace-control.sqlite");
const q = (name: string) => `"${name.replaceAll('"', '""')}"`;
type Schema = { kind: string; name: string; table: string; sql: string };
const schema = (db: Database) =>
	db
		.query(
			"SELECT type AS kind,name,tbl_name AS 'table',sql FROM sqlite_schema WHERE substr(name,1,7) <> 'sqlite_' ORDER BY name",
		)
		.all() as Schema[];
function inventory(dir: string, prefix = ""): Record<string, string> {
	const result: Record<string, string> = {};
	for (const e of readdirSync(dir, { withFileTypes: true }).sort((a, b) =>
		a.name.localeCompare(b.name),
	)) {
		const f = join(dir, e.name),
			key = prefix + e.name,
			mode = lstatSync(f).mode & 0o777;
		if (e.isDirectory()) {
			result[`${key}/`] = `directory:${mode}`;
			Object.assign(result, inventory(f, `${key}/`));
		} else if (e.isSymbolicLink()) result[key] = "symlink";
		else
			result[key] =
				`${mode}:` + createHash("sha256").update(readFileSync(f)).digest("hex");
	}
	return result;
}
function invoke(workspace: string, extra: string[] = []) {
	const p = Bun.spawnSync(
		[binary, "rehearse-control-migration", "--workspace", workspace, ...extra],
		{
			stdout: "pipe",
			stderr: "pipe",
			timeout: 20000,
			env: { ...process.env, TMPDIR: copies },
		},
	);
	assert.deepEqual(
		readdirSync(copies),
		[],
		"private copies must be deleted on success/refusal",
	);
	assert.equal(p.stderr.length, 0, p.stderr.toString());
	const text = p.stdout.toString();
	assert.equal(text.trim().split("\n").length, 1);
	for (const canary of canaries)
		assert(!text.includes(canary), "credential/identifier leaked");
	assert(
		!text.includes("data_digest"),
		"secret-table digests must remain internal",
	);
	return { exit: p.exitCode, body: JSON.parse(text) };
}
function seed(db: Database, version: number) {
	if (version >= 1)
		db.run(
			"INSERT INTO workspace_audit(event_type,entity_type,actor) VALUES ('synthetic','test','agent:synthetic')",
		);
	if (version >= 2) {
		for (const [id, name, email, enabled] of [
			[canaries[1], "Enabled", "enabled@example.invalid", 1],
			[canaries[2], "Disabled", "disabled@example.invalid", 0],
		] as const) {
			db.run(
				"INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES (?,?,?,1,'2026-01-01','2026-01-01',?)",
				[id, name, email, version >= 5 ? 0 : enabled],
			);
		}
		if (version >= 5)
			db.run("UPDATE user SET twoFactorEnabled=1 WHERE id=?", [canaries[1]]);
		for (const [id, token, user] of [
			[canaries[3], canaries[0] + "-one", canaries[1]],
			[canaries[4], canaries[0] + "-two", canaries[2]],
		])
			db.run(
				"INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES (?,'2026-12-31',?,'2026-01-01','2026-01-01',?)",
				[id, token, user],
			);
		db.run(
			"INSERT INTO account(id,issuer,accountId,providerId,userId,password,accessToken,refreshToken,idToken,createdAt,updatedAt) VALUES ('a','synthetic','external','credential',?,?,?,?,?,'2026-01-01','2026-01-01')",
			[canaries[1], ...Array(4).fill(canaries[0])],
		);
		db.run(
			"INSERT INTO verification(id,identifier,value,expiresAt,createdAt,updatedAt) VALUES ('v','synthetic',?,'2026-12-31','2026-01-01','2026-01-01')",
			[canaries[0]],
		);
		db.run(
			"INSERT INTO twoFactor(id,secret,backupCodes,userId) VALUES ('t',?,?,?)",
			[canaries[0], canaries[0], canaries[1]],
		);
	}
	if (version >= 6)
		db.run(
			"INSERT INTO rm_workspace_auth_telemetry_events(id,endpoint,outcome,user_id,session_id,identity_hash,identity_key_version,created_at) VALUES (7,'sign-in-email','accepted',?,?,?,3,'2026-01-02 03:04:05')",
			[canaries[1], canaries[3], "a".repeat(64)],
		);
	if (version >= 17)
		db.run(
			"INSERT INTO apikey(id,configId,referenceId,key,createdAt,updatedAt) VALUES ('synthetic-key','synthetic',?,?,'2026-01-01','2026-01-01')",
			[canaries[1], canaries[0]],
		);
	if (version >= 15)
		db.run(
			"INSERT INTO rm_party_company_roles(party_id,company_slug,role,defaults_json) VALUES ('synthetic-party','synthetic-company','vendor','{\"synthetic\":true}')",
		);
}
function fixture(version: number) {
	const workspace = join(temp, `${seq++}-prefix${version}`);
	mkdirSync(join(workspace, ".rentemester"), { recursive: true });
	mkdirSync(join(workspace, "companies/synthetic/data"), { recursive: true });
	writeFileSync(
		join(workspace, "companies/synthetic/data/ledger.sqlite"),
		"company sentinel must never open",
	);
	const db = new Database(path(workspace));
	db.exec("PRAGMA foreign_keys=ON");
	for (const a of artifacts.slice(0, version)) {
		db.exec(a.sql);
		db.run(
			"INSERT INTO workspace_schema_migrations(id,name,checksum,applied_at,applied_by_version) VALUES (?,?,?,'2026-01-01 00:00:00','synthetic-prior')",
			[a.id, a.name, a.checksum],
		);
	}
	seed(db, version);
	db.close();
	return workspace;
}
function dropHistoryGuards(db: Database) {
	db.exec(
		"DROP TRIGGER workspace_schema_migrations_no_update;DROP TRIGGER workspace_schema_migrations_no_delete",
	);
}
function reject(
	label: string,
	edit: (db: Database) => void,
	code = "unsupported_schema",
	version = 30,
) {
	const workspace = fixture(version),
		db = new Database(path(workspace));
	edit(db);
	db.close();
	const before = inventory(workspace),
		result = invoke(workspace);
	assert.equal(result.exit, 1, JSON.stringify(result));
	assert.equal(result.body.code, code, JSON.stringify(result));
	assert.deepEqual(inventory(workspace), before);
	passed.push(label);
}
function rows(db: Database, table: string) {
	return db
		.query(`SELECT * FROM ${q(table)}`)
		.all()
		.map((r) => JSON.stringify(r))
		.sort();
}
try {
	for (let version = 0; version <= 30; version++) {
		const workspace = fixture(version),
			before = inventory(workspace),
			result = invoke(workspace);
		assert.equal(result.exit, 0, JSON.stringify(result));
		assert.equal(
			result.body.format,
			"rentemester-control-migration-rehearsal/1",
		);
		const data = result.body.data;
		assert.deepEqual(
			Object.keys(data).sort(),
			[
				"from_version",
				"to_version",
				"applied",
				"canonical_schema_sha256",
				"schema_objects",
				"prior_data_preserved",
				"prior_provenance_preserved",
				"sessions_invalidated",
				"mfa_events_backfilled",
				"private_copy_discarded",
				"writer_authority",
			].sort(),
		);
		assert.equal(data.from_version, version);
		assert.equal(data.to_version, 30);
		assert.equal(data.writer_authority, false);
		assert.equal(data.private_copy_discarded, true);
		assert.equal(data.prior_data_preserved, true);
		assert.equal(data.prior_provenance_preserved, true);
		assert.equal(
			data.sessions_invalidated,
			version >= 2 && version < 5 ? 2 : 0,
		);
		assert.equal(
			data.mfa_events_backfilled,
			version >= 2 && version < 5 ? 1 : 0,
		);
		assert.deepEqual(inventory(workspace), before);
		const ts = join(temp, `${seq++}-ts`);
		cpSync(workspace, ts, { recursive: true });
		const prior = new Database(path(ts));
		const oldHistory = version
			? prior
					.query("SELECT * FROM workspace_schema_migrations ORDER BY id")
					.all()
			: [];
		const oldTables = schema(prior)
			.filter(
				(o) =>
					o.kind === "table" &&
					o.name !== "workspace_schema_migrations" &&
					!(version < 5 && o.name === "session"),
			)
			.map((o) => o.name);
		const oldData = Object.fromEntries(
			oldTables.map((t) => [t, rows(prior, t)]),
		);
		prior.close();
		const db = openWorkspaceControlDb(ts);
		const expectedSchema = schema(db);
		assert.equal(
			data.canonical_schema_sha256,
			createHash("sha256").update(JSON.stringify(expectedSchema)).digest("hex"),
		);
		assert.equal(data.schema_objects, expectedSchema.length);
		assert.equal(data.schema_objects, 170);
		assert.deepEqual(
			data.applied,
			db
				.query(
					"SELECT id,name,checksum FROM workspace_schema_migrations WHERE id>? ORDER BY id",
				)
				.all(version),
		);
		assert.deepEqual(
			db
				.query(
					"SELECT * FROM workspace_schema_migrations WHERE id<=? ORDER BY id",
				)
				.all(version),
			oldHistory,
		);
		for (const t of oldTables)
			assert.deepEqual(
				rows(db, t),
				oldData[t],
				`TS source data changed in ${t}`,
			);
		assert.equal(
			(db.query("SELECT COUNT(*) AS n FROM session").get() as { n: number }).n,
			version < 5 ? 0 : 2,
		);
		db.close();
		assert.deepEqual(invoke(workspace).body.data, data);
		assert.deepEqual(inventory(workspace), before);
		passed.push(
			`exact prefix${version} ->30 TS schema/history parity, opaque data preservation, explicit session effects and repeat`,
		);
	}
	for (const [label, sql] of [
		[
			"history checksum tamper",
			"UPDATE workspace_schema_migrations SET checksum='changed' WHERE id=2",
		],
		[
			"history name tamper",
			"UPDATE workspace_schema_migrations SET name='changed' WHERE id=2",
		],
		["history gap", "DELETE FROM workspace_schema_migrations WHERE id=2"],
		[
			"newer history",
			"INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version) VALUES (31,'newer','newer','synthetic')",
		],
	])
		reject(label, (db) => {
			dropHistoryGuards(db);
			db.exec(sql);
		});
	reject(
		"empty provenance",
		(db) => {
			dropHistoryGuards(db);
			db.exec(
				"UPDATE workspace_schema_migrations SET applied_by_version='' WHERE id=2",
			);
		},
		"invalid_ledger",
	);
	reject("missing history", (db) => {
		dropHistoryGuards(db);
		db.exec("DROP TABLE workspace_schema_migrations");
	});
	reject("legal sqliteShadow table is not hidden as internal", (db) =>
		db.exec("CREATE TABLE sqliteShadow(secret TEXT)"),
	);
	reject(
		"populated sqliteShadow-only source cannot adopt prefix0",
		(db) => db.exec("CREATE TABLE sqliteShadow(secret TEXT)"),
		"unsupported_schema",
		0,
	);
	reject("legal sqliteShadow trigger is not hidden as internal", (db) =>
		db.exec(
			"CREATE TRIGGER sqliteShadow BEFORE INSERT ON workspace_audit BEGIN SELECT 1; END",
		),
	);
	reject("missing canonical guard", (db) =>
		db.exec("DROP TRIGGER workspace_audit_no_update"),
	);
	reject("altered canonical guard", (db) =>
		db.exec(
			"DROP TRIGGER workspace_audit_no_update;CREATE TRIGGER workspace_audit_no_update BEFORE UPDATE ON workspace_audit BEGIN SELECT 1;END",
		),
	);
	reject("unexpected object and secret error sanitization", (db) =>
		db.exec(
			`CREATE TABLE synthetic_extra(secret TEXT DEFAULT '${canaries[0]}')`,
		),
	);
	reject("changed canonical constraints", (db) =>
		db.exec("ALTER TABLE workspace_audit ADD COLUMN extra TEXT"),
	);
	reject("empty history with existing objects", (db) => {
		dropHistoryGuards(db);
		db.exec("DELETE FROM workspace_schema_migrations");
	});
	reject(
		"FK corruption",
		(db) => {
			db.exec("PRAGMA foreign_keys=OFF");
			db.run(
				"INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES ('bad','2026-12-31','bad','2026-01-01','2026-01-01','missing')",
			);
		},
		"invalid_ledger",
	);
	reject(
		"valid prefix28 conflicting assignments refuses without deduplication",
		(db) => {
			db.exec(
				"INSERT INTO rm_workspace_inbox_sources VALUES ('synthetic-source','synthetic-company','synthetic-key',X'00','synthetic-hash','synthetic.pdf','application/pdf','upload',NULL,'2026-01-01','{}','agent:synthetic','2026-01-01');INSERT INTO rm_workspace_inbox_assignments(source_id,company_slug,state,assigned_by,assigned_at) VALUES ('synthetic-source','synthetic-a','approved','agent:synthetic','2026-01-01'),('synthetic-source','synthetic-b','completed','agent:synthetic','2026-01-01')",
			);
		},
		"migration_failed",
		28,
	);
	const active = fixture(4),
		walDb = new Database(path(active));
	walDb.exec("PRAGMA journal_mode=WAL;PRAGMA wal_autocheckpoint=0");
	walDb.run(
		"INSERT INTO verification(id,identifier,value,expiresAt,createdAt,updatedAt) VALUES ('wal','opaque',?,'2026-12-31','2026-01-01','2026-01-01')",
		[canaries[0]],
	);
	const wal = inventory(active);
	assert(Object.keys(wal).some((f) => f.endsWith("-wal")));
	assert.equal(invoke(active).exit, 0);
	assert.deepEqual(inventory(active), wal);
	walDb.close();
	passed.push(
		"private recovery with active source WAL/SHM; file names, bytes and modes unchanged",
	);
	const corrupt = fixture(0);
	writeFileSync(path(corrupt), "not a SQLite database " + canaries[0]);
	const corruptBefore = inventory(corrupt),
		corruptResult = invoke(corrupt);
	assert.equal(corruptResult.exit, 1);
	assert.equal(corruptResult.body.code, "invalid_ledger");
	assert.deepEqual(inventory(corrupt), corruptBefore);
	passed.push(
		"corrupt private recovery refuses with opaque error and checked cleanup",
	);
	const missingExisting = join(temp, "existing-with-missing-control");
	mkdirSync(join(missingExisting, ".rentemester"), { recursive: true });
	const missingBefore = inventory(missingExisting);
	assert.equal(invoke(missingExisting).body.code, "source_unavailable");
	assert.deepEqual(inventory(missingExisting), missingBefore);
	const source = fixture(4),
		link = join(temp, "workspace-link");
	symlinkSync(source, link, "dir");
	for (const target of [link, `${link}/`, `${link}/.`, join(temp, "missing")])
		assert.equal(invoke(target).body.code, "source_unavailable");
	passed.push("missing/final symlink workspace roots refuse without creation");
	const directoryLink = join(temp, "directory-link");
	mkdirSync(directoryLink);
	symlinkSync(
		join(source, ".rentemester"),
		join(directoryLink, ".rentemester"),
		"dir",
	);
	assert.equal(invoke(directoryLink).body.code, "source_unavailable");
	const fileLink = join(temp, "file-link");
	mkdirSync(join(fileLink, ".rentemester"), { recursive: true });
	symlinkSync(path(source), path(fileLink));
	assert.equal(invoke(fileLink).body.code, "source_unavailable");
	passed.push("control directory/file links refuse");
	const sidecar = fixture(4),
		target = join(temp, "sidecar-target");
	writeFileSync(target, "opaque");
	symlinkSync(target, `${path(sidecar)}-wal`);
	const beforeSide = inventory(sidecar);
	assert.equal(invoke(sidecar).body.code, "source_unavailable");
	assert.deepEqual(inventory(sidecar), beforeSide);
	assert.equal(readFileSync(target, "utf8"), "opaque");
	passed.push("linked sidecar refuses without following or rewriting it");
	for (const args of [
		[],
		["--workspace"],
		["--workspace", source, "--target", "30"],
		["--workspace", ""],
		["--workspace", source, "--actor", "agent:synthetic"],
	]) {
		const p = Bun.spawnSync([binary, "rehearse-control-migration", ...args], {
			stdout: "pipe",
			stderr: "pipe",
		});
		assert.equal(p.exitCode, 2);
		assert.equal(JSON.parse(p.stdout.toString()).code, "invalid_input");
	}
	passed.push("strict CLI usage refuses extra mutation/target flags");
	const storageBefore = inventory(source),
		badStorage = Bun.spawnSync(
			[binary, "rehearse-control-migration", "--workspace", source],
			{
				stdout: "pipe",
				stderr: "pipe",
				timeout: 20000,
				env: { ...process.env, TMPDIR: path(source) },
			},
		);
	assert.equal(badStorage.exitCode, 1);
	assert.equal(
		JSON.parse(badStorage.stdout.toString()).code,
		"storage_failure",
	);
	assert.deepEqual(inventory(source), storageBefore);
	passed.push("unusable private storage refuses without source effects");
	console.log(
		JSON.stringify(
			{
				ok: true,
				scope: "all control prefixes0–30 on private copies; no live writer",
				checks: passed.length,
				passed,
				platform: process.platform,
				architecture: process.arch,
				limits: [
					"Synthetic direct-prefix fixtures, not historical production backups",
					"No source migrations, company baseline port, live writer authority or full R12 acceptance",
					"Process crash qualification is separate from hardware power-loss and storage qualification",
				],
			},
			null,
			2,
		),
	);
} finally {
	rmSync(temp, { recursive: true, force: true });
}
