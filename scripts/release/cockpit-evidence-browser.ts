import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const STDERR_LIMIT = 8_192;

/** Chrome owns its debugging port; callers own the temporary profile. */
export async function startEvidenceBrowser(
  command: string[],
  profile: string,
  startupTimeoutMs = 30_000,
) {
  const browser = Bun.spawn([
    ...command,
    "--headless=new",
    "--disable-gpu",
    "--disable-extensions",
    "--remote-debugging-address=127.0.0.1",
    "--remote-debugging-port=0",
    `--user-data-dir=${profile}`,
    "--no-first-run",
    "--no-default-browser-check",
    "about:blank",
  ], { stdin: "ignore", stdout: "ignore", stderr: "pipe" });
  let stderr = "";
  let stderrEnded = false;
  const reader = browser.stderr.getReader();
  const drain = (async () => {
    const decoder = new TextDecoder();
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        stderr = (stderr + decoder.decode(value, { stream: true })).slice(-STDERR_LIMIT);
      }
      stderr = (stderr + decoder.decode()).slice(-STDERR_LIMIT);
    } catch (error) {
      stderr = (stderr + `\nstderr read failed: ${String(error)}`).slice(-STDERR_LIMIT);
    } finally {
      stderrEnded = true;
      reader.releaseLock();
    }
  })();
  let stopping: Promise<void> | undefined;
  function stop() {
    if (stopping) return stopping;
    stopping = (async () => {
      if (browser.exitCode === null) {
        browser.kill();
        await Promise.race([browser.exited, Bun.sleep(1_000)]);
        if (browser.exitCode === null) browser.kill("SIGKILL");
      }
      await browser.exited;
      // A surviving Chrome child can hold the inherited stderr pipe open.
      await Promise.race([drain, Bun.sleep(250)]);
      if (!stderrEnded) await reader.cancel().catch(() => undefined);
      await drain;
    })();
    return stopping;
  }
  const deadline = performance.now() + startupTimeoutMs;
  try {
    while (performance.now() < deadline) {
      if (browser.exitCode !== null) throw new Error("process exited before CDP was ready");
      try {
        const raw = await Bun.file(join(profile, "DevToolsActivePort")).text();
        const port = Number(raw.split("\n")[0]);
        if (Number.isInteger(port) && port > 0 && port <= 65_535) {
          const remaining = Math.max(1, deadline - performance.now());
          const response = await fetch(`http://127.0.0.1:${port}/json/version`, {
            signal: AbortSignal.timeout(Math.ceil(Math.min(1_000, remaining))),
          });
          if (response.ok) return { port, stop };
        }
      } catch {
        // The file and CDP listener become ready independently during startup.
      }
      await Bun.sleep(Math.min(100, Math.max(1, deadline - performance.now())));
    }
    throw new Error(`CDP startup deadline exceeded (${startupTimeoutMs} ms)`);
  } catch (error) {
    await stop();
    const reason = error instanceof Error ? error.message : String(error);
    throw new Error(`Chrome startup failed (${command[0]}): ${reason}; exit=${browser.exitCode}; signal=${browser.signalCode ?? "none"}; stderr tail:\n${stderr || "(empty)"}`);
  }
}

if (import.meta.main) {
  const chrome = ["google-chrome", "chromium", "chromium-browser"].map((name) => Bun.which(name)).find(Boolean);
  if (!chrome) throw new Error("headless Chrome executable unavailable");
  const profile = mkdtempSync(join(tmpdir(), "rentemester-browser-preflight-"));
  let browser: Awaited<ReturnType<typeof startEvidenceBrowser>> | undefined;
  try {
    browser = await startEvidenceBrowser([chrome], profile);
    console.log("headless Chrome CDP startup verified");
  } finally {
    await browser?.stop();
    rmSync(profile, { recursive: true, force: true });
  }
}
