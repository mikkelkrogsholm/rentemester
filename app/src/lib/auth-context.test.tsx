import { beforeEach, expect, mock, test } from "bun:test";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { stubGlobal } from "../test/globals";

const user = { id: "synthetic-reader", email: "reader@example.invalid", emailVerified: true, twoFactorEnabled: true };
const context = { ok: true, user, workspaceRole: "member", companies: [] };
let sessionStatus = 200;
let sessionUser = user;
const defaultSession = () => sessionStatus === 200
  ? { data: { user: sessionUser, session: { id: `session:${sessionUser.id}` } }, error: null }
  : { data: null, error: { status: sessionStatus, message: "Temporarily unavailable" } };
let sessionLoader = async () => defaultSession();
mock.module("./auth-client", () => ({ authClient: { getSession: () => sessionLoader() } }));
const { AuthProvider, useAuth } = await import("./auth-context");
beforeEach(() => { sessionStatus = 200; sessionUser = user; sessionLoader = async () => defaultSession(); });
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

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

test("logout invalidates an already authorized workspace context reply", async () => {
  const pending = deferred<Response>();
  let reads = 0;
  stubGlobal("fetch", () => { reads++; return pending.promise; });
  const { result } = renderHook(useAuth, { wrapper });
  await waitFor(() => expect(reads).toBe(1));
  act(() => result.current.clear());
  await act(async () => { pending.resolve(response(context)); });
  expect(result.current.session).toBeNull();
  expect(result.current.currentSessionId).toBeNull();
  expect(result.current.context).toBeNull();
  expect(result.current.loading).toBe(false);
});

test("logout invalidates a pending session reply before it can load workspace grants", async () => {
  const pending = deferred<ReturnType<typeof defaultSession>>();
  let reads = 0;
  sessionLoader = () => pending.promise;
  stubGlobal("fetch", async () => { reads++; return response(context); });
  const { result } = renderHook(useAuth, { wrapper });
  act(() => result.current.clear());
  await act(async () => { pending.resolve(defaultSession()); });
  expect(result.current.session).toBeNull();
  expect(result.current.currentSessionId).toBeNull();
  expect(result.current.context).toBeNull();
  expect(result.current.loading).toBe(false);
  expect(reads).toBe(0);
});

test.each([200, 401])("old context HTTP %s cannot overwrite or log out a newer identity", async (status) => {
  const pending = deferred<Response>();
  const nextUser = { ...user, id: "synthetic-next-reader", email: "next@example.invalid" };
  let reads = 0;
  stubGlobal("fetch", async () => ++reads === 1 ? pending.promise : response({ ...context, user: nextUser }));
  const { result } = renderHook(useAuth, { wrapper });
  await waitFor(() => expect(reads).toBe(1));
  sessionUser = nextUser;
  await act(() => result.current.refresh());
  await act(async () => { pending.resolve(response(status === 200 ? context : { ok: false }, status)); });
  expect(result.current.session?.id).toBe(nextUser.id);
  expect(result.current.context?.user.id).toBe(nextUser.id);
  expect(result.current.currentSessionId).toBe(`session:${nextUser.id}`);
  expect(result.current.loading).toBe(false);
});

test("old completion cannot finish a newer pending refresh", async () => {
  const first = deferred<Response>();
  const second = deferred<Response>();
  let reads = 0;
  stubGlobal("fetch", () => ++reads === 1 ? first.promise : second.promise);
  const { result } = renderHook(useAuth, { wrapper });
  await waitFor(() => expect(reads).toBe(1));
  let refresh!: Promise<void>;
  act(() => { refresh = result.current.refresh(); });
  await waitFor(() => expect(reads).toBe(2));
  await act(async () => { first.resolve(response(context)); });
  expect(result.current.loading).toBe(true);
  expect(result.current.context).toBeNull();
  await act(async () => { second.resolve(response(context)); await refresh; });
  expect(result.current.loading).toBe(false);
  expect(result.current.context?.user.id).toBe(user.id);
});

test("session and workspace identity must agree", async () => {
  stubGlobal("fetch", async () => response({ ...context, user: { ...user, id: "synthetic-other-user" } }));
  const { result } = renderHook(useAuth, { wrapper });
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.context).toBeNull();
  expect(result.current.bootstrapError).toBeTruthy();
});
