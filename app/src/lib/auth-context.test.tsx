import { beforeEach, expect, mock, test } from "bun:test";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { stubGlobal } from "../test/globals";

const user = { id: "synthetic-reader", email: "reader@example.invalid", emailVerified: true, twoFactorEnabled: true };
const context = { ok: true, user, workspaceRole: "member", companies: [] };
let sessionStatus = 200;
mock.module("./auth-client", () => ({ authClient: { getSession: async () => sessionStatus === 200
  ? { data: { user, session: { id: "synthetic-session" } }, error: null }
  : { data: null, error: { status: sessionStatus, message: "Temporarily unavailable" } } } }));
const { AuthProvider, useAuth } = await import("./auth-context");
beforeEach(() => { sessionStatus = 200; });
const wrapper = ({ children }: { children: ReactNode }) => <AuthProvider hosted backend="rust">{children}</AuthProvider>;
function response(value: unknown, status = 200) {
  return new Response(JSON.stringify(value), { status, headers: { "content-type": "application/json" } });
}

for (const status of [429, 503]) {
  test(`temporary session HTTP ${status} blocks access explicitly and permits a read-only refresh`, async () => {
    sessionStatus = status;
    let contextReads = 0;
    stubGlobal("fetch", async () => {
      contextReads++;
      return response(context);
    });
    const { result } = renderHook(useAuth, { wrapper });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.bootstrapError).toBeTruthy();
    expect(result.current.context).toBeNull();
    expect(contextReads).toBe(0);
    sessionStatus = 200;
    await act(() => result.current.refresh());
    expect(result.current.bootstrapError).toBeNull();
    expect(result.current.session?.id).toBe(user.id);
    expect(result.current.context?.user.id).toBe(user.id);
  });
}

test("unavailable workspace context hides old grants, while definite unauthorized clears identity", async () => {
  let status = 200;
  stubGlobal("fetch", async () => response(status === 200 ? context : { ok: false, code: status === 401 ? "unauthorized" : "busy" }, status));
  const { result } = renderHook(useAuth, { wrapper });
  await waitFor(() => expect(result.current.context?.user.id).toBe(user.id));
  status = 503;
  await act(() => result.current.refresh());
  expect(result.current.session?.id).toBe(user.id);
  expect(result.current.context).toBeNull();
  expect(result.current.bootstrapError).toBeTruthy();
  status = 401;
  await act(() => result.current.refresh());
  expect(result.current.session).toBeNull();
  expect(result.current.context).toBeNull();
  expect(result.current.bootstrapError).toBeNull();
});
