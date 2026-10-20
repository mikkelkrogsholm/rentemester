#!/usr/bin/env bun
import { readFileSync } from "node:fs";
import { createRustManifest } from "./rust-manifest";
function required(name: string): string {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required`);
  return value;
}
if (process.argv.length !== 3) throw new Error("usage: create-rust-manifest.ts <packaged-build-identity.json>");
const manifest = createRustManifest(readFileSync(process.argv[2]!, "utf8"), {
  version: required("RELEASE_VERSION").replace(/^v/, ""),
  gitCommit: required("RELEASE_GIT_COMMIT"),
  builtAt: required("RELEASE_BUILT_AT"),
  imageRepository: required("RELEASE_IMAGE_REPOSITORY"),
  imageDigest: required("RELEASE_IMAGE_DIGEST"),
  workflowRunId: required("RELEASE_WORKFLOW_RUN_ID"),
  workflowRunAttempt: Number(required("RELEASE_WORKFLOW_RUN_ATTEMPT")),
  sbomSha256: required("RELEASE_SBOM_SHA256"),
  supplyChainSha256: required("RELEASE_SUPPLY_CHAIN_SHA256"),
  agentDiscoverySha256: required("RELEASE_AGENT_DISCOVERY_SHA256"),
});
process.stdout.write(`${JSON.stringify(manifest, null, 2)}\n`);
