import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { makeWorkspace } from "./server-api/_shared";

const emptyActorEnv = { PATH: process.env.PATH ?? "", HOME: process.env.HOME ?? "" };

function command(args: string[], env = process.env) {
  return Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(), env, stdout: "pipe", stderr: "pipe",
  });
}

describe("workspace registry CLI safety gates", () => {
  test("rejects an unconfirmed mutation and then a mutation without an audit actor", async () => {
    const workspace = mkdtempSync(join(tmpdir(), "rentemester-workspace-registry-cli-"));
    const input = join(workspace, "party.json");
    writeFileSync(input, JSON.stringify({ kind: "organization", name: "Synthetic party", source: "test", observedAt: "2026-01-01", reviewAssertion: "synthetic evidence" }));
    try {
      const noConfirm = command(["party", "create", "--workspace", workspace, "--input", input], emptyActorEnv);
      expect(await noConfirm.exited).toBe(2);
      expect(await new Response(noConfirm.stderr).text()).toContain("--confirm must be exactly yes");

      const noActor = command(["party", "create", "--workspace", workspace, "--input", input, "--confirm", "yes"], emptyActorEnv);
      expect(await noActor.exited).toBe(2);
      expect(await new Response(noActor.stderr).text()).toContain("actor required for mutations");
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  test("keeps read commands independent of mutation actor and confirmation gates", async () => {
    const workspace = makeWorkspace("workspace-registry-cli-read", ["Synthetic Company"]);
    const db = openWorkspaceControlDb(workspace);
    db.close();
    try {
      const read = command(["party", "search", "--workspace", workspace, "--company", "synthetic-company"], emptyActorEnv);
      expect(await read.exited).toBe(0);
      expect(JSON.parse(await new Response(read.stdout).text())).toMatchObject({ ok: true, rows: [], count: 0 });
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
