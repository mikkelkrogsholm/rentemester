/** Real native HTTP + admitted synthetic control transaction + postcommit mail; no product writer. */
import assert from "node:assert/strict";
import { Database } from "bun:sqlite";
import {
	mkdtempSync,
	mkdirSync,
	copyFileSync,
	readFileSync,
	existsSync,
	rmSync,
} from "node:fs";
import { tmpdir, homedir } from "node:os";
import { join, resolve } from "node:path";
import {
	openWorkspaceControlDb,
	workspaceControlPaths,
} from "../../src/core/workspace-control";
import { createBetterAuthRuntime } from "../../src/server/better-auth";
const root = resolve(import.meta.dir, "../.."),
	cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const build = Bun.spawnSync(
	[
		cargo,
		"test",
		"--locked",
		"--release",
		"-p",
		"rentemester-server",
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
		(v) => v.executable && v.target.name === "rentemester_server",
	)?.executable;
assert(helper);
const work = mkdtempSync(join(tmpdir(), "rm-http-recovery-")),
	seed = join(work, "seed");
mkdirSync(seed);
const key = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
	now = Date.now();
const db = openWorkspaceControlDb(seed);
db.exec(`PRAGMA wal_checkpoint(TRUNCATE);PRAGMA journal_mode=DELETE;
INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);
INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic');`);
db.close();
let checks = 0;
const observations: any[] = [];
const note = (id: string) => {
	checks++;
	observations.push({ id });
};
const privateValues = [
	key,
	"u@example.invalid",
	"missing@example.invalid",
	"synthetic private mail failure",
];
function fixture(id: string, mode = "delete") {
	const p = join(work, id);
	mkdirSync(p);
	const control = join(p, "control.sqlite");
	copyFileSync(workspaceControlPaths(seed).db, control);
	const d = new Database(control);
	d.exec(`PRAGMA journal_mode=${mode}`);
	d.close();
	return {
		control,
		mailLog: join(p, "mail.jsonl"),
		completionLog: join(p, "completion.jsonl"),
	};
}
async function start(f: any, extra: any = {}) {
	const p = Bun.spawn(
		[helper, "tests::probe", "--exact", "--ignored", "--nocapture"],
		{ cwd: root, stdin: "pipe", stdout: "pipe", stderr: "pipe" },
	);
	const stderr = new Response(p.stderr).text();
	p.stdin.write(JSON.stringify({ ...f, key, now, ...extra }) + "\n");
	const reader = p.stdout.getReader();
	let text = "",
		origin = "";
	const timer = setTimeout(() => p.kill(), 60000);
	try {
		while (!origin) {
			const v = await reader.read();
			assert(!v.done, "helper stopped before binding");
			text += new TextDecoder().decode(v.value);
			for (const l of text.split("\n")) {
				if (l.startsWith('{"origin":')) origin = JSON.parse(l).origin;
			}
		}
	} catch (e) {
		clearTimeout(timer);
		p.kill();
		throw e;
	}
	return {
		origin,
		async stop() {
			p.stdin.write("\n");
			p.stdin.end();
			while (!(await reader.read()).done) {}
			assert.equal(await p.exited, 0);
			clearTimeout(timer);
			assert.equal(await stderr, "");
		},
		kill() {
			clearTimeout(timer);
			p.kill();
		},
	};
}
function rows(f: any, table: string) {
	const d = new Database(f.control);
	try {
		return d.query(`SELECT * FROM ${table} ORDER BY rowid`).all() as any[];
	} finally {
		d.close();
	}
}
const sent = (f: any) =>
	existsSync(f.mailLog)
		? readFileSync(f.mailLog, "utf8")
				.trim()
				.split("\n")
				.map((s) => JSON.parse(s))
		: [];
async function post(server: any, path: string, body: any) {
	return fetch(`${server.origin}/api/auth/${path}`, {
		method: "POST",
		headers: { origin: server.origin, "content-type": "application/json" },
		body: JSON.stringify(body),
	});
}
async function issue(server: any, f: any) {
	const before = sent(f).length;
	const r = await post(server, "request-password-reset", {
		email: "u@example.invalid",
		redirectTo: "/reset-password",
	});
	assert.equal(r.status, 200);
	const m = sent(f)[before];
	assert(m);
	privateValues.push(m.token, m.url);
	return m;
}
function update(f: any, sql: string, args: any[] = []) {
	const d = new Database(f.control);
	try {
		d.query(sql).run(...args);
	} finally {
		d.close();
	}
}
async function callback(server: any, m: any) {
	return fetch(m.url, {
		headers: { "sec-fetch-site": "cross-site" },
		redirect: "manual",
	});
}
const password = "HTTP replacement password 123!";
privateValues.push(password);
try {
	const sourceFixture = fixture("source-oracle"),
		sdb = new Database(sourceFixture.control),
		source = createBetterAuthRuntime(sdb, {
			secret: key,
			trustedOrigins: ["https://example.invalid"],
			baseURL: "https://example.invalid",
			useSecureCookies: true,
			emailSender: { async send() {} },
		});
	const sourceCall = (path: string, body: any) =>
		source.handler(
			new Request(`https://example.invalid/api/auth/${path}`, {
				method: "POST",
				headers: {
					origin: "https://example.invalid",
					"content-type": "application/json",
				},
				body: JSON.stringify(body),
			}),
		);
	const missing = await sourceCall("reset-password", {
		token: "X".repeat(24),
		newPassword: password,
	});
	assert.equal(missing.status, 400);
	const invalidBody = await missing.json();
	assert.equal(invalidBody.code, "INVALID_TOKEN");
	note("actual-source-invalid-token-status-code-body");
	for (const [pw, code] of [
		["x".repeat(11), "PASSWORD_TOO_SHORT"],
		["😀".repeat(65), "PASSWORD_TOO_LONG"],
	]) {
		const r = await sourceCall("reset-password", {
			token: "X".repeat(24),
			newPassword: pw,
		});
		assert.equal(r.status, 400);
		assert.equal((await r.json()).code, code);
		note(`actual-source-${code}`);
	}
	for (const mode of ["wal", "delete"]) {
		const f = fixture(`${mode}-flow`, mode),
			server = await start(f);
		try {
			const chosenPassword = mode === "wal" ? "😀".repeat(6) : password;
			privateValues.push(chosenPassword);
			const m = await issue(server, f),
				sibling = await issue(server, f),
				before = rows(f, "verification"),
				users = rows(f, "user");
			for (let n = 0; n < 2; n++) {
				const r = await callback(server, m);
				assert.equal(r.status, 302);
				assert.equal(
					r.headers.get("location"),
					`${server.origin}/reset-password?token=${m.token}`,
				);
				assert.equal(r.headers.get("cache-control"), "no-store");
				assert.equal(r.headers.get("referrer-policy"), "no-referrer");
				assert.equal(r.headers.get("set-cookie"), null);
				assert.deepEqual(rows(f, "verification"), before);
			}
			note(`${mode}-mail-navigation-repeat-preview-nonconsuming`);
			const sr = await source.handler(
				new Request(
					`https://example.invalid/api/auth/reset-password/${m.token}?callbackURL=%2Freset-password`,
				),
			);
			assert.equal(sr.status, 302);
			assert.equal(
				sr.headers.get("location"),
				`https://example.invalid/reset-password?error=INVALID_TOKEN`,
			);
			note(`${mode}-actual-source-invalid-callback-redirect`);
			update(
				f,
				"INSERT INTO session(id,token,userId,createdAt,updatedAt,expiresAt) VALUES('http-session','synthetic-http-session','u',?,?,?)",
				[
					new Date(now).toISOString(),
					new Date(now).toISOString(),
					new Date(now + 3600000).toISOString(),
				],
			);
			const bad = await post(server, "reset-password", {
				token: m.token,
				newPassword: "x".repeat(11),
			});
			assert.equal(bad.status, 400);
			assert.equal((await bad.json()).code, "PASSWORD_TOO_SHORT");
			assert.deepEqual(rows(f, "verification"), before);
			assert.equal(rows(f, "session").length, 1);
			note(`${mode}-invalid-password-no-consume-or-session-change`);
			const hostileBefore = rows(f, "verification");
			const hostile = await fetch(`${server.origin}/api/auth/reset-password`, {
				method: "POST",
				headers: {
					origin: "https://evil.invalid",
					"content-type": "application/json",
				},
				body: JSON.stringify({ token: m.token, newPassword: chosenPassword }),
			});
			assert.equal(hostile.status, 403);
			assert.deepEqual(rows(f, "verification"), hostileBefore);
			assert.equal(rows(f, "account").length, 0);
			note(`${mode}-hostile-completion-no-effect`);
			const r = await post(server, "reset-password", {
				token: m.token,
				newPassword: chosenPassword,
			});
			assert.equal(r.status, 200);
			assert.deepEqual(await r.json(), { status: true });
			assert.equal(r.headers.get("set-cookie"), null);
			assert.equal(rows(f, "verification").length, 0);
			assert.equal(rows(f, "session").length, 0);
			assert.deepEqual(rows(f, "user"), users);
			const account = rows(f, "account")[0];
			assert.equal(account.userId, "u");
			const { verifyPassword } = await import(
				"../../node_modules/better-auth/dist/crypto/password.mjs"
			);
			assert.equal(
				await verifyPassword({
					hash: account.password,
					password: chosenPassword,
				}),
				true,
			);
			note(
				`${mode}-native-completion-credential-source-verifiable-siblings-and-sessions-invalidated`,
			);
			const replay = await post(server, "reset-password", {
				token: sibling.token,
				newPassword: password,
			});
			assert.equal(replay.status, 400);
			assert.deepEqual(await replay.json(), invalidBody);
			assert.equal(rows(f, "account")[0].password, account.password);
			note(`${mode}-sibling-replay-public-invalid-token`);
			const used = await callback(server, m);
			assert.equal(used.status, 302);
			assert.equal(
				used.headers.get("location"),
				`${server.origin}/reset-password?error=INVALID_TOKEN`,
			);
			note(`${mode}-consumed-callback-invalid-without-token-leak`);
			await server.stop();
		} finally {
			server.kill();
		}
	}
	// Native-issued current link is accepted by actual source callback and completion.
	const interop = fixture("source-interop"),
		is = await start(interop);
	try {
		const m = await issue(is, interop),
			idb = new Database(interop.control),
			auth = createBetterAuthRuntime(idb, {
				secret: key,
				trustedOrigins: ["https://example.invalid"],
				baseURL: "https://example.invalid",
				useSecureCookies: true,
				emailSender: { async send() {} },
			});
		const r = await auth.handler(
			new Request(
				`https://example.invalid/api/auth/reset-password/${m.token}?callbackURL=%2Freset-password`,
			),
		);
		assert.equal(r.status, 302);
		assert.equal(
			r.headers.get("location"),
			`https://example.invalid/reset-password?token=${m.token}`,
		);
		note("actual-source-accepts-native-http-issued-callback");
		const done = await auth.handler(
			new Request("https://example.invalid/api/auth/reset-password", {
				method: "POST",
				headers: {
					origin: "https://example.invalid",
					"content-type": "application/json",
				},
				body: JSON.stringify({ token: m.token, newPassword: password }),
			}),
		);
		assert.equal(done.status, 200);
		assert.deepEqual(await done.json(), { status: true });
		note("actual-source-completes-native-http-issued-link");
		idb.close();
		await is.stop();
	} finally {
		is.kill();
	}
	for (const mode of ["wal", "delete"]) {
		const f = fixture(`${mode}-race`, mode),
			s = await start(f);
		try {
			const m = await issue(s, f);
			const results = await Promise.all([
				post(s, "reset-password", { token: m.token, newPassword: password }),
				post(s, "reset-password", { token: m.token, newPassword: password }),
			]);
			assert.equal(results.filter((r) => r.status === 200).length, 1);
			assert(results.every((r) => [200, 400, 503].includes(r.status)));
			assert.equal(rows(f, "account").length, 1);
			assert.equal(rows(f, "verification").length, 0);
			note(`${mode}-concurrent-http-token-one-success`);
			await s.stop();
		} finally {
			s.kill();
		}
	}
	const busy = fixture("commit-busy", "delete"),
		bs = await start(busy);
	try {
		const m = await issue(bs, busy),
			before = rows(busy, "verification"),
			reader = new Database(busy.control);
		reader.exec("BEGIN");
		reader.query("SELECT * FROM user").all();
		try {
			const r = await post(bs, "reset-password", {
				token: m.token,
				newPassword: password,
			});
			assert.equal(r.status, 503);
			assert.equal(r.headers.get("retry-after"), null);
			assert.deepEqual(await r.json(), {
				message: "Reset outcome could not be confirmed",
				code: "RESET_UNCONFIRMED",
			});
		} finally {
			reader.exec("ROLLBACK");
			reader.close();
		}
		const outcomes = readFileSync(busy.completionLog, "utf8")
			.trim()
			.split("\n")
			.map((s) => JSON.parse(s));
		assert.deepEqual(outcomes, [
			{ ok: false, stage: "OutcomeUnknown", code: "commit_outcome_unknown" },
		]);
		assert.deepEqual(rows(busy, "verification"), before);
		assert.equal(rows(busy, "account").length, 0);
		note("actual-delete-commit-busy-is-unconfirmed-no-retry-claim");
		await bs.stop();
	} finally {
		bs.kill();
	}
	for (const kind of ["expiry", "orphan"]) {
		const f = fixture(kind),
			initial = await start(f);
		let m: any;
		try {
			m = await issue(initial, f);
			await initial.stop();
		} finally {
			initial.kill();
		}
		if (kind === "orphan")
			update(
				f,
				"UPDATE verification SET value='missing-subject' WHERE identifier=?",
				[`reset-password:${m.token}`],
			);
		const before = rows(f, "verification"),
			server = await start(f, { now: kind === "expiry" ? now + 3600000 : now });
		try {
			const r = await fetch(
				`${server.origin}/api/auth/reset-password/${m.token}?callbackURL=%2Freset-password`,
				{ redirect: "manual" },
			);
			assert.equal(r.status, 302);
			assert.equal(
				r.headers.get("location"),
				`${server.origin}/reset-password?error=INVALID_TOKEN`,
			);
			assert.deepEqual(rows(f, "verification"), before);
			note(`${kind}-callback-invalid-nonconsuming`);
			const done = await post(server, "reset-password", {
				token: m.token,
				newPassword: password,
			});
			assert.equal(done.status, 400);
			assert.deepEqual(await done.json(), invalidBody);
			assert.deepEqual(rows(f, "verification"), before);
			assert.equal(rows(f, "account").length, 0);
			note(`${kind}-completion-invalid-without-state-change`);
			await server.stop();
		} finally {
			server.kill();
		}
	}

	sdb.close();
	const report = JSON.stringify(
		{
			ok: true,
			scope:
				"Native real HTTP recovery callback/completion and current synthetic WAL/DELETE controls with actual source oracles",
			checks,
			observations,
			limits: [
				"Trusted synthetic fixture executor only; no actual physical/workspace/legacy writer admission or product data opener",
				"No hosted TLS/global-persisted rate/socket/header/shutdown qualification, login/MFA/provider/UI deployment or full R41/R42 completion",
				"Equal output does not prove constant-time privacy; unknown outcome remains unconfirmed and is never automatically retried",
			],
		},
		null,
		2,
	);
	for (const value of privateValues) assert(!report.includes(value));
	console.log(report);
} finally {
	rmSync(work, { recursive: true, force: true });
}
