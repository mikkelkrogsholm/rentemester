// Tests: src/cli/period.ts, src/cli.ts (period CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, unlinkSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

async function reviewReadiness(company: string, from: string, to: string) {
  const readiness = JSON.parse(await Bun.$`bun run src/cli.ts period readiness --company ${company} --from ${from} --to ${to} --format json`.text());
  const review = JSON.parse(await Bun.$`bun run src/cli.ts period review --company ${company} --from ${from} --to ${to} --packet-hash ${readiness.packet.hash} --confirm yes --actor user:ejer --format json`.text());
  return { readiness: readiness.packet, review };
}

describe("period close CLI", () => {
  test("review binds the exact read-only packet hash before it persists evidence", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-periodreview-cli-"));
    const company = join(root, "company");
    await Bun.$`bun run src/cli.ts init --company ${company} --vat-period month`.quiet();
    const proc = Bun.spawn([
      "bun", "run", "src/cli.ts", "period", "review", "--company", company,
      "--from", "2026-05-01", "--to", "2026-05-31", "--packet-hash", "0".repeat(64),
      "--confirm", "yes", "--actor", "user:ejer", "--format", "json",
    ], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    rmSync(root, { recursive: true, force: true });
    expect(exitCode).toBe(1);
    expect(JSON.parse(stdout)).toMatchObject({ ok: false, errors: ["PERIOD_CLOSE_PACKET_STALE_OR_MISSING"] });
  });

  test("closes a period and blocks later journal posting inside that period", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-periodcli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company} --vat-period month`.quiet();

    const { readiness, review } = await reviewReadiness(company, "2026-05-01", "2026-05-31");

    const closeProc = Bun.spawn([
      "bun", "run", "src/cli.ts", "period", "close",
      "--company", company,
      "--from", "2026-05-01",
      "--to", "2026-05-31",
      "--kind", "vat_quarter",
      "--reference", "SKAT-Q2-2026", "--packet-hash", readiness.hash,
      "--review-id", String(review.id), "--confirm", "yes", "--actor", "user:ejer"
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });
    const closeStdout = await new Response(closeProc.stdout).text();
    const closeStderr = await new Response(closeProc.stderr).text();
    const closeExitCode = await closeProc.exited;

    const postProc = Bun.spawn([
      "bun", "run", "src/cli.ts", "journal", "post",
      "--company", company,
      "--input", "examples/journal-entry.owner-contribution.json"
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
      env: { ...process.env, RENTEMESTER_TODAY: "2026-06-15" },
    });
    const postStdout = await new Response(postProc.stdout).text();
    const postStderr = await new Response(postProc.stderr).text();
    const postExitCode = await postProc.exited;

    rmSync(root, { recursive: true, force: true });

    expect({ closeExitCode, closeStderr }).toEqual({ closeExitCode: 0, closeStderr: "" });
    const closed = JSON.parse(closeStdout);
    expect(closed.ok).toBe(true);
    expect(closed.kind).toBe("vat_period");

    expect({ postExitCode, postStderr }).toEqual({ postExitCode: 1, postStderr: "" });
    const blocked = JSON.parse(postStdout);
    expect(blocked.ok).toBe(false);
    expect(blocked.errors).toContain("transactionDate 2026-05-16 falls in closed period vat_period 2026-05-01..2026-05-31 ref SKAT-Q2-2026");
  });
});

describe("period reopen CLI (#247)", () => {
  test("reopen preserves explicit and derived machine actor behavior without company policy", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-periodreopen-no-policy-"));
    const company = join(root, "company");
    try {
      await Bun.$`bun run src/cli.ts init --company ${company} --vat-period month`.quiet();
      const { readiness, review } = await reviewReadiness(company, "2026-05-01", "2026-05-31");
      await Bun.$`bun run src/cli.ts period close --company ${company} --from 2026-05-01 --to 2026-05-31 --packet-hash ${readiness.hash} --review-id ${review.id} --confirm yes --actor user:ejer`.quiet();
      unlinkSync(join(company, "config", "policy.yaml"));
      for (const actor of ["agent:synthetic", "system:synthetic"]) {
        const proc = Bun.spawn(["bun", "run", "src/cli.ts", "period", "reopen", "--company", company,
          "--from", "2026-05-01", "--to", "2026-05-31", "--reason", "Synthetic late evidence",
          "--actor", actor, "--format", "json"], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" });
        expect(await new Response(proc.stdout).text()).toBe("");
        expect(await new Response(proc.stderr).text()).toContain("actor_allowlist");
        expect(await proc.exited).toBe(2);
      }
      const proc = Bun.spawn(["bun", "run", "src/cli.ts", "period", "reopen", "--company", company,
        "--from", "2026-05-01", "--to", "2026-05-31", "--reason", "Synthetic late evidence", "--format", "json"], {
        cwd: process.cwd(), stdout: "pipe", stderr: "pipe",
        env: { PATH: process.env.PATH ?? "", HOME: process.env.HOME ?? "", RENTEMESTER_AGENT: "synthetic" },
      });
      const result = JSON.parse(await new Response(proc.stdout).text());
      expect(await new Response(proc.stderr).text()).toBe("");
      expect(await proc.exited).toBe(0);
      expect(result).toMatchObject({ ok: true, effectiveStatus: "open", reopenedBy: "agent:synthetic via rentemester-cli" });
    } finally { rmSync(root, { recursive: true, force: true }); }
  });

  test("central actor policy rejects malformed and unallowlisted actors and attributes a derived actor", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-periodreopen-policy-"));
    const company = join(root, "company");
    try {
      await Bun.$`bun run src/cli.ts init --company ${company} --vat-period month`.quiet();
      const { readiness, review } = await reviewReadiness(company, "2026-05-01", "2026-05-31");
      await Bun.$`bun run src/cli.ts period close --company ${company} --from 2026-05-01 --to 2026-05-31 --packet-hash ${readiness.hash} --review-id ${review.id} --confirm yes --actor user:ejer`.quiet();
      const base = ["bun", "run", "src/cli.ts", "period", "reopen", "--company", company,
        "--from", "2026-05-01", "--to", "2026-05-31", "--reason", "Synthetic late evidence", "--format", "json"];
      for (const [extra, user, expected] of [
        [["--actor", "malformed"], "ejer", "explicit actor must use canonical format"],
        [["--actor", "user:outsider"], "ejer", "actor_allowlist"],
        [[], "outsider", "actor_allowlist"],
      ] as const) {
        const proc = Bun.spawn([...base, ...extra], {
          cwd: process.cwd(), stdout: "pipe", stderr: "pipe",
          env: { PATH: process.env.PATH ?? "", HOME: process.env.HOME ?? "", USER: user },
        });
        expect(await new Response(proc.stdout).text()).toBe("");
        expect(await new Response(proc.stderr).text()).toContain(expected);
        expect(await proc.exited).toBe(2);
      }
      // The denied attempts must leave the closed period available to this one reopen.
      const proc = Bun.spawn(base, {
        cwd: process.cwd(), stdout: "pipe", stderr: "pipe",
        env: { PATH: process.env.PATH ?? "", HOME: process.env.HOME ?? "", USER: "ejer" },
      });
      const result = JSON.parse(await new Response(proc.stdout).text());
      expect(await new Response(proc.stderr).text()).toBe("");
      expect(await proc.exited).toBe(0);
      expect(result).toMatchObject({ ok: true, effectiveStatus: "open", reopenedBy: "user:ejer via rentemester-cli" });
    } finally { rmSync(root, { recursive: true, force: true }); }
  });

  test("requires an actor, then reopens a closed period and unblocks posting", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-periodreopen-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company} --vat-period month`.quiet();

    // Close 2026-05.
    const { readiness, review } = await reviewReadiness(company, "2026-05-01", "2026-05-31");
    await Bun.$`bun run src/cli.ts period close --company ${company} --from 2026-05-01 --to 2026-05-31 --kind vat_quarter --actor user:ejer --packet-hash ${readiness.hash} --review-id ${review.id} --confirm yes`.quiet();

    // Reopen with no actor at all — must be refused (clearly attributable).
    const noActor = Bun.spawn(
      ["bun", "run", "src/cli.ts", "period", "reopen", "--company", company,
        "--from", "2026-05-01", "--to", "2026-05-31", "--kind", "vat_quarter",
        "--reason", "Manglende bilag", "--format", "json"],
      {
        cwd: process.cwd(),
        stdout: "pipe",
        stderr: "pipe",
        // Strip every actor-bearing env var so no actor can be inferred.
        env: { PATH: process.env.PATH ?? "", HOME: process.env.HOME ?? "" },
      },
    );
    const noActorStderr = await new Response(noActor.stderr).text();
    const noActorExit = await noActor.exited;
    expect(noActorExit).toBe(2);
    expect(noActorStderr).toContain("actor required for mutations");

    // Reopen properly with an explicit allow-listed actor.
    const reopen = Bun.spawn(
      ["bun", "run", "src/cli.ts", "period", "reopen", "--company", company,
        "--from", "2026-05-01", "--to", "2026-05-31", "--kind", "vat_quarter",
        "--reason", "Restaurantbilag bogført for sent", "--actor", "user:ejer", "--format", "json"],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const reopenStdout = await new Response(reopen.stdout).text();
    const reopenStderr = await new Response(reopen.stderr).text();
    const reopenExit = await reopen.exited;
    expect({ reopenExit, reopenStderr }).toEqual({ reopenExit: 0, reopenStderr: "" });
    const reopened = JSON.parse(reopenStdout);
    expect(reopened.ok).toBe(true);
    expect(reopened.effectiveStatus).toBe("open");
    expect(reopened.reopenedBy).toContain("user:ejer");

    // A posting inside the reopened period is now accepted.
    const post = Bun.spawn(
      ["bun", "run", "src/cli.ts", "journal", "post", "--company", company,
        "--input", "examples/journal-entry.owner-contribution.json", "--actor", "user:ejer"],
      {
        cwd: process.cwd(),
        stdout: "pipe",
        stderr: "pipe",
        env: { ...process.env, RENTEMESTER_TODAY: "2026-06-15" },
      },
    );
    const postStdout = await new Response(post.stdout).text();
    const postExit = await post.exited;
    expect(postExit).toBe(0);
    expect(JSON.parse(postStdout).ok).toBe(true);

    rmSync(root, { recursive: true, force: true });
  });

  test("refuses to reopen without a reason", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-periodreopen-noreason-"));
    const company = join(root, "company");
    await Bun.$`bun run src/cli.ts init --company ${company} --vat-period month`.quiet();
    const { readiness, review } = await reviewReadiness(company, "2026-05-01", "2026-05-31");
    await Bun.$`bun run src/cli.ts period close --company ${company} --from 2026-05-01 --to 2026-05-31 --kind vat_quarter --actor user:ejer --packet-hash ${readiness.hash} --review-id ${review.id} --confirm yes`.quiet();

    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "period", "reopen", "--company", company,
        "--from", "2026-05-01", "--to", "2026-05-31", "--kind", "vat_quarter", "--actor", "user:ejer"],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stderr = await new Response(proc.stderr).text();
    const exit = await proc.exited;
    expect(exit).toBe(2);
    expect(stderr).toContain("Missing required --reason");

    rmSync(root, { recursive: true, force: true });
  });
});
