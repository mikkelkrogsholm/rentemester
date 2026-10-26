import * as stylex from "@stylexjs/stylex";
import {
	type ButtonHTMLAttributes,
	cloneElement,
	forwardRef,
	type InputHTMLAttributes,
	isValidElement,
	type ReactNode,
	type RefObject,
	type SelectHTMLAttributes,
	type TextareaHTMLAttributes,
	useEffect,
	useId,
	useRef,
	useState,
} from "react";
import { Link, type LinkProps } from "react-router-dom";
import type { RoutePermission } from "../../../../src/core/access-permissions";
import { cockpitStyles } from "../../design/cockpit.stylex";
import { lockDocumentScroll } from "../../design/document-style";
import {
	colors,
	controls,
	rounded,
	spacing,
	typography,
} from "../../design/tokens.stylex";
import { formatKroner, parseDanishAmount } from "../../lib/format";
import { useCapabilities } from "../../lib/useCapabilities";

const styles = stylex.create({
	button: {
		display: "inline-flex",
		alignItems: "center",
		justifyContent: "center",
		gap: spacing.xs,
		boxSizing: "border-box",
		minWidth: 0,
		maxWidth: "100%",
		whiteSpace: "normal",
		textAlign: "center",
		minHeight: controls.height,
		paddingInline: spacing.md,
		paddingBlock: spacing.xs,
		borderWidth: 1,
		borderStyle: "solid",
		borderColor: colors.ink,
		borderRadius: rounded.md,
		backgroundColor: { default: colors.ink, ":hover": colors.inkMuted },
		color: colors.paper,
		fontFamily: typography.bodyFamily,
		fontSize: typography.bodySize,
		fontWeight: 500,
		textDecoration: "none",
		cursor: { default: "pointer", ":disabled": "not-allowed" },
		outlineWidth: { default: null, ":focus-visible": controls.focusWidth },
		outlineStyle: { default: null, ":focus-visible": "solid" },
		outlineColor: { default: null, ":focus-visible": colors.info },
		outlineOffset: { default: null, ":focus-visible": controls.focusOffset },
		opacity: { default: null, ":disabled": 0.6 },
	},
	primaryLink: { color: colors.paper },
	secondary: {
		backgroundColor: { default: colors.paperRaised, ":hover": colors.paper },
		color: colors.ink,
		borderColor: colors.borderStrong,
	},
	quiet: {
		backgroundColor: { default: "transparent", ":hover": colors.paperRaised },
		color: colors.ink,
		borderColor: "transparent",
	},
	danger: {
		backgroundColor: { default: colors.danger, ":hover": colors.accent },
		borderColor: colors.danger,
	},
	control: {
		boxSizing: "border-box",
		minHeight: controls.height,
		maxWidth: "100%",
		minWidth: 0,
		paddingInline: spacing.sm,
		paddingBlock: spacing.xs,
		borderWidth: 1,
		borderStyle: "solid",
		borderColor: {
			default: colors.borderStrong,
			":user-invalid": colors.danger,
		},
		borderRadius: rounded.md,
		backgroundColor: colors.paperRaised,
		color: colors.ink,
		fontFamily: typography.bodyFamily,
		fontSize: typography.bodySize,
		outlineWidth: { default: null, ":focus-visible": controls.focusWidth },
		outlineStyle: { default: null, ":focus-visible": "solid" },
		outlineColor: { default: null, ":focus-visible": colors.info },
		outlineOffset: { default: null, ":focus-visible": controls.focusOffset },
		opacity: { default: null, ":disabled": 0.6 },
		cursor: { default: null, ":disabled": "not-allowed" },
	},
	checkbox: {
		minHeight: 20,
		width: 20,
		height: 20,
		accentColor: colors.ink,
		verticalAlign: "middle",
	},
	field: { display: "grid", gap: spacing.xs, minWidth: 0 },
	label: { fontWeight: 500 },
	help: { color: colors.inkMuted, fontSize: typography.sizeSm },
	error: { color: colors.danger, fontSize: typography.sizeSm },
	header: {
		display: "flex",
		flexWrap: "wrap",
		justifyContent: "space-between",
		alignItems: "flex-start",
		gap: spacing.md,
		marginBottom: spacing.lg,
	},
	title: {
		fontFamily: typography.headlineFamily,
		fontSize: typography.size2xl,
		lineHeight: 1.2,
		marginTop: 0,
		marginBottom: spacing.xs,
		overflowWrap: "anywhere",
	},
	description: { color: colors.inkMuted, margin: 0, maxWidth: "68ch" },
	actions: { display: "flex", flexWrap: "wrap", gap: spacing.xs },
	amount: {
		fontFamily: typography.monoFamily,
		fontVariantNumeric: "tabular-nums",
		whiteSpace: "nowrap",
	},
	dialog: {
		width: "min(640px, calc(100% - 32px))",
		maxHeight: "calc(100dvh - 32px)",
		overflowY: "auto",
		padding: spacing.lg,
		margin: "auto",
		color: colors.ink,
		backgroundColor: {
			default: colors.paperRaised,
			"::backdrop": "rgb(27 26 23 / 40%)",
		},
		borderWidth: 1,
		borderStyle: "solid",
		borderColor: colors.borderStrong,
		borderRadius: rounded.md,
		boxShadow: "none",
	},
	page: {
		width: "100%",
		maxWidth: "100%",
		maxHeight: "none",
		overflowY: "visible",
		padding: 0,
		borderWidth: 0,
		backgroundColor: "transparent",
	},
	dialogTitle: {
		fontFamily: typography.headlineFamily,
		fontSize: typography.sizeXl,
		marginTop: 0,
		marginBottom: spacing.md,
		overflowWrap: "anywhere",
	},
	mobileMenu: {
		display: { default: "none", "@media (max-width: 1023px)": "inline-flex" },
	},
	mobileFilter: {
		display: { default: "none", "@media (max-width: 639px)": "inline-flex" },
	},
	pagination: {
		display: "flex",
		flexWrap: "wrap",
		alignItems: "center",
		gap: spacing.sm,
		paddingBlock: spacing.md,
		borderTopWidth: 1,
		borderTopStyle: "solid",
		borderTopColor: colors.border,
	},
	filters: {
		display: { default: "block", "@media print": "none" },
		padding: spacing.md,
		marginBottom: spacing.md,
		backgroundColor: colors.paperRaised,
		borderWidth: 1,
		borderStyle: "solid",
		borderColor: colors.border,
		borderRadius: rounded.md,
	},
});

