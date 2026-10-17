/** Compiled manual policy metadata/content provenance; no product YAML/Bun runtime. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { readRuleBundleMetadata } from "../../src/core/rules-metadata";
const root = resolve(import.meta.dir, "../.."),
	bundles = readRuleBundleMetadata();
const asset = {
	bundles: bundles.map(({ name, version, ruleIds }) => ({
		name,
		version,
		ruleIds,
	})),
	sourceFiles: bundles.map(({ name, path }) => ({
		path: `rules/dk/${name}.yaml`,
		sha256: createHash("sha256").update(readFileSync(path)).digest("hex"),
	})),
};
const path = join(
		root,
		"crates/rentemester-storage/src/manual-rule-bundles.json",
	),
	text = JSON.stringify(asset, null, 2) + "\n";
if (process.argv.includes("--write")) writeFileSync(path, text);
else
	assert.equal(
		readFileSync(path, "utf8"),
		text,
		"compiled manual rule metadata/source hashes drifted;review policy before regenerating",
	);
console.log(
	JSON.stringify({
		ok: true,
		scope: "compiled manual rule bundle metadata and exact source hashes",
		bundles: bundles.length,
		artifact_sha256: createHash("sha256").update(text).digest("hex"),
	}),
);
