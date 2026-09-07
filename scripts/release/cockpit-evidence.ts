import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { basename, resolve } from "node:path";

export const COCKPIT_EVIDENCE_VERSION = 1;
export const REQUIRED_ISSUES = [649, 650, 651, 652, 653, 654, 655, 656, 657] as const;
export const REQUIRED_STATES = ["normal", "loading", "empty", "warning-or-blocked", "error"] as const;

export type Scenario = {
  issue: number;
  scenario: string;
  route: string;
  state: (typeof REQUIRED_STATES)[number];
  viewport: { width: number; height: number };
  zoom: number;
  keyboard: { selector: string; key: string; expectFocused: boolean }[];
  interception?: { urlIncludes: string; status: 200 | 403 | 500; body: string; delayMs?: number };
};

export type EvidenceArtifact = {
  path: string;
  sha256: string;
};

export type EvidenceManifest = {
  manifestVersion: number;
  commit: string;
  image: string;
  imageDigest: string;
  generatedAt: string;
  artifacts: EvidenceArtifact[];
  scenarios: Array<Scenario & { screenshot: string; keyboardAssertions: string[]; interceptionDescription: string }>;
};

export function sha256(path: string): string {
  return `sha256:${createHash("sha256").update(readFileSync(path)).digest("hex")}`;
}

export function assertImmutableImage(image: string): void {
  if (!/^ghcr\.io\/[a-z0-9][a-z0-9._/-]*@sha256:[0-9a-f]{64}$/i.test(image)) {
    throw new Error("candidate image must be an immutable ghcr.io repository@sha256:<64-hex-digest>; mutable tags are forbidden");
  }
}

export function parseScenarios(path: string): Scenario[] {
  const value: unknown = JSON.parse(readFileSync(path, "utf8"));
  if (!Array.isArray(value) || value.length === 0) throw new Error("scenario list must be a non-empty JSON array");
  const scenarios = value as Scenario[];
  for (const scenario of scenarios) {
    if (!REQUIRED_ISSUES.includes(scenario.issue as (typeof REQUIRED_ISSUES)[number])) throw new Error(`scenario ${scenario.scenario} has unsupported issue #${scenario.issue}`);
    if (!scenario.scenario || !scenario.route.startsWith("/") || !REQUIRED_STATES.includes(scenario.state)) throw new Error(`scenario for #${scenario.issue} is missing scenario, route, or valid state`);
    if (!Number.isInteger(scenario.viewport?.width) || !Number.isInteger(scenario.viewport?.height) || scenario.viewport.width < 320 || scenario.zoom < 1) throw new Error(`scenario ${scenario.scenario} has invalid viewport or zoom`);
    if (!Array.isArray(scenario.keyboard) || scenario.keyboard.length === 0) throw new Error(`scenario ${scenario.scenario} must declare keyboard assertions`);
    if (scenario.interception && ![200, 403, 500].includes(scenario.interception.status)) throw new Error(`scenario ${scenario.scenario} interception may only present deterministic loading, 403, or error responses`);
  }
  return scenarios;
}

function assertScenarios(scenarios: EvidenceManifest["scenarios"]): void {
  const issues = new Set(scenarios.map((scenario) => scenario.issue));
  const states = new Set(scenarios.map((scenario) => scenario.state));
  const missingIssues = REQUIRED_ISSUES.filter((issue) => !issues.has(issue));
  const missingStates = REQUIRED_STATES.filter((state) => !states.has(state));
  if (missingIssues.length) throw new Error(`missing required Epic #648 issue scenarios: ${missingIssues.map((issue) => `#${issue}`).join(", ")}`);
  if (missingStates.length) throw new Error(`missing required Cockpit states: ${missingStates.join(", ")}`);
  if (!scenarios.some((scenario) => scenario.viewport.width === 390)) throw new Error("missing required 390px viewport scenario");
  if (!scenarios.some((scenario) => scenario.zoom === 2)) throw new Error("missing required 200 percent zoom scenario");
  if (!scenarios.some((scenario) => scenario.viewport.width > 390)) throw new Error("missing required desktop viewport scenario");
}

export function verifyEvidence(manifestPath: string): EvidenceManifest {
  const absoluteManifestPath = resolve(manifestPath);
  const manifest = JSON.parse(readFileSync(absoluteManifestPath, "utf8")) as EvidenceManifest;
  if (manifest.manifestVersion !== COCKPIT_EVIDENCE_VERSION) throw new Error("unsupported cockpit evidence manifestVersion");
  if (!/^[0-9a-f]{40}$/i.test(manifest.commit)) throw new Error("cockpit evidence commit must be a full 40-character commit id");
  assertImmutableImage(manifest.image);
  if (!/^sha256:[0-9a-f]{64}$/i.test(manifest.imageDigest) || !manifest.image.endsWith(`@${manifest.imageDigest}`)) throw new Error("cockpit evidence imageDigest must match the immutable image reference");
  if (!Array.isArray(manifest.artifacts) || !Array.isArray(manifest.scenarios)) throw new Error("cockpit evidence must contain artifacts and scenarios arrays");
  assertScenarios(manifest.scenarios);
  const artifactPaths = new Set<string>();
  for (const artifact of manifest.artifacts) {
    if (!artifact.path.endsWith(".png")) throw new Error(`evidence artifact must be a PNG screenshot: ${artifact.path}`);
    if (artifactPaths.has(artifact.path)) throw new Error(`duplicate evidence artifact: ${artifact.path}`);
    artifactPaths.add(artifact.path);
    const absolute = resolve(resolve(absoluteManifestPath, ".."), artifact.path);
    if (!existsSync(absolute)) throw new Error(`missing screenshot artifact: ${artifact.path}`);
    if (!/^sha256:[0-9a-f]{64}$/i.test(artifact.sha256) || sha256(absolute) !== artifact.sha256) throw new Error(`screenshot SHA-256 mismatch: ${artifact.path}`);
  }
  for (const scenario of manifest.scenarios) {
    if (!artifactPaths.has(scenario.screenshot)) throw new Error(`scenario ${scenario.scenario} is missing its screenshot artifact`);
    if (!Array.isArray(scenario.keyboardAssertions) || scenario.keyboardAssertions.length === 0) throw new Error(`scenario ${scenario.scenario} is missing keyboard assertion results`);
    if (typeof scenario.interceptionDescription !== "string") throw new Error(`scenario ${scenario.scenario} is missing interception description`);
  }
  return manifest;
}

export function screenshotName(scenario: Scenario): string {
  return `${scenario.issue}-${scenario.scenario.replace(/[^a-z0-9]+/gi, "-").replace(/^-|-$/g, "")}.png`;
}

export function relativeArtifact(path: string): string {
  return basename(path);
}
