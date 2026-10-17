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
const work = mkdtempSync(join(tmpdir(), "rm-http-reset-")),
	seed = join(work, "seed");
mkdirSync(seed);
const key = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
	now = 1767225600000;
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
	return { control, mailLog: join(p, "mail.jsonl") };
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
	const timer = setTimeout(() => p.kill(), 15000);
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
try {
	const sourceFixture = fixture("source"),
		sdb = new Database(sourceFixture.control);
	const source = createBetterAuthRuntime(sdb, {
		secret: key,
		trustedOrigins: ["https://example.invalid"],
		baseURL: "https://example.invalid",
		useSecureCookies: true,
		emailSender: {
			async send() {
				throw new Error("synthetic private mail failure");
			},
		},
	});
	const expected = await source.handler(
		new Request("https://example.invalid/api/auth/request-password-reset", {
			method: "POST",
			headers: {
				origin: "https://example.invalid",
				"content-type": "application/json",
			},
			body: JSON.stringify({
				email: "u@example.invalid",
				redirectTo: "/reset-password",
			}),
		}),
	);
	assert.equal(expected.status, 200);
	const ack = await expected.text();
	sdb.close();
	note("actual-source-committed-mail-failure-acknowledgement");
	for (const mode of ["wal", "delete"])
		for (const scenario of ["known", "unknown", "mail-failure"]) {
			const f = fixture(`${mode}-${scenario}`, mode),
				server = await start(f, { failMail: scenario === "mail-failure" });
			try {
				const before = rows(f, "user"),
					email =
						scenario === "unknown"
							? "missing@example.invalid"
							: "U@EXAMPLE.INVALID";
				const r = await fetch(
					`${server.origin}/api/auth/request-password-reset`,
					{
						method: "POST",
						headers: {
							origin: server.origin,
							"content-type": "application/json",
						},
						body: JSON.stringify({ email, redirectTo: "/reset-password" }),
					},
				);
				assert.equal(r.status, 200);
				assert.equal(await r.text(), ack);
				assert.equal(r.headers.get("cache-control"), "no-store");
				assert.equal(r.headers.get("referrer-policy"), "no-referrer");
				assert.equal(r.headers.get("set-cookie"), null);
				note(`${mode}-${scenario}-identical-source-public-ack`);
				const links = rows(f, "verification"),
					mail = sent(f);
				assert.equal(links.length, scenario === "unknown" ? 0 : 1);
				assert.equal(mail.length, scenario === "unknown" ? 0 : 1);
				for (const m of mail) {
					assert.equal(m.recipient, "u@example.invalid");
					assert.match(m.token, /^[A-Za-z0-9]{24}$/);
					assert.equal(links[0].identifier, `reset-password:${m.token}`);
					assert.equal(
						m.url,
						`${server.origin}/api/auth/reset-password/${m.token}?callbackURL=%2Freset-password`,
					);
					privateValues.push(m.token, m.url);
				}
				assert.deepEqual(rows(f, "user"), before);
				assert.equal(
					rows(f, "rm_workspace_auth_telemetry_events").filter(
						(v) => v.endpoint === "request-password-reset",
					).length,
					1,
				);
				note(`${mode}-${scenario}-native-commit-unlocked-mail-and-state`);
				const blocked = await fetch(
					`${server.origin}/api/auth/request-password-reset`,
					{
						method: "POST",
						headers: {
							origin: "https://evil.invalid",
							"content-type": "application/json",
						},
						body: JSON.stringify({ email: "u@example.invalid" }),
					},
				);
				assert.equal(blocked.status, 403);
				assert.equal(rows(f, "verification").length, links.length);
				assert.equal(sent(f).length, mail.length);
				note(`${mode}-${scenario}-hostile-origin-no-native-effect`);
				await server.stop();
			} finally {
				server.kill();
			}
		}
	const f = fixture("unavailable");
	const d = new Database(f.control);
	d.exec("DROP TABLE verification");
	d.close();
	const server = await start(f);
	try {
		const r = await fetch(`${server.origin}/api/auth/request-password-reset`, {
			method: "POST",
			headers: { origin: server.origin, "content-type": "application/json" },
			body: JSON.stringify({ email: "u@example.invalid" }),
		});
		assert.equal(r.status, 200);
		assert.equal(await r.text(), ack);
		assert.deepEqual(sent(f), []);
		assert.equal(rows(f, "rm_workspace_auth_telemetry_events").length, 0);
		note("native-rollback-failure-concealed-without-mail");
		await server.stop();
	} finally {
		server.kill();
	}
	const report = JSON.stringify(
		{
			ok: true,
			scope:
				"Native loopback HTTP reset request through current synthetic control and postcommit injected mail, actual source acknowledgement oracle",
			checks,
			observations,
			limits: [
				"No product workspace/physical handle/lifetime or legacy writer admission; synthetic helper alone opens fixture paths",
				"Only reset request route; no callback/completion/login/session/SPA/TLS listener, production transport or full R41/R42 completion",
				"Process-local bounded limiter, socket peer only; no persisted/restart/multi-process or proxy identity qualification",
				"Same public acknowledgement does not establish constant-time or constant-I/O privacy",
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
