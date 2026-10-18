/** Native human /api/me parity: own current control snapshot and trusted registered catalog. */
import assert from "node:assert/strict";
import { Database } from "bun:sqlite";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { getWorkspaceSessionContext } from "../../src/core/workspace-access";
import { serializeSignedCookie } from "../../node_modules/better-call/dist/cookies.mjs";
const token = "workspace-context-private-current";
const catalog = [
  { slug: "z-last", name: "Synthetic Z ApS", archived: false },
  { slug: "archive", name: "Synthetic archived ApS", archived: true },
  { slug: "ungranted", name: "Hidden synthetic ApS", archived: false },
  { slug: "a-first", name: "Synthetic A ApS", archived: false },
  { slug: "review", name: "Synthetic review ApS", archived: false },
  { slug: "read", name: "Synthetic read ApS", archived: false },
];
function db<T>(f: any, fn: (d: Database) => T): T {
  const d = new Database(f.control);
  try {
    return fn(d);
  } finally {
    d.close();
  }
}
function image(f: any) {
  return db(f, (d) =>
    JSON.stringify(
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
function setup(f: any, now: number) {
  db(f, (d) => {
    d.exec(
      "INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('foreign','Foreign','foreign@example.invalid',1,'2026-01-01','2026-01-01',1)",
    );
    d.query(
      "INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt) VALUES('context-session','u',?,?,?,?)",
    ).run(
      token,
      new Date(now - 3600000).toISOString(),
      new Date(now - 3600000).toISOString(),
      new Date(now + 3600000).toISOString(),
    );
    const insert = d.query(
      "INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor,created_at) VALUES(?,?,?,?,?,?)",
    );
    for (const [slug, role] of [
      ["z-last", "owner"],
      ["archive", "bookkeeper"],
      ["a-first", "reviewer"],
      ["review", "reviewer"],
      ["read", "reader"],
      ["dangling", "owner"],
    ])
      insert.run(
        "u",
        slug,
        "grant",
        role,
        "agent:synthetic",
        "2030-01-01T00:00:00.000Z",
      );
    insert.run(
      "u",
      "a-first",
      "revoke",
      null,
      "agent:synthetic",
      "2020-01-01T00:00:00.000Z",
    );
    insert.run(
      "foreign",
      "ungranted",
      "grant",
      "owner",
      "agent:synthetic",
      "2026-01-01T00:00:00.000Z",
    );
  });
  return f;
}
function expected(role = "member") {
  return {
    ok: true,
    user: {
      id: "u",
      email: "u@example.invalid",
      emailVerified: true,
      twoFactorEnabled: true,
    },
    workspaceRole: role,
    companies: catalog
      .filter((c) => !["ungranted", "a-first"].includes(c.slug))
      .map((c) => ({
        ...c,
        role: (
          {
            "z-last": "owner",
            archive: "bookkeeper",
            review: "reviewer",
            read: "reader",
          } as any
        )[c.slug],
      })),
  };
}
function source(f: any, selected: any[]) {
  const workspace = f.dir ?? join(f.control, "..");
  writeFileSync(
    join(workspace, "workspace.json"),
    JSON.stringify({
      version: 1,
      companies: selected.map((c) => ({
        ...c,
        createdAt: "2026-01-01T00:00:00.000Z",
      })),
    }),
  );
  return db(f, (d) => ({
    ok: true,
    ...getWorkspaceSessionContext(d, workspace, "u"),
  }));
}
export async function verifyWorkspaceContext(c: any) {
  const { fixture, run, start, done, marker, record, now, key } = c;
  const note = (id: string) => record(`workspace-context/${id}`);
  const request = (extra: any = {}) => ({
    mode: "workspace-context",
    token,
    catalog,
    ...extra,
  });
  for (const mode of ["wal", "delete"]) {
    const f = setup(fixture(`context-${mode}`, mode), now),
      before = image(f);
    let r = run(f, request());
    assert(r.ok);
    assert.deepEqual(r.context, expected());
    assert.deepEqual(r.context, source(f, catalog));
    assert.equal(image(f), before);
    note(
      `${mode}-source-exact-manifest-order-archive-explicit-latest-ID-membership-no-writes`,
    );
    db(f, (d) =>
      d.exec(
        "INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','workspace_owner','agent:synthetic')",
      ),
    );
    const owner = image(f);
    r = run(f, request());
    assert(r.ok);
    assert.deepEqual(r.context, expected("workspace_owner"));
    assert.deepEqual(r.context, source(f, catalog));
    assert.equal(image(f), owner);
    note(`${mode}-owner-no-company-inheritance`);
    r = run(f, request({ catalog: [] }));
    assert(r.ok);
    assert.deepEqual(r.context.companies, []);
    assert.equal(image(f), owner);
    note(`${mode}-empty-catalog-hides-dangling-grants`);
    // No local password credential is required for an already authenticated human context.
    db(f, (d) => d.exec("DELETE FROM account"));
    const noCredential = image(f);
    r = run(f, request());
    assert(r.ok);
    assert.deepEqual(r.context, expected("workspace_owner"));
    assert.equal(image(f), noCredential);
    note(`${mode}-nonfresh-current-session-needs-no-password-credential`);
  }
  for (const [id, mutation, extra] of [
    ["revoked-session", "DELETE FROM session", {}],
    [
      "expired-session",
      `UPDATE session SET expiresAt='${new Date(now - 1).toISOString()}'`,
      {},
    ],
    ["unverified-email", "UPDATE user SET emailVerified=0 WHERE id='u'", {}],
    ["disabled-MFA", "UPDATE user SET twoFactorEnabled=0 WHERE id='u'", {}],
    [
      "unverified-factor",
      "UPDATE twoFactor SET verified=0 WHERE userId='u'",
      {},
    ],
    [
      "disabled-access",
      "INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','disable',NULL,'agent:synthetic')",
      {},
    ],
    ["bad-user-email", "UPDATE user SET email='' WHERE id='u'", {}],
    ["wrong-proof-workspace", "", { proofWorkspace: "other" }],
    ["wrong-proof-profile", "", { proofDevelopment: true }],
    [
      "wrong-proof-key",
      "",
      { proofKey: "YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXoxMjM0NTY" },
    ],
    ["wrong-catalog-workspace", "", { catalogWorkspace: "other" }],
    ["duplicate-catalog", "", { catalog: [catalog[0], catalog[0]] }],
    [
      "invalid-slug",
      "",
      { catalog: [{ slug: "../escape", name: "Synthetic", archived: false }] },
    ],
    [
      "blank-name",
      "",
      { catalog: [{ slug: "valid", name: " ", archived: false }] },
    ],
    [
      "control-name",
      "",
      { catalog: [{ slug: "valid", name: "Synthetic\n", archived: false }] },
    ],
    [
      "long-name",
      "",
      { catalog: [{ slug: "valid", name: "X".repeat(1025), archived: false }] },
    ],
    [
      "catalog-count",
      "",
      {
        catalog: Array.from({ length: 1001 }, (_, i) => ({
          slug: `c${i}`,
          name: "Synthetic",
          archived: false,
        })),
      },
    ],
  ] as any[]) {
    const f = setup(fixture(`context-refuse-${id}`), now);
    if (mutation) db(f, (d) => d.exec(mutation));
    const before = image(f),
      r = run(f, request(extra));
    assert.equal(r.ok, false, id);
    assert(!r.context, id);
    assert.equal(image(f), before, id);
    note(`${id}-no-context-no-write`);
  }
  const repeated = setup(fixture("context-same-connection"), now),
    beforeRepeat = image(repeated),
    repeat = run(repeated, request({ repeat: true }));
  assert(repeat.ok);
  assert.deepEqual(repeat.context, expected());
  assert.deepEqual(repeat.repeatContext, expected());
  assert.equal(image(repeated), beforeRepeat);
  note("same-connection-successful-rollback-reuse-without-effects");
  const longSlug = setup(fixture("context-long-slug"), now),
    beforeLong = image(longSlug);
  assert.equal(
    run(
      longSlug,
      request({
        catalog: [
          { slug: "a".repeat(1025), name: "Synthetic", archived: false },
        ],
      }),
    ).ok,
    false,
  );
  assert.equal(image(longSlug), beforeLong);
  note("catalog-slug-byte-bound-no-partial-output");
  const big = setup(fixture("context-output-bound"), now);
  const huge = Array.from({ length: 994 }, (_, i) => ({
    slug: `company-${i}`,
    name: "X".repeat(1024),
    archived: false,
  }));
  db(big, (d) => {
    const q = d.query(
      "INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u',?,'grant','reader','agent:synthetic')",
    );
    for (const c of huge) q.run(c.slug);
  });
  let before = image(big);
  assert.equal(run(big, request({ catalog: huge })).ok, false);
  assert.equal(image(big), before);
  note("serialized-output-bound-no-partial-context");
  const cardinal = setup(fixture("context-membership-count"), now);
  db(cardinal, (d) => {
    const q = d.query(
      "INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u',?,'revoke',NULL,'agent:synthetic')",
    );
    for (let i = 0; i < 994; i++) q.run(`missing-${i}`);
  });
  before = image(cardinal);
  let cardinalResult = run(cardinal, request());
  assert(cardinalResult.ok);
  assert.deepEqual(cardinalResult.context, expected());
  assert.equal(image(cardinal), before);
  note("exact1000-latest-membership-slugs-including-revoked-pass");
  db(cardinal, (d) =>
    d.exec(
      "INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','overflow-dangling','grant','reader','agent:synthetic')",
    ),
  );
  before = image(cardinal);
  cardinalResult = run(cardinal, request());
  assert.equal(cardinalResult.ok, false);
  assert(!cardinalResult.context);
  assert.equal(image(cardinal), before);
  note(
    "1001-latest-membership-slugs-refuse-independent-of-catalog-output-bound",
  );
  const f = setup(fixture("context-held-writer"), now);
  before = image(f);
  const p = start(f, request(), "context-read");
  await marker(p, f);
  const competitor = new Database(f.control);
  try {
    assert.throws(() => competitor.exec("BEGIN IMMEDIATE"));
    note("writer-held-through-projection-excludes-revocation");
    p.child.stdin.write("\n");
    p.child.stdin.end();
    const r = await done(p);
    assert(r.ok);
    assert.deepEqual(r.context, expected());
    assert.equal(image(f), before);
    competitor.exec(
      "INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','disable',NULL,'agent:synthetic')",
    );
    before = image(f);
    assert.equal(run(f, request()).ok, false);
    assert.equal(image(f), before);
    note("next-request-observes-revocation-after-lock-release");
  } finally {
    competitor.close();
  }
  const busy = setup(fixture("context-writer-busy"), now);
  before = image(busy);
  const blocker = new Database(busy.control);
  try {
    blocker.exec("BEGIN IMMEDIATE");
    const r = run(busy, request());
    assert.equal(r.ok, false);
    assert.equal(r.code, "control_busy_or_unavailable");
    assert(!r.context);
    blocker.exec("ROLLBACK");
    assert.equal(image(busy), before);
    note("actual-control-writer-contention-withholds-context");
  } finally {
    blocker.close();
  }
  return [token, key];
}
export async function verifyHttpWorkspaceContext(c: any) {
  const { fixture, start, key, now, note } = c;
  const cookie = (
    await serializeSignedCookie("better-auth.session_token", token, key)
  ).split(";")[0]!;
  const record = (id: string) => note(`workspace-context/${id}`);
  const get = async (s: any, extra: any = {}) => {
    const r = await fetch(`${s.origin}/api/me${extra.query ?? ""}`, {
      method: extra.method ?? "GET",
      headers: { cookie, ...extra.headers },
    });
    assert.equal(r.headers.getSetCookie().length, 0);
    assert.equal(r.headers.get("cache-control"), "no-store");
    assert.equal(r.headers.get("referrer-policy"), "no-referrer");
    const body = await r.text();
    return {
      status: r.status,
      body,
      data: body ? JSON.parse(body) : null,
      headers: r.headers,
    };
  };
  for (const mode of ["WAL", "DELETE"]) {
    const f = setup(fixture(`http-context-${mode}`, mode), now),
      before = image(f),
      s = await start(f, { workspaceContext: catalog });
    try {
      const r = await get(s);
      assert.equal(r.status, 200);
      assert.deepEqual(r.data, expected());
      assert.deepEqual(r.data, source(f, catalog));
      assert.equal(image(f), before);
      assert(!r.body.includes(token));
      record(`${mode}-source-exact-read-only-current-human-context`);
      db(f, (d) =>
        d.exec(
          "INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','z-last','revoke',NULL,'agent:synthetic')",
        ),
      );
      const revoked = image(f),
        updated = expected();
      updated.companies = updated.companies.filter((c) => c.slug !== "z-last");
      const next = await get(s);
      assert.equal(next.status, 200);
      assert.deepEqual(next.data, updated);
      assert.equal(image(f), revoked);
      record(`${mode}-current-membership-revocation-without-stale-cache`);
    } finally {
      await s.stop();
    }
  }
  for (const [id, extra, status] of [
    ["query", { query: "?userId=foreign" }, 400],
    ["foreign-origin", { headers: { origin: "https://foreign.invalid" } }, 403],
    ["fetch-site", { headers: { "sec-fetch-site": "cross-site" } }, 403],
    ["service", { headers: { authorization: "Bearer private-service" } }, 401],
    [
      "service-key",
      { headers: { "x-rentemester-service-key": "private-service" } },
      401,
    ],
    ["missing-cookie", { headers: { cookie: "" } }, 401],
    [
      "bare-cookie",
      { headers: { cookie: `${cookie}; better-auth.session_token` } },
      401,
    ],
    ["duplicate-cookie", { headers: { cookie: `${cookie}; ${cookie}` } }, 401],
    [
      "bare-before-valid",
      { headers: { cookie: `better-auth.session_token; ${cookie}` } },
      401,
    ],
    [
      "secure-bare-before-valid",
      { headers: { cookie: `__Secure-better-auth.session_token; ${cookie}` } },
      401,
    ],
    [
      "secure-bare-after-valid",
      { headers: { cookie: `${cookie}; __Secure-better-auth.session_token` } },
      401,
    ],
    [
      "pending-cookie",
      { headers: { cookie: cookie.replace("session_token", "two_factor") } },
      401,
    ],
    ["encoding", { headers: { "content-encoding": "gzip" } }, 415],
  ] as any[]) {
    const f = setup(fixture(`http-context-refuse-${id}`, "DELETE"), now),
      before = image(f),
      s = await start(f, { workspaceContext: catalog });
    try {
      const r = await get(s, extra);
      assert.equal(r.status, status, id);
      assert(!r.data?.user, id);
      assert.equal(image(f), before, id);
      record(`${id}-no-context-or-write`);
    } finally {
      await s.stop();
    }
  }
  for (const [id, mutation, config, status] of [
    ["revoked", "DELETE FROM session", {}, 401],
    [
      "disabled",
      "INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','disable',NULL,'agent:synthetic')",
      {},
      401,
    ],
    ["bad-user", "UPDATE user SET email='' WHERE id='u'", {}, 503],
    ["bad-catalog", "", { workspaceContext: [catalog[0], catalog[0]] }, 503],
    ["wrong-workspace", "", { catalogWorkspace: "other" }, 401],
  ] as any[]) {
    const f = setup(fixture(`http-context-${id}`, "DELETE"), now);
    if (mutation) db(f, (d) => d.exec(mutation));
    const before = image(f),
      s = await start(f, { workspaceContext: catalog, ...config });
    try {
      const r = await get(s);
      assert.equal(r.status, status, id);
      assert(!r.data?.user);
      assert.equal(image(f), before);
      record(`${id}-native-refusal-no-context`);
    } finally {
      await s.stop();
    }
  }
  const f = setup(fixture("http-context-rate", "DELETE"), now),
    before = image(f),
    s = await start(f, { workspaceContext: catalog });
  try {
    for (let i = 0; i < 12; i++) {
      const r = await get(s, { method: "HEAD" });
      assert.equal(r.status, 405);
      assert.equal(r.headers.get("allow"), "GET");
      assert.equal(r.body, "");
    }
    for (let i = 0; i < 10; i++)
      assert.equal(
        (
          await get(s, {
            headers: { "x-forwarded-for": `198.51.100.${i + 1}` },
          })
        ).status,
        200,
      );
    const rate = await get(s);
    assert.equal(rate.status, 429);
    assert.equal(rate.headers.get("retry-after"), "60");
    assert.equal(image(f), before);
    record("HEAD-no-rate-charge-and-direct-peer-bound-ignores-forwarded");
  } finally {
    await s.stop();
  }
  const missing = setup(fixture("http-context-missing", "DELETE"), now),
    server = await start(missing);
  try {
    assert.equal((await get(server)).status, 404);
    record("missing-handler-no-legacy-fallback");
  } finally {
    await server.stop();
  }
  const busy = setup(fixture("http-context-busy", "DELETE"), now),
    beforeBusy = image(busy),
    b = new Database(busy.control),
    busyServer = await start(busy, { workspaceContext: catalog });
  try {
    b.exec("BEGIN IMMEDIATE");
    const r = await get(busyServer);
    assert.equal(r.status, 503);
    assert(!r.data?.user);
    b.exec("ROLLBACK");
    assert.equal(image(busy), beforeBusy);
    record("actual-writer-contention-no-context");
  } finally {
    b.close();
    await busyServer.stop();
  }
  return [token, cookie, key];
}
