/**
 * The HTTP route capability vocabulary is intentionally core-owned so both
 * server dispatch and workspace authorization can depend on it without a
 * core-to-server import cycle.
 */
export const ROUTE_PERMISSIONS = [
  "public.read",
  "public.invitation.claim",
  "workspace.read",
  "workspace.group.read",
  "workspace.manage",
  "workspace.members.read",
  "workspace.members.manage",
  "company.read",
  "company.documents.read",
  "company.documents.upload",
  "company.master-data",
  "company.draft.write",
  "company.ledger.post",
  "company.review",
  "company.period.force-close",
  "company.export",
  "company.external-lookup",
  "company.external-send",
  "company.admin",
  "company.knowledge.read",
  "company.knowledge.manage",
  "company.ownership.read",
  "company.ownership.manage",
] as const;

export type RoutePermission = typeof ROUTE_PERMISSIONS[number];

/** A workspace owner administers access, but does not implicitly access every company. */
export type WorkspaceRole = "workspace_owner" | "member";
/** Company roles are scoped to one registered legal entity/ledger. */
export type CompanyRole = "owner" | "bookkeeper" | "reviewer" | "reader";

export const WORKSPACE_ROLES: readonly WorkspaceRole[] = ["workspace_owner", "member"];
export const COMPANY_ROLES: readonly CompanyRole[] = ["owner", "bookkeeper", "reviewer", "reader"];

/** Keep this list adjacent to the policy so a RoutePermission addition fails tests until classified. */
export const ALL_ROUTE_PERMISSIONS = ROUTE_PERMISSIONS;

export const COMPANY_PERMISSIONS: Readonly<Record<CompanyRole, readonly RoutePermission[]>> = {
  // Company ownership is deliberately local to this one legal entity.
  owner: ALL_ROUTE_PERMISSIONS.filter((permission) => permission.startsWith("company.")),
  // A bookkeeper can operate locally, but cannot approve, administer, or send externally.
  bookkeeper: [
    "company.read",
    "company.documents.read",
    "company.documents.upload",
    "company.master-data",
    "company.draft.write",
    "company.ledger.post",
    "company.export",
    "company.external-lookup",
    "company.knowledge.read",
    "company.knowledge.manage",
    "company.ownership.read",
    "company.ownership.manage",
  ],
  reviewer: ["company.read", "company.documents.read", "company.review", "company.export", "company.knowledge.read", "company.knowledge.manage", "company.ownership.read", "company.ownership.manage"],
  reader: ["company.read", "company.documents.read", "company.export", "company.knowledge.read", "company.ownership.read"],
};

export const ROUTE_PERMISSION_POLICY: Readonly<Record<CompanyRole | WorkspaceRole, readonly RoutePermission[]>> = {
  // `public.read` is listed here solely to make the policy exhaustive; public
  // authorization itself remains anonymous on the server.
  workspace_owner: [
    "public.read", "public.invitation.claim", "workspace.read", "workspace.group.read",
    "workspace.manage", "workspace.members.read", "workspace.members.manage",
  ],
  member: ["workspace.read"],
  ...COMPANY_PERMISSIONS,
};

/** A presentation check only; the server still authorizes every request. */
export function roleAllowsPermission(
  role: CompanyRole | WorkspaceRole | null | undefined,
  permission: RoutePermission,
): boolean {
  return role != null && ROUTE_PERMISSION_POLICY[role].includes(permission);
}
