/** Owner-only native lifecycle, source interoperability, secret-free recovery and exact rollback. */
import assert from "node:assert/strict";
import { Database } from "bun:sqlite";
import { defaultKeyHasher } from "../../node_modules/@better-auth/api-key/dist/index.mjs";
import { serializeSignedCookie } from "../../node_modules/better-call/dist/cookies.mjs";
import { createBetterAuthRuntime } from "../../src/server/better-auth";
import {
  createWorkspaceServicePrincipal,
  rotateWorkspaceServiceCredential,
} from "../../src/core/workspace-service-principals";
const ownerToken = "lifecycle-private-owner-session";
let serial = 0;
const uuid = () =>
  `00000000-0000-4000-8000-${String(++serial).padStart(12, "0")}`;
function db<T>(f: any, fn: (d: Database) => T): T {
  const d = new Database(f.control, { safeIntegers: f.safe === true });
  try {
    return fn(d);
  } finally {
    d.close();
  }
}
const q = (f: any, s: string) => db(f, (d) => d.query(s).all() as any[]);
const sql = (f: any, s: string) => db(f, (d) => d.exec(s));
const tables = (f: any) =>
  db({ ...f, safe: true }, (d) =>
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
const image = (f: any) =>
  JSON.stringify(tables(f), (_k, v) =>
    typeof v === "bigint" ? v.toString() : v,
  );
function unchanged(before: any, after: any, allowed: string[]) {
  for (const k of Object.keys(before))
    if (!allowed.includes(k)) assert.deepEqual(after[k], before[k], k);
}
const opts = {
  secret: "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
  baseURL: "https://example.invalid",
  trustedOrigins: ["https://example.invalid"],
  useSecureCookies: true,
};
function setup(c: any, prefix: string) {
  const now = Math.floor((Date.now() + 60000) / 1000) * 1000,
    iso = (n: number) => new Date(n).toISOString();
  function fresh(id: string, mode = "DELETE", age = 1000) {
    const f = c.fixture(`${prefix}-${id}`, mode);
    db(f, (d) => {
      d.exec(
        "INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','workspace_owner','agent:synthetic')",
      );
      d.query(
        "INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt) VALUES('lifecycle-owner','u',?,?,?,?)",
      ).run(ownerToken, iso(now - age), iso(now - age), iso(now + 3600000));
    });
    return f;
  }
  return { now, iso, fresh };
}
export async function verifyServiceLifecycle(c: any) {
  const { now, iso, fresh } = setup(c, "lifecycle-core"),
    secrets: string[] = [],
    note = (id: string) => c.record(`service-lifecycle/${id}`);
  const request = (action: string, extra: any = {}) => ({
    mode: "service-lifecycle",
    token: ownerToken,
    now,
    action,
    ...extra,
  });
  const run = (f: any, action: string, extra: any = {}) =>
    c.run(f, request(action, extra));
  function issued(r: any) {
    assert(r.ok);
    const v = r.receipt;
    assert(v.ok);
    assert.match(v.secret, /^rms_[a-zA-Z]{64}$/);
    secrets.push(v.secret);
    return v;
  }
  const create = (f: any, extra: any = {}) =>
    issued(
      run(f, "create", {
        displayName: "Synthetic automation",
        operationId: uuid(),
        ...extra,
      }),
    );
  for (const mode of ["WAL", "DELETE"]) {
    const f = fresh(`normal-${mode}`, mode),
      before = tables(f),
      operation = uuid(),
      one = create(f, { operationId: operation });
    assert.equal(run(f, "list").receipt.principals.length, 1);
    const key = q(f, "SELECT * FROM apikey")[0];
    assert.equal(key.key, await defaultKeyHasher(one.secret));
    assert.equal(key.prefix, "rms_");
    assert.equal(key.start, one.secret.slice(0, 6));
    assert.equal(key.requestCount, 0);
    assert.equal(key.rateLimitMax, 600);
    assert.equal(key.rateLimitTimeWindow, 60000);
    assert.equal(key.expiresAt, iso(now + 7776000000));
    const user = q(
      f,
      `SELECT * FROM user WHERE id='${one.serviceAccountId}'`,
    )[0];
    assert.equal(user.emailVerified, 0);
    assert.equal(user.twoFactorEnabled, 0);
    assert.equal(user.email, `${one.serviceAccountId}@service.invalid`);
    for (const table of [
      "account",
      "session",
      "rm_workspace_user_access_events",
      "rm_company_membership_events",
    ])
      assert.equal(
        q(
          f,
          `SELECT count(*) n FROM ${table} WHERE ${["account", "session"].includes(table) ? "userId" : "user_id"}='${one.serviceAccountId}'`,
        )[0].n,
        0,
      );
    unchanged(before, tables(f), [
      "user",
      "apikey",
      "rm_workspace_service_principals",
      "rm_workspace_service_principal_events",
      "rm_workspace_service_principal_operation_events",
    ]);
    note(
      `${mode}-atomic-source-compatible-key-user-registry-events-no-implicit-authority`,
    );
    const events = q(
      f,
      "SELECT event_type,actor FROM rm_workspace_service_principal_events",
    );
    assert.deepEqual(events, [
      { event_type: "created", actor: "user:u" },
      { event_type: "credential_issued", actor: "user:u" },
    ]);
    const ops = q(
      f,
      "SELECT operation_status,actor FROM rm_workspace_service_principal_operation_events",
    );
    assert.deepEqual(ops, [
      { operation_status: "pending", actor: "user:u" },
      { operation_status: "completed", actor: "user:u" },
    ]);
    note(`${mode}-precise-current-owner-audit-and-operation-order`);
    let snapshot = image(f),
      list = run(f, "list");
    assert(list.ok);
    assert.equal(list.receipt.principals[0].activeCredentialCount, 1);
    assert.deepEqual(list.receipt.principals[0].credentials, [
      {
        credentialId: one.credentialId,
        enabled: true,
        expiresAt: key.expiresAt,
      },
    ]);
    assert.equal(image(f), snapshot);
    assert(!JSON.stringify(list).includes(one.secret));
    assert(!JSON.stringify(list).includes(key.key));
    note(`${mode}-pure-bounded-owner-inventory-no-secret-or-hash`);
    const retry = run(f, "create", {
      displayName: "Synthetic automation",
      operationId: operation,
    });
    assert.equal(retry.ok, false);
    assert.equal(retry.code, "operation_reused");
    assert.equal(image(f), snapshot);
    note(`${mode}-same-operation-never-reissues-or-rediscloses`);
    const recovered = run(f, "recover", { operationId: operation });
    assert(recovered.ok);
    assert.equal(recovered.receipt.status, "completed");
    assert.equal(recovered.receipt.recovered, false);
    assert.equal(recovered.receipt.replacementCredentialId, one.credentialId);
    assert.equal(recovered.receipt.credentialId, one.credentialId);
    assert(!recovered.receipt.secret);
    assert.equal(image(f), snapshot);
    note(`${mode}-completed-lost-reply-nonsecret-targeted-readback`);
    const d = new Database(f.control),
      auth = createBetterAuthRuntime(d, opts);
    try {
      assert.equal(
        (
          await (auth.api as any).verifyApiKey({
            body: { key: one.secret, configId: "workspace-service-principal" },
          })
        ).valid,
        true,
      );
    } finally {
      d.close();
    }
    note(`${mode}-actual-installed-source-verifies-native-issued-key`);
    const rotation = uuid(),
      two = issued(
        run(f, "rotate", {
          operationId: rotation,
          serviceAccountId: one.serviceAccountId,
          credentialId: one.credentialId,
          expiresInSeconds: 86400,
        }),
      );
    assert.equal(two.serviceAccountId, one.serviceAccountId);
    assert.equal(
      q(f, `SELECT enabled FROM apikey WHERE id='${one.credentialId}'`)[0]
        .enabled,
      0,
    );
    assert.equal(
      q(f, `SELECT expiresAt FROM apikey WHERE id='${two.credentialId}'`)[0]
        .expiresAt,
      iso(now + 86400000),
    );
    assert.equal(
      q(f, "SELECT count(*) n FROM rm_workspace_service_principals")[0].n,
      1,
    );
    note(
      `${mode}-rotation-one-new-key-old-disabled-stable-subject-custom-expiry`,
    );
    const sd = new Database(f.control),
      sa = createBetterAuthRuntime(sd, opts);
    try {
      assert.equal(
        (
          await (sa.api as any).verifyApiKey({
            body: { key: one.secret, configId: "workspace-service-principal" },
          })
        ).valid,
        false,
      );
      assert.equal(
        (
          await (sa.api as any).verifyApiKey({
            body: { key: two.secret, configId: "workspace-service-principal" },
          })
        ).valid,
        true,
      );
    } finally {
      sd.close();
    }
    note(`${mode}-source-old-refuses-and-new-accepts-after-native-rotation`);
    const cleanup = run(f, "revoke", {
      operationId: uuid(),
      serviceAccountId: two.serviceAccountId,
      credentialId: two.credentialId,
    });
    assert(cleanup.ok);
    assert.equal(cleanup.receipt.revoked, true);
    assert.equal(q(f, "SELECT count(*) n FROM apikey WHERE enabled=1")[0].n, 0);
    note(`${mode}-explicit-lost-secret-target-cleanup-without-secret-readback`);
    snapshot = image(f);
    list = run(f, "list");
    assert.equal(list.receipt.principals[0].activeCredentialCount, 0);
    assert.equal(list.receipt.principals[0].credentials.length, 2);
    assert.equal(image(f), snapshot);
    note(`${mode}-disabled-credentials-remain-visible-for-owner-inspection`);
  }
  for (const [id, mutation, extra] of [
    [
      "member",
      "INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic')",
      {},
    ],
    [
      "disabled-owner",
      "INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','disable',NULL,'agent:synthetic')",
      {},
    ],
    ["unverified", "UPDATE user SET emailVerified=0 WHERE id='u'", {}],
    ["factor", "UPDATE twoFactor SET verified=0 WHERE userId='u'", {}],
    ["revoked", "DELETE FROM session", {}],
    ["wrong-workspace", "", { proofWorkspace: "other" }],
    [
      "wrong-key",
      "",
      { proofKey: "YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXoxMjM0NTY" },
    ],
    ["bad-name", "", { displayName: "\n" }],
    ["bad-uuid", "", { operationId: "not-uuid" }],
    ["bad-expiry", "", { expiresInSeconds: 86399 }],
    ["lateSQL", "", { failSql: true }],
  ] as any[]) {
    const f = fresh(`refuse-${id}`);
    if (mutation) sql(f, mutation);
    const before = image(f),
      r = run(f, "create", {
        displayName: "Synthetic",
        operationId: uuid(),
        ...extra,
      });
    assert.equal(r.ok, false, id);
    assert(!r.receipt);
    assert.equal(image(f), before, id);
    note(`${id}-no-orphan-key-event-or-secret`);
  }
  const stale = fresh("stale", "DELETE", 600000);
  let before = image(stale);
  assert(run(stale, "list").ok);
  assert.equal(
    run(stale, "create", { displayName: "Synthetic", operationId: uuid() }).ok,
    false,
  );
  assert.equal(image(stale), before);
  note("Current-list-Fresh-mutation-exact-ten-minute-boundary");
  const target = fresh("target");
  const own = create(target),
    foreign = create(target);
  before = image(target);
  assert.equal(
    run(target, "rotate", {
      serviceAccountId: own.serviceAccountId,
      credentialId: foreign.credentialId,
      operationId: uuid(),
    }).ok,
    false,
  );
  assert.equal(image(target), before);
  note("foreign-target-never-disabled-or-replacement-issued");
  const count = fresh("inventory-expired-null");
  const co = create(count);
  sql(
    count,
    `UPDATE apikey SET expiresAt='2020-01-01T00:00:00.000Z',enabled=NULL WHERE id='${co.credentialId}'`,
  );
  before = image(count);
  const ci = run(count, "list");
  assert.equal(ci.receipt.principals[0].activeCredentialCount, 1);
  assert.equal(ci.receipt.principals[0].credentials[0].enabled, null);
  assert.equal(image(count), before);
  note(
    "source-compatible-enabled-count-includes-expired-and-null-not-live-auth",
  );
  // Actual source crash windows and native reconciliation. No external provider worker survives.
  for (const [action, point] of [
    ["create", "before-provider-create"],
    ["create", "after-provider-create"],
    ["rotate", "after-provider-create"],
    ["rotate", "after-old-disabled"],
  ] as const) {
    const f = fresh(`legacy-${action}-${point}`),
      d = new Database(f.control),
      auth = createBetterAuthRuntime(d, opts),
      operation = uuid();
    let old: any;
    try {
      if (action === "create")
        await assert.rejects(
          createWorkspaceServicePrincipal(d, auth, {
            displayName: "Synthetic legacy",
            actor: "agent:synthetic",
            operationId: operation,
            crashAt: point,
          }),
        );
      else {
        old = await createWorkspaceServicePrincipal(d, auth, {
          displayName: "Synthetic legacy",
          actor: "agent:synthetic",
        });
        secrets.push(old.secret);
        await assert.rejects(
          rotateWorkspaceServiceCredential(d, auth, {
            serviceAccountId: old.serviceAccountId,
            credentialId: old.credentialId,
            actor: "agent:synthetic",
            operationId: operation,
            crashAt: point,
          }),
        );
      }
    } finally {
      d.close();
    }
    const oldEnabled = old
      ? q(f, `SELECT enabled FROM apikey WHERE id='${old.credentialId}'`)[0]
          .enabled
      : null;
    const r = run(f, "recover", { operationId: operation });
    assert(r.ok);
    assert.equal(r.receipt.status, "recovered");
    assert.equal(r.receipt.recovered, true);
    assert(!r.receipt.secret);
    const marker = q(
      f,
      `SELECT provider_operation_marker m FROM rm_workspace_service_principal_operation_events WHERE operation_id='${operation}' LIMIT 1`,
    )[0].m;
    assert.equal(
      q(
        f,
        `SELECT count(*) n FROM apikey WHERE name='${marker}' AND enabled=1`,
      )[0].n,
      0,
    );
    if (old)
      assert.equal(
        q(f, `SELECT enabled FROM apikey WHERE id='${old.credentialId}'`)[0]
          .enabled,
        oldEnabled,
      );
    note(
      `actual-source-${action}-${point}-marker-cleanup-never-reactivates-old`,
    );
    before = image(f);
    const again = run(f, "recover", { operationId: operation });
    assert(again.ok);
    assert.equal(again.receipt.status, "recovered");
    assert.equal(image(f), before);
    note(`${action}-${point}-terminal-recovery-never-reissues-or-mutates`);
  }
  // Recovery fixtures use source-compatible UTC or ISO timestamps and append-only histories.
  function historyFixture(
    id: string,
    state = "pending",
    age = 30000,
    kind = "create",
    format = "sql",
  ) {
    const f = fresh(`history-${id}`),
      one = create(f),
      operation = uuid(),
      marker = "1234567890abcdef1234567890abcdef";
    const at =
      format === "sql"
        ? iso(now - age)
            .replace("T", " ")
            .slice(0, 19)
        : iso(now - age);
    db(f, (d) => {
      d.query(
        "INSERT INTO rm_workspace_service_principal_operation_events(operation_id,operation_type,operation_status,user_id,credential_id,provider_operation_marker,actor,created_at) VALUES(?,?,'pending',?,?,?,'agent:synthetic',?)",
      ).run(
        operation,
        kind,
        one.serviceAccountId,
        kind === "create" ? null : one.credentialId,
        marker,
        at,
      );
      if (state !== "pending")
        d.query(
          "INSERT INTO rm_workspace_service_principal_operation_events(operation_id,operation_type,operation_status,user_id,credential_id,provider_operation_marker,recovery_claim_id,actor,created_at) VALUES(?,?,?,?,?,?,?,'agent:synthetic',?)",
        ).run(
          operation,
          kind,
          state,
          one.serviceAccountId,
          kind === "create" ? null : one.credentialId,
          marker,
          state === "recovering" ? uuid() : null,
          at,
        );
    });
    return { f, one, operation, marker };
  }
  for (const [id, state, age, format, expected] of [
    ["fresh-sql", "recovering", 29000, "sql", "recovering"],
    ["fresh-iso", "recovering", 29999, "iso", "recovering"],
    ["exact-lease", "recovering", 30000, "sql", "recovered"],
    ["failed", "failed", 30000, "sql", "recovered"],
  ] as any[]) {
    const { f, operation } = historyFixture(id, state, age, "create", format);
    before = image(f);
    const r = run(f, "recover", { operationId: operation });
    assert(r.ok, id);
    assert.equal(r.receipt.status, expected, id);
    if (expected === "recovering") assert.equal(image(f), before);
    else {
      const replay = run(f, "recover", { operationId: operation });
      assert(replay.ok);
    }
    note(`${id}-strict-source-timestamp-and-recovery-state`);
  }
  const rev = historyFixture("revoke", "pending", 30000, "revoke");
  assert.equal(
    run(rev.f, "recover", { operationId: rev.operation }).receipt.recovered,
    true,
  );
  assert.equal(
    q(rev.f, `SELECT enabled FROM apikey WHERE id='${rev.one.credentialId}'`)[0]
      .enabled,
    0,
  );
  assert.equal(
    run(rev.f, "recover", { operationId: rev.operation }).receipt.recovered,
    false,
  );
  note("revoke-recovery-first-true-terminal-completed-readback-false");
  for (const id of [
    "marker-reuse",
    "future",
    "bad-transition",
    "null-completed",
    "changed-owner",
  ]) {
    const h = historyFixture(id);
    if (id === "marker-reuse") {
      const other = uuid();
      sql(
        h.f,
        `INSERT INTO rm_workspace_service_principal_operation_events(operation_id,operation_type,operation_status,user_id,provider_operation_marker,actor,created_at) VALUES('${other}','create','pending','${h.one.serviceAccountId}','${h.marker}','agent:synthetic','${iso(now)}')`,
      );
    } else if (id === "future") {
      sql(
        h.f,
        `INSERT INTO rm_workspace_service_principal_operation_events(operation_id,operation_type,operation_status,user_id,provider_operation_marker,recovery_claim_id,actor,created_at) VALUES('${h.operation}','create','recovering','${h.one.serviceAccountId}','${h.marker}','${uuid()}','agent:synthetic','${iso(now + 1)}')`,
      );
    } else if (id === "changed-owner") {
      const other = create(h.f);
      sql(
        h.f,
        `INSERT INTO rm_workspace_service_principal_operation_events(operation_id,operation_type,operation_status,user_id,provider_operation_marker,actor,created_at) VALUES('${h.operation}','create','failed','${other.serviceAccountId}','${h.marker}','agent:synthetic','${iso(now)}')`,
      );
    } else {
      sql(
        h.f,
        `INSERT INTO rm_workspace_service_principal_operation_events(operation_id,operation_type,operation_status,user_id,provider_operation_marker,actor,created_at) VALUES('${h.operation}','create','${id === "null-completed" ? "completed" : "recovered"}','${h.one.serviceAccountId}','${h.marker}','agent:synthetic','${iso(now)}')`,
      );
    }
    before = image(h.f);
    assert.equal(
      run(h.f, "recover", { operationId: h.operation }).ok,
      false,
      id,
    );
    assert.equal(image(h.f), before, id);
    note(`${id}-refuses-cleanup-before-any-key-event-effect`);
  }
  const compensation = fresh("actual-failed-compensation"),
    cd = new Database(compensation.control),
    ca = createBetterAuthRuntime(cd, opts),
    cop = uuid();
  let base: any;
  try {
    base = await createWorkspaceServicePrincipal(cd, ca, {
      displayName: "Synthetic compensation",
      actor: "agent:synthetic",
    });
    secrets.push(base.secret);
    const broken = {
      api: {
        createApiKey: (ca.api as any).createApiKey,
        updateApiKey: async () => {
          throw new Error("synthetic provider unavailable");
        },
      },
    };
    await assert.rejects(
      rotateWorkspaceServiceCredential(cd, broken as any, {
        serviceAccountId: base.serviceAccountId,
        credentialId: base.credentialId,
        actor: "agent:synthetic",
        operationId: cop,
      }),
    );
  } finally {
    cd.close();
  }
  assert.equal(
    q(
      compensation,
      `SELECT operation_status s FROM rm_workspace_service_principal_operation_events WHERE operation_id='${cop}' ORDER BY id DESC LIMIT 1`,
    )[0].s,
    "failed",
  );
  assert.equal(
    q(compensation, "SELECT count(*) n FROM apikey WHERE enabled=1")[0].n,
    2,
  );
  assert.equal(
    run(compensation, "recover", { operationId: cop }).receipt.status,
    "recovered",
  );
  assert.equal(
    q(compensation, "SELECT count(*) n FROM apikey WHERE enabled=1")[0].n,
    1,
  );
  assert.equal(
    q(
      compensation,
      `SELECT enabled FROM apikey WHERE id='${base.credentialId}'`,
    )[0].enabled,
    1,
  );
  note(
    "actual-source-failed-compensation-cleanup-disables-only-unreceived-replacement",
  );
  const missing = historyFixture("missing-revoke", "pending", 30000, "revoke");
  sql(missing.f, `DELETE FROM apikey WHERE id='${missing.one.credentialId}'`);
  const eventBefore = q(
    missing.f,
    "SELECT count(*) n FROM rm_workspace_service_principal_events",
  )[0].n;
  assert.equal(
    run(missing.f, "recover", { operationId: missing.operation }).receipt
      .status,
    "completed",
  );
  assert.equal(
    q(
      missing.f,
      "SELECT count(*) n FROM rm_workspace_service_principal_events",
    )[0].n,
    eventBefore,
  );
  note("deleted-revoke-target-completes-without-fabricated-key-event");
  const mismatch = historyFixture("foreign-revoke", "pending", 30000, "revoke");
  sql(
    mismatch.f,
    `UPDATE apikey SET configId='other' WHERE id='${mismatch.one.credentialId}'`,
  );
  before = image(mismatch.f);
  assert.equal(
    run(mismatch.f, "recover", { operationId: mismatch.operation }).ok,
    false,
  );
  assert.equal(image(mismatch.f), before);
  note("recovery-foreign-config-target-never-disabled-or-completed");
  const invalidRevoke = historyFixture(
    "invalid-revoke-state",
    "failed",
    30000,
    "revoke",
  );
  sql(
    invalidRevoke.f,
    `INSERT INTO rm_workspace_service_principal_operation_events(operation_id,operation_type,operation_status,user_id,credential_id,provider_operation_marker,actor,created_at) VALUES('${invalidRevoke.operation}','revoke','recovered','${invalidRevoke.one.serviceAccountId}','${invalidRevoke.one.credentialId}','${invalidRevoke.marker}','agent:synthetic','${iso(now)}')`,
  );
  before = image(invalidRevoke.f);
  assert.equal(
    run(invalidRevoke.f, "recover", { operationId: invalidRevoke.operation })
      .ok,
    false,
  );
  assert.equal(image(invalidRevoke.f), before);
  note("unproducible-revoke-recovered-history-refused");
  const recoveryMax = fresh("recovery-near-max"),
    rm = create(recoveryMax),
    rid = uuid();
  sql(
    recoveryMax,
    `INSERT INTO rm_workspace_service_principal_operation_events(id,operation_id,operation_type,operation_status,user_id,provider_operation_marker,actor,created_at) VALUES(9223372036854775806,'${rid}','create','pending','${rm.serviceAccountId}','abcdef1234567890abcdef1234567890','agent:synthetic','${iso(now)}')`,
  );
  before = image(recoveryMax);
  assert.equal(run(recoveryMax, "recover", { operationId: rid }).ok, false);
  assert.equal(image(recoveryMax), before);
  note("recovery-MAXminus1-two-appends-roll-back-without-random-ID");
  const inventoryMany = fresh("inventory-principal-bound");
  db(inventoryMany, (d) => {
    d.exec("BEGIN");
    const user = d.query(
        "INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES(?,'Synthetic',?,0,0,?,?)",
      ),
      reg = d.query(
        "INSERT INTO rm_workspace_service_principals(user_id,display_name,created_at) VALUES(?,'Synthetic',?)",
      );
    for (let i = 0; i < 1001; i++) {
      const id = `service_synthetic_${i}`;
      user.run(id, `${id}@service.invalid`, iso(now), iso(now));
      reg.run(id, iso(now));
    }
    d.exec("COMMIT");
  });
  before = image(inventoryMany);
  assert.equal(run(inventoryMany, "list").ok, false);
  assert.equal(image(inventoryMany), before);
  note("inventory1001-principals-refuses-without-partial-output");
  for (const [id, n, long] of [
    ["credentials", 1001, false],
    ["serialized", 300, true],
  ] as const) {
    const f = fresh(`inventory-${id}`),
      one = create(f),
      hash = await defaultKeyHasher(one.secret);
    db(f, (d) => {
      d.exec("BEGIN");
      const insert = d.query(
        "INSERT INTO apikey(id,configId,referenceId,key,enabled,createdAt,updatedAt) VALUES(?,'workspace-service-principal',?,?,1,?,?)",
      );
      for (let i = 0; i < n; i++)
        insert.run(
          `key_${i}${long ? "x".repeat(4080) : ""}`,
          one.serviceAccountId,
          hash,
          iso(now),
          iso(now),
        );
      d.exec("COMMIT");
    });
    before = image(f);
    assert.equal(run(f, "list").ok, false);
    assert.equal(image(f), before);
    note(`inventory-${id}-bound-no-partial-or-secret-output`);
  }
  for (const mode of ["WAL", "DELETE"]) {
    for (const action of ["rotate", "revoke"]) {
      const f = fresh(`race-${action}-${mode}`, mode),
        one = create(f);
      const start = () =>
        c.start(
          f,
          request(action, {
            serviceAccountId: one.serviceAccountId,
            credentialId: one.credentialId,
            operationId: uuid(),
          }),
        );
      const a = start(),
        b = start();
      a.child.stdin.end();
      b.child.stdin.end();
      const rr = await Promise.all([c.done(a), c.done(b)]),
        accepted = rr.filter((r) => r.ok);
      assert(accepted.length >= 1);
      if (action === "rotate") for (const r of accepted) issued(r);
      assert.equal(
        q(f, "SELECT count(*) n FROM apikey")[0].n,
        action === "rotate" ? 1 + accepted.length : 1,
      );
      assert.equal(
        q(f, `SELECT enabled FROM apikey WHERE id='${one.credentialId}'`)[0]
          .enabled,
        0,
      );
      assert.equal(
        q(f, "SELECT count(*) n FROM rm_workspace_service_principals")[0].n,
        1,
      );
      assert.equal(
        q(
          f,
          "SELECT count(*) n FROM rm_workspace_service_principal_operation_events WHERE operation_status='completed'",
        )[0].n,
        1 + accepted.length,
      );
      note(
        `${mode}-concurrent-${action}-exact-success-count-no-partial-key-or-operation`,
      );
    }
  }
  const max = "9223372036854775807",
    near = "9223372036854775806";
  for (const table of [
    "rm_workspace_service_principal_events",
    "rm_workspace_service_principal_operation_events",
  ]) {
    const f = fresh(`near-max-${table}`),
      one = create(f);
    if (table.endsWith("operation_events"))
      sql(
        f,
        `INSERT INTO ${table}(id,operation_id,operation_type,operation_status,user_id,provider_operation_marker,actor,created_at) VALUES(${near},'${uuid()}','create','pending','${one.serviceAccountId}','abcdef1234567890abcdef1234567890','agent:synthetic','${iso(now)}')`,
      );
    else
      sql(
        f,
        `INSERT INTO ${table}(id,user_id,event_type,actor,created_at) VALUES(${near},'${one.serviceAccountId}','created','agent:synthetic','${iso(now)}')`,
      );
    before = image(f);
    assert.equal(
      run(f, "create", {
        displayName: "Synthetic overflow",
        operationId: uuid(),
      }).ok,
      false,
    );
    assert.equal(image(f), before);
    note(`${table}-MAXminus1-multiappend-rolls-back-no-random-order`);
  }
  const poisoned = historyFixture("poison");
  sql(
    poisoned.f,
    `INSERT INTO rm_workspace_service_principal_operation_events(id,operation_id,operation_type,operation_status,user_id,provider_operation_marker,actor,created_at) VALUES(${max},'${uuid()}','create','pending','${poisoned.one.serviceAccountId}','abcdef1234567890abcdef1234567890','agent:synthetic','${iso(now)}')`,
  );
  sql(
    poisoned.f,
    `INSERT INTO rm_workspace_service_principal_operation_events(operation_id,operation_type,operation_status,user_id,provider_operation_marker,actor,created_at) VALUES('${uuid()}','create','pending','${poisoned.one.serviceAccountId}','abcdef1234567890abcdef1234567891','agent:synthetic','${iso(now)}')`,
  );
  before = image(poisoned.f);
  assert.equal(
    run(poisoned.f, "recover", { operationId: poisoned.operation }).ok,
    false,
  );
  assert.equal(image(poisoned.f), before);
  note("MAX-and-auto-lower-poison-never-trusted-as-current-history");
  for (const mode of ["WAL", "DELETE"]) {
    const f = fresh(`concurrent-${mode}`, mode),
      operation = uuid();
    const a = c.start(
        f,
        request("create", {
          displayName: "Concurrent synthetic",
          operationId: operation,
        }),
      ),
      b = c.start(
        f,
        request("create", {
          displayName: "Concurrent synthetic",
          operationId: operation,
        }),
      );
    a.child.stdin.end();
    b.child.stdin.end();
    const results = await Promise.all([c.done(a), c.done(b)]);
    assert.equal(results.filter((r) => r.ok).length, 1);
    issued(results.find((r) => r.ok));
    assert.equal(q(f, "SELECT count(*) n FROM apikey")[0].n, 1);
    assert.equal(
      q(f, "SELECT count(*) n FROM rm_workspace_service_principals")[0].n,
      1,
    );
    note(`${mode}-same-operation-concurrent-one-issuance`);
    const killed = fresh(`kill-${mode}`, mode);
    before = image(killed);
    const p = c.start(
      killed,
      request("create", {
        displayName: "Synthetic killed",
        operationId: uuid(),
      }),
      "lifecycle-before-commit",
    );
    await c.marker(p, killed);
    const competitor = new Database(killed.control);
    try {
      assert.throws(() => competitor.exec("BEGIN IMMEDIATE"));
      p.child.kill();
      await p.child.exited;
      clearTimeout(p.timer);
      assert.equal(image(killed), before);
    } finally {
      competitor.close();
    }
    note(
      `${mode}-held-writer-excludes-owner-revocation-and-death-rolls-back-all`,
    );
  }
  const unknown = fresh("unknown", "DELETE"),
    reader = new Database(unknown.control);
  try {
    reader.exec("BEGIN");
    reader.query("SELECT * FROM user").all();
    const r = run(unknown, "create", {
      displayName: "Synthetic unknown",
      operationId: uuid(),
    });
    assert.equal(r.ok, false);
    assert.equal(r.stage, "OutcomeUnknown");
    assert.equal(r.reusable, false);
    assert(!r.receipt);
    reader.exec("ROLLBACK");
    note("actual-DELETE-commit-unknown-no-secret-or-success");
  } finally {
    reader.close();
  }
  return [ownerToken, ...secrets];
}
export async function verifyHttpServiceLifecycle(c: any) {
  const { now, fresh } = setup(c, "lifecycle-http"),
    cookie = (
      await serializeSignedCookie(
        "better-auth.session_token",
        ownerToken,
        opts.secret,
      )
    ).split(";")[0]!,
    secrets: string[] = [],
    record = (id: string) => c.note(`service-lifecycle/${id}`);
  async function request(s: any, action = "list", body?: any, extra: any = {}) {
    const path =
      "/api/workspace/service-principals" +
      (action === "list" || action === "create" ? "" : `/${action}`);
    const r = await fetch(`${s.origin}${path}${extra.query ?? ""}`, {
      method: extra.method ?? (action === "list" ? "GET" : "POST"),
      headers: {
        cookie,
        ...(action === "list"
          ? {}
          : { origin: s.origin, "content-type": "application/json" }),
        ...extra.headers,
      },
      body:
        body === undefined
          ? undefined
          : typeof body === "string"
            ? body
            : JSON.stringify(body),
    });
    assert.equal(r.headers.getSetCookie().length, 0);
    assert.equal(r.headers.get("cache-control"), "no-store");
    const text = await r.text();
    return {
      status: r.status,
      headers: r.headers,
      data: text ? JSON.parse(text) : null,
      text,
    };
  }
  for (const mode of ["WAL", "DELETE"]) {
    const f = fresh(`normal-${mode}`, mode),
      s = await c.start(f, { serviceLifecycle: true, now });
    try {
      const operation = uuid();
      let r = await request(s, "create", {
        confirm: true,
        displayName: "Synthetic HTTP",
        operationId: operation,
      });
      assert.equal(r.status, 201);
      assert.match(r.data.secret, /^rms_[a-zA-Z]{64}$/);
      secrets.push(r.data.secret);
      const one = r.data;
      const list = await request(s);
      assert.equal(list.status, 200);
      assert.equal(
        list.data.principals[0].credentials[0].credentialId,
        one.credentialId,
      );
      assert(!list.text.includes(one.secret));
      record(`${mode}-known-create201-secret-once-owner-inventory`);
      r = await request(s, "recover", {
        confirm: true,
        operationId: operation,
      });
      assert.equal(r.status, 200);
      assert.equal(r.data.status, "completed");
      assert.equal(r.data.recovered, false);
      assert.equal(r.data.replacementCredentialId, one.credentialId);
      assert(!r.data.secret);
      const repeat = await request(s, "create", {
        confirm: true,
        displayName: "Synthetic HTTP",
        operationId: operation,
      });
      assert.equal(repeat.status, 409);
      assert(!repeat.data.secret);
      record(`${mode}-lost-reply-readback-and-reused-operation-conflict`);
      r = await request(s, "rotate", {
        confirm: true,
        serviceAccountId: one.serviceAccountId,
        credentialId: one.credentialId,
        operationId: uuid(),
      });
      assert.equal(r.status, 200);
      assert.equal(r.data.serviceAccountId, one.serviceAccountId);
      assert.notEqual(r.data.secret, one.secret);
      secrets.push(r.data.secret);
      const replacement = r.data;
      assert.equal(
        q(f, `SELECT enabled FROM apikey WHERE id='${one.credentialId}'`)[0]
          .enabled,
        0,
      );
      record(`${mode}-rotation-stable-subject-old-disabled`);
      r = await request(s, "revoke", {
        confirm: true,
        serviceAccountId: one.serviceAccountId,
        credentialId: replacement.credentialId,
        operationId: uuid(),
      });
      assert.equal(r.status, 200);
      assert.equal(r.data.revoked, true);
      assert.equal(
        q(f, "SELECT count(*) n FROM apikey WHERE enabled=1")[0].n,
        0,
      );
      record(`${mode}-explicit-targeted-cleanup-after-secret-loss`);
    } finally {
      await s.stop();
    }
  }
  for (const [id, action, body, extra, status] of [
    ["no-confirm", "create", { displayName: "Synthetic" }, {}, 400],
    [
      "null-id",
      "create",
      { confirm: true, displayName: "Synthetic", operationId: null },
      {},
      400,
    ],
    [
      "unknown-actor",
      "create",
      { confirm: true, displayName: "Synthetic", actor: "user:foreign" },
      {},
      400,
    ],
    ["array", "create", '[true,"Synthetic"]', {}, 400],
    [
      "duplicate",
      "create",
      '{"confirm":true,"confirm":true,"displayName":"Synthetic"}',
      {},
      400,
    ],
    [
      "expiry",
      "create",
      { confirm: true, displayName: "Synthetic", expiresInSeconds: 1.5 },
      {},
      400,
    ],
    ["query", "list", undefined, { query: "?subject=foreign" }, 400],
    [
      "bare-cookie",
      "list",
      undefined,
      { headers: { cookie: `${cookie}; better-auth.session_token` } },
      401,
    ],
    [
      "service",
      "list",
      undefined,
      { headers: { "x-rentemester-service-key": "private-service" } },
      401,
    ],
    [
      "foreign-origin",
      "create",
      { confirm: true, displayName: "Synthetic" },
      { headers: { origin: "https://foreign.invalid" } },
      403,
    ],
    ["missing-recover-id", "recover", { confirm: true }, {}, 400],
    [
      "nonowner-target",
      "rotate",
      { confirm: true, serviceAccountId: "unknown", credentialId: "unknown" },
      {},
      400,
    ],
  ] as any[]) {
    const f = fresh(`reject-${id}`),
      before = image(f),
      s = await c.start(f, { serviceLifecycle: true, now });
    try {
      const r = await request(s, action, body, extra);
      assert.equal(r.status, status, id);
      assert(!r.data?.secret, id);
      assert.equal(image(f), before, id);
      record(`${id}-strict-admission-no-secret-or-effects`);
    } finally {
      await s.stop();
    }
  }
  const stale = fresh("stale", "DELETE", 600000),
    ss = await c.start(stale, { serviceLifecycle: true, now });
  try {
    assert.equal((await request(ss)).status, 200);
    assert.equal(
      (await request(ss, "create", { confirm: true, displayName: "Synthetic" }))
        .status,
      401,
    );
    record("current-list-Fresh-mutation-boundary");
  } finally {
    await ss.stop();
  }
  const missing = fresh("missing"),
    ms = await c.start(missing, { now });
  try {
    assert.equal((await request(ms)).status, 404);
    assert.equal(
      (await request(ms, "create", { confirm: true, displayName: "Synthetic" }))
        .status,
      404,
    );
    record("missing-handler-no-provider-or-legacy-fallback");
  } finally {
    await ms.stop();
  }
  const rate = fresh("rate"),
    rs = await c.start(rate, { serviceLifecycle: true, now });
  try {
    for (let i = 0; i < 12; i++)
      assert.equal(
        (await request(rs, "list", undefined, { method: "HEAD" })).status,
        405,
      );
    for (let i = 0; i < 5; i++) assert.equal((await request(rs)).status, 200);
    for (let i = 0; i < 5; i++)
      assert.equal(
        (
          await request(
            rs,
            "recover",
            { confirm: true, operationId: uuid() },
            { headers: { "x-forwarded-for": `198.51.100.${i + 1}` } },
          )
        ).status,
        400,
      );
    assert.equal((await request(rs)).status, 429);
    record("HEAD-no-charge-one-direct-peer-list-mutation-budget");
  } finally {
    await rs.stop();
  }
  const unknown = fresh("unknown", "DELETE"),
    us = await c.start(unknown, { serviceLifecycle: true, now }),
    reader = new Database(unknown.control);
  try {
    reader.exec("BEGIN");
    reader.query("SELECT * FROM user").all();
    const r = await request(us, "create", {
      confirm: true,
      displayName: "Synthetic unknown",
      operationId: uuid(),
    });
    assert.equal(r.status, 503);
    assert(!r.data.secret);
    assert(!r.data.serviceAccountId);
    reader.exec("ROLLBACK");
    record("actual-DELETE-unknown-no-issued-receipt-cookie-or-success");
  } finally {
    reader.close();
    await us.stop();
  }
  return [ownerToken, cookie, ...secrets];
}
