import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import {
	Link,
	useLocation,
	useParams,
	useSearchParams,
} from "react-router-dom";
import { ErrorState, Loading } from "../components/Feedback";
import {
	Button,
	ButtonLink,
	Field,
	Input,
	PageHeader,
	Select,
} from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { colors, rounded, spacing } from "../design/tokens.stylex";
import { api } from "../lib/api";
import {
	boardFor,
	canWriteScope,
	outcomeLabel,
	referenceHref,
	scopeLabel,
	statusOptions,
	type Task,
	type TaskProjection,
	type TaskScope,
	type TaskSeries,
	type TasksView as TasksData,
	taskQuery,
	taskReturnTo,
	taskSourceHref,
	taskStatusLabel,
	taskTypeLabel,
	taskUrgency,
	today,
} from "../lib/tasks";
import { useAsync } from "../lib/useAsync";
import { useTaskMutation } from "../lib/useTaskMutation";
import {
	BoardEditor,
	CompletionEditor,
	ReminderEditor,
	SeriesEditor,
	TaskEditor,
	taskAssignees,
} from "./TaskForms";

const styles = stylex.create({
	inlineLink: { textDecorationLine: "underline" },
	viewGroup: {
		display: "flex",
		flexWrap: "wrap",
		gap: spacing.xxs,
		paddingBottom: spacing.sm,
		borderBottomWidth: 1,
		borderBottomStyle: "solid",
		borderBottomColor: colors.border,
		marginBottom: spacing.md,
	},
	viewChoice: {
		borderColor: "transparent",
		backgroundColor: { default: "transparent", ":hover": colors.paperRaised },
		color: colors.inkMuted,
	},
	selectedView: {
		color: colors.ink,
		backgroundColor: colors.paperRaised,
		borderColor: colors.borderStrong,
		fontWeight: 600,
	},
	selectedFilter: {
		borderColor: colors.ink,
		backgroundColor: colors.paper,
		fontWeight: 600,
	},
	scope: {
		color: colors.inkMuted,
		fontSize: "14px",
		marginTop: 0,
		marginBottom: spacing.sm,
	},
	filterSummary: {
		display: "flex",
		flexWrap: "wrap",
		alignItems: "center",
		gap: spacing.sm,
		color: colors.inkMuted,
		fontSize: "14px",
		marginBlock: spacing.sm,
	},

	page: { minWidth: 0, overflowWrap: "anywhere" },
	toolbar: {
		display: "flex",
		flexWrap: "wrap",
		gap: spacing.sm,
		alignItems: "end",
		marginBottom: spacing.md,
	},
	filters: {
		display: "grid",
		gridTemplateColumns: "repeat(auto-fit, minmax(min(100%, 190px), 1fr))",
		gap: spacing.sm,
		marginBlock: spacing.md,
	},
	list: { display: "grid", gap: spacing.sm, padding: 0, listStyle: "none" },
	card: {
		minWidth: 0,
		display: "grid",
		gap: spacing.xs,
		padding: spacing.md,
		backgroundColor: colors.paperRaised,
		borderWidth: 1,
		borderStyle: "solid",
		borderColor: colors.border,
		borderRadius: rounded.md,
	},
	title: { margin: 0, fontSize: "18px", overflowWrap: "anywhere" },
	meta: { margin: 0, color: colors.inkMuted },
	actions: { display: "flex", flexWrap: "wrap", gap: spacing.xs },
	board: {
		display: "grid",
		gridTemplateColumns: "repeat(auto-fit, minmax(min(100%, 220px), 1fr))",
		gap: spacing.md,
		alignItems: "start",
	},
	column: {
		minWidth: 0,
		backgroundColor: colors.paper,
		padding: spacing.sm,
		borderWidth: 1,
		borderStyle: "solid",
		borderColor: colors.border,
		borderRadius: rounded.md,
	},
	calendar: {
		display: "grid",
		gridTemplateColumns: {
			default: "repeat(7, minmax(0, 1fr))",
			"@media (max-width: 639px)": "1fr",
		},
		gap: spacing.xs,
	},
	date: {
		padding: spacing.xs,
		minWidth: 0,
		borderWidth: 1,
		borderStyle: "solid",
		borderColor: colors.border,
		backgroundColor: colors.paperRaised,
	},
	dateList: { listStyle: "none", padding: 0, display: "grid", gap: spacing.xs },
	wheel: {
		display: "grid",
		gridTemplateColumns: "repeat(auto-fit, minmax(min(100%, 250px), 1fr))",
		gap: spacing.md,
	},
	detail: {
		display: "grid",
		gridTemplateColumns: "minmax(0, 1fr)",
		gap: spacing.md,
	},
	badge: { fontWeight: 600, color: colors.warning },
});
type ViewName = "list" | "calendar" | "kanban" | "wheel";
const tabs: Array<[ViewName, string]> = [
	["list", "Liste"],
	["calendar", "Kalender"],
	["kanban", "Kanban"],
	["wheel", "Årshjul"],
];
function monthName(month: string) {
	return new Intl.DateTimeFormat("da-DK", {
		month: "long",
		year: "numeric",
		timeZone: "UTC",
	}).format(new Date(`${month}-01T12:00:00Z`));
}
function dateAdd(date: string, days: number) {
	const parsed = new Date(`${date}T12:00:00Z`);
	parsed.setUTCDate(parsed.getUTCDate() + days);
	return parsed.toISOString().slice(0, 10);
}
function validDate(value: string | null) {
	return value &&
		/^\d{4}-\d{2}-\d{2}$/.test(value) &&
		!Number.isNaN(new Date(`${value}T12:00:00Z`).getTime())
		? value
		: today();
}
function validYear(value: string | null) {
	return value &&
		/^\d{4}$/.test(value) &&
		Number(value) >= 1900 &&
		Number(value) <= 9998
		? Number(value)
		: Number(today().slice(0, 4));
}
function calendarRange(date: string, mode: string) {
	if (mode === "week") {
		const weekday = new Date(`${date}T12:00:00Z`).getUTCDay();
		const from = dateAdd(date, -(weekday === 0 ? 6 : weekday - 1));
		return { from, to: dateAdd(from, 6) };
	}
	const from = `${date.slice(0, 7)}-01`;
	const last = new Date(`${from}T12:00:00Z`);
	last.setUTCMonth(last.getUTCMonth() + 1, 0);
	return { from, to: last.toISOString().slice(0, 10) };
}
function dateSequence(from: string, to: string) {
	const result: string[] = [];
	for (
		let cursor = from;
		cursor <= to && result.length < 370;
		cursor = dateAdd(cursor, 1)
	)
		result.push(cursor);
	return result;
}

