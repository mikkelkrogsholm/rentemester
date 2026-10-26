import { expect, test } from "bun:test";
import { appendFileSync, chmodSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createCompany } from "../../src/core/company";
import { initWorkspace, companyRootForSlug } from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { migrate, openDb } from "../../src/core/db";
import { createVendor } from "../../src/core/master-data";
import { ingestDocument } from "../../src/core/documents";
import { createParty, linkPartyRole } from "../../src/core/party-registry";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { grantCompanyMembership } from "../../src/core/workspace-access";
import { planDocumentPartyLink } from "../../src/core/document-party-links";
import { createSystemBackup } from "../../src/core/system-backups";
import { restoreSystemBackup } from "../../src/core/system-restore";

test("#639 CLI uses verified bootstrap subject, never actor or caller principal", async () => {
  const temp = mkdtempSync(join(tmpdir(), "rm-principal-639-"));
  const workspace = join(temp, "workspace");
  const baseEnv = { PATH: process.env.PATH, HOME: process.env.HOME };
  const run = async (args: string[], env: Record<string, string | undefined> = baseEnv) => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], { env, stdout: "pipe", stderr: "pipe" });
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    return { code: await proc.exited, stdout, stderr };
  };
  const good = async (args: string[], env: Record<string, string | undefined> = baseEnv) => {
    const result = await run(args, env);
    expect(result.code, result.stderr).toBe(0);
    return JSON.parse(result.stdout);
  };
  try {
    initWorkspace(workspace);
    const company = createCompany(workspace, { name: "Synthetic Principal Company", onboardingActor: "agent:codex" });
    const root = companyRootForSlug(workspace, company.slug);
    appendFileSync(join(root, "config/policy.yaml"), "  agents:\n    - agent:principal-reviewer\n    - agent:second-reviewer\n");
    const secretFile = join(temp, "auth-secret");
    writeFileSync(secretFile, "I0UjL6i0-ScgvjfIgzMKJxPQyDpPXwg2mMKdLW3Y3WQ", { mode: 0o600 });
    chmodSync(secretFile, 0o600);
    const service = await good(["workspace-access", "bootstrap-local-service", "--workspace", workspace,
      "--company", company.slug, "--display-name", "Synthetic bookkeeper", "--company-role", "bookkeeper",
      "--auth-secret-file", secretFile, "--confirm", "yes", "--actor", "agent:codex"]);
    const env = { ...baseEnv, RENTEMESTER_WORKSPACE: workspace, RENTEMESTER_SERVICE_PRINCIPAL_TOKEN: service.credential };
    const db = openDb(companyPaths(root).db); migrate(db);
    const vendor = createVendor(db, { name: "Synthetic Foreign Supplier", address: "1 Synthetic Road", notes: "Preserve note" });
    if (!vendor.ok) throw new Error("fixture vendor failed");
    const file = join(temp, "invoice.txt"); writeFileSync(file, "Synthetic immutable source");
    const document = ingestDocument(db, root, file, { source: "synthetic", documentType: "purchase_sale",
      issueDate: "2026-09-01", invoiceNo: "SYN-639", amountIncVat: 100, vatAmount: 0, currency: "DKK",
      deliveryDescription: "Synthetic service", sender: { name: "Synthetic Foreign Supplier", address: "1 Synthetic Road", countryCode: "US", identifierKind: "non_eu" },
      recipient: { name: company.name, address: "Buyer Road", vatOrCvr: "DK87654321" } });
    if (!document.ok) throw new Error("fixture document failed");
    const original = db.query("SELECT * FROM documents").all();
    const control = openWorkspaceControlDb(workspace);
    createParty(control, { partyId: "synthetic-639", kind: "organization", name: "Synthetic Foreign Supplier", identifiers: [], source: "synthetic", observedAt: "2026-09-01T00:00:00.000Z", reviewAssertion: "Reviewed original", actor: "agent:codex" });
    linkPartyRole(control, { partyId: "synthetic-639", companySlug: company.slug, role: "vendor", actor: "agent:codex" });
    const identity = ["--workspace", workspace, "--company", company.slug, "--vendor-id", String(vendor.vendorId), "--document-id", String(document.documentId), "--country-code", "US", "--identifier-kind", "non_eu", "--reviewed-reference", "review:synthetic:639"];
    const enrichment = await good(["vendor-identity-enrichment", "plan", ...identity]);
    const approval = ["--actor", "agent:principal-reviewer", "--confirm", "yes"];
    await good(["vendor-identity-enrichment", "apply", ...identity, ...approval, "--plan-hash", enrichment.plan.planHash, "--idempotency-key", "enrich-639"], env);
    const mapping = ["--workspace", workspace, "--company", company.slug, "--legacy-kind", "vendor", "--legacy-id", String(vendor.vendorId), "--party-id", "synthetic-639", "--role", "vendor", "--document-id", String(document.documentId), "--reviewed-legacy-reference", "review:synthetic:639"];
    const mapped = await good(["legacy-party-mapping", "plan", ...mapping]);
    await good(["legacy-party-mapping", "apply", ...mapping, ...approval, "--plan-hash", mapped.plan.planHash, "--idempotency-key", "mapping-639"], env);
    const input = { companySlug: company.slug, documentId: document.documentId, legacyKind: "vendor" as const, legacyId: String(vendor.vendorId), partyId: "synthetic-639", role: "vendor" as const, reviewedLegacyReference: "review:synthetic:639" };
    const plan = planDocumentPartyLink(db, control, input);
    if (!plan.ok) throw new Error("document plan failed");
    const args = ["documents", "party-link-apply", "--workspace", workspace, "--company", root, "--company-slug", company.slug, "--document-id", String(document.documentId), "--role", "vendor", "--party-id", "synthetic-639", "--legacy-kind", "vendor", "--legacy-id", String(vendor.vendorId), "--reviewed-legacy-reference", "review:synthetic:639", "--plan-hash", plan.plan.planHash, "--idempotency-key", "link-639", "--confirm", "yes"];
    const denied = async (testEnv: Record<string, string | undefined>, extra: string[] = []) => {
      expect((await run([...args, "--actor", "agent:principal-reviewer", ...extra], testEnv)).code).not.toBe(0);
      expect(db.query("SELECT count(*) AS n FROM document_party_link_events").get()).toEqual({ n: 0 });
    };
    await denied(baseEnv);
    await denied({ ...env, RENTEMESTER_SERVICE_PRINCIPAL_TOKEN: "invalid" });
    const expiry = control.query('SELECT "expiresAt" FROM apikey WHERE id=?').get(service.credentialId) as { expiresAt: string };
    control.query('UPDATE apikey SET "expiresAt"=? WHERE id=?').run("2000-01-01T00:00:00.000Z", service.credentialId);
    await denied(env);
    control.query('UPDATE apikey SET "expiresAt"=? WHERE id=?').run(expiry.expiresAt, service.credentialId);
    await denied(env, ["--principal", "service-account:someone-else"]);
    grantCompanyMembership(control, workspace, { userId: service.serviceAccountId, companySlug: company.slug, role: "reader", actor: "agent:codex" });
    await denied(env);
    grantCompanyMembership(control, workspace, { userId: service.serviceAccountId, companySlug: company.slug, role: "bookkeeper", actor: "agent:codex" });
    expect(await good([...args, "--actor", "agent:principal-reviewer"], env)).toMatchObject({ ok: true, idempotent: false });
    expect(await good([...args, "--actor", "agent:second-reviewer"], env)).toMatchObject({ ok: true, idempotent: true });
    const evidence = db.query("SELECT principal,actor FROM document_party_link_events").all();
    expect(evidence).toEqual([{ principal: `service-account:${service.serviceAccountId}`, actor: "agent:principal-reviewer" }]);
    expect(db.query("SELECT * FROM documents").all()).toEqual(original);
    const backup = createSystemBackup(db, root); expect(backup.ok).toBeTrue();
    const restoredRoot = join(temp, "restored");
    expect(restoreSystemBackup({ backupDir: backup.backupDir!, targetCompanyRoot: restoredRoot }).ok).toBeTrue();
    db.close();
    const reopened = openDb(companyPaths(root).db);
    expect(reopened.query("SELECT principal,actor FROM document_party_link_events").all()).toEqual(evidence); reopened.close();
    const restored = openDb(companyPaths(restoredRoot).db);
    expect(restored.query("SELECT principal,actor FROM document_party_link_events").all()).toEqual(evidence); restored.close();
    await good(["workspace-access", "local-service-revoke", "--workspace", workspace, "--company", company.slug, "--service-account-id", service.serviceAccountId, "--credential-id", service.credentialId, "--auth-secret-file", secretFile, "--confirm", "yes", "--actor", "agent:codex"]);
    expect((await run([...args, "--actor", "agent:principal-reviewer"], env)).code).not.toBe(0);
    control.close();
  } finally { rmSync(temp, { recursive: true, force: true }); }
}, 20000);
