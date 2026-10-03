import { runSql } from "./sqlite";
import type { Database } from "bun:sqlite";
import { resolveActor, type ResolveActorInput } from "./actor";
import { findWorkspaceCompany, isValidSlug, listWorkspaceCompanies } from "./workspace";
import {
  ALL_ROUTE_PERMISSIONS,
  COMPANY_PERMISSIONS,
  ROUTE_PERMISSION_POLICY,
  WORKSPACE_ROLES,
  COMPANY_ROLES,
  type WorkspaceRole,
  type CompanyRole,
  type RoutePermission,
} from "./access-permissions";

// Backwards-compatible core exports; the policy itself has no database or
// runtime dependencies and is shared with the browser.
export {
  WORKSPACE_ROLES,
  COMPANY_ROLES,
  ALL_ROUTE_PERMISSIONS,
  ROUTE_PERMISSION_POLICY,
} from "./access-permissions";
export type { WorkspaceRole, CompanyRole } from "./access-permissions";

type BetterAuthUserRow = {
  id: string;
  email: string;
  emailVerified: number;
  twoFactorEnabled: number | null;
};

type UserAccessEventRow = {
  id: number;
  event_type: "activate" | "disable";
  workspace_role: WorkspaceRole | null;
  created_at: string;
};

type MembershipEventRow = {
  id: number;
  event_type: "grant" | "revoke";
  company_role: CompanyRole | null;
  created_at: string;
};

export type WorkspaceUserAccess = {
  active: boolean;
  workspaceRole: WorkspaceRole | null;
  eventId: number | null;
  changedAt: string | null;
};

export type CompanyMembership = {
  active: boolean;
  role: CompanyRole | null;
  eventId: number | null;
  changedAt: string | null;
};

/** Secret-free identity and membership context for the hosted cockpit UI. */
export type WorkspaceSessionContext = {
  user: {
    id: string;
    email: string;
    emailVerified: boolean;
    twoFactorEnabled: boolean;
  };
  workspaceRole: WorkspaceRole;
  companies: Array<{
    slug: string;
    name: string;
    role: CompanyRole;
    archived: boolean;
  }>;
};

/** Current control-plane access only; no company ledger is opened. */
export type WorkspaceMember = {
  userId: string;
  name: string;
  email: string;
  emailVerified: boolean;
  twoFactorEnabled: boolean;
  accessReady: boolean;
  workspaceRole: WorkspaceRole;
  memberships: Array<{
    companySlug: string;
    companyName: string;
    role: CompanyRole;
    archived: boolean;
  }>;
};

export type AccessDecision = { allowed: true } | { allowed: false };

export type SessionInvalidation = {
  /** Monotonic append-only audit epoch; live rejection is enforced by session-row deletion. */
  epoch: number;
  invalidatedAt: string | null;
};

export type WorkspaceAccessMutationInput = ResolveActorInput & { userId: string };

function assertNonBlank(value: string, label: string): string {
  const normalized = typeof value === "string" ? value.trim() : "";
  if (!normalized) throw new Error(`${label} is required`);
  return normalized;
}

function readBetterAuthUser(db: Database, userId: string): BetterAuthUserRow | null {
  return db.query(
    'SELECT id, email, emailVerified, twoFactorEnabled FROM "user" WHERE id = ?',
  ).get(userId) as BetterAuthUserRow | null;
}

function assertExistingBetterAuthUser(db: Database, userId: string): string {
  const normalized = assertNonBlank(userId, "userId");
  if (!readBetterAuthUser(db, normalized)) {
    throw new Error("Better Auth user does not exist");
  }
  return normalized;
}

function isSecurityReady(user: BetterAuthUserRow | null): boolean {
  // Access is fail-closed until the Better Auth account has both verified email and TOTP enabled.
  return user?.emailVerified === 1 && user.twoFactorEnabled === 1;
}

function isAuthorizedIdentity(db: Database, user: BetterAuthUserRow | null): boolean {
  if (!user) return false;
  if (isSecurityReady(user)) return true;
  // A service account is intentionally not MFA-capable and has no browser
  // password/account row. It is admitted only after credential verification
  // and through the same workspace/company membership events as humans.
  return db.query("SELECT 1 FROM rm_workspace_service_principals WHERE user_id = ?").get(user.id) != null;
}

