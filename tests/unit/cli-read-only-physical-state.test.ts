import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { lstatSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, relative } from "node:path";
import { createCompany } from "../../src/core/company";
import { openDb } from "../../src/core/db";
import { companyPaths } from "../../src/core/paths";

function treeIdentity(root: string): Array<{ path: string; sha256: string }> {
  const rows: Array<{ path: string; sha256: string }> = [];
  const visit = (dir: string) => {
    for (const name of readdirSync(dir).sort()) {
      const path = join(dir, name);
      const stat = lstatSync(path);
      if (stat.isDirectory()) visit(path);
      else if (stat.isFile()) rows.push({
        path: relative(root, path),
        sha256: createHash("sha256").update(readFileSync(path)).digest("hex"),
      });
    }
  };
  visit(root);
  return rows;
}

describe("CLI physical read-only contract (#659)", () => {
  test("repeated journal reads and previews preserve main DB and sidecars byte-for-byte", async () => {
    const workspace = mkdtempSync(join(tmpdir(), "rentemester-cli-readonly-"));
    try {
      const created = createCompany(workspace, { name: "Synthetic Read Only ApS", slug: "synthetic" });
      const db = openDb(companyPaths(created.companyRoot).db);
      db.run("PRAGMA wal_checkpoint(TRUNCATE)");
      db.close();
      const payloadPath = join(workspace, "preview.json");
      writeFileSync(payloadPath, JSON.stringify({
        transactionDate: "2026-05-16",
        text: "Synthetic preview",
        lines: [
          { accountNo: "2000", debitAmount: 100 },
          { accountNo: "5020", creditAmount: 100 },
        ],
      }));
      const dashboardPath = join(workspace, "dashboard.html");
      const compliancePath = join(workspace, "compliance.html");
      const before = treeIdentity(created.companyRoot);

      for (let pass = 0; pass < 2; pass += 1) {
        for (const command of [
          ["journal", "list", "--company", created.companyRoot, "--format", "json"],
          ["journal", "dry-run", "--company", created.companyRoot, "--input", payloadPath],
          ["system", "healthcheck", "--company", created.companyRoot, "--format", "json"],
          ["dashboard", "--company", created.companyRoot, "--out", dashboardPath, "--as-of", "2026-05-16", "--format", "json"],
          ["compliance", "report", "--company", created.companyRoot, "--out", compliancePath, "--as-of", "2026-05-16", "--as-of-instant", "2026-05-16T12:00:00.000Z", "--format", "json"],
        ]) {
          const process = Bun.spawn(
            ["bun", "run", "src/cli.ts", ...command],
            { stdout: "pipe", stderr: "pipe" },
          );
          const stdout = await new Response(process.stdout).text();
          const stderr = await new Response(process.stderr).text();
          expect(await process.exited, stderr).toBe(0);
          expect(JSON.parse(stdout)).toBeDefined();
        }
      }

      expect(treeIdentity(created.companyRoot)).toEqual(before);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