export type StyledControl = { xstyle?: stylex.StyleXStyles };
type ButtonProps = Omit<
	ButtonHTMLAttributes<HTMLButtonElement>,
	"className" | "style"
> &
	StyledControl & {
		variant?: "primary" | "secondary" | "quiet" | "danger";
		busy?: boolean;
		requiredPermission?: RoutePermission;
	};
export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
	function Button(
		{
			variant = "primary",
			busy,
			requiredPermission,
			xstyle,
			disabled,
			children,
			type = "button",
			...props
		},
		ref,
	) {
		const { can } = useCapabilities();
		if (requiredPermission && !can(requiredPermission)) return null;
		return (
			<button
				{...props}
				{...stylex.props(
					cockpitStyles.element,
					styles.button,
					variant !== "primary" && styles[variant],
					xstyle,
				)}
				ref={ref}
				data-variant={variant}
				type={type}
				disabled={disabled || busy}
				aria-busy={busy || undefined}
				onClick={(event) => {
					event.currentTarget.focus();
					props.onClick?.(event);
				}}
			>
				{children}
			</button>
		);
	},
);
/** Navigation with the same visual contract and focus target as an action. */
export const ButtonLink = forwardRef<
	HTMLAnchorElement,
	Omit<LinkProps, "className" | "style"> &
		StyledControl & { variant?: ButtonProps["variant"] }
