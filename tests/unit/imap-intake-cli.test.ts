// Tests: src/cli/imap-intake.ts, src/cli.ts (bilagsmail IMAP intake CLI — #181)
//
// The CLI's `imap-intake poll` drives the production IMAP client, which
// would touch a real server. These tests therefore cover the deterministic,
// network-free surface: command registration and credential validation.
// The poller logic itself is fully covered against an injected fake client
// in imap-intake.test.ts.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

describe("imap-intake poll CLI", () => {
  test("is a registered command and accepts its flags", async () => {
    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "imap-intake", "poll", "--help"],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    expect(stdout).toContain("imap-intake poll");
    expect(stdout).toContain("--imap-host");
    expect(stdout).toContain("--metadata");
  });

  test("fails cleanly when IMAP credentials are missing (no network)", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-imapcli-"));
    const company = join(root, "company");
    // #248: the derived OS user that runs `init` is seeded into the
    // allowlist. Pin USER to `tester` for both calls so the seeded actor
    // and the actor on the mutating `poll` are the same.
    await Bun.$`bun run src/cli.ts init --company ${company}`.env({ ...process.env, USER: "tester" }).quiet();

    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "imap-intake", "poll", "--company", company, "--format", "json"],
      {
        cwd: process.cwd(),
        stdout: "pipe",
        stderr: "pipe",
        // Strip any inherited IMAP env so the missing-credential path is hit
        // deterministically regardless of the host environment.
        env: {
          ...process.env,
          RENTEMESTER_IMAP_HOST: "",
          RENTEMESTER_IMAP_USERNAME: "",
          RENTEMESTER_IMAP_PASSWORD: "",
          USER: "tester",
        },
      },
    );
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(1);
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(false);
    expect(JSON.stringify(parsed.errors)).toContain("IMAP host missing");

    rmSync(root, { recursive: true, force: true });
  });

  test("rejects an invalid --imap-port without contacting a server", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-imapcli-port-"));
    const company = join(root, "company");
    // #248: keep the OS-derived actor consistent between init and poll so
    // the allowlist seed matches who actually runs the mutating command.
    await Bun.$`bun run src/cli.ts init --company ${company}`.env({ ...process.env, USER: "tester" }).quiet();

    const proc = Bun.spawn(
      [
        "bun", "run", "src/cli.ts", "imap-intake", "poll",
        "--company", company, "--imap-port", "not-a-number", "--format", "json",
      ],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe", env: { ...process.env, USER: "tester" } },
    );
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(1);
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(false);
    expect(JSON.stringify(parsed.errors)).toContain("--imap-port");

    rmSync(root, { recursive: true, force: true });
  });
});
