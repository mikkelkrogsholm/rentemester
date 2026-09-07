#!/usr/bin/env bun
/** Digest-bound, isolated browser evidence. Feature-owned selectors deliberately fail closed until their UI lands. */
import { copyFileSync, mkdtempSync, mkdirSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import {
  assertImmutableImage,
  parseScenarios,
  pngDimensions,
  screenshotName,
  sha256,
  type EvidenceManifest,
  type Scenario,
  verifyEvidence,
} from "./cockpit-evidence";
import { internalAppIpv4, startLoopbackProxy, type NetworkSettings } from "./cockpit-evidence-proxy";

const required = (name: string) => {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required`);
  return value;
};
const image = required("COCKPIT_EVIDENCE_IMAGE"),
  commit = required("COCKPIT_EVIDENCE_COMMIT"),
  output = resolve(process.env.COCKPIT_EVIDENCE_OUT ?? "cockpit-evidence"),
  scenarioPath = resolve(
    process.env.COCKPIT_EVIDENCE_SCENARIOS ??
      "scripts/release/cockpit-evidence-scenarios.json",
  ),
  regressionQueryPath = resolve(required("COCKPIT_EVIDENCE_REGRESSION_QUERY"));
assertImmutableImage(image);
if (!/^[0-9a-f]{40}$/i.test(commit))
  throw new Error(
    "COCKPIT_EVIDENCE_COMMIT must be a full 40-character commit id",
  );
const scenarios = parseScenarios(scenarioPath);
mkdirSync(output, { recursive: true });
async function command(
  command: string,
  args: string[],
  quiet = false,
): Promise<string> {
  const child = Bun.spawn([command, ...args], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, stderr, code] = await Promise.all([
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
    child.exited,
  ]);
  if (code !== 0)
    throw new Error(`${command} ${args.join(" ")} failed: ${stderr || stdout}`);
  return quiet ? "" : stdout.trim();
}
async function waitFor(url: string) {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(url)).ok) return;
    } catch {}
    await Bun.sleep(100);
  }
  throw new Error(`candidate did not become ready at ${url}`);
}
async function freePort() {
  const listener = Bun.listen({
    hostname: "127.0.0.1",
    port: 0,
    socket: { data() {}, open() {} },
  });
  const port = listener.port;
  listener.stop();
  return port;
}
type Cdp = {
  call(
    method: string,
    params?: Record<string, unknown>,
  ): Promise<Record<string, unknown>>;
  on(handler: (message: Record<string, unknown>) => void): () => void;
  close(): void;
};
async function openCdp(port: number): Promise<Cdp> {
  const targets = (await fetch(`http://127.0.0.1:${port}/json/list`).then((r) =>
    r.json(),
  )) as Array<{ webSocketDebuggerUrl?: string }>;
  const url = targets.find((t) => t.webSocketDebuggerUrl)?.webSocketDebuggerUrl;
  if (!url) throw new Error("Chrome did not expose a DevTools page target");
  const socket = new WebSocket(url),
    pending = new Map<
      number,
      {
        resolve: (v: Record<string, unknown>) => void;
        reject: (e: Error) => void;
      }
    >(),
    events: Array<(m: Record<string, unknown>) => void> = [];
  let id = 0;
  await new Promise<void>((resolveOpen, rejectOpen) => {
    const timer = setTimeout(
      () => rejectOpen(new Error("CDP connect timed out")),
      10_000,
    );
    socket.onopen = () => {
      clearTimeout(timer);
      resolveOpen();
    };
    socket.onerror = () => {
      clearTimeout(timer);
      rejectOpen(new Error("unable to connect to Chrome DevTools"));
    };
  });
  socket.onmessage = (e) => {
    const m = JSON.parse(String(e.data)) as Record<string, unknown>;
    if (typeof m.id === "number") {
      const p = pending.get(m.id);
      if (!p) return;
      pending.delete(m.id);
      m.error
        ? p.reject(
            new Error(
              `Chrome protocol error: ${String((m.error as { message?: string }).message ?? "")}`,
            ),
          )
        : p.resolve((m.result ?? {}) as Record<string, unknown>);
    } else events.forEach((h) => { h(m); });
  };
  socket.onclose = () => {
    for (const p of pending.values()) p.reject(new Error("CDP socket closed"));
    pending.clear();
  };
  return {
    call(method, params = {}) {
      const requestId = ++id;
      socket.send(JSON.stringify({ id: requestId, method, params }));
      return new Promise((resolveCall, rejectCall) => {
        const timer = setTimeout(() => {
          pending.delete(requestId);
          rejectCall(new Error(`CDP timeout: ${method}`));
        }, 15_000);
        pending.set(requestId, {
          resolve: (v) => {
            clearTimeout(timer);
            resolveCall(v);
          },
          reject: (e) => {
            clearTimeout(timer);
            rejectCall(e);
          },
        });
      });
    },
    on(handler) {
      events.push(handler);
      return () => events.splice(events.indexOf(handler), 1);
    },
    close() {
      socket.close();
    },
  };
}
const expression = (a: {
  selector: string;
  text?: string;
  visible?: boolean;
}) =>
  `(()=>{const e=document.querySelector(${JSON.stringify(a.selector)});if(!e)return false;const visible=${a.visible !== false};return (!visible||(!!(e.offsetWidth||e.offsetHeight||e.getClientRects().length)))&&${a.text ? `e.textContent.includes(${JSON.stringify(a.text)})` : "true"};})()`;
