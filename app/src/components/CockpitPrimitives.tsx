import { cloneElement, createElement, type ReactElement, type ReactNode } from "react";
import * as stylex from "@stylexjs/stylex";
import { colors, spacing, rounded, typography } from "../design/tokens.stylex";
import { Button, Input, Select, Textarea } from "./ui";
import type { DataCoverage } from "../lib/types";

const styles = stylex.create({
  field: { display: "grid", gap: spacing.xs, minWidth: 0 },
  label: { fontWeight: 500 },
  hint: { margin: 0, color: colors.inkMuted, fontSize: typography.sizeSm },
  error: { margin: 0, color: colors.danger, fontSize: typography.sizeSm },
  filters: { display: "grid", gap: spacing.sm, padding: spacing.md, marginBottom: spacing.md, backgroundColor: colors.paperRaised, borderWidth: 1, borderStyle: "solid", borderColor: colors.border, borderRadius: rounded.md },
  state: { marginBlock: spacing.md, padding: spacing.lg, borderWidth: 1, borderStyle: "solid", borderColor: colors.border, backgroundColor: colors.paperRaised, borderRadius: rounded.md },
  warningState: { borderLeftWidth: 4, borderLeftColor: colors.warning },
  errorState: { borderLeftWidth: 4, borderLeftColor: colors.danger },
  title: { marginTop: 0, marginBottom: spacing.sm, fontFamily: typography.headlineFamily, fontSize: typography.sizeXl },
  actions: { display: "flex", flexWrap: "wrap", gap: spacing.xs, alignItems: "center" },
  badge: { display: "inline-flex", alignItems: "center", gap: spacing.xxs, borderRadius: rounded.sm, paddingInline: spacing.xs, paddingBlock: spacing.xxs, fontSize: typography.sizeSm, color: colors.ink, backgroundColor: colors.paperRaised },
  success: { backgroundColor: colors.successSoft },
  warning: { backgroundColor: colors.warningSoft },
  danger: { backgroundColor: colors.dangerSoft },
  info: { backgroundColor: colors.infoSoft },
  card: { padding: spacing.md, borderWidth: 1, borderStyle: "solid", borderColor: colors.border, backgroundColor: colors.paperRaised, borderRadius: rounded.md },
  metric: { fontFamily: typography.monoFamily, fontSize: typography.sizeXl, fontVariantNumeric: "tabular-nums" },
  tableShell: { minWidth: 0, borderWidth: 1, borderStyle: "solid", borderColor: colors.border, backgroundColor: colors.paperRaised, borderRadius: rounded.md },
});

type Tone = "neutral" | "success" | "warning" | "danger" | "info";

export function FormField({
  label,
  hint,
  error,
  children,
  className = "",
}: {
  label: string;
  hint?: string;
  error?: string;
  children: ReactElement<{ id?: string; "aria-describedby"?: string; "aria-invalid"?: boolean }>;
  className?: string;
}) {
  const id = children.props.id ?? `field-${label.toLowerCase().replace(/[^a-z0-9]+/gi, "-")}`;
  const hintId = hint ? `${id}-hint` : undefined;
  const errorId = error ? `${id}-error` : undefined;
  const describedBy = [children.props["aria-describedby"], hintId, errorId].filter(Boolean).join(" ") || undefined;
  const controlProps = { ...children.props, id, "aria-describedby": describedBy, "aria-invalid": error ? true : children.props["aria-invalid"] };
  // Legacy callers keep the same label/hint IDs while controls use the shared UI.
  const control = children.type === "input" ? createElement(Input, controlProps)
    : children.type === "select" ? createElement(Select, controlProps)
    : children.type === "textarea" ? createElement(Textarea, controlProps)
    : cloneElement(children, controlProps);
  return <div className={[stylex.props(styles.field).className, "form-field", className].filter(Boolean).join(" ")}>
    <label htmlFor={id} {...stylex.props(styles.label)}>{label}</label>
    {control}
    {hint && <p className={[stylex.props(styles.hint).className, "field-hint"].join(" ")} id={hintId}>{hint}</p>}
    {error && <p className={[stylex.props(styles.error).className, "field-error"].join(" ")} id={errorId} role="alert">{error}</p>}
  </div>;
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
  return <section className={[stylex.props(styles.filters).className, "filter-bar cockpit-filter-bar"].join(" ")} aria-label="Filtre" role="search">
    <div className="filter-bar-controls">{children}</div>
    {advanced && <details className="filter-bar-advanced" data-evidence-progressive={advancedEvidence || undefined}><summary>Avancerede filtre</summary><div>{advanced}</div></details>}
    {activeFilters.length > 0 && <div className="active-filters" aria-live="polite">
      <span>Aktive filtre: {activeFilters.join(", ")}</span>
      {onReset && <Button variant="secondary" onClick={onReset}>{resetLabel}</Button>}
    </div>}
  </section>;
}

export function StatusChip({ tone = "neutral", children, coverage }: { tone?: Tone; children?: ReactNode; coverage?: DataCoverage }) {
  return <span className={[stylex.props(styles.badge, tone !== "neutral" && styles[tone]).className, `status-chip status-chip--${tone}`].join(" ")} data-coverage={coverage?.kind}>{coverage ? coverage.label : children}</span>;
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
  return <section className={[stylex.props(styles.state, (kind === "warning" || kind === "blocked") && styles.warningState, kind === "error" && styles.errorState).className, `page-state page-state--${kind}`].join(" ")} role={role} aria-live={live} data-page-state={kind}>
    <h2 {...stylex.props(styles.title)}>{title}</h2>
    {children && <p>{children}</p>}
    {(onRetry || actions) && <div className="page-state-actions">{onRetry && <Button variant="secondary" onClick={onRetry}>Prøv igen</Button>}{actions}</div>}
  </section>;
}

export function PageHeaderActions({ children }: { children: ReactNode }) {
  return <div className={[stylex.props(styles.actions).className, "page-header-actions"].join(" ")}>{children}</div>;
}

export function MetricCard({ label, value, children }: { label: string; value: ReactNode; children?: ReactNode }) {
  return <section className={[stylex.props(styles.card).className, "card metric-card"].join(" ")}>
    <h2>{label}</h2>
    <div className={[stylex.props(styles.metric).className, "metric-card-value"].join(" ")}>{value}</div>
    {children && <p className="muted">{children}</p>}
  </section>;
}

/** A deliberately small semantic table wrapper. Cells may use data-label for mobile detail labels. */
export function ResponsiveTable({ children, className = "", label }: { children: ReactNode; className?: string; label?: string }) {
  return <div className={[stylex.props(styles.tableShell).className, "card responsive-table-shell"].join(" ")} data-responsive-table>
    <div className="responsive-table-scroll"><table aria-label={label} className={`data responsive-table ${className}`.trim()}>{children}</table></div>
  </div>;
}
