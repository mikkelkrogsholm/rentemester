/** Source-issued service credentials, atomic native quota/rate and exclusive HTTP context. */
import assert from "node:assert/strict";
import { Database } from "bun:sqlite";
import { copyFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { serializeSignedCookie } from "../../node_modules/better-call/dist/cookies.mjs";
import { defaultKeyHasher } from "../../node_modules/@better-auth/api-key/dist/index.mjs";
import { createBetterAuthRuntime } from "../../src/server/better-auth";
import {
  createWorkspaceServicePrincipal,
  rotateWorkspaceServiceCredential,
} from "../../src/core/workspace-service-principals";
import { getWorkspaceSessionContext } from "../../src/core/workspace-access";
const catalog = [
  { slug: "zeta", name: "Synthetic Z ApS", archived: false },
  { slug: "hidden", name: "Hidden synthetic ApS", archived: false },
  { slug: "archive", name: "Archived synthetic ApS", archived: true },
];
function db<T>(f: any, fn: (d: Database) => T): T {
  const d = new Database(f.control);
  try {
    return fn(d);
  } finally {
    d.close();
  }
}
function tables(f: any) {
  return db(f, (d) =>
    Object.fromEntries(
      (
        d
          .query(
            "SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name",
          )
          .all() as any[]
      ).map(({ name }) => [
        name,
        d
          .query(`SELECT * FROM "${name.replaceAll('"', '""')}" ORDER BY rowid`)
          .all(),
      ]),
    ),
  );
}
const image = (f: any) => JSON.stringify(tables(f));
const opts = {
  secret: "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
  trustedOrigins: ["https://example.invalid"],
  baseURL: "https://example.invalid",
  useSecureCookies: true,
};
async function prepare(c: any, prefix: string) {
  const seed = c.fixture(`${prefix}-source-issued`, "DELETE");
  const sourceDb = new Database(seed.control),
    auth = createBetterAuthRuntime(sourceDb, opts);
  const issued = await createWorkspaceServicePrincipal(sourceDb, auth, {
    displayName: "Synthetic automation",
    actor: "agent:synthetic",
  });
  const { serviceAccountId: subject, credentialId, secret } = issued;
  sourceDb
    .query(
      "INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES(?,'activate','member','agent:synthetic')",
    )
    .run(subject);
  for (const [slug, role] of [
    ["zeta", "bookkeeper"],
    ["archive", "reader"],
    ["dangling", "owner"],
  ])
    sourceDb
      .query(
        "INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?,?,'grant',?,'agent:synthetic')",
      )
      .run(subject, slug, role);
  sourceDb.exec("PRAGMA wal_checkpoint(TRUNCATE);PRAGMA journal_mode=DELETE");
  sourceDb.close();
  const now = Date.now() + 2000,
    iso = (n: number) => new Date(n).toISOString();
  function fresh(id: string, mode = "DELETE") {
    const f = c.fixture(`${prefix}-${id}`, mode);
    copyFileSync(seed.control, f.control);
    db(f, (d) => d.exec(`PRAGMA journal_mode=${mode}`));
    return f;
  }
  const row = (f: any) =>
    db(
      f,
      (d) =>
        d.query("SELECT * FROM apikey WHERE id=?").get(credentialId) as any,
    );
  const mutate = (f: any, s: string) => db(f, (d) => d.exec(s));
  const update = (f: any, fields: any) =>
    Object.keys(fields).length === 0
      ? undefined
      : db(f, (d) =>
          d
            .query(
              `UPDATE apikey SET ${Object.keys(fields)
                .map((k) => `"${k}"=?`)
                .join(",")} WHERE id=?`,
            )
            .run(...(Object.values(fields) as any[]), credentialId),
        );
  const expected = {
    ok: true,
    user: {
      id: subject,
      email: `${subject}@service.invalid`,
      emailVerified: false,
      twoFactorEnabled: false,
    },
    workspaceRole: "member",
    companies: [
      { ...catalog[0], role: "bookkeeper" },
      { ...catalog[2], role: "reader" },
    ],
  };
  function preserved(before: any, after: any) {
    for (const v of [before, after])
      for (const r of v.apikey)
        if (r.id === credentialId)
          for (const k of [
            "requestCount",
            "remaining",
            "lastRequest",
            "lastRefillAt",
            "updatedAt",
          ])
            delete r[k];
    assert.deepEqual(after, before);
  }
  function sourceContext(f: any) {
    const root = f.dir ?? dirname(f.control);
    writeFileSync(
      join(root, "workspace.json"),
      JSON.stringify({
        version: 1,
        companies: catalog.map((c) => ({ ...c, createdAt: iso(now) })),
      }),
    );
    return db(f, (d) => ({
      ok: true,
      ...getWorkspaceSessionContext(d, root, subject),
    }));
  }
  async function sourceVerify(f: any) {
    const d = new Database(f.control),
      a = createBetterAuthRuntime(d, opts),
      RealDate = globalThis.Date;
    class Fixed extends RealDate {
      constructor(v?: any) {
        super(v === undefined ? now : v);
      }
      static now() {
        return now;
      }
    }
    globalThis.Date = Fixed as typeof Date;
    try {
      return await (a.api as any).verifyApiKey({
        body: { key: secret, configId: "workspace-service-principal" },
      });
    } finally {
      globalThis.Date = RealDate;
      d.close();
    }
  }
  assert.equal(row(seed).key, await defaultKeyHasher(secret));
  return {
    seed,
    subject,
    credentialId,
    secret,
    now,
    iso,
    fresh,
    row,
    mutate,
    update,
    expected,
    preserved,
    sourceContext,
    sourceVerify,
  };
}
export async function verifyServiceContext(c: any) {
  const t = await prepare(c, "service-core"),
    {
      subject,
      credentialId,
      secret,
      now,
      iso,
      fresh,
      row,
      mutate,
      update,
      expected,
      preserved,
      sourceContext,
      sourceVerify,
    } = t;
  const note = (id: string) => c.record(`service-context/${id}`),
    request = (extra: any = {}) => ({
      mode: "service-context",
      serviceSecret: secret,
      catalog,
      now,
      ...extra,
    }),
    run = (f: any, extra: any = {}) => c.run(f, request(extra));
  const sourceSafe = (f: any) => [row(f).key, secret];
  for (const mode of ["WAL", "DELETE"]) {
    const f = fresh(`default-${mode}`, mode),
      before = tables(f),
      r = run(f);
    assert(r.ok);
    assert.deepEqual(r.context, expected);
    assert.deepEqual(r.context, sourceContext(f));
    assert.equal(row(f).requestCount, 1);
    assert.equal(row(f).lastRequest, iso(now));
    preserved(before, tables(f));
    note(`${mode}-source-issued-key-exact-own-context-and-usage-only`);
    assert.equal(
      db(
        f,
        (d) =>
          d
            .query("SELECT count(*) n FROM session WHERE userId=?")
            .get(subject) as any,
      ).n,
      0,
    );
    assert.equal(
      db(
        f,
        (d) =>
          d
            .query("SELECT count(*) n FROM account WHERE userId=?")
            .get(subject) as any,
      ).n,
      0,
    );
    note(`${mode}-registered-flags0-no-human-session-password-or-MFA`);
    const next = run(f);
    assert(next.ok);
    assert.equal(row(f).requestCount, 2);
    note(`${mode}-restart-observes-persisted-admission-count`);
  }
  const rotated = fresh("actual-source-rotation");
  const rd = new Database(rotated.control),
    ra = createBetterAuthRuntime(rd, opts);
  const replacement = await rotateWorkspaceServiceCredential(rd, ra, {
    serviceAccountId: subject,
    credentialId,
    actor: "agent:synthetic",
  });
  rd.close();
  let rotationBefore = image(rotated);
  assert.equal(run(rotated).ok, false);
  assert.equal(image(rotated), rotationBefore);
  const replacementResult = run(rotated, {
    serviceSecret: replacement.secret,
    now: Date.now() + 1000,
  });
  assert(replacementResult.ok);
  assert.deepEqual(replacementResult.context, expected);
  assert.equal(
    db(
      rotated,
      (d) =>
        d
          .query("SELECT requestCount FROM apikey WHERE id=?")
          .get(replacement.credentialId) as any,
    ).requestCount,
    1,
  );
  note(
    "actual-source-rotation-disables-old-and-retains-stable-own-service-context",
  );
  const owner = fresh("service-owner-no-inheritance");
  mutate(
    owner,
    `INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('${subject}','activate','workspace_owner','agent:synthetic')`,
  );
  const ownerResult = run(owner);
  assert(ownerResult.ok);
  assert.deepEqual(ownerResult.context, {
    ...expected,
    workspaceRole: "workspace_owner",
  });
  note("service-owner-never-inherits-ungranted-companies");
  const policies: any[] = [
    ["default", {}],
    ["quota", { remaining: 3 }],
    [
      "refill-due",
      {
        remaining: 0,
        refillInterval: 1000,
        refillAmount: 7,
        lastRefillAt: iso(now - 1001),
        createdAt: iso(now - 10000),
      },
    ],
    [
      "refill-equality-decrement",
      {
        remaining: 3,
        refillInterval: 1000,
        refillAmount: 7,
        lastRefillAt: iso(now - 1000),
        createdAt: iso(now - 10000),
      },
    ],
    [
      "refill-from-created",
      {
        remaining: 0,
        refillInterval: 1000,
        refillAmount: 7,
        lastRefillAt: null,
        createdAt: iso(now - 1001),
        updatedAt: iso(now - 1001),
      },
    ],
    [
      "unlimited-ignores-refill",
      {
        remaining: null,
        refillInterval: 1,
        refillAmount: 7,
        createdAt: iso(now - 10000),
      },
    ],
    [
      "partial-null-refill",
      { remaining: 3, refillInterval: null, refillAmount: 7 },
    ],
    ["zero-refill", { remaining: 3, refillInterval: 0, refillAmount: 7 }],
    [
      "rate-disabled",
      {
        rateLimitEnabled: 0,
        requestCount: 650,
        rateLimitTimeWindow: null,
        rateLimitMax: null,
      },
    ],
    [
      "null-rate-fields",
      { rateLimitMax: null, requestCount: 17, lastRequest: iso(now - 100) },
    ],
    [
      "null-rate-enabled",
      { rateLimitEnabled: null, requestCount: 1, lastRequest: iso(now - 100) },
    ],
    [
      "idle-reset-count-above-max",
      {
        requestCount: 999,
        rateLimitMax: 2,
        lastRequest: iso(now - 60001),
        createdAt: iso(now - 100000),
      },
    ],
    ["null-expiry", { expiresAt: null }],
    [
      "disabled-zero-rate-policy",
      { rateLimitEnabled: 0, rateLimitMax: 0, rateLimitTimeWindow: 0 },
    ],
    [
      "null-window-zero-max-skip",
      { rateLimitMax: 0, rateLimitTimeWindow: null },
    ],
  ];
  for (const [id, fields] of policies) {
    const native = fresh(`policy-${id}`),
      source = fresh(`oracle-${id}`);
    update(native, fields);
    update(source, fields);
    const before = tables(native),
      r = run(native);
    assert(r.ok, id);
    const oracle = await sourceVerify(source);
    assert.equal(oracle.valid, true, id);
    const n = row(native),
      s = row(source);
    for (const k of [
      "requestCount",
      "remaining",
      "lastRequest",
      "lastRefillAt",
      "updatedAt",
    ])
      assert.deepEqual(n[k], s[k], `${id}/${k}`);
    assert.deepEqual(r.context, sourceContext(source));
    preserved(before, tables(native));
    note(`actual-source-${id}-exact-usage-and-context`);
  }
  const boundary = fresh("native-exact-window");
  update(boundary, {
    requestCount: 1,
    rateLimitMax: 3,
    lastRequest: iso(now - 60000),
    createdAt: iso(now - 100000),
  });
  const boundaryBefore = tables(boundary),
    boundaryResult = run(boundary);
  assert(boundaryResult.ok);
  assert.equal(row(boundary).requestCount, 2);
  assert.equal(row(boundary).lastRequest, iso(now));
  preserved(boundaryBefore, tables(boundary));
  note("native-exact-window-increments-without-source-CAS-recursion");
  const refusals: any[] = [
    ["disabled", { enabled: 0 }],
    ["null-enabled", { enabled: null }],
    ["bad-enabled", { enabled: 2 }],
    ["expired", { expiresAt: iso(now - 1) }],
    ["expiry-equality", { expiresAt: iso(now) }],
    ["wrong-config", { configId: "other" }],
    ["quota-zero", { remaining: 0 }],
    [
      "rate-full-equality",
      {
        requestCount: 2,
        rateLimitMax: 2,
        lastRequest: iso(now - 60000),
        createdAt: iso(now - 100000),
      },
    ],
    [
      "refill-equality-no-slot",
      {
        remaining: 0,
        refillInterval: 1000,
        refillAmount: 7,
        lastRefillAt: iso(now - 1000),
        createdAt: iso(now - 10000),
      },
    ],
    ["negative-quota", { remaining: -1 }],
    ["fractional-count", { requestCount: 1.5 }],
    ["unsafe-count", { requestCount: 9007199254740992 }],
    ["null-count", { requestCount: null }],
    ["bad-rate-flag", { rateLimitEnabled: 2 }],
    ["zero-rate-max", { rateLimitMax: 0 }],
    ["negative-window", { rateLimitTimeWindow: -1 }],
    ["bad-expiry", { expiresAt: "2030-01-01 00:00:00" }],
    ["future-created", { createdAt: iso(now + 1) }],
    ["future-updated", { updatedAt: iso(now + 1) }],
    ["future-request", { lastRequest: iso(now + 1) }],
    ["future-refill", { lastRefillAt: iso(now + 1) }],
  ];
  for (const [id, fields] of refusals) {
    const f = fresh(`refuse-${id}`);
    update(f, fields);
    const before = image(f),
      r = run(f);
    assert.equal(r.ok, false, id);
    assert(!r.context, id);
    assert.equal(image(f), before, id);
    note(`${id}-no-context-no-partial-usage`);
  }
  for (const [id, sql, extra] of [
    [
      "disabled-access",
      `INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('${subject}','disable',NULL,'agent:synthetic')`,
      {},
    ],
    ["invalid-key", "", { serviceSecret: "unknown-private-service-value" }],
    ["wrong-workspace", "", { credentialWorkspace: "other" }],
    ["wrong-catalog", "", { catalogWorkspace: "other" }],
    ["bad-context", `UPDATE user SET email='' WHERE id='${subject}'`, {}],
    ["late-SQL", "", { failSql: true }],
  ] as any[]) {
    const f = fresh(`guard-${id}`);
    if (sql) mutate(f, sql);
    update(f, { remaining: 3 });
    const before = image(f),
      r = run(f, extra);
    assert.equal(r.ok, false, id);
    assert(!r.context);
    assert.equal(image(f), before);
    note(`${id}-rollback-includes-usage`);
  }
  const duplicate = fresh("ambiguous-hash");
  db(duplicate, (d) => {
    const r = row(duplicate);
    d.query(
      `INSERT INTO apikey(${Object.keys(r)
        .map((k) => `"${k}"`)
        .join(",")}) VALUES(${Object.keys(r)
        .map(() => "?")
        .join(",")})`,
    ).run(
      ...(Object.entries(r).map(([k, v]) =>
        k === "id" ? "duplicate" : k === "configId" ? "other" : v,
      ) as any[]),
    );
  });
  let before = image(duplicate);
  assert.equal(run(duplicate).ok, false);
  assert.equal(image(duplicate), before);
  note("duplicate-hash-across-configs-refuses-all-usage");
  const orphan = fresh("orphan-registry");
  db(orphan, (d) => {
    d.exec("PRAGMA foreign_keys=OFF");
    d.query("UPDATE apikey SET referenceId='u' WHERE id=?").run(credentialId);
  });
  before = image(orphan);
  assert.equal(run(orphan).ok, false);
  assert.equal(image(orphan), before);
  note("unregistered-human-subject-never-becomes-service");
  for (const mode of ["WAL", "DELETE"]) {
    const f = fresh(`last-slot-${mode}`, mode);
    update(f, { remaining: 1, rateLimitEnabled: 0 });
    const a = c.start(f, request()),
      b = c.start(f, request());
    a.child.stdin.end();
    b.child.stdin.end();
    const results = await Promise.all([c.done(a), c.done(b)]);
    assert.equal(results.filter((r) => r.ok).length, 1);
    assert.equal(row(f).remaining, 0);
    note(`${mode}-last-quota-slot-one-commit`);
    const held = fresh(`held-${mode}`, mode),
      p = c.start(held, request(), "service-before-commit");
    await c.marker(p, held);
    const writer = new Database(held.control);
    try {
      assert.throws(() => writer.exec("BEGIN IMMEDIATE"));
      p.child.stdin.write("\n");
      p.child.stdin.end();
      assert((await c.done(p)).ok);
      writer.query("UPDATE apikey SET enabled=0 WHERE id=?").run(credentialId);
      const revoked = image(held);
      assert.equal(run(held).ok, false);
      assert.equal(image(held), revoked);
      note(`${mode}-writer-excludes-revoke-then-next-admission-refuses`);
    } finally {
      writer.close();
    }
    const killed = fresh(`kill-${mode}`, mode);
    before = image(killed);
    const kp = c.start(killed, request(), "service-before-commit");
    await c.marker(kp, killed);
    kp.child.kill();
    await kp.child.exited;
    clearTimeout(kp.timer);
    assert.equal(image(killed), before);
    note(`${mode}-death-before-commit-rolls-back-usage`);
  }
  const unknown = fresh("commit-unknown", "DELETE"),
    reader = new Database(unknown.control);
  try {
    reader.exec("BEGIN");
    reader.query("SELECT * FROM user").all();
    const r = run(unknown);
    assert.equal(r.ok, false);
    assert.equal(r.stage, "OutcomeUnknown");
    assert.equal(r.reusable, false);
    assert(!r.context);
    reader.exec("ROLLBACK");
    note("actual-DELETE-commit-busy-unknown-withholds-context-no-replay");
  } finally {
    reader.close();
  }
  const busy = fresh("writer-busy"),
    blocker = new Database(busy.control);
  before = image(busy);
  try {
    blocker.exec("BEGIN IMMEDIATE");
    const r = run(busy);
    assert.equal(r.ok, false);
    assert(!r.context);
    blocker.exec("ROLLBACK");
    assert.equal(image(busy), before);
    note("actual-busy-writer-no-usage-or-context");
  } finally {
    blocker.close();
  }
  return [secret, replacement.secret, ...sourceSafe(t.seed)];
}
export async function verifyHttpServiceContext(c: any) {
  const t = await prepare(c, "service-http"),
    {
      secret,
      now,
      fresh,
      row,
      update,
      mutate,
      expected,
      preserved,
      sourceContext,
      credentialId,
      subject,
    } = t;
  const record = (id: string) => c.note(`service-context/${id}`);
  async function get(s: any, extra: any = {}) {
    const r = await fetch(
      `${s.origin}${extra.path ?? "/api/me"}${extra.query ?? ""}`,
      {
        method: extra.method ?? "GET",
        headers: { "x-rentemester-service-key": secret, ...extra.headers },
      },
    );
    assert.equal(r.headers.getSetCookie().length, 0);
    assert.equal(r.headers.get("cache-control"), "no-store");
    const body = await r.text();
    return {
      status: r.status,
      body,
      data: body ? JSON.parse(body) : null,
      headers: r.headers,
    };
  }
  for (const mode of ["WAL", "DELETE"]) {
    const f = fresh(`happy-${mode}`, mode),
      before = tables(f),
      s = await c.start(f, { serviceContext: catalog, now });
    try {
      let r = await get(s);
      assert.equal(r.status, 200);
      assert.deepEqual(r.data, expected);
      assert.deepEqual(r.data, sourceContext(f));
      assert.equal(row(f).requestCount, 1);
      assert(!r.body.includes(secret));
      preserved(before, tables(f));
      record(`${mode}-source-issued-key-own-context-and-exact-usage-only`);
      mutate(
        f,
        `INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('${subject}','zeta','revoke',NULL,'agent:synthetic')`,
      );
      r = await get(s);
      assert.equal(r.status, 200);
      assert.deepEqual(r.data.companies, [{ ...catalog[2], role: "reader" }]);
      record(`${mode}-current-membership-revocation-not-cached`);
      update(f, { enabled: 0 });
      const revoked = image(f);
      r = await get(s);
      assert.equal(r.status, 401);
      assert(!r.data.user);
      assert.equal(image(f), revoked);
      record(`${mode}-current-credential-revocation-no-fallback-or-usage`);
    } finally {
      await s.stop();
    }
  }
  for (const [id, extra, status] of [
    ["empty", { headers: { "x-rentemester-service-key": "" } }, 401],
    [
      "oversize",
      { headers: { "x-rentemester-service-key": "X".repeat(4097) } },
      401,
    ],
    ["mixed-cookie", { headers: { cookie: "unrelated=private" } }, 401],
    [
      "mixed-authorization",
      { headers: { authorization: "Bearer private" } },
      401,
    ],
    [
      "invalid",
      { headers: { "x-rentemester-service-key": "invalid-private-service" } },
      401,
    ],
    ["query", { query: "?userId=foreign" }, 400],
    ["origin", { headers: { origin: "https://foreign.invalid" } }, 403],
    ["fetch-site", { headers: { "sec-fetch-site": "cross-site" } }, 403],
    ["encoding", { headers: { "content-encoding": "gzip" } }, 415],
  ] as any[]) {
    const f = fresh(`wire-${id}`),
      before = image(f),
      s = await c.start(f, {
        serviceContext: catalog,
        workspaceContext: catalog,
        now,
      });
    try {
      const r = await get(s, extra);
      assert.equal(r.status, status, id);
      assert(!r.data?.user);
      assert.equal(image(f), before);
      record(`${id}-no-context-no-usage-no-human-dispatch`);
    } finally {
      await s.stop();
    }
  }
  for (const [id, fields] of [
    ["expired", { expiresAt: new Date(now).toISOString() }],
    ["quota", { remaining: 0 }],
    [
      "rate",
      {
        rateLimitMax: 1,
        requestCount: 1,
        lastRequest: new Date(now).toISOString(),
      },
    ],
  ] as any[]) {
    const f = fresh(`deny-${id}`);
    update(f, fields);
    const before = image(f),
      s = await c.start(f, { serviceContext: catalog, now });
    try {
      const r = await get(s);
      assert.equal(r.status, 401);
      assert(!r.data.user);
      assert.equal(image(f), before);
      record(`${id}-same-opaque-denial-without-usage`);
    } finally {
      await s.stop();
    }
  }
  const f = fresh("shared-head-rate"),
    s = await c.start(f, {
      serviceContext: catalog,
      workspaceContext: catalog,
      now,
    });
  try {
    for (let i = 0; i < 12; i++)
      assert.equal((await get(s, { method: "HEAD" })).status, 405);
    for (let i = 0; i < 5; i++)
      assert.equal(
        (
          await get(s, {
            headers: { "x-forwarded-for": `198.51.100.${i + 1}` },
          })
        ).status,
        200,
      );
    const humanToken = "service-context-private-human";
    db(f, (d) =>
      d
        .query(
          "INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt) VALUES('service-human','u',?,?,?,?)",
        )
        .run(
          humanToken,
          new Date(now - 1000).toISOString(),
          new Date(now - 1000).toISOString(),
          new Date(now + 3600000).toISOString(),
        ),
    );
    const humanCookie = (
      await serializeSignedCookie(
        "better-auth.session_token",
        humanToken,
        opts.secret,
      )
    ).split(";")[0]!;
    for (let i = 0; i < 5; i++) {
      const human = await fetch(`${s.origin}/api/me`, {
        headers: { cookie: humanCookie },
      });
      assert.equal(human.status, 200);
      assert.equal(((await human.json()) as any).user.id, "u");
    }
    const r = await get(s);
    assert.equal(r.status, 429);
    assert.equal(r.headers.get("retry-after"), "60");
    assert.equal(row(f).requestCount, 5);
    record("HEAD-no-charge-shared-human-service-direct-peer-context-budget");
  } finally {
    await s.stop();
  }
  const absent = fresh("absent"),
    server = await c.start(absent, { workspaceContext: catalog, now });
  try {
    const before = image(absent);
    assert.equal((await get(server)).status, 404);
    assert.equal(image(absent), before);
    record("absent-service-handler-no-human-or-legacy-fallback");
  } finally {
    await server.stop();
  }
  const humanOnly = fresh("human-only"),
    hs = await c.start(humanOnly, { serviceContext: catalog, now });
  try {
    const before = image(humanOnly);
    const r = await get(hs, { path: "/api/auth/get-session" });
    assert.equal(r.status, 401);
    assert.equal(image(humanOnly), before);
    record("human-only-session-route-never-admits-service");
  } finally {
    await hs.stop();
  }
  const unknown = fresh("unknown", "DELETE"),
    us = await c.start(unknown, { serviceContext: catalog, now }),
    reader = new Database(unknown.control);
  try {
    reader.exec("BEGIN");
    reader.query("SELECT * FROM user").all();
    const r = await get(us);
    assert.equal(r.status, 503);
    assert(!r.data.user);
    assert(!r.body.includes(secret));
    reader.exec("ROLLBACK");
    record("actual-DELETE-unknown-no-context-cookie-or-success");
  } finally {
    reader.close();
    await us.stop();
  }
  return [secret, t.row(t.seed).key, credentialId];
}
