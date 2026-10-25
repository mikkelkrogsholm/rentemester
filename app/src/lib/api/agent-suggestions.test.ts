import { describe, expect, test, vi } from "bun:test";
import { agentSuggestionsApi as api } from "./exceptions";
import { withProtocol } from "../../test/protocol";
import { stubGlobal } from "../../test/globals";

const routes = [
  ["GET", "/api/companies/{slug}/agent-suggestions"],
  ["POST", "/api/companies/{slug}/agent-suggestions/{id}/approve"],
  ["POST", "/api/companies/{slug}/agent-suggestions/{id}/reject"],
] as const;
function install(body: unknown) {
  const handler = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => Response.json(body));
  stubGlobal("fetch", withProtocol(handler, routes));
  return handler;
}
describe("Agent review wire", () => {
  for (const decision of ["approve", "reject"] as const) {
    test(`${decision} sends only trimmed note and validates correlated receipt`, async () => {
      const expected = decision === "approve" ? "approved" : "rejected";
      const handler = install({ ok: true, suggestion: { id: 101, decision: expected, resolved: true } });
      const method = decision === "approve" ? api.approveAgentSuggestion : api.rejectAgentSuggestion;
      expect(await method("synthetic", 101, "  review  ")).toEqual({ id: 101, decision: expected, resolved: true });
      expect(handler).toHaveBeenCalledTimes(1);
      expect(handler.mock.calls[0]?.[1]?.body).toBe('{"note":"review"}');
    });
  }
  for (const suggestion of [null, {}, { id: 102, decision: "approved", resolved: true }, { id: 101, decision: "rejected", resolved: true }, { id: 101, decision: "approved", resolved: false }]) {
    test(`unknown result remains internal: ${JSON.stringify(suggestion)}`, async () => {
      install({ ok: true, suggestion });
      await expect(api.approveAgentSuggestion("synthetic", 101)).rejects.toMatchObject({ code: "internal" });
    });
  }
  test("invalid ids and non-string notes never dispatch", async () => {
    const handler = install({});
    for (const id of [0, -1, 1.5, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
      await expect(api.approveAgentSuggestion("synthetic", id)).rejects.toMatchObject({ code: "bad_request" });
    }
    await expect(api.rejectAgentSuggestion("synthetic", 1, {} as string)).rejects.toMatchObject({ code: "bad_request" });
    expect(handler).not.toHaveBeenCalled();
  });
  test("empty note sends empty object; malformed JSON is unknown without retry", async () => {
    const handler = vi.fn(async (_url: RequestInfo | URL, _init?: RequestInit) => new Response("{"));
    stubGlobal("fetch", withProtocol(handler, routes));
    await expect(api.rejectAgentSuggestion("synthetic", 101, " ")).rejects.toMatchObject({ code: "internal" });
    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler.mock.calls[0]?.[1]?.body).toBe("{}");
  });
});
