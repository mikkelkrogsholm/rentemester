// Tests: src/cli/period.ts, src/cli.ts (period CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("period close CLI", () => {
  test("closes a period and blocks later journal posting inside that period", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-periodcli-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const closeProc = Bun.spawn([
      "bun", "run", "src/cli.ts", "period", "close",
      "--company", company,
      "--from", "2026-05-01",
      "--to", "2026-05-31",
      "--kind", "vat_quarter",
      "--reference", "SKAT-Q2-2026"
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

    cleanupDir(root);

    expect({ closeExitCode, closeStderr }).toEqual({ closeExitCode: 0, closeStderr: "" });
    const closed = JSON.parse(closeStdout);
    expect(closed.ok).toBe(true);
    expect(closed.kind).toBe("vat_quarter");

    expect({ postExitCode, postStderr }).toEqual({ postExitCode: 1, postStderr: "" });
    const blocked = JSON.parse(postStdout);
    expect(blocked.ok).toBe(false);
    expect(blocked.errors).toContain("transactionDate 2026-05-16 falls in closed period vat_quarter 2026-05-01..2026-05-31 ref SKAT-Q2-2026");
  });
});

describe("period reopen CLI (#247)", () => {
  test("requires an actor, then reopens a closed period and unblocks posting", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-periodreopen-"));
    const company = join(root, "company");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    // Close 2026-05.
    await Bun.$`bun run src/cli.ts period close --company ${company} --from 2026-05-01 --to 2026-05-31 --kind vat_quarter --actor user:ejer`.quiet();

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

    cleanupDir(root);
  });

  test("refuses to reopen without a reason", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-periodreopen-noreason-"));
    const company = join(root, "company");
    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts period close --company ${company} --from 2026-05-01 --to 2026-05-31 --kind vat_quarter --actor user:ejer`.quiet();

    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "period", "reopen", "--company", company,
        "--from", "2026-05-01", "--to", "2026-05-31", "--kind", "vat_quarter", "--actor", "user:ejer"],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stderr = await new Response(proc.stderr).text();
    const exit = await proc.exited;
    expect(exit).toBe(2);
    expect(stderr).toContain("Missing required --reason");

    cleanupDir(root);
  });
});
