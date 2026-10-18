/** Native enrolled MFA flow, actual source crypto/session oracle, independent process races. */
import assert from "node:assert/strict";
import { verifyEmailVerification } from "./email-verification-cases";
import { verifyPasswordReset } from "./password-reset-cases";
import { verifyResetRequest } from "./reset-request-cases";
import { verifySessionControl } from "./session-control-cases";
import { verifyManagement } from "./backup-management-cases";
import { verifyEnrollment } from "./mfa-enrollment-cases";
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
import { createHash, createHmac } from "node:crypto";
import { Database } from "bun:sqlite";
import {
	openWorkspaceControlDb,
	workspaceControlPaths,
} from "../../src/core/workspace-control";
import { createBetterAuthRuntime } from "../../src/server/better-auth";
import { hashPassword } from "../../node_modules/better-auth/dist/crypto/password.mjs";
import { generateBackupCodes } from "../../node_modules/better-auth/dist/plugins/two-factor/backup-codes/index.mjs";
import {
	symmetricDecrypt,
	symmetricEncrypt,
} from "../../node_modules/better-auth/dist/crypto/index.mjs";
import { serializeSignedCookie } from "../../node_modules/better-call/dist/cookies.mjs";
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
const work = mkdtempSync(join(tmpdir(), "rm-mfa-login-")),
	seed = join(work, "seed");
mkdirSync(seed);
const key = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
	secret = "12345678901234567890",
	password = "Synthetic password 123!",
	now = 1767225600000;
const encrypted = await symmetricEncrypt({
		key: { currentVersion: 1, keys: new Map([[1, key]]) },
		data: secret,
	}),
	passwordHash = await hashPassword(password);
