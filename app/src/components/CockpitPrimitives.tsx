import * as stylex from "@stylexjs/stylex";
import {
	cloneElement,
	createElement,
	type ReactElement,
	type ReactNode,
} from "react";
import { cockpitStyles } from "../design/cockpit.stylex";
import { colors, rounded, spacing, typography } from "../design/tokens.stylex";
import type { DataCoverage } from "../lib/types";
import { Button, Input, Select, Textarea } from "./ui";

const styles = stylex.create({
	field: { display: "grid", gap: spacing.xs, minWidth: 0 },
	label: { fontWeight: 500 },
	hint: { margin: 0, color: colors.inkMuted, fontSize: typography.sizeSm },
	error: { margin: 0, color: colors.danger, fontSize: typography.sizeSm },
	filters: {
		display: "grid",
		gap: spacing.sm,
		padding: spacing.md,
		marginBottom: spacing.md,
		backgroundColor: colors.paperRaised,
		borderWidth: 1,
		borderStyle: "solid",
		borderColor: colors.border,
		borderRadius: rounded.md,
	},
	state: {
		marginBlock: spacing.md,
		padding: spacing.lg,
		borderWidth: 1,
		borderStyle: "solid",
		borderColor: colors.border,
		backgroundColor: colors.paperRaised,
		borderRadius: rounded.md,
	},
	warningState: { borderLeftWidth: 4, borderLeftColor: colors.warning },
	errorState: { borderLeftWidth: 4, borderLeftColor: colors.danger },
	title: {
		marginTop: 0,
		marginBottom: spacing.sm,
		fontFamily: typography.headlineFamily,
		fontSize: typography.sizeXl,
	},
	actions: {
		display: "flex",
		flexWrap: "wrap",
		gap: spacing.xs,
		alignItems: "center",
	},
	badge: {
		display: "inline-flex",
		alignItems: "center",
		gap: spacing.xxs,
		borderRadius: rounded.sm,
		paddingInline: spacing.xs,
		paddingBlock: spacing.xxs,
		fontSize: typography.sizeSm,
		color: colors.ink,
		backgroundColor: colors.paperRaised,
	},
	success: { backgroundColor: colors.successSoft },
	warning: { backgroundColor: colors.warningSoft },
	danger: { backgroundColor: colors.dangerSoft },
	info: { backgroundColor: colors.infoSoft },
	card: {
		padding: spacing.md,
		borderWidth: 1,
		borderStyle: "solid",
		borderColor: colors.border,
		backgroundColor: colors.paperRaised,
		borderRadius: rounded.md,
	},
	metric: {
		fontFamily: typography.monoFamily,
		fontSize: typography.sizeXl,
		fontVariantNumeric: "tabular-nums",
	},
	tableShell: {
		minWidth: 0,
		borderWidth: 1,
		borderStyle: "solid",
		borderColor: colors.border,
		backgroundColor: colors.paperRaised,
		borderRadius: rounded.md,
	},
});

type Tone = "neutral" | "success" | "warning" | "danger" | "info";

export function FormField({
	label,
	hint,
	error,
	children,
	xstyle,
}: {
	label: string;
	hint?: string;
	error?: string;
	children: ReactElement<{
		id?: string;
		"aria-describedby"?: string;
		"aria-invalid"?: boolean;
	}>;
	xstyle?: stylex.StyleXStyles;
}) {
	const id =
		children.props.id ??
		`field-${label.toLowerCase().replace(/[^a-z0-9]+/gi, "-")}`;
	const hintId = hint ? `${id}-hint` : undefined;
	const errorId = error ? `${id}-error` : undefined;
	const describedBy =
		[children.props["aria-describedby"], hintId, errorId]
			.filter(Boolean)
			.join(" ") || undefined;
	const controlProps = {
		...children.props,
		id,
		"aria-describedby": describedBy,
		"aria-invalid": error ? true : children.props["aria-invalid"],
	};
	// Legacy callers keep the same label/hint IDs while controls use the shared UI.
	const control =
		children.type === "input"
			? createElement(Input, controlProps)
			: children.type === "select"
				? createElement(Select, controlProps)
				: children.type === "textarea"
					? createElement(Textarea, controlProps)
					: cloneElement(children, controlProps);
	return (
		<div
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.formField,
				styles.field,
				xstyle,
			)}
		>
			<label
				htmlFor={id}
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.formFieldLabel,
					styles.label,
				)}
			>
				{label}
			</label>
			{control}
			{hint && (
				<p
					id={hintId}
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.fieldHint,
						styles.hint,
					)}
				>
					{hint}
				</p>
			)}
			{error && (
				<p
					id={errorId}
					role="alert"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.fieldError,
						styles.error,
					)}
				>
					{error}
				</p>
			)}
		</div>
	);
}

