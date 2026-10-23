import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { createProtocolManifest, protocolAdmission, protocolPathMatches } from "../../src/server/protocol";
import type { ServerConfig } from "../../src/server/config";

const manifest = createProtocolManifest([{ method: "GET", pattern: "/api/protocol" }, { method: "POST", pattern: "/api/companies" }]);
function headers(overrides: Record<string, string> = {}) {
  return { "x-rentemester-protocol": "1", "x-rentemester-manifest": manifest.manifestId, "x-rentemester-wire-profile": "legacy-react-v1", ...overrides };
}
const url = "http://localhost/api/companies";

describe("HTTP protocol major 1", () => {
  test("public metadata does not authenticate, open data or dispatch writes", async () => {
    const { handleRequest, ROUTE_CATALOG } = await import("../../src/server/router");
    let calls = 0;
    const config = { authenticateRequest() { calls++; throw new Error("must not run"); } } as unknown as ServerConfig;
    const response = await handleRequest(new Request("http://localhost/api/protocol"), config);
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual(createProtocolManifest(ROUTE_CATALOG));
    expect(calls).toBe(0);
    expect(manifest.wireProfiles).toEqual(["legacy-react-v1"]);
    expect(JSON.stringify(manifest).length).toBeLessThan(1024 * 1024);
    expect(manifest.manifestId).toMatch(/^[a-f0-9]{64}$/);
    expect(manifest.operations.some(op => op.path.includes("/original/"))).toBe(false);
  });

  test("incompatible writes have zero handler/auth dispatch; admitted writes still need auth", async () => {
    const { handleRequest, ROUTE_CATALOG } = await import("../../src/server/router");
    const installed = createProtocolManifest(ROUTE_CATALOG);
    const installedHeaders = () => headers({ "x-rentemester-manifest": installed.manifestId });
    let calls = 0;
    const config = { authenticateRequest() { calls++; throw new Error("authentication required"); } } as unknown as ServerConfig;
    for (const h of [
      { "x-rentemester-protocol": "1" },
      { ...installedHeaders(), "x-rentemester-protocol": "2" },
      { ...installedHeaders(), "x-rentemester-manifest": "0".repeat(64) },
      { ...installedHeaders(), "x-rentemester-wire-profile": "rust-http-v1" },
    ]) {
      const response = await handleRequest(new Request(url, { method: "POST", headers: h }), config);
      expect(response.status).toBe(409);
      expect((await response.json()).code).toBe("protocol_incompatible");
    }
    expect(calls).toBe(0);
    await handleRequest(new Request(url, { method: "POST", headers: installedHeaders() }), config);
    expect(calls).toBe(1);
    await handleRequest(new Request(url, { method: "POST" }), config);
    expect(calls).toBe(2);
  });

  test("exact segments and no unsupported dispatch", () => {
    expect(protocolPathMatches("/api/x/{id}", "/api/x/one")).toBe(true);
    for (const path of ["/api/x/", "/api/x/one/two", "/api/x/one?q=1", "/api/xx/one"]) expect(protocolPathMatches("/api/x/{id}", path)).toBe(false);
    expect(protocolAdmission(new Request("http://localhost/api/not-installed", { method: "POST", headers: headers() }), manifest)?.status).toBe(409);
    expect(() => createProtocolManifest([{ method: "POST", pattern: "/api/x" }, { method: "POST", pattern: "/api/x" }])).toThrow("Duplicate");
  });

  test("stable SHA256 identity excludes manifestId and uses sorted compact JSON", () => {
    const m = createProtocolManifest([{ method: "GET", pattern: "/api/test" }]);
    const identity = '{"format":"rentemester-http-protocol/1","operations":[{"capability":"GET /api/test","method":"GET","path":"/api/test"}],"protocolMajor":1,"wireProfiles":["legacy-react-v1"]}';
    expect(m.manifestId).toBe(createHash("sha256").update(identity).digest("hex"));
  });
});
