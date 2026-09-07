#!/usr/bin/env bun
/**
 * Direct, synthetic-browser acceptance evidence for a published candidate. It
 * deliberately owns one disposable Docker workspace and one system Chrome;
 * it never accepts a host workspace path or a mutable image tag.
 */
import { mkdirSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  assertImmutableImage,
  parseScenarios,
  relativeArtifact,
  screenshotName,
  sha256,
  type EvidenceManifest,
  type Scenario,
  verifyEvidence,
} from "./cockpit-evidence";

function required(name: string): string {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required`);
  return value;
}

const image = required("COCKPIT_EVIDENCE_IMAGE");
const commit = required("COCKPIT_EVIDENCE_COMMIT");
const output = resolve(process.env.COCKPIT_EVIDENCE_OUT ?? "cockpit-evidence");
const scenarioPath = resolve(process.env.COCKPIT_EVIDENCE_SCENARIOS ?? "scripts/release/cockpit-evidence-scenarios.json");
assertImmutableImage(image);
if (!/^[0-9a-f]{40}$/i.test(commit)) throw new Error("COCKPIT_EVIDENCE_COMMIT must be a full 40-character commit id");
const scenarios = parseScenarios(scenarioPath);
mkdirSync(output, { recursive: true });

function command(command: string, args: string[], quiet = false): Promise<string> {
  const child = Bun.spawn([command, ...args], { stdout: "pipe", stderr: "pipe" });
  return new Promise(async (resolveCommand, reject) => {
    const [stdout, stderr, exitCode] = await Promise.all([
      new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited,
    ]);
    if (exitCode !== 0) reject(new Error(`${command} ${args.join(" ")} failed: ${stderr || stdout}`));
    else resolveCommand(quiet ? "" : stdout.trim());
  });
}

async function waitFor(url: string): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try { if ((await fetch(url)).ok) return; } catch { /* container is starting */ }
    await Bun.sleep(100);
  }
  throw new Error(`candidate did not become ready at ${url}`);
}

type Cdp = { call(method: string, params?: Record<string, unknown>): Promise<Record<string, unknown>>; on(handler: (message: Record<string, unknown>) => void): () => void; close(): void };
async function openCdp(port: number): Promise<Cdp> {
  const targets = await fetch(`http://127.0.0.1:${port}/json/list`).then((response) => response.json()) as Array<{ webSocketDebuggerUrl: string }>;
  const wsUrl = targets.find((target) => target.webSocketDebuggerUrl)?.webSocketDebuggerUrl;
  if (!wsUrl) throw new Error("system Chrome did not expose a DevTools page target");
  const socket = new WebSocket(wsUrl);
  const pending = new Map<number, { resolve(value: Record<string, unknown>): void; reject(reason: Error): void }>();
  const events: Array<(message: Record<string, unknown>) => void> = [];
  let nextId = 1;
  await new Promise<void>((resolveOpen, rejectOpen) => {
    socket.onopen = () => resolveOpen();
    socket.onerror = () => rejectOpen(new Error("unable to connect to system Chrome DevTools"));
  });
  socket.onmessage = (event) => {
    const message = JSON.parse(String(event.data)) as Record<string, unknown>;
    if (typeof message.id === "number") {
      const request = pending.get(message.id);
      if (!request) return;
      pending.delete(message.id);
      if (message.error) request.reject(new Error(`Chrome ${String((message.error as { message?: string }).message ?? "protocol error")}`));
      else request.resolve((message.result ?? {}) as Record<string, unknown>);
      return;
    }
    events.forEach((handler) => {
      handler(message);
    });
  };
  return {
    call(method, params = {}) {
      const id = nextId++;
      socket.send(JSON.stringify({ id, method, params }));
      return new Promise((resolveCall, rejectCall) => pending.set(id, { resolve: resolveCall, reject: rejectCall }));
    },
    on(handler) {
      events.push(handler);
      return () => events.splice(events.indexOf(handler), 1);
    },
    close() { socket.close(); },
  };
}

