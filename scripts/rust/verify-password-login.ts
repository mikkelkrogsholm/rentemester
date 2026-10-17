/** Actual framework/control fixtures and independent native credential transaction races. */
import assert from "node:assert/strict";
import {
	copyFileSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	existsSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { homedir, tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { Database } from "bun:sqlite";
import { z } from "zod";
import {
	openWorkspaceControlDb,
	workspaceControlPaths,
} from "../../src/core/workspace-control";
import {
	hashPassword,
	verifyPassword,
} from "../../node_modules/better-auth/dist/crypto/password.mjs";
import { createLocalAccountIssuer } from "@better-auth/core/db";
import { createBetterAuthRuntime } from "../../src/server/better-auth";
const root = resolve(import.meta.dir, "../.."),
	cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const build = Bun.spawnSync(
	[
		cargo,
		"test",
		"--locked",
		"--release",
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
const work = mkdtempSync(join(tmpdir(), "rm-password-login-"));
const password = "Synthetic password 123!",
	hash = await hashPassword(password),
	issuer = createLocalAccountIssuer("credential");
assert.equal(issuer, "local:credential");
const seed = join(work, "seed");
mkdirSync(seed);
const db = openWorkspaceControlDb(seed);
db.exec(`PRAGMA wal_checkpoint(TRUNCATE);PRAGMA journal_mode=DELETE;
INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);
`);
db.query(
	"INSERT INTO account(id,userId,accountId,providerId,issuer,password,createdAt,updatedAt) VALUES('a','u','u','credential',?,?, '2026-01-01','2026-01-01')",
).run(issuer, hash);
db.close();
let checks = 0;
const observations: any[] = [];
function record(id: string, result: any) {
	checks++;
	observations.push({ id, ...result });
}
function fixture(id: string, mode = "delete", sql = "") {
	const dir = join(work, id);
	mkdirSync(dir);
	mkdirSync(join(dir, ".rentemester"));
	const control = workspaceControlPaths(dir).db;
	copyFileSync(workspaceControlPaths(seed).db, control);
	const db = new Database(control);
	db.exec(`PRAGMA foreign_keys=ON;PRAGMA journal_mode=${mode};`);
	if (id !== "absent-access")
		db.exec(
			"INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic')",
		);
	if (sql) db.exec(sql);
	db.close();
	return { dir, control, marker: join(dir, "marker") };
}
function image(f: any) {
	const db = new Database(f.control);
	const value = createHash("sha256").update(db.serialize()).digest("hex");
	db.close();
	return value;
}
function count(f: any) {
	const db = new Database(f.control);
	const n = (
		db
			.query(
				"SELECT count(*) n FROM workspace_audit WHERE event_type='synthetic-password-continuation'",
			)
			.get() as any
	).n;
	db.close();
	return n;
}
function start(f: any, request: any = {}, point = "") {
	const child = Bun.spawn(
		[
			helper,
			"--ignored",
			"--exact",
			"password_login::tests::probe",
			"--nocapture",
		],
		{
			cwd: work,
			env: {
				...process.env,
				RM_PASSWORD_LOGIN_POINT: point,
				RM_PASSWORD_LOGIN_MARKER: f.marker,
			},
			stdin: "pipe",
			stdout: "pipe",
			stderr: "pipe",
		},
	);
	child.stdin.write(
		JSON.stringify({
			control: f.control,
			email: "U@Example.Invalid",
			password,
			...request,
		}) + "\n",
	);
	const timer = setTimeout(() => child.kill(), 30000);
	return { child, timer };
}
async function done(p: ReturnType<typeof start>) {
	try {
		const [exit, out, err] = await Promise.all([
			p.child.exited,
			new Response(p.child.stdout).text(),
			new Response(p.child.stderr).text(),
		]);
		assert.equal(exit, 0, out + err);
		for (const secret of [
			password,
			hash,
			"secret callback diagnostic",
			"private-canary",
		]) {
			assert(!out.includes(secret));
			assert(!err.includes(secret));
		}
		const match = out.match(/RM_LOGIN_RESULT=(.*)/);
		assert(match, out);
		return JSON.parse(match[1]);
	} finally {
		clearTimeout(p.timer);
		p.child.kill();
	}
}
async function marker(p: ReturnType<typeof start>, f: any) {
	const deadline = Date.now() + 10000;
	while (!existsSync(f.marker)) {
		assert(Date.now() < deadline, "native checkpoint timeout");
		if (p.child.exitCode !== null)
			throw new Error("native helper ended before checkpoint");
		await Bun.sleep(10);
	}
}
const disable =
	"INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')";
try {
	// Actual configured framework adapter determines issuer, account ownership and email lookup.
	const oracle = fixture("oracle");
	const odb = new Database(oracle.control);
	const runtime = createBetterAuthRuntime(odb, {
		secret: "synthetic-password-oracle-secret-32-bytes-long",
		baseURL: "https://example.invalid",
		trustedOrigins: ["https://example.invalid"],
	});
	const context = await runtime.$context;
	const user = await context.internalAdapter.findUserByEmail(
		"U@Example.Invalid",
		{ includeAccounts: true },
	);
	const account = user?.accounts.find(
		(a: any) =>
			a.providerId === "credential" &&
			a.issuer === issuer &&
			a.accountId === user.user.id,
	);
	assert(account);
	assert(await verifyPassword({ password, hash: account.password! }));
	record("installed-framework-account-selection", { ok: true });
	odb.close();
	const emails = [
		"A_B+tag@Example.INVALID",
		"a'b@a-.invalid",
		"a@b.co",
		"a@a.b.example",
		"",
		" a@b.co",
		"a@b.co\n",
		"a..b@b.co",
		".a@b.co",
		"a.@b.co",
		"a'@b.co",
		"a@b.c",
		"a@-b.co",
		"a@b..co",
		"æ@b.co",
		"a@@b.co",
		"a@b.123",
	];
	// More independently classified ASCII local/domain combinations; fixed seed is reproducible.
	let state = 17;
	const chars = "abAB09_+'-.@ !æ";
	for (let n = 0; n < 240; n++) {
		let local = "";
		for (let j = 0; j < 1 + (n % 9); j++) {
			state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
			local += chars[state % chars.length];
		}
		emails.push(`${local}@a${n % 3 === 0 ? "-" : ""}.example`);
	}
	const ep = start(oracle, { mode: "email", emails });
	ep.child.stdin.end();
	const eo = await done(ep);
	for (let i = 0; i < emails.length; i++) {
		const e = emails[i];
		const expected =
			z.email().safeParse(e).success && !/[\x00-\x1f\x7f]/.test(e)
				? e.toLowerCase()
				: null;
		assert.equal(eo.emails[i], expected);
		record(`email-${i}`, { ok: true });
	}
	const cases: [string, string, string, any, string?][] = [
		["challenge", "delete", "", {}, "challenge"],
		["wal-challenge", "wal", "", {}, "challenge"],
		[
			"enrollment",
			"delete",
			"UPDATE user SET twoFactorEnabled=0",
			{},
			"enrollment",
		],
		[
			"nullable-enrollment",
			"delete",
			"UPDATE user SET twoFactorEnabled=NULL",
			{},
			"enrollment",
		],
		["wrong-password", "delete", "", { password: "wrong" }],
		["missing-user", "delete", "", { email: "missing@example.invalid" }],
		["missing-account", "delete", "DELETE FROM account", {}],
		["null-password", "delete", "UPDATE account SET password=NULL", {}],
		[
			"malformed-hash",
			"delete",
			"UPDATE account SET password='not-a-hash'",
			{},
		],
		[
			"provider-mismatch",
			"delete",
			"UPDATE account SET providerId='oauth'",
			{},
		],
		[
			"issuer-mismatch",
			"delete",
			"UPDATE account SET issuer='local:oauth:credential'",
			{},
		],
		[
			"account-id-mismatch",
			"delete",
			"UPDATE account SET accountId='different'",
			{},
		],
		["email-not-verified", "delete", "UPDATE user SET emailVerified=0", {}],
		["invalid-email-flag", "delete", "UPDATE user SET emailVerified=2", {}],
		["invalid-mfa-flag", "delete", "UPDATE user SET twoFactorEnabled=2", {}],
		["disabled", "delete", disable, {}],
		["absent-access", "delete", "", {}],
		["callback-failure", "delete", "", { fail: true }],
		["newer-schema", "delete", "CREATE TABLE unexpected_extra(n INTEGER)", {}],
		["weak-connection", "delete", "", { preflight: "PRAGMA synchronous=OFF" }],
		[
			"attached-connection",
			"delete",
			"",
			{ preflight: "ATTACH DATABASE ':memory:' AS auxiliary" },
		],
		[
			"temp-schema",
			"delete",
			"",
			{ preflight: "CREATE TEMP TABLE forged(n INTEGER)" },
		],
		["active-transaction", "delete", "", { preflight: "BEGIN" }],
		["oversize-password", "delete", "", { password: "a".repeat(4097) }],
		["control-email", "delete", "", { email: "u@example.invalid\n" }],
		[
			"invalid-input-unready",
			"delete",
			"",
			{ email: "invalid", preflight: "BEGIN" },
		],
		[
			"oversize-input-unready",
			"delete",
			"",
			{ password: "a".repeat(4097), preflight: "PRAGMA synchronous=OFF" },
		],
	];
	for (const [id, mode, sql, request, next] of cases) {
		const f = fixture(id, mode, sql),
			before = image(f),
			p = start(f, request);
		p.child.stdin.end();
		const result = await done(p);
		assert.equal(result.ok, Boolean(next), id);
		if (next) {
			assert.equal(result.next, next);
			assert.equal(count(f), 1);
		} else {
			assert.equal(count(f), 0);
			assert.equal(image(f), before);
			assert.equal(result.stage, "NotCommitted");
		}
		record(id, result);
	}
	for (const mode of ["wal", "delete"]) {
		for (const [id, sql] of [
			[
				"password-reset",
				"UPDATE account SET password='different-hash',updatedAt='2026-01-02'",
			],
			["credential-replacement", "UPDATE account SET id='replacement'"],
			["credential-retarget", "UPDATE account SET accountId='different'"],
			["email-change", "UPDATE user SET email='changed@example.invalid'"],
			["email-verification-change", "UPDATE user SET emailVerified=0"],
			["mfa-change", "UPDATE user SET twoFactorEnabled=0"],
			["revoke", disable],
			[
				"revoke-reactivate",
				`${disable};INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic')`,
			],
			["user-update", "UPDATE user SET updatedAt='2026-01-02'"],
			["account-update", "UPDATE account SET updatedAt='2026-01-02'"],
			[
				"ordered-workspace",
				"INSERT INTO rm_workspace_user_access_events(id,user_id,event_type,workspace_role,actor) VALUES(9223372036854775807,'u','activate','member','agent:synthetic')",
			],
			[
				"ordered-membership",
				"INSERT INTO rm_company_membership_events(id,user_id,company_slug,event_type,company_role,actor) VALUES(9223372036854775807,'u','a','grant','reader','agent:synthetic')",
			],
			[
				"ordered-security",
				"INSERT INTO rm_workspace_security_events(id,scope,event_type,actor) VALUES(9223372036854775807,'workspace','session_invalidate','agent:synthetic')",
			],
		] as const) {
			const f = fixture(`${mode}-${id}`, mode),
				p = start(f, {}, "unlocked");
			try {
				await marker(p, f);
				const db = new Database(f.control);
				db.exec("PRAGMA busy_timeout=0;");
				db.exec(sql);
				db.close();
				const before = image(f);
				p.child.stdin.write("continue\n");
				p.child.stdin.end();
				const result = await done(p);
				assert.equal(result.ok, false, id);
				assert.equal(
					result.code,
					id.startsWith("ordered-") ? "invalid_control" : "invalid_credentials",
					id,
				);
				assert.equal(count(f), 0);
				assert.equal(image(f), before);
				record(`${mode}-${id}`, {
					...result,
					competingChangeCommittedWhileUnlocked: true,
				});
			} finally {
				clearTimeout(p.timer);
				p.child.kill();
				await p.child.exited;
			}
		}
		const f = fixture(`${mode}-locked-revoke`, mode),
			p = start(f, {}, "locked");
		try {
			await marker(p, f);
			const db = new Database(f.control);
			db.exec("PRAGMA busy_timeout=0;");
			assert.throws(
				() => db.exec(disable),
				(e: any) => e.code === "SQLITE_BUSY",
			);
			p.child.stdin.write("continue\n");
			p.child.stdin.end();
			const result = await done(p);
			assert.equal(result.ok, true);
			assert.equal(count(f), 1);
			db.exec(disable);
			db.close();
			record(`${mode}-locked-revoke`, { ok: true, blockedUntilCommit: true });
		} finally {
			clearTimeout(p.timer);
			p.child.kill();
			await p.child.exited;
		}
	}
	// An external reader permits the reserved writer but prevents DELETE-mode commit.
	const uncertain = fixture("commit-busy"),
		up = start(uncertain, {}, "locked");
	const reader = new Database(uncertain.control);
	try {
		await marker(up, uncertain);
		reader.exec("BEGIN;SELECT count(*) FROM user;");
		up.child.stdin.write("continue\n");
		up.child.stdin.end();
		const result = await done(up);
		assert.equal(result.ok, false);
		assert.equal(result.stage, "OutcomeUnknown");
		assert.equal(result.code, "commit_outcome_unknown");
		assert.equal(result.reusable, false);
		reader.exec("ROLLBACK");
		assert.equal(count(uncertain), 0);
		record("commit-busy", {
			...result,
			syntheticEffectAbsentAfterFailure: true,
		});
	} finally {
		reader.close();
		clearTimeout(up.timer);
		up.child.kill();
		await up.child.exited;
	}
	const busy = fixture("writer-busy"),
		bp = start(busy, {}, "unlocked"),
		writer = new Database(busy.control);
	try {
		await marker(bp, busy);
		writer.exec("BEGIN IMMEDIATE");
		bp.child.stdin.write("continue\n");
		bp.child.stdin.end();
		const result = await done(bp);
		assert.equal(result.ok, false);
		assert.equal(result.stage, "NotCommitted");
		assert.equal(result.code, "control_busy_or_unavailable");
		writer.exec("ROLLBACK");
		assert.equal(count(busy), 0);
		record("writer-busy", result);
	} finally {
		writer.close();
		clearTimeout(bp.timer);
		bp.child.kill();
		await bp.child.exited;
	}
	console.log(
		JSON.stringify(
			{
				ok: true,
				scope:
					"native existing password-to-pending-MFA transaction; canonical synthetic source fixtures and actual framework oracle",
				checks,
				observations,
				limits: [
					"Internal admitted caller-owned handle only; physical admission and endpoint lifecycle remain pending",
					"No actual challenge/session/MFA completion or company authority",
					"Commit busy is exercised; commit IO failure injection and uncooperative namespace swaps are not qualified",
				],
			},
			null,
			2,
		),
	);
} finally {
	rmSync(work, { recursive: true, force: true });
}