export function TasksView() {
	const { slug } = useParams();
	const location = useLocation();
	const [params, setParams] = useSearchParams();
	const listParams = new URLSearchParams(params);
	listParams.set("view", "list");
	const listHref = `${location.pathname}?${listParams}`;
	const view = tabs.some(([tab]) => tab === params.get("view"))
		? (params.get("view") as ViewName)
		: "list";
	const calendarMode = ["month", "week", "agenda"].includes(
		params.get("calendar") ?? "",
	)
		? params.get("calendar")!
		: typeof window !== "undefined" &&
				window.matchMedia?.("(max-width: 639px)").matches
			? "agenda"
			: "month";
	const date = validDate(params.get("date"));
	const wheelYear = validYear(params.get("wheelYear"));
	const wheelMonthRaw = Number(params.get("wheelStartMonth") ?? 1);
	const wheelMonth =
		Number.isInteger(wheelMonthRaw) && wheelMonthRaw >= 1 && wheelMonthRaw <= 12
			? wheelMonthRaw
			: 1;
	const range =
		view === "wheel"
			? {
					from: `${wheelYear}-${String(wheelMonth).padStart(2, "0")}-01`,
					to: dateAdd(
						`${wheelYear + 1}-${String(wheelMonth).padStart(2, "0")}-01`,
						-1,
					),
				}
			: calendarRange(date, calendarMode);
	const query = {
		...taskQuery(params, slug),
		...(view === "wheel" ? { showDone: true } : {}),
	};
	const key = JSON.stringify(query);
	const state = useAsync(
		async (signal) => {
			const base = await api.tasks(query, { signal });
			if (view === "calendar" || view === "wheel") {
				const projected = await api.tasks(
					{ ...query, from: range.from, to: range.to },
					{ signal },
				);
				return { ...base, projections: projected.projections };
			}
			return base;
		},
		[slug, key, view, range.from, range.to],
	);
	const [editor, setEditor] = useState(false);
	const [seriesEditor, setSeriesEditor] = useState<TaskSeries | "new" | null>(
		null,
	);
	const [boardEditor, setBoardEditor] = useState(false);
	const [sourceErrors, setSourceErrors] = useState<
		Array<{ companySlug: string; reason: string }>
	>([]);
	const write = useTaskMutation(state.reload, "sync");
	function setParam(key: string, value: string) {
		const next = new URLSearchParams(params);
		if (value) next.set(key, value);
		else next.delete(key);
		setParams(next, { replace: true });
	}
	if (state.loading && !state.data) return <Loading label="Henter opgaver…" />;
	if (!state.data)
		return (
			<ErrorState
				message={state.error ?? "Opgaver kunne ikke hentes."}
				onRetry={state.reload}
			/>
		);
	const data = state.data;
	const board = boardFor(data, slug);
	const selectedCompanies = query.companySlugs ?? [];
	const initialScope: TaskScope | undefined =
		slug || selectedCompanies.length === 1
			? { kind: "company", companySlug: slug ?? selectedCompanies[0]! }
			: undefined;
	const canCreate =
		data.companies.some((company) => company.canWrite) ||
		data.canManageWorkspace;
	const canManage = canWriteScope(board.scope, data, true);
	const syncCompanies = data.companies
		.filter(
			(company) =>
				company.canWrite &&
				!company.archived &&
				(selectedCompanies.length === 0 ||
					selectedCompanies.includes(company.slug)),
		)
		.map((company) => company.slug);
	const focus = params.get("focus") ?? "";
	const externalAssignee = params.get("assigneeId")?.startsWith("external:")
		? params.get("assigneeId")!.slice(9)
		: null;
	const externalNames = [
		...new Set(
			data.tasks.flatMap((task) =>
				task.assignee?.kind === "external" ? [task.assignee.name] : [],
			),
		),
	];
	if (externalAssignee && !externalNames.includes(externalAssignee))
		externalNames.push(externalAssignee);
	const filtered = data.tasks
		.filter(
			(task) =>
				!externalAssignee ||
				(task.assignee?.kind === "external" &&
					task.assignee.name === externalAssignee),
		)
		.filter((task) =>
			focus === "overdue"
				? task.status !== "done" &&
					!!task.deadline &&
					task.deadline.date < today()
				: focus === "this_month"
					? [task.workDate, task.deadline?.date].some(
							(value) => value?.slice(0, 7) === today().slice(0, 7),
						)
					: focus === "waiting_me"
						? task.status === "waiting" &&
							task.assignee?.kind === "member" &&
							task.assignee.userId === data.currentUserId
						: true,
		);
	const tasks = [...filtered].sort(
		(a, b) =>
			(a.status === "done" ? 1 : 0) - (b.status === "done" ? 1 : 0) ||
			(a.deadline?.date ?? a.workDate ?? "9999").localeCompare(
				b.deadline?.date ?? b.workDate ?? "9999",
			) ||
			a.title.localeCompare(b.title, "da"),
	);
	const activeFilterLabels = [
		...selectedCompanies.map(
			(value) =>
				data.companies.find((company) => company.slug === value)?.name ?? value,
		),
		params.get("search") && `Søg: ${params.get("search")}`,
		params.get("status") &&
			`Status: ${statusOptions.find(([value]) => value === params.get("status"))?.[1] ?? params.get("status")}`,
		params.get("type") &&
			`Type: ${taskTypeLabel[params.get("type") as Task["type"]] ?? params.get("type")}`,
		params.get("assigneeId") &&
			`Ansvarlig: ${externalAssignee ?? taskAssignees(data).find((member) => member.userId === params.get("assigneeId"))?.name ?? params.get("assigneeId")}`,
		params.get("includeArchived") === "true" && "Arkiverede selskaber",
		params.get("showDone") === "true" && "Afsluttede opgaver",
		params.get("undated") === "true" && "Uden dato",
		params.get("unassigned") === "true" && "Ikke tildelt",
	].filter(Boolean);
	function clearFilters() {
		const next = new URLSearchParams(params);
		for (const key of [
			"companySlug",
			"search",
			"status",
			"type",
			"assigneeId",
			"includeArchived",
			"showDone",
			"undated",
			"unassigned",
			"focus",
		])
			next.delete(key);
		setParams(next, { replace: true });
	}
	const returnTo = `${location.pathname}${location.search}`;
	const visibleSourceErrors = sourceErrors.filter(
		(error) =>
			data.companies.some((company) => company.slug === error.companySlug) &&
			(selectedCompanies.length === 0 ||
				selectedCompanies.includes(error.companySlug)),
	);
	async function sync() {
		const result = await write.run<{
			sync: { errors: Array<{ companySlug: string; reason: string }> };
		}>("/api/tasks/sync", { companySlugs: syncCompanies });
		if (result) setSourceErrors(result.sync.errors);
	}
	async function materialize() {
		await write.run("/api/task-series/materialize", {
			asOfDate: today(),
			...(selectedCompanies.length > 0
				? { companySlugs: selectedCompanies }
				: {}),
		});
	}
	return (
		<section
			data-cockpit-page="tasks"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				styles.page,
			)}
		>
			<PageHeader
				title="Opgaver"
				description={
					slug
						? `${data.companies.find((company) => company.slug === slug)?.name ?? slug} · Planlægning og dokumenteret afslutning`
						: "Samlet økonomiarbejde på tværs af dine selskaber"
				}
				actions={
					<>
						{canCreate && (
							<Button
								onClick={() => setEditor(true)}
								xstyle={[cockpitStyles.buttonComposition]}
							>
								Opret opgave
							</Button>
						)}
						{syncCompanies.length > 0 && (
							<Button
								variant="secondary"
								busy={write.busy}
								disabled={write.blocked}
								onClick={() => void sync()}
								xstyle={[cockpitStyles.buttonComposition]}
							>
								Opdatér fra kilder
							</Button>
						)}
					</>
				}
			/>
			{write.feedback}
			{state.error && (
				<p
					role="alert"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.banner,
						cockpitStyles.bannerWarning,
					)}
				>
					{state.error} Senest hentede data vises. Formularer bevares.
				</p>
			)}
			{visibleSourceErrors.length > 0 && (
				<div
					role="alert"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.banner,
						cockpitStyles.bannerWarning,
					)}
				>
					<h2
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h2,
						)}
					>
						Kildegrundlaget er ufuldstændigt
					</h2>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Disse selskabers kilder kunne ikke gennemlæses. En tom liste betyder
						ikke, at arbejdet er afsluttet. Kontrollér fejlen og opdatér fra
						kilder igen.
					</p>
					<ul
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{visibleSourceErrors.map((error) => (
							<li
								key={error.companySlug}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{
									data.companies.find(
										(company) => company.slug === error.companySlug,
									)?.name
								}
								: {error.reason}
							</li>
						))}
					</ul>
				</div>
			)}
			{data.notifications.length > 0 && (
				<details
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
					)}
				>
					<summary {...stylex.props(cockpitStyles.summaryComposition)}>
						Påmindelser til dig ({data.notifications.length})
					</summary>
					<ul
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{data.notifications.map((notification) => (
							<li
								key={notification.notificationId}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<Link
									to={`/opgaver/${encodeURIComponent(notification.taskId)}?${new URLSearchParams({ returnTo })}`}
									{...stylex.props(cockpitStyles.aComposition)}
								>
									{notification.title}
								</Link>{" "}
								· {notification.createdAt.slice(0, 10)}
							</li>
						))}
					</ul>
				</details>
			)}
			<p role="note" {...stylex.props(styles.scope)} aria-label="Opgavernes selskabsscope">
				{slug
					? `Selskab: ${scopeLabel({ kind: "company", companySlug: slug }, data)}`
					: selectedCompanies.length
						? `Selskaber: ${selectedCompanies.map((value) => data.companies.find((company) => company.slug === value)?.name ?? value).join(", ")}`
						: "Alle selskaber med adgang"}
			</p>
			<div
				role="group"
				aria-label="Opgavevisning"
				{...stylex.props(styles.viewGroup)}
			>
				{tabs.map(([tab, label]) => (
					<Button
						key={tab}
						variant="quiet"
						aria-pressed={view === tab}
						onClick={() => setParam("view", tab)}
						xstyle={[styles.viewChoice, view === tab && styles.selectedView]}
					>
						{label}
					</Button>
				))}
			</div>
			<div
				role="group"
				aria-label="Hurtigfiltre"
				{...stylex.props(styles.toolbar)}
			>
				{[
					["", "Alle åbne"],
					["overdue", "Forfaldent"],
					["this_month", "Denne måned"],
					["waiting_me", "Afventer mig"],
				].map(([value, label]) => (
					<Button
						key={value}
						variant="secondary"
						aria-pressed={focus === value}
						onClick={() => setParam("focus", value!)}
						xstyle={focus === value && styles.selectedFilter}
					>
						{label}
					</Button>
				))}
			</div>
			{activeFilterLabels.length > 0 && (
				<div
					aria-label="Aktive opgavefiltre"
                    role="group"
					aria-live="polite"
					{...stylex.props(styles.filterSummary)}
				>
					<span>Aktive filtre: {activeFilterLabels.join(" · ")}</span>
					<Button variant="quiet" onClick={clearFilters}>
						Ryd filtre
					</Button>
				</div>
			)}
			<details
				open={!!params.get("search")}
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<summary {...stylex.props(cockpitStyles.summaryComposition)}>
					Filtre og selskabsvalg
				</summary>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						styles.filters,
					)}
				>
					{!slug && (
						<fieldset
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							<legend
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Vis selskaber
							</legend>
							{data.companies.map((company) => (
								<label
									key={company.slug}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<Input
										type="checkbox"
										checked={selectedCompanies.includes(company.slug)}
										onChange={(event) => {
											const next = new URLSearchParams(params);
											next.delete("companySlug");
											const slugs = event.target.checked
												? [...selectedCompanies, company.slug]
												: selectedCompanies.filter(
														(value) => value !== company.slug,
													);
											for (const value of slugs)
												next.append("companySlug", value);
											setParams(next, { replace: true });
										}}
										xstyle={[cockpitStyles.inputComposition]}
									/>{" "}
									{company.name}
									{company.archived ? " · arkiveret" : ""}
									<br
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									/>
								</label>
							))}
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.muted,
								)}
							>
								Intet valgt: alle selskaber med adgang.
							</p>
						</fieldset>
					)}
					<Field label="Søg titel eller reference">
						<Input
							type="search"
							value={params.get("search") ?? ""}
							onChange={(event) => setParam("search", event.target.value)}
							xstyle={[cockpitStyles.inputComposition]}
						/>
					</Field>
					<Field label="Statusfilter">
						<Select
							value={params.get("status") ?? ""}
							onChange={(event) => setParam("status", event.target.value)}
							xstyle={[cockpitStyles.selectComposition]}
						>
							<option
								value=""
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Alle statusser
							</option>
							{statusOptions.map(([status, label]) => (
								<option
									key={status}
									value={status}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{label}
								</option>
							))}
						</Select>
					</Field>
					<Field label="Typefilter">
						<Select
							value={params.get("type") ?? ""}
							onChange={(event) => setParam("type", event.target.value)}
							xstyle={[cockpitStyles.selectComposition]}
						>
							<option
								value=""
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Alle typer
							</option>
							{Object.entries(taskTypeLabel).map(([type, label]) => (
								<option
									key={type}
									value={type}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{label}
								</option>
							))}
						</Select>
					</Field>
					<Field label="Ansvarligfilter">
						<Select
							value={params.get("assigneeId") ?? ""}
							onChange={(event) => setParam("assigneeId", event.target.value)}
							xstyle={[cockpitStyles.selectComposition]}
						>
							<option
								value=""
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Alle ansvarlige
							</option>
							{taskAssignees(data).map((member) => (
								<option
									key={member.userId}
									value={member.userId}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{member.name}
								</option>
							))}
							{externalNames.map((name) => (
								<option
									key={`external:${name}`}
									value={`external:${name}`}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{name} · ekstern
								</option>
							))}
						</Select>
					</Field>
					<Field label="Fra dato (eksplicit afgrænsning)">
						<Input
							type="date"
							value={params.get("from") ?? ""}
							onChange={(event) => setParam("from", event.target.value)}
							xstyle={[cockpitStyles.inputComposition]}
						/>
					</Field>
					<Field label="Til dato">
						<Input
							type="date"
							value={params.get("to") ?? ""}
							onChange={(event) => setParam("to", event.target.value)}
							xstyle={[cockpitStyles.inputComposition]}
						/>
					</Field>
					{(
						[
							["undated", "Uden dato"],
							["unassigned", "Uden ansvarlig"],
							["showDone", "Vis afsluttede og historik"],
							["includeArchived", "Vis arkiverede selskabers opgaver"],
						] as const
					).map(([key, label]) => (
						<label
							key={key}
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							<Input
								type="checkbox"
								checked={params.get(key) === "true"}
								onChange={(event) =>
									setParam(key, event.target.checked ? "true" : "")
								}
								xstyle={[cockpitStyles.inputComposition]}
							/>{" "}
							{label}
						</label>
					))}
					<Button
						variant="secondary"
						onClick={() => {
							const next = new URLSearchParams();
							next.set("view", view);
							setParams(next, { replace: true });
						}}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						Nulstil filtre
					</Button>
				</div>
			</details>
			<p
				role="status"
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				{tasks.length} opgaver ·{" "}
				{data.runtime.running
					? "Påmindelsesruntime kører"
					: "Påmindelsesruntime er inaktiv"}
			</p>
			{view === "list" && (
				<TaskList
					tasks={tasks}
					data={data}
					returnTo={returnTo}
					onSaved={state.reload}
				/>
			)}
			{view === "kanban" && (
				<>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							styles.toolbar,
						)}
					>
						<h2
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.h2,
							)}
						>
							Kanban
						</h2>
						{canManage && (
							<Button
								variant="secondary"
								onClick={() => setBoardEditor(true)}
								xstyle={[cockpitStyles.buttonComposition]}
							>
								Tilpas kolonner
							</Button>
						)}
					</div>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							styles.board,
						)}
					>
						{board.columns.map((column) => {
							const columnTasks = tasks.filter((task) => {
								const saved = slug ? task.columnId : task.workspaceColumnId;
								const target =
									board.columns.find(
										(col) =>
											col.columnId === saved && col.status === task.status,
									) ??
									board.columns.find(
										(col) => col.status === task.status && col.isDefault,
									) ??
									board.columns.find((col) => col.status === task.status);
								return target?.columnId === column.columnId;
							});
							return (
								<section
									key={column.columnId}
									aria-label={`Kolonne ${column.name}`}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										styles.column,
									)}
								>
									<h2
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.h2,
										)}
									>
										{column.name} ({columnTasks.length})
									</h2>
									<p
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
										)}
									>
										{taskStatusLabel[column.status]}
										{column.isDefault ? " · standardvalg" : ""}
									</p>
									<TaskList
										tasks={columnTasks}
										data={data}
										returnTo={returnTo}
										onSaved={state.reload}
										compact
									/>
								</section>
							);
						})}
					</div>
				</>
			)}
			{view === "calendar" && (
				<>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							styles.toolbar,
						)}
					>
						<Field label="Kalenderdato">
							<Input
								type="date"
								value={date}
								onChange={(event) => setParam("date", event.target.value)}
								xstyle={[cockpitStyles.inputComposition]}
							/>
						</Field>
						<Field label="Kalendervisning">
							<Select
								value={calendarMode}
								onChange={(event) => setParam("calendar", event.target.value)}
								xstyle={[cockpitStyles.selectComposition]}
							>
								<option
									value="month"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Måned
								</option>
								<option
									value="week"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Uge
								</option>
								<option
									value="agenda"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Agenda
								</option>
							</Select>
						</Field>
					</div>
					<h2
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h2,
						)}
					>
						{calendarMode === "week"
							? `${range.from} – ${range.to}`
							: monthName(date.slice(0, 7))}
					</h2>
					<Calendar
						tasks={tasks}
						projections={data.projections}
						data={data}
						returnTo={returnTo}
						range={range}
						mode={calendarMode}
					/>
					<h2
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h2,
						)}
					>
						Uden dato (
						{tasks.filter((task) => !task.workDate && !task.deadline).length})
					</h2>
					<TaskList
						tasks={tasks.filter((task) => !task.workDate && !task.deadline)}
						data={data}
						returnTo={returnTo}
						onSaved={state.reload}
					/>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Arbejdsdato og frist vises særskilt. Hver opgave tæller én gang i
						oversigten.{" "}
						<Link
							to={listHref}
							{...stylex.props(cockpitStyles.aComposition, styles.inlineLink)}
						>
							Se også udestående arbejde uden for kalenderperioden
						</Link>
						.
					</p>
				</>
			)}
			{view === "wheel" && (
				<>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							styles.toolbar,
						)}
					>
						<Field label="Årshjulets startår">
							<Input
								type="number"
								min={1900}
								max={9998}
								value={wheelYear}
								onChange={(event) => setParam("wheelYear", event.target.value)}
								xstyle={[cockpitStyles.inputComposition]}
							/>
						</Field>
						<Field label="Årshjulets startmåned">
							<Select
								value={wheelMonth}
								onChange={(event) =>
									setParam("wheelStartMonth", event.target.value)
								}
								xstyle={[cockpitStyles.selectComposition]}
							>
								{Array.from({ length: 12 }, (_, i) => (
									<option
										key={i}
										value={i + 1}
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{
											monthName(`2026-${String(i + 1).padStart(2, "0")}`).split(
												" ",
											)[0]
										}
									</option>
								))}
							</Select>
						</Field>
						{(data.companies.some((company) => company.canManage) ||
							data.canManageWorkspace) && (
							<Button
								onClick={() => setSeriesEditor("new")}
								xstyle={[cockpitStyles.buttonComposition]}
							>
								Opret rutine
							</Button>
						)}
						{canCreate && (
							<Button
								variant="secondary"
								busy={write.busy}
								disabled={write.blocked}
								onClick={() => void materialize()}
								xstyle={[cockpitStyles.buttonComposition]}
							>
								Opret aktuelle forekomster
							</Button>
						)}
					</div>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Perioder følger hver rutines kalender- eller regnskabsår. Forventede
						gentagelser er planlægning; de er endnu ikke konkrete opgaver.
						Frister med ukendt grundlag skal afklares.
					</p>
					<YearWheel
						data={data}
						tasks={tasks}
						range={range}
						returnTo={returnTo}
					/>
					<h2
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h2,
						)}
					>
						Rutiner
					</h2>
					{data.series.length === 0 ? (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Ingen rutiner oprettet.
						</p>
					) : (
						<ul
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								styles.list,
							)}
						>
							{data.series.map((series) => (
								<li
									key={series.seriesId}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										styles.card,
									)}
								>
									<h3
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.h3,
										)}
									>
										{series.title}
									</h3>
									<p
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{scopeLabel(series.scope, data)} ·{" "}
										{series.active ? "Aktiv" : "Pauseret"} ·{" "}
										{series.anchor === "fiscal"
											? "Regnskabsperioder"
											: "Kalenderperioder"}
									</p>
									{canWriteScope(series.scope, data, true) && (
										<Button
											variant="secondary"
											onClick={() => setSeriesEditor(series)}
											xstyle={[cockpitStyles.buttonComposition]}
										>
											Redigér eller pausér fremtidige gentagelser
										</Button>
									)}
								</li>
							))}
						</ul>
					)}
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<Link
							to={listHref}
							{...stylex.props(cockpitStyles.aComposition, styles.inlineLink)}
						>
							Se alle udestående forekomster, også fra tidligere år
						</Link>
					</p>
				</>
			)}
			<details
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<summary {...stylex.props(cockpitStyles.summaryComposition)}>
					Kildedækning og grundlag
				</summary>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					Opgaver viser arbejdsstatus. Regnskabets korrekthed og overholdelse
					skal verificeres ved kilden.
				</p>
				{data.sourceCoverage.length > 0 ? (
					<ul
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{data.sourceCoverage.map((source) => (
							<li
								key={source}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{source}
							</li>
						))}
					</ul>
				) : (
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Kildedækning er ikke verificeret. Brug Opdatér fra kilder for at
						hente kendt arbejde.
					</p>
				)}
			</details>
			{editor && (
				<TaskEditor
					view={data}
					initialScope={initialScope}
					onSaved={state.reload}
					onClose={() => setEditor(false)}
				/>
			)}
			{seriesEditor && (
				<SeriesEditor
					view={data}
					series={seriesEditor === "new" ? undefined : seriesEditor}
					initialScope={initialScope}
					onSaved={state.reload}
					onClose={() => setSeriesEditor(null)}
				/>
			)}
			{boardEditor && (
				<BoardEditor
					view={data}
					board={board}
					onSaved={state.reload}
					onClose={() => setBoardEditor(false)}
				/>
			)}
		</section>
	);
}

