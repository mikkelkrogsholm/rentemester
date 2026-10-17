/** Installed Better Auth oracle + native credential/fence processes on synthetic stores. */
import assert from "node:assert/strict";
import {
	copyFileSync,
	existsSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { homedir, tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { Database } from "bun:sqlite";
import {
	openWorkspaceControlDb,
	workspaceControlPaths,
} from "../../src/core/workspace-control";
import { createBetterAuthRuntime } from "../../src/server/better-auth";
import { serializeSignedCookie } from "../../node_modules/better-call/dist/cookies.mjs";
const root = resolve(import.meta.dir, "../.."),
	cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const build = Bun.spawnSync(
	[
		cargo,
		"test",
		"--locked",
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
const work = mkdtempSync(join(tmpdir(), "rm-session-cookie-"));
const corpus = JSON.parse(
	readFileSync(
		join(root, "crates/rentemester-runtime/fixtures/session-cookie.json"),
		"utf8",
	),
);
const key = corpus.key,
	cookie = corpus.vectors.find((v: any) => v.id === "valid-0").header;
const now = 1767225600000,
	created = "2026-01-01T00:00:00.000Z",
	expires = "2026-01-01T12:00:00.000Z";
const hash = (b: Buffer) => createHash("sha256").update(b).digest("hex");
let checks = 0;
const observations: any[] = [];
const seed = join(work, "seed");
mkdirSync(seed);
const seedDb = openWorkspaceControlDb(seed);
seedDb.exec(`PRAGMA wal_checkpoint(TRUNCATE);PRAGMA journal_mode=DELETE;
INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);
INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic');
INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','bookkeeper','agent:synthetic');`);
seedDb
	.query(
		"INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt) VALUES(?,?,?,?,?,?)",
	)
	.run("s", "u", "synthetic-token", created, created, expires);
seedDb.close();
function fixture(id: string, mode: string, sql = "") {
	const dir = join(work, id);
	mkdirSync(dir);
	mkdirSync(join(dir, ".rentemester"));
	const c = workspaceControlPaths(dir).db,
		l = join(dir, "effect.sqlite");
	copyFileSync(workspaceControlPaths(seed).db, c);
	const db = new Database(c);
	db.exec(`PRAGMA foreign_keys=ON;PRAGMA journal_mode=${mode};`);
	if (sql) db.exec(sql);
	db.close();
	const ledger = new Database(l);
	ledger.exec(
		`PRAGMA journal_mode=${mode};PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;CREATE TABLE sequences(kind TEXT,scope TEXT,value INTEGER,PRIMARY KEY(kind,scope));CREATE TABLE companion(n INTEGER PRIMARY KEY CHECK(n>0));`,
	);
	ledger.close();
	return {
		dir,
		c,
		l,
		output: join(dir, "output.json"),
		marker: join(dir, "marker"),
	};
}
function start(f: ReturnType<typeof fixture>, request: any = {}, point = "") {
	return Bun.spawn(
		[
			helper,
			"--ignored",
			"--exact",
			"control_fence::tests::cookie_child",
			"--nocapture",
		],
		{
			cwd: work,
			env: {
				...process.env,
				TZ: "Pacific/Honolulu",
				RENTEMESTER_ACTOR: "user:forged",
				RM_FENCE_CONTROL: f.c,
				RM_FENCE_COMPANY: f.l,
				RM_FENCE_REQUEST: JSON.stringify({ key, cookie, now, ...request }),
				RM_FENCE_OUTPUT: f.output,
				RM_FENCE_POINT: point,
				RM_FENCE_MARKER: f.marker,
			},
			stdin: "pipe",
			stdout: "pipe",
			stderr: "pipe",
		},
	);
}
async function done(
	f: ReturnType<typeof fixture>,
	child: ReturnType<typeof start>,
) {
	const exit = await child.exited;
	const output = await new Response(child.stdout).text(),
		err = await new Response(child.stderr).text();
	assert.equal(exit, 0, output + err);
	// Diagnostics must never contain synthetic bearer/key/header bytes.
	for (const value of [key, cookie, "synthetic-token"]) {
		assert(!output.includes(value));
		assert(!err.includes(value));
	}
	return JSON.parse(readFileSync(f.output, "utf8"));
}
function effectCount(f: ReturnType<typeof fixture>) {
	const db = new Database(f.l);
	const n = (db.query("SELECT count(*) n FROM companion").get() as any).n;
	const seq = db.query("SELECT value FROM sequences").all();
	assert.deepEqual(seq, n ? [{ value: 1 }] : []);
	db.close();
	return n;
}
function image(f: ReturnType<typeof fixture>) {
	const db = new Database(f.c);
	const value = hash(db.serialize());
	db.close();
	return value;
}
async function marker(
	f: ReturnType<typeof fixture>,
	child: ReturnType<typeof start>,
) {
	const deadline = Date.now() + 20000;
	while (!existsSync(f.marker)) {
		assert.equal(child.exitCode, null);
		assert(Date.now() < deadline);
		await Bun.sleep(10);
	}
}
try {
	const cases = [
		{ id: "current", allowed: true },
		{ id: "fresh", allowed: true, request: { fresh: true, now: now + 599999 } },
		{ id: "fresh-boundary", request: { fresh: true, now: now + 600000 } },
		{ id: "fresh-after", request: { fresh: true, now: now + 600001 } },
		{
			id: "current-still-valid",
			allowed: true,
			request: { now: now + 600000 },
		},
		{
			id: "expiry-before",
			allowed: true,
			request: { now: now + 43200000 - 1 },
		},
		{ id: "expiry-equal", request: { now: now + 43200000 } },
		{ id: "expiry-after", request: { now: now + 43200001 } },
		{ id: "future-created", request: { now: now - 1 } },
		{ id: "negative-clock", request: { now: -1 } },
		{ id: "out-of-range-clock", request: { now: 253402300800000 } },
		{
			id: "retokenized",
			sql: "UPDATE session SET token='replacement' WHERE id='s'",
		},
		{ id: "deleted", sql: "DELETE FROM session" },
		{ id: "blank-session-id", sql: "UPDATE session SET id=' '" },
		{
			id: "legacy-coerced-date",
			sql: "UPDATE session SET createdAt='2026-01-01'",
		},
		{
			id: "invalid-gregorian",
			sql: "UPDATE session SET createdAt='2026-02-29T00:00:00.000Z'",
		},
		{
			id: "invalid-time",
			sql: "UPDATE session SET createdAt='2026-01-01T00:00:60.000Z'",
		},
		{
			id: "24h-refused",
			sql: "UPDATE session SET expiresAt='2026-01-02T00:00:00.000Z'",
		},
		{ id: "zero-lifetime", sql: "UPDATE session SET expiresAt=createdAt" },
		{ id: "integer-time", sql: "UPDATE session SET createdAt=1767225600000" },
		{
			id: "mfa-disabled",
			sql: "UPDATE user SET twoFactorEnabled=0",
			code: "access_denied",
		},
		{
			id: "email-unverified",
			sql: "UPDATE user SET emailVerified=0",
			code: "access_denied",
		},
		{
			id: "revoked",
			sql: "INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','revoke',NULL,'agent:synthetic')",
			code: "access_denied",
		},
		{ id: "service-invalid", request: { service: "invalid" } },
		{ id: "service-empty", request: { service: "" } },
		{
			id: "signature-tamper",
			request: { cookie: cookie.replace("synthetic", "synthetix") },
		},
	];
	for (const mode of ["DELETE", "WAL"]) {
		for (const test of cases) {
			const f = fixture(mode + "-" + test.id, mode, test.sql),
				before = image(f);
			const result = await done(f, start(f, test.request));
			assert.equal(
				result.outcome,
				test.allowed ? "committed" : "not_committed",
				test.id,
			);
			if (!test.allowed)
				assert.equal(result.code, test.code ?? "invalid_credentials", test.id);
			assert.equal(effectCount(f), test.allowed ? 1 : 0);
			assert.equal(image(f), before);
			checks++;
			observations.push({ id: test.id, mode, outcome: result.outcome });
		}
		for (const mutation of [
			"UPDATE session SET token='replacement' WHERE id='s'",
			"DELETE FROM session WHERE id='s'",
		]) {
			// MAC already verified, but token is not resolved until the control lock is held.
			const f = fixture(mode + "-before-" + checks, mode),
				child = start(f, {}, "cookie_verified");
			await marker(f, child);
			const db = new Database(f.c);
			db.exec(mutation);
			db.close();
			child.stdin.write("continue\n");
			child.stdin.end();
			const result = await done(f, child);
			assert.equal(result.outcome, "not_committed");
			assert.equal(result.code, "invalid_credentials");
			assert.equal(effectCount(f), 0);
			checks++;
			const held = fixture(mode + "-held-" + checks, mode),
				writer = start(held, {}, "authorized");
			await marker(held, writer);
			const peer = new Database(held.c);
			peer.exec("PRAGMA busy_timeout=0");
			assert.throws(() => peer.exec(mutation), /locked|busy/i);
			writer.stdin.write("continue\n");
			writer.stdin.end();
			assert.equal((await done(held, writer)).outcome, "committed");
			assert.equal(effectCount(held), 1);
			peer.exec(mutation);
			peer.close();
			assert.equal((await done(held, start(held))).outcome, "not_committed");
			assert.equal(effectCount(held), 1);
			checks++;
		}
	}
	// Actual framework issuance and authentication, not a copied MAC/session implementation.
	const f = fixture("framework", "DELETE"),
		db = new Database(f.c);
	const opts = {
		trustedOrigins: ["https://synthetic.example.test"],
		baseURL: "https://synthetic.example.test",
		useSecureCookies: true,
	};
	const auth = createBetterAuthRuntime(db, { ...opts, secret: key });
	const ctx = await auth.$context;
	const session = await ctx.internalAdapter.createSession("u", false);
	const issued = (
		await serializeSignedCookie(
			ctx.authCookies.sessionToken.name,
			session.token,
			ctx.secret,
			{ secure: true },
		)
	).split(";")[0]!;
	const accepted = await auth.api.getSession({
		headers: new Headers({ cookie: issued }),
	});
	assert(accepted);
	assert.equal(accepted.user.id, "u");
	assert.equal(accepted.session.id, session.id);
	assert.equal(
		(
			db
				.query("SELECT typeof(createdAt) t FROM session WHERE id=?")
				.get(session.id) as any
		).t,
		"text",
	);
	assert.equal(
		(
			await done(
				f,
				start(f, {
					cookie: issued,
					now: Date.now(),
					expectedSubject: accepted.user.id,
					expectedSession: accepted.session.id,
				}),
			)
		).outcome,
		"committed",
	);
	checks++;
	const nextKey = Buffer.alloc(32, 2).toString("base64url");
	const rotated = createBetterAuthRuntime(db, {
		...opts,
		secret: nextKey,
		secrets: [
			{ version: 2, value: nextKey },
			{ version: 1, value: key },
		],
		legacySecret: key,
	});
	assert.equal(
		await rotated.api.getSession({ headers: new Headers({ cookie: issued }) }),
		null,
	);
	assert.equal(
		(await done(f, start(f, { cookie: issued, key: nextKey, now: Date.now() })))
			.outcome,
		"not_committed",
	);
	checks++;
	const day = await ctx.internalAdapter.createSession("u", true);
	const dayCookie = (
		await serializeSignedCookie(
			ctx.authCookies.sessionToken.name,
			day.token,
			ctx.secret,
			{ secure: true },
		)
	).split(";")[0]!;
	assert(
		await auth.api.getSession({ headers: new Headers({ cookie: dayCookie }) }),
	);
	assert.equal(
		(await done(f, start(f, { cookie: dayCookie, now: Date.now() }))).outcome,
		"not_committed",
	);
	checks++;
	// Source's first-duplicate and lenient base64 pad bits are intentional native refusals.
	assert(
		await auth.api.getSession({
			headers: new Headers({ cookie: issued + "; " + issued }),
		}),
	);
	assert.equal(
		(
			await done(
				f,
				start(f, { cookie: issued + "; " + issued, now: Date.now() }),
			)
		).outcome,
		"not_committed",
	);
	checks++;
	// Native deliberately rejects equivalent noncanonical signature spellings.
	const wire = decodeURIComponent(issued.slice(issued.indexOf("=") + 1));
	const dot = wire.lastIndexOf(".");
	const tag = wire.slice(dot + 1),
		alphabet =
			"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
	const alias =
		tag.slice(0, -2) + alphabet[alphabet.indexOf(tag.at(-2)!) ^ 1] + "=";
	const aliasCookie =
		issued.slice(0, issued.indexOf("=") + 1) +
		encodeURIComponent(wire.slice(0, dot + 1) + alias);
	assert(
		await auth.api.getSession({
			headers: new Headers({ cookie: aliasCookie }),
		}),
	);
	assert.equal(
		(await done(f, start(f, { cookie: aliasCookie, now: Date.now() }))).outcome,
		"not_committed",
	);
	checks++;
	// Freeze only this isolated oracle's global clock to prove the exact expiry equality difference.
	const RealDate = globalThis.Date,
		frozen = RealDate.now();
	db.query("UPDATE session SET createdAt=?,expiresAt=? WHERE id=?").run(
		new RealDate(frozen - 3600000).toISOString(),
		new RealDate(frozen).toISOString(),
		session.id,
	);
	globalThis.Date = new Proxy(RealDate, {
		construct(target, args, newTarget) {
			return Reflect.construct(
				target,
				args.length ? args : [frozen],
				newTarget,
			);
		},
		get(target, property, receiver) {
			return property === "now"
				? () => frozen
				: Reflect.get(target, property, receiver);
		},
	});
	try {
		assert(
			await auth.api.getSession({ headers: new Headers({ cookie: issued }) }),
		);
	} finally {
		globalThis.Date = RealDate;
	}
	assert.equal(
		(await done(f, start(f, { cookie: issued, now: frozen }))).outcome,
		"not_committed",
	);
	checks++;
	db.close();
	console.log(
		JSON.stringify(
			{
				ok: true,
				checks,
				wireVectors: corpus.vectors.length,
				scope:
					"active-key MAC, locked exact session/time/access policy, actual framework internal session issuance and getSession, before/held token-update/deletion races",
				observations,
				limits:
					"No native login/MFA/reset/service issuance or product endpoint. Authentication/freshness are admission-time checks. Native parser,12h lifetime and exact time boundaries deliberately fail closed more strictly than legacy.",
			},
			null,
			2,
		),
	);
} finally {
	rmSync(work, { recursive: true, force: true });
}
