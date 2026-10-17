/** Independent Decimal expectations; actual legacy numeric bridges are frozen separately. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
	existsSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import {
	addDkk,
	multiplyDkk,
	normalizeCurrency,
	percentOfDkk,
	roundDiv,
	roundDkk,
	roundRate6,
	subtractDkk,
	toOre,
} from "../../src/core/money";
const root = resolve(import.meta.dir, "../..");
const dir = join(
	root,
	"docs/architecture/rust-target/implementation/exact-arithmetic",
);
const cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const hash = (s: string | Buffer) =>
	createHash("sha256").update(s).digest("hex");
const vectorPath = join(dir, "vectors.jsonl"),
	text = readFileSync(vectorPath, "utf8");
const manifest = JSON.parse(
	readFileSync(join(dir, "vectors-manifest.json"), "utf8"),
);
assert.equal(hash(text), manifest.vectors_sha256);
assert.equal(
	hash(readFileSync(join(root, "scripts/rust/arithmetic-vectors.py"))),
	manifest.generator_sha256,
);
type Case = {
	id: string;
	op: string;
	input?: unknown;
	left?: string;
	right?: string;
	currency?: string;
	expected: string | { error: string };
};
const cases: Case[] = text
	.trimEnd()
	.split("\n")
	.map((s) => JSON.parse(s));
assert.equal(cases.length, manifest.checks);
function fixed(n: bigint, scale: number) {
	const sign = n < 0n ? "-" : "",
		digits = (n < 0n ? -n : n).toString().padStart(scale + 1, "0");
	return `${sign}${digits.slice(0, -scale)}.${digits.slice(-scale)}`;
}
// Interpret the actual finite Number's decimal spelling, including exponents.
// This only presents legacy observations; independent native expectations never use it.
function rateText(n: number) {
	const match = /^(-?)(\d+)(?:\.(\d+))?(?:e([+-]?\d+))?$/.exec(String(n));
	assert.ok(match);
	let value = BigInt(match[2] + (match[3] ?? "")) * (match[1] ? -1n : 1n);
	const shift = 6 - (match[3]?.length ?? 0) + Number(match[4] ?? 0);
	value =
		shift >= 0
			? value * 10n ** BigInt(shift)
			: roundDiv(value, 10n ** BigInt(-shift));
	return fixed(value, 6);
}
const observations: unknown[] = [],
	classifications: Record<string, number> = {};
for (const v of cases) {
	let actual: unknown, classification: string;
	if (v.op.endsWith("-wire")) {
		classification = "native_canonical_only";
		actual = { no_legacy_operation: true };
	} else {
		const bridge: Record<string, string> = {};
		const number = (key: "input" | "left" | "right") => {
			const n = Number(v[key]);
			bridge[key] = String(n);
			return n;
		};
		let value: string | undefined;
		let failed = false;
		try {
			let result: number;
			if (v.op === "currency") {
				value = normalizeCurrency(v.input as string);
				actual = { ok: true, value };
			} else {
				switch (v.op) {
					case "money":
						value = fixed(toOre(number("input")), 2);
						break;
					case "rate":
						result = roundRate6(number("input"));
						break;
					case "multiply":
						result = multiplyDkk(number("left"), number("right"));
						break;
					case "percent":
						result = percentOfDkk(number("left"), number("right"));
						break;
					case "add":
						result = addDkk(number("left"), number("right"));
						break;
					case "subtract":
						result = subtractDkk(number("left"), number("right"));
						break;
					case "fx": {
						// Arithmetic portion of ledger FX validation, not whole journal admission.
						const amount = roundDkk(number("left")),
							rate = roundRate6(number("right"));
						bridge.amountNormalized = String(amount);
						bridge.rateNormalized = String(rate);
						bridge.binaryProduct = String(amount * rate);
						bridge.currencyNormalized = normalizeCurrency(v.currency);
						result = roundDkk(amount * rate);
						break;
					}
					default:
						throw Error(`unknown synthetic operation ${v.op}`);
				}
				if (value !== undefined) actual = { ok: true, value, bridge };
				else if (!Number.isFinite(result!)) {
					failed = true;
					actual = { ok: false, nonfinite: String(result!), bridge };
				} else {
					value =
						v.op === "rate" ? rateText(result!) : fixed(toOre(result!), 2);
					actual = { ok: true, value, resultNumber: String(result!), bridge };
				}
			}
		} catch (e) {
			failed = true;
			actual = {
				ok: false,
				error: e instanceof Error ? e.message : String(e),
				bridge,
			};
		}
		const nativeRefusal = typeof v.expected !== "string";
		classification = failed
			? nativeRefusal
				? "both_refuse"
				: "legacy_number_refusal"
			: nativeRefusal
				? "native_typed_refusal"
				: value === v.expected
					? "match"
					: "difference";
	}
	classifications[classification] = (classifications[classification] ?? 0) + 1;
	if (classification !== "match")
		observations.push({ id: v.id, classification, legacy: actual });
}
const frozen = observations.map((x) => JSON.stringify(x)).join("\n") + "\n",
	frozenPath = join(dir, "legacy-observations.jsonl");
const build = Bun.spawnSync(
	[
		cargo,
		"test",
		"--locked",
		"-p",
		"rentemester-cli",
		"--test",
		"arithmetic_vectors",
		"--no-run",
		"--message-format=json",
	],
	{ cwd: root, stdout: "pipe", stderr: "pipe" },
);
assert.equal(build.exitCode, 0, build.stderr.toString());
const helper = build.stdout
	.toString()
	.split("\n")
	.filter(Boolean)
	.map((s) => JSON.parse(s))
	.find(
		(x) =>
			x.reason === "compiler-artifact" &&
			x.target.name === "arithmetic_vectors" &&
			x.profile.test &&
			x.executable,
	)?.executable;
assert.ok(helper);
const temp = mkdtempSync(join(tmpdir(), "rm-arithmetic-"));
try {
	let previous: unknown;
	for (const zone of ["UTC", "Pacific/Honolulu"]) {
		const output = join(temp, zone.replaceAll("/", "-") + ".json");
		const run = Bun.spawnSync(
			[helper, "--exact", "vectors", "--ignored", "--nocapture"],
			{
				cwd: root,
				env: {
					...process.env,
					RM_ARITHMETIC_INPUT: vectorPath,
					RM_ARITHMETIC_OUTPUT: output,
					TZ: zone,
					RENTEMESTER_TODAY: "invalid-clock-override",
				},
				stdout: "pipe",
				stderr: "pipe",
				timeout: 30000,
			},
		);
		assert.equal(
			run.exitCode,
			0,
			run.stderr.toString() + run.stdout.toString(),
		);
		const values = JSON.parse(readFileSync(output, "utf8"));
		assert.deepEqual(
			values,
			cases.map((v) => v.expected),
		);
		if (previous) assert.deepEqual(values, previous);
		previous = values;
	}
	if (process.argv.includes("--capture-legacy")) {
		assert.ok(!existsSync(frozenPath), "legacy observations already frozen");
		writeFileSync(frozenPath, frozen);
	} else
		assert.equal(
			readFileSync(frozenPath, "utf8"),
			frozen,
			"actual legacy observations changed; investigate explicitly",
		);
	console.log(
		JSON.stringify(
			{
				scope:
					"exact amount/currency/FX arithmetic with independent Decimal expectations",
				checks: cases.length + 1,
				vectors: cases.length,
				operations: manifest.operations,
				legacy_classifications: classifications,
				corpus_sha256: manifest.vectors_sha256,
				legacy_observations_sha256: hash(frozen),
				passed: [
					"native results exactly match independent fixed corpus",
					"actual legacy numeric bridges, refusals and differences explicitly frozen",
					"canonical wire independently verified; alternate environment stable",
				],
				platform: process.platform,
				architecture: process.arch,
				limits: [
					"Legacy API takes Numbers; exact decimal strings are explicitly bridged through Number before actual source calls",
					"FX comparison is arithmetic only; typed positive rate/currency admission is not a claim about old whole-journal validation",
					"Canonical wire has no equivalent old serde operation; transport budgets remain caller responsibility",
					"No source mutation, journal policy, original writes, runtime authority, UI, distribution, cutover or broad R09 completion",
				],
			},
			null,
			2,
		),
	);
} finally {
	rmSync(temp, { recursive: true, force: true });
}
