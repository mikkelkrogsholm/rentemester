/** Historical native-v1 fixtures and current main-v2 writer vs native descriptor observation. */
import assert from "node:assert/strict";
import {
	mkdtempSync,
	mkdirSync,
	writeFileSync,
	readFileSync,
	readdirSync,
	lstatSync,
	rmSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { homedir, tmpdir } from "node:os";
import { createHash } from "node:crypto";
import {
	saveWorkspaceManifest,
	loadWorkspaceManifest,
} from "../../src/core/workspace";
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
const work = mkdtempSync(join(tmpdir(), "rm-layout-"));
const entry = {
	slug: "a",
	name: "Synthetic",
	createdAt: "2026-01-01T00:00:00.000Z",
	archived: false,
};
// Frozen historical native-v1 fixture, NOT the current main writer contract.
// Current native admission also accepts strict v2/live; v1 still forbids purpose fields.
const canonical = { version: 1 as const, companies: [entry] };
const historicalNativeV1Json = `${JSON.stringify(canonical, null, 2)}\n`;
const cases = [
	{ id: "historical-native-v1-canonical", allowed: true },
	{ id: "main-v2-writer", mainWriter: true, allowed: true },
	{
		id: "alternate-json-format",
		manifest: JSON.stringify(canonical),
		allowed: true,
	},
	{
		id: "reordered-keys",
		manifest: JSON.stringify({
			companies: [
				{
					archived: false,
					createdAt: entry.createdAt,
					name: entry.name,
					slug: "a",
				},
			],
			version: 1,
		}),
		allowed: true,
	},
	{
		id: "missing-name",
		manifest:
			'{"version":1,"companies":[{"slug":"a","createdAt":"2026-01-01","archived":false}]}',
	},
	{
		id: "missing-created",
		manifest:
			'{"version":1,"companies":[{"slug":"a","name":"Synthetic","archived":false}]}',
	},
	{ id: "missing-companies", manifest: '{"version":1}' },
	{ id: "wrong-companies-shape", manifest: '{"version":1,"companies":{}}' },
	{
		id: "other-archived",
		manifest: JSON.stringify({
			...canonical,
			companies: [entry, { ...entry, slug: "b", archived: true }],
		}),
		allowed: true,
	},
	{
		id: "archived",
		manifest: JSON.stringify({
			...canonical,
			companies: [{ ...entry, archived: true }],
		}),
	},
	{
		id: "duplicate-slug",
		manifest: JSON.stringify({ ...canonical, companies: [entry, entry] }),
	},
	{
		id: "coerced-boolean",
		manifest: JSON.stringify({
			...canonical,
			companies: [{ ...entry, archived: "false" }],
		}),
	},
	{
		id: "omitted-archived",
		manifest:
			'{"version":1,"companies":[{"slug":"a","name":"Synthetic","createdAt":"2026-01-01"}]}',
	},
	{
		id: "unknown-field",
		manifest: JSON.stringify({ ...canonical, unknown: true }),
	},
	{
		id: "unknown-version",
		manifest: JSON.stringify({ ...canonical, version: 3 }),
	},
	{
		id: "duplicate-json-key",
		manifest: JSON.stringify(canonical).replace(
			'"version":1',
			'"version":1,"version":1',
		),
	},
	{
		id: "coerced-name",
		manifest: JSON.stringify({
			...canonical,
			companies: [{ ...entry, name: 42 }],
		}),
	},
	{
		id: "blank-name",
		manifest: JSON.stringify({
			...canonical,
			companies: [{ ...entry, name: " " }],
		}),
	},
	{
		id: "unsafe-other-slug",
		manifest: JSON.stringify({
			...canonical,
			companies: [entry, { ...entry, slug: "../outside" }],
		}),
	},
	{ id: "missing-manifest", missing: true },
	{ id: "malformed-json", manifest: "{broken" },
	{ id: "unregistered", slug: "b" },
	{ id: "unsafe-selection", slug: "../a" },
];
function inventory(path: string): string[] {
	return readdirSync(path)
		.sort()
		.flatMap((name) => {
			const p = join(path, name);
			return lstatSync(p).isDirectory()
				? inventory(p).map((v) => name + "/" + v)
				: [
						name +
							":" +
							createHash("sha256").update(readFileSync(p)).digest("hex"),
					];
		});
}
const observations = [];
try {
	for (const test of cases) {
		const dir = join(work, test.id);
		mkdirSync(dir);
		mkdirSync(join(dir, ".rentemester"));
		mkdirSync(join(dir, "a/data"), { recursive: true });
		// Exercise the real writer before installing opaque descriptor-only DB bytes:
		// its retirement fence must not be bypassed or asked to open those bytes.
		if (test.mainWriter) {
			saveWorkspaceManifest(dir, { version: 2, companies: [entry] });
			assert.equal(JSON.parse(readFileSync(join(dir, "workspace.json"), "utf8")).version, 2);
		}
		writeFileSync(
			join(dir, ".rentemester/workspace-control.sqlite"),
			"synthetic control bytes; no SQLite open",
		);
		writeFileSync(
			join(dir, "a/data/ledger.sqlite"),
			"synthetic ledger bytes; no SQLite open",
		);
		if (!test.missing) {
			if (!test.mainWriter)
				writeFileSync(join(dir, "workspace.json"), historicalNativeV1Json);
			if (test.manifest !== undefined)
				writeFileSync(join(dir, "workspace.json"), test.manifest);
		}
		const before = inventory(dir);
		let legacyLoads = false;
		try {
			loadWorkspaceManifest(dir);
			legacyLoads = true;
		} catch {}
		assert.deepEqual(inventory(dir), before);
		if (test.mainWriter) assert.equal(legacyLoads, true, "main-v2 writer round-trips through main loader");
		const child = Bun.spawnSync(
			[helper, "--ignored", "--exact", "local::layout::probe", "--nocapture"],
			{
				cwd: work,
				env: {
					...process.env,
					RENTEMESTER_WORKSPACE: "/forged",
					RM_LAYOUT_REQUEST: JSON.stringify({
						root: dir,
						workspace: "w",
						slug: test.slug ?? "a",
					}),
				},
				stdout: "pipe",
				stderr: "pipe",
				timeout: 20000,
			},
		);
		assert.equal(
			child.exitCode,
			0,
			child.stdout.toString() + child.stderr.toString(),
		);
		const line = child.stdout
			.toString()
			.split("\n")
			.find((s) => s.startsWith("RM_LAYOUT_RESULT="));
		assert(line);
		const result = JSON.parse(line.slice("RM_LAYOUT_RESULT=".length));
		assert.equal(result.ok, !!test.allowed, test.id);
		assert.equal(result.writerAuthority, false);
		if (result.ok) {
			assert.equal(result.workspace, "w");
			assert.equal(result.slug, "a");
		}
		assert.deepEqual(inventory(dir), before);
		observations.push({
			id: test.id,
			legacyLoads,
			nativeBinds: result.ok,
			...(result.code ? { nativeCode: result.code } : {}),
		});
	}
	console.log(
		JSON.stringify(
			{
				ok: true,
				checks: cases.length,
				scope:
					"historical native-v1 canonical fixture and actual main-v2 writer/loader vs native descriptor layout; exact source inventory preserved; no SQLite admission",
				observations,
				limits: [
					"Main-v2 native port remains required; current main writer output is intentionally refused by native v1 admission",
					"Loader acceptance is not legacy authorization; typed native profile intentionally stricter",
					"Physical-layout observation only; arbitrary SQLite handle identity and writer authority remain unproven",
					"No cross-device mount or hardware/platform qualification",
				],
			},
			null,
			2,
		),
	);
} finally {
	rmSync(work, { recursive: true, force: true });
}
