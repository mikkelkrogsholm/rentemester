import { expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startEvidenceBrowser } from "../../scripts/release/cockpit-evidence-browser";

async function fixture(mode: string) {
  const profile = mkdtempSync(join(tmpdir(), "rentemester-browser-test-"));
  const script = join(profile, "browser.js");
  await Bun.write(script, `
const profile = process.argv.find(a => a.startsWith('--user-data-dir=')).split('=').slice(1).join('=');
await Bun.write(profile + '/pid', String(process.pid));
if (process.argv[2] === 'orphan') {
  const child = Bun.spawn([process.execPath, '-e', 'setInterval(() => {}, 1000)'], {stdin:'ignore',stdout:'ignore',stderr:'inherit'});
  await Bun.write(profile + '/descendant', String(child.pid));
  process.stderr.write('inherited pipe startup failure\\n');
  process.exit(23);
}
if (process.argv[2] === 'exit') {
  process.stderr.write('x'.repeat(100000) + '\\nconcrete browser startup failure\\n');
  process.exit(23);
}
process.on('SIGTERM', async () => {
  if (process.argv[2] === 'stubborn') return;
  await Bun.sleep(60);
  await Bun.write(profile + '/terminated', 'process finished before profile cleanup');
  process.exit(0);
});
if (['silent', 'stubborn'].includes(process.argv[2])) setInterval(() => {}, 100);
else {
  const server = Bun.serve({hostname:'127.0.0.1', port:0, fetch:() => process.argv[2] === 'hanging' ? new Promise(() => {}) : Response.json({Browser:'synthetic-CDP'})});
  await Bun.sleep(150);
  await Bun.write(profile + '/DevToolsActivePort', server.port + '\\n/devtools/browser/synthetic\\n');
}
`);
  return { profile, command: [process.execPath, script, mode], cleanup: () => rmSync(profile, { recursive: true, force: true }) };
}

test("Chrome early exit reports its bounded stderr tail and exit code", async () => {
  const f = await fixture("exit");
  const started = performance.now();
  try {
    const error = await startEvidenceBrowser(f.command, f.profile, 5_000).then(() => { throw new Error("expected failure"); }, error => error);
    expect(error.message).toContain("process exited before CDP was ready");
    expect(error.message).toContain("exit=23");
    expect(error.message).toContain("concrete browser startup failure");
    expect(error.message.length).toBeLessThan(9_000);
    expect(performance.now() - started).toBeLessThan(2_000);
  } finally { f.cleanup(); }
});

test("delayed CDP startup uses the child-selected port and stop awaits process cleanup", async () => {
  const f = await fixture("ready");
  try {
    const browser = await startEvidenceBrowser(f.command, f.profile, 3_000);
    const activePort = Number((await Bun.file(join(f.profile, "DevToolsActivePort")).text()).split("\n")[0]);
    expect(browser.port).toBe(activePort);
    expect(await fetch(`http://127.0.0.1:${browser.port}/json/version`).then(r => r.json())).toEqual({ Browser: "synthetic-CDP" });
    await Promise.all([browser.stop(), browser.stop()]);
    expect(await Bun.file(join(f.profile, "terminated")).text()).toContain("before profile cleanup");
    const pid = Number(await Bun.file(join(f.profile, "pid")).text());
    expect(() => process.kill(pid, 0)).toThrow();
  } finally { f.cleanup(); }
});

for (const mode of ["silent", "hanging"]) {
  test(`Chrome ${mode} startup is bounded and reaps the process before profile removal`, async () => {
    const f = await fixture(mode);
    const started = performance.now();
    try {
      await expect(startEvidenceBrowser(f.command, f.profile, 400)).rejects.toThrow("CDP startup deadline exceeded (400 ms)");
      expect(performance.now() - started).toBeLessThan(2_000);
      expect(await Bun.file(join(f.profile, "terminated")).text()).toContain("before profile cleanup");
      const pid = Number(await Bun.file(join(f.profile, "pid")).text());
      expect(() => process.kill(pid, 0)).toThrow();
    } finally { f.cleanup(); }
  });
}

test("startup timeout forcibly reaps a browser that ignores graceful termination", async () => {
  const f = await fixture("stubborn");
  try {
    await expect(startEvidenceBrowser(f.command, f.profile, 400)).rejects.toThrow("signal=SIGKILL");
    const pid = Number(await Bun.file(join(f.profile, "pid")).text());
    expect(() => process.kill(pid, 0)).toThrow();
  } finally { f.cleanup(); }
});

test("an inherited stderr pipe cannot hold startup failure open after parent exit", async () => {
  const f = await fixture("orphan");
  const started = performance.now();
  try {
    await expect(startEvidenceBrowser(f.command, f.profile, 400)).rejects.toThrow("inherited pipe startup failure");
    expect(performance.now() - started).toBeLessThan(2_000);
    const descendant = Number(await Bun.file(join(f.profile, "descendant")).text());
    expect(() => process.kill(descendant, 0)).not.toThrow();
  } finally {
    const descendant = Number(await Bun.file(join(f.profile, "descendant")).text());
    try { process.kill(descendant, "SIGKILL"); } catch {}
    f.cleanup();
  }
});
