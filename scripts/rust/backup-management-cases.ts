/** Own encrypted-code view/regeneration, current identity/password fence, actual source readers. */
import assert from "node:assert/strict";
import { copyFileSync } from "node:fs";
import { join } from "node:path";
import { Database } from "bun:sqlite";
import { serializeSignedCookie } from "../../node_modules/better-call/dist/cookies.mjs";
import { generateBackupCodes } from "../../node_modules/better-auth/dist/plugins/two-factor/backup-codes/index.mjs";
import {
	symmetricEncrypt,
	symmetricDecrypt,
} from "../../node_modules/better-auth/dist/crypto/index.mjs";
import { createBetterAuthRuntime } from "../../src/server/better-auth";
export async function verifyManagement(c: any) {
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
	const note = (id: string) => record(`management-${id}`);
	const q = (f: any, text: string) => {
		const db = new Database(f.control);
		try {
			return db.query(text).all() as any[];
		} finally {
			db.close();
		}
	};
	const decrypt = (value: string) =>
		symmetricDecrypt({
			key: { currentVersion: 1, keys: new Map([[1, key]]) },
			data: value,
		});
	const encrypt = (value: string) =>
		symmetricEncrypt({
			key: { currentVersion: 1, keys: new Map([[1, key]]) },
			data: value,
		});
	const signed = async (token: string, signer = key, development = false) =>
		(
			await serializeSignedCookie(
				`${development ? "" : "__Secure-"}better-auth.session_token`,
				token,
				signer,
				{ path: "/", httpOnly: true, secure: !development, sameSite: "lax" },
			)
		).split(";")[0];
	const generated = await generateBackupCodes(
		{ currentVersion: 1, keys: new Map([[1, key]]) },
		{ storeBackupCodes: "encrypted" },
	);
	const original = generated.backupCodes;
	sensitive.push(...original, generated.encryptedBackupCodes);
	const factor = (f: any) => q(f, "SELECT * FROM twoFactor")[0];
	async function fresh(
		id: string,
		mode = "delete",
		time = now,
		pool = generated.encryptedBackupCodes,
	) {
		const f = fixture(`management-${id}`, mode),
			db = new Database(f.control);
		db.query("UPDATE twoFactor SET backupCodes=?").run(pool);
		db.query(
			"INSERT INTO session(id,token,userId,createdAt,updatedAt,expiresAt) VALUES('management-session','management-token','u',?,?,?)",
		).run(
			new Date(time).toISOString(),
			new Date(time).toISOString(),
			new Date(time + 43200000).toISOString(),
		);
		db.close();
		const cookie = await signed("management-token");
		sensitive.push(cookie);
		return { ...f, cookie };
	}
	const manage = (f: any, operation = "view", extra: any = {}) =>
		run(f, { mode: `codes-${operation}`, cookie: f.cookie, ...extra });
	const unchanged = (f: any) =>
		JSON.stringify(
			[
				"user",
				"account",
				"session",
				"verification",
				"rm_workspace_auth_state_events",
				"rm_workspace_auth_telemetry_events",
			].map((t) => q(f, `SELECT * FROM ${t} ORDER BY rowid`)),
		);
	function refused(
		id: string,
		f: any,
		request: any = {},
		code = "invalid_credentials",
	) {
		const before = image(f),
			r = manage(f, "view", request);
		assert.equal(r.ok, false, id);
		assert.equal(r.code, code, id);
		assert.equal(r.codes, undefined);
		assert.equal(r.stage, "NotCommitted");
		assert.equal(image(f), before, id);
		note(id);
		return r;
	}
	for (const mode of ["wal", "delete"]) {
		const f = await fresh(`happy-${mode}`, mode),
			before = unchanged(f),
			old = factor(f),
			view = manage(f);
		assert.equal(view.ok, true);
		assert.deepEqual(view.codes, original);
		assert.equal(unchanged(f), before);
		assert.deepEqual(factor(f), old);
		let audit = q(
			f,
			"SELECT * FROM workspace_audit WHERE entity_type='native_mfa_backup_management_v1' ORDER BY id",
		);
		assert.equal(audit.length, 1);
		assert.equal(audit[0].event_type, "mfa_backup_codes_viewed");
		assert.deepEqual(JSON.parse(audit[0].entity_id), [
			"u",
			"management-session",
			1,
		]);
		assert.equal(audit[0].actor, "user:u");
		assert.equal(audit[0].created_at, new Date(now).toISOString());
		note(`${mode}-own-view-current-pool-and-atomic-audit`);
		const regen = manage(f, "regenerate");
		assert.equal(regen.ok, true);
		assert.equal(regen.codes.length, 10);
		assert.equal(new Set(regen.codes).size, 10);
		sensitive.push(...regen.codes);
		for (const code of regen.codes)
			assert.match(code, /^[A-Za-z0-9]{5}-[A-Za-z0-9]{5}$/);
		assert(!regen.codes.some((s: string) => original.includes(s)));
		const updated = factor(f);
		assert.notEqual(updated.backupCodes, old.backupCodes);
		assert.deepEqual(
			JSON.parse(await decrypt(updated.backupCodes)),
			regen.codes,
		);
		assert.deepEqual({ ...updated, backupCodes: old.backupCodes }, old);
		assert.equal(unchanged(f), before);
		audit = q(
			f,
			"SELECT * FROM workspace_audit WHERE entity_type='native_mfa_backup_management_v1' ORDER BY id",
		);
		assert.equal(audit.length, 2);
		assert.equal(audit[1].event_type, "mfa_backup_codes_regenerated");
		note(`${mode}-replace-ten-codes-preserves-session-factor-budgets-replay`);
		assert.deepEqual(manage(f).codes, regen.codes);
		note(`${mode}-repeat-view-returns-current-pool`);
		const login = run(f, { mode: "begin" });
		assert.equal(login.pending, true);
		assert.equal(
			run(f, { backup: true, cookie: login.cookie, code: original[0] })
				.rejected,
			"InvalidCode",
		);
		assert.equal(
			run(f, { backup: true, cookie: login.cookie, code: regen.codes[0] })
				.session,
			true,
		);
		assert.deepEqual(manage(f).codes, regen.codes.slice(1));
		note(`${mode}-old-code-refuses-new-code-consumes-current-pool`);
		// A pre-existing pending challenge must read the replacement pool, never retain its old pool.
		const pending = run(f, { mode: "begin" }),
			again = manage(f, "regenerate");
		sensitive.push(...again.codes);
		assert.equal(
			run(f, { backup: true, cookie: pending.cookie, code: again.codes[0] })
				.session,
			true,
		);
		note(`${mode}-existing-login-challenge-uses-new-pool`);
		for (const operation of ["view", "regenerate"]) {
			const late = await fresh(`late-${mode}-${operation}`, mode),
				before = image(late),
				r = manage(late, operation, { failSql: true });
			assert.equal(r.code, "invalid_control");
			assert.equal(r.reusable, true);
			assert.equal(r.codes, undefined);
			assert.equal(image(late), before);
			note(`${mode}-${operation}-late-SQL-restores-pool-and-audit`);
		}
		// The mutation admission lock covers both audit-only view and regeneration; revocation cannot slip in.
		const held = await fresh(`held-${mode}`, mode),
			p = start(
				held,
				{ mode: "codes-regenerate", cookie: held.cookie },
				"password:locked",
			);
		try {
			await marker(p, held);
			assert.throws(() => sql(held, "DELETE FROM session"));
			p.child.stdin.write("continue\n");
			p.child.stdin.end();
			const r = await done(p);
			assert.equal(r.ok, true);
			sensitive.push(...r.codes);
			sql(held, "DELETE FROM session");
			refused(`${mode}-revoked-after-commit`, held);
			note(`${mode}-writer-lock-fences-revoke-through-commit`);
		} finally {
			clearTimeout(p.timer);
			p.child.kill();
			await p.child.exited;
		}
		const consumption = await fresh(`consume-race-${mode}`, mode),
			bp = run(consumption, { mode: "begin" }),
			consumer = start(
				consumption,
				{ backup: true, cookie: bp.cookie, code: original[0] },
				"locked",
			);
		try {
			await marker(consumer, consumption);
			const r = manage(consumption, "regenerate");
			assert.equal(r.code, "control_busy_or_unavailable");
			consumer.child.stdin.write("continue\n");
			consumer.child.stdin.end();
			assert.equal((await done(consumer)).session, true);
			assert.deepEqual(manage(consumption).codes, original.slice(1));
			note(`${mode}-concurrent-backup-consumption-no-lost-update`);
		} finally {
			clearTimeout(consumer.timer);
			consumer.child.kill();
			await consumer.child.exited;
		}
	}
	const bad = await fresh("password-proof");
	refused("password-required", bad, { password: "wrong" });
	refused("empty-password-refuses", bad, { password: "" });
	refused("unknown-session-refuses", bad, {
		cookie: await signed("missing-token"),
	});
	refused("pending-challenge-not-session", bad, {
		cookie: run(bad, { mode: "begin" }).cookie,
	});
	const otherKey = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE";
	for (const [id, request] of [
		[
			"proof-key",
			{
				proofKey: otherKey,
				cookie: await signed("management-token", otherKey),
			},
		],
		[
			"proof-profile",
			{
				proofDevelopment: true,
				cookie: await signed("management-token", key, true),
			},
		],
		["proof-workspace", { proofWorkspace: "other" }],
		["service-confusion", { service: "" }],
	] as const)
		refused(id, bad, request);
	for (const preflight of [
		"BEGIN IMMEDIATE",
		"PRAGMA query_only=ON",
		"CREATE TEMP TABLE forbidden(x)",
	]) {
		const r = manage(bad, "view", { proofWorkspace: "other", preflight });
		assert.equal(r.code, "connection_refused");
		assert.equal(r.reusable, false);
		assert.equal(r.codes, undefined);
		note("wrong-proof-unready-handle-" + preflight.split(" ")[0]);
	}
	for (const [id, change, err] of [
		[
			"disabled-access",
			"INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')",
		],
		["unverified-email", "UPDATE user SET emailVerified=0"],
		["disabled-MFA", "UPDATE user SET twoFactorEnabled=0"],
		["unverified-factor", "UPDATE twoFactor SET verified=0"],
		["NULL-factor", "UPDATE twoFactor SET verified=NULL"],
		["missing-factor", "DELETE FROM twoFactor"],
		[
			"duplicate-factor",
			"INSERT INTO twoFactor(id,userId,secret,backupCodes,verified) SELECT 'duplicate-factor',userId,secret,backupCodes,verified FROM twoFactor",
			"invalid_control",
		],
		[
			"bad-budget",
			"UPDATE twoFactor SET failedVerificationCount=-1",
			"invalid_control",
		],
		[
			"ordered-exhaustion",
			"INSERT INTO rm_workspace_user_access_events(id,user_id,event_type,workspace_role,actor) VALUES(9223372036854775807,'u','activate','member','agent:synthetic')",
			"invalid_control",
		],
		["missing-credential", "DELETE FROM account"],
	] as const) {
		const f = await fresh(id);
		sql(f, change);
		refused(id, f, {}, err ?? "invalid_credentials");
	}
	const boundary = await fresh("time-boundaries");
	assert.equal(manage(boundary, "view", { now: now + 599999 }).ok, true);
	note("fresh-before-ten-minutes");
	refused("exact-fresh-expiry", boundary, { now: now + 600000 });
	refused("future-session", boundary, { now: now - 1 });
	refused("exact-session-expiry", boundary, { now: now + 43200000 });
	const long = await fresh("lifetime-limit");
	sql(long, "UPDATE session SET expiresAt='2026-01-01T12:00:00.001Z'");
	refused("over-twelve-hour-session", long);
	const locked = await fresh("preserve-lock");
	sql(
		locked,
		`UPDATE twoFactor SET failedVerificationCount=10,lockedUntil='${new Date(now + 900000).toISOString()}'`,
	);
	const lockBefore = factor(locked);
	const lr = manage(locked, "regenerate");
	assert.equal(lr.ok, true);
	sensitive.push(...lr.codes);
	assert.equal(
		factor(locked).failedVerificationCount,
		lockBefore.failedVerificationCount,
	);
	assert.equal(factor(locked).lockedUntil, lockBefore.lockedUntil);
	note("management-never-resets-login-lock");
	for (const [id, pool] of [
		["empty", "[]"],
		["consumed", JSON.stringify(original.slice(4))],
	]) {
		const f = await fresh(`pool-${id}`, "delete", now, await encrypt(pool));
		assert.deepEqual(manage(f).codes, JSON.parse(pool));
		assert.equal(manage(f, "regenerate").codes.length, 10);
		note(`valid-${id}-pool-view-and-replace`);
	}
	for (const [id, pool] of [
		["duplicate", JSON.stringify([original[0], original[0]])],
		["null", "null"],
		["format", '["short"]'],
		["count", JSON.stringify([...original, "ABCDE-67890"])],
	]) {
		const f = await fresh(`pool-${id}`, "delete", now, await encrypt(pool));
		refused(`pool-${id}`, f, {}, "invalid_control");
		refused(
			`replace-refuses-pool-${id}`,
			f,
			{ mode: "codes-regenerate" },
			"invalid_control",
		);
	}
	// Password KDF intentionally unlocked: separate writers can change facts; final admission refuses them.
	for (const [id, change] of [
		["password", "UPDATE account SET password='changed'"],
		[
			"disable",
			"INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')",
		],
		["session-delete", "DELETE FROM session"],
		["session-id", "UPDATE session SET id='replacement-session'"],
		["email", "UPDATE user SET email='changed@example.invalid'"],
		["MFA", "UPDATE user SET twoFactorEnabled=0"],
		["factor", "UPDATE twoFactor SET verified=0"],
		[
			"retarget",
			"INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES('v','Synthetic other','v@example.invalid',1,1,'2026-01-01','2026-01-01');UPDATE session SET userId='v'",
		],
	]) {
		const f = await fresh(`unlocked-${id}`),
			p = start(
				f,
				{ mode: "codes-regenerate", cookie: f.cookie },
				"password:unlocked",
			);
		try {
			await marker(p, f);
			sql(f, change);
			const before = image(f);
			p.child.stdin.write("continue\n");
			p.child.stdin.end();
			const r = await done(p);
			assert.equal(r.ok, false);
			assert.equal(r.code, "invalid_credentials");
			assert.equal(r.codes, undefined);
			assert.equal(image(f), before);
			note(`unlocked-${id}-change-refuses`);
		} finally {
			clearTimeout(p.timer);
			p.child.kill();
			await p.child.exited;
		}
	}
	for (const operation of ["view", "regenerate"]) {
		const f = await fresh(`commit-busy-${operation}`),
			before = image(f),
			p = start(
				f,
				{ mode: `codes-${operation}`, cookie: f.cookie },
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
			assert.equal(r.codes, undefined);
			reader.exec("ROLLBACK");
			assert.equal(image(f), before);
			note(`${operation}-real-commit-busy-withholds-codes`);
		} finally {
			reader.close();
			clearTimeout(p.timer);
			p.child.kill();
			await p.child.exited;
		}
	}
	const interop = await fresh("source-interop", "delete", Date.now()),
		ir = manage(interop, "regenerate", { now: Date.now() });
	assert.equal(ir.ok, true);
	sensitive.push(...ir.codes);
	const copy = join(interop.dir, "source-copy.sqlite");
	copyFileSync(interop.control, copy);
	const sourceDb = new Database(copy),
		auth = createBetterAuthRuntime(sourceDb, opts);
	try {
		// Conditional plugin registration erases this server-only method from InferAPI.
		const sourceView = Reflect.get(auth.api, "viewBackupCodes");
		assert.equal(typeof sourceView, "function");
		assert.deepEqual(
			(await sourceView({ body: { userId: "u" } })).backupCodes,
			ir.codes,
		);
		note("actual-source-view-reads-native-regeneration");
		const r = await auth.handler(
			new Request(
				"https://example.invalid/api/auth/two-factor/generate-backup-codes",
				{
					method: "POST",
					headers: {
						cookie: interop.cookie,
						origin: "https://example.invalid",
						"content-type": "application/json",
					},
					body: JSON.stringify({ password: "Synthetic password 123!" }),
				},
			),
		);
		assert.equal(r.status, 200);
		const generated = await r.json();
		sensitive.push(...generated.backupCodes);
		sourceDb.close();
		copyFileSync(copy, interop.control);
		assert.deepEqual(
			manage(interop, "view", { now: Date.now() }).codes,
			generated.backupCodes,
		);
		note("native-view-reads-actual-source-HTTP-regeneration");
	} finally {
		sourceDb.close();
	}
	return sensitive;
}
