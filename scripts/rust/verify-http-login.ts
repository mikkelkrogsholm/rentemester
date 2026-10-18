import { verifyHttpServiceLifecycle } from "./service-lifecycle-cases";
import { verifyHttpServiceContext } from "./service-context-cases";
import { verifyHttpWorkspaceContext } from "./workspace-context-cases";
/** Real native HTTP enrolled login, actual control transactions, source crypto and cookie oracle. */
import assert from "node:assert/strict";
import { verifyHttpPasswordChange } from "./http-password-change-cases";
import { verifyHttpBackupManagement } from "./http-backup-management-cases";
import { verifyHttpSessions } from "./http-session-cases";
import { verifyHttpVerification } from "./http-verification-cases";
import { verifyHttpEnrollment } from "./http-enrollment-cases";
import { Database } from "bun:sqlite";
import { mkdtempSync, mkdirSync, copyFileSync, rmSync } from "node:fs";
import { tmpdir, homedir } from "node:os";
import { join, resolve } from "node:path";
import { createHmac } from "node:crypto";
import {
	openWorkspaceControlDb,
	workspaceControlPaths,
} from "../../src/core/workspace-control";
import { createBetterAuthRuntime } from "../../src/server/better-auth";
import { hashPassword } from "../../node_modules/better-auth/dist/crypto/password.mjs";
import { symmetricEncrypt } from "../../node_modules/better-auth/dist/crypto/index.mjs";
import { generateBackupCodes } from "../../node_modules/better-auth/dist/plugins/two-factor/backup-codes/index.mjs";
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
const work = mkdtempSync(join(tmpdir(), "rm-http-login-")),
	seed = join(work, "seed");
mkdirSync(seed);
const key = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
	secret = "12345678901234567890",
	password = "Synthetic password 123!",
	now = Date.now();
const keyset = { currentVersion: 1, keys: new Map([[1, key]]) };
const encrypted = await symmetricEncrypt({ key: keyset, data: secret }),
	passwordHash = await hashPassword(password);
