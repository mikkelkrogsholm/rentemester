import { describe, expect, test } from "bun:test";
import { dispatchGroupWorkspaceRoute } from "../../src/server/router/group-workspace-dispatch";

function handlers(calls: Array<{ name: string; args: unknown[] }>) {
  return new Proxy({}, {
    get: (_, name: string) => (...args: unknown[]) => {
      calls.push({ name, args });
      return new Response(name);
    },
  }) as Record<string, (...args: any[]) => Response>;
}

describe("group/workspace route dispatch", () => {
  test("matches exact group and workspace paths without owning authorization", async () => {
    const calls: Array<{ name: string; args: unknown[] }> = [];
    const group = await dispatchGroupWorkspaceRoute(
      "/api/group-overview", "GET", new URL("http://test/api/group-overview?asOf=2026-01-01"), new Request("http://test"), handlers(calls),
    );
    expect(await group?.text()).toBe("groupOverview");
    expect(calls).toEqual([{ name: "groupOverview", args: ["2026-01-01"] }]);

    const inbox = await dispatchGroupWorkspaceRoute(
      "/api/companies/acme%20aps/workspace-inbox/doc%2F1/complete", "POST", new URL("http://test"), new Request("http://test"), handlers(calls),
    );
    expect(await inbox?.text()).toBe("workspaceInboxComplete");
    expect(calls.at(-1)).toEqual({ name: "workspaceInboxComplete", args: ["acme aps", "doc/1"] });
    expect(await dispatchGroupWorkspaceRoute("/api/companies/acme/dashboard", "GET", new URL("http://test"), new Request("http://test"), handlers(calls))).toBeNull();
  });

  test("preserves method and query validation", async () => {
    const calls: Array<{ name: string; args: unknown[] }> = [];
    await expect(dispatchGroupWorkspaceRoute("/api/group-overview", "POST", new URL("http://test"), new Request("http://test"), handlers(calls))).rejects.toThrow("kun GET");
    await expect(dispatchGroupWorkspaceRoute("/api/group-overview", "GET", new URL("http://test"), new Request("http://test"), handlers(calls))).rejects.toThrow("exactly one asOf");
    expect(calls).toEqual([]);
  });
});
