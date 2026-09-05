import { describe, expect, test } from "bun:test";
import { appendFileSync, chmodSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { createCompany } from "../../src/core/company";
import { openDb } from "../../src/core/db";
import { companyPaths } from "../../src/core/paths";
import { companyRootForSlug, initWorkspace } from "../../src/core/workspace";
import { tmpRoot } from "./server-api/_shared";

type CliResult = { exitCode: number; stdout: string; stderr: string; json: any };

async function runCli(args: string[], env: Record<string, string | undefined> = {}): Promise<CliResult> {
  const child = Bun.spawn(["bun", "run", "src/cli.ts", ...args, "--json"], {
    cwd: process.cwd(), stdout: "pipe", stderr: "pipe", env: { ...process.env, ...env },
  });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited,
  ]);
  return { exitCode, stdout, stderr, json: stdout.trim() ? JSON.parse(stdout) : null };
}

describe("purchase-case CLI lifecycle", () => {
  test("shows stale source evidence, reassesses the exact version, and updates period readiness", async () => {
    const workspace = tmpRoot("purchase-case-cli");
    const authSecret = "I0UjL6i0-ScgvjfIgzMKJxPQyDpPXwg2mMKdLW3Y3WQ";
    try {
      initWorkspace(workspace);
      const company = createCompany(workspace, { name: "Synthetic CLI Purchase Company" });
      const companyRoot = companyRootForSlug(workspace, company.slug);
      appendFileSync(join(companyRoot, "config", "policy.yaml"), "  agents:\n    - agent:codex\n    - agent:reviewer\n");
      const secretPath = join(workspace, "auth-secret");
      writeFileSync(secretPath, authSecret, { mode: 0o600 }); chmodSync(secretPath, 0o600);
      const bootstrap = await runCli(["workspace-access", "bootstrap-local-service", "--workspace", workspace, "--company", company.slug, "--display-name", "Synthetic purchase reviewer", "--company-role", "owner", "--auth-secret-file", secretPath, "--confirm", "yes", "--actor", "agent:codex"]);
      expect(bootstrap.exitCode, bootstrap.stderr).toBe(0);
      const reviewerBootstrap = await runCli(["workspace-access", "bootstrap-local-service", "--workspace", workspace, "--company", company.slug, "--display-name", "Independent synthetic reviewer", "--company-role", "owner", "--auth-secret-file", secretPath, "--confirm", "yes", "--actor", "agent:reviewer"]);
      expect(reviewerBootstrap.exitCode, reviewerBootstrap.stderr).toBe(0);
      const env = { RENTEMESTER_WORKSPACE: workspace, RENTEMESTER_SERVICE_PRINCIPAL_TOKEN: bootstrap.json.credential as string };
      const reviewerEnv = { RENTEMESTER_WORKSPACE: workspace, RENTEMESTER_SERVICE_PRINCIPAL_TOKEN: reviewerBootstrap.json.credential as string };
      const db = openDb(companyPaths(companyRoot).db);
      db.run("INSERT INTO bank_transactions(id,transaction_date,amount,currency,text,transaction_hash) VALUES(1,'2026-01-02',-125,'DKK','Synthetic original',?)", "a".repeat(64));
      db.close();

      const created = await runCli(["purchase-case", "create", "--company", company.slug, "--case-id", "cli-stale-case", "--source-kind", "bank_transaction", "--source-id", "1", "--idempotency-key", "cli-stale-create", "--confirm", "yes", "--actor", "agent:codex"], env);
      expect(created.exitCode, created.stderr).toBe(0);
      const original = created.json.purchaseCase as { version: number; sourceFingerprint: string };

      const changed = openDb(companyPaths(companyRoot).db);
      changed.run("UPDATE bank_transactions SET text='Synthetic changed' WHERE id=1");
      changed.close();

      const stale = await runCli(["purchase-case", "show", "--company", company.slug, "--case-id", "cli-stale-case"], env);
      expect(stale.json.purchaseCase).toMatchObject({ version: 1, sourceStatus: { status: "stale" } });
      const currentFingerprint = stale.json.purchaseCase.sourceStatus.currentSourceFingerprint as string;
      const readiness = await runCli(["period", "readiness", "--company", company.slug, "--from", "2026-01-01", "--to", "2026-01-31"], env);
      expect(readiness.json.packet.items.find((item: any) => item.code === "PURCHASE_CASE_DOCUMENTATION")).toMatchObject({ status: "blocked", evidence: [{ sourceStatus: "stale" }] });

      const reassessed = await runCli(["purchase-case", "reassess", "--company", company.slug, "--case-id", "cli-stale-case", "--expected-version", String(original.version), "--expected-source-fingerprint", original.sourceFingerprint, "--current-source-fingerprint", currentFingerprint, "--documentation-outcome", "ordinary_evidence_sufficient", "--reason", "Synthetic CLI source review", "--idempotency-key", "cli-stale-reassess", "--confirm", "yes", "--actor", "agent:reviewer"], reviewerEnv);
      expect(reassessed.exitCode, `${reassessed.stderr}\n${reassessed.stdout}`).toBe(0);
      expect(reassessed.json.purchaseCase).toMatchObject({ version: 2, sourceFingerprint: currentFingerprint, sourceStatus: { status: "current" } });
      const readBack = await runCli(["purchase-case", "show", "--company", company.slug, "--case-id", "cli-stale-case"], env);
      expect(readBack.json.purchaseCase).toMatchObject({ version: 2, documentationOutcome: "ordinary_evidence_sufficient", sourceStatus: { status: "current" } });
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
