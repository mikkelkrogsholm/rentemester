import { expect, test } from "bun:test";
import { mkdtempSync, rmSync, unlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { parseScenarios, screenshotName, sha256, verifyEvidence, type EvidenceManifest } from "../../scripts/release/cockpit-evidence";

const scenarios = parseScenarios(join(import.meta.dir, "..", "..", "scripts", "release", "cockpit-evidence-scenarios.json"));
function fixture(): { dir: string; manifest: EvidenceManifest; path: string } {
  const dir = mkdtempSync(join(tmpdir(), "rentemester-cockpit-evidence-"));
  const artifacts = scenarios.map((scenario) => {
    const path = join(dir, screenshotName(scenario));
    writeFileSync(path, "synthetic png bytes");
    return { path: screenshotName(scenario), sha256: sha256(path) };
  });
  const manifest: EvidenceManifest = {
    manifestVersion: 1,
    commit: "a".repeat(40),
    image: `ghcr.io/example/rentemester@sha256:${"b".repeat(64)}`,
    imageDigest: `sha256:${"b".repeat(64)}`,
    generatedAt: "2026-01-01T00:00:00.000Z",
    artifacts,
    scenarios: scenarios.map((scenario) => ({ ...scenario, screenshot: screenshotName(scenario), keyboardAssertions: ["Tab: #root focused"], interceptionDescription: scenario.interception ? "deterministic synthetic response" : "none" })),
  };
  const path = join(dir, "cockpit-evidence.json");
  writeFileSync(path, JSON.stringify(manifest));
  return { dir, manifest, path };
}

test("verifies the declarative #649-#657 Cockpit evidence contract", () => {
  const value = fixture();
  try { expect(verifyEvidence(value.path).scenarios).toHaveLength(9); } finally { rmSync(value.dir, { recursive: true, force: true }); }
});

test("rejects mutable image tags, absent screenshots, hash mismatch, and missing required scenarios", () => {
  const value = fixture();
  try {
    value.manifest.image = "ghcr.io/example/rentemester:candidate";
    writeFileSync(value.path, JSON.stringify(value.manifest));
    expect(() => verifyEvidence(value.path)).toThrow("mutable tags");
    value.manifest.image = `ghcr.io/example/rentemester@sha256:${"b".repeat(64)}`;
    unlinkSync(join(value.dir, value.manifest.artifacts[0]!.path));
    writeFileSync(value.path, JSON.stringify(value.manifest));
    expect(() => verifyEvidence(value.path)).toThrow("missing screenshot artifact");
    writeFileSync(join(value.dir, value.manifest.artifacts[0]!.path), "synthetic png bytes");
    value.manifest.artifacts[0]!.sha256 = `sha256:${"0".repeat(64)}`;
    writeFileSync(value.path, JSON.stringify(value.manifest));
    expect(() => verifyEvidence(value.path)).toThrow("SHA-256 mismatch");
    value.manifest.artifacts[0]!.sha256 = sha256(join(value.dir, value.manifest.artifacts[0]!.path));
    value.manifest.scenarios = value.manifest.scenarios.filter((scenario) => scenario.issue !== 657);
    writeFileSync(value.path, JSON.stringify(value.manifest));
    expect(() => verifyEvidence(value.path)).toThrow("#657");
  } finally { rmSync(value.dir, { recursive: true, force: true }); }
});

test("rejects an open high or critical related regression with its issue number", () => {
  const value = fixture();
  try {
    const regressions = join(value.dir, "open-regressions.json");
    writeFileSync(regressions, JSON.stringify([{ number: 999, title: "Synthetic blocker", labels: [{ name: "severity:high" }, { name: "regression" }] }]));
    const result = Bun.spawnSync(["bun", "run", "scripts/release/verify-cockpit-evidence.ts", value.path, regressions], { cwd: join(import.meta.dir, "..", ".."), stdout: "pipe", stderr: "pipe" });
    expect(result.exitCode).toBe(1);
    expect(new TextDecoder().decode(result.stderr)).toContain("#999 Synthetic blocker");
  } finally { rmSync(value.dir, { recursive: true, force: true }); }
});