>(function ButtonLink({ variant = "primary", xstyle, ...props }, ref) {
	return (
		<Link
			{...props}
			{...stylex.props(
				cockpitStyles.element,
				styles.button,
				xstyle,
				variant !== "primary" && styles[variant],
				variant === "primary" && styles.primaryLink,
			)}
			ref={ref}
		/>
	);
});
export const Input = forwardRef<
	HTMLInputElement,
	Omit<InputHTMLAttributes<HTMLInputElement>, "className" | "style"> &
		StyledControl
>(function Input({ xstyle, type, ...props }, ref) {
	return (
		<input
			{...props}
			{...stylex.props(
				cockpitStyles.element,
				styles.control,
				(type === "checkbox" || type === "radio") && styles.checkbox,
				xstyle,
			)}
			ref={ref}
			type={type}
		/>
	);
});
export const Select = forwardRef<
	HTMLSelectElement,
	Omit<SelectHTMLAttributes<HTMLSelectElement>, "className" | "style"> &
		StyledControl
>(function Select({ xstyle, ...props }, ref) {
	return (
		<select
			{...props}
			{...stylex.props(cockpitStyles.element, styles.control, xstyle)}
			ref={ref}
		/>
	);
});
export const Textarea = forwardRef<
	HTMLTextAreaElement,
	Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "className" | "style"> &
		StyledControl
>(function Textarea({ xstyle, ...props }, ref) {
	return (
		<textarea
			{...props}
			{...stylex.props(cockpitStyles.element, styles.control, xstyle)}
			ref={ref}
		/>
	);
});
export function Field({
	label,
	help,
	error,
	children,
}: {
	label: ReactNode;
	help?: ReactNode;
	error?: ReactNode;
	children: ReactNode;
}) {
	const generatedId = useId();
	const control = isValidElement<{
		id?: string;
		"aria-describedby"?: string;
		"aria-invalid"?: boolean;
	}>(children)
		? children
		: null;
	const id = control?.props.id ?? generatedId;
	const helpId = `${id}-help`,
		errorId = `${id}-error`;
	const described = [
		control?.props["aria-describedby"],
		help ? helpId : undefined,
		error ? errorId : undefined,
	]
		.filter(Boolean)
		.join(" ");
	const linked = control
		? cloneElement(control, {
				id,
				"aria-describedby": described || undefined,
				"aria-invalid": error ? true : control.props["aria-invalid"],
			})
		: children;
	return (
		<div
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				styles.field,
			)}
		>
			<label
				htmlFor={id}
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					styles.label,
				)}
			>
				{label}
			</label>
			{linked}
			{help && (
				<span
					id={helpId}
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						styles.help,
					)}
				>
					{help}
				</span>
			)}
			{error && (
				<span
					id={errorId}
					role="alert"
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						styles.error,
					)}
				>
					{error}
				</span>
			)}
		</div>
	);
}
export function MoneyInput({
	value,
	onValueChange,
	...props
}: Omit<
	InputHTMLAttributes<HTMLInputElement>,
	"value" | "onChange" | "className" | "style"
> &
	StyledControl & { value: string; onValueChange: (value: string) => void }) {
	return (
		<Input
			{...props}
			value={value}
			inputMode="decimal"
			onChange={(event) => onValueChange(event.target.value)}
			xstyle={[cockpitStyles.inputComposition]}
		/>
	);
}
export { parseDanishAmount };
export function Amount({
	value,
	currency,
}: {
	value: number | null | undefined;
	currency: string;
}) {
	return (
		<span
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				styles.amount,
			)}
		>
			{value == null ? "—" : formatKroner(value, currency)}
		</span>
	);
}
export function PageHeader({
	title,
	description,
	actions,
	children,
	evidenceHeading = false,
}: {
	title: ReactNode;
	description?: ReactNode;
	actions?: ReactNode;
	children?: ReactNode;
	evidenceHeading?: boolean;
}) {
	return (
		<header
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				styles.header,
			)}
		>
			<div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				<h1
					data-evidence-heading={evidenceHeading || undefined}
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h1,
						styles.title,
					)}
				>
					{title}
				</h1>
				{description && (
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							styles.description,
						)}
					>
						{description}
					</div>
				)}
				{children}
			</div>
			{actions && (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						styles.actions,
					)}
				>
					{actions}
				</div>
			)}
		</header>
	);
}

