import { Button, Select } from "./ui";
// Per-company chrome shared by every company view (cockpit-redesign it. 2).
//
// Two concerns live here so the four company views stay declarative:
//
//   * `useCompanyYear` — the selected fiscal year, carried in the URL as a
//     `?year=` query param. Carrying it in the route (not React state) means
//     the choice survives navigation between a company's sub-views and a page
//     reload, and every in-app link below preserves it automatically.
//
//   * `CompanyNav` — the sub-navigation bar plus the fiscal-year selector,
//     rendered at the top of each company view. The destinations are classified
//     into six task areas; only the active area's destinations are shown.

import { NavLink, useLocation, useSearchParams } from "react-router-dom";
import {
  createContext,
  useContext,
  useEffect,
  useState,
  useCallback,
  useMemo,
  useId,
  type ReactNode,
} from "react";
import type { FiscalYearEntry } from "../lib/types";
import {
  COMPANY_ROUTE_DEFINITIONS,
  COMPANY_TASK_AREAS,
  companyRouteForPath,
  type CompanyTaskAreaId,
  type CompanyRouteId,
} from "../company-navigation";

/**
 * The selected fiscal year as a URL query param. `year` is `undefined` until
 * the user picks one (the backend then defaults to the most recent live
 * year); `setYear` writes it back to the URL so it persists across views.
 */
export function useCompanyYear(): {
  year: string | undefined;
  setYear: (year: string) => void;
} {
  const [params, setParams] = useSearchParams();
  const year = params.get("year") ?? undefined;
  const setYear = (next: string) => {
    const updated = new URLSearchParams(params);
    updated.set("year", next);
    setParams(updated, { replace: true });
  };
  return { year, setYear };
}

/**
 * The route for a single account's postings — the Posteringer view filtered
 * to one account via `?account=`. The fiscal year is carried through so the
 * drill-down lands on the same year the statement was showing. Used by the
 * statement views (Resultatopgørelse · Balance · Saldobalance) to make every
 * account row a drill-down link.
 */
export function accountPostingsTo(
  slug: string,
  year: string,
  accountNo: string,
): string {
  const params = new URLSearchParams();
  if (year) params.set("year", year);
  params.set("account", accountNo);
  return `/companies/${slug}/posteringer?${params.toString()}`;
}

type YearControl = { owner: string; slug: string; years: FiscalYearEntry[]; selected: string };
type ShellContext = { controls: YearControl | null; register: (control: YearControl) => void; unregister: (owner: string) => void };
const CompanyNavigationShellContext = createContext<ShellContext | null>(null);
export function useCompanyShell() { return useContext(CompanyNavigationShellContext); }

/** Marks routes that already receive the shared navigation from the app shell. */
export function CompanyNavigationShell({ children }: { children: ReactNode }) {
  const [controls, setControls] = useState<YearControl | null>(null);
  const register = useCallback((control: YearControl) => setControls((previous) => previous?.owner === control.owner && previous.slug === control.slug && previous.selected === control.selected && JSON.stringify(previous.years) === JSON.stringify(control.years) ? previous : control), []);
  const unregister = useCallback((_owner: string) => { /* Keep company metadata while the next year is loading. */ }, []);
  const value = useMemo(() => ({ controls, register, unregister }), [controls, register, unregister]);
  return <CompanyNavigationShellContext.Provider value={value}>{children}</CompanyNavigationShellContext.Provider>;
}

