import { expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  parseScenarios,
  screenshotName,
  sha256,
  verifyEvidence,
  type EvidenceManifest,
} from "../../scripts/release/cockpit-evidence";

const scenarios = parseScenarios(
  join(
    import.meta.dir,
    "..",
    "..",
    "scripts",
    "release",
    "cockpit-evidence-scenarios.json",
  ),
);
// 1×1 RGBA PNG: the verifier checks the actual container structure, not a MIME claim.
const png = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAF/gJ+QvXW4QAAAABJRU5ErkJggg==",
  "base64",
);
function fixture() {
  const dir = mkdtempSync(join(tmpdir(), "rentemester-cockpit-evidence-"));
  const query = join(dir, "cockpit-epic-648-open-issues.json");
  writeFileSync(query, "[]");
  const artifacts = scenarios.map((s) => {
    const path = join(dir, screenshotName(s));
    writeFileSync(path, png);
    return {
      path: screenshotName(s),
      sha256: sha256(path),
      width: 1,
      height: 1,
    };
  });
  const manifest: EvidenceManifest = {
    manifestVersion: 2,
    commit: "a".repeat(40),
    image: `ghcr.io/example/rentemester@sha256:${"b".repeat(64)}`,
    imageDigest: `sha256:${"b".repeat(64)}`,
    generatedAt: "2026-01-01T00:00:00.000Z",
    runtime: {
      imageRepoDigest: `ghcr.io/example/rentemester@sha256:${"b".repeat(64)}`,
      repoDigests: [`ghcr.io/example/rentemester@sha256:${"b".repeat(64)}`],
      health: { gitCommit: "a".repeat(40) },
    },
    regressionQuery: {
      path: "cockpit-epic-648-open-issues.json",
      sha256: sha256(query),
      issues: [],
    },
    artifacts,
    scenarios: scenarios.map((s) => ({
      ...s,
      screenshot: screenshotName(s),
      keyboardAssertions: ["Tab: natural focus and UI state verified"],
      interception: {
        requested: s.interception?.urlPattern ?? null,
        actualRequests: s.interception ? [s.interception.urlPattern] : [],
      },
      consoleErrors: [],
      domAssertions: [s.dom.heading.selector],
    })),
  };
  const path = join(dir, "cockpit-evidence.json");
  writeFileSync(path, JSON.stringify(manifest));
  return { dir, manifest, path, query };
}
test("requires every state and responsive normal mode for every #649-#657 feature", () => {
  expect(scenarios).toHaveLength(63);
  for (const issue of [649, 650, 651, 652, 653, 654, 655, 656, 657])
    expect(scenarios.filter((s) => s.issue === issue)).toHaveLength(7);
});
test("verifies PNG structure, dimensions, one-to-one mapping and query checksum", () => {
  const value = fixture();
  try {
    expect(verifyEvidence(value.path).scenarios).toHaveLength(63);
    value.manifest.artifacts[0]!.width = 390;
    writeFileSync(value.path, JSON.stringify(value.manifest));
    expect(() => verifyEvidence(value.path)).toThrow("dimension mismatch");
    value.manifest.artifacts[0]!.width = 1;
    value.manifest.scenarios[1]!.screenshot =
      value.manifest.scenarios[0]!.screenshot;
    writeFileSync(value.path, JSON.stringify(value.manifest));
    expect(() => verifyEvidence(value.path)).toThrow("one-to-one");
    value.manifest.scenarios[1]!.screenshot = value.manifest.artifacts[1]!.path;
    writeFileSync(join(value.dir, "orphan.png"), png);
    writeFileSync(value.path, JSON.stringify(value.manifest));
    expect(() => verifyEvidence(value.path)).toThrow("unreferenced screenshot");
  } finally {
    rmSync(value.dir, { recursive: true, force: true });
  }
});
test("rejects fake PNG, swapped runtime identity, unsafe paths and open epic blockers", () => {
  const value = fixture();
  try {
    writeFileSync(
      join(value.dir, value.manifest.artifacts[0]!.path),
      "not a PNG",
    );
    writeFileSync(value.path, JSON.stringify(value.manifest));
    expect(() => verifyEvidence(value.path)).toThrow("PNG");
    writeFileSync(join(value.dir, value.manifest.artifacts[0]!.path), png);
    value.manifest.artifacts[0]!.sha256 = sha256(
      join(value.dir, value.manifest.artifacts[0]!.path),
    );
    value.manifest.runtime.health.gitCommit = "c".repeat(40);
    writeFileSync(value.path, JSON.stringify(value.manifest));
    expect(() => verifyEvidence(value.path)).toThrow("runtime build identity");
    value.manifest.runtime.health.gitCommit = "a".repeat(40);
    value.manifest.artifacts[0]!.path = "../escape.png";
    writeFileSync(value.path, JSON.stringify(value.manifest));
    expect(() => verifyEvidence(value.path)).toThrow("unsafe screenshot path");
    value.manifest.artifacts[0]!.path = screenshotName(scenarios[0]!);
    value.manifest.regressionQuery.issues = [
      {
        number: 999,
        title: "Synthetic blocker",
        labels: [{ name: "epic:648" }, { name: "severity:high" }],
      },
    ];
    writeFileSync(value.path, JSON.stringify(value.manifest));
    expect(() => verifyEvidence(value.path)).toThrow("#999 Synthetic blocker");
  } finally {
    rmSync(value.dir, { recursive: true, force: true });
  }
});
