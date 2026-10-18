/** Password-bound own-factor disclosure/replacement through real native HTTP, source crypto. */
import assert from "node:assert/strict";
import { Database } from "bun:sqlite";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { serializeSignedCookie } from "../../node_modules/better-call/dist/cookies.mjs";
import {
  symmetricEncrypt,
  symmetricDecrypt,
} from "../../node_modules/better-auth/dist/crypto/index.mjs";
export async function verifyHttpBackupManagement(c: any) {
  const { fixture, start, key, now, note } = c,
    password = "Synthetic password 123!",
    token = "http-backup-private-session";
  const cookie = (
    await serializeSignedCookie("better-auth.session_token", token, key)
  ).split(";")[0]!;
  const keyset = { currentVersion: 1, keys: new Map([[1, key]]) },
    observations: string[] = [],
    sensitive = [password, token, cookie, key];
  const record = (id: string) => {
    observations.push(id);
    note(`backup-management/${id}`);
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
        "session",
        "verification",
        "twoFactor",
        "user",
        "account",
        "workspace_audit",
        "rm_workspace_auth_state_events",
      ].map((t) => q(f, `SELECT * FROM ${t} ORDER BY rowid`)),
    );
  function fresh(id: string, mode = "DELETE", age = 1000) {
    const f = fixture(`backup-http-${id}`, mode);
    db(f, (d) =>
      d
        .query(
          "INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt) VALUES('management-current','u',?,?,?,?)",
        )
        .run(
          token,
          new Date(now - age).toISOString(),
          new Date(now - age).toISOString(),
          new Date(now + 3600000).toISOString(),
        ),
    );
    return {
      ...f,
      outcomeLog: join(dirname(f.control), "backup-outcomes.jsonl"),
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
  const pool = (f: any) =>
    q(f, "SELECT backupCodes FROM twoFactor WHERE userId='u'")[0].backupCodes;
  async function decrypt(f: any) {
    const codes = JSON.parse(
      await symmetricDecrypt({ key: keyset, data: pool(f) }),
    );
    sensitive.push(...codes, pool(f));
    return codes as string[];
  }
  const post = (
    s: any,
    op = "view",
    body: any = { password },
    headers: any = {},
  ) =>
    fetch(`${s.origin}/api/auth/two-factor/${op}-backup-codes`, {
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
  async function codes(r: Response, count: number) {
    assert.equal(r.status, 200);
    assert.equal(r.headers.get("cache-control"), "no-store");
    assert.equal(r.headers.get("referrer-policy"), "no-referrer");
    assert.equal(r.headers.get("set-cookie"), null);
    const v = (await r.json()) as any;
    assert.deepEqual(Object.keys(v).sort(), ["backupCodes", "status"]);
    assert.equal(v.status, true);
    assert.equal(v.backupCodes.length, count);
    assert.equal(new Set(v.backupCodes).size, count);
    assert(
      v.backupCodes.every(
        (c: any) =>
          typeof c === "string" && /^[A-Za-z0-9]{5}-[A-Za-z0-9]{5}$/.test(c),
      ),
    );
    sensitive.push(...v.backupCodes);
    return v.backupCodes as string[];
  }
  for (const mode of ["WAL", "DELETE"]) {
    const f = fresh(`${mode}-flow`, mode),
      s = await start(f, { backupManagement: true });
    try {
      const original = await decrypt(f),
        before = q(
          f,
          "SELECT id,secret,verified,failedVerificationCount,lockedUntil FROM twoFactor",
        ),
        sessions = q(f, "SELECT * FROM session");
      const returned = await codes(await post(s), 10);
      assert.deepEqual(returned, original);
      assert.deepEqual(q(f, "SELECT * FROM session"), sessions);
      record(
        `${mode}-source-encrypted-pool-own-password-view-no-session-or-pool-change`,
      );
      const replaced = await codes(await post(s, "generate"), 10);
      assert.deepEqual(await decrypt(f), replaced);
      assert(!replaced.some((v) => original.includes(v)));
      assert.deepEqual(
        q(
          f,
          "SELECT id,secret,verified,failedVerificationCount,lockedUntil FROM twoFactor",
        ),
        before,
      );
      assert.deepEqual(q(f, "SELECT * FROM session"), sessions);
      const audit = q(
        f,
        "SELECT event_type,entity_id FROM workspace_audit WHERE entity_type='native_mfa_backup_management_v1' ORDER BY id",
      );
      assert.deepEqual(
        audit.map((a) => a.event_type),
        ["mfa_backup_codes_viewed", "mfa_backup_codes_regenerated"],
      );
      assert(!sensitive.some((value) => JSON.stringify(audit).includes(value)));
      record(
        `${mode}-known-regeneration-source-decrypts-new10-precise-audit-no-other-factor-or-session-change`,
      );
      const login = async (code: string) => {
        const first = await fetch(`${s.origin}/api/auth/sign-in/email`, {
          method: "POST",
          headers: { origin: s.origin, "content-type": "application/json" },
          body: JSON.stringify({ email: "u@example.invalid", password }),
        });
        assert.equal(first.status, 200);
        const challenge = first.headers
          .getSetCookie()
          .find(
            (v) =>
              v.startsWith("better-auth.two_factor=") &&
              !v.includes("Max-Age=0"),
          )
          ?.split(";")[0];
        assert(challenge);
        sensitive.push(challenge);
        return fetch(`${s.origin}/api/auth/two-factor/verify-backup-code`, {
          method: "POST",
          headers: {
            origin: s.origin,
            "content-type": "application/json",
            cookie: challenge,
          },
          body: JSON.stringify({ code }),
        });
      };
      const old = await login(original[0]!);
      assert.equal(old.status, 401);
      assert.equal(((await old.json()) as any).code, "INVALID_BACKUP_CODE");
      assert.equal(old.headers.get("set-cookie"), null);
      assert.deepEqual(await decrypt(f), replaced);
      record(`${mode}-old-code-cannot-authenticate-after-replacement`);
      const current = await login(replaced[0]!);
      assert.equal(current.status, 200);
      assert(
        current.headers
          .getSetCookie()
          .some(
            (v) =>
              v.startsWith("better-auth.session_token=") &&
              !v.includes("Max-Age=0"),
          ),
      );
      const remaining = await codes(await post(s), 9);
      assert.deepEqual(remaining, replaced.slice(1));
      record(
        `${mode}-new-code-authenticates-once-and-authenticated-view-shows-remaining9`,
      );
    } finally {
      await s.stop();
    }
  }
  for (const count of [0, 1]) {
    const f = fresh(`pool-${count}`),
      plain = count ? ["ABCDE-12345"] : [];
    sensitive.push(...plain);
    const encrypted = await symmetricEncrypt({
      key: keyset,
      data: JSON.stringify(plain),
    });
    db(f, (d) => d.query("UPDATE twoFactor SET backupCodes=?").run(encrypted));
    const s = await start(f, { backupManagement: true });
    try {
      assert.deepEqual(await codes(await post(s), count), plain);
      record(`remaining-pool-${count}-valid-view-no-fabricated-codes`);
    } finally {
      await s.stop();
    }
  }
  const invalid = [
    { id: "missing-password", body: {} },
    { id: "null-password", body: { password: null } },
    { id: "array-body", body: [password] },
    { id: "duplicate-password", body: '{"password":"one","password":"two"}' },
    { id: "caller-user", body: { password, userId: "foreign" } },
    {
      id: "caller-email",
      body: { password, email: "foreign@example.invalid" },
    },
    { id: "long-password", body: { password: "p".repeat(4097) } },
    {
      id: "bare-cookie",
      headers: { cookie: `better-auth.session_token; ${cookie}` },
      status: 401,
    },
    {
      id: "pending-cookie",
      headers: { cookie: "better-auth.two_factor=not-session" },
      status: 401,
    },
    {
      id: "service",
      headers: { "x-rentemester-service-key": "private-service" },
      status: 401,
    },
    {
      id: "foreign-origin",
      headers: { origin: "https://foreign.invalid" },
      status: 403,
    },
    {
      id: "wrong-content",
      headers: { "content-type": "text/plain" },
      status: 415,
    },
    { id: "body-bound", body: " ".repeat(8193), status: 413 },
  ] as { id: string; body?: any; headers?: any; status?: number }[];
  for (const t of invalid) {
    const f = fresh(t.id),
      s = await start(f, { backupManagement: true });
    try {
      const before = image(f),
        r = await post(s, "view", t.body ?? { password }, t.headers);
      assert.equal(r.status, t.status ?? 400, t.id);
      assert.equal(r.headers.get("set-cookie"), null);
      assert.equal(image(f), before);
      assert.deepEqual(outcomes(f), []);
      record(`${t.id}-refused-before-native-dispatch`);
    } finally {
      await s.stop();
    }
  }
  for (const [id, sql, age, pass, status] of [
    ["wrong-password", "", 1000, "Incorrect private password", 401],
    ["stale", "", 600000, password, 401],
    [
      "disabled",
      "INSERT INTO rm_workspace_user_access_events(user_id,event_type,actor) VALUES('u','disable','agent:synthetic')",
      1000,
      password,
      401,
    ],
    [
      "unverified",
      "UPDATE user SET emailVerified=0 WHERE id='u'",
      1000,
      password,
      401,
    ],
    ["revoked", "DELETE FROM session", 1000, password, 401],
    [
      "malformed-pool",
      "UPDATE twoFactor SET backupCodes='invalid-private-cipher'",
      1000,
      password,
      503,
    ],
  ] as const) {
    const f = fresh(id, "WAL", age);
    if (sql) db(f, (d) => d.exec(sql));
    const s = await start(f, { backupManagement: true });
    try {
      const before = image(f),
        r = await post(s, "generate", { password: pass });
      assert.equal(r.status, status, id);
      const text = await r.text();
      assert(!text.includes("backupCodes"));
      assert.equal(r.headers.get("set-cookie"), null);
      assert.equal(image(f), before);
      record(`${id}-no-codes-no-replacement-or-accepted-audit`);
    } finally {
      await s.stop();
    }
  }
  for (const op of ["view", "generate"]) {
    const f = fresh(`unknown-${op}`),
      s = await start(f, { backupManagement: true }),
      blocker = new Database(f.control);
    try {
      blocker.exec("BEGIN;SELECT * FROM twoFactor");
      const before = image(f),
        r = await post(s, op);
      assert.equal(r.status, 503);
      assert.deepEqual(await r.json(), {
        message: "Authentication outcome could not be confirmed",
        code: "AUTH_UNCONFIRMED",
      });
      assert.equal(r.headers.get("set-cookie"), null);
      assert.equal(image(f), before);
      assert.equal(outcomes(f).at(-1)?.stage, "OutcomeUnknown");
      record(`${op}-real-DELETE-commit-busy-withholds-all-codes-and-success`);
    } finally {
      blocker.exec("ROLLBACK");
      blocker.close();
      await s.stop();
    }
  }
  const rate = fresh("rate"),
    s = await start(rate, { backupManagement: true });
  try {
    for (let i = 0; i < 3; i++)
      assert.equal(
        (
          await post(
            s,
            i % 2 ? "generate" : "view",
            { password: "wrong-private-password" },
            { "x-forwarded-for": `203.0.113.${i}` },
          )
        ).status,
        401,
      );
    const before = image(rate),
      r = await post(s, "generate");
    assert.equal(r.status, 429);
    assert.equal(r.headers.get("retry-after"), "60");
    assert.equal(image(rate), before);
    assert.equal(outcomes(rate).length, 3);
    record(
      "shared-view-regeneration-direct-peer-KDF-budget-ignores-forwarded-rotation",
    );
  } finally {
    await s.stop();
  }
  const absent = fresh("absent"),
    none = await start(absent);
  try {
    const before = image(absent);
    for (const op of ["view", "generate"])
      assert.equal((await post(none, op)).status, 404);
    assert.equal(image(absent), before);
    record(
      "missing-trusted-handler-no-source-serverOnly-or-regeneration-fallback",
    );
  } finally {
    await none.stop();
  }
  assert(!sensitive.some((v) => JSON.stringify(observations).includes(v)));
}
