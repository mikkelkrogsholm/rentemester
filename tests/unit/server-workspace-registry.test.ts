import { describe, expect, test } from "bun:test";
import { rmSync } from "node:fs";
import { createParty, linkPartyRole } from "../../src/core/party-registry";
import { ingestCorporateRecord, linkCorporateRecord } from "../../src/core/corporate-records";
import { activateWorkspaceUser, authorizeWorkspaceRoute, grantCompanyMembership } from "../../src/core/workspace-access";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { createWorkspaceServicePrincipal, revokeWorkspaceServiceCredential, rotateWorkspaceServiceCredential } from "../../src/core/workspace-service-principals";
import { proposeCompanyKnowledge, reviewCompanyKnowledge } from "../../src/core/company-knowledge";
import { createBetterAuthRequestProvider, openWorkspaceBetterAuth, WORKSPACE_SERVICE_PRINCIPAL_HEADER } from "../../src/server/better-auth";
import { config, get, makeWorkspace } from "./server-api/_shared";
import { applyOwnershipSnapshot, proposeOwnershipSnapshot, reviewOwnershipSnapshot } from "../../src/core/ownership-graph";

const SECRET = "I0UjL6i0-ScgvjfIgzMKJxPQyDpPXwg2mMKdLW3Y3WQ";
const ORIGIN = "http://127.0.0.1:4319";
const at = "2026-08-30T10:00:00.000Z";

function party(db: ReturnType<typeof openWorkspaceControlDb>, partyId: string, name: string, companySlug: string) {
  createParty(db, { partyId, kind: "organization", name, source: "synthetic-test", observedAt: at, reviewAssertion: "synthetic evidence", actor: "user:owner" });
  linkPartyRole(db, { partyId, companySlug, role: "vendor", actor: "user:owner", observedAt: at });
}

function record(db: ReturnType<typeof openWorkspaceControlDb>, recordId: string, companySlug: string) {
  return ingestCorporateRecord(db, { recordId, type: "articles", bytes: new TextEncoder().encode(recordId), filename: `${recordId}.pdf`, source: "synthetic-test", receivedAt: at, uploader: "synthetic-user", actor: "user:owner", links: [{ type: "company", id: companySlug }] });
}

