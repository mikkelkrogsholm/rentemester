import { expect, test } from "bun:test";
import { LoginAttempts } from "./login-attempt";
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
for (const stage of ["signin", "totp", "backup", "enrollment begin", "enrollment confirm"]) {
  test(`cancelled ${stage} settles before logout and readback; remount cannot start`, async () => {
    const reply = deferred<void>(); const logout = deferred<{ error?: unknown }>(); const calls: string[] = [];
    const flow = new LoginAttempts(() => { calls.push("logout"); return logout.promise; }, async () => { calls.push("read"); return { data: null }; });
    const attempt = flow.start(" Owner@Example.test ");
    const effect = flow.run(attempt, () => reply.promise);
    const cleanup = flow.cancel(attempt);
    expect(flow.current(attempt)).toBe(false);
    expect(() => flow.start("next@example.test")).toThrow(); expect(calls).toEqual([]);
    reply.resolve(); await effect; await Promise.resolve();
    expect(calls).toEqual(["logout"]); expect(() => flow.start("next@example.test")).toThrow();
    logout.resolve({}); await cleanup;
    expect(calls).toEqual(["logout", "read"]); expect(flow.start("next@example.test").email).toBe("next@example.test");
  });
}
test("cleanup failure blocks credentials until explicit safe recovery", async () => {
  let failed = true;
  const flow = new LoginAttempts(async () => ({ error: failed ? {} : undefined }), async () => ({ data: null }));
  flow.start("owner@example.test"); await flow.cancel();
  expect(flow.snapshot()).toBe("blocked"); expect(() => flow.start("next@example.test")).toThrow();
  failed = false; await flow.cancel(); expect(flow.snapshot()).toBe("idle");
});
test("wrong account, changed stable ID and incomplete MFA are refused", () => {
  const flow = new LoginAttempts(async () => ({}), async () => ({ data: null }));
  const attempt = flow.start(" Owner@Example.test "); flow.ready(attempt);
  const user = { id: "stable", email: "owner@example.test", emailVerified: true, twoFactorEnabled: true };
  expect(() => flow.assertSession({ ...user, email: "other@example.test" })).toThrow();
  flow.assertSession(user);
  expect(() => flow.assertSession({ ...user, id: "different" })).toThrow();
  expect(() => flow.assertSession({ ...user, twoFactorEnabled: false })).toThrow();
});
test("remaining cookie after signout is blocked, not adopted", async () => {
  const flow = new LoginAttempts(async () => ({}), async () => ({ data: { user: { id: "late-cookie" } } }));
  flow.start("owner@example.test"); await flow.cancel(); expect(flow.snapshot()).toBe("blocked");
});

for (const data of [undefined, {}, { user: null }]) test(`malformed no-session readback ${JSON.stringify(data)} keeps credentials fenced`, async () => {
  const flow = new LoginAttempts(async () => ({}), async () => ({ data }));
  flow.start("owner@example.test"); await flow.cancel();
  expect(flow.snapshot()).toBe("blocked");
  expect(() => flow.start("other@example.test")).toThrow();
});
