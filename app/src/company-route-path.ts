/** Pure company-route URL matching shared by the registry and navigation UI. */
export type CompanyRoutePathDescriptor = { segment: string };

export function companyRouteForPath<Route extends CompanyRoutePathDescriptor>(
  pathname: string,
  routes: readonly Route[],
): Route | undefined {
  if (pathname === "/companies/new" || pathname.startsWith("/companies/new/")) return undefined;
  const match = pathname.match(/^\/companies\/[^/]+\/?(.*)$/);
  if (!match) return undefined;
  const segment = match[1].replace(/\/$/, "");
  const parts = segment ? segment.split("/") : [];
  // Prefer literal segments (fakturaer/ny) over parameterized detail routes.
  const candidates = [...routes].sort((a, b) => {
    const specificity = (route: Route) => route.segment.split("/").filter((part) => part && !part.startsWith(":")).length;
    return specificity(b) - specificity(a);
  });
  return candidates.find((route) => {
    const expected = route.segment ? route.segment.split("/") : [];
    return expected.length === parts.length && expected.every((part, index) => part.startsWith(":") ? Boolean(parts[index]) : part === parts[index]);
  });
}
