/** Native own-session lifecycle, bounded nonbearer lists and atomic deletion authority. */
import assert from "node:assert/strict";
import { copyFileSync } from "node:fs";
import { join } from "node:path";
import { Database } from "bun:sqlite";
import { serializeSignedCookie } from "../../node_modules/better-call/dist/cookies.mjs";
import { createBetterAuthRuntime } from "../../src/server/better-auth";
export async function verifySessionControl(c: any) {
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
	const note = (id: string) => record(`sessions-${id}`);
	const q = (f: any, text: string) => {
		const db = new Database(f.control);
		try {
			return db.query(text).all() as any[];
		} finally {
			db.close();
		}
	};
	const signed = async (token: string, signer = key, development = false) =>
		(
			await serializeSignedCookie(
				`${development ? "" : "__Secure-"}better-auth.session_token`,
				token,
				signer,
				{ path: "/", httpOnly: true, secure: !development, sameSite: "lax" },
			)
		).split(";")[0];
	async function fresh(id: string, mode = "delete", time = now) {
		const f = fixture(`sessions-${id}`, mode),
			db = new Database(f.control);
		db.exec(
			"INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES('v','Synthetic other','v@example.invalid',1,1,'2026-01-01','2026-01-01');",
		);
		const insert = db.query(
			"INSERT INTO session(id,token,userId,createdAt,updatedAt,expiresAt,ipAddress,userAgent) VALUES(?,?,?,?,?,?,?,?)",
		);
		for (const [sid, user, created, expires, ip, agent] of [
			["s0", "u", time, time + 43200000, null, null],
			[
				"s2",
				"u",
				time - 1000,
				time + 43199000,
				"192.0.2.2",
				"Synthetic browser",
			],
			["s1", "u", time - 1000, time + 43199000, null, "Synthetic device"],
			["sx", "u", time - 60000, time - 1, null, null],
			["foreign", "v", time, time + 43200000, null, null],
		] as any[]) {
			insert.run(
				sid,
				`synthetic-bearer-${sid}`,
				user,
				new Date(created).toISOString(),
				new Date(created).toISOString(),
				new Date(expires).toISOString(),
				ip,
				agent,
			);
			sensitive.push(`synthetic-bearer-${sid}`);
		}
		db.close();
		const cookie = await signed("synthetic-bearer-s0");
		sensitive.push(cookie);
		return { ...f, cookie };
	}
	const act = (f: any, op = "list", extra: any = {}) =>
		run(f, { mode: `session-${op}`, cookie: f.cookie, ...extra });
	const audits = (f: any) =>
		q(
			f,
			"SELECT * FROM workspace_audit WHERE entity_type='native_session_control_v1' ORDER BY id",
		);
	const deleted = (f: any) =>
		q(
			f,
			"SELECT * FROM rm_workspace_auth_state_events WHERE state_transition='session_deleted'",
		);
	const protectedState = (f: any) =>
		JSON.stringify(
			["user", "account", "twoFactor"].map((t) =>
				q(f, `SELECT * FROM ${t} ORDER BY rowid`),
			),
		);
	function refused(
		id: string,
		f: any,
		op = "list",
		extra: any = {},
		code = "invalid_credentials",
	) {
		const before = image(f),
			r = act(f, op, extra);
		assert.equal(r.ok, false, id);
		assert.equal(r.code, code, id);
		assert.equal(r.stage, "NotCommitted");
		assert.equal(r.sessions, undefined);
		assert.equal(r.clearCookies, undefined);
		assert.equal(image(f), before, id);
		note(id);
		return r;
	}
	const checkClear = (r: any, development = false) => {
		assert.equal(r.clearCookies.length, 4);
		for (const h of r.clearCookies) {
			assert(h.includes("Max-Age=0"));
			assert(h.includes("HttpOnly; SameSite=Lax"));
			assert.equal(h.startsWith("__Secure-"), !development);
			assert.equal(h.endsWith("; Secure"), !development);
			assert(!h.includes("synthetic-bearer"));
		}
	};
	for (const mode of ["wal", "delete"]) {
		const f = await fresh(`happy-${mode}`, mode),
			before = protectedState(f),
			r = act(f);
		assert.equal(r.ok, true);
		assert.deepEqual(
			r.sessions.map((s: any) => s.id),
			["s0", "s1", "s2"],
		);
		assert.deepEqual(
			r.sessions.map((s: any) => s.current),
			[true, false, false],
		);
		assert.equal(r.sessions[0].ipAddress, null);
		assert.equal(r.sessions[2].userAgent, "Synthetic browser");
		assert.equal(r.clearCookies.length, 0);
		assert(!JSON.stringify(r).includes("synthetic-bearer"));
		assert(
			r.sessions.every(
				(s: any) =>
					Object.keys(s).sort().join(",") ===
					"createdAt,current,expiresAt,id,ipAddress,userAgent",
			),
		);
		assert.equal(protectedState(f), before);
		assert.equal(q(f, "SELECT * FROM session").length, 5);
		assert.equal(deleted(f).length, 0);
		let a = audits(f);
		assert.equal(a.length, 1);
		assert.equal(a[0].event_type, "session_listed");
		assert.deepEqual(JSON.parse(a[0].entity_id), ["u", "s0", 1, null, 0]);
		assert.equal(a[0].actor, "user:u");
		assert.equal(a[0].created_at, new Date(now).toISOString());
		note(`${mode}-bounded-own-list-no-bearers-stable-metadata`);
		for (const target of ["foreign", "missing"]) {
			const r = act(f, "one", { target });
			assert.equal(r.ok, true);
			assert.equal(r.deleted, 0);
			assert.equal(r.clearCookies.length, 0);
		}
		assert.equal(q(f, "SELECT * FROM session").length, 5);
		assert.equal(deleted(f).length, 0);
		note(`${mode}-foreign-missing-target-same-audited-noop`);
		const pending = run(f, { mode: "begin" });
		assert.equal(pending.pending, true);
		const revoke = act(f, "one", { target: "s1" });
		assert.equal(revoke.deleted, 1);
		assert.equal(revoke.clearCookies.length, 0);
		assert.equal(deleted(f).length, 1);
		assert.equal(
			act(f, "list", { cookie: await signed("synthetic-bearer-s1") }).code,
			"invalid_credentials",
		);
		const oldPending = run(f, { cookie: pending.cookie, code: "wrong" });
		assert.equal(oldPending.code, "invalid_credentials");
		assert.equal(protectedState(f), before);
		note(`${mode}-single-delete-stale-token-and-pending-MFA-refuse`);
		const others = act(f, "others");
		assert.equal(others.deleted, 2);
		assert.equal(others.clearCookies.length, 0);
		assert.deepEqual(
			q(f, "SELECT id FROM session ORDER BY id").map((s) => s.id),
			["foreign", "s0"],
		);
		assert.equal(deleted(f).length, 3);
		assert.equal(protectedState(f), before);
		note(`${mode}-atomic-all-other-rows-including-expired`);
		const all = act(f, "all");
		assert.equal(all.deleted, 1);
		checkClear(all);
		assert.deepEqual(
			q(f, "SELECT id FROM session").map((s) => s.id),
			["foreign"],
		);
		assert.equal(deleted(f).length, 4);
		assert.equal(act(f).code, "invalid_credentials");
		assert.equal(protectedState(f), before);
		note(`${mode}-revoke-all-removes-current-and-preserves-other-user`);
		const n = audits(f).length,
			again = act(f, "out");
		assert.equal(again.ok, true);
		assert.equal(again.deleted, 0);
		checkClear(again);
		assert.equal(audits(f).length, n);
		note(`${mode}-already-missing-logout-no-fabricated-audit`);
		const self = await fresh(`self-${mode}`, mode);
		const selfRevoked = act(self, "one", { target: "s0" });
		assert.equal(selfRevoked.deleted, 1);
		checkClear(selfRevoked);
		checkClear(act(self, "out"));
		assert.equal(act(self).code, "invalid_credentials");
		note(`${mode}-single-current-revoke-clears-and-rejects-current`);
		const noAccount = await fresh(`no-account-${mode}`, mode);
		sql(noAccount, "DELETE FROM account");
		assert.equal(act(noAccount).ok, true);
		assert.equal(act(noAccount, "others").deleted, 3);
		note(`${mode}-no-local-credential-dependency`);
		for (const op of ["list", "out", "one", "others", "all"]) {
			const late = await fresh(`late-${mode}-${op}`, mode),
				before = image(late),
				r = act(late, op, { target: "s1", failSql: true });
			assert.equal(r.code, "invalid_control");
			assert.equal(r.reusable, true);
			assert.equal(r.clearCookies, undefined);
			assert.equal(r.sessions, undefined);
			assert.equal(image(late), before);
			note(`${mode}-${op}-late-SQL-restores-rows-triggers-audit`);
		}
		const held = await fresh(`held-${mode}`, mode),
			hp = start(
				held,
				{ mode: "session-others", cookie: held.cookie },
				"before-commit",
			);
		try {
			await marker(hp, held);
			assert.equal(
				act(held, "one", { target: "s0" }).code,
				"control_busy_or_unavailable",
			);
			hp.child.stdin.write("continue\n");
			hp.child.stdin.end();
			assert.equal((await done(hp)).deleted, 3);
			assert.equal(act(held).sessions.length, 1);
			note(`${mode}-writer-serializes-revocations-through-commit`);
		} finally {
			clearTimeout(hp.timer);
			hp.child.kill();
			await hp.child.exited;
		}
		const race = await fresh(`MFA-race-${mode}`, mode),
			p = run(race, { mode: "begin" }),
			rp = start(
				race,
				{ mode: "session-one", cookie: race.cookie, target: "s1" },
				"before-commit",
			);
		try {
			await marker(rp, race);
			assert.equal(
				run(race, { cookie: p.cookie, code: "wrong" }).code,
				"control_busy_or_unavailable",
			);
			rp.child.stdin.write("continue\n");
			rp.child.stdin.end();
			assert.equal((await done(rp)).deleted, 1);
			assert.equal(
				run(race, { cookie: p.cookie, code: "wrong" }).code,
				"invalid_credentials",
			);
			assert.equal(q(race, "SELECT * FROM session WHERE id='s1'").length, 0);
			note(`${mode}-pending-MFA-cannot-race-past-revoke-or-resurrect`);
		} finally {
			clearTimeout(rp.timer);
			rp.child.kill();
			await rp.child.exited;
		}
	}
	for (const [id, change] of [
		[
			"expired",
			"UPDATE session SET createdAt='2025-12-31T23:00:00.000Z',expiresAt='2026-01-01T00:00:00.000Z' WHERE id='s0'",
		],
		[
			"malformed-time",
			"UPDATE session SET createdAt='bad',expiresAt='bad' WHERE id='s0'",
		],
		[
			"disabled",
			"INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')",
		],
		[
			"unverified",
			"UPDATE user SET emailVerified=0,twoFactorEnabled=0;DELETE FROM twoFactor",
		],
		[
			"old",
			"UPDATE session SET createdAt='2025-12-31T23:00:00.000Z',expiresAt='2026-01-01T11:00:00.000Z' WHERE id='s0'",
		],
	]) {
		const f = await fresh(`logout-${id}`);
		sql(f, change);
		const r = act(f, "out");
		assert.equal(r.ok, true);
		assert.equal(r.deleted, 1);
		checkClear(r);
		assert.equal(q(f, "SELECT * FROM session WHERE id='s0'").length, 0);
		assert.equal(deleted(f).length, 1);
		note(`logout-${id}-does-not-grant-other-authority`);
	}
	const dev = await fresh("development");
	const d = act(dev, "out", {
		development: true,
		proofDevelopment: true,
		cookie: await signed("synthetic-bearer-s0", key, true),
	});
	assert.equal(d.ok, true);
	checkClear(d, true);
	note("development-profile-cookie-clearing");
	const bad = await fresh("proof");
	const otherKey = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE";
	for (const [id, request] of [
		["workspace", { proofWorkspace: "other" }],
		[
			"key",
			{
				proofKey: otherKey,
				cookie: await signed("synthetic-bearer-s0", otherKey),
			},
		],
		[
			"profile",
			{
				proofDevelopment: true,
				cookie: await signed("synthetic-bearer-s0", key, true),
			},
		],
		["pending", { cookie: run(bad, { mode: "begin" }).cookie }],
		["service", { service: "" }],
	] as const) {
		refused(`proof-${id}`, bad, "list", request);
		refused(`logout-proof-${id}`, bad, "out", request);
	}
	for (const preflight of [
		"BEGIN IMMEDIATE",
		"PRAGMA query_only=ON",
		"CREATE TEMP TABLE forbidden(x)",
	]) {
		const r = act(bad, "list", { proofWorkspace: "other", preflight });
		assert.equal(r.code, "connection_refused");
		assert.equal(r.reusable, false);
		note(`unready-wrong-proof-${preflight.split(" ")[0]}`);
	}
	for (const [id, change, err] of [
		[
			"access",
			"INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')",
		],
		["email", "UPDATE user SET emailVerified=0"],
		["MFA", "UPDATE user SET twoFactorEnabled=0"],
		["factor", "UPDATE twoFactor SET verified=0"],
		["NULL-factor", "UPDATE twoFactor SET verified=NULL"],
		["factor-missing", "DELETE FROM twoFactor"],
		[
			"factor-duplicate",
			"INSERT INTO twoFactor(id,userId,secret,backupCodes,verified) SELECT 'duplicate-factor',userId,secret,backupCodes,verified FROM twoFactor",
			"invalid_control",
		],
		[
			"exhaustion",
			"INSERT INTO rm_workspace_user_access_events(id,user_id,event_type,workspace_role,actor) VALUES(9223372036854775807,'u','activate','member','agent:synthetic')",
			"invalid_control",
		],
	] as const) {
		const f = await fresh(`refuse-${id}`);
		sql(f, change);
		refused(id, f, "list", {}, err ?? "invalid_credentials");
		refused(`revoke-${id}`, f, "all", {}, err ?? "invalid_credentials");
	}
	for (const target of ["", " ", "bad\nID", "x".repeat(4097)])
		refused(
			`invalid-target-${target.length}`,
			bad,
			"one",
			{ target },
			"invalid_request",
		);
	const times = await fresh("freshness");
	assert.equal(act(times, "list", { now: now + 599999 }).ok, true);
	note("before-exact-freshness-boundary");
	refused("exact-freshness", times, "list", { now: now + 600000 });
	refused("revocation-exact-freshness", times, "all", { now: now + 600000 });
	refused("future-clock", times, "list", { now: now - 1 });
	refused("exact-session-expiry", times, "list", { now: now + 43200000 });
	for (const [id, change] of [
		["expired-malformed", "UPDATE session SET expiresAt='bad' WHERE id='sx'"],
		[
			"future-created",
			"UPDATE session SET createdAt='2026-01-01T00:00:01.000Z' WHERE id='s1'",
		],
		[
			"long-life",
			"UPDATE session SET expiresAt='2026-01-01T12:00:00.001Z' WHERE id='s0'",
		],
		["UA-control", "UPDATE session SET userAgent=char(10) WHERE id='s1'"],
		["IP-control", "UPDATE session SET ipAddress=char(0) WHERE id='s1'"],
	]) {
		const f = await fresh(`metadata-${id}`);
		sql(f, change);
		refused(
			`metadata-${id}`,
			f,
			"list",
			{},
			id === "long-life" ? "invalid_credentials" : "invalid_control",
		);
		assert.equal(act(f, "out").ok, true);
		note(`logout-available-with-${id}`);
	}
	for (const [id, value, column] of [
		["IP", "x".repeat(257), "ipAddress"],
		["UA", "x".repeat(2049), "userAgent"],
	]) {
		const f = await fresh(`length-${id}`),
			db = new Database(f.control);
		db.query(`UPDATE session SET ${column}=? WHERE id='s1'`).run(value);
		db.close();
		refused(`oversized-${id}`, f, "list", {}, "invalid_control");
	}
	const many = await fresh("row-limit"),
		mdb = new Database(many.control),
		add = mdb.query(
			"INSERT INTO session(id,token,userId,createdAt,updatedAt,expiresAt) VALUES(?,?,'u','2025-12-31T23:59:00.000Z','2025-12-31T23:59:00.000Z','2025-12-31T23:59:59.999Z')",
		);
	mdb.transaction(() => {
		for (let i = 0; i < 996; i++) add.run(`extra-${i}`, `extra-bearer-${i}`);
	})();
	mdb.close();
	assert.equal(act(many).sessions.length, 3);
	note("exact1000-including-expired-rows-accepted");
	sql(
		many,
		"INSERT INTO session(id,token,userId,createdAt,updatedAt,expiresAt) VALUES('extra-over','extra-over-bearer','u','2025-12-31T23:59:00.000Z','2025-12-31T23:59:00.000Z','2025-12-31T23:59:59.999Z')",
	);
	refused(
		"1001-total-expired-and-active-rows-refuse-partial-list",
		many,
		"list",
		{},
		"session_listing_limit",
	);
	assert.equal(act(many, "others").deleted, 1000);
	assert.equal(act(many).sessions.length, 1);
	note("revocation-cleans-oversized-list");
	for (const escaped of [false, true]) {
		const f = await fresh(`wire-limit-${escaped}`),
			db = new Database(f.control),
			add = db.query(
				"INSERT INTO session(id,token,userId,createdAt,updatedAt,expiresAt,userAgent) VALUES(?,?,'u','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z',?)",
			);
		db.transaction(() => {
			for (let i = 0; i < (escaped ? 400 : 600); i++)
				add.run(
					`payload-${i}`,
					`payload-bearer-${i}`,
					(escaped ? '"' : "x").repeat(2000),
				);
		})();
		db.close();
		refused(
			escaped
				? "serialized-escaping-limit-below-raw-budget"
				: "raw-metadata-byte-limit",
			f,
			"list",
			{},
			"session_listing_limit",
		);
		assert(act(f, "others").deleted > 0);
		assert.equal(act(f).sessions.length, 1);
		note(`limit-cleanup-${escaped}`);
	}
	for (const op of ["list", "out", "one", "others", "all"]) {
		const f = await fresh(`commit-busy-${op}`),
			before = image(f),
			p = start(
				f,
				{ mode: `session-${op}`, cookie: f.cookie, target: "s1" },
				"before-commit",
			),
			reader = new Database(f.control);
		try {
			await marker(p, f);
			reader.exec("BEGIN;SELECT count(*) FROM user");
			p.child.stdin.write("continue\n");
			p.child.stdin.end();
			const r = await done(p);
			assert.equal(r.stage, "OutcomeUnknown");
			assert.equal(r.reusable, false);
			assert.equal(r.sessions, undefined);
			assert.equal(r.clearCookies, undefined);
			reader.exec("ROLLBACK");
			assert.equal(image(f), before);
			note(`${op}-real-commit-busy-withholds-result-and-headers`);
		} finally {
			reader.close();
			clearTimeout(p.timer);
			p.child.kill();
			await p.child.exited;
		}
	}
	const interop = await fresh("source-interop", "delete", Date.now()),
		copy = join(interop.dir, "source-copy.sqlite");
	copyFileSync(interop.control, copy);
	const db = new Database(copy),
		auth = createBetterAuthRuntime(db, opts);
	try {
		const list = await auth.api.listSessions({
			headers: new Headers({ cookie: interop.cookie }),
		});
		assert.deepEqual(
			list.map((s: any) => s.id).sort(),
			act(interop, "list", { now: Date.now() })
				.sessions.map((s: any) => s.id)
				.sort(),
		);
		note("source-and-native-active-session-metadata-IDs-agree");
		const response = await auth.handler(
			new Request("https://example.invalid/api/auth/revoke-session", {
				method: "POST",
				headers: {
					cookie: interop.cookie,
					origin: "https://example.invalid",
					"content-type": "application/json",
				},
				body: JSON.stringify({ token: "synthetic-bearer-s1" }),
			}),
		);
		assert.equal(response.status, 200);
		assert.equal((await response.json()).status, true);
		db.close();
		copyFileSync(copy, interop.control);
		refused(
			"actual-source-revocation-rejected-by-native-current-resolver",
			interop,
			"list",
			{ cookie: await signed("synthetic-bearer-s1"), now: Date.now() },
		);
	} finally {
		db.close();
	}
	const nr = act(interop, "one", { target: "s2", now: Date.now() });
	assert.equal(nr.deleted, 1);
	const sdb = new Database(interop.control),
		source = createBetterAuthRuntime(sdb, opts);
	try {
		assert.equal(
			await source.api.getSession({
				headers: new Headers({ cookie: await signed("synthetic-bearer-s2") }),
			}),
			null,
		);
		assert.equal(
			(
				await source.api.getSession({
					headers: new Headers({ cookie: interop.cookie }),
				})
			)?.user.id,
			"u",
		);
		note("native-revocation-rejected-by-actual-source-reader");
		const out = await source.handler(
			new Request("https://example.invalid/api/auth/sign-out", {
				method: "POST",
				headers: {
					cookie: interop.cookie,
					origin: "https://example.invalid",
					"content-type": "application/json",
				},
				body: "{}",
			}),
		);
		assert.equal(out.status, 200);
		assert.equal((await out.json()).success, true);
		const headers = out.headers.getSetCookie();
		for (const suffix of ["session_token", "session_data", "dont_remember"])
			assert(
				headers.some(
					(h) =>
						h.startsWith(`__Secure-better-auth.${suffix}=`) &&
						h.includes("Max-Age=0"),
				),
			);
		note("actual-source-native-profile-clear-cookie-attributes");
	} finally {
		sdb.close();
	}
	assert.equal(
		act(interop, "list", { now: Date.now() }).code,
		"invalid_credentials",
	);
	assert.equal(act(interop, "out", { now: Date.now() }).deleted, 0);
	note("source-logout-native-idempotent-missing-row");
	return sensitive;
}