/** Task navigation shared by every company route, including pages without a year selector. */
export function CompanyTaskNavigation({
  visibleRouteIds,
}: {
  /** Presentation filter only; the server remains the authorization boundary. */
  visibleRouteIds?: readonly CompanyRouteId[];
}) {
  const [params] = useSearchParams();
  const location = useLocation();
  // #UI-4: only the fiscal year is a cross-view concern. Threading the WHOLE
  // query string leaked per-view filters (Bank's q/from/to/status, a posting
  // account=…) onto every other tab. Whitelist `?year=` and drop the rest —
  // each view owns its own filter namespace.
  const year = params.get("year");
  const suffix = year ? `?year=${encodeURIComponent(year)}` : "";
  const currentRoute = companyRouteForPath(location.pathname);
  const slug = location.pathname.match(/^\/companies\/([^/]+)/)?.[1];
  const visibleRoutes = COMPANY_ROUTE_DEFINITIONS.filter(
    (route) => route.kind === "page" && (!visibleRouteIds || visibleRouteIds.includes(route.id)),
  );
  const visibleAreas = COMPANY_TASK_AREAS.filter((area) =>
    visibleRoutes.some((route) => route.area === area.id),
  );
  const defaultArea = visibleAreas.some((area) => area.id === currentRoute?.area)
    ? currentRoute?.area
    : visibleAreas[0]?.id;
  const [selectedArea, setSelectedArea] = useState<CompanyTaskAreaId | undefined>(
    defaultArea,
  );
  useEffect(() => setSelectedArea(defaultArea), [defaultArea]);
  if (!currentRoute || !slug) return null;
  const currentAreaRoutes = visibleRoutes.filter((route) => route.area === selectedArea);
  const toPath = (segment: string) =>
    `${segment ? `/companies/${slug}/${segment}` : `/companies/${slug}`}${suffix}`;

  return (
    <section className="company-task-navigation" aria-label="Virksomhedsnavigation">
      <nav className="company-areas" aria-label="Opgaveområder">
        {visibleAreas.map((area) => {
          const active = area.id === selectedArea;
          const current = area.id === currentRoute.area;
          return (
            <Button
              key={area.id}
              type="button"
              className={[active && "active", current && "current"]
                .filter(Boolean)
                .join(" ") || undefined}
              aria-pressed={active}
              aria-current={current ? "true" : undefined}
              aria-controls="company-area-destinations"
              onClick={() => setSelectedArea(area.id)}
            >
              {area.label}
            </Button>
          );
        })}
      </nav>
      {currentAreaRoutes.length > 0 && (
        <nav
          id="company-area-destinations"
          className="company-destinations"
          aria-label={`Sider i ${COMPANY_TASK_AREAS.find((area) => area.id === selectedArea)?.label}`}
        >
          {currentAreaRoutes.map((route) => (
            <NavLink key={route.id} to={toPath(route.segment)} end>
              {route.label}
            </NavLink>
          ))}
        </nav>
      )}
    </section>
  );
}

/** The fiscal-year control retained by year-aware company views. */
export function CompanyNav({
  slug,
  years,
  selectedYear,
  onYearChange,
}: {
  slug: string;
  years: FiscalYearEntry[];
  selectedYear: string;
  onYearChange: (year: string) => void;
}) {
  const shell = useContext(CompanyNavigationShellContext);
  const owner = useId();
  const register = shell?.register;
  const unregister = shell?.unregister;
  useEffect(() => { register?.({ owner, slug, years, selected: selectedYear }); }, [register, owner, slug, years, selectedYear]);
  useEffect(() => () => unregister?.(owner), [unregister, owner]);
  if (shell) return null;
  return (
    <>
      <CompanyTaskNavigation />
      <div className="company-year-controls">
        <YearSelector
          years={years}
          selected={selectedYear}
          onChange={onYearChange}
        />
      </div>
    </>
  );
}

/** The fiscal-year dropdown — shared by every company view. */
export function YearSelector({
  years,
  selected,
  onChange,
}: {
  years: FiscalYearEntry[];
  selected: string;
  onChange: (year: string) => void;
}) {
  return (
    <label className="year-selector">
      <span className="ys-label">Regnskabsår</span>
      <Select
        value={selected}
        onChange={(e) => onChange(e.target.value)}
        aria-label="Vælg regnskabsår"
      >
        {years.map((y) => (
          <option key={y.label} value={y.label}>
            {y.label}
            {y.source === "archive" ? " (arkiv)" : ""}
          </option>
        ))}
      </Select>
    </label>
  );
}
