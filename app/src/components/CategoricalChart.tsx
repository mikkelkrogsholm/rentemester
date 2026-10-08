import { useId, useLayoutEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import * as stylex from "@stylexjs/stylex";
import { colors, controls, rounded, spacing, typography } from "../design/tokens.stylex";
import { CHART_AXIS_NUMBER, chartCurrency } from "./chart-format";

type ChartTone = "success" | "accent" | "info";
export type ChartSeries = {
  id: string;
  label: string;
  values: readonly (number | null)[];
  tone: ChartTone;
  kind?: "bar" | "line";
  axis?: "left" | "right";
};

export const chartStyles = stylex.create({
  chartCard: { minWidth: 0, padding: spacing.md },
  pnlChart: { width: "100%", minWidth: 0 },
  chartData: { marginTop: spacing.md },
  summary: { minHeight: controls.height, cursor: "pointer", paddingBlock: spacing.xs },
  legend: { display: "flex", flexWrap: "wrap", justifyContent: "flex-end", gap: spacing.xs, marginBottom: spacing.xs, "@media print": { display: "none" } },
  toggle: {
    display: "inline-flex", alignItems: "center", gap: spacing.xs, minHeight: controls.height,
    paddingInline: spacing.xs, paddingBlock: spacing.xxs, backgroundColor: colors.paperRaised,
    borderWidth: 1, borderStyle: "solid", borderColor: colors.border, borderRadius: rounded.sm,
    color: colors.ink, fontFamily: typography.bodyFamily, fontSize: typography.sizeSm, cursor: "pointer",
    ":hover": { backgroundColor: colors.paper },
    ":focus-visible": { outlineWidth: controls.focusWidth, outlineStyle: "solid", outlineColor: colors.info, outlineOffset: controls.focusOffset },
  },
  disabledSeries: { color: colors.inkMuted, textDecoration: "line-through" },
  swatch: { width: 12, height: 12, borderRadius: rounded.sm, flexShrink: 0 },
  svg: {
    display: "block", width: "100%", height: 320, overflow: "visible", touchAction: "pan-y",
    "@media (max-width: 639px)": { height: 300 },
    ":focus-visible": { outlineWidth: controls.focusWidth, outlineStyle: "solid", outlineColor: colors.info, outlineOffset: controls.focusOffset },
  },
  short: { height: 260, "@media (max-width: 639px)": { height: 260 } },
  grid: { stroke: colors.border, strokeWidth: 1 },
  baseline: { stroke: colors.borderStrong, strokeWidth: 1 },
  axis: { fill: colors.inkMuted, fontFamily: typography.monoFamily, fontSize: 12, fontVariantNumeric: "tabular-nums" },
  category: { fill: colors.inkMuted, fontFamily: typography.bodyFamily, fontSize: 12 },
  success: { fill: colors.success, stroke: colors.success, backgroundColor: colors.success },
  accent: { fill: colors.accent, stroke: colors.accent, backgroundColor: colors.accent },
  info: { fill: colors.info, stroke: colors.info, backgroundColor: colors.info },
  bar: { strokeWidth: 0 },
  line: { fill: "none", strokeWidth: 2, strokeLinejoin: "round", strokeLinecap: "round" },
  point: { strokeWidth: 0 },
  selected: { stroke: colors.inkMuted, strokeWidth: 1, strokeDasharray: "3 4" },
  details: { minHeight: 44, marginTop: spacing.xs, marginBottom: 0, color: colors.inkMuted, fontSize: typography.sizeSm, overflowWrap: "anywhere", "@media print": { display: "none" } },
  value: { fontFamily: typography.monoFamily, fontVariantNumeric: "tabular-nums" },
  empty: { color: colors.inkMuted, marginBlock: spacing.md },
  srOnly: { position: "absolute", width: 1, height: 1, padding: 0, margin: -1, overflow: "hidden", clipPath: "inset(50%)", whiteSpace: "nowrap", borderWidth: 0 },
});

function known(value: number | null | undefined): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

/** Nice finite domains include zero for bars; a bank balance keeps its own range. */
function axisScale(values: readonly number[], beginAtZero: boolean) {
  let min = values.length ? Math.min(...values) : 0;
  let max = values.length ? Math.max(...values) : 1;
  if (beginAtZero) { min = Math.min(0, min); max = Math.max(0, max); }
  if (min === max) {
    const padding = Math.abs(min) * 0.1 || 1;
    if (!beginAtZero || min < 0) min -= padding;
    max += padding;
  }
  const roughStep = (max - min) / 4;
  const power = 10 ** Math.floor(Math.log10(roughStep));
  const fraction = roughStep / power;
  const step = (fraction <= 1 ? 1 : fraction <= 2 ? 2 : fraction <= 5 ? 5 : 10) * power;
  min = Math.floor(min / step) * step;
  max = Math.ceil(max / step) * step;
  const count = Math.round((max - min) / step);
  const ticks = Array.from({ length: count + 1 }, (_, index) => Number((min + index * step).toPrecision(12)));
  const format = step < 1 ? new Intl.NumberFormat("da-DK", { maximumFractionDigits: 2 }) : CHART_AXIS_NUMBER;
  return { min, max, ticks, format };
}

function curve(points: readonly { x: number; y: number }[]) {
  return points.map((point, index) => {
    if (!index) return `M${point.x},${point.y}`;
    const previous = points[index - 1]!;
    const control = (point.x - previous.x) * 0.2;
    return `C${previous.x + control},${previous.y} ${point.x - control},${point.y} ${point.x},${point.y}`;
  }).join(" ");
}

/** Categorical financial charts share one accessible, dependency-free SVG renderer. */
export function CategoricalChart({ labels, series, currency = "DKK", label, dataTableId, height = "bars" }: {
  labels: readonly (string | readonly string[])[];
  series: readonly ChartSeries[];
  currency?: string;
  label: string;
  dataTableId?: string;
  height?: "bars" | "line";
}) {
  const id = useId();
  const svg = useRef<SVGSVGElement>(null);
  const [size, setSize] = useState({ width: 640, height: height === "line" ? 260 : 320 });
  const [hidden, setHidden] = useState<ReadonlySet<string>>(new Set());
  const [active, setActive] = useState<number | null>(null);
  const [touchSelection, setTouchSelection] = useState(false);
  useLayoutEffect(() => {
    const element = svg.current;
    if (!element) return;
    const measure = () => {
      const bounds = element.getBoundingClientRect();
      if (bounds.width > 0 && bounds.height > 0) setSize((current) => current.width === bounds.width && current.height === bounds.height ? current : { width: bounds.width, height: bounds.height });
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const visible = series.filter((item) => !hidden.has(item.id));
  const valuesFor = (axis: "left" | "right") => visible.filter((item) => (item.axis ?? "left") === axis).flatMap((item) => item.values.filter(known));
  const left = axisScale(valuesFor("left"), visible.some((item) => (item.axis ?? "left") === "left" && item.kind !== "line"));
  const right = axisScale(valuesFor("right"), false);
  const hasRight = visible.some((item) => item.axis === "right");
  const gutter = (scale: typeof left) => Math.max(48, ...scale.ticks.map((tick) => scale.format.format(tick).length * 7.3 + 14));
  const leftEdge = gutter(left);
  const rightEdge = size.width - (hasRight ? gutter(right) : 12);
  const top = 12;
  const bottom = size.height - 48;
  const plotWidth = Math.max(1, rightEdge - leftEdge);
  const band = plotWidth / Math.max(1, labels.length);
  const x = (index: number) => leftEdge + band * (index + 0.5);
  const y = (value: number, axis: "left" | "right" = "left") => {
    const scale = axis === "right" ? right : left;
    return bottom - (value - scale.min) / (scale.max - scale.min) * (bottom - top);
  };
  const bars = visible.filter((item) => item.kind !== "line");
  const barWidth = Math.max(1, Math.min(48, band * 0.72 / Math.max(1, bars.length)));
  const labelEvery = Math.max(1, Math.ceil(labels.length * 38 / plotWidth));
  const selected = active !== null && active < labels.length ? active : null;
  const currencyFormat = chartCurrency(currency);
  const categoryLabel = (index: number) => [labels[index]].flat().join(" ");
  const onPointer = (event: PointerEvent<SVGSVGElement>) => {
    if (!labels.length) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    if (!bounds.width) return;
    const position = (event.clientX - bounds.left) / bounds.width * size.width;
    setActive(Math.max(0, Math.min(labels.length - 1, Math.floor((position - leftEdge) / band))));
    setTouchSelection(event.pointerType === "touch");
  };
  const onKeyDown = (event: KeyboardEvent<SVGSVGElement>) => {
    if (!labels.length || !["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    setActive(event.key === "Home" ? 0 : event.key === "End" ? labels.length - 1 : Math.max(0, Math.min(labels.length - 1, (selected ?? 0) + (event.key === "ArrowLeft" ? -1 : 1))));
  };

  return <div {...stylex.props(chartStyles.pnlChart)}>
    <div {...stylex.props(chartStyles.legend)} role="group" aria-label={`Vælg serier: ${label}`}>
      {series.map((item) => <button key={item.id} type="button" aria-pressed={!hidden.has(item.id)} {...stylex.props(chartStyles.toggle, hidden.has(item.id) && chartStyles.disabledSeries)} onClick={() => setHidden((current) => {
        const next = new Set(current);
        if (next.has(item.id)) next.delete(item.id); else next.add(item.id);
        return next;
      })}><span aria-hidden="true" {...stylex.props(chartStyles.swatch, chartStyles[item.tone])} />{item.label}</button>)}
    </div>
    <p id={`${id}-instructions`} {...stylex.props(chartStyles.srOnly)}>Brug venstre og højre piletast til at vælge periode, Home til den første og End til den sidste. Værdierne findes også i tabellen.</p>
    <svg ref={svg} {...stylex.props(chartStyles.svg, height === "line" && chartStyles.short)} viewBox={`0 0 ${size.width} ${size.height}`} role="img" aria-label={label} aria-details={dataTableId} aria-describedby={`${id}-instructions ${id}-values`} tabIndex={labels.length ? 0 : undefined} onKeyDown={onKeyDown} onFocus={() => { if (labels.length) setActive((current) => current ?? 0); }} onPointerMove={onPointer} onPointerDown={onPointer} onPointerLeave={() => { if (!touchSelection) setActive(null); }}>
      <title>{label}</title>
      {left.ticks.map((tick) => <g key={tick} aria-hidden="true"><line {...stylex.props(chartStyles.grid, tick === 0 && chartStyles.baseline)} x1={leftEdge} x2={rightEdge} y1={y(tick)} y2={y(tick)} /><text {...stylex.props(chartStyles.axis)} x={leftEdge - 10} y={y(tick) + 4} textAnchor="end">{left.format.format(tick)}</text></g>)}
      {hasRight && right.ticks.map((tick) => <text key={tick} aria-hidden="true" {...stylex.props(chartStyles.axis, chartStyles.info)} x={rightEdge + 10} y={y(tick, "right") + 4} textAnchor="start">{right.format.format(tick)}</text>)}
      {bars.map((item, seriesIndex) => <g key={item.id} aria-hidden="true" data-series={item.id}>{item.values.map((value, index) => known(value) && index < labels.length ? <rect key={index} {...stylex.props(chartStyles[item.tone], chartStyles.bar)} x={x(index) + (seriesIndex - bars.length / 2) * barWidth} y={Math.min(y(0, item.axis), y(value, item.axis))} width={Math.max(1, barWidth - 1)} height={Math.abs(y(value, item.axis) - y(0, item.axis))} rx={2} data-category={index} data-value={value} /> : null)}</g>)}
      {visible.filter((item) => item.kind === "line").map((item) => {
        // Keep each category's position; connect known values across missing balances.
        const points = item.values.flatMap((value, index) => known(value) && index < labels.length ? [{ x: x(index), y: y(value, item.axis), index, value }] : []);
        return <g key={item.id} aria-hidden="true" data-series={item.id}>{points.length > 1 && <path {...stylex.props(chartStyles[item.tone], chartStyles.line)} d={curve(points)} />}{points.map((point) => <circle key={point.index} {...stylex.props(chartStyles[item.tone], chartStyles.point)} cx={point.x} cy={point.y} r={point.index === selected ? 4 : 3} data-category={point.index} data-value={point.value} />)}</g>;
      })}
      {selected !== null && <line aria-hidden="true" {...stylex.props(chartStyles.selected)} x1={x(selected)} x2={x(selected)} y1={top} y2={bottom} />}
      {labels.map((category, index) => (index % labelEvery === 0 || index === labels.length - 1) && <text key={index} aria-hidden="true" {...stylex.props(chartStyles.category)} x={x(index)} y={bottom + 20} textAnchor="middle">{[category].flat().map((line, lineIndex) => <tspan key={lineIndex} x={x(index)} dy={lineIndex ? 14 : 0}>{line}</tspan>)}</text>)}
    </svg>
    {!labels.length && <p {...stylex.props(chartStyles.empty)}>Ingen data at vise endnu.</p>}
    {labels.length > 0 && !visible.length && <p {...stylex.props(chartStyles.empty)}>Vælg en serie for at vise grafen.</p>}
    <p id={`${id}-values`} {...stylex.props(chartStyles.details)} aria-live="polite" aria-atomic="true">{selected !== null ? <><strong>{categoryLabel(selected)}</strong>{visible.map((item) => <span key={item.id}> · {item.label}: <span {...stylex.props(chartStyles.value)}>{known(item.values[selected]) ? currencyFormat.format(item.values[selected]!) : "Ukendt"}</span></span>)}</> : "Peg eller tryk på en periode for at se værdierne."}</p>
  </div>;
}