describe("workspace registry HTTP access projection", () => {
  test("enforces ownership roles for every endpoint, survives rotation and revokes immediately", async () => {
    const workspace = makeWorkspace("ownership-http", ["Allowed ApS", "Hidden ApS"]);
    const runtime = openWorkspaceBetterAuth(workspace, { secret: SECRET, trustedOrigins: [ORIGIN, "http://localhost"], baseURL: ORIGIN });
    const db = openWorkspaceControlDb(workspace);
    let verifierRuntime: ReturnType<typeof openWorkspaceBetterAuth> | undefined;
    try {
      const owner = { kind: "local_operator" as const, id: "synthetic-owner" };
      const initial = proposeOwnershipSnapshot(db, { snapshotId: "edge-existing", source: "synthetic", observedAt: at, facts: [{ owner: { kind: "company", companySlug: "allowed-aps" }, ownedCompanySlug: "hidden-aps", validFrom: "2026-01-01", validToExclusive: "2027-01-01", economicBasisPoints: 10000, controlType: "equity", jurisdiction: "DK", evidenceRefs: ["synthetic"] }], actor: "user:owner", principal: owner });
      reviewOwnershipSnapshot(db, { snapshotId: initial.snapshotId, decision: "approved", actor: "user:review", principal: { kind: "local_operator", id: "synthetic-reviewer" } });
      applyOwnershipSnapshot(db, { snapshotId: initial.snapshotId, snapshotHash: initial.snapshotHash, diffHash: initial.diffHash, actor: "user:review", principal: owner, authorized: true });
      const service = await createWorkspaceServicePrincipal(db, runtime.auth, { displayName: "ownership test", actor: "user:owner" });
      activateWorkspaceUser(db, { userId: service.serviceAccountId, workspaceRole: "member", actor: "user:owner" });
      grantCompanyMembership(db, workspace, { userId: service.serviceAccountId, companySlug: "allowed-aps", role: "reader", actor: "user:owner" });
      const hostedAuth = { secret: SECRET, secrets: [{ version: 1, value: SECRET }], baseURL: ORIGIN, trustedOrigins: [ORIGIN], authEmail: { provider: "http-json-v1" as const, url: "https://mailer.example.test/send", bearerToken: "synthetic-mail-token", from: "auth@example.test" }, rateLimitIpHeader: "x-real-ip" as const };
      const hosted = config({ workspaceRoot: workspace, deploymentProfile: "hosted", hostedBetterAuth: hostedAuth, betterAuthProvider: createBetterAuthRequestProvider(runtime.auth) });
      const headers = { [WORKSPACE_SERVICE_PRINCIPAL_HEADER]: service.secret, "content-type": "application/json", origin: ORIGIN };
      const filtered = await get(hosted, "/api/companies/allowed-aps/ownership?asOf=2026-02-01", { headers });
      expect(filtered.status).toBe(200);
      expect(filtered.body).toMatchObject({ ok: true, partial: true, facts: [] });
      expect(JSON.stringify(filtered.body)).not.toContain("hidden-aps");
      expect((await get(hosted, "/api/companies/allowed-aps/ownership/propose", { method: "POST", headers, body: JSON.stringify({ confirm: true }) })).status).toBe(401);
      expect((await get(hosted, "/api/companies/allowed-aps/ownership?asOf=2026-02-01", { headers: { actor: "user:owner" } })).status).toBe(401);
      grantCompanyMembership(db, workspace, { userId: service.serviceAccountId, companySlug: "allowed-aps", role: "reviewer", actor: "user:owner" });
      const body = { confirm: true, snapshotId: "edge-new", source: "synthetic", observedAt: at, facts: [{ owner: { kind: "company", companySlug: "allowed-aps" }, ownedCompanySlug: "hidden-aps", validFrom: "2027-01-01", economicBasisPoints: 10000, controlType: "equity", jurisdiction: "DK", evidenceRefs: ["new"] }] };
      expect((await get(hosted, "/api/companies/allowed-aps/ownership/propose", { method: "POST", headers, body: JSON.stringify(body) })).status).toBe(401);
      grantCompanyMembership(db, workspace, { userId: service.serviceAccountId, companySlug: "hidden-aps", role: "reviewer", actor: "user:owner" });
      expect(authorizeWorkspaceRoute(db, workspace, { userId: service.serviceAccountId, companySlug: "allowed-aps", permission: "company.ownership.manage" }).allowed).toBe(true);
      expect(authorizeWorkspaceRoute(db, workspace, { userId: service.serviceAccountId, companySlug: "hidden-aps", permission: "company.ownership.manage" }).allowed).toBe(true);
      // The HTTP adapter preserves the core confirmation boundary even after
      // every endpoint permission has been granted.
      expect((await get(hosted, "/api/companies/allowed-aps/ownership/propose", { method: "POST", headers, body: JSON.stringify({ ...body, confirm: false }) })).status).toBe(400);
      expect((await get(hosted, "/api/companies/allowed-aps/ownership/propose", { method: "POST", headers, body: JSON.stringify(body) })).status).toBe(200);
      verifierRuntime = openWorkspaceBetterAuth(workspace, { secret: SECRET, trustedOrigins: [ORIGIN, "http://localhost"], baseURL: ORIGIN });
      const verifier = createBetterAuthRequestProvider(verifierRuntime.auth);
      const continuedHosted = config({ workspaceRoot: workspace, deploymentProfile: "hosted", hostedBetterAuth: hostedAuth, betterAuthProvider: verifier });
      const rotated = await rotateWorkspaceServiceCredential(db, runtime.auth, { serviceAccountId: service.serviceAccountId, credentialId: service.credentialId, actor: "user:owner" });
      expect((await get(hosted, "/api/companies/allowed-aps/ownership?asOf=2026-02-01", { headers })).status).toBe(401);
      const rotatedHeaders = { ...headers, [WORKSPACE_SERVICE_PRINCIPAL_HEADER]: rotated.secret };
      expect((await verifier.verifyServicePrincipal!(new Request(`${ORIGIN}/api`, { headers: rotatedHeaders }))).state).toBe("valid");
      const continued = await get(continuedHosted, "/api/companies/allowed-aps/ownership?asOf=2026-02-01", { headers: rotatedHeaders });
      expect(continued).toEqual({ status: 200, body: expect.any(Object) });
      await revokeWorkspaceServiceCredential(db, runtime.auth, { serviceAccountId: service.serviceAccountId, credentialId: rotated.credentialId, actor: "user:owner" });
      expect((await get(continuedHosted, "/api/companies/allowed-aps/ownership?asOf=2026-02-01", { headers: rotatedHeaders })).status).toBe(401);
    } finally {
      verifierRuntime?.close();
      db.close();
      runtime.close();
      rmSync(workspace, { recursive: true, force: true });
    }
  });
  test("uses the live service-principal membership before pagination and requires every corporate company scope", async () => {
    const workspace = makeWorkspace("workspace-registry-http", ["Allowed ApS", "Hidden ApS"]);
    const runtime = openWorkspaceBetterAuth(workspace, { secret: SECRET, trustedOrigins: [ORIGIN], baseURL: ORIGIN });
    const db = openWorkspaceControlDb(workspace);
    try {
      const service = await createWorkspaceServicePrincipal(db, runtime.auth, { displayName: "Synthetic registry reader", actor: "user:owner" });
      activateWorkspaceUser(db, { userId: service.serviceAccountId, workspaceRole: "member", actor: "user:owner" });
      grantCompanyMembership(db, workspace, { userId: service.serviceAccountId, companySlug: "allowed-aps", role: "reader", actor: "user:owner" });

      // The hidden party sorts first. A response containing the visible party
      // at limit=1 proves that authorization filtering happens before paging.
      party(db, "party-a-hidden", "Hidden first", "hidden-aps");
      party(db, "party-z-visible", "Visible after hidden", "allowed-aps");
      record(db, "record-allowed", "allowed-aps");
      record(db, "record-shared", "allowed-aps");
      linkCorporateRecord(db, { recordId: "record-shared", type: "company", id: "hidden-aps", actor: "user:owner", at });
      const principal={kind:"service_account" as const,id:service.serviceAccountId}; const visibleKnowledge=proposeCompanyKnowledge(db,{companySlug:"allowed-aps",predicate:"markets",value:["DK"],source:{kind:"user",ref:"synthetic-owner"},validFrom:at,actor:"user:owner",principal});reviewCompanyKnowledge(db,{assertionId:visibleKnowledge.assertionId,decision:"approved",actor:"user:review",principal});

      const hosted = config({ workspaceRoot: workspace, deploymentProfile: "hosted", betterAuthProvider: createBetterAuthRequestProvider(runtime.auth) });
      const headers = { [WORKSPACE_SERVICE_PRINCIPAL_HEADER]: service.secret };
      const parties = await get(hosted, "/api/companies/allowed-aps/workspace-parties?limit=1", { headers });
      expect(parties.status).toBe(200);
      expect(parties.body).toMatchObject({ ok: true, count: 1, rows: [{ partyId: "party-z-visible", name: "Visible after hidden" }] });
      expect(JSON.stringify(parties.body)).not.toContain("party-a-hidden");

      const records = await get(hosted, "/api/companies/allowed-aps/corporate-records", { headers });
      expect(records.status).toBe(200);
      expect(records.body).toMatchObject({ ok: true, count: 1, rows: [{ recordId: "record-allowed" }] });
      expect(JSON.stringify(records.body)).not.toContain("record-shared");
      const deniedInspection = await get(hosted, "/api/companies/allowed-aps/corporate-records/record-shared", { headers });
      expect(deniedInspection.status).toBe(404);
      expect(JSON.stringify(deniedInspection.body)).not.toContain("record-shared");
      const knowledge=await get(hosted,"/api/companies/allowed-aps/knowledge?asOf=2026-09-01",{headers});expect(knowledge.status).toBe(200);expect(knowledge.body).toMatchObject({ok:true,context:{assertions:[{predicate:"markets",source:{kind:"user"}}]}});
      expect((await get(hosted,"/api/companies/hidden-aps/knowledge?asOf=2026-09-01",{headers})).status).toBe(401);
    } finally {
      db.close();
      runtime.close();
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
