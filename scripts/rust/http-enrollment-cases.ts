/** Password-bound HTTP setup using actual native handlers and installed source crypto. */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { Database } from "bun:sqlite";
import { createHmac } from "node:crypto";
import { symmetricDecrypt } from "../../node_modules/better-auth/dist/crypto/index.mjs";
import { createOTP } from "../../node_modules/@better-auth/utils/dist/otp.mjs";
export async function verifyHttpEnrollment(c: any) {
	const { fixture, start, key, now, password, note } = c;
	const keyset = { currentVersion: 1, keys: new Map([[1, key]]) };
	const query = (f: any, table: string) => {
		const d = new Database(f.control);
		try {
			return d.query(`SELECT * FROM ${table} ORDER BY rowid`).all() as any[];
		} finally {
			d.close();
		}
	};
	const sql = (f: any, text: string) => {
		const d = new Database(f.control);
		try {
			d.exec(text);
		} finally {
			d.close();
		}
	};
	const image = (f: any) =>
		JSON.stringify(
			[
				"user",
				"account",
				"twoFactor",
				"session",
				"verification",
				"rm_workspace_user_access_events",
			].map((t) => query(f, t)),
		);
	const fresh = (id: string, mode: string) => {
		const f = fixture(`enrollment-${id}`, mode);
		sql(
			f,
			`UPDATE user SET twoFactorEnabled=0; DELETE FROM twoFactor;
      INSERT INTO session(id,token,userId,createdAt,updatedAt,expiresAt) VALUES('old','old-synthetic-token','u','${new Date(now - 1000).toISOString()}','${new Date(now - 1000).toISOString()}','${new Date(now + 3600000).toISOString()}');`,
		);
		return { ...f, outcomeLog: f.control + ".outcomes.jsonl" };
	};
	const code = (secret: string) => {
		const counter = Buffer.alloc(8);
		counter.writeBigUInt64BE(BigInt(Math.floor(now / 30000)));
		const h = createHmac("sha1", secret).update(counter).digest();
		return String(
			(h.readUInt32BE(h[19]! & 15) & 0x7fffffff) % 1000000,
		).padStart(6, "0");
	};
	const decrypt = (data: string) => symmetricDecrypt({ key: keyset, data });
	function browser(server: any) {
		const jar = new Map<string, string>();
		return {
			header: () => [...jar.values()].join("; "),
			async call(
				path: string,
				body?: object,
				extra: Record<string, string> = {},
			) {
				const headers: Record<string, string> = {
					cookie: [...jar.values()].join("; "),
					...extra,
				};
				if (body !== undefined) {
					headers.origin = server.origin;
					headers["content-type"] = "application/json";
				}
				const r = await fetch(server.origin + "/api/auth/" + path, {
					method: body === undefined ? "GET" : "POST",
					headers,
					body: body === undefined ? undefined : JSON.stringify(body),
					redirect: "manual",
				});
				const cookies = r.headers.getSetCookie();
				for (const h of cookies) {
					const pair = h.split(";")[0]!,
						name = pair.split("=")[0]!;
					if (h.includes("Max-Age=0")) jar.delete(name);
					else jar.set(name, pair);
				}
				assert.equal(r.headers.get("cache-control"), "no-store");
				return { status: r.status, data: (await r.json()) as any, cookies };
			},
		};
	}
	for (const mode of ["DELETE", "WAL"]) {
		const f = fresh(`${mode}-journey`, mode),
			server = await start(f),
			b = browser(server);
		try {
			let r = await b.call("mfa-enrollment/begin", {
				email: "u@example.invalid",
				password,
			});
			assert.equal(r.status, 200);
			assert.equal(r.data.method, "totp");
			assert.equal(r.data.backupCodes, undefined);
			assert.equal(r.data.status, undefined);
			const pending = b.header(),
				factor = query(f, "twoFactor")[0],
				secret = await decrypt(factor.secret),
				pool = JSON.parse(await decrypt(factor.backupCodes));
			assert.equal(
				r.data.totpURI,
				createOTP(secret).url("Rentemester", "u@example.invalid"),
			);
			assert.equal(factor.verified, 0);
			assert.equal(query(f, "user")[0].twoFactorEnabled, 0);
			assert.equal(query(f, "session")[0].id, "old");
			note(`enrollment/${mode}/known-begin-uri-only-pending`);
			r = await b.call("get-session");
			assert.equal(r.status, 200);
			assert.equal(r.data, null);
			assert.equal(b.header(), pending);
			note(`enrollment/${mode}/pending-no-session`);
			const original = image(f);
			for (const route of [
				"two-factor/verify-totp",
				"two-factor/verify-backup-code",
			]) {
				r = await b.call(route, {
					code: route.endsWith("totp") ? code(secret) : pool[0],
				});
				assert.equal(r.status, 401);
				assert.equal(r.cookies.length, 0);
				assert.equal(image(f), original);
				note(`enrollment/${mode}/cross-login-refused-${route}`);
			}
			r = await b.call(
				"mfa-enrollment/confirm",
				{ code: code(secret) },
				{ "x-rentemester-service-key": "" },
			);
			assert.equal(r.status, 401);
			assert.equal(image(f), original);
			assert.equal(r.cookies.length, 0);
			note(`enrollment/${mode}/mixed-credentials-refused`);
			r = await b.call("mfa-enrollment/confirm", { code: code(secret) });
			assert.equal(r.status, 200);
			assert.equal(r.data.status, true);
			assert.deepEqual(r.data.backupCodes, pool);
			assert.equal(pool.length, 10);
			assert.equal(new Set(pool).size, 10);
			assert(!r.cookies.at(-1)!.includes("Max-Age"));
			assert.equal(query(f, "user")[0].twoFactorEnabled, 1);
			assert.equal(query(f, "twoFactor")[0].verified, 1);
			const sessions = query(f, "session");
			assert.equal(sessions.length, 1);
			assert.notEqual(sessions[0].id, "old");
			note(`enrollment/${mode}/confirm-once-codes-session-and-old-revocation`);
			r = await b.call("get-session");
			assert.equal(r.status, 200);
			assert.equal(r.data.user.id, "u");
			assert(!("token" in r.data.session));
			note(`enrollment/${mode}/current-session`);
			const committed = image(f);
			r = await b.call(
				"mfa-enrollment/confirm",
				{ code: code(secret) },
				{ cookie: pending },
			);
			assert.equal(r.status, 401);
			assert.equal(r.data.backupCodes, undefined);
			assert.equal(r.cookies.length, 0);
			assert.equal(image(f), committed);
			note(`enrollment/${mode}/replay-no-disclosure`);
			r = await b.call("sign-in/email", {
				email: "u@example.invalid",
				password,
				rememberMe: false,
			});
			assert.equal(r.status, 200);
			const enrolled = image(f);
			// Confirmation has consumed three IP slots; a new listener proves domain rejection, not rate429.
			const second = await start(f);
			try {
				const other = browser(second);
				r = await other.call(
					"mfa-enrollment/confirm",
					{ code: code(secret) },
					{ cookie: b.header() },
				);
				assert.equal(r.status, 401);
				assert.equal(r.cookies.length, 0);
				assert.equal(image(f), enrolled);
			} finally {
				await second.stop();
			}
			note(`enrollment/${mode}/login-challenge-cannot-enroll`);
		} finally {
			await server.stop();
		}

		const restart = fresh(`${mode}-restart`, mode),
			rs = await start(restart),
			rb = browser(rs);
		try {
			let r = await rb.call("mfa-enrollment/begin", {
				email: "u@example.invalid",
				password,
			});
			assert.equal(r.status, 200);
			const old = rb.header(),
				oldFactor = query(restart, "twoFactor")[0];
			const oldSecret = await decrypt(oldFactor.secret);
			r = await rb.call("mfa-enrollment/confirm", { code: "invalid" });
			assert.equal(r.status, 401);
			const failures = query(restart, "twoFactor")[0].failedVerificationCount;
			assert.equal(failures, 1);
			r = await rb.call("mfa-enrollment/begin", {
				email: "u@example.invalid",
				password,
			});
			assert.equal(r.status, 200);
			const current = query(restart, "twoFactor")[0];
			assert.notEqual(current.secret, oldFactor.secret);
			assert.equal(current.failedVerificationCount, failures);
			note(`enrollment/${mode}/restart-preserves-account-budget`);
			r = await rb.call(
				"mfa-enrollment/confirm",
				{ code: code(oldSecret) },
				{ cookie: old },
			);
			assert.equal(r.status, 401);
			assert.equal(r.cookies.length, 0);
			assert.equal(query(restart, "user")[0].twoFactorEnabled, 0);
			note(`enrollment/${mode}/restart-invalidates-old-context`);
			const secret = await decrypt(current.secret);
			const before = image(restart);
			const raceServer = await start(restart);
			try {
				const racer = browser(raceServer),
					pending = rb.header();
				const [a, b] = await Promise.all([
					racer.call(
						"mfa-enrollment/confirm",
						{ code: code(secret) },
						{ cookie: pending },
					),
					racer.call(
						"mfa-enrollment/confirm",
						{ code: code(secret) },
						{ cookie: pending },
					),
				]);
				assert(
					[a, b].every((r) => r.status !== 429),
					"race must reach native admission, not IP rate",
				);
				assert.equal([a, b].filter((r) => r.status === 200).length, 1);
				assert.equal(
					[a, b].filter((r) => r.data.backupCodes !== undefined).length,
					1,
				);
				assert.equal(query(restart, "session").length, 1);
				assert.notEqual(image(restart), before);
			} finally {
				await raceServer.stop();
			}
			note(`enrollment/${mode}/concurrent-confirm-one-disclosure`);
		} finally {
			await rs.stop();
		}
	}
	for (const mode of ["DELETE", "WAL"]) {
		const f = fresh(`${mode}-strict`, mode),
			server = await start(f),
			b = browser(server);
		try {
			const before = image(f);
			for (const [body, status] of [
				[{ email: "u@example.invalid", password, issuer: "Hostile" }, 400],
				[{ email: "u@example.invalid", password, rememberMe: false }, 400],
				[{ email: "u@example.invalid", password: "wrong-password" }, 401],
			] as const) {
				const r = await b.call("mfa-enrollment/begin", body);
				assert.equal(r.status, status);
				assert.equal(r.cookies.length, 0);
				assert.equal(image(f), before);
				note(
					`enrollment/${mode}/strict-begin-${Object.keys(body).join("-")}-${status}`,
				);
			}
		} finally {
			await server.stop();
		}
		const next = await start(f),
			cb = browser(next);
		try {
			let r = await cb.call("mfa-enrollment/begin", {
				email: "u@example.invalid",
				password,
			});
			assert.equal(r.status, 200);
			const before = image(f);
			for (const body of [
				{ code: "123456", trustDevice: false },
				{ code: "123456", disableSession: false },
			]) {
				r = await cb.call("mfa-enrollment/confirm", body);
				assert.equal(r.status, 400);
				assert.equal(r.cookies.length, 0);
				assert.equal(image(f), before);
				note(
					`enrollment/${mode}/strict-confirm-${Object.keys(body).join("-")}`,
				);
			}
			const r2 = await fetch(next.origin + "/api/auth/mfa-enrollment/confirm", {
				method: "POST",
				headers: {
					origin: next.origin,
					"content-type": "application/json",
					cookie: cb.header(),
				},
				body: '{"code":"123456","code":"654321"}',
			});
			assert.equal(r2.status, 400);
			assert.equal(r2.headers.getSetCookie().length, 0);
			assert.equal(image(f), before);
			note(`enrollment/${mode}/duplicate-confirm-refused`);
		} finally {
			await next.stop();
		}
	}
	// DELETE reader allows writes but forces actual commit-busy at either native transaction.
	for (const operation of ["begin", "confirm"]) {
		const f = fresh(`commit-busy-${operation}`, "DELETE"),
			server = await start(f),
			b = browser(server);
		try {
			let secret = "";
			if (operation === "confirm") {
				const r = await b.call("mfa-enrollment/begin", {
					email: "u@example.invalid",
					password,
				});
				assert.equal(r.status, 200);
				secret = await decrypt(query(f, "twoFactor")[0].secret);
			}
			const before = image(f),
				reader = new Database(f.control);
			reader.exec("BEGIN;SELECT * FROM user;");
			try {
				const r = await b.call(
					`mfa-enrollment/${operation}`,
					operation === "begin"
						? { email: "u@example.invalid", password }
						: { code: code(secret) },
				);
				assert.equal(r.status, 503);
				assert.equal(r.data.code, "AUTH_UNCONFIRMED");
				assert.equal(r.cookies.length, 0);
				assert.equal(r.data.backupCodes, undefined);
				assert.equal(r.data.totpURI, undefined);
			} finally {
				reader.exec("ROLLBACK");
				reader.close();
			}
			assert.equal(image(f), before);
			const observed = JSON.parse(
				readFileSync(f.outcomeLog, "utf8").trim().split("\n").at(-1)!,
			);
			assert.equal(observed.operation, operation);
			assert.equal(observed.stage, "OutcomeUnknown");
			assert.equal(observed.code, "commit_outcome_unknown");
			note(
				`enrollment/DELETE/${operation}-commit-busy-no-authority-disclosure`,
			);
		} finally {
			await server.stop();
		}
	}
}
