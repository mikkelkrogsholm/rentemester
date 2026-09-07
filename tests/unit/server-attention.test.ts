import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../src/server/router";
import { createCompany } from "../../src/core/company";
import { initWorkspace, companyRootForSlug } from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { recordException } from "../../src/core/exceptions";

test("#649 attention is read-only and does not duplicate an agent exception as a suggestion", async () => {
  const root = mkdtempSync(join(tmpdir(), "rentemester-attention-"));
  try {
    initWorkspace(root); const created = createCompany(root, { name: "Acme ApS" }); const db = openDb(companyPaths(companyRootForSlug(root, created.slug)).db);
    try { migrate(db); recordException(db, { type: "AGENT_PAYABLE_OVERDUE", severity: "high", message: "Gennemgå forfalden post", requiredAction: "Betal eller afklar posten" }); } finally { db.close(); }
    const response = await handleRequest(new Request(`http://localhost/api/companies/${created.slug}/attention`), { host: "127.0.0.1", port: 0, workspaceRoot: root, authRequired: false, authToken: null });
    expect(response.status).toBe(200); const body = await response.json() as any;
    expect(body.attention.items.filter((item: any) => item.sourceIdentity === "exception:1")).toHaveLength(1);
    expect(body.attention.items[0]).toMatchObject({ source: "agent-proposal", actor: null, destination: "leverandoerfaktura" });
  } finally { rmSync(root, { recursive: true, force: true }); }
});