export function FilterBar({
	children,
	activeFilters = [],
	onReset,
	resetLabel = "Nulstil filtre",
	advanced,
	advancedEvidence = false,
}: {
	children: ReactNode;
	activeFilters?: string[];
	onReset?: () => void;
	resetLabel?: string;
	advanced?: ReactNode;
	advancedEvidence?: boolean;
}) {
	return (
		<section
			aria-label="Filtre"
			role="search"
			{...stylex.props(cockpitStyles.filterBarComposition, styles.filters)}
		>
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.filterBarControls,
				)}
			>
				{children}
			</div>
			{advanced && (
				<details
					data-evidence-progressive={advancedEvidence || undefined}
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.filterBarAdvanced,
					)}
				>
					<summary
						{...stylex.props(cockpitStyles.filterBarAdvancedSummaryComposition)}
					>
						Avancerede filtre
					</summary>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.filterBarAdvancedDiv,
						)}
					>
						{advanced}
					</div>
				</details>
			)}
			{activeFilters.length > 0 && (
				<div
					aria-live="polite"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.activeFilters,
					)}
				>
					<span
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Aktive filtre: {activeFilters.join(", ")}
					</span>
					{onReset && (
						<Button
							variant="secondary"
							onClick={onReset}
							xstyle={[cockpitStyles.buttonComposition]}
						>
							{resetLabel}
						</Button>
					)}
				</div>
			)}
		</section>
	);
}

export function StatusChip({
	tone = "neutral",
	children,
	coverage,
}: {
	tone?: Tone;
	children?: ReactNode;
	coverage?: DataCoverage;
}) {
	return (
		<span
			data-tone={tone}
			data-coverage={coverage?.kind}
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statusChip,
				tone === "success" && cockpitStyles.statusChipSuccess,
				tone === "warning" && cockpitStyles.statusChipWarning,
				tone === "danger" && cockpitStyles.statusChipDanger,
				tone === "info" && cockpitStyles.statusChipInfo,
				styles.badge,
				tone !== "neutral" && styles[tone],
			)}
		>
			{coverage ? coverage.label : children}
		</span>
	);
}

export function PageState({
	kind,
	title,
	children,
	onRetry,
	actions,
}: {
	kind: "loading" | "empty" | "warning" | "blocked" | "error";
	title: string;
	children?: ReactNode;
	onRetry?: () => void;
	actions?: ReactNode;
}) {
	const role = kind === "error" || kind === "blocked" ? "alert" : "status";
	const live = kind === "loading" ? "polite" : undefined;
	return (
		<section
			role={role}
			aria-live={live}
			data-page-state={kind}
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.pageState,
				kind === "warning" && cockpitStyles.pageStateWarning,
				kind === "blocked" && cockpitStyles.pageStateBlocked,
				kind === "error" && cockpitStyles.pageStateError,
				styles.state,
				(kind === "warning" || kind === "blocked") && styles.warningState,
				kind === "error" && styles.errorState,
			)}
		>
			<h2
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h2,
					cockpitStyles.pageStateH2,
					styles.title,
				)}
			>
				{title}
			</h2>
			{children && (
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.pageStateP,
					)}
				>
					{children}
				</p>
			)}
			{(onRetry || actions) && (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.pageStateActions,
					)}
				>
					{onRetry && (
						<Button
							variant="secondary"
							onClick={onRetry}
							xstyle={[cockpitStyles.buttonComposition]}
						>
							Prøv igen
						</Button>
					)}
					{actions}
				</div>
			)}
		</section>
	);
}

export function PageHeaderActions({ children }: { children: ReactNode }) {
	return (
		<div
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.pageHeaderActions,
				styles.actions,
			)}
		>
			{children}
		</div>
	);
}

export function MetricCard({
	label,
	value,
	children,
}: {
	label: string;
	value: ReactNode;
	children?: ReactNode;
}) {
	return (
		<section
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.card,
				styles.card,
			)}
		>
			<h2
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h2,
					cockpitStyles.metricCardH2,
				)}
			>
				{label}
			</h2>
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.metricCardValue,
					styles.metric,
				)}
			>
				{value}
			</div>
			{children && (
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
						cockpitStyles.metricCardP,
					)}
				>
					{children}
				</p>
			)}
		</section>
	);
}

/** A deliberately small semantic table wrapper. Cells may use data-label for mobile detail labels. */
export function ResponsiveTable({
	children,
	xstyle,
	label,
}: {
	children: ReactNode;
	xstyle?: stylex.StyleXStyles;
	label?: string;
}) {
	return (
		<div
			data-responsive-table
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.responsiveTableShell,
				cockpitStyles.card,
				styles.tableShell,
			)}
		>
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.responsiveTableScroll,
				)}
			>
				<table
					aria-label={label}
					{...stylex.props(cockpitStyles.responsiveTableComposition, xstyle)}
				>
					{children}
				</table>
			</div>
		</div>
	);
}
