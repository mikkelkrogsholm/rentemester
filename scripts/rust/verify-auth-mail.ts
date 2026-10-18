/** Real local TLS/DNS transport probes and independent source-wire oracle. No external mail. */
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createServer as tlsServer } from "node:tls";
import { createSocket } from "node:dgram";
import { createHttpJsonV1AuthEmailSender } from "../../src/server/auth-email";
const root = resolve(import.meta.dir, "../.."),
	cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const build = Bun.spawnSync(
	[
		cargo,
		"test",
		"--locked",
		"--release",
		"-p",
		"rentemester-runtime",
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
		(v) => v.executable && v.target.name === "rentemester_runtime",
	)?.executable;
assert(helper);
const work = mkdtempSync(join(tmpdir(), "rm-auth-mail-"));
const bearer = "synthetic-gateway-bearer-private",
	key = "synthetic-mail-idempotency-secret-123456",
	from = "auth@example.invalid",
	recipient = "person@example.invalid",
	token = "AbCdEfGhIjKlMnOpQrStUv01",
	actionUrl = `https://example.invalid/api/auth/reset-password/${token}?callbackURL=%2Freset`;
let checks = 0;
const observations: any[] = [];
const note = (id: string) => {
	checks++;
	observations.push({ id });
};
function openssl(args: string[]) {
	const p = Bun.spawnSync(["openssl", ...args], {
		cwd: work,
		stdout: "pipe",
		stderr: "pipe",
	});
	assert.equal(p.exitCode, 0, "synthetic TLS certificate generation failed");
}
writeFileSync(
	join(work, "ca.cnf"),
	"[req]\ndistinguished_name=dn\nx509_extensions=ext\nprompt=no\n[dn]\nCN=Rentemester synthetic test CA\n[ext]\nbasicConstraints=critical,CA:TRUE\nkeyUsage=critical,keyCertSign,cRLSign\n",
);
writeFileSync(
	join(work, "leaf.cnf"),
	"[req]\ndistinguished_name=dn\nprompt=no\n[dn]\nCN=gateway.test\n",
);
writeFileSync(
	join(work, "ext.cnf"),
	"basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1,DNS:gateway.test\n",
);
openssl([
	"req",
	"-x509",
	"-newkey",
	"rsa:2048",
	"-nodes",
	"-days",
	"2",
	"-config",
	"ca.cnf",
	"-keyout",
	"ca.key",
	"-out",
	"ca.pem",
]);
openssl([
	"req",
	"-new",
	"-newkey",
	"rsa:2048",
	"-nodes",
	"-config",
	"leaf.cnf",
	"-keyout",
	"leaf.key",
	"-out",
	"leaf.csr",
]);
openssl([
	"x509",
	"-req",
	"-in",
	"leaf.csr",
	"-CA",
	"ca.pem",
	"-CAkey",
	"ca.key",
	"-CAcreateserial",
	"-days",
	"2",
	"-extfile",
	"ext.cnf",
	"-out",
	"leaf.pem",
]);
writeFileSync(
	join(work, "wrong-ext.cnf"),
	readFileSync(join(work, "ext.cnf"), "utf8").replace(
		"IP:127.0.0.1,DNS:gateway.test",
		"DNS:wrong.invalid",
	),
);
openssl([
	"x509",
	"-req",
	"-in",
	"leaf.csr",
	"-CA",
	"ca.pem",
	"-CAkey",
	"ca.key",
	"-CAserial",
	"ca.srl",
	"-days",
	"2",
	"-extfile",
	"wrong-ext.cnf",
	"-out",
	"wrong.pem",
]);
const rootPem = readFileSync(join(work, "ca.pem"), "utf8"),
	tls = {
		key: readFileSync(join(work, "leaf.key")),
		cert: readFileSync(join(work, "leaf.pem")),
	};
async function native(endpoint: string, extra: any = {}) {
	const child = Bun.spawn(
		[helper, "--ignored", "--exact", "auth_mail::tests::probe", "--nocapture"],
		{
			cwd: work,
			env: {
				...process.env,
				NO_PROXY: "",
				no_proxy: "",
				http_proxy: "http://127.0.0.1:1",
				https_proxy: "http://127.0.0.1:1",
				HTTP_PROXY: "http://127.0.0.1:1",
				HTTPS_PROXY: "http://127.0.0.1:1",
				ALL_PROXY: "http://127.0.0.1:1",
				SSLKEYLOGFILE: join(work, "forbidden-keylog"),
			},
			stdin: Buffer.from(
				JSON.stringify({
					endpoint,
					bearer,
					key,
					from,
					recipient,
					token,
					actionUrl,
					rootPem,
					...extra,
				}) + "\n",
			),
			stdout: "pipe",
			stderr: "pipe",
		},
	);
	const timer = setTimeout(() => child.kill(), 20000);
	try {
		const [exit, out, err] = await Promise.all([
			child.exited,
			new Response(child.stdout).text(),
			new Response(child.stderr).text(),
		]);
		assert.equal(exit, 0, "native auth-mail helper failed");
		const match = out.match(/RM_AUTH_MAIL_RESULT=(.*)/);
		assert(match, "missing auth-mail result");
		const diagnostic = out.replace(match[0], "") + err;
		for (const secret of [bearer, key, recipient, token, actionUrl])
			assert(!diagnostic.includes(secret), "sensitive transport diagnostic");
		return JSON.parse(match[1]);
	} finally {
		clearTimeout(timer);
		child.kill();
		await child.exited;
	}
}
async function exercise(
	id: string,
	statuses: number[],
	extra: any = {},
	expected = true,
) {
	const captured: any[] = [];
	let trapCalls = 0;
	const trap = Bun.serve({
		hostname: "127.0.0.1",
		port: 0,
		tls,
		fetch() {
			trapCalls++;
			return new Response(null, { status: 202 });
		},
	});
	const server = Bun.serve({
		hostname: "127.0.0.1",
		port: 0,
		tls,
		async fetch(request) {
			captured.push({
				method: request.method,
				url: request.url,
				headers: Object.fromEntries(request.headers),
				body: await request.text(),
			});
			const status =
				statuses[Math.min(captured.length - 1, statuses.length - 1)];
			return new Response(
				status === 204 ? null : "synthetic private provider response",
				{
					status,
					headers: { location: `https://127.0.0.1:${trap.port}/leak` },
				},
			);
		},
	});
	try {
		const endpoint = `https://127.0.0.1:${server.port}/v1/send`,
			result = await native(endpoint, extra);
		assert.equal(result.ok, expected, id);
		assert.equal(trapCalls, 0, "redirect forwarded sensitive payload");
		for (const r of captured) {
			assert.equal(r.method, "POST");
			assert.equal(r.url, endpoint);
			assert.equal(r.headers.authorization, `Bearer ${bearer}`);
			assert.equal(r.headers["content-type"], "application/json");
			assert.equal(r.headers.cookie, undefined);
			assert.equal(r.headers.referer, undefined);
			assert.equal(r.headers["proxy-authorization"], undefined);
		}
		if (captured.length > 1) {
			assert.equal(new Set(captured.map((r) => r.body)).size, 1);
			assert.equal(
				new Set(captured.map((r) => r.headers["idempotency-key"])).size,
				1,
			);
		}
		note(id);
		return { captured, result, endpoint };
	} finally {
		await server.stop(true);
		await trap.stop(true);
	}
}
try {
	for (const kind of [
		"verification",
		"password-reset",
		"workspace-invitation",
	] as const) {
		const url =
			kind === "workspace-invitation"
				? "https://example.invalid/invite#token=synthetic-fragment"
				: actionUrl;
		const observed = await exercise(`wire-${kind}`, [202], {
			kind,
			actionUrl: url,
		});
		assert.equal(observed.captured.length, 1);
		let oracle: Request | undefined;
		const sender = createHttpJsonV1AuthEmailSender(
			{
				url: observed.endpoint,
				bearerToken: bearer,
				from: ` ${from} `,
				idempotencySecret: key,
			},
			{
				async fetch(r) {
					oracle = r;
					return new Response(null, { status: 202 });
				},
			},
		);
		await sender.send({ kind, recipient, url, token });
		assert(oracle);
		assert.deepEqual(
			JSON.parse(observed.captured[0].body),
			await oracle.json(),
		);
		for (const header of [
			"authorization",
			"content-type",
			"idempotency-key",
			"x-rentemester-auth-event",
		])
			assert.equal(
				observed.captured[0].headers[header],
				oracle.headers.get(header),
			);
		note(`actual-source-wire-HMAC-${kind}`);
	}
	const unicode = await exercise(
		"actual-source-Unicode-address-and-HMAC",
		[202],
		{
			from: " áuth@eksempel.invalid ",
			recipient: "møller@eksempel.invalid",
			key: "synthetic nøgle 123",
		},
	);
	let unicodeOracle: Request | undefined;
	const unicodeSender = createHttpJsonV1AuthEmailSender(
		{
			url: unicode.endpoint,
			bearerToken: bearer,
			from: " áuth@eksempel.invalid ",
			idempotencySecret: "synthetic nøgle 123",
		},
		{
			async fetch(r) {
				unicodeOracle = r;
				return new Response(null, { status: 202 });
			},
		},
	);
	await unicodeSender.send({
		kind: "password-reset",
		recipient: "møller@eksempel.invalid",
		url: actionUrl,
		token,
	});
	assert(unicodeOracle);
	assert.deepEqual(
		JSON.parse(unicode.captured[0].body),
		await unicodeOracle.json(),
	);
	assert.equal(
		unicode.captured[0].headers["idempotency-key"],
		unicodeOracle.headers.get("idempotency-key"),
	);
	const viaTrait = await exercise("native-reset-sender-trait-real-TLS", [202], {
		asResetSender: true,
	});
	assert.equal(JSON.parse(viaTrait.captured[0].body).actionUrl, actionUrl);
	const verificationToken = "e30.e30.c2ln";
	const verificationUrl = `https://example.invalid/api/auth/verify-email?token=${verificationToken}&callbackURL=%2Flogin`;
	const viaVerification = await exercise(
		"native-verification-sender-trait-real-TLS",
		[202],
		{
			asVerificationSender: true,
			token: verificationToken,
			kind: "verification",
		},
	);
	assert.equal(
		JSON.parse(viaVerification.captured[0].body).actionUrl,
		verificationUrl,
	);
	assert.equal(
		JSON.parse(viaVerification.captured[0].body).kind,
		"verification",
	);
	let verificationOracle: Request | undefined;
	const verificationSender = createHttpJsonV1AuthEmailSender(
		{
			url: viaVerification.endpoint,
			bearerToken: bearer,
			from,
			idempotencySecret: key,
		},
		{
			async fetch(r) {
				verificationOracle = r;
				return new Response(null, { status: 202 });
			},
		},
	);
	await verificationSender.send({
		kind: "verification",
		recipient,
		url: verificationUrl,
		token: verificationToken,
	});
	assert(verificationOracle);
	assert.deepEqual(
		JSON.parse(viaVerification.captured[0].body),
		await verificationOracle.json(),
	);
	assert.equal(
		viaVerification.captured[0].headers["idempotency-key"],
		verificationOracle.headers.get("idempotency-key"),
	);
	note("verification-trait-actual-source-wire-HMAC");
	for (const status of [200, 204]) {
		const r = await exercise(`status-${status}`, [status]);
		assert.equal(r.captured.length, 1);
	}
	for (const status of [400, 401, 403, 404, 408, 422]) {
		const r = await exercise(`nonretry-${status}`, [status], {}, false);
		assert.equal(r.captured.length, 1);
		assert.equal(r.result.code, "auth_delivery_unconfirmed");
	}
	for (const status of [301, 302, 303, 307, 308]) {
		const r = await exercise(
			`redirect-${status}-no-forward`,
			[status],
			{},
			false,
		);
		assert.equal(r.captured.length, 1);
	}
	for (const [id, statuses, count, ok] of [
		["rate-limit", [429, 202], 2, true],
		["server-retries", [503, 502, 202], 3, true],
		["attempt-limit", [503, 503, 503], 3, false],
		["retry-then-client-error", [503, 400], 2, false],
	] as const) {
		const r = await exercise(id, [...statuses], {}, ok);
		assert.equal(r.captured.length, count);
	}
	const a = await exercise("idempotency-stable-separate-call", [202]);
	const b = await exercise("idempotency-new-token", [202], {
		token: "AbCdEfGhIjKlMnOpQrStUv02",
	});
	assert.notEqual(
		a.captured[0].headers["idempotency-key"],
		b.captured[0].headers["idempotency-key"],
	);
	const unknown = await exercise(
		"untrusted-root-refused",
		[202],
		{ rootPem: null },
		false,
	);
	assert.equal(unknown.captured.length, 0);
	const wrong = Bun.serve({
		hostname: "127.0.0.1",
		port: 0,
		tls: { key: tls.key, cert: readFileSync(join(work, "wrong.pem")) },
		fetch() {
			throw new Error("unreachable certificate name");
		},
	});
	try {
		const r = await native(`https://127.0.0.1:${wrong.port}/v1/send`);
		assert.equal(r.ok, false);
		note("TLS-name-mismatch-refused");
	} finally {
		await wrong.stop(true);
	}
	let networkConnections = 0;
	const dropped = tlsServer(tls, (socket) => {
		networkConnections++;
		socket.destroy();
	});
	await new Promise<void>((r) => dropped.listen(0, "127.0.0.1", r));
	try {
		const address = dropped.address() as any;
		const r = await native(`https://127.0.0.1:${address.port}/v1/send`);
		assert.equal(r.ok, false);
		assert.equal(networkConnections, 3);
		note("network-failure-max-three-connections");
	} finally {
		await new Promise<void>((r) => dropped.close(() => r()));
	}
	const stalled = Bun.serve({
		hostname: "127.0.0.1",
		port: 0,
		tls,
		idleTimeout: 30,
		fetch() {
			return new Promise<Response>(() => {});
		},
	});
	try {
		const start = Date.now(),
			r = await native(`https://127.0.0.1:${stalled.port}/v1/send`);
		assert.equal(r.ok, false);
		assert(r.elapsedMs >= 7500 && r.elapsedMs < 10000);
		assert(Date.now() - start < 12000);
		note("real-stalled-TLS-response-shares-eight-second-budget");
	} finally {
		await stalled.stop(true);
	}
	const dns = createSocket("udp4");
	let queries = 0;
	dns.on("message", (message, remote) => {
		queries++;
		let end = 12;
		while (message[end] !== 0) end += message[end] + 1;
		end += 5;
		const header = Buffer.alloc(12);
		message.copy(header, 0, 0, 2);
		header.writeUInt16BE(0x8180, 2);
		header.writeUInt16BE(1, 4);
		header.writeUInt16BE(1, 6);
		const answer = Buffer.from([
			0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 127, 0, 0, 1,
		]);
		dns.send(
			Buffer.concat([header, message.subarray(12, end), answer]),
			remote.port,
			remote.address,
		);
	});
	await new Promise<void>((r) => dns.bind(0, "127.0.0.1", r));
	const named = Bun.serve({
		hostname: "127.0.0.1",
		port: 0,
		tls,
		fetch() {
			return new Response(null, { status: 202 });
		},
	});
	try {
		const port = (dns.address() as any).port;
		const r = await native(`https://gateway.test:${named.port}/v1/send`, {
			dnsPort: port,
		});
		assert.equal(r.ok, true);
		assert(queries > 0);
		note("actual-async-DNS-and-SAN-hostname-TLS");
	} finally {
		dns.close();
		await named.stop(true);
	}
	const blackhole = createSocket("udp4");
	let unansweredQueries = 0;
	blackhole.on("message", () => {
		unansweredQueries++;
	});
	await new Promise<void>((r) => blackhole.bind(0, "127.0.0.1", r));
	try {
		const r = await native("https://gateway.test:443/v1/send", {
			dnsPort: (blackhole.address() as any).port,
		});
		assert.equal(r.ok, false);
		assert(unansweredQueries > 0);
		assert(r.elapsedMs >= 7500 && r.elapsedMs < 10000);
		note("real-unanswered-async-DNS-bounded-by-shared-budget");
	} finally {
		blackhole.close();
	}
	for (const [id, extra, code] of [
		["query-endpoint", {}, "invalid_auth_config"],
		["header-injection", { bearer: "x\r\nprivate" }, "invalid_auth_config"],
		["empty-secret", { key: "" }, "invalid_auth_config"],
		["invalid-from", { from: "not-email" }, "invalid_auth_config"],
		[
			"invalid-recipient",
			{ recipient: "not-email" },
			"auth_delivery_unconfirmed",
		],
		[
			"body-budget",
			{ actionUrl: "https://example.invalid/" + '"'.repeat(16300) },
			"auth_delivery_unconfirmed",
		],
	] as any[]) {
		const captured: any[] = [];
		const server = Bun.serve({
			hostname: "127.0.0.1",
			port: 0,
			tls,
			fetch(req) {
				captured.push(req);
				return new Response(null, { status: 202 });
			},
		});
		try {
			const endpoint = `https://127.0.0.1:${server.port}/v1/send${id === "query-endpoint" ? "?private=query" : ""}`,
				r = await native(endpoint, extra);
			assert.equal(r.ok, false);
			assert.equal(r.code, code);
			assert.equal(captured.length, 0);
			note(`input-${id}-no-network-request`);
		} finally {
			await server.stop(true);
		}
	}
	assert(!(await Bun.file(join(work, "forbidden-keylog")).exists()));
	note("proxy-environment-and-TLS-keylog-not-used");
	const report = JSON.stringify(
		{
			ok: true,
			scope:
				"Concrete native HTTPS auth-mail gateway on synthetic local TLS/DNS with actual source wire oracle",
			checks,
			observations,
			limits: [
				"Only local generated CA/leaf trust override in ignored helper; production uses pinned Mozilla roots and no private trust bypass",
				"Eight seconds bounds send budget under ordinary scheduling, not construction, runtime thread Drop, hard realtime or whole server worker admission",
				"No external email, public HTTP acknowledgement/CSRF/rate dispatch, durable outbox/guaranteed delivery, full identity/platform/distribution or cutover",
			],
		},
		null,
		2,
	);
	for (const value of [bearer, key, recipient, token, actionUrl])
		assert(!report.includes(value));
	console.log(report);
} finally {
	rmSync(work, { recursive: true, force: true });
}