async function evaluateBoolean(cdp: Cdp, source: string, label: string) {
  const value = await cdp.call("Runtime.evaluate", {
    expression: source,
    returnByValue: true,
    awaitPromise: true,
  });
  if ((value.result as { value?: unknown })?.value !== true)
    throw new Error(`DOM assertion failed: ${label}`);
}
async function renderScenario(
  chrome: string,
  base: string,
  scenario: Scenario,
) {
  const profile = mkdtempSync(join(tmpdir(), "rentemester-cockpit-profile-")),
    port = await freePort();
  let browser: ReturnType<typeof Bun.spawn> | undefined;
  let cdp!: Cdp;
  try {
    browser = Bun.spawn(
      [
        chrome,
        "--headless=new",
        "--disable-gpu",
        `--remote-debugging-port=${port}`,
        `--user-data-dir=${profile}`,
        "--no-first-run",
        "--no-default-browser-check",
        "about:blank",
      ],
      { stdout: "ignore", stderr: "ignore" },
    );
    await waitFor(`http://127.0.0.1:${port}/json/version`);
    cdp = await openCdp(port);
    const actualRequests: string[] = [],
      consoleErrors: string[] = [],
      interceptions: Promise<unknown>[] = [];
    const stop = cdp.on((message) => {
      if (
        message.method === "Runtime.exceptionThrown" ||
        message.method === "Log.entryAdded"
      )
        consoleErrors.push(JSON.stringify(message.params));
      if (message.method !== "Fetch.requestPaused") return;
      const p = message.params as {
        requestId?: string;
        request?: { url?: string };
      };
      if (!p.requestId) return;
      const match =
        !!scenario.interception &&
        p.request?.url === `${base}${scenario.interception.urlPattern}`;
      if (p.request?.url) actualRequests.push(p.request.url);
      interceptions.push(
        match
          ? (async () => {
              if (scenario.interception?.delayMs)
                await Bun.sleep(scenario.interception.delayMs);
              await cdp!.call("Fetch.fulfillRequest", {
                requestId: p.requestId,
                responseCode: scenario.interception!.status,
                responseHeaders: [
                  { name: "content-type", value: "application/json" },
                ],
                body: Buffer.from(scenario.interception!.body).toString(
                  "base64",
                ),
              });
            })()
          : cdp.call("Fetch.continueRequest", { requestId: p.requestId }),
      );
    });
    await cdp.call("Runtime.enable");
    await cdp.call("Log.enable");
    await cdp.call("Emulation.setDeviceMetricsOverride", {
      width: scenario.viewport.width,
      height: scenario.viewport.height,
      deviceScaleFactor: scenario.viewport.deviceScaleFactor,
      mobile: false,
    });
    await cdp.call("Fetch.enable", {
      patterns: [{ urlPattern: `${base}${scenario.endpoint}`, requestStage: "Request" }],
    });
    await cdp.call("Page.navigate", { url: `${base}${scenario.route}` });
    await Bun.sleep(scenario.interception?.delayMs ? 300 : 1000);
    await evaluateBoolean(
      cdp,
      `window.innerWidth === ${scenario.viewport.width} && window.innerHeight === ${scenario.viewport.height} && window.devicePixelRatio === ${scenario.viewport.deviceScaleFactor}`,
      `${scenario.scenario} CSS viewport and device scale`,
    );
    const assertions = [
      scenario.dom.heading,
      scenario.dom.status,
      ...scenario.dom.controls,
      ...scenario.dom.data,
      ...(scenario.dom.coreAction ? [scenario.dom.coreAction] : []),
    ];
    for (const a of assertions)
      await evaluateBoolean(cdp, expression(a), a.selector);
    await evaluateBoolean(
      cdp,
      "document.documentElement.scrollWidth <= window.innerWidth",
      `${scenario.scenario} has no horizontal overflow`,
    );
    const keyboardAssertions: string[] = [];
    for (const step of scenario.keyboard ?? []) {
      await cdp.call("Input.dispatchKeyEvent", {
        type: "keyDown",
        key: step.key,
        code: step.key === "Space" ? "Space" : step.key,
        modifiers: step.key === "Shift+Tab" ? 8 : 0,
      });
      await cdp.call("Input.dispatchKeyEvent", {
        type: "keyUp",
        key: step.key,
        code: step.key === "Space" ? "Space" : step.key,
        modifiers: step.key === "Shift+Tab" ? 8 : 0,
      });
      await evaluateBoolean(
        cdp,
        `(()=>{const e=document.activeElement;return e instanceof Element&&e.matches(${JSON.stringify(step.expectFocus.selector)});})()`,
        `${scenario.scenario} focus after ${step.key}`,
      );
      await evaluateBoolean(
        cdp,
        expression(step.expectState),
        `${scenario.scenario} task outcome after ${step.key}`,
      );
      keyboardAssertions.push(
        `${step.key}: natural focus and UI state verified`,
      );
    }
    await Promise.all(interceptions);
    if (!actualRequests.includes(`${base}${scenario.endpoint}`))
      throw new Error(
        `expected feature endpoint was not observed: ${scenario.endpoint}`,
      );
    if (consoleErrors.length)
      throw new Error(
        `console errors in ${scenario.scenario}: ${consoleErrors.join("\n")}`,
      );
    const screenshot = await cdp.call("Page.captureScreenshot", {
      format: "png",
      captureBeyondViewport: false,
    });
    if (typeof screenshot.data !== "string")
      throw new Error("Chrome did not return PNG");
    return {
      png: Uint8Array.fromBase64(screenshot.data),
      keyboardAssertions,
      interception: {
        requested: scenario.interception?.urlPattern ?? null,
        actualRequests,
      },
      consoleErrors,
      domAssertions: assertions.map((a) => a.selector),
    };
  } finally {
    try {
      await cdp?.call("Fetch.disable");
    } catch {}
    cdp?.close();
    browser?.kill();
    rmSync(profile, { recursive: true, force: true });
  }
}
const chrome = await command("sh", [
    "-c",
    "command -v google-chrome || command -v chromium || command -v chromium-browser",
  ]),
  container = `rentemester-cockpit-evidence-${crypto.randomUUID().slice(0, 12)}`,
  network = `rentemester-cockpit-evidence-${crypto.randomUUID().slice(0, 12)}`;
