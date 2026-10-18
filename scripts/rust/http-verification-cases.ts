/** Native HTTP verification against admitted synthetic controls; no legacy proxy or real mail. */
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { randomBytes } from "node:crypto";
import { Database } from "bun:sqlite";
import { createEmailVerificationToken } from "../../node_modules/better-auth/dist/api/routes/email-verification.mjs";
import { serializeSignedCookie } from "../../node_modules/better-call/dist/cookies.mjs";
export async function verifyHttpVerification(c: any) {
	const { fixture, start, key, now, note } = c;
	const sensitive: string[] = [
		key,
		"u@example.invalid",
		"missing@example.invalid",
		"http-verification-session",
	];
	const observations: string[] = [];
	const record = (id: string) => {
		observations.push(id);
		note(`verification/${id}`);
	};
	function q(f: any, s: string) {
		const d = new Database(f.control);
		try {
			return d.query(s).all() as any[];
		} finally {
			d.close();
		}
	}
	function sql(f: any, s: string) {
		const d = new Database(f.control);
		try {
			d.exec(s);
		} finally {
			d.close();
		}
	}
	function snapshot(f: any) {
		return JSON.stringify(
			[
				"user",
				"account",
				"verification",
				"session",
				"twoFactor",
				"rm_workspace_user_access_events",
				"workspace_audit",
				"rm_workspace_auth_telemetry_events",
				"rm_workspace_auth_state_events",
			].map((t) => q(f, `SELECT * FROM ${t} ORDER BY rowid`)),
		);
	}
	function fresh(id: string, mode = "DELETE") {
		const f = fixture(`verification-${id}`, mode);
		sql(f, "UPDATE user SET emailVerified=0 WHERE id='u'");
		return {
			...f,
			mailLog: join(dirname(f.control), "mail.jsonl"),
			outcomeLog: join(dirname(f.control), "outcome.jsonl"),
		};
	}
	const mail = (f: any) =>
		existsSync(f.mailLog)
			? readFileSync(f.mailLog, "utf8")
					.trim()
					.split("\n")
					.map((s: string) => JSON.parse(s))
			: [];
	const outcomes = (f: any) =>
		existsSync(f.outcomeLog)
			? readFileSync(f.outcomeLog, "utf8")
					.trim()
					.split("\n")
					.map((s: string) => JSON.parse(s))
			: [];
	const post = (
		s: any,
		body: any = { email: "u@example.invalid", callbackURL: "/login" },
		headers: any = {},
	) =>
		fetch(`${s.origin}/api/auth/send-verification-email`, {
			method: "POST",
			headers: {
				origin: s.origin,
				"content-type": "application/json",
				...headers,
			},
			body: typeof body === "string" ? body : JSON.stringify(body),
			redirect: "manual",
		});
	const get = (s: any, token: string, callback?: string) =>
		fetch(
			`${s.origin}/api/auth/verify-email?token=${token}${callback === undefined ? "" : `&callbackURL=${encodeURIComponent(callback)}`}`,
			{ redirect: "manual", headers: { "sec-fetch-site": "cross-site" } },
		);
	async function issue(f: any, s: any, body?: any, headers?: any) {
		const start = Date.now(),
			r = await post(s, body, headers);
		assert.equal(r.status, 200);
		assert.deepEqual(await r.json(), { status: true });
		assert(Date.now() - start >= 450);
		assert.equal(r.headers.get("cache-control"), "no-store");
		assert.equal(r.headers.get("referrer-policy"), "no-referrer");
		assert.equal(r.headers.get("set-cookie"), null);
		const sent = mail(f);
		for (const m of sent) sensitive.push(m.token, m.url, m.recipient);
		return sent.at(-1);
	}
	for (const mode of ["DELETE", "WAL"]) {
		const f = fresh(`${mode}-success`, mode),
			s = await start(f, { verification: true });
		try {
			const m = await issue(f, s);
			assert(m);
			const url = new URL(m.url);
			assert.equal(url.origin, s.origin);
			assert.equal(url.pathname, "/api/auth/verify-email");
			assert.equal(url.searchParams.get("callbackURL"), "/login");
			assert.equal(q(f, "SELECT * FROM verification").length, 1);
			record(`${mode}-generic-ack-floor-committed-unlocked-mail`);
			for (let i = 0; i < 12; i++)
				assert.equal(
					(await fetch(m.url, { method: "HEAD", redirect: "manual" })).status,
					405,
				);
			assert.equal(
				q(f, "SELECT emailVerified v FROM user WHERE id='u'")[0].v,
				0,
			);
			record(`${mode}-HEAD-no-consume-or-rate-charge`);
			const r = await get(
				s,
				m.token,
				"/login?token=old&error=old&verified=true&x=1",
			);
			assert.equal(r.status, 302);
			assert.equal(r.headers.get("location"), `${s.origin}/login?x=1`);
			assert.equal(r.headers.get("set-cookie"), null);
			assert.equal(
				q(f, "SELECT emailVerified v FROM user WHERE id='u'")[0].v,
				1,
			);
			assert.equal(q(f, "SELECT * FROM session").length, 0);
			assert.equal(q(f, "SELECT * FROM verification").length, 0);
			record(`${mode}-known-completion-sanitized-redirect-no-session`);
			const before = snapshot(f),
				replay = await get(s, m.token);
			assert.equal(replay.status, 401);
			assert.deepEqual(await replay.json(), { code: "INVALID_TOKEN" });
			assert.equal(snapshot(f), before);
			record(`${mode}-one-time-replay-refusal`);
		} finally {
			await s.stop();
		}
	}
	for (const id of ["missing", "verified", "delivery-fail", "writer-busy"]) {
		const f = fresh(id);
		if (id === "verified")
			sql(f, "UPDATE user SET emailVerified=1 WHERE id='u'");
		const s = await start(f, {
			verification: true,
			deliveryFail: id === "delivery-fail",
		});
		const held = id === "writer-busy" ? new Database(f.control) : undefined;
		try {
			held?.exec("BEGIN IMMEDIATE");
			const before = snapshot(f);
			const r = await post(s, {
				email:
					id === "missing" ? "missing@example.invalid" : "u@example.invalid",
			});
			assert.equal(r.status, 200);
			assert.deepEqual(await r.json(), { status: true });
			if (id !== "delivery-fail") {
				assert.deepEqual(mail(f), []);
				assert.equal(snapshot(f), before);
			} else {
				assert.equal(mail(f).length, 1);
				assert.equal(outcomes(f).at(-1).stage, "Committed");
				sensitive.push(mail(f)[0].token, mail(f)[0].url);
				assert.equal((await get(s, mail(f)[0].token)).status, 200);
			}
			record(`${id}-same-public-ack-no-delivery-claim`);
		} finally {
			held?.exec("ROLLBACK");
			held?.close();
			await s.stop();
		}
	}
	{
		const f = fresh("JSON-success"),
			s = await start(f, { verification: true });
		try {
			const m = await issue(f, s, { email: "u@example.invalid" });
			const r = await get(s, m.token);
			assert.equal(r.status, 200);
			assert.deepEqual(await r.json(), { status: true, user: null });
			record("no-callback-token-free-JSON-success");
		} finally {
			await s.stop();
		}
	}
	const malformed: [string, any, any, number][] = [
		["unknown-field", { email: "u@example.invalid", extra: true }, {}, 400],
		[
			"null-callback",
			{ email: "u@example.invalid", callbackURL: null },
			{},
			400,
		],
		[
			"duplicate-field",
			'{"email":"u@example.invalid","email":"missing@example.invalid"}',
			{},
			400,
		],
		["bad-email", { email: "invalid" }, {}, 400],
		[
			"bad-origin",
			{ email: "u@example.invalid" },
			{ origin: "https://evil.invalid" },
			403,
		],
		[
			"bad-content",
			{ email: "u@example.invalid" },
			{ "content-type": "text/plain" },
			415,
		],
		[
			"service",
			{ email: "u@example.invalid" },
			{ "x-rentemester-service-key": "invalid" },
			401,
		],
		[
			"unsafe-callback",
			{ email: "u@example.invalid", callbackURL: "//evil.invalid" },
			{},
			400,
		],
		[
			"bad-cookie",
			{ email: "u@example.invalid" },
			{ cookie: "better-auth.session_token=bad" },
			401,
		],
		[
			"oversize",
			JSON.stringify({
				email: "u@example.invalid",
				callbackURL: "/" + "a".repeat(9000),
			}),
			{},
			413,
		],
	];
	for (const [id, body, headers, status] of malformed) {
		const f = fresh(id),
			s = await start(f, { verification: true });
		try {
			const before = snapshot(f);
			assert.equal((await post(s, body, headers)).status, status, id);
			assert.equal(snapshot(f), before);
			assert.equal(outcomes(f).length, 0);
			record(`${id}-before-native-dispatch`);
		} finally {
			await s.stop();
		}
	}
	{
		const f = fresh("invalid-query"),
			s = await start(f, { verification: true });
		try {
			const m = await issue(f, s),
				before = snapshot(f);
			for (const suffix of [
				"&callbackURL=%FF",
				"&callbackURL=%",
				"&callbackURL=%2Flogin&callbackURL=%2Flogin",
				"&extra=x",
				"&callbackURL=%2F%2Fevil.invalid",
				"&callbackURL=https%3A%2F%2Fevil.invalid",
			]) {
				assert.equal(
					(
						await fetch(
							`${s.origin}/api/auth/verify-email?token=${m.token}${suffix}`,
							{ redirect: "manual" },
						)
					).status,
					400,
				);
				assert.equal(snapshot(f), before);
			}
			assert.equal(outcomes(f).length, 1);
			record("strict-query-duplicates-UTF8-escape-origin-before-mutation");
		} finally {
			await s.stop();
		}
	}
	{
		const f = fresh("legacy-unissued"),
			before = snapshot(f);
		for (const extra of [
			undefined,
			{
				purpose: "rentemester-email-verification-v1",
				workspace: "w",
				jti: randomBytes(32).toString("base64url"),
			},
		]) {
			const token = await createEmailVerificationToken(
				key,
				"u@example.invalid",
				undefined,
				3600,
				extra,
			);
			const claims = JSON.parse(
				Buffer.from(token.split(".")[1], "base64url").toString(),
			);
			assert.equal(claims.exp - claims.iat, 3600);
			// Installed signer samples wall time, not the harness's earlier fixed clock.
			// Admit a fresh server at this token's issuance instant so future-iat refusal
			// cannot masquerade as successful persisted-context enforcement.
			const s = await start(f, { verification: true, now: claims.iat * 1000 });
			try {
				sensitive.push(token);
				assert.equal((await get(s, token)).status, 401);
				assert.equal(snapshot(f), before);
			} finally {
				await s.stop();
			}
		}
		record(
			"actual-source-current-clock-stateless-and-signed-unissued-native-envelope-refused-no-legacy-fallback",
		);
	}

	{
		const f = fresh("expired"),
			s = await start(f, { verification: true });
		const m = await issue(f, s);
		await s.stop();
		const later = await start(f, { verification: true, now: now + 3600000 });
		try {
			const before = snapshot(f),
				r = await get(later, m.token, "/login");
			assert.equal(r.status, 302);
			assert.equal(
				r.headers.get("location"),
				`${later.origin}/login?error=INVALID_TOKEN`,
			);
			assert.equal(snapshot(f), before);
			record("expired-link-fixed-error-redirect-no-mutation");
		} finally {
			await later.stop();
		}
	}
	{
		const f = fresh("caller");
		const db = new Database(f.control);
		db.query(
			"INSERT INTO session(id,token,userId,createdAt,updatedAt,expiresAt) VALUES('caller','http-verification-session','u',?,?,?)",
		).run(
			new Date(now).toISOString(),
			new Date(now).toISOString(),
			new Date(now + 43200000).toISOString(),
		);
		db.close();
		const cookie = (
			await serializeSignedCookie(
				"better-auth.session_token",
				"http-verification-session",
				key,
			)
		).split(";")[0];
		sensitive.push(cookie);
		const s = await start(f, { verification: true });
		try {
			const m = await issue(f, s, undefined, { cookie });
			assert(m);
			record("actual-source-cookie-resolves-current-unverified-caller");
			const before = snapshot(f);
			for (const bad of [
				`better-auth.session_token; ${cookie}`,
				`${cookie}; __Secure-better-auth.session_token`,
			])
				assert.equal((await post(s, undefined, { cookie: bad })).status, 401);
			assert.equal(snapshot(f), before);
			record(
				"malformed-named-plus-valid-cookie-never-downgrades-or-dispatches",
			);
		} finally {
			await s.stop();
		}
	}
	for (const operation of ["request", "complete"]) {
		const f = fresh(`commit-${operation}`),
			s = await start(f, { verification: true }),
			m = operation === "complete" ? await issue(f, s) : undefined,
			reader = new Database(f.control);
		try {
			reader.exec("BEGIN;SELECT COUNT(*) FROM user");
			const before = snapshot(f);
			const r = operation === "request" ? await post(s) : await get(s, m.token);
			assert.equal(r.status, operation === "request" ? 200 : 503);
			assert.deepEqual(
				await r.json(),
				operation === "request"
					? { status: true }
					: { code: "VERIFICATION_UNCONFIRMED" },
			);
			assert.equal(outcomes(f).at(-1).stage, "OutcomeUnknown");
			assert.equal(mail(f).length, operation === "request" ? 0 : 1);
			reader.exec("ROLLBACK");
			assert.equal(snapshot(f), before);
			record(`${operation}-real-unknown-commit-no-success-or-reissue`);
		} finally {
			reader.close();
			await s.stop();
		}
	}
	{
		const f = fresh("rate"),
			s = await start(f, { verification: true });
		try {
			for (let i = 0; i < 4; i++) {
				const r = await post(
					s,
					{ email: "missing@example.invalid" },
					{ "x-forwarded-for": `192.0.2.${i}` },
				);
				assert.equal(r.status, i < 3 ? 200 : 429);
			}
			for (let i = 0; i < 11; i++) {
				const r = await get(s, `e30.e30.${i}`);
				assert.equal(r.status, i < 10 ? 401 : 429);
			}
			record(
				"direct-peer-fixed-operation-rates-ignore-forwarded-address-and-token-rotation",
			);
		} finally {
			await s.stop();
		}
	}
	const report = JSON.stringify(observations);
	for (const value of sensitive) assert(!report.includes(value));
}
