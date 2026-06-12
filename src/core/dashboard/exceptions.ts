// "Åbne exceptions" — owner-facing list of exception rows with severity pill,
// plain-Danish type heading and (when present) required-action guidance.

import { exceptionTypeDa, severityDa } from "../messages";
import { escapeHtml, type DashboardInput } from "./_shared";

// A bare exception count tells the owner *that* something needs attention but
// not *what* — which only creates unease and forces a trip to the terminal.
// The dashboard therefore lists each open exception as a short line: severity,
// type, and a (truncated) message. (#263)
const EXCEPTION_SEVERITY_PILL: Record<string, "danger" | "warning" | "neutral"> = {
  high: "danger",
  critical: "danger",
  medium: "warning",
  low: "neutral",
};

// The owner faces the dashboard, not the developer. An exception heading like
// `UNMATCHED_BANK_TRANSACTION` and an English severity pill "medium" read as
// the system's internals — so the static dashboard renders a plain-Danish
// label for the type and the severity, matching what the Cockpit SPA shows.
// The Danish labels live in core/messages.ts: `exceptionTypeDa` plain-Danish
// heading (humanises unknown codes), `severityDa(.., "title")` capitalised
// severity. (#270, #316)

export function exceptionsSection(input: DashboardInput): string {
  const result = input.exceptions;
  if (result.count === 0 || result.rows.length === 0) {
    return `<div class="empty-state">Ingen åbne exceptions</div>`;
  }
  // Stable order: highest severity first, then by id so the render is
  // deterministic regardless of the row order the CLI passes in.
  const severityRank: Record<string, number> = { critical: 0, high: 1, medium: 2, low: 3 };
  const sorted = [...result.rows].sort((a, b) => {
    const ra = severityRank[a.severity] ?? 9;
    const rb = severityRank[b.severity] ?? 9;
    if (ra !== rb) return ra - rb;
    return a.id - b.id;
  });
  const maxRows = 12;
  const visible = sorted.slice(0, maxRows);
  const overflow = sorted.length - visible.length;
  const items = visible.map((row) => {
    const pillClass = EXCEPTION_SEVERITY_PILL[row.severity] ?? "neutral";
    // The full message — not a mid-sentence-truncated fragment. The Cockpit
    // SPA shows the whole message; the static dashboard must too. (#270)
    const action = (row.requiredAction ?? "").trim();
    const actionHtml = action
      ? `\n      <div class="detail"><strong>Sådan løser du den:</strong> ${escapeHtml(action)}</div>`
      : "";
    return `  <div class="status-row">
    <div>
      <div class="label">${escapeHtml(exceptionTypeDa(row.type))}</div>
      <div class="detail">${escapeHtml(row.message)}</div>${actionHtml}
    </div>
    <div><span class="pill ${pillClass}">${escapeHtml(severityDa(row.severity, "title"))}</span></div>
  </div>`;
  }).join("\n");
  const overflowRow = overflow > 0
    ? `<div class="muted" style="margin-top: var(--space-xs); font-size: 13px;">… og ${overflow} yderligere</div>`
    : "";
  return `<div class="section">
${items}
</div>
${overflowRow}`;
}