let proxy: ReturnType<typeof Bun.serve> | undefined;
try {
  await command("docker", ["pull", image], true);
  const repoDigests = JSON.parse(
    await command("docker", [
      "image",
      "inspect",
      image,
      "--format",
      "{{json .RepoDigests}}",
    ]),
  );
  if (!Array.isArray(repoDigests) || !repoDigests.includes(image))
    throw new Error(
      "docker pull did not resolve the requested immutable digest",
    );
  const regressionPath = join(output, "cockpit-epic-648-open-issues.json");
  copyFileSync(regressionQueryPath, regressionPath);
  const regressionIssues = JSON.parse(
    await Bun.file(regressionPath).text(),
  ) as EvidenceManifest["regressionQuery"]["issues"];
  await command("docker", ["network", "create", "--internal", network], true);
  const internal = await command("docker", [
    "network", "inspect", network, "--format", "{{.Internal}}",
  ]);
  if (internal !== "true")
    throw new Error("Docker did not create an internal evidence network");
  await command(
    "docker",
    [
      "run",
      "--detach",
      "--name",
      container,
      "--read-only",
      "--security-opt",
      "no-new-privileges",
      "--cap-drop",
      "ALL",
      "--pids-limit",
      "128",
      "--memory",
      "512m",
      "--cpus",
      "1",
      "--init",
      "--network",
      network,
      "--tmpfs",
      "/tmp:rw,noexec,nosuid,size=64m",
      "--tmpfs",
      "/workspace:rw,nosuid,size=64m,uid=1000,gid=1000",
      "--tmpfs",
      "/import:rw,nosuid,size=64m,uid=1000,gid=1000",
      "--env",
      "RENTEMESTER_DEPLOYMENT_PROFILE=local-container",
      "--env",
      "RENTEMESTER_APP_AUTH=off",
      image,
    ],
    true,
  );
  const settings = JSON.parse(
    await command("docker", [
      "inspect",
      container,
      "--format",
      "{{json .NetworkSettings}}",
    ]),
  ) as NetworkSettings;
  const appAddress = internalAppIpv4(settings, network);
  const access = startLoopbackProxy(appAddress);
  proxy = access.proxy;
  const base = access.base;
  await waitFor(`${base}/api/ready`);
  const health = (await fetch(`${base}/api/health`).then((r) => r.json())) as {
    build?: { gitCommit?: string; version?: string };
  };
  if (health.build?.gitCommit !== commit)
    throw new Error(
      "runtime /api/health gitCommit does not match expected commit",
    );
  await command(
    "docker",
    [
      "exec",
      container,
      "bun",
      "run",
      "src/cli.ts",
      "company",
      "add",
      "--workspace",
      "/workspace",
      "--name",
      "Synthetic Evidence Fixture",
      "--slug",
      "evidence-fixture",
      "--cvr",
      "12345678",
      "--bank-name",
      "Synthetic Bank",
      "--bank-reg",
      "1234",
      "--bank-account",
      "5678901234",
      "--actor",
      "system:release-candidate",
    ],
    true,
  );
  const artifacts: EvidenceManifest["artifacts"] = [],
    generated: EvidenceManifest["scenarios"] = [];
  for (const scenario of scenarios) {
    const rendered = await renderScenario(chrome, base, scenario),
      name = screenshotName(scenario),
      path = join(output, name);
    await Bun.write(path, rendered.png);
    const dimensions = pngDimensions(path);
    if (
      dimensions.width !== scenario.capture.width ||
      dimensions.height !== scenario.capture.height
    )
      throw new Error(
        `${scenario.scenario} screenshot dimensions must be ${scenario.capture.width}x${scenario.capture.height}`,
      );
    artifacts.push({ path: name, sha256: sha256(path), ...dimensions });
    generated.push({ ...scenario, screenshot: name, ...rendered });
  }
  const manifest: EvidenceManifest = {
    manifestVersion: 2,
    commit,
    image,
    imageDigest: image.slice(image.lastIndexOf("@") + 1),
    generatedAt: new Date().toISOString(),
    runtime: {
      imageRepoDigest: image,
      repoDigests,
      health: { gitCommit: commit, version: health.build?.version },
    },
    regressionQuery: {
      path: "cockpit-epic-648-open-issues.json",
      sha256: sha256(regressionPath),
      issues: regressionIssues,
    },
    artifacts,
    scenarios: generated,
  };
  const manifestPath = join(output, "cockpit-evidence.json");
  await Bun.write(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  verifyEvidence(manifestPath);
  process.stdout.write(`${manifestPath}\n`);
} finally {
  await proxy?.stop(true);
  await command("docker", ["rm", "--force", container], true).catch(
    () => undefined,
  );
  await command("docker", ["network", "rm", network], true).catch(
    () => undefined,
  );
}
