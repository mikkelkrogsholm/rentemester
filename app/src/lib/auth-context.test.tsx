import { withProtocol } from "../test/protocol";
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
const { request, requestBlob } = await import("./api/_shared");
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
    stubAuthFetch( async () => {
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
  stubAuthFetch( async () => response(status === 200 ? context : { ok: false, code: status === 401 ? "unauthorized" : "busy" }, status));
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
  stubAuthFetch( () => { reads++; return pending.promise; });
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
  stubAuthFetch( async () => { reads++; return response(context); });
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
  stubAuthFetch( async () => ++reads === 1 ? pending.promise : response({ ...context, user: nextUser }));
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
  stubAuthFetch( () => ++reads === 1 ? first.promise : second.promise);
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

for (const binary of [false, true]) {
  test(`old ordinary ${binary ? "blob" : "JSON"} 401 cannot cancel a pending same-subject session refresh`, async () => {
    const oldReply = deferred<Response>();
    const newSession = deferred<ReturnType<typeof defaultSession>>();
    let ordinaryStarted = false;
    stubAuthFetch(async (input) => {
      if (String(input) === "/api/companies") {
        ordinaryStarted = true;
        return oldReply.promise;
      }
      return response(context);
    });
    const { result } = renderHook(useAuth, { wrapper });
    await waitFor(() => expect(result.current.context?.user.id).toBe(user.id));
    // No view cleanup or AbortSignal: this is an ordinary request still in flight.
    const oldRequest = (binary ? requestBlob : request)("/api/companies").catch(error => error);
    await waitFor(() => expect(ordinaryStarted).toBe(true));
    sessionLoader = () => newSession.promise;
    let refresh!: Promise<void>;
    act(() => { refresh = result.current.refresh({ expectedUserId: user.id }); });
    await act(async () => {
      oldReply.resolve(response({ ok: false, code: "unauthorized" }, 401));
      expect(await oldRequest).toMatchObject({ status: 401, code: "unauthorized" });
    });
    await act(async () => {
      newSession.resolve({ data: { user, session: { id: "rotated-same-subject" } }, error: null });
      await refresh;
    });
    expect(result.current.session?.id).toBe(user.id);
    expect(result.current.currentSessionId).toBe("rotated-same-subject");
    expect(result.current.context?.user.id).toBe(user.id);
    expect(result.current.bootstrapError).toBeNull();
    expect(result.current.loading).toBe(false);
  });

  test(`current ordinary ${binary ? "blob" : "JSON"} 401 still clears identity and grants`, async () => {
    stubAuthFetch(async (input) => String(input) === "/api/companies"
      ? response({ ok: false }, 401) : response(context));
    const { result } = renderHook(useAuth, { wrapper });
    await waitFor(() => expect(result.current.context?.user.id).toBe(user.id));
    await act(async () => {
      await expect((binary ? requestBlob : request)("/api/companies")).rejects.toMatchObject({ status: 401 });
    });
    expect(result.current.session).toBeNull();
    expect(result.current.currentSessionId).toBeNull();
    expect(result.current.context).toBeNull();
    expect(result.current.loading).toBe(false);
  });
}

test("untagged local cancellation fences a pending session reply", async () => {
  const pending = deferred<ReturnType<typeof defaultSession>>();
  sessionLoader = () => pending.promise;
  let reads = 0;
  stubAuthFetch(async () => { reads++; return response(context); });
  const { result } = renderHook(useAuth, { wrapper });
  act(() => { window.dispatchEvent(new Event("rentemester:auth-expired")); });
  await act(async () => { pending.resolve(defaultSession()); });
  expect(result.current.session).toBeNull();
  expect(result.current.currentSessionId).toBeNull();
  expect(result.current.context).toBeNull();
  expect(result.current.loading).toBe(false);
  expect(reads).toBe(0);
});

test("session and workspace identity must agree", async () => {
  stubAuthFetch( async () => response({ ...context, user: { ...user, id: "synthetic-other-user" } }));
  const { result } = renderHook(useAuth, { wrapper });
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.context).toBeNull();
  expect(result.current.bootstrapError).toBeTruthy();
});

function stubAuthFetch(handler: (input: RequestInfo | URL, init?: RequestInit) => Promise<any>) {
  stubGlobal("fetch", withProtocol(handler, [["GET", "/api/health"], ["GET", "/api/me"], ["GET", "/api/companies"], ["POST", "/api/auth/mfa-enrollment/begin"], ["POST", "/api/auth/mfa-enrollment/confirm"]]));
}
