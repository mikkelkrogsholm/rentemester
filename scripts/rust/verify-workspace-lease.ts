/** Cooperative process exclusion only. This is not current legacy mutation admission. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dir, "../..");
const cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const build = Bun.spawnSync(
  [
    cargo,
    "build",
    "--locked",
    "-p",
    "rentemester-runtime",
    "--example",
    "workspace-lease",
  ],
  { cwd: root, stdout: "pipe", stderr: "pipe" },
);
assert.equal(build.exitCode, 0, build.stderr.toString());
const binary = join(root, "target/debug/examples/workspace-lease");
const temp = mkdtempSync(join(tmpdir(), "rentemester-process-lease-"));
const children = new Set<Bun.Subprocess<"pipe", "pipe", "pipe">>();
const passed: string[] = [];
async function bounded<T>(promise: Promise<T>): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      promise,
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error("process probe timed out")),
          10000,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}
function start(args: string[], cwd = root) {
  const process = Bun.spawn(args, {
    cwd,
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });
  children.add(process);
  const reader = process.stdout.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  const stderr = new Response(process.stderr).text();
  return {
    process,
    async line(): Promise<any> {
      while (!buffer.includes("\n")) {
        const next = await bounded(reader.read());
        assert(!next.done, `probe ended before response: ${buffer}`);
        buffer += decoder.decode(next.value, { stream: true });
      }
      const index = buffer.indexOf("\n");
      const line = buffer.slice(0, index);
      buffer = buffer.slice(index + 1);
      return JSON.parse(line);
    },
    async send(text: string) {
      process.stdin.write(text + "\n");
      await process.stdin.flush();
    },
    async finish(expected: number | null = 0) {
      const code = await bounded(process.exited);
      if (expected === null) assert.notEqual(code, 0);
      else assert.equal(code, expected);
      assert.equal(await bounded(stderr), "");
      children.delete(process);
    },
  };
}
function probe(path: string, cwd = root) {
  return start([binary, path], cwd);
}
async function held(path: string) {
  const p = probe(path);
  const value = await p.line();
  assert.equal(value.ok, true, JSON.stringify(value));
  assert.equal(value.state, "held");
  assert.equal(value.writerAuthority, false);
  assert.equal(value.scope, "cooperative_process_lease_only");
  assert.equal(typeof value.identity.device, "string");
  assert.equal(typeof value.identity.inode, "string");
  assert(["apfs", "ext", "xfs", "btrfs", "tmpfs"].includes(value.filesystem));
  return { ...p, identity: value.identity, filesystem: value.filesystem };
}
async function busy(path: string) {
  const p = probe(path);
  assert.deepEqual(await p.line(), { ok: false, code: "workspace_busy" });
  await p.finish(1);
}
async function release(p: ReturnType<typeof probe>) {
  await p.send("release");
  assert.deepEqual(await p.line(), { ok: true, state: "released" });
  await p.finish();
}
function inventory(dir: string) {
  const result: string[][] = [];
  function walk(current: string) {
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const path = join(current, entry.name),
        key = path.slice(dir.length + 1);
      if (entry.isSymbolicLink()) result.push([key, "symlink"]);
      else if (entry.isDirectory()) walk(path);
      else
        result.push([
          key,
          createHash("sha256").update(readFileSync(path)).digest("hex"),
        ]);
    }
  }
  walk(dir);
  return result.sort();
}
try {
  const alpha = join(temp, "alpha"),
    beta = join(temp, "beta");
  mkdirSync(alpha);
  mkdirSync(beta);
  writeFileSync(join(alpha, "workspace.json"), '{"synthetic":true}\n');
  const before = inventory(temp);
  const owner = await held(alpha);
  await busy(alpha);
  await busy(alpha + "/./");
  const other = await held(beta);
  assert.notDeepEqual(other.identity, owner.identity);
  await release(other);
  await owner.send("check");
  assert.deepEqual(await owner.line(), { ok: true, state: "current" });
  await release(owner);
  await release(await held(alpha));
  assert.deepEqual(inventory(temp), before);
  passed.push(
    "distinct processes exclude same root; independent roots, explicit release, identity and source inventory",
  );

  const racing = [probe(alpha), probe(alpha), probe(alpha), probe(alpha)];
  const responses = await Promise.all(racing.map((p) => p.line()));
  assert.equal(responses.filter((v) => v.ok).length, 1);
  for (let i = 0; i < racing.length; i++) {
    if (responses[i].ok) continue;
    assert.deepEqual(responses[i], { ok: false, code: "workspace_busy" });
    await racing[i].finish(1);
  }
  await release(racing[responses.findIndex((v) => v.ok)]);
  passed.push("four simultaneous real contenders produce exactly one holder");

  const metadataOwner = await held(alpha);
  const metadata = join(alpha, "runtime-owner.json");
  for (const pid of [metadataOwner.process.pid, process.pid, 99999999]) {
    writeFileSync(
      metadata,
      JSON.stringify({ pid, timestamp: "1900-01-01", nonce: "not-authority" }),
    );
    await busy(alpha);
    rmSync(metadata);
    await busy(alpha);
  }
  await release(metadataOwner);
  passed.push(
    "hung/live owner remains busy despite stale/reused PID, nonce and missing/replaced metadata",
  );

  const crash = await held(alpha);
  crash.process.kill("SIGKILL");
  await crash.finish(null);
  await release(await held(alpha));
  const eof = await held(alpha);
  eof.process.stdin.end();
  await eof.finish();
  await release(await held(alpha));
  passed.push(
    "kernel reclaims on forced process death and EOF, without stale-lock deletion",
  );

  const link = join(temp, "linked-root");
  symlinkSync(alpha, link);
  for (const path of [link, link + "/", link + "/./"]) {
    const p = probe(path);
    assert.deepEqual(await p.line(), {
      ok: false,
      code: "workspace_unavailable",
    });
    await p.finish(1);
  }
  const parentAlias = join(temp, "parent-alias");
  symlinkSync(temp, parentAlias);
  const aliasOwner = await held(alpha);
  await busy(join(parentAlias, "alpha"));
  await release(aliasOwner);
  rmSync(link);
  rmSync(parentAlias);
  passed.push(
    "final symlinks refuse including slash/dot; ancestor aliases lock same physical scope",
  );

  for (const [path, code] of [
    ["", "invalid_root"],
    [temp + "/alpha/../beta", "invalid_root"],
    [join(alpha, "workspace.json"), "workspace_unavailable"],
    [join(temp, "missing"), "workspace_unavailable"],
  ]) {
    const p = probe(path);
    assert.deepEqual(await p.line(), { ok: false, code });
    await p.finish(1);
  }
  assert.equal(existsSync(join(temp, "missing")), false);
  const relative = probe("alpha", temp);
  assert.equal((await relative.line()).ok, true);
  await release(relative);
  passed.push(
    "missing/file/empty/parent roots refuse without creation; relative root binds acquisition directory",
  );

  const moved = join(temp, "moved-alpha");
  const lost = await held(alpha);
  renameSync(alpha, moved);
  mkdirSync(alpha);
  await busy(moved);
  const replacement = await held(alpha);
  assert.notDeepEqual(replacement.identity, lost.identity);
  await release(replacement);
  await lost.send("check");
  assert.deepEqual(await lost.line(), { ok: false, code: "workspace_changed" });
  await lost.finish(1);
  rmSync(alpha, { recursive: true });
  renameSync(moved, alpha);
  passed.push(
    "moved/replaced path invalidates old root; replacement is explicitly different physical scope, not logical adoption",
  );

  const execOwner = await held(alpha);
  await execOwner.send("spawn-exec-child");
  const descendant = await execOwner.line();
  assert.equal(descendant.state, "child");
  assert(descendant.pid > 0);
  await execOwner.send("release");
  assert.deepEqual(await execOwner.line(), { ok: true, state: "released" });
  process.kill(descendant.pid, 0); // Only the recorded test descendant, not a lease takeover heuristic.
  const nextOwner = await held(alpha);
  process.kill(descendant.pid, 0);
  await release(nextOwner);
  await execOwner.finish();
  passed.push(
    "exec child remains live while next owner acquires: CLOEXEC prevents inherited hidden lease; parent reaps child",
  );

  // Test-only cooperative participant. The current TypeScript product does NOT yet acquire this lease.
  const bridge = join(temp, "cooperative.py");
  writeFileSync(
    bridge,
    "import sys,os,fcntl,json\nf=os.open(sys.argv[1],os.O_RDONLY|os.O_DIRECTORY)\ntry: fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)\nexcept BlockingIOError:\n print(json.dumps({'ok':False,'code':'workspace_busy'}),flush=True);sys.exit(1)\nprint(json.dumps({'ok':True,'state':'held'}),flush=True)\nsys.stdin.readline()\nos.close(f)\n",
  );
  const python = Bun.which("python3");
  assert(
    python,
    "python3 is required for the independent cooperative protocol probe",
  );
  const rustOwner = await held(alpha);
  const blockedPython = start([python, bridge, alpha]);
  assert.deepEqual(await blockedPython.line(), {
    ok: false,
    code: "workspace_busy",
  });
  await blockedPython.finish(1);
  await release(rustOwner);
  const pythonOwner = start([python, bridge, alpha]);
  assert.deepEqual(await pythonOwner.line(), { ok: true, state: "held" });
  await busy(alpha);
  pythonOwner.process.stdin.end();
  await pythonOwner.finish();
  await release(await held(alpha));
  rmSync(bridge);
  passed.push(
    "independent Python flock participant and Rust exclude each other in both directions; no legacy ingress claim",
  );
  assert.deepEqual(inventory(temp), before);
  assert.equal(children.size, 0);
  passed.push(
    "source file inventories unchanged and all foreground process probes reaped",
  );
  console.log(
    JSON.stringify(
      {
        ok: true,
        scope: "cooperative physical-directory process lease only",
        checks: passed.length,
        passed,
        platform: process.platform,
        architecture: process.arch,
        filesystem: owner.filesystem,
        limits: [
          "Not current TS product admission, writable Rust authority, logical workspace identity or offline adoption",
          "Only development host filesystem exercised; no Windows/network/sync-filesystem/distributed qualification",
          "Root check is an observation; protected descriptor-relative writes/commit fencing remain required",
        ],
      },
      null,
      2,
    ),
  );
} finally {
  // Let the Rust probe reap its bounded exec child before stopping a failed test parent.
  for (const child of children) {
    try {
      child.stdin.end();
    } catch {}
  }
  try {
    await bounded(Promise.all([...children].map((child) => child.exited)));
  } finally {
    for (const child of children)
      if (child.exitCode === null) child.kill("SIGKILL");
    await Promise.allSettled([...children].map((child) => child.exited));
  }
  rmSync(temp, { recursive: true, force: true });
}
