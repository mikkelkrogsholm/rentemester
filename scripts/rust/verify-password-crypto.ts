/** Native/installed-framework bidirectional crypto + independent Python and Unicode corpus. */
import assert from "node:assert/strict";
import { readFileSync, mkdtempSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir, homedir } from "node:os";
import { createHash } from "node:crypto";
import {
	hashPassword,
	verifyPassword,
} from "../../node_modules/better-auth/dist/crypto/password.mjs";
const root = resolve(import.meta.dir, "../.."),
	cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
// Release only for the independent repeated KDF corpus; no tuning to weaken KDF parameters.
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
const work = mkdtempSync(join(tmpdir(), "rm-password-crypto-"));
const sourceMetadata = JSON.parse(
	readFileSync(
		join(root, "crates/rentemester-runtime/fixtures/password-sources.json"),
		"utf8",
	),
);
assert.equal(sourceMetadata.unicode_version, "17.0.0");
for (const source of sourceMetadata.sources) {
	assert.equal(
		createHash("sha256")
			.update(
				readFileSync(
					join(root, "crates/rentemester-runtime/fixtures", source.file),
				),
			)
			.digest("hex"),
		source.sha256,
		"reference source digest mismatch",
	);
}
const fixture = JSON.parse(
	readFileSync(
		join(root, "crates/rentemester-runtime/fixtures/password.json"),
		"utf8",
	),
);
let checks = 0;
const observations: any[] = [];
function native(request: any) {
	const result = Bun.spawnSync(
		[helper, "--ignored", "--exact", "password::tests::probe", "--nocapture"],
		{
			cwd: work,
			env: {
				...process.env,
				TZ: "Pacific/Honolulu",
				RENTEMESTER_PASSWORD: "forged",
				RENTEMESTER_ACTOR: "user:forged",
			},
			stdin: Buffer.from(JSON.stringify(request)),
			stdout: "pipe",
			stderr: "pipe",
			timeout: 30000,
		},
	);
	assert.equal(
		result.exitCode,
		0,
		"native probe failed without exposing input",
	);
	const text = result.stdout.toString(),
		error = result.stderr.toString();
	const line = text
		.split("\n")
		.find((s) => s.startsWith("RM_PASSWORD_RESULT="));
	assert(line);
	assert.equal(error, "");
	return JSON.parse(line.slice("RM_PASSWORD_RESULT=".length));
}
try {
	for (const v of fixture.vectors) {
		assert(await verifyPassword({ password: v.password, hash: v.hash }));
		const actual = native({ password: v.password, hash: v.hash });
		assert.equal(actual.ok, true, v.id);
		const wrong = native({ password: v.password + "!wrong", hash: v.hash });
		assert.deepEqual(wrong, { ok: false, code: "invalid_credentials" });
		observations.push({
			id: v.id,
			frameworkVerified: true,
			nativeVerified: true,
			wrongPasswordRefused: true,
		});
		checks += 2;
	}
	// Framework-generated random salts are checked natively; no copied hash helper acts as oracle.
	for (const password of [
		"Synthetic long password42!",
		"ＡＢＣ１２３Å\u030a",
		"\u{1ccd6} source current Unicode",
		"",
	]) {
		const hash = await hashPassword(password);
		assert.equal(hash.length, 161);
		assert.equal(native({ password, hash }).ok, true);
		checks++;
		const issued = native({ mode: "issue", password });
		assert.equal(issued.ok, true);
		assert(/^[0-9a-f]{32}:[0-9a-f]{128}$/.test(issued.hash));
		assert(await verifyPassword({ password, hash: issued.hash }));
		assert.equal(native({ password, hash: issued.hash }).ok, true);
		checks++;
	}
	const first = native({
			mode: "issue",
			password: "Same synthetic password42!",
		}),
		second = native({ mode: "issue", password: "Same synthetic password42!" });
	assert(first.ok && second.ok);
	assert.notEqual(first.hash.slice(0, 32), second.hash.slice(0, 32));
	checks++;
	const good = fixture.vectors[1];
	for (const [id, hash] of [
		["missing", undefined],
		["empty", ""],
		["upper", good.hash.toUpperCase()],
		["trailing", good.hash + ":ignored"],
		["short", good.hash.slice(0, -1)],
		["unknown-phc", "$scrypt$unsupported"],
		["salt-short", good.hash.slice(1)],
		["salt-nonhex", "g" + good.hash.slice(1)],
	]) {
		const result = native({ password: good.password, hash });
		assert.deepEqual(result, { ok: false, code: "invalid_credentials" }, id);
		checks++;
	}
	// Legacy ignores trailing fields after a valid salt:key; native deliberately refuses.
	assert(
		await verifyPassword({
			password: good.password,
			hash: good.hash + ":ignored",
		}),
	);
	checks++;
	for (const password of ["a".repeat(4097), "\ufdfa".repeat(1365)]) {
		assert.deepEqual(native({ mode: "issue", password }), {
			ok: false,
			code: "invalid_credentials",
		});
		assert.deepEqual(native({ password, hash: good.hash }), {
			ok: false,
			code: "invalid_credentials",
		});
		checks += 2;
	}
	const limit = "a".repeat(4096),
		issued = native({ mode: "issue", password: limit });
	assert.equal(issued.ok, true);
	assert(await verifyPassword({ password: limit, hash: issued.hash }));
	checks++;
	// Actual Bun normalization independently checks every column against the Unicode17 reference.
	assert.equal(process.versions.unicode, "17.0");
	let normalizationChecks = 0;
	for (const raw of readFileSync(
		join(
			root,
			"crates/rentemester-runtime/fixtures/NormalizationTest-17.0.0.txt",
		),
		"utf8",
	).split("\n")) {
		const line = raw.split("#")[0]!.trim();
		if (!line || line.startsWith("@")) continue;
		const columns = line
			.split(";")
			.slice(0, 5)
			.map((s) =>
				String.fromCodePoint(
					...s
						.trim()
						.split(/\s+/)
						.map((n) => parseInt(n, 16)),
				),
			);
		for (const input of columns) {
			assert.equal(input.normalize("NFKC"), columns[3]);
			normalizationChecks++;
		}
	}
	assert.equal(normalizationChecks, 100170);
	const scalarHash = createHash("sha256"),
		header = Buffer.alloc(8);
	let scalarCount = 0;
	for (let scalar = 0; scalar <= 0x10ffff; scalar++) {
		if (scalar >= 0xd800 && scalar <= 0xdfff) continue;
		const text = Buffer.from(String.fromCodePoint(scalar).normalize("NFKC"));
		header.writeUInt32LE(scalar, 0);
		header.writeUInt32LE(text.length, 4);
		scalarHash.update(header).update(text);
		scalarCount++;
	}
	const nativeScalars = native({ mode: "normalization" });
	assert.deepEqual(nativeScalars.unicodeVersion, [17, 0, 0]);
	assert.equal(nativeScalars.scalarCount, scalarCount);
	assert.equal(nativeScalars.scalarDigest, scalarHash.digest("hex"));
	assert.equal(scalarCount, 1112064);
	checks++;

	console.log(
		JSON.stringify(
			{
				ok: true,
				checks,
				normalizationChecks,
				scalarNormalizationChecks: scalarCount,
				pythonVectors: fixture.vectors.length,
				framework:
					"Better Auth1.7.1/@better-auth/utils0.4.2 actual password helpers",
				unicode: {
					bun: process.versions.unicode,
					icu: process.versions.icu,
					version: "17.0.0",
				},
				observations,
				limits: [
					"Native password crypto only, not DB login/MFA/session or service lifecycle",
					"Synchronous fixed32MiB KDF requires future bounded blocking/rate-limit dispatch",
					"Canonical hash and input budgets stricter than permissive source helper; no production credential data",
					"Normalization reference columns tested, not a claim of complete whole-runtime authentication parity",
				],
			},
			null,
			2,
		),
	);
} finally {
	rmSync(work, { recursive: true, force: true });
}
