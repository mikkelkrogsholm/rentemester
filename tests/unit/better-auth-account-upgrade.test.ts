import { Database } from "bun:sqlite";
import { describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { hashPassword } from "better-auth/crypto";
import { loadMigrationCatalog } from "../../src/core/migration-catalog";
import { openWorkspaceControlDb, workspaceControlPaths } from "../../src/core/workspace-control";
import { openWorkspaceBetterAuth } from "../../src/server/better-auth";

function legacyWorkspace() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-auth-upgrade-"));
  const paths = workspaceControlPaths(root);
  mkdirSync(paths.directory, { recursive: true });
  const db = new Database(paths.db);
  db.exec("PRAGMA foreign_keys = ON");
  for (const migration of loadMigrationCatalog(join(import.meta.dir, "../../src/core/workspace-migrations"))) {
    if (migration.id >= 35) break;
    db.exec(migration.sql!);
    db.query("INSERT INTO workspace_schema_migrations (id,name,checksum,applied_by_version) VALUES (?,?,?,'test')")
      .run(migration.id, migration.name, migration.checksum);
  }
  db.query('INSERT INTO "user" (id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES (?,?,?,1,?,?,0)')
    .run("existing-user", "Synthetic User", "existing@example.test", "2026-01-01", "2026-01-01");
  return { root, db };
}

describe("Better Auth patched account schema upgrade", () => {
  test("preserves credentials and sessions, allows existing login and enforces provider/account uniqueness", async () => {
    const { root, db } = legacyWorkspace();
    const password = "synthetic-upgrade-password";
    const passwordHash = await hashPassword(password);
    db.query('INSERT INTO "account" (id,issuer,accountId,providerId,userId,password,createdAt,updatedAt) VALUES (?,?,?,?,?,?,?,?)')
      .run("existing-account", "credential", "existing-user", "credential", "existing-user", passwordHash, "2026-01-01", "2026-01-01");
    db.exec(`INSERT INTO "session" (id,expiresAt,token,createdAt,updatedAt,userId)
      VALUES ('existing-session','2030-01-01','synthetic-session-token','2026-01-01','2026-01-01','existing-user')`);
    const previousAccount = db.query('SELECT id,accountId,providerId,userId,password,createdAt,updatedAt FROM "account"').get();
    const previousSession = db.query('SELECT * FROM "session"').get();
    db.close();
    try {
      const upgraded = openWorkspaceControlDb(root);
      try {
        expect(upgraded.query('SELECT id,accountId,providerId,userId,password,createdAt,updatedAt FROM "account"').get()).toEqual(previousAccount);
        expect(upgraded.query('SELECT * FROM "session"').get()).toEqual(previousSession);
        expect((upgraded.query('PRAGMA table_info("account")').all() as { name: string }[]).some(({ name }) => name === "issuer")).toBe(false);
        const insert = upgraded.query('INSERT INTO "account" (id,accountId,providerId,userId,createdAt,updatedAt) VALUES (?,?,?,?,?,?)');
        insert.run("other-provider", "existing-user", "synthetic-provider", "existing-user", "2026-01-01", "2026-01-01");
        expect(() => insert.run("duplicate", "existing-user", "credential", "existing-user", "2026-01-01", "2026-01-01")).toThrow("UNIQUE");
        expect(() => insert.run("orphan", "orphan", "credential", "absent-user", "2026-01-01", "2026-01-01")).toThrow("FOREIGN KEY");
      } finally { upgraded.close(); }
      const runtime = openWorkspaceBetterAuth(root, {
        secret: "I0UjL6i0-ScgvjfIgzMKJxPQyDpPXwg2mMKdLW3Y3WQ",
        trustedOrigins: ["http://127.0.0.1"], baseURL: "http://127.0.0.1",
      });
      try {
        const result = await runtime.auth.api.signInEmail({ body: { email: "existing@example.test", password } });
        expect(result.user.id).toBe("existing-user");
      } finally { runtime.close(); }
    } finally { rmSync(root, { recursive: true, force: true }); }
  });

  test("duplicate legacy provider keys reject the migration atomically without merging users or deleting columns", () => {
    const { root, db } = legacyWorkspace();
    db.exec(`INSERT INTO "user" (id,name,email,emailVerified,createdAt,updatedAt) VALUES ('other-user','Other Synthetic','other@example.test',1,'2026-01-01','2026-01-01');
      INSERT INTO "account" (id,issuer,accountId,providerId,userId,createdAt,updatedAt) VALUES
      ('first','first-issuer','same-subject','synthetic-provider','existing-user','2026-01-01','2026-01-01'),
      ('second','second-issuer','same-subject','synthetic-provider','other-user','2026-01-01','2026-01-01');`);
    expect(db.query('SELECT id,providerId,accountId FROM "account" ORDER BY id').all()).toEqual([
      { id: "first", providerId: "synthetic-provider", accountId: "same-subject" },
      { id: "second", providerId: "synthetic-provider", accountId: "same-subject" },
    ]);
    db.close();
    try {
      expect(() => openWorkspaceControlDb(root)).toThrow("UNIQUE");
      const unchanged = new Database(workspaceControlPaths(root).db, { readonly: true });
      try {
        expect(unchanged.query('SELECT id,userId,issuer FROM "account" ORDER BY id').all()).toEqual([
          { id: "first", userId: "existing-user", issuer: "first-issuer" },
          { id: "second", userId: "other-user", issuer: "second-issuer" },
        ]);
        expect(unchanged.query("SELECT MAX(id) AS version FROM workspace_schema_migrations").get()).toEqual({ version: 34 });
        expect(unchanged.query("SELECT name FROM sqlite_master WHERE name='account_providerId_accountId_uidx'").get()).toBeNull();
      } finally { unchanged.close(); }
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
});
