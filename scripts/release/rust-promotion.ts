import { createHash } from "node:crypto";
import { lstatSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { assertRustManifest } from "./rust-manifest";

type Manifest = Record<string, any>;
const common = ["release-manifest.json", "sbom.spdx.json", "supply-chain-evidence.json", "agent-discovery-coverage-report.json"];
export const RUST_PROMOTION_FILES = ["rust-build-identity.json", "rust-package-evidence.json", "rust-license-evidence.json"];
const digest = (raw: Buffer) => createHash("sha256").update(raw).digest("hex");
function read(directory: string, name: string): Buffer {
  const path = join(directory, name);
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.size > 32 * 1024 * 1024) throw new Error(`Candidate requires bounded regular file ${name}`);
  return readFileSync(path);
}
/** Explicit historical v1 or strict Rust v2; never select a fallback after failure. */
export function promotionRuntime(manifest: Manifest, identityRaw?: string): "rust" | "bun" {
  if (manifest.manifestVersion === 2) {
    if (identityRaw === undefined) throw new Error("Rust v2 approval requires the exact packaged build-identity.json");
    assertRustManifest(manifest, identityRaw);
    return "rust";
  }
  if (manifest.manifestVersion === 1 && identityRaw === undefined &&
      typeof manifest.runtime?.bunVersion === "string" &&
      typeof manifest.runtime?.baseImageDigest === "string" &&
      manifest.runtime.kind === undefined && manifest.runtime.buildIdentitySha256 === undefined) return "bun";
  throw new Error("Explicit manifestVersion1 Bun or manifestVersion2 Rust identity required");
}
export function verifyPromotionArtifacts(directory: string, expected: { commit: string; runId: string; runAttempt: string }) {
  const manifest: Manifest = JSON.parse(read(directory, "release-manifest.json").toString("utf8"));
  const identityRaw = manifest.manifestVersion === 2 ? read(directory, "rust-build-identity.json").toString("utf8") : undefined;
  const runtime = promotionRuntime(manifest, identityRaw);
  if (!/^[0-9a-f]{40}$/.test(expected.commit) || !/^[0-9]+$/.test(expected.runId) || !/^[1-9][0-9]*$/.test(expected.runAttempt) ||
      manifest.release?.gitCommit !== expected.commit || manifest.workflow?.runId !== expected.runId ||
      String(manifest.workflow?.runAttempt) !== expected.runAttempt ||
      manifest.image?.repository !== "ghcr.io/mikkelkrogsholm/rentemester") throw new Error("Candidate evidence does not match trusted run/source/repository");
  for (const name of [...common, ...(runtime === "rust" ? RUST_PROMOTION_FILES : [])]) {
    const raw = read(directory, name);
    const hash = digest(raw);
    const checksum = read(directory, name + ".sha256").toString("utf8");
    if (checksum !== `${hash}  ${name}\n`) throw new Error(`Candidate checksum mismatch: ${name}`);
    const field = ({"sbom.spdx.json":"sbomSha256", "supply-chain-evidence.json":"supplyChainSha256", "agent-discovery-coverage-report.json":"agentDiscoverySha256"} as Record<string,string>)[name];
    if (field && manifest.evidence?.[field] !== `sha256:${hash}`) throw new Error(`Candidate manifest evidence mismatch: ${name}`);
  }
  return { runtime, manifestVersion: manifest.manifestVersion };
}
if (import.meta.main) {
  if (process.argv.length !== 6) throw new Error("usage: rust-promotion.ts EVIDENCE_DIRECTORY COMMIT RUN_ID RUN_ATTEMPT");
  const result = verifyPromotionArtifacts(process.argv[2]!, {commit: process.argv[3]!, runId: process.argv[4]!, runAttempt: process.argv[5]!});
  process.stdout.write(`runtime=${result.runtime}\nmanifest_version=${result.manifestVersion}\n`);
}
