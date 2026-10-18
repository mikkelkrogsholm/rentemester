/** Actual HTTP current-password management, strict admission and known-commit-only rotation. */
import assert from "node:assert/strict";
import { Database } from "bun:sqlite";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { serializeSignedCookie } from "../../node_modules/better-call/dist/cookies.mjs";
import { verifyPassword } from "../../node_modules/better-auth/dist/crypto/password.mjs";
export async function verifyHttpPasswordChange(c: any) {
  const { fixture, start, key, now, note } = c,
    currentPassword = "Synthetic password 123!",
    newPassword = "HTTP changed synthetic password 456!",
    token = "http-change-private-session";
  const cookie = (
      await serializeSignedCookie("better-auth.session_token", token, key)
    ).split(";")[0]!,
    sensitive = [cookie, token, currentPassword, newPassword, key],
    observations: string[] = [];
  const record = (id: string) => {
    observations.push(id);
    note(`password-change/${id}`);
  };
  function db<T>(f: any, fn: (d: Database) => T): T {
    const d = new Database(f.control);
    try {
      return fn(d);
    } finally {
      d.close();
    }
  }
  const q = (f: any, s: string) => db(f, (d) => d.query(s).all() as any[]);
  const image = (f: any) =>
    JSON.stringify(
      [
        "account",
        "session",
        "user",
        "twoFactor",
        "verification",
        "workspace_audit",
        "rm_workspace_auth_state_events",
      ].map((t) => q(f, `SELECT * FROM ${t} ORDER BY rowid`)),
    );
  function fresh(id: string, mode = "DELETE", age = 1000) {
    const f = fixture(`http-password-change-${id}`, mode);
    db(f, (d) => {
      d.exec(
        "INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('foreign','Foreign','foreign@example.invalid',1,'2026-01-01','2026-01-01',1)",
      );
      for (const [id, user, bearer] of [
        ["current", "u", token],
        ["other", "u", "private-other-token"],
        ["foreign", "foreign", "private-foreign-token"],
      ])
        d.query(
          "INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt) VALUES(?,?,?,?,?,?)",
        ).run(
          id,
          user,
          bearer,
          new Date(now - age).toISOString(),
          new Date(now - age).toISOString(),
          new Date(now + 3600000).toISOString(),
        );
    });
    return {
      ...f,
      outcomeLog: join(dirname(f.control), "change-outcomes.jsonl"),
    };
  }
  const outcomes = (f: any) =>
    existsSync(f.outcomeLog)
      ? readFileSync(f.outcomeLog, "utf8")
          .trim()
          .split("\n")
          .filter(Boolean)
          .map((l) => JSON.parse(l))
      : [];
  const post = (
    s: any,
    body: any = { currentPassword, newPassword, revokeOtherSessions: true },
    headers: any = {},
  ) =>
    fetch(`${s.origin}/api/auth/change-password`, {
      method: "POST",
      headers: {
        origin: s.origin,
        "content-type": "application/json",
        cookie,
        ...headers,
      },
      body: typeof body === "string" ? body : JSON.stringify(body),
      redirect: "manual",
    });
  for (const mode of ["WAL", "DELETE"])
    for (const flag of [true, false, undefined]) {
      const f = fresh(`${mode}-${flag}`, mode),
        s = await start(f, { passwordChange: true });
      try {
        const old = q(f, "SELECT * FROM session ORDER BY id"),
          body: any = { currentPassword, newPassword };
        if (flag !== undefined) body.revokeOtherSessions = flag;
        const r = await post(s, body);
        assert.equal(r.status, 200);
        assert.equal(r.headers.get("cache-control"), "no-store");
        const v = (await r.json()) as any;
        assert.equal(v.status, true);
        assert.equal(v.token, null);
        assert.equal(v.user.id, "u");
        assert.deepEqual(Object.keys(v).sort(), ["status", "token", "user"]);
        assert(!JSON.stringify(v).includes(token));
        const hash = q(f, "SELECT password FROM account WHERE id='a'")[0]
          .password;
        sensitive.push(hash);
        assert(await verifyPassword({ hash, password: newPassword }));
        assert(!(await verifyPassword({ hash, password: currentPassword })));
        const cookies = r.headers.getSetCookie();
        sensitive.push(...cookies);
        if (flag) {
          assert.equal(cookies.length, 5);
          assert.equal(
            cookies.filter((v) => v.includes("Max-Age=0")).length,
            4,
          );
          const sessionCookie = cookies
            .find(
              (v) =>
                v.startsWith("better-auth.session_token=") &&
                !v.includes("Max-Age=0"),
            )!
            .split(";")[0]!;
          assert(sessionCookie);
          sensitive.push(sessionCookie);
          const session = await fetch(`${s.origin}/api/auth/get-session`, {
            headers: { cookie: sessionCookie },
          });
          assert.equal(session.status, 200);
          assert.equal(((await session.json()) as any).user.id, "u");
          assert.equal(
            q(f, "SELECT count(*) n FROM session WHERE userId='u'")[0].n,
            1,
          );
        } else {
          assert.deepEqual(cookies, []);
          assert.deepEqual(q(f, "SELECT * FROM session ORDER BY id"), old);
        }
        assert.equal(
          q(f, "SELECT count(*) n FROM session WHERE userId='foreign'")[0].n,
          1,
        );
        assert.equal(
          q(
            f,
            "SELECT count(*) n FROM workspace_audit WHERE entity_type='native_password_change_v1'",
          )[0].n,
          1,
        );
        record(
          `${mode}-${flag === undefined ? "omitted" : flag}-source-KDF-and-token-free-receipt-with-exact-optional-rotation`,
        );
      } finally {
        await s.stop();
      }
    }
  const invalid = [
    {
      id: "null-flag",
      body: { currentPassword, newPassword, revokeOtherSessions: null },
    },
    {
      id: "string-flag",
      body: { currentPassword, newPassword, revokeOtherSessions: "true" },
    },
    {
      id: "duplicate-flag",
      body:
        JSON.stringify({ currentPassword, newPassword }).slice(0, -1) +
        ',"revokeOtherSessions":true,"revokeOtherSessions":false}',
    },
    { id: "array", body: [currentPassword, newPassword, true] },
    {
      id: "subject",
      body: { currentPassword, newPassword, userId: "foreign" },
    },
    { id: "missing-current", body: { newPassword } },
    { id: "new-short", body: { currentPassword, newPassword: "x".repeat(11) } },
    { id: "new-long", body: { currentPassword, newPassword: "😀".repeat(65) } },
    {
      id: "current-bound",
      body: { currentPassword: "x".repeat(4097), newPassword },
    },
    {
      id: "bare-cookie",
      headers: { cookie: `better-auth.session_token; ${cookie}` },
      status: 401,
    },
    {
      id: "service",
      headers: { authorization: "Bearer private-service" },
      status: 401,
    },
    {
      id: "origin",
      headers: { origin: "https://foreign.invalid" },
      status: 403,
    },
    { id: "content", headers: { "content-type": "text/plain" }, status: 415 },
    { id: "body-bound", body: " ".repeat(8193), status: 413 },
  ] as { id: string; body?: any; headers?: any; status?: number }[];
  for (const t of invalid) {
    const f = fresh(t.id),
      s = await start(f, { passwordChange: true });
    try {
      const before = image(f),
        r = await post(
          s,
          t.body ?? { currentPassword, newPassword },
          t.headers,
        );
      assert.equal(r.status, t.status ?? 400, t.id);
      assert.equal(r.headers.get("set-cookie"), null);
      assert.equal(image(f), before);
      assert.deepEqual(outcomes(f), []);
      record(`${t.id}-strict-admission-before-native-dispatch`);
    } finally {
      await s.stop();
    }
  }
  for (const [id, mutate, pass, age, status] of [
    ["wrong", "", "Incorrect private password", 1000, 401],
    ["stale", "", currentPassword, 600000, 401],
    [
      "disabled",
      "INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')",
      currentPassword,
      1000,
      401,
    ],
    [
      "bad-result",
      "UPDATE user SET image=char(10) WHERE id='u'",
      currentPassword,
      1000,
      503,
    ],
  ] as const) {
    const f = fresh(id, "WAL", age);
    if (mutate) db(f, (d) => d.exec(mutate));
    const s = await start(f, { passwordChange: true });
    try {
      const before = image(f),
        r = await post(s, {
          currentPassword: pass,
          newPassword,
          revokeOtherSessions: true,
        });
      assert.equal(r.status, status);
      assert.equal(r.headers.get("set-cookie"), null);
      assert(!(await r.text()).includes('"user"'));
      assert.equal(image(f), before);
      record(`${id}-no-update-rotation-audit-or-receipt`);
    } finally {
      await s.stop();
    }
  }
  for (const rotate of [false, true]) {
    const f = fresh(`unknown-${rotate}`),
      s = await start(f, { passwordChange: true }),
      blocker = new Database(f.control);
    try {
      blocker.exec("BEGIN;SELECT * FROM session");
      const before = image(f),
        r = await post(s, {
          currentPassword,
          newPassword,
          revokeOtherSessions: rotate,
        });
      assert.equal(r.status, 503);
      assert.equal(r.headers.get("set-cookie"), null);
      assert.equal(((await r.json()) as any).code, "AUTH_UNCONFIRMED");
      assert.equal(outcomes(f).at(-1)?.stage, "OutcomeUnknown");
      assert.equal(image(f), before);
      record(
        `DELETE-${rotate}-actual-unknown-commit-no-user-cookie-or-success`,
      );
    } finally {
      blocker.exec("ROLLBACK");
      blocker.close();
      await s.stop();
    }
  }
  const rate = fresh("rate"),
    s = await start(rate, { passwordChange: true });
  try {
    for (let i = 0; i < 10; i++)
      assert.equal(
        (
          await fetch(`${s.origin}/api/auth/change-password`, {
            method: "HEAD",
          })
        ).status,
        405,
      );
    for (let i = 0; i < 3; i++)
      assert.equal(
        (
          await post(
            s,
            {
              currentPassword: "wrong-private-password",
              newPassword,
              revokeOtherSessions: !!(i % 2),
            },
            { "x-forwarded-for": `203.0.113.${i}` },
          )
        ).status,
        401,
      );
    const before = image(rate),
      r = await post(s);
    assert.equal(r.status, 429);
    assert.equal(image(rate), before);
    assert.equal(outcomes(rate).length, 3);
    record(
      "HEAD-no-rate-charge-direct-peer-KDF-budget-ignores-forwarded-and-rotation-flag",
    );
  } finally {
    await s.stop();
  }
  const unsupported = fresh("unsupported"),
    u = await start(unsupported);
  try {
    const before = image(unsupported),
      r = await post(u);
    assert.equal(r.status, 404);
    assert.equal(image(unsupported), before);
    record("missing-trusted-handler-no-legacy-fallback");
  } finally {
    await u.stop();
  }
  assert(!sensitive.some((v) => JSON.stringify(observations).includes(v)));
}
