import * as stylex from "@stylexjs/stylex";
import { Input, PageHeader, Select } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Lovgrundlag-viewer (#347) — workspace-level read-only side der viser de
// danske regler Rentemester anvender, med klikbar SHA-256-citation pr.
// provision og deep-links til retsinformation.dk.
//
// Acceptkriterium fra #347: read-only (regler kan kun ændres via PR i
// `rules/dk/`), bruger `parseRuleBundle` + `readLegalSourceIds` via det nye
// `/api/rules`-endpoint, og citationerne er SHA-256-fingeraftryk pr. paragraf.

import { useMemo, useState } from "react";
import { ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import type { LegalSource, RuleSummary, RulesResponse } from "../lib/types";
import { useAsync } from "../lib/useAsync";

export function RulesView() {
	const state = useAsync<RulesResponse>((signal) => api.rules({ signal }), []);
	const [bundleFilter, setBundleFilter] = useState<string>("");
	const [search, setSearch] = useState("");

	const sources = useMemo(() => {
		const map = new Map<string, LegalSource>();
		for (const s of state.data?.legalSources ?? []) map.set(s.id, s);
		return map;
	}, [state.data]);

	const bundles = state.data?.ruleBundles ?? [];
	const allRules = state.data?.rules ?? [];

	const filteredRules = useMemo(() => {
		const needle = search.trim().toLowerCase();
		return allRules.filter((r) => {
			if (bundleFilter !== "" && r.bundle !== bundleFilter) return false;
			if (needle === "") return true;
			return (
				r.ruleId.toLowerCase().includes(needle) ||
				r.name.toLowerCase().includes(needle) ||
				r.explanation.toLowerCase().includes(needle) ||
				r.provisions.some((p) => p.ref.toLowerCase().includes(needle))
			);
		});
	}, [allRules, bundleFilter, search]);

	if (state.loading) return <Loading />;
	if (state.error) return <ErrorState message={state.error} />;

	return (
		<section
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<PageHeader title="Lovgrundlag">
				<div
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Rentemester citerer linje for linje fra retsinformation.dk via
						SHA-256-fingeraftryk. Siden viser hvilke regler der p.t. styrer
						bogføringen — og hvilken paragraf hver regel hænger på. Read-only:
						regler kan kun ændres ved en PR i{" "}
						<code
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.code,
							)}
						>
							rules/dk/
						</code>
						.
					</p>
				</div>
			</PageHeader>

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
					Aktive regelbundler
				</h3>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.tableScroll,
					)}
					data-ui="table-scroll"
				>
					<table
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<thead
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Bundle
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Version
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Antal regler
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Kilder
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									VAT-koder
								</th>
							</tr>
						</thead>
						<tbody
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{bundles.map((b) => (
								<tr
									key={b.name}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{b.name}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										<code
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.code,
											)}
										>
											{b.version}
										</code>
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{b.ruleCount}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{b.sources.join(", ")}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{b.vatCodes.join(", ") || "—"}
									</td>
								</tr>
							))}
						</tbody>
					</table>
				</div>
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
					Regler ({filteredRules.length} af {allRules.length})
				</h3>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.filterBar,
					)}
				>
					<label
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.filterBarLabel,
						)}
					>
						Bundle{" "}
						<Select
							value={bundleFilter}
							onChange={(e) => setBundleFilter(e.target.value)}
							xstyle={[cockpitStyles.filterBarSelectComposition]}
						>
							<option
								value=""
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Alle
							</option>
							{bundles.map((b) => (
								<option
									key={b.name}
									value={b.name}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{b.name}
								</option>
							))}
						</Select>
					</label>
					<label
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.filterBarLabel,
						)}
					>
						Søg{" "}
						<Input
							type="search"
							value={search}
							onChange={(e) => setSearch(e.target.value)}
							placeholder="regel-id, paragraf, ord …"
							xstyle={[cockpitStyles.filterBarInputComposition2]}
						/>
					</label>
				</div>

				<ul
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{filteredRules.map((r) => (
						<RuleEntry
							key={r.ruleId}
							rule={r}
							source={sources.get(r.sourceId) ?? null}
						/>
					))}
					{filteredRules.length === 0 && (
						<li
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.muted,
							)}
						>
							Ingen regler matcher filtret.
						</li>
					)}
				</ul>
			</section>
		</section>
	);
}

function RuleEntry({
	rule,
	source,
}: {
	rule: RuleSummary;
	source: LegalSource | null;
}) {
	return (
		<li {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
			<div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				<span
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<code
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.code,
						)}
					>
						{rule.ruleId}
					</code>
				</span>
				<span
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{rule.name}
				</span>
				<span
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{rule.severity || "info"}
				</span>
				<span
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{rule.category || "—"}
				</span>
				<span
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{rule.bundle}
				</span>
			</div>
			{rule.explanation && (
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					{rule.explanation}
				</p>
			)}
			<details
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<summary {...stylex.props(cockpitStyles.summaryComposition)}>
					Citationer ({rule.provisions.length})
				</summary>
				{source && (
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Kilde:{" "}
						<a
							href={source.url}
							target="_blank"
							rel="noopener noreferrer"
							{...stylex.props(cockpitStyles.aComposition)}
						>
							{source.title}
						</a>{" "}
						({source.authority})
					</p>
				)}
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.tableScroll,
					)}
					data-ui="table-scroll"
				>
					<table
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<thead
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Paragraf
								</th>
								<th
									{...stylex.props(
										cockpitStyles.tableDataTh,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									SHA-256-fingeraftryk
								</th>
							</tr>
						</thead>
						<tbody
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{rule.provisions.map((p, i) => (
								<tr
									key={`${rule.ruleId}-${i}`}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{p.ref}
									</td>
									<td
										{...stylex.props(
											cockpitStyles.tableDataTd,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										<code
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.code,
											)}
										>
											{p.textHash}
										</code>
									</td>
								</tr>
							))}
						</tbody>
					</table>
				</div>
			</details>
		</li>
	);
}