function currentWorkspaceOwnerUserIds(db: Database): string[] {
  return (db.query(
    `SELECT current.user_id
       FROM rm_workspace_user_access_events AS current
       JOIN (
         SELECT user_id, MAX(id) AS latest_id
           FROM rm_workspace_user_access_events
          GROUP BY user_id
       ) AS latest ON latest.latest_id = current.id
      WHERE current.event_type = 'activate' AND current.workspace_role = 'workspace_owner'`,
  ).all() as Array<{ user_id: string }>).map((row) => row.user_id);
}

function currentCompanyOwnerUserIds(db: Database, companySlug: string): string[] {
  return (db.query(
    `SELECT current.user_id
       FROM rm_company_membership_events AS current
       JOIN (
         SELECT user_id, company_slug, MAX(id) AS latest_id
           FROM rm_company_membership_events
          WHERE company_slug = ?
          GROUP BY user_id, company_slug
       ) AS latest ON latest.latest_id = current.id
      WHERE current.company_slug = ?
        AND current.event_type = 'grant'
        AND current.company_role = 'owner'`,
  ).all(companySlug, companySlug) as Array<{ user_id: string }>).map((row) => row.user_id);
}

function isEffectiveWorkspaceOwner(db: Database, userId: string): boolean {
  const access = getWorkspaceUserAccess(db, userId);
  return isSecurityReady(readBetterAuthUser(db, userId)) &&
    access.active && access.workspaceRole === "workspace_owner";
}

function countEffectiveWorkspaceOwners(db: Database): number {
  return currentWorkspaceOwnerUserIds(db)
    .filter((userId) => isEffectiveWorkspaceOwner(db, userId)).length;
}

function isEffectiveCompanyOwner(db: Database, userId: string, companySlug: string): boolean {
  const membership = getCompanyMembership(db, userId, companySlug);
  return isEffectiveWorkspaceOwner(db, userId) && membership.active && membership.role === "owner";
}

function countEffectiveCompanyOwners(db: Database, companySlug: string): number {
  return currentCompanyOwnerUserIds(db, companySlug)
    .filter((userId) => isEffectiveCompanyOwner(db, userId, companySlug)).length;
}

function assertWorkspaceOwnerCanBeRemoved(db: Database, userId: string): void {
  const effectiveOwners = countEffectiveWorkspaceOwners(db);
  const removal = isEffectiveWorkspaceOwner(db, userId) ? 1 : 0;
  if (effectiveOwners - removal < 1) {
    throw new Error("the final effective workspace owner cannot be removed or demoted");
  }
  for (const companySlug of currentCompanyOwnerSlugs(db, userId)) {
    assertCompanyOwnerCanBeRemoved(db, userId, companySlug);
  }
}

function currentCompanyOwnerSlugs(db: Database, userId: string): string[] {
  return (db.query(
    `SELECT current.company_slug
       FROM rm_company_membership_events AS current
       JOIN (
         SELECT company_slug, MAX(id) AS latest_id
           FROM rm_company_membership_events
          WHERE user_id = ?
          GROUP BY company_slug
       ) AS latest ON latest.latest_id = current.id
      WHERE current.user_id = ?
        AND current.event_type = 'grant'
        AND current.company_role = 'owner'`,
  ).all(userId, userId) as Array<{ company_slug: string }>).map((row) => row.company_slug);
}

function assertCompanyOwnerCanBeRemoved(db: Database, userId: string, companySlug: string): void {
  const effectiveOwners = countEffectiveCompanyOwners(db, companySlug);
  const removal = isEffectiveCompanyOwner(db, userId, companySlug) ? 1 : 0;
  if (effectiveOwners - removal < 1) {
    throw new Error("the final effective company owner cannot be removed or demoted");
  }
}

function latestUserAccessEvent(db: Database, userId: string): UserAccessEventRow | null {
  return db.query(
    `SELECT id, event_type, workspace_role, created_at
       FROM rm_workspace_user_access_events
      WHERE user_id = ?
      ORDER BY id DESC
      LIMIT 1`,
  ).get(userId) as UserAccessEventRow | null;
}

