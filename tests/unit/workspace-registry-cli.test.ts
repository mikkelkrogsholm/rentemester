import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { initWorkspace } from "../../src/core/workspace";
import { workspaceControlPaths } from "../../src/core/workspace-control";

async function run(args: string[]) {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args, "--format", "json"], {
    cwd: process.cwd(), stdout: "pipe", stderr: "pipe",
    env: { PATH: process.env.PATH ?? "", HOME: process.env.HOME ?? "" },
  });
  const [stdout, stderr, exit] = await Promise.all([
    new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited,
  ]);
  return { exit, stdout, stderr };
}

describe("workspace registry CLI mutation scope", () => {
  for (const [family, command, message] of [
    ["workspace", "snapshot", "--confirm yes required"],
    ["workspace", "restore", "--confirm yes required"],
    ["recurring-invoice", "run-workspace", "--confirm yes required to run workspace recurring invoices"],
    ["efaktura", "modtag-workspace", "--confirm yes required to poll workspace DigiSense inbound"],
    ["group", "propose-mapping", "--confirm yes required to propose intercompany mapping"],
    ["group", "propose-profile", "--confirm yes required to propose consolidation profile"],
    ["group", "propose-elimination", "--confirm yes required to propose consolidation elimination"],
    ["group", "propose-disposition", "--confirm yes required to propose intercompany disposition"],
  ] as const) {
    test(`${family} ${command} preserves confirmation rejection before missing or malformed identity`, async () => {
      for (const extra of [[], ["--actor", "malformed"]]) {
        const result = await run([family, command, ...extra]);
        expect(result.exit).toBe(1);
        expect(JSON.parse(result.stdout)).toMatchObject({ ok: false, errors: [message] });
      }
    });
  }

  for (const [command, message] of [
    ["snapshot", "workspace snapshot requires --workspace and --out"],
    ["restore", "workspace restore requires --snapshot and --target-workspace"],
  ] as const) {
    test(`workspace ${command} preserves normalized confirmation and target validation before identity`, async () => {
      for (const extra of [[], ["--actor", "malformed"]]) {
        const result = await run(["workspace", command, "--confirm", " YES ", ...extra]);
        expect(result.exit).toBe(2);
        expect(result.stdout).toBe("");
        expect(result.stderr).toContain(message);
      }
    });
  }

  for (const [family, commands] of [
    ["ownership", ["propose", "review", "apply"]],
    ["company-knowledge", ["propose", "review", "supersede"]],
  ] as const) {
    for (const command of commands) {
      test(`${family} ${command} refuses missing and malformed actors before control DB creation`, async () => {
        const workspace = mkdtempSync(join(tmpdir(), "rentemester-registry-gate-"));
        try {
          initWorkspace(workspace);
          for (const [extra, message] of [
            [[], "actor required for mutations"],
            [["--actor", "malformed"], "explicit actor must use canonical format"],
          ] as const) {
            const result = await run([family, command, "--workspace", workspace, "--confirm", "yes", ...extra]);
            expect(result.exit).toBe(2);
            expect(result.stdout).toBe("");
            expect(result.stderr).toContain(message);
            expect(existsSync(workspaceControlPaths(workspace).db)).toBe(false);
          }
        } finally { rmSync(workspace, { recursive: true, force: true }); }
      });
    }
  }

  test("ownership proposes, independently reviews and applies exact hashes without company-ledger routing", async () => {
    const workspace = mkdtempSync(join(tmpdir(), "rentemester-ownership-cli-"));
    try {
      initWorkspace(workspace);
      const path = join(workspace, "synthetic-ownership.json");
      writeFileSync(path, JSON.stringify({ snapshotId: "synthetic-snapshot", source: "synthetic", observedAt: "2026-02-01T00:00:00Z", facts: [
        { owner: { kind: "company", companySlug: "synthetic-parent" }, ownedCompanySlug: "synthetic-child", validFrom: "2026-01-01", economicBasisPoints: 10000, controlType: "equity", jurisdiction: "DK", evidenceRefs: ["synthetic-evidence"] },
      ] }));
      const base = ["--workspace", workspace, "--confirm", "yes"];
      const proposed = await run(["ownership", "propose", ...base, "--input", path, "--actor", "user:maker", "--principal-id", "maker"]);
      expect(proposed.exit, proposed.stderr).toBe(0);
      const snapshot = JSON.parse(proposed.stdout).snapshot;
      const samePrincipal = await run(["ownership", "review", ...base, "--snapshot-id", snapshot.snapshotId, "--decision", "approved", "--actor", "user:maker", "--principal-id", "maker"]);
      expect(samePrincipal.exit).toBe(1);
      expect(samePrincipal.stderr).toContain("principal");
      const reviewed = await run(["ownership", "review", ...base, "--snapshot-id", snapshot.snapshotId, "--decision", "approved", "--actor", "user:reviewer", "--principal-id", "reviewer"]);
      expect(reviewed.exit, reviewed.stderr).toBe(0);
      const apply = ["ownership", "apply", ...base, "--snapshot-id", snapshot.snapshotId, "--snapshot-hash", snapshot.snapshotHash, "--diff-hash", snapshot.diffHash, "--actor", "user:reviewer", "--principal-id", "reviewer"];
      const applied = await run(apply);
      expect(applied.exit, applied.stderr).toBe(0);
      expect(JSON.parse(applied.stdout)).toMatchObject({ ok: true, status: "applied" });
      expect(JSON.parse((await run(apply)).stdout)).toMatchObject({ ok: true, status: "unchanged" });
      expect(existsSync(join(workspace, "synthetic-parent"))).toBe(false);
      expect(existsSync(join(workspace, "synthetic-child"))).toBe(false);
    } finally { rmSync(workspace, { recursive: true, force: true }); }
  });

  test("company knowledge retains slug metadata and review/supersession without a company path", async () => {
    const workspace = mkdtempSync(join(tmpdir(), "rentemester-knowledge-cli-"));
    try {
      initWorkspace(workspace);
      const path = join(workspace, "synthetic-knowledge.json");
      const input = { assertionId: "synthetic-knowledge", predicate: "markets", value: ["DK"], source: { kind: "user", ref: "synthetic-source" }, validFrom: "2026-01-01T00:00:00Z" };
      writeFileSync(path, JSON.stringify(input));
      const base = ["--workspace", workspace, "--confirm", "yes", "--actor", "user:operator", "--principal-id", "operator"];
      const proposed = await run(["company-knowledge", "propose", ...base, "--company", "synthetic-company", "--input", path]);
      expect(proposed.exit, proposed.stderr).toBe(0);
      expect(JSON.parse(proposed.stdout)).toMatchObject({ assertion: { companySlug: "synthetic-company", reviewState: "proposed" } });
      const reviewed = await run(["company-knowledge", "review", ...base, "--assertion-id", input.assertionId, "--decision", "approved"]);
      expect(reviewed.exit, reviewed.stderr).toBe(0);
      writeFileSync(path, JSON.stringify({ ...input, assertionId: "synthetic-replacement", value: ["DK", "SE"] }));
      const superseded = await run(["company-knowledge", "supersede", ...base, "--assertion-id", input.assertionId, "--replacement", path]);
      expect(superseded.exit, superseded.stderr).toBe(0);
      expect(JSON.parse(superseded.stdout)).toMatchObject({ superseded: { reviewState: "superseded" }, replacement: { companySlug: "synthetic-company", reviewState: "proposed" } });
      expect(existsSync(join(workspace, "synthetic-company"))).toBe(false);
    } finally { rmSync(workspace, { recursive: true, force: true }); }
  });
});