const db = openWorkspaceControlDb(seed);
db.exec(`PRAGMA wal_checkpoint(TRUNCATE);PRAGMA journal_mode=DELETE;
INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);
INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic');`);
db.query(
	"INSERT INTO account(id,userId,accountId,providerId,issuer,password,createdAt,updatedAt) VALUES('a','u','u','credential','local:credential',?,'2026-01-01','2026-01-01')",
).run(passwordHash);
db.query(
	"INSERT INTO twoFactor(id,userId,secret,backupCodes,verified,failedVerificationCount) VALUES('f','u',?,'unused-synthetic-backup-placeholder',1,0)",
).run(encrypted);
db.close();
let checks = 0;
const observations: any[] = [];
function record(id: string, extra: any = {}) {
	checks++;
	observations.push({ id, ...extra });
}
function fixture(id: string, mode = "delete", sql = "") {
	const dir = join(work, id);
	mkdirSync(dir);
	const control = join(dir, "control.sqlite");
	copyFileSync(workspaceControlPaths(seed).db, control);
	const db = new Database(control);
	db.exec(`PRAGMA foreign_keys=ON;PRAGMA journal_mode=${mode};`);
	if (sql) db.exec(sql);
	db.close();
	return { dir, control, marker: join(dir, "marker") };
}
function run(f: any, request: any = {}) {
	const p = Bun.spawnSync(
		[helper, "--ignored", "--exact", "mfa_login::tests::probe", "--nocapture"],
		{
			cwd: work,
			env: { ...process.env, RM_MFA_FAIL_SQL: request.failSql ? "1" : "0" },
			stdin: Buffer.from(
				JSON.stringify({ control: f.control, key, now, ...request }) + "\n",
			),
			stdout: "pipe",
			stderr: "pipe",
		},
	);
	return parse(p.exitCode, p.stdout.toString(), p.stderr.toString());
}
function parse(exit: any, out: string, err: string) {
	assert.equal(exit, 0, "native MFA login helper failed");
	const match = out.match(/RM_MFA_LOGIN_RESULT=(.*)/);
	assert(match, "missing MFA result");
	const diagnostics = out.replace(match[0], "") + err;
	for (const value of [key, secret, password, passwordHash, encrypted])
		assert(!diagnostics.includes(value), "sensitive native diagnostic");
	return JSON.parse(match[1]);
}
function start(f: any, request: any = {}, point = "") {
	const child = Bun.spawn(
		[helper, "--ignored", "--exact", "mfa_login::tests::probe", "--nocapture"],
		{
			cwd: work,
			env: {
				...process.env,
				RM_MFA_POINT: point.startsWith("password:") ? "" : point,
				RM_MFA_MARKER: f.marker,
				RM_PASSWORD_LOGIN_POINT: point.startsWith("password:")
					? point.slice(9)
					: "",
				RM_PASSWORD_LOGIN_MARKER: f.marker,
			},
			stdin: "pipe",
			stdout: "pipe",
			stderr: "pipe",
		},
	);
	child.stdin.write(
		JSON.stringify({ control: f.control, key, now, ...request }) + "\n",
	);
	return { child, timer: setTimeout(() => child.kill(), 30000) };
}
async function done(p: ReturnType<typeof start>) {
	try {
		const [exit, out, err] = await Promise.all([
			p.child.exited,
			new Response(p.child.stdout).text(),
			new Response(p.child.stderr).text(),
		]);
		return parse(exit, out, err);
	} finally {
		clearTimeout(p.timer);
		p.child.kill();
	}
}
async function marker(p: ReturnType<typeof start>, f: any) {
	const deadline = Date.now() + 10000;
	while (!existsSync(f.marker)) {
		assert(Date.now() < deadline, "native checkpoint timeout");
		assert(p.child.exitCode === null, "native helper ended before checkpoint");
		await Bun.sleep(10);
	}
}
function sql(f: any, text: string) {
	const db = new Database(f.control);
	db.exec("PRAGMA foreign_keys=ON;PRAGMA busy_timeout=0;");
	db.exec(text);
	db.close();
}
function state(f: any) {
	const db = new Database(f.control);
	const one = (q: string) => db.query(q).get() as any;
	const result = {
		sessions: one("SELECT count(*) n FROM session").n,
		pending: one(
			"SELECT count(*) n FROM verification WHERE identifier LIKE '2fa-%' OR identifier LIKE 'rm-native-mfa-v1:%'",
		).n,
		replays: one(
			"SELECT count(*) n FROM verification WHERE identifier LIKE 'rm-native-totp-step-v1:%'",
		).n,
		factor: one(
			"SELECT failedVerificationCount count,lockedUntil locked FROM twoFactor WHERE id='f'",
		),
		audits: one(
			"SELECT count(*) n FROM rm_workspace_auth_telemetry_events WHERE endpoint='two-factor-verify'",
		).n,
		created: one(
			"SELECT count(*) n FROM rm_workspace_auth_state_events WHERE state_transition='session_created'",
		).n,
	};
	db.close();
	return result;
}
function image(f: any) {
	const db = new Database(f.control);
	const result = createHash("sha256").update(db.serialize()).digest("hex");
	db.close();
	return result;
}
function code(time = now) {
	const counter = Buffer.alloc(8);
	counter.writeBigUInt64BE(BigInt(Math.floor(time / 30000)));
	const h = createHmac("sha1", secret).update(counter).digest(),
		o = h[19] & 15;
	return String((h.readUInt32BE(o) & 0x7fffffff) % 1000000).padStart(6, "0");
}
function begin(f: any, time = now) {
	const b = run(f, { mode: "begin", now: time });
	assert.equal(b.ok, true);
	assert.equal(b.pending, true);
	assert(b.cookie.startsWith("__Secure-better-auth.two_factor="));
	assert(b.setCookie.includes("HttpOnly; SameSite=Lax; Max-Age=600; Secure"));
	return b;
}
function unchangedRefusal(
	id: string,
	f: any,
	request: any,
	expected = "invalid_credentials",
) {
	const before = image(f),
		r = run(f, request);
	assert.equal(r.ok, false, id);
	assert.equal(r.code, expected, id);
	assert.equal(r.stage, "NotCommitted", id);
	assert.equal(image(f), before, id);
	record(id, { code: r.code, stage: r.stage });
}
const disable =
	"INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')";