// Native disclosures are tab stops. Controls inside closed or CSS-hidden
// containers are not; keeping them in the cycle can strand keyboard focus.
function dialogTabStops(root: HTMLElement) {
	return Array.from(
		root.querySelectorAll<HTMLElement>(
			'button, [href], input:not([type="hidden"]), select, textarea, summary, [contenteditable="true"], [tabindex]',
		),
	).filter((element) => {
		if (
			element.tabIndex < 0 ||
			element.matches(":disabled") ||
			element.closest("[hidden], [inert]")
		)
			return false;
		for (
			let ancestor: HTMLElement | null = element;
			ancestor;
			ancestor = ancestor.parentElement
		) {
			const style = getComputedStyle(ancestor);
			if (style.display === "none" || style.visibility === "hidden")
				return false;
			if (ancestor instanceof HTMLDetailsElement && !ancestor.open) {
				const summary = Array.from(ancestor.children).find(
					(child) => child.tagName === "SUMMARY",
				);
				if (!summary?.contains(element)) return false;
			}
			if (ancestor === root) break;
		}
		return true;
	});
}
export function Dialog({
	id,
	title,
	onClose,
	busy = false,
	children,
	xstyle,
	initialFocusRef,
	returnFocusRef,
	evidenceTaskOutcome = false,
	mode = "dialog",
}: {
	id?: string;
	title: string;
	onClose: () => void;
	busy?: boolean;
	children: ReactNode;
	xstyle?: stylex.StyleXStyles;
	initialFocusRef?: RefObject<HTMLElement>;
	returnFocusRef?: RefObject<HTMLElement>;
	evidenceTaskOutcome?: boolean;
	mode?: "dialog" | "page";
}) {
	const titleId = useId();
	const ref = useRef<HTMLDialogElement>(null);
	const current = useRef({ busy, onClose });
	current.current = { busy, onClose };
	useEffect(() => {
		if (mode === "page") return;
		// Async loading may temporarily disable the trigger and move native focus
		// to body before showModal. Its explicit ref preserves the return target.
		const previous =
			returnFocusRef?.current ?? (document.activeElement as HTMLElement | null);
		const root = ref.current;
		if (!root) return;
		const unlock = lockDocumentScroll();
		try {
			if (!root.open) root.showModal();
		} catch (error) {
			unlock();
			throw error;
		}
		const elements = dialogTabStops(root);
		const requested = initialFocusRef?.current;
		(requested && !requested.matches(":disabled")
			? requested
			: (elements.find((element) => element.hasAttribute("autofocus")) ??
				elements[0] ??
				root)
		).focus();
		function keydown(event: KeyboardEvent) {
			const dialogs =
				document.querySelectorAll<HTMLDialogElement>("dialog[open]");
			if (
				!root ||
				dialogs[dialogs.length - 1] !== root ||
				!root.contains(document.activeElement)
			)
				return;
			if (event.key === "Escape") {
				event.preventDefault();
				if (!current.current.busy) current.current.onClose();
			}
			if (event.key !== "Tab") return;
			const elements = dialogTabStops(root);
			event.preventDefault();
			if (!elements.length) {
				root.focus();
				return;
			}
			const index = elements.indexOf(document.activeElement as HTMLElement);
			const next =
				index < 0
					? event.shiftKey
						? elements.length - 1
						: 0
					: (index + (event.shiftKey ? -1 : 1) + elements.length) %
						elements.length;
			elements[next]?.focus();
		}
		window.addEventListener("keydown", keydown);
		return () => {
			unlock();
			window.removeEventListener("keydown", keydown);
			if (root.open) root.close();
			if (previous?.isConnected) previous.focus();
		};
	}, [mode, initialFocusRef, returnFocusRef]);

	const content = (
		<>
			<h2
				id={titleId}
				data-evidence-task-outcome={evidenceTaskOutcome || undefined}
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h2,
					styles.dialogTitle,
				)}
			>
				{title}
			</h2>
			{children}
		</>
	);
	if (mode === "page")
		return (
			<section
				id={id}
				aria-labelledby={titleId}
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					styles.dialog,
					mode === "page" && styles.page,
					xstyle,
				)}
			>
				{content}
			</section>
		);
	return (
		<dialog
			id={id}
			ref={ref}
			aria-labelledby={titleId}
			aria-modal="true"
			tabIndex={-1}
			onCancel={(event) => {
				event.preventDefault();
				if (!busy) onClose();
			}}
			onClick={(event) => {
				if (event.target !== event.currentTarget || busy) return;
				const bounds = event.currentTarget.getBoundingClientRect();
				if (
					event.clientX < bounds.left ||
					event.clientX > bounds.right ||
					event.clientY < bounds.top ||
					event.clientY > bounds.bottom
				)
					onClose();
			}}
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				styles.dialog,
				xstyle,
			)}
		>
			{content}
		</dialog>
	);
}
export function Pagination({
	total,
	page,
	pageSize,
	onPageChange,
	onPageSizeChange,
}: {
	total: number;
	page: number;
	pageSize: number;
	onPageChange: (page: number) => void;
	onPageSizeChange: (size: number) => void;
}) {
	const pages = Math.max(1, Math.ceil(total / pageSize));
	return (
		<nav
			aria-label="Sideskift"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				styles.pagination,
			)}
		>
			<span
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				{total === 0
					? "Ingen poster"
					: `${(page - 1) * pageSize + 1}–${Math.min(total, page * pageSize)} af ${total}`}
			</span>
			<Button
				variant="secondary"
				disabled={page <= 1}
				onClick={() => onPageChange(page - 1)}
				xstyle={[cockpitStyles.buttonComposition]}
			>
				Forrige
			</Button>
			<span
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				Side {page} af {pages}
			</span>
			<Button
				variant="secondary"
				disabled={page >= pages}
				onClick={() => onPageChange(page + 1)}
				xstyle={[cockpitStyles.buttonComposition]}
			>
				Næste
			</Button>
			<label
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				Poster pr. side{" "}
				<Select
					value={pageSize}
					onChange={(event) => onPageSizeChange(Number(event.target.value))}
					xstyle={[cockpitStyles.selectComposition]}
				>
					{[25, 50, 100].map((size) => (
						<option
							key={size}
							value={size}
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{size}
						</option>
					))}
				</Select>
			</label>
		</nav>
	);
}
export function FilterBar({
	children,
	activeCount = 0,
	onReset,
}: {
	children: ReactNode;
	activeCount?: number;
	onReset?: () => void;
}) {
	const [expanded, setExpanded] = useState(false);
	const fieldsId = useId();
	return (
		<section
			{...stylex.props(cockpitStyles.element, styles.filters)}
			aria-label="Filtre"
		>
			<Button
				variant="secondary"
				xstyle={styles.mobileFilter}
				aria-expanded={expanded}
				aria-controls={fieldsId}
				onClick={() => setExpanded((value) => !value)}
			>
				Filtre{activeCount ? ` (${activeCount})` : ""}
			</Button>
			<div
				id={fieldsId}
				{...stylex.props(
					cockpitStyles.uiFilterFields,
					expanded && cockpitStyles.uiFilterFieldsExpanded,
				)}
			>
				{children}
			</div>
			{onReset && activeCount > 0 && (
				<Button variant="quiet" onClick={onReset}>
					Ryd filtre
				</Button>
			)}
		</section>
	);
}

