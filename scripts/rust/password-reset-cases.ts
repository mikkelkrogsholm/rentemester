/** Source-issued reset consumption, credential creation/update and atomic revocation. */
import assert from "node:assert/strict";
import { Database } from "bun:sqlite";
import { verifyPassword } from "../../node_modules/better-auth/dist/crypto/password.mjs";
import { serializeSignedCookie } from "../../node_modules/better-call/dist/cookies.mjs";
import { createBetterAuthRuntime } from "../../src/server/better-auth";
export async function verifyPasswordReset(c: any) {
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
		opts,
		key,
	} = c;
	const token = "AbCdEfGhIjKlMnOpQrStUv01",
		sibling = "AbCdEfGhIjKlMnOpQrStUv02",
		foreign = "AbCdEfGhIjKlMnOpQrStUv03",
		password = "Replacement password 456!";
	const sensitive = [token, sibling, foreign, password],
		note = (id: string) => record(`reset-${id}`);
	function q(f: any, text: string) {
		const db = new Database(f.control);
		try {
			return db.query(text).all() as any[];
		} finally {
			db.close();
		}
	}
	function fresh(id: string, mode = "delete", time = now) {
		const f = fixture(`reset-${id}`, mode),
			db = new Database(f.control);
		db.exec(
			"INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES('v','Synthetic other','v@example.invalid',0,0,'2026-01-01','2026-01-01');",
		);
		const add = db.query(
			"INSERT INTO verification(id,identifier,value,createdAt,updatedAt,expiresAt) VALUES(?,?,?,?,?,?)",
		);
		for (const [id, t, subject] of [
			["r", token, "u"],
			["rs", sibling, "u"],
			["rv", foreign, "v"],
		])
			add.run(
				id,
				`reset-password:${t}`,
				subject,
				new Date(time - 1000).toISOString(),
				new Date(time - 1000).toISOString(),
				new Date(time + 3599000).toISOString(),
			);
		add.run(
			"unrelated",
			"unrelated-verification",
			"u",
			new Date(time).toISOString(),
			new Date(time).toISOString(),
			new Date(time + 3600000).toISOString(),
		);
		const session = db.query(
			"INSERT INTO session(id,token,userId,createdAt,updatedAt,expiresAt) VALUES(?,?,?,?,?,?)",
		);
		for (const [id, subject, expiry] of [
			["active", "u", time + 43200000],
			["expired", "u", time - 1],
			["foreign", "v", time + 43200000],
		] as any[])
			session.run(
				id,
				`reset-synthetic-session-${id}`,
				subject,
				new Date(time - 1000).toISOString(),
				new Date(time - 1000).toISOString(),
				new Date(expiry).toISOString(),
			);
		db.close();
		return f;
	}
	const act = (f: any, extra: any = {}) =>
		run(f, { mode: "password-reset", token, newPassword: password, ...extra });
	function refused(
		id: string,
		f: any,
		extra: any = {},
		code = "invalid_credentials",
	) {
		const before = image(f),
			r = act(f, extra);
		assert.equal(r.ok, false, id);
		assert.equal(r.code, code, id);
		assert.equal(r.stage, "NotCommitted", id);
		assert.equal(image(f), before, id);
		assert.equal(r.cookie, undefined);
		assert.equal(r.clearCookies, undefined);
		note(id);
		return r;
	}
	const unchanged = (f: any) =>
		JSON.stringify(
			["user", "twoFactor", "rm_workspace_user_access_events"].map((t) =>
				q(f, `SELECT * FROM ${t} ORDER BY rowid`),
			),
		);
	for (const mode of ["wal", "delete"]) {
		const f = fresh(`${mode}-success`, mode),
			before = unchanged(f),
			old = q(f, "SELECT * FROM account WHERE id='a'")[0];
		const pending = run(f, { mode: "begin" });
		assert.equal(pending.pending, true);
		sensitive.push(pending.cookie);
		assert.deepEqual(act(f), { ok: true });
		const account = q(f, "SELECT * FROM account WHERE id='a'")[0];
		sensitive.push(account.password);
		assert.notEqual(account.password, old.password);
		assert(await verifyPassword({ hash: account.password, password }));
		assert(
			!(await verifyPassword({
				hash: account.password,
				password: "Synthetic password 123!",
			})),
		);
		const { password: oldHash, updatedAt: oldUpdated, ...oldStable } = old,
			{ password: newHash, updatedAt: newUpdated, ...newStable } = account;
		assert.deepEqual(newStable, oldStable);
		assert.equal(newUpdated, new Date(now).toISOString());
		assert.equal(unchanged(f), before);
		note(`${mode}-native-hash-source-verifier-and-preserved-account`);
		assert.deepEqual(q(f, "SELECT id FROM session ORDER BY id"), [
			{ id: "foreign" },
		]);
		assert.deepEqual(
			q(
				f,
				"SELECT id FROM verification WHERE id IN ('r','rs','rv','unrelated') ORDER BY id",
			),
			[{ id: "rv" }, { id: "unrelated" }],
		);
		assert.equal(
			q(
				f,
				"SELECT * FROM rm_workspace_auth_state_events WHERE state_transition='credential_updated'",
			).length,
			1,
		);
		assert.equal(
			q(
				f,
				"SELECT * FROM rm_workspace_auth_state_events WHERE state_transition='session_deleted'",
			).length,
			2,
		);
		const a = q(
			f,
			"SELECT * FROM workspace_audit WHERE entity_type='native_password_reset_v1'",
		);
		assert.equal(a.length, 1);
		assert.equal(a[0].event_type, "password_reset_completed");
		assert.equal(a[0].actor, "user:u");
		assert.equal(a[0].created_at, new Date(now).toISOString());
		assert.deepEqual(JSON.parse(a[0].entity_id), ["u", 1, false, 2, 2]);
		const t = q(
			f,
			"SELECT * FROM rm_workspace_auth_telemetry_events WHERE endpoint='reset-password'",
		);
		assert.equal(t.length, 1);
		assert.equal(t[0].outcome, "accepted");
		assert.equal(t[0].identity_hash, null);
		assert.equal(t[0].session_id, null);
		assert.equal(t[0].created_at, new Date(now).toISOString());
		note(`${mode}-atomic-reset-links-sessions-triggers-audit`);
		assert.equal(run(f, { mode: "begin" }).code, "invalid_credentials");
		assert.equal(run(f, { mode: "begin", password }).pending, true);
		note(`${mode}-only-new-password-starts-MFA`);
		assert.equal(
			run(f, { cookie: pending.cookie, code: "000000" }).code,
			"invalid_credentials",
		);
		note(`${mode}-prior-native-MFA-context-invalidated`);
		refused(`${mode}-same-token-one-time`, f);
		refused(`${mode}-sibling-token-revoked`, f, { token: sibling });
		assert.equal(act(f, { token: foreign }).ok, true);
		assert.equal(q(f, "SELECT * FROM session").length, 0);
		note(`${mode}-foreign-link-only-affects-own-subject`);
	}
	for (const [id, change] of [
		["without-local-credential", "DELETE FROM account WHERE id='a'"],
		[
			"unverified-disabled-MFA-off",
			"UPDATE user SET emailVerified=0,twoFactorEnabled=0 WHERE id='u'; DELETE FROM twoFactor WHERE userId='u'; INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')",
		],
	]) {
		const f = fresh(id);
		sql(f, change);
		const before = unchanged(f);
		assert.equal(act(f).ok, true);
		assert.equal(unchanged(f), before);
		const accounts = q(
			f,
			"SELECT * FROM account WHERE issuer='local:credential' AND accountId='u'",
		);
		assert.equal(accounts.length, 1);
		assert(await verifyPassword({ hash: accounts[0].password, password }));
		sensitive.push(accounts[0].password);
		assert.equal(q(f, "SELECT * FROM session WHERE userId='u'").length, 0);
		note(id);
		if (id === "without-local-credential") {
			assert.equal(
				q(
					f,
					"SELECT * FROM rm_workspace_auth_state_events WHERE state_transition='credential_updated'",
				).length,
				0,
			);
			assert.deepEqual(
				JSON.parse(
					q(
						f,
						"SELECT entity_id FROM workspace_audit WHERE entity_type='native_password_reset_v1'",
					)[0].entity_id,
				),
				["u", 1, true, 2, 2],
			);
			note("credential-create-no-fabricated-update-trigger");
		} else {
			assert.equal(
				run(f, { mode: "begin", password }).code,
				"invalid_credentials",
			);
			note("reset-never-reactivates-disabled-user");
		}
	}
	const missingAccess = fresh("no-access");
	sql(
		missingAccess,
		"UPDATE verification SET value='v' WHERE id='r'; UPDATE user SET emailVerified=1,twoFactorEnabled=1 WHERE id='v'; INSERT INTO twoFactor SELECT 'fv',secret,backupCodes,'v',verified,failedVerificationCount,lockedUntil FROM twoFactor WHERE id='f'",
	);
	assert.equal(act(missingAccess).ok, true);
	assert.equal(
		run(missingAccess, { mode: "begin", email: "v@example.invalid", password })
			.code,
		"invalid_credentials",
	);
	note("reset-without-membership-does-not-grant-access");
	for (const [id, value] of [
		["short", "x".repeat(11)],
		["long", "x".repeat(129)],
		["astral-long", "😀".repeat(65)],
		["nfkc-before-policy", "ﬃ".repeat(4)],
	])
		refused(
			`password-${id}`,
			fresh(`password-${id}`),
			{ newPassword: value },
			"invalid_password",
		);
	for (const [id, value] of [
		["min", "x".repeat(12)],
		["max", "x".repeat(128)],
		["astral-min", "😀".repeat(6)],
		["astral-max", "😀".repeat(64)],
		["spaces", " ".repeat(12)],
		["nfkc", "ﬃ".repeat(12)],
	]) {
		const f = fresh(`password-${id}`);
		assert.equal(act(f, { newPassword: value }).ok, true);
		const hash = q(f, "SELECT password FROM account WHERE id='a'")[0].password;
		sensitive.push(hash, value);
		assert(await verifyPassword({ hash, password: value }));
		note(`password-${id}-source-compatible`);
	}
	for (const [id, value] of [
		["missing", ""],
		["long", "x".repeat(25)],
		["short", "x".repeat(23)],
		["separator", "x".repeat(23) + ":"],
		["encoded", "%41" + token.slice(1)],
		["unicode", "é" + token.slice(1)],
		["control", token.slice(0, 23) + "\n"],
		["wrong", "Z".repeat(24)],
		["case", token.toLowerCase()],
	])
		refused(`token-${id}`, fresh(`token-${id}`), { token: value });
	for (const [id, change, code] of [
		[
			"duplicate",
			"INSERT INTO verification SELECT 'duplicate',identifier,value,expiresAt,createdAt,updatedAt FROM verification WHERE id='r'",
			"invalid_control",
		],
		[
			"missing-row",
			"DELETE FROM verification WHERE id='r'",
			"invalid_credentials",
		],
		[
			"missing-user",
			"UPDATE verification SET value='missing' WHERE id='r'",
			"invalid_credentials",
		],
		[
			"bad-subject",
			"UPDATE verification SET value=char(10) WHERE id='r'",
			"invalid_control",
		],
		[
			"bad-created",
			"UPDATE verification SET createdAt='bad' WHERE id='r'",
			"invalid_control",
		],
		[
			"future-updated",
			"UPDATE verification SET updatedAt='2026-01-01T00:00:00.001Z' WHERE id='r'",
			"invalid_control",
		],
		[
			"updated-before-created",
			"UPDATE verification SET updatedAt='2025-12-31T23:59:58.999Z' WHERE id='r'",
			"invalid_control",
		],
		[
			"over-one-hour",
			"UPDATE verification SET expiresAt='2026-01-01T00:59:59.001Z' WHERE id='r'",
			"invalid_control",
		],
		[
			"expired",
			"UPDATE verification SET expiresAt='2025-12-31T23:59:59.999Z' WHERE id='r'",
			"invalid_credentials",
		],
		[
			"exact-expiry",
			"UPDATE verification SET expiresAt='2026-01-01T00:00:00.000Z' WHERE id='r'",
			"invalid_credentials",
		],
		[
			"invalid-life",
			"UPDATE verification SET expiresAt=createdAt WHERE id='r'",
			"invalid_control",
		],
		[
			"canonical-key-foreign-owner",
			"UPDATE account SET userId='v' WHERE id='a'",
			"invalid_control",
		],
		[
			"canonical-key-wrong-provider",
			"UPDATE account SET providerId='foreign' WHERE id='a'",
			"invalid_control",
		],
	]) {
		const f = fresh(id);
		sql(f, change);
		refused(id, f, {}, code);
	}
	const boundary = fresh("expiry-minus-one");
	sql(
		boundary,
		"UPDATE verification SET expiresAt='2026-01-01T00:00:00.001Z' WHERE id='r'",
	);
	assert.equal(act(boundary).ok, true);
	note("expiry-minus-one-accepted");
	const providers = fresh("provider-preservation");
	sql(
		providers,
		"INSERT INTO account(id,userId,accountId,providerId,issuer,password,createdAt,updatedAt,accessToken) VALUES('oauth','u','foreign-subject','foreign','https://provider.invalid','opaque-provider-password','2026-01-01','2026-01-01','opaque-provider-access')",
	);
	const foreignBefore = q(providers, "SELECT * FROM account WHERE id='oauth'");
	assert.equal(act(providers).ok, true);
	assert.deepEqual(
		q(providers, "SELECT * FROM account WHERE id='oauth'"),
		foreignBefore,
	);
	note("foreign-provider-credentials-never-touched");
	for (const [id, extra] of [
		["negative-clock", { now: -1 }],
		["out-of-range-clock", { now: 253402300800000 }],
		["invalid-audit-config", { auditVersion: 9007199254740992 }],
	] as any[])
		refused(
			id,
			fresh(id),
			extra,
			id === "invalid-audit-config"
				? "invalid_auth_config"
				: "invalid_auth_clock",
		);
	for (const [id, preflight] of [
		["query-only", "PRAGMA query_only=ON"],
		["attached", "ATTACH DATABASE ':memory:' AS other"],
		["active-transaction", "BEGIN"],
	]) {
		const f = fresh(id),
			r = refused(id, f, { preflight }, "connection_refused");
		assert.equal(r.reusable, false);
	}
	for (const mode of ["wal", "delete"])
		for (const create of [false, true]) {
			const f = fresh(`rollback-${mode}-${create}`, mode);
			if (create) sql(f, "DELETE FROM account WHERE id='a'");
			const r = refused(
				`late-SQL-rollback-${mode}-${create}`,
				f,
				{ failSql: true },
				"invalid_control",
			);
			assert.equal(r.reusable, true);
			assert.equal(act(f).ok, true);
			note(`rolled-back-token-remains-usable-${mode}-${create}`);
		}
	async function finish(p: any) {
		p.child.stdin.write("continue\n");
		p.child.stdin.end();
		return await done(p);
	}
	async function cleanup(p: any) {
		clearTimeout(p.timer);
		p.child.kill();
		await p.child.exited;
	}
	for (const mode of ["wal", "delete"]) {
		const f = fresh(`writer-race-${mode}`, mode),
			p = start(
				f,
				{ mode: "password-reset", token, newPassword: password },
				"before-commit",
			);
		try {
			await marker(p, f);
			assert.equal(act(f).code, "control_busy_or_unavailable");
			assert.equal(
				run(f, { mode: "begin" }).code,
				"control_busy_or_unavailable",
			);
			assert.deepEqual(await finish(p), { ok: true });
			refused(`serialized-replay-${mode}`, f);
			note(`writer-serializes-reset-and-login-${mode}`);
		} finally {
			await cleanup(p);
		}
		const changed = fresh(`unlocked-${mode}`, mode),
			racer = start(
				changed,
				{ mode: "password-reset", token, newPassword: password },
				"reset-unlocked",
			);
		try {
			await marker(racer, changed);
			sql(changed, "DELETE FROM verification WHERE id='r'");
			const before = image(changed);
			assert.equal((await finish(racer)).code, "invalid_credentials");
			assert.equal(image(changed), before);
			note(`current-token-only-after-unlocked-KDF-${mode}`);
		} finally {
			await cleanup(racer);
		}
	}
	for (const create of [false, true]) {
		const f = fresh(`commit-busy-${create}`);
		if (create) sql(f, "DELETE FROM account WHERE id='a'");
		const before = image(f),
			p = start(
				f,
				{ mode: "password-reset", token, newPassword: password },
				"before-commit",
			),
			reader = new Database(f.control);
		try {
			await marker(p, f);
			reader.exec("BEGIN;SELECT count(*) FROM user");
			const r = await finish(p);
			assert.equal(r.stage, "OutcomeUnknown");
			assert.equal(r.reusable, false);
			assert.equal(r.cookie, undefined);
			reader.exec("ROLLBACK");
			assert.equal(image(f), before);
			note(`real-commit-busy-retires-handle-${create}`);
		} finally {
			reader.close();
			await cleanup(p);
		}
	}
	const interop = fresh("actual-source", "delete", Date.now()),
		db = new Database(interop.control);
	let delivered: any;
	const auth = createBetterAuthRuntime(db, {
		...opts,
		emailSender: {
			async send(message: any) {
				delivered = message;
			},
		},
	});
	try {
		const response = await auth.handler(
			new Request("https://example.invalid/api/auth/request-password-reset", {
				method: "POST",
				headers: {
					origin: "https://example.invalid",
					"content-type": "application/json",
				},
				body: JSON.stringify({ email: "u@example.invalid" }),
			}),
		);
		assert.equal(response.status, 200);
		assert.equal(delivered.kind, "password-reset");
		assert.equal(delivered.token.length, 24);
		sensitive.push(delivered.token, delivered.url);
		const real = q(
			interop,
			"SELECT * FROM verification WHERE id NOT IN ('r','rs','rv','unrelated') AND identifier LIKE 'reset-password:%'",
		);
		assert.equal(real.length, 1);
		assert.equal(real[0].identifier, `reset-password:${delivered.token}`);
		note("actual-source-request-token-format-and-persistence");
		assert.equal(
			act(interop, { token: delivered.token, now: Date.now() }).ok,
			true,
		);
		const hash = q(interop, "SELECT password FROM account WHERE id='a'")[0]
			.password;
		sensitive.push(hash);
		assert(await verifyPassword({ hash, password }));
		const cookie = (
			await serializeSignedCookie(
				"__Secure-better-auth.session_token",
				"reset-synthetic-session-active",
				key,
				{ path: "/", httpOnly: true, secure: true, sameSite: "lax" },
			)
		).split(";")[0];
		sensitive.push(cookie);
		assert.equal(
			await auth.api.getSession({ headers: new Headers({ cookie }) }),
			null,
		);
		note("native-source-issued-reset-source-hash-and-session-readers");
	} finally {
		db.close();
	}
	const reverse = fresh("source-complete", "delete", Date.now()),
		sdb = new Database(reverse.control),
		source = createBetterAuthRuntime(sdb, opts);
	try {
		const response = await source.handler(
			new Request("https://example.invalid/api/auth/reset-password", {
				method: "POST",
				headers: {
					origin: "https://example.invalid",
					"content-type": "application/json",
				},
				body: JSON.stringify({ token, newPassword: password }),
			}),
		);
		assert.equal(response.status, 200);
		assert.equal((await response.json()).status, true);
		assert.equal(
			q(reverse, "SELECT * FROM session WHERE userId='u'").length,
			0,
		);
		assert.equal(
			q(reverse, "SELECT * FROM verification WHERE id='r'").length,
			0,
		);
		assert.equal(
			q(reverse, "SELECT * FROM verification WHERE id='rs'").length,
			1,
		);
		refused("actual-source-consumed-token-refuses-native", reverse, {
			now: Date.now(),
		});
		assert.equal(
			run(reverse, { mode: "begin", password, now: Date.now() }).pending,
			true,
		);
		note("source-reset-observed-by-native-credential-login");
	} finally {
		sdb.close();
	}
	return sensitive;
}
