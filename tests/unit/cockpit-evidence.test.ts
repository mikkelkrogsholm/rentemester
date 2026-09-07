import { expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  parseScenarios,
  screenshotName,
  sha256,
  verifyEvidence,
  type EvidenceManifest,
} from "../../scripts/release/cockpit-evidence";
import { createCompany } from "../../src/core/company";
import { initWorkspace } from "../../src/core/workspace";
import { handleRequest } from "../../src/server/router";

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
// Minimal dimension-bearing PNG for structural verifier tests.
function png(width: number, height: number) {
  const header = Buffer.alloc(25);
  header.writeUInt32BE(13, 0);
  header.write("IHDR", 4);
  header.writeUInt32BE(width, 8);
  header.writeUInt32BE(height, 12);
  header[16] = 8;
  header[17] = 6;
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), header,
    Buffer.from([0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130]),
  ]);
}
function fixture() {
  const dir = mkdtempSync(join(tmpdir(), "rentemester-cockpit-evidence-"));
  const query = join(dir, "cockpit-epic-648-open-issues.json");
  writeFileSync(query, "[]");
  const artifacts = scenarios.map((s) => {
    const path = join(dir, screenshotName(s));
    writeFileSync(path, png(s.capture.width, s.capture.height));
    return {
      path: screenshotName(s),
      sha256: sha256(path),
      width: s.capture.width,
      height: s.capture.height,
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
        actualRequests: [s.endpoint],
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
  const zoom = scenarios.find((s) => s.issue === 649 && s.mode === "zoom");
  expect(zoom?.viewport).toEqual({ width: 720, height: 450, deviceScaleFactor: 2 });
  expect(zoom?.capture).toEqual({ width: 1440, height: 900 });
});
test("rejects a manifest missing one issue-local state or responsive mode", () => {
  const value = fixture();
  const modeValue = fixture();
  try {
    value.manifest.scenarios = value.manifest.scenarios.filter(
      (scenario) => !(scenario.issue === 649 && scenario.state === "empty"),
    );
    writeFileSync(value.path, JSON.stringify(value.manifest));
    expect(() => verifyEvidence(value.path)).toThrow("missing empty scenario for #649");
    modeValue.manifest.scenarios = modeValue.manifest.scenarios.filter(
      (scenario) => !(scenario.issue === 650 && scenario.state === "normal" && scenario.mode === "zoom"),
    );
    writeFileSync(modeValue.path, JSON.stringify(modeValue.manifest));
    expect(() => verifyEvidence(modeValue.path)).toThrow("missing normal zoom scenario for #650");
  } finally {
    rmSync(value.dir, { recursive: true, force: true });
    rmSync(modeValue.dir, { recursive: true, force: true });
  }
});
test("rejects a manifest with a cross-feature route or endpoint mapping", () => {
  const value = fixture();
  try {
    value.manifest.scenarios[0]!.endpoint =
      "/api/companies/evidence-fixture/overview/changes";
    writeFileSync(value.path, JSON.stringify(value.manifest));
    expect(() => verifyEvidence(value.path)).toThrow(
      "wrong route or endpoint mapping for #649",
    );
  } finally {
    rmSync(value.dir, { recursive: true, force: true });
  }
});
test("expands only the nine exact feature profiles, resolves #651 live route, and rejects a wrong mapping", async () => {
  expect(scenarios.find((s) => s.issue === 650)?.route).toBe("/companies/evidence-fixture/batchbogfoering");
  expect(scenarios.find((s) => s.issue === 651)?.endpoint).toBe("/api/companies/evidence-fixture/changes-since?after=0");
  const workspace = mkdtempSync(join(tmpdir(), "rentemester-cockpit-route-"));
  try {
    initWorkspace(workspace);
    createCompany(workspace, { name: "Evidence Fixture", cvr: "DK90000000" });
    const response = await handleRequest(
      new Request("http://localhost/api/companies/evidence-fixture/changes-since?after=0"),
      { workspaceRoot: workspace, host: "127.0.0.1", port: 0, authRequired: false, authToken: null },
    );
    expect(response.status).toBe(200);
  } finally {
    rmSync(workspace, { recursive: true, force: true });
  }
  const dir = mkdtempSync(join(tmpdir(), "rentemester-cockpit-config-"));
  try {
    const config = JSON.parse(readFileSync(join(import.meta.dir, "..", "..", "scripts", "release", "cockpit-evidence-scenarios.json"), "utf8"));
    config.profiles[0].endpoint = "/api/companies/evidence-fixture/overview";
    const path = join(dir, "scenarios.json");
    writeFileSync(path, JSON.stringify(config));
    expect(() => parseScenarios(path)).toThrow("invalid issue profile for #649");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
test("verifies PNG structure, dimensions, one-to-one mapping and query checksum", () => {
  const value = fixture();
  try {
    expect(verifyEvidence(value.path).scenarios).toHaveLength(63);
    value.manifest.artifacts[0]!.width = 390;
    writeFileSync(value.path, JSON.stringify(value.manifest));
    expect(() => verifyEvidence(value.path)).toThrow("dimension mismatch");
    value.manifest.artifacts[0]!.width = value.manifest.scenarios[0]!.capture.width;
    value.manifest.scenarios[1]!.screenshot =
      value.manifest.scenarios[0]!.screenshot;
    writeFileSync(value.path, JSON.stringify(value.manifest));
    expect(() => verifyEvidence(value.path)).toThrow("one-to-one");
    value.manifest.scenarios[1]!.screenshot = value.manifest.artifacts[1]!.path;
    writeFileSync(join(value.dir, "orphan.png"), png(1, 1));
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
    writeFileSync(
      join(value.dir, value.manifest.artifacts[0]!.path),
      png(
        value.manifest.scenarios[0]!.capture.width,
        value.manifest.scenarios[0]!.capture.height,
      ),
    );
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
