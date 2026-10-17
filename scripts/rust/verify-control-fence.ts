/** Real native processes and canonical synthetic control DBs; never real company data. */
import assert from "node:assert/strict";
import {
	copyFileSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { homedir, tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { Database } from "bun:sqlite";
import {
	openWorkspaceControlDb,
	workspaceControlPaths,
} from "../../src/core/workspace-control";
import {
	authorizeWorkspaceRoute,
	hasCurrentBetterAuthSession,
} from "../../src/core/workspace-access";
const root = resolve(import.meta.dir, "../.."),
	cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const build = Bun.spawnSync(
	[
		cargo,
		"test",
		"--locked",
		"-p",
		"rentemester-application",
		"--lib",
		"--no-run",
		"--message-format=json",
	],
	{ cwd: root, stdout: "pipe", stderr: "pipe" },
);
assert.equal(build.exitCode, 0, build.stderr.toString());
const helper = build.stdout
	.toString()
	.trim()
	.split("\n")
	.map((s) => JSON.parse(s))
	.find(
		(v) => v.executable && v.target.name === "rentemester_application",
	)?.executable;
assert(helper);
const work = mkdtempSync(join(tmpdir(), "rm-control-fence-"));
const hash = (b: Buffer) => createHash("sha256").update(b).digest("hex");
const seed = join(work, "seed");
mkdirSync(seed);
const control = openWorkspaceControlDb(seed);
control.exec(`PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;
INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic U','u@example.invalid',1,'2026-01-01','2026-01-01',1),('v','Synthetic V','v@example.invalid',1,'2026-01-01','2026-01-01',1);
INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2099-01-01','fake-s','2026-01-01','2026-01-01','u'),('t','2099-01-01','fake-t','2026-01-01','2026-01-01','v');
INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor,created_at) VALUES('u','activate','member','agent:synthetic','2099-01-01');
INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor,created_at) VALUES('u','a','grant','bookkeeper','agent:synthetic','2099-01-01');`);
control.close();
const seedCompany = join(seed, "effect.sqlite"),
	ledger = new Database(seedCompany);
ledger.exec(
	"PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;CREATE TABLE sequences(kind TEXT,scope TEXT,value INTEGER,PRIMARY KEY(kind,scope));CREATE TABLE companion(n INTEGER PRIMARY KEY CHECK(n>0));",
);
ledger.close();
let checks = 0;
const observations: any[] = [];
function fixture(name: string, mode: string, sql = "") {
	const dir = join(work, name);
	mkdirSync(dir);
	mkdirSync(join(dir, ".rentemester"));
	const c = workspaceControlPaths(dir).db,
		l = join(dir, "effect.sqlite");
	copyFileSync(workspaceControlPaths(seed).db, c);
	copyFileSync(seedCompany, l);
	const db = new Database(c);
	// Unsafe mode is installed and asserted by the native child. Start its main file
	// in DELETE so that setup does not convert WAL headers inside the preservation window.
	const controlMode = sql.startsWith("PRAGMA journal_mode=") ? "DELETE" : mode;
	db.exec(
		`PRAGMA foreign_keys=ON;PRAGMA journal_mode=${controlMode};PRAGMA synchronous=FULL;`,
	);
	assert.equal(
		(db.query("PRAGMA journal_mode").get() as any).journal_mode,
		controlMode.toLowerCase(),
	);
	if (sql) db.exec(sql);
	const image = hash(db.serialize());
	db.close();
	const ledger = new Database(l);
	ledger.exec(
		`PRAGMA foreign_keys=ON;PRAGMA journal_mode=${mode};PRAGMA synchronous=FULL;`,
	);
	ledger.close();
	return {
		dir,
		c,
		l,
		image,
		output: join(dir, "output.json"),
		marker: join(dir, "marker"),
	};
}
function start(f: ReturnType<typeof fixture>, request: any = {}, point = "") {
	return Bun.spawn(
		[
			helper,
			"--ignored",
			"--exact",
			"control_fence::tests::child",
			"--nocapture",
		],
		{
			cwd: work,
			env: {
				...process.env,
				RM_FENCE_CONTROL: f.c,
				RM_FENCE_COMPANY: f.l,
				RM_FENCE_REQUEST: JSON.stringify(request),
				RM_FENCE_OUTPUT: f.output,
				RM_FENCE_POINT: point,
				RM_FENCE_MARKER: f.marker,
			},
			stdin: "pipe",
			stdout: "pipe",
			stderr: "pipe",
		},
	);
}
async function completed(child: ReturnType<typeof start>) {
	const exit = await child.exited;
	const out = await new Response(child.stdout).text();
	const err = await new Response(child.stderr).text();
	assert.equal(exit, 0, out + err);
}
async function marker(
	f: ReturnType<typeof fixture>,
	child: ReturnType<typeof start>,
) {
	const until = Date.now() + 20000;
	while (!existsSync(f.marker)) {
		assert.equal(child.exitCode, null, "child exited before marker");
		assert(
			Date.now() < until,
			"live child marker deadline expired; do not restart",
		);
		await Bun.sleep(10);
	}
}
function state(f: ReturnType<typeof fixture>) {
	const db = new Database(f.l);
	const rows = db.query("SELECT n FROM companion ORDER BY n").all();
	const seq = db.query("SELECT value FROM sequences").all();
	db.close();
	return { rows, seq };
}
function controlPreserved(f: ReturnType<typeof fixture>) {
	const db = new Database(f.c);
	assert.equal(hash(db.serialize()), f.image, f.dir);
	db.close();
}
const deny = { outcome: "not_committed", code: "access_denied" };
const cases: any[] = [
	{ id: "commit", expected: { outcome: "committed" }, effects: 1 },
	{
		id: "business-error",
		request: { work: "reject" },
		expected: { outcome: "not_committed", code: "synthetic_business" },
	},
	{
		id: "sql-error",
		request: { work: "sqlfail" },
		expected: { outcome: "not_committed", code: "synthetic_sql" },
	},
	{
		id: "manual-commit",
		request: { work: "escape" },
		expected: { outcome: "unknown", code: "transaction_escaped" },
		effects: 1,
	},
	{ id: "provider-denial", request: { authenticated: false }, expected: deny },
	{
		id: "cross-workspace",
		request: { companyWorkspace: "other" },
		expected: deny,
	},
	{
		id: "empty-workspace",
		request: { workspace: "", companyWorkspace: "" },
		expected: deny,
	},
	{ id: "empty-subject", request: { subject: "" }, expected: deny },
	{ id: "empty-session", request: { session: "" }, expected: deny },
	{ id: "wrong-session-subject", request: { session: "t" }, expected: deny },
	{ id: "unknown-subject", request: { subject: "missing" }, expected: deny },
	{
		id: "session-deleted",
		sql: "DELETE FROM session WHERE id='s'",
		expected: deny,
	},
	{
		id: "email-unverified",
		sql: "UPDATE user SET emailVerified=0 WHERE id='u'",
		expected: deny,
	},
	{
		id: "mfa-disabled",
		sql: "UPDATE user SET twoFactorEnabled=0 WHERE id='u'",
		expected: deny,
	},
	{
		id: "no-access-events",
		request: { subject: "v", session: "t" },
		expected: deny,
	},
	{
		id: "workspace-disabled",
		sql: "INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor,created_at) VALUES('u','disable',NULL,'agent:synthetic','2000-01-01')",
		expected: deny,
	},
	{
		id: "membership-revoked",
		sql: "INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor,created_at) VALUES('u','a','revoke',NULL,'agent:synthetic','2000-01-01')",
		expected: deny,
	},
	{
		id: "full-width-revoke",
		sql: "INSERT INTO rm_company_membership_events(id,user_id,company_slug,event_type,company_role,actor) VALUES(9223372036854775807,'u','a','revoke',NULL,'agent:synthetic')",
		expected: deny,
	},
	{
		id: "earlier-negative-owner",
		sql: "INSERT INTO rm_company_membership_events(id,user_id,company_slug,event_type,company_role,actor,created_at) VALUES(-9223372036854775808,'u','a','grant','owner','agent:synthetic','2200-01-01')",
		request: { permission: "company.review" },
		expected: deny,
	},
	{
		id: "workspace-owner-no-company-bypass",
		sql: "INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','workspace_owner','agent:synthetic')",
		request: { slug: "b" },
		expected: deny,
	},
	{ id: "archived", request: { archived: true }, expected: deny },
	{ id: "unregistered", request: { registered: false }, expected: deny },
	{ id: "invalid-slug", request: { slug: "../a" }, expected: deny },
	{
		id: "public-not-company-authority",
		request: { permission: "public.read" },
		expected: deny,
	},
	{
		id: "workspace-not-company-authority",
		request: { permission: "workspace.read" },
		expected: deny,
	},
	{
		id: "role-denied",
		request: { permission: "company.review" },
		expected: deny,
	},
	{ id: "untrimmed-subject", request: { subject: " u " }, expected: deny },
	{ id: "untrimmed-session", request: { session: " s " }, expected: deny },
	{
		id: "control-temp",
		request: { controlSql: "CREATE TEMP TABLE evil(x)" },
		expected: { outcome: "not_committed", code: "connection_refused" },
	},
	{
		id: "company-temp",
		request: { companySql: "CREATE TEMP TABLE evil(x)" },
		expected: { outcome: "not_committed", code: "connection_refused" },
	},
	{
		id: "attached",
		request: { controlSql: "ATTACH ':memory:' AS other" },
		expected: { outcome: "not_committed", code: "connection_refused" },
	},
	{
		id: "fk-disabled",
		request: { controlSql: "PRAGMA foreign_keys=OFF" },
		expected: { outcome: "not_committed", code: "connection_refused" },
	},
	{
		id: "weak-durability",
		request: { companySql: "PRAGMA synchronous=OFF" },
		expected: { outcome: "not_committed", code: "connection_refused" },
	},
	{
		id: "query-only",
		request: { controlSql: "PRAGMA query_only=ON" },
		expected: { outcome: "not_committed", code: "connection_refused" },
	},
	{
		id: "pinned-control",
		request: { controlSql: "BEGIN DEFERRED;SELECT * FROM user" },
		expected: { outcome: "not_committed", code: "active_transaction" },
	},
	{
		id: "company-first",
		request: { companySql: "BEGIN IMMEDIATE" },
		expected: { outcome: "not_committed", code: "active_transaction" },
	},
	{
		id: "malformed-security-flag",
		sql: "UPDATE user SET emailVerified=2 WHERE id='u'",
		expected: { outcome: "not_committed", code: "invalid_control" },
	},
	{
		id: "noncanonical-control",
		sql: "CREATE TABLE extra(x)",
		expected: { outcome: "not_committed", code: "invalid_control" },
	},
	{
		id: "control-readonly",
		request: { readonlyControl: true },
		expected: { outcome: "not_committed", code: "connection_refused" },
	},
	{
		id: "company-readonly",
		request: { readonlyCompany: true },
		expected: { outcome: "not_committed", code: "connection_refused" },
	},
	{
		id: "foreign-key-corruption",
		sql: "PRAGMA foreign_keys=OFF;INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('bad','2099','bad-token','2026','2026','missing');PRAGMA foreign_keys=ON",
		expected: { outcome: "not_committed", code: "invalid_control" },
	},
	{
		id: "control-journal-off",
		sql: "PRAGMA journal_mode=OFF",
		request: {
			controlSql: "PRAGMA journal_mode=OFF",
			expectedControlMode: "off",
		},
		expected: { outcome: "not_committed", code: "connection_refused" },
	},
	{
		id: "company-journal-off",
		request: {
			companySql: "PRAGMA journal_mode=OFF",
			expectedCompanyMode: "off",
		},
		expected: { outcome: "not_committed", code: "connection_refused" },
	},
	{
		id: "control-journal-memory",
		sql: "PRAGMA journal_mode=MEMORY",
		request: {
			controlSql: "PRAGMA journal_mode=MEMORY",
			expectedControlMode: "memory",
		},
		expected: { outcome: "not_committed", code: "connection_refused" },
	},
	{
		id: "company-journal-memory",
		request: {
			companySql: "PRAGMA journal_mode=MEMORY",
			expectedCompanyMode: "memory",
		},
		expected: { outcome: "not_committed", code: "connection_refused" },
	},
];
try {
	for (const mode of ["WAL", "DELETE"]) {
		for (const v of cases) {
			const f = fixture(`${mode}-${v.id}`, mode, v.sql);
			const request = v.request ?? {};
			writeFileSync(
				join(f.dir, "workspace.json"),
				JSON.stringify({
					version: 1,
					companies:
						request.registered === false
							? []
							: [
									{
										slug: "a",
										name: "Synthetic A",
										createdAt: "2026-01-01",
										archived: request.archived === true,
									},
								],
				}),
			);
			const source = new Database(f.c);
			const legacyAllowed =
				request.authenticated !== false &&
				hasCurrentBetterAuthSession(
					source,
					request.subject ?? "u",
					request.session ?? "s",
				) &&
				authorizeWorkspaceRoute(source, f.dir, {
					userId: request.subject ?? "u",
					companySlug: request.slug ?? "a",
					permission: request.permission ?? "company.ledger.post",
				}).allowed;
			const legacyAllows = new Set([
				"commit",
				"business-error",
				"sql-error",
				"manual-commit",
				"cross-workspace",
				"empty-workspace",
				"untrimmed-subject",
				"untrimmed-session",
				"public-not-company-authority",
				"workspace-not-company-authority",
				"control-temp",
				"company-temp",
				"attached",
				"fk-disabled",
				"weak-durability",
				"query-only",
				"pinned-control",
				"company-first",
				"noncanonical-control",
				"control-readonly",
				"company-readonly",
				"foreign-key-corruption",
				"control-journal-off",
				"company-journal-off",
				"control-journal-memory",
				"company-journal-memory",
			]);
			assert.equal(
				legacyAllowed,
				legacyAllows.has(v.id),
				`${v.id} unchanged legacy core/session observation`,
			);
			source.close();
			const child = start(f, v.request);
			child.stdin.end();
			await completed(child);
			const result = JSON.parse(readFileSync(f.output, "utf8"));
			for (const [key, value] of Object.entries(v.expected))
				assert.equal(result[key], value, `${mode}/${v.id}/${key}`);
			const actual = state(f);
			assert.equal(actual.rows.length, v.effects ?? 0);
			assert.equal(actual.seq.length, v.effects ?? 0);
			if (v.effects) {
				assert.deepEqual(actual, { rows: [{ n: 1 }], seq: [{ value: 1 }] });
			}
			controlPreserved(f);
			observations.push({ mode, id: v.id, result, legacyAllowed });
			checks++;
		}
		for (const which of ["control", "company"]) {
			const f = fixture(`${mode}-busy-${which}`, mode),
				held = new Database(which === "control" ? f.c : f.l);
			held.exec("BEGIN IMMEDIATE");
			const child = start(f);
			child.stdin.end();
			await completed(child);
			const result = JSON.parse(readFileSync(f.output, "utf8"));
			assert.equal(result.outcome, "not_committed");
			assert.equal(result.code, `${which}_busy_or_unavailable`);
			held.exec("ROLLBACK");
			held.close();
			assert.equal(state(f).rows.length, 0);
			controlPreserved(f);
			observations.push({ mode, id: `busy-${which}`, result });
			checks++;
		}

		for (const action of ["revoke", "disable", "session-delete"]) {
			const f = fixture(`${mode}-race-${action}`, mode),
				child = start(f, {}, "precommit");
			await marker(f, child);
			const revoke = new Database(f.c);
			revoke.exec("PRAGMA busy_timeout=0;PRAGMA foreign_keys=ON");
			const sql =
				action === "revoke"
					? "INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','revoke',NULL,'agent:synthetic')"
					: action === "disable"
						? "INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','disable',NULL,'agent:synthetic')"
						: "DELETE FROM session WHERE id='s'";
			assert.throws(
				() => revoke.transaction(() => revoke.exec(sql)).immediate(),
				/locked|busy/i,
			);
			assert.equal(state(f).rows.length, 0);
			child.stdin.write("continue\n");
			child.stdin.end();
			await completed(child);
			assert.equal(
				JSON.parse(readFileSync(f.output, "utf8")).outcome,
				"committed",
			);
			assert.equal(state(f).rows.length, 1);
			revoke.transaction(() => revoke.exec(sql)).immediate();
			revoke.close();
			const retry = start(f);
			retry.stdin.end();
			await completed(retry);
			assert.equal(
				JSON.parse(readFileSync(f.output, "utf8")).code,
				"access_denied",
			);
			assert.equal(state(f).rows.length, 1);
			observations.push({
				mode,
				id: `race-${action}`,
				blockedBeforeCommit: true,
				revokedRetryDenied: true,
			});
			checks++;
		}
		// A real rollback-journal reader forces COMMIT busy; WAL permits commit and
		// the old reader retains its original snapshot. Neither case is simulated.
		{
			const f = fixture(`${mode}-commit-reader`, mode),
				reader = new Database(f.l);
			reader.exec("BEGIN DEFERRED");
			assert.equal(
				(reader.query("SELECT count(*) AS n FROM companion").get() as any).n,
				0,
			);
			const child = start(f, {}, "precommit");
			await marker(f, child);
			child.stdin.write("continue\n");
			child.stdin.end();
			await completed(child);
			const result = JSON.parse(readFileSync(f.output, "utf8"));
			assert.equal(result.outcome, mode === "DELETE" ? "unknown" : "committed");
			if (mode === "DELETE") {
				assert.equal(result.code, "commit_outcome_unknown");
				assert.equal(result.companyReusable, false);
			}
			assert.equal(
				(reader.query("SELECT count(*) AS n FROM companion").get() as any).n,
				0,
			);
			reader.exec("ROLLBACK");
			reader.close();
			assert.equal(state(f).rows.length, mode === "DELETE" ? 0 : 1);
			controlPreserved(f);
			const revoked = new Database(f.c);
			revoked.exec("PRAGMA busy_timeout=0");
			revoked
				.transaction(() => revoked.exec("DELETE FROM session WHERE id='s'"))
				.immediate();
			revoked.close();
			observations.push({
				mode,
				id: "commit-reader",
				result,
				oldReaderSnapshotPreserved: true,
				controlReleased: true,
			});
			checks++;
		}

		for (const point of ["precommit", "postcommit"]) {
			const f = fixture(`${mode}-crash-${point}`, mode),
				child = start(f, {}, point);
			await marker(f, child);
			child.kill("SIGKILL");
			await child.exited;
			assert.equal(child.signalCode, "SIGKILL");
			assert.equal(state(f).rows.length, point === "precommit" ? 0 : 1);
			controlPreserved(f);
			if (point === "precommit") {
				const resumed = start(f);
				resumed.stdin.end();
				await completed(resumed);
				assert.equal(JSON.parse(readFileSync(f.output, "utf8")).value, 1);
				assert.deepEqual(state(f), { rows: [{ n: 1 }], seq: [{ value: 1 }] });
			}
			const revoke = new Database(f.c);
			revoke.exec("PRAGMA busy_timeout=0");
			revoke
				.transaction(() => revoke.exec("DELETE FROM session WHERE id='s'"))
				.immediate();
			revoke.close();
			const retry = start(f);
			retry.stdin.end();
			await completed(retry);
			assert.equal(
				JSON.parse(readFileSync(f.output, "utf8")).code,
				"access_denied",
			);
			assert.equal(state(f).rows.length, 1);
			observations.push({
				mode,
				id: `crash-${point}`,
				preservedCommit: point === "postcommit",
				lockReleased: true,
				retryFromPriorSequence: point === "precommit",
			});
			checks++;
		}
	}
	console.log(
		JSON.stringify(
			{
				ok: true,
				checks,
				casesPerJournalMode: cases.length,
				journalModes: ["WAL", "DELETE"],
				observations,
				limits: [
					"Admitted borrowed connections and provider/manifest identity remain prerequisites",
					"Effect ledgers prove transaction mechanics, not full posting or durable idempotency",
					"Control row snapshots preserved; no physical source/WAL immutability claim for a real writer-lock primitive",
				],
			},
			null,
			2,
		),
	);
} finally {
	rmSync(work, { recursive: true, force: true });
}