function TaskList({
	tasks,
	data,
	returnTo,
	onSaved,
	compact = false,
}: {
	tasks: Task[];
	data: TasksData;
	returnTo: string;
	onSaved: () => void;
	compact?: boolean;
}) {
	const { slug } = useParams();
	if (!tasks.length)
		return (
			<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				Ingen gemte opgaver matcher denne visning.
				{!compact &&
					" Kilder opdateres med Opdatér fra kilder; en tom liste beviser ikke, at alt arbejde er afsluttet."}
			</p>
		);
	return (
		<ul
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				styles.list,
			)}
		>
			{tasks.map((task) => (
				<li
					key={task.taskId}
					data-task-id={task.taskId}
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						styles.card,
					)}
				>
					<h3
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							styles.title,
						)}
					>
						<Link
							to={`/opgaver/${encodeURIComponent(task.taskId)}?${new URLSearchParams({ returnTo })}`}
							{...stylex.props(cockpitStyles.aComposition)}
						>
							{task.title}
						</Link>
					</h3>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							styles.meta,
						)}
					>
						{scopeLabel(task.scope, data)} · {taskTypeLabel[task.type]} ·{" "}
						{task.assignee?.name ?? "Ikke tildelt"}
					</p>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							styles.meta,
						)}
					>
						{task.period?.label}
						{task.workDate ? ` · Arbejdsdato ${task.workDate}` : ""}
						{task.deadline
							? ` · ${task.deadline.kind === "statutory" ? "Myndighedsfrist" : "Frist"} ${task.deadline.date}`
							: ""}
					</p>
					<span
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							styles.badge,
						)}
					>
						{taskUrgency(task)}
					</span>
					{task.origin === "proposal" && (
						<>
							<strong
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Forslag · skal vurderes
							</strong>
							{canWriteScope(task.scope, data) && (
								<AcceptProposal task={task} onSaved={onSaved} />
							)}
						</>
					)}
					{task.waitingOn && (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Afventer {task.waitingOn}
						</p>
					)}
					{task.nextAction && (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{task.nextAction}
						</p>
					)}
					{canWriteScope(task.scope, data) && (
						<TaskMove
							task={task}
							data={data}
							aggregate={!slug}
							onSaved={onSaved}
						/>
					)}
				</li>
			))}
		</ul>
	);
}

