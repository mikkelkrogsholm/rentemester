import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { describe, expect, test } from "bun:test";
import { getGhcrManifestStatus } from "../../scripts/release/registry-manifest-status";

const repositoryRoot = join(import.meta.dir, "..", "..");
const commit = "a".repeat(40);
const imageDigest = `sha256:${"b".repeat(64)}`;
const builtAt = "2026-07-19T12:00:00.000Z";
const bunVersion = "1.4.0";
const baseImageDigest = `sha256:${"d".repeat(64)}`;
const sbomSha256 = `sha256:${"e".repeat(64)}`;
const supplyChainSha256 = `sha256:${"f".repeat(64)}`;
const agentDiscoverySha256 = `sha256:${"1".repeat(64)}`;
const cockpitEvidenceSha256 = `sha256:${"2".repeat(64)}`;
const cockpitRegressionQuerySha256 = `sha256:${"3".repeat(64)}`;

function runScript(script: string, args: string[], env: NodeJS.ProcessEnv = {}) {
  return spawnSync("bun", ["run", script, ...args], {
    cwd: repositoryRoot,
    env: { ...process.env, ...env },
    encoding: "utf8",
  });
}

describe("release evidence scripts", () => {
  test("classifies only GHCR HTTP 404 as an absent manifest", async () => {
    const requests: Array<{ url: string; init?: RequestInit }> = [];
    const absentFetch = (async (input: string | URL | Request, init?: RequestInit) => {
      requests.push({ url: String(input), init });
      if (requests.length === 1) {
        return new Response(JSON.stringify({ token: "registry-bearer" }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      return new Response(null, { status: 404 });
    }) as typeof fetch;

    await expect(
      getGhcrManifestStatus(
        "https://ghcr.io/mikkelkrogsholm/rentemester",
        "v0.1.0",
        "release-actor",
        "workflow-token",
        absentFetch,
      ),
    ).resolves.toBe(404);
    expect(requests[0]?.url).toContain("scope=repository%3Amikkelkrogsholm%2Frentemester%3Apull");
    expect(requests[1]?.init?.method).toBe("HEAD");
    expect(new Headers(requests[1]?.init?.headers).get("authorization")).toBe(
      "Bearer registry-bearer",
    );

    const unauthorizedFetch = (async (_input: string | URL | Request, _init?: RequestInit) => {
      if (!_init?.method) {
        return new Response(JSON.stringify({ token: "registry-bearer" }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      return new Response(null, { status: 401 });
    }) as typeof fetch;
    await expect(
      getGhcrManifestStatus(
        "https://ghcr.io/mikkelkrogsholm/rentemester",
        "v0.1.0",
        "release-actor",
        "workflow-token",
        unauthorizedFetch,
      ),
    ).rejects.toThrow("HTTP 401");
  });

  test("rejects SemVer forms with leading zeroes or numeric prerelease zeroes", () => {
    for (const invalid of ["01.0.0", "1.01.0", "1.0.0-01", "1.0.0-.."]) {
      const checked = runScript("scripts/release/check-version.ts", [invalid]);
      expect(checked.status).not.toBe(0);
      expect(checked.stderr).toContain("not valid SemVer");
    }

    const created = runScript("scripts/release/create-manifest.ts", [], {
      RELEASE_VERSION: "1.0.0-01",
      RELEASE_GIT_COMMIT: commit,
      RELEASE_BUILT_AT: builtAt,
      RELEASE_IMAGE_REPOSITORY: "ghcr.io/mikkelkrogsholm/rentemester",
      RELEASE_IMAGE_DIGEST: imageDigest,
      RELEASE_WORKFLOW_RUN_ID: "123456789",
      RELEASE_WORKFLOW_RUN_ATTEMPT: "1",
      RENTEMESTER_GIT_COMMIT: commit,
      RENTEMESTER_BUILT_AT: builtAt,
      RENTEMESTER_BUN_VERSION: bunVersion,
      RENTEMESTER_BASE_IMAGE_DIGEST: baseImageDigest,
      RELEASE_SBOM_SHA256: sbomSha256,
      RELEASE_SUPPLY_CHAIN_SHA256: supplyChainSha256,
      RELEASE_AGENT_DISCOVERY_SHA256: agentDiscoverySha256,
      RELEASE_COCKPIT_EVIDENCE_SHA256: cockpitEvidenceSha256,
      RELEASE_COCKPIT_REGRESSION_QUERY_SHA256: cockpitRegressionQuerySha256,
    });
    expect(created.status).not.toBe(0);
    expect(created.stderr).toContain("invalid release SemVer");
  });

  test("creates a manifest and accepts only an exactly bound Digisense approval", () => {
    const directory = mkdtempSync(join(tmpdir(), "rentemester-release-evidence-"));
    try {
      const created = runScript("scripts/release/create-manifest.ts", [], {
        RELEASE_VERSION: "0.2.0",
        RELEASE_GIT_COMMIT: commit,
        RELEASE_BUILT_AT: builtAt,
        RELEASE_IMAGE_REPOSITORY: "ghcr.io/mikkelkrogsholm/rentemester",
        RELEASE_IMAGE_DIGEST: imageDigest,
        RELEASE_WORKFLOW_RUN_ID: "123456789",
        RELEASE_WORKFLOW_RUN_ATTEMPT: "1",
        RENTEMESTER_GIT_COMMIT: commit,
        RENTEMESTER_BUILT_AT: builtAt,
        RENTEMESTER_BUN_VERSION: bunVersion,
        RENTEMESTER_BASE_IMAGE_DIGEST: baseImageDigest,
        RELEASE_SBOM_SHA256: sbomSha256,
        RELEASE_SUPPLY_CHAIN_SHA256: supplyChainSha256,
        RELEASE_AGENT_DISCOVERY_SHA256: agentDiscoverySha256,
        RELEASE_COCKPIT_EVIDENCE_SHA256: cockpitEvidenceSha256,
        RELEASE_COCKPIT_REGRESSION_QUERY_SHA256: cockpitRegressionQuerySha256,
      });
      expect(created.status).toBe(0);
      expect(JSON.parse(created.stdout).workflow).toEqual({
        runId: "123456789",
        runAttempt: 1,
      });
      expect(JSON.parse(created.stdout).runtime).toEqual({ bunVersion, baseImageDigest });
      expect(JSON.parse(created.stdout).evidence).toEqual({ sbomSha256, supplyChainSha256, agentDiscoverySha256, cockpitEvidenceSha256, cockpitRegressionQuerySha256 });
      const manifestPath = join(directory, "release-manifest.json");
      writeFileSync(manifestPath, created.stdout);
      const releaseManifestDigest = `sha256:${createHash("sha256")
        .update(created.stdout)
        .digest("hex")}`;
      const approval = {
        schemaVersion: 1,
        approvalId: "DIGISENSE-TEST-1",
        decision: "approved",
        approvedAt: "2026-07-19T13:00:00.000Z",
        reviewer: { organization: "Digisense", name: "Test reviewer" },
        releaseManifestDigest,
        imageDigest,
        version: "0.2.0",
        gitCommit: commit,
      };
      const approvalPath = join(directory, "approval.json");
      writeFileSync(approvalPath, `${JSON.stringify(approval, null, 2)}\n`);

      const verified = runScript(
        "scripts/release/verify-approval.ts",
        [manifestPath, approvalPath],
      );
      expect(verified.status).toBe(0);
      expect(verified.stdout).toContain("Digisense approved 0.2.0");

      const manifest = JSON.parse(created.stdout);
      manifest.evidence.cockpitEvidenceSha256 = "sha256:not-a-checksum";
      writeFileSync(manifestPath, JSON.stringify(manifest));
      const invalidCockpitEvidence = runScript(
        "scripts/release/verify-approval.ts",
        [manifestPath, approvalPath],
      );
      expect(invalidCockpitEvidence.status).not.toBe(0);
      expect(invalidCockpitEvidence.stderr).toContain("evidence checksums are invalid");
      writeFileSync(manifestPath, created.stdout);

      writeFileSync(
        approvalPath,
        `${JSON.stringify({ ...approval, imageDigest: `sha256:${"c".repeat(64)}` })}\n`,
      );
      const rejected = runScript(
        "scripts/release/verify-approval.ts",
        [manifestPath, approvalPath],
      );
      expect(rejected.status).not.toBe(0);
      expect(rejected.stderr).toContain("imageDigest does not match");

      for (const invalidApproval of [
        { ...approval, extra: true },
        { ...approval, reviewer: { organization: "digisense", name: "Test reviewer" } },
        { ...approval, approvedAt: "July 19, 2026 13:00:00 UTC" },
        { ...approval, approvedAt: "2027-02-30T13:00:00Z" },
        { ...approval, approvedAt: "2026-01-01T24:00:00Z" },
      ]) {
        writeFileSync(approvalPath, `${JSON.stringify(invalidApproval)}\n`);
        const invalid = runScript(
          "scripts/release/verify-approval.ts",
          [manifestPath, approvalPath],
        );
        expect(invalid.status).not.toBe(0);
      }
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  test("records the raw SHA-256 values of the Cockpit evidence fixtures", () => {
    const directory = mkdtempSync(join(tmpdir(), "rentemester-release-cockpit-"));
    try {
      const evidencePath = join(directory, "cockpit-evidence.json");
      const regressionQueryPath = join(directory, "cockpit-epic-648-open-issues.json");
      writeFileSync(evidencePath, '{"fixture":"cockpit evidence"}\n');
      writeFileSync(regressionQueryPath, "[]\n");
      const fixtureSha256 = (path: string) =>
        `sha256:${createHash("sha256").update(readFileSync(path)).digest("hex")}`;
      const cockpitEvidenceSha256 = fixtureSha256(evidencePath);
      const cockpitRegressionQuerySha256 = fixtureSha256(regressionQueryPath);
      const created = runScript("scripts/release/create-manifest.ts", [], {
        RELEASE_VERSION: "0.2.0",
        RELEASE_GIT_COMMIT: commit,
        RELEASE_BUILT_AT: builtAt,
        RELEASE_IMAGE_REPOSITORY: "ghcr.io/example/rentemester",
        RELEASE_IMAGE_DIGEST: imageDigest,
        RELEASE_WORKFLOW_RUN_ID: "123456789",
        RELEASE_WORKFLOW_RUN_ATTEMPT: "1",
        RENTEMESTER_GIT_COMMIT: commit,
        RENTEMESTER_BUILT_AT: builtAt,
        RENTEMESTER_BUN_VERSION: bunVersion,
        RENTEMESTER_BASE_IMAGE_DIGEST: baseImageDigest,
        RELEASE_SBOM_SHA256: sbomSha256,
        RELEASE_SUPPLY_CHAIN_SHA256: supplyChainSha256,
        RELEASE_AGENT_DISCOVERY_SHA256: agentDiscoverySha256,
        RELEASE_COCKPIT_EVIDENCE_SHA256: cockpitEvidenceSha256,
        RELEASE_COCKPIT_REGRESSION_QUERY_SHA256: cockpitRegressionQuerySha256,
      });

      expect(created.status).toBe(0);
      expect(JSON.parse(created.stdout).evidence).toMatchObject({
        cockpitEvidenceSha256,
        cockpitRegressionQuerySha256,
      });
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  test("refuses release evidence without an explicit runtime identity", () => {
    const created = runScript("scripts/release/create-manifest.ts", [], {
      RELEASE_VERSION: "0.2.0",
      RELEASE_GIT_COMMIT: commit,
      RELEASE_BUILT_AT: builtAt,
      RELEASE_IMAGE_REPOSITORY: "ghcr.io/mikkelkrogsholm/rentemester",
      RELEASE_IMAGE_DIGEST: imageDigest,
      RELEASE_WORKFLOW_RUN_ID: "123456789",
      RELEASE_WORKFLOW_RUN_ATTEMPT: "1",
      RENTEMESTER_GIT_COMMIT: commit,
      RENTEMESTER_BUILT_AT: builtAt,
      RENTEMESTER_BUN_VERSION: "",
      RENTEMESTER_BASE_IMAGE_DIGEST: "",
      RELEASE_SBOM_SHA256: sbomSha256,
      RELEASE_SUPPLY_CHAIN_SHA256: supplyChainSha256,
      RELEASE_AGENT_DISCOVERY_SHA256: agentDiscoverySha256,
      RELEASE_COCKPIT_EVIDENCE_SHA256: cockpitEvidenceSha256,
      RELEASE_COCKPIT_REGRESSION_QUERY_SHA256: cockpitRegressionQuerySha256,
    });
    expect(created.status).not.toBe(0);
    expect(created.stderr).toContain("runtime must declare Bun version and base image digest");
  });

  test("refuses release evidence without a checksum for the extracted SBOM", () => {
    const created = runScript("scripts/release/create-manifest.ts", [], {
      RELEASE_VERSION: "0.2.0",
      RELEASE_GIT_COMMIT: commit,
      RELEASE_BUILT_AT: builtAt,
      RELEASE_IMAGE_REPOSITORY: "ghcr.io/mikkelkrogsholm/rentemester",
      RELEASE_IMAGE_DIGEST: imageDigest,
      RELEASE_WORKFLOW_RUN_ID: "123456789",
      RELEASE_WORKFLOW_RUN_ATTEMPT: "1",
      RENTEMESTER_GIT_COMMIT: commit,
      RENTEMESTER_BUILT_AT: builtAt,
      RENTEMESTER_BUN_VERSION: bunVersion,
      RENTEMESTER_BASE_IMAGE_DIGEST: baseImageDigest,
      RELEASE_SBOM_SHA256: "",
      RELEASE_SUPPLY_CHAIN_SHA256: supplyChainSha256,
      RELEASE_AGENT_DISCOVERY_SHA256: agentDiscoverySha256,
      RELEASE_COCKPIT_EVIDENCE_SHA256: cockpitEvidenceSha256,
      RELEASE_COCKPIT_REGRESSION_QUERY_SHA256: cockpitRegressionQuerySha256,
    });
    expect(created.status).not.toBe(0);
    expect(created.stderr).toContain("RELEASE_SBOM_SHA256 is required");
  });

  test("refuses release evidence without the lockfile-bound supply-chain checksum", () => {
    const created = runScript("scripts/release/create-manifest.ts", [], {
      RELEASE_VERSION: "0.2.0",
      RELEASE_GIT_COMMIT: commit,
      RELEASE_BUILT_AT: builtAt,
      RELEASE_IMAGE_REPOSITORY: "ghcr.io/mikkelkrogsholm/rentemester",
      RELEASE_IMAGE_DIGEST: imageDigest,
      RELEASE_WORKFLOW_RUN_ID: "123456789",
      RELEASE_WORKFLOW_RUN_ATTEMPT: "1",
      RENTEMESTER_GIT_COMMIT: commit,
      RENTEMESTER_BUILT_AT: builtAt,
      RENTEMESTER_BUN_VERSION: bunVersion,
      RENTEMESTER_BASE_IMAGE_DIGEST: baseImageDigest,
      RELEASE_SBOM_SHA256: sbomSha256,
      RELEASE_SUPPLY_CHAIN_SHA256: "",
      RELEASE_AGENT_DISCOVERY_SHA256: agentDiscoverySha256,
      RELEASE_COCKPIT_EVIDENCE_SHA256: cockpitEvidenceSha256,
      RELEASE_COCKPIT_REGRESSION_QUERY_SHA256: cockpitRegressionQuerySha256,
    });
    expect(created.status).not.toBe(0);
    expect(created.stderr).toContain("RELEASE_SUPPLY_CHAIN_SHA256 is required");
  });

  test("refuses release evidence without the digest-bound agent-discovery checksum", () => {
    const created = runScript("scripts/release/create-manifest.ts", [], {
      RELEASE_VERSION: "0.2.0",
      RELEASE_GIT_COMMIT: commit,
      RELEASE_BUILT_AT: builtAt,
      RELEASE_IMAGE_REPOSITORY: "ghcr.io/mikkelkrogsholm/rentemester",
      RELEASE_IMAGE_DIGEST: imageDigest,
      RELEASE_WORKFLOW_RUN_ID: "123456789",
      RELEASE_WORKFLOW_RUN_ATTEMPT: "1",
      RENTEMESTER_GIT_COMMIT: commit,
      RENTEMESTER_BUILT_AT: builtAt,
      RENTEMESTER_BUN_VERSION: bunVersion,
      RENTEMESTER_BASE_IMAGE_DIGEST: baseImageDigest,
      RELEASE_SBOM_SHA256: sbomSha256,
      RELEASE_SUPPLY_CHAIN_SHA256: supplyChainSha256,
      RELEASE_AGENT_DISCOVERY_SHA256: "",
      RELEASE_COCKPIT_EVIDENCE_SHA256: cockpitEvidenceSha256,
      RELEASE_COCKPIT_REGRESSION_QUERY_SHA256: cockpitRegressionQuerySha256,
    });
    expect(created.status).not.toBe(0);
    expect(created.stderr).toContain("RELEASE_AGENT_DISCOVERY_SHA256 is required");
  });
});
