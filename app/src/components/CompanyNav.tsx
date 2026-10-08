import * as stylex from "@stylexjs/stylex";
import { cockpitStyles } from "../design/cockpit.stylex";

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

import {
	createContext,
	type ReactNode,
	useCallback,
	useContext,
	useEffect,
	useId,
	useMemo,
	useState,
} from "react";
import { NavLink, useLocation, useSearchParams } from "react-router-dom";
import { companyRouteForPath } from "../company-route-path";
import type { CompanyRouteId } from "../company-route-registry";
import type { FiscalYearEntry } from "../lib/types";
import { Select } from "./ui";

export type CompanyRouteNavigationProjection = {
	routes: readonly {
		id: CompanyRouteId;
		segment: string;
		label: string;
		area: string;
	}[];
	areas: readonly {
		id: string;
		label: string;
		destination: string;
	}[];
};

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

type YearControl = {
	owner: string;
	slug: string;
	years: FiscalYearEntry[];
	selected: string;
};
type ShellContext = {
	navigation: CompanyRouteNavigationProjection;
	rendersNavigation: boolean;
	controls: YearControl | null;
	register: (control: YearControl) => void;
	unregister: (owner: string) => void;
};
const CompanyNavigationShellContext = createContext<ShellContext | null>(null);
export function useCompanyShell() {
	return useContext(CompanyNavigationShellContext);
}

/** Supplies main's navigation projection and centralized fiscal-year control. */
export function CompanyNavigationShell({
	children,
	navigation,
	rendersNavigation = false,
}: {
	children: ReactNode;
	navigation: CompanyRouteNavigationProjection;
	rendersNavigation?: boolean;
}) {
	const [controls, setControls] = useState<YearControl | null>(null);
	const register = useCallback(
		(control: YearControl) =>
			setControls((previous) =>
				previous?.owner === control.owner &&
				previous.slug === control.slug &&
				previous.selected === control.selected &&
				JSON.stringify(previous.years) === JSON.stringify(control.years)
					? previous
					: control,
			),
		[],
	);
	const unregister = useCallback((_owner: string) => {
		/* Keep company metadata while the next year is loading. */
	}, []);
	const value = useMemo(
		() => ({ navigation, rendersNavigation, controls, register, unregister }),
		[navigation, rendersNavigation, controls, register, unregister],
	);
	return (
		<CompanyNavigationShellContext.Provider value={value}>
			{children}
		</CompanyNavigationShellContext.Provider>
	);
}

/** Task navigation shared by every company route, including pages without a year selector. */
export function CompanyTaskNavigation({
	visibleRouteIds,
	navigation: navigationOverride,
}: {
	/** Presentation filter only; the server remains the authorization boundary. */
	visibleRouteIds?: readonly CompanyRouteId[];
	/** Lets isolated component hosts provide the same projection as the app shell. */
	navigation?: CompanyRouteNavigationProjection;
}) {
	const shellNavigation = useContext(CompanyNavigationShellContext);
	const navigation = navigationOverride ?? shellNavigation?.navigation;
	const [params] = useSearchParams();
	const location = useLocation();
	// #UI-4: only the fiscal year is a cross-view concern. Threading the WHOLE
	// query string leaked per-view filters (Bank's q/from/to/status, a posting
	// account=…) onto every other tab. Whitelist `?year=` and drop the rest —
	// each view owns its own filter namespace.
	const year = params.get("year");
	const suffix = year ? `?year=${encodeURIComponent(year)}` : "";
	const currentRoute =
		navigation && companyRouteForPath(location.pathname, navigation.routes);
	const slug = location.pathname.match(/^\/companies\/([^/]+)/)?.[1];
	const visibleRoutes =
		navigation?.routes.filter(
			(route) => !visibleRouteIds || visibleRouteIds.includes(route.id),
		) ?? [];
	const visibleAreas =
		navigation?.areas.filter((area) =>
			visibleRoutes.some((route) => route.area === area.id),
		) ?? [];
	if (!currentRoute || !slug) return null;
	const toPath = (segment: string) =>
		`${segment ? `/companies/${slug}/${segment}` : `/companies/${slug}`}${suffix}`;

	return (
		<section
			aria-label="Virksomhedsnavigation"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.companyTaskNavigation,
			)}
		>
			<nav
				aria-label="Daglige opgaver"
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.companyAreas,
				)}
			>
				{visibleAreas.map((area) => {
					const destination =
						visibleRoutes.find((route) => route.segment === area.destination) ??
						visibleRoutes.find((route) => route.area === area.id);
					if (!destination) return null;
					return (
						<NavLink
							key={area.id}
							to={toPath(destination.segment)}
							{...stylex.props(
								cockpitStyles.companyAreasAComposition,
								currentRoute.area === area.id &&
									cockpitStyles.companyAreasAActive,
							)}
						>
							{area.label}
						</NavLink>
					);
				})}
			</nav>
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
	const shellNavigation = useContext(CompanyNavigationShellContext);
	const owner = useId();
	const register = shellNavigation?.register;
	const unregister = shellNavigation?.unregister;
	useEffect(() => {
		register?.({ owner, slug, years, selected: selectedYear });
	}, [register, owner, slug, years, selectedYear]);
	useEffect(() => () => unregister?.(owner), [unregister, owner]);
	if (shellNavigation?.rendersNavigation) return null;
	return (
		<>
			<CompanyTaskNavigation />
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.companyYearControls,
				)}
			>
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
		<label
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.yearSelector,
			)}
		>
			<span
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.yearSelectorYsLabel,
				)}
			>
				Regnskabsår
			</span>
			<Select
				value={selected}
				onChange={(e) => onChange(e.target.value)}
				aria-label="Vælg regnskabsår"
				xstyle={[cockpitStyles.yearSelectorSelectFocusComposition]}
			>
				{years.map((y) => (
					<option
						key={y.label}
						value={y.label}
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{y.label}
						{y.source === "archive" ? " (arkiv)" : ""}
					</option>
				))}
			</Select>
		</label>
	);
}