async function renderScenario(cdp: Cdp, baseUrl: string, scenario: Scenario): Promise<{ png: Uint8Array; keyboardAssertions: string[]; interceptionDescription: string }> {
  await cdp.call("Emulation.setDeviceMetricsOverride", { width: scenario.viewport.width, height: scenario.viewport.height, deviceScaleFactor: 1, mobile: false });
  await cdp.call("Emulation.setPageScaleFactor", { pageScaleFactor: scenario.zoom });
  const interceptionDescription = scenario.interception
    ? `deterministic ${scenario.interception.status} presentation for requests containing ${scenario.interception.urlIncludes}${scenario.interception.delayMs ? ` after ${scenario.interception.delayMs}ms` : ""}`
    : "none";
  const stopIntercepting = scenario.interception ? cdp.on((message) => {
    if (message.method !== "Fetch.requestPaused") return;
    const params = message.params as { requestId?: string; request?: { url?: string } } | undefined;
    const requestId = params?.requestId;
    if (!requestId) return;
    const matches = params.request?.url?.includes(scenario.interception!.urlIncludes);
    if (!matches) {
      void cdp.call("Fetch.continueRequest", { requestId });
      return;
    }
    void (async () => {
      if (scenario.interception?.delayMs) await Bun.sleep(scenario.interception.delayMs);
      await cdp.call("Fetch.fulfillRequest", {
        requestId,
        responseCode: scenario.interception!.status,
        responseHeaders: [{ name: "content-type", value: "application/json" }],
        body: Buffer.from(scenario.interception!.body).toString("base64"),
      });
    })();
  }) : undefined;
  if (scenario.interception) await cdp.call("Fetch.enable", { patterns: [{ urlPattern: "*", requestStage: "Request" }] });
  await cdp.call("Page.navigate", { url: `${baseUrl}${scenario.route}` });
  await Bun.sleep(scenario.interception?.delayMs ? 400 : 1200);
  const keyboardAssertions: string[] = [];
  for (const assertion of scenario.keyboard) {
    const expression = `(() => { const element=document.querySelector(${JSON.stringify(assertion.selector)}); if (!element) return 'missing selector'; element.setAttribute('tabindex','-1'); element.focus(); return document.activeElement===element ? 'focused' : 'not focused'; })()`;
    const result = await cdp.call("Runtime.evaluate", { expression, returnByValue: true });
    const value = (((result.result as { value?: unknown } | undefined)?.value) ?? "") as string;
    await cdp.call("Input.dispatchKeyEvent", { type: "keyDown", key: assertion.key });
    await cdp.call("Input.dispatchKeyEvent", { type: "keyUp", key: assertion.key });
    if (value !== "focused" || !assertion.expectFocused) throw new Error(`keyboard assertion failed for ${scenario.scenario}: ${assertion.selector} after ${assertion.key}`);
    keyboardAssertions.push(`${assertion.key}: ${assertion.selector} focused`);
  }
  const result = await cdp.call("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
  const data = result.data;
  if (typeof data !== "string") throw new Error(`Chrome did not return a PNG for ${scenario.scenario}`);
  if (scenario.interception) await cdp.call("Fetch.disable");
  stopIntercepting?.();
  return { png: Uint8Array.fromBase64(data), keyboardAssertions, interceptionDescription };
}

const docker = "docker";
const chrome = await command("sh", ["-c", "command -v google-chrome || command -v chromium || command -v chromium-browser"]);
const container = `rentemester-cockpit-evidence-${crypto.randomUUID().slice(0, 12)}`;
let chromeProcess: ReturnType<typeof Bun.spawn> | undefined;
try {
  await command(docker, ["pull", image], true);
  await command(docker, ["run", "--detach", "--name", container, "--read-only", "--publish", "127.0.0.1::4319", "--tmpfs", "/tmp:rw,noexec,nosuid,size=64m", "--tmpfs", "/workspace:rw,nosuid,size=64m,uid=1000,gid=1000", "--tmpfs", "/import:rw,nosuid,size=64m,uid=1000,gid=1000", "--env", "RENTEMESTER_DEPLOYMENT_PROFILE=local-container", "--env", "RENTEMESTER_APP_AUTH=off", image], true);
  const address = await command(docker, ["port", container, "4319/tcp"]);
  if (!/^127\.0\.0\.1:\d+$/.test(address)) throw new Error(`candidate must publish only loopback, got ${address}`);
  const baseUrl = `http://${address}`;
  await waitFor(`${baseUrl}/api/ready`);
  await command(docker, ["exec", container, "bun", "run", "src/cli.ts", "company", "add", "--workspace", "/workspace", "--name", "Synthetic Evidence Fixture", "--slug", "evidence-fixture", "--cvr", "12345678", "--bank-name", "Synthetic Bank", "--bank-reg", "1234", "--bank-account", "5678901234"], true);
  const debugPort = 9222;
  chromeProcess = Bun.spawn([chrome, "--headless", "--no-sandbox", "--disable-gpu", `--remote-debugging-port=${debugPort}`, "about:blank"], { stdout: "ignore", stderr: "ignore" });
  await waitFor(`http://127.0.0.1:${debugPort}/json/version`);
  const cdp = await openCdp(debugPort);
  const generated: EvidenceManifest["scenarios"] = [];
  const artifacts: EvidenceManifest["artifacts"] = [];
  for (const scenario of scenarios) {
    const screenshot = screenshotName(scenario);
    const rendered = await renderScenario(cdp, baseUrl, scenario);
    const screenshotPath = join(output, screenshot);
    await Bun.write(screenshotPath, rendered.png);
    artifacts.push({ path: relativeArtifact(screenshotPath), sha256: sha256(screenshotPath) });
    generated.push({ ...scenario, screenshot, keyboardAssertions: rendered.keyboardAssertions, interceptionDescription: rendered.interceptionDescription });
  }
  cdp.close();
  const manifest: EvidenceManifest = { manifestVersion: 1, commit, image, imageDigest: image.slice(image.lastIndexOf("@") + 1), generatedAt: new Date().toISOString(), artifacts, scenarios: generated };
  const manifestPath = join(output, "cockpit-evidence.json");
  await Bun.write(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  verifyEvidence(manifestPath);
  process.stdout.write(`${manifestPath}\n`);
} finally {
  chromeProcess?.kill();
  await command(docker, ["rm", "--force", container], true).catch(() => undefined);
}
