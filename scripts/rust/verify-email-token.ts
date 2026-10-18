/** Pure native token metadata and actual installed JWT/source signer, no user mutation. */
import assert from "node:assert/strict";
import { join, resolve } from "node:path";
import { homedir } from "node:os";
import { createHmac } from "node:crypto";
import { jwtVerify, SignJWT } from "jose";
import { createEmailVerificationToken } from "../../node_modules/better-auth/dist/api/routes/email-verification.mjs";
import { verifyJWT } from "../../node_modules/better-auth/dist/crypto/jwt.mjs";
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
const key = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
	now = Date.now(),
	observations: { id: string }[] = [];
const note = (id: string) => observations.push({ id });
function native(request: any) {
	const p = Bun.spawnSync(
		[
			helper,
			"--exact",
			"email_token::tests::probe",
			"--ignored",
			"--nocapture",
		],
		{
			cwd: root,
			stdin: Buffer.from(JSON.stringify({ key, now, ...request })),
			stdout: "pipe",
			stderr: "pipe",
		},
	);
	assert.equal(p.exitCode, 0, "native private JWT helper failed");
	const out = p.stdout.toString(),
		match = out.match(/RM_EMAIL_TOKEN_RESULT=(.*)/);
	assert(match);
	const diagnostics = out.replace(match[0], "") + p.stderr.toString();
	assert(!diagnostics.includes(key));
	assert(!diagnostics.includes(request.token ?? "never-secret-marker"));
	return JSON.parse(match[1]);
}
const issued = native({ mode: "issue", email: "u@example.invalid" });
assert(issued.ok);
const token = issued.token as string;
const parsed = await jwtVerify(token, new TextEncoder().encode(key), {
	algorithms: ["HS256"],
	currentDate: new Date(now),
});
assert.deepEqual(parsed.protectedHeader, { alg: "HS256" });
assert.equal(parsed.payload.email, "u@example.invalid");
assert.equal(parsed.payload.purpose, "rentemester-email-verification-v1");
assert.equal(parsed.payload.exp! - parsed.payload.iat!, 3600);
assert.equal((await verifyJWT(token, key))?.email, "u@example.invalid");
note("installed-jose-and-source-reader-accept-native-envelope");
const self = native({ token });
assert(self.ok);
assert.equal(self.email, "u@example.invalid");
note("native-accepts-own-envelope");
const other = native({ mode: "issue", email: "u@example.invalid" });
assert(other.ok);
assert.notEqual(other.token, token);
assert.notEqual(native({ token: other.token }).nonce, self.nonce);
note("same-clock-issuance-independent-256bit-nonce");
const source = await createEmailVerificationToken(
	key,
	"U@example.invalid",
	undefined,
	3600,
	{
		purpose: "rentemester-email-verification-v1",
		workspace: "w",
		jti: self.nonce,
	},
);
assert(native({ token: source, now: Date.now() }).ok);
note("actual-source-signer-native-envelope-interop");
const legacy = await createEmailVerificationToken(key, "u@example.invalid");
assert(await verifyJWT(legacy, key));
assert(!native({ token: legacy, now: Date.now() }).ok);
note("legacy-stateless-link-is-not-native-context-authority");
const signingKey = new TextEncoder().encode(key),
	base = parsed.payload;
for (const [id, patch] of [
	["missing-exp", { exp: undefined }],
	["missing-iat", { iat: undefined }],
	["future", { iat: base.iat! + 1, exp: base.exp! + 1 }],
	["long-life", { exp: base.exp! + 1 }],
	["expired", { iat: base.iat! - 3600, exp: base.iat! }],
	["wrong-purpose", { purpose: "password-reset" }],
	["wrong-workspace", { workspace: "other" }],
	["change-email", { updateTo: "other@example.invalid" }],
	["request-type", { requestType: "change-email-confirmation" }],
	["bad-nonce", { jti: "bad" }],
	["uppercase-email", { email: "U@example.invalid" }],
] as const) {
	const bad = await new SignJWT({ ...base, ...patch })
		.setProtectedHeader({ alg: "HS256" })
		.sign(signingKey);
	assert(!native({ token: bad }).ok, id);
	note(`native-refuses-${id}`);
}
for (const field of ["email", "iat", "exp"]) {
	const h = Buffer.from('{"alg":"HS256"}').toString("base64url"),
		p = Buffer.from(
			JSON.stringify(base).replace(
				"{",
				`{${JSON.stringify(field)}:${JSON.stringify(base[field])},`,
			),
		).toString("base64url"),
		input = `${h}.${p}`,
		sig = createHmac("sha256", key).update(input).digest("base64url");
	assert(!native({ token: `${input}.${sig}` }).ok);
	note(`duplicate-${field}-signed-refused`);
}
for (const wrong of [
	token + "=",
	token + ".x",
	token.replace(/.$/, token.endsWith("A") ? "B" : "A"),
]) {
	assert(!native({ token: wrong }).ok);
	note("compact-mutation-refused");
}
assert(native({ token, now: self.exp * 1000 - 1 }).ok);
assert(!native({ token, now: self.exp * 1000 }).ok);
assert(!native({ token, now: self.iat * 1000 - 1 }).ok);
note("exact-expiry-and-future-boundaries");
assert(!native({ token, workspace: "other" }).ok);
assert(
	!native({ token, key: "AQECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8" }).ok,
);
note("workspace-and-active-key-binding");
console.log(
	JSON.stringify({
		ok: true,
		checks: observations.length,
		observations,
		limits: [
			"Crypto metadata only; native writer context/one-time consumption/current user binding and HTTP/mail remain required",
			"Legacy stateless links deliberately refused, no keyring fallback or change-email authority",
		],
	}),
);
