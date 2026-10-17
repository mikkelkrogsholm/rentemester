/** Native issuance, secret mail handoff and non-consuming redirect preview. */
import assert from "node:assert/strict";
import { Database } from "bun:sqlite";
import { createHmac } from "node:crypto";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { createBetterAuthRuntime } from "../../src/server/better-auth";
export async function verifyResetRequest(c: any) {
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
	const sensitive: string[] = [];
	const note = (id: string) => record(`request-${id}`);
	function q(f: any, s: string) {
		const db = new Database(f.control);
		try {
			return db.query(s).all() as any[];
		} finally {
			db.close();
		}
	}
	const fresh = (id: string, mode = "delete") => fixture(`request-${id}`, mode);
	const act = (f: any, extra: any = {}) =>
		run(f, {
			mode: "reset-request",
			email: "u@example.invalid",
			redirect: "/reset",
			...extra,
		});
	const preview = (f: any, token: string, extra: any = {}) =>
		run(f, { mode: "reset-callback", token, redirect: "/reset", ...extra });
	const rows = (f: any) =>
		q(
			f,
			"SELECT * FROM verification WHERE identifier LIKE 'reset-password:%' ORDER BY rowid",
		);
	const protectedState = (f: any) =>
		JSON.stringify(
			[
				"user",
				"account",
				"twoFactor",
				"rm_workspace_user_access_events",
				"rm_workspace_auth_state_events",
				"session",
			].map((t) => q(f, `SELECT * FROM ${t} ORDER BY rowid`)),
		);
	function remember(message: any) {
		assert.match(message.token, /^[A-Za-z0-9]{24}$/);
		sensitive.push(message.token, message.url);
	}
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
		assert.deepEqual(r.sent, []);
		assert.equal(image(f), before, id);
		note(id);
		return r;
	}
	for (const mode of ["wal", "delete"]) {
		const f = fresh(`${mode}-known`, mode),
			before = protectedState(f),
			r = act(f, { email: "U@EXAMPLE.INVALID" });
		assert.equal(r.ok, true);
		assert.equal(r.sent.length, 1);
		const m = r.sent[0];
		remember(m);
		assert.equal(m.recipient, "u@example.invalid");
		assert.equal(
			m.url,
			`https://example.invalid/api/auth/reset-password/${m.token}?callbackURL=%2Freset`,
		);
		const v = rows(f);
		assert.equal(v.length, 1);
		assert.equal(v[0].identifier, `reset-password:${m.token}`);
		assert.equal(v[0].value, "u");
		assert.equal(v[0].createdAt, new Date(now).toISOString());
		assert.equal(v[0].updatedAt, v[0].createdAt);
		assert.equal(v[0].expiresAt, new Date(now + 3600000).toISOString());
		assert.equal(protectedState(f), before);
		note(`${mode}-source-compatible-issuance-and-unlocked-mail`);
		const telemetry = q(
			f,
			"SELECT * FROM rm_workspace_auth_telemetry_events WHERE endpoint='request-password-reset'",
		);
		assert.equal(telemetry.length, 1);
		assert.equal(telemetry[0].outcome, "attempted");
		const hash = createHmac("sha256", key)
			.update("rentemester-auth-audit-v1\0u@example.invalid")
			.digest("hex");
		assert.equal(telemetry[0].identity_hash, hash);
		assert.equal(telemetry[0].user_id, "u");
		assert.equal(telemetry[0].session_id, null);
		const audit = q(
			f,
			"SELECT * FROM workspace_audit WHERE entity_type='native_reset_request_v1'",
		);
		assert.equal(audit.length, 1);
		assert.deepEqual(JSON.parse(audit[0].entity_id), [hash, 1, "u"]);
		assert.equal(audit[0].actor, "system:auth-recovery");
		assert.equal(audit[0].created_at, new Date(now).toISOString());
		note(`${mode}-audit-contains-HMAC-no-email-or-bearer`);
		const snap = image(f),
			p = preview(f, m.token);
		assert.equal(p.ok, true);
		assert.deepEqual(p.sent, []);
		assert.equal(p.url, `https://example.invalid/reset?token=${m.token}`);
		sensitive.push(p.url);
		assert.equal(image(f), snap);
		note(`${mode}-callback-readonly-does-not-consume`);
		const again = act(f);
		assert.equal(again.sent.length, 1);
		remember(again.sent[0]);
		assert.notEqual(again.sent[0].token, m.token);
		assert.equal(rows(f).length, 2);
		assert.equal(preview(f, m.token).ok, true);
		note(`${mode}-requests-retain-older-links`);
		const complete = run(f, {
			mode: "password-reset",
			token: m.token,
			newPassword: "Request replacement password 789!",
		});
		assert.equal(complete.ok, true);
		assert.equal(rows(f).length, 0);
		assert.equal(
			preview(f, m.token).url,
			"https://example.invalid/reset?error=INVALID_TOKEN",
		);
		note(`${mode}-native-request-to-completion-and-sibling-invalidation`);
	}
	for (const mode of ["wal", "delete"]) {
		const f = fresh(`${mode}-unknown`, mode),
			before = protectedState(f);
		sql(
			f,
			"INSERT INTO verification(id,identifier,value,createdAt,updatedAt,expiresAt) VALUES('unrelated','expired-unrelated','u','2020-01-01','2020-01-01','2020-01-02')",
		);
		const r = act(f, { email: "absent@example.invalid" });
		assert.deepEqual(r, { ok: true, sent: [] });
		assert.equal(rows(f).length, 0);
		assert.equal(protectedState(f), before);
		assert.equal(
			q(f, "SELECT * FROM verification WHERE id='unrelated'").length,
			1,
		);
		assert.equal(
			q(
				f,
				"SELECT user_id FROM rm_workspace_auth_telemetry_events WHERE endpoint='request-password-reset'",
			)[0].user_id,
			null,
		);
		note(`${mode}-unknown-email-unit-no-mail-no-global-cleanup`);
	}
	for (const [id, change] of [
		["no-local-credential", "DELETE FROM account"],
		[
			"unverified-disabled-MFA-off",
			"UPDATE user SET emailVerified=0,twoFactorEnabled=0 WHERE id='u';DELETE FROM twoFactor;INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')",
		],
	]) {
		const f = fresh(id);
		sql(f, change);
		const before = protectedState(f),
			r = act(f);
		assert.equal(r.ok, true);
		assert.equal(r.sent.length, 1);
		remember(r.sent[0]);
		assert.equal(protectedState(f), before);
		note(`${id}-request-does-not-grant-authority`);
	}
	const failure = fresh("delivery-unconfirmed"),
		r = act(failure, { deliveryFail: true });
	assert.equal(r.ok, false);
	assert.equal(r.stage, "Committed");
	assert.equal(r.code, "auth_delivery_unconfirmed");
	assert.equal(r.reusable, true);
	assert.equal(r.sent.length, 1);
	remember(r.sent[0]);
	assert.equal(rows(failure).length, 1);
	assert.equal(preview(failure, r.sent[0].token).ok, true);
	assert.equal(
		run(failure, {
			mode: "password-reset",
			token: r.sent[0].token,
			newPassword: "Request replacement password 789!",
		}).ok,
		true,
	);
	note("delivery-error-preserves-committed-usable-link-not-false-rollback");
	for (const email of ["u@example.invalid", "absent@example.invalid"])
		refused(
			`disabled-${email.split("@")[0]}`,
			fresh(`disabled-${email.split("@")[0]}`),
			{ email, disabled: true },
			"auth_delivery_disabled",
		);
	for (const [id, email] of [
		["spaces", " u@example.invalid"],
		["control", "u@example.invalid\n"],
		["unicode", "æ@example.invalid"],
		["malformed", "not-email"],
	])
		refused(`email-${id}`, fresh(`email-${id}`), { email });
	for (const [id, redirect] of [
		["prefix", "https://example.invalid.evil/reset"],
		["userinfo", "https://example.invalid@evil.invalid/reset"],
		["authority", "//evil.invalid/reset"],
		["encoded-slash", "/%2fattack"],
		["upper-encoded-slash", "/%2Fattack"],
		["backslash", "/\\evil"],
		["relative-fragment", "/reset#fragment"],
		["scheme", "javascript:alert(1)"],
		["relative-no-root", "reset"],
		["control", "/reset\n"],
	])
		refused(
			`redirect-${id}`,
			fresh(`redirect-${id}`),
			{ redirect },
			"invalid_redirect",
		);
	for (const [id, extra] of [
		["config-empty-allowlist", { trustedOrigins: [] }],
		["config-untrusted-base", { baseOrigin: "https://other.invalid" }],
		["config-userinfo", { baseOrigin: "https://user@example.invalid" }],
		[
			"config-insecure",
			{
				baseOrigin: "http://example.invalid",
				trustedOrigins: ["http://example.invalid"],
			},
		],
		["config-path", { baseOrigin: "https://example.invalid/path" }],
		["audit-overflow", { auditVersion: 9007199254740992 }],
	] as any[])
		refused(id, fresh(id), extra, "invalid_auth_config");
	for (const [id, preflight] of [
		["query-only", "PRAGMA query_only=ON"],
		["attached", "ATTACH DATABASE ':memory:' AS other"],
		["active-transaction", "BEGIN"],
	]) {
		const r = refused(id, fresh(id), { preflight }, "connection_refused");
		assert.equal(r.reusable, false);
	}
	for (const [id, time] of [
		["negative", -1],
		["too-late", 253402300799999 - 3599999],
	] as any[])
		refused(
			`clock-${id}`,
			fresh(`clock-${id}`),
			{ now: time },
			"invalid_auth_clock",
		);
	for (const mode of ["wal", "delete"])
		for (const email of ["u@example.invalid", "absent@example.invalid"]) {
			const id = `rollback-${mode}-${email.split("@")[0]}`,
				f = fresh(id, mode);
			const r = refused(id, f, { email, failSql: true }, "invalid_control");
			assert.equal(r.reusable, true);
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
		const f = fresh(`commit-before-mail-${mode}`, mode),
			mail = join(f.dir, "mail"),
			p = start(
				f,
				{
					mode: "reset-request",
					email: "u@example.invalid",
					redirect: "/reset",
					mailMarker: mail,
				},
				"request-committed",
			);
		try {
			await marker(p, f);
			assert.equal(rows(f).length, 1);
			assert.equal(existsSync(mail), false);
			sql(f, "BEGIN IMMEDIATE;COMMIT");
			const response = await finish(p);
			assert.equal(response.ok, true);
			assert.equal(existsSync(mail), true);
			remember(response.sent[0]);
			note(`${mode}-known-commit-visible-before-mail-without-lock`);
		} finally {
			await cleanup(p);
		}
		const g = fresh(`writer-contention-${mode}`, mode),
			held = start(
				g,
				{
					mode: "reset-request",
					email: "u@example.invalid",
					redirect: "/reset",
				},
				"before-commit",
			);
		try {
			await marker(held, g);
			const denied = act(g);
			assert.equal(denied.code, "control_busy_or_unavailable");
			assert.equal(denied.reusable, false);
			assert.deepEqual(denied.sent, []);
			const response = await finish(held);
			assert.equal(response.ok, true);
			remember(response.sent[0]);
			assert.equal(rows(g).length, 1);
			note(`${mode}-writer-contention-never-sends-before-commit`);
		} finally {
			await cleanup(held);
		}
	}
	for (const email of ["u@example.invalid", "absent@example.invalid"]) {
		const f = fresh(`commit-busy-${email.split("@")[0]}`),
			before = image(f),
			mail = join(f.dir, "mail"),
			p = start(
				f,
				{ mode: "reset-request", email, redirect: "/reset", mailMarker: mail },
				"before-commit",
			),
			reader = new Database(f.control);
		try {
			await marker(p, f);
			reader.exec("BEGIN;SELECT count(*) FROM user");
			const r = await finish(p);
			assert.equal(r.stage, "OutcomeUnknown");
			assert.equal(r.reusable, false);
			assert.deepEqual(r.sent, []);
			assert.equal(existsSync(mail), false);
			reader.exec("ROLLBACK");
			assert.equal(image(f), before);
			note(`commit-busy-${email.split("@")[0]}-withholds-mail`);
		} finally {
			reader.close();
			await cleanup(p);
		}
	}
	const callbacks = fresh("callback-shapes"),
		issued = act(callbacks, { redirect: null });
	assert.equal(issued.ok, true);
	const m = issued.sent[0];
	remember(m);
	assert(m.url.endsWith("?callbackURL="));
	note("omitted-redirect-source-compatible-mail-link");
	const token = m.token;
	for (const [id, extra, expected] of [
		[
			"query-position",
			{
				redirect:
					"https://example.invalid/reset?token=old&x=a%20b&token=old2#fragment",
			},
			`https://example.invalid/reset?token=${token}&x=a+b#fragment`,
		],
		[
			"omitted",
			{ redirect: null },
			"https://example.invalid/api/auth/error?error=INVALID_TOKEN",
		],
		[
			"empty",
			{ redirect: "" },
			"https://example.invalid/api/auth/error?error=INVALID_TOKEN",
		],
		[
			"expired",
			{ now: now + 3600000 },
			"https://example.invalid/reset?error=INVALID_TOKEN",
		],
		[
			"bad-token",
			{ token: "bad" },
			"https://example.invalid/reset?error=INVALID_TOKEN",
		],
	] as any[]) {
		const before = image(callbacks),
			r = preview(callbacks, token, extra);
		assert.equal(r.ok, true);
		assert.equal(r.url, expected);
		assert.equal(image(callbacks), before);
		if (r.url.includes(token)) sensitive.push(r.url);
		note(`callback-${id}-readonly`);
	}
	sql(
		callbacks,
		"INSERT INTO verification SELECT 'duplicate',identifier,value,expiresAt,createdAt,updatedAt FROM verification WHERE identifier LIKE 'reset-password:%'",
	);
	const before = image(callbacks),
		duplicate = preview(callbacks, token);
	assert.equal(duplicate.code, "invalid_control");
	assert.equal(image(callbacks), before);
	note("callback-duplicate-refuses-same-current-rule-as-completion");
	const dev = fresh("development"),
		d = act(dev, {
			development: true,
			baseOrigin: "http://127.0.0.1:3000",
			trustedOrigins: ["http://127.0.0.1:3000"],
		});
	assert.equal(d.ok, true);
	remember(d.sent[0]);
	assert(
		d.sent[0].url.startsWith("http://127.0.0.1:3000/api/auth/reset-password/"),
	);
	note("loopback-development-injected-origin");
	// Source HTTP callback consumes native links without consuming their record. Native
	// callback and source callback agree on source-compatible query ordering/encoding.
	const interop = fresh("source-interop"),
		i = act(interop, {
			now: Date.now(),
			redirect: "https://example.invalid/reset?token=old&x=a%20b#fragment",
		});
	assert.equal(i.ok, true);
	remember(i.sent[0]);
	const db = new Database(interop.control),
		auth = createBetterAuthRuntime(db, opts);
	try {
		const before = q(interop, "SELECT * FROM verification ORDER BY id");
		const response = await auth.handler(new Request(i.sent[0].url));
		assert.equal(response.status, 302);
		const native = preview(interop, i.sent[0].token, {
			now: Date.now(),
			redirect: "https://example.invalid/reset?token=old&x=a%20b#fragment",
		});
		assert.equal(native.ok, true);
		assert.equal(response.headers.get("location"), native.url);
		sensitive.push(native.url);
		assert.deepEqual(
			q(interop, "SELECT * FROM verification ORDER BY id"),
			before,
		);
		note("actual-source-callback-agrees-with-native-nonconsuming-preview");
		const complete = await auth.handler(
			new Request("https://example.invalid/api/auth/reset-password", {
				method: "POST",
				headers: {
					origin: "https://example.invalid",
					"content-type": "application/json",
				},
				body: JSON.stringify({
					token: i.sent[0].token,
					newPassword: "Request replacement password 789!",
				}),
			}),
		);
		assert.equal(complete.status, 200);
		assert.equal((await complete.json()).status, true);
		assert.equal(
			preview(interop, i.sent[0].token, { now: Date.now() }).url,
			"https://example.invalid/reset?error=INVALID_TOKEN",
		);
		note("native-issued-link-completes-in-actual-source");
	} finally {
		db.close();
	}
	// Source request exposes the same normal acknowledgement and link format while
	// source framework swallows sender failure; native records committed uncertainty.
	const sourceFixture = fresh("source-request"),
		sdb = new Database(sourceFixture.control);
	let sent: any;
	const source = createBetterAuthRuntime(sdb, {
		...opts,
		emailSender: {
			async send(message: any) {
				sent = message;
				throw new Error("synthetic private delivery failure");
			},
		},
	});
	try {
		const response = await source.handler(
			new Request("https://example.invalid/api/auth/request-password-reset", {
				method: "POST",
				headers: {
					origin: "https://example.invalid",
					"content-type": "application/json",
				},
				body: JSON.stringify({
					email: "u@example.invalid",
					redirectTo: "https://example.invalid/reset?q=a b#fragment",
				}),
			}),
		);
		assert.equal(response.status, 200);
		assert.equal((await response.json()).status, true);
		remember(sent);
		assert.equal(
			sent.url,
			`https://example.invalid/api/auth/reset-password/${sent.token}?callbackURL=${encodeURIComponent("https://example.invalid/reset?q=a b#fragment")}`,
		);
		assert.equal(rows(sourceFixture).length, 1);
		const native = act(fresh("source-format"), {
			redirect: "https://example.invalid/reset?q=a b#fragment",
		});
		remember(native.sent[0]);
		assert.equal(
			native.sent[0].url.replace(native.sent[0].token, "TOKEN"),
			sent.url.replace(sent.token, "TOKEN"),
		);
		note("actual-source-link-encoding-and-swallowed-mail-error-correction");
	} finally {
		sdb.close();
	}
	return sensitive;
}