const domainStyles = stylex.create({
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
	tableWrap: {
		width: "100%",
		maxWidth: "100%",
		overflowX: "auto",
		marginBlock: spacing.md,
	},
	table: {
		width: "100%",
		borderCollapse: "collapse",
		textAlign: "left",
		fontSize: typography.sizeSm,
	},
	cell: {
		padding: spacing.sm,
		borderBottomWidth: 1,
		borderBottomStyle: "solid",
		borderBottomColor: colors.border,
		verticalAlign: "top",
	},
	head: { fontWeight: 600, backgroundColor: colors.paperRaised },
	receipt: {
		padding: spacing.md,
		borderWidth: 1,
		borderStyle: "solid",
		borderColor: colors.border,
		backgroundColor: colors.paperRaised,
		borderRadius: rounded.md,
	},
});
export function StatusBadge({
	label,
	tone = "neutral",
}: {
	label: string;
	tone?: "neutral" | "success" | "warning" | "danger" | "info";
}) {
	return (
		<span
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				domainStyles.badge,
				tone !== "neutral" && domainStyles[tone],
			)}
		>
			{label}
		</span>
	);
}
export type TableColumn<T> = {
	id: string;
	label: string;
	render: (row: T) => ReactNode;
};
export function DataTable<T>({
	rows,
	columns,
	rowKey,
	caption,
}: {
	rows: readonly T[];
	columns: readonly TableColumn<T>[];
	rowKey: (row: T) => string | number;
	caption: string;
}) {
	return (
		<div
			role="region"
			aria-label={caption}
			tabIndex={0}
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				domainStyles.tableWrap,
			)}
		>
			<table
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					domainStyles.table,
				)}
			>
				<caption
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{caption}
				</caption>
				<thead
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<tr
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{columns.map((column) => (
							<th
								key={column.id}
								scope="col"
								{...stylex.props(
									cockpitStyles.tableDataTh,
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									domainStyles.cell,
									domainStyles.head,
								)}
							>
								{column.label}
							</th>
						))}
					</tr>
				</thead>
				<tbody
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{rows.map((row) => (
						<tr
							key={rowKey(row)}
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{columns.map((column) => (
								<td
									key={column.id}
									{...stylex.props(
										cockpitStyles.tableDataTd,
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										domainStyles.cell,
									)}
								>
									{column.render(row)}
								</td>
							))}
						</tr>
					))}
				</tbody>
			</table>
		</div>
	);
}
export function ResultReceipt({
	title,
	children,
}: {
	title: string;
	children: ReactNode;
}) {
	return (
		<section
			role="status"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				domainStyles.receipt,
			)}
		>
			<h2
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h2,
				)}
			>
				{title}
			</h2>
			{children}
		</section>
	);
}
export function EntitySelect({
	label,
	value,
	options,
	onChange,
	disabled,
}: {
	label: string;
	value: string;
	options: readonly { id: string; label: string }[];
	onChange: (value: string) => void;
	disabled?: boolean;
}) {
	const [search, setSearch] = useState("");
	const matching = options.filter(
		(option) =>
			option.id === value ||
			option.label
				.toLocaleLowerCase("da")
				.includes(search.toLocaleLowerCase("da")),
	);
	return (
		<div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
			<Field label={`Søg i ${label.toLocaleLowerCase("da")}`}>
				<Input
					value={search}
					onChange={(event) => setSearch(event.target.value)}
					disabled={disabled}
					xstyle={[cockpitStyles.inputComposition]}
				/>
			</Field>
			<Field label={label}>
				<Select
					value={value}
					onChange={(event) => onChange(event.target.value)}
					disabled={disabled}
					xstyle={[cockpitStyles.selectComposition]}
				>
					<option
						value=""
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Vælg…
					</option>
					{matching.map((option) => (
						<option
							value={option.id}
							key={option.id}
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{option.label}
						</option>
					))}
				</Select>
			</Field>
		</div>
	);
}