function AcceptProposal({
	task,
	onSaved,
}: {
	task: Task;
	onSaved: () => void;
}) {
	const write = useTaskMutation(onSaved, `accept:${task.taskId}`);
	return (
		<div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
			{write.feedback}
			<Button
				variant="secondary"
				busy={write.busy}
				disabled={write.blocked}
				onClick={() =>
					void write.run(
						`/api/tasks/${encodeURIComponent(task.taskId)}/update`,
						{ patch: { relevance: "relevant" }, expectedVersion: task.version },
					)
				}
				xstyle={[cockpitStyles.buttonComposition]}
			>
				Accepter forslag
			</Button>
		</div>
	);
}

function TaskMove({
	task,
	data,
	aggregate,
	onSaved,
}: {
	task: Task;
	data: TasksData;
	aggregate: boolean;
	onSaved: () => void;
}) {
	const local = boardFor(
		data,
		task.scope.kind === "company" ? task.scope.companySlug : undefined,
	);
	const targetBoard = aggregate ? boardFor(data) : local;
	const [selection, setSelection] = useState("");
	const [localTarget, setLocalTarget] = useState("");
	const [complete, setComplete] = useState(false);
	const [reopen, setReopen] = useState(false);
	const selected = targetBoard.columns.find(
		(column) => column.columnId === selection,
	);
	const candidates = selected
		? local.columns.filter((column) => column.status === selected.status)
		: [];
	const localDefault = candidates.find((column) => column.isDefault);
	const ambiguous =
		aggregate &&
		task.scope.kind === "company" &&
		candidates.length > 1 &&
		!localDefault;
	const write = useTaskMutation(onSaved, `move:${task.taskId}`);
	async function move() {
		if (!selected) return;
		if (selected.status === "done") {
			setComplete(true);
			return;
		}
		if (ambiguous && !localTarget) return;
		await write.run(`/api/tasks/${encodeURIComponent(task.taskId)}/move`, {
			status: selected.status,
			expectedVersion: task.version,
			...(aggregate
				? {
						workspaceColumnId: selection,
						...(task.scope.kind === "company"
							? {
									columnId:
										localTarget ||
										localDefault?.columnId ||
										candidates[0]?.columnId,
								}
							: {}),
					}
				: { columnId: selection }),
		});
		setSelection("");
		setLocalTarget("");
	}
	if (task.status === "done")
		return (
			<div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				<Button
					variant="secondary"
					onClick={() => setReopen(true)}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					Genåbn med årsag
				</Button>
				{reopen && (
					<CompletionEditor
						task={task}
						reopen
						onSaved={onSaved}
						onClose={() => setReopen(false)}
					/>
				)}
			</div>
		);
	return (
		<div
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				styles.detail,
			)}
		>
			{write.feedback}
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					styles.actions,
				)}
			>
				<Field label={`Flyt ${task.title}`}>
					<Select
						value={selection}
						onChange={(event) => {
							setSelection(event.target.value);
							setLocalTarget("");
						}}
						xstyle={[cockpitStyles.selectComposition]}
					>
						<option
							value=""
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Vælg kolonne
						</option>
						{targetBoard.columns.map((column) => (
							<option
								key={column.columnId}
								value={column.columnId}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{column.name} · {taskStatusLabel[column.status]}
							</option>
						))}
					</Select>
				</Field>
				{ambiguous && (
					<Field label={`Selskabets målkolonne for ${task.title}`}>
						<Select
							required
							value={localTarget}
							onChange={(event) => setLocalTarget(event.target.value)}
							xstyle={[cockpitStyles.selectComposition]}
						>
							<option
								value=""
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Vælg selskabets målkolonne
							</option>
							{candidates.map((column) => (
								<option
									key={column.columnId}
									value={column.columnId}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{column.name}
								</option>
							))}
						</Select>
					</Field>
				)}
				<Button
					variant="secondary"
					busy={write.busy}
					disabled={write.blocked || !selection || (ambiguous && !localTarget)}
					onClick={() => void move()}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					Flyt opgave
				</Button>
			</div>
			{aggregate &&
				selected &&
				task.scope.kind === "company" &&
				localDefault && (
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Selskabets standardvalg: {localDefault.name}
					</p>
				)}
			{complete && (
				<CompletionEditor
					task={task}
					onSaved={onSaved}
					onClose={() => setComplete(false)}
				/>
			)}
		</div>
	);
}

