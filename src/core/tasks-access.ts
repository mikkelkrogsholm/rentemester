import type { Database } from "bun:sqlite";
import { COMPANY_PERMISSIONS } from "./access-permissions";
import { authorizeWorkspaceRoute, getCompanyMembership, getWorkspaceUserAccess } from "./workspace-access";
import { listWorkspaceCompanies } from "./workspace";
import type { Task, TaskIdentity, TaskScope } from "./tasks-types";

export type TaskAccess = { owner: boolean; read: Set<string>; write: Set<string>; manage: Set<string> };

/** Archived company task duties remain accessible through retained membership.
 * This does not loosen authorization of any accounting or document operation. */
export function taskAccess(db: Database, workspaceRoot: string, identity: TaskIdentity): TaskAccess {
  const result: TaskAccess = { owner: false, read: new Set(), write: new Set(), manage: new Set() };
  const companies = listWorkspaceCompanies(workspaceRoot);
  if (identity.local) {
    result.owner = true;
    for (const company of companies) {
      result.read.add(company.slug); result.write.add(company.slug); result.manage.add(company.slug);
    }
    return result;
  }
  const userId = identity.userId;
  if (!userId || !authorizeWorkspaceRoute(db, workspaceRoot, { userId, permission: "workspace.tasks.read" }).allowed) return result;
  result.owner = getWorkspaceUserAccess(db, userId).workspaceRole === "workspace_owner";
  for (const company of companies) {
    const membership = getCompanyMembership(db, userId, company.slug);
    if (!membership.active || !membership.role) continue;
    const permissions = COMPANY_PERMISSIONS[membership.role];
    if (permissions.includes("company.tasks.read")) result.read.add(company.slug);
    if (permissions.includes("company.tasks.write")) result.write.add(company.slug);
    if (permissions.includes("company.tasks.manage")) result.manage.add(company.slug);
  }
  return result;
}

export function taskScopeCompanies(scope: TaskScope): string[] {
  return scope.kind === "company" ? [scope.companySlug] : [...scope.companySlugs];
}

export function canAccessTaskScope(access: TaskAccess, scope: TaskScope, mode: "read" | "write" | "manage"): boolean {
  const companies = taskScopeCompanies(scope);
  return companies.length === 0 ? access.owner : companies.every(slug => access[mode].has(slug));
}

/** References cannot widen the task's authorized scope. */
export function taskReferencesWithinScope(task: Pick<Task, "scope" | "references" | "source">): boolean {
  const companies = new Set(taskScopeCompanies(task.scope));
  return task.references.every(reference => !reference.companySlug || companies.has(reference.companySlug))
    && (!task.source || companies.has(task.source.companySlug));
}

export function canReadTask(access: TaskAccess, task: Task): boolean {
  return canAccessTaskScope(access, task.scope, "read") && taskReferencesWithinScope(task);
}
