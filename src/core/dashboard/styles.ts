// CSS + font-link for the dashboard render-engine.
//
// `buildStyle()` is a self-contained fat string built from the DESIGN.md
// tokens; `fontLink()` returns the single deterministic Google Fonts URL.

import { TOKENS } from "./_shared";

// --------------------------------------------------------------------------
// CSS (inline, generated from TOKENS)
// --------------------------------------------------------------------------

export function buildStyle(): string {
  // Body fallback chain stays consistent across browsers without needing the
  // Google Fonts request to succeed. Mono fallback chain keeps tabular figures.
  const bodyStack = `"${TOKENS.bodyFamily}", -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif`;
  const headlineStack = `"${TOKENS.headlineFamily}", Georgia, "Times New Roman", serif`;
  const monoStack = `"${TOKENS.monoFamily}", "SF Mono", Menlo, Consolas, monospace`;

  return `
:root {
  --paper: ${TOKENS.paper};
  --paper-raised: ${TOKENS.paperRaised};
  --ink: ${TOKENS.ink};
  --ink-muted: ${TOKENS.inkMuted};
  --accent: ${TOKENS.accent};
  --on-accent: ${TOKENS.onAccent};
  --danger: ${TOKENS.danger};
  --success: ${TOKENS.success};
  --warning: ${TOKENS.warning};
  --info: ${TOKENS.info};
  --accent-soft: ${TOKENS.accentSoft};
  --danger-soft: ${TOKENS.dangerSoft};
  --success-soft: ${TOKENS.successSoft};
  --warning-soft: ${TOKENS.warningSoft};
  --info-soft: ${TOKENS.infoSoft};
  --space-xxs: ${TOKENS.spaceXxs};
  --space-xs: ${TOKENS.spaceXs};
  --space-sm: ${TOKENS.spaceSm};
  --space-md: ${TOKENS.spaceMd};
  --space-lg: ${TOKENS.spaceLg};
  --space-xl: ${TOKENS.spaceXl};
  --rounded-sm: ${TOKENS.roundedSm};
  --rounded-md: ${TOKENS.roundedMd};
  --rounded-lg: ${TOKENS.roundedLg};
}
* { box-sizing: border-box; }
html, body {
  margin: 0;
  padding: 0;
  background: var(--paper);
  color: var(--ink);
  font-family: ${bodyStack};
  font-size: ${TOKENS.bodySize};
  line-height: ${TOKENS.bodyLineHeight};
}
.page {
  max-width: 960px;
  margin: 0 auto;
  padding: var(--space-xl) var(--space-lg);
}
.headline {
  font-family: ${headlineStack};
  font-weight: 600;
  letter-spacing: -0.01em;
}
.mono {
  font-family: ${monoStack};
  font-variant-numeric: tabular-nums;
  font-feature-settings: "tnum" 1;
}
.amount {
  font-family: ${monoStack};
  font-variant-numeric: tabular-nums;
  font-feature-settings: "tnum" 1;
  text-align: right;
  white-space: nowrap;
}
.amount-lg {
  font-family: ${monoStack};
  font-variant-numeric: tabular-nums;
  font-feature-settings: "tnum" 1;
  font-size: 28px;
  line-height: 1.1;
  color: var(--ink);
}
.label-sm {
  font-family: ${bodyStack};
  font-size: 11px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--ink-muted);
}
.header {
  background: var(--paper-raised);
  border: 1px solid var(--ink-muted);
  border-radius: var(--rounded-md);
  padding: var(--space-lg);
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: var(--space-lg);
}
.header h1 {
  margin: 0 0 var(--space-xs) 0;
  font-family: ${headlineStack};
  font-size: 24px;
  font-weight: 600;
  color: var(--ink);
}
.header .meta {
  color: var(--ink-muted);
  font-size: 14px;
}
.header .cvr {
  font-family: ${monoStack};
  color: var(--ink-muted);
  font-size: 14px;
}
.metrics {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: var(--space-sm);
  margin-bottom: var(--space-lg);
}
.metric-card {
  background: var(--paper-raised);
  border: 1px solid var(--ink-muted);
  border-radius: var(--rounded-md);
  padding: var(--space-md);
}
.metric-card.accent { border-color: var(--accent); }
.metric-card.danger { border-color: var(--danger); }
.metric-card .value { color: var(--ink); }
.metric-card .secondary {
  color: var(--ink-muted);
  font-size: 13px;
  margin-top: var(--space-xxs);
}
.metric-card .label-sm { margin-top: var(--space-sm); }
.section {
  margin-bottom: var(--space-lg);
}
.section h2 {
  font-family: ${headlineStack};
  font-size: 18px;
  font-weight: 600;
  margin: 0 0 var(--space-sm) 0;
  color: var(--ink);
  border-bottom: 1px solid var(--ink-muted);
  padding-bottom: var(--space-xs);
}
.deadline-card {
  background: var(--paper-raised);
  border: 1px solid var(--ink-muted);
  border-radius: var(--rounded-md);
  padding: var(--space-md);
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--space-md);
}
.deadline-card .label-sm { margin-bottom: var(--space-xxs); }
table.dash-table {
  width: 100%;
  border-collapse: collapse;
  background: var(--paper);
}
table.dash-table th, table.dash-table td {
  padding: var(--space-xs) var(--space-sm);
  text-align: left;
  border-bottom: 1px solid var(--ink-muted);
  font-size: 14px;
}
table.dash-table th {
  color: var(--ink-muted);
  font-weight: 500;
  font-size: 11px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}
table.dash-table tbody tr:nth-child(even) { background: var(--paper-raised); }
table.dash-table td.amount, table.dash-table th.amount { text-align: right; }
table.dash-table td.center, table.dash-table th.center { text-align: center; }
.pill {
  display: inline-block;
  padding: 2px var(--space-xs);
  border-radius: var(--rounded-sm);
  font-size: 12px;
  font-weight: 500;
  letter-spacing: 0.02em;
  color: var(--ink);
}
.pill.success { background: var(--success-soft); }
.pill.warning { background: var(--warning-soft); }
.pill.danger { background: var(--danger-soft); }
.pill.neutral { background: var(--paper-raised); border: 1px solid var(--ink-muted); }
.status-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--space-sm) 0;
  border-bottom: 1px solid var(--ink-muted);
}
.status-row:last-child { border-bottom: none; }
.status-row .label { color: var(--ink); font-weight: 500; }
.status-row .detail { color: var(--ink-muted); font-size: 13px; }
.muted { color: var(--ink-muted); }
.empty-state {
  color: var(--ink-muted);
  font-style: italic;
  padding: var(--space-sm) 0;
}
.footer {
  background: var(--paper-raised);
  border: 1px solid var(--ink-muted);
  border-radius: var(--rounded-md);
  padding: var(--space-md);
  color: var(--ink-muted);
  font-size: 12px;
  margin-top: var(--space-lg);
}
.footer .row { display: flex; justify-content: space-between; gap: var(--space-md); }
.footer .mono { color: var(--ink-muted); }
.footer .provenance {
  margin-top: var(--space-xs);
  color: var(--ink-muted);
  font-size: 11px;
}
.footer .provenance summary {
  cursor: pointer;
  letter-spacing: 0.04em;
}
.activity-log {
  display: grid;
  grid-template-columns: auto auto auto 1fr;
  gap: var(--space-xs) var(--space-md);
  font-size: 14px;
}
.activity-log .time { font-family: ${monoStack}; color: var(--ink); white-space: nowrap; }
.activity-log .actor { color: var(--ink-muted); }
.activity-log .event { color: var(--ink); font-weight: 500; }
.activity-log .message { color: var(--ink-muted); }
@media print {
  body { background: white; }
  .page { padding: var(--space-md); max-width: 100%; }
}
`.trim();
}

export function fontLink(): string {
  // Single deterministic Google Fonts URL. HTML still renders if blocked.
  const families = [
    "family=Source+Serif+4:wght@400;600",
    "family=IBM+Plex+Sans:wght@400;500",
    "family=IBM+Plex+Mono:wght@400;500",
  ].join("&");
  return `<link rel="preconnect" href="https://fonts.googleapis.com">\n<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>\n<link rel="stylesheet" href="https://fonts.googleapis.com/css2?${families}&display=swap">`;
}
