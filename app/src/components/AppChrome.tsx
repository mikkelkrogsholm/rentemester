import * as stylex from "@stylexjs/stylex";
import { useEffect, useId, useRef, useState } from "react";
import {
	NavLink,
	useLocation,
	useNavigate,
	useSearchParams,
} from "react-router-dom";
import {
	ROUTE_PERMISSION_POLICY,
	type RoutePermission,
} from "../../../src/core/access-permissions";
import {
	COMPANY_ROUTE_DEFINITIONS,
	COMPANY_TASK_AREAS,
	companyRouteForPath,
	companyYearScope,
} from "../company-navigation";
import { cockpitStyles } from "../design/cockpit.stylex";
import { colors, layout, spacing, typography } from "../design/tokens.stylex";
import { api } from "../lib/api";
import { useAuth } from "../lib/auth-context";
import { useAsync } from "../lib/useAsync";
import { useCapabilities } from "../lib/useCapabilities";
import { useCompanyShell, useCompanyYear, YearSelector } from "./CompanyNav";
import { Button, Dialog, Select } from "./ui";

const styles = stylex.create({
	shell: {
		display: { default: "grid", "@media print": "block" },
		gridTemplateColumns: {
			default: "248px minmax(0, 1fr)",
			"@media (max-width: 1023px)": "minmax(0, 1fr)",
		},
		maxWidth: layout.maxWidth,
		marginInline: "auto",
	},
	workspace: { gridTemplateColumns: "minmax(0, 1fr)" },
	main: {
		minWidth: 0,
		padding: { default: spacing.lg, "@media (max-width: 639px)": spacing.md },
	},
	aside: {
		padding: spacing.md,
		borderRightWidth: 1,
		borderRightStyle: "solid",
		borderRightColor: colors.border,
		display: {
			default: null,
			"@media (max-width: 1023px)": "none",
			"@media print": "none",
		},
	},
	group: { marginBottom: spacing.md },
	groupTitle: {
		display: "list-item",
		minHeight: 44,
		cursor: "pointer",
		fontSize: typography.sizeSm,
		fontWeight: 600,
		color: colors.inkMuted,
		paddingBlock: spacing.xs,
	},
	navLink: {
		display: "block",
		minHeight: 44,
		paddingBlock: spacing.xs,
		paddingInline: spacing.sm,
		color: colors.ink,
		borderRadius: 4,
		textDecoration: "none",
		backgroundColor: { default: null, ":hover": colors.paperRaised },
		outline: { default: null, ":focus-visible": "2px solid" },
		outlineColor: { default: null, ":focus-visible": colors.info },
		outlineOffset: { default: null, ":focus-visible": 2 },
	},
	active: {
		backgroundColor: colors.paperRaised,
		borderLeftWidth: 3,
		borderLeftStyle: "solid",
		borderLeftColor: colors.accent,
		fontWeight: 600,
	},
	context: {
		display: { default: "flex", "@media print": "none" },
		flexWrap: "wrap",
		alignItems: {
			default: "center",
			"@media (max-width: 639px)": "flex-start",
		},
		justifyContent: "space-between",
		gap: spacing.sm,
		paddingBottom: spacing.md,
		marginBottom: spacing.lg,
		borderBottomWidth: 1,
		borderBottomStyle: "solid",
		borderBottomColor: colors.border,
	},
	identity: { display: "grid", gap: spacing.xxs, minWidth: 0 },
	scope: { color: colors.inkMuted, fontSize: typography.sizeSm },
	picker: { maxWidth: "min(400px, 100%)" },
});
const restricted: Partial<Record<string, RoutePermission>> = {
	manage: "company.admin",
};
function TaskLinks({ onNavigate }: { onNavigate?: () => void }) {
	const location = useLocation();
	const route = companyRouteForPath(location.pathname);
	const slug = location.pathname.match(/^\/companies\/([^/]+)/)?.[1];
	const [params] = useSearchParams();
	const year = params.get("year");
	const { can } = useCapabilities(slug);
	if (!slug) return null;
	const priority = ["documents", "bank", "batch-bookkeeping", "drafts"];
	return (
		<nav
			aria-label="Virksomhedsnavigation"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			{COMPANY_TASK_AREAS.map((area) => {
				const pages = COMPANY_ROUTE_DEFINITIONS.filter(
					(entry) =>
						entry.kind === "page" &&
						entry.area === area.id &&
						can(restricted[entry.id] ?? "company.read"),
				).sort((a, b) => {
					const ai = priority.indexOf(a.id),
						bi = priority.indexOf(b.id);
					return (ai < 0 ? 99 : ai) - (bi < 0 ? 99 : bi);
				});
				if (pages.length === 0) return null;
				const destination =
					pages.find((entry) => entry.segment === area.destination) ??
					pages[0]!;
				const suffix = year ? `?year=${encodeURIComponent(year)}` : "";
				const nestedPages = pages.filter((entry) => entry.label !== area.label);
				return (
					<div
						key={area.id}
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							styles.group,
						)}
					>
						<NavLink
							to={`/companies/${slug}${destination.segment ? "/" + destination.segment : ""}${suffix}`}
							end
							aria-current={nestedPages.length > 0 ? false : undefined}
							onClick={onNavigate}
							{...stylex.props(
								cockpitStyles.aComposition,
								styles.navLink,
								area.id === route?.area && styles.active,
							)}
						>
							{area.label}
						</NavLink>
						{nestedPages.length > 0 && (
							<details
								open={area.id === route?.area}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<summary
									{...stylex.props(
										cockpitStyles.summaryComposition,
										styles.groupTitle,
									)}
								>
									Sider i {area.label}
								</summary>
								{nestedPages.map((entry) => {
									const active = entry.id === (route?.parentId ?? route?.id);
									return (
										<NavLink
											key={entry.id}
											to={`/companies/${slug}${entry.segment ? "/" + entry.segment : ""}${suffix}`}
											end
											onClick={onNavigate}
											aria-current={active ? "page" : undefined}
											{...stylex.props(
												cockpitStyles.aComposition,
												styles.navLink,
												active && styles.active,
											)}
										>
											{entry.label}
										</NavLink>
									);
								})}
							</details>
						)}
					</div>
				);
			})}
		</nav>
	);
}
export function CockpitLayout({ children }: { children: React.ReactNode }) {
	const location = useLocation();
	const route = companyRouteForPath(location.pathname);
	return (
		<div
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				!route && cockpitStyles.workspaceLayout,
				cockpitStyles.cockpitLayout,
				styles.shell,
				!route && styles.workspace,
			)}
		>
			{route && (
				<aside
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.aside,
						styles.aside,
					)}
				>
					<TaskLinks />
				</aside>
			)}
			<main
				id="main-content"
				tabIndex={-1}
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					styles.main,
				)}
			>
				{route && <CompanyContextHeader />}
				{children}
			</main>
		</div>
	);
}
function CompanyContextHeader() {
	const location = useLocation();
	const route = companyRouteForPath(location.pathname)!;
	const slug = location.pathname.match(/^\/companies\/([^/]+)/)?.[1] ?? "";
	const auth = useAuth();
	const { year, setYear } = useCompanyYear();
	const shell = useCompanyShell();
	const navigate = useNavigate();
	const [menu, setMenu] = useState(false);
	const menuId = useId();
	const switchRead = useRef<AbortController | null>(null);
	// In hosted mode the membership context already supplies names without extra access.
	const companies = useAsync<
		Array<{ slug: string; name: string; archived: boolean }>
	>(
		(signal) =>
			auth.hosted
				? Promise.resolve(auth.context?.companies ?? [])
				: api.companies({ signal }),
		[auth.hosted, auth.context],
	);
	const company = companies.data?.find((entry) => entry.slug === slug);
	// biome-ignore lint/correctness/useExhaustiveDependencies: navigation closes the menu regardless of the destination.
	useEffect(() => {
		setMenu(false);
	}, [location.pathname]);
	// A late year lookup must not undo a newer company choice or navigation.
	// biome-ignore lint/correctness/useExhaustiveDependencies: pathname changes invalidate the pending destination.
	useEffect(() => () => switchRead.current?.abort(), [location.pathname]);
	async function switchCompany(nextSlug: string) {
		if (!nextSlug) return;
		switchRead.current?.abort();
		const controller = new AbortController();
		switchRead.current = controller;
		let nextYear: string | undefined;
		if (year) {
			try {
				const years = await api.fiscalYears(nextSlug, {
					signal: controller.signal,
				});
				if (years.some((entry) => entry.label === year)) nextYear = year;
			} catch {
				/* Destination displays its own authoritative access/read error. */
			}
		}
		if (controller.signal.aborted) return;
		const permission = restricted[route.id] ?? "company.read";
		const membership = auth.context?.companies.find(
			(entry) => entry.slug === nextSlug,
		);
		const allowed =
			!auth.hosted ||
			(membership &&
				ROUTE_PERMISSION_POLICY[membership.role].includes(permission));
		const destination = route.kind === "page" && allowed ? route.segment : "";
		navigate(
			`/companies/${encodeURIComponent(nextSlug)}${destination ? "/" + destination : ""}${nextYear ? "?year=" + encodeURIComponent(nextYear) : ""}`,
		);
	}
	const scope = companyYearScope(route.id);
	return (
		<header
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.companyContext,
				styles.context,
			)}
		>
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					styles.identity,
				)}
			>
				<label
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.companyContextLabel,
					)}
				>
					Virksomhed{" "}
					<Select
						aria-label="Skift virksomhed"
						value={slug}
						onChange={(event) => void switchCompany(event.target.value)}
						xstyle={[
							cockpitStyles.companyContextSelectComposition,
							styles.picker,
						]}
					>
						{!company && (
							<option
								value={slug}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{slug}
							</option>
						)}
						{companies.data?.map((entry) => (
							<option
								value={entry.slug}
								key={entry.slug}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{entry.name}
								{entry.archived ? " (arkiv)" : ""}
							</option>
						))}
					</Select>
				</label>
				{company?.archived && (
					<span
						role="status"
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Arkiveret virksomhed · læseadgang
					</span>
				)}
				<span
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						styles.scope,
					)}
				>
					{scope === "company"
						? "Hele virksomheden"
						: scope === "multi-year"
							? "Flere regnskabsår"
							: scope === "vat-period"
								? "Momsperiode vælges på siden"
								: "Regnskabsår"}
				</span>
			</div>
			{scope === "year" && shell?.controls?.slug === slug && (
				<YearSelector
					years={shell.controls.years}
					selected={year ?? shell.controls.selected}
					onChange={setYear}
				/>
			)}
			<Button
				variant="secondary"
				aria-haspopup="dialog"
				aria-expanded={menu}
				aria-controls={menuId}
				onClick={() => setMenu(true)}
				xstyle={[cockpitStyles.mobileMenuTriggerComposition]}
			>
				Selskabsmenu
			</Button>
			{menu && (
				<Dialog
					id={menuId}
					title="Selskabsmenu"
					onClose={() => setMenu(false)}
					xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
				>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.navigationDialogActions,
						)}
					>
						<Button
							variant="secondary"
							onClick={() => setMenu(false)}
							xstyle={[cockpitStyles.buttonComposition]}
						>
							Luk selskabsmenu
						</Button>
					</div>
					<TaskLinks onNavigate={() => setMenu(false)} />
				</Dialog>
			)}
		</header>
	);
}
export function SkipLink() {
	return (
		<a
			href="#main-content"
			onClick={(event) => {
				event.preventDefault();
				document.getElementById("main-content")?.focus();
			}}
			{...stylex.props(cockpitStyles.skipLinkFocusComposition)}
		>
			Spring til indhold
		</a>
	);
}