function latestMembershipEvent(db: Database, userId: string, companySlug: string): MembershipEventRow | null {
  return db.query(
    `SELECT id, event_type, company_role, created_at
       FROM rm_company_membership_events
      WHERE user_id = ? AND company_slug = ?
      ORDER BY id DESC
      LIMIT 1`,
  ).get(userId, companySlug) as MembershipEventRow | null;
}

function assertWorkspaceCompany(workspaceRoot: string, companySlug: string, allowArchived: boolean) {
  const normalized = assertNonBlank(companySlug, "companySlug");
  if (!isValidSlug(normalized)) throw new Error("companySlug is invalid");
  const company = findWorkspaceCompany(workspaceRoot, normalized);
  if (!company) throw new Error("company is not registered in this workspace");
  if (company.archived && !allowArchived) throw new Error("company is archived");
  return company;
}

/** Derives current workspace status from the last append-only event for the user. */
export function getWorkspaceUserAccess(db: Database, userId: string): WorkspaceUserAccess {
  const row = latestUserAccessEvent(db, assertNonBlank(userId, "userId"));
  return {
    active: row?.event_type === "activate",
    workspaceRole: row?.event_type === "activate" ? row.workspace_role : null,
    eventId: row?.id ?? null,
    changedAt: row?.created_at ?? null,
  };
}

/** Derives current company access from the last append-only event for this user/company pair. */
export function getCompanyMembership(
  db: Database,
  userId: string,
  companySlug: string,
): CompanyMembership {
  const row = latestMembershipEvent(
    db,
    assertNonBlank(userId, "userId"),
    assertNonBlank(companySlug, "companySlug"),
  );
  return {
    active: row?.event_type === "grant",
    role: row?.event_type === "grant" ? row.company_role : null,
    eventId: row?.id ?? null,
    changedAt: row?.created_at ?? null,
  };
}

/**
 * Returns the currently granted company slugs for one user, in workspace
 * manifest order. It reads only the append-only control events and manifest:
 * company ledgers are deliberately never resolved or opened here. Archived
 * registered companies remain in the result so hosted list/portfolio output
 * preserves the legacy archived-company visibility semantics.
 */
export function listActiveCompanyMembershipSlugs(
  db: Database,
  workspaceRoot: string,
  userId: string,
): string[] {
  const normalizedUserId = assertNonBlank(userId, "userId");
  const rows = db.query(
    `SELECT current.company_slug
       FROM rm_company_membership_events AS current
       JOIN (
         SELECT company_slug, MAX(id) AS latest_id
           FROM rm_company_membership_events
          WHERE user_id = ?
          GROUP BY company_slug
       ) AS latest ON latest.latest_id = current.id
      WHERE current.user_id = ? AND current.event_type = 'grant'`,
  ).all(normalizedUserId, normalizedUserId) as Array<{ company_slug: string }>;
  const granted = new Set(rows.map((row) => row.company_slug));
  return listWorkspaceCompanies(workspaceRoot)
    .filter((company) => granted.has(company.slug))
    .map((company) => company.slug);
}

/**
 * Builds the hosted UI's safe session context from control data only. Null is
 * deliberately non-disclosing: callers treat it as an authentication failure.
 * This never discovers companies or opens company ledgers.
 */
export function getWorkspaceSessionContext(
  db: Database,
  workspaceRoot: string,
  userId: string,
): WorkspaceSessionContext | null {
  const normalizedUserId = typeof userId === "string" ? userId.trim() : "";
  if (!normalizedUserId) return null;
  const user = readBetterAuthUser(db, normalizedUserId);
  if (!user || !isAuthorizedIdentity(db, user)) return null;
  const access = getWorkspaceUserAccess(db, normalizedUserId);
  if (!access.active || !access.workspaceRole) return null;

  const memberships = db.query(
    `SELECT current.company_slug, current.company_role
       FROM rm_company_membership_events AS current
       JOIN (
         SELECT company_slug, MAX(id) AS latest_id
           FROM rm_company_membership_events
          WHERE user_id = ?
          GROUP BY company_slug
       ) AS latest ON latest.latest_id = current.id
      WHERE current.user_id = ? AND current.event_type = 'grant'`,
  ).all(normalizedUserId, normalizedUserId) as Array<{
    company_slug: string;
    company_role: CompanyRole;
  }>;
  const roleBySlug = new Map(memberships.map((membership) => [
    membership.company_slug,
    membership.company_role,
  ]));

  return {
    user: {
      id: user.id,
      email: user.email,
      emailVerified: user.emailVerified === 1,
      twoFactorEnabled: user.twoFactorEnabled === 1,
    },
    workspaceRole: access.workspaceRole,
    companies: listWorkspaceCompanies(workspaceRoot)
      .flatMap((company) => {
        const role = roleBySlug.get(company.slug);
        return role ? [{ slug: company.slug, name: company.name, role, archived: company.archived }] : [];
      }),
  };
}

