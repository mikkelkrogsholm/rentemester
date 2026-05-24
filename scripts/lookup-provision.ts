import { extractProvisions } from "../src/core/legal-provisions";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const sourceId = process.argv[2];
const refQuery = process.argv[3];
if (!sourceId || !refQuery) {
  console.error("usage: bun run lookup-provision <sourceId> <ref>");
  process.exit(2);
}
const xmlPath = join("sources/downloaded", `${sourceId}.xml`);
const xml = readFileSync(xmlPath, "utf8");
const provisions = extractProvisions(xml, sourceId);
const matches = provisions.filter((p) => p.ref === refQuery || p.ref.startsWith(refQuery + ","));
if (matches.length === 0) {
  console.log("No exact match. Closest refs:");
  for (const p of provisions.slice(0, 20)) console.log(`  ${p.ref}`);
  process.exit(1);
}
for (const m of matches) {
  console.log(`ref: ${m.ref}`);
  // The Provision.textHash already has the "sha256:" prefix.
  console.log(`textHash: ${m.textHash}`);
  console.log(`text: ${m.text.slice(0, 200)}...`);
  console.log("---");
}
