import { expect, test, vi } from "bun:test";
import { admittedAuthFetch, authClient } from "./auth-client";
import { stubGlobal } from "../test/globals";

test("incompatible protocol prevents Better Auth credential POST", async () => {
  const calls: string[] = [];
  stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => { calls.push(String(input)); return Response.json({ protocolMajor: 99 }); }));
  await expect(authClient.signIn.email({ email: "owner@example.test", password: "never-send" }, { retry: 0 })).rejects.toThrow();
  expect(calls).toEqual(["/api/protocol"]);
});
test("cross-origin credential requests are refused before preflight", async () => {
  const fetch = vi.fn(); stubGlobal("fetch", fetch);
  await expect(admittedAuthFetch("https://foreign.example/api/auth/sign-in/email", { method: "POST", body: "secret" })).rejects.toThrow();
  expect(fetch).not.toHaveBeenCalled();
});
test("admitted auth sends only same-origin without redirects", async () => {
  const path = "/api/auth/sign-in/email"; const calls: RequestInit[] = [];
  stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    calls.push(init!);
    if (String(input) === "/api/protocol") return Response.json({ format: "rentemester-http-protocol/1", protocolMajor: 1, manifestId: "a".repeat(64), wireProfiles: ["legacy-react-v1"], operations: [{ method: "POST", path, capability: `POST ${path}` }] });
    return Response.json({ status: true });
  }));
  await admittedAuthFetch(`${window.location.origin}${path}`, { method: "POST", body: "secret" });
  expect(calls[1]?.credentials).toBe("same-origin"); expect(calls[1]?.redirect).toBe("error");
  expect(new Headers(calls[1]?.headers).get("x-rentemester-protocol")).toBe("1");
});