/**
 * Lists active workspace identities and their current append-only memberships.
 * The server layer may further reduce memberships to companies the requesting
 * owner administers. This function never opens a legal entity's ledger.
 */
export function listWorkspaceMembers(
  db: Database,
  workspaceRoot: string,
): WorkspaceMember[] {
  const users = db.query(
    `SELECT u.id, u.name, u.email, u.emailVerified, u.twoFactorEnabled,
            access.workspace_role
       FROM "user" AS u
       JOIN rm_workspace_user_access_events AS access ON access.user_id = u.id
       JOIN (
         SELECT user_id, MAX(id) AS latest_id
           FROM rm_workspace_user_access_events
          GROUP BY user_id
       ) AS latest ON latest.latest_id = access.id
      WHERE access.event_type = 'activate'
      ORDER BY lower(u.email), u.id`,
  ).all() as Array<{
    id: string;
    name: string;
    email: string;
    emailVerified: number;
    twoFactorEnabled: number | null;
    workspace_role: WorkspaceRole;
  }>;
  const companies = new Map(listWorkspaceCompanies(workspaceRoot).map((company) => [company.slug, company]));

  return users.map((user) => {
    const memberships = db.query(
      `SELECT membership.company_slug, membership.company_role
         FROM rm_company_membership_events AS membership
         JOIN (
           SELECT company_slug, MAX(id) AS latest_id
             FROM rm_company_membership_events
            WHERE user_id = ?
            GROUP BY company_slug
         ) AS latest ON latest.latest_id = membership.id
        WHERE membership.user_id = ? AND membership.event_type = 'grant'
        ORDER BY membership.company_slug`,
    ).all(user.id, user.id) as Array<{ company_slug: string; company_role: CompanyRole }>;
    return {
      userId: user.id,
      name: user.name,
      email: user.email,
      emailVerified: user.emailVerified === 1,
      twoFactorEnabled: user.twoFactorEnabled === 1,
      accessReady: user.emailVerified === 1 && user.twoFactorEnabled === 1,
      workspaceRole: user.workspace_role,
      memberships: memberships.flatMap((membership) => {
        const company = companies.get(membership.company_slug);
        return company ? [{
          companySlug: company.slug,
          companyName: company.name,
          role: membership.company_role,
          archived: company.archived,
        }] : [];
      }),
    };
  });
}

/**
 * Activates an existing Better Auth user in this workspace. A changed role is
 * represented by a new activate event; no authorization history is updated.
 */
export function activateWorkspaceUser(
  db: Database,
  input: WorkspaceAccessMutationInput & { workspaceRole: WorkspaceRole },
): { status: "activated" | "role-updated" | "already-active"; access: WorkspaceUserAccess } {
  const userId = assertExistingBetterAuthUser(db, input.userId);
  if (!WORKSPACE_ROLES.includes(input.workspaceRole)) throw new Error("workspaceRole is invalid");
  return db.transaction(() => {
    const current = getWorkspaceUserAccess(db, userId);
    if (current.active && current.workspaceRole === input.workspaceRole) {
      return { status: "already-active" as const, access: current };
    }
    if (current.active && current.workspaceRole === "workspace_owner" && input.workspaceRole !== "workspace_owner") {
      assertWorkspaceOwnerCanBeRemoved(db, userId);
    }
    const actor = resolveActor(input);
    db.query(
      `INSERT INTO rm_workspace_user_access_events (user_id, event_type, workspace_role, actor)
       VALUES (?, 'activate', ?, ?)`,
    ).run(userId, input.workspaceRole, actor.auditActor);
    return {
      status: current.active ? "role-updated" as const : "activated" as const,
      access: getWorkspaceUserAccess(db, userId),
    };
  }).immediate();
}

