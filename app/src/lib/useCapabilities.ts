import { useContext } from 'react';
import { UNSAFE_RouteContext } from 'react-router-dom';
import { ROUTE_PERMISSION_POLICY, type CompanyRole, type RoutePermission } from '../../../src/core/access-permissions';
import { useOptionalAuth } from './auth-context';

/** Presentation uses the exact server policy. The server authorizes every request. */
export function useCapabilities(companySlug?: string) {
  const route = useContext(UNSAFE_RouteContext);
  const slug = route.matches.at(-1)?.params.slug;
  const auth = useOptionalAuth();
  const hosted = auth?.hosted ?? false;
  const company = auth?.context?.companies.find((entry) => entry.slug === (companySlug ?? slug));
  const role: CompanyRole | null = hosted ? company?.role ?? null : 'owner';
  const workspaceRole = auth?.context?.workspaceRole;
  return {
    role, hosted, archived: company?.archived ?? false,
    can(permission: RoutePermission): boolean {
      if (!hosted) return true;
      if (permission.startsWith('company.')) return role !== null && ROUTE_PERMISSION_POLICY[role].includes(permission);
      return workspaceRole !== undefined && ROUTE_PERMISSION_POLICY[workspaceRole].includes(permission);
    },
  };
}
