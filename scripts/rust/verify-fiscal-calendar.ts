/** Independent Gregorian fixed corpus, actual legacy observations and native engine values. */
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
import { isValidIsoDate } from "../../src/core/dates";
import {
	annualReportDeadline,
	fiscalYearForDate,
} from "../../src/core/fiscal-year";
const root = resolve(import.meta.dir, "../.."),
	dir = join(
		root,
		"docs/architecture/rust-target/implementation/fiscal-calendar",
	),
	cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const hash = (s: string | Buffer) =>
	createHash("sha256").update(s).digest("hex");
const vectorPath = join(dir, "vectors.jsonl"),
	text = readFileSync(vectorPath, "utf8"),
	manifest = JSON.parse(
		readFileSync(join(dir, "vectors-manifest.json"), "utf8"),
	);
assert.equal(hash(text), manifest.vectors_sha256);
assert.equal(
	hash(readFileSync(join(root, "scripts/rust/fiscal-calendar-vectors.py"))),
	manifest.generator_sha256,
);
type Case = {
	id: string;
	op: "date" | "fiscal" | "deadline";
	date: string;
	startMonth?: number;
	strategy?: string;
	expected: unknown;
	legacy: "match" | "difference" | "refusal" | "typed_input";
};
const cases: Case[] = text
	.trimEnd()
	.split("\n")
	.map((s) => JSON.parse(s));
assert.equal(cases.length, manifest.checks);
const observations: unknown[] = [];
for (const v of cases) {
	let actual: { ok: true; value: unknown } | { ok: false; error: string };
	try {
		if (v.op === "date") {
			if (!isValidIsoDate(v.date)) throw Error("invalid ISO date");
			actual = { ok: true, value: v.date };
		} else
			actual = {
				ok: true,
				value:
					v.op === "fiscal"
						? fiscalYearForDate(
								v.date,
								v.startMonth!,
								v.strategy as "start-year" | "end-year" | "span",
							)
						: annualReportDeadline(v.date),
			};
	} catch (e) {
		actual = { ok: false, error: e instanceof Error ? e.message : String(e) };
	}
	if (v.legacy === "match") {
		assert.equal(actual.ok, true, v.id);
		assert.deepEqual(actual.ok ? actual.value : actual, v.expected, v.id);
	} else {
		if (v.legacy === "refusal") assert.equal(actual.ok, false, v.id);
		if (v.legacy === "difference") {
			assert.equal(actual.ok, true, v.id);
			assert.notDeepEqual(actual.ok ? actual.value : actual, v.expected, v.id);
		}
		if (v.legacy === "typed_input")
			assert.ok((v.expected as { error?: string }).error, v.id);
		observations.push({ id: v.id, classification: v.legacy, legacy: actual });
	}
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
		"fiscal_vectors",
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
			x.target.name === "fiscal_vectors" &&
			x.profile.test &&
			x.executable,
	)?.executable;
assert.ok(helper);
const temp = mkdtempSync(join(tmpdir(), "rm-fiscal-"));
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
					RM_FISCAL_INPUT: vectorPath,
					RM_FISCAL_OUTPUT: output,
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
		assert.ok(
			!existsSync(frozenPath),
			"legacy observation file already frozen",
		);
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
					"typed fiscal calendar: independent Gregorian expectations and exact actual legacy behavior",
				checks: cases.length + 1,
				vectors: cases.length,
				operations: manifest.operations,
				legacy_policies: manifest.legacy_policies,
				corpus_sha256: manifest.vectors_sha256,
				legacy_observations_sha256: hash(frozen),
				passed: [
					"all native results exactly match independent fixed corpus",
					"ordinary legacy results match; each correction/refusal explicitly classified and frozen",
					"canonical serialization and alternate TZ/environment stability",
				],
				platform: process.platform,
				architecture: process.arch,
				limits: [
					"Pure calendar arithmetic only; no company legal eligibility, filing action, clock or persisted identifier rewrite",
					"Typed native configuration constructors reject invalid raw settings; adapters still own legacy default normalization",
					"Low-year and derived-range legacy differences are explicit; existing HTTP fiscal-selection defects remain separate",
					"No original writes, full R09/R12 acceptance, runtime authority, UI/distribution or cutover",
				],
			},
			null,
			2,
		),
	);
} finally {
	rmSync(temp, { recursive: true, force: true });
}