function Calendar({
	tasks,
	projections,
	data,
	returnTo,
	range,
	mode,
}: {
	tasks: Task[];
	projections: TaskProjection[];
	data: TasksData;
	returnTo: string;
	range: { from: string; to: string };
	mode: string;
}) {
	const days = dateSequence(range.from, range.to);
	return (
		<div
			role="group"
			aria-label="Opgavekalender"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				mode === "agenda" ? styles.list : styles.calendar,
			)}
		>
			{days
				.filter(
					(day) =>
						mode !== "agenda" ||
						tasks.some(
							(task) => task.workDate === day || task.deadline?.date === day,
						) ||
						projections.some(
							(projection) =>
								!projection.concreteTaskId &&
								(projection.workDate === day ||
									projection.deadline?.date === day),
						),
				)
				.map((day) => (
					<section
						key={day}
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							styles.date,
						)}
					>
						<h3
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.h3,
							)}
						>
							{new Intl.DateTimeFormat("da-DK", {
								weekday: "short",
								day: "numeric",
								month: "short",
								timeZone: "UTC",
							}).format(new Date(`${day}T12:00:00Z`))}
						</h3>
						<ul
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								styles.dateList,
							)}
						>
							{tasks
								.filter(
									(task) =>
										task.workDate === day || task.deadline?.date === day,
								)
								.map((task) => (
									<li
										key={task.taskId}
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										<Link
											to={`/opgaver/${encodeURIComponent(task.taskId)}?${new URLSearchParams({ returnTo })}`}
											{...stylex.props(cockpitStyles.aComposition)}
										>
											{task.title}
										</Link>
										<div
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.muted,
											)}
										>
											{scopeLabel(task.scope, data)} ·{" "}
											{task.workDate === day ? "Arbejdsdato" : ""}
											{task.workDate === day && task.deadline?.date === day
												? " + "
												: ""}
											{task.deadline?.date === day
												? `${task.deadline.kind === "statutory" ? "Myndighedsfrist" : "Frist"}${task.deadline.certainty === "unconfirmed" ? " · skal bekræftes" : ""}`
												: ""}{" "}
											· {taskUrgency(task)}
										</div>
									</li>
								))}
							{projections
								.filter(
									(projection) =>
										!projection.concreteTaskId &&
										(projection.workDate === day ||
											projection.deadline?.date === day),
								)
								.map((projection) => (
									<li
										key={projection.projectionId}
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										Forventet: {projection.title}
										<div
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
												cockpitStyles.muted,
											)}
										>
											{scopeLabel(projection.scope, data)} ·{" "}
											{projection.period.label} · endnu ikke oprettet
											{projection.deadline?.certainty === "unconfirmed"
												? " · frist skal bekræftes"
												: ""}
										</div>
									</li>
								))}
						</ul>
					</section>
				))}
		</div>
	);
}

