/** Native committed verification mail, trusted redirects and current optional caller. */
import assert from "node:assert/strict";
import { Database } from "bun:sqlite";
export async function verifyVerificationRequest(c: any) {
	const { fixture, run, start, done, marker, sql, image, record, now } = c;
	const sensitive: string[] = [];
	const note = (id: string) => record(`verification-request-${id}`);
	const fresh = (id: string, mode = "delete") =>
		fixture(
			`verification-request-${id}`,
			mode,
			"UPDATE user SET emailVerified=0 WHERE id='u'",
		);
	const act = (f: any, extra: any = {}) =>
		run(f, {
			mode: "verification-request",
			email: "u@example.invalid",
			redirect: "/login",
			...extra,
		});
	const complete = (f: any, token: string, extra: any = {}) =>
		run(f, {
			mode: "verification-callback",
			token,
			redirect: "/login",
			...extra,
		});
	function q(f: any, s: string) {
		const db = new Database(f.control);
		try {
			return db.query(s).all() as any[];
		} finally {
			db.close();
		}
	}
	function remember(m: any) {
		sensitive.push(m.recipient, m.token, m.url);
		const u = new URL(m.url);
		assert.equal(u.pathname, "/api/auth/verify-email");
		assert.equal(u.searchParams.get("token"), m.token);
		return m.token;
	}
	function issue(f: any, extra: any = {}) {
		const r = act(f, extra);
		assert.equal(r.ok, true);
		assert.equal(r.sent.length, 1);
		remember(r.sent[0]);
		return r.sent[0];
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
			m = issue(f, {
				email: "U@EXAMPLE.INVALID",
				redirect: "https://example.invalid/login?x=a%20b#fragment",
			});
		assert.equal(m.recipient, "u@example.invalid");
		assert.equal(
			new URL(m.url).searchParams.get("callbackURL"),
			"https://example.invalid/login?x=a%20b#fragment",
		);
		const rows = q(
			f,
			"SELECT * FROM verification WHERE identifier LIKE 'rm-native-email-v1:%'",
		);
		assert.equal(rows.length, 1);
		assert(!JSON.stringify(rows).includes(m.token));
		note(`${mode}-committed-digest-before-unlocked-mail`);
		const r = complete(f, m.token, {
			redirect:
				"https://example.invalid/login?token=old&error=old&verified=true&x=a%20b#fragment",
		});
		assert.deepEqual(r, {
			ok: true,
			url: "https://example.invalid/login?x=a+b#fragment",
			sent: [],
		});
		assert.equal(
			q(f, "SELECT emailVerified FROM user WHERE id='u'")[0].emailVerified,
			1,
		);
		assert.equal(q(f, "SELECT * FROM session").length, 0);
		note(`${mode}-known-completion-no-session-no-injected-bearer`);
		const before = image(f),
			replay = complete(f, m.token);
		assert.equal(replay.ok, false);
		assert.equal(replay.stage, "NotCommitted");
		assert.equal(replay.url, undefined);
		assert.equal(image(f), before);
		note(`${mode}-replay-no-success-destination`);
	}
	for (const [id, extra] of [
		["missing", { email: "missing@example.invalid" }],
		["verified", {}],
	] as any[]) {
		const f = fresh(id);
		if (id === "verified")
			sql(f, "UPDATE user SET emailVerified=1 WHERE id='u'");
		const before = image(f);
		assert.deepEqual(act(f, extra), { ok: true, sent: [] });
		assert.equal(image(f), before);
		note(`${id}-same-unit-ack-no-mail-or-intent`);
	}
	{
		const f = fresh("default"),
			m = issue(f, { redirect: undefined });
		assert.equal(new URL(m.url).searchParams.get("callbackURL"), "/");
		assert.deepEqual(complete(f, m.token, { redirect: undefined }), {
			ok: true,
			url: null,
			sent: [],
		});
		note("default-message-root-and-no-callback-JSON-continuation");
	}
	for (const value of [
		"//evil.invalid/login",
		"https://example.invalid.evil/login",
		"https://u@example.invalid/login",
		"javascript:evil",
		"/login\n",
		"/%2Fattack",
	]) {
		const f = fresh(`redirect-${value.length}`);
		refused(
			`redirect-refused-${value.length}`,
			f,
			{ redirect: value },
			"invalid_redirect",
		);
	}
	{
		const f = fresh("oversize-precommit"),
			email = `${"a".repeat(3900)}@example.invalid`;
		const db = new Database(f.control);
		db.query("UPDATE user SET email=? WHERE id='u'").run(email);
		db.close();
		refused(
			"oversize-message-prepared-before-commit",
			f,
			{ email, redirect: `/login?x=${"%".repeat(4000)}` },
			"invalid_redirect",
		);
	}
	for (const id of ["disabled-known", "disabled-unknown"]) {
		const f = fresh(id);
		refused(
			id,
			f,
			{
				disabled: true,
				...(id.endsWith("unknown") ? { email: "missing@example.invalid" } : {}),
			},
			"auth_delivery_disabled",
		);
	}
	{
		const f = fresh("delivery-error"),
			r = act(f, { deliveryFail: true });
		assert.equal(r.ok, false);
		assert.equal(r.stage, "Committed");
		assert.equal(r.code, "auth_delivery_unconfirmed");
		assert.equal(r.sent.length, 1);
		const token = remember(r.sent[0]);
		assert.equal(
			q(
				f,
				"SELECT * FROM verification WHERE identifier LIKE 'rm-native-email-v1:%'",
			).length,
			1,
		);
		assert.deepEqual(complete(f, token), {
			ok: true,
			url: "https://example.invalid/login",
			sent: [],
		});
		note("uncertain-delivery-retains-one-committed-link-no-automatic-reissue");
	}
	for (const mode of ["wal", "delete"]) {
		const f = fresh(`${mode}-sql-fail`, mode);
		assert.equal(
			refused(
				`${mode}-late-SQL-no-mail`,
				f,
				{ failSql: true },
				"invalid_control",
			).reusable,
			true,
		);
	}
	function session(f: any, subject = "u", expires = now + 43200000) {
		const d = new Database(f.control);
		d.query(
			"INSERT INTO session(id,token,userId,createdAt,updatedAt,expiresAt) VALUES('caller-session','verification-private-session',?,?,?,?)",
		).run(
			subject,
			new Date(now).toISOString(),
			new Date(now).toISOString(),
			new Date(expires).toISOString(),
		);
		d.close();
	}
	{
		const f = fresh("caller-known");
		session(f);
		const before = q(f, "SELECT * FROM session"),
			m = issue(f, { callerToken: "verification-private-session" });
		assert.equal(m.recipient, "u@example.invalid");
		assert.deepEqual(q(f, "SELECT * FROM session"), before);
		note(
			"current-unverified-caller-can-resend-own-address-without-session-refresh",
		);
	}
	for (const id of [
		"caller-other-address",
		"caller-expired",
		"caller-revoked",
		"caller-other-workspace",
	]) {
		const f = fresh(id);
		session(f, "u", id === "caller-expired" ? now : now + 43200000);
		if (id === "caller-revoked")
			sql(f, "DELETE FROM session WHERE id='caller-session'");
		refused(id, f, {
			callerToken: "verification-private-session",
			...(id === "caller-other-address"
				? { email: "other@example.invalid" }
				: {}),
			...(id === "caller-other-workspace" ? { callerWorkspace: "other" } : {}),
		});
	}
	for (const preflight of [
		"BEGIN",
		"PRAGMA query_only=ON",
		"CREATE TEMP TABLE hidden(x)",
	]) {
		const f = fresh(`preflight-${preflight.length}`);
		assert.equal(
			refused(
				`unready-disabled-bad-input-${preflight.length}`,
				f,
				{ preflight, disabled: true, email: "bad" },
				"connection_refused",
			).reusable,
			false,
		);
	}
	{
		const f = fresh("invalid-callback-destination"),
			m = issue(f),
			before = image(f);
		const r = complete(f, m.token, { redirect: "https://evil.invalid/login" });
		assert.equal(r.ok, false);
		assert.equal(r.code, "invalid_redirect");
		assert.equal(r.url, undefined);
		assert.equal(image(f), before);
		assert.equal(complete(f, m.token).ok, true);
		note("destination-validation-precedes-email-flag-mutation");
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
		const f = fresh(`${mode}-caller-race`, mode);
		session(f);
		const p = start(
			f,
			{
				mode: "verification-request",
				email: "u@example.invalid",
				redirect: "/login",
				callerToken: "verification-private-session",
			},
			"email-unlocked",
		);
		try {
			await marker(p, f);
			sql(f, "DELETE FROM session WHERE id='caller-session'");
			const before = image(f),
				r = await finish(p);
			assert.equal(r.code, "invalid_credentials");
			assert.deepEqual(r.sent, []);
			assert.equal(image(f), before);
			note(`${mode}-current-caller-rechecked-after-writer`);
		} finally {
			await cleanup(p);
		}
		const sent = fresh(`${mode}-aftercommit`, mode),
			racer = start(
				sent,
				{
					mode: "verification-request",
					email: "u@example.invalid",
					redirect: "/login",
				},
				"verification-request-committed",
			);
		try {
			await marker(racer, sent);
			sql(
				sent,
				"UPDATE account SET password='changed-after-commit' WHERE id='a'",
			);
			const r = await finish(racer);
			assert.equal(r.ok, true);
			assert.equal(r.sent.length, 1);
			const token = remember(r.sent[0]);
			assert.equal(complete(sent, token).ok, false);
			note(`${mode}-postcommit-stale-link-cannot-verify-current-user`);
		} finally {
			await cleanup(racer);
		}
	}
	for (const operation of ["verification-request", "verification-callback"]) {
		const f = fresh(`commit-${operation}`),
			token =
				operation === "verification-callback" ? issue(f).token : undefined,
			before = image(f),
			p = start(
				f,
				{
					mode: operation,
					email: "u@example.invalid",
					token,
					redirect: "/login",
				},
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
			assert.equal(r.url, undefined);
			reader.exec("ROLLBACK");
			assert.equal(image(f), before);
			note(`${operation}-unknown-commit-no-mail-or-success-redirect`);
		} finally {
			reader.close();
			await cleanup(p);
		}
	}
	return sensitive;
}