const backups = await generateBackupCodes(keyset, {
	storeBackupCodes: "encrypted",
});
const db = openWorkspaceControlDb(seed);
db.exec(`PRAGMA wal_checkpoint(TRUNCATE);PRAGMA journal_mode=DELETE;
INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);
INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic');`);
db.query(
	"INSERT INTO account(id,userId,accountId,providerId,issuer,password,createdAt,updatedAt) VALUES('a','u','u','credential','local:credential',?,'2026-01-01','2026-01-01')",
).run(passwordHash);
db.query(
	"INSERT INTO twoFactor(id,userId,secret,backupCodes,verified,failedVerificationCount) VALUES('f','u',?,?,1,0)",
).run(encrypted, backups.encryptedBackupCodes);
db.close();
let checks = 0;
const observations: { id: string }[] = [];
function note(id: string) {
	checks++;
	observations.push({ id });
}
function fixture(id: string, mode: string) {
	const dir = join(work, id);
	mkdirSync(dir);
	const control = join(dir, "control.sqlite");
	copyFileSync(workspaceControlPaths(seed).db, control);
	const d = new Database(control);
	d.exec(`PRAGMA journal_mode=${mode}`);
	d.close();
	return { control };
}
function code() {
	const counter = Buffer.alloc(8);
	counter.writeBigUInt64BE(BigInt(Math.floor(now / 30000)));
	const h = createHmac("sha1", secret).update(counter).digest(),
		offset = h[h.length - 1]! & 15;
	return String((h.readUInt32BE(offset) & 0x7fffffff) % 1000000).padStart(
		6,
		"0",
	);
}
function sql(f: { control: string }, s: string) {
	const d = new Database(f.control);
	try {
		d.exec(s);
	} finally {
		d.close();
	}
}
function snapshot(f: { control: string }) {
	const d = new Database(f.control);
	try {
		return JSON.stringify(
			[
				"user",
				"account",
				"session",
				"verification",
				"twoFactor",
				"rm_workspace_user_access_events",
			].map((t) => d.query(`SELECT * FROM ${t} ORDER BY rowid`).all()),
		);
	} finally {
		d.close();
	}
}
async function start(f: any, extra: any = {}) {
	const p = Bun.spawn(
		[helper, "auth_http::tests::probe", "--exact", "--ignored", "--nocapture"],
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

try {
	for (const mode of ["DELETE", "WAL"]) {
		for (const factor of ["totp", "backup-code"]) {
			const f = fixture(`${mode}-${factor}`, mode),
				server = await start(f),
				jar = new Map<string, string>();
			async function request(
				path: string,
				body?: unknown,
				extra: Record<string, string> = {},
			) {
				const headers: Record<string, string> = { ...extra };
				if (jar.size) headers.cookie = [...jar.values()].join("; ");
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
				for (const c of cookies) {
					const pair = c.split(";")[0]!,
						name = pair.split("=")[0]!;
					if (c.includes("Max-Age=0")) jar.delete(name);
					else jar.set(name, pair);
				}
				assert.equal(r.headers.get("cache-control"), "no-store");
				return { status: r.status, data: await r.json(), cookies };
			}
			try {
				let r = await request("sign-in/email", {
					email: "u@example.invalid",
					password,
					rememberMe: false,
				});
				assert.equal(r.status, 200);
				assert.equal((r.data as any).twoFactorRedirect, true);
				assert.equal(jar.size, 1);
				assert(jar.has("better-auth.two_factor"));
				note(`${mode}/${factor}/pending-only`);
				r = await request("get-session");
				assert.equal(r.status, 200);
				assert.equal(r.data, null);
				assert.equal(r.cookies.length, 0);
				assert.equal(jar.size, 1);
				note(`${mode}/${factor}/pending-session-null`);
				const beforeMixed = snapshot(f);
				r = await request(
					"two-factor/verify-" + factor,
					{ code: factor === "totp" ? code() : backups.backupCodes[0] },
					{ "x-rentemester-service-key": "" },
				);
				assert.equal(r.status, 401);
				assert.equal(snapshot(f), beforeMixed);
				assert.equal(r.cookies.length, 0);
				note(`${mode}/${factor}/mixed-credentials-no-transaction`);
				r = await request("two-factor/verify-" + factor, {
					code: factor === "totp" ? code() : backups.backupCodes[0],
					trustDevice: false,
				});
				assert.equal(r.status, 200);
				assert.deepEqual(r.data, { status: true });
				assert.equal(jar.size, 1);
				assert(jar.has("better-auth.session_token"));
				assert(!r.cookies.at(-1)!.includes("Max-Age"));
				const sessionCookie = [...jar.values()].join("; ");
				note(`${mode}/${factor}/known-session-browser-cookie`);
				const before = snapshot(f);
				r = await request("get-session");
				assert.equal(r.status, 200);
				assert.equal((r.data as any).user.id, "u");
				assert.equal((r.data as any).session.userId, "u");
				assert.equal(
					(r.data as any).user.createdAt,
					"2026-01-01T00:00:00.000Z",
				);
				assert(!("token" in (r.data as any).session));
				assert.equal(snapshot(f), before);
				assert.equal(r.cookies.length, 0);
				note(`${mode}/${factor}/current-token-free-no-refresh`);
				// Same signature is accepted by the installed source auth runtime, with its development profile.
				const sourceDb = new Database(f.control);
				try {
					const auth = createBetterAuthRuntime(sourceDb, {
						secret: key,
						trustedOrigins: [server.origin],
						baseURL: server.origin,
						useSecureCookies: false,
					});
					const view = await auth.api.getSession({
						headers: new Headers({ cookie: sessionCookie }),
					});
					assert.equal(view?.user.id, "u");
				} finally {
					sourceDb.close();
				}
				note(`${mode}/${factor}/source-reader-accepts-native-cookie`);
				sql(f, "UPDATE user SET name=char(10) WHERE id='u'");
				const malformed = snapshot(f);
				r = await request("get-session");
				assert.equal(r.status, 503);
				assert.equal(r.cookies.length, 0);
				assert.equal(snapshot(f), malformed);
				note(`${mode}/${factor}/malformed-metadata-no-cleanup`);
				sql(f, "UPDATE user SET name='Synthetic' WHERE id='u'");
				const held = new Database(f.control);
				held.exec("BEGIN IMMEDIATE");
				try {
					r = await request("sign-out", {});
					assert.equal(r.status, 503);
					assert.equal(r.cookies.length, 0);
					assert(jar.has("better-auth.session_token"));
				} finally {
					held.exec("ROLLBACK");
					held.close();
				}
				note(`${mode}/${factor}/busy-logout-no-revocation-claim`);
				sql(
					f,
					"INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','disable',NULL,'agent:synthetic')",
				);
				const revoked = snapshot(f);
				r = await request("get-session");
				assert.equal(r.status, 200);
				assert.equal(r.data, null);
				assert.equal(snapshot(f), revoked);
				note(`${mode}/${factor}/current-access-revocation`);
				r = await request("sign-out", {});
				assert.equal(r.status, 200);
				assert.deepEqual(r.data, { success: true });
				assert.equal(jar.size, 0);
				const d = new Database(f.control);
				assert.equal(
					(d.query("SELECT count(*) n FROM session").get() as any).n,
					0,
				);
				d.close();
				note(`${mode}/${factor}/disabled-access-still-signs-out`);
				r = await request("get-session");
				assert.equal(r.status, 200);
				assert.equal(r.data, null);
				note(`${mode}/${factor}/logout-session-null`);
			} finally {
				await server.stop();
			}
		}
	}
	await verifyHttpEnrollment({ fixture, start, key, now, password, note });
	await verifyHttpServiceLifecycle({fixture,start,note});
	await verifyHttpServiceContext({fixture,start,note});
	await verifyHttpWorkspaceContext({fixture,start,key,now,note});
	await verifyHttpPasswordChange({ fixture,start,key,now,note });
	await verifyHttpBackupManagement({ fixture, start, key, now, note });
	await verifyHttpSessions({ fixture, start, key, now, note });
	await verifyHttpVerification({ fixture, start, key, now, note });
	console.log(JSON.stringify({ ok: true, checks, observations }));
} finally {
	rmSync(work, { recursive: true, force: true });
}