/** Disable is idempotent: an already-disabled/no-event user yields no new event. */
export function disableWorkspaceUser(
  db: Database,
  input: WorkspaceAccessMutationInput,
): { status: "disabled" | "already-disabled"; access: WorkspaceUserAccess } {
  const userId = assertExistingBetterAuthUser(db, input.userId);
  return db.transaction(() => {
    const current = getWorkspaceUserAccess(db, userId);
    if (!current.active) return { status: "already-disabled" as const, access: current };
    if (current.workspaceRole === "workspace_owner") assertWorkspaceOwnerCanBeRemoved(db, userId);
    const actor = resolveActor(input);
    db.query(
      `INSERT INTO rm_workspace_user_access_events (user_id, event_type, workspace_role, actor)
       VALUES (?, 'disable', NULL, ?)`,
    ).run(userId, actor.auditActor);
    db.query(
      `INSERT INTO rm_workspace_security_events (scope, user_id, event_type, actor)
       VALUES ('user', ?, 'session_invalidate', ?)`,
    ).run(userId, actor.auditActor);
    db.query('DELETE FROM "session" WHERE "userId" = ?').run(userId);
    return { status: "disabled" as const, access: getWorkspaceUserAccess(db, userId) };
  }).immediate();
}

/** Grants require an existing Better Auth user and a registered, non-archived company. */
export function grantCompanyMembership(
  db: Database,
  workspaceRoot: string,
  input: WorkspaceAccessMutationInput & { companySlug: string; role: CompanyRole },
): { status: "granted" | "role-updated" | "already-granted"; membership: CompanyMembership } {
  const userId = assertExistingBetterAuthUser(db, input.userId);
  const company = assertWorkspaceCompany(workspaceRoot, input.companySlug, false);
  if (!COMPANY_ROLES.includes(input.role)) throw new Error("company role is invalid");
  return db.transaction(() => {
    const current = getCompanyMembership(db, userId, company.slug);
    if (current.active && current.role === input.role) {
      return { status: "already-granted" as const, membership: current };
    }
    if (current.active && current.role === "owner" && input.role !== "owner") {
      assertCompanyOwnerCanBeRemoved(db, userId, company.slug);
    }
    const actor = resolveActor(input);
    db.query(
      `INSERT INTO rm_company_membership_events (user_id, company_slug, event_type, company_role, actor)
       VALUES (?, ?, 'grant', ?, ?)`,
    ).run(userId, company.slug, input.role, actor.auditActor);
    return {
      status: current.active ? "role-updated" as const : "granted" as const,
      membership: getCompanyMembership(db, userId, company.slug),
    };
  }).immediate();
}

/**
 * Revocation is idempotent. We permit revoking an archived registered company
 * because it only reduces access; a missing company fails closed.
 */
export function revokeCompanyMembership(
  db: Database,
  workspaceRoot: string,
  input: WorkspaceAccessMutationInput & { companySlug: string },
): { status: "revoked" | "already-revoked"; membership: CompanyMembership } {
  const userId = assertExistingBetterAuthUser(db, input.userId);
  const company = assertWorkspaceCompany(workspaceRoot, input.companySlug, true);
  return db.transaction(() => {
    const current = getCompanyMembership(db, userId, company.slug);
    if (!current.active) return { status: "already-revoked" as const, membership: current };
    if (current.role === "owner") assertCompanyOwnerCanBeRemoved(db, userId, company.slug);
    const actor = resolveActor(input);
    db.query(
      `INSERT INTO rm_company_membership_events (user_id, company_slug, event_type, company_role, actor)
       VALUES (?, ?, 'revoke', NULL, ?)`,
    ).run(userId, company.slug, actor.auditActor);
    return { status: "revoked" as const, membership: getCompanyMembership(db, userId, company.slug) };
  }).immediate();
}

/**
 * Non-disclosing authorization check. It reads only the workspace-control DB
 * and workspace manifest; it never resolves, opens, or inspects a company ledger.
 */
