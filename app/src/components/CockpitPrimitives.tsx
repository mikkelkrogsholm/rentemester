import { cloneElement, type ReactElement, type ReactNode } from "react";

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
  return <div className={`form-field ${className}`.trim()}>
    <label htmlFor={id}>{label}</label>
    {cloneElement(children, { id, "aria-describedby": describedBy, "aria-invalid": error ? true : undefined })}
    {hint && <p className="field-hint" id={hintId}>{hint}</p>}
    {error && <p className="field-error" id={errorId} role="alert">{error}</p>}
  </div>;
}

export function FilterBar({
  children,
  activeFilters = [],
  onReset,
  advanced,
}: {
  children: ReactNode;
  activeFilters?: string[];
  onReset?: () => void;
  advanced?: ReactNode;
}) {
  return <section className="filter-bar cockpit-filter-bar" aria-label="Filtre">
    <div className="filter-bar-controls">{children}</div>
    {advanced && <details className="filter-bar-advanced"><summary>Avancerede filtre</summary><div>{advanced}</div></details>}
    {activeFilters.length > 0 && <div className="active-filters" aria-live="polite">
      <span>Aktive filtre: {activeFilters.join(", ")}</span>
      {onReset && <button type="button" className="btn secondary" onClick={onReset}>Nulstil filtre</button>}
    </div>}
  </section>;
}

export function StatusChip({ tone = "neutral", children }: { tone?: Tone; children: ReactNode }) {
  return <span className={`status-chip status-chip--${tone}`}>{children}</span>;
}

export function PageState({
  kind,
  title,
  children,
  onRetry,
}: {
  kind: "loading" | "empty" | "warning" | "blocked" | "error";
  title: string;
  children?: ReactNode;
  onRetry?: () => void;
}) {
  const role = kind === "error" || kind === "blocked" ? "alert" : "status";
  const live = kind === "loading" ? "polite" : undefined;
  return <section className={`page-state page-state--${kind}`} role={role} aria-live={live} data-page-state={kind}>
    <h2>{title}</h2>
    {children && <p>{children}</p>}
    {onRetry && <button className="btn secondary" type="button" onClick={onRetry}>Prøv igen</button>}
  </section>;
}

export function PageHeaderActions({ children }: { children: ReactNode }) {
  return <div className="page-header-actions">{children}</div>;
}

export function MetricCard({ label, value, children }: { label: string; value: ReactNode; children?: ReactNode }) {
  return <section className="card metric-card">
    <h2>{label}</h2>
    <div className="metric-card-value">{value}</div>
    {children && <p className="muted">{children}</p>}
  </section>;
}

/** A deliberately small semantic table wrapper. Cells may use data-label for mobile detail labels. */
export function ResponsiveTable({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <div className="card responsive-table-shell" data-responsive-table>
    <div className="responsive-table-scroll"><table className={`data responsive-table ${className}`.trim()}>{children}</table></div>
  </div>;
}
