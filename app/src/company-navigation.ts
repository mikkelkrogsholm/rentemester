/**
 * Compatibility facade for navigation-focused imports. The route registry is
 * the source of truth; this module deliberately owns no route metadata.
 */
export {
  COMPANY_ROUTE_REGISTRY,
  COMPANY_FLOW_ROUTE_REGISTRY,
  COMPANY_ROUTE_DEFINITIONS,
  companyYearScope,
  type CompanyYearScope,
  COMPANY_TASK_AREAS,
  assertCompanyRouteRegistry,
  companyRouteForPath,
  companyRoutePattern,
  type CompanyRouteDefinition,
  type CompanyRouteId,
  type CompanyTaskAreaId,
} from "./company-route-registry";