function YearWheel({
	data,
	tasks,
	range,
	returnTo,
}: {
	data: TasksData;
	tasks: Task[];
	range: { from: string; to: string };
	returnTo: string;
}) {
	const start = new Date(`${range.from}T12:00:00Z`);
	const months = Array.from({ length: 12 }, (_, i) => {
		const month = new Date(start);
		month.setUTCMonth(month.getUTCMonth() + i);
		return month.toISOString().slice(0, 7);
	});
	return (
		<div
			role="group"
			aria-label="Årshjul"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				styles.wheel,
			)}
		>
			{months.map((month) => (
				<section
					key={month}
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						styles.card,
					)}
				>
					<h2
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h2,
						)}
					>
						{monthName(month)}
					</h2>
					<ul
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							styles.dateList,
						)}
					>
						{tasks
							.filter((task) =>
								[task.workDate, task.deadline?.date, task.period?.from].some(
									(date) => date?.slice(0, 7) === month,
								),
							)
							.map((task) => (
								<li
									key={task.taskId}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<Link
										to={`/opgaver/${encodeURIComponent(task.taskId)}?${new URLSearchParams({ returnTo })}`}
										{...stylex.props(cockpitStyles.aComposition)}
									>
										{task.title}
									</Link>
									<p
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
										)}
									>
										{scopeLabel(task.scope, data)} · {task.period?.label} ·{" "}
										{taskUrgency(task)}
										{task.deadline ? ` · frist ${task.deadline.date}` : ""}
									</p>
								</li>
							))}
						{data.projections
							.filter(
								(projection) =>
									!projection.concreteTaskId &&
									projection.period.from.slice(0, 7) === month,
							)
							.map((projection) => (
								<li
									key={projection.projectionId}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Forventet: {projection.title}
									<p
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.muted,
										)}
									>
										{scopeLabel(projection.scope, data)} ·{" "}
										{projection.period.label} · endnu ikke oprettet
										{projection.relevance !== "relevant"
											? " · relevans skal afklares"
											: ""}
										{projection.deadline?.certainty === "unconfirmed"
											? " · frist skal bekræftes"
											: ""}
									</p>
								</li>
							))}
					</ul>
				</section>
			))}
		</div>
	);
}