try {
	for (const mode of ["wal", "delete"]) {
		const f = fixture(`${mode}-happy`, mode),
			b = begin(f);
		assert.equal(state(f).sessions, 0);
		assert.equal(state(f).pending, 3);
		assert.equal(state(f).created, 0);
		record(`${mode}-pending-no-session`);
		const bad = run(f, { cookie: b.cookie, code: "wrong" });
		assert.equal(bad.rejected, "InvalidCode");
		assert.equal(state(f).factor.count, 1);
		assert.equal(state(f).audits, 1);
		assert.equal(state(f).sessions, 0);
		record(`${mode}-committed-rejection`);
		const ok = run(f, { cookie: b.cookie, code: code() });
		assert.equal(ok.session, true);
		assert.equal(ok.nativeCurrentFresh, true);
		assert(ok.setCookie.includes("Max-Age=43200; Secure"));
		const s = state(f);
		assert.equal(s.sessions, 1);
		assert.equal(s.created, 1);
		assert.equal(s.pending, 0);
		assert.equal(s.replays, 1);
		assert.equal(s.factor.count, 0);
		assert.equal(s.audits, 2);
		record(`${mode}-session-after-MFA`);
		unchangedRefusal(`${mode}-challenge-one-time`, f, {
			cookie: b.cookie,
			code: code(),
		});
		const next = begin(f),
			replay = run(f, { cookie: next.cookie, code: code() });
		assert.equal(replay.rejected, "ReplayedCode");
		assert.equal(state(f).sessions, 1);
		assert.equal(state(f).factor.count, 1);
		record(`${mode}-TOTP-replay-persisted`);
		assert.equal(
			run(f, { cookie: next.cookie, code: code(now + 30000), now: now + 30000 })
				.session,
			true,
		);
		assert.equal(state(f).sessions, 2);
		record(`${mode}-next-step-can-authenticate`);
		const fail = fixture(`${mode}-failures`, mode);
		let current = begin(fail);
		for (let i = 1; i <= 5; i++) {
			assert.equal(
				run(fail, { cookie: current.cookie, code: "000000" }).rejected,
				"InvalidCode",
			);
			assert.equal(state(fail).factor.count, i);
			assert.equal(state(fail).sessions, 0);
		}
		assert.equal(state(fail).pending, 0);
		record(`${mode}-fifth-failure-burn`);
		current = begin(fail);
		const standby = begin(fail);
		for (let i = 6; i <= 10; i++) {
			assert.equal(
				run(fail, { cookie: current.cookie, code: "000000" }).rejected,
				"InvalidCode",
			);
			assert.equal(state(fail).factor.count, i);
		}
		assert.equal(
			state(fail).factor.locked,
			new Date(now + 900000).toISOString(),
		);
		record(`${mode}-account-budget-across-challenges`);
		assert.equal(
			run(fail, { cookie: standby.cookie, code: code() }).rejected,
			"AccountLocked",
		);
		assert.equal(state(fail).factor.count, 10);
		record(`${mode}-locked-valid-code-refuses`);
		unchangedRefusal(`${mode}-locked-cannot-reissue`, fail, { mode: "begin" });
		unchangedRefusal(`${mode}-unlock-exclusive-before`, fail, {
			mode: "begin",
			now: now + 899999,
		});
		const unlocked = begin(fail, now + 900000);
		assert.equal(
			run(fail, {
				cookie: unlocked.cookie,
				code: code(now + 900000),
				now: now + 900000,
			}).session,
			true,
		);
		assert.equal(state(fail).factor.count, 0);
		assert.equal(state(fail).factor.locked, null);
		record(`${mode}-exact-unlock-success`);
		const late = fixture(`${mode}-lateSQL`, mode),
			lb = begin(late);
		const before = image(late);
		const failure = run(late, {
			cookie: lb.cookie,
			code: code(),
			failSql: true,
		});
		assert.equal(failure.ok, false);
		assert.equal(failure.code, "invalid_control");
		assert.equal(failure.reusable, true);
		assert.equal(image(late), before);
		assert.equal(state(late).created, 0);
		record(`${mode}-late-SQL-rolls-back-all`);
		const races = fixture(`${mode}-single-consumer`, mode),
			rb = begin(races),
			p = start(races, { cookie: rb.cookie, code: code() }, "locked");
		try {
			await marker(p, races);
			const second = run(races, { cookie: rb.cookie, code: code() });
			assert.equal(second.ok, false);
			assert.equal(second.code, "control_busy_or_unavailable");
			p.child.stdin.write("continue\n");
			p.child.stdin.end();
			assert.equal((await done(p)).session, true);
			assert.equal(state(races).sessions, 1);
			unchangedRefusal(`${mode}-concurrent-second-consumer`, races, {
				cookie: rb.cookie,
				code: code(),
			});
			record(`${mode}-serialized-one-consumer`);
		} finally {
			clearTimeout(p.timer);
			p.child.kill();
			await p.child.exited;
		}
		const parallel = fixture(`${mode}-parallel-failures`, mode),
			pb = begin(parallel);
		const children = Array.from({ length: 4 }, () =>
			start(parallel, { cookie: pb.cookie, code: "wrong" }),
		);
		for (const c of children) c.child.stdin.end();
		const results = await Promise.all(children.map(done));
		const rejected = results.filter((r) => r.rejected === "InvalidCode").length;
		assert(rejected >= 1);
		assert.equal(state(parallel).factor.count, rejected);
		assert.equal(state(parallel).audits, rejected);
		assert.equal(state(parallel).sessions, 0);
		record(`${mode}-parallel-failures-no-lost-update`, {
			committedFailures: rejected,
		});
	}
	for (const [id, change, err] of [
		["email-unverified", "UPDATE user SET emailVerified=0"],
		[
			"ordered-workspace",
			"INSERT INTO rm_workspace_user_access_events(id,user_id,event_type,workspace_role,actor) VALUES(9223372036854775807,'u','activate','member','agent:synthetic')",
			"invalid_control",
		],
		[
			"ordered-membership",
			"INSERT INTO rm_company_membership_events(id,user_id,company_slug,event_type,company_role,actor) VALUES(9223372036854775807,'u','a','grant','reader','agent:synthetic')",
			"invalid_control",
		],
		[
			"ordered-security",
			"INSERT INTO rm_workspace_security_events(id,scope,event_type,actor) VALUES(9223372036854775807,'workspace','session_invalidate','agent:synthetic')",
			"invalid_control",
		],
		["MFA-disabled", "UPDATE user SET twoFactorEnabled=0"],
		["email-changed", "UPDATE user SET email='different@example.invalid'"],
		["password-reset", "UPDATE account SET password='different'"],
		["account-replaced", "UPDATE account SET id='different'"],
		["access-revoked", disable],
		[
			"access-reactivated",
			`${disable};INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic')`,
		],
		["factor-unverified", "UPDATE twoFactor SET verified=0"],
		["legacy-null-factor", "UPDATE twoFactor SET verified=NULL"],
		["factor-replaced", "UPDATE twoFactor SET id='new-factor'"],
		["factor-secret-changed", "UPDATE twoFactor SET secret='different'"],
		[
			"account-negative-counter",
			"UPDATE twoFactor SET failedVerificationCount=-1",
			"invalid_control",
		],
		[
			"account-malformed-counter",
			"UPDATE twoFactor SET failedVerificationCount=1.5",
			"invalid_control",
		],
		[
			"account-over-counter",
			"UPDATE twoFactor SET failedVerificationCount=11",
			"invalid_control",
		],
		[
			"account-malformed-lock",
			"UPDATE twoFactor SET lockedUntil='bad'",
			"invalid_control",
		],
		[
			"session-deletion-revokes-pending",
			"INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt) VALUES('other-session','u','other-token','2026-01-01','2026-01-01','2026-01-02');DELETE FROM session WHERE id='other-session'",
		],
		[
			"challenge-duplicate",
			"INSERT INTO verification SELECT 'duplicate',identifier,value,expiresAt,createdAt,updatedAt FROM verification WHERE identifier LIKE '2fa-%' AND identifier NOT LIKE '2fa-attempts-%'",
			"invalid_control",
		],
		[
			"factor-duplicate",
			`INSERT INTO twoFactor(id,userId,secret,backupCodes,verified,failedVerificationCount) SELECT 'duplicate-factor',userId,secret,backupCodes,1,0 FROM twoFactor WHERE id='f'`,
			"invalid_control",
		],
		[
			"context-missing",
			"DELETE FROM verification WHERE identifier LIKE 'rm-native-mfa-v1:%'",
		],
		[
			"attempt-noncanonical",
			"UPDATE verification SET value='00' WHERE identifier LIKE '2fa-attempts-%'",
		],
		[
			"noncanonical-expiry",
			"UPDATE verification SET expiresAt='2026-01-01' WHERE identifier LIKE '2fa-%'",
			"invalid_control",
		],
	] as const) {
		const f = fixture(id),
			b = begin(f);
		sql(f, change);
		unchangedRefusal(
			id,
			f,
			{ cookie: b.cookie, code: code() },
			err ?? "invalid_credentials",
		);
	}
	const rowidOverflow = fixture("lifecycle-rowid-overflow");
	sql(
		rowidOverflow,
		"INSERT INTO rm_workspace_auth_state_events(id,state_transition,user_id) VALUES(9223372036854775807,'credential_updated','u')",
	);
	const overflowPending = begin(rowidOverflow);
	sql(
		rowidOverflow,
		"INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt) VALUES('other-session','u','other-token','2026-01-01','2026-01-01','2026-01-02');DELETE FROM session WHERE id='other-session'",
	);
	unchangedRefusal("lifecycle-rowid-overflow", rowidOverflow, {
		cookie: overflowPending.cookie,
		code: code(),
	});
	for (const [id, request] of [
		["exact-expiry", { now: now + 600000 }],
		["before-creation", { now: now - 1 }],
		["different-workspace", { workspace: "other" }],
		["malformed-cookie", { cookie: "__Secure-better-auth.two_factor=invalid" }],
		["service-cookie-confusion", { service: "" }],
	] as const) {
		const f = fixture(id),
			b = begin(f);
		const before = image(f),
			r = run(f, { cookie: b.cookie, code: code(), ...request });
		assert.equal(r.ok, false, id);
		assert.equal(r.code, "invalid_credentials", id);
		assert.equal(image(f), before, id);
		record(id, { code: r.code });
	}
	const enrollment = fixture(
		"enrollment",
		"delete",
		"UPDATE user SET twoFactorEnabled=0",
	);
	const en = run(enrollment, { mode: "begin" });
	assert.equal(en.enrollmentRequired, true);
	assert.equal(state(enrollment).sessions, 0);
	assert.equal(state(enrollment).pending, 0);
	record("unenrolled-no-setup-authority");
	const busy = fixture("commit-busy"),
		bb = begin(busy),
		bp = start(busy, { cookie: bb.cookie, code: code() }, "before-commit"),
		reader = new Database(busy.control);
	try {
		await marker(bp, busy);
		reader.exec("BEGIN;SELECT count(*) FROM user");
		bp.child.stdin.write("continue\n");
		bp.child.stdin.end();
		const r = await done(bp);
		assert.equal(r.ok, false);
		assert.equal(r.stage, "OutcomeUnknown");
		assert.equal(r.reusable, false);
		assert.equal(r.cookie, undefined);
		reader.exec("ROLLBACK");
		assert.equal(state(busy).sessions, 0);
		assert.equal(state(busy).pending, 3);
		assert.equal(state(busy).replays, 0);
		record("commit-busy-withholds-cookie", { stage: r.stage });
	} finally {
		reader.close();
		clearTimeout(bp.timer);
		bp.child.kill();
		await bp.child.exited;
	}
	// Actual source reader authenticates the native-issued signed session; source verifier can
	// also consume a native-issued standard pending challenge in a separate private copy.
	const oracle = fixture("framework-oracle"),
		realNow = Date.now(),
		ob = begin(oracle, realNow),
		sourceFile = join(oracle.dir, "source-copy.sqlite");
	copyFileSync(oracle.control, sourceFile);
	const nativeSession = run(oracle, {
		cookie: ob.cookie,
		code: code(realNow),
		now: realNow,
	});
	assert.equal(nativeSession.session, true);
	const opts = {
		secret: key,
		trustedOrigins: ["https://example.invalid"],
		baseURL: "https://example.invalid",
		useSecureCookies: true,
	};
	const odb = new Database(oracle.control),
		auth = createBetterAuthRuntime(odb, opts),
		ctx = await auth.$context;
	const session = await auth.api.getSession({
		headers: new Headers({ cookie: nativeSession.cookie }),
	});
	assert.equal(session?.user.id, "u");
	assert(session?.session.id);
	assert.equal(
		new Date(session!.session.expiresAt).getTime() -
			new Date(session!.session.createdAt).getTime(),
		43200000,
	);
	const serialized = await serializeSignedCookie(
		ctx.authCookies.sessionToken.name,
		session!.session.token,
		key,
		ctx.authCookies.sessionToken.attributes,
	);
	assert.equal(serialized.split(";")[0], nativeSession.cookie);
	record("actual-framework-reads-native-session");
	odb.close();
	const sdb = new Database(sourceFile),
		sourceAuth = createBetterAuthRuntime(sdb, opts);
	const sourceResponse = await sourceAuth.handler(
		new Request("https://example.invalid/api/auth/two-factor/verify-totp", {
			method: "POST",
			headers: {
				cookie: ob.cookie,
				origin: "https://example.invalid",
				"content-type": "application/json",
			},
			body: JSON.stringify({ code: code(realNow) }),
		}),
	);
	assert.equal(sourceResponse.status, 200);
	const sourceResult = await sourceResponse.json();
	assert.equal(sourceResult.user.id, "u");
	assert.equal((sdb.query("SELECT count(*) n FROM session").get() as any).n, 1);
	record("actual-framework-consumes-standard-pending-copy");
	sdb.close();
	const sensitiveRecoveryCodes: string[] = [];
	{
		// Actual configured source generator supplies the starting encrypted recovery pool.
		const generated = await generateBackupCodes(
			{ currentVersion: 1, keys: new Map([[1, key]]) },
			{ storeBackupCodes: "encrypted" },
		);
		const recoveryCodes: string[] = generated.backupCodes;
		sensitiveRecoveryCodes.push(...recoveryCodes);
		assert.equal(new Set(recoveryCodes).size, 10);
		assert(
			recoveryCodes.every((c) => /^[A-Za-z0-9]{5}-[A-Za-z0-9]{5}$/.test(c)),
		);
		async function backupFixture(
			id: string,
			mode = "delete",
			pool: unknown = recoveryCodes,
		) {
			const f = fixture(id, mode);
			if (pool === recoveryCodes) {
				const db = new Database(f.control);
				db.query("UPDATE twoFactor SET backupCodes=? WHERE id='f'").run(
					generated.encryptedBackupCodes,
				);
				db.close();
			} else {
				await setPool(f, JSON.stringify(pool));
			}
			return f;
		}
		async function setPool(f: any, plaintext: string) {
			const ciphertext = await symmetricEncrypt({
				key: { currentVersion: 1, keys: new Map([[1, key]]) },
				data: plaintext,
			});
			const db = new Database(f.control);
			db.query("UPDATE twoFactor SET backupCodes=? WHERE id='f'").run(
				ciphertext,
			);
			db.close();
		}
		async function remaining(f: any): Promise<string[]> {
			const db = new Database(f.control);
			const r = db
				.query("SELECT backupCodes FROM twoFactor WHERE id='f'")
				.get() as { backupCodes: string };
			db.close();
			return JSON.parse(
				await symmetricDecrypt({
					key: { currentVersion: 1, keys: new Map([[1, key]]) },
					data: r.backupCodes,
				}),
			);
		}
		function recoveryAudit(f: any) {
			const d = new Database(f.control);
			const rows = d
				.query(
					"SELECT outcome,count(*) n FROM rm_workspace_auth_telemetry_events WHERE endpoint='two-factor-verify-backup-code' GROUP BY outcome ORDER BY outcome",
				)
				.all();
			d.close();
			return rows;
		}
		function replayRows(f: any) {
			const d = new Database(f.control);
			const rows = d
				.query(
					"SELECT * FROM verification WHERE identifier LIKE 'rm-native-totp-step-v1:%' ORDER BY id",
				)
				.all();
			d.close();
			return rows;
		}
		const recover = (
			f: any,
			cookie: string,
			code = recoveryCodes[0],
			request: any = {},
		) => run(f, { backup: true, cookie, code, ...request });
		for (const mode of ["wal", "delete"]) {
			const f = await backupFixture(`backup-${mode}`, mode),
				a = begin(f),
				b = begin(f);
			assert.equal(recover(f, a.cookie, "bad").rejected, "InvalidCode");
			assert.equal(state(f).factor.count, 1);
			assert.equal(recover(f, a.cookie).session, true);
			assert.deepEqual(await remaining(f), recoveryCodes.slice(1));
			assert.equal(state(f).sessions, 1);
			assert.equal(state(f).replays, 0);
			assert.equal(state(f).factor.count, 0);
			assert.deepEqual(recoveryAudit(f), [
				{ outcome: "accepted", n: 1 },
				{ outcome: "rejected", n: 1 },
			]);
			record(`backup-${mode}-atomic-consumption-and-distinct-audit`);
			unchangedRefusal(`backup-${mode}-consumed-challenge`, f, {
				backup: true,
				cookie: a.cookie,
				code: recoveryCodes[0],
			});
			assert.equal(recover(f, b.cookie).rejected, "InvalidCode");
			assert.equal(recover(f, b.cookie, recoveryCodes[1]).session, true);
			assert.deepEqual(await remaining(f), recoveryCodes.slice(2));
			assert.equal(state(f).sessions, 2);
			record(`backup-${mode}-one-time-code-across-different-challenges`);
			const replay = await backupFixture(`backup-replay-${mode}`, mode),
				rt = begin(replay);
			assert.equal(
				run(replay, { cookie: rt.cookie, code: code() }).session,
				true,
			);
			const beforeReplay = replayRows(replay),
				rp = begin(replay);
			assert.equal(recover(replay, rp.cookie).session, true);
			assert.deepEqual(replayRows(replay), beforeReplay);
			record(`backup-${mode}-preserves-TOTP-replay`);
			const late = await backupFixture(`backup-late-${mode}`, mode),
				lp = begin(late),
				before = image(late);
			const failed = recover(late, lp.cookie, recoveryCodes[0], {
				failSql: true,
			});
			assert.equal(failed.ok, false);
			assert.equal(failed.code, "invalid_control");
			assert.equal(failed.reusable, true);
			assert.equal(image(late), before);
			assert.deepEqual(await remaining(late), recoveryCodes);
			assert.equal(state(late).sessions, 0);
			assert.deepEqual(recoveryAudit(late), []);
			assert.equal(recover(late, lp.cookie).session, true);
			record(`backup-${mode}-late-SQL-full-rollback`);
			const race = await backupFixture(`backup-race-${mode}`, mode),
				p1 = begin(race),
				p2 = begin(race),
				held = start(
					race,
					{ backup: true, cookie: p1.cookie, code: recoveryCodes[0] },
					"locked",
				);
			try {
				await marker(held, race);
				const second = recover(race, p2.cookie);
				assert.equal(second.code, "control_busy_or_unavailable");
				held.child.stdin.write("continue\n");
				held.child.stdin.end();
				assert.equal((await done(held)).session, true);
				assert.equal(recover(race, p2.cookie).rejected, "InvalidCode");
				assert.deepEqual(await remaining(race), recoveryCodes.slice(1));
				assert.equal(state(race).sessions, 1);
				assert.equal(recover(race, p2.cookie, recoveryCodes[1]).session, true);
				assert.equal(state(race).sessions, 2);
				record(`backup-${mode}-different-challenge-same-code-race`);
			} finally {
				clearTimeout(held.timer);
				held.child.kill();
				await held.child.exited;
			}
			const locks = await backupFixture(`backup-locks-${mode}`, mode),
				l1 = begin(locks);
			for (let i = 0; i < 5; i++)
				assert.equal(
					run(locks, { cookie: l1.cookie, code: "wrong", backup: i % 2 === 0 })
						.rejected,
					"InvalidCode",
				);
			assert.equal(state(locks).pending, 0);
			assert.equal(state(locks).factor.count, 5);
			const l2 = begin(locks),
				standby = begin(locks);
			for (let i = 0; i < 5; i++)
				assert.equal(
					run(locks, { cookie: l2.cookie, code: "wrong", backup: i % 2 !== 0 })
						.rejected,
					"InvalidCode",
				);
			assert.equal(state(locks).factor.count, 10);
			assert.equal(recover(locks, standby.cookie).rejected, "AccountLocked");
			assert.deepEqual(await remaining(locks), recoveryCodes);
			const unlocked = run(locks, { mode: "begin", now: now + 900000 });
			assert.equal(
				recover(locks, unlocked.cookie, recoveryCodes[0], { now: now + 900000 })
					.session,
				true,
			);
			assert.equal(state(locks).factor.count, 0);
			record(`backup-${mode}-shared-TOTP-failure-budgets-and-lock`);
		}
		for (const [id, pool] of [
			["empty", []],
			["object", {}],
			["null", null],
			["numbers", [1]],
			["duplicate", [recoveryCodes[0], recoveryCodes[0]]],
			["oversized", [...recoveryCodes, "ABCDE-12345"]],
			["wrong-profile", ["short"]],
			["non-ASCII", ["ＡBCDE-12345"]],
		] as const) {
			const f = await backupFixture(`backup-pool-${id}`, "delete", pool),
				p = begin(f),
				before = image(f),
				r = recover(f, p.cookie);
			if (id === "empty") {
				assert.equal(r.rejected, "InvalidCode");
				assert.equal(state(f).factor.count, 1);
			} else {
				assert.equal(r.ok, false);
				assert.equal(r.code, "invalid_control");
				assert.equal(image(f), before);
			}
			assert.equal(state(f).sessions, 0);
			record(`backup-pool-${id}`);
		}
		const malformed = await backupFixture("backup-malformed"),
			mp = begin(malformed);
		await setPool(malformed, "[] trailing");
		unchangedRefusal(
			"backup-malformed-JSON",
			malformed,
			{ backup: true, cookie: mp.cookie, code: recoveryCodes[0] },
			"invalid_control",
		);
		const damaged = await backupFixture("backup-damaged"),
			dp = begin(damaged);
		sql(damaged, "UPDATE twoFactor SET backupCodes='not-an-envelope'");
		unchangedRefusal(
			"backup-damaged-envelope",
			damaged,
			{ backup: true, cookie: dp.cookie, code: recoveryCodes[0] },
			"invalid_control",
		);
		const escaped = await backupFixture("backup-escaped");
		await setPool(escaped, ' [ "\\u0041BCDE-12345", "abcde-67890" ] ');
		const xp = begin(escaped);
		assert.equal(
			recover(escaped, xp.cookie, "abcde-12345").rejected,
			"InvalidCode",
		);
		assert.equal(recover(escaped, xp.cookie, "ABCDE-12345").session, true);
		assert.deepEqual(await remaining(escaped), ["abcde-67890"]);
		record("backup-JSON-escapes-whitespace-case");
		const current = await backupFixture("backup-current-pool"),
			cp = begin(current);
		await setPool(current, JSON.stringify(["XYZab-67890"]));
		assert.equal(recover(current, cp.cookie).rejected, "InvalidCode");
		assert.equal(recover(current, cp.cookie, "XYZab-67890").session, true);
		record("backup-current-pool-read-under-lock");
		for (const [id, change, request] of [
			["expired", "", { now: now + 600000 }],
			[
				"revoked",
				"INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')",
				{},
			],
			["factor-unverified", "UPDATE twoFactor SET verified=0", {}],
			["service-header", "", { service: "" }],
			["workspace", "", { workspace: "other" }],
		] as const) {
			const f = await backupFixture(`backup-refuse-${id}`),
				p = begin(f);
			if (change) sql(f, change);
			unchangedRefusal(`backup-refuse-${id}`, f, {
				backup: true,
				cookie: p.cookie,
				code: recoveryCodes[0],
				...request,
			});
		}
		const busy = await backupFixture("backup-commit-busy"),
			bb = begin(busy),
			busyBefore = image(busy),
			br = new Database(busy.control),
			bp = start(
				busy,
				{ backup: true, cookie: bb.cookie, code: recoveryCodes[0] },
				"before-commit",
			);
		try {
			await marker(bp, busy);
			br.exec("BEGIN;SELECT count(*) FROM user");
			bp.child.stdin.write("continue\n");
			bp.child.stdin.end();
			const r = await done(bp);
			assert.equal(r.stage, "OutcomeUnknown");
			assert.equal(r.reusable, false);
			assert.equal(r.cookie, undefined);
			br.exec("ROLLBACK");
			assert.equal(image(busy), busyBefore);
			assert.deepEqual(await remaining(busy), recoveryCodes);
			record("backup-commit-busy-withholds-cookie-and-preserves-pool");
		} finally {
			br.close();
			clearTimeout(bp.timer);
			bp.child.kill();
			await bp.child.exited;
		}
		const interop = await backupFixture("backup-source-interop"),
			ip1 = begin(interop, Date.now()),
			ip2 = begin(interop, Date.now());
		const nativeBackup = recover(interop, ip1.cookie, recoveryCodes[0], {
			now: Date.now(),
		});
		assert.equal(nativeBackup.session, true);
		const sourceCopy = join(interop.dir, "source-copy.sqlite");
		copyFileSync(interop.control, sourceCopy);
		const interopDb = new Database(sourceCopy),
			sourceBackupAuth = createBetterAuthRuntime(interopDb, opts);
		assert.equal(
			(
				await sourceBackupAuth.api.getSession({
					headers: new Headers({ cookie: nativeBackup.cookie }),
				})
			)?.user.id,
			"u",
		);
		const sourceResponse = await sourceBackupAuth.handler(
			new Request(
				"https://example.invalid/api/auth/two-factor/verify-backup-code",
				{
					method: "POST",
					headers: {
						cookie: ip2.cookie,
						origin: "https://example.invalid",
						"content-type": "application/json",
					},
					body: JSON.stringify({ code: recoveryCodes[1] }),
				},
			),
		);
		assert.equal(sourceResponse.status, 200);
		assert.equal((await sourceResponse.json()).user.id, "u");
		const updated = interopDb
			.query("SELECT backupCodes FROM twoFactor WHERE id='f'")
			.get() as { backupCodes: string };
		assert.deepEqual(
			JSON.parse(
				await symmetricDecrypt({
					key: { currentVersion: 1, keys: new Map([[1, key]]) },
					data: updated.backupCodes,
				}),
			),
			recoveryCodes.slice(2),
		);
		interopDb.close();
		record("backup-actual-source-session-reader-and-HTTP-consumption");
	}
	sensitiveRecoveryCodes.push(
		...(await verifyEnrollment({
			fixture,
			run,
			start,
			done,
			marker,
			sql,
			image,
			record,
			now,
			key,
			opts,
		})),
	);
	sensitiveRecoveryCodes.push(
		...(await verifyManagement({
			fixture,
			run,
			start,
			done,
			marker,
			sql,
			image,
			record,
			now,
			key,
			opts,
		})),
	);
	sensitiveRecoveryCodes.push(
		...(await verifySessionControl({
			fixture,
			run,
			start,
			done,
			marker,
			sql,
			image,
			record,
			now,
			key,
			opts,
		})),
	);
	sensitiveRecoveryCodes.push(
		...(await verifyPasswordReset({
			fixture,
			run,
			start,
			done,
			marker,
			sql,
			image,
			record,
			now,
			key,
			opts,
		})),
	);
	sensitiveRecoveryCodes.push(
		...(await verifyResetRequest({
			fixture,
			run,
			start,
			done,
			marker,
			sql,
			image,
			record,
			now,
			key,
			opts,
		})),
	);
	sensitiveRecoveryCodes.push(
		...(await verifyEmailVerification({
			fixture,
			run,
			start,
			done,
			marker,
			sql,
			image,
			record,
			now,
		})),
	);
	const report = JSON.stringify(
		{
			ok: true,
			scope:
				"Native MFA enrollment, enrolled TOTP/backup-code login, own-code/session management, reset issuance/completion, postcommit injected mail and callback on admitted synthetic control handles; actual source oracle and independent WAL/DELETE process races",
			checks,
			observations,
			limits: [
				"This identity suite uses injected mail; concrete HTTPS transport is verified separately. No public acknowledgement mapper/endpoints, actual handle/lifetime admission or live dual writers",
				"Trusted-device/rememberMe/OS-native credential and full identity/provider parity remain pending",
				"Private native context and replay state are stricter than legacy in-flight challenges",
				"Commit busy exercised; OS/storage crash and restore-lineage qualification remain separate",
			],
		},
		null,
		2,
	);
	for (const value of [
		key,
		secret,
		passwordHash,
		encrypted,
		password,
		...sensitiveRecoveryCodes,
	])
		assert(!report.includes(value), "sensitive persisted MFA evidence");
	console.log(report);
} finally {
	rmSync(work, { recursive: true, force: true });
}