export function authorizeWorkspaceRoute(
  db: Database,
  workspaceRoot: string,
  input: { userId?: string | null; permission: RoutePermission; companySlug?: string | null },
): AccessDecision {
  if (!ALL_ROUTE_PERMISSIONS.includes(input.permission)) return { allowed: false };
  if (input.permission === "public.read" || input.permission === "public.invitation.claim") {
    return { allowed: true };
  }
  const userId = typeof input.userId === "string" ? input.userId.trim() : "";
  if (!userId || !isAuthorizedIdentity(db, readBetterAuthUser(db, userId))) return { allowed: false };

  const workspace = getWorkspaceUserAccess(db, userId);
  if (!workspace.active || !workspace.workspaceRole) return { allowed: false };
  if (input.permission.startsWith("workspace.")) {
    return ROUTE_PERMISSION_POLICY[workspace.workspaceRole].includes(input.permission)
      ? { allowed: true }
      : { allowed: false };
  }

  const companySlug = typeof input.companySlug === "string" ? input.companySlug.trim() : "";
  if (!companySlug || !isValidSlug(companySlug)) return { allowed: false };
  const company = findWorkspaceCompany(workspaceRoot, companySlug);
  if (!company || company.archived) return { allowed: false };
  const membership = getCompanyMembership(db, userId, companySlug);
  if (!membership.active || !membership.role) return { allowed: false };
  return COMPANY_PERMISSIONS[membership.role].includes(input.permission)
    ? { allowed: true }
    : { allowed: false };
}

/** Appends a user-specific security epoch which future Better Auth session checks must enforce. */
export function invalidateUserSessions(
  db: Database,
  input: WorkspaceAccessMutationInput,
): SessionInvalidation {
  const userId = assertExistingBetterAuthUser(db, input.userId);
  return db.transaction(() => {
    const actor = resolveActor(input);
    db.query(
      `INSERT INTO rm_workspace_security_events (scope, user_id, event_type, actor)
       VALUES ('user', ?, 'session_invalidate', ?)`,
    ).run(userId, actor.auditActor);
    // The Better Auth session table is deliberately mutable. Deletion is the
    // authorization boundary, so no timestamp precision can admit a session
    // minted in the same second as an emergency invalidation.
    db.query('DELETE FROM "session" WHERE "userId" = ?').run(userId);
    return getSessionInvalidation(db, userId);
  })();
}

/** Appends a workspace-wide security epoch for restore or emergency session invalidation. */
export function invalidateWorkspaceSessions(
  db: Database,
  input: ResolveActorInput = {},
): SessionInvalidation {
  return db.transaction(() => {
    const actor = resolveActor(input);
    db.query(
      `INSERT INTO rm_workspace_security_events (scope, user_id, event_type, actor)
       VALUES ('workspace', NULL, 'session_invalidate', ?)`,
    ).run(actor.auditActor);
    runSql(db, 'DELETE FROM "session"');
    return getSessionInvalidation(db, null);
  })();
}

/**
 * Exact session-id lookup for the auth boundary. A deleted Better Auth row is
 * never rescued by a stale provider response, so this is safe at millisecond
 * and same-second boundaries without relying on timestamp comparisons.
 */
export function hasCurrentBetterAuthSession(
  db: Database,
  userId: string,
  sessionId: string,
): boolean {
  const normalizedUserId = typeof userId === "string" ? userId.trim() : "";
  const normalizedSessionId = typeof sessionId === "string" ? sessionId.trim() : "";
  if (!normalizedUserId || !normalizedSessionId) return false;
  return db.query(
    'SELECT 1 FROM "session" WHERE id = ? AND "userId" = ?',
  ).get(normalizedSessionId, normalizedUserId) != null;
}

/** Latest applicable append-only epoch. User lookups include workspace-wide invalidations. */
export function getSessionInvalidation(db: Database, userId: string | null): SessionInvalidation {
  const normalized = typeof userId === "string" ? userId.trim() : "";
  const row = normalized
    ? db.query(
      `SELECT id, created_at FROM rm_workspace_security_events
        WHERE scope = 'workspace' OR (scope = 'user' AND user_id = ?)
        ORDER BY id DESC LIMIT 1`,
    ).get(normalized) as { id: number; created_at: string } | null
    : db.query(
      `SELECT id, created_at FROM rm_workspace_security_events
        WHERE scope = 'workspace'
        ORDER BY id DESC LIMIT 1`,
    ).get() as { id: number; created_at: string } | null;
  return { epoch: row?.id ?? 0, invalidatedAt: row?.created_at ?? null };
}
