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
type Mode = (typeof REQUIRED_MODES)[number];

const MODE_VIEWPORTS: Record<
  Mode,
  { width: number; height: number; deviceScaleFactor: number }
> = {
  desktop: { width: 1440, height: 900, deviceScaleFactor: 1 },
  mobile: { width: 390, height: 844, deviceScaleFactor: 1 },
  // A 720×450 CSS layout viewport at device scale 2 is the deterministic
  // equivalent of desktop 200% zoom; Chrome captures 1440×900 physical pixels.
  zoom: { width: 720, height: 450, deviceScaleFactor: 2 },
};
const REQUIRED_ISSUE_MAPPING: Record<number, { route: string; endpoint: string }> = {
  649: { route: "/companies/evidence-fixture", endpoint: "/api/companies/evidence-fixture/attention" },
  650: { route: "/companies/evidence-fixture/batchbogfoering", endpoint: "/api/companies/evidence-fixture/bookkeeping-workbench" },
  651: { route: "/companies/evidence-fixture", endpoint: "/api/companies/evidence-fixture/overview/changes" },
  652: { route: "/companies/evidence-fixture/posteringer", endpoint: "/api/companies/evidence-fixture/journal/explanation" },
  653: { route: "/companies/evidence-fixture/kontakter", endpoint: "/api/companies/evidence-fixture/party-projection" },
  654: { route: "/companies/evidence-fixture/balance", endpoint: "/api/companies/evidence-fixture/balance" },
  655: { route: "/companies/evidence-fixture/bank", endpoint: "/api/companies/evidence-fixture/bank?year=2026" },
  656: { route: "/companies/evidence-fixture/moms", endpoint: "/api/companies/evidence-fixture/vat/readiness" },
  657: { route: "/companies/evidence-fixture/manage", endpoint: "/api/companies/evidence-fixture/company/profile" },
};

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
  endpoint: string;
  state: State;
  mode: Mode;
  viewport: { width: number; height: number; deviceScaleFactor: number };
  capture: { width: number; height: number };
  dom: {
    heading: DomAssertion;
    status: DomAssertion;
    controls: DomAssertion[];
    data: DomAssertion[];
    coreAction?: DomAssertion;
    noHorizontalOverflow: true;
  };
  keyboard?: {
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
type IssueProfile = {
  issue: number;
  owner: string;
  route: string;
  endpoint: string;
  heading: string;
  coreAction: string;
  data: string;
  progressive: string;
  taskOutcome: string;
  states: Record<State, string>;
};
type ScenarioConfig = { profiles: IssueProfile[] };
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
  if (!value || typeof value !== "object" || !Array.isArray((value as ScenarioConfig).profiles))
    throw new Error("scenario configuration must contain issue profiles");
  const profiles = (value as ScenarioConfig).profiles;
  if (profiles.length !== REQUIRED_ISSUES.length)
    throw new Error("scenario configuration must contain exactly nine issue profiles");
  const scenarios = profiles.flatMap(expandProfile);
  for (const s of scenarios) {
    if (
      !REQUIRED_ISSUES.includes(s.issue as never) ||
      !s.scenario ||
      !s.owner ||
      !s.route.startsWith("/") ||
      !/^\/api\/[a-z0-9/:?=&._-]+$/i.test(s.endpoint) ||
      !REQUIRED_STATES.includes(s.state) ||
      !REQUIRED_MODES.includes(s.mode)
    )
      throw new Error(`invalid scenario contract for #${s.issue}`);
    if (
      !Number.isInteger(s.viewport?.width) ||
      !Number.isInteger(s.viewport?.height) ||
      s.viewport.width < 390
    )
      throw new Error(`invalid viewport for ${s.scenario}`);
    if (
      s.viewport.width !== MODE_VIEWPORTS[s.mode].width ||
      s.viewport.height !== MODE_VIEWPORTS[s.mode].height ||
      s.viewport.deviceScaleFactor !== MODE_VIEWPORTS[s.mode].deviceScaleFactor ||
      s.capture.width !== s.viewport.width * s.viewport.deviceScaleFactor ||
      s.capture.height !== s.viewport.height * s.viewport.deviceScaleFactor
    )
      throw new Error(`mode dimensions must be explicit for ${s.scenario}`);
    if (
      !s.dom?.heading?.selector ||
      !s.dom.status?.selector ||
      !s.dom.noHorizontalOverflow ||
      !Array.isArray(s.dom.controls) ||
      !Array.isArray(s.dom.data)
    )
      throw new Error(
        `page-local heading, status and structural contract required for ${s.scenario}`,
      );
    if (
      s.endpoint === "/api/health" || s.endpoint.includes("identity") ||
      (s.interception && s.interception.urlPattern !== s.endpoint)
    )
      throw new Error(
        `endpoint and interception must be exact, route-specific, and exclude health/identity for ${s.scenario}`,
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
    if (!mine.some((s) => s.state === "normal" && s.keyboard?.length))
      throw new Error(`missing keyboard trace for #${issue}`);
  }
  return scenarios;
}
function assertScenarios(
  scenarios: Array<Pick<Scenario, "issue" | "state" | "mode" | "route" | "endpoint">>,
): void {
  for (const issue of REQUIRED_ISSUES) {
    const mine = scenarios.filter((s) => s.issue === issue);
    if (
      mine.some(
        (s) =>
          s.route !== REQUIRED_ISSUE_MAPPING[issue].route ||
          s.endpoint !== REQUIRED_ISSUE_MAPPING[issue].endpoint,
      )
    )
      throw new Error(`wrong route or endpoint mapping for #${issue}`);
    for (const state of REQUIRED_STATES)
      if (!mine.some((s) => s.state === state))
        throw new Error(`missing ${state} scenario for #${issue}`);
    for (const mode of REQUIRED_MODES)
      if (!mine.some((s) => s.state === "normal" && s.mode === mode))
        throw new Error(`missing normal ${mode} scenario for #${issue}`);
  }
}
function expandProfile(profile: IssueProfile): Scenario[] {
  if (
    !REQUIRED_ISSUES.includes(profile.issue as never) ||
    !profile.owner || !profile.route.startsWith("/") ||
    !/^\/api\/[a-z0-9/:?=&._-]+$/i.test(profile.endpoint) ||
    !profile.heading || !profile.coreAction || !profile.data || !profile.progressive ||
    !profile.taskOutcome || REQUIRED_STATES.some((state) => !profile.states?.[state]) ||
    profile.route !== REQUIRED_ISSUE_MAPPING[profile.issue]?.route ||
    profile.endpoint !== REQUIRED_ISSUE_MAPPING[profile.issue]?.endpoint
  ) throw new Error(`invalid issue profile for #${profile.issue}`);
  const root = `[data-evidence-issue="${profile.issue}"]`;
  const dom = (state: State) => ({
    heading: { selector: `${root} [data-evidence-heading]`, text: profile.heading, visible: true },
    status: { selector: `${root} [data-evidence-status="${state}"]`, text: profile.states[state], visible: true },
    controls: state === "normal" ? [{ selector: `${root} [data-evidence-core-action]`, text: profile.coreAction, visible: true }] : [],
    data: state === "normal" ? [
      { selector: `${root} [data-evidence-data]`, text: profile.data, visible: true },
      { selector: `${root} [data-evidence-progressive]`, text: profile.progressive, visible: true },
    ] : [],
    coreAction: state === "normal" ? { selector: `${root} [data-evidence-core-action]`, text: profile.coreAction, visible: true } : undefined,
    noHorizontalOverflow: true as const,
  });
  const keyboard = [{
    key: "Tab" as const,
    expectFocus: { selector: `${root} [data-evidence-core-action]`, visible: true },
    expectState: { selector: `${root} [data-evidence-core-action]`, text: profile.coreAction, visible: true },
  }, {
    key: "Enter" as const,
    expectFocus: { selector: `${root} [data-evidence-core-action]`, visible: true },
    expectState: { selector: `${root} [data-evidence-task-outcome]`, text: profile.taskOutcome, visible: true },
  }];
  const scenario = (state: State, mode: Mode): Scenario => ({
    issue: profile.issue, scenario: `issue-${profile.issue}-${state}-${mode}`,
    owner: profile.owner, route: profile.route, endpoint: profile.endpoint, state, mode,
    viewport: MODE_VIEWPORTS[mode],
    capture: {
      width: MODE_VIEWPORTS[mode].width * MODE_VIEWPORTS[mode].deviceScaleFactor,
      height: MODE_VIEWPORTS[mode].height * MODE_VIEWPORTS[mode].deviceScaleFactor,
    },
    dom: dom(state), keyboard: state === "normal" && mode === "desktop" ? keyboard : undefined,
    ...(["loading", "warning-or-blocked", "error"] as State[]).includes(state) ? {
      interception: {
        urlPattern: profile.endpoint,
        status: state === "warning-or-blocked" ? 403 : state === "error" ? 500 : 200,
        body: "{}", ...(state === "loading" ? { delayMs: 3000 } : {}),
      },
    } : {},
  });
  return [scenario("normal", "desktop"), scenario("normal", "mobile"), scenario("normal", "zoom"), ...REQUIRED_STATES.slice(1).map((state) => scenario(state, "desktop"))];
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
      (s.keyboard?.length && !s.keyboardAssertions?.length) ||
      !s.domAssertions?.length ||
      !s.interception ||
      !Array.isArray(s.consoleErrors) ||
      s.consoleErrors.length
    )
      throw new Error(`incomplete scenario result: ${s.scenario}`);
    const screenshot = artifacts.get(s.screenshot)!;
    if (
      screenshot.width !== s.capture.width ||
      screenshot.height !== s.capture.height
    )
      throw new Error(`scenario capture dimensions mismatch: ${s.scenario}`);
    const requested = s.interception.requested;
    if (requested !== null && requested !== s.endpoint)
      throw new Error(`interception endpoint mismatch: ${s.scenario}`);
    if (!s.interception.actualRequests.includes(s.endpoint))
      throw new Error(`feature endpoint was not observed for ${s.scenario}`);
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
