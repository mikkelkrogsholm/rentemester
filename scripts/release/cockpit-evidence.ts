import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { isAbsolute, resolve, sep } from "node:path";

export const COCKPIT_EVIDENCE_VERSION = 2;
export const REQUIRED_ISSUES = [
  649, 650, 651, 652, 653, 654, 655, 656, 657,
] as const;
export const REQUIRED_STATES = [
  "normal",
  "loading",
  "empty",
  "warning-or-blocked",
  "error",
] as const;
export const REQUIRED_MODES = ["desktop", "mobile", "zoom"] as const;
type State = (typeof REQUIRED_STATES)[number];

export type DomAssertion = {
  selector: string;
  text?: string;
  visible?: boolean;
};
export type Scenario = {
  issue: number;
  scenario: string;
  owner: string;
  route: string;
  state: State;
  mode: (typeof REQUIRED_MODES)[number];
  viewport: { width: number; height: number };
  zoom: number;
  dom: {
    heading: DomAssertion;
    status: DomAssertion;
    controls: DomAssertion[];
    data: DomAssertion[];
    coreAction: DomAssertion;
    noHorizontalOverflow: true;
  };
  keyboard: {
    key: "Tab" | "Shift+Tab" | "Enter" | "Space";
    expectFocus: DomAssertion;
    expectState: DomAssertion;
  }[];
  interception?: {
    urlPattern: string;
    status: 200 | 403 | 500;
    body: string;
    delayMs?: number;
  };
};
export type EvidenceArtifact = {
  path: string;
  sha256: string;
  width: number;
  height: number;
};
export type RuntimeIdentity = {
  imageRepoDigest: string;
  repoDigests: string[];
  health: { gitCommit: string; version?: string };
};
export type EvidenceManifest = {
  manifestVersion: number;
  commit: string;
  image: string;
  imageDigest: string;
  generatedAt: string;
  runtime: RuntimeIdentity;
  artifacts: EvidenceArtifact[];
  regressionQuery: {
    path: string;
    sha256: string;
    issues: Array<{
      number?: number;
      title?: string;
      labels?: Array<{ name?: string }>;
    }>;
  };
  scenarios: Array<
    Omit<Scenario, "interception"> & {
      screenshot: string;
      keyboardAssertions: string[];
      interception: { requested: string | null; actualRequests: string[] };
      consoleErrors: string[];
      domAssertions: string[];
    }
  >;
};
export function sha256(path: string): string {
  return `sha256:${createHash("sha256").update(readFileSync(path)).digest("hex")}`;
}
export function assertImmutableImage(image: string): void {
  if (!/^ghcr\.io\/[a-z0-9][a-z0-9._/-]*@sha256:[0-9a-f]{64}$/i.test(image))
    throw new Error(
      "candidate image must be an immutable ghcr.io repository@sha256:<64-hex-digest>; mutable tags are forbidden",
    );
}
function safeRelative(path: string): boolean {
  return !!path && !isAbsolute(path) && !path.split(/[\\/]/).includes("..");
}
export function pngDimensions(path: string): { width: number; height: number } {
  const value = readFileSync(path);
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  if (value.length < 45 || !value.subarray(0, 8).equals(signature))
    throw new Error(`invalid PNG signature: ${path}`);
  let offset = 8;
  let ihdr: { width: number; height: number } | undefined;
  let ended = false;
  while (offset + 12 <= value.length) {
    const size = value.readUInt32BE(offset);
    const type = value.subarray(offset + 4, offset + 8).toString("ascii");
    if (offset + 12 + size > value.length)
      throw new Error(`invalid PNG chunk length: ${path}`);
    if (!ihdr) {
      if (type !== "IHDR" || size !== 13)
        throw new Error(`PNG must begin with IHDR: ${path}`);
      ihdr = {
        width: value.readUInt32BE(offset + 8),
        height: value.readUInt32BE(offset + 12),
      };
      if (!ihdr.width || !ihdr.height)
        throw new Error(`invalid PNG IHDR dimensions: ${path}`);
    }
    if (type === "IEND") {
      if (size !== 0 || offset + 12 !== value.length)
        throw new Error(`invalid PNG IEND: ${path}`);
      ended = true;
      break;
    }
    offset += 12 + size;
  }
  if (!ihdr || !ended) throw new Error(`PNG is missing IHDR or IEND: ${path}`);
  return ihdr;
}
export function parseScenarios(path: string): Scenario[] {
  const value: unknown = JSON.parse(readFileSync(path, "utf8"));
  if (!Array.isArray(value))
    throw new Error("scenario list must be a non-empty JSON array");
  const scenarios = value as Scenario[];
  for (const s of scenarios) {
    if (
      !REQUIRED_ISSUES.includes(s.issue as never) ||
      !s.scenario ||
      !s.owner ||
      !s.route.startsWith("/") ||
      !REQUIRED_STATES.includes(s.state) ||
      !REQUIRED_MODES.includes(s.mode)
    )
      throw new Error(`invalid scenario contract for #${s.issue}`);
    if (
      !Number.isInteger(s.viewport?.width) ||
      !Number.isInteger(s.viewport?.height) ||
      s.viewport.width < 390 ||
      s.zoom < 1
    )
      throw new Error(`invalid viewport for ${s.scenario}`);
    if (
      (s.mode === "desktop" && (s.viewport.width <= 390 || s.zoom !== 1)) ||
      (s.mode === "mobile" && (s.viewport.width !== 390 || s.zoom !== 1)) ||
      (s.mode === "zoom" && s.zoom !== 2)
    )
      throw new Error(`mode dimensions must be explicit for ${s.scenario}`);
    if (
      !s.dom?.heading?.selector ||
      !s.dom.status?.selector ||
      !s.dom.coreAction?.selector ||
      !s.dom.noHorizontalOverflow ||
      !s.dom.controls?.length ||
      !s.dom.data?.length ||
      !s.keyboard?.length
    )
      throw new Error(
        `page-local DOM and keyboard contract required for ${s.scenario}`,
      );
    if (
      s.interception &&
      (!/^\/api\/[a-z0-9/:?=&._-]+$/i.test(s.interception.urlPattern) ||
        s.interception.urlPattern === "/api/health" ||
        s.interception.urlPattern.includes("identity"))
    )
      throw new Error(
        `interception must be exact, route-specific, and exclude health/identity for ${s.scenario}`,
      );
    if (
      ["loading", "warning-or-blocked", "error"].includes(s.state) !==
      !!s.interception
    )
      throw new Error(
        `state-to-interception semantics required for ${s.scenario}`,
      );
  }
  for (const issue of REQUIRED_ISSUES) {
    const mine = scenarios.filter((s) => s.issue === issue);
    for (const state of REQUIRED_STATES)
      if (!mine.some((s) => s.state === state))
        throw new Error(`missing ${state} scenario for #${issue}`);
    for (const mode of REQUIRED_MODES)
      if (!mine.some((s) => s.state === "normal" && s.mode === mode))
        throw new Error(`missing normal ${mode} scenario for #${issue}`);
    if (!mine.some((s) => s.keyboard.length))
      throw new Error(`missing keyboard trace for #${issue}`);
  }
  return scenarios;
}
function assertScenarios(scenarios: Array<Pick<Scenario, "issue" | "state" | "mode">>): void {
  parseScenariosValue(scenarios);
}
function parseScenariosValue(scenarios: Array<Pick<Scenario, "issue" | "state" | "mode">>): void {
  const temp = "/tmp";
  /* keep manifest and source contracts identical without accepting global coverage */ void temp;
  for (const issue of REQUIRED_ISSUES) {
    const mine = scenarios.filter((s) => s.issue === issue);
    for (const state of REQUIRED_STATES)
      if (!mine.some((s) => s.state === state))
        throw new Error(`missing ${state} scenario for #${issue}`);
    for (const mode of REQUIRED_MODES)
      if (!mine.some((s) => s.state === "normal" && s.mode === mode))
        throw new Error(`missing normal ${mode} scenario for #${issue}`);
  }
}
export function verifyEvidence(manifestPath: string): EvidenceManifest {
  const absolute = resolve(manifestPath),
    root = resolve(absolute, "..");
  const manifest = JSON.parse(
    readFileSync(absolute, "utf8"),
  ) as EvidenceManifest;
  if (manifest.manifestVersion !== COCKPIT_EVIDENCE_VERSION)
    throw new Error("unsupported cockpit evidence manifestVersion");
  if (!/^[0-9a-f]{40}$/i.test(manifest.commit))
    throw new Error(
      "cockpit evidence commit must be a full 40-character commit id",
    );
  assertImmutableImage(manifest.image);
  if (
    !/^sha256:[0-9a-f]{64}$/i.test(manifest.imageDigest) ||
    !manifest.image.endsWith(`@${manifest.imageDigest}`)
  )
    throw new Error("cockpit evidence imageDigest must match immutable image");
  if (
    manifest.runtime?.imageRepoDigest !== manifest.image ||
    !Array.isArray(manifest.runtime.repoDigests) ||
    !manifest.runtime.repoDigests.includes(manifest.image) ||
    manifest.runtime.health?.gitCommit !== manifest.commit
  )
    throw new Error(
      "verified runtime build identity does not match candidate image and commit",
    );
  if (
    !manifest.regressionQuery ||
    !safeRelative(manifest.regressionQuery.path) ||
    !/^sha256:[0-9a-f]{64}$/i.test(manifest.regressionQuery.sha256) ||
    !Array.isArray(manifest.regressionQuery.issues)
  )
    throw new Error("missing exact epic:648 regression query evidence");
  const queryPath = resolve(root, manifest.regressionQuery.path);
  if (
    !queryPath.startsWith(root + sep) ||
    !existsSync(queryPath) ||
    sha256(queryPath) !== manifest.regressionQuery.sha256
  )
    throw new Error("regression query checksum mismatch");
  const blocking = manifest.regressionQuery.issues.filter((i) =>
    i.labels?.some(
      (l) => l.name === "severity:critical" || l.name === "severity:high",
    ),
  );
  if (blocking.length)
    throw new Error(
      `open related severity:critical or severity:high regression blocks Cockpit evidence: ${blocking.map((i) => `#${i.number} ${i.title ?? ""}`.trim()).join(", ")}`,
    );
  if (!Array.isArray(manifest.artifacts) || !Array.isArray(manifest.scenarios))
    throw new Error(
      "cockpit evidence must contain artifacts and scenarios arrays",
    );
  assertScenarios(manifest.scenarios);
  const artifacts = new Map<string, EvidenceArtifact>();
  for (const a of manifest.artifacts) {
    if (!safeRelative(a.path) || !a.path.endsWith(".png"))
      throw new Error(`unsafe screenshot path: ${a.path}`);
    if (artifacts.has(a.path))
      throw new Error(`duplicate evidence artifact: ${a.path}`);
    const target = resolve(root, a.path);
    if (!target.startsWith(root + sep) || !existsSync(target))
      throw new Error(`missing screenshot artifact: ${a.path}`);
    const dimensions = pngDimensions(target);
    if (dimensions.width !== a.width || dimensions.height !== a.height)
      throw new Error(`PNG dimension mismatch: ${a.path}`);
    if (!/^sha256:[0-9a-f]{64}$/i.test(a.sha256) || sha256(target) !== a.sha256)
      throw new Error(`screenshot SHA-256 mismatch: ${a.path}`);
    artifacts.set(a.path, a);
  }
  const used = new Set<string>();
  for (const s of manifest.scenarios) {
    if (!artifacts.has(s.screenshot) || used.has(s.screenshot))
      throw new Error(
        `scenario ${s.scenario} must map one-to-one to a screenshot artifact`,
      );
    used.add(s.screenshot);
    if (
      !s.keyboardAssertions?.length ||
      !s.domAssertions?.length ||
      !s.interception ||
      !Array.isArray(s.consoleErrors) ||
      s.consoleErrors.length
    )
      throw new Error(`incomplete scenario result: ${s.scenario}`);
    const requested = s.interception.requested;
    if (
      requested &&
      !s.interception.actualRequests.some((url) => url.includes(requested))
    )
      throw new Error(`interception was not observed for ${s.scenario}`);
  }
  if (used.size !== artifacts.size)
    throw new Error("unreferenced screenshot artifact");
  const pngs = readdirSync(root).filter((p) => p.endsWith(".png"));
  if (pngs.some((p) => !artifacts.has(p)))
    throw new Error("unreferenced screenshot file");
  return manifest;
}
export function screenshotName(s: Scenario): string {
  return `${s.issue}-${s.scenario.replace(/[^a-z0-9]+/gi, "-").replace(/^-|-$/g, "")}.png`;
}
