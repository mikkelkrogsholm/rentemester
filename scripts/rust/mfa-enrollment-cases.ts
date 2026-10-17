/** Synthetic private MFA enrollment: actual installed crypto/URI and process/transaction oracles. */
import assert from "node:assert/strict";
import { copyFileSync } from "node:fs";
import { join } from "node:path";
import { Database } from "bun:sqlite";
import { createOTP } from "../../node_modules/@better-auth/utils/dist/otp.mjs";
import {
	symmetricDecrypt,
	symmetricEncrypt,
} from "../../node_modules/better-auth/dist/crypto/index.mjs";
import { createBetterAuthRuntime } from "../../src/server/better-auth";
export async function verifyEnrollment(c: any) {
	const {
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
	} = c;
	const sensitive: string[] = [];
	const note = (id: string) => record(`enrollment-${id}`);
	const query = (f: any, q: string) => {
		const db = new Database(f.control);
		try {
			return db.query(q).all() as any[];
		} finally {
			db.close();
		}
	};
	const fresh = (id: string, mode = "delete") =>
		fixture(
			`enrollment-${id}`,
			mode,
			`UPDATE user SET twoFactorEnabled=0;DELETE FROM twoFactor;
 INSERT INTO session(id,token,userId,createdAt,updatedAt,expiresAt) VALUES('old-1','old-token-1','u','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-02T00:00:00.000Z'),('old-2','old-token-2','u','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-02T00:00:00.000Z');`,
		);
	const qfactor = (f: any) =>
		query(f, "SELECT * FROM twoFactor WHERE userId='u'")[0];
	const decrypt = (text: string) =>
		symmetricDecrypt({
			key: { currentVersion: 1, keys: new Map([[1, key]]) },
			data: text,
		});
	async function setup(f: any, extra: any = {}) {
		const b = run(f, { mode: "enroll-begin", ...extra });
		assert.equal(b.ok, true);
		assert.equal(b.pending, true);
		assert.equal(b.codes, undefined);
		assert.equal(b.session, undefined);
		assert.equal(b.setCookie, undefined);
		const factor = qfactor(f),
			secret = await decrypt(factor.secret);
		sensitive.push(b.cookie, b.uri, secret, factor.secret, factor.backupCodes);
		assert.equal(secret.length, 32);
		assert.equal(
			b.uri,
			createOTP(secret).url(extra.issuer ?? "Rentemester", "u@example.invalid"),
		);
		const pool = JSON.parse(await decrypt(factor.backupCodes));
		assert.equal(pool.length, 10);
		assert.equal(new Set(pool).size, 10);
		for (const s of pool) {
			assert.match(s, /^[a-zA-Z0-9]{5}-[a-zA-Z0-9]{5}$/);
			sensitive.push(s);
		}
		return { ...b, secret, pool };
	}
	const otp = (secret: string, t = now) =>
		createOTP(secret).hotp(Math.floor(t / 30000));
	const confirm = (f: any, b: any, code: string, extra: any = {}) =>
		run(f, { mode: "enroll-confirm", cookie: b.cookie, code, ...extra });
	function refused(
		id: string,
		f: any,
		request: any,
		code = "invalid_credentials",
	) {
		const before = image(f),
			r = run(f, request);
		assert.equal(r.ok, false, id);
		assert.equal(r.code, code, id);
		assert.equal(r.stage, "NotCommitted");
		assert.equal(r.cookie, undefined);
		assert.equal(r.uri, undefined);
		assert.equal(r.codes, undefined);
		assert.equal(image(f), before, id);
		note(id);
	}
	const consumed = (f: any, b: any, code: string, id: string) =>
		refused(id, f, { mode: "enroll-confirm", cookie: b.cookie, code });
	for (const mode of ["wal", "delete"]) {
		const f = fresh(`happy-${mode}`, mode),
			b = await setup(f, { issuer: "Rente mester!~'() Æ" });
		assert.equal(query(f, "SELECT * FROM session").length, 2);
		assert.equal(query(f, "SELECT twoFactorEnabled n FROM user")[0].n, 0);
		assert.equal(
			query(
				f,
				"SELECT * FROM rm_workspace_mfa_events WHERE event_type='mfa_enabled'",
			).length,
			0,
		);
		assert.equal(qfactor(f).verified, 0);
		assert.equal(query(f, "SELECT * FROM verification").length, 3);
		assert.equal(
			query(
				f,
				"SELECT * FROM rm_workspace_auth_telemetry_events WHERE endpoint='two-factor-enable' AND outcome='accepted'",
			).length,
			1,
		);
		note(`${mode}-pending-secret-URI-no-session-no-codes`);
		refused(`${mode}-enrollment-cookie-refuses-TOTP-login`, f, {
			cookie: b.cookie,
			code: await otp(b.secret),
		});
		refused(`${mode}-enrollment-cookie-refuses-backup-login`, f, {
			backup: true,
			cookie: b.cookie,
			code: b.pool[0],
		});
		const bad = confirm(f, b, "not-an-otp");
		assert.equal(bad.rejected, "InvalidCode");
		assert.equal(qfactor(f).failedVerificationCount, 1);
		assert.equal(bad.codes, undefined);
		note(`${mode}-wrong-TOTP-counted`);
		const r = confirm(f, b, await otp(b.secret));
		assert.equal(r.session, true);
		assert.equal(r.nativeCurrentFresh, true);
		assert.deepEqual(r.codes, b.pool);
		sensitive.push(r.cookie, ...r.codes);
		assert.equal(query(f, "SELECT * FROM session").length, 1);
		assert.equal(
			query(f, "SELECT * FROM session WHERE id LIKE 'old-%'").length,
			0,
		);
		assert.equal(query(f, "SELECT twoFactorEnabled n FROM user")[0].n, 1);
		assert.equal(qfactor(f).verified, 1);
		assert.equal(qfactor(f).failedVerificationCount, 0);
		assert.equal(
			query(
				f,
				"SELECT * FROM verification WHERE identifier LIKE '2fa-%' OR identifier LIKE 'rm-native-enroll-v1:%'",
			).length,
			0,
		);
		assert.equal(
			query(
				f,
				"SELECT * FROM verification WHERE identifier LIKE 'rm-native-totp-step-v1:%'",
			).length,
			1,
		);
		const mfa = query(
			f,
			"SELECT * FROM rm_workspace_mfa_events WHERE event_type='mfa_enabled'",
		);
		assert.equal(mfa.length, 1);
		assert.equal(mfa[0].actor, "system:better-auth");
		assert.equal(
			query(
				f,
				"SELECT * FROM rm_workspace_auth_state_events WHERE state_transition='session_deleted'",
			).length,
			2,
		);
		assert.equal(
			query(
				f,
				"SELECT * FROM rm_workspace_auth_telemetry_events WHERE endpoint='two-factor-verify' AND outcome='accepted'",
			).length,
			1,
		);
		note(`${mode}-atomic-confirm-revokes-old-sessions-codes-once`);
		consumed(
			f,
			b,
			await otp(b.secret),
			`${mode}-consumed-no-code-redisclosure`,
		);
		refused(`${mode}-enabled-cannot-reenroll`, f, { mode: "enroll-begin" });
		const login = run(f, { mode: "begin" });
		assert.equal(login.pending, true);
		refused(`${mode}-login-cookie-refuses-enrollment`, f, {
			mode: "enroll-confirm",
			cookie: login.cookie,
			code: await otp(b.secret),
		});
		const replay = run(f, { cookie: login.cookie, code: await otp(b.secret) });
		assert.equal(replay.rejected, "ReplayedCode");
		note(`${mode}-enrollment-counter-prevents-login-replay`);
		assert.equal(
			run(f, {
				cookie: login.cookie,
				code: await otp(b.secret, now + 30000),
				now: now + 30000,
			}).session,
			true,
		);
		note(`${mode}-next-TOTP-native-login`);
		const backup = run(f, { mode: "begin" });
		assert.equal(
			run(f, { backup: true, cookie: backup.cookie, code: b.pool[0] }).session,
			true,
		);
		assert.deepEqual(
			JSON.parse(await decrypt(qfactor(f).backupCodes)),
			b.pool.slice(1),
		);
		note(`${mode}-native-backup-login-after-enrollment`);
		const restart = fresh(`restart-${mode}`, mode),
			old = await setup(restart);
		assert.equal(confirm(restart, old, "wrong").rejected, "InvalidCode");
		const oldFactor = qfactor(restart),
			second = await setup(restart);
		assert.equal(qfactor(restart).id, oldFactor.id);
		assert.equal(qfactor(restart).failedVerificationCount, 1);
		assert.notEqual(second.secret, old.secret);
		assert.notDeepEqual(second.pool, old.pool);
		assert.equal(query(restart, "SELECT * FROM twoFactor").length, 1);
		consumed(
			restart,
			old,
			await otp(old.secret),
			`${mode}-restart-invalidates-old-context-preserves-budget`,
		);
		assert.equal(
			confirm(restart, second, second.pool[0]).rejected,
			"InvalidCode",
		);
		note(`${mode}-backup-cannot-complete-setup`);
		const late = fresh(`late-${mode}`, mode),
			lb = await setup(late),
			before = image(late);
		const failure = confirm(late, lb, await otp(lb.secret), { failSql: true });
		assert.equal(failure.code, "invalid_control");
		assert.equal(failure.reusable, true);
		assert.equal(failure.codes, undefined);
		assert.equal(image(late), before);
		assert.equal(query(late, "SELECT * FROM session").length, 2);
		note(
			`${mode}-late-SQL-restores-all-flags-sessions-pool-challenge-replay-audit`,
		);
		assert.equal(confirm(late, lb, await otp(lb.secret)).session, true);
		note(`${mode}-rollback-preserves-confirmability`);
		const startLate = fresh(`start-late-${mode}`, mode),
			startBefore = image(startLate),
			sl = run(startLate, { mode: "enroll-begin", failSql: true });
		assert.equal(sl.code, "invalid_control");
		assert.equal(sl.uri, undefined);
		assert.equal(image(startLate), startBefore);
		note(`${mode}-begin-late-SQL-withholds-URI-and-preserves-state`);
		const race = fresh(`confirm-race-${mode}`, mode),
			rb = await setup(race),
			p = start(
				race,
				{
					mode: "enroll-confirm",
					cookie: rb.cookie,
					code: await otp(rb.secret),
				},
				"locked",
			);
		try {
			await marker(p, race);
			const other = confirm(race, rb, await otp(rb.secret));
			assert.equal(other.code, "control_busy_or_unavailable");
			p.child.stdin.write("continue\n");
			p.child.stdin.end();
			const won = await done(p);
			assert.equal(won.session, true);
			assert.deepEqual(won.codes, rb.pool);
			assert.equal(query(race, "SELECT * FROM session").length, 1);
			consumed(
				race,
				rb,
				await otp(rb.secret),
				`${mode}-second-confirm-no-redisclosure`,
			);
			note(`${mode}-one-concurrent-confirm-commit`);
		} finally {
			clearTimeout(p.timer);
			p.child.kill();
			await p.child.exited;
		}
		const parallel = fresh(`begin-race-${mode}`, mode),
			bp = start(parallel, { mode: "enroll-begin" }, "before-commit");
		try {
			await marker(bp, parallel);
			const other = run(parallel, { mode: "enroll-begin" });
			assert.equal(other.code, "control_busy_or_unavailable");
			assert.equal(other.uri, undefined);
			bp.child.stdin.write("continue\n");
			bp.child.stdin.end();
			const won = await done(bp);
			assert.equal(won.pending, true);
			sensitive.push(won.cookie, won.uri);
			assert.equal(query(parallel, "SELECT * FROM twoFactor").length, 1);
			const current = await decrypt(qfactor(parallel).secret);
			assert.equal(
				won.uri,
				createOTP(current).url("Rentemester", "u@example.invalid"),
			);
			note(`${mode}-concurrent-begins-single-factor`);
		} finally {
			clearTimeout(bp.timer);
			bp.child.kill();
			await bp.child.exited;
		}
	}
	const locks = fresh("lock-budget");
	let b = await setup(locks);
	for (let i = 1; i <= 5; i++) {
		assert.equal(confirm(locks, b, "wrong").rejected, "InvalidCode");
		assert.equal(qfactor(locks).failedVerificationCount, i);
	}
	consumed(locks, b, await otp(b.secret), "fifth-failure-burns-enrollment");
	b = await setup(locks);
	for (let i = 6; i <= 10; i++) {
		assert.equal(confirm(locks, b, "wrong").rejected, "InvalidCode");
		assert.equal(qfactor(locks).failedVerificationCount, i);
	}
	assert.equal(
		qfactor(locks).lockedUntil,
		new Date(now + 900000).toISOString(),
	);
	note("ten-account-failures-across-restart-lock");
	refused("locked-cannot-restart", locks, { mode: "enroll-begin" });
	refused("before-exact-unlock-refused", locks, {
		mode: "enroll-begin",
		now: now + 899999,
	});
	b = await setup(locks, { now: now + 900000 });
	assert.equal(
		confirm(locks, b, await otp(b.secret, now + 900000), { now: now + 900000 })
			.session,
		true,
	);
	assert.equal(qfactor(locks).failedVerificationCount, 0);
	assert.equal(qfactor(locks).lockedUntil, null);
	note("exact-unlock-confirms-and-resets-budget");
	for (const [id, change, err] of [
		[
			"disabled-access",
			"INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')",
		],
		["unverified-email", "UPDATE user SET emailVerified=0"],
		["password-change", "UPDATE account SET password='changed'"],
		["factor-verified", "UPDATE twoFactor SET verified=1"],
		["factor-null", "UPDATE twoFactor SET verified=NULL"],
		["factor-removed", "DELETE FROM twoFactor"],
		["user-enabled", "UPDATE user SET twoFactorEnabled=1"],
		["secret-changed", "UPDATE twoFactor SET secret='different'"],
		["backup-pool-changed", "UPDATE twoFactor SET backupCodes='different'"],
		[
			"ordered-exhaustion",
			"INSERT INTO rm_workspace_user_access_events(id,user_id,event_type,workspace_role,actor) VALUES(9223372036854775807,'u','activate','member','agent:synthetic')",
			"invalid_control",
		],
	] as const) {
		const f = fresh(id),
			b = await setup(f);
		sql(f, change);
		refused(
			"pending-" + id,
			f,
			{ mode: "enroll-confirm", cookie: b.cookie, code: await otp(b.secret) },
			err ?? "invalid_credentials",
		);
	}
	// Standalone begin gates and unsupported unverified state.
	for (const [id, change, err] of [
		[
			"begin-unverified-email",
			"UPDATE user SET emailVerified=0",
			"invalid_credentials",
		],
		[
			"begin-disabled",
			"INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')",
			"invalid_credentials",
		],
	] as const) {
		const f = fresh(id);
		sql(f, change);
		refused(id, f, { mode: "enroll-begin" }, err);
	}
	const badPassword = fresh("bad-password");
	refused("password-required", badPassword, {
		mode: "enroll-begin",
		password: "wrong",
	});
	refused(
		"issuer-required",
		badPassword,
		{ mode: "enroll-begin", issuer: "" },
		"invalid_auth_config",
	);
	for (const [id, change] of [
		["null", "UPDATE twoFactor SET verified=NULL"],
		[
			"duplicate",
			"INSERT INTO twoFactor(id,userId,secret,backupCodes,verified,failedVerificationCount) SELECT 'duplicate-factor',userId,secret,backupCodes,0,0 FROM twoFactor",
		],
		["bad-lock", "UPDATE twoFactor SET lockedUntil='malformed'"],
		["verified", "UPDATE twoFactor SET verified=1"],
		["bad-failure", "UPDATE twoFactor SET failedVerificationCount=-1"],
		[
			"replay",
			"INSERT INTO verification(id,identifier,value,createdAt,updatedAt,expiresAt) SELECT 'stale-replay','rm-native-totp-step-v1:'||id,'1','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','9999-12-31T23:59:59.999Z' FROM twoFactor",
		],
	]) {
		const f = fresh(`restart-${id}`);
		await setup(f);
		sql(f, change);
		refused(
			`restart-refuses-${id}`,
			f,
			{ mode: "enroll-begin" },
			id === "null" || id === "verified"
				? "invalid_credentials"
				: "invalid_control",
		);
	}
	const expired = fresh("expiry"),
		eb = await setup(expired);
	refused("exact-ten-minute-expiry", expired, {
		mode: "enroll-confirm",
		cookie: eb.cookie,
		code: await otp(eb.secret, now + 600000),
		now: now + 600000,
	});
	refused("clock-rollback", expired, {
		mode: "enroll-confirm",
		cookie: eb.cookie,
		code: await otp(eb.secret),
		now: now - 1,
	});
	for (const phase of ["begin", "confirm"]) {
		const f = fresh(`commit-busy-${phase}`),
			b = phase === "confirm" ? await setup(f) : null,
			before = image(f),
			request =
				phase === "begin"
					? { mode: "enroll-begin" }
					: {
							mode: "enroll-confirm",
							cookie: b.cookie,
							code: await otp(b.secret),
						},
			p = start(f, request, "before-commit"),
			reader = new Database(f.control);
		try {
			await marker(p, f);
			reader.exec("BEGIN;SELECT count(*) FROM user");
			p.child.stdin.write("continue\n");
			p.child.stdin.end();
			const r = await done(p);
			assert.equal(r.stage, "OutcomeUnknown");
			assert.equal(r.reusable, false);
			assert.equal(r.cookie, undefined);
			assert.equal(r.uri, undefined);
			assert.equal(r.codes, undefined);
			reader.exec("ROLLBACK");
			assert.equal(image(f), before);
			note(`${phase}-real-commit-busy-withholds-secrets`);
		} finally {
			reader.close();
			clearTimeout(p.timer);
			p.child.kill();
			await p.child.exited;
		}
	}
	// Source crypto and live session/TOTP/backup readers operate on a separate private copy.
	const interop = fresh("source-interop"),
		time = Date.now(),
		ib = await setup(interop, { now: time }),
		ir = confirm(interop, ib, await otp(ib.secret, time), { now: time });
	assert.equal(ir.session, true);
	const pending = run(interop, { mode: "begin", now: time }),
		pending2 = run(interop, { mode: "begin", now: time }),
		copy = join(interop.dir, "source-copy.sqlite");
	copyFileSync(interop.control, copy);
	const sourceDb = new Database(copy),
		auth = createBetterAuthRuntime(sourceDb, opts);
	try {
		assert.equal(
			(
				await auth.api.getSession({
					headers: new Headers({ cookie: ir.cookie }),
				})
			)?.user.id,
			"u",
		);
		note("actual-source-reads-completed-native-session");
		for (const [method, pendingCookie, code] of [
			["verify-totp", pending.cookie, await otp(ib.secret, Date.now())],
			["verify-backup-code", pending2.cookie, ib.pool[0]],
		]) {
			const response = await auth.handler(
				new Request(`https://example.invalid/api/auth/two-factor/${method}`, {
					method: "POST",
					headers: {
						cookie: pendingCookie,
						origin: "https://example.invalid",
						"content-type": "application/json",
					},
					body: JSON.stringify({ code }),
				}),
			);
			assert.equal(response.status, 200);
			assert.equal((await response.json()).user.id, "u");
			note(`actual-source-${method}-uses-native-enrolled-rows`);
		}
		const pool = (
			sourceDb.query("SELECT backupCodes FROM twoFactor").get() as any
		).backupCodes;
		assert.deepEqual(JSON.parse(await decrypt(pool)), ib.pool.slice(1));
		note("actual-source-consumes-native-issued-backup-code");
	} finally {
		sourceDb.close();
	}
	return sensitive;
}
