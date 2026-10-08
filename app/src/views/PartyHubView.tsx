import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import {
	Link,
	useNavigate,
	useParams,
	useSearchParams,
} from "react-router-dom";
import { FilterBar, PageState } from "../components/CockpitPrimitives";
import { PartySummary } from "../components/PartyLink";
import { Button, ButtonLink, Input, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { request } from "../lib/api/_shared";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";

type Hub = {
	rows: Array<{
		partyId: string;
		name: string;
		roles: string[];
		recentActivity: string | null;
		computedSpend: number;
		partyLink: { href: string };
	}>;
};
type Profile = {
	party: { name: string; kind: string; roles: Array<{ role: string }> };
	computed: {
		period: { from: string; to: string };
		asOf: string;
		purchase: { amount: number };
		sales: { amount: number };
		sourceCoverage: { documents: number };
	};
	research: {
		assertions: Array<{
			field: string;
			value: string;
			source: string;
			observed_at: string;
			review_state: string;
		}>;
		warnings: string[];
	};
	links: {
		documents: Array<{ id: number; label: string; role: string }>;
		relations: Array<{ type: string; companySlug: string; href: string }>;
	};
};
const endpoint = (slug: string, path = "") =>
	`/api/companies/${encodeURIComponent(slug)}/party-hub${path}`;
export function PartyHubView() {
	const { slug = "" } = useParams();
	const navigate = useNavigate();
	const [params] = useSearchParams();
	const [query, setQuery] = useState("");
	const context = params.toString() ? `?${params}` : "";
	const state = useAsync(
		(signal) =>
			request<Hub>(`${endpoint(slug)}?query=${encodeURIComponent(query)}`, {
				signal,
			}),
		[slug, query],
	);
	const denied = state.error && /403|forbudt|adgang/i.test(state.error);
	return (
		<section
			data-cockpit-page="party-hub"
			data-evidence-issue="653"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<PageHeader
				title="Parter"
				evidenceHeading
				actions={
					<ButtonLink
						variant="secondary"
						to={`/companies/${slug}/workspace-register`}
						xstyle={[cockpitStyles.aComposition]}
					>
						Styring og dokumentation
					</ButtonLink>
				}
			>
				<p
					data-evidence-progressive
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Søg og sortér canonical parter. Beløb vises kun fra eksplicitte
					partskoblinger.
				</p>
			</PageHeader>
			<FilterBar
				activeFilters={query ? [`Søgning: ${query}`] : []}
				onReset={() => setQuery("")}
			>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Søg{" "}
					<Input
						type="search"
						aria-label="Søg parter"
						value={query}
						onChange={(event) => setQuery(event.currentTarget.value)}
						xstyle={[cockpitStyles.pageInputComposition]}
					/>
				</label>
			</FilterBar>
			{state.loading && !state.data ? (
				<div
					data-evidence-status="loading"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<PageState kind="loading" title="Henter parter…" />
				</div>
			) : state.error ? (
				<div
					data-evidence-status={denied ? "warning-or-blocked" : "error"}
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{denied ? "Part kræver afklaring" : "Parter kunne ikke hentes"}
					</p>
					<PageState
						kind="error"
						title="Parter kunne ikke hentes"
						onRetry={state.reload}
					>
						{state.error}
					</PageState>
				</div>
			) : state.data?.rows.length ? (
				<>
					<p
						data-evidence-status="normal"
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Parter klar
					</p>
					<ul
						data-evidence-data
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.card,
						)}
					>
						{state.data.rows.map((row) => (
							<li
								key={row.partyId}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<Button
									type="button"
									data-evidence-core-action
									onClick={() =>
										navigate(
											`/companies/${slug}/parter/${row.partyId}${context}`,
										)
									}
									xstyle={[cockpitStyles.pageButtonComposition]}
								>
									<PartySummary name={row.name} roles={row.roles} />
								</Button>
								<span
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.muted,
									)}
								>
									{" "}
									· køb {formatKroner(row.computedSpend)} · senest{" "}
									{row.recentActivity ?? "ingen aktivitet"}
								</span>
							</li>
						))}
					</ul>
				</>
			) : (
				<div
					data-evidence-status="empty"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<PageState kind="empty" title="Ingen parter endnu">
						Der er ingen canonical parter med en synlig virksomhedsrolle.
					</PageState>
				</div>
			)}
		</section>
	);
}
export function PartyProfileView() {
	const { slug = "", partyId = "" } = useParams(),
		[params] = useSearchParams();
	const context = params.toString() ? `?${params}` : "";
	const state = useAsync(
		() =>
			request<Profile>(
				endpoint(slug, `/${encodeURIComponent(partyId)}${context}`),
			),
		[slug, partyId, context],
	);
	if (state.loading && !state.data)
		return <PageState kind="loading" title="Henter partsprofil" />;
	if (state.error)
		return (
			<PageState
				kind="error"
				title="Partsprofil kunne ikke hentes"
				onRetry={state.reload}
			>
				{state.error}
			</PageState>
		);
	const p = state.data!;
	return (
		<section
			data-cockpit-detail-route="party-profile"
			data-evidence-issue="655"
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<Link
				to={`/companies/${slug}/parter${context}`}
				{...stylex.props(cockpitStyles.pageBtnComposition)}
			>
				Tilbage til parter
			</Link>
			<header
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.pageHead,
				)}
			>
				<div
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Canonical part
					</p>
					<h2
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h2,
							cockpitStyles.pageHeadH2,
						)}
					>
						{p.party.name}
					</h2>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						{p.party.kind} · {p.party.roles.map((r) => r.role).join(", ")}
					</p>
				</div>
			</header>
			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Beregnede Rentemester-tal
				</h3>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					Periode {p.computed.period.from} – {p.computed.period.to} · pr.{" "}
					{p.computed.asOf}
				</p>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					Køb: {formatKroner(p.computed.purchase.amount)} · Salg:{" "}
					{formatKroner(p.computed.sales.amount)}
				</p>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Kildedækning: {p.computed.sourceCoverage.documents} eksplicit koblede
					bilag.
				</p>
			</section>
			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Kildeunderbygget research og review
				</h3>
				{p.research.warnings.map((w) => (
					<p
						role="alert"
						key={w}
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{w}
					</p>
				))}
				{p.research.assertions.length ? (
					<ul
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{p.research.assertions.map((a, index) => (
							<li
								key={index}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{a.field}: {a.value}{" "}
								<span
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.muted,
									)}
								>
									· {a.source} · {a.observed_at} · {a.review_state}
								</span>
							</li>
						))}
					</ul>
				) : (
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Ingen research-assertions.
					</p>
				)}
			</section>
			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Forbundne kilder og relationer
				</h3>
				<ul
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{p.links.documents.map((d) => (
						<li
							key={d.id}
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Bilag {d.label} · {d.role}
						</li>
					))}
					{p.links.relations.map((r) => (
						<li
							key={`${r.type}-${r.companySlug}`}
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							<a href={r.href} {...stylex.props(cockpitStyles.aComposition)}>
								{r.type}: {r.companySlug}
							</a>
						</li>
					))}
				</ul>
			</section>
		</section>
	);
}
