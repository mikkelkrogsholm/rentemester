/** Persisted native email intents: current facts, atomic consumption, rollback and writers. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { Database } from "bun:sqlite";
export async function verifyEmailVerification(c: any) {
	const { fixture, run, start, done, marker, sql, image, record, now } = c;
	const sensitive: string[] = [];
	const note = (id: string) => record(`email-intent-${id}`);
	const fresh = (id: string, mode = "delete") =>
		fixture(
			`email-${id}`,
			mode,
			"UPDATE user SET emailVerified=0 WHERE id='u'; INSERT INTO session(id,token,userId,createdAt,updatedAt,expiresAt) VALUES('existing-email-session','private-email-session-value','u','2026-01-01','2026-01-01','2026-01-02')",
		);
	function q(f: any, query: string) {
		const db = new Database(f.control);
		try {
			return db.query(query).all() as any[];
		} finally {
			db.close();
		}
	}
	const act = (f: any, extra: any = {}) =>
		run(f, { mode: "email-issue", email: "u@example.invalid", ...extra });
	const complete = (f: any, token: string, extra: any = {}) =>
		run(f, { mode: "email-complete", token, ...extra });
	function issue(f: any, extra: any = {}) {
		const r = act(f, extra);
		assert.equal(r.ok, true);
		assert.equal(typeof r.token, "string");
		sensitive.push(r.token);
		return r.token;
	}
	function refused(
		id: string,
		f: any,
		token: string,
		extra: any = {},
		code = "invalid_credentials",
	) {
		const before = image(f),
			r = complete(f, token, extra);
		assert.equal(r.ok, false, id);
		assert.equal(r.code, code, id);
		assert.equal(r.stage, "NotCommitted", id);
		assert.equal(image(f), before, id);
		assert.equal(r.cookie, undefined);
		note(id);
		return r;
	}
	function protectedState(f: any) {
		return JSON.stringify(
			[
				"account",
				"twoFactor",
				"session",
				"rm_workspace_user_access_events",
			].map((t) => q(f, `SELECT * FROM ${t} ORDER BY rowid`)),
		);
	}
	for (const mode of ["wal", "delete"]) {
		const f = fresh(`${mode}-success`, mode),
			before = protectedState(f),
			token = issue(f),
			sibling = issue(f);
		const rows = q(
			f,
			"SELECT * FROM verification WHERE identifier LIKE 'rm-native-email-v1:%'",
		);
		assert.equal(rows.length, 2);
		const digest = createHash("sha256").update(token).digest("hex");
		assert.equal(rows[0].identifier, `rm-native-email-v1:${digest}`);
		assert(!JSON.stringify(rows).includes(token));
		assert.equal(rows[0].createdAt, new Date(now).toISOString());
		assert.equal(rows[0].expiresAt, new Date(now + 3600000).toISOString());
		assert.equal(protectedState(f), before);
		note(`${mode}-digest-only-persisted-intent`);
		assert.deepEqual(complete(f, token), { ok: true });
		assert.equal(
			q(f, "SELECT emailVerified FROM user WHERE id='u'")[0].emailVerified,
			1,
		);
		assert.equal(protectedState(f), before);
		assert.equal(
			q(
				f,
				"SELECT * FROM rm_workspace_auth_state_events WHERE state_transition='email_verified'",
			).length,
			1,
		);
		assert.equal(
			q(
				f,
				"SELECT * FROM workspace_audit WHERE event_type='email_verification_completed'",
			).length,
			1,
		);
		assert.equal(
			q(
				f,
				"SELECT * FROM rm_workspace_auth_telemetry_events WHERE endpoint='verify-email' AND outcome='accepted'",
			).length,
			1,
		);
		note(`${mode}-flag-audit-consume-only-no-session-role-or-MFA-effects`);
		refused(`${mode}-replay`, f, token);
		refused(`${mode}-sibling-after-success`, f, sibling);
	}
	for (const [id, extra] of [
		["unknown", { email: "missing@example.invalid" }],
		["already-verified", {}],
	] as any[]) {
		const f = fresh(id);
		if (id === "already-verified")
			sql(f, "UPDATE user SET emailVerified=1 WHERE id='u'");
		const before = image(f);
		assert.deepEqual(act(f, extra), { ok: true, token: null });
		assert.equal(image(f), before);
		note(`${id}-no-intent`);
	}
	for (const [id, mutation] of [
		["password-change", "UPDATE account SET password='different' WHERE id='a'"],
		[
			"credential-change-and-restore",
			"CREATE TEMP TABLE saved AS SELECT password FROM account WHERE id='a'; UPDATE account SET password='different' WHERE id='a'; UPDATE account SET password=(SELECT password FROM saved) WHERE id='a'",
		],
		["credential-deletion", "DELETE FROM account WHERE id='a'"],
		[
			"credential-coordinate",
			"UPDATE account SET accountId='other' WHERE id='a'",
		],
		["user-updated", "UPDATE user SET updatedAt='2026-01-02' WHERE id='u'"],
		["mfa-change", "UPDATE user SET twoFactorEnabled=0 WHERE id='u'"],
		[
			"email-recycled",
			"UPDATE user SET email='old@example.invalid' WHERE id='u'; INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES('other','Synthetic other','u@example.invalid',0,1,'2026-01-01','2026-01-01')",
		],
		[
			"lifecycle",
			"INSERT INTO rm_workspace_auth_state_events(state_transition,user_id) VALUES('session_deleted','u')",
		],
	]) {
		const f = fresh(id),
			token = issue(f),
			beforeAccount = JSON.stringify(q(f, "SELECT * FROM account ORDER BY id"));
		sql(f, mutation);
		if (id === "credential-change-and-restore")
			assert.equal(
				JSON.stringify(q(f, "SELECT * FROM account ORDER BY id")),
				beforeAccount,
			);
		refused(id, f, token);
	}
	{
		const f = fresh("null-password");
		sql(f, "UPDATE account SET password=NULL WHERE id='a'");
		const token = issue(f);
		sql(
			f,
			"CREATE TEMP TABLE copy AS SELECT * FROM account WHERE id='a'; DELETE FROM account WHERE id='a'; UPDATE copy SET password=''; INSERT INTO account SELECT * FROM copy",
		);
		refused("null-to-empty-replacement", f, token);
	}
	for (const [id, mutation] of [
		[
			"duplicate",
			"INSERT INTO verification SELECT 'dup',identifier,value,createdAt,updatedAt,expiresAt FROM verification WHERE identifier LIKE 'rm-native-email-v1:%'",
		],
		[
			"corrupt-binding",
			"UPDATE verification SET value='{}' WHERE identifier LIKE 'rm-native-email-v1:%'",
		],
		[
			"corrupt-expiry",
			"UPDATE verification SET expiresAt='2027-01-01T00:00:00.000Z' WHERE identifier LIKE 'rm-native-email-v1:%'",
		],
	]) {
		const f = fresh(id),
			token = issue(f);
		sql(f, mutation);
		refused(id, f, token, {}, "invalid_control");
	}
	for (const [id, extra] of [
		["expiry", { now: now + 3600000 }],
		["future", { now: now - 1 }],
		["workspace", { workspace: "other" }],
		["key", { key: "YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXoxMjM0NTY" }],
	] as any[]) {
		const f = fresh(id),
			token = issue(f);
		refused(id, f, token, extra);
	}
	{
		const f = fresh("last-ms"),
			token = issue(f);
		assert.deepEqual(complete(f, token, { now: now + 3599999 }), { ok: true });
		note("last-valid-millisecond");
		const inactive = fresh("no-access");
		sql(
			inactive,
			"INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')",
		);
		const inactiveToken = issue(inactive),
			before = protectedState(inactive);
		assert.deepEqual(complete(inactive, inactiveToken), { ok: true });
		assert.equal(protectedState(inactive), before);
		note("address-verification-independent-of-membership-no-access-grant");
	}
	for (const mode of ["wal", "delete"]) {
		const f = fresh(`${mode}-rollback`, mode),
			token = issue(f);
		assert.equal(
			refused(
				`${mode}-late-SQL-rollback`,
				f,
				token,
				{ failSql: true },
				"invalid_control",
			).reusable,
			true,
		);
		assert.deepEqual(complete(f, token), { ok: true });
		note(`${mode}-rollback-token-still-usable`);
		const issuance = fresh(`${mode}-issuance-rollback`, mode),
			before = image(issuance);
		const r = act(issuance, { failSql: true });
		assert.equal(r.ok, false);
		assert.equal(r.token, undefined);
		assert.equal(image(issuance), before);
		note(`${mode}-issuance-no-release-before-commit`);
	}
	for (const preflight of [
		"BEGIN",
		"PRAGMA query_only=ON",
		"CREATE TEMP TABLE hidden(x)",
	]) {
		const f = fresh(`unready-${preflight.length}`),
			before = image(f);
		const r = act(f, { email: "bad", preflight });
		assert.equal(r.code, "connection_refused");
		assert.equal(r.reusable, false);
		assert.equal(image(f), before);
		note(`malformed-input-unready-${preflight.length}`);
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
		const f = fresh(`${mode}-writer`, mode),
			token = issue(f),
			p = start(f, { mode: "email-complete", token }, "before-commit");
		try {
			await marker(p, f);
			assert.equal(complete(f, token).code, "control_busy_or_unavailable");
			assert.deepEqual(await finish(p), { ok: true });
			refused(`${mode}-serialized-replay`, f, token);
			note(`${mode}-single-writer-consumption`);
		} finally {
			await cleanup(p);
		}
		const stale = fresh(`${mode}-current`, mode),
			staleToken = issue(stale),
			racer = start(
				stale,
				{ mode: "email-complete", token: staleToken },
				"email-unlocked",
			);
		try {
			await marker(racer, stale);
			sql(stale, "UPDATE account SET password='changed' WHERE id='a'");
			const before = image(stale);
			assert.equal((await finish(racer)).code, "invalid_credentials");
			assert.equal(image(stale), before);
			note(`${mode}-fresh-facts-after-writer`);
		} finally {
			await cleanup(racer);
		}
	}
	for (const operation of ["email-issue", "email-complete"]) {
		const f = fresh(`commit-${operation}`),
			token = operation === "email-complete" ? issue(f) : undefined,
			before = image(f),
			p = start(
				f,
				{ mode: operation, token, email: "u@example.invalid" },
				"before-commit",
			),
			reader = new Database(f.control);
		try {
			await marker(p, f);
			reader.exec("BEGIN;SELECT count(*) FROM user");
			const r = await finish(p);
			assert.equal(r.stage, "OutcomeUnknown");
			assert.equal(r.reusable, false);
			assert.equal(r.token, undefined);
			reader.exec("ROLLBACK");
			assert.equal(image(f), before);
			note(`${operation}-commit-busy-no-bearer-release-retired-handle`);
		} finally {
			reader.close();
			await cleanup(p);
		}
	}
	return sensitive;
}
