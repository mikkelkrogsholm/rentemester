#!/usr/bin/env bun
import { getReleaseProvenance } from "../../src/core/release-provenance";
import { isValidSemVer } from "../../src/core/semver";

function required(name: string): string {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required`);
  return value;
}

const version = required("RELEASE_VERSION").replace(/^v/, "");
const gitCommit = required("RELEASE_GIT_COMMIT");
const builtAt = required("RELEASE_BUILT_AT");
const imageRepository = required("RELEASE_IMAGE_REPOSITORY");
const imageDigest = required("RELEASE_IMAGE_DIGEST");
const workflowRunId = required("RELEASE_WORKFLOW_RUN_ID");
const workflowRunAttempt = Number(required("RELEASE_WORKFLOW_RUN_ATTEMPT"));
const sbomSha256 = required("RELEASE_SBOM_SHA256");
const supplyChainSha256 = required("RELEASE_SUPPLY_CHAIN_SHA256");
const agentDiscoverySha256 = required("RELEASE_AGENT_DISCOVERY_SHA256");
const cockpitEvidenceSha256 = required("RELEASE_COCKPIT_EVIDENCE_SHA256");
const cockpitRegressionQuerySha256 = required(
  "RELEASE_COCKPIT_REGRESSION_QUERY_SHA256",
);

if (!isValidSemVer(version)) {
  throw new Error(`invalid release SemVer: ${version}`);
}
if (version.includes("+")) {
  throw new Error(
    "release versions must not contain SemVer build metadata ('+')",
  );
}
if (!/^[0-9a-f]{40}$/i.test(gitCommit)) {
  throw new Error("RELEASE_GIT_COMMIT must be the full 40-character commit id");
}
if (!/^sha256:[0-9a-f]{64}$/i.test(imageDigest)) {
  throw new Error("RELEASE_IMAGE_DIGEST must be a sha256 OCI digest");
}
if (!/^sha256:[0-9a-f]{64}$/i.test(sbomSha256)) {
  throw new Error("RELEASE_SBOM_SHA256 must be a sha256 digest");
}
if (!/^sha256:[0-9a-f]{64}$/i.test(supplyChainSha256)) {
  throw new Error("RELEASE_SUPPLY_CHAIN_SHA256 must be a sha256 digest");
}
if (
  !/^sha256:[0-9a-f]{64}$/i.test(agentDiscoverySha256) ||
  !/^sha256:[0-9a-f]{64}$/i.test(cockpitEvidenceSha256) ||
  !/^sha256:[0-9a-f]{64}$/i.test(cockpitRegressionQuerySha256)
) {
  throw new Error("RELEASE_AGENT_DISCOVERY_SHA256 must be a sha256 digest");
}
if (Number.isNaN(Date.parse(builtAt))) {
  throw new Error("RELEASE_BUILT_AT must be an ISO-8601 timestamp");
}
if (!/^\d+$/.test(workflowRunId)) {
  throw new Error("RELEASE_WORKFLOW_RUN_ID must be numeric");
}
if (!Number.isSafeInteger(workflowRunAttempt) || workflowRunAttempt < 1) {
  throw new Error("RELEASE_WORKFLOW_RUN_ATTEMPT must be a positive integer");
}

const provenance = getReleaseProvenance();
if (provenance.product.version !== version) {
  throw new Error(
    `source version ${provenance.product.version} does not match requested ${version}`,
  );
}
if (provenance.product.gitCommit !== gitCommit) {
  throw new Error(
    "runtime provenance commit does not match RELEASE_GIT_COMMIT",
  );
}
if (provenance.product.builtAt !== builtAt) {
  throw new Error(
    "runtime provenance build timestamp does not match RELEASE_BUILT_AT",
  );
}
if (!provenance.product.bunVersion || !provenance.product.baseImageDigest) {
  throw new Error(
    "release runtime must declare Bun version and base image digest",
  );
}

const manifest = {
  manifestVersion: 1,
  release: {
    version,
    gitCommit,
    builtAt,
  },
  source: {
    repository: "https://github.com/mikkelkrogsholm/rentemester",
    ref: gitCommit,
  },
  workflow: {
    runId: workflowRunId,
    runAttempt: workflowRunAttempt,
  },
  image: {
    repository: imageRepository,
    digest: imageDigest,
    platforms: ["linux/amd64"],
  },
  runtime: {
    bunVersion: provenance.product.bunVersion,
    baseImageDigest: provenance.product.baseImageDigest,
  },
  evidence: {
    sbomSha256,
    supplyChainSha256,
    agentDiscoverySha256,
    cockpitEvidenceSha256,
    cockpitRegressionQuerySha256,
  },
  provenance,
};

process.stdout.write(`${JSON.stringify(manifest, null, 2)}\n`);