export function TaskDetailView() {
	const { taskId = "" } = useParams();
	const [params] = useSearchParams();
	const returnTo = taskReturnTo(params.get("returnTo"));
	const state = useAsync((signal) => api.task(taskId, { signal }), [taskId]);
	const metadata = useAsync(
		(signal) =>
			api.tasks({ includeArchived: true, showDone: true }, { signal }),
		[taskId],
	);
	const [editor, setEditor] = useState<
		"edit" | "complete" | "reopen" | "reminder" | null
	>(null);
	const reload = () => {
		state.reload();
		metadata.reload();
	};
	if ((state.loading || metadata.loading) && (!state.data || !metadata.data))
		return <Loading label="Henter opgaven…" />;
	if (!state.data || !metadata.data)
		return (
			<ErrorState
				message={
					state.error ?? metadata.error ?? "Opgaven er ikke tilgængelig."
				}
				onRetry={reload}
			/>
		);
	const { task, history } = state.data;
	const data = metadata.data;
	const writable = canWriteScope(task.scope, data);
	return (
		<section
			data-cockpit-page="task-detail"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				styles.page,
			)}
		>
			<PageHeader
				title={task.title}
				description={`${scopeLabel(task.scope, data)} · ${task.taskId} · ${taskTypeLabel[task.type]}`}
				actions={
					<>
						<ButtonLink
							variant="secondary"
							to={returnTo}
							xstyle={[cockpitStyles.aComposition]}
						>
							Tilbage til opgaver
						</ButtonLink>
						{writable && (
							<Button
								onClick={() => setEditor("edit")}
								xstyle={[cockpitStyles.buttonComposition]}
							>
								Redigér opgave
							</Button>
						)}
					</>
				}
			/>
			{(state.error || metadata.error) && (
				<p
					role="alert"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Senest hentede data vises. Hent aktuel status før ændringer.
				</p>
			)}
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					styles.detail,
				)}
			>
				<article
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						styles.card,
					)}
				>
					<h2
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h2,
						)}
					>
						{taskUrgency(task)}
					</h2>
					{task.description && (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{task.description}
						</p>
					)}
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Næste handling: {task.nextAction || "Ikke angivet"}
					</p>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Ansvarlig: {task.assignee?.name ?? "Ikke tildelt"}
						{task.assignee?.kind === "external"
							? " · ekstern, ingen adgang eller besked ved tildeling"
							: ""}
					</p>
					{task.waitingOn && (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Afventer {task.waitingOn}
						</p>
					)}
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Periode:{" "}
						{task.period
							? `${task.period.label} · ${task.period.from} – ${task.period.to}`
							: "Ikke angivet"}
					</p>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Arbejdsdato: {task.workDate ?? "Ikke angivet"}
					</p>
					{task.deadline ? (
						<>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{task.deadline.kind === "statutory"
									? "Myndighedsfrist"
									: task.deadline.kind === "agreement"
										? "Aftalt frist"
										: "Intern frist"}
								: {task.deadline.date} ·{" "}
								{task.deadline.certainty === "confirmed"
									? "Bekræftet"
									: "Skal bekræftes"}
							</p>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Fristgrundlag: {task.deadline.basis || "Ukendt"}
							</p>
						</>
					) : (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Frist: ikke angivet
						</p>
					)}
					{task.origin === "proposal" && (
						<>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Forslag · kræver din vurdering før det accepteres.
							</p>
							{writable && <AcceptProposal task={task} onSaved={reload} />}
						</>
					)}
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{task.evidenceRequired
							? "Afslutningsbevis kræves."
							: "En afslutningsnote er tilstrækkelig."}
					</p>
					{task.source && (
						<>
							<h3
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.h3,
								)}
							>
								Autoritativ kilde
							</h3>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{task.source.state === "unknown"
									? "Kilden skal verificeres"
									: task.source.state === "resolved"
										? "Kilden er løst"
										: "Kilden er fortsat åben"}
							</p>
							{taskSourceHref(task.source.href) && (
								<Link
									to={`${taskSourceHref(task.source.href)}${taskSourceHref(task.source.href)?.includes("?") ? "&" : "?"}${new URLSearchParams({ returnTo: `/opgaver/${encodeURIComponent(task.taskId)}?${new URLSearchParams({ returnTo })}` })}`}
									{...stylex.props(cockpitStyles.aComposition)}
								>
									Åbn kilde
								</Link>
							)}
						</>
					)}
					{task.references.length > 0 && (
						<>
							<h3
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.h3,
								)}
							>
								Referencer
							</h3>
							<References
								references={task.references}
								scope={task.scope}
								returnTo={`/opgaver/${encodeURIComponent(task.taskId)}?${new URLSearchParams({ returnTo })}`}
							/>
						</>
					)}
					{writable && (
						<>
							<TaskMove
								task={task}
								data={data}
								aggregate={task.scope.kind !== "company"}
								onSaved={reload}
							/>
							<div
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									styles.actions,
								)}
							>
								<Button
									onClick={() =>
										setEditor(task.status === "done" ? "reopen" : "complete")
									}
									xstyle={[cockpitStyles.buttonComposition]}
								>
									{task.status === "done"
										? "Genåbn opgave"
										: "Afslut med dokumentation"}
								</Button>
								<Button
									variant="secondary"
									onClick={() => setEditor("reminder")}
									xstyle={[cockpitStyles.buttonComposition]}
								>
									Påmindelser
								</Button>
							</div>
						</>
					)}
				</article>
				<article
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						styles.card,
					)}
				>
					<h2
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h2,
						)}
					>
						Afslutning og påmindelser
					</h2>
					{task.completion ? (
						<>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{outcomeLabel[task.completion.outcome]} ·{" "}
								{task.completion.assurance === "product_verified"
									? "Verificeret ved produktkilden"
									: "Oplyst af brugeren"}
							</p>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{task.completion.note}
							</p>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{task.completion.at} · {task.completion.actor}
							</p>
							<References
								references={task.completion.references}
								scope={task.scope}
								returnTo={`/opgaver/${encodeURIComponent(task.taskId)}?${new URLSearchParams({ returnTo })}`}
							/>
						</>
					) : (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Ingen registreret afslutning.
						</p>
					)}
					{task.reminders.length === 0 ? (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Ingen aktiverede påmindelser.
						</p>
					) : (
						<ul
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{task.reminders.map((reminder) => (
								<li
									key={reminder.reminderId}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{reminder.enabled &&
									data.runtime.running &&
									task.status !== "done"
										? "Aktiv levering"
										: "Inaktiv levering"}{" "}
									·{" "}
									{reminder.kind === "deadline"
										? `${reminder.daysBefore ?? 0} dage før frist ${task.deadline?.date ?? "(fristen mangler)"}${task.deadline ? ` · planlagt fra ${dateAdd(task.deadline.date, -(reminder.daysBefore ?? 0))}` : ""}`
										: `Opfølgning ${reminder.followUpDate}`}{" "}
									· {reminder.frequency === "daily" ? "dagligt" : "én gang"} ·{" "}
									{reminder.timeZone} · Rentemester
								</li>
							))}
						</ul>
					)}
					{!data.runtime.running && (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Påmindelsesruntime er stoppet. Gemte planer leverer ikke beskeder.
						</p>
					)}
				</article>
				<article
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						styles.card,
					)}
				>
					<h2
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h2,
						)}
					>
						Historik
					</h2>
					{history.length ? (
						<ol
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{history.map((event) => (
								<li
									key={`${event.version}:${event.operation}`}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<strong
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{event.operation}
									</strong>{" "}
									· {event.at} · {event.actor} · version {event.version}
									<p
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{taskStatusLabel[event.task.status]} ·{" "}
										{event.task.assignee?.name ?? "Ikke tildelt"} · arbejdsdato{" "}
										{event.task.workDate ?? "ingen"} · frist{" "}
										{event.task.deadline?.date ?? "ingen"}
									</p>
									{event.task.completion && (
										<>
											<p
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{outcomeLabel[event.task.completion.outcome]}:{" "}
												{event.task.completion.note}
											</p>
											<References
												references={event.task.completion.references}
												scope={event.task.scope}
												returnTo={`/opgaver/${encodeURIComponent(task.taskId)}?${new URLSearchParams({ returnTo })}`}
											/>
										</>
									)}
								</li>
							))}
						</ol>
					) : (
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Ingen historik tilgængelig.
						</p>
					)}
				</article>
			</div>
			{editor === "edit" && (
				<TaskEditor
					task={task}
					view={data}
					onSaved={reload}
					onClose={() => setEditor(null)}
				/>
			)}
			{(editor === "complete" || editor === "reopen") && (
				<CompletionEditor
					task={task}
					reopen={editor === "reopen"}
					onSaved={reload}
					onClose={() => setEditor(null)}
				/>
			)}
			{editor === "reminder" && (
				<ReminderEditor
					task={task}
					view={data}
					onSaved={reload}
					onClose={() => setEditor(null)}
				/>
			)}
		</section>
	);
}
function References({
	references,
	scope,
	returnTo,
}: {
	references: Task["references"];
	scope: TaskScope;
	returnTo: string;
}) {
	return (
		<ul {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
			{references.map((reference, i) => {
				const href = referenceHref(reference, scope);
				return (
					<li
						key={`${reference.kind}:${reference.ref}:${i}`}
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{href ? (
							<Link
								to={`${href}${href.includes("?") ? "&" : "?"}${new URLSearchParams({ returnTo })}`}
								{...stylex.props(cockpitStyles.aComposition)}
							>
								{reference.kind}: {reference.ref}
							</Link>
						) : (
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{reference.kind}: {reference.ref}
							</span>
						)}
					</li>
				);
			})}
		</ul>
	);
}
