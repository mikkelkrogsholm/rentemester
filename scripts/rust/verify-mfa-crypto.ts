/** Independent libsodium/RFC vectors + native/actual installed Better Auth MFA crypto/OTP. */
import assert from "node:assert/strict";
import { readFileSync, mkdtempSync, rmSync } from "node:fs";
import { resolve, join } from "node:path";
import { homedir, tmpdir } from "node:os";
import { createHmac, createHash } from "node:crypto";
import {
	symmetricEncrypt,
	symmetricDecrypt,
} from "../../node_modules/better-auth/dist/crypto/index.mjs";
import { createOTP } from "@better-auth/utils/otp";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { createBetterAuthRuntime } from "../../src/server/better-auth";
const root = resolve(import.meta.dir, "../.."),
	cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const build = Bun.spawnSync(
	[
		cargo,
		"test",
		"--locked",
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
const corpus = JSON.parse(
	readFileSync(
		join(root, "crates/rentemester-runtime/fixtures/mfa.json"),
		"utf8",
	),
);
const key = corpus.aead[0].key,
	oldKey = Buffer.from(Array.from({ length: 32 }, (_, i) => i + 61)).toString(
		"base64url",
	);
const versions = [
		{ version: 8, value: oldKey },
		{ version: 7, value: key },
	],
	nativeKeys = [{ version: 7, value: key }];
const work = mkdtempSync(join(tmpdir(), "rm-mfa-crypto-"));
let checks = 0;
const observations: any[] = [];
function record(id: string, extra: any = {}) {
	checks++;
	observations.push({ id, ok: true, ...extra });
}
function native(request: any) {
	const p = Bun.spawnSync(
		[helper, "--ignored", "--exact", "mfa::tests::probe", "--nocapture"],
		{
			cwd: work,
			stdin: Buffer.from(JSON.stringify({ keys: nativeKeys, ...request })),
			stdout: "pipe",
			stderr: "pipe",
		},
	);
	assert.equal(p.exitCode, 0, "native MFA probe failed");
	const out = p.stdout.toString(),
		err = p.stderr.toString();
	assert(!err.includes(key));
	assert(!err.includes(oldKey));
	const match = out.match(/RM_MFA_RESULT=(.*)/);
	assert(match, "missing native MFA result");
	return JSON.parse(match[1]);
}
function good(request: any) {
	const r = native(request);
	assert.equal(r.ok, true);
	return r;
}
function refuse(id: string, request: any, code = "invalid_credentials") {
	const r = native(request);
	assert.deepEqual(r, { ok: false, code });
	record(id, { code });
}
function otp(secret: string, counter: number | bigint) {
	const bytes = Buffer.alloc(8);
	bytes.writeBigUInt64BE(BigInt(counter));
	const h = createHmac("sha1", Buffer.from(secret, "utf8"))
		.update(bytes)
		.digest();
	const offset = h[19] & 15;
	return String((h.readUInt32BE(offset) & 0x7fffffff) % 1000000).padStart(
		6,
		"0",
	);
}
function nativeCounter(secret: string, code: string, nowMs: number) {
	return native({ mode: "totp", secret, code, nowMs });
}
const nowOriginal = Date.now;
try {
	const db = openWorkspaceControlDb(work),
		auth = createBetterAuthRuntime(db, {
			secret: oldKey,
			secrets: versions,
			legacySecret: key,
			trustedOrigins: ["https://example.invalid"],
			baseURL: "https://example.invalid",
		});
	const ctx = await auth.$context;
	const config = ctx.secretConfig;
	assert(typeof config !== "string");
	assert.equal(config.currentVersion, 8);
	assert.equal(config.keys.get(7), key);
	assert.equal(config.legacySecret, key);
	record("actual-configured-framework-keyring");
	for (const v of corpus.aead) {
		const r = native({ mode: "decrypt", value: v.envelope });
		if (v.validUtf8) {
			assert.equal(r.plaintext, v.decoded);
			assert.equal(
				await symmetricDecrypt({ key: config, data: v.envelope }),
				v.decoded,
			);
			record(v.id, { independentLibsodium: true });
		} else {
			assert.equal(r.ok, false);
			assert.equal(r.code, "invalid_credentials");
			assert.equal(
				await symmetricDecrypt({ key: config, data: v.envelope }),
				"\ufffd\ufffd\u0000",
			);
			record(v.id, { nativeCorrection: "strict-authenticated-UTF8" });
		}
	}
	for (const [i, text] of [
		"",
		"synthetic-mfa-secret",
		"æ雪💡\u0000",
		'["abcde-FG123","67890-abcDE"]',
		"a".repeat(4096),
		"\ufefftext",
		"\ufeff\ufefftext",
		"prefix\ufefftext",
	].entries()) {
		const decoded = text.startsWith("\ufeff") ? text.slice(1) : text;
		const issued = good({
			mode: "encrypt",
			keys: versions,
			plaintext: text,
		}).value;
		assert(issued.startsWith("$ba$8$"));
		assert.equal(
			await symmetricDecrypt({ key: config, data: issued }),
			decoded,
		);
		assert.equal(
			good({ mode: "decrypt", keys: versions, value: issued }).plaintext,
			decoded,
		);
		record(`native-issue-${i}`, { activeVersion: 8 });
		const source = await symmetricEncrypt({ key: config, data: text });
		assert.equal(
			good({ mode: "decrypt", keys: versions, value: source }).plaintext,
			decoded,
		);
		record(`source-issue-${i}`);
	}
	const old = corpus.aead[1];
	assert.equal(
		good({ mode: "decrypt", keys: versions, value: old.envelope }).plaintext,
		old.plaintext,
	);
	record("historical-version-decryption");
	assert.equal(
		good({ mode: "decrypt", keys: versions, legacy: key, value: old.raw })
			.plaintext,
		old.plaintext,
	);
	record("explicit-legacy-decryption");
	refuse("legacy-not-inferred", { mode: "decrypt", value: old.raw });
	refuse("retired-version", {
		mode: "decrypt",
		keys: [versions[0]],
		value: old.envelope,
	});
	refuse("unknown-version", {
		mode: "decrypt",
		value: old.envelope.replace("$7$", "$9$"),
	});
	refuse("wrong-key-material", {
		mode: "decrypt",
		keys: [{ version: 7, value: oldKey }],
		value: old.envelope,
	});
	// Version headers are not AEAD-associated data in the source format. Preserve this limitation.
	const relabeled = old.envelope.replace("$7$", "$9$");
	assert.equal(
		good({
			mode: "decrypt",
			keys: [{ version: 9, value: key }],
			value: relabeled,
		}).plaintext,
		old.plaintext,
	);
	record("source-header-not-authenticated", { noHeaderOwnershipClaim: true });
	for (const spelling of ["07", "+7", "7suffix", "7.0", "7e0", " 7"]) {
		const value = `$ba$${spelling}$${old.raw}`;
		assert.equal(
			await symmetricDecrypt({ key: config, data: value }),
			old.plaintext,
		);
		refuse(`version-correction-${spelling.trim()}`, { mode: "decrypt", value });
	}
	assert.equal(
		await symmetricDecrypt({
			key: config,
			data: `$ba$7$${old.raw.toUpperCase()}`,
		}),
		old.plaintext,
	);
	refuse("uppercase-hex-correction", {
		mode: "decrypt",
		value: `$ba$7$${old.raw.toUpperCase()}`,
	});
	for (const [i, value] of [
		"",
		"$ba$",
		"$ba$7",
		"$ba$7$00",
		"$ba$7$xyz",
		`${old.envelope}0`,
		`${old.envelope}$tail`,
		"x".repeat(8295),
	].entries())
		refuse(`malformed-${i}`, { mode: "decrypt", value });
	for (const [i, keys] of [
		[],
		[
			{ version: 7, value: key },
			{ version: 7, value: key },
		],
		[{ version: 9007199254740992, value: key }],
		[{ version: 1, value: "not-key" }],
		[{ version: 1, value: key + "=" }],
		[{ version: 1, value: "a".repeat(4097) }],
		Array.from({ length: 17 }, (_, i) => ({ version: i, value: key })),
	].entries())
		refuse(
			`configuration-${i}`,
			{ mode: "decrypt", keys, value: old.envelope },
			"invalid_auth_config",
		);
	refuse(
		"invalid-explicit-legacy",
		{ mode: "decrypt", legacy: "bad", value: old.envelope },
		"invalid_auth_config",
	);
	refuse("plaintext-budget", { mode: "encrypt", plaintext: "a".repeat(4097) });
	const nonces = new Set<string>();
	for (let i = 0; i < 20; i++) {
		const value = good({
			mode: "encrypt",
			plaintext: "same synthetic text",
		}).value;
		const raw = value.split("$")[3];
		assert(/^[0-9a-f]+$/.test(raw));
		nonces.add(raw.slice(0, 48));
		assert.equal(
			await symmetricDecrypt({ key: config, data: value }),
			"same synthetic text",
		);
	}
	assert.equal(nonces.size, 20);
	record("fresh-OS-nonce", { samples: 20 });
	const secretSet = new Set<string>();
	for (let i = 0; i < 20; i++) {
		const secret = good({ mode: "new-secret" }).secret;
		assert(/^[a-zA-Z0-9_-]{32}$/.test(secret));
		secretSet.add(secret);
	}
	assert.equal(secretSet.size, 20);
	record("fresh-OS-enrollment-secret", { samples: 20 });
	const bytes = Buffer.from(old.raw, "hex");
	for (let i = 0; i < bytes.length; i++) {
		const altered = Buffer.from(bytes);
		altered[i] ^= 1;
		const value = "$ba$7$" + altered.toString("hex");
		refuse(`corrupt-byte-${i}`, { mode: "decrypt", value });
		await assert.rejects(() => symmetricDecrypt({ key: config, data: value }));
	}
	for (const [i, v] of corpus.hotp.entries()) {
		assert.equal(await createOTP(corpus.rfcSecret).hotp(v.counter), v.code);
		assert.equal(otp(corpus.rfcSecret, v.counter), v.code);
		record(`rfc4226-${i}`);
	}
	for (const [i, v] of corpus.totp.entries()) {
		Date.now = () => v.nowMs;
		const source = await createOTP(corpus.rfcSecret).totp();
		assert.equal(source, v.code);
		const r = nativeCounter(corpus.rfcSecret, v.code, v.nowMs);
		assert.equal(r.ok, true);
		assert.equal(r.counter, v.counter);
		assert(await createOTP(corpus.rfcSecret).verify(v.code));
		record(`rfc6238-${i}`);
	}
	for (const [si, secret] of [
		"synthetic-enrollment-secret",
		"12345678901234567890",
		"æ\u0000雪",
		"é",
		"e\u0301",
		"a".repeat(1024),
	].entries()) {
		for (const time of [
			0, 1, 29999, 30000, 30001, 1767225600000, 253402300799999,
		]) {
			Date.now = () => time;
			const step = Math.floor(time / 30000);
			for (const delta of [-2, -1, 0, 1, 2]) {
				if (step + delta < 0) continue;
				const code = otp(secret, step + delta);
				const r = nativeCounter(secret, code, time);
				const source = await createOTP(secret).verify(code);
				const expected = Math.abs(delta) <= 1;
				assert.equal(r.ok, expected);
				assert.equal(source, expected);
				if (expected) assert.equal(r.counter, step + delta);
				record(`otp-${si}-${time}-${delta}`, { accepted: expected });
			}
		}
	}
	Date.now = () => 0;
	const wrapped = otp(corpus.rfcSecret, 18446744073709551615n);
	assert(await createOTP(corpus.rfcSecret).verify(wrapped));
	refuse("epoch-negative-neighbor-correction", {
		mode: "totp",
		secret: corpus.rfcSecret,
		code: wrapped,
		nowMs: 0,
	});
	for (const [i, code] of [
		"",
		"12345",
		"1234567",
		" 123456",
		"１２３４５６",
		"12345\n",
		"abcdef",
	].entries())
		refuse(`otp-code-grammar-${i}`, {
			mode: "totp",
			secret: corpus.rfcSecret,
			code,
			nowMs: 1767225600000,
		});
	for (const secret of ["", "a".repeat(1025)])
		refuse(`otp-secret-budget-${secret.length}`, {
			mode: "totp",
			secret,
			code: "123456",
			nowMs: 1767225600000,
		});
	for (const nowMs of [-1, 253402300800000])
		refuse(
			`clock-${nowMs}`,
			{ mode: "totp", secret: corpus.rfcSecret, code: "123456", nowMs },
			"invalid_auth_clock",
		);
	refuse(
		"unavailable-trusted-clock",
		{
			mode: "totp",
			secret: corpus.rfcSecret,
			code: "123456",
			nowMs: 0,
			clockUnavailable: true,
		},
		"invalid_auth_clock",
	);
	db.close();
	const output = {
		ok: true,
		scope:
			"Native source-compatible MFA cryptography only; no current identity/challenge/session or replay authority",
		checks,
		libsodiumFixtureVersion: corpus.libsodiumVersion,
		observations,
		limits: [
			"No DB/MFA/session/HTTP lifecycle, challenge consumption/attempt/lockout/replay policy or actual handle admission",
			"Existing empty-AAD envelope does not bind header/workspace/user/purpose; exact current-row identity remains mandatory",
			"Strict version/hex/UTF8/epoch and fixed budgets correct explicit source permissiveness",
			"OS failure injection and other hardware/platform qualification remain pending",
		],
	};
	const json = JSON.stringify(output, null, 2);
	for (const secret of [
		key,
		oldKey,
		corpus.rfcSecret,
		"synthetic-enrollment-secret",
		old.raw,
		old.envelope,
	])
		assert(!json.includes(secret), "sensitive process evidence");
	console.log(json);
} finally {
	Date.now = nowOriginal;
	rmSync(work, { recursive: true, force: true });
}
